//! Angles: radians for arithmetic, degrees where a record or an authored file writes them.

use std::fmt;

/// An angle in radians. Finite.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Radians(f64);

impl Radians {
    /// No turn.
    pub const ZERO: Self = Self(0.0);

    /// `radians` as an angle; none unless finite.
    #[must_use]
    pub const fn new(radians: f64) -> Option<Self> {
        if radians.is_finite() {
            Some(Self(radians))
        } else {
            None
        }
    }

    /// The radians.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The lesser of two angles, as `f64::min` takes it.
    #[must_use]
    pub const fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// The greater of two angles, as `f64::max` takes it.
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    /// The same angle in degrees, as `f64::to_degrees` gives it; none where that leaves the finite range.
    #[must_use]
    pub const fn to_degrees(self) -> Option<Degrees> {
        Degrees::new(self.0.to_degrees())
    }
}

impl TryFrom<f64> for Radians {
    type Error = NotFinite;

    fn try_from(radians: f64) -> Result<Self, NotFinite> {
        Self::new(radians).ok_or(NotFinite)
    }
}

impl From<Radians> for f64 {
    fn from(radians: Radians) -> Self {
        radians.0
    }
}

/// An angle in degrees, the unit a record or an authored file writes and some kernels search in. Finite.
///
/// A record that writes degrees holds a `Degrees`, not a [`Radians`] converted on the way out: a degree value turned
/// to radians and back is not always the same `f64`.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Degrees(f64);

impl Degrees {
    /// No angle.
    pub const ZERO: Self = Self(0.0);

    /// `degrees` as an angle; none unless finite. In a `const` item, `.expect` makes a literal.
    #[must_use]
    pub const fn new(degrees: f64) -> Option<Self> {
        if degrees.is_finite() {
            Some(Self(degrees))
        } else {
            None
        }
    }

    /// The degrees.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The same angle in radians, as `f64::to_radians` gives it: the one conversion where degrees meet trigonometry.
    #[must_use]
    pub const fn to_radians(self) -> Radians {
        Radians(self.0.to_radians())
    }
}

impl TryFrom<f64> for Degrees {
    type Error = NotFinite;

    fn try_from(degrees: f64) -> Result<Self, NotFinite> {
        Self::new(degrees).ok_or(NotFinite)
    }
}

impl From<Degrees> for f64 {
    fn from(degrees: Degrees) -> Self {
        degrees.0
    }
}

/// A value that is not finite, refused where an angle or a signed distance is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotFinite;

impl fmt::Display for NotFinite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the value is not finite")
    }
}

impl std::error::Error for NotFinite {}
