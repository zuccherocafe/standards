//! Runs one command in a measured checkout's development shell, and records what happened to a step.
//!
//! Every command runs as `nix develop --unset PATH <checkout> -c <command>` in that checkout, so each revision runs with
//! its own pinned tools. Every project has a Nix development shell, so this launcher is fixed, not configured.
//! dandegate finds `nix` on its own `PATH` first. The shell then starts without that `PATH`, so the command finds only
//! the tools the revision's shell provides, however the shell sets its `PATH`: a tool the shell lacks is not found, and
//! the step fails with the shell's error in its log, rather than run a tool the revision does not pin. The rest of the
//! environment passes through, except `DANDEGATE_REPOSITORY` and `DANDEGATE_MEASURES`, which hold this run's values or
//! are unset. The host programs `dandegate.toml` names are the one exception to the shell's `PATH`: the command finds
//! them after the shell's own tools ([`host`]).
//!
//! A command runs below a supervisor, a copy of dandegate that owns every process the command starts ([`supervise`]),
//! and writes its output to a file. When the command exits, the supervisor stops whatever it left running. When its
//! time runs out, dandegate tells the supervisor to stop everything, and waits for it. When dandegate itself ends, the
//! supervisor stops everything too. Nothing the command started keeps running after its step.
//!
//! A failure here becomes an outcome on the page. It never stops the page.

pub(crate) mod host;
pub(crate) mod supervise;

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};

use crate::compare::image;
use crate::compare::measure::Measures;
use crate::plan::{Output, StepName};

/// How often a running command is checked for its end: far below any step's length, and well above the cost of a
/// check.
const POLL: Duration = Duration::from_millis(20);

/// The variables dandegate sets for every command: what a command inherits under these names is removed first.
pub(crate) const RESERVED: [&str; 2] = ["DANDEGATE_REPOSITORY", "DANDEGATE_MEASURES"];

/// One command to run in a checkout's development shell.
#[derive(Debug)]
pub(crate) struct Launch<'a> {
    /// The checkout whose development shell runs the command.
    pub(crate) checkout: &'a Utf8Path,
    /// Where the command runs: the checkout or a folder in it.
    pub(crate) directory: &'a Utf8Path,
    pub(crate) command: &'a [String],
    /// Variables set for this command only, over the inherited environment.
    pub(crate) env: &'a [(String, String)],
    pub(crate) stdout: File,
    pub(crate) stderr: File,
    pub(crate) deadline: Instant,
    /// The run's folder of host programs, which the command finds after its shell's own tools; none when the plan
    /// names no host program.
    pub(crate) host: Option<&'a Utf8Path>,
}

/// Why a command did not finish.
#[derive(Debug, thiserror::Error)]
pub(crate) enum LaunchError {
    #[error("`nix` is not on dandegate's PATH")]
    NoNix,
    #[error("dandegate cannot find its own executable to supervise the command")]
    Supervisor(#[source] io::Error),
    #[error("the command could not be started")]
    Start(#[source] io::Error),
    #[error("stopped when its time ran out")]
    TimedOut,
}

/// Run a command to its end or its deadline.
///
/// # Errors
/// [`LaunchError::TimedOut`] when the deadline passes first, and the other variants when the command cannot start.
pub(crate) fn launch(launch: Launch<'_>) -> Result<ExitStatus, LaunchError> {
    let caller = env::var_os("PATH").unwrap_or_default();
    let nix = find_on(&caller, "nix").ok_or(LaunchError::NoNix)?;
    let supervisor = env::current_exe().map_err(LaunchError::Supervisor)?;
    let mut command = Command::new(&supervisor);
    command
        .arg(supervise::FLAG)
        .arg("--")
        .arg(nix)
        .args(["develop", "--unset", "PATH"])
        .arg(launch.checkout)
        .arg("-c");
    if let Some(folder) = launch.host {
        command
            .arg(&supervisor)
            .arg(host::FLAG)
            .arg(folder)
            .arg("--");
    }
    command.args(launch.command).current_dir(launch.directory);
    for name in RESERVED {
        command.env_remove(name);
    }
    command
        .envs(launch.env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::piped())
        .stdout(launch.stdout)
        .stderr(launch.stderr)
        .process_group(0);
    let mut child = command.spawn().map_err(LaunchError::Start)?;
    // The supervisor runs the command while this pipe stays open.
    let pipe = child.stdin.take();
    loop {
        if let Some(status) = child.try_wait().map_err(LaunchError::Start)? {
            return Ok(status);
        }
        let remaining = launch.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            drop(pipe);
            // The supervisor stops and reaps everything below it before it exits.
            let _ = child.wait();
            return Err(LaunchError::TimedOut);
        }
        thread::sleep(remaining.min(POLL));
    }
}

/// The first executable file called `name` in the folders of `path`.
fn find_on(path: &OsStr, name: &str) -> Option<PathBuf> {
    let folders: Vec<PathBuf> = env::split_paths(path)
        .filter(|folder| !folder.as_os_str().is_empty())
        .collect();
    find_in(&folders, name)
}

/// The first executable file called `name` in `folders`.
fn find_in(folders: &[PathBuf], name: &str) -> Option<PathBuf> {
    folders.iter().map(|folder| folder.join(name)).find(|file| {
        fs::metadata(file)
            .is_ok_and(|found| found.is_file() && found.permissions().mode() & 0o111 != 0)
    })
}

/// A command as a log shows it: words with spaces, quotes or nothing in them single-quoted, as a shell reads them.
pub(crate) fn shown(command: &[String]) -> String {
    command
        .iter()
        .map(|word| {
            let plain = !word.is_empty()
                && word.chars().all(|character| {
                    character.is_ascii_alphanumeric() || "-_./=:,+@%{}".contains(character)
                });
            if plain {
                word.clone()
            } else {
                format!("'{}'", word.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How one declared output of a finished step reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputCheck {
    Complete,
    Missing(Utf8PathBuf),
    Malformed {
        file: Utf8PathBuf,
        why: String,
    },
    /// A section whose status says it is partial.
    Incomplete {
        section: Utf8PathBuf,
        why: String,
    },
}

impl OutputCheck {
    /// Why the output is not complete, for the page and the printed lines.
    pub(crate) fn problem(&self) -> Option<String> {
        match self {
            Self::Complete => None,
            Self::Missing(path) => Some(format!("{path} is missing")),
            Self::Malformed { file, why } => Some(format!("{file} cannot be read: {why}")),
            Self::Incomplete { section, why } => Some(format!("{section} is incomplete: {why}")),
        }
    }
}

/// What a section's status file says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SectionStatus {
    Complete,
    Incomplete(String),
}

impl SectionStatus {
    /// The status file's text: `complete`, or `incomplete: <why>`, on one line.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let line = text.strip_suffix('\n').unwrap_or(text);
        if line.contains('\n') {
            return None;
        }
        match line {
            "complete" => Some(Self::Complete),
            _ => line
                .strip_prefix("incomplete: ")
                .filter(|why| !why.trim().is_empty())
                .map(|why| Self::Incomplete(why.to_owned())),
        }
    }

    /// The text a status file holds.
    pub(crate) fn written(&self) -> String {
        match self {
            Self::Complete => "complete\n".to_owned(),
            Self::Incomplete(why) => format!("incomplete: {why}\n"),
        }
    }
}

/// The path of a section's status file.
pub(crate) fn status_path(section: &Utf8Path) -> Utf8PathBuf {
    let mut name = section.file_name().unwrap_or_default().to_owned();
    name.push_str(".status");
    section.with_file_name(name)
}

/// The names an images output's expected list holds: one file name a line, blank lines aside.
pub(crate) fn expected_names(text: &str) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.contains('/') {
            return Err(format!("{line:?} is not a file name"));
        }
        names.insert(line.to_owned());
    }
    Ok(names)
}

/// Each declared output of a step whose output folder is `out`, checked, with paths inside `out`.
pub(crate) fn check_outputs(outputs: &[Output], out: &Utf8Path) -> Vec<OutputCheck> {
    outputs
        .iter()
        .map(|output| check_output(output, out))
        .collect()
}

fn check_output(output: &Output, out: &Utf8Path) -> OutputCheck {
    let read = |path: &Utf8Path| fs::read_to_string(out.join(path));
    match output {
        Output::Images { folder, expected } => {
            let (folder, list) = (folder.as_path(), expected.as_path());
            if !out.join(folder).is_dir() {
                return OutputCheck::Missing(folder.to_owned());
            }
            match read(list) {
                Err(_) => OutputCheck::Missing(list.to_owned()),
                Ok(text) => match expected_names(&text) {
                    Err(why) => OutputCheck::Malformed {
                        file: list.to_owned(),
                        why,
                    },
                    Ok(names) => names
                        .iter()
                        .map(|name| folder.join(name))
                        .find_map(|path| {
                            if !out.join(&path).is_file() {
                                return Some(OutputCheck::Missing(path));
                            }
                            image::readable(&out.join(&path))
                                .err()
                                .map(|why| OutputCheck::Malformed { file: path, why })
                        })
                        .unwrap_or(OutputCheck::Complete),
                },
            }
        }
        Output::Measures { from } => {
            let file = from.as_path();
            match read(file) {
                Err(_) => OutputCheck::Missing(file.to_owned()),
                Ok(text) => match Measures::from_lines(&text) {
                    Ok(_) => OutputCheck::Complete,
                    Err(error) => OutputCheck::Malformed {
                        file: file.to_owned(),
                        why: error.to_string(),
                    },
                },
            }
        }
        Output::Section { file } => {
            let section = file.as_path();
            let status = status_path(section);
            if !out.join(section).is_file() {
                return OutputCheck::Missing(section.to_owned());
            }
            match read(&status) {
                Err(_) => OutputCheck::Missing(status),
                Ok(text) => match SectionStatus::parse(&text) {
                    None => OutputCheck::Malformed {
                        file: status,
                        why: "a status is `complete` or `incomplete: <why>` on one line".to_owned(),
                    },
                    Some(SectionStatus::Complete) => OutputCheck::Complete,
                    Some(SectionStatus::Incomplete(why)) => OutputCheck::Incomplete {
                        section: section.to_owned(),
                        why,
                    },
                },
            }
        }
    }
}

/// Why a step did not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Skip {
    /// A step it needs did not succeed.
    NeedFailed(StepName),
    /// Only steps that were reused, or are not running, need it.
    Unneeded,
    /// No Rust differs between the two commits.
    NoRustChanged,
}

/// What happened to one step on one side.
#[derive(Clone, Debug)]
pub(crate) enum Outcome {
    /// Optional and not selected.
    Disabled,
    Skipped(Skip),
    LaunchFailed(String),
    TimedOut {
        after: Duration,
    },
    /// The project's command exited, with how its outputs read.
    Exited {
        status: ExitStatus,
        outputs: Vec<OutputCheck>,
    },
    /// A built-in report finished, with how its outputs read.
    Reported {
        outputs: Vec<OutputCheck>,
    },
    /// Copied from a kept result whose identity matched.
    Reused {
        entry: Utf8PathBuf,
        outputs: Vec<OutputCheck>,
    },
}

impl Outcome {
    /// Whether the step did its work, so the steps that need it may run. Its outputs may still be partial.
    pub(crate) fn succeeded(&self) -> bool {
        match self {
            Self::Exited { status, .. } => status.success(),
            Self::Reported { .. } | Self::Reused { .. } => true,
            Self::Disabled | Self::Skipped(_) | Self::LaunchFailed(_) | Self::TimedOut { .. } => {
                false
            }
        }
    }

    /// Whether the step succeeded and every output it declares is valid and complete: what may be kept.
    pub(crate) fn complete(&self) -> bool {
        self.succeeded()
            && self
                .outputs()
                .iter()
                .all(|output| *output == OutputCheck::Complete)
    }

    pub(crate) fn outputs(&self) -> &[OutputCheck] {
        match self {
            Self::Exited { outputs, .. }
            | Self::Reported { outputs }
            | Self::Reused { outputs, .. } => outputs,
            Self::Disabled | Self::Skipped(_) | Self::LaunchFailed(_) | Self::TimedOut { .. } => {
                &[]
            }
        }
    }

    /// Whether the step should have run and did not finish its work: it failed, timed out, could not start, or was
    /// skipped for a failed need.
    pub(crate) fn failed(&self) -> bool {
        match self {
            Self::Skipped(Skip::NeedFailed(_)) | Self::LaunchFailed(_) | Self::TimedOut { .. } => {
                true
            }
            Self::Exited { status, .. } => !status.success(),
            Self::Disabled
            | Self::Skipped(Skip::Unneeded | Skip::NoRustChanged)
            | Self::Reported { .. }
            | Self::Reused { .. } => false,
        }
    }

    /// Why the step leaves the page incomplete, if it does: it failed, or left an output missing, unreadable or
    /// partial.
    pub(crate) fn incomplete(&self) -> Vec<String> {
        let mut reasons = if self.failed() {
            vec![self.to_string()]
        } else {
            Vec::new()
        };
        reasons.extend(self.outputs().iter().filter_map(OutputCheck::problem));
        reasons
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => formatter.write_str("not selected"),
            Self::Skipped(Skip::NeedFailed(need)) => {
                write!(formatter, "skipped: {need} did not succeed")
            }
            Self::Skipped(Skip::Unneeded) => {
                formatter.write_str("not run: nothing running needs it")
            }
            Self::Skipped(Skip::NoRustChanged) => {
                formatter.write_str("not run: no Rust differs between the revisions")
            }
            Self::LaunchFailed(why) => write!(formatter, "could not start: {why}"),
            Self::TimedOut { after } => write!(
                formatter,
                "stopped after {} s, its time limit",
                after.as_secs()
            ),
            Self::Exited { status, .. } => match status.code() {
                Some(0) => formatter.write_str("exited 0"),
                Some(code) => write!(formatter, "failed: exited {code}"),
                None => write!(formatter, "failed: {status}"),
            },
            Self::Reported { .. } => formatter.write_str("reported"),
            Self::Reused { entry, .. } => write!(formatter, "reused from {entry}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_is_complete_or_says_why_it_is_not() {
        assert_eq!(
            SectionStatus::parse("complete\n"),
            Some(SectionStatus::Complete)
        );
        assert_eq!(
            SectionStatus::parse("complete"),
            Some(SectionStatus::Complete)
        );
        assert_eq!(
            SectionStatus::parse("incomplete: the before side failed\n"),
            Some(SectionStatus::Incomplete(
                "the before side failed".to_owned()
            ))
        );
        for text in [
            "",
            "done",
            "incomplete:",
            "incomplete: ",
            "complete\nmore",
            "Complete",
        ] {
            assert_eq!(SectionStatus::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn an_expected_list_names_files() {
        assert_eq!(
            expected_names("a.png\n\n  b.png \n").expect("names"),
            BTreeSet::from(["a.png".to_owned(), "b.png".to_owned()])
        );
        assert!(expected_names("cells/a.png").is_err());
    }

    #[test]
    fn a_command_is_shown_as_a_shell_reads_it() {
        let command = ["sh", "-c", "echo 'hi' > \"$1\"", "", "{out}/x.md"].map(str::to_owned);
        assert_eq!(
            shown(&command),
            "sh -c 'echo '\\''hi'\\'' > \"$1\"' '' {out}/x.md"
        );
    }

    #[test]
    fn the_launcher_is_the_first_executable_file_of_its_name_on_the_path() {
        // A folder this test makes for itself, never one a test in another PID namespace made under the same number.
        let temporary = Utf8PathBuf::try_from(env::temp_dir()).expect("a UTF-8 temporary folder");
        let root =
            super::super::acquire(&temporary, "dandegate-find-on-").expect("a folder of its own");
        let (folder, plain, executable) = (
            root.join("folder").into_std_path_buf(),
            root.join("plain").into_std_path_buf(),
            root.join("runs").into_std_path_buf(),
        );
        fs::create_dir_all(folder.join("nix")).expect("a folder named nix");
        fs::create_dir_all(&plain).expect("a folder");
        fs::write(plain.join("nix"), "").expect("a file that cannot run");
        fs::create_dir_all(&executable).expect("a folder");
        fs::write(executable.join("nix"), "").expect("a file");
        fs::set_permissions(executable.join("nix"), fs::Permissions::from_mode(0o700))
            .expect("executable by its owner");
        let path = env::join_paths([&folder, &plain, &executable]).expect("a PATH");
        assert_eq!(find_on(&path, "nix"), Some(executable.join("nix")));
        let without = env::join_paths([&folder, &plain]).expect("a PATH");
        assert_eq!(find_on(&without, "nix"), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_status_file_sits_beside_its_section() {
        assert_eq!(
            status_path(Utf8Path::new("/o/report.md")),
            Utf8PathBuf::from("/o/report.md.status")
        );
    }
}
