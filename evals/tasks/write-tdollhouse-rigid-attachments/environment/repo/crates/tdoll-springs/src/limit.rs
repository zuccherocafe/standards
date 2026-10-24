//! A PMX joint's rotation box as Bullet's generic six-degree joint reads it, and the cushion easing a bone into it.
//!
//! Each chain bone that a PMX joint holds to its parent's rigid body keeps within that joint's rotation box, applied to
//! the simulated state: the tail is where the limited bone points, and its velocity carries on from there. A box is
//! read as Bullet's generic six-degree joint, which MMD solves joints with, reads it: X, then Y, then Z angles of the
//! relative frame from the first body's joint frame to the second's, which Bullet takes from the frame's transpose, so
//! they are the angles of the inverse turn; for a joint that turns the bone from its parent's body, the box limits the
//! angles of the inverse of the bone's turn from rest about the joint's axes, and for one `reversed`, naming the two
//! rigid bodies the other way round, the angles of the bone's turn itself. A PMX axis whose minimum exceeds its maximum
//! is free, a locked axis away from rest holds its value, and a range that leaves out rest keeps the bone inside it.
//! Boxes in one frame, as the chest's two joints are, merge to their intersection before any easing, so a doubled joint
//! slows the bone once. A bone held by joints in different frames, whose boxes could conflict, keeps its first joint's
//! box; each box always holds, so a bone never sticks outside its box.
//!
//! A spring group's `cushion` is the share of each side of a box, next to the limit, where the bone slows into it
//! rather than moving freely: 0.3 when left out, 0 stops at the limit, 1 slows from rest. The bone never passes the
//! limit, except where furniture forces it: a limited bone that would lie in furniture is turned back out of it, since
//! a PMX box describes how stiff the fabric is, not a wall. Where the box takes back a collider's push, the colliders
//! and the box take turns, up to six a step, each push tried again square to what the box took back, both taken square
//! to the bone since only a turn moves its tail, while that takes the tail further out; a turn inside the box that
//! clears the colliders is kept, and otherwise the box wins as the first turn left it, and the tail stays in the
//! collider.

use glam::{DQuat, DVec3};
use tdoll_vocab::{BoneName, Radians};

use crate::pose::{basis, equal_approx, euler_xyz, from_euler_xyz, rotation_of, sign_of};

/// One axis of a joint's box: free, or held between its least and greatest angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisLimit {
    /// The bone turns freely about the axis.
    Free,
    /// The bone's angle about the axis is held from `min` to `max`.
    Held {
        /// The least angle.
        min: Radians,
        /// The greatest angle.
        max: Radians,
    },
}

/// A joint's box as the solver holds it: angles about the joint's X, then Y, then Z axes, each free or held, about a
/// frame of unit length.
///
/// Bullet takes the angles of the relative frame's transpose, which is the inverse turn, so the angles of a bone
/// turned by a joint from its parent's body are those of the inverse turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointBox {
    frame: DQuat,
    reversed: bool,
    axes: [AxisLimit; 3],
}

impl JointBox {
    /// The box in the joint's `frame`, given in the bone's rest axes and normalised here, `reversed` when the joint
    /// names its bodies the other way round and so measures the bone's turn itself, with `axes` about X, Y and Z;
    /// none for a frame no length can normalise, a zero or non-finite quaternion.
    #[must_use]
    pub fn new(frame: DQuat, reversed: bool, axes: [AxisLimit; 3]) -> Option<Self> {
        (frame.is_finite() && frame.length_recip().is_finite()).then(|| Self {
            frame: frame.normalize(),
            reversed,
            axes,
        })
    }

    /// The joint's angles, in radians, for a bone's turn from rest.
    #[must_use]
    pub(crate) fn angles(&self, bone_turn: DQuat) -> DVec3 {
        let mut relative = self.frame * bone_turn * self.frame.inverse();
        if !self.reversed {
            relative = relative.inverse();
        }
        euler_xyz(basis(relative))
    }

    /// The bone's turn from rest that gives the joint `angles`, in radians.
    #[must_use]
    pub(crate) fn turn_of(&self, angles: DVec3) -> DQuat {
        let mut relative = rotation_of(from_euler_xyz(angles));
        if !self.reversed {
            relative = relative.inverse();
        }
        self.frame.inverse() * relative * self.frame
    }

    /// The bone's turn held within the box, eased into by `cushion`, the share of each range next to its limit.
    #[must_use]
    pub fn hold(&self, bone_turn: DQuat, cushion: f64) -> DQuat {
        let now = self.angles(bone_turn);
        let mut kept = now;
        for (axis, limit) in self.axes.iter().enumerate() {
            if let AxisLimit::Held { min, max } = *limit {
                kept[axis] = ease_angle(now[axis], min.get(), max.get(), cushion);
            }
        }
        if equal_approx(kept, now) {
            bone_turn
        } else {
            self.turn_of(kept)
        }
    }

    /// How far the turn lies outside the box, or zero inside it, and for a turn that is not finite.
    #[must_use]
    pub fn excess(&self, bone_turn: DQuat) -> Radians {
        let now = self.angles(bone_turn);
        let mut worst = 0.0_f64;
        for (axis, limit) in self.axes.iter().enumerate() {
            if let AxisLimit::Held { min, max } = *limit {
                worst = worst.max((now[axis] - max.get()).max(min.get() - now[axis]));
            }
        }
        // The bounds are finite and an angle is finite or not a number, which `max` passes over, so this is finite.
        Radians::new(worst).unwrap_or(Radians::ZERO)
    }
}

/// A joint's box on a named bone, as the source declares it: its frame as given, which [`Limit::joint`] normalises.
#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    /// The bone it holds.
    pub bone: BoneName,
    /// The joint's frame in the bone's rest axes.
    pub frame: DQuat,
    /// Whether the joint names its bodies the other way round, so measures the bone's turn itself.
    pub reversed: bool,
    /// Each axis about the joint's X, Y and Z.
    pub axes: [AxisLimit; 3],
}

impl Limit {
    /// The box as the solver holds it; none for a frame that cannot be normalised.
    #[must_use]
    pub fn joint(&self) -> Option<JointBox> {
        JointBox::new(self.frame, self.reversed, self.axes)
    }
}

/// `angle` free until `cushion` of the way from its limit, then progressively slower, so it never passes the limit.
///
/// A cushion of zero stops at the limit. A range that does not hold rest, including a locked axis away from it, is
/// held to exactly as the joint gives it, and a side whose limit is rest itself is held there. Radians throughout.
#[must_use]
pub(crate) fn ease_angle(angle: f64, low: f64, high: f64, cushion: f64) -> f64 {
    if low > 0.0 || high < 0.0 {
        return within(angle, low, high);
    }
    let limit = if angle > 0.0 { high } else { low };
    if limit.abs() < 1e-6 {
        return within(angle, low, high);
    }
    let eased_span = cushion.clamp(0.0, 1.0) * limit.abs();
    let free = limit.abs() - eased_span;
    let reach = angle.abs();
    if reach <= free {
        return angle;
    }
    if eased_span < 1e-9 {
        return limit;
    }
    let eased = sign_of(angle) * (free + eased_span * ((reach - free) / eased_span).tanh());
    within(eased, low, high)
}

/// `value` raised to `low` or else lowered to `high`, the low bound tested first, as the engine clamps.
fn within(value: f64, low: f64, high: f64) -> f64 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use glam::{DMat3, DQuat, DVec3};
    use tdoll_vocab::Radians;

    use super::*;
    use crate::pose::basis;

    /// A small deterministic source of made-up turns.
    struct Turns(u64);

    impl Turns {
        fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            #[expect(
                clippy::cast_precision_loss,
                reason = "the top 53 bits fit an f64 exactly"
            )]
            let share = (self.0 >> 11) as f64 / (1_u64 << 53) as f64;
            share
        }

        fn turn(&mut self, most: f64) -> DQuat {
            let axis = DVec3::new(
                self.unit() * 2.0 - 1.0,
                self.unit() * 2.0 - 1.0,
                self.unit() * 2.0 - 1.0,
            );
            DQuat::from_axis_angle(axis.normalize(), self.unit() * most)
        }
    }

    /// How far outside `limit` Bullet's generic joint puts a bone turned by `turn`: the relative frame of body A's joint
    /// frame to B's is the bone's turn in the joint's axes when the bone is B, its inverse when the joint is reversed, and
    /// Bullet reads X, Y, Z angles from its elements (index % 3, index / 3), so from its transpose.
    fn bullet_excess(limit: &Limit, turn: DQuat) -> f64 {
        let bone_frame = limit.frame * turn * limit.frame.inverse();
        let relative: DMat3 = basis(if limit.reversed {
            bone_frame.inverse()
        } else {
            bone_frame
        });
        let element = |index: usize| relative.col(index / 3)[index % 3];
        let angles = DVec3::new(
            (-element(5)).atan2(element(8)),
            element(2).clamp(-1.0, 1.0).asin(),
            (-element(1)).atan2(element(0)),
        );
        let mut worst = f64::NEG_INFINITY;
        for (axis, bounds) in limit.axes.iter().enumerate() {
            if let AxisLimit::Held { min, max } = *bounds {
                worst = worst.max((angles[axis] - max.get()).max(min.get() - angles[axis]));
            }
        }
        worst
    }

    /// A box on a bone holding each axis from `low` to `high` unless `free`.
    fn boxed(frame: DQuat, reversed: bool, low: DVec3, high: DVec3, free: [bool; 3]) -> Limit {
        let mut axes = [AxisLimit::Free; 3];
        let bounds = low.to_array().into_iter().zip(high.to_array()).zip(free);
        for (axis, ((min, max), free)) in axes.iter_mut().zip(bounds) {
            if !free {
                *axis = AxisLimit::Held {
                    min: Radians::new(min).unwrap(),
                    max: Radians::new(max).unwrap(),
                };
            }
        }
        Limit {
            bone: BoneName::new("bone").unwrap(),
            frame,
            reversed,
            axes,
        }
    }

    #[test]
    fn a_turn_is_outside_the_box_exactly_when_bullet_reads_it_outside_and_a_held_turn_is_inside() {
        let mut turns = Turns(7);
        for reversed in [false, true] {
            for frame in [
                DQuat::IDENTITY,
                DQuat::from_euler(glam::EulerRot::YXZ, 0.5, 0.3, -0.2),
            ] {
                let declared = boxed(
                    frame,
                    reversed,
                    DVec3::new(-0.7, 0.0, -0.3),
                    DVec3::new(0.7, 0.0, 0.5),
                    [false; 3],
                );
                let limit = declared.joint().unwrap();
                let mut wanted = vec![DQuat::from_mat3(&DMat3::from_euler(
                    glam::EulerRot::XYZ,
                    0.7,
                    0.0,
                    0.7,
                ))];
                wanted.extend((0..200).map(|_| turns.turn(1.2)));
                for turn in wanted {
                    let outside = bullet_excess(&declared, turn);
                    if outside.abs() > 1e-3 {
                        assert_eq!(
                            outside > 0.0,
                            limit.excess(turn).get() > 1e-6,
                            "reversed {reversed}, {turn}"
                        );
                    }
                    assert!(
                        bullet_excess(&declared, limit.hold(turn, 0.0)) <= 1e-5,
                        "reversed {reversed}, {turn}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_free_axis_never_counts_and_inside_reads_no_excess() {
        let limit = boxed(
            DQuat::IDENTITY,
            true,
            DVec3::splat(-0.1),
            DVec3::splat(0.1),
            [true, false, false],
        )
        .joint()
        .unwrap();
        let about_x = DQuat::from_rotation_x(1.0);
        assert!(limit.excess(about_x).get().abs() < 1e-12 && limit.hold(about_x, 0.3) == about_x);
        let about_z = DQuat::from_rotation_z(0.3);
        assert!((limit.excess(about_z).get() - 0.2).abs() < 1e-9);
        assert!((limit.angles(limit.hold(about_z, 0.0)).z - 0.1).abs() < 1e-9);
        assert!((limit.excess(DQuat::from_rotation_z(0.05)).get()).abs() < 1e-12);
    }

    #[test]
    fn easing_is_free_short_of_its_soft_zone_continuous_monotonic_and_never_past_the_limit() {
        for cushion in [0.0, 0.3, 0.7, 1.0] {
            let mut previous = ease_angle(-2.0, -0.4, 0.6, cushion);
            for step in 1..=4000 {
                let angle = -2.0 + f64::from(step) * 0.001;
                let eased = ease_angle(angle, -0.4, 0.6, cushion);
                let free = angle <= 0.6 * (1.0 - cushion) && angle >= -0.4 * (1.0 - cushion);
                assert!(
                    eased >= previous && eased - previous <= 0.001 + 1e-9,
                    "cushion {cushion} at {angle}"
                );
                assert!(
                    (-0.4..=0.6).contains(&eased),
                    "cushion {cushion} leaves {angle} at {eased}"
                );
                assert!(
                    !free || (eased - angle).abs() < 1e-9,
                    "cushion {cushion} moves the free {angle}"
                );
                previous = eased;
            }
        }
    }

    #[test]
    fn a_side_whose_limit_is_rest_holds_at_rest_and_a_locked_axis_away_from_rest_holds_its_value() {
        assert!(ease_angle(0.2, -0.3, 0.0, 0.3).abs() < 1e-12);
        assert!((ease_angle(0.0, 0.4, 0.4, 0.3) - 0.4).abs() < 1e-9);
        assert!(
            (ease_angle(-1.0, 0.2, 0.5, 0.3) - 0.2).abs() < 1e-12,
            "a range beside rest holds to its edge"
        );
        assert!(
            (ease_angle(0.3, 0.5, 0.2, 0.3) - 0.5).abs() < 1e-12,
            "inverted bounds hold to the low one"
        );
    }

    #[test]
    fn a_frame_no_length_can_normalise_makes_no_box_and_any_other_is_normalised_as_before() {
        let held = [AxisLimit::Held {
            min: Radians::new(-0.2).unwrap(),
            max: Radians::new(0.2).unwrap(),
        }; 3];
        for frame in [
            DQuat::from_xyzw(0.0, 0.0, 0.0, 0.0),
            DQuat::from_xyzw(f64::NAN, 0.0, 0.0, 1.0),
            DQuat::from_xyzw(f64::INFINITY, 0.0, 0.0, 1.0),
        ] {
            assert_eq!(JointBox::new(frame, false, held), None, "{frame}");
        }
        for frame in [
            DQuat::from_xyzw(0.1, -0.7, 0.2, 0.6),
            DQuat::from_xyzw(0.0, 0.0, 0.0, 3.0),
            DQuat::IDENTITY,
        ] {
            let made = JointBox::new(frame, true, held).unwrap();
            assert_eq!(
                made.frame.to_array().map(f64::to_bits),
                frame.normalize().to_array().map(f64::to_bits)
            );
        }
    }

    #[test]
    fn easing_a_narrow_cushion_never_passes_the_limit_where_tanh_rounds_to_one() {
        // Angle 3.0327, limits -3.0448 and 2.2404, cushion 0.0072: the scaled overshoot's tanh rounds to exactly one,
        // and the eased sum rounds to one ulp above the high limit unless the result is held within the limits.
        let angle = f64::from_bits(0x4008_4310_aed6_7229);
        let low = f64::from_bits(0xc008_5bcb_37bf_a1d4);
        let high = f64::from_bits(0x4001_ec5a_97ea_8b2b);
        let cushion = f64::from_bits(0x3f7d_a69f_bd82_3380);
        let eased = ease_angle(angle, low, high, cushion);
        assert!(
            (low..=high).contains(&eased),
            "{angle} leaves {eased} past {high}"
        );
    }

    #[test]
    fn a_range_from_rest_to_one_side_cushions_towards_its_far_limit_and_one_away_from_rest_does_not()
     {
        let up = ease_angle(0.3, 0.0, 0.6, 1.0);
        assert!(up > 0.0 && up < 0.3, "{up}");
        let down = ease_angle(-0.3, -0.6, 0.0, 1.0);
        assert!(down < 0.0 && down > -0.3, "{down}");
        assert!((ease_angle(-0.4, -0.6, -0.2, 1.0) + 0.4).abs() < 1e-12);
    }
}
