//! Independent product scenarios through the configuration and serving boundaries.

use std::{
    error::Error,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use makiatto::{
    config::Config,
    web::{Response, WebServer},
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let root = PathBuf::from(format!(
            ".cache/functional-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("sites/example.test/nested"))?;
        let fixture = Self { root };
        fixture.write_png(
            "photo.png",
            4,
            2,
            &[RED, GREEN, BLUE, WHITE].concat().repeat(2),
        )?;
        fixture.write_png("solid.png", 4, 2, &GREEN.repeat(8))?;
        fixture.write_png(
            "portrait.png",
            2,
            4,
            &[
                RED.repeat(2),
                GREEN.repeat(2),
                BLUE.repeat(2),
                WHITE.repeat(2),
            ]
            .concat(),
        )?;
        fs::write(fixture.site().join("index.html"), b"home page")?;
        fs::write(fixture.site().join("nested/index.html"), b"nested page")?;
        fs::write(fixture.site().join("notes.txt"), b"not an image")?;
        fs::write(fixture.root.join("sites/private.txt"), b"outside domain")?;
        Ok(fixture)
    }

    fn site(&self) -> PathBuf {
        self.root.join("sites/example.test")
    }

    fn cache(&self) -> PathBuf {
        self.root.join("outputs")
    }

    fn config_text(&self, settings: &str) -> String {
        format!(
            "[web]\nstatic_dir = {:?}\n[domains]\n'alias.test' = 'example.test'\n\
             [images]\ncache_dir = {:?}\n{settings}\n",
            self.root.join("sites").to_string_lossy(),
            self.cache().to_string_lossy()
        )
    }

    fn server(&self, settings: &str) -> TestResult<WebServer> {
        Ok(WebServer::new(Config::from_toml(
            &self.config_text(settings),
        )?))
    }

    fn write_png(&self, name: &str, width: u32, height: u32, pixels: &[u8]) -> TestResult {
        fs::write(self.site().join(name), encode_png(width, height, pixels)?)?;
        Ok(())
    }

    fn cached_files(&self) -> TestResult<Vec<PathBuf>> {
        let mut files = Vec::new();
        if self.cache().exists() {
            collect_files(&self.cache(), &mut files)?;
        }
        files.sort();
        Ok(files)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) -> TestResult {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn encode_png(width: u32, height: u32, pixels: &[u8]) -> TestResult<Vec<u8>> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::NoCompression);
        encoder.write_header()?.write_image_data(pixels)?;
    }
    Ok(output)
}

fn decode_png(response: &Response) -> TestResult<(u32, u32, Vec<u8>)> {
    assert_eq!(response.content_type, "image/png");
    let mut decoder = png::Decoder::new(Cursor::new(&response.body));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("missing PNG output size")?
    ];
    let frame = reader.next_frame(&mut pixels)?;
    pixels.truncate(frame.buffer_size());
    let rgba = match frame.color_type {
        png::ColorType::Rgba => pixels,
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        png::ColorType::Grayscale => pixels
            .into_iter()
            .flat_map(|value| [value, value, value, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        png::ColorType::Indexed => return Err("PNG decoder did not expand palette".into()),
    };
    Ok((frame.width, frame.height, rgba))
}

fn request(server: &WebServer, query: &str) -> TestResult<Response> {
    Ok(server.serve("example.test", "/photo.png", query)?)
}

#[test]
fn originals_domains_and_static_fallbacks_preserve_bytes() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    let original = fs::read(fixture.site().join("photo.png"))?;
    assert_eq!(request(&server, "")?.body, original);
    assert_eq!(
        server.serve("alias.test:8080", "/photo.png", "")?.body,
        original
    );
    for query in ["new=2", "width_hint=2", "wish=2", "quality_hint=1"] {
        assert_eq!(request(&server, query)?.body, original, "query: {query}");
    }
    assert_eq!(server.serve("example.test", "/", "")?.body, b"home page");
    assert_eq!(
        server.serve("example.test", "/nested/", "")?.body,
        b"nested page"
    );
    assert_eq!(
        server
            .serve("example.test", "/notes.txt", "w=0&format=bad")?
            .body,
        b"not an image"
    );
    assert_eq!(fixture.cached_files()?.len(), 0);
    Ok(())
}

#[test]
fn domain_resolution_cannot_escape_the_static_tree() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    for path in [
        "/../private.txt",
        "/nested/../../private.txt",
        "/missing.png",
    ] {
        assert!(
            server.serve("example.test", path, "").is_err(),
            "path: {path}"
        );
    }
    assert!(server.serve("missing.test", "/photo.png", "").is_err());
    assert!(server.serve("..", "/private.txt", "").is_err());
    Ok(())
}

#[test]
fn contain_fits_inside_the_box_without_distorting_pixels() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    for query in ["w=2&h=2&fit=contain", "w=2&h=2"] {
        let image = decode_png(&server.serve("example.test", "/solid.png", query)?)?;
        assert_eq!((image.0, image.1), (2, 1), "query: {query}");
        assert_eq!(image.2, GREEN.repeat(2));
    }
    let portrait =
        decode_png(&server.serve("example.test", "/portrait.png", "w=2&h=2&fit=contain")?)?;
    assert_eq!((portrait.0, portrait.1), (1, 2));
    Ok(())
}

#[test]
fn contain_can_upscale_but_scale_down_cannot() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    let enlarged = decode_png(&request(&server, "w=8&h=8&fit=contain")?)?;
    assert_eq!((enlarged.0, enlarged.1), (8, 4));
    let unchanged = decode_png(&request(&server, "w=8&h=8&fit=scale-down")?)?;
    assert_eq!((unchanged.0, unchanged.1), (4, 2));
    assert_eq!(unchanged.2, [RED, GREEN, BLUE, WHITE].concat().repeat(2));
    let smaller = decode_png(&request(&server, "w=2&h=2&fit=scale-down")?)?;
    assert_eq!((smaller.0, smaller.1), (2, 1));
    Ok(())
}

#[test]
fn cover_crops_the_centre_in_both_orientations() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    let landscape = decode_png(&request(&server, "w=2&h=2&fit=cover")?)?;
    assert_eq!((landscape.0, landscape.1), (2, 2));
    assert_eq!(landscape.2, [GREEN, BLUE].concat().repeat(2));
    let portrait =
        decode_png(&server.serve("example.test", "/portrait.png", "w=2&h=2&fit=cover")?)?;
    assert_eq!((portrait.0, portrait.1), (2, 2));
    assert_eq!(portrait.2, [GREEN.repeat(2), BLUE.repeat(2)].concat());
    let enlarged = decode_png(&server.serve("example.test", "/solid.png", "w=6&h=6&fit=cover")?)?;
    assert_eq!((enlarged.0, enlarged.1), (6, 6));
    assert_eq!(enlarged.2, GREEN.repeat(36));
    Ok(())
}

#[test]
fn fill_stretches_to_both_requested_dimensions() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    let stretched = decode_png(&request(&server, "w=4&h=4&fit=fill")?)?;
    assert_eq!((stretched.0, stretched.1), (4, 4));
    assert_eq!(stretched.2, [RED, GREEN, BLUE, WHITE].concat().repeat(4));
    let narrow = decode_png(&server.serve("example.test", "/solid.png", "w=1&h=5&fit=fill")?)?;
    assert_eq!((narrow.0, narrow.1), (1, 5));
    assert_eq!(narrow.2, GREEN.repeat(5));
    Ok(())
}

#[test]
fn single_axis_and_long_aliases_preserve_the_source_aspect_ratio() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    for query in [
        "w=2",
        "width=2",
        "h=1",
        "height=1",
        "width=2&quality=100&format=png",
        "tracking=ignored&w=2&height_hint=80",
        "%77=%32&%71=100&format=%70ng",
    ] {
        let image = decode_png(&request(&server, query)?)?;
        assert_eq!((image.0, image.1), (2, 1), "query: {query}");
    }
    let enlarged = decode_png(&request(&server, "h=4")?)?;
    assert_eq!((enlarged.0, enlarged.1), (8, 4));
    fixture.write_png("odd.png", 3, 2, &GREEN.repeat(6))?;
    let rounded = decode_png(&server.serve("example.test", "/odd.png", "w=4")?)?;
    assert_eq!((rounded.0, rounded.1), (4, 3));
    assert_eq!(rounded.2, GREEN.repeat(12));
    Ok(())
}

#[test]
fn raw_rgba_outputs_the_actual_pixels_without_a_png_wrapper() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    let raw = request(&server, "format=raw_rgba")?;
    assert_eq!(raw.content_type, "application/octet-stream");
    assert_eq!(raw.body, [RED, GREEN, BLUE, WHITE].concat().repeat(2));
    let cropped = request(&server, "w=2&h=2&fit=cover&format=raw_rgba&q=1")?;
    assert_eq!(cropped.body, [GREEN, BLUE].concat().repeat(2));
    Ok(())
}

#[test]
fn malformed_transform_options_are_errors_not_original_file_fallbacks() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    for query in [
        "w=0",
        "width=-1",
        "h=no",
        "height=0",
        "fit=crop",
        "fit=",
        "format=jpeg",
        "format=",
        "q=0",
        "quality=101",
        "quality=256",
        "q=bad",
        "w=2&w=3",
        "width=2&w=3",
        "q=10&quality=20",
        "fit=contain&fit=cover",
        "w=%zz",
        "w=999999999999999999999999",
    ] {
        let error = request(&server, query).expect_err(query);
        assert!(!error.to_string().is_empty(), "query: {query}");
    }
    for query in ["q=1", "quality=100"] {
        let image = decode_png(&request(&server, query)?)?;
        assert_eq!((image.0, image.1), (4, 2));
    }
    Ok(())
}

#[test]
fn invalid_image_configuration_is_refused_during_loading() -> TestResult {
    let fixture = Fixture::new()?;
    for settings in [
        "max_width=0",
        "max_height=0",
        "max_width=-1",
        "max_height='large'",
        "default_quality=0",
        "default_quality=101",
        "default_quality=-1",
        "max_file_size=0",
        "max_wdth=4",
    ] {
        assert!(
            Config::from_toml(&fixture.config_text(settings)).is_err(),
            "settings: {settings}"
        );
    }
    for settings in [
        "default_quality=1",
        "default_quality=100",
        "max_width=1\nmax_height=1",
    ] {
        assert!(
            Config::from_toml(&fixture.config_text(settings)).is_ok(),
            "settings: {settings}"
        );
    }
    Ok(())
}

#[test]
fn both_requested_and_inferred_dimensions_obey_limits() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("max_width=4\nmax_height=2")?;
    let accepted = decode_png(&request(&server, "w=4&h=2&fit=fill")?)?;
    assert_eq!((accepted.0, accepted.1), (4, 2));
    for query in [
        "w=5",
        "h=3",
        "w=5&h=1&fit=contain",
        "w=1&h=3&fit=scale-down",
    ] {
        assert!(request(&server, query).is_err(), "query: {query}");
    }
    assert!(request(&fixture.server("max_width=3")?, "h=2").is_err());
    assert!(request(&fixture.server("max_height=1")?, "w=4").is_err());
    Ok(())
}

#[test]
fn source_byte_limit_is_inclusive_and_does_not_change_original_serving() -> TestResult {
    let fixture = Fixture::new()?;
    let source = fs::read(fixture.site().join("photo.png"))?;
    let exact = fixture.server(&format!("max_file_size={}", source.len()))?;
    assert_eq!(decode_png(&request(&exact, "w=2")?)?.0, 2);
    let limited = fixture.server(&format!("max_file_size={}", source.len() - 1))?;
    assert!(request(&limited, "w=2").is_err());
    assert_eq!(request(&limited, "")?.body, source);
    let disabled = fixture.server("enabled=false\nmax_file_size=1\nmax_width=1\nmax_height=1")?;
    assert_eq!(request(&disabled, "w=99999&format=bad&q=0")?.body, source);
    fs::write(fixture.site().join("broken.png"), b"invalid png bytes")?;
    assert!(
        fixture
            .server("")?
            .serve("example.test", "/broken.png", "w=2")
            .is_err()
    );
    let mut oversized = source;
    oversized.resize(10 * 1024 * 1024 + 1, 0);
    fs::write(fixture.site().join("photo.png"), oversized)?;
    assert!(request(&fixture.server("")?, "w=2").is_err());
    Ok(())
}

#[test]
fn cached_outputs_are_reused_across_aliases_without_rewriting() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    let first = request(&server, "w=2&h=2&fit=cover&q=85&format=png")?;
    let files = fixture.cached_files()?;
    assert!(!files.is_empty(), "a transformed output must exist on disk");
    for path in &files {
        fs::File::options()
            .write(true)
            .open(path)?
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000))?;
    }
    let timestamps = files
        .iter()
        .map(|path| fs::metadata(path)?.modified())
        .collect::<Result<Vec<_>, _>>()?;
    let second = fixture.server("")?.serve(
        "alias.test",
        "/photo.png",
        "width=2&height=2&fit=cover&quality=85&format=png",
    )?;
    assert_eq!(second.body, first.body);
    assert_eq!(fixture.cached_files()?, files);
    let repeated = files
        .iter()
        .map(|path| fs::metadata(path)?.modified())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        repeated, timestamps,
        "a cache hit must not rewrite the stored output"
    );
    Ok(())
}

#[test]
fn every_result_shaping_option_has_a_distinct_cache_identity() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    let mut previous = Vec::new();
    for query in [
        "w=4&h=2&fit=fill&q=85&format=png",
        "w=3&h=2&fit=fill&q=85&format=png",
        "w=4&h=1&fit=fill&q=85&format=png",
        "w=4&h=2&fit=cover&q=85&format=png",
        "w=4&h=2&fit=fill&q=86&format=png",
        "w=4&h=2&fit=fill&q=85&format=raw_rgba",
    ] {
        request(&server, query)?;
        let current = fixture.cached_files()?;
        assert!(
            current.len() > previous.len(),
            "cache inputs omitted for {query}"
        );
        assert!(previous.iter().all(|file| current.contains(file)));
        previous = current;
    }
    let before = fixture.cached_files()?;
    request(
        &fixture.server("default_quality=87")?,
        "w=4&h=2&fit=fill&format=png",
    )?;
    assert!(
        fixture.cached_files()?.len() > before.len(),
        "configured default quality shapes cache identity"
    );
    Ok(())
}

#[test]
fn same_size_source_replacement_invalidates_cache_without_waiting() -> TestResult {
    let fixture = Fixture::new()?;
    let server = fixture.server("")?;
    let query = "format=raw_rgba";
    let initial = request(&server, query)?;
    let initial_files = fixture.cached_files()?;
    let source = fixture.site().join("photo.png");
    let metadata = fs::metadata(&source)?;
    let modified = metadata.modified()?;
    fixture.write_png(
        "photo.png",
        4,
        2,
        &[WHITE, BLUE, GREEN, RED].concat().repeat(2),
    )?;
    assert_eq!(
        fs::metadata(&source)?.len(),
        metadata.len(),
        "replacement has equal encoded length"
    );
    fs::File::options()
        .write(true)
        .open(&source)?
        .set_modified(modified)?;
    let changed = request(&server, query)?;
    assert_eq!(changed.body, [WHITE, BLUE, GREEN, RED].concat().repeat(2));
    assert_ne!(changed.body, initial.body);
    assert!(fixture.cached_files()?.len() > initial_files.len());
    Ok(())
}
