//! A pin file that cannot be used, and reading one: every refusal names the file, and the field where one applies.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A pin file that cannot be used, with the file always named and the field only where one applies.
#[derive(Debug, thiserror::Error)]
#[error("{}: {cause}", .file.display())]
pub(crate) struct PinError {
    file: PathBuf,
    cause: Cause,
}

impl PinError {
    pub(crate) fn new(file: &Path, cause: Cause) -> Self {
        Self {
            file: file.to_owned(),
            cause,
        }
    }

    /// The file's `field` has `problem`.
    pub(crate) fn field(file: &Path, field: impl Into<String>, problem: FieldProblem) -> Self {
        Self::new(
            file,
            Cause::Field {
                field: field.into(),
                problem,
            },
        )
    }
}

/// Why a pin file cannot be used. Each cause is one line, its detail inline.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Cause {
    #[error("cannot be read: {}", Strerror(.0))]
    Read(io::Error),
    #[error("is not UTF-8 text")]
    NotUtf8,
    /// The parser's message, on one line.
    #[error("is not TOML: {0}")]
    Toml(String),
    /// The parser's message, on one line.
    #[error("is not JSON: {0}")]
    Json(String),
    #[error("is not a JSON object")]
    NotAJsonObject,
    #[error("{field}: {problem}")]
    Field {
        field: String,
        problem: FieldProblem,
    },
}

/// What is wrong with one field of a pin file.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FieldProblem {
    #[error("missing, or not a string")]
    NotAString,
    #[error("missing, or not an object")]
    NotAnObject,
    #[error("the root node is missing, or not an object")]
    NoRootNode,
    /// A present field that is not an object, where a missing one is empty.
    #[error("not an object")]
    PresentButNotAnObject,
    #[error("neither a node name nor a follows path")]
    NotAReference,
    #[error("names node {0:?}, which is missing")]
    NoSuchNode(String),
    #[error("{0:?} is not a dated nightly or a numbered release")]
    NotAPin(String),
    #[error("{0:?} names no real day")]
    NoRealDay(String),
    #[error("{0} is not an integer")]
    NotAnInteger(Box<serde_json::Value>),
    #[error("{0} is not a time")]
    NotATime(i128),
}

/// The UTF-8 text of the file at `path`.
pub(crate) fn read_text(path: &Path) -> Result<String, PinError> {
    let bytes = fs::read(path).map_err(|error| PinError::new(path, Cause::Read(error)))?;
    String::from_utf8(bytes).map_err(|_| PinError::new(path, Cause::NotUtf8))
}

/// The system's description of an error, without the number the standard library appends to it.
struct Strerror<'a>(&'a io::Error);

impl fmt::Display for Strerror<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self.0.to_string();
        let number = self
            .0
            .raw_os_error()
            .map(|code| format!(" (os error {code})"));
        f.write_str(
            number
                .and_then(|number| text.strip_suffix(&number))
                .unwrap_or(&text),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scratch;

    #[test]
    fn a_file_that_cannot_be_read_is_refused_with_the_systems_reason() {
        let folder = Scratch::new("read");
        let missing = folder.path().join("missing.toml");
        assert_eq!(
            read_text(&missing).expect_err("refused").to_string(),
            format!(
                "{}: cannot be read: No such file or directory",
                missing.display()
            )
        );
        assert_eq!(
            read_text(folder.path()).expect_err("refused").to_string(),
            format!(
                "{}: cannot be read: Is a directory",
                folder.path().display()
            )
        );
    }

    #[test]
    fn a_file_that_is_not_utf8_is_refused() {
        let folder = Scratch::new("utf8");
        let path = folder.path().join("flake.lock");
        fs::write(&path, b"{\"root\": \"\xff\"}").expect("the scratch folder is writable");
        assert_eq!(
            read_text(&path).expect_err("refused").to_string(),
            format!("{}: is not UTF-8 text", path.display())
        );
    }
}
