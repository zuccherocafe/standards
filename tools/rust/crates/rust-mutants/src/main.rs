//! The Rust standard's mutation report: runs cargo-mutants over the Rust a change touched, or over a whole crate, and
//! lists each mutant the tests missed for the writer to judge, and each that timed out for the writer to diagnose. It
//! fails nothing: it is a review aid (rules/languages/rust/tests.md, Mutation report).
//!
//! Run from inside the repository; `--workspace` is the folder holding the workspace Cargo.toml:
//!
//! ```text
//! rust-mutants --workspace rust --base <revision> --out <folder outside the repository> [--shards 2] [-- <options>]
//! rust-mutants --workspace rust --package <crate> --out <folder> [--shards 2] [-- <cargo-mutants options>]
//! ```
//!
//! With `--base`, only the Rust changed since that revision is mutated, in every workspace member ([`repository`]
//! takes the diff). With `--package` instead, every mutant in that crate is tested.
//!
//! Each shard gets its own copy of the whole repository's tracked and unignored files under `--out`, and cargo-mutants
//! mutates that copy in place, with `CARGO_TARGET_DIR` set to the copy's own `target` (which also overrides a target
//! folder in Cargo's configuration): shards sharing a target folder test each other's builds. The shards run at once
//! and their lists are merged ([`outcomes`]); the copies are removed afterwards, and on any failure, an interrupt
//! included, every started shard is stopped first ([`shards`]). The tool owns only `--out`'s `rust-mutants` folder,
//! emptied at the start so what is printed is this run's. Needs `git`, `cargo mutants` and mold's `ld.mold` on PATH, a
//! nightly Cargo for the profile's flags and the test harness's options, `systemd-run` for the memory cap and
//! `unshare` for the PID namespaces.
//!
//! Each shard runs in a systemd user scope holding an equal part of a 16 GiB memory cap, without swap, so a mutant
//! that allocates without end is killed and counted while the run goes on. Where `systemd-run --user` cannot start a
//! scope, the shards run without a cap and the report says so. The shards share the jobserver's tasks, one per CPU
//! online, and each test binary stops at its first failing test, since one failure is enough to catch a mutant.
//!
//! Every mutant builds with the Cargo profile `mutants`, which the tool defines, so a project need not. The profile is
//! the development profile with light optimisation, incremental builds, many codegen units and no debug information,
//! and with dependencies fully optimised, since they build once per shard. The tool passes these settings with Cargo's
//! `--config` on cargo-mutants' build, check and test calls, the baseline's included. A project's own
//! `[profile.mutants]` keeps only the keys the tool does not set.
//!
//! mold links the profile, through the profile's own `rustflags`, which Cargo adds to the rustflags it selects from
//! the environment or its configuration. With cargo-mutants' lint cap on (`--cap-lints true`), cargo-mutants sets
//! `CARGO_ENCODED_RUSTFLAGS`, so Cargo drops configured target and build rustflags; the profile's own still apply.
//!
//! Doctests are skipped, since each is a binary of its own to build. A `--profile` among the options after `--` turns
//! the tool's profile off: the tool then defines no profile and selects none, and that profile builds as the project
//! defines it, linked by the project's own linker ([`command`]).
//!
//! Every test binary runs as the first process of a PID namespace of its own, through Cargo's test runner for the host
//! (`CARGO_TARGET_<HOST>_RUNNER`, wrapping any runner already set there), so a mutant in code that signals processes
//! can reach only the processes its test started, and every one of them ends with the binary. Where an unprivileged
//! PID namespace cannot be made, the test binaries run uncontained and the report says so ([`machine`]).
//!
//! Notices and refusals begin `rust_mutants:`, but not a refusal's `caused by:` lines; the report's rows, `missed, to
//! judge:` and `timed out, to diagnose:`, and its counts have no prefix. The exit status is 0 when
//! every shard finished, whatever the tests missed, 1 when the run could not finish, and 2 for a malformed argument.
//! Each shard starts in a session of its own through the nightly standard library's `process_setsid`, as this crate
//! forbids `unsafe` and the stable library offers no other way.

#![feature(process_setsid)]

mod arguments;
mod command;
mod machine;
mod outcomes;
mod repository;
mod shards;

use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::arguments::{Arguments, Scope};
use crate::command::{Prepared, Selection, Shard, Work, command};
use crate::machine::{Machine, Runner};
use crate::outcomes::{Outcome, Outcomes, ReadError};
use crate::repository::{CopyError, Repository, RepositoryError, changed_rust};
use crate::shards::{Copies, Interrupted, Running, WaitError};

/// cargo-mutants' exit codes for a finished run: all caught, some missed, some timed out.
const FINISHED: [i32; 3] = [0, 2, 3];

/// What begins each notice and each refusal line but its causes.
const LEAD: &str = "rust_mutants: ";

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            render(&error);
            ExitCode::FAILURE
        }
    }
}

/// The whole run: prepare, run the shards, print what they found.
fn run(arguments: &Arguments) -> Result<(), RunError> {
    let workspace = resolve(&arguments.workspace).map_err(RunError::CurrentFolder)?;
    let out = resolve(&arguments.out).map_err(RunError::CurrentFolder)?;
    let work = Work::under(&out);
    let _ = fs::remove_dir_all(work.folder());
    fs::create_dir_all(&out)
        .and_then(|()| fs::create_dir(work.folder()))
        .map_err(|source| RunError::Make {
            path: work.folder().to_owned(),
            source,
        })?;
    let repository = Repository::holding(&workspace)?;
    let selection = match &arguments.scope {
        Scope::Changed(base) => {
            let diff = repository::diff(&workspace, base)?;
            let changed = changed_rust(&diff).len();
            let base = base.as_os_str().display();
            if changed == 0 {
                say(format_args!("{LEAD}no Rust changed since {base}"))?;
                return Ok(());
            }
            fs::write(work.diff(), &diff).map_err(|source| RunError::Write {
                path: work.diff(),
                source,
            })?;
            say(format_args!(
                "{LEAD}{changed} Rust files changed since {base}"
            ))?;
            Selection::Diff(work.diff())
        }
        Scope::Package(package) => Selection::Package(package.clone()),
    };
    let files = repository.files()?;
    let prepared = Prepared {
        workspace_in_repository: repository.relative(&workspace)?,
        work,
        selection,
    };
    let machine = Machine::discover(&workspace);
    if !machine.capped {
        say(format_args!(
            "{LEAD}systemd-run --user cannot start a scope here, so the shards run without a memory cap"
        ))?;
    }
    if machine.runner == Runner::Uncontained {
        say(format_args!(
            "{LEAD}a PID namespace cannot be made here, so the test binaries run uncontained"
        ))?;
    }
    let codes = run_shards(arguments, &repository, &files, &prepared, &machine)
        .map_err(RunError::Stopped)?;
    let failed: Vec<Failed> = codes
        .iter()
        .filter(|(_, code)| !FINISHED.contains(code))
        .map(|&(shard, code)| Failed {
            shard,
            code,
            log: prepared.work.output(shard).join("mutants.out"),
        })
        .collect();
    if !failed.is_empty() {
        return Err(RunError::Shards(failed));
    }
    let outcomes =
        Outcomes::read(Shard::all(arguments.shards).map(|shard| prepared.work.output(shard)))?;
    for mutant in &outcomes[Outcome::Missed] {
        say(format_args!("missed, to judge: {mutant}"))?;
    }
    for mutant in &outcomes[Outcome::Timeout] {
        say(format_args!("timed out, to diagnose: {mutant}"))?;
    }
    let counts: Vec<String> = Outcome::ALL
        .iter()
        .map(|&outcome| format!("{}: {}", outcome.name(), outcomes[outcome].len()))
        .collect();
    say(format_args!("{}", counts.join(" ")))?;
    match prepared.selection {
        Selection::Diff(_) if outcomes.is_empty() => say(format_args!(
            "{LEAD}the changed Rust holds no function cargo-mutants can mutate"
        ))?,
        Selection::Diff(_) | Selection::Package(_) => {}
    }
    Ok(())
}

/// Copies the repository and starts each shard in turn, then waits for them all, and gives each shard's exit code.
/// On a failure or an interrupt, every started shard is stopped; the copies are removed in every case.
fn run_shards(
    arguments: &Arguments,
    repository: &Repository,
    files: &[PathBuf],
    prepared: &Prepared,
    machine: &Machine,
) -> Result<Vec<(Shard, i32)>, StopError> {
    let interrupted = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&interrupted))
        .map_err(StopError::Watch)?;
    let _copies = Copies {
        work: &prepared.work,
        count: arguments.shards,
    };
    let mut running = Running::default();
    for shard in Shard::all(arguments.shards) {
        let copy = prepared.work.copy(shard);
        for name in files {
            if interrupted.load(Ordering::SeqCst) {
                return Err(Interrupted.into());
            }
            repository.copy(name, &copy)?;
        }
        let command = command(&arguments.passed, prepared, shard, machine);
        let program = command.get_program().to_owned();
        if interrupted.load(Ordering::SeqCst) {
            return Err(Interrupted.into());
        }
        running
            .start(shard, command)
            .map_err(|source| StopError::Start {
                program: program.into(),
                source,
            })?;
    }
    Ok(running.wait(&interrupted)?)
}

/// `path` made absolute against the current folder, each link in it resolved as far as it exists; the rest is kept as
/// written, with `..` taking away the name before it.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    let mut resolved = env::current_dir()?;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => resolved.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                resolved.push(name);
                if let Ok(real) = fs::canonicalize(&resolved) {
                    resolved = real;
                }
            }
        }
    }
    Ok(resolved)
}

/// Prints one line of the report, at once, so it keeps its place among the shards' own output.
fn say(line: fmt::Arguments<'_>) -> Result<(), RunError> {
    writeln!(io::stdout(), "{line}").map_err(RunError::Print)
}

/// The error and each of its causes, once, on standard error; a line for each shard that failed.
fn render(error: &RunError) {
    let mut errors = io::stderr().lock();
    // Standard error is where a failure to write is reported, so a failure to write there has nowhere to go.
    if let RunError::Shards(failed) = error {
        for failed in failed {
            let _ = writeln!(errors, "{LEAD}{failed}");
        }
        return;
    }
    let mut cause: Option<&dyn Error> = Some(error);
    let mut lead = LEAD;
    while let Some(error) = cause {
        let _ = writeln!(errors, "{lead}{error}");
        lead = "  caused by: ";
        cause = error.source();
    }
}

/// A new, empty folder in the system's temporary folder for one test, removed by the test. A folder is taken by making
/// it, since a test binary in a PID namespace of its own can share its process number with another.
#[cfg(test)]
fn scratch(name: &str) -> PathBuf {
    let mut number = 0_u32;
    loop {
        let folder = env::temp_dir().join(format!(
            "rust-mutants-{}-{number}-{name}",
            std::process::id()
        ));
        match fs::create_dir(&folder) {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => number += 1,
            made => {
                made.expect("the temporary folder is writable");
                return folder;
            }
        }
    }
}

/// A shard that ended without finishing its run.
#[derive(Debug)]
struct Failed {
    shard: Shard,
    code: i32,
    log: PathBuf,
}

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "shard {} stopped with exit code {}; its log is under {}",
            self.shard.index(),
            self.code,
            self.log.display()
        )
    }
}

/// Why the run could not finish.
#[derive(Debug, thiserror::Error)]
enum RunError {
    #[error("could not read the current folder")]
    CurrentFolder(#[source] io::Error),
    #[error("could not make {}", .path.display())]
    Make {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("could not write {}", .path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("stopped before every shard finished")]
    Stopped(#[source] StopError),
    /// Rendered as one line for each shard.
    #[error("shards stopped without finishing")]
    Shards(Vec<Failed>),
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error("could not print the report")]
    Print(#[source] io::Error),
}

/// Why the shards were stopped before every one finished.
#[derive(Debug, thiserror::Error)]
enum StopError {
    #[error("could not watch for an interrupt")]
    Watch(#[source] io::Error),
    #[error(transparent)]
    Copy(#[from] CopyError),
    #[error("could not start {}", .program.display())]
    Start {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Interrupted(#[from] Interrupted),
    #[error(transparent)]
    Wait(#[from] WaitError),
}
