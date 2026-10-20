//! Reads `MEASURE:` lines and saved measures records, and compares the two sides' observations by name.
//!
//! A line has the form `MEASURE: <name> <value> <unit> [higher-is-worse|lower-is-worse]`. The unit is every word
//! between the value and an optional direction word at the end, so a unit such as `mm before closure` stays whole.
//! dandegate calls a measure worse only when a line gives its direction, because only the code that takes a measure
//! knows which way is worse.
//!
//! A side keeps every distinct observation of a name. More than one observation is a conflict, and the page shows every
//! value. A side with no direction for a name borrows the other side's direction. When the two sides give different
//! directions, dandegate calls neither worse. The difference and the relative change use checked arithmetic. When one
//! of them cannot be represented, because the before value is zero or the difference overflows, the comparison holds
//! none and the page says so. The 5% marker says that a measure moved, not that the movement is significant.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::revision::PerSide;

/// The relative change past which a measure is marked as moved.
const MOVED: f64 = 0.05;
/// What a measure line starts with, after any indentation.
const PREFIX: &str = "MEASURE: ";
/// The saved record's format tag.
const FORMAT: &str = "dandegate-measures-v1";

/// A finite number.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub(crate) struct Finite(f64);

impl Finite {
    /// A finite constant. A value that is not finite fails to compile where the constant is built.
    pub(crate) const fn constant(value: f64) -> Self {
        assert!(value.is_finite(), "a finite constant");
        Self(value)
    }

    pub(crate) fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub(crate) fn get(self) -> f64 {
        self.0
    }
}

/// The shortest text that reads back as the same number, in exponent form when very large or small.
impl fmt::Display for Finite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let size = self.0.abs();
        if size != 0.0 && !(1e-5..1e15).contains(&size) {
            write!(formatter, "{:e}", self.0)
        } else {
            write!(formatter, "{}", self.0)
        }
    }
}

/// A measure's name: no whitespace, never empty.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MeasureName(Box<str>);

impl MeasureName {
    fn new(text: &str) -> Option<Self> {
        (!text.is_empty() && !text.contains(char::is_whitespace)).then(|| Self(text.into()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MeasureName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The way a measure gets worse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Direction {
    Higher,
    Lower,
}

impl Direction {
    /// The word a measure line ends with.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Higher => "higher-is-worse",
            Self::Lower => "lower-is-worse",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        match word {
            "higher-is-worse" => Some(Self::Higher),
            "lower-is-worse" => Some(Self::Lower),
            _ => None,
        }
    }

    /// Whether going from `before` to `after` goes this way.
    fn worsened(self, before: f64, after: f64) -> bool {
        match self {
            Self::Higher => after > before,
            Self::Lower => after < before,
        }
    }
}

/// One observed value of a measure, with its unit, which may be several words or none, and which way it is worse.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Observation {
    value: Finite,
    unit: String,
    worse: Option<Direction>,
}

impl Observation {
    pub(crate) fn worse(&self) -> Option<Direction> {
        self.worse
    }
}

/// The observation as the page shows it: value, unit and direction.
impl fmt::Display for Observation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.value)?;
        if !self.unit.is_empty() {
            write!(formatter, " {}", self.unit)?;
        }
        if let Some(worse) = self.worse {
            write!(formatter, " ({})", worse.word())?;
        }
        Ok(())
    }
}

/// A side's measures: each name's distinct observations, in the order first seen.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Measures(IndexMap<MeasureName, Vec<Observation>>);

/// A measure line that cannot be read.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {why}")]
pub(crate) struct LineError {
    line: usize,
    why: LineProblem,
}

/// Why a measure line cannot be read.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum LineProblem {
    #[error("a measure line is `MEASURE: <name> <value> <unit>`")]
    Shape,
    #[error("{0:?} is not a finite number")]
    Value(String),
}

/// A saved measures record that cannot be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SavedError {
    #[error("it is not JSON")]
    Json(#[source] serde_json::Error),
    #[error("its format is {0:?}, not {FORMAT:?} or an earlier report's")]
    Format(String),
    #[error("{name}: {why}")]
    Shape { name: String, why: &'static str },
}

/// The saved record, in the current format.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    format: String,
    measures: BTreeMap<String, Vec<SavedObservation>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedObservation {
    value: f64,
    unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worse: Option<Direction>,
}

impl Measures {
    /// Add an observation to `name`'s unless it is already there.
    fn observe(&mut self, name: MeasureName, observation: Observation) {
        let observations = self.0.entry(name).or_default();
        if !observations.contains(&observation) {
            observations.push(observation);
        }
    }

    /// The `MEASURE:` lines of `text`, a step's measures file or a gate log. It ignores every other line.
    ///
    /// # Errors
    /// [`LineError`] for the first measure line without a name and a finite value.
    pub(crate) fn from_lines(text: &str) -> Result<Self, LineError> {
        let mut measures = Self::default();
        for (index, line) in text.lines().enumerate() {
            let Some(rest) = line.trim_start().strip_prefix(PREFIX) else {
                continue;
            };
            let refuse = |why| LineError {
                line: index + 1,
                why,
            };
            let mut words: Vec<&str> = rest.split_whitespace().collect();
            let worse = words.last().and_then(|word| Direction::from_word(word));
            if worse.is_some() {
                words.pop();
            }
            let [name, value, unit @ ..] = &words[..] else {
                return Err(refuse(LineProblem::Shape));
            };
            let name = MeasureName::new(name).ok_or_else(|| refuse(LineProblem::Shape))?;
            let value = value
                .parse()
                .ok()
                .and_then(Finite::new)
                .ok_or_else(|| refuse(LineProblem::Value((*value).to_owned())))?;
            measures.observe(
                name,
                Observation {
                    value,
                    unit: unit.join(" "),
                    worse,
                },
            );
        }
        Ok(measures)
    }

    /// A record this tool saved, or an earlier report's: `{name: [[value, unit], ...]}`, or before that
    /// `{name: [value, unit]}` and, for a conflict, `{name: [[values], unit]}`.
    ///
    /// # Errors
    /// [`SavedError`] for text that is none of these.
    pub(crate) fn from_saved(text: &str) -> Result<Self, SavedError> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(SavedError::Json)?;
        if let Some(format) = value.get("format") {
            if format.as_str() != Some(FORMAT) {
                return Err(SavedError::Format(format.to_string()));
            }
            let saved: Saved = serde_json::from_value(value).map_err(SavedError::Json)?;
            let mut measures = Self::default();
            for (name, observations) in saved.measures {
                let checked = MeasureName::new(&name).ok_or(SavedError::Shape {
                    name: name.clone(),
                    why: "is not a measure's name",
                })?;
                for saved in observations {
                    let value = Finite::new(saved.value).ok_or(SavedError::Shape {
                        name: name.clone(),
                        why: "holds a value that is not finite",
                    })?;
                    measures.observe(
                        checked.clone(),
                        Observation {
                            value,
                            unit: saved.unit,
                            worse: saved.worse,
                        },
                    );
                }
            }
            return Ok(measures);
        }
        let serde_json::Value::Object(entries) = value else {
            return Err(SavedError::Format("not an object".to_owned()));
        };
        let mut measures = Self::default();
        for (name, entry) in entries {
            let shape = |why| SavedError::Shape {
                name: name.clone(),
                why,
            };
            let checked =
                MeasureName::new(&name).ok_or_else(|| shape("is not a measure's name"))?;
            for (value, unit) in earlier_observations(&entry)
                .ok_or_else(|| shape("is not a list of observations"))?
            {
                let value =
                    Finite::new(value).ok_or_else(|| shape("holds a value that is not finite"))?;
                measures.observe(
                    checked.clone(),
                    Observation {
                        value,
                        unit: unit.to_owned(),
                        worse: None,
                    },
                );
            }
        }
        Ok(measures)
    }

    /// The record `measures.json` holds.
    pub(crate) fn saved(&self) -> String {
        let measures = self
            .0
            .iter()
            .map(|(name, observations)| {
                let saved = observations
                    .iter()
                    .map(|observation| SavedObservation {
                        value: observation.value.get(),
                        unit: observation.unit.clone(),
                        worse: observation.worse,
                    })
                    .collect();
                (name.to_string(), saved)
            })
            .collect();
        let saved = Saved {
            format: FORMAT.to_owned(),
            measures,
        };
        // A record of strings and finite numbers always serialises.
        serde_json::to_string_pretty(&saved).unwrap_or_default() + "\n"
    }

    /// These measures with `own`'s names replaced by `own`'s observations: a revision's own step measures stand for
    /// their names, whatever a gate log or earlier report says.
    pub(crate) fn replaced_by(mut self, own: Self) -> Self {
        for (name, observations) in own.0 {
            self.0.insert(name, observations);
        }
        self
    }

    /// These measures with every observation of `other` added.
    pub(crate) fn merged(mut self, other: Self) -> Self {
        for (name, observations) in other.0 {
            for observation in observations {
                self.observe(name.clone(), observation);
            }
        }
        self
    }
}

/// An earlier report's observations of one name, or none for any other shape.
fn earlier_observations(entry: &serde_json::Value) -> Option<Vec<(f64, &str)>> {
    let items = entry.as_array()?;
    if let [value, serde_json::Value::String(unit)] = &items[..] {
        let values = match value {
            serde_json::Value::Array(values) => values
                .iter()
                .map(serde_json::Value::as_f64)
                .collect::<Option<Vec<_>>>()?,
            single => vec![single.as_f64()?],
        };
        return Some(
            values
                .into_iter()
                .map(|value| (value, unit.as_str()))
                .collect(),
        );
    }
    items
        .iter()
        .map(|item| match item.as_array().map(Vec::as_slice) {
            Some([value, serde_json::Value::String(unit)]) => {
                Some((value.as_f64()?, unit.as_str()))
            }
            _ => None,
        })
        .collect()
}

/// How a measure compares between the two sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Verdict {
    Same,
    /// A different value in the same unit: the difference and the change relative to the before value, each none
    /// where it cannot be represented, and the direction it got worse in, if it did.
    Moved {
        change: Option<Finite>,
        relative: Option<Finite>,
        worse: Option<Direction>,
    },
    New,
    Gone,
    UnitChanged,
    Conflict,
}

/// One measure's observations on both sides and how they compare.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MeasureRow {
    name: MeasureName,
    sides: PerSide<Vec<Observation>>,
    verdict: Verdict,
}

impl MeasureRow {
    pub(crate) fn name(&self) -> &MeasureName {
        &self.name
    }

    pub(crate) fn sides(&self) -> &PerSide<Vec<Observation>> {
        &self.sides
    }

    /// Whether the row is listed first: anything but the same value or a change within the movement marker.
    pub(crate) fn flagged(&self) -> bool {
        match self.verdict {
            Verdict::Same => false,
            Verdict::Moved { relative, .. } => {
                relative.is_none_or(|relative| relative.get() > MOVED)
            }
            Verdict::New | Verdict::Gone | Verdict::UnitChanged | Verdict::Conflict => true,
        }
    }

    /// Whether both sides give a direction and they differ, so neither is used.
    pub(crate) fn directions_disagree(&self) -> bool {
        let first =
            |observations: &[Observation]| observations.first().and_then(Observation::worse);
        matches!(
            (first(&self.sides.before), first(&self.sides.after)),
            (Some(before), Some(after)) if before != after
        )
    }

    /// The change column's text.
    pub(crate) fn change(&self) -> String {
        match self.verdict {
            Verdict::Same => String::new(),
            Verdict::New => "new".to_owned(),
            Verdict::Gone => "gone".to_owned(),
            Verdict::UnitChanged => "unit changed".to_owned(),
            Verdict::Conflict => "conflict".to_owned(),
            Verdict::Moved {
                change,
                relative,
                worse,
            } => {
                let mut parts = vec![change.map_or_else(
                    || "difference too large to represent".to_owned(),
                    |change| Signed(change).to_string(),
                )];
                // A finite relative change can still overflow as a percentage.
                let percent = relative.and_then(|relative| Finite::new(relative.get() * 100.0));
                parts.push(percent.map_or_else(
                    || "relative change not representable".to_owned(),
                    |percent| format!("{:.1}%", percent.get()),
                ));
                if self.flagged() {
                    parts.push("moved".to_owned());
                }
                if let Some(worse) = worse {
                    parts.push(format!("worse ({})", worse.word()));
                }
                if self.directions_disagree() {
                    parts.push("directions disagree".to_owned());
                }
                parts.join(", ")
            }
        }
    }
}

/// A number with its sign always written.
struct Signed(Finite);

impl fmt::Display for Signed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.get() >= 0.0 {
            formatter.write_str("+")?;
        }
        write!(formatter, "{}", self.0)
    }
}

/// Every name either side has, compared, flagged rows first and each group by name.
pub(crate) fn compare(sides: &PerSide<Measures>) -> Vec<MeasureRow> {
    let mut names: Vec<&MeasureName> = sides.before.0.keys().chain(sides.after.0.keys()).collect();
    names.sort();
    names.dedup();
    let mut rows: Vec<MeasureRow> = names
        .into_iter()
        .map(|name| {
            let observed = |measures: &Measures| measures.0.get(name).cloned().unwrap_or_default();
            let observations = PerSide {
                before: observed(&sides.before),
                after: observed(&sides.after),
            };
            let verdict = verdict(&observations);
            MeasureRow {
                name: name.clone(),
                sides: observations,
                verdict,
            }
        })
        .collect();
    rows.sort_by_key(|row| !row.flagged());
    rows
}

fn verdict(sides: &PerSide<Vec<Observation>>) -> Verdict {
    match (&sides.before[..], &sides.after[..]) {
        ([_, _, ..], _) | (_, [_, _, ..]) => Verdict::Conflict,
        ([], _) => Verdict::New,
        (_, []) => Verdict::Gone,
        ([before], [after]) if before.unit != after.unit => Verdict::UnitChanged,
        ([before], [after]) => {
            let (old, new) = (before.value.get(), after.value.get());
            #[expect(
                clippy::float_cmp,
                reason = "a measure is the same only when its value is exactly the same"
            )]
            let same = old == new;
            if same {
                return Verdict::Same;
            }
            let change = Finite::new(new - old);
            let relative = change
                .filter(|_| old != 0.0)
                .and_then(|change| Finite::new(change.get().abs() / old.abs()));
            let direction = match (before.worse, after.worse) {
                (Some(old_way), Some(new_way)) if old_way != new_way => None,
                (old_way, new_way) => new_way.or(old_way),
            };
            Verdict::Moved {
                change,
                relative,
                worse: direction.filter(|way| way.worsened(old, new)),
            }
        }
    }
}

/// The rows that moved in their worse direction, the largest relative change first, an unrepresentable one first of
/// all.
pub(crate) fn worse(rows: &[MeasureRow]) -> Vec<&MeasureRow> {
    let mut found: Vec<(Option<Finite>, &MeasureRow)> = rows
        .iter()
        .filter_map(|row| match row.verdict {
            Verdict::Moved {
                relative,
                worse: Some(_),
                ..
            } => Some((relative, row)),
            Verdict::Moved { worse: None, .. }
            | Verdict::Same
            | Verdict::New
            | Verdict::Gone
            | Verdict::UnitChanged
            | Verdict::Conflict => None,
        })
        .collect();
    found.sort_by(|(left, _), (right, _)| match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => right.get().total_cmp(&left.get()),
    });
    found.into_iter().map(|(_, row)| row).collect()
}

#[cfg(test)]
mod tests;
