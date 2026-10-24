//! Distances in metres: lengths that cannot be negative, and signed gaps.

use std::fmt;

use crate::NotFinite;

/// A length that cannot be negative, in metres: radii, sizes, margins, shortfalls, jumps. Finite, at least zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Length(f64);

impl Length {
    /// Zero metres.
    pub const ZERO: Self = Self(0.0);

    /// `metres` as a length; none unless finite and at least zero. `-0.0` is accepted as `0.0`, so no length carries
    /// a sign bit. In a `const` item, `.expect` makes a literal.
    #[must_use]
    pub const fn new(metres: f64) -> Option<Self> {
        if metres.is_finite() && metres >= 0.0 {
            Some(Self(metres.abs()))
        } else {
            None
        }
    }

    /// The metres.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Length {
    type Error = InvalidLength;

    fn try_from(metres: f64) -> Result<Self, InvalidLength> {
        Self::new(metres).ok_or(InvalidLength)
    }
}

impl From<Length> for f64 {
    fn from(length: Length) -> Self {
        length.0
    }
}

/// A length that is negative or not finite, refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidLength;

impl fmt::Display for InvalidLength {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a length is a finite number of metres, at least zero")
    }
}

impl std::error::Error for InvalidLength {}

/// A signed distance in metres: a gap, negative where two shapes overlap. Finite.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Metres(f64);

impl Metres {
    /// `metres` as a distance; none unless finite.
    #[must_use]
    pub const fn new(metres: f64) -> Option<Self> {
        if metres.is_finite() {
            Some(Self(metres))
        } else {
            None
        }
    }

    /// The metres.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The nearer of two distances, as `f64::min` takes it.
    #[must_use]
    pub const fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }
}

impl TryFrom<f64> for Metres {
    type Error = NotFinite;

    fn try_from(metres: f64) -> Result<Self, NotFinite> {
        Self::new(metres).ok_or(NotFinite)
    }
}

impl From<Metres> for f64 {
    fn from(metres: Metres) -> Self {
        metres.0
    }
}

impl From<Length> for Metres {
    fn from(length: Length) -> Self {
        Self(length.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{Length, Metres};

    #[test]
    fn a_distance_is_any_finite_value_and_the_nearer_is_the_lesser() {
        let near = Metres::new(-0.01).unwrap();
        let far = Metres::new(0.5).unwrap();
        assert_eq!(near.min(far), near);
        assert_eq!(far.min(near), near);
        assert_eq!(Metres::new(f64::NEG_INFINITY), None);
        assert_eq!(Metres::from(Length::new(0.25).unwrap()).get(), 0.25);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_length_is_read_as_its_metres_and_a_negative_one_is_refused() {
        use serde::Deserialize as _;
        use serde::de::value::{Error, F64Deserializer};
        let read = |metres| Length::deserialize(F64Deserializer::<Error>::new(metres));
        assert_eq!(read(0.02).unwrap(), Length::new(0.02).unwrap());
        assert!(read(-0.02).is_err());
    }
}
