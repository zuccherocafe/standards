//! The springs as a host uses them, through the public solver. Its uses, most common first, and where each is stated:
//!
//! 1. A host steps a character's chains every frame from the clip's pose and draws them: a chain trails its turning
//!    parent and settles where the clip points it, whatever the display's rate. Here; that the path is alike at
//!    every rate, through a host's own time, resets and hitches, in `src/solver/tests.rs`.
//! 2. A chain keeps within its joint boxes and out of its bone colliders: `src/limit.rs` and
//!    `src/solver/tests.rs`.
//! 3. A bone meets furniture and stays out of it: `src/contact/tests.rs` and `src/solver/tests/contact.rs`.
//! 4. Friction holds a bone on a surface it rests on: `src/contact/friction/tests.rs` and
//!    `src/solver/tests/friction.rs`.
//! 5. A warm step allocates nothing: `tests/allocation.rs`.

use std::f64::consts::PI;

use glam::{DAffine3, DQuat, DVec3};
use tdoll_springs::chain::ChainSpec;
use tdoll_springs::furniture::Furniture;
use tdoll_springs::joint::Feel;
use tdoll_springs::pose::Pose;
use tdoll_springs::{Bone, BoneIndex, Frame, Poses, Springs, Unmeasured};
use tdoll_vocab::{BoneName, Length, Seconds};

/// A trailing tail this many degrees behind where its head points it is one a person sees lag.
const LAG_SEEN_DEGREES: f64 = 1.0;
/// The lag is read once the turn has run this long, past the moment the head starts, when nothing has lagged yet.
const TURN_UNDER_WAY: f64 = 0.25;
/// A tail moving slower than this, in metres a second, looks still: a millimetre over half a second.
const STILL_METRES_PER_SECOND: f64 = 0.002;
/// The chain looks still this long after the head stops, as a ponytail's swing dies down within a couple of seconds.
const SETTLED_WITHIN: f64 = 2.0;
/// Once still, the tail rests this near, in degrees, to where the clip points it.
const REST_DEGREES: f64 = 0.5;

/// The head's turn rate, in degrees a second, a brisk look to one side, and how long it turns.
const TURN_DEGREES_PER_SECOND: f64 = 120.0;
const TURN_SECONDS: f64 = 0.75;

/// A head 1.6 m up turning about the vertical, and a three-bone ponytail held out straight behind it by its clip.
struct Head {
    bones: Vec<Bone>,
    yaw: f64,
}

impl Head {
    fn new() -> Self {
        let bone = |name: &str, parent: Option<usize>, at: DVec3| Bone {
            name: BoneName::new(name).ok(),
            parent,
            rest: DAffine3::from_translation(at),
        };
        Self {
            bones: vec![
                bone("root", None, DVec3::ZERO),
                bone("head", Some(0), DVec3::new(0.0, 1.6, 0.0)),
                bone("tail_1", Some(1), DVec3::new(0.0, 0.0, -0.08)),
                bone("tail_2", Some(2), DVec3::new(0.0, 0.0, -0.1)),
                bone("tail_3", Some(3), DVec3::new(0.0, 0.0, -0.1)),
            ],
            yaw: 0.0,
        }
    }

    fn local_of(&self, bone: usize) -> Pose {
        let rest = self
            .bones
            .get(bone)
            .map_or(DAffine3::IDENTITY, |bone| bone.rest);
        Pose {
            translation: rest.translation,
            rotation: if bone == 1 {
                DQuat::from_rotation_y(self.yaw)
            } else {
                DQuat::IDENTITY
            },
            scale: DVec3::ONE,
        }
    }

    fn world_of(&self, bone: Option<usize>) -> DAffine3 {
        let Some(index) = bone else {
            return DAffine3::IDENTITY;
        };
        let parent = self.bones.get(index).and_then(|bone| bone.parent);
        self.world_of(parent) * self.local_of(index).affine()
    }
}

impl Poses for Head {
    fn local(&self, bone: BoneIndex) -> Pose {
        self.local_of(bone.index())
    }

    fn world(&self, bone: Option<BoneIndex>) -> DAffine3 {
        self.world_of(bone.map(BoneIndex::index))
    }
}

/// `angle` wrapped into a half turn either side of zero.
fn wrapped(angle: f64) -> f64 {
    (angle + PI).rem_euclid(2.0 * PI) - PI
}

/// One run at `hz` frames a second: the head still for a second, turning for [`TURN_SECONDS`], then held for
/// four. Each frame's time since the turn began, the tail's lag behind where the clip points it in degrees, positive
/// trailing the turn, and how fast the tail moved over the frame.
fn run(hz: u32) -> Vec<(f64, f64, f64)> {
    let mut head = Head::new();
    let mut springs = Springs::new(head.bones.clone(), tdoll_springs::RATE);
    let name = |text: &str| BoneName::new(text).expect("a bone's name is not empty");
    springs
        .add_chain(&ChainSpec {
            bones: vec![name("tail_1"), name("tail_2"), name("tail_3")],
            tail: Length::new(0.1).expect("a length"),
            feel: Feel {
                stiffness: 0.5,
                drag: 0.4,
                gravity: 0.0,
                radius: Length::new(0.01).expect("a length"),
                cushion: tdoll_springs::CUSHION,
                stiffness_tip: 1.0,
                drag_tip: 1.0,
                gravity_root: 1.0,
                friction: 0.0,
            },
            limits: Vec::new(),
            colliders: Vec::new(),
        })
        .expect("the ponytail's bones exist");
    let delta = 1.0 / f64::from(hz);
    // A second still, the turn and four seconds held: 5.75 seconds.
    let frames = hz * 23 / 4;
    let mut out = Vec::new();
    let mut before: Option<DVec3> = None;
    for frame in 0..frames {
        let since = f64::from(frame + 1) * delta - 1.0;
        head.yaw = (since.clamp(0.0, TURN_SECONDS) * TURN_DEGREES_PER_SECOND).to_radians();
        let step = Seconds::new(delta).expect("a frame's length is finite");
        if let Frame::Step { .. } = springs.capture(step, &head) {
            springs.advance(&Furniture::default(), &mut Unmeasured);
        }
        let tail = springs.joints().last().expect("the chain has bones").tail();
        let offset = tail - head.world_of(Some(1)).translation;
        // Behind the head, -Z, turned by the yaw, lies at azimuth yaw + 180 degrees about the vertical.
        let lag = wrapped(head.yaw + PI - offset.x.atan2(offset.z)).to_degrees();
        let speed = before.map_or(0.0, |before| tail.distance(before) / delta);
        before = Some(tail);
        out.push((since, lag, speed));
    }
    out
}

#[test]
fn a_ponytail_trails_its_turning_head_and_then_settles_where_the_clip_points_it_at_60_and_144_hz() {
    for hz in [60, 144] {
        let frames = run(hz);
        for (since, lag, _) in &frames {
            if (TURN_UNDER_WAY..=TURN_SECONDS).contains(since) {
                assert!(
                    *lag >= LAG_SEEN_DEGREES,
                    "{hz} Hz, {since:.3} s into the turn: {lag:.3} degrees behind"
                );
            }
        }
        let held = frames
            .iter()
            .filter(|(since, ..)| *since >= TURN_SECONDS + SETTLED_WITHIN);
        for (since, lag, speed) in held {
            assert!(
                *speed <= STILL_METRES_PER_SECOND,
                "{hz} Hz, {since:.3} s: the tail moving at {speed} m/s"
            );
            assert!(
                lag.abs() <= REST_DEGREES,
                "{hz} Hz, {since:.3} s: resting {lag:.3} degrees off"
            );
        }
    }
}
