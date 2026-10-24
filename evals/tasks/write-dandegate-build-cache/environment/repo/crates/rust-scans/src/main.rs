//! Scans a Rust workspace for I/O in core crates, unit-suffixed names, codes in error messages and async code.
//! The four scans are review aids that read the source as Rust syntax and print what they find, failing nothing
//! (rules/languages/rust/documentation.md, Report scans).
//!
//! - H1 reports file, process, environment and clock access (`std::fs`, `std::process`, `std::env`,
//!   `std::time::Instant`, `std::time::SystemTime`) in the crates named as cores.
//! - H2 reports a primitive field or parameter whose name ends in a unit suffix, such as `margin_metres: f64`, outside
//!   test code and the modules named as kernels. The built-in suffixes are `metres`, `seconds`, `degrees`, `radians`
//!   and `hz`; `--unit` adds a project's own.
//! - H3 reports a code in an `#[error(...)]` message, in every crate.
//! - H4 reports, in the cores, a path through an async runtime, a network client or async I/O (`tokio`, `async_std`,
//!   `smol`, `futures::io`, `std::net`, `reqwest`, `hyper`, `h3`, `quinn`, `wtransport`, `sqlx`, `libsql`), and every
//!   `async fn`, `async` block or closure, and `.await`.
//!
//! H1 and H4 read test code too and mark it. The scans know only syntax. Imports are approximated file-wide (see
//! [`scan`]), so a name imported in one module of a file is resolved in all of it, and a local module spelt like a
//! flagged crate is reported; a re-export in another file, a glob's names and a macro's arguments are missed.
//!
//! The one clock read times the run for the last line, `seconds:`, and changes no finding.
//! The process and file work is in [`workspace`]; [`scan`] and [`source`] are functions over parsed source, tested on
//! made-up text; this file runs them and prints, and is the one place errors are rendered.

mod options;
mod scan;
mod source;
mod workspace;

use std::error::Error;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;

use crate::options::{Arguments, Options};
use crate::scan::{Finding, Rules, Scan, scan_file};
use crate::source::{SourceFile, declared_test_modules, module_folder, test_file, within};
use crate::workspace::{Member, MetadataError, ReadError, Workspace, parse, sources};

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    let started = Instant::now();
    let (folder, options) = arguments.into_parts();
    let printed = run(&folder, &options)
        .map_err(Failure::Scan)
        .and_then(|findings| print(&findings, started).map_err(Failure::Print));
    match printed {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            render(&failure);
            ExitCode::FAILURE
        }
    }
}

/// Every finding of every scan over the workspace in `folder`, member by member in folder order.
fn run(folder: &Utf8Path, options: &Options) -> Result<Vec<Finding>, ScanError> {
    let workspace = Workspace::discover(folder)?;
    let current = std::env::current_dir()
        .ok()
        .and_then(|current| Utf8PathBuf::try_from(current).ok());
    let rules = Rules::new(options.units());
    let mut findings = Vec::new();
    for member in workspace.members() {
        findings.extend(scan_member(member, options, &rules, current.as_deref())?);
    }
    Ok(findings)
}

/// One member's findings, file by file in path order; each file is shown relative to `current` when it is inside it.
fn scan_member(
    member: &Member,
    options: &Options,
    rules: &Rules,
    current: Option<&Utf8Path>,
) -> Result<Vec<Finding>, ReadError> {
    let src = member.src();
    let parsed = sources(&src)?
        .into_iter()
        .map(|path| Ok((parse(&path)?, path)))
        .collect::<Result<Vec<_>, _>>()?;
    let tests: Vec<Utf8PathBuf> = parsed
        .iter()
        .flat_map(|(source, path)| {
            declared_test_modules(source, module_folder(path, member.roots()))
        })
        .collect();
    let kernels: Vec<Utf8PathBuf> = options
        .kernels(member.name())
        .iter()
        .map(|module| module.under(&src))
        .collect();
    let mut findings = Vec::new();
    for (source, path) in &parsed {
        let under_src = path.strip_prefix(&src).unwrap_or(path);
        let test_code = test_file(under_src) || tests.iter().any(|module| within(path, module));
        let kernel = kernels.iter().any(|module| within(path, module));
        let shown = current
            .and_then(|current| path.strip_prefix(current).ok())
            .unwrap_or(path);
        let file = SourceFile::new(
            shown.to_owned(),
            member.name().clone(),
            test_code,
            kernel,
            options.is_core(member.name()),
        );
        findings.extend(scan_file(source, &file, rules));
    }
    Ok(findings)
}

/// The findings grouped by scan, then a count per scan, zero counts included, then the time since `started`.
fn print(findings: &[Finding], started: Instant) -> io::Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    for scan in Scan::ALL {
        for finding in findings.iter().filter(|finding| finding.scan() == scan) {
            writeln!(out, "{finding}")?;
        }
    }
    for scan in Scan::ALL {
        let count = findings
            .iter()
            .filter(|finding| finding.scan() == scan)
            .count();
        writeln!(out, "{scan}: {count}")?;
    }
    writeln!(out, "seconds: {:.3}", started.elapsed().as_secs_f64())?;
    out.flush()
}

/// The error and each of its causes, once, on standard error.
fn render(failure: &Failure) {
    let mut errors = io::stderr().lock();
    let mut cause: Option<&dyn Error> = Some(failure);
    let mut lead = "rust-scans: ";
    while let Some(error) = cause {
        // Standard error is where a failure to write is reported, so a failure to write there has nowhere to go.
        let _ = writeln!(errors, "{lead}{error}");
        lead = "  caused by: ";
        cause = error.source();
    }
}

/// Why the workspace could not be scanned.
#[derive(Debug, thiserror::Error)]
enum ScanError {
    #[error(transparent)]
    Metadata(#[from] MetadataError),
    #[error(transparent)]
    Read(#[from] ReadError),
}

/// Why the command failed: the scan, or printing its findings.
#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error(transparent)]
    Scan(ScanError),
    #[error("could not print the findings")]
    Print(#[source] io::Error),
}
