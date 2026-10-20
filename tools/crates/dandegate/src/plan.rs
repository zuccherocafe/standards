//! Reads `dandegate.toml` from the after revision into a checked plan, once, and applies the command line's `--with`
//! and `--input`. Every refusal names the file and line, or the option, and the field. This is the file's format
//! reference.
//!
//! The file has three groups of tables:
//!
//! - `[project]` says what the project is: `host`, the programs its steps take from the caller's machine, and one
//!   table for each language it holds. `[project.rust]` names the Cargo `workspace`; `[project.godot]` names the Godot
//!   `project` folder and its `tests` folders. A language with no table is not reported on.
//! - `[reports]` tunes the built-in reports: `[reports.measures]` names the measuring code (`code`, `exclude`,
//!   `code-measures`), and `[reports.rust]` gives the Rust reports their arguments (`scans`, `mutation`).
//! - `[steps]` holds what the project adds: `[steps.inputs]` and each `[[steps.step]]`, with its
//!   `[steps.step.identity]`.
//!
//! The path fields are `[project.rust]`'s `workspace`, `[project.godot]`'s `project` and `tests`,
//! `[reports.measures]`' `code` and `exclude`, and each step output's `folder`, `expected`, `from` and `file`. In each,
//! a folder is written with a trailing `/` and a file without one. A repository path is relative and plain, and is
//! checked against the after revision's tree: a language table's folders must exist there, and `[reports.measures]`'
//! `code` and `exclude` may name a path the after revision lacks, since a change can remove measuring code, but a path
//! it holds must be written as what it is. Each is a literal name, never a git pattern. An output path is a plain path
//! under `{out}/`. Nothing else is a path field: command words, input defaults, `host` and the Rust reports' arguments
//! are taken as written.
//!
//! When the after revision holds a Cargo workspace but the file has no `[project.rust]`, the plan carries a
//! [`Notice`] naming the table to add, which the page shows and dandegate prints.
//!
//! A plan guarantees these facts to the code that reads it:
//!
//! - Every step name is unique, and none is reserved.
//! - Every placeholder is one that the step's kind has, and every input it names is declared.
//! - Every output is a plain path under `{out}`, of a kind that the step can write.
//! - Every need names a step of a kind that the needing step can wait for. The needs have no cycle, and no ordinary
//!   step needs an exclusive one, since exclusive steps run after every other step.
//! - The enabled steps are the ordinary steps, the steps that `--with` names, and every step they need.
//! - Every host program is a bare file name, named once.
//!
//! A per-revision step can need per-revision steps, on its own side. An after step can need per-revision steps, on the
//! after side, or after steps. A once step can need any step, on every side that step runs.
//!
//! When `[project.rust]` names a folder holding a Cargo workspace, the plan adds the Rust reports as built-in steps
//! under reserved names ([`Report`]). `[reports.rust]` without `[project.rust]` is refused, since it would tune
//! nothing; an empty one is the same as none.
//!
//! This module holds the types that the rest of dandegate reads. [`read`] builds them from the file's TOML shapes in
//! [`raw`]. [`template`] parses each command word's placeholders, [`repo`] holds the repository's paths and tree,
//! and [`error`] holds every refusal.

mod error;
mod raw;
mod read;
mod repo;
mod template;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use indexmap::IndexMap;

pub(crate) use crate::plan::error::PlanError;
pub(crate) use crate::plan::repo::{RepoDir, RepoPath, Tree};
pub(crate) use crate::plan::template::{
    Arg, Expansion, PairArg, PairValues, Template, TreeArg, TreeValues,
};

/// How long a step may run when it does not say.
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_mins(15);
/// How long the mutation report may run: a campaign's length grows with the change, well past an ordinary step's.
const MUTANTS_TIMEOUT: Duration = Duration::from_hours(2);
/// The environment variables every reusable step's identity holds, before its own.
const IDENTITY_ENV: [&str; 3] = ["CARGO*", "RUST*", "LD_LIBRARY_PATH"];

/// A step's name: lowercase letters, digits and hyphens.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct StepName(Box<str>);

/// An input's name, as `{input.<name>}` and `--input <name>=` write it: lowercase letters, digits and hyphens.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct InputName(Box<str>);

/// A name that is not lowercase letters, digits and hyphens.
#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not lowercase letters, digits and hyphens")]
pub(crate) struct BadName(String);

fn checked_name(text: &str) -> Result<Box<str>, BadName> {
    let allowed = |character: char| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    };
    if !text.is_empty() && text.chars().all(allowed) {
        Ok(text.into())
    } else {
        Err(BadName(text.to_owned()))
    }
}

impl StepName {
    /// # Errors
    /// [`BadName`] for an empty name or one with another character.
    pub(crate) fn new(text: &str) -> Result<Self, BadName> {
        checked_name(text).map(Self)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl InputName {
    /// # Errors
    /// [`BadName`] for an empty name or one with another character.
    pub(crate) fn new(text: &str) -> Result<Self, BadName> {
        checked_name(text).map(Self)
    }
}

impl fmt::Display for StepName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for InputName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A declared input: its value for this run, the default unless `--input` set it, and its help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Input {
    value: String,
    help: String,
}

impl Input {
    pub(crate) fn new(value: String, help: String) -> Self {
        Self { value, help }
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

/// A program the steps take from the caller's machine rather than the revision's shell: a bare file name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct HostProgram(Box<str>);

impl HostProgram {
    /// `None` for an empty name, `.`, `..`, or one holding `/` or NUL.
    pub(crate) fn new(text: &str) -> Option<Self> {
        let plain = !matches!(text, "" | "." | "..") && !text.contains(['/', '\0']);
        plain.then(|| Self(text.into()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostProgram {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// An environment variable's name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EnvName(Box<str>);

impl EnvName {
    fn new(text: &str) -> Option<Self> {
        let mut characters = text.chars();
        let first = characters.next()?;
        let valid = (first.is_ascii_alphabetic() || first == '_')
            && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
            && !text.starts_with("DANDEGATE_");
        valid.then(|| Self(text.into()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Environment variables a step's identity holds: a name, or a prefix ending in `*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EnvPattern {
    prefix: Box<str>,
    whole: bool,
}

impl EnvPattern {
    fn new(text: &str) -> Option<Self> {
        let (name, whole) = match text.strip_suffix('*') {
            Some(prefix) => (prefix, false),
            None => (text, true),
        };
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_');
        valid.then(|| Self {
            prefix: name.into(),
            whole,
        })
    }

    pub(crate) fn matches(&self, name: &str) -> bool {
        if self.whole {
            name == &*self.prefix
        } else {
            name.starts_with(&*self.prefix)
        }
    }

    pub(crate) fn written(&self) -> String {
        if self.whole {
            self.prefix.to_string()
        } else {
            format!("{}*", self.prefix)
        }
    }
}

/// A path inside a step's output folder: relative, with only ordinary parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutPath(Utf8PathBuf);

impl OutPath {
    fn new(text: &str) -> Option<Self> {
        let path = Utf8Path::new(text);
        let plain = path
            .components()
            .all(|part| matches!(part, Utf8Component::Normal(_)));
        (plain && path.components().next().is_some()).then(|| Self(path.to_owned()))
    }

    pub(crate) fn as_path(&self) -> &Utf8Path {
        &self.0
    }
}

/// What a step leaves in its output folder for the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Output {
    /// PNGs in `folder`, compared by name with the other side's. `expected` lists the names the step says must
    /// exist, one a line.
    Images { folder: OutPath, expected: OutPath },
    /// `MEASURE: <name> <value> <unit> [higher-is-worse|lower-is-worse]` lines.
    Measures { from: OutPath },
    /// A Markdown file linked from the page, with its status beside it in `<file>.status`.
    Section { file: OutPath },
}

/// The measuring code the page warns about when it differs between the two commits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MeasureRules {
    pub(crate) code: Vec<RepoPath>,
    pub(crate) exclude: Vec<RepoPath>,
    pub(crate) code_measures: Vec<String>,
}

/// The Godot project `[project.godot]` names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GodotProject {
    project: RepoDir,
    tests: Vec<RepoDir>,
}

/// What the reader noticed about the steps file that changes no step, which the page shows and dandegate prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Notice {
    /// The after revision holds these Cargo workspaces, `.` for the root, but no `[project.rust]` names one, so the
    /// Rust reports are off.
    RustOff(Vec<Utf8PathBuf>),
}

impl fmt::Display for Notice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RustOff(workspaces) => {
                let written: Vec<String> = workspaces
                    .iter()
                    .map(|folder| format!("{folder}/"))
                    .collect();
                // The one folder is shown as the TOML string to write, so a quote or backslash in its name stays intact.
                let (noun, example) = match written.as_slice() {
                    [one] => (
                        "a Cargo workspace",
                        toml::Value::String(one.clone()).to_string(),
                    ),
                    _ => ("Cargo workspaces", "\"<folder>/\"".to_owned()),
                };
                write!(
                    formatter,
                    "the Rust reports are off: the after revision has {noun} in {}, but dandegate.toml has no \
                     [project.rust] table; add one with workspace = {example} to turn them on",
                    written.join(", ")
                )
            }
        }
    }
}

/// The Rust reports' settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RustSettings {
    /// The workspace folder `[project.rust]` names.
    pub(crate) workspace: RepoDir,
    /// rust-scans' arguments besides `--workspace`.
    pub(crate) scans: Vec<String>,
    /// cargo-mutants' arguments, after `--`.
    pub(crate) mutation: Vec<String>,
}

/// A Rust report, run as a step under its reserved name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Report {
    /// Both revisions' rust-scans findings and counts, side by side. It fails nothing.
    Scans,
    /// Each crate's production and test lines at both revisions.
    Lines,
    /// The mutation report of the Rust changed since the before commit, in the after checkout.
    Mutants,
    /// One revision's public API listing of each library crate, which the API diff reads.
    ApiListings,
    /// The public API diff of each library crate between the revisions.
    Api,
}

impl Report {
    const ALL: [Self; 5] = [
        Self::Scans,
        Self::Lines,
        Self::Mutants,
        Self::ApiListings,
        Self::Api,
    ];

    /// The report's step.
    pub(crate) fn step(self) -> StepName {
        StepName(self.name().into())
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Scans => "scans",
            Self::Lines => "lines",
            Self::Mutants => "mutants",
            Self::ApiListings => "api-listings",
            Self::Api => "api",
        }
    }
}

/// What a step does when it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Work<A> {
    /// The project's command, run in the measured checkout's development shell.
    Command(Vec<Template<A>>),
    /// A built-in Rust report.
    Rust(Report),
}

/// A step's command and the values it fills in, typed by the placeholders its kind has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Recipe<A> {
    work: Work<A>,
    env: BTreeMap<EnvName, Template<A>>,
    collect: Option<Vec<Template<A>>>,
}

impl<A: Arg> Recipe<A> {
    pub(crate) fn work(&self) -> &Work<A> {
        &self.work
    }

    pub(crate) fn env(&self) -> &BTreeMap<EnvName, Template<A>> {
        &self.env
    }

    fn report(&self) -> Option<Report> {
        match &self.work {
            Work::Rust(report) => Some(*report),
            Work::Command(_) => None,
        }
    }

    /// The identity collector's command, if the step names one.
    pub(crate) fn collect(&self) -> Option<&[Template<A>]> {
        self.collect.as_deref()
    }

    fn templates(&self) -> impl Iterator<Item = &Template<A>> {
        let run = match &self.work {
            Work::Command(run) => &run[..],
            Work::Rust(_) => &[],
        };
        run.iter()
            .chain(self.env.values())
            .chain(self.collect.iter().flatten())
    }
}

/// How many times a step runs and what it is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Runs {
    /// Once for each revision, in its own checkout.
    PerRevision(Recipe<TreeArg>),
    /// Only for the after revision.
    After(Recipe<TreeArg>),
    /// Once, given both checkouts, in the after one.
    Once(Recipe<PairArg>),
}

/// The kind of a step, without its recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    PerRevision,
    After,
    Once,
}

impl Kind {
    /// The kind with its article, as a sentence names it.
    fn with_article(self) -> &'static str {
        match self {
            Self::PerRevision => "a per-revision",
            Self::After => "an after",
            Self::Once => "a once",
        }
    }

    /// Whether a step of this kind can wait for one of `need`'s kind.
    fn can_need(self, need: Self) -> bool {
        match self {
            Self::PerRevision => need == Self::PerRevision,
            Self::After => need != Self::Once,
            Self::Once => true,
        }
    }
}

impl Runs {
    pub(crate) fn kind(&self) -> Kind {
        match self {
            Self::PerRevision(_) => Kind::PerRevision,
            Self::After(_) => Kind::After,
            Self::Once(_) => Kind::Once,
        }
    }

    fn inputs(&self) -> BTreeSet<&InputName> {
        match self {
            Self::PerRevision(recipe) | Self::After(recipe) => {
                recipe.templates().flat_map(Template::inputs).collect()
            }
            Self::Once(recipe) => recipe.templates().flat_map(Template::inputs).collect(),
        }
    }
}

/// One step of the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "live, enabled, reuse and exclusive are independent switches; every combination is a valid step"
)]
pub(crate) struct Step {
    runs: Runs,
    needs: Vec<StepName>,
    live: bool,
    enabled: bool,
    reuse: bool,
    exclusive: bool,
    timeout: Duration,
    identity_env: Vec<EnvPattern>,
    outputs: Vec<Output>,
}

impl Step {
    pub(crate) fn runs(&self) -> &Runs {
        &self.runs
    }

    pub(crate) fn kind(&self) -> Kind {
        self.runs.kind()
    }

    pub(crate) fn needs(&self) -> &[StepName] {
        &self.needs
    }

    /// Whether the step needs its needs' real checkouts from this run, so they run fresh for the sides it uses.
    pub(crate) fn live(&self) -> bool {
        self.live
    }

    /// Whether the step runs at all: an ordinary step, one `--with` names, or one an enabled step needs.
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn reuse(&self) -> bool {
        self.reuse
    }

    pub(crate) fn exclusive(&self) -> bool {
        self.exclusive
    }

    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The environment variables, by name or prefix, the step's identity holds.
    pub(crate) fn identity_env(&self) -> &[EnvPattern] {
        &self.identity_env
    }

    pub(crate) fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    /// The inputs the step's templates use.
    pub(crate) fn inputs(&self) -> BTreeSet<&InputName> {
        self.runs.inputs()
    }

    /// The built-in Rust report the step runs, if it is one.
    pub(crate) fn report(&self) -> Option<Report> {
        match &self.runs {
            Runs::PerRevision(recipe) | Runs::After(recipe) => recipe.report(),
            Runs::Once(recipe) => recipe.report(),
        }
    }

    /// The environment variables the step sets.
    pub(crate) fn env_names(&self) -> BTreeSet<&str> {
        match &self.runs {
            Runs::PerRevision(recipe) | Runs::After(recipe) => {
                recipe.env.keys().map(EnvName::as_str).collect()
            }
            Runs::Once(recipe) => recipe.env.keys().map(EnvName::as_str).collect(),
        }
    }

    /// The step as written, placeholders by name, as a value its kept results' identity holds. The name, whether it
    /// is optional or reusable and its timeout are left out, since none changes what a successful run produces.
    pub(crate) fn written(&self) -> serde_json::Value {
        fn recipe<A: Arg>(recipe: &Recipe<A>) -> serde_json::Value {
            let work = match recipe.work() {
                Work::Command(run) => {
                    serde_json::json!({ "run": run.iter().map(Template::written).collect::<Vec<_>>() })
                }
                Work::Rust(report) => serde_json::json!({ "rust": report.name() }),
            };
            let env: BTreeMap<&str, String> = recipe
                .env()
                .iter()
                .map(|(name, value)| (name.as_str(), value.written()))
                .collect();
            let collect = recipe
                .collect()
                .map(|collect| collect.iter().map(Template::written).collect::<Vec<_>>());
            serde_json::json!({ "work": work, "env": env, "collect": collect })
        }
        let recipe = match &self.runs {
            Runs::PerRevision(written) | Runs::After(written) => recipe(written),
            Runs::Once(written) => recipe(written),
        };
        let outputs: Vec<serde_json::Value> = self
            .outputs
            .iter()
            .map(|output| match output {
                Output::Images { folder, expected } => {
                    serde_json::json!({ "images": folder.as_path().as_str(), "expected": expected.as_path().as_str() })
                }
                Output::Measures { from } => serde_json::json!({ "measures": from.as_path().as_str() }),
                Output::Section { file } => serde_json::json!({ "section": file.as_path().as_str() }),
            })
            .collect();
        serde_json::json!({
            "recipe": recipe,
            "needs": self.needs.iter().map(StepName::as_str).collect::<Vec<_>>(),
            "live": self.live,
            "exclusive": self.exclusive,
            "identity-env": self.identity_env.iter().map(EnvPattern::written).collect::<Vec<_>>(),
            "outputs": outputs,
        })
    }
}

/// The steps file read into checked values, with the command line applied.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    /// Unique, in the file's order.
    host: Vec<HostProgram>,
    #[expect(
        dead_code,
        reason = "no report reads the Godot project yet; the reader checks it"
    )]
    godot: Option<GodotProject>,
    inputs: IndexMap<InputName, Input>,
    steps: IndexMap<StepName, Step>,
    measures: MeasureRules,
    rust: Option<RustSettings>,
    notices: Vec<Notice>,
}

impl Plan {
    /// The host programs every step's command finds after its revision shell's own tools.
    pub(crate) fn host(&self) -> &[HostProgram] {
        &self.host
    }

    pub(crate) fn inputs(&self) -> &IndexMap<InputName, Input> {
        &self.inputs
    }

    pub(crate) fn steps(&self) -> &IndexMap<StepName, Step> {
        &self.steps
    }

    pub(crate) fn measures(&self) -> &MeasureRules {
        &self.measures
    }

    pub(crate) fn rust(&self) -> Option<&RustSettings> {
        self.rust.as_ref()
    }

    pub(crate) fn notices(&self) -> &[Notice] {
        &self.notices
    }
}
#[cfg(test)]
mod tests;
