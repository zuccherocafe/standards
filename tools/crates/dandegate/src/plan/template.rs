//! A command word or environment value from the steps file, with its placeholders parsed.
//!
//! A template's argument type decides which placeholders it can hold. A per-revision or after step's templates take
//! [`TreeArg`], and a once step's templates take [`PairArg`], so no template can hold the other kind's placeholder.
//! `{out}`, `{adapters}`, `{cache}` and `{input.<name>}` are open to both kinds. `{cache}` is the step's build cache at
//! its place, a folder kept from one report to the next (`crate::report::cache`).

use std::fmt::Write as _;

use camino::Utf8Path;
use indexmap::IndexMap;

use crate::plan::{Input, InputName};
use crate::revision::CommitId;

/// The placeholders a kind of step fills from its revisions, and the values it fills them with.
pub(crate) trait Arg: Copy {
    /// What the step's revisions give its placeholders.
    type Values<'a>;
    /// The placeholders, as written between braces, for a refusal to list.
    const NAMES: &'static str;
    /// The placeholder `name` names, if this kind has it.
    fn named(name: &str) -> Option<Self>;
    fn name(self) -> &'static str;
    fn value<'a>(self, values: &'a Self::Values<'_>) -> &'a str;
}

/// A per-revision or after step's placeholders: its checkout and the commit checked out there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TreeArg {
    Tree,
    Commit,
}

/// What a per-revision or after step's placeholders stand for.
#[derive(Debug)]
pub(crate) struct TreeValues<'a> {
    pub(crate) tree: &'a Utf8Path,
    pub(crate) commit: &'a CommitId,
}

impl Arg for TreeArg {
    type Values<'a> = TreeValues<'a>;
    const NAMES: &'static str = "{tree}, {commit}, {out}, {adapters}, {cache} and {input.<name>}";

    fn named(name: &str) -> Option<Self> {
        match name {
            "tree" => Some(Self::Tree),
            "commit" => Some(Self::Commit),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Commit => "commit",
        }
    }

    fn value<'a>(self, values: &'a TreeValues<'_>) -> &'a str {
        match self {
            Self::Tree => values.tree.as_str(),
            Self::Commit => values.commit.as_str(),
        }
    }
}

/// A once step's placeholders: both checkouts and both commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PairArg {
    Before,
    After,
    BeforeCommit,
    AfterCommit,
}

/// What a once step's placeholders stand for.
#[derive(Debug)]
pub(crate) struct PairValues<'a> {
    pub(crate) before: TreeValues<'a>,
    pub(crate) after: TreeValues<'a>,
}

impl Arg for PairArg {
    type Values<'a> = PairValues<'a>;
    const NAMES: &'static str = "{before}, {after}, {before-commit}, {after-commit}, {out}, {adapters}, {cache} and {input.<name>}";

    fn named(name: &str) -> Option<Self> {
        match name {
            "before" => Some(Self::Before),
            "after" => Some(Self::After),
            "before-commit" => Some(Self::BeforeCommit),
            "after-commit" => Some(Self::AfterCommit),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
            Self::BeforeCommit => "before-commit",
            Self::AfterCommit => "after-commit",
        }
    }

    fn value<'a>(self, values: &'a PairValues<'_>) -> &'a str {
        match self {
            Self::Before => values.before.tree.as_str(),
            Self::After => values.after.tree.as_str(),
            Self::BeforeCommit => values.before.commit.as_str(),
            Self::AfterCommit => values.after.commit.as_str(),
        }
    }
}

/// One part of a template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Piece<A> {
    Text(String),
    Arg(A),
    Input(InputName),
    Out,
    Adapters,
    Cache,
}

/// A command word or environment value with its placeholders parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Template<A>(Vec<Piece<A>>);

/// Why a template's text cannot be read.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TemplateError {
    #[error("a `{{` opens a placeholder that has no `}}`")]
    Unclosed,
    #[error("a `}}` closes no placeholder")]
    Unopened,
    #[error("{{{name}}} is not a placeholder this kind of step has; it has {names}")]
    Unknown { name: String, names: &'static str },
    #[error("{{input.{name}}} names no input [steps.inputs] declares")]
    Undeclared { name: String },
}

/// What a template's placeholders are filled with when its step runs.
#[derive(Debug)]
pub(crate) struct Expansion<'a, 'v, A: Arg> {
    pub(crate) values: &'a A::Values<'v>,
    pub(crate) inputs: &'a IndexMap<InputName, Input>,
    pub(crate) out: &'a Utf8Path,
    pub(crate) adapters: &'a Utf8Path,
    pub(crate) cache: &'a Utf8Path,
}

impl<A: Arg> Template<A> {
    /// The template `text` writes, its inputs checked against `inputs`.
    ///
    /// # Errors
    /// [`TemplateError`] for a brace that opens or closes nothing, a placeholder this kind of step does not have,
    /// or an input not declared.
    pub(crate) fn parse(
        text: &str,
        inputs: &IndexMap<InputName, Input>,
    ) -> Result<Self, TemplateError> {
        let mut pieces = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find(['{', '}']) {
            if rest[at..].starts_with('}') {
                return Err(TemplateError::Unopened);
            }
            if at > 0 {
                pieces.push(Piece::Text(rest[..at].to_owned()));
            }
            let inside = &rest[at + 1..];
            let end = inside
                .find(['{', '}'])
                .filter(|&end| inside[end..].starts_with('}'))
                .ok_or(TemplateError::Unclosed)?;
            pieces.push(Self::placeholder(&inside[..end], inputs)?);
            rest = &inside[end + 1..];
        }
        if !rest.is_empty() {
            pieces.push(Piece::Text(rest.to_owned()));
        }
        Ok(Self(pieces))
    }

    /// A template of plain text, which has no placeholder.
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self(vec![Piece::Text(text.into())])
    }

    fn placeholder(
        name: &str,
        inputs: &IndexMap<InputName, Input>,
    ) -> Result<Piece<A>, TemplateError> {
        if let Some(input) = name.strip_prefix("input.") {
            return InputName::new(input)
                .ok()
                .filter(|input| inputs.contains_key(input))
                .map(Piece::Input)
                .ok_or_else(|| TemplateError::Undeclared {
                    name: input.to_owned(),
                });
        }
        match name {
            "out" => Ok(Piece::Out),
            "adapters" => Ok(Piece::Adapters),
            "cache" => Ok(Piece::Cache),
            _ => A::named(name)
                .map(Piece::Arg)
                .ok_or_else(|| TemplateError::Unknown {
                    name: name.to_owned(),
                    names: A::NAMES,
                }),
        }
    }

    /// The text with every placeholder filled in.
    pub(crate) fn expand(&self, expansion: &Expansion<'_, '_, A>) -> String {
        let mut text = String::new();
        for piece in &self.0 {
            match piece {
                Piece::Text(part) => text.push_str(part),
                Piece::Arg(arg) => text.push_str(arg.value(expansion.values)),
                Piece::Input(name) => {
                    text.push_str(expansion.inputs.get(name).map_or("", Input::value));
                }
                Piece::Out => text.push_str(expansion.out.as_str()),
                Piece::Adapters => text.push_str(expansion.adapters.as_str()),
                Piece::Cache => text.push_str(expansion.cache.as_str()),
            }
        }
        text
    }

    /// The template as the steps file writes it, placeholders by name: what a kept result's identity holds, so a
    /// new output or scratch folder leaves it unchanged.
    pub(crate) fn written(&self) -> String {
        let mut text = String::new();
        for piece in &self.0 {
            // Writing to a String cannot fail.
            let _ = match piece {
                Piece::Text(part) => write!(text, "{part}"),
                Piece::Arg(arg) => write!(text, "{{{}}}", arg.name()),
                Piece::Input(name) => write!(text, "{{input.{name}}}"),
                Piece::Out => write!(text, "{{out}}"),
                Piece::Adapters => write!(text, "{{adapters}}"),
                Piece::Cache => write!(text, "{{cache}}"),
            };
        }
        text
    }

    /// The inputs the template uses.
    pub(crate) fn inputs(&self) -> impl Iterator<Item = &InputName> {
        self.0.iter().filter_map(|piece| match piece {
            Piece::Input(name) => Some(name),
            Piece::Text(_) | Piece::Arg(_) | Piece::Out | Piece::Adapters | Piece::Cache => None,
        })
    }

    /// Whether the template names the step's build cache.
    pub(crate) fn uses_cache(&self) -> bool {
        self.0.iter().any(|piece| matches!(piece, Piece::Cache))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> IndexMap<InputName, Input> {
        let mut inputs = IndexMap::new();
        inputs.insert(
            InputName::new("renders").expect("a name"),
            Input::new("3".to_owned(), String::new()),
        );
        inputs
    }

    #[test]
    fn placeholders_are_filled_and_text_is_kept() {
        let template = Template::<TreeArg>::parse(
            "--at={tree}/x:{commit} {input.renders}{out}{adapters}{cache}",
            &inputs(),
        )
        .expect("a template");
        let commit = CommitId::new("0123456789abcdef0123456789abcdef01234567").expect("a commit");
        let values = TreeValues {
            tree: Utf8Path::new("/t"),
            commit: &commit,
        };
        let expansion = Expansion::<TreeArg> {
            values: &values,
            inputs: &inputs(),
            out: Utf8Path::new("/o"),
            adapters: Utf8Path::new("/a"),
            cache: Utf8Path::new("/c"),
        };
        assert_eq!(
            template.expand(&expansion),
            "--at=/t/x:0123456789abcdef0123456789abcdef01234567 3/o/a/c"
        );
        assert_eq!(
            template.written(),
            "--at={tree}/x:{commit} {input.renders}{out}{adapters}{cache}"
        );
        assert!(template.uses_cache());
    }

    #[test]
    fn a_placeholder_of_the_other_kind_is_refused() {
        assert!(matches!(
            Template::<TreeArg>::parse("{before}", &inputs()),
            Err(TemplateError::Unknown { name, .. }) if name == "before"
        ));
        assert!(matches!(
            Template::<PairArg>::parse("{tree}", &inputs()),
            Err(TemplateError::Unknown { name, .. }) if name == "tree"
        ));
        let pair = Template::<PairArg>::parse(
            "{before}:{after}:{before-commit}:{after-commit}",
            &inputs(),
        )
        .expect("a template");
        assert_eq!(
            pair.written(),
            "{before}:{after}:{before-commit}:{after-commit}"
        );
    }

    #[test]
    fn a_stray_brace_or_undeclared_input_is_refused() {
        for (text, error) in [
            ("{tree", TemplateError::Unclosed),
            ("{{tree}", TemplateError::Unclosed),
            ("tree}", TemplateError::Unopened),
            ("}{tree}", TemplateError::Unopened),
            (
                "{input.nope}",
                TemplateError::Undeclared {
                    name: "nope".to_owned(),
                },
            ),
        ] {
            assert_eq!(
                Template::<TreeArg>::parse(text, &inputs()),
                Err(error),
                "{text}"
            );
        }
    }
}
