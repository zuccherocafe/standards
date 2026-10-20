//! Reads the steps file's TOML into checked steps, applies the command line's selections and inputs, and checks every
//! need. It refuses at the first problem, and names the problem's place.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use indexmap::IndexMap;
use toml::Spanned;

use crate::plan::error::{Place, PlanError, Problem};
use crate::plan::raw::{RawOutput, RawOutputKind, RawPlan, RawRuns, RawRust, RawStep};
use crate::plan::template::{Arg, Template};
use crate::plan::{
    DEFAULT_TIMEOUT, EnvName, EnvPattern, HostProgram, IDENTITY_ENV, Input, InputName, Kind,
    MUTANTS_TIMEOUT, MeasureRules, OutPath, Output, Plan, Recipe, Report, Runs, RustSettings, Step,
    StepName, Work,
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
        let path =
            |field: &'static str, value: &Option<Spanned<String>>| -> Result<OutPath, PlanError> {
                let text = value.as_ref().ok_or_else(|| {
                    refuse(format!(
                        "an output of kind {} needs {field}",
                        fields.kind.get_ref().as_str()
                    ))
                })?;
                let inside = text
                    .get_ref()
                    .strip_prefix("{out}/")
                    .filter(|rest| !rest.contains(['{', '}']))
                    .and_then(OutPath::new);
                inside.ok_or_else(|| {
                    self.at(
                        text,
                        Problem::Output {
                            step: step.clone(),
                            why: format!(
                                "{field}: {:?} is not a plain path under {{out}}/",
                                text.get_ref()
                            ),
                        },
                    )
                })
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
                    folder: path("folder", &fields.folder)?,
                    expected: path("expected", &fields.expected)?,
                }
            }
            RawOutputKind::Measures => {
                unused(&[
                    ("folder", &fields.folder),
                    ("expected", &fields.expected),
                    ("file", &fields.file),
                ])?;
                Output::Measures {
                    from: path("from", &fields.from)?,
                }
            }
            RawOutputKind::Section => {
                unused(&[
                    ("folder", &fields.folder),
                    ("expected", &fields.expected),
                    ("from", &fields.from),
                ])?;
                Output::Section {
                    file: path("file", &fields.file)?,
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
    /// applied. `workspaces` are the folders of the after revision holding a Cargo.toml with a `[workspace]` table.
    ///
    /// # Errors
    /// [`PlanError`] for the first problem found: the file's syntax, a field's value, a need, a selection or an
    /// input.
    pub(crate) fn read(
        text: &[u8],
        file: &Utf8Path,
        with: &[String],
        set: &[String],
        workspaces: &[Utf8PathBuf],
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

        let mut host: Vec<HostProgram> = Vec::new();
        for written in &raw.host {
            let program = HostProgram::new(written.get_ref()).ok_or_else(|| {
                reader.at(written, Problem::HostProgram(written.get_ref().clone()))
            })?;
            if host.contains(&program) {
                return Err(reader.at(written, Problem::DuplicateHost(program.to_string())));
            }
            host.push(program);
        }

        let mut inputs = IndexMap::new();
        for (name, input) in &raw.inputs {
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
        for raw_step in &raw.step {
            let (name, step) = reader.step(raw_step, &inputs)?;
            if steps.contains_key(&name) {
                return Err(reader.at(
                    &raw_step.get_ref().name,
                    Problem::DuplicateStep(name.to_string()),
                ));
            }
            steps.insert(name, step);
        }
        let rust = match rust_settings(&reader, raw.rust.as_ref(), workspaces)? {
            None => None,
            Some(settings) => {
                for report in Report::ALL {
                    steps.insert(report.step(), built_in(report, &settings));
                }
                Some(settings)
            }
        };
        check_needs(&reader, &raw.step, &steps)?;
        select(&mut steps, with)?;

        let measures = raw.measures.map_or_default(Spanned::into_inner);
        Ok(Self {
            host,
            inputs,
            steps,
            measures: MeasureRules {
                code: measures.code,
                exclude: measures.exclude,
                code_measures: measures.code_measures,
            },
            rust,
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

/// The Rust reports' settings, when the after revision has a workspace or `[rust]` names one.
fn rust_settings(
    reader: &Reader<'_>,
    raw: Option<&Spanned<RawRust>>,
    workspaces: &[Utf8PathBuf],
) -> Result<Option<RustSettings>, PlanError> {
    let named = raw.and_then(|rust| rust.get_ref().workspace.as_ref());
    let workspace = match (named, workspaces) {
        (Some(folder), _) => {
            let path = Utf8Path::new(folder.get_ref());
            let inside = path
                .components()
                .all(|part| matches!(part, Utf8Component::Normal(_) | Utf8Component::CurDir));
            if !inside || folder.get_ref().is_empty() {
                return Err(reader.at(folder, Problem::WorkspacePath(folder.get_ref().clone())));
            }
            path.to_owned()
        }
        (None, [one]) => one.clone(),
        (None, []) => {
            return match raw {
                Some(rust) => Err(reader.at(rust, Problem::NoWorkspace)),
                None => Ok(None),
            };
        }
        (None, several) => {
            let listed: Vec<&str> = several.iter().map(|path| path.as_str()).collect();
            let place = raw.map_or_else(
                || PlanError {
                    place: Place::File(reader.file.to_owned()),
                    problem: Problem::SeveralWorkspaces(listed.join(", ")),
                },
                |rust| reader.at(rust, Problem::SeveralWorkspaces(listed.join(", "))),
            );
            return Err(place);
        }
    };
    let settings = raw.map(Spanned::get_ref);
    Ok(Some(RustSettings {
        workspace,
        scans: settings.map_or_default(|rust| rust.scans.clone()),
        mutation: settings.map_or_default(|rust| rust.mutation.clone()),
    }))
}

/// A Rust report's step.
fn built_in(report: Report, settings: &RustSettings) -> Step {
    let section = |file: &str| Output::Section {
        file: OutPath(Utf8PathBuf::from(file)),
    };
    let once = |reportable| {
        Runs::Once(Recipe {
            work: Work::Rust(reportable),
            env: BTreeMap::new(),
            collect: None,
        })
    };
    let (runs, needs, optional, reuse, timeout, outputs) = match report {
        Report::Scans => (
            once(report),
            vec![],
            false,
            false,
            DEFAULT_TIMEOUT,
            vec![section("scans.md")],
        ),
        Report::Lines => (
            once(report),
            vec![],
            false,
            false,
            DEFAULT_TIMEOUT,
            vec![section("lines.md")],
        ),
        Report::Mutants => (
            once(report),
            vec![],
            false,
            false,
            MUTANTS_TIMEOUT,
            vec![section("mutants.md")],
        ),
        Report::ApiListings => (
            Runs::PerRevision(Recipe {
                work: Work::Rust(report),
                env: BTreeMap::new(),
                collect: Some(
                    [
                        "bash",
                        "-c",
                        crate::rust::TOOLCHAIN_COLLECTOR,
                        "collect",
                        settings.workspace.as_str(),
                    ]
                    .into_iter()
                    .map(Template::text)
                    .collect(),
                ),
            }),
            vec![],
            true,
            true,
            DEFAULT_TIMEOUT,
            vec![section("listings.md")],
        ),
        Report::Api => (
            once(report),
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
        timeout,
        identity_env: default_identity_env(),
        outputs,
    }
}

/// Every need names a step of a kind its step can wait for, no ordinary step needs an exclusive one, and there is no
/// cycle.
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
