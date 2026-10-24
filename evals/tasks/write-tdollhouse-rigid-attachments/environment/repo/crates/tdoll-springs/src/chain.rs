//! One simulated chain: its joints root to tip and the colliders it keeps out of.

use std::ops::{AddAssign, Range};

use glam::{DAffine3, DVec3};
use tdoll_vocab::{BoneName, Length};

use crate::collider::{Collider, ColliderId};
use crate::contact::Friction;
use crate::joint::{Feel, Joint, Met};
use crate::limit::Limit;
use crate::pose::{scale_of, zero_approx};
use crate::solver::{Bone, bone_named};
use crate::{Error, TRAVEL};

/// A chain as the character file declares it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChainSpec {
    /// Its bones, root to tip.
    pub bones: Vec<BoneName>,
    /// How far the last bone's tail carries on past it.
    pub tail: Length,
    /// Its feel.
    pub feel: Feel,
    /// The joint boxes holding its bones, by bone; the last naming a bone holds it.
    pub limits: Vec<Limit>,
    /// The solver's colliders its bones keep out of.
    pub colliders: Vec<ColliderId>,
}

/// A chain placed for the solver.
#[derive(Clone, Debug)]
pub(crate) struct Chain {
    /// Its joints, root to tip, in the solver's joints.
    pub(crate) joints: Range<usize>,
    /// The solver's colliders it keeps out of.
    pub(crate) colliders: Vec<ColliderId>,
    /// The colliders within its reach this frame.
    pub(crate) touching: Vec<ColliderId>,
    /// Where the chain hangs from this frame.
    pub(crate) root: DVec3,
    /// How far from the root its tails can be before the next frame.
    pub(crate) reach: f64,
    /// How far any tail can be from its bone's ends before the next frame.
    pub(crate) span: f64,
    /// The scale of the skeleton where the chain hangs.
    pub(crate) scale: f64,
}

/// The joints of the chain `spec` declares in `skeleton`, stepped `rate` times a second; a bone with no length,
/// which nothing can point, has none.
///
/// # Errors
///
/// [`Error::MissingBone`] when the chain names a bone the skeleton lacks, and [`Error::InvalidFrame`] for a joint box
/// on one of its bones whose frame cannot be normalised.
pub(crate) fn joints_of(
    spec: &ChainSpec,
    skeleton: &[Bone],
    rate: f64,
) -> Result<Vec<Joint>, Error> {
    let find = |name: &BoneName| {
        bone_named(skeleton, name).ok_or_else(|| Error::MissingBone {
            chain: spec.bones.first().unwrap_or(name).clone(),
            bone: name.clone(),
        })
    };
    let indices = spec.bones.iter().map(find).collect::<Result<Vec<_>, _>>()?;
    #[expect(clippy::cast_precision_loss, reason = "a chain has a handful of bones")]
    let along = |index: usize| index as f64 / (spec.bones.len().saturating_sub(1).max(1)) as f64;

    let mut joints = Vec::new();
    for (index, (bone, name)) in indices.iter().zip(&spec.bones).enumerate() {
        let Some(own) = skeleton.get(bone.index()) else {
            continue;
        };
        let next = indices
            .get(index + 1)
            .and_then(|child| skeleton.get(child.index()));
        let axis = match next {
            Some(child) => child.rest.translation,
            None => {
                (own.rest.matrix3.inverse() * own.rest.translation).normalize_or_zero()
                    * spec.tail.get()
            }
        };
        if zero_approx(axis) {
            continue;
        }
        let limit = spec
            .limits
            .iter()
            .rev()
            .find(|limit| &limit.bone == name)
            .map(|limit| {
                limit.joint().ok_or_else(|| Error::InvalidFrame {
                    chain: spec.bones.first().unwrap_or(name).clone(),
                    bone: name.clone(),
                })
            })
            .transpose()?;
        let mut joint = Joint::new(*bone, own, axis, limit);
        joint.feel(&spec.feel, along(index), rate);
        joints.push(joint);
    }
    Ok(joints)
}

/// What meeting plain furniture took over a frame's steps, counted in bone steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contacts {
    /// Steps that started again from the present pose.
    pub recovered: usize,
    /// Steps that began inside furniture, without starting again.
    pub caught: usize,
    /// Steps the escape cleared.
    pub escaped: usize,
    /// Steps no direction cleared.
    pub unresolved: usize,
    /// Steps whose steering reached its weight cap.
    pub capped: usize,
    /// Steps static friction held at every loaded contact.
    pub stuck: usize,
    /// Steps a loaded contact let slide.
    pub slid: usize,
}

impl AddAssign<Met> for Contacts {
    /// One bone step's meeting with furniture counted.
    fn add_assign(&mut self, met: Met) {
        self.recovered += usize::from(met.recovered);
        self.caught += usize::from(met.caught);
        self.escaped += usize::from(met.escaped);
        self.capped += usize::from(met.capped);
        self.unresolved += usize::from(met.unresolved);
        match met.friction {
            Friction::None => {}
            Friction::Stuck => self.stuck += 1,
            Friction::Slid => self.slid += 1,
        }
    }
}

impl Chain {
    /// The chain's root, reach and scale this frame, and the colliders within its reach of the root, with room for a
    /// frame's travel; its bones' steps test only those.
    pub(crate) fn sense(&mut self, joints: &[Joint], colliders: &[Collider]) {
        let Some(first) = joints
            .get(self.joints.start)
            .filter(|_| !self.joints.is_empty())
        else {
            return;
        };
        let parent = parent_world(joints, first);
        self.root = first.origin_now(parent);
        self.scale = scale_of(parent.unwrap_or(first.parent_now).matrix3).x;
        self.reach = TRAVEL;
        self.span = 0.0;
        for joint in joints.get(self.joints.clone()).unwrap_or_default() {
            self.reach += (joint.length + joint.radius) * self.scale;
            self.span = self.span.max((joint.length + joint.radius) * self.scale);
        }
        self.span += TRAVEL;

        self.touching.clear();
        for id in &self.colliders {
            if let Some(collider) = colliders.get(id.index())
                && collider.now(self.root) <= self.reach + collider.radius
            {
                self.touching.push(*id);
            }
        }
    }
}

/// The world placement of `joint`'s simulated parent as its last step left it, or none when the clip places it: no
/// simulated parent, or one that has not stepped since it was made or reset, whose placement is not this pose's.
pub(crate) fn parent_world(joints: &[Joint], joint: &Joint) -> Option<DAffine3> {
    joint
        .parent
        .and_then(|parent| joints.get(parent.index()))
        .filter(|parent| parent.started)
        .map(|parent| parent.world)
}

#[cfg(test)]
mod tests {
    use glam::{DAffine3, DQuat, DVec3};
    use tdoll_vocab::{BoneName, Length};

    use super::Chain;
    use crate::collider::{Collider, ColliderId, ColliderShape, ColliderSpec};
    use crate::joint::Joint;
    use crate::solver::{Bone, BoneIndex};

    /// A bone `length` long with hit radius `radius`, its parent placed by `parent`.
    fn joint(length: f64, radius: f64, parent: DAffine3) -> Joint {
        let bone = Bone {
            name: None,
            parent: None,
            rest: DAffine3::IDENTITY,
        };
        let mut joint = Joint::new(BoneIndex::new(0), &bone, DVec3::Y * length, None);
        joint.radius = radius;
        joint.parent_now = parent;
        joint
    }

    /// A ball of `radius` centred at `centre`, placed this frame.
    fn ball(centre: DVec3, radius: f64) -> Collider {
        let spec = ColliderSpec::new(
            BoneName::new("bone").unwrap(),
            DQuat::IDENTITY,
            centre,
            ColliderShape::Sphere {
                radius: Length::new(radius).unwrap(),
            },
        );
        let mut collider = Collider::new(BoneIndex::new(0), &spec);
        collider.capture(DAffine3::IDENTITY);
        collider
    }

    /// Bones 1 m and 0.5 m long with hit radii 0.1 m and 0.2 m at scale 2: the chain reaches the travel allowance and
    /// both scaled bones past its root, 3.85 m, and any tail reaches the allowance and the longer scaled bone past its
    /// bone, 2.45 m. A ball whose near surface lies inside that reach is touching, though its centre lies beyond.
    #[test]
    fn a_scaled_chain_reaches_its_bones_and_radii_and_touches_a_ball_whose_surface_is_within_it() {
        let root = DVec3::new(1.0, 2.0, 3.0);
        let parent =
            DAffine3::from_scale_rotation_translation(DVec3::splat(2.0), DQuat::IDENTITY, root);
        let joints = [joint(1.0, 0.1, parent), joint(0.5, 0.2, parent)];
        let colliders = [
            ball(root + DVec3::X * 3.95, 0.2),
            ball(root + DVec3::X * 4.1, 0.2),
        ];
        let mut chain = Chain {
            joints: 0..2,
            colliders: vec![ColliderId::new(0), ColliderId::new(1)],
            touching: Vec::new(),
            root: DVec3::ZERO,
            reach: 0.0,
            span: 0.0,
            scale: 1.0,
        };
        chain.sense(&joints, &colliders);
        assert!(chain.root.distance(root) < 1e-12);
        assert!((chain.reach - 3.85).abs() < 1e-12, "{}", chain.reach);
        assert!((chain.span - 2.45).abs() < 1e-12, "{}", chain.span);
        assert_eq!(chain.touching, [ColliderId::new(0)]);
    }
}
