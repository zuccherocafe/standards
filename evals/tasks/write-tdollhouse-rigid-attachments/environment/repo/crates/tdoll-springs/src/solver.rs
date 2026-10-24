//! The solver for one set of chains: each frame the clip's pose is taken, and the chains step as many whole steps as
//! have fallen due at a fixed rate, each with the clip interpolated to its own time.
//!
//! The chains step at a fixed 120 Hz whatever the display's rate. Each frame runs the steps that have fallen due in the
//! time the host advanced the skeleton's animation, which `GFBody` hands the solver each frame (the frame's delta when
//! no host does), so a clock that runs apart from the frames cannot shake the chains, each step with the clip's pose
//! and the colliders interpolated to its own time; it draws each bone one step behind the frame, between its last two
//! simulated rotations. A frame longer than four steps drops the rest, and a frame in which no time passed, as under a
//! paused clock, steps nothing and draws each bone as the last frame drew it. A chain looks only at the colliders and
//! furniture within its reach of its root, and each bone only at the furniture within a frame's travel of it.

use std::ops::Range;

use glam::{DAffine3, DQuat, DVec3};
use tdoll_vocab::{BoneName, Metres, Rate, Seconds};

use crate::chain::{Chain, ChainSpec, Contacts, joints_of, parent_world};
use crate::collider::{Collider, ColliderId, ColliderSpec, Touching};
use crate::contact::Scratch;
use crate::furniture::{Furniture, Measured, Near, Solid};
use crate::joint::{FrameStep, Joint, Step};
use crate::pose::{Bounds, Pose};
use crate::{CATCH_UP, Error, LINK_PASSES, SNAP, TRAVEL};

/// A skeleton bone as the host describes it: its name, its parent and its rest placement in the parent's space.
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// The bone's name, which chains and colliders name it by; none for a bone the host leaves unnamed, which nothing
    /// can name.
    pub name: Option<BoneName>,
    /// Its parent, by its place in the skeleton, or none for a root; a place the skeleton lacks is taken as none.
    pub parent: Option<usize>,
    /// Its rest placement relative to its parent.
    pub rest: DAffine3,
}

/// A bone of the skeleton a solver was given, by its place there; the solver issues it, and the host reads its place
/// back to find the bone in its own skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoneIndex(usize);

impl BoneIndex {
    /// The bone at `index`, a place the solver has checked its skeleton holds.
    pub(crate) fn new(index: usize) -> Self {
        Self(index)
    }

    /// Its place in the skeleton.
    #[must_use]
    pub fn index(self) -> usize {
        self.0
    }
}

/// A simulated bone, by its place in its solver's joints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JointIndex(usize);

impl JointIndex {
    /// The joint at `index` in its solver's order.
    pub(crate) fn new(index: usize) -> Self {
        Self(index)
    }

    /// Its place in its solver's order.
    pub(crate) fn index(self) -> usize {
        self.0
    }
}

/// The worst of one measure over the solver's bones: how much and at which bone. A measure that is not finite is never
/// the worst.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Worst {
    /// How much.
    pub metres: Metres,
    /// The bone it was measured at.
    pub bone: BoneIndex,
}

/// The skeleton's pose this frame, as the clip leaves it before the springs.
pub trait Poses {
    /// The bone's pose relative to its parent.
    fn local(&self, bone: BoneIndex) -> Pose;
    /// The bone's placement in the world, or the skeleton's own for none.
    fn world(&self, bone: Option<BoneIndex>) -> DAffine3;
}

/// What a frame needs once its pose is taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frame {
    /// No time passed: no pose is taken and nothing steps; the bones are drawn as the last frame drew them.
    Still,
    /// No step fell due; the bones are drawn between the last two steps.
    Draw,
    /// Steps fell due: gather the furniture in `area`, none when no chain has bones, then advance.
    Step {
        /// The region every chain's tails can reach before the next frame.
        area: Option<Bounds>,
    },
}

/// The spring chains of one part of a character.
///
/// A host makes it over its skeleton, adds each chain, and every frame captures the clip's pose, advances when a step
/// falls due and draws the bones' rotations back into its skeleton:
///
/// ```
/// use glam::{DAffine3, DQuat, DVec3};
/// use tdoll_springs::chain::ChainSpec;
/// use tdoll_springs::furniture::Furniture;
/// use tdoll_springs::joint::Feel;
/// use tdoll_springs::pose::Pose;
/// use tdoll_springs::{Bone, BoneIndex, Frame, Poses, Springs, Unmeasured};
/// use tdoll_vocab::{BoneName, Length, Seconds};
///
/// // A root and one simulated bone, the clip holding both at rest.
/// struct Skeleton(Vec<Bone>);
/// impl Poses for Skeleton {
///     fn local(&self, bone: BoneIndex) -> Pose {
///         let rest = self.0[bone.index()].rest;
///         Pose { translation: rest.translation, rotation: DQuat::IDENTITY, scale: DVec3::ONE }
///     }
///     fn world(&self, bone: Option<BoneIndex>) -> DAffine3 {
///         bone.map_or(DAffine3::IDENTITY, |bone| self.0[bone.index()].rest)
///     }
/// }
/// let bone = |name: &str, parent| Bone { name: BoneName::new(name).ok(), parent, rest: DAffine3::IDENTITY };
/// let skeleton = Skeleton(vec![bone("root", None), bone("tail", Some(0))]);
/// let mut springs = Springs::new(skeleton.0.clone(), tdoll_springs::RATE);
/// let length = |metres| Length::new(metres).expect("a length");
/// springs
///     .add_chain(&ChainSpec {
///         bones: vec![BoneName::new("tail").expect("named")],
///         tail: length(0.1),
///         feel: Feel::played(0.5, 0.4, length(0.01), &Default::default()),
///         limits: Vec::new(),
///         colliders: Vec::new(),
///     })
///     .expect("the bone exists");
/// // A second at 60 frames a second, with no furniture around and no gravity.
/// for _ in 0..60 {
///     let frame = Seconds::new(1.0 / 60.0).expect("finite");
///     if let Frame::Step { .. } = springs.capture(frame, &skeleton) {
///         springs.advance(&Furniture::default(), &mut Unmeasured);
///     }
///     for (bone, rotation) in springs.drawn() {
///         // The host writes `rotation` into `bone`'s pose: a still clip leaves the bone where the clip holds it.
///         assert!(rotation.angle_between(DQuat::IDENTITY) < 1e-6, "{bone:?} turned");
///     }
/// }
/// ```
#[derive(Clone, Debug)]
pub struct Springs {
    skeleton: Vec<Bone>,
    rate: f64,
    chains: Vec<Chain>,
    joints: Vec<Joint>,
    colliders: Vec<Collider>,
    solids: Vec<Solid>,
    /// Where each shape the host named stood at the last step, to tell a new or moved one.
    placements: std::collections::HashMap<u64, DAffine3>,
    /// Where they stood at the step before, the two maps swapped and refilled each step so a warm step allocates no
    /// map.
    placed_before: std::collections::HashMap<u64, DAffine3>,
    near: Vec<usize>,
    area: Option<Bounds>,
    /// Each chain's worst link stretch over the last frame's steps.
    stretch: Vec<f64>,
    contacts: Contacts,
    behind: f64,
    span: f64,
    due: bool,
    /// The number of the last step taken, counted from one and never reset.
    step: u64,
    /// What each bone-step's solve reuses from the last, so a warm solve allocates nothing.
    scratch: Scratch,
    followed: bool,
    given: f64,
}

impl Springs {
    /// No chains yet, over `skeleton`, stepping `rate` times a second.
    #[must_use]
    pub fn new(mut skeleton: Vec<Bone>, rate: Rate) -> Self {
        let count = skeleton.len();
        for bone in &mut skeleton {
            bone.parent = bone.parent.filter(|parent| *parent < count);
        }
        Self {
            skeleton,
            rate: f64::from(rate.hz()),
            chains: Vec::new(),
            joints: Vec::new(),
            colliders: Vec::new(),
            solids: Vec::new(),
            placements: std::collections::HashMap::new(),
            placed_before: std::collections::HashMap::new(),
            near: Vec::new(),
            area: None,
            stretch: Vec::new(),
            contacts: Contacts::default(),
            behind: 0.0,
            span: 0.0,
            due: false,
            step: 0,
            scratch: Scratch::default(),
            followed: false,
            given: 0.0,
        }
    }

    /// Adds a collider; returns its id, which chains name it by.
    ///
    /// # Errors
    ///
    /// [`Error::MissingColliderBone`] when the collider names a bone the skeleton lacks.
    pub fn add_collider(&mut self, spec: &ColliderSpec) -> Result<ColliderId, Error> {
        let bone =
            bone_named(&self.skeleton, spec.bone()).ok_or_else(|| Error::MissingColliderBone {
                bone: spec.bone().clone(),
            })?;
        self.colliders.push(Collider::new(bone, spec));
        Ok(ColliderId::new(self.colliders.len() - 1))
    }

    /// The collider added `index`th, counting from zero, as chain `chain` names it.
    ///
    /// # Errors
    ///
    /// [`Error::MissingCollider`] when the solver has no such collider.
    pub fn collider(&self, chain: &BoneName, index: usize) -> Result<ColliderId, Error> {
        if index < self.colliders.len() {
            Ok(ColliderId::new(index))
        } else {
            Err(Error::MissingCollider {
                chain: chain.clone(),
                index,
            })
        }
    }

    /// Adds a chain; a bone with no length, which nothing can point, is skipped.
    ///
    /// # Errors
    ///
    /// [`Error::MissingBone`] when the chain names a bone the skeleton lacks, and [`Error::InvalidFrame`] for a joint box
    /// whose frame cannot be normalised.
    pub fn add_chain(&mut self, spec: &ChainSpec) -> Result<(), Error> {
        let joints = joints_of(spec, &self.skeleton, self.rate)?;
        let start = self.joints.len();
        self.joints.extend(joints);
        self.stretch.push(0.0);
        self.chains.push(Chain {
            joints: start..self.joints.len(),
            colliders: spec.colliders.clone(),
            touching: Vec::new(),
            root: DVec3::ZERO,
            reach: 0.0,
            span: 0.0,
            scale: 1.0,
        });
        let by_bone: std::collections::HashMap<BoneIndex, JointIndex> = self
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| (joint.bone, JointIndex::new(index)))
            .collect();
        for joint in &mut self.joints {
            joint.parent = joint.up.and_then(|up| by_bone.get(&up).copied());
        }
        Ok(())
    }

    /// The host advanced the skeleton's animation by `delta`; from now on the chains follow that time rather than
    /// the frame's.
    pub fn follow(&mut self, delta: Seconds) {
        self.followed = true;
        self.given += delta.get();
    }

    /// Forgets every step: the next frame takes its pose afresh and the first step places each bone as the clip
    /// holds it, as for a new solver; time handed and not yet taken is dropped.
    pub fn reset(&mut self) {
        for joint in &mut self.joints {
            joint.reset();
        }
        for collider in &mut self.colliders {
            collider.reset();
        }
        self.behind = 0.0;
        self.given = 0.0;
        self.due = false;
    }

    /// Takes the frame's pose and says what the frame needs: a frame `frame_delta` long, or as long as the host has
    /// advanced its animation once it follows one.
    pub fn capture(&mut self, frame_delta: Seconds, poses: &dyn Poses) -> Frame {
        let delta = if self.followed {
            self.given
        } else {
            frame_delta.get()
        };
        self.given = 0.0;
        self.due = false;
        for joint in &mut self.joints {
            joint.records.clear();
        }
        if delta <= 0.0 {
            return Frame::Still;
        }
        // A bone whose simulated parent has not stepped hangs, like a root bone, from the clip's parent placement.
        for index in 0..self.joints.len() {
            let lent = self
                .joints
                .get(index)
                .is_some_and(|joint| parent_world(&self.joints, joint).is_some());
            if let Some(joint) = self.joints.get_mut(index) {
                let parent = (!lent).then(|| poses.world(joint.up));
                joint.capture(poses.local(joint.bone), parent);
            }
        }
        for collider in &mut self.colliders {
            collider.capture(poses.world(Some(collider.bone)));
        }

        self.span = delta * self.rate;
        self.behind = (self.behind + self.span).min(CATCH_UP);
        if self.behind < 1.0 - SNAP {
            return Frame::Draw;
        }
        self.due = true;
        for chain in &mut self.chains {
            chain.sense(&self.joints, &self.colliders);
        }
        self.area = self
            .chains
            .iter()
            .filter(|chain| !chain.joints.is_empty())
            .map(|chain| Bounds::around(chain.root, chain.reach))
            .reduce(Bounds::merge);
        Frame::Step { area: self.area }
    }

    /// Steps as many whole steps as fell due at the last capture, with `furniture` around the chains and `measured`
    /// asked about the furniture only the host measures.
    pub fn advance(&mut self, furniture: &Furniture, measured: &mut dyn Measured) {
        if !std::mem::take(&mut self.due) {
            return;
        }
        self.gather(furniture);
        let measured_near = furniture.measured && self.area.is_some();
        self.sense_furniture(measured_near, measured);
        self.stretch.fill(0.0);
        self.contacts = Contacts::default();

        let mut in_frame = FrameStep::First;
        while self.behind >= 1.0 - SNAP {
            self.behind -= 1.0;
            self.step += 1;
            let step = Step {
                number: self.step,
                in_frame,
            };
            let at = (1.0 - self.behind.max(0.0) / self.span).clamp(0.0, 1.0);
            for collider in &mut self.colliders {
                collider.place(at);
            }
            for chain in 0..self.chains.len() {
                let stretch = self.step_chain(chain, at, step, measured);
                if let Some(worst) = self.stretch.get_mut(chain) {
                    *worst = worst.max(stretch);
                }
            }
            in_frame = FrameStep::Later;
        }
    }

    /// One step of chain `chain` as one system, `step`: every tail moves on and
    /// towards where the clip points its bone from the poses the step began with; each link then keeps its length,
    /// moving both its ends, the root's end held by the animated parent; and last each bone is placed, root to tip,
    /// within its box and clear of colliders and furniture, `measured` asked about furniture only the host measures.
    /// Returns how far, in metres, the link passes left the link furthest from its length, before placement took it up.
    fn step_chain(
        &mut self,
        chain: usize,
        at: f64,
        step: Step,
        measured: &mut dyn Measured,
    ) -> f64 {
        let Some(range) = self.chains.get(chain).map(|chain| chain.joints.clone()) else {
            return 0.0;
        };
        let Some(tip) = self.joints.get(range.clone()).and_then(<[Joint]>::last) else {
            return 0.0;
        };
        if !tip.started {
            // The first step places each bone as the clip holds it, parents first, so no link starts from nowhere.
            for index in range {
                predict(&mut self.joints, index, at);
                self.settle(chain, index, step, measured);
            }
            return 0.0;
        }
        for index in range.clone() {
            predict(&mut self.joints, index, at);
        }
        let joints = &mut self.joints;
        for _ in 0..LINK_PASSES {
            for index in range.clone() {
                let up = linked_up(joints, &range, index);
                let from = up.and_then(|up| joints.get(up)).map(|joint| joint.tail);
                let given = joints
                    .get_mut(index)
                    .map_or(DVec3::ZERO, |joint| joint.link(from));
                if let Some(joint) = up.and_then(|up| joints.get_mut(up)) {
                    joint.tail += given;
                }
            }
        }
        let stretch = range.clone().fold(0.0, |worst: f64, index| {
            let from = linked_up(joints, &range, index)
                .and_then(|up| joints.get(up))
                .map(|joint| joint.tail);
            joints
                .get(index)
                .map_or(worst, |joint| worst.max(joint.stretch(from)))
        });
        for index in range.clone() {
            self.settle(chain, index, step, measured);
        }
        let joints = &mut self.joints;
        // Placing a bone at its length moves only its tail; half that move belongs to the tail it hangs from, as a
        // link between equal masses shares it, so its velocity takes it back rather than piling it onto the next step.
        for index in range.start..range.end.saturating_sub(1) {
            if parent_of(joints, index + 1) == Some(index) {
                let placed = joints
                    .get(index + 1)
                    .map_or(DVec3::ZERO, |child| child.placed);
                if let Some(joint) = joints.get_mut(index) {
                    joint.take_back(placed);
                }
            }
        }
        stretch
    }

    /// Places joint `index` of chain `chain` from its parent as the step leaves it, within the chain's colliders and
    /// its furniture, and counts what meeting the furniture took.
    fn settle(&mut self, chain: usize, index: usize, step: Step, measured: &mut dyn Measured) {
        let Some(joint) = self.joints.get(index) else {
            return;
        };
        let parent = parent_world(&self.joints, joint);
        let touching = self
            .chains
            .get(chain)
            .map_or(&[][..], |chain| chain.touching.as_slice());
        let Some(joint) = self.joints.get_mut(index) else {
            return;
        };
        let near = std::mem::take(&mut joint.near);
        let measured = if joint.asks { Some(measured) } else { None };
        let met = joint.settle(
            parent,
            Touching {
                all: &self.colliders,
                ids: touching,
            },
            Near {
                all: &self.solids,
                indices: &near,
            },
            measured,
            step,
            &mut self.scratch,
        );
        joint.near = near;
        self.contacts += met;
    }

    /// The plain shapes around the chains this frame, each marked when it is new or jumped since the last.
    fn gather(&mut self, furniture: &Furniture) {
        self.solids.clear();
        if let Some(area) = self.area {
            self.solids.extend(
                furniture
                    .shapes
                    .iter()
                    .map(|placed| Solid::new(*placed))
                    .filter(|solid| solid.bounds.is_none_or(|bounds| bounds.intersects(area))),
            );
        }
        std::mem::swap(&mut self.placements, &mut self.placed_before);
        self.placements.clear();
        self.placements.extend(
            furniture
                .shapes
                .iter()
                .map(|placed| (placed.id, placed.placement)),
        );
        let (before, now, area) = (&self.placed_before, &self.placements, self.area);
        for solid in &mut self.solids {
            solid.changed = before.get(&solid.id).is_none_or(|then| {
                area.is_some_and(|area| jumped(*then, now.get(&solid.id).copied(), area))
            });
        }
    }

    /// The furniture each bone could meet before the next frame: the plain shapes within a frame's travel of it, and
    /// whether to ask the host about the rest.
    fn sense_furniture(&mut self, measured_near: bool, measured: &mut dyn Measured) {
        for chain in &self.chains {
            self.near.clear();
            for (index, solid) in self.solids.iter().enumerate() {
                if solid.signed(chain.root).0 <= chain.reach {
                    self.near.push(index);
                }
            }
            for joint in self
                .joints
                .get_mut(chain.joints.clone())
                .unwrap_or_default()
            {
                joint.near.clear();
                joint.asks = false;
                let room = TRAVEL + (joint.length + joint.radius) * chain.scale;
                for index in &self.near {
                    let Some(solid) = self.solids.get(*index) else {
                        continue;
                    };
                    let around = solid
                        .bounds
                        .is_none_or(|bounds| bounds.grow(room).has_point(joint.world.translation));
                    if !joint.started || around {
                        joint.near.push(*index);
                    }
                }
            }
            if measured_near && !chain.joints.is_empty() {
                ask(chain, &mut self.joints, measured);
            }
        }
    }

    /// Each simulated bone and its rotation drawn one step behind the frame, between the last two steps, so the drawn
    /// motion is smooth at any display rate; a bone that has not stepped is left as the clip holds it.
    pub fn drawn(&self) -> impl Iterator<Item = (BoneIndex, DQuat)> + '_ {
        let shown = self.behind.clamp(0.0, 1.0);
        self.joints
            .iter()
            .filter_map(move |joint| joint.drawn(shown).map(|turn| (joint.bone, turn)))
    }

    /// What meeting plain furniture took over the last frame's steps.
    #[must_use]
    pub fn contacts(&self) -> Contacts {
        self.contacts
    }

    /// How many chains the solver steps.
    #[must_use]
    pub fn chain_count(&self) -> usize {
        self.chains.len()
    }

    /// The simulated bones, chain by chain, root to tip.
    #[must_use]
    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }

    /// The link the four passes left furthest from its length over the last frame's steps, before placement took the
    /// rest up: how far, and its chain's root bone; none with no chains.
    #[must_use]
    pub fn link_stretch(&self) -> Option<Worst> {
        self.chains
            .iter()
            .zip(&self.stretch)
            .filter_map(|(chain, stretch)| {
                let root = self.joints.get(chain.joints.start)?;
                Some(Worst {
                    metres: Metres::new(*stretch)?,
                    bone: root.bone,
                })
            })
            .reduce(|worst, each| {
                if each.metres.get() > worst.metres.get() {
                    each
                } else {
                    worst
                }
            })
    }

    /// How deep the deepest started tail lies in its chain's own colliders, negative when clear, and its bone; none
    /// with no started tail beside a collider.
    #[must_use]
    pub fn deepest_in_colliders(&self) -> Option<Worst> {
        let mut deepest: Option<Worst> = None;
        for chain in &self.chains {
            for joint in self
                .joints
                .get(chain.joints.clone())
                .unwrap_or_default()
                .iter()
                .filter(|joint| joint.started)
            {
                for collider in chain
                    .colliders
                    .iter()
                    .filter_map(|id| self.colliders.get(id.index()))
                {
                    let depth = collider.radius + joint.hit - collider.distance(joint.tail);
                    if let Some(metres) = Metres::new(depth)
                        && deepest.is_none_or(|worst| depth > worst.metres.get())
                    {
                        deepest = Some(Worst {
                            metres,
                            bone: joint.bone,
                        });
                    }
                }
            }
        }
        deepest
    }
}

/// The bone of `skeleton` named `name`.
pub(crate) fn bone_named(skeleton: &[Bone], name: &BoneName) -> Option<BoneIndex> {
    skeleton
        .iter()
        .position(|bone| bone.name.as_ref() == Some(name))
        .map(BoneIndex::new)
}

/// Whether a shape's placement from `then` to `now` carried some point of `area`, where the chains are, further than a
/// tail can travel in a frame: a jump the bones cannot have followed, not motion. None now is no jump, since it was
/// not gathered. The carry is affine in the point, so its longest is at a corner.
fn jumped(then: DAffine3, now: Option<DAffine3>, area: Bounds) -> bool {
    let Some(now) = now else {
        return false;
    };
    let carry = now * then.inverse();
    (0..8).any(|corner: u32| {
        let pick = |axis: u32| if corner & (1 << axis) == 0 { 0.0 } else { 1.0 };
        let point = area.position + area.size * DVec3::new(pick(0), pick(1), pick(2));
        let far = carry.transform_point3(point).distance(point);
        far.is_nan() || far > TRAVEL
    })
}

/// Marks the bones of `chain` whose tail or origin lies within a frame's travel of furniture only the host measures.
/// Both ends are asked about, since a ball centred inside a one-sided mesh does not see it; a bone's origin is where
/// the bone it hangs from ends, so each end is asked about once.
fn ask(chain: &Chain, joints: &mut [Joint], measured: &mut dyn Measured) {
    let Some(tip) = joints.get(chain.joints.clone()).and_then(<[Joint]>::last) else {
        return;
    };
    if tip.started
        && !measured.within(chain.root, chain.reach)
        && !measured.within(tip.tail, chain.reach)
    {
        return;
    }
    let mut above = false;
    let mut previous: Option<JointIndex> = None;
    for index in chain.joints.clone() {
        let Some(joint) = joints.get(index) else {
            continue;
        };
        if previous.is_none() || joint.parent != previous {
            above = measured.within(joint.origin_now(parent_world(joints, joint)), chain.span);
        }
        let here = !joint.started || measured.within(joint.tail, chain.span);
        if let Some(joint) = joints.get_mut(index) {
            joint.asks = here || above;
        }
        above = here;
        previous = Some(JointIndex::new(index));
    }
}

/// Starts joint `index`'s step `at` of the way through the frame, from its parent as the last step left it.
fn predict(joints: &mut [Joint], index: usize, at: f64) {
    let parent = joints
        .get(index)
        .and_then(|joint| parent_world(joints, joint));
    if let Some(joint) = joints.get_mut(index) {
        joint.predict(at, parent);
    }
}

/// The joint in `range` whose tail joint `index` hangs from, the one before it, if it simulates the bone's parent.
fn linked_up(joints: &[Joint], range: &Range<usize>, index: usize) -> Option<usize> {
    index
        .checked_sub(1)
        .filter(|up| range.contains(up) && parent_of(joints, index) == Some(*up))
}

/// The joint simulating joint `index`'s parent bone, if any.
fn parent_of(joints: &[Joint], index: usize) -> Option<usize> {
    joints
        .get(index)
        .and_then(|joint| joint.parent)
        .map(JointIndex::index)
}

#[cfg(test)]
mod tests;
