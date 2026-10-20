//! Runs the Rust reports as built-in steps under reserved names. dandegate adds them when `dandegate.toml` names the
//! Cargo workspace; [`crate::plan`] describes the file.
//!
//! - `scans` runs rust-scans in each checkout, and shows both sides' counts and the after side's findings. It is a
//!   review aid and fails nothing.
//! - `lines` counts each crate's production and test lines at both revisions. It finds the crates with `cargo
//!   metadata`.
//! - `mutants` runs the mutation report on the Rust that changed since the before commit. It runs in the after
//!   checkout, which has the history the report diffs, and does not run when no Rust differs.
//! - `api-listings` lists each library crate's public items at one revision. It uses the nightly and the
//!   `cargo-public-api` that the revision's development shell pins, and builds in a target folder of its own. dandegate
//!   keeps and reuses its result like any step's, under the identity that its toolchain collector prints.
//! - `api`, which `--with api` selects, diffs the two sides' listings. A crate that one side failed to list shows as
//!   incomplete, never as removed. So does every crate when one side's crates could not be found at all.
//!
//! Each report writes a Markdown section with its status, so a side that fails makes the page incomplete instead of
//! stopping the report. Every command runs in the development shell of the checkout it reads.

mod api;
mod lines;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::process::{Command, ExitStatus};
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};

use crate::plan::{Report, RustSettings};
use crate::report::rendered;
use crate::report::step::{Launch, LaunchError, SectionStatus, launch, shown, status_path};
use crate::revision::git::{GitError, Repository};
use crate::revision::{CommitId, PerSide, Revision, Side};
use crate::rust::api::{Crate, LISTINGS_FORMAT, Listed, Listings, Workspace};
use crate::rust::lines::{Lines, crate_lines};

/// The api-listings step's identity collector, run with `bash -c` in a checkout's development shell, given the
/// workspace folder: the compiler, rustdoc and Cargo builds, Cargo's merged configuration there, and the digest of
/// each program Cargo runs, `cargo-public-api` and any wrapper included. A program is the one Cargo chooses: its
/// variable (`RUSTC`, `RUSTDOC`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`), then its `build` setting, which Cargo's
/// own variables also set, then the default. Each value is taken in an assignment of its own, so any command that
/// fails, or a setting it cannot place, fails the collector, and the step runs fresh and keeps nothing.
pub(crate) const TOOLCHAIN_COLLECTOR: &str = r#"set -euo pipefail
shopt -s inherit_errexit
cd "$1"
# Cargo's setting build.<name>: a program's name or absolute path, or nothing when it is not set.
configured() {
  local value inner
  if ! value=$(cargo -Zunstable-options config get --format json-value "build.$1" 2>&1); then
    case $value in
      *"config value \`build.$1\` is not set"*) return 0 ;;
      *) echo "cargo cannot read build.$1: $value" >&2; return 1 ;;
    esac
  fi
  inner=${value#\"}
  inner=${inner%\"}
  case $inner in
    *[\"\\]* | "") echo "build.$1 is not a plain path: $value" >&2; return 1 ;;
    /*) ;;
    */*) echo "build.$1 is a path relative to a configuration file: $value" >&2; return 1 ;;
  esac
  if [ "$value" != "\"$inner\"" ]; then echo "build.$1 is not a string: $value" >&2; return 1; fi
  printf '%s\n' "$inner"
}
# The program Cargo runs for a variable and its setting, or the default, or nothing.
chosen() {
  local value
  if [ -n "${!1:-}" ]; then printf '%s\n' "${!1}"; return 0; fi
  value=$(configured "$2")
  printf '%s\n' "${value:-$3}"
}
# A program's file and the SHA-256 of its content.
digest() {
  local file sum
  file=$(command -v -- "$1")
  sum=$(sha256sum -- "$file")
  printf '%s\n' "${sum%% *}"
}
rustc=$(chosen RUSTC rustc rustc)
rustdoc=$(chosen RUSTDOC rustdoc rustdoc)
wrapper=$(chosen RUSTC_WRAPPER rustc-wrapper "")
workspace_wrapper=$(chosen RUSTC_WORKSPACE_WRAPPER rustc-workspace-wrapper "")
rustc_version=$("$rustc" -vV)
rustdoc_version=$("$rustdoc" -vV)
cargo_version=$(cargo -vV)
configuration=$(cargo -Zunstable-options config get --format json-value)
configuration_digest=$(printf '%s\n' "$configuration" | sha256sum)
printf 'rustc=%s\n' "${rustc_version//$'\n'/ }"
printf 'rustdoc=%s\n' "${rustdoc_version//$'\n'/ }"
printf 'cargo=%s\n' "${cargo_version//$'\n'/ }"
printf 'cargo-configuration=%s\n' "${configuration_digest%% *}"
for pair in "rustc=$rustc" "rustdoc=$rustdoc" cargo=cargo cargo-public-api=cargo-public-api \
  "rustc-wrapper=$wrapper" "rustc-workspace-wrapper=$workspace_wrapper"; do
  name=${pair%%=*}
  program=${pair#*=}
  if [ -n "$program" ]; then
    sum=$(digest "$program")
    printf '%s-digest=%s\n' "$name" "$sum"
  fi
done
"#;

/// Every folder of `commit`'s tree holding a `Cargo.toml` with a `[workspace]` table, `.` for the root. `files` are
/// the tree's files.
///
/// # Errors
/// [`GitError`] when a manifest cannot be read.
pub(crate) fn workspaces(
    repository: &Repository,
    commit: &CommitId,
    files: &[String],
) -> Result<Vec<Utf8PathBuf>, GitError> {
    let mut found = Vec::new();
    for path in files {
        if path != "Cargo.toml" && !path.ends_with("/Cargo.toml") {
            continue;
        }
        let Some(bytes) = repository.file_at(commit, path)? else {
            continue;
        };
        let manifest = String::from_utf8(bytes)
            .ok()
            .and_then(|text| toml::from_str::<toml::Table>(&text).ok());
        if manifest.is_some_and(|table| table.get("workspace").is_some_and(toml::Value::is_table)) {
            let folder = Utf8Path::new(path)
                .parent()
                .map_or_default(Utf8Path::as_str);
            found.push(Utf8PathBuf::from(if folder.is_empty() {
                "."
            } else {
                folder
            }));
        }
    }
    Ok(found)
}

/// Whether any Rust file of the workspace differs between the two commits.
///
/// # Errors
/// [`GitError`] when either commit cannot be read.
pub(crate) fn rust_changed(
    repository: &Repository,
    settings: &RustSettings,
    revisions: &PerSide<Revision>,
) -> Result<bool, GitError> {
    let changed = repository.changed(
        revisions.before.commit(),
        revisions.after.commit(),
        &[settings.workspace.as_path().as_str()],
    )?;
    Ok(changed
        .iter()
        .any(|path| Utf8Path::new(path).extension() == Some("rs")))
}

/// What a report needs of the run.
#[derive(Debug)]
pub(crate) struct Context<'a> {
    pub(crate) settings: &'a RustSettings,
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
}

/// Why a report stopped before writing its section.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Stop {
    #[error("stopped when its time ran out")]
    TimedOut,
    #[error("could not write its output")]
    Write(#[source] io::Error),
}

/// Why a command a report runs did not succeed.
#[derive(Debug, thiserror::Error)]
enum CommandError {
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
    fn partial(self) -> Result<String, Stop> {
        match self {
            Self::TimedOut => Err(Stop::TimedOut),
            Self::Files(error) => Err(Stop::Write(error)),
            other => Ok(rendered(&other)),
        }
    }
}

/// Why a side's workspace packages could not be listed.
#[derive(Debug, thiserror::Error)]
enum MetadataError {
    #[error(transparent)]
    Command(CommandError),
    #[error("cargo metadata's output could not be read")]
    Read(#[source] io::Error),
    #[error("cargo metadata printed what dandegate cannot read: {0}")]
    Unreadable(String),
}

impl MetadataError {
    /// As [`CommandError::partial`].
    fn partial(self) -> Result<String, Stop> {
        match self {
            Self::Command(error) => error.partial(),
            Self::Read(error) => Err(Stop::Write(error)),
            unreadable @ Self::Unreadable(_) => Ok(unreadable.to_string()),
        }
    }
}

/// Run a report for one side, or for both when `side` is none.
///
/// # Errors
/// [`Stop`] when its time runs out or its files cannot be written. A command that fails makes a partial section
/// instead.
pub(crate) fn run(report: Report, side: Option<Side>, context: &Context<'_>) -> Result<(), Stop> {
    match (report, side) {
        (Report::Scans, _) => scans(context),
        (Report::Lines, _) => line_counts(context),
        (Report::Mutants, _) => mutants(context),
        (Report::ApiListings, Some(side)) => api_listings(context, side),
        (Report::ApiListings, None) => api_listings(context, Side::After),
        (Report::Api, _) => api_diff(context),
    }
}

impl Context<'_> {
    /// Run `command` in `side`'s development shell, in `inside`, a folder of the checkout, with `extra` variables,
    /// its standard output in `stdout`.
    fn shell(
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

    fn workspace(&self) -> &Utf8Path {
        self.settings.workspace.as_path()
    }

    fn write(&self, name: &str, text: &str, problems: &[String]) -> Result<(), Stop> {
        let section = self.out.join(name);
        fs::write(&section, text).map_err(Stop::Write)?;
        let status = if problems.is_empty() {
            SectionStatus::Complete
        } else {
            SectionStatus::Incomplete(problems.join("; "))
        };
        fs::write(status_path(&section), status.written()).map_err(Stop::Write)
    }

    /// `side`'s workspace packages from `cargo metadata`: each package's name, folder relative to the checkout,
    /// and library target's name, if it has one.
    fn packages(&self, side: Side) -> Result<Vec<Package>, MetadataError> {
        let file = self.out.join(format!("{}-metadata.json", side.as_str()));
        let command = [
            "cargo",
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ];
        self.shell(side, self.workspace(), &command, &[], &file)
            .map_err(MetadataError::Command)?;
        let checkout = self.checkouts[side]
            .as_ref()
            .map_err(|why| MetadataError::Unreadable(why.clone()))?;
        let text = fs::read_to_string(&file).map_err(MetadataError::Read)?;
        packages(&text, checkout).map_err(MetadataError::Unreadable)
    }
}

/// A workspace package as `cargo metadata` describes it.
#[derive(Debug)]
struct Package {
    name: String,
    folder: Utf8PathBuf,
    library: Option<String>,
}

/// The library target kinds: a binary-only package promises nothing to other crates.
const LIBRARY_KINDS: [&str; 6] = ["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];

/// The packages `cargo metadata --no-deps` printed, folders relative to `checkout`.
fn packages(text: &str, checkout: &Utf8Path) -> Result<Vec<Package>, String> {
    let metadata: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("cargo metadata printed no JSON: {error}"))?;
    let listed = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or("cargo metadata printed no packages")?;
    let mut found = Vec::new();
    for package in listed {
        let name = package.get("name").and_then(serde_json::Value::as_str);
        let manifest = package
            .get("manifest_path")
            .and_then(serde_json::Value::as_str);
        let targets = package.get("targets").and_then(serde_json::Value::as_array);
        let (Some(name), Some(manifest), Some(targets)) = (name, manifest, targets) else {
            return Err("cargo metadata printed a package that is not one".to_owned());
        };
        let folder = Utf8Path::new(manifest)
            .parent()
            .and_then(|folder| folder.strip_prefix(checkout).ok())
            .ok_or_else(|| format!("{name}'s manifest {manifest} is not in the checkout"))?;
        let library = targets.iter().find_map(|target| {
            let kinds = target.get("kind")?.as_array()?;
            kinds
                .iter()
                .any(|kind| {
                    kind.as_str()
                        .is_some_and(|kind| LIBRARY_KINDS.contains(&kind))
                })
                .then(|| target.get("name")?.as_str().map(str::to_owned))?
        });
        found.push(Package {
            name: name.to_owned(),
            folder: folder.to_owned(),
            library,
        });
    }
    Ok(found)
}

/// The text of every Rust file git tracks in `checkout`, by its path there.
fn tracked_rust(checkout: &Utf8Path) -> Result<BTreeMap<Utf8PathBuf, String>, String> {
    let done = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(["ls-files", "-z", "--", "*.rs"])
        .output()
        .map_err(|error| error.to_string())?;
    if !done.status.success() {
        return Err(String::from_utf8_lossy(&done.stderr).trim().to_owned());
    }
    let mut files = BTreeMap::new();
    for path in String::from_utf8_lossy(&done.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
    {
        let bytes = fs::read(checkout.join(path)).map_err(|error| format!("{path}: {error}"))?;
        files.insert(
            Utf8PathBuf::from(path),
            String::from_utf8_lossy(&bytes).into_owned(),
        );
    }
    Ok(files)
}

fn scans(context: &Context<'_>) -> Result<(), Stop> {
    let mut command = vec!["rust-scans", "--workspace", context.workspace().as_str()];
    command.extend(context.settings.scans.iter().map(String::as_str));
    let mut printed = PerSide::<Option<String>>::default();
    let mut problems = Vec::new();
    for side in Side::BOTH {
        let file = context.out.join(format!("{}.txt", side.as_str()));
        match context.shell(side, Utf8Path::new("."), &command, &[], &file) {
            Ok(()) => printed[side] = Some(fs::read_to_string(&file).map_err(Stop::Write)?),
            Err(error) => problems.push(format!(
                "the {} side's scans did not finish: {}",
                side.as_str(),
                error.partial()?
            )),
        }
    }
    let counts = printed.as_ref().map(|_, text| {
        text.as_deref().map_or_default(|text| {
            text.lines()
                .filter_map(|line| line.split_once(": "))
                .filter(|(scan, count)| scan.starts_with('H') && count.parse::<u64>().is_ok())
                .map(|(scan, count)| (scan.to_owned(), count.to_owned()))
                .collect::<BTreeMap<_, _>>()
        })
    });
    let scans: BTreeSet<&String> = counts.before.keys().chain(counts.after.keys()).collect();
    let mut text = format!(
        "# Rust scans from {} to {}\n\nThe Rust standard's report scans are review prompts and fail nothing. The \
         table gives each revision's counts, and the after revision's findings follow it. `before.txt` and \
         `after.txt` hold both revisions' output.\n\n| Scan | Before | After |\n|---|---|---|\n",
        context.revisions.before, context.revisions.after
    );
    for scan in scans {
        let count = |side: &BTreeMap<String, String>| {
            side.get(scan).cloned().unwrap_or_else(|| "-".to_owned())
        };
        let _ = writeln!(
            text,
            "| {scan} | {} | {} |",
            count(&counts.before),
            count(&counts.after)
        );
    }
    match &printed.after {
        None => text.push_str("\nThe after revision's scans did not finish. Its log says why.\n"),
        Some(after) => {
            let findings: Vec<&str> = after
                .lines()
                .filter(|line| {
                    line.starts_with('H')
                        && line
                            .split_once(' ')
                            .is_some_and(|(scan, _)| !scan.ends_with(':'))
                })
                .collect();
            if findings.is_empty() {
                text.push_str("\nNo findings.\n");
            } else {
                let _ = write!(text, "\n```text\n{}\n```\n", findings.join("\n"));
            }
        }
    }
    context.write("scans.md", &text, &problems)
}

fn line_counts(context: &Context<'_>) -> Result<(), Stop> {
    let mut problems = Vec::new();
    let mut counted = PerSide::<Option<BTreeMap<String, Lines>>>::default();
    for side in Side::BOTH {
        let packages = match context.packages(side) {
            Ok(packages) => packages,
            Err(error) => {
                problems.push(format!(
                    "the {} side's crates could not be listed: {}",
                    side.as_str(),
                    error.partial()?
                ));
                continue;
            }
        };
        let Ok(checkout) = &context.checkouts[side] else {
            continue;
        };
        match tracked_rust(checkout) {
            Err(why) => problems.push(format!(
                "the {} side's files could not be read: {why}",
                side.as_str()
            )),
            Ok(files) => counted[side] = Some(crates_lines(&packages, &files)),
        }
    }
    let mut text = format!(
        "# Line counts from {} to {}\n\nThese counts include every line of each crate's Rust files, with blank \
         lines, comments and documentation. A crate's test lines are its `tests/` and `benches/` files, files named \
         `tests.rs` or `*_tests.rs` or in a `tests` folder, each module declared under `#[cfg(test)]` or \
         `#[cfg(kani)]` with everything below it, and each item under either attribute in another file.\n\n\
         | Crate | Production | Tests |\n|---|---|---|\n",
        context.revisions.before, context.revisions.after
    );
    let names: BTreeSet<&String> = counted
        .before
        .iter()
        .chain(counted.after.iter())
        .flat_map(BTreeMap::keys)
        .collect();
    let cell = |side: &Option<BTreeMap<String, Lines>>, name: &str, tests: bool| {
        side.as_ref()
            .and_then(|crates| crates.get(name))
            .map(|lines| if tests { lines.tests } else { lines.production })
    };
    for name in names {
        let production = change(
            cell(&counted.before, name, false),
            cell(&counted.after, name, false),
        );
        let tests = change(
            cell(&counted.before, name, true),
            cell(&counted.after, name, true),
        );
        let _ = writeln!(text, "| {name} | {production} | {tests} |");
    }
    let total = |side: &Option<BTreeMap<String, Lines>>, tests: bool| {
        side.as_ref().map(|crates| {
            crates
                .values()
                .map(|lines| if tests { lines.tests } else { lines.production })
                .sum()
        })
    };
    let _ = writeln!(
        text,
        "| total | {} | {} |",
        change(total(&counted.before, false), total(&counted.after, false)),
        change(total(&counted.before, true), total(&counted.after, true))
    );
    context.write("lines.md", &text, &problems)
}

/// Each package's lines, every file given to the package whose folder holds it most closely.
fn crates_lines(
    packages: &[Package],
    files: &BTreeMap<Utf8PathBuf, String>,
) -> BTreeMap<String, Lines> {
    let mut by_package: BTreeMap<&str, BTreeMap<Utf8PathBuf, String>> = BTreeMap::new();
    for (path, text) in files {
        let owner = packages
            .iter()
            .filter(|package| path.starts_with(&package.folder))
            .max_by_key(|package| package.folder.components().count());
        if let Some(owner) = owner
            && let Ok(inside) = path.strip_prefix(&owner.folder)
        {
            by_package
                .entry(&owner.name)
                .or_default()
                .insert(inside.to_owned(), text.clone());
        }
    }
    packages
        .iter()
        .map(|package| {
            let lines = by_package
                .get(package.name.as_str())
                .map_or_default(crate_lines);
            (package.name.clone(), lines)
        })
        .collect()
}

/// `before → after (+change)`, `-` for a side that lacks it.
fn change(before: Option<u64>, after: Option<u64>) -> String {
    let shown =
        |count: Option<u64>| count.map_or_else(|| "-".to_owned(), |count| count.to_string());
    match (before, after) {
        (Some(old), Some(new)) if old != new => {
            let difference = i128::from(new) - i128::from(old);
            format!("{old} → {new} ({difference:+})")
        }
        _ => format!("{} → {}", shown(before), shown(after)),
    }
}

fn mutants(context: &Context<'_>) -> Result<(), Stop> {
    let before = context.revisions.before.commit().to_string();
    let run = context.out.join("run");
    let mut command = vec![
        "rust-mutants",
        "--workspace",
        context.workspace().as_str(),
        "--base",
        &before,
        "--out",
        run.as_str(),
        "--",
    ];
    command.extend(context.settings.mutation.iter().map(String::as_str));
    let printed = context.out.join("mutants.txt");
    let problems = match context.shell(Side::After, Utf8Path::new("."), &command, &[], &printed) {
        Ok(()) => Vec::new(),
        Err(error) => vec![error.partial()?],
    };
    let text = fs::read_to_string(&printed).map_err(Stop::Write)?;
    let section = format!(
        "# Mutation report\n\nThese are the mutants of the Rust that changed since {}, tested in {}. Judge each \
         missed mutant and diagnose each timed-out one. `run/rust-mutants/` holds every list.\n\n```text\n{}```\n",
        context.revisions.before, context.revisions.after, text
    );
    context.write("mutants.md", &section, &problems)
}

fn api_listings(context: &Context<'_>, side: Side) -> Result<(), Stop> {
    let mut problems = Vec::new();
    let workspace = match context.packages(side) {
        Err(error) => {
            let why = error.partial()?;
            problems.push(format!("the crates could not be listed: {why}"));
            Workspace::Unknown { why }
        }
        Ok(packages) => {
            let mut crates = BTreeMap::new();
            let target = context.scratch.join("target");
            let extra = [("CARGO_TARGET_DIR".to_owned(), target.to_string())];
            for package in &packages {
                let Some(library) = &package.library else {
                    continue;
                };
                let listed = list_crate(context, side, &package.name, library, &target, &extra)?;
                if let Listed::Failed { why } = &listed {
                    problems.push(format!("{}: {why}", package.name));
                }
                crates.insert(package.name.clone(), listed);
            }
            Workspace::Listed { crates }
        }
    };
    let record = Listings {
        format: LISTINGS_FORMAT.to_owned(),
        workspace,
    };
    let text = serde_json::to_string_pretty(&record)
        .map_err(|error| Stop::Write(io::Error::other(error)))?;
    fs::write(context.out.join("listings.json"), text + "\n").map_err(Stop::Write)?;
    let mut section = format!(
        "# Public API listings at {}\n\nThese listings hold each library crate's public items, without blanket and \
         auto-trait impls (`-ss`). `<crate>.txt` holds a crate's listing and `<crate>.json` the rustdoc JSON it \
         comes from.\n\n",
        context.revisions[side]
    );
    match &record.workspace {
        Workspace::Unknown { why } => {
            let _ = writeln!(section, "The crates could not be listed: {why}");
        }
        Workspace::Listed { crates } => {
            section.push_str("| Crate | Items |\n|---|---|\n");
            for (name, listed) in crates {
                let shown = match listed {
                    Listed::Listed { items } => items.to_string(),
                    Listed::Failed { why } => format!("not listed: {why}"),
                };
                let _ = writeln!(section, "| {name} | {shown} |");
            }
        }
    }
    context.write("listings.md", &section, &problems)
}

/// One library crate's rustdoc JSON and listing, kept in the step's output folder.
fn list_crate(
    context: &Context<'_>,
    side: Side,
    package: &str,
    library: &str,
    target: &Utf8Path,
    extra: &[(String, String)],
) -> Result<Listed, Stop> {
    let ignored = context.scratch.join(format!("{package}.stdout"));
    let rustdoc = [
        "cargo",
        "rustdoc",
        "--locked",
        "-p",
        package,
        "--lib",
        "-Z",
        "unstable-options",
        "--output-format",
        "json",
    ];
    if let Err(error) = context.shell(side, context.workspace(), &rustdoc, extra, &ignored) {
        return Ok(Listed::Failed {
            why: error.partial()?,
        });
    }
    let document = context.out.join(format!("{package}.json"));
    let built = target
        .join("doc")
        .join(format!("{}.json", library.replace('-', "_")));
    if let Err(error) = fs::copy(&built, &document) {
        return Ok(Listed::Failed {
            why: format!("rustdoc wrote no {built}: {error}"),
        });
    }
    let printed = context.scratch.join(format!("{package}.listing"));
    let lister = [
        "cargo-public-api",
        "-ss",
        "--color",
        "never",
        "--rustdoc-json",
        document.as_str(),
    ];
    if let Err(error) = context.shell(side, Utf8Path::new("."), &lister, &[], &printed) {
        return Ok(Listed::Failed {
            why: error.partial()?,
        });
    }
    let text = fs::read_to_string(&printed).map_err(Stop::Write)?;
    let items = api::distinct(text.lines());
    let listing: String = items.iter().flat_map(|item| [*item, "\n"]).collect();
    fs::write(context.out.join(format!("{package}.txt")), listing).map_err(Stop::Write)?;
    Ok(Listed::Listed { items: items.len() })
}

fn api_diff(context: &Context<'_>) -> Result<(), Stop> {
    let mut problems = Vec::new();
    // Each side's crates, or why they are unknown: a side whose record cannot be read, or whose workspace could not be
    // listed, has no crates to call added or removed.
    let sides: PerSide<Result<BTreeMap<String, Listed>, String>> = PerSide::from_fn(|side| {
        let path = context.listings[side].join("listings.json");
        let read = fs::read_to_string(&path)
            .map_err(|error| format!("its listings cannot be read: {error}"))
            .and_then(|text| {
                serde_json::from_str::<Listings>(&text)
                    .map_err(|error| format!("its listings cannot be read: {error}"))
            })
            .and_then(|listings| {
                if listings.format == LISTINGS_FORMAT {
                    Ok(listings)
                } else {
                    Err(format!(
                        "its listings cannot be read: their format is {:?}",
                        listings.format
                    ))
                }
            });
        match read?.workspace {
            Workspace::Listed { crates } => Ok(crates),
            Workspace::Unknown { why } => Err(format!("its crates could not be listed: {why}")),
        }
    });
    let unknown: Vec<String> = Side::BOTH
        .into_iter()
        .filter_map(|side| {
            let why = sides[side].as_ref().err()?;
            Some(format!("the {} side: {why}", side.as_str()))
        })
        .collect();
    problems.extend(unknown.iter().cloned());
    let known = |side: Side| sides[side].as_ref().ok();
    let names: BTreeSet<&String> = Side::BOTH
        .into_iter()
        .filter_map(known)
        .flat_map(BTreeMap::keys)
        .collect();
    let mut crates: BTreeMap<String, Crate> = BTreeMap::new();
    for name in names {
        let mut found = Crate::default();
        for side in Side::BOTH {
            match sides[side].as_ref().map(|crates| crates.get(name)) {
                Ok(Some(Listed::Listed { items })) => found.items[side] = Some(*items),
                Ok(Some(Listed::Failed { .. })) => found.missing.push(format!(
                    "not listed on the {} side; see its api-listings step",
                    side.as_str()
                )),
                Ok(None) => {}
                Err(_) => found.missing.push(format!(
                    "not compared: the {} side's crates are unknown",
                    side.as_str()
                )),
            }
        }
        if found.missing.is_empty() {
            found.diff = Some(crate_diff(context, name, &sides, &mut found.missing)?);
            if !found.missing.is_empty() {
                found.diff = None;
            }
        }
        problems.extend(found.missing.iter().map(|why| format!("{name}: {why}")));
        crates.insert(name.clone(), found);
    }
    let title = format!(
        "Public API from {} to {}",
        context.revisions.before, context.revisions.after
    );
    context.write("api.md", &api::report(&title, &crates, &unknown), &problems)
}

/// One crate's diff: `cargo-public-api diff` when both sides list it, every item removed or added when only one does.
fn crate_diff(
    context: &Context<'_>,
    name: &str,
    sides: &PerSide<Result<BTreeMap<String, Listed>, String>>,
    missing: &mut Vec<String>,
) -> Result<api::Diff, Stop> {
    let listing = |side: Side| -> Result<Vec<String>, Stop> {
        let text = fs::read_to_string(context.listings[side].join(format!("{name}.txt")))
            .map_err(Stop::Write)?;
        Ok(text.lines().map(str::to_owned).collect())
    };
    let has = |side: Side| {
        sides[side]
            .as_ref()
            .is_ok_and(|crates| crates.contains_key(name))
    };
    match (has(Side::Before), has(Side::After)) {
        (true, false) => Ok(api::Diff {
            removed: listing(Side::Before)?,
            ..api::Diff::default()
        }),
        (false, true) => Ok(api::Diff {
            added: listing(Side::After)?,
            ..api::Diff::default()
        }),
        _ => {
            let documents =
                PerSide::from_fn(|side| context.listings[side].join(format!("{name}.json")));
            let printed = context.scratch.join(format!("{name}.diff"));
            let command = [
                "cargo-public-api",
                "-ss",
                "--color",
                "never",
                "diff",
                documents.before.as_str(),
                documents.after.as_str(),
            ];
            if let Err(error) =
                context.shell(Side::After, Utf8Path::new("."), &command, &[], &printed)
            {
                missing.push(format!(
                    "cargo-public-api could not diff it: {}",
                    error.partial()?
                ));
                Ok(api::Diff::default())
            } else {
                let text = fs::read_to_string(&printed).map_err(Stop::Write)?;
                api::parse_diff(&text).or_else(|why| {
                    missing.push(format!("its diff could not be read: {why}"));
                    Ok(api::Diff::default())
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_shows_both_counts_and_the_difference() {
        assert_eq!(change(Some(10), Some(12)), "10 → 12 (+2)");
        assert_eq!(change(Some(12), Some(10)), "12 → 10 (-2)");
        assert_eq!(change(Some(7), Some(7)), "7 → 7");
        assert_eq!(change(None, Some(7)), "- → 7");
    }

    #[test]
    fn packages_come_from_metadata_with_their_library_targets() {
        let metadata = r#"{"packages": [
            {"name": "core-lib", "manifest_path": "/c/crates/core/Cargo.toml",
             "targets": [{"name": "core_lib", "kind": ["lib"]}, {"name": "tool", "kind": ["bin"]}]},
            {"name": "tool", "manifest_path": "/c/Cargo.toml", "targets": [{"name": "tool", "kind": ["bin"]}]}
        ]}"#;
        let found = packages(metadata, Utf8Path::new("/c")).expect("packages");
        let summary: Vec<(&str, &str, Option<&str>)> = found
            .iter()
            .map(|package| {
                (
                    package.name.as_str(),
                    package.folder.as_str(),
                    package.library.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("core-lib", "crates/core", Some("core_lib")),
                ("tool", "", None)
            ]
        );
    }

    #[test]
    fn a_file_belongs_to_the_package_whose_folder_holds_it_most_closely() {
        let packages = [
            Package {
                name: "root".to_owned(),
                folder: Utf8PathBuf::new(),
                library: None,
            },
            Package {
                name: "inner".to_owned(),
                folder: Utf8PathBuf::from("crates/inner"),
                library: None,
            },
        ];
        let files: BTreeMap<Utf8PathBuf, String> = [
            ("src/main.rs", "fn main() {}\n"),
            ("crates/inner/src/lib.rs", "pub fn f() {}\npub fn g() {}\n"),
        ]
        .into_iter()
        .map(|(path, text)| (Utf8PathBuf::from(path), text.to_owned()))
        .collect();
        let counted = crates_lines(&packages, &files);
        assert_eq!(
            counted["root"],
            Lines {
                production: 1,
                tests: 0
            }
        );
        assert_eq!(
            counted["inner"],
            Lines {
                production: 2,
                tests: 0
            }
        );
    }
}
