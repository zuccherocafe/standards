//! What every built-in report needs of the run, and the one dispatch from a report to its work. [`crate::rust`] holds
//! the Rust reports and [`crate::gdscript`] the GDScript scans. Each writes a Markdown section with its status, so a
//! side that fails makes the page incomplete instead of stopping the report.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::process::ExitStatus;
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};

use crate::plan::{Plan, Report};
use crate::report::cpus::Cpus;
use crate::report::rendered;
use crate::report::step::{Launch, LaunchError, SectionStatus, launch, shown, status_path};
use crate::revision::{PerSide, Revision, Side};
use crate::{gdscript, rust};

/// What a report needs of the run.
#[derive(Debug)]
pub(crate) struct Context<'a, S> {
    /// The settings of the steps file the report reads.
    pub(crate) settings: &'a S,
    pub(crate) revisions: &'a PerSide<Revision>,
    pub(crate) checkouts: &'a PerSide<Result<Utf8PathBuf, String>>,
    /// This step's output folder.
    pub(crate) out: &'a Utf8Path,
    /// Each side's api-listings output folder, which the API diff reads.
    pub(crate) listings: PerSide<Utf8PathBuf>,
    /// A folder of the run's scratch for this step.
    pub(crate) scratch: &'a Utf8Path,
    /// This step's log, which every command's error output joins.
    pub(crate) log: &'a Utf8Path,
    pub(crate) env: &'a [(String, String)],
    pub(crate) deadline: Instant,
    /// The run's folder of host programs, as [`Launch`] takes it.
    pub(crate) host: Option<&'a Utf8Path>,
    /// The CPUs the step runs on, as [`Launch`] takes them.
    pub(crate) cpus: Option<&'a Cpus>,
}

/// Why a report stopped before writing its section.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Stop {
    #[error("stopped when its time ran out")]
    TimedOut,
    #[error("could not write its output")]
    Write(#[source] io::Error),
    #[error("the plan has no {0} table")]
    Unplanned(&'static str),
}

/// Why a command a report runs did not succeed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CommandError {
    #[error("the {side} checkout could not be made: {why}")]
    NoCheckout { side: &'static str, why: String },
    #[error("`{command}` failed: {status}")]
    Failed { command: String, status: ExitStatus },
    #[error("`{command}` could not run")]
    Launch {
        command: String,
        #[source]
        source: LaunchError,
    },
    #[error("stopped when its time ran out")]
    TimedOut,
    #[error("its log or output file could not be opened")]
    Files(#[source] io::Error),
}

impl CommandError {
    /// What the report does with the failure: a timeout or a file it cannot write stops the report, and anything else
    /// leaves the report's section partial, with why.
    pub(crate) fn partial(self) -> Result<String, Stop> {
        match self {
            Self::TimedOut => Err(Stop::TimedOut),
            Self::Files(error) => Err(Stop::Write(error)),
            other => Ok(rendered(&other)),
        }
    }
}

impl<'a, S> Context<'a, S> {
    /// The same run with other settings.
    pub(crate) fn with<T>(&self, settings: &'a T) -> Context<'a, T> {
        Context {
            settings,
            revisions: self.revisions,
            checkouts: self.checkouts,
            out: self.out,
            listings: self.listings.clone(),
            scratch: self.scratch,
            log: self.log,
            env: self.env,
            deadline: self.deadline,
            host: self.host,
            cpus: self.cpus,
        }
    }

    /// Run `command` in `side`'s development shell, in `inside`, a folder of the checkout, with `extra` variables,
    /// its standard output in `stdout`.
    pub(crate) fn shell(
        &self,
        side: Side,
        inside: &Utf8Path,
        command: &[&str],
        extra: &[(String, String)],
        stdout: &Utf8Path,
    ) -> Result<(), CommandError> {
        let checkout = self.checkouts[side]
            .as_ref()
            .map_err(|why| CommandError::NoCheckout {
                side: side.as_str(),
                why: why.clone(),
            })?;
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log)
            .map_err(CommandError::Files)?;
        let command: Vec<String> = command.iter().map(|word| (*word).to_owned()).collect();
        writeln!(log, "$ {}", shown(&command)).map_err(CommandError::Files)?;
        let mut env = self.env.to_vec();
        env.extend_from_slice(extra);
        let launched = launch(Launch {
            checkout,
            directory: &checkout.join(inside),
            command: &command,
            env: &env,
            stdout: File::create(stdout).map_err(CommandError::Files)?,
            stderr: log,
            deadline: self.deadline,
            host: self.host,
            cpus: self.cpus,
        });
        match launched {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(CommandError::Failed {
                command: shown(&command),
                status,
            }),
            Err(LaunchError::TimedOut) => Err(CommandError::TimedOut),
            Err(source) => Err(CommandError::Launch {
                command: shown(&command),
                source,
            }),
        }
    }

    /// Write the section `name` and its status: incomplete, with why, when there are `problems`.
    pub(crate) fn write(&self, name: &str, text: &str, problems: &[String]) -> Result<(), Stop> {
        let section = self.out.join(name);
        fs::write(&section, text).map_err(Stop::Write)?;
        let status = if problems.is_empty() {
            SectionStatus::Complete
        } else {
            SectionStatus::Incomplete(problems.join("; "))
        };
        fs::write(status_path(&section), status.written()).map_err(Stop::Write)
    }
}

/// Run a built-in report for one side, or for both when `side` is none, with the settings of the table that turns it
/// on.
///
/// # Errors
/// [`Stop`] when its time runs out, its files cannot be written, or the plan lacks its table. A command that fails
/// makes a partial section instead.
pub(crate) fn run(
    report: Report,
    side: Option<Side>,
    plan: &Plan,
    context: &Context<'_, ()>,
) -> Result<(), Stop> {
    let rust = || {
        plan.rust()
            .map(|settings| context.with(settings))
            .ok_or(Stop::Unplanned("[project.rust]"))
    };
    match report {
        Report::RustScans => rust::scans(&rust()?),
        Report::Lines => rust::line_counts(&rust()?),
        Report::Mutants => rust::mutants(&rust()?),
        Report::ApiListings => rust::api_listings(&rust()?, side.unwrap_or(Side::After)),
        Report::Api => rust::api_diff(&rust()?),
        Report::GdscriptScans => {
            let godot = plan.godot().ok_or(Stop::Unplanned("[project.godot]"))?;
            gdscript::scans(&context.with(godot))
        }
    }
}
