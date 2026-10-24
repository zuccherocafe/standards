//! Spring chains solved as XPBD with small steps, the source's joint limits and colliders applied to the simulated
//! state itself; engine-independent, the host reading and writing the skeleton.
//!
//! It runs every frame on the headset. Its budget, with its engine binding, is 1 ms of a 90 Hz frame there, and a
//! warm step allocates nothing. `tdoll-measure perf springs` measures a step's instructions and CPU on a built
//! character, `tools/perf.py` compares two revisions and states the budget's standing, and
//! `tests/allocation.rs` states that a warm step allocates nothing.
//!
//! Every chain, whatever its family, runs on this one solver, judged against the motion measures. The solver is XPBD
//! with small steps: each tail is a unit mass, and each chain is solved as one system, so its motion is the same
//! whatever rate it steps at. It computes in double precision; the engine's single-precision shortest arc, which treats
//! an aim under 0.04° as none, is the known source of differences under 0.07° from a single-precision solver. Lengths
//! and hit radii scale with the character.
//!
//! What moves a bone has a strict priority: furniture, then the chain's colliders and its joint box's hard limit, then
//! friction, then the clip's target. The modules hold the design: `solver` the schedule of steps, `joint` the step
//! itself, `limit` the joint boxes and their turns with the colliders, `furniture` the shapes furniture takes,
//! `contact` how a bone meets plain furniture, with `friction` beside it.

#![forbid(unsafe_code)]
#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#![warn(clippy::indexing_slicing)]

/// One simulated chain: its layout, feel and step.
pub mod chain;
/// Bone colliders, placed where the bone's pose puts them.
pub mod collider;
/// A bone meeting plain furniture as a capsule turned about its origin.
pub mod contact;
/// Furniture shapes as they stand, furniture only a host's physics measures, behind a trait the host implements, and
/// the sweep against it.
pub mod furniture;
/// One bone of a chain and its simulated tail.
pub mod joint;
/// A source joint's rotation box and the cushion that eases a bone into it.
pub mod limit;
/// Poses and the few rotation operations the solver needs.
pub mod pose;
/// The searches along an arc or a bone: a level crossing and an ordered predicate's rightmost share.
mod roots;
/// The solver for one set of chains: the fixed step, the frame's capture and the drawn pose.
pub mod solver;

pub use chain::Contacts;
pub use furniture::{Measured, Unmeasured};
pub use solver::{Bone, BoneIndex, Frame, Poses, Springs, Worst};
use tdoll_vocab::{BoneName, Rate};

/// The chains step at this rate whatever the display's, in steps a second; each chain is solved as one system, so
/// the feel does not depend on it.
pub const RATE: Rate = Rate::new(120).expect("a literal");
/// The links of a chain are solved this many times a step, enough that what is left for each bone's own placement
/// is small; a few passes cost little beside the placement.
const LINK_PASSES: usize = 4;
/// A spring group's stiffness, drag and gravity mean what VRM's per-frame step gives them at this rate.
const TUNED: f64 = 60.0;
/// A frame longer than this many steps, a hitch, drops the time beyond them rather than stepping to catch up.
const CATCH_UP: f64 = 4.0;
/// A frame within this share of a step of a whole number of steps takes that number, whatever the delta's rounding.
const SNAP: f64 = 1e-4;
/// A spring group without a cushion cushions the last 30% of each joint range: the bone slows into the limit there.
pub const CUSHION: f64 = 0.3;
/// A spring group without a friction coefficient meets plain furniture with this one; 0 is none.
pub const FRICTION: f64 = 0.4;
/// A tail pushed out of a collider and back to its bone's length settles within a few pushes.
const PUSHES: usize = 8;
/// The colliders and a joint's box take turns this many times a step to find a turn inside the box that clears.
const ALTERNATIONS: usize = 6;
/// A sweep that has not come within touch of furniture after this many advances passes it by.
const TRACES: usize = 16;
/// A tail this close to furniture or a collider, in metres, touches it.
const TOUCH: f64 = 0.0005;
/// Furniture closer than this, in metres, touches the bone and further in it is met: the solver's own rounding is
/// about 1e-14 m, and the smallest hit radius in use is 3 mm.
const TOLERANCE: f64 = 1e-6;
/// A search along a bone stops once its interval is shorter than this, in metres, a hundredth of the tolerance.
const RESOLUTION: f64 = 1e-8;
/// How far, in metres, a chain's tails can travel in one frame when the chain looks for furniture around it.
const TRAVEL: f64 = 0.25;

/// Refusals of a chain or collider whose declaration does not match the skeleton or the solver; each has a stable code,
/// which every host reporting it delegates to.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A chain names a bone the skeleton lacks, as a stale bake would.
    #[error("chain {chain} names bone {bone}, which the skeleton lacks; rebake")]
    MissingBone {
        /// The chain's root bone, which names it.
        chain: BoneName,
        /// The bone the skeleton lacks.
        bone: BoneName,
    },
    /// A collider names a bone the skeleton lacks.
    #[error("a collider names bone {bone}, which the skeleton lacks; rebake")]
    MissingColliderBone {
        /// The bone the skeleton lacks.
        bone: BoneName,
    },
    /// A chain's joint box has a frame that cannot be normalised, a zero or non-finite quaternion.
    #[error("chain {chain} holds bone {bone} in a joint box whose frame is not a turn")]
    InvalidFrame {
        /// The chain's root bone, which names it.
        chain: BoneName,
        /// The bone the box holds.
        bone: BoneName,
    },
    /// A chain names a collider the solver was not given.
    #[error("chain {chain} names collider {index}, which the solver lacks")]
    MissingCollider {
        /// The chain's root bone, which names it.
        chain: BoneName,
        /// The collider's place in the order the solver was given them.
        index: usize,
    },
}

impl Error {
    /// The refusal's stable code, which the host rendering it prints once.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MissingBone { .. } => "springs_missing_bone",
            Self::MissingColliderBone { .. } => "springs_missing_collider_bone",
            Self::MissingCollider { .. } => "springs_missing_collider",
            Self::InvalidFrame { .. } => "springs_invalid_frame",
        }
    }
}
