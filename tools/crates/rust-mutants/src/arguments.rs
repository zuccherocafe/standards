//! The command line, parsed once. Paths, the revision, the crate and the options after `--` stay native strings: git,
//! cargo-mutants and the file system take any bytes, so the tool refuses none of them itself.

use std::ffi::{OsStr, OsString};
use std::num::NonZeroU32;
use std::path::PathBuf;

use clap::{ArgGroup, Parser};

/// The command line as clap reads it, gathered into [`Arguments`] by [`Arguments::parse`].
#[derive(Debug, Parser)]
#[command(
    name = "rust-mutants",
    about = "Runs cargo-mutants over the Rust a change touched, or over a crate, and lists what the tests missed",
    long_about = LONG_ABOUT,
    group(ArgGroup::new("scope").required(true).args(["base", "package"]))
)]
struct CommandLine {
    /// The folder holding the workspace Cargo.toml (default: the current folder).
    #[arg(long, value_name = "FOLDER", default_value = ".")]
    workspace: PathBuf,
    /// Mutate only the Rust changed since this revision.
    #[arg(long, value_name = "REVISION")]
    base: Option<OsString>,
    /// Mutate every function in this crate.
    #[arg(long, value_name = "CRATE")]
    package: Option<OsString>,
    /// The report folder, outside the repository.
    #[arg(long, value_name = "FOLDER")]
    out: PathBuf,
    /// Copies tested at once.
    #[arg(long, value_name = "COUNT", default_value = "2")]
    shards: NonZeroU32,
    /// Options for cargo-mutants, after `--`.
    #[arg(value_name = "OPTIONS")]
    passed: Vec<OsString>,
}

const LONG_ABOUT: &str = "\
The Rust standard's mutation report: runs cargo-mutants over the Rust a change touched, or over a whole crate, and \
lists each mutant the tests missed for the writer to judge, and each that timed out for the writer to diagnose. It \
fails nothing.

  rust-mutants --workspace rust --base <revision> --out <folder outside the repository> [--shards 2] [-- <options>]
  rust-mutants --workspace rust --package <crate> --out <folder> [--shards 2] [-- <cargo-mutants options>]

Run it from inside the repository. Each shard tests its own copy of the repository under `--out`'s `rust-mutants` \
folder, which the tool empties first. The exit status is 0 when every shard finished, whatever the tests missed, 1 \
when the run could not finish, and 2 for a malformed argument. Needs `git`, `cargo mutants` and mold's `ld.mold` on \
PATH, a nightly Cargo, `systemd-run` for the memory cap and `unshare` for the PID namespaces.";

/// A revision as the caller wrote it; git checks it, the tool does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Revision(OsString);

impl From<OsString> for Revision {
    fn from(revision: OsString) -> Self {
        Self(revision)
    }
}

impl Revision {
    pub(crate) fn as_os_str(&self) -> &OsStr {
        &self.0
    }
}

/// A crate's name as the caller wrote it; cargo-mutants checks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Package(OsString);

impl From<OsString> for Package {
    fn from(name: OsString) -> Self {
        Self(name)
    }
}

impl Package {
    pub(crate) fn as_os_str(&self) -> &OsStr {
        &self.0
    }
}

/// What the run mutates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The Rust changed since the revision, in every workspace member.
    Changed(Revision),
    /// Every function in the crate.
    Package(Package),
}

/// The command line, parsed once.
#[derive(Debug)]
pub(crate) struct Arguments {
    /// As given: relative to the current folder, or absolute.
    pub(crate) workspace: PathBuf,
    pub(crate) scope: Scope,
    pub(crate) out: PathBuf,
    pub(crate) shards: NonZeroU32,
    /// The options after `--`, given to cargo-mutants unchanged.
    pub(crate) passed: Vec<OsString>,
}

impl Arguments {
    /// The process's command line; clap prints a malformed one's refusal and exits 2.
    pub(crate) fn parse() -> Self {
        CommandLine::parse().into()
    }
}

impl From<CommandLine> for Arguments {
    fn from(line: CommandLine) -> Self {
        let scope = match (line.base, line.package) {
            (Some(base), _) => Scope::Changed(base.into()),
            (None, package) => Scope::Package(
                package
                    .expect("clap requires exactly one of --base and --package")
                    .into(),
            ),
        };
        Self {
            workspace: line.workspace,
            scope,
            out: line.out,
            shards: line.shards,
            passed: line.passed,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    fn parse_from(arguments: &[&str]) -> Result<Arguments, clap::Error> {
        CommandLine::try_parse_from(arguments).map(Arguments::from)
    }

    #[test]
    fn a_base_and_a_package_together_are_refused() {
        let refused = parse_from(&[
            "rust-mutants",
            "--base",
            "main",
            "--package",
            "core",
            "--out",
            "/report",
        ])
        .expect_err("refused");
        assert_eq!(refused.kind(), ErrorKind::ArgumentConflict);
        assert_eq!(refused.exit_code(), 2);
    }

    #[test]
    fn a_scope_and_an_out_folder_are_required_and_shards_are_at_least_one() {
        for arguments in [
            &["rust-mutants", "--out", "/report"][..],
            &["rust-mutants", "--package", "core"],
            &[
                "rust-mutants",
                "--package",
                "core",
                "--out",
                "/report",
                "--shards",
                "0",
            ],
        ] {
            let refused = parse_from(arguments).expect_err("refused");
            assert_eq!(refused.exit_code(), 2, "{arguments:?}");
        }
    }

    #[test]
    fn the_options_after_the_separator_are_kept_whole_a_later_separator_included() {
        let parsed = parse_from(&[
            "rust-mutants",
            "--package",
            "core",
            "--out",
            "/report",
            "--",
            "--",
            "--profile=release",
        ])
        .expect("parsed");
        assert_eq!(parsed.passed, ["--", "--profile=release"]);
        assert_eq!(parsed.shards.get(), 2);
        assert_eq!(parsed.workspace, PathBuf::from("."));
        let bare = parse_from(&["rust-mutants", "--base", "main", "--out", "/report", "--"])
            .expect("parsed");
        assert_eq!(bare.passed, Vec::<OsString>::new());
        assert_eq!(bare.scope, Scope::Changed(OsString::from("main").into()));
    }
}
