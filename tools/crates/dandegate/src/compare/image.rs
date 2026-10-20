//! Compares the images that a per-revision step writes on each side, by file name.
//!
//! For each name that either side expected or wrote, the page shows whether each side expected the image and whether
//! the image exists and can be read. A missing, unexpected or unreadable image appears before the differences and makes
//! the page incomplete. A readable pair gets the mean absolute difference of its RGBA samples, from 0 to 255. Two
//! images of different sizes differ by 255. dandegate decodes every PNG colour type and depth as 8-bit RGBA, and drops
//! an image's samples once it has compared them, so it holds one pair in memory at a time.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::Cursor;

use camino::{Utf8Path, Utf8PathBuf};

use crate::compare::measure::Finite;
use crate::revision::{PerSide, Side};

/// A decoded picture's size and its RGBA samples, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Whether the file at `path` is a PNG the comparison can read, or why not.
///
/// # Errors
/// Why the file cannot be read or decoded.
pub(crate) fn readable(path: &Utf8Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    decode(&bytes).map(|_| ())
}

/// Decode a PNG of any colour type and depth into 8-bit RGBA.
///
/// # Errors
/// The decoder's reason, as text, for anything that is not a whole PNG.
fn decode(bytes: &[u8]) -> Result<Pixels, String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "the image is too large to decode".to_owned())?;
    let mut buffer = vec![0; size];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|error| error.to_string())?;
    let samples = &buffer[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => samples.to_vec(),
        png::ColorType::Rgb => samples
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => samples
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|ga| [ga[0], ga[0], ga[0], ga[1]])
            .collect(),
        png::ColorType::Grayscale => samples
            .iter()
            .flat_map(|&grey| [grey, grey, grey, 255])
            .collect(),
        png::ColorType::Indexed => {
            return Err("a paletted image the decoder did not expand".to_owned());
        }
    };
    Ok(Pixels {
        width: frame.width,
        height: frame.height,
        rgba,
    })
}

/// The mean absolute difference of two pictures' RGBA samples, from 0 to 255, or 255 when their sizes differ.
fn mean_difference(a: &Pixels, b: &Pixels) -> Finite {
    const MOST: Finite = Finite::constant(255.0);
    if (a.width, a.height) != (b.width, b.height) {
        return MOST;
    }
    let total: u64 = a
        .rgba
        .iter()
        .zip(&b.rgba)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    #[expect(
        clippy::cast_precision_loss,
        reason = "a sum of bytes and a sample count are far below 2^52"
    )]
    let mean = total as f64 / a.rgba.len().max(1) as f64;
    Finite::new(mean).expect("a mean of byte differences is between 0 and 255")
}

/// What one side holds under a name.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImageState {
    Absent,
    /// There, but not a picture: the decoder's reason.
    Unreadable(String),
    /// There and decoded, with its size.
    Readable {
        width: u32,
        height: u32,
    },
}

/// A problem with a cell's evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    /// Expected on this side and not there.
    Missing(Side),
    /// There on this side, which did not expect it.
    Unexpected(Side),
    /// There on this side and not a picture.
    Unreadable(Side, String),
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(side) => write!(formatter, "missing on the {} side", side.as_str()),
            Self::Unexpected(side) => {
                write!(formatter, "not expected on the {} side", side.as_str())
            }
            Self::Unreadable(side, why) => {
                write!(formatter, "unreadable on the {} side: {why}", side.as_str())
            }
        }
    }
}

/// One name's images on both sides.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cell {
    name: String,
    expected: PerSide<bool>,
    image: PerSide<ImageState>,
    difference: Option<Finite>,
}

impl Cell {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn image(&self) -> &PerSide<ImageState> {
        &self.image
    }

    /// The mean RGBA difference, when both sides are readable.
    pub(crate) fn difference(&self) -> Option<Finite> {
        self.difference
    }

    /// Every problem with the cell's evidence, before side first.
    pub(crate) fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        for side in Side::BOTH {
            match (&self.image[side], self.expected[side]) {
                (ImageState::Absent, true) => problems.push(Problem::Missing(side)),
                (ImageState::Absent, false) => {}
                (present, expected) => {
                    if !expected {
                        problems.push(Problem::Unexpected(side));
                    }
                    if let ImageState::Unreadable(why) = present {
                        problems.push(Problem::Unreadable(side, why.clone()));
                    }
                }
            }
        }
        problems
    }
}

/// Every name either side expected or produced, compared: those with a problem first, then the largest differences.
/// `found` is each side's images by name.
pub(crate) fn cells(
    expected: &PerSide<BTreeSet<String>>,
    found: &PerSide<BTreeMap<String, Utf8PathBuf>>,
) -> Vec<Cell> {
    let names: BTreeSet<&String> = expected
        .before
        .iter()
        .chain(&expected.after)
        .chain(found.before.keys())
        .chain(found.after.keys())
        .collect();
    let mut cells: Vec<Cell> = names
        .into_iter()
        .map(|name| {
            let decoded = PerSide::from_fn(|side| {
                found[side].get(name).map(|path| {
                    fs::read(path)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| decode(&bytes))
                })
            });
            let difference = match (&decoded.before, &decoded.after) {
                (Some(Ok(before)), Some(Ok(after))) => Some(mean_difference(before, after)),
                _ => None,
            };
            let image = decoded.map(|_, state| match state {
                None => ImageState::Absent,
                Some(Err(why)) => ImageState::Unreadable(why),
                Some(Ok(pixels)) => ImageState::Readable {
                    width: pixels.width,
                    height: pixels.height,
                },
            });
            Cell {
                name: name.clone(),
                expected: PerSide::from_fn(|side| expected[side].contains(name)),
                image,
                difference,
            }
        })
        .collect();
    cells.sort_by(|left, right| {
        let order = left.problems().is_empty().cmp(&right.problems().is_empty());
        let size = |cell: &Cell| cell.difference.map_or(0.0, Finite::get);
        order.then_with(|| size(right).total_cmp(&size(left)))
    });
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, colour: png::ColorType, samples: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(colour);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("a header");
        writer.write_image_data(samples).expect("the samples");
        writer.finish().expect("a whole image");
        bytes
    }

    #[test]
    fn the_mean_difference_is_per_rgba_sample() {
        let a = decode(&png(1, 1, png::ColorType::Rgba, &[10, 20, 30, 255])).expect("a picture");
        let b = decode(&png(1, 1, png::ColorType::Rgba, &[13, 20, 27, 255])).expect("a picture");
        // |10-13| + |20-20| + |30-27| + |255-255| = 6, over four samples.
        assert_eq!(mean_difference(&a, &b).get(), 1.5);
        assert_eq!(mean_difference(&a, &a).get(), 0.0);
    }

    #[test]
    fn pictures_of_different_sizes_differ_by_the_most() {
        let a = decode(&png(1, 1, png::ColorType::Rgba, &[0, 0, 0, 0])).expect("a picture");
        let b = decode(&png(2, 1, png::ColorType::Rgba, &[0; 8])).expect("a picture");
        assert_eq!(mean_difference(&a, &b).get(), 255.0);
    }

    #[test]
    fn grey_and_rgb_pictures_read_as_rgba() {
        let grey = decode(&png(1, 1, png::ColorType::Grayscale, &[7])).expect("a picture");
        let grey_alpha =
            decode(&png(1, 1, png::ColorType::GrayscaleAlpha, &[7, 9])).expect("a picture");
        let rgb = decode(&png(1, 1, png::ColorType::Rgb, &[1, 2, 3])).expect("a picture");
        assert_eq!(grey.rgba, [7, 7, 7, 255]);
        assert_eq!(grey_alpha.rgba, [7, 7, 7, 9]);
        assert_eq!(rgb.rgba, [1, 2, 3, 255]);
    }

    #[test]
    fn bytes_that_are_not_a_png_are_unreadable() {
        assert!(decode(b"not a picture").is_err());
        let mut cut = png(1, 1, png::ColorType::Rgba, &[1, 2, 3, 4]);
        cut.truncate(cut.len() - 20);
        assert!(decode(&cut).is_err());
    }
}
