//! The body's two sides, and a value for each.

use std::fmt;
use std::ops::{Index, IndexMut};

/// A side of the body, the character's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Side {
    /// The character's left.
    Left,
    /// The character's right.
    Right,
}

impl Side {
    /// Both sides, left first.
    pub const BOTH: [Self; 2] = [Self::Left, Self::Right];

    /// The other side.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Left => "left",
            Self::Right => "right",
        })
    }
}

/// One value for each side. Any pair is valid; it does not prove the two belong to one character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerSide<T> {
    /// The left side's.
    pub left: T,
    /// The right side's.
    pub right: T,
}

impl<T> PerSide<T> {
    /// Each side's value from `make`, left first.
    pub fn from_fn(mut make: impl FnMut(Side) -> T) -> Self {
        let left = make(Side::Left);
        Self {
            left,
            right: make(Side::Right),
        }
    }

    /// Each value mapped, left first.
    pub fn map<U>(self, mut map: impl FnMut(T) -> U) -> PerSide<U> {
        let left = map(self.left);
        PerSide {
            left,
            right: map(self.right),
        }
    }

    /// Each side and its value, left first.
    pub fn iter(&self) -> impl Iterator<Item = (Side, &T)> {
        [(Side::Left, &self.left), (Side::Right, &self.right)].into_iter()
    }
}

impl<T> Index<Side> for PerSide<T> {
    type Output = T;

    fn index(&self, side: Side) -> &T {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }
}

impl<T> IndexMut<Side> for PerSide<T> {
    fn index_mut(&mut self, side: Side) -> &mut T {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }
}

impl<T> From<[T; 2]> for PerSide<T> {
    fn from([left, right]: [T; 2]) -> Self {
        Self { left, right }
    }
}

impl<T> From<PerSide<T>> for [T; 2] {
    fn from(pair: PerSide<T>) -> Self {
        [pair.left, pair.right]
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for PerSide<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        [&self.left, &self.right].serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for PerSide<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <[T; 2]>::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::Side;

    #[test]
    fn a_side_reads_in_lowercase() {
        assert_eq!(format!("{} {}", Side::Left, Side::Right), "left right");
        assert_eq!(format!("{:?}", Side::Left), "Left");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_side_and_a_pair_read_as_today_s_records_write_them() {
        use serde::Deserialize as _;
        use serde::de::value::{Error, SeqDeserializer, StrDeserializer};

        use super::PerSide;
        assert_eq!(
            Side::deserialize(StrDeserializer::<Error>::new("left")).unwrap(),
            Side::Left
        );
        assert!(Side::deserialize(StrDeserializer::<Error>::new("Left")).is_err());
        let pair =
            PerSide::<u32>::deserialize(SeqDeserializer::<_, Error>::new([3_u32, 4].into_iter()));
        assert_eq!(pair.unwrap(), PerSide { left: 3, right: 4 });
    }
}
