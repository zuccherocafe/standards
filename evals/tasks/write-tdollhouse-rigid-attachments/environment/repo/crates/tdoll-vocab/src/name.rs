//! Bone names as a model or skeleton writes them.

use std::fmt;

/// A bone, as its model or skeleton names it: the authored text, never empty. Two names are the same bone only when
/// their text is the same, case included.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct BoneName(Box<str>);

impl BoneName {
    /// `name` as a bone's name.
    ///
    /// # Errors
    ///
    /// [`EmptyName`] for an empty name, which names no bone.
    pub fn new(name: impl Into<Box<str>>) -> Result<Self, EmptyName> {
        let name = name.into();
        if name.is_empty() {
            Err(EmptyName)
        } else {
            Ok(Self(name))
        }
    }

    /// The name's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BoneName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for BoneName {
    type Error = EmptyName;

    fn try_from(name: String) -> Result<Self, EmptyName> {
        Self::new(name)
    }
}

impl From<BoneName> for String {
    fn from(name: BoneName) -> Self {
        name.0.into()
    }
}

/// An empty bone name, refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyName;

impl fmt::Display for EmptyName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bone's name is not empty")
    }
}

impl std::error::Error for EmptyName {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "serde")]
    #[test]
    fn a_name_is_read_as_its_text_and_an_empty_one_is_refused() {
        use serde::Deserialize as _;
        use serde::de::value::{Error, StrDeserializer};

        use super::BoneName;
        let read = |text| BoneName::deserialize(StrDeserializer::<Error>::new(text));
        assert_eq!(read("Hips").unwrap(), BoneName::new("Hips").unwrap());
        assert!(read("").is_err());
    }
}
