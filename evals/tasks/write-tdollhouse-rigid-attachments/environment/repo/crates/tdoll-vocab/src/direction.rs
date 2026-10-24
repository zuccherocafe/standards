//! Directions in glTF's Y-up space: unit directions, and ways along the floor that need not be unit.

use std::fmt;

use glam::{DQuat, DVec3};

use crate::Radians;

/// A direction: finite, unit within `UNIT_TOLERANCE`, kept exactly as given (never renormalised).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "DVec3", into = "DVec3"))]
pub struct Direction(DVec3);

impl Direction {
    /// How far a direction's squared length may stray from one.
    pub const UNIT_TOLERANCE: f64 = 1e-6;

    /// `vector` unchanged as a direction; none unless finite and unit within the tolerance.
    #[must_use]
    pub fn new(vector: DVec3) -> Option<Self> {
        let unit = (vector.length_squared() - 1.0).abs() <= Self::UNIT_TOLERANCE;
        (vector.is_finite() && unit).then_some(Self(vector))
    }

    /// `vector` scaled to unit length, the bits `normalize_or_zero` gives; none where it gives no direction or one not
    /// unit within the tolerance, as for a vector shorter than about 1e-158, whose subnormal squared length loses the
    /// precision the scaling needs.
    #[must_use]
    pub fn normalise(vector: DVec3) -> Option<Self> {
        vector.try_normalize().and_then(Self::new)
    }

    /// The unit vector.
    #[must_use]
    pub const fn get(self) -> DVec3 {
        self.0
    }

    /// The turn of `angle` about this axis: the only axis-angle turn, so no zero axis reaches glam.
    #[must_use]
    pub fn about(self, angle: Radians) -> DQuat {
        DQuat::from_axis_angle(self.0, angle.get())
    }
}

impl TryFrom<DVec3> for Direction {
    type Error = NotUnit;

    fn try_from(vector: DVec3) -> Result<Self, NotUnit> {
        Self::new(vector).ok_or(NotUnit)
    }
}

impl From<Direction> for DVec3 {
    fn from(direction: Direction) -> Self {
        direction.0
    }
}

/// A vector that is not finite or not unit within the tolerance, refused as a direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotUnit;

impl fmt::Display for NotUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a direction is a finite unit vector")
    }
}

impl std::error::Error for NotUnit {}

/// A way along the floor in glTF's Y-up space, such as where a character faces: finite, rising or falling no more
/// than 1e-4 in `y` and with a squared length of at least 1e-4, kept exactly as given. Unit length is not part of it:
/// a facing measured from a skeleton is unit, while a way read from elsewhere need only lie along the floor and be
/// long enough to normalise.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "DVec3", into = "DVec3"))]
pub struct Horizontal(DVec3);

impl Horizontal {
    /// How far the way may rise or fall, as its `y` component.
    const RISE_TOLERANCE: f64 = 1e-4;
    /// The least squared length the way has.
    const LEAST_SQUARED: f64 = 1e-4;
    /// Towards +X.
    pub const X: Self = Self(DVec3::X);
    /// Towards -X.
    pub const NEG_X: Self = Self(DVec3::NEG_X);
    /// Towards +Z.
    pub const Z: Self = Self(DVec3::Z);
    /// Towards -Z.
    pub const NEG_Z: Self = Self(DVec3::NEG_Z);

    /// `vector` unchanged as a way along the floor; none unless finite, rising no more than the tolerance and long
    /// enough.
    #[must_use]
    pub fn new(vector: DVec3) -> Option<Self> {
        let flat = vector.y.abs() <= Self::RISE_TOLERANCE;
        let long = vector.length_squared() >= Self::LEAST_SQUARED;
        (vector.is_finite() && flat && long).then_some(Self(vector))
    }

    /// The vector.
    #[must_use]
    pub const fn get(self) -> DVec3 {
        self.0
    }
}

impl TryFrom<DVec3> for Horizontal {
    type Error = NotHorizontal;

    fn try_from(vector: DVec3) -> Result<Self, NotHorizontal> {
        Self::new(vector).ok_or(NotHorizontal)
    }
}

impl From<Horizontal> for DVec3 {
    fn from(horizontal: Horizontal) -> Self {
        horizontal.0
    }
}

/// A vector that is not finite, rises off the floor or is too short to be a way along it, refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotHorizontal;

impl fmt::Display for NotHorizontal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a way along the floor is finite, level within 1e-4 and not too short")
    }
}

impl std::error::Error for NotHorizontal {}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::{Direction, Horizontal, NotHorizontal};
    use crate::Radians;

    #[test]
    fn a_positive_quarter_turn_about_z_takes_x_to_y_and_keeps_z() {
        let turn = Direction::new(DVec3::Z)
            .unwrap()
            .about(Radians::new(std::f64::consts::FRAC_PI_2).unwrap());
        assert!((turn * DVec3::X).distance(DVec3::Y) < 1e-15);
        assert!((turn * DVec3::Z).distance(DVec3::Z) < 1e-15);
    }

    #[test]
    fn a_way_along_the_floor_rises_no_more_than_its_tolerance_and_is_long_enough() {
        let kept =
            |x: f64, y: f64, z: f64| Horizontal::new(DVec3::new(x, y, z)).map(Horizontal::get);
        assert_eq!(kept(0.0, 1e-4, 1.0), Some(DVec3::new(0.0, 1e-4, 1.0)));
        assert_eq!(kept(0.0, -1e-4, 1.0), Some(DVec3::new(0.0, -1e-4, 1.0)));
        assert_eq!(kept(0.0, 1.0001e-4, 1.0), None);
        assert_eq!(kept(0.0, 0.0, 0.01), Some(DVec3::new(0.0, 0.0, 0.01)));
        assert_eq!(kept(0.0, 0.0, 0.0099), None);
        assert_eq!(kept(3.0, 0.0, 4.0), Some(DVec3::new(3.0, 0.0, 4.0)));
        assert_eq!(kept(f64::NAN, 0.0, 1.0), None);
        assert_eq!(kept(f64::INFINITY, 0.0, 0.0), None);
        assert_eq!(Horizontal::try_from(DVec3::Y), Err(NotHorizontal));
        for axis in [
            Horizontal::X,
            Horizontal::NEG_X,
            Horizontal::Z,
            Horizontal::NEG_Z,
        ] {
            assert_eq!(Horizontal::new(axis.get()), Some(axis));
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_way_and_a_direction_are_read_as_their_components_through_their_checks() {
        use serde::Deserialize as _;
        use serde::de::value::{Error, SeqDeserializer};
        let seq = |components: [f64; 3]| SeqDeserializer::<_, Error>::new(components.into_iter());
        assert_eq!(
            Horizontal::deserialize(seq([0.0, 0.0, 1.0])).ok(),
            Some(Horizontal::Z)
        );
        assert!(Horizontal::deserialize(seq([0.0, 1.0, 0.0])).is_err());
        let direction = Direction::deserialize(seq([0.0, 1.0, 0.0])).ok();
        assert_eq!(direction.map(Direction::get), Some(DVec3::Y));
        assert!(Direction::deserialize(seq([0.0, 2.0, 0.0])).is_err());
    }
}
