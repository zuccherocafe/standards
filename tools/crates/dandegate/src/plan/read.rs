//! Reads the steps file's TOML into checked steps, applies the command line's selections and inputs, and checks every
//! need. It refuses at the first problem, and names the problem's place.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use indexmap::IndexMap;
use toml::Spanned;

use crate::plan::error::{Place, PlanError, Problem};
use crate::plan::raw::{
    RawGodotProject, RawOutput, RawOutputKind, RawPlan, RawRuns, RawRustReports, RawStep,
};
use crate::plan::repo::{PathProblem, RepoDir, RepoPath, Tree};
use crate::plan::template::{Arg, Template};
use crate::plan::{
    DEFAULT_TIMEOUT, EnvName, EnvPattern, GodotProject, HostProgram, IDENTITY_ENV, Input,
    InputName, Kind, MUTANTS_TIMEOUT, MeasureRules, Notice, OutPath, Output, Plan, Recipe, Report,
    Runs, RustSettings, Step, StepName, Work,
};

/// The text's line holding byte `offset`, counted from 1.
fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .split(|&byte| byte == b'\n')
        .count()
}

/// Reads one steps file, refusing at the first problem.
struct Reader<'a> {
    text: &'a str,
    file: &'a Utf8Path,
}

impl Reader<'_> {
    fn at<T>(&self, spanned: &Spanned<T>, problem: Problem) -> PlanError {
        PlanError {
            place: Place::Line {
                file: self.file.to_owned(),
                line: line_of(self.text, spanned.span().start),
            },
            problem,
        }
    }

    /// A path field's value refused, with the field as `[table]: key`.
    fn path(&self, text: &Spanned<String>, field: &'static str, problem: PathProblem) -> PlanError {
        self.at(
            text,
            Problem::Path {
                field,
                text: text.get_ref().clone(),
                problem,
            },
        )
    }

    /// A folder field's value, which must name a folder the after revision holds.
    fn folder(
        &self,
        text: &Spanned<String>,
        field: &'static str,
        tree: &Tree,
    ) -> Result<RepoDir, PlanError> {
        let folder =
            RepoDir::parse(text.get_ref()).map_err(|problem| self.path(text, field, problem))?;
        if tree.holds_folder(folder.as_path()) {
            Ok(folder)
        } else {
            Err(self.path(text, field, PathProblem::NoFolder))
        }
    }

    /// A field listing folders and files, any of which the change may remove.
    fn paths(
        &self,
        texts: &[Spanned<String>],
        field: &'static str,
        tree: &Tree,
    ) -> Result<Vec<RepoPath>, PlanError> {
        texts
            .iter()
            .map(|text| {
                RepoPath::parse(text.get_ref())
                    .and_then(|path| tree.check(&path).map(|()| path))
                    .map_err(|problem| self.path(text, field, problem))
            })
            .collect()
    }

    fn godot(&self, raw: &RawGodotProject, tree: &Tree) -> Result<GodotProject, PlanError> {
        Ok(GodotProject {
            project: self.folder(&raw.project, "[project.godot]: project", tree)?,
            tests: raw
                .tests
                .iter()
                .map(|text| self.folder(text, "[project.godot]: tests", tree))
                .collect::<Result<_, _>>()?,
        })
    }

    fn template<A: Arg>(
        &self,
        step: &StepName,
        field: &str,
        text: &Spanned<String>,
        inputs: &IndexMap<InputName, Input>,
    ) -> Result<Template<A>, PlanError> {
        Template::parse(text.get_ref(), inputs).map_err(|error| {
            self.at(
                text,
                Problem::Placeholder {
                    step: step.clone(),
                    field: field.to_owned(),
                    error,
                },
            )
        })
    }

    fn recipe<A: Arg>(
        &self,
        name: &StepName,
        raw: &RawStep,
        inputs: &IndexMap<InputName, Input>,
    ) -> Result<Recipe<A>, PlanError> {
        if raw.run.get_ref().is_empty() {
            return Err(self.at(&raw.run, Problem::EmptyRun(name.clone())));
        }
        let run = raw
            .run
            .get_ref()
            .iter()
            .map(|word| self.template(name, "run", word, inputs))
            .collect::<Result<_, _>>()?;
        let mut env = BTreeMap::new();
        for (variable, value) in &raw.env {
            let env_name = EnvName::new(variable.get_ref()).ok_or_else(|| {
                self.at(
                    variable,
                    Problem::EnvName {
                        step: name.clone(),
                        name: variable.get_ref().clone(),
                    },
                )
            })?;
            let template = self.template(name, &format!("env.{variable}"), value, inputs)?;
            env.insert(env_name, template);
        }
        let collect = match raw
            .identity
            .as_ref()
            .and_then(|identity| identity.get_ref().collect.as_ref())
        {
            None => None,
            Some(words) if words.get_ref().is_empty() => {
                return Err(self.at(words, Problem::EmptyCollect(name.clone())));
            }
            Some(words) => Some(
                words
                    .get_ref()
                    .iter()
                    .map(|word| self.template(name, "identity.collect", word, inputs))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        };
        Ok(Recipe {
            work: Work::Command(run),
            env,
            collect,
        })
    }

    fn output(
        &self,
        step: &StepName,
        kind: Kind,
        raw: &Spanned<RawOutput>,
    ) -> Result<Output, PlanError> {
        let refuse = |why: String| {
            self.at(
                raw,
                Problem::Output {
                    step: step.clone(),
                    why,
                },
            )
        };
        let fields = raw.get_ref();
        let path = |field: &'static str,
                    folder: bool,
                    value: &Option<Spanned<String>>|
         -> Result<OutPath, PlanError> {
            let text = value.as_ref().ok_or_else(|| {
                refuse(format!(
                    "an output of kind {} needs {field}",
                    fields.kind.get_ref().as_str()
                ))
            })?;
            self.out_path(step, field, folder, text)
        };
        let unused = |names: &[(&str, &Option<Spanned<String>>)]| -> Result<(), PlanError> {
            match names.iter().find(|(_, value)| value.is_some()) {
                Some((field, _)) => Err(refuse(format!(
                    "an output of kind {} has no {field}",
                    fields.kind.get_ref().as_str()
                ))),
                None => Ok(()),
            }
        };
        let output = match fields.kind.get_ref() {
            RawOutputKind::Images => {
                unused(&[("from", &fields.from), ("file", &fields.file)])?;
                Output::Images {
                    folder: path("folder", true, &fields.folder)?,
                    expected: path("expected", false, &fields.expected)?,
                }
            }
            RawOutputKind::Measures => {
                unused(&[
                    ("folder", &fields.folder),
                    ("expected", &fields.expected),
                    ("file", &fields.file),
                ])?;
                Output::Measures {
                    from: path("from", false, &fields.from)?,
                }
            }
            RawOutputKind::Section => {
                unused(&[
                    ("folder", &fields.folder),
                    ("expected", &fields.expected),
                    ("from", &fields.from),
                ])?;
                Output::Section {
                    file: path("file", false, &fields.file)?,
                }
            }
        };
        let allowed = match &output {
            Output::Images { .. } => kind == Kind::PerRevision,
            Output::Measures { .. } => kind != Kind::Once,
            Output::Section { .. } => true,
        };
        if allowed {
            Ok(output)
        } else {
            Err(refuse(format!(
                "{} step has no output of kind {}: {}",
                kind.with_article(),
                fields.kind.get_ref().as_str(),
                match output {
                    Output::Images { .. } =>
                        "images are compared between the two revisions, which only a per-revision step measures",
                    Output::Measures { .. } | Output::Section { .. } =>
                        "its measures would belong to neither revision",
                }
            )))
        }
    }

    /// An output's path under `{out}/`: a folder written with a trailing `/`, a file without.
    fn out_path(
        &self,
        name: &StepName,
        field: &str,
        folder: bool,
        text: &Spanned<String>,
    ) -> Result<OutPath, PlanError> {
        let written = text.get_ref();
        let wrong = |why: String| {
            self.at(
                text,
                Problem::Output {
                    step: name.clone(),
                    why: format!("{field}: {written:?} {why}"),
                },
            )
        };
        let not_plain = || wrong("is not a plain path under {out}/".to_owned());
        let inside = written
            .strip_prefix("{out}/")
            .filter(|rest| !rest.contains(['{', '}']))
            .ok_or_else(not_plain)?;
        let inside = match (folder, inside.strip_suffix('/')) {
            (true, Some(within)) => within,
            (false, None) => inside,
            (true, None) => return Err(wrong(PathProblem::FolderWithoutSlash.to_string())),
            (false, Some(_)) => return Err(wrong(PathProblem::FileWithSlash.to_string())),
        };
        OutPath::new(inside).ok_or_else(not_plain)
    }

    fn step(
        &self,
        raw: &Spanned<RawStep>,
        inputs: &IndexMap<InputName, Input>,
    ) -> Result<(StepName, Step), PlanError> {
        let fields = raw.get_ref();
        let name = StepName::new(fields.name.get_ref())
            .map_err(|error| self.at(&fields.name, Problem::StepName(error)))?;
        if Report::ALL
            .iter()
            .any(|report| report.name() == name.as_str())
        {
            return Err(self.at(&fields.name, Problem::Reserved(name.to_string())));
        }
        let runs = match fields
            .runs
            .as_ref()
            .map_or(RawRuns::PerRevision, |runs| *runs.get_ref())
        {
            RawRuns::PerRevision => Runs::PerRevision(self.recipe(&name, fields, inputs)?),
            RawRuns::After => Runs::After(self.recipe(&name, fields, inputs)?),
            RawRuns::Once => Runs::Once(self.recipe(&name, fields, inputs)?),
        };
        let timeout = match &fields.timeout {
            None => DEFAULT_TIMEOUT,
            Some(text) => timeout(text.get_ref()).ok_or_else(|| {
                self.at(
                    text,
                    Problem::Timeout {
                        step: name.clone(),
                        text: text.get_ref().clone(),
                    },
                )
            })?,
        };
        let mut identity_env = default_identity_env();
        for pattern in fields
            .identity
            .iter()
            .flat_map(|identity| &identity.get_ref().env)
        {
            identity_env.push(EnvPattern::new(pattern.get_ref()).ok_or_else(|| {
                self.at(
                    pattern,
                    Problem::EnvPattern {
                        step: name.clone(),
                        pattern: pattern.get_ref().clone(),
                    },
                )
            })?);
        }
        let outputs = fields
            .output
            .iter()
            .map(|output| self.output(&name, runs.kind(), output))
            .collect::<Result<_, _>>()?;
        let cpus = match &fields.cpus {
            None => None,
            Some(count) if *count.get_ref() == 0 => {
                return Err(self.at(count, Problem::NoCpus(name)));
            }
            Some(count) if fields.exclusive => {
                return Err(self.at(count, Problem::ExclusiveShare(name)));
            }
            Some(count) => Some(*count.get_ref() as usize),
        };
        let mut needs = Vec::new();
        for need in &fields.needs {
            let named = StepName::new(need.get_ref())
                .map_err(|error| self.at(need, Problem::StepName(error)))?;
            needs.push(named);
        }
        let step = Step {
            runs,
            needs,
            live: fields.live,
            enabled: !fields.optional,
            reuse: fields.reuse,
            exclusive: fields.exclusive,
            cpus,
            timeout,
            identity_env,
            outputs,
        };
        Ok((name, step))
    }
}

fn default_identity_env() -> Vec<EnvPattern> {
    IDENTITY_ENV
        .iter()
        .filter_map(|pattern| EnvPattern::new(pattern))
        .collect()
}

/// A timeout as the steps file writes it: a whole, positive number of seconds, minutes or hours.
pub(super) fn timeout(text: &str) -> Option<Duration> {
    let unit = match text.chars().last()? {
        's' => 1,
        'm' => 60,
        'h' => 60 * 60,
        _ => return None,
    };
    let count: u64 = text[..text.len() - 1]
        .parse()
        .ok()
        .filter(|&count| count > 0)?;
    count.checked_mul(unit).map(Duration::from_secs)
}

impl Plan {
    /// The steps file `text`, read from `file`, with `with`'s optional steps enabled and `set`'s `name=value` inputs
    /// applied. `tree` is the after revision's, which every repository path is checked against.
    ///
    /// # Errors
    /// [`PlanError`] for the first problem found: the file's syntax, a field's value, a need, a selection or an
    /// input.
    pub(crate) fn read(
        text: &[u8],
        file: &Utf8Path,
        with: &[String],
        set: &[String],
        tree: &Tree,
    ) -> Result<Self, PlanError> {
        let text = std::str::from_utf8(text).map_err(|_| PlanError {
            place: Place::File(file.to_owned()),
            problem: Problem::NotUtf8,
        })?;
        let reader = Reader { text, file };
        let raw: RawPlan = toml::from_str(text).map_err(|error| PlanError {
            place: error.span().map_or_else(
                || Place::File(file.to_owned()),
                |span| Place::Line {
                    file: file.to_owned(),
                    line: line_of(text, span.start),
                },
            ),
            problem: Problem::Toml(error.message().to_owned()),
        })?;
        let RawPlan {
            project,
            reports,
            steps: raw_steps,
        } = raw;

        let mut host: Vec<HostProgram> = Vec::new();
        for written in &project.host {
            let program = HostProgram::new(written.get_ref()).ok_or_else(|| {
                reader.at(written, Problem::HostProgram(written.get_ref().clone()))
            })?;
            if host.contains(&program) {
                return Err(reader.at(written, Problem::DuplicateHost(program.to_string())));
            }
            host.push(program);
        }
        let godot = project
            .godot
            .as_ref()
            .map(|godot| reader.godot(godot.get_ref(), tree))
            .transpose()?;

        let mut inputs = IndexMap::new();
        for (name, input) in &raw_steps.inputs {
            let checked = InputName::new(name.get_ref())
                .map_err(|error| reader.at(name, Problem::InputName(error)))?;
            let input = input.get_ref();
            inputs.insert(
                checked,
                Input::new(input.default.clone(), input.help.clone()),
            );
        }
        apply_inputs(&mut inputs, set)?;

        let mut steps = IndexMap::new();
        for raw_step in &raw_steps.step {
            let (name, step) = reader.step(raw_step, &inputs)?;
            if steps.contains_key(&name) {
                return Err(reader.at(
                    &raw_step.get_ref().name,
                    Problem::DuplicateStep(name.to_string()),
                ));
            }
            steps.insert(name, step);
        }
        let rust = rust_settings(
            &reader,
            project.rust.as_ref().map(|rust| &rust.get_ref().workspace),
            reports.rust.as_ref(),
            tree,
        )?;
        if let Some(settings) = &rust {
            for report in Report::ALL {
                steps.insert(report.step(), built_in(report, settings));
            }
        }
        check_needs(&reader, &raw_steps.step, &steps)?;

        let measures = match &reports.measures {
            None => MeasureRules::default(),
            Some(measures) => {
                let measures = measures.get_ref();
                MeasureRules {
                    code: reader.paths(&measures.code, "[reports.measures]: code", tree)?,
                    exclude: reader.paths(
                        &measures.exclude,
                        "[reports.measures]: exclude",
                        tree,
                    )?,
                    code_measures: measures.code_measures.clone(),
                }
            }
        };
        select(&mut steps, with)?;
        let workspaces: Vec<Utf8PathBuf> = tree.workspaces().map(Utf8Path::to_owned).collect();
        let notices = if rust.is_none() && !workspaces.is_empty() {
            vec![Notice::RustOff(workspaces)]
        } else {
            Vec::new()
        };
        Ok(Self {
            host,
            godot,
            inputs,
            steps,
            measures,
            rust,
            notices,
        })
    }
}

/// `--input name=value` applied over the declared defaults.
fn apply_inputs(inputs: &mut IndexMap<InputName, Input>, set: &[String]) -> Result<(), PlanError> {
    let refuse = |problem| PlanError {
        place: Place::Option("--input"),
        problem,
    };
    let mut seen = BTreeSet::new();
    for assignment in set {
        let (name, value) = assignment
            .split_once('=')
            .ok_or_else(|| refuse(Problem::InputSyntax(assignment.clone())))?;
        let declared = InputName::new(name)
            .ok()
            .filter(|name| inputs.contains_key(name));
        let Some(declared) = declared else {
            let listed: Vec<String> = inputs
                .iter()
                .map(|(name, input)| format!("{name} ({})", input.help))
                .collect();
            let declared = if listed.is_empty() {
                "it declares none".to_owned()
            } else {
                format!("it declares {}", listed.join(", "))
            };
            return Err(refuse(Problem::UnknownInput {
                name: name.to_owned(),
                declared,
            }));
        };
        if !seen.insert(declared.clone()) {
            return Err(refuse(Problem::InputTwice(name.to_owned())));
        }
        if let Some(input) = inputs.get_mut(&declared) {
            value.clone_into(&mut input.value);
        }
    }
    Ok(())
}

/// The Rust reports' settings, when `[project.rust]` names the workspace. `[reports.rust]` tunes them, so it is
/// refused without `[project.rust]`.
fn rust_settings(
    reader: &Reader<'_>,
    workspace: Option<&Spanned<String>>,
    reports: Option<&Spanned<RawRustReports>>,
    tree: &Tree,
) -> Result<Option<RustSettings>, PlanError> {
    let Some(written) = workspace else {
        return match reports {
            Some(reports) => Err(reader.at(reports, Problem::RustReportsAlone)),
            None => Ok(None),
        };
    };
    let field = "[project.rust]: workspace";
    let workspace = reader.folder(written, field, tree)?;
    if !tree.holds_workspace(&workspace) {
        return Err(reader.at(written, Problem::NotWorkspace(workspace)));
    }
    let reports = reports.map(Spanned::get_ref);
    Ok(Some(RustSettings {
        workspace,
        scans: reports.map_or_default(|rust| rust.scans.clone()),
        mutation: reports.map_or_default(|rust| rust.mutation.clone()),
    }))
}

/// A built-in report's step.
fn built_in(report: Report, settings: &RustSettings) -> Step {
    let section = |file: &str| Output::Section {
        file: OutPath(Utf8PathBuf::from(file)),
    };
    let once = |reportable, collect| {
        Runs::Once(Recipe {
            work: Work::BuiltIn(reportable),
            env: BTreeMap::new(),
            collect,
        })
    };
    let (runs, needs, optional, reuse, timeout, outputs) = match report {
        Report::RustScans => (
            once(report, None),
            vec![],
            false,
            false,
            DEFAULT_TIMEOUT,
            vec![section("scans.md")],
        ),
        Report::Lines => (
            once(report, None),
            vec![],
            false,
            false,
            DEFAULT_TIMEOUT,
            vec![section("lines.md")],
        ),
        Report::Mutants => (
            once(
                report,
                Some(collector(settings, &["rust-mutants", "cargo-mutants"])),
            ),
            vec![],
            false,
            true,
            MUTANTS_TIMEOUT,
            vec![section("mutants.md")],
        ),
        Report::ApiListings => (
            Runs::PerRevision(Recipe {
                work: Work::BuiltIn(report),
                env: BTreeMap::new(),
                collect: Some(collector(settings, &["cargo-public-api"])),
            }),
            vec![],
            true,
            true,
            DEFAULT_TIMEOUT,
            vec![section("listings.md")],
        ),
        Report::Api => (
            once(report, None),
            vec![Report::ApiListings.step()],
            true,
            false,
            DEFAULT_TIMEOUT,
            vec![section("api.md")],
        ),
    };
    Step {
        runs,
        needs,
        live: false,
        enabled: !optional,
        reuse,
        exclusive: false,
        cpus: None,
        timeout,
        identity_env: default_identity_env(),
        outputs,
    }
}

/// A built-in report's identity collector, [`crate::rust::TOOLCHAIN_COLLECTOR`], given the workspace folder and the
/// programs the report runs besides Cargo's.
fn collector<A: Arg>(settings: &RustSettings, programs: &[&str]) -> Vec<Template<A>> {
    [
        "bash",
        "-c",
        crate::rust::TOOLCHAIN_COLLECTOR,
        "collect",
        settings.workspace.as_path().as_str(),
    ]
    .into_iter()
    .chain(programs.iter().copied())
    .map(Template::text)
    .collect()
}

/// Every need names a step of a kind its step can wait for, no ordinary step needs an exclusive one or one with a CPU
/// share, and there is no cycle.
fn check_needs(
    reader: &Reader<'_>,
    raw: &[Spanned<RawStep>],
    steps: &IndexMap<StepName, Step>,
) -> Result<(), PlanError> {
    let spans: BTreeMap<&str, &Spanned<RawStep>> = raw
        .iter()
        .map(|step| (step.get_ref().name.get_ref().as_str(), step))
        .collect();
    for (name, step) in steps {
        let Some(raw_step) = spans.get(name.as_str()) else {
            continue;
        };
        for (need, written) in step.needs.iter().zip(&raw_step.get_ref().needs) {
            let Some(needed) = steps.get(need) else {
                return Err(reader.at(
                    written,
                    Problem::UnknownNeed {
                        step: name.clone(),
                        need: need.to_string(),
                    },
                ));
            };
            if !step.kind().can_need(needed.kind()) {
                return Err(reader.at(
                    written,
                    Problem::NeedKind {
                        step: name.clone(),
                        kind: step.kind().with_article(),
                        need: need.clone(),
                        need_kind: needed.kind().with_article(),
                    },
                ));
            }
            if needed.exclusive && !step.exclusive {
                return Err(reader.at(
                    written,
                    Problem::NeedsExclusive {
                        step: name.clone(),
                        need: need.clone(),
                    },
                ));
            }
            // A step with a share runs alone, last, when the CPUs are too few to split.
            if needed.cpus.is_some() && !step.exclusive && step.cpus.is_none() {
                return Err(reader.at(
                    written,
                    Problem::NeedsShare {
                        step: name.clone(),
                        need: need.clone(),
                    },
                ));
            }
        }
    }
    if let Some(cycle) = cycle(steps) {
        let first = spans
            .get(cycle[0].as_str())
            .map(|step| &step.get_ref().name);
        return Err(match first {
            Some(written) => reader.at(written, Problem::Cycle(cycle)),
            None => PlanError {
                place: Place::File(reader.file.to_owned()),
                problem: Problem::Cycle(cycle),
            },
        });
    }
    Ok(())
}

/// A cycle of needs, its first step repeated at its end, if there is one.
fn cycle(steps: &IndexMap<StepName, Step>) -> Option<Vec<StepName>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Open,
        Done,
    }
    fn visit<'a>(
        name: &'a StepName,
        steps: &'a IndexMap<StepName, Step>,
        marks: &mut BTreeMap<&'a StepName, Mark>,
        path: &mut Vec<&'a StepName>,
    ) -> Option<Vec<StepName>> {
        match marks.get(name) {
            Some(Mark::Done) => return None,
            Some(Mark::Open) => {
                let start = path.iter().position(|step| *step == name).unwrap_or(0);
                let mut cycle: Vec<StepName> =
                    path[start..].iter().map(|&step| step.clone()).collect();
                cycle.push(name.clone());
                return Some(cycle);
            }
            None => {}
        }
        marks.insert(name, Mark::Open);
        path.push(name);
        for need in steps.get(name).map_or_default(Step::needs) {
            if let Some(found) = visit(need, steps, marks, path) {
                return Some(found);
            }
        }
        path.pop();
        marks.insert(name, Mark::Done);
        None
    }
    let mut marks = BTreeMap::new();
    steps
        .keys()
        .find_map(|name| visit(name, steps, &mut marks, &mut Vec::new()))
}

/// Enable each step `--with` names, then every step an enabled step needs.
fn select(steps: &mut IndexMap<StepName, Step>, with: &[String]) -> Result<(), PlanError> {
    for name in with {
        let found = StepName::new(name)
            .ok()
            .filter(|name| steps.contains_key(name));
        let Some(found) = found else {
            let known: Vec<&str> = steps.keys().map(StepName::as_str).collect();
            return Err(PlanError {
                place: Place::Option("--with"),
                problem: Problem::UnknownSelection {
                    name: name.clone(),
                    known: if known.is_empty() {
                        "none".to_owned()
                    } else {
                        known.join(", ")
                    },
                },
            });
        };
        if let Some(step) = steps.get_mut(&found) {
            step.enabled = true;
        }
    }
    let mut pending: Vec<StepName> = steps
        .iter()
        .filter(|(_, step)| step.enabled)
        .map(|(name, _)| name.clone())
        .collect();
    while let Some(name) = pending.pop() {
        let needs = steps.get(&name).map_or_default(|step| step.needs.clone());
        for need in needs {
            if let Some(step) = steps.get_mut(&need)
                && !step.enabled
            {
                step.enabled = true;
                pending.push(need);
            }
        }
    }
    Ok(())
}
