//! dandegate compares two revisions of a project and writes an HTML report of what changed.
//!
//! It resolves both revisions to commits and checks each one out. It runs the steps in the project's `dandegate.toml`
//! in each checkout's own development shell, compares the images, measures and sections the steps write, and writes one
//! page. A difference never fails the command. When a step fails or leaves partial evidence, the top of the page says
//! that the report is incomplete.
//!
//! dandegate exits with 0 whenever it writes the page. It exits with 1 when it cannot write one, because the steps file
//! cannot run, a revision names no commit, the command line names an input or step that the steps file does not
//! declare, `--out` exists or is inside the repository, or dandegate cannot read the `--measures` or `--previous` file.
//!
//! dandegate knows revisions, steps, measures, images, sections, kept results and the page. It knows nothing about a
//! project's domain. A project's own scripts, its adapters in the after revision's `dandegate/` folder, do that work.
//! For a Rust workspace, dandegate adds the Rust reports as built-in steps ([`rust`]).
//!
//! dandegate runs steps on threads, not on an async runtime. It runs up to four steps at once, and runs exclusive steps
//! last, one at a time. It runs every git operation with the `git` command and every step under `nix develop`, below a
//! copy of itself started as `dandegate --supervise -- <command>`, which owns every process the step starts.
//!
//! - [`plan`] reads the steps file once into checked types. No later code checks them again.
//! - [`revision`] resolves the two revisions, with [`revision::git`].
//! - [`report`] does one report's work, from the checkouts to the page.
//! - [`compare`] compares the two sides' measures and images.
//! - [`page`] writes the HTML.
//! - [`rust`] runs the built-in Rust reports.

mod compare;
mod page;
mod plan;
mod report;
mod revision;
mod rust;

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write as _};
use std::process::ExitCode;

use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;

use crate::compare::measure::{LineError, Measures, SavedError};
use crate::plan::{Plan, PlanError};
use crate::report::Request;
use crate::report::step::supervise;
use crate::revision::PerSide;
use crate::revision::git::{Repository, ResolveError};

/// The command line.
#[derive(Debug, Parser)]
#[command(
    name = "dandegate",
    version,
    about = "Compares two revisions of a project and writes an HTML report of what changed. It fails nothing.",
    long_about = LONG_ABOUT
)]
struct Arguments {
    /// The revision the report starts from.
    #[arg(long, value_name = "REVISION")]
    before: String,
    /// The revision the report ends at.
    #[arg(long, value_name = "REVISION", default_value = "HEAD")]
    after: String,
    /// A new folder outside the repository for the page and everything it links.
    #[arg(long, value_name = "FOLDER")]
    out: Utf8PathBuf,
    /// Optional steps to run, with everything they need.
    #[arg(long, value_name = "STEP,...", value_delimiter = ',')]
    with: Vec<String>,
    /// A value for an input that dandegate.toml declares. Repeat it for each input.
    #[arg(long = "input", value_name = "NAME=VALUE")]
    inputs: Vec<String>,
    /// Run every step afresh, replacing what is kept.
    #[arg(long)]
    no_reuse: bool,
    /// A gate log or file of MEASURE lines for the after side. Steps find it in `DANDEGATE_MEASURES`.
    #[arg(long, value_name = "FILE")]
    measures: Option<Utf8PathBuf>,
    /// An earlier report folder, its measures.json, or a gate log, for the before side's measures.
    #[arg(long, value_name = "REPORT|FILE")]
    previous: Option<Utf8PathBuf>,
}

const LONG_ABOUT: &str = "\
Compares two revisions of a project and writes an HTML report of what changed.

dandegate resolves both revisions to commits and checks each one out under the repository's .cache/dandegate/scratch/. \
It reads dandegate.toml from the after revision and runs its steps in each checkout's `nix develop` shell. It \
compares the images, measures and sections the steps write, and writes <out>/index.html, with every file it links \
inside <out>. dandegate keeps each step's complete success in .cache/dandegate/ and reuses it while its identity \
holds.

It prints DANDEGATE-INCOMPLETE, DANDEGATE-MEASURE-CODE, DANDEGATE-WORSE, DANDEGATE-REUSED and DANDEGATE-STEP-FAILED \
lines, then a DANDEGATE line naming the page. It exits with 0 whenever it writes the page, complete or not, with 1 \
when it cannot write one, and with 2 for a malformed argument.";

/// Why no page can be written.
#[derive(Debug, thiserror::Error)]
enum DandegateError {
    #[error(transparent)]
    Plan(PlanError),
    #[error(transparent)]
    Resolve(ResolveError),
    #[error(transparent)]
    Out(OutError),
    #[error(transparent)]
    Imported(ImportError),
}

/// Why `--out` cannot hold the report.
#[derive(Debug, thiserror::Error)]
enum OutError {
    #[error("--out {0} exists; give a new folder")]
    Exists(Utf8PathBuf),
    #[error("--out {0} is inside the repository; give a folder outside it")]
    Inside(Utf8PathBuf),
    #[error("--out {0} cannot be made")]
    Make(Utf8PathBuf, #[source] io::Error),
    #[error("the current folder cannot be read or is not UTF-8")]
    Current,
}

/// Why `--measures` or `--previous` cannot be read.
#[derive(Debug, thiserror::Error)]
enum ImportError {
    #[error("{option} {file} cannot be read")]
    Read {
        option: &'static str,
        file: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{option} {file}")]
    Line {
        option: &'static str,
        file: Utf8PathBuf,
        #[source]
        source: LineError,
    },
    #[error("--previous {file} is not a saved measures record")]
    Saved {
        file: Utf8PathBuf,
        #[source]
        source: SavedError,
    },
}

fn main() -> ExitCode {
    let mut given = std::env::args_os().skip(1);
    if given.next().is_some_and(|first| first == supervise::FLAG) {
        let rest: Vec<OsString> = given.collect();
        let command = match rest.split_first() {
            Some((separator, command)) if separator == "--" => command,
            _ => &rest,
        };
        return ExitCode::from(supervise::supervise(command));
    }
    let arguments = Arguments::parse();
    let executable = executable_digest();
    match accept(&arguments, executable) {
        Ok(request) => {
            let finished = report::report(&request);
            let mut out = io::stdout().lock();
            for line in &finished.lines {
                // If standard output closed early, only these lines are lost. The page exists.
                let _ = writeln!(out, "{line}");
            }
            if finished.written {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            render(&error);
            ExitCode::FAILURE
        }
    }
}

/// The digest of the running executable, which every kept result's identity holds.
fn executable_digest() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|error| error.to_string())?;
    let path = Utf8PathBuf::try_from(path).map_err(|error| error.to_string())?;
    report::store::file_sha256(&path)
        .map_err(|error| format!("dandegate's own executable cannot be read: {error}"))
}

/// The command line accepted, the revisions resolved, the plan read and the output folder made: everything that can
/// stop a page, before any step runs.
fn accept(
    arguments: &Arguments,
    executable: Result<String, String>,
) -> Result<Request, DandegateError> {
    let current = std::env::current_dir()
        .ok()
        .and_then(|folder| Utf8PathBuf::try_from(folder).ok())
        .ok_or(DandegateError::Out(OutError::Current))?;
    let repository = Repository::discover(&current).map_err(DandegateError::Resolve)?;
    let revisions = PerSide {
        before: repository
            .resolve(&arguments.before)
            .map_err(DandegateError::Resolve)?,
        after: repository
            .resolve(&arguments.after)
            .map_err(DandegateError::Resolve)?,
    };
    let out = new_folder(&current.join(&arguments.out), repository.root())
        .map_err(DandegateError::Out)?;
    let measures_file = arguments
        .measures
        .as_ref()
        .map(|file| current.join(file))
        .map(|file| {
            file.canonicalize_utf8()
                .map_err(|source| read_error("--measures", &file, source))
        })
        .transpose()
        .map_err(DandegateError::Imported)?;
    let after = match &measures_file {
        None => Measures::default(),
        Some(file) => gate_log("--measures", file).map_err(DandegateError::Imported)?,
    };
    let before = match &arguments.previous {
        None => Measures::default(),
        Some(previous) => {
            previous_measures(&current.join(previous)).map_err(DandegateError::Imported)?
        }
    };
    let file = Utf8Path::new("dandegate.toml");
    let text = repository
        .file_at(revisions.after.commit(), file.as_str())
        .map_err(|cause| {
            DandegateError::Resolve(ResolveError::Unresolved {
                requested: arguments.after.clone(),
                cause,
            })
        })?
        .unwrap_or_default();
    let workspaces = rust::workspaces(&repository, revisions.after.commit()).map_err(|cause| {
        DandegateError::Resolve(ResolveError::Unresolved {
            requested: arguments.after.clone(),
            cause,
        })
    })?;
    let plan = Plan::read(&text, file, &arguments.with, &arguments.inputs, &workspaces)
        .map_err(DandegateError::Plan)?;
    fs::create_dir_all(&out)
        .map_err(|source| DandegateError::Out(OutError::Make(out.clone(), source)))?;
    Ok(Request {
        repository,
        revisions,
        plan,
        out,
        no_reuse: arguments.no_reuse,
        measures_file,
        imported: PerSide { before, after },
        executable,
    })
}

fn read_error(option: &'static str, file: &Utf8Path, source: io::Error) -> ImportError {
    ImportError::Read {
        option,
        file: file.to_owned(),
        source,
    }
}

/// The `MEASURE:` lines of a gate log.
fn gate_log(option: &'static str, file: &Utf8Path) -> Result<Measures, ImportError> {
    let text = fs::read_to_string(file).map_err(|source| read_error(option, file, source))?;
    Measures::from_lines(&text).map_err(|source| ImportError::Line {
        option,
        file: file.to_owned(),
        source,
    })
}

/// The before side's measures from an earlier report's folder, its saved record, or a gate log.
fn previous_measures(previous: &Utf8Path) -> Result<Measures, ImportError> {
    let saved = if previous.is_dir() {
        previous.join("measures.json")
    } else if previous.extension() == Some("json") {
        previous.to_owned()
    } else {
        return gate_log("--previous", previous);
    };
    let text =
        fs::read_to_string(&saved).map_err(|source| read_error("--previous", &saved, source))?;
    Measures::from_saved(&text).map_err(|source| ImportError::Saved {
        file: saved,
        source,
    })
}

/// `folder`, absolute, when it does not exist and is outside the repository.
fn new_folder(folder: &Utf8Path, repository: &Utf8Path) -> Result<Utf8PathBuf, OutError> {
    if folder.symlink_metadata().is_ok() {
        return Err(OutError::Exists(folder.to_owned()));
    }
    let mut existing = folder.to_owned();
    let mut rest = Vec::new();
    while !existing.exists() {
        let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
            break;
        };
        rest.push(name.to_owned());
        existing = parent.to_owned();
    }
    let mut resolved = existing
        .canonicalize_utf8()
        .unwrap_or_else(|_| existing.clone());
    for part in rest.iter().rev() {
        resolved.push(part);
    }
    let repository = repository
        .canonicalize_utf8()
        .unwrap_or_else(|_| repository.to_owned());
    if resolved.starts_with(&repository) {
        return Err(OutError::Inside(folder.to_owned()));
    }
    Ok(resolved)
}

/// The error and each of its causes, once, on standard error.
fn render(error: &DandegateError) {
    let mut errors = io::stderr().lock();
    let mut cause: Option<&dyn Error> = Some(error);
    let mut lead = "dandegate: ";
    while let Some(found) = cause {
        // Standard error is where a failure to write is reported, so a failure to write there has nowhere to go.
        let _ = writeln!(errors, "{lead}{found}");
        lead = "  caused by: ";
        cause = found.source();
    }
}
