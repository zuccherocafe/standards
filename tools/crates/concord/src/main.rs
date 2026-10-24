//! Checks the shape of a project's concord notes, one living file per area under `docs/concord/`. This is the
//! format's reference; `skills/concord/SKILL.md` says when a concord note is due and how it changes, and
//! `templates/CONCORD.md` is the shape to copy.
//!
//! ```text
//! concord check [<file>]...
//! ```
//!
//! With no file, it checks every `*.md` file directly in `docs/concord/` under the working directory except
//! `README.md`, in name order. Each concord note is checked alone. The tool reads two parts of it and leaves every
//! other section as prose for people. Headings, tables and fields inside fenced code are examples, not the file's own
//! ([`markdown`]).
//!
//! - **Header**: the `- Field: value` lines before the first level-two heading, which are exactly `Area`,
//!   `Last full read` and `Inspected`, each given once and non-empty. `Last full read` is a full commit id, 40
//!   lowercase hexadecimal digits.
//! - **`## Disagreements`**: one section holding one table whose columns are exactly `Concept | Owner | Sites |
//!   Consequence | Done when`, followed by a delimiter row of five cells. Every row has five non-empty cells. No two
//!   rows have the same Concept, compared after trimming and ignoring case. A file with no open disagreements says
//!   `None found.` in the section instead of a table, and never beside one, even one without rows ([`read`]).
//!
//! It does not check the prose sections, whether the commit in `Last full read` exists, whether the sites a row names
//! exist or are right, whether a disagreement is still open, or the folder's `README.md`. Nothing here reads the code
//! a concord note names.
//!
//! Each file that passes prints `<file>: passes` on standard output. Each refusal prints one line on standard error,
//! `<file>:<line>: <field>: <reason>`, where the field is a header field, a column, or a section's title. A file that
//! cannot be read as UTF-8 text prints `<file>: cannot be read: <reason>` on standard error. Every file is checked
//! whatever an earlier one gave. The exit status is 0 when every file passes, 1 when any is refused and all could be
//! read, and 2 for a usage fault: a malformed command line, a file that cannot be read, or, with no file named, a
//! `docs/concord/` folder that cannot be listed.

mod markdown;
mod read;

use std::fs;
use std::io;
use std::process::ExitCode;

use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};

use crate::read::Refusal;

/// The exit status when any file is refused.
const REFUSED: u8 = 1;
/// The exit status for a usage fault.
const USAGE: u8 = 2;
/// The folder checked when no file is named, relative to the working directory.
const FOLDER: &str = "docs/concord";
/// The folder's description for readers, which is not a concord note.
const README: &str = "README.md";

#[derive(Debug, Parser)]
#[command(name = "concord", about = "Check concord notes.")]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check each concord note's shape.
    Check {
        /// The concord notes, Markdown; every one in `docs/concord/` when none is named.
        files: Vec<Utf8PathBuf>,
    },
}

fn main() -> ExitCode {
    let Command::Check { files } = Arguments::parse().command;
    let mut out = io::stdout().lock();
    let mut errors = io::stderr().lock();
    // Standard error is where a failure to write is reported, so a failure to write there has nowhere to go.
    let written = run(files, &mut out, &mut errors);
    written.map_or(ExitCode::FAILURE, ExitCode::from)
}

/// Checks `files`, or the folder's concord notes when none is named, writing each result; the exit status.
fn run(
    files: Vec<Utf8PathBuf>,
    out: &mut impl io::Write,
    errors: &mut impl io::Write,
) -> io::Result<u8> {
    let files = if files.is_empty() {
        match concord_notes(Utf8Path::new(FOLDER)) {
            Ok(files) => files,
            Err(error) => {
                writeln!(errors, "{FOLDER}: cannot be listed: {error}")?;
                return Ok(USAGE);
            }
        }
    } else {
        files
    };
    let mut status = 0;
    for file in &files {
        match check(file) {
            Ok(refusals) if refusals.is_empty() => writeln!(out, "{file}: passes")?,
            Ok(refusals) => {
                for refusal in &refusals {
                    writeln!(errors, "{file}:{refusal}")?;
                }
                status = status.max(REFUSED);
            }
            Err(error) => {
                writeln!(errors, "{file}: cannot be read: {error}")?;
                status = USAGE;
            }
        }
    }
    Ok(status)
}

/// Every `*.md` file directly in `folder` except its README, in name order.
fn concord_notes(folder: &Utf8Path) -> io::Result<Vec<Utf8PathBuf>> {
    let mut files = Vec::new();
    for entry in folder.read_dir_utf8()? {
        let entry = entry?;
        let path = entry.path();
        if fs::metadata(path)?.is_file()
            && path.extension() == Some("md")
            && entry.file_name() != README
        {
            files.push(path.to_owned());
        }
    }
    files.sort();
    Ok(files)
}

/// The refusals of the concord note at `file`, sorted by line for reading; the order is not a promise.
fn check(file: &Utf8Path) -> io::Result<Vec<Refusal>> {
    let text = fs::read_to_string(file)?;
    let mut refusals = read::refusals(&text);
    refusals.sort_by_key(Refusal::line);
    Ok(refusals)
}
