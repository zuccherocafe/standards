//! Ends a report. It compares the two sides and writes the page, `measures.json` and the printed lines.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::compare::image::cells;
use crate::compare::measure::{MeasureRow, Measures, compare, worse};
use crate::page::{self, Images, Page, Section, Shown};
use crate::plan::Output;
use crate::report::schedule::{Instance, PerInstance, Place};
use crate::report::step::{Outcome, OutputCheck, expected_names};
use crate::report::{Finished, Session};
use crate::revision::{PerSide, Side};

impl Session<'_> {
    /// Compare the two sides, write the page and `measures.json`, and give the printed lines.
    pub(super) fn page(&self, outcomes: &PerInstance<Outcome>) -> Finished {
        let mut incomplete: Vec<String> = outcomes
            .iter()
            .flat_map(|(at, outcome)| {
                let instance = self.graph.instance(at);
                outcome
                    .incomplete()
                    .into_iter()
                    .map(move |why| format!("{} {}: {why}", instance.place, instance.step))
            })
            .collect();
        let Compared {
            mut own,
            images,
            sections,
            problems,
        } = self.compared(outcomes);
        incomplete.extend(problems);

        let measures = PerSide::from_fn(|side| {
            self.request.imported[side]
                .clone()
                .replaced_by(std::mem::take(&mut own[side]))
        });
        let rows = compare(&measures);
        let got_worse = worse(&rows);
        if let Err(error) = fs::write(
            self.request.out.join("measures.json"),
            measures.after.saved(),
        ) {
            incomplete.push(format!("measures.json could not be written: {error}"));
        }
        let code = self.measure_code();
        let prefixes = &self.plan().measures().code_measures;
        let affected: Vec<&MeasureRow> = rows
            .iter()
            .filter(|row| row.flagged())
            .filter(|row| {
                prefixes.is_empty()
                    || prefixes
                        .iter()
                        .any(|prefix| row.name().as_str().starts_with(prefix.as_str()))
            })
            .collect();
        let notes = self.notes.lock().map_or_default(|notes| notes.clone());
        let steps: Vec<Shown> = outcomes
            .iter()
            .map(|(at, outcome)| (at, self.graph.instance(at), outcome))
            .map(|(at, instance, outcome)| Shown {
                place: instance.place.to_string(),
                step: instance.step.to_string(),
                outcome: outcome.to_string(),
                failed: !outcome.incomplete().is_empty(),
                log: (!matches!(outcome, Outcome::Disabled | Outcome::Skipped(_)))
                    .then(|| relative(&self.request.out, &self.folder(instance).join("log.txt"))),
                reuse: notes.get(&at).map(|notes| notes.join("; ")),
                cpus: self.cpus.label(at),
            })
            .collect();
        let notices: Vec<String> = self
            .plan()
            .notices()
            .iter()
            .map(ToString::to_string)
            .collect();
        let page = Page {
            revisions: &self.request.revisions,
            incomplete: &incomplete,
            host: &self.host_warnings,
            notices: &notices,
            measure_code: code.as_ref().ok().map(Vec::as_slice),
            measure_code_error: code.as_ref().err().map(String::as_str),
            affected: &affected,
            worse: &got_worse,
            steps: &steps,
            sections: &sections,
            measures: &rows,
            images: &images,
        };
        let index = self.request.out.join("index.html");
        let written = fs::write(&index, page::render(&page));
        let mut lines = self.printed(
            outcomes,
            &incomplete,
            &notices,
            code.as_deref().unwrap_or_default(),
            &got_worse,
        );
        match &written {
            Ok(()) => lines.push(format!("DANDEGATE: {index}")),
            Err(error) => lines.push(format!("dandegate: the page could not be written: {error}")),
        }
        Finished {
            lines,
            written: written.is_ok(),
        }
    }

    /// Each step's measures, images and sections, read from the output folder, and the problems they show.
    fn compared(&self, outcomes: &PerInstance<Outcome>) -> Compared {
        let mut compared = Compared::default();
        for (name, step) in self.plan().steps() {
            for (index, output) in step.outputs().iter().enumerate() {
                // An instance that ran or was reused, with how this output reads.
                let ran = |place: Place| {
                    let at = self.graph.find(name, place)?;
                    let check = outcomes[at].outputs().get(index)?;
                    Some((self.folder(self.graph.instance(at)).join("out"), check))
                };
                match output {
                    Output::Measures { from } => {
                        for side in Side::BOTH {
                            let Some((out, OutputCheck::Complete)) = ran(Place::Revision(side))
                            else {
                                continue;
                            };
                            let read = fs::read_to_string(out.join(from.as_path()))
                                .map(|text| Measures::from_lines(&text));
                            if let Ok(Ok(measures)) = read {
                                let taken = std::mem::take(&mut compared.own[side]);
                                compared.own[side] = taken.merged(measures);
                            }
                        }
                    }
                    Output::Images { folder, expected } => {
                        let mut wanted = PerSide::<BTreeSet<String>>::default();
                        let mut found = PerSide::<BTreeMap<String, Utf8PathBuf>>::default();
                        for side in Side::BOTH {
                            let Some((out, _)) = ran(Place::Revision(side)) else {
                                continue;
                            };
                            let listed = fs::read_to_string(out.join(expected.as_path()))
                                .map(|text| expected_names(&text));
                            if let Ok(Ok(names)) = listed {
                                wanted[side] = names;
                            }
                            found[side] = pictures(&out.join(folder.as_path()));
                        }
                        let folders = PerSide::from_fn(|side| {
                            let instance = Instance {
                                step: name.clone(),
                                place: Place::Revision(side),
                            };
                            relative(
                                &self.request.out,
                                &self.folder(&instance).join("out").join(folder.as_path()),
                            )
                        });
                        let cells = cells(&wanted, &found);
                        for cell in &cells {
                            compared.problems.extend(
                                cell.problems().iter().map(|problem| {
                                    format!("{name} image {}: {problem}", cell.name())
                                }),
                            );
                        }
                        compared.images.push(Images {
                            step: name.to_string(),
                            folders,
                            cells,
                        });
                    }
                    Output::Section { file } => {
                        let instances = self.graph.ids().map(|at| self.graph.instance(at));
                        for instance in instances.filter(|instance| instance.step == *name) {
                            let Some((out, check)) = ran(instance.place) else {
                                continue;
                            };
                            let path = out.join(file.as_path());
                            compared.sections.push(Section {
                                title: format!("{} ({})", instance.step, instance.place),
                                link: relative(&self.request.out, &path),
                                text: fs::read_to_string(&path).ok(),
                                problem: check.problem(),
                            });
                        }
                    }
                }
            }
        }
        compared
    }

    /// The lines printed after the page is written, before its own.
    fn printed(
        &self,
        outcomes: &PerInstance<Outcome>,
        incomplete: &[String],
        notices: &[String],
        code: &[String],
        got_worse: &[&MeasureRow],
    ) -> Vec<String> {
        let mut lines: Vec<String> = incomplete
            .iter()
            .map(|why| format!("DANDEGATE-INCOMPLETE: {why}"))
            .collect();
        lines.extend(
            notices
                .iter()
                .map(|notice| format!("DANDEGATE-NOTICE: {notice}")),
        );
        lines.extend(
            code.iter()
                .map(|file| format!("DANDEGATE-MEASURE-CODE: {file}")),
        );
        for row in got_worse {
            let shown = |side: Side| {
                row.sides()[side]
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ")
            };
            lines.push(format!(
                "DANDEGATE-WORSE: {} {} -> {} ({})",
                row.name(),
                shown(Side::Before),
                shown(Side::After),
                row.change()
            ));
        }
        for (instance, outcome) in outcomes
            .iter()
            .map(|(at, outcome)| (self.graph.instance(at), outcome))
        {
            if matches!(outcome, Outcome::Reused { .. }) {
                let revision = self.place_revision(instance.place);
                lines.push(format!(
                    "DANDEGATE-REUSED: {} {} {revision}",
                    instance.place, instance.step
                ));
            }
        }
        for (instance, outcome) in outcomes
            .iter()
            .map(|(at, outcome)| (self.graph.instance(at), outcome))
        {
            if outcome.failed() {
                let revision = self.place_revision(instance.place);
                lines.push(format!(
                    "DANDEGATE-STEP-FAILED: {} {} {revision}: {outcome}",
                    instance.place, instance.step
                ));
            }
        }
        lines
    }

    /// The revision or revisions a place measures, as typed and resolved.
    fn place_revision(&self, place: Place) -> String {
        match place {
            Place::Revision(side) => self.request.revisions[side].to_string(),
            Place::Pair => format!(
                "{} -> {}",
                self.request.revisions.before, self.request.revisions.after
            ),
        }
    }

    /// The measuring code that differs between the two commits, exclusions left out.
    fn measure_code(&self) -> Result<Vec<String>, String> {
        let rules = self.plan().measures();
        if rules.code.is_empty() {
            return Ok(Vec::new());
        }
        let written: Vec<String> = rules.code.iter().map(ToString::to_string).collect();
        let paths: Vec<&str> = written.iter().map(String::as_str).collect();
        let changed = self
            .request
            .repository
            .changed(
                self.request.revisions.before.commit(),
                self.request.revisions.after.commit(),
                &paths,
            )
            .map_err(|error| error.to_string())?;
        Ok(changed
            .into_iter()
            .filter(|file| {
                !rules
                    .exclude
                    .iter()
                    .any(|excluded| Utf8Path::new(file).starts_with(excluded.as_path()))
            })
            .collect())
    }
}

/// What the steps' outputs hold, read from the output folder.
#[derive(Debug, Default)]
struct Compared {
    /// Each side's own step measures.
    own: PerSide<Measures>,
    images: Vec<Images>,
    sections: Vec<Section>,
    /// Missing, unexpected and unreadable images.
    problems: Vec<String>,
}

/// The PNGs directly in `folder`, by file name.
fn pictures(folder: &Utf8Path) -> BTreeMap<String, Utf8PathBuf> {
    let Ok(entries) = folder.read_dir_utf8() else {
        return BTreeMap::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension() == Some("png") && entry.path().is_file())
        .map(|entry| (entry.file_name().to_owned(), entry.path().to_owned()))
        .collect()
}

/// `path` relative to the output folder, for a link.
fn relative(out: &Utf8Path, path: &Utf8Path) -> String {
    path.strip_prefix(out)
        .map_or_else(|_| path.to_string(), ToString::to_string)
}
