//! Checks a design read's shape and that it carries forward what the previous read left open. This is the format's
//! reference; `skills/design-read/SKILL.md` says when a read is due and what stays judgement, and
//! `templates/DESIGN-READ.md` is the shape to copy.
//!
//! ```text
//! design-read check <file>
//! ```
//!
//! A design read is one Markdown file. The tool reads three parts of it and leaves every other section as prose for
//! people. Headings, tables and fields inside fenced code are examples, not the read's own ([`markdown`]).
//!
//! - **Header**: the `- Field: value` lines before the first level-two heading. `Area`, `Revision`, `Previous` and
//!   `Inspected` are each given once and non-empty. `Revision` is a full commit id, 40 lowercase hexadecimal digits.
//!   `Previous` is `none` for a first read, or a path relative to the read naming a file that can be read.
//! - **`## After the last phase`**: present and non-empty when `Previous` names a read, absent when it is `none`.
//! - **`## Disagreements`**: one section holding one table whose columns are exactly `ID | Concept | Owner | Sites |
//!   Consequence | Done when | Status`, followed by a delimiter row of seven cells. Every row has seven non-empty
//!   cells. An ID is `<read>-D<n>`, the read's name in lowercase letters, digits and hyphens and `n` a positive number,
//!   and is unique in the read. A status is `new`, `open`, `resolved` or `parked`. A read with no rows says
//!   `None found.` in the section instead of a table, and never beside rows ([`read`]).
//!
//! Against the previous read's table, or against no rows for a first read, an ID is `new` exactly when the previous
//! read lacks it, and every ID the previous read left `new`, `open` or `parked` appears again ([`carry`]). Only the
//! previous read's table is read: its own shape is not checked and its `Previous` link is not followed, so each read
//! is checked once, when it is written. Nothing here reads the code a read names.
//!
//! A read that passes exits 0 and prints `<file>: passes: disagreements: <n> new, <n> open, <n> resolved, <n> parked`,
//! or `<file>: passes: none found`, on standard output. A refused read exits 1 and prints one line per refusal on
//! standard error, `<file>:<line>: <field>: <reason>`, where the field is a header field, a column, or a section's
//! title. A malformed command line, or a read that cannot be read as UTF-8 text, exits 2.

mod carry;
mod markdown;
mod read;

use std::fs;
use std::io::{self, Write as _};
use std::process::ExitCode;

use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};

use crate::read::{Previous, Read, Refusal, Row};

/// The exit status for a read that is refused.
const REFUSED: u8 = 1;
/// The exit status for a read that cannot be read at all.
const UNUSABLE: u8 = 2;

#[derive(Debug, Parser)]
#[command(name = "design-read", about = "Check design reads.")]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check one read's shape and what it carries forward from its previous read.
    Check {
        /// The read, a Markdown file.
        file: Utf8PathBuf,
    },
}

fn main() -> ExitCode {
    let Command::Check { file } = Arguments::parse().command;
    let mut out = io::stdout().lock();
    let mut errors = io::stderr().lock();
    // Standard error is where a failure to write is reported, so a failure to write there has nowhere to go.
    let written = match check(&file) {
        Ok(passed) => writeln!(out, "{file}: passes: {passed}").map(|()| ExitCode::SUCCESS),
        Err(Failure::Refused(refusals)) => refusals
            .iter()
            .try_for_each(|refusal| writeln!(errors, "{file}:{refusal}"))
            .map(|()| ExitCode::from(REFUSED)),
        Err(Failure::Unusable(reason)) => {
            writeln!(errors, "{file}: {reason}").map(|()| ExitCode::from(UNUSABLE))
        }
    };
    written.unwrap_or(ExitCode::FAILURE)
}

/// Why a read does not pass.
#[derive(Debug)]
enum Failure {
    /// The read's refusals, sorted by line for reading; the order is not a promise.
    Refused(Vec<Refusal>),
    /// The file could not be read as text, so it has no lines to refuse.
    Unusable(String),
}

impl Failure {
    fn refused(mut refusals: Vec<Refusal>) -> Self {
        refusals.sort_by_key(Refusal::line);
        Self::Refused(refusals)
    }
}

/// The summary of the read at `file` when it passes, read with its previous read.
fn check(file: &Utf8Path) -> Result<String, Failure> {
    let text = fs::read_to_string(file)
        .map_err(|error| Failure::Unusable(format!("cannot be read: {error}")))?;
    let read = Read::parse(&text).map_err(Failure::refused)?;
    let refusals = match read.previous() {
        None => carry::refusals(&read, &[]),
        Some(previous) => match previous_rows(file, previous) {
            Ok(rows) => carry::refusals(&read, &rows),
            Err(refusal) => vec![refusal],
        },
    };
    if refusals.is_empty() {
        Ok(read.summary())
    } else {
        Err(Failure::refused(refusals))
    }
}

/// The rows of the previous read, found relative to `file`; refused when it cannot be read or has no table.
fn previous_rows(file: &Utf8Path, previous: &Previous) -> Result<Vec<Row>, Refusal> {
    let path = file
        .parent()
        .unwrap_or(Utf8Path::new(""))
        .join(previous.path());
    let text = fs::read_to_string(&path)
        .map_err(|error| previous.refuse(format!("{} cannot be read: {error}", previous.path())))?;
    read::rows_of(&text).map_err(|reason| {
        previous.refuse(format!(
            "{} has no table to compare with: {reason}",
            previous.path()
        ))
    })
}
