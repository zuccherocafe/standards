//! One bone of a chain and its tail, a unit mass whose position and velocity the steps carry.
//!
//! Each step, every tail of a chain first moves on with its velocity under gravity, and the stiffness pulls it towards
//! where the clip points its bone, from the poses the step began with, as a zero-length spring of compliance 1/k. Each
//! link then keeps its length by moving both its tails alike, the root's link moving only its own since the animated
//! parent holds the other, four times over; what length they leave unmet, which placement then takes up, the motion
//! layer reports as `physics.link_stretch_mm`, the worst link under its drive. Then, root to tip, each bone is placed
//! at its length from its parent as the step leaves it, and half the move that takes is taken off the velocity of the
//! tail it hangs from, the share an equal-mass link would have given it, so placing a bone does not pile its parent's
//! move onto the next step. It turns about the bone's origin out of each collider the chain uses, the joint's box
//! holding it, the two taking turns as `limit` says, and out of every static body on the host's obstacle mask (layer 1
//! unless the host sets it: the floor and furniture) by the chain's hit radius, as `contact` and `furniture` say. Its
//! velocity is what the step moved it, less the drag. The bone then points at its tail, the shortest way from the
//! clip's pose. Where no direction is defined, as head on or at a ball's centre, the bone takes the side its step began
//! on, then its parent's direction, then its parent's axes.
//!
//! The authored values keep what VRM's per-frame step gives them at 60 Hz, the rate they were tuned at: gravity is an
//! acceleration of 60 × `gravity` m/s²; k = 60 × `stiffness` / the bone's length, which turns the bone with the same
//! force VRM's stiffness push gives it at every angle; and `drag` removes that share of the velocity every sixtieth of
//! a second, so a step of h keeps (1 − `drag`)^(60h). The falloff ratios scale stiffness and drag from the root to the
//! tip, and gravity from the root, easing between the ends with level tangents.

use glam::{DAffine3, DMat3, DQuat, DVec3};
use tdoll_vocab::{Length, Metres, Radians};

use crate::collider::{Collider, Touching};
use crate::contact::{Friction, Gripped, Lever, Problem, Scratch, Settled, Solved, Yields};
use crate::furniture::{Measured, Near, clear_measured};
use crate::limit::JointBox;
use crate::pose::{
    Pose, arc, basis, closest_on_segment, equal_approx, interpolate, rotation_of, scale_of,
    smoothstep,
};
use crate::solver::{Bone, BoneIndex, JointIndex};
use crate::{ALTERNATIONS, TOUCH, TUNED};

/// What meeting furniture took in one bone's step: whether it started again from its present pose, and why, whether
/// the escape cleared it and whether nothing could.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a count the frame keeps apart"
)]
pub(crate) struct Met {
    /// The step started again from the present pose: a new or reset bone, a new or jumping shape, a scale change or an
    /// unresolved last step.
    pub recovered: bool,
    /// The step began inside furniture, whatever left it there; within a touch it starts from there, deeper afresh.
    pub caught: bool,
    /// No clear iterate was reached, so the escape placed the bone.
    pub escaped: bool,
    /// Steering reached its weight cap at some step of the solve.
    pub capped: bool,
    /// No direction cleared every shape.
    pub unresolved: bool,
    /// What static friction did.
    pub friction: Friction,
}

/// What furniture did to a bone at one step: the step's number, how far furniture turned the bone from where the
/// step would have put it, how its solve against plain shapes ended, none with no plain shape in reach, the physical
/// constraint's least at the solve's answer, how deep in plain furniture the step finally left the bone, and how deep
/// its origin is anchored, the allowed embedding, apart. Each quantity is none where the arithmetic left it not
/// finite, as the turn of a bone scaled to no length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    /// The solver's number for the step.
    pub step: u64,
    /// The turn furniture took the bone through.
    pub turn: Option<Radians>,
    /// How the solve ended.
    pub settled: Option<Settled>,
    /// The physical constraint's least at the solve's answer, zero with no plain shape in reach.
    pub residual: Option<Metres>,
    /// `max(0, −C)` at the direction the step left, after measured furniture.
    pub depth: Option<Length>,
    /// The deepest origin anchoring.
    pub sunk: Option<Length>,
    /// What static friction did and the largest tangential turn it allowed.
    pub gripped: Gripped,
}

impl Default for Record {
    /// A step that met no plain furniture: no turn, nothing left over and nothing deep.
    fn default() -> Self {
        Self {
            step: 0,
            turn: Some(Radians::ZERO),
            settled: None,
            residual: Metres::new(0.0),
            depth: Some(Length::ZERO),
            sunk: Some(Length::ZERO),
            gripped: Gripped::default(),
        }
    }
}

/// A springs family's feel: each value it sets overrides a chain's own, and each it leaves unset keeps the chain's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Family {
    /// The pull towards the clip's pose.
    pub stiffness: Option<f64>,
    /// The share of the velocity each tuned step removes.
    pub drag: Option<f64>,
    /// Gravity.
    pub gravity: Option<f64>,
    /// The cushioned share of each joint range.
    pub cushion: Option<f64>,
    /// Stiffness at the tip as a share of the root's.
    pub stiffness_tip: Option<f64>,
    /// Drag at the tip as a share of the root's.
    pub drag_tip: Option<f64>,
    /// Gravity at the root as a share of the tip's.
    pub gravity_root: Option<f64>,
    /// The coefficient of static friction against plain furniture.
    pub friction: Option<f64>,
}

/// A chain's feel: its stiffness, drag and gravity, the root-to-tip falloff of each, its hit radius and cushion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Feel {
    /// The pull towards where the clip points the bone, as VRM's per-frame stiffness at the tuned rate.
    pub stiffness: f64,
    /// The share of the velocity each tuned step removes.
    pub drag: f64,
    /// Gravity, in VRM's units at the tuned rate.
    pub gravity: f64,
    /// The bone's hit radius against colliders and furniture.
    pub radius: Length,
    /// The cushioned share of each joint range, next to its limit.
    pub cushion: f64,
    /// Stiffness at the tip as a share of the root's.
    pub stiffness_tip: f64,
    /// Drag at the tip as a share of the root's.
    pub drag_tip: f64,
    /// Gravity at the root as a share of the tip's.
    pub gravity_root: f64,
    /// The coefficient of static friction against plain furniture; none at 0.
    pub friction: f64,
}

impl Feel {
    /// The feel a chain plays with: its own stiffness, drag and hit radius, gravity off, each falloff flat, and the
    /// solver's cushion and friction, each value `family` sets overriding them.
    #[must_use]
    pub fn played(stiffness: f64, drag: f64, radius: Length, family: &Family) -> Self {
        Self {
            stiffness: family.stiffness.unwrap_or(stiffness),
            drag: family.drag.unwrap_or(drag),
            gravity: family.gravity.unwrap_or(0.0),
            radius,
            cushion: family.cushion.unwrap_or(crate::CUSHION),
            stiffness_tip: family.stiffness_tip.unwrap_or(1.0),
            drag_tip: family.drag_tip.unwrap_or(1.0),
            gravity_root: family.gravity_root.unwrap_or(1.0),
            friction: family.friction.unwrap_or(crate::FRICTION),
        }
    }
}

/// Where a bone turns about this step, how far its tail lies from there, its hit radius and where its tail began.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Swing {
    /// The bone's origin.
    pub origin: DVec3,
    /// The bone's length as scaled.
    pub reach: f64,
    /// The bone's hit radius as scaled.
    pub hit: f64,
    /// Where the tail began the step.
    pub before: DVec3,
}

impl Swing {
    /// `from` turned about the bone's origin, keeping its length, to the nearest place on the clear side of the plane
    /// through `surface` facing `normal`; head on, it turns the way the tail came from. When no turn clears the plane,
    /// the bone points straight out of it.
    #[must_use]
    pub(crate) fn clear_of(&self, from: DVec3, surface: DVec3, normal: DVec3) -> DVec3 {
        if normal.dot(from - surface) >= 0.0 {
            return from;
        }
        let height = normal.dot(surface - self.origin);
        if height >= self.reach {
            return self.origin + normal * self.reach;
        }
        let square =
            |point: DVec3| (point - self.origin) - normal * normal.dot(point - self.origin);
        let mut across = square(from);
        if across.length_squared() < 1e-12 {
            across = square(self.before);
        }
        if across.length_squared() < 1e-12 {
            across = normal.cross(if normal.x.abs() < 0.9 {
                DVec3::X
            } else {
                DVec3::Y
            });
        }
        let side = (self.reach * self.reach - height * height).max(0.0).sqrt();
        self.origin + normal * height + across.normalize_or_zero() * side
    }
}

/// A push along `normal` turned square to `blocked`, the way a joint's box took back the last one, both taken square
/// to the bone, since only a turn moves its tail; across `blocked` and the bone where the box took back nearly all.
#[must_use]
pub(crate) fn aside(normal: DVec3, blocked: DVec3, bone: DVec3) -> DVec3 {
    let line = bone.normalize_or_zero();
    let push = normal - line * normal.dot(line);
    let taken = (blocked - line * blocked.dot(line)).normalize_or_zero();
    let mut free = push - taken * push.dot(taken);
    if free.length() < 0.1 {
        free = taken.cross(line);
        if free.dot(normal) < 0.0 {
            free = -free;
        }
    }
    if crate::pose::zero_approx(free) {
        normal
    } else {
        free.normalize_or_zero()
    }
}

/// Which of a frame's steps a step is: the first starts a bone again near furniture that is new or has moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameStep {
    /// The frame's first step.
    First,
    /// A step after it in the same frame.
    Later,
}

/// A step as the solver takes it: its number and which of its frame's steps it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    /// The solver's number for it.
    pub(crate) number: u64,
    /// Whether it is its frame's first.
    pub(crate) in_frame: FrameStep,
}

/// The numbers of the two steps a bone's drawing interpolates between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawnSteps {
    /// The older step.
    pub older: u64,
    /// The newer step; the older one's after a reset or the first placement.
    pub newer: u64,
}

/// One simulated bone.
#[derive(Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "a bone's step keeps four states apart"
)]
pub struct Joint {
    /// The skeleton bone it simulates.
    pub(crate) bone: BoneIndex,
    /// The joint simulating the bone's parent, or none when the clip alone places the parent.
    pub(crate) parent: Option<JointIndex>,
    /// The skeleton's parent of the bone, or none for a root bone.
    pub(crate) up: Option<BoneIndex>,
    /// The plain furniture the bone tests this frame, by index into the solver's.
    pub(crate) near: Vec<usize>,
    /// Whether the bone asks the host's physics about furniture only it measures this frame.
    pub(crate) asks: bool,
    rest: DQuat,
    forward: DVec3,
    /// The bone's length before the skeleton's scale, in metres.
    pub(crate) length: f64,
    stiffness: f64,
    gravity: f64,
    /// The hit radius before the skeleton's scale, in metres.
    pub(crate) radius: f64,
    cushion: f64,
    friction: f64,
    drag: f64,
    damping: f64,
    rate: f64,
    limit: Option<JointBox>,
    /// The tail, in the world.
    pub(crate) tail: DVec3,
    /// The tail's velocity, in metres a second.
    pub(crate) velocity: DVec3,
    /// Whether the joint has stepped.
    pub(crate) started: bool,
    /// What furniture did to the bone at each of the frame's steps so far.
    pub(crate) records: Vec<Record>,
    /// The numbers of the steps the bone's drawing interpolates between, the older first.
    steps: DrawnSteps,
    captured: bool,
    clip_then: Pose,
    clip_now: Pose,
    parent_then: DAffine3,
    /// The world placement of the bone's parent this frame, when no joint simulates it.
    pub(crate) parent_now: DAffine3,
    /// The bone's simulated local rotation at the second-last step.
    pub(crate) local_then: DQuat,
    /// The bone's simulated local rotation at the last step.
    pub(crate) local: DQuat,
    /// The bone's world placement at the last step.
    pub(crate) world: DAffine3,
    origin: DVec3,
    /// The skeleton's scale at the bone the last step.
    scale_then: f64,
    /// Whether no direction cleared the bone's furniture at the last step.
    unresolved: bool,
    reach: f64,
    /// The hit radius as scaled this step.
    pub(crate) hit: f64,
    before: DVec3,
    /// How far placing the bone at its length moved its tail this step.
    pub(crate) placed: DVec3,
    parent_step: DAffine3,
    clip: Pose,
}

impl Joint {
    /// A joint for skeleton bone `bone`, laid out as `own`, its tail `axis` along it in its own axes.
    #[must_use]
    pub(crate) fn new(bone: BoneIndex, own: &Bone, axis: DVec3, limit: Option<JointBox>) -> Self {
        Self {
            bone,
            parent: None,
            up: own.parent.map(BoneIndex::new),
            near: Vec::new(),
            asks: false,
            rest: rotation_of(own.rest.matrix3),
            forward: axis.normalize_or_zero(),
            length: axis.length(),
            stiffness: 0.0,
            gravity: 0.0,
            radius: 0.0,
            cushion: 0.0,
            friction: 0.0,
            drag: 0.0,
            damping: 1.0,
            rate: f64::from(crate::RATE.hz()),
            limit,
            tail: DVec3::ZERO,
            velocity: DVec3::ZERO,
            records: Vec::new(),
            steps: DrawnSteps { older: 0, newer: 0 },
            started: false,
            captured: false,
            clip_then: Pose::IDENTITY,
            clip_now: Pose::IDENTITY,
            parent_then: DAffine3::IDENTITY,
            parent_now: DAffine3::IDENTITY,
            local_then: DQuat::IDENTITY,
            local: DQuat::IDENTITY,
            world: DAffine3::IDENTITY,
            origin: DVec3::ZERO,
            scale_then: 1.0,
            unresolved: false,
            reach: 0.0,
            hit: 0.0,
            before: DVec3::ZERO,
            placed: DVec3::ZERO,
            parent_step: DAffine3::IDENTITY,
            clip: Pose::IDENTITY,
        }
    }

    /// The chain's feel at the joint `along` of the way from root to tip, each falloff easing between its ends with
    /// level tangents, stepped `rate` times a second. VRM's drag removes that share of the velocity each tuned step,
    /// so the same share per second.
    pub(crate) fn feel(&mut self, feel: &Feel, along: f64, rate: f64) {
        let eased = smoothstep(along);
        let lerp = |from: f64, to: f64| from + (to - from) * eased;
        self.stiffness = feel.stiffness * lerp(1.0, feel.stiffness_tip);
        self.gravity = feel.gravity * lerp(feel.gravity_root, 1.0);
        self.radius = feel.radius.get();
        self.cushion = feel.cushion;
        self.friction = feel.friction.clamp(0.0, 2.0);
        self.drag = feel.drag * lerp(1.0, feel.drag_tip);
        self.rate = rate;
        self.damping = (1.0 - self.drag.clamp(0.0, 1.0)).powf(TUNED / rate);
    }

    /// The share of the way to where the clip points it that one step moves the tail: XPBD's update for a zero-length
    /// spring of compliance 1/k, where k = TUNED × stiffness / reach turns the bone with the same force VRM's
    /// per-frame stiffness gives it at the tuned rate, at every angle.
    fn pull(&self) -> f64 {
        if self.stiffness <= 0.0 {
            return 0.0;
        }
        1.0 / (1.0 + self.reach / (self.stiffness * TUNED) * self.rate * self.rate)
    }

    /// Where the bone's origin stands this frame, from its parent as the last step left it: `parent_world` when a
    /// joint simulates the parent.
    #[must_use]
    pub(crate) fn origin_now(&self, parent_world: Option<DAffine3>) -> DVec3 {
        parent_world
            .unwrap_or(self.parent_now)
            .transform_point3(self.clip_now.translation)
    }

    /// Forgets the steps: the next capture takes the clip afresh and the next step places the bone as it holds it.
    pub(crate) fn reset(&mut self) {
        self.started = false;
        self.captured = false;
        self.unresolved = false;
        self.velocity = DVec3::ZERO;
    }

    /// The clip's pose this frame, kept beside last frame's, and the parent's world placement when no joint
    /// simulates the parent.
    pub(crate) fn capture(&mut self, clip: Pose, parent_world: Option<DAffine3>) {
        self.clip_then = self.clip_now;
        self.clip_now = Pose {
            rotation: clip.rotation.normalize(),
            ..clip
        };
        if let Some(world) = parent_world {
            self.parent_then = self.parent_now;
            self.parent_now = world;
        }
        if !self.captured {
            self.clip_then = self.clip_now;
            self.parent_then = self.parent_now;
            self.captured = true;
        }
    }

    /// The step's start, with the clip taken `at` of the way from last frame's pose to this frame's and the parent as
    /// the step began, `parent_world` when a joint simulates it: gravity moves the tail on with its velocity, and the
    /// stiffness pulls it towards where the clip points the bone, with compliance.
    pub(crate) fn predict(&mut self, at: f64, parent_world: Option<DAffine3>) {
        self.parent_step =
            parent_world.unwrap_or_else(|| interpolate(self.parent_then, self.parent_now, at));
        self.clip = if self.clip_then == self.clip_now {
            self.clip_now
        } else {
            self.clip_then.interpolate(self.clip_now, at)
        };
        let parent_turn = rotation_of(self.parent_step.matrix3);
        let scale = scale_of(self.parent_step.matrix3).x;
        self.reach = self.length * scale;
        self.hit = self.radius * scale;
        self.origin = self.parent_step.transform_point3(self.clip.translation);
        let target = self.origin + (parent_turn * self.clip.rotation) * self.forward * self.reach;
        if !self.started {
            self.tail = target;
            self.velocity = DVec3::ZERO;
        }
        self.before = self.tail;
        self.velocity += DVec3::NEG_Y * self.gravity * TUNED / self.rate;
        self.tail += self.velocity / self.rate;
        self.tail += (target - self.tail) * self.pull();
    }

    /// The tail kept at the bone's length from `from`, the tail the bone hangs from or, for none, the bone's origin;
    /// returns the move that belongs to `from`, half the fix, when it is a tail.
    pub(crate) fn link(&mut self, from: Option<DVec3>) -> DVec3 {
        let apart = self.tail - from.unwrap_or(self.origin);
        let span = apart.length();
        if span < 1e-9 {
            return DVec3::ZERO;
        }
        let fix = apart / span * (span - self.reach);
        if from.is_none() {
            self.tail -= fix;
            return DVec3::ZERO;
        }
        self.tail -= fix * 0.5;
        fix * 0.5
    }

    /// How far the tail lies from the bone's length away from `from`, the tail it hangs from or, for none, its origin.
    pub(crate) fn stretch(&self, from: Option<DVec3>) -> f64 {
        (self.tail.distance(from.unwrap_or(self.origin)) - self.reach).abs()
    }

    /// The bone placed from its parent as the step leaves it, `parent_world` when a joint simulates it: at its
    /// length, the colliders its chain is `touching` and the joint's box holding it between them, and its furniture
    /// `near` turning it out of that, winning over the box, the frame's first step starting it again near a new or
    /// moved shape and `measured` asked about shapes only the host measures. The velocity is what `step` moved the
    /// tail, less the drag; a step that starts again from the present pose keeps none of furniture's push in it.
    /// `scratch` is what each bone-step's solve reuses from the last.
    pub(crate) fn settle(
        &mut self,
        parent_world: Option<DAffine3>,
        touching: Touching<'_>,
        near: Near<'_>,
        measured: Option<&mut dyn Measured>,
        step: Step,
        scratch: &mut Scratch,
    ) -> Met {
        let linked = self.tail;
        let parent = parent_world.unwrap_or(self.parent_step);
        let parent_turn = rotation_of(parent.matrix3);
        self.origin = parent.transform_point3(self.clip.translation);
        let rotation = parent_turn * self.clip.rotation;
        self.tail = self.origin + (self.tail - self.origin).normalize_or_zero() * self.reach;
        self.placed = self.tail - linked;

        self.local_then = self.local;
        self.steps = DrawnSteps {
            older: self.steps.newer,
            newer: step.number,
        };
        let held = self.held(parent_turn, rotation, touching);
        let (met, mut record) = self.meet(parent, held, touching, near, step.in_frame, scratch);
        let solved = self.tail;
        if let Some(server) = measured {
            self.tail = clear_measured(&self.swing(), self.tail, server);
        }
        // Measured furniture moved the tail: the whole capsule is measured again where it now lies.
        if self.tail != solved
            && let Some(lever) = self.lever(parent, held, near, &mut scratch.near)
        {
            let problem = lever.problem_in(std::mem::take(&mut scratch.spare));
            let direction = (self.tail - self.origin).normalize_or_zero();
            record.depth = Length::new((-problem.physical(direction)).max(0.0));
            scratch.spare = problem.finish();
        }
        record.step = step.number;
        record.turn = Radians::new((held - self.origin).angle_between(self.tail - self.origin));
        self.records.push(record);
        if !equal_approx(self.tail, held) {
            self.local =
                parent_turn.inverse() * self.aim(parent_turn * self.local, self.tail - self.origin);
        }
        if !self.started {
            self.local_then = self.local;
            self.steps = DrawnSteps {
                older: step.number,
                newer: step.number,
            };
            self.started = true;
        }

        let moved = if met.recovered { held } else { self.tail };
        self.velocity = (moved - self.before) * self.rate * self.damping;
        let local = basis(self.local) * DMat3::from_diagonal(self.clip.scale);
        self.world = parent * DAffine3::from_mat3_translation(local, self.clip.translation);
        self.scale_then = self.reach / self.length;
        met
    }

    /// The tail turned out of the plain furniture `near` within the bone's reach, its parent placed at `parent`: from
    /// where its path from the step's start first meets one, keeping its side, or afresh from `held` when the step
    /// starts again from the present pose, as on the frame's `first` step near a new or moved shape, or begins inside
    /// furniture. Friction yields to the push of the colliders it is `touching`.
    fn meet(
        &mut self,
        parent: DAffine3,
        held: DVec3,
        touching: Touching<'_>,
        near: Near<'_>,
        first: FrameStep,
        scratch: &mut Scratch,
    ) -> (Met, Record) {
        let mut met = Met::default();
        let Some(lever) = self.lever(parent, held, near, &mut scratch.near) else {
            self.unresolved = false;
            return (met, Record::default());
        };
        let target = (held - self.origin).normalize_or_zero();
        let rescaled = (self.reach / self.length / self.scale_then - 1.0).abs() > 1e-6;
        met.recovered = !self.started
            || self.unresolved
            || rescaled
            || (first == FrameStep::First && lever.solids.solids().any(|solid| solid.changed));
        let parent_turn = rotation_of(parent.matrix3);
        let rotation = parent_turn * self.clip.rotation;
        let friction = self.friction > 0.0 && !met.recovered;
        // Where the colliders' push and the box's hard limit would take the start: friction yields to them, but they
        // move nothing, so furniture alone wins.
        let yields = if friction {
            self.yields(parent_turn, rotation, lever.start, touching)
        } else {
            Yields::default()
        };
        let mut problem = lever.problem_in(std::mem::take(&mut scratch.spare));
        // A recovered bone starts again from its target, so where it began the step is never assessed.
        let start = (!met.recovered).then(|| problem.assess(lever.start));
        met.caught = start.as_ref().is_some_and(|start| !start.feasible);
        let gripped = match &start {
            Some(start) if friction => problem.grip(start, target, &yields, self.friction),
            _ => Gripped::default(),
        };
        let outcome = problem.resolve(target, start.as_ref());
        if let Some(start) = start {
            problem.recycle(start);
        }
        met.friction = gripped.friction;
        let record = self.ended(&mut met, &problem, &outcome, gripped);
        problem.recycle(outcome.assessment);
        scratch.spare = problem.finish();
        (met, record)
    }

    /// The tail where `outcome` left it, `met` told how the solve ended, and the step's record.
    fn ended(
        &mut self,
        met: &mut Met,
        problem: &Problem<'_>,
        outcome: &Solved,
        gripped: Gripped,
    ) -> Record {
        self.unresolved = outcome.unresolved();
        met.escaped = outcome.escaped();
        met.capped = outcome.capped;
        met.unresolved = outcome.unresolved();
        self.tail = self.origin + outcome.direction * self.reach;
        let residual = problem.residual(&outcome.assessment);
        Record {
            settled: Some(outcome.settled),
            residual: Metres::new(residual),
            depth: Length::new((-residual).max(0.0)),
            sunk: Length::new(problem.sunk()),
            gripped,
            ..Record::default()
        }
    }

    /// Where the push of the colliders the chain is `touching`, and then the joint box's hard limit, the cushion aside,
    /// would turn `direction`, one pass of `held`'s loop; none for either that would not move it.
    fn yields(
        &self,
        parent_turn: DQuat,
        rotation: DQuat,
        direction: DVec3,
        touching: Touching<'_>,
    ) -> Yields {
        let start = self.origin + direction * self.reach;
        let pushed = self.pushed(start, touching.colliders(), DVec3::ZERO);
        let mut tail = pushed;
        if let Some(limit) = self.limit {
            let wanted = self.rest.inverse()
                * parent_turn.inverse()
                * self.aim(rotation, tail - self.origin);
            let local = self.rest * limit.hold(wanted, 0.0);
            tail = self.origin + (parent_turn * local) * self.forward * self.reach;
        }
        let way = |tail: DVec3| {
            (tail.distance(start) > 1e-12).then(|| (tail - self.origin).normalize_or(direction))
        };
        Yields {
            pushed: way(pushed),
            limited: way(tail),
        }
    }

    /// `tail` pushed out of each of `touching` it lies within, in turn, a push the box took back turned square to
    /// `blocked`.
    fn pushed<'a>(
        &self,
        mut tail: DVec3,
        touching: impl Iterator<Item = &'a Collider>,
        blocked: DVec3,
    ) -> DVec3 {
        for collider in touching {
            let clearance = collider.radius + self.hit;
            let nearest = closest_on_segment(tail, collider.start, collider.end);
            if tail.distance_squared(nearest) < clearance * clearance {
                tail = collider.push(tail, &self.swing(), blocked);
            }
        }
        tail
    }

    /// The bone as its plain furniture sees it this step, turning from where it began towards `held`, over the near
    /// shapes within its reach, their indices put in `kept` in the order of the host's names for them; none when no
    /// plain shape is within its reach.
    fn lever<'s>(
        &self,
        parent: DAffine3,
        held: DVec3,
        near: Near<'s>,
        kept: &'s mut Vec<usize>,
    ) -> Option<Lever<'s>> {
        kept.clear();
        kept.extend(near.indices.iter().copied().filter(|index| {
            near.all
                .get(*index)
                .is_some_and(|solid| solid.signed(self.origin).0 <= self.reach + self.hit)
        }));
        if kept.is_empty() {
            return None;
        }
        kept.sort_by_key(|index| near.all.get(*index).map_or(0, |solid| solid.id));
        let target = (held - self.origin).normalize_or_zero();
        let turn = rotation_of(parent.matrix3);
        Some(Lever {
            origin: self.origin,
            reach: self.reach,
            hit: self.hit,
            start: (self.before - self.origin).normalize_or(target),
            frame: [
                (self.origin - parent.translation).normalize_or_zero(),
                turn * DVec3::X,
                turn * DVec3::Z,
                turn * DVec3::Y,
            ],
            solids: Near {
                all: near.all,
                indices: kept,
            },
        })
    }

    /// Where the bone turns about this step and how far its tail lies from there.
    #[must_use]
    pub(crate) fn swing(&self) -> Swing {
        Swing {
            origin: self.origin,
            reach: self.reach,
            hit: self.hit,
            before: self.before,
        }
    }

    /// The colliders the chain is `touching` turn the tail out of them and the box holds the
    /// bone, in turn, while that takes the tail further out. A push the box takes back is tried again square to what it
    /// took; where no turn inside the box clears the colliders, the box wins as the first turn left it.
    fn held(&mut self, parent_turn: DQuat, rotation: DQuat, touching: Touching<'_>) -> DVec3 {
        let mut blocked = DVec3::ZERO;
        let mut first = DVec3::ZERO;
        let mut first_local = DQuat::IDENTITY;
        let mut was = f64::INFINITY;
        let mut settled = true;
        for attempt in 0..ALTERNATIONS {
            self.tail = self.pushed(self.tail, touching.colliders(), blocked);
            let wanted = self.rest.inverse()
                * parent_turn.inverse()
                * self.aim(rotation, self.tail - self.origin);
            self.local = self.rest * self.limited(wanted);
            let held = self.origin + (parent_turn * self.local) * self.forward * self.reach;
            blocked = self.tail - held;
            self.tail = held;
            if attempt == 0 {
                first = held;
                first_local = self.local;
            }
            if self.limit.is_none() {
                break;
            }
            let inside = self.sunk(touching.colliders());
            settled = inside <= TOUCH;
            if settled || blocked.length_squared() < 1e-12 || inside > was - TOUCH {
                break;
            }
            was = inside;
        }
        if !settled {
            self.tail = first;
            self.local = first_local;
        }
        self.tail
    }

    /// How far the tail lies in its colliders, by the deepest.
    fn sunk<'a>(&self, touching: impl Iterator<Item = &'a Collider>) -> f64 {
        touching.fold(0.0, |deepest, collider| {
            deepest.max(collider.radius + self.hit - collider.distance(self.tail))
        })
    }

    /// The bone's rotation turned the shortest way from its forward to `towards`.
    fn aim(&self, rotation: DQuat, towards: DVec3) -> DQuat {
        let from = (rotation * self.forward).normalize_or_zero();
        let to = towards.normalize_or_zero();
        if from.dot(to) < -0.9999 {
            return rotation;
        }
        arc(from, to) * rotation
    }

    /// The bone's turn from rest held within its box, eased into by the cushion.
    fn limited(&self, wanted: DQuat) -> DQuat {
        self.limit
            .map_or(wanted, |limit| limit.hold(wanted, self.cushion))
    }

    /// Takes back from the tail's velocity half the move placing its child bone, `child_placed`, gave that child's
    /// tail, as a link between equal masses shares it, so the move does not pile onto the next step.
    pub(crate) fn take_back(&mut self, child_placed: DVec3) {
        self.velocity -= child_placed * self.rate * 0.5;
    }

    /// The skeleton bone it simulates.
    #[must_use]
    pub fn bone(&self) -> BoneIndex {
        self.bone
    }

    /// The simulated tail, in the world.
    #[must_use]
    pub fn tail(&self) -> DVec3 {
        self.tail
    }

    /// What furniture did to the bone at each step of the last advance; none when no step fell due.
    #[must_use]
    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// The numbers of the two steps the bone's drawing interpolates between, the older first; after a reset or the
    /// first placement, both that step.
    #[must_use]
    pub fn drawn_steps(&self) -> DrawnSteps {
        self.steps
    }

    /// Whether the bone asked the host's physics about furniture only it measures this frame.
    #[must_use]
    pub fn asks(&self) -> bool {
        self.asks
    }

    /// The bone's rotation drawn `shown` of the way from its second-last step to its last, once it has stepped.
    #[must_use]
    pub(crate) fn drawn(&self, shown: f64) -> Option<DQuat> {
        self.started
            .then(|| crate::pose::slerp(self.local_then, self.local, shown))
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use glam::{DAffine3, DQuat, DVec3};
    use tdoll_vocab::{BoneName, Length, Radians};

    use super::*;
    use crate::chain::{ChainSpec, joints_of};
    use crate::collider::{ColliderId, ColliderShape, ColliderSpec};
    use crate::furniture::{Placed, Shape, Solid};
    use crate::limit::{AxisLimit, JointBox};
    use crate::{CUSHION, FRICTION};

    #[test]
    fn a_chain_plays_its_own_feel_with_gravity_off_and_each_value_its_family_sets_overriding_it() {
        let hit = Length::new(0.01).unwrap();
        let own = Feel::played(0.4, 0.2, hit, &Family::default());
        assert_eq!(
            own,
            Feel {
                stiffness: 0.4,
                drag: 0.2,
                gravity: 0.0,
                radius: hit,
                cushion: CUSHION,
                stiffness_tip: 1.0,
                drag_tip: 1.0,
                gravity_root: 1.0,
                friction: FRICTION,
            }
        );
        let family = Family {
            stiffness: Some(0.0),
            drag: None,
            gravity: Some(2.5),
            cushion: None,
            stiffness_tip: Some(0.5),
            drag_tip: None,
            gravity_root: Some(0.25),
            friction: Some(1.5),
        };
        let played = Feel::played(0.4, 0.2, hit, &family);
        assert_eq!(
            played,
            Feel {
                stiffness: 0.0,
                drag: 0.2,
                gravity: 2.5,
                radius: hit,
                cushion: CUSHION,
                stiffness_tip: 0.5,
                drag_tip: 1.0,
                gravity_root: 0.25,
                friction: 1.5,
            }
        );
    }

    fn name(text: &str) -> BoneName {
        BoneName::new(text).unwrap()
    }

    fn bone(parent: Option<usize>, at: DVec3) -> Bone {
        Bone {
            name: None,
            parent,
            rest: DAffine3::from_translation(at),
        }
    }

    /// A bone `length` long along `forward`, held by `limit`, its hit radius `hit`.
    fn joint(forward: DVec3, length: f64, hit: f64, limit: Option<JointBox>) -> Joint {
        let mut joint = Joint::new(
            BoneIndex::new(0),
            &bone(None, DVec3::ZERO),
            forward * length,
            limit,
        );
        joint.radius = hit;
        joint
    }

    /// The bone placed for a step with its parent at `parent` and the clip holding it at rest.
    fn placed(joint: &mut Joint, parent: DAffine3) {
        joint.capture(Pose::IDENTITY, Some(parent));
        joint.predict(1.0, None);
    }

    /// A plain shape as the solver holds it.
    fn plain(placement: DAffine3, shape: Shape) -> Solid {
        Solid::new(Placed {
            placement,
            shape,
            id: 1,
        })
    }

    /// One step of `joint` with its parent at `parent`, among `solids`.
    fn settled(joint: &mut Joint, parent: DAffine3, solids: &[Solid], number: u64) -> Met {
        placed(joint, parent);
        let indices: Vec<usize> = (0..solids.len()).collect();
        joint.settle(
            None,
            Touching { all: &[], ids: &[] },
            Near {
                all: solids,
                indices: &indices,
            },
            None,
            Step {
                number,
                in_frame: FrameStep::Later,
            },
            &mut Scratch::default(),
        )
    }

    /// Three unit bones, stiffness 2 falling to a quarter at the tip, drag 0.2 to a half, and gravity 4 rising from a
    /// quarter at the root: the middle bone, halfway, has each eased halfway.
    #[test]
    fn a_chains_feel_eases_from_root_to_tip() {
        let skeleton = [
            Bone {
                name: Some(name("root")),
                ..bone(None, DVec3::ZERO)
            },
            Bone {
                name: Some(name("a")),
                ..bone(Some(0), DVec3::Y)
            },
            Bone {
                name: Some(name("b")),
                ..bone(Some(1), DVec3::Y)
            },
            Bone {
                name: Some(name("c")),
                ..bone(Some(2), DVec3::Y)
            },
        ];
        let feel = Feel {
            stiffness: 2.0,
            drag: 0.2,
            gravity: 4.0,
            radius: Length::new(0.01).unwrap(),
            cushion: CUSHION,
            stiffness_tip: 0.25,
            drag_tip: 0.5,
            gravity_root: 0.25,
            friction: FRICTION,
        };
        let spec = ChainSpec {
            bones: vec![name("a"), name("b"), name("c")],
            tail: Length::new(1.0).unwrap(),
            feel,
            limits: Vec::new(),
            colliders: Vec::new(),
        };
        let joints = joints_of(&spec, &skeleton, 120.0).unwrap();
        let read: Vec<[f64; 3]> = joints
            .iter()
            .map(|joint| [joint.stiffness, joint.drag, joint.gravity])
            .collect();
        let expected = [[2.0, 0.2, 1.0], [1.25, 0.15, 2.5], [0.5, 0.1, 4.0]];
        for (found, wanted) in read.iter().zip(expected) {
            for (value, want) in found.iter().zip(wanted) {
                assert!((value - want).abs() < 1e-12, "{read:?}");
            }
        }
    }

    /// A plane a metre above a half-metre bone's origin is out of its reach: the bone points straight out of it.
    #[test]
    fn a_plane_out_of_reach_leaves_the_bone_pointing_straight_out_at_its_length() {
        let origin = DVec3::new(2.0, 3.0, 4.0);
        let swing = Swing {
            origin,
            reach: 0.5,
            hit: 0.01,
            before: origin + DVec3::X * 0.5,
        };
        let out = swing.clear_of(origin + DVec3::X * 0.5, DVec3::new(2.0, 4.0, 4.0), DVec3::Y);
        assert!(out.distance(DVec3::new(2.0, 3.5, 4.0)) < 1e-12, "{out}");
    }

    #[test]
    fn a_push_the_box_took_nearly_all_of_goes_across_on_the_side_of_its_normal() {
        for side in [-1.0, 1.0] {
            let normal = DVec3::new(1.0, 0.0, 0.01 * side).normalize();
            let across = aside(normal, DVec3::X, DVec3::Y);
            assert!(
                (across - DVec3::Z * side).length() < 1e-12,
                "{side}: {across}"
            );
        }
    }

    /// At stiffness 1 and 60 steps a second a unit bone's k is 60, so one step moves the tail 1/61 of the way.
    #[test]
    fn one_step_pulls_the_tail_by_the_compliant_share() {
        let mut joint = joint(DVec3::Y, 1.0, 0.0, None);
        let feel = Feel::played(1.0, 0.0, Length::ZERO, &Family::default());
        joint.feel(&feel, 0.0, 60.0);
        joint.reach = 1.0;
        assert!((joint.pull() - 1.0 / 61.0).abs() < 1e-15);
    }

    #[test]
    fn a_step_a_quarter_through_a_frame_takes_the_clip_a_quarter_of_the_way() {
        let mut joint = joint(DVec3::Y, 1.0, 0.0, None);
        let at = |x: f64| Pose {
            translation: DVec3::X * x,
            ..Pose::IDENTITY
        };
        joint.capture(at(0.0), Some(DAffine3::IDENTITY));
        joint.capture(at(2.0), Some(DAffine3::IDENTITY));
        joint.predict(0.25, None);
        assert!(
            joint.origin.distance(DVec3::X * 0.5) < 1e-12,
            "{}",
            joint.origin
        );
    }

    #[test]
    fn a_bone_scaled_twice_reaches_and_hits_twice_as_far() {
        let mut joint = joint(DVec3::Y, 0.5, 0.1, None);
        placed(&mut joint, DAffine3::from_scale(DVec3::splat(2.0)));
        assert!((joint.reach - 1.0).abs() < 1e-12 && (joint.hit - 0.2).abs() < 1e-12);
        assert!((joint.tail.distance(joint.origin) - 1.0).abs() < 1e-12);
    }

    /// A unit bone the clip points at a wall 1.1 cm beyond its origin, with a 1 cm hit radius, is turned until its tail
    /// is a millimetre past the origin's plane: a turn of acos(0.001), whatever its parent's place and turn, and the
    /// bone is aimed at where the tail was left.
    #[test]
    fn furniture_turns_a_bone_about_its_own_origin_and_aims_it_at_its_tail() {
        let moved = DAffine3::from_translation(DVec3::new(2.0, 3.0, 4.0));
        let turned = DAffine3::from_rotation_translation(
            DQuat::from_rotation_z(FRAC_PI_2),
            DVec3::new(2.0, 3.0, 4.0),
        );
        for parent in [DAffine3::IDENTITY, moved, turned] {
            let mut joint = joint(DVec3::X, 1.0, 0.01, None);
            let ahead = parent.transform_vector3(DVec3::X);
            let surface = parent.translation + ahead * 0.011;
            let wall = plain(
                DAffine3::IDENTITY,
                Shape::Plane {
                    normal: -ahead,
                    distance: -ahead.dot(surface),
                },
            );
            settled(&mut joint, parent, &[wall], 1);
            let turn = joint.records.last().and_then(|record| record.turn).unwrap();
            assert!(
                (turn.get() - 0.001_f64.acos()).abs() < 1e-6,
                "{}",
                turn.get()
            );
            let way = (joint.tail - joint.origin).normalize();
            let aimed = rotation_of(parent.matrix3) * joint.local * joint.forward;
            assert!(aimed.distance(way) < 1e-9, "aimed {aimed}, not {way}");
        }
    }

    /// A bone over a floor in reach: started again on its first step and when its scale changes, and not otherwise.
    #[test]
    fn a_bone_starts_again_once_when_its_scale_changes() {
        let mut joint = joint(DVec3::Y, 0.5, 0.01, None);
        let floor = [plain(
            DAffine3::IDENTITY,
            Shape::Plane {
                normal: DVec3::Y,
                distance: -0.5,
            },
        )];
        let scaled = |scale: f64| DAffine3::from_scale(DVec3::splat(scale));
        let recovered: Vec<bool> = [2.0, 2.0, 3.0, 3.0]
            .into_iter()
            .zip(1..)
            .map(|(scale, number)| settled(&mut joint, scaled(scale), &floor, number).recovered)
            .collect();
        assert_eq!(recovered, [true, false, true, false]);
    }

    /// A box holding the turn about its X axis within 0.3 rad, the bone's own turn.
    fn held_about_x() -> JointBox {
        let free = AxisLimit::Free;
        let held = AxisLimit::Held {
            min: Radians::new(-0.3).unwrap(),
            max: Radians::new(0.3).unwrap(),
        };
        JointBox::new(DQuat::IDENTITY, true, [held, free, free]).unwrap()
    }

    /// Where the colliders and then the box would turn `direction` for a unit bone at `pivot` along +Y under a parent
    /// turned a quarter about Z.
    fn yields_at(
        pivot: DVec3,
        limit: Option<JointBox>,
        colliders: &[Collider],
        direction: DVec3,
    ) -> Yields {
        let mut joint = joint(DVec3::Y, 1.0, 0.01, limit);
        joint.origin = pivot;
        joint.reach = 1.0;
        joint.hit = 0.01;
        let parent = DQuat::from_rotation_z(FRAC_PI_2);
        let ids: Vec<ColliderId> = (0..colliders.len()).map(ColliderId::new).collect();
        let touching = Touching {
            all: colliders,
            ids: &ids,
        };
        joint.yields(parent, parent, direction, touching)
    }

    #[test]
    fn friction_yields_to_where_the_box_and_the_colliders_would_turn_the_bone_wherever_it_stands() {
        let parent = DQuat::from_rotation_z(FRAC_PI_2);
        let turned = |angle: f64| parent * (DQuat::from_rotation_x(angle) * DVec3::Y);
        for pivot in [DVec3::ZERO, DVec3::new(2.0, 3.0, 4.0)] {
            let limited = yields_at(pivot, Some(held_about_x()), &[], turned(0.5));
            assert!(limited.pushed.is_none());
            let way = limited.limited.unwrap();
            assert!(way.distance(turned(0.3)) < 1e-9, "{pivot}: {way}");
            let inside = yields_at(pivot, Some(held_about_x()), &[], turned(0.2));
            assert!(inside.pushed.is_none() && inside.limited.is_none());
        }
        let pushed_from = |pivot: DVec3| {
            let centre = pivot + DVec3::new(-1.0, 0.1, 0.0);
            let spec = ColliderSpec::new(
                name("bone"),
                DQuat::IDENTITY,
                centre,
                ColliderShape::Sphere {
                    radius: Length::new(0.2).unwrap(),
                },
            );
            let mut ball = Collider::new(BoneIndex::new(0), &spec);
            ball.capture(DAffine3::IDENTITY);
            ball.place(1.0);
            let found = yields_at(pivot, None, &[ball], DVec3::NEG_X);
            let way = found.pushed.unwrap();
            assert_eq!(found.limited, Some(way));
            assert!((way.length() - 1.0).abs() < 1e-12);
            assert!(
                (pivot + way).distance(centre) >= 0.21 - 1e-6,
                "{pivot}: {way}"
            );
            way
        };
        let (here, there) = (
            pushed_from(DVec3::ZERO),
            pushed_from(DVec3::new(2.0, 3.0, 4.0)),
        );
        assert!(here.distance(there) < 1e-12, "{here} and {there}");
    }

    /// A ball of radius 0.2 and a hit radius of 0.1: a tail 0.25 m from its centre is pushed out, one 0.4 m away is
    /// not.
    #[test]
    fn a_tail_within_a_colliders_radius_and_its_own_is_pushed_and_one_beyond_is_not() {
        let spec = ColliderSpec::new(
            name("bone"),
            DQuat::IDENTITY,
            DVec3::new(0.0, 1.0, 0.0),
            ColliderShape::Sphere {
                radius: Length::new(0.2).unwrap(),
            },
        );
        let mut ball = Collider::new(BoneIndex::new(0), &spec);
        ball.capture(DAffine3::IDENTITY);
        ball.place(1.0);
        let mut joint = joint(DVec3::Y, 1.0, 0.1, None);
        joint.reach = 1.0;
        joint.hit = 0.1;
        let far = DVec3::new(0.4, 1.0, 0.0).normalize();
        assert_eq!(joint.pushed(far, std::iter::once(&ball), DVec3::ZERO), far);
        let near = DVec3::new(0.25, 1.0, 0.0).normalize();
        let out = joint.pushed(near, std::iter::once(&ball), DVec3::ZERO);
        assert!(
            out.distance(DVec3::new(0.0, 1.0, 0.0)) >= 0.3 - 1e-6,
            "{out}"
        );
    }

    #[test]
    fn a_bones_contact_target_start_and_parent_hint_are_taken_from_its_own_origin() {
        let floor = [plain(
            DAffine3::IDENTITY,
            Shape::Plane {
                normal: DVec3::Y,
                distance: 2.5,
            },
        )];
        let mut joint = joint(DVec3::Y, 1.0, 0.01, None);
        let pivot = DVec3::new(2.0, 3.0, 4.0);
        joint.origin = pivot;
        joint.reach = 1.0;
        let parent = DAffine3::from_translation(DVec3::new(1.0, 3.0, 4.0));
        let held = pivot + DVec3::Y;
        let near = Near {
            all: &floor,
            indices: &[0],
        };
        let mut kept = Vec::new();
        joint.before = pivot + DVec3::X;
        let lever = joint.lever(parent, held, near, &mut kept).unwrap();
        assert!(lever.start.distance(DVec3::X) < 1e-12, "{}", lever.start);
        assert!(
            lever.frame[0].distance(DVec3::X) < 1e-12,
            "{}",
            lever.frame[0]
        );
        joint.before = pivot;
        let lever = joint.lever(parent, held, near, &mut kept).unwrap();
        assert!(
            lever.start.distance(DVec3::Y) < 1e-12,
            "the target: {}",
            lever.start
        );
    }

    /// A bone locked along +Y whose tail sits at a ball's centre, where no push has a way: the box wins as its first
    /// turn left the bone, at its own pivot.
    #[test]
    fn a_collider_no_turn_can_clear_leaves_the_box_holding_the_bone_at_its_pivot() {
        let locked = AxisLimit::Held {
            min: Radians::ZERO,
            max: Radians::ZERO,
        };
        let limit = JointBox::new(DQuat::IDENTITY, true, [locked; 3]);
        let pivot = DVec3::new(2.0, 3.0, 4.0);
        let spec = ColliderSpec::new(
            name("bone"),
            DQuat::IDENTITY,
            pivot + DVec3::Y,
            ColliderShape::Sphere {
                radius: Length::new(0.2).unwrap(),
            },
        );
        let mut ball = Collider::new(BoneIndex::new(0), &spec);
        ball.capture(DAffine3::IDENTITY);
        ball.place(1.0);
        let mut joint = joint(DVec3::Y, 1.0, 0.01, limit);
        joint.origin = pivot;
        joint.reach = 1.0;
        joint.hit = 0.01;
        joint.tail = pivot + DVec3::Y;
        let touching = Touching {
            all: std::slice::from_ref(&ball),
            ids: &[ColliderId::new(0)],
        };
        let held = joint.held(DQuat::IDENTITY, DQuat::IDENTITY, touching);
        assert!(held.distance(pivot + DVec3::Y) < 1e-12, "{held}");
    }
}
