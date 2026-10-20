//! Why dandegate refuses a plan, and where the refused value is written.

use std::fmt;

use camino::Utf8PathBuf;

use crate::plan::repo::{PathProblem, RepoDir};
use crate::plan::template::TemplateError;
use crate::plan::{BadName, StepName};

/// Where a refused value was written.
#[derive(Debug)]
pub(crate) enum Place {
    /// A line of the steps file.
    Line { file: Utf8PathBuf, line: usize },
    /// The steps file as a whole.
    File(Utf8PathBuf),
    /// A command-line option.
    Option(&'static str),
}

impl fmt::Display for Place {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Line { file, line } => write!(formatter, "{file}:{line}"),
            Self::File(file) => write!(formatter, "{file}"),
            Self::Option(option) => formatter.write_str(option),
        }
    }
}

/// A steps file or command-line selection that cannot be run, where it was written and why.
#[derive(Debug, thiserror::Error)]
#[error("{place}: {problem}")]
pub(crate) struct PlanError {
    pub(super) place: Place,
    pub(super) problem: Problem,
}

/// Why a plan cannot be run.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Problem {
    #[error("the file is not UTF-8")]
    NotUtf8,
    #[error("{0}")]
    Toml(String),
    #[error("host: {0:?} is not a program's file name")]
    HostProgram(String),
    #[error("host: {0:?} is named twice")]
    DuplicateHost(String),
    #[error("input name: {0}")]
    InputName(BadName),
    #[error("step name: {0}")]
    StepName(BadName),
    #[error("step {0:?} is declared twice")]
    DuplicateStep(String),
    #[error("step {0:?} is a name dandegate's built-in reports use")]
    Reserved(String),
    #[error("step \"{0}\": run: names no command")]
    EmptyRun(StepName),
    #[error("step \"{0}\": identity.collect: names no command")]
    EmptyCollect(StepName),
    #[error("step \"{step}\": {field}: {error}")]
    Placeholder {
        step: StepName,
        field: String,
        error: TemplateError,
    },
    #[error("step \"{step}\": env: {name:?} is not an environment variable's name a step may set")]
    EnvName { step: StepName, name: String },
    #[error(
        "step \"{step}\": identity.env: {pattern:?} is not a variable's name or a prefix ending in `*`"
    )]
    EnvPattern { step: StepName, pattern: String },
    #[error(
        "step \"{step}\": timeout: {text:?} is not a whole number of seconds, minutes or hours, such as \"90s\" or \"30m\""
    )]
    Timeout { step: StepName, text: String },
    #[error("step \"{step}\": output: {why}")]
    Output { step: StepName, why: String },
    #[error("step \"{step}\": needs {need:?}, which no step is called")]
    UnknownNeed { step: StepName, need: String },
    #[error("step \"{step}\": {kind} step cannot need \"{need}\", {need_kind} step")]
    NeedKind {
        step: StepName,
        kind: &'static str,
        need: StepName,
        need_kind: &'static str,
    },
    #[error(
        "step \"{step}\": needs \"{need}\", which is exclusive and so runs after every step that is not"
    )]
    NeedsExclusive { step: StepName, need: StepName },
    #[error(
        "step \"{step}\": needs \"{need}\", which has a CPU share and so can run after every step without one"
    )]
    NeedsShare { step: StepName, need: StepName },
    #[error("step \"{0}\": cpus: a share is at least one CPU")]
    NoCpus(StepName),
    #[error(
        "step \"{0}\": cpus: an exclusive step runs alone on every CPU, so it has no share of them"
    )]
    ExclusiveShare(StepName),
    #[error("the steps need each other in a cycle: {}", .0.iter().map(StepName::as_str).collect::<Vec<_>>().join(" -> "))]
    Cycle(Vec<StepName>),
    #[error("{name:?} names no step; the steps are {known}")]
    UnknownSelection { name: String, known: String },
    #[error("{0:?} is not <name>=<value>")]
    InputSyntax(String),
    #[error("{name:?} is not an input dandegate.toml declares; {declared}")]
    UnknownInput { name: String, declared: String },
    #[error("{0:?} is set twice")]
    InputTwice(String),
    #[error("{field}: {text:?} {problem}")]
    Path {
        field: &'static str,
        text: String,
        problem: PathProblem,
    },
    #[error(
        "[project.rust]: workspace: \"{0}\" holds no Cargo.toml with a [workspace] table in the after revision"
    )]
    NotWorkspace(RepoDir),
    #[error("[project.godot]: project: \"{0}\" holds no project.godot in the after revision")]
    NotGodotProject(RepoDir),
    #[error("[project.godot]: tests: \"{tests}\" is not inside the project \"{project}\"")]
    TestsOutside { tests: RepoDir, project: RepoDir },
    #[error(
        "[reports.rust]: tunes the Rust reports, which run only when [project.rust] names the workspace; add [project.rust] or remove [reports.rust]"
    )]
    RustReportsAlone,
}
