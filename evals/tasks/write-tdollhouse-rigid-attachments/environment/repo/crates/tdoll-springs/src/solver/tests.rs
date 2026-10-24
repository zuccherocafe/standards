use std::f64::consts::TAU;

use glam::{DAffine3, DQuat, DVec3};
use tdoll_vocab::{Length, Radians, Rate};

use super::*;
use crate::collider::ColliderShape;
use crate::furniture::{Contact, Placed, Shape, Unmeasured};
use crate::joint::Feel;
use crate::limit::{AxisLimit, Limit};
use crate::pose::{rotation_of, smoothstep};

const STEP: f64 = 1.0 / 60.0;

/// A bone's name, written in a test.
pub(super) fn name(text: &str) -> BoneName {
    BoneName::new(text).unwrap()
}

/// A duration, written in a test.
pub(super) fn seconds(value: f64) -> Seconds {
    Seconds::new(value).unwrap()
}

/// A length, written in a test.
pub(super) fn length(metres: f64) -> Length {
    Length::new(metres).unwrap()
}

/// An angle, written in a test.
pub(super) fn angle(radians: f64) -> Radians {
    Radians::new(radians).unwrap()
}

/// A box on the bone `bone` holding each axis from `low` to `high`.
pub(super) fn held(bone: &str, frame: DQuat, reversed: bool, low: DVec3, high: DVec3) -> Limit {
    let axis = |index: usize| AxisLimit::Held {
        min: angle(low[index]),
        max: angle(high[index]),
    };
    Limit {
        bone: name(bone),
        frame,
        reversed,
        axes: [axis(0), axis(1), axis(2)],
    }
}

/// A made-up skeleton: bones with rests and the poses a clip leaves them in, placed in the world.
pub(super) struct Rig {
    pub(super) bones: Vec<Bone>,
    pub(super) poses: Vec<Pose>,
    pub(super) placement: DAffine3,
}

impl Rig {
    /// A skeleton of `rests` given as (name, parent, translation), each at rest.
    pub(super) fn new(rests: &[(&str, Option<usize>, DVec3)]) -> Self {
        let bones: Vec<Bone> = rests
            .iter()
            .map(|(called, parent, at)| Bone {
                name: Some(name(called)),
                parent: *parent,
                rest: DAffine3::from_translation(*at),
            })
            .collect();
        let poses = bones
            .iter()
            .map(|bone| Pose {
                translation: bone.rest.translation,
                rotation: rotation_of(bone.rest.matrix3),
                scale: DVec3::ONE,
            })
            .collect();
        Self {
            bones,
            poses,
            placement: DAffine3::IDENTITY,
        }
    }

    pub(super) fn turn(&mut self, bone: usize, rotation: DQuat) {
        if let Some(pose) = self.poses.get_mut(bone) {
            pose.rotation = rotation;
        }
    }
}

impl Rig {
    /// Bone `bone`'s pose relative to its parent.
    pub(super) fn local(&self, bone: usize) -> Pose {
        self.poses.get(bone).copied().unwrap_or(Pose::IDENTITY)
    }

    /// Bone `bone`'s placement in the world, or the skeleton's own for none.
    pub(super) fn world(&self, bone: Option<usize>) -> DAffine3 {
        let Some(index) = bone else {
            return self.placement;
        };
        let parent = self.bones.get(index).and_then(|bone| bone.parent);
        Self::world(self, parent) * Self::local(self, index).affine()
    }
}

impl Poses for Rig {
    fn local(&self, bone: BoneIndex) -> Pose {
        Self::local(self, bone.index())
    }

    fn world(&self, bone: Option<BoneIndex>) -> DAffine3 {
        Self::world(self, bone.map(BoneIndex::index))
    }
}

/// The feel `_rig` gave its one-bone chain: a family setting only gravity and the cushion.
fn feel(gravity: f64, cushion: f64) -> Feel {
    Feel {
        stiffness: 0.2,
        drag: 0.3,
        gravity,
        radius: length(0.01),
        cushion,
        stiffness_tip: 1.0,
        drag_tip: 1.0,
        gravity_root: 1.0,
        friction: crate::FRICTION,
    }
}

/// How a test hangs its one-bone chain: its joint boxes, cushion, colliders on the parent, how high above the parent
/// the bone stands and its gravity.
struct Setup {
    limits: Vec<Limit>,
    cushion: f64,
    colliders: Vec<ColliderSpec>,
    height: f64,
    gravity: f64,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            limits: Vec::new(),
            cushion: crate::CUSHION,
            colliders: Vec::new(),
            height: 1.0,
            gravity: 2.0,
        }
    }
}

/// A made-up skeleton and the solver over it.
struct Hung {
    rig: Rig,
    springs: Springs,
}

impl Hung {
    /// One frame `delta` long with `furniture` around, the drawn rotations written back as the skeleton would.
    fn frame(&mut self, delta: f64, furniture: &Furniture, measured: &mut dyn Measured) {
        if let Frame::Step { .. } = self.springs.capture(seconds(delta), &self.rig) {
            self.springs.advance(furniture, measured);
        }
        let Self { rig, springs } = self;
        for (bone, turn) in springs.drawn() {
            rig.turn(bone.index(), turn);
        }
    }

    /// One frame with the bone and tip as the clip leaves them, at rest; returns the bone's drawn turn from rest.
    fn step(&mut self, delta: f64) -> DQuat {
        self.rig.turn(1, DQuat::IDENTITY);
        self.rig.turn(2, DQuat::IDENTITY);
        self.frame(delta, &Furniture::default(), &mut Unmeasured);
        self.rig.local(1).rotation
    }

    /// Where each simulated tail lies.
    fn tails(&self) -> Vec<DVec3> {
        self.springs
            .joints()
            .iter()
            .map(|joint| joint.tail)
            .collect()
    }
}

/// A skeleton whose middle bone stands above its parent as `setup` says, a one-bone chain 0.1 m long.
fn one_bone(setup: &Setup) -> Hung {
    let rig = Rig::new(&[
        ("parent", None, DVec3::ZERO),
        ("bone", Some(0), DVec3::Y * setup.height),
        ("tip", Some(1), DVec3::Y * -0.1),
    ]);
    let mut springs = Springs::new(rig.bones.clone(), crate::RATE);
    let indices: Vec<ColliderId> = setup
        .colliders
        .iter()
        .map(|spec| springs.add_collider(spec).unwrap())
        .collect();
    let chain = ChainSpec {
        bones: vec![name("bone"), name("tip")],
        tail: length(0.05),
        feel: feel(setup.gravity, setup.cushion),
        limits: setup.limits.clone(),
        colliders: indices,
    };
    springs.add_chain(&chain).unwrap();
    Hung { rig, springs }
}

fn apart(a: DQuat, b: DQuat) -> f64 {
    let dot = a.normalize().dot(b.normalize());
    (dot * dot * 2.0 - 1.0).clamp(-1.0, 1.0).acos()
}

fn cube(limit: f64) -> Limit {
    held(
        "bone",
        DQuat::IDENTITY,
        false,
        DVec3::splat(-limit),
        DVec3::splat(limit),
    )
}

#[test]
fn a_full_turn_of_the_parent_never_takes_the_bone_past_its_limit_nor_makes_it_jump() {
    for cushion in [0.0, 0.7] {
        let limits = [cube(0.3)];
        let mut hung = one_bone(&Setup {
            limits: limits.to_vec(),
            cushion,
            height: 0.3,
            gravity: 0.0,
            ..Setup::default()
        });
        let mut previous = DQuat::IDENTITY;
        for index in 0..120 {
            hung.rig.turn(
                0,
                DQuat::from_rotation_z(TAU * smoothstep(f64::from(index) / 120.0)),
            );
            let turn = hung.step(STEP);
            assert!(
                cube(0.3).joint().unwrap().excess(turn).get() <= 1e-4,
                "cushion {cushion}: past its limit at {index}"
            );
            assert!(
                index == 0 || apart(turn, previous) <= 0.1,
                "cushion {cushion}: jumps at {index}"
            );
            previous = turn;
        }
    }
}

#[test]
fn one_swing_drawn_at_60_90_and_144_hz_moves_the_spring_alike() {
    let mut paths: Vec<Vec<DQuat>> = Vec::new();
    for rate in [60_u32, 90, 144] {
        let mut hung = one_bone(&Setup {
            gravity: 2.5,
            ..Setup::default()
        });
        let mut path = Vec::new();
        for frame in 0..rate * 2 {
            let time = f64::from(frame + 1) / f64::from(rate);
            hung.rig
                .turn(0, DQuat::from_rotation_z(0.8 * smoothstep(time / 0.25)));
            let turn = hung.step(1.0 / f64::from(rate));
            if (frame + 1) % (rate / 6) == 0 {
                path.push(turn);
            }
        }
        paths.push(path);
    }
    // A degree apart at the same moment would read as a different feel on another screen.
    for path in paths.iter().skip(1) {
        for (at_60, other) in paths.first().into_iter().flatten().zip(path) {
            assert!(apart(*at_60, *other) <= 1.0_f64.to_radians());
        }
    }
}

/// A chain of `links` 0.1 m links hanging from a parent 1 m up, its tail `tail`, stepped `rate` times a second.
fn hanging(links: usize, tail: f64, gravity: f64, rate: Rate) -> Hung {
    let names: Vec<String> = (0..links).map(|link| format!("link{link}")).collect();
    let mut rests = vec![("parent", None, DVec3::ZERO)];
    for (link, name) in names.iter().enumerate() {
        rests.push((
            name.as_str(),
            Some(link),
            if link == 0 { DVec3::Y } else { DVec3::Y * -0.1 },
        ));
    }
    let rig = Rig::new(&rests);
    let mut springs = Springs::new(rig.bones.clone(), rate);
    let spec = ChainSpec {
        bones: names.iter().map(|bone| name(bone)).collect(),
        tail: length(tail),
        feel: Feel {
            stiffness: 0.8,
            ..feel(gravity, crate::CUSHION)
        },
        limits: Vec::new(),
        colliders: Vec::new(),
    };
    springs.add_chain(&spec).unwrap();
    Hung { rig, springs }
}

#[test]
fn a_swaying_travelling_chain_takes_the_same_path_at_60_120_and_240_steps_a_second() {
    let mut paths = Vec::new();
    for rate in [60, 120, 240].map(|hz| Rate::new(hz).unwrap()) {
        let mut hung = hanging(9, 0.1, 2.5, rate);
        let mut path = Vec::new();
        for frame in 0..240 {
            let time = f64::from(frame) / 60.0;
            hung.rig.placement =
                DAffine3::from_translation(DVec3::X * 0.3 * (TAU * 0.7 * time).sin());
            hung.rig
                .turn(0, DQuat::from_rotation_z(0.5 * (TAU * 0.5 * time).sin()));
            for link in 1..=9 {
                hung.rig.turn(link, DQuat::IDENTITY);
            }
            hung.frame(STEP, &Furniture::default(), &mut Unmeasured);
            path.push(hung.tails().last().copied().unwrap_or_default());
        }
        paths.push(path);
    }
    // Apart by a tenth of the chain's 0.9 m, the same sway reads as a different swing.
    let finest = paths.last().cloned().unwrap_or_default();
    for path in paths.iter().take(2) {
        let apart = path
            .iter()
            .zip(&finest)
            .map(|(a, b)| a.distance(*b))
            .fold(0.0, f64::max);
        assert!(
            apart <= 0.09,
            "the tip strays {apart} m from its path at 240"
        );
    }
}

#[test]
fn a_chain_reports_the_link_its_passes_left_furthest_from_its_length_and_none_hanging_still() {
    let mut still = hanging(9, 0.1, 0.0, crate::RATE);
    let mut swaying = hanging(9, 0.1, 2.5, crate::RATE);
    for frame in 0..120 {
        let time = f64::from(frame) / 60.0;
        swaying.rig.placement =
            DAffine3::from_translation(DVec3::X * 0.3 * (TAU * 0.7 * time).sin());
        swaying
            .rig
            .turn(0, DQuat::from_rotation_z(0.5 * (TAU * 0.5 * time).sin()));
        for hung in [&mut still, &mut swaying] {
            for link in 1..=9 {
                hung.rig.turn(link, DQuat::IDENTITY);
            }
            hung.frame(STEP, &Furniture::default(), &mut Unmeasured);
        }
    }
    let still = still.springs.link_stretch().unwrap();
    let rest = still.metres.get();
    assert!(
        rest < 1e-9,
        "a chain hanging still left a link {rest} m off"
    );
    assert_eq!(still.bone.index(), 1);
    let moving = swaying.springs.link_stretch().unwrap().metres.get();
    assert!(
        moving > 1e-5 && moving < 0.01,
        "a swaying chain's passes left a link {moving} m off"
    );
}

#[test]
fn a_host_that_advanced_its_animation_by_other_time_than_the_frames_has_the_springs_step_through_its_time()
 {
    let mut tails = Vec::new();
    for followed in [true, false] {
        let mut hung = one_bone(&Setup::default());
        for frame in 0..30 {
            hung.rig
                .turn(0, DQuat::from_rotation_z(f64::from(frame) * 0.03));
            if followed {
                hung.springs.follow(seconds(STEP * 2.0));
            }
            hung.step(if followed { STEP } else { STEP * 2.0 });
        }
        tails.push(hung.tails());
    }
    assert!(
        tails
            .first()
            .zip(tails.last())
            .is_some_and(|(one, other)| one == other)
    );
}

#[test]
fn once_followed_a_frame_handed_no_time_takes_no_step_and_draws_the_held_pose() {
    let mut hung = one_bone(&Setup::default());
    hung.rig.turn(0, DQuat::from_rotation_z(1.0));
    for _ in 0..10 {
        hung.springs.follow(seconds(STEP));
        hung.step(STEP);
    }
    let tails = hung.tails();
    let drawn: Vec<(BoneIndex, DQuat)> = hung.springs.drawn().collect();
    hung.rig.turn(1, DQuat::from_rotation_x(0.5));
    assert_eq!(hung.springs.capture(seconds(STEP), &hung.rig), Frame::Still);
    assert_eq!(hung.tails(), tails);
    assert_eq!(drawn.len(), 2);
    assert_eq!(hung.springs.drawn().collect::<Vec<_>>(), drawn);
}

#[test]
fn a_reset_solver_starts_again_from_the_clips_pose_as_a_new_one_with_no_time_kept() {
    let mut reset = one_bone(&Setup::default());
    reset.rig.turn(0, DQuat::from_rotation_z(1.0));
    for _ in 0..20 {
        reset.springs.follow(seconds(STEP));
        reset.step(STEP);
    }
    reset.springs.follow(seconds(2.0));
    reset.springs.reset();
    let mut fresh = one_bone(&Setup::default());
    for hung in [&mut reset, &mut fresh] {
        hung.rig.placement = DAffine3::from_translation(DVec3::X * 0.5);
        hung.rig.turn(0, DQuat::IDENTITY);
        for _ in 0..3 {
            hung.springs.follow(seconds(STEP));
            hung.step(STEP);
        }
    }
    // Time the reset solver was owed would have stepped it further than the fresh one.
    assert_eq!(reset.tails(), fresh.tails());
}

/// A fork hanging from a body: `upper` and `lower` one chain, `twig` a second chain hanging from `upper`'s simulated
/// placement, and a ball on the body just beside where the twig's tail hangs.
fn fork() -> Hung {
    let rig = Rig::new(&[
        ("body", None, DVec3::ZERO),
        ("upper", Some(0), DVec3::Y),
        ("lower", Some(1), DVec3::new(0.05, -0.1, 0.0)),
        ("twig", Some(1), DVec3::new(-0.05, -0.1, 0.0)),
    ]);
    let mut springs = Springs::new(rig.bones.clone(), crate::RATE);
    let body = springs
        .add_collider(&ColliderSpec::new(
            name("body"),
            DQuat::IDENTITY,
            DVec3::new(-0.11, 0.86, 0.0),
            ColliderShape::Sphere {
                radius: length(0.03),
            },
        ))
        .unwrap();
    for bones in [vec!["upper", "lower"], vec!["twig"]] {
        let chain = ChainSpec {
            bones: bones.iter().map(|bone| name(bone)).collect(),
            tail: length(0.06),
            feel: feel(2.0, crate::CUSHION),
            limits: Vec::new(),
            colliders: vec![body],
        };
        springs.add_chain(&chain).unwrap();
    }
    Hung { rig, springs }
}

/// One followed frame of the fork with its links as the clip leaves them, at rest. With `first`, the frame is the first
/// since the solver was made or reset, and each chain must be sensed where its first bone stands in the clip's
/// present pose, the furniture area covering every bone, whatever stood there before.
fn fork_frame(hung: &mut Hung, first: bool) {
    for bone in 1..=3 {
        hung.rig.turn(bone, DQuat::IDENTITY);
    }
    let origins: Vec<DVec3> = hung
        .springs
        .joints
        .iter()
        .map(|joint| hung.rig.world(Some(joint.bone.index())).translation)
        .collect();
    let roots: Vec<DVec3> = hung
        .springs
        .chains
        .iter()
        .filter_map(|chain| origins.get(chain.joints.start).copied())
        .collect();
    hung.springs.follow(seconds(STEP));
    hung.frame(STEP, &Furniture::default(), &mut Unmeasured);
    if !first {
        return;
    }
    for (chain, at) in hung.springs.chains.iter().zip(&roots) {
        assert!(
            chain.root.distance(*at) < 1e-9,
            "a chain was sensed at {} with its first bone at {at}",
            chain.root
        );
    }
    let area = hung
        .springs
        .area
        .unwrap_or(Bounds::around(DVec3::ZERO, 0.0));
    assert!(
        origins.iter().all(|at| area.has_point(*at)),
        "the furniture area {area:?} leaves out a bone of the present pose"
    );
}

#[test]
fn a_reset_fork_moved_while_hidden_senses_and_steps_from_its_present_pose_as_a_new_one() {
    let mut reset = fork();
    reset.rig.turn(0, DQuat::from_rotation_z(0.6));
    for _ in 0..20 {
        reset.springs.follow(seconds(STEP));
        reset.frame(STEP, &Furniture::default(), &mut Unmeasured);
    }
    reset.springs.reset();
    let mut fresh = fork();
    for hung in [&mut reset, &mut fresh] {
        hung.rig.placement = DAffine3::from_translation(DVec3::new(0.5, 0.2, -0.3));
        hung.rig.turn(0, DQuat::IDENTITY);
    }
    // Later frames hang the twig from its parent's simulated placement, so the reset fork is compared with the new one.
    for frame in 0..4 {
        fork_frame(&mut reset, frame == 0);
        fork_frame(&mut fresh, frame == 0);
        let touching = |hung: &Hung| -> Vec<Vec<ColliderId>> {
            hung.springs
                .chains
                .iter()
                .map(|chain| chain.touching.clone())
                .collect()
        };
        assert_eq!(touching(&reset), touching(&fresh), "frame {frame}");
        assert_eq!(reset.springs.area, fresh.springs.area, "frame {frame}");
        assert_eq!(reset.tails(), fresh.tails(), "frame {frame}");
        let drawn = |hung: &Hung| -> Vec<(BoneIndex, DQuat)> { hung.springs.drawn().collect() };
        assert_eq!(drawn(&reset), drawn(&fresh), "frame {frame}");
    }
}

#[test]
fn a_hitch_takes_four_steps_and_drops_the_rest() {
    let mut hitched = one_bone(&Setup::default());
    let mut paced = one_bone(&Setup::default());
    // The parent held turned, so the bone swings down under gravity and every step moves it.
    hitched.rig.turn(0, DQuat::from_rotation_z(1.0));
    paced.rig.turn(0, DQuat::from_rotation_z(1.0));
    hitched.step(STEP);
    paced.step(STEP);
    hitched.step(2.0);
    for _ in 0..4 {
        paced.step(crate::RATE.time_of(1).get());
    }
    for (one, other) in hitched.tails().iter().zip(paced.tails()) {
        assert!(
            one.distance(other) < 1e-12,
            "a 2 s frame stepped other than four steps"
        );
    }
    // The rest of the hitch is dropped, not owed: a frame of no time after it takes no step.
    let before = hitched.tails();
    hitched.step(0.0);
    assert_eq!(
        hitched.tails(),
        before,
        "the hitch's dropped time was stepped later"
    );
}

#[test]
fn a_locked_axis_stays_at_rest_whatever_the_parent_does_named_either_way_round() {
    for reversed in [false, true] {
        let limits = [held(
            "bone",
            DQuat::from_axis_angle(DVec3::new(0.2, 0.9, 0.1).normalize(), 0.6),
            reversed,
            DVec3::new(-0.7, 0.0, -0.7),
            DVec3::new(0.7, 0.0, 0.7),
        )];
        let mut hung = one_bone(&Setup {
            limits: limits.to_vec(),
            height: 0.1,
            ..Setup::default()
        });
        let mut worst = 0.0_f64;
        for index in 0..240 {
            let axis = DVec3::new(1.0, 0.5, 0.3).normalize();
            hung.rig.turn(
                0,
                DQuat::from_axis_angle(axis, (f64::from(index) * 0.08).sin() * 1.2),
            );
            let turn = hung.step(STEP);
            worst = worst.max(
                limits
                    .iter()
                    .map(|limit| limit.joint().unwrap().angles(turn).y.abs())
                    .fold(0.0, f64::max),
            );
        }
        assert!(
            worst <= 1e-4,
            "reversed {reversed}: a locked axis moves {worst} rad"
        );
    }
}

/// A ball of 3 cm on the parent at `at`.
fn ball(at: DVec3) -> ColliderSpec {
    ColliderSpec::new(
        name("parent"),
        DQuat::IDENTITY,
        at,
        ColliderShape::Sphere {
            radius: length(0.03),
        },
    )
}

/// How far the bone's tail, as the frame drew it, is from the ball at `at`.
fn tail_to_ball(rig: &Rig, at: DVec3) -> f64 {
    let tip = rig.world(Some(1)).transform_point3(DVec3::Y * -0.1);
    tip.distance(rig.world(Some(0)).transform_point3(at))
}

#[test]
fn a_tail_falling_onto_a_collider_keeps_its_radius_and_the_bones_away() {
    let at = DVec3::new(0.02, 0.9, 0.0);
    let colliders = [ball(at)];
    let mut hung = one_bone(&Setup {
        colliders: colliders.to_vec(),
        ..Setup::default()
    });
    let mut nearest = f64::INFINITY;
    for index in 0..240 {
        hung.rig.turn(
            0,
            DQuat::from_rotation_z((f64::from(index) * 0.02).min(1.2)),
        );
        hung.step(STEP);
        nearest = nearest.min(tail_to_ball(&hung.rig, at));
    }
    assert!(
        nearest >= 0.03 + 0.01 - 1e-4,
        "the tail came {nearest} m from the collider"
    );
}

#[test]
fn a_collider_only_a_locked_axis_would_clear_is_cleared_by_a_turn_the_box_allows() {
    let limits = [held(
        "bone",
        DQuat::IDENTITY,
        false,
        DVec3::new(-0.5, -0.5, 0.0),
        DVec3::new(0.5, 0.5, 0.0),
    )];
    for across in [0.01, 0.02, 0.03, 0.035] {
        let at = DVec3::new(across, 0.9, 0.0);
        let colliders = [ball(at)];
        let mut hung = one_bone(&Setup {
            limits: limits.to_vec(),
            cushion: 0.0,
            colliders: colliders.to_vec(),
            ..Setup::default()
        });
        for _ in 0..120 {
            hung.step(STEP);
        }
        assert!(
            tail_to_ball(&hung.rig, at) >= 0.03 + 0.01 - 1e-3,
            "{across} m across: the tail stayed in"
        );
        assert!(
            hung.springs
                .deepest_in_colliders()
                .is_some_and(|worst| worst.metres.get() <= 1e-3 && worst.bone.index() == 1)
        );
    }
}

#[test]
fn a_bone_lowered_onto_a_thin_board_at_walking_and_falling_speed_stays_on_top_at_its_length() {
    let board = Furniture {
        shapes: vec![Placed {
            placement: DAffine3::from_translation(DVec3::X * 0.02),
            shape: Shape::Box {
                half: DVec3::new(0.5, 0.01, 0.5),
            },
            id: 1,
        }],
        measured: false,
    };
    for speed in [2.0, 0.3] {
        let mut hung = one_bone(&Setup {
            height: 0.0,
            ..Setup::default()
        });
        let mut lowest = f64::INFINITY;
        for index in 0..120 {
            let height = (0.4 - f64::from(index) * speed / 60.0).max(0.06);
            hung.rig.placement = DAffine3::from_translation(DVec3::Y * height);
            hung.rig.turn(1, DQuat::IDENTITY);
            hung.rig.turn(2, DQuat::IDENTITY);
            hung.frame(STEP, &board, &mut Unmeasured);
            let tail = hung.tails().first().copied().unwrap_or_default();
            lowest = lowest.min(tail.y);
            let length = tail.distance(hung.rig.world(Some(1)).translation);
            assert!(
                (length - 0.1).abs() <= 1e-4,
                "the simulated tail is {length} m from its bone's origin"
            );
        }
        assert!(
            lowest >= 0.01 + 0.01 - 0.001,
            "lowered at {speed} m/s, the tail went through to {lowest}"
        );
    }
}

#[test]
fn a_shape_that_appears_or_jumps_onto_a_tail_puts_it_out() {
    let mut hung = one_bone(&Setup::default());
    let sphere = |at: DVec3| Furniture {
        shapes: vec![Placed {
            placement: DAffine3::from_translation(at),
            shape: Shape::Sphere { radius: 0.05 },
            id: 7,
        }],
        measured: false,
    };
    // A ball 5 cm across appears centred on the tail: the next frame the tail lies outside it.
    let tail = *hung.tails().first().unwrap();
    hung.frame(STEP, &sphere(tail), &mut Unmeasured);
    let out = *hung.tails().first().unwrap();
    assert!(
        out.distance(tail) >= 0.05 - 1e-3,
        "the tail stays {} m from the ball's centre",
        out.distance(tail)
    );
    // It jumps to wherever the tail now is: the tail comes out again.
    hung.frame(STEP, &sphere(out), &mut Unmeasured);
    let again = *hung.tails().first().unwrap();
    assert!(
        again.distance(out) >= 0.05 - 1e-3,
        "the tail stays {} m from the ball's centre",
        again.distance(out)
    );
}

/// Furniture only the host measures: the floor y = `height`, every query it answers counted.
struct Floor {
    height: f64,
    queries: usize,
}

impl Measured for Floor {
    fn within(&mut self, centre: DVec3, reach: f64) -> bool {
        self.queries += 1;
        centre.y - self.height <= reach
    }

    fn rest(&mut self, point: DVec3, radius: f64) -> Option<Contact> {
        self.queries += 1;
        let contact = Contact {
            point: DVec3::new(point.x, self.height, point.z),
            normal: DVec3::Y,
        };
        (point.y - self.height <= radius).then_some(contact)
    }

    fn cast(&mut self, start: DVec3, motion: DVec3, radius: f64) -> f64 {
        self.queries += 1;
        if motion.y >= 0.0 {
            return 1.0;
        }
        ((start.y - self.height - radius) / -motion.y).clamp(0.0, 1.0)
    }
}

#[test]
fn measured_furniture_is_asked_about_only_for_the_bones_within_a_frames_travel_of_it() {
    let around = Furniture {
        shapes: Vec::new(),
        measured: true,
    };
    let mut floor = Floor {
        height: 0.2,
        queries: 0,
    };
    let mut hung = hanging(6, 0.05, 2.0, crate::RATE);
    for _ in 0..30 {
        for link in 1..=6 {
            hung.rig.turn(link, DQuat::IDENTITY);
        }
        hung.frame(STEP, &around, &mut floor);
    }
    let asks: Vec<bool> = hung
        .springs
        .joints()
        .iter()
        .map(|joint| joint.asks)
        .collect();
    assert_eq!(
        asks.first(),
        Some(&false),
        "the highest bone, 0.8 m above the floor, asks about it"
    );
    assert_eq!(
        asks.last(),
        Some(&true),
        "the lowest bone, 0.3 m above the floor, does not ask about it"
    );

    let mut unasked = Floor {
        height: 0.2,
        queries: 0,
    };
    hanging(6, 0.05, 2.0, crate::RATE).frame(STEP, &Furniture::default(), &mut unasked);
    assert_eq!(
        unasked.queries, 0,
        "with no measured furniture around, the host is never asked"
    );
}

#[test]
fn a_chain_naming_a_bone_the_skeleton_lacks_refuses_with_its_name_and_a_bone_with_no_length_is_skipped()
 {
    let rig = Rig::new(&[
        ("parent", None, DVec3::ZERO),
        ("bone", Some(0), DVec3::Y),
        ("mid", Some(1), DVec3::ZERO),
        ("tip", Some(2), DVec3::Y * -0.1),
    ]);
    let mut springs = Springs::new(rig.bones.clone(), crate::RATE);
    let mut spec = ChainSpec {
        bones: vec![name("bone"), name("gone")],
        tail: length(0.05),
        feel: feel(0.0, crate::CUSHION),
        limits: Vec::new(),
        colliders: Vec::new(),
    };
    let refusal = springs.add_chain(&spec).unwrap_err();
    assert_eq!(refusal.code(), "springs_missing_bone");
    assert!(
        refusal
            .to_string()
            .contains("chain bone names bone gone, which the skeleton lacks"),
        "{refusal}"
    );
    assert_eq!(springs.chain_count(), 0);

    spec.bones = vec![name("bone"), name("mid"), name("tip")];
    assert_eq!(
        springs.collider(&name("bone"), 0).unwrap_err().code(),
        "springs_missing_collider"
    );
    springs.add_chain(&spec).unwrap();
    // `bone` has no length, its child `mid` sitting where it does: the chain moves `mid` and `tip` alone.
    let simulated: Vec<usize> = springs
        .joints()
        .iter()
        .map(|joint| joint.bone().index())
        .collect();
    assert_eq!(springs.chain_count(), 1);
    assert_eq!(simulated, [2, 3], "mid and tip, the third and fourth bones");
    let missing = ColliderSpec::new(
        name("nowhere"),
        DQuat::IDENTITY,
        DVec3::ZERO,
        ColliderShape::Sphere {
            radius: length(0.03),
        },
    );
    assert_eq!(
        springs.add_collider(&missing).unwrap_err().code(),
        "springs_missing_collider_bone"
    );
    let ball = springs.add_collider(&ball(DVec3::ZERO)).unwrap();
    assert_eq!(springs.collider(&name("bone"), 0).unwrap(), ball);
}

#[test]
fn a_chain_hanging_from_another_chains_bone_follows_that_bones_simulated_placement() {
    let rig = Rig::new(&[
        ("parent", None, DVec3::ZERO),
        ("upper", Some(0), DVec3::Y),
        ("lower", Some(1), DVec3::Y * -0.1),
        ("end", Some(2), DVec3::Y * -0.1),
    ]);
    let mut springs = Springs::new(rig.bones.clone(), crate::RATE);
    let chain = |bones: &[&str]| ChainSpec {
        bones: bones.iter().map(|bone| name(bone)).collect(),
        tail: length(0.05),
        feel: feel(2.0, crate::CUSHION),
        limits: Vec::new(),
        colliders: Vec::new(),
    };
    springs.add_chain(&chain(&["lower", "end"])).unwrap();
    springs.add_chain(&chain(&["upper"])).unwrap();
    let parents: Vec<Option<usize>> = springs
        .joints()
        .iter()
        .map(|joint| joint.parent.map(JointIndex::index))
        .collect();
    assert_eq!(
        parents,
        [Some(2), Some(0), None],
        "a chain declared first still hangs from one declared later"
    );
}

#[test]
fn a_tail_turned_into_plain_furniture_by_measured_furniture_meets_it_afresh() {
    // A tail resting on a floor only the host measures, under a plain board too low to clear both: each step the
    // floor turns it up into the board, so each step begins inside the board, which nothing about the shapes says.
    let mut floor = Floor {
        height: 0.92,
        queries: 0,
    };
    let board = Furniture {
        shapes: vec![Placed {
            placement: DAffine3::from_translation(DVec3::new(-0.15, 0.95, 0.0)),
            shape: Shape::Box {
                half: DVec3::new(0.1, 0.015, 0.1),
            },
            id: 1,
        }],
        measured: true,
    };
    let floor_only = Furniture {
        shapes: Vec::new(),
        measured: true,
    };
    let mut hung = hanging(1, 0.1, 2.0, crate::RATE);
    let mut caught = 0;
    let mut recovered = 0;
    for index in 0..240 {
        hung.rig.turn(1, DQuat::from_rotation_z(0.5));
        let around = if index < 60 { &floor_only } else { &board };
        hung.frame(crate::RATE.time_of(1).get(), around, &mut floor);
        let contacts = hung.springs.contacts();
        if index > 0 {
            recovered += contacts.recovered;
        }
        caught += contacts.caught;
        let tail = hung.tails().first().copied().unwrap_or_default();
        assert!(tail.is_finite(), "frame {index}: the tail is lost");
    }
    assert_eq!(
        recovered, 1,
        "only the board's arrival starts the bone again"
    );
    assert_eq!(
        caught, 179,
        "{caught} of the 179 steps after the board's arrival met it afresh"
    );
}

#[test]
fn records_number_every_step_once_and_a_frame_with_no_step_due_has_none() {
    let mut hung = hanging(2, 0.1, 2.0, crate::RATE);
    let around = Furniture {
        shapes: vec![Placed {
            placement: DAffine3::from_translation(DVec3::new(0.0, 0.75, 0.0)),
            shape: Shape::Box {
                half: DVec3::new(0.5, 0.05, 0.5),
            },
            id: 1,
        }],
        measured: false,
    };
    let mut seen: Vec<u64> = Vec::new();
    for index in 0..40 {
        // Frames of a sixtieth, then of a two-hundred-and-fortieth: two steps, then one every other frame.
        let delta = if index < 20 { STEP } else { 1.0 / 240.0 };
        hung.frame(delta, &around, &mut Unmeasured);
        let joints = hung.springs.joints();
        let steps: Vec<u64> = joints.first().map_or(Vec::new(), |joint| {
            joint.records().iter().map(|record| record.step).collect()
        });
        for joint in joints {
            let mine: Vec<u64> = joint.records().iter().map(|record| record.step).collect();
            assert_eq!(
                mine, steps,
                "frame {index}: bones disagree on the steps taken"
            );
            let crate::joint::DrawnSteps { older, newer } = joint.drawn_steps();
            assert!(
                older + 1 == newer || (index == 0 && older == newer),
                "frame {index}: drawn between {older} and {newer}"
            );
        }
        seen.extend(steps);
    }
    assert!(
        seen.windows(2).all(|pair| pair.first() < pair.last()),
        "steps not numbered once each in order"
    );
    assert_eq!(
        seen.len(),
        20 * 2 + 10,
        "a frame with no step due added records"
    );
    hung.springs.reset();
    hung.frame(crate::RATE.time_of(1).get(), &around, &mut Unmeasured);
    for joint in hung.springs.joints() {
        let crate::joint::DrawnSteps { older, newer } = joint.drawn_steps();
        assert_eq!(older, newer, "after a reset the drawing spans two steps");
    }
}

#[test]
fn a_records_depth_is_its_residuals_unless_measured_furniture_moved_the_tail() {
    let mut floor = Floor {
        height: 0.92,
        queries: 0,
    };
    let board = Furniture {
        shapes: vec![Placed {
            placement: DAffine3::from_translation(DVec3::new(-0.15, 0.95, 0.0)),
            shape: Shape::Box {
                half: DVec3::new(0.1, 0.015, 0.1),
            },
            id: 1,
        }],
        measured: true,
    };
    let plain = Furniture {
        measured: false,
        ..board.clone()
    };
    let floor_only = Furniture {
        shapes: Vec::new(),
        measured: true,
    };
    let mut hung = hanging(1, 0.1, 2.0, crate::RATE);
    let mut alone = hanging(1, 0.1, 2.0, crate::RATE);
    for index in 0..120 {
        hung.rig.turn(1, DQuat::from_rotation_z(0.5));
        alone.rig.turn(1, DQuat::from_rotation_z(0.5));
        let around = if index < 60 { &floor_only } else { &board };
        hung.frame(crate::RATE.time_of(1).get(), around, &mut floor);
        alone.frame(crate::RATE.time_of(1).get(), &plain, &mut Unmeasured);
    }
    // The measured floor turns the tail up into the board after the solve: its depth is measured where it lies.
    let pushed = hung
        .springs
        .joints()
        .first()
        .and_then(|joint| joint.records().last().copied())
        .unwrap();
    let (residual, depth) = (pushed.residual.unwrap().get(), pushed.depth.unwrap().get());
    assert!(residual >= -1e-6 && depth > 1e-4, "{pushed:?}");
    let resting = alone
        .springs
        .joints()
        .first()
        .and_then(|joint| joint.records().last().copied())
        .unwrap();
    let residual = resting.residual.unwrap().get();
    assert_eq!(
        resting.depth.unwrap().get(),
        (-residual).max(0.0),
        "{resting:?}"
    );
    assert!(residual >= -1e-6);
}

#[test]
fn a_chain_whose_joint_box_has_no_turn_for_a_frame_is_refused() {
    let mut zero = cube(0.3);
    zero.frame = DQuat::from_xyzw(0.0, 0.0, 0.0, 0.0);
    let rig = Rig::new(&[
        ("parent", None, DVec3::ZERO),
        ("bone", Some(0), DVec3::Y),
        ("tip", Some(1), DVec3::Y * -0.1),
    ]);
    let mut springs = Springs::new(rig.bones.clone(), crate::RATE);
    let chain = ChainSpec {
        bones: vec![name("bone"), name("tip")],
        tail: length(0.05),
        feel: feel(0.0, crate::CUSHION),
        limits: vec![zero],
        colliders: Vec::new(),
    };
    let refusal = springs.add_chain(&chain).unwrap_err();
    assert_eq!(refusal.code(), "springs_invalid_frame");
    assert_eq!(springs.chain_count(), 0);
}

mod contact;

mod friction;

/// A two-metre bone with a hit radius of half a metre on a skeleton scaled four times, and a small box 8 m from its
/// origin: within the bone's room of a frame's travel and its scaled length and radius, 10.25 m, so the bone tests it.
#[test]
fn a_scaled_bone_tests_the_furniture_within_its_scaled_length_and_radius() {
    let mut rig = Rig::new(&[
        ("parent", None, DVec3::ZERO),
        ("bone", Some(0), DVec3::ZERO),
        ("tip", Some(1), DVec3::Y * -2.0),
    ]);
    rig.placement = DAffine3::from_scale(DVec3::splat(4.0));
    let mut springs = Springs::new(rig.bones.clone(), crate::RATE);
    let chain = ChainSpec {
        bones: vec![name("bone"), name("tip")],
        tail: length(0.05),
        feel: Feel {
            radius: length(0.5),
            ..feel(0.0, crate::CUSHION)
        },
        limits: Vec::new(),
        colliders: Vec::new(),
    };
    springs.add_chain(&chain).unwrap();
    let mut hung = Hung { rig, springs };
    let furniture = Furniture {
        shapes: vec![Placed {
            placement: DAffine3::from_translation(DVec3::new(8.05, 0.0, 0.0)),
            shape: Shape::Box {
                half: DVec3::splat(0.05),
            },
            id: 1,
        }],
        measured: false,
    };
    for _ in 0..2 {
        hung.frame(STEP, &furniture, &mut Unmeasured);
    }
    let bone = hung.springs.joints().first().unwrap();
    assert!(bone.started);
    assert_eq!(bone.near, [0]);
}

#[test]
fn half_a_step_draws_without_stepping() {
    let rig = Rig::new(&[("parent", None, DVec3::ZERO)]);
    let mut springs = Springs::new(rig.bones.clone(), crate::RATE);
    assert_eq!(springs.capture(seconds(1.0 / 240.0), &rig), Frame::Draw);
}

/// A shape's move from where it stood to `now`, turned by `angle` about `axis` through `pivot`.
fn turned_about(axis: DVec3, pivot: DVec3, angle: f64) -> DAffine3 {
    DAffine3::from_translation(pivot)
        * DAffine3::from_axis_angle(axis, angle)
        * DAffine3::from_translation(-pivot)
}

/// The turn that moves a point `radius` from the axis by `distance`.
fn moving(distance: f64, radius: f64) -> f64 {
    2.0 * (distance / (2.0 * radius)).asin()
}

/// A unit cube of chains: a shape that moved some corner of it further than 0.25 m jumped, wherever that corner is,
/// and one that moved every corner no further than that did not.
#[test]
fn a_shape_jumped_when_it_carried_any_corner_of_the_chains_area_further_than_a_frame_can_travel() {
    let area = Bounds {
        position: DVec3::ZERO,
        size: DVec3::ONE,
    };
    let moved = |by: f64| DAffine3::from_translation(DVec3::X * by);
    assert!(jumped(DAffine3::IDENTITY, Some(moved(0.3)), area));
    assert!(!jumped(DAffine3::IDENTITY, Some(moved(0.2)), area));
    assert!(
        !jumped(DAffine3::IDENTITY, Some(moved(0.25)), area),
        "exactly the travel"
    );
    assert!(!jumped(DAffine3::IDENTITY, None, area));
    let diagonal = 2.0_f64.sqrt();
    for (axis, pivot) in [(DVec3::X, DVec3::ZERO), (DVec3::Z, DVec3::ONE)] {
        let far = turned_about(axis, pivot, moving(0.3, diagonal));
        assert!(
            jumped(DAffine3::IDENTITY, Some(far), area),
            "{axis} through {pivot}"
        );
        let near = turned_about(axis, pivot, moving(0.2, diagonal));
        assert!(
            !jumped(DAffine3::IDENTITY, Some(near), area),
            "{axis} through {pivot}"
        );
    }
}

/// Measured furniture at one spot, within reach of any centre that close to it.
struct Spot(DVec3);

impl Measured for Spot {
    fn within(&mut self, centre: DVec3, reach: f64) -> bool {
        centre.distance(self.0) <= reach
    }

    fn rest(&mut self, _point: DVec3, _radius: f64) -> Option<Contact> {
        None
    }

    fn cast(&mut self, _start: DVec3, _motion: DVec3, _radius: f64) -> f64 {
        1.0
    }
}

/// A unit bone hanging down from `origin`, its tail placed, from a parent the clip places or, with `parent`, from
/// the joint simulating it.
fn hanging_from(origin: DVec3, parent: Option<usize>, started: bool) -> Joint {
    let bone = Bone {
        name: None,
        parent: None,
        rest: DAffine3::IDENTITY,
    };
    let mut joint = Joint::new(BoneIndex::new(1), &bone, DVec3::NEG_Y, None);
    let pose = Pose {
        translation: origin,
        ..Pose::IDENTITY
    };
    joint.capture(pose, Some(DAffine3::IDENTITY));
    joint.parent = parent.map(JointIndex::new);
    joint.tail = origin + DVec3::NEG_Y;
    joint.started = started;
    joint
}

/// Which of `joints`, one chain rooted at the origin with unit bones and 1 cm hit radii, ask about the furniture at
/// `spot`.
fn asking(mut joints: Vec<Joint>, spot: DVec3) -> Vec<bool> {
    let chain = Chain {
        joints: 0..joints.len(),
        colliders: Vec::new(),
        touching: Vec::new(),
        root: DVec3::ZERO,
        reach: TRAVEL + 2.0 * 1.01,
        span: TRAVEL + 1.01,
        scale: 1.0,
    };
    ask(&chain, &mut joints, &mut Spot(spot));
    joints.iter().map(|joint| joint.asks).collect()
}

/// A chain of two linked unit bones hanging from the origin: furniture a metre below its last tail is asked about by
/// that bone, a metre above its root by the root bone. A second bone hanging on its own 5 m aside is asked about for
/// furniture a metre above its own origin. Bones not yet placed always ask.
#[test]
fn measured_furniture_is_asked_about_at_both_ends_of_each_bone_it_lies_near() {
    let linked = || {
        vec![
            hanging_from(DVec3::ZERO, None, true),
            hanging_from(DVec3::NEG_Y, Some(0), true),
        ]
    };
    assert_eq!(asking(linked(), DVec3::new(0.0, -3.0, 0.0)), [false, true]);
    assert_eq!(asking(linked(), DVec3::Y), [true, false]);
    let aside = vec![
        hanging_from(DVec3::ZERO, None, true),
        hanging_from(DVec3::X * 5.0, None, true),
    ];
    assert_eq!(asking(aside, DVec3::new(5.0, 1.0, 0.0)), [false, true]);
    let new = vec![
        hanging_from(DVec3::ZERO, None, false),
        hanging_from(DVec3::NEG_Y, Some(0), false),
    ];
    assert_eq!(asking(new, DVec3::X * 50.0), [true, true]);
}

#[test]
fn a_link_joins_a_bone_only_to_the_one_before_it_in_its_chain_that_simulates_its_parent() {
    let joint = |parent: Option<usize>| hanging_from(DVec3::ZERO, parent, true);
    let joints = [joint(None), joint(Some(0)), joint(Some(1)), joint(None)];
    assert_eq!(linked_up(&joints, &(0..2), 1), Some(0));
    assert_eq!(
        linked_up(&joints, &(2..4), 2),
        None,
        "the other chain's bone"
    );
    assert_eq!(linked_up(&joints, &(2..4), 3), None, "not its parent");
}
