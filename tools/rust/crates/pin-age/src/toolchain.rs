//! A `rust-toolchain.toml`'s pin: a dated nightly, published on its date, or a numbered release, which is not aged.

use std::path::Path;

use crate::date::Date;
use crate::error::{Cause, FieldProblem, PinError, read_text};
use crate::pins::Pin;

/// The field the pin is read from.
const CHANNEL: &str = "[toolchain] channel";

/// The Rust toolchain the file at `path` pins, or `None` for a numbered release, which carries no date. Refused for a
/// file that cannot be read or parsed, a missing channel, or a channel that is neither a dated nightly on a real day,
/// `nightly-YYYY-MM-DD`, nor a numbered release, such as `1.92` or `1.92.0`.
pub(crate) fn toolchain_pin(path: &Path) -> Result<Option<Pin>, PinError> {
    let text = read_text(path)?;
    let document: toml::Table = text
        .parse()
        .map_err(|error| PinError::new(path, Cause::Toml(one_line(&error, &text))))?;
    let channel = document
        .get("toolchain")
        .and_then(toml::Value::as_table)
        .and_then(|toolchain| toolchain.get("channel"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| PinError::field(path, CHANNEL, FieldProblem::NotAString))?;
    if is_release(channel) {
        return Ok(None);
    }
    let day = channel
        .strip_prefix("nightly-")
        .and_then(DayDigits::read)
        .ok_or_else(|| PinError::field(path, CHANNEL, FieldProblem::NotAPin(channel.to_owned())))?;
    let published = Date::new(day.year, day.month, day.day).ok_or_else(|| {
        PinError::field(path, CHANNEL, FieldProblem::NoRealDay(channel.to_owned()))
    })?;
    Ok(Some(Pin::new(
        format!("the Rust {channel} in {}", path.display()),
        published,
    )))
}

/// Whether `channel` is a numbered release: two or three dot-separated runs of ASCII digits.
fn is_release(channel: &str) -> bool {
    let mut parts = channel.split('.');
    (2..=3).contains(&parts.clone().count())
        && parts.all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The numbers written as `YYYY-MM-DD` in ASCII digits, whether or not they name a real day.
struct DayDigits {
    year: u16,
    month: u8,
    day: u8,
}

impl DayDigits {
    fn read(text: &str) -> Option<Self> {
        let mut fields = text.split('-');
        let (Some(year), Some(month), Some(day), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return None;
        };
        let digits = |field: &str, length: usize| {
            field.len() == length && field.bytes().all(|byte| byte.is_ascii_digit())
        };
        (digits(year, 4) && digits(month, 2) && digits(day, 2)).then_some(())?;
        Some(Self {
            year: year.parse().ok()?,
            month: month.parse().ok()?,
            day: day.parse().ok()?,
        })
    }
}

/// The parser's message on one line, with the line and column where it stopped when it knows them.
fn one_line(error: &toml::de::Error, text: &str) -> String {
    let message = error.message().replace('\n', "; ");
    let Some(start) = error.span().map(|span| span.start) else {
        return message;
    };
    let before = text.get(..start).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before, |(_, last)| last)
        .chars()
        .count()
        + 1;
    format!("{message} (at line {line}, column {column})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Scratch, toolchain};

    #[test]
    fn a_dated_nightly_was_published_on_its_date() {
        let folder = Scratch::new("nightly");
        let path = folder.write("rust-toolchain.toml", &toolchain("nightly-2026-09-24"));
        assert_eq!(
            toolchain_pin(&path).expect("read"),
            Some(Pin::new(
                format!("the Rust nightly-2026-09-24 in {}", path.display()),
                Date::new(2026, 9, 24).expect("a real day")
            ))
        );
    }

    #[test]
    fn a_numbered_release_is_not_aged() {
        let folder = Scratch::new("release");
        for channel in ["1.92.0", "1.92"] {
            let path = folder.write("rust-toolchain.toml", &toolchain(channel));
            assert_eq!(toolchain_pin(&path).expect("read"), None, "{channel}");
        }
    }

    #[test]
    fn a_channel_that_moves_by_itself_or_is_shaped_wrong_is_refused() {
        let folder = Scratch::new("moving");
        for channel in [
            "stable",
            "nightly",
            "beta",
            "1",
            "1.92.0.1",
            "1..2",
            "beta-2026-09-24",
            "nightly-2026-9-24",
            "nightly-2026-09-24-x",
        ] {
            let path = folder.write("rust-toolchain.toml", &toolchain(channel));
            assert_eq!(
                toolchain_pin(&path).expect_err("refused").to_string(),
                format!(
                    "{}: [toolchain] channel: \"{channel}\" is not a dated nightly or a numbered release",
                    path.display()
                )
            );
        }
    }

    #[test]
    fn a_nightly_on_a_day_that_does_not_exist_is_refused() {
        let folder = Scratch::new("no-day");
        for channel in ["nightly-2026-02-30", "nightly-0000-01-01"] {
            let path = folder.write("rust-toolchain.toml", &toolchain(channel));
            assert_eq!(
                toolchain_pin(&path).expect_err("refused").to_string(),
                format!(
                    "{}: [toolchain] channel: \"{channel}\" names no real day",
                    path.display()
                )
            );
        }
    }

    #[test]
    fn a_file_without_a_channel_string_is_refused() {
        let folder = Scratch::new("no-channel");
        for text in [
            "[toolchain]\nprofile = \"minimal\"\n",
            "[toolchain]\nchannel = 1\n",
            "toolchain = \"nightly-2026-09-24\"\n",
            "",
        ] {
            let path = folder.write("rust-toolchain.toml", text);
            assert_eq!(
                toolchain_pin(&path).expect_err("refused").to_string(),
                format!(
                    "{}: [toolchain] channel: missing, or not a string",
                    path.display()
                ),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_file_that_is_not_toml_is_refused_in_one_line_with_where_it_stopped() {
        let folder = Scratch::new("not-toml");
        let path = folder.write("rust-toolchain.toml", "[toolchain\n");
        let refusal = toolchain_pin(&path).expect_err("refused").to_string();
        assert!(
            refusal.starts_with(&format!("{}: is not TOML: ", path.display())),
            "{refusal}"
        );
        assert!(refusal.ends_with(" (at line 1, column 11)"), "{refusal}");
        assert!(!refusal.contains('\n'), "{refusal}");
    }

    #[test]
    fn a_missing_file_is_refused() {
        let folder = Scratch::new("missing");
        let path = folder.path().join("rust-toolchain.toml");
        assert_eq!(
            toolchain_pin(&path).expect_err("refused").to_string(),
            format!(
                "{}: cannot be read: No such file or directory",
                path.display()
            )
        );
    }
}
