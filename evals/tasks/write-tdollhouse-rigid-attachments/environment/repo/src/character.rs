use serde::{Deserialize, Serialize};
use tdoll_vocab::BoneName;

use crate::Error;

/// A material names one or more cut primitives, not a bone or a removable part.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MaterialName(String);

impl MaterialName {
    pub fn new(text: impl Into<String>) -> Result<Self, tdoll_vocab::EmptyName> {
        let text = text.into();
        BoneName::new(text.clone())?;
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for MaterialName {
    type Error = tdoll_vocab::EmptyName;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::new(text)
    }
}

impl From<MaterialName> for String {
    fn from(name: MaterialName) -> Self {
        name.0
    }
}

impl std::fmt::Display for MaterialName {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(out)
    }
}

/// Author intent before binding to the retained model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentDecl {
    pub piece: BoneName,
    pub surface: MaterialName,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Character {
    pub attachments: Vec<AttachmentDecl>,
}

impl Character {
    /// Read the focused character syntax: `character {`, one `attach "piece" to="material"` per line, and `}`.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut character = Self::default();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line == "character {" || line == "}" || line.starts_with("//") {
                continue;
            }
            let invalid = || Error::Declaration { line: index + 1 };
            let rest = line.strip_prefix("attach ").ok_or_else(invalid)?.trim();
            let (piece, rest) = quoted(rest).ok_or_else(invalid)?;
            let rest = rest.trim().strip_prefix("to=").ok_or_else(invalid)?;
            let (surface, trailing) = quoted(rest).ok_or_else(invalid)?;
            if !trailing.trim().is_empty() {
                return Err(invalid());
            }
            character.attachments.push(AttachmentDecl {
                piece: BoneName::new(piece).map_err(|_| invalid())?,
                surface: MaterialName::new(surface).map_err(|_| invalid())?,
            });
        }
        Ok(character)
    }
}

fn quoted(text: &str) -> Option<(&str, &str)> {
    let text = text.strip_prefix('"')?;
    let end = text.find('"')?;
    Some((&text[..end], &text[end + 1..]))
}
