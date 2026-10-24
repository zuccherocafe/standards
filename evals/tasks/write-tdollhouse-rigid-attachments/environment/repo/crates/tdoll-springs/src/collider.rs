//! A collider on a bone: a sphere, or a capsule along its local Y, placed where the bone's pose puts it at the last two
//! frames and, for each step, between them.

use glam::{DAffine3, DQuat, DVec3};
use tdoll_vocab::{BoneName, Length};

use crate::PUSHES;
use crate::joint::{Swing, aside};
use crate::pose::{closest_on_segment, scale_of, zero_approx};
use crate::solver::BoneIndex;

/// A collider's shape as the character file gives it, in metres before the skeleton's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColliderShape {
    /// A ball.
    Sphere {
        /// Its radius.
        radius: Length,
    },
    /// A capsule along the collider's Y, its middle at the collider's origin.
    Capsule {
        /// Its radius.
        radius: Length,
        /// Half its whole length, from the tip of one cap to the tip of the other. Below the radius it is a ball.
        half_length: Length,
    },
}

impl ColliderShape {
    /// The shape's radius.
    #[must_use]
    pub fn radius(self) -> Length {
        match self {
            Self::Sphere { radius } | Self::Capsule { radius, .. } => radius,
        }
    }

    /// Half the length of the segment its radius sweeps: none for a ball.
    fn half_segment(self) -> f64 {
        match self {
            Self::Sphere { .. } => 0.0,
            Self::Capsule {
                radius,
                half_length,
            } => (half_length.get() - radius.get()).max(0.0),
        }
    }
}

/// A collider as the character file declares it: on which bone, where on it and of what shape.
#[derive(Clone, Debug, PartialEq)]
pub struct ColliderSpec {
    bone: BoneName,
    rotation: DQuat,
    position: DVec3,
    shape: ColliderShape,
}

impl ColliderSpec {
    /// The collider of `shape` on `bone`, turned by `rotation` and centred at `position` in the bone's space. The
    /// rotation is kept as given, so a record of it reads back the same, and is normalised where the solver places it.
    #[must_use]
    pub fn new(bone: BoneName, rotation: DQuat, position: DVec3, shape: ColliderShape) -> Self {
        Self {
            bone,
            rotation,
            position,
            shape,
        }
    }

    /// The bone it rides on.
    #[must_use]
    pub fn bone(&self) -> &BoneName {
        &self.bone
    }

    /// How it is turned in the bone's space, as given.
    #[must_use]
    pub fn rotation(&self) -> DQuat {
        self.rotation
    }

    /// Where its middle sits in the bone's space.
    #[must_use]
    pub fn position(&self) -> DVec3 {
        self.position
    }

    /// Its shape.
    #[must_use]
    pub fn shape(&self) -> ColliderShape {
        self.shape
    }
}

/// A collider of one solver, by the order it was added; only that solver issues it. It proves which kind of index it
/// is, not which solver issued it: one solver's id given to another names whatever that one added in its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColliderId(usize);

impl ColliderId {
    /// The collider at `index` in its solver's order.
    pub(crate) fn new(index: usize) -> Self {
        Self(index)
    }

    /// Its place in its solver's order.
    #[must_use]
    pub fn index(self) -> usize {
        self.0
    }
}

/// A collider placed for the solver.
#[derive(Clone, Debug)]
pub(crate) struct Collider {
    /// The skeleton bone it rides on.
    pub(crate) bone: BoneIndex,
    offset: DAffine3,
    size: f64,
    half: f64,
    captured: bool,
    start_then: DVec3,
    end_then: DVec3,
    start_now: DVec3,
    end_now: DVec3,
    /// Its radius as scaled this frame.
    pub(crate) radius: f64,
    /// Its segment's ends this step.
    pub(crate) start: DVec3,
    /// See `start`.
    pub(crate) end: DVec3,
}

impl Collider {
    /// The collider `spec` rides on skeleton bone `bone`.
    #[must_use]
    pub(crate) fn new(bone: BoneIndex, spec: &ColliderSpec) -> Self {
        let radius = spec.shape.radius().get();
        Self {
            bone,
            offset: DAffine3::from_rotation_translation(spec.rotation.normalize(), spec.position),
            size: radius,
            half: spec.shape.half_segment(),
            captured: false,
            start_then: DVec3::ZERO,
            end_then: DVec3::ZERO,
            start_now: DVec3::ZERO,
            end_now: DVec3::ZERO,
            radius,
            start: DVec3::ZERO,
            end: DVec3::ZERO,
        }
    }

    /// Forgets last frame's placement, so the next capture takes the collider afresh.
    pub(crate) fn reset(&mut self) {
        self.captured = false;
    }

    /// Where the bone's placement in the world, `bone_world`, puts the collider's ends this frame, beside last frame's.
    pub(crate) fn capture(&mut self, bone_world: DAffine3) {
        let placed = bone_world * self.offset;
        self.start_then = self.start_now;
        self.end_then = self.end_now;
        self.start_now = placed.transform_point3(DVec3::Y * self.half);
        self.end_now = placed.transform_point3(DVec3::NEG_Y * self.half);
        self.radius = self.size * scale_of(placed.matrix3).x;
        if !self.captured {
            self.start_then = self.start_now;
            self.end_then = self.end_now;
            self.captured = true;
        }
    }

    /// The collider `at` of the way from last frame's placement to this frame's.
    pub(crate) fn place(&mut self, at: f64) {
        self.start = self.start_then.lerp(self.start_now, at);
        self.end = self.end_then.lerp(self.end_now, at);
    }

    /// How far `point` is from the collider's segment this step.
    #[must_use]
    pub(crate) fn distance(&self, point: DVec3) -> f64 {
        point.distance(closest_on_segment(point, self.start, self.end))
    }

    /// How far `point` is from where the bone's pose puts the collider's segment this frame.
    #[must_use]
    pub(crate) fn now(&self, point: DVec3) -> f64 {
        point.distance(closest_on_segment(point, self.start_now, self.end_now))
    }

    /// The tail turned about the swing's origin out of the collider by its hit radius, straight out or, where the
    /// joint's box took back a push the way `blocked` points, square to that. The surface is taken as flat where the
    /// tail leaves it, so a few turns settle it on a round one.
    #[must_use]
    pub(crate) fn push(&self, tail: DVec3, swing: &Swing, blocked: DVec3) -> DVec3 {
        let mut out = tail;
        let clearance = self.radius + swing.hit;
        for _ in 0..PUSHES {
            let away = out - closest_on_segment(out, self.start, self.end);
            if away.length() >= clearance - 1e-6 || zero_approx(away) {
                return out;
            }
            let mut normal = away.normalize_or_zero();
            if !zero_approx(blocked) {
                normal = aside(normal, blocked, out - swing.origin);
            }
            let along = away.dot(normal);
            let leave = (along * along - away.length_squared() + clearance * clearance)
                .max(0.0)
                .sqrt()
                - along;
            out = swing.clear_of(out, out + normal * leave, normal);
        }
        out
    }
}

/// The colliders a chain is touching: the solver's colliders, by id.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Touching<'a> {
    /// Every collider the solver has.
    pub(crate) all: &'a [Collider],
    /// The ones the chain is touching.
    pub(crate) ids: &'a [ColliderId],
}

impl<'a> Touching<'a> {
    /// The colliders the chain is touching.
    pub(crate) fn colliders(self) -> impl Iterator<Item = &'a Collider> {
        self.ids.iter().filter_map(|id| self.all.get(id.index()))
    }
}

#[cfg(test)]
mod tests {
    use glam::{DAffine3, DQuat, DVec3};
    use tdoll_vocab::{BoneName, Length};

    use super::{Collider, ColliderShape, ColliderSpec};
    use crate::joint::Swing;
    use crate::solver::{Bone, BoneIndex, Springs};

    fn length(metres: f64) -> Length {
        Length::new(metres).unwrap()
    }

    fn name(text: &str) -> BoneName {
        BoneName::new(text).unwrap()
    }

    fn ball(radius: f64) -> ColliderShape {
        ColliderShape::Sphere {
            radius: length(radius),
        }
    }

    /// The collider `shape` centred at `position` on a bone placed at `bone_world`, placed for this step.
    fn placed(shape: ColliderShape, position: DVec3, bone_world: DAffine3) -> Collider {
        let spec = ColliderSpec::new(name("bone"), DQuat::IDENTITY, position, shape);
        let mut collider = Collider::new(BoneIndex::new(0), &spec);
        collider.capture(bone_world);
        collider.place(1.0);
        collider
    }

    #[test]
    fn a_capsules_segment_is_its_length_less_both_caps_and_a_ball_has_none() {
        let capsule = ColliderShape::Capsule {
            radius: length(0.2),
            half_length: length(0.7),
        };
        let collider = placed(capsule, DVec3::new(1.0, 2.0, 3.0), DAffine3::IDENTITY);
        assert!(collider.start.distance(DVec3::new(1.0, 2.5, 3.0)) < 1e-12);
        assert!(collider.end.distance(DVec3::new(1.0, 1.5, 3.0)) < 1e-12);
        let short = ColliderShape::Capsule {
            radius: length(0.2),
            half_length: length(0.1),
        };
        for shape in [ball(0.2), short] {
            let collider = placed(shape, DVec3::new(1.0, 2.0, 3.0), DAffine3::IDENTITY);
            assert_eq!(collider.start, DVec3::new(1.0, 2.0, 3.0), "{shape:?}");
            assert_eq!(collider.end, DVec3::new(1.0, 2.0, 3.0), "{shape:?}");
        }
    }

    #[test]
    fn a_collider_on_a_bone_scaled_twice_is_twice_as_wide() {
        let collider = placed(
            ball(0.2),
            DVec3::ZERO,
            DAffine3::from_scale(DVec3::splat(2.0)),
        );
        assert!((collider.radius - 0.4).abs() < 1e-12);
    }

    #[test]
    fn a_collider_spec_keeps_its_offset_and_turn_as_given() {
        let turn = DQuat::from_xyzw(0.0, 0.0, 0.0, 3.0);
        let spec = ColliderSpec::new(name("bone"), turn, DVec3::new(1.0, 2.0, 3.0), ball(0.1));
        assert_eq!(spec.position(), DVec3::new(1.0, 2.0, 3.0));
        assert_eq!(spec.rotation(), turn);
    }

    #[test]
    fn colliders_are_numbered_in_the_order_they_are_added() {
        let bone = Bone {
            name: Some(name("bone")),
            parent: None,
            rest: DAffine3::IDENTITY,
        };
        let mut springs = Springs::new(vec![bone], crate::RATE);
        let spec =
            |x: f64| ColliderSpec::new(name("bone"), DQuat::IDENTITY, DVec3::X * x, ball(0.1));
        let first = springs.add_collider(&spec(1.0)).unwrap();
        let second = springs.add_collider(&spec(2.0)).unwrap();
        assert_eq!((first.index(), second.index()), (0, 1));
    }

    #[test]
    fn the_distance_to_a_collider_is_to_the_nearest_point_of_its_segment() {
        let sphere = placed(ball(0.1), DVec3::new(2.0, 0.0, 0.0), DAffine3::IDENTITY);
        assert!((sphere.now(DVec3::ZERO) - 2.0).abs() < 1e-12);
        let capsule = ColliderShape::Capsule {
            radius: length(0.1),
            half_length: length(0.6),
        };
        let capsule = placed(capsule, DVec3::new(5.0, 0.0, 0.0), DAffine3::IDENTITY);
        assert!(
            (capsule.now(DVec3::new(5.0, 2.0, 0.0)) - 1.5).abs() < 1e-12,
            "past an end"
        );
        assert!(
            (capsule.now(DVec3::new(6.0, 0.2, 0.0)) - 1.0).abs() < 1e-12,
            "beside it"
        );
    }

    /// A unit bone whose tail lies 0.2 m inside a 0.3 m ball, with a 0.1 m hit radius, is turned about its origin
    /// until it touches the ball's inflated surface; the push is taken as flat, which overshoots a round surface by
    /// under a millimetre here.
    #[test]
    fn a_pushed_tail_turns_out_to_the_inflated_surface_at_its_length() {
        for origin in [DVec3::ZERO, DVec3::new(3.0, 2.0, 1.0)] {
            for blocked in [DVec3::ZERO, DVec3::Z] {
                let centre = origin + DVec3::new(0.2, 1.0, 0.0);
                let collider = placed(ball(0.3), centre, DAffine3::IDENTITY);
                let tail = origin + DVec3::Y;
                let swing = Swing {
                    origin,
                    reach: 1.0,
                    hit: 0.1,
                    before: tail,
                };
                let out = collider.push(tail, &swing, blocked);
                let label = format!("from {origin}, blocked {blocked}");
                assert!((out.distance(origin) - 1.0).abs() < 1e-9, "{label}: {out}");
                let gap = out.distance(centre);
                assert!((0.4 - 1e-6..0.401).contains(&gap), "{label}: {gap}");
                assert!(
                    (out.z - origin.z).abs() < 1e-9,
                    "{label}: left its plane, {out}"
                );
            }
        }
    }
}
