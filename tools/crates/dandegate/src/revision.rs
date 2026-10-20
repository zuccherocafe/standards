//! The two revisions a report compares. A revision holds what the person typed and the commit it named when the report
//! started, and each revision stands on one side. dandegate resolves each revision once, before any work, so a branch
//! that moves while steps run changes nothing. Every step, page line and printed line uses the commit.

pub(crate) mod git;

use std::fmt;
use std::ops::{Index, IndexMut};

/// A commit's full object name: 40 lowercase hexadecimal digits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CommitId(Box<str>);

/// Text that is not a full commit name.
#[derive(Debug, thiserror::Error)]
#[error("{text:?} is not a full commit name")]
pub(crate) struct BadCommit {
    text: String,
}

impl CommitId {
    /// # Errors
    /// [`BadCommit`] unless `text` is 40 lowercase hexadecimal digits.
    pub(crate) fn new(text: &str) -> Result<Self, BadCommit> {
        let hex = |byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
        if text.len() == 40 && text.bytes().all(hex) {
            Ok(Self(text.into()))
        } else {
            Err(BadCommit {
                text: text.to_owned(),
            })
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A revision as typed and the commit it resolved to.
#[derive(Clone, Debug)]
pub(crate) struct Revision {
    requested: String,
    commit: CommitId,
}

impl Revision {
    pub(crate) fn new(requested: String, commit: CommitId) -> Self {
        Self { requested, commit }
    }

    pub(crate) fn commit(&self) -> &CommitId {
        &self.commit
    }
}

/// Both names a person needs to recognise a revision: `main (0123…)`, with the full commit.
impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} ({})", self.requested, self.commit)
    }
}

/// One of the two revisions a report compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Side {
    Before,
    After,
}

impl Side {
    pub(crate) const BOTH: [Self; 2] = [Self::Before, Self::After];

    /// The side's name as folders, page lines and printed lines spell it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

/// One value for each side.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerSide<T> {
    pub(crate) before: T,
    pub(crate) after: T,
}

impl<T> PerSide<T> {
    /// The value `make` gives for each side, before first.
    pub(crate) fn from_fn(mut make: impl FnMut(Side) -> T) -> Self {
        let before = make(Side::Before);
        let after = make(Side::After);
        Self { before, after }
    }

    pub(crate) fn as_ref(&self) -> PerSide<&T> {
        PerSide {
            before: &self.before,
            after: &self.after,
        }
    }

    pub(crate) fn map<U>(self, mut change: impl FnMut(Side, T) -> U) -> PerSide<U> {
        let before = change(Side::Before, self.before);
        let after = change(Side::After, self.after);
        PerSide { before, after }
    }
}

impl<T> Index<Side> for PerSide<T> {
    type Output = T;

    fn index(&self, side: Side) -> &T {
        match side {
            Side::Before => &self.before,
            Side::After => &self.after,
        }
    }
}

impl<T> IndexMut<Side> for PerSide<T> {
    fn index_mut(&mut self, side: Side) -> &mut T {
        match side {
            Side::Before => &mut self.before,
            Side::After => &mut self.after,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commit_is_forty_lowercase_hexadecimal_digits() {
        assert!(CommitId::new("0123456789abcdef0123456789abcdef01234567").is_ok());
        for text in [
            "",
            "0123456789abcdef0123456789abcdef0123456",
            "0123456789abcdef0123456789abcdef012345678",
            "0123456789ABCDEF0123456789abcdef01234567",
            "0123456789abcdeg0123456789abcdef01234567",
            "main",
        ] {
            assert!(CommitId::new(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn a_side_indexes_its_own_value() {
        let mut sides = PerSide {
            before: 1,
            after: 2,
        };
        sides[Side::After] += 10;
        assert_eq!((sides[Side::Before], sides[Side::After]), (1, 12));
        assert_eq!(
            PerSide::from_fn(Side::as_str),
            PerSide {
                before: "before",
                after: "after"
            }
        );
    }
}
