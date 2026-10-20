//! The command line, parsed once. Paths stay native: the system reads any bytes, so the tool refuses none of them.
//! An option may be shortened to any prefix that names only it.

use std::env;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use clap::builder::{OsStringValueParser, TypedValueParser};
use clap::error::ErrorKind;
use clap::{CommandFactory as _, Parser};

/// The pin files the command line names: the toolchain files in the order given, and the lock, each path tidied.
#[derive(Debug, PartialEq, Eq, Parser)]
#[command(
    name = "pin-age",
    about = "Say which toolchain pins are due to move.",
    infer_long_args = true
)]
pub(crate) struct Arguments {
    /// A rust-toolchain.toml; may be repeated.
    #[arg(long, value_parser = tidied())]
    pub(crate) toolchain: Vec<PathBuf>,
    /// A flake.lock; the last one given is read.
    #[arg(long, value_parser = tidied(), overrides_with = "lock")]
    pub(crate) lock: Option<PathBuf>,
}

impl Arguments {
    /// The process's command line; clap prints a malformed one's refusal and exits 2, and prints the help and exits 0.
    pub(crate) fn parse() -> Self {
        Self::parse_from(env::args_os()).unwrap_or_else(|error| error.exit())
    }

    /// `arguments`, the program's name first; refused when malformed or when it names no file, since the command
    /// line must name at least one.
    fn parse_from(
        arguments: impl IntoIterator<Item = impl Into<OsString> + Clone>,
    ) -> Result<Self, clap::Error> {
        let parsed = Self::try_parse_from(arguments)?;
        if parsed.toolchain.is_empty() && parsed.lock.is_none() {
            return Err(Self::command().error(
                ErrorKind::MissingRequiredArgument,
                "give at least one --toolchain or --lock",
            ));
        }
        Ok(parsed)
    }
}

/// Reads a path argument as given, in any bytes, and tidies it.
fn tidied() -> impl TypedValueParser<Value = PathBuf> {
    OsStringValueParser::new().map(|raw| tidy(Path::new(&raw)))
}

/// `path` without its `.` components and with its repeated and trailing separators removed; `.` for a path left
/// empty. The file is read and named by this path.
fn tidy(path: &Path) -> PathBuf {
    let tidy: PathBuf = path
        .components()
        .filter(|component| *component != Component::CurDir)
        .collect();
    if tidy.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        tidy
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn each_toolchain_is_kept_in_order_and_the_last_lock_is_read() {
        let parsed = Arguments::parse_from([
            "pin-age",
            "--toolchain",
            "b.toml",
            "--lock",
            "old.lock",
            "--toolchain=a.toml",
            "--lock",
            "flake.lock",
        ])
        .expect("parsed");
        assert_eq!(
            parsed,
            Arguments {
                toolchain: vec![PathBuf::from("b.toml"), PathBuf::from("a.toml")],
                lock: Some(PathBuf::from("flake.lock")),
            }
        );
    }

    #[test]
    fn each_path_is_tidied_as_it_is_read() {
        let parsed = Arguments::parse_from([
            "pin-age",
            "--toolchain",
            "./a.toml",
            "--lock",
            "./b//flake.lock",
        ])
        .expect("parsed");
        assert_eq!(parsed.toolchain[0].as_os_str(), OsStr::new("a.toml"));
        assert_eq!(
            parsed.lock.as_deref().map(Path::as_os_str),
            Some(OsStr::new("b/flake.lock"))
        );
    }

    #[test]
    fn an_option_may_be_shortened_to_a_prefix_only_it_has() {
        let parsed = Arguments::parse_from(["pin-age", "--tool", "a.toml", "--l", "flake.lock"])
            .expect("parsed");
        assert_eq!(parsed.toolchain, [PathBuf::from("a.toml")]);
        assert_eq!(parsed.lock, Some(PathBuf::from("flake.lock")));
    }

    #[test]
    fn naming_no_file_is_a_usage_error() {
        let refused = Arguments::parse_from(["pin-age"]).expect_err("refused");
        assert_eq!(refused.exit_code(), 2);
        assert!(
            refused
                .to_string()
                .contains("give at least one --toolchain or --lock"),
            "{refused}"
        );
    }

    #[test]
    fn an_unknown_option_or_an_operand_is_a_usage_error() {
        for arguments in [
            &["pin-age", "--lock", "flake.lock", "--frozen"][..],
            &["pin-age", "--lock", "flake.lock", "extra"],
            &["pin-age", "--lock"],
        ] {
            let refused = Arguments::parse_from(arguments).expect_err("refused");
            assert_eq!(refused.exit_code(), 2, "{arguments:?}");
        }
    }

    #[test]
    fn a_path_is_named_without_its_dot_components_and_spare_separators() {
        for (given, named) in [
            (
                "./tools//./rust-toolchain.toml",
                "tools/rust-toolchain.toml",
            ),
            ("flake.lock/", "flake.lock"),
            ("/abs/../flake.lock", "/abs/../flake.lock"),
            ("", "."),
            ("./", "."),
        ] {
            assert_eq!(
                tidy(Path::new(given)).as_os_str(),
                OsStr::new(named),
                "{given:?}"
            );
        }
    }
}
