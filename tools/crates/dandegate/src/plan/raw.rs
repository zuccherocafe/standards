//! The steps file's TOML shapes, each value with its place in the text, before dandegate checks any of them.

use std::collections::BTreeMap;

use serde::Deserialize;
use toml::Spanned;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPlan {
    #[serde(default)]
    pub(super) project: RawProject,
    #[serde(default)]
    pub(super) reports: RawReports,
    #[serde(default)]
    pub(super) steps: RawSteps,
}

/// `[project]`: what the project is, one table for each language it holds.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawProject {
    #[serde(default)]
    pub(super) host: Vec<Spanned<String>>,
    pub(super) rust: Option<Spanned<RawRustProject>>,
    pub(super) godot: Option<Spanned<RawGodotProject>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawRustProject {
    pub(super) workspace: Spanned<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawGodotProject {
    pub(super) project: Spanned<String>,
    #[serde(default)]
    pub(super) tests: Vec<Spanned<String>>,
}

/// `[reports]`: how the built-in reports are tuned.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawReports {
    pub(super) measures: Option<Spanned<RawMeasures>>,
    pub(super) rust: Option<Spanned<RawRustReports>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct RawMeasures {
    #[serde(default)]
    pub(super) code: Vec<Spanned<String>>,
    #[serde(default)]
    pub(super) exclude: Vec<Spanned<String>>,
    #[serde(default)]
    pub(super) code_measures: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawRustReports {
    #[serde(default)]
    pub(super) scans: Vec<String>,
    #[serde(default)]
    pub(super) mutation: Vec<String>,
}

/// `[steps]`: what the project adds.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawSteps {
    #[serde(default)]
    pub(super) inputs: BTreeMap<Spanned<String>, Spanned<RawInput>>,
    #[serde(default)]
    pub(super) step: Vec<Spanned<RawStep>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawInput {
    pub(super) default: String,
    pub(super) help: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RawRuns {
    PerRevision,
    After,
    Once,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the steps file's independent switches, as written"
)]
pub(super) struct RawStep {
    pub(super) name: Spanned<String>,
    pub(super) runs: Option<Spanned<RawRuns>>,
    pub(super) run: Spanned<Vec<Spanned<String>>>,
    #[serde(default)]
    pub(super) needs: Vec<Spanned<String>>,
    #[serde(default)]
    pub(super) live: bool,
    #[serde(default)]
    pub(super) optional: bool,
    #[serde(default)]
    pub(super) reuse: bool,
    #[serde(default)]
    pub(super) exclusive: bool,
    #[serde(default)]
    pub(super) env: BTreeMap<Spanned<String>, Spanned<String>>,
    pub(super) timeout: Option<Spanned<String>>,
    pub(super) identity: Option<Spanned<RawIdentity>>,
    #[serde(default)]
    pub(super) output: Vec<Spanned<RawOutput>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawIdentity {
    #[serde(default)]
    pub(super) env: Vec<Spanned<String>>,
    pub(super) collect: Option<Spanned<Vec<Spanned<String>>>>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RawOutputKind {
    Images,
    Measures,
    Section,
}

impl RawOutputKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Measures => "measures",
            Self::Section => "section",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawOutput {
    pub(super) kind: Spanned<RawOutputKind>,
    pub(super) folder: Option<Spanned<String>>,
    pub(super) expected: Option<Spanned<String>>,
    pub(super) from: Option<Spanned<String>>,
    pub(super) file: Option<Spanned<String>>,
}
