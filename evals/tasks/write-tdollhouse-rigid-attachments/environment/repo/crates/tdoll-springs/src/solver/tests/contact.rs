//! Made-up rigs meeting plain furniture through the whole solver: recovery, a bone gliding onto a board, and a hem.

use glam::{DAffine3, DQuat, DVec3};

use super::{Rig, length, name, seconds};
use crate::chain::ChainSpec;
use crate::collider::{ColliderShape, ColliderSpec};
use crate::furniture::Unmeasured;
use crate::furniture::{Placed, Shape};
use crate::joint::Feel;
use crate::solver::*;

pub(super) const STEP: f64 = 1.0 / 60.0;

/// A chain's stiffness, drag and gravity, and the friction it meets plain furniture with.
#[derive(Clone, Copy, Debug)]
pub(super) struct Tuning {
    pub(super) stiffness: f64,
    pub(super) drag: f64,
    pub(super) gravity: f64,
    pub(super) friction: f64,
}

impl Tuning {
    /// The tuning at the solver's default friction.
    pub(super) const fn new(stiffness: f64, drag: f64, gravity: f64) -> Self {
        Self {
            stiffness,
            drag,
            gravity,
            friction: crate::FRICTION,
        }
    }

    /// The tuning at friction `friction`.
    pub(super) const fn with_friction(self, friction: f64) -> Self {
        Self { friction, ..self }
    }
}

/// A parent and bones of `lengths` hanging straight down from it, each at rest.
pub(super) fn rig(lengths: &[f64]) -> Rig {
    let names: Vec<String> = (0..lengths.len())
        .map(|index| format!("link{index}"))
        .collect();
    let drops = std::iter::once(0.01).chain(lengths.iter().copied());
    let mut rests = vec![("parent", None, DVec3::ZERO)];
    for (index, (name, drop)) in names.iter().zip(drops).enumerate() {
        rests.push((name.as_str(), Some(index), DVec3::NEG_Y * drop));
    }
    Rig::new(&rests)
}

/// A chain of bones of `lengths` and `radius` hanging from a parent, `feel` its stiffness, drag and gravity, with a
/// ball collider behind the parent when `collider`.
fn tuned_with(lengths: &[f64], radius: f64, feel: Tuning, collider: bool) -> (Rig, Springs) {
    let rig = rig(lengths);
    let mut springs = Springs::new(rig.bones.clone(), crate::RATE);
    let colliders = if collider {
        let spec = ColliderSpec::new(
            name("parent"),
            DQuat::IDENTITY,
            DVec3::new(0.0, 0.0, -0.08),
            ColliderShape::Sphere {
                radius: length(0.06),
            },
        );
        vec![springs.add_collider(&spec).unwrap()]
    } else {
        Vec::new()
    };
    let spec = ChainSpec {
        bones: (0..lengths.len())
            .map(|index| name(&format!("link{index}")))
            .collect(),
        tail: length(lengths.last().copied().unwrap_or(0.05)),
        feel: Feel {
            stiffness: feel.stiffness,
            drag: feel.drag,
            gravity: feel.gravity,
            radius: length(radius),
            cushion: 0.0,
            stiffness_tip: 1.0,
            drag_tip: 1.0,
            gravity_root: 1.0,
            friction: feel.friction,
        },
        limits: Vec::new(),
        colliders,
    };
    springs.add_chain(&spec).unwrap();
    (rig, springs)
}

/// A chain of bones of `lengths` and `radius`, with no collider, `feel` its stiffness, drag and gravity.
pub(super) fn tuned(lengths: &[f64], radius: f64, feel: Tuning) -> (Rig, Springs) {
    tuned_with(lengths, radius, feel, false)
}

/// A soft chain of bones of `lengths` and `radius` under `gravity`, and the solver.
fn chain(lengths: &[f64], radius: f64, gravity: f64, collider: bool) -> (Rig, Springs) {
    let feel = Tuning::new(0.1, 0.3, gravity);
    tuned_with(lengths, radius, feel, collider)
}

/// One frame with `shapes` around and the drawn turns written back.
fn frame(rig: &mut Rig, springs: &mut Springs, shapes: &[Placed]) {
    frame_of(STEP, rig, springs, shapes);
}

/// One frame of `delta` seconds against `shapes`.
pub(super) fn frame_of(delta: f64, rig: &mut Rig, springs: &mut Springs, shapes: &[Placed]) {
    let furniture = Furniture {
        shapes: shapes.to_vec(),
        measured: false,
    };
    if let Frame::Step { .. } = springs.capture(seconds(delta), rig) {
        springs.advance(&furniture, &mut Unmeasured);
    }
    for (bone, turn) in springs.drawn() {
        if let Some(pose) = rig.poses.get_mut(bone.index()) {
            pose.rotation = turn;
        }
    }
}

/// A box of half extents `half` centred at `at`, named `id`.
pub(super) fn slab(at: DVec3, half: DVec3, id: u64) -> Placed {
    Placed {
        placement: DAffine3::from_translation(at),
        shape: Shape::Box { half },
        id,
    }
}

/// The least of each bone's constraint against `shapes`, as the contact solve measures it.
pub(super) fn worst(springs: &Springs, shapes: &[Placed]) -> f64 {
    let solids: Vec<Solid> = shapes.iter().map(|placed| Solid::new(*placed)).collect();
    let indices: Vec<usize> = (0..solids.len()).collect();
    springs
        .joints()
        .iter()
        .filter(|joint| joint.started)
        .map(|joint| {
            let origin = joint.world.translation;
            let reach = joint.tail.distance(origin);
            let lever = crate::contact::Lever {
                origin,
                reach,
                hit: joint.hit,
                start: DVec3::X,
                frame: [DVec3::Y, DVec3::X, DVec3::Z, DVec3::Y],
                solids: Near {
                    all: &solids,
                    indices: &indices,
                },
            };
            lever.physical((joint.tail - origin) / reach)
        })
        .fold(f64::INFINITY, f64::min)
}

/// A 1 cm board a metre square centred at height `at`.
fn board(at: f64) -> [Placed; 1] {
    [slab(
        DVec3::new(0.0, at, 0.0),
        DVec3::new(0.5, 0.005, 0.5),
        9,
    )]
}

#[test]
fn k16_a_board_appearing_through_a_hanging_bone_takes_it_out_without_flicking_it() {
    let (mut rig, mut springs) = chain(&[0.1], 0.01, 1.0, false);
    rig.placement = DAffine3::from_translation(DVec3::Y);
    for _ in 0..60 {
        frame(&mut rig, &mut springs, &[]);
    }
    frame(&mut rig, &mut springs, &board(0.93));
    let new = springs.contacts();
    assert!(
        new.recovered >= 1 && new.caught == 0,
        "a new shape starts the bone again: {new:?}"
    );
    assert!(worst(&springs, &board(0.93)) >= -1e-6, "it stayed in");
    let speed = springs.joints().first().unwrap().velocity.length();
    assert!(speed < 0.05, "the push became {speed} m/s");
    // A board rising into the bone is motion the bone meets, not a start again: it takes the bone out, and the
    // tail moves on no faster than the board pushed it.
    frame(&mut rig, &mut springs, &board(0.94));
    assert_eq!(
        springs.contacts().recovered,
        0,
        "a shape moving starts the bone again"
    );
    assert!(
        worst(&springs, &board(0.94)) >= -1e-6,
        "it stayed in the risen board"
    );
    let speed = springs.joints().first().unwrap().velocity.length();
    assert!(
        speed <= 0.01 * f64::from(crate::RATE.hz()),
        "a rise of 1.2 m/s became {speed} m/s"
    );
}

#[test]
fn k16_a_scale_change_and_a_reset_start_a_bone_against_a_board_again() {
    let (mut rig, mut springs) = chain(&[0.1], 0.01, 1.0, false);
    rig.placement = DAffine3::from_translation(DVec3::Y);
    for _ in 0..60 {
        frame(&mut rig, &mut springs, &board(0.94));
    }
    assert_eq!(springs.contacts().recovered, 0);
    rig.placement =
        DAffine3::from_scale_rotation_translation(DVec3::splat(1.1), DQuat::IDENTITY, DVec3::Y);
    frame(&mut rig, &mut springs, &board(0.94));
    assert!(
        springs.contacts().recovered >= 1,
        "a scale change starts the bone again"
    );
    springs.reset();
    frame(&mut rig, &mut springs, &board(0.94));
    assert!(
        springs.contacts().recovered >= 1,
        "a reset starts the bone again"
    );
    assert!(worst(&springs, &board(0.94)) >= -1e-6);
}

#[test]
fn k12_a_bone_gliding_fast_onto_a_finite_board_meets_it_from_the_step_it_can() {
    for hover in [0.0001, 0.0003, 0.0005] {
        let (mut rig, mut springs) = chain(&[0.1], 0.01, 1.0, false);
        let top = 1.0 - 0.01 - 0.1 - 0.01 - hover;
        let shapes = [slab(
            DVec3::new(0.8, top - 0.005, 0.0),
            DVec3::new(0.5, 0.005, 0.5),
            1,
        )];
        rig.placement = DAffine3::from_translation(DVec3::Y);
        for _ in 0..30 {
            frame(&mut rig, &mut springs, &shapes);
        }
        for index in 0..60 {
            let at = DVec3::new(f64::from(index) * 0.025, 1.0, 0.0);
            rig.placement = DAffine3::from_translation(at);
            frame(&mut rig, &mut springs, &shapes);
            assert!(
                worst(&springs, &shapes) >= -1e-6,
                "{hover}: in the board at {index}"
            );
            let origin = springs.joints().first().unwrap().world.translation;
            let committed = rig.world(Some(1)).translation;
            assert!(
                origin.distance(committed) < 1e-9,
                "the origin did not arrive"
            );
        }
    }
}

/// A seat 0.45 m high with its front at z = 0 and a backrest 0.4 m behind, as the gym's sofa stands.
fn sofa() -> [Placed; 2] {
    [
        slab(DVec3::new(0.0, 0.225, -0.5), DVec3::new(1.0, 0.225, 0.5), 1),
        slab(DVec3::new(0.0, 0.7, -0.48), DVec3::new(1.0, 0.25, 0.08), 2),
    ]
}

/// The hem's frames: the root lowered onto the seat over a second, slid along `path` over `moving` frames, then held
/// for two seconds; each frame's directions and contacts, and how far the tip and the root travelled after the
/// lowering.
fn hem(moving: u32, path: (DVec3, DVec3)) -> (Vec<Vec<(DVec3, bool)>>, f64, f64) {
    let lengths = [
        0.05, 0.062, 0.045, 0.058, 0.07, 0.04, 0.055, 0.066, 0.05, 0.06,
    ];
    let (mut rig, mut springs) = chain(&lengths, 0.008, 2.0, true);
    let shapes = sofa();
    let solids: Vec<Solid> = shapes.iter().map(|placed| Solid::new(*placed)).collect();
    let (from, to) = path;
    let above = from + DVec3::Y * 0.41;
    for index in 0..120 {
        let share = (f64::from(index) / 60.0).min(1.0);
        rig.placement = DAffine3::from_translation(above.lerp(from, share));
        frame(&mut rig, &mut springs, &shapes);
    }
    let tip_from = springs.joints().last().unwrap().tail;
    let mut frames = Vec::new();
    for index in 0..moving + 120 {
        let share = (f64::from(index) / f64::from(moving)).min(1.0);
        rig.placement = DAffine3::from_translation(from.lerp(to, crate::pose::smoothstep(share)));
        frame(&mut rig, &mut springs, &shapes);
        let deepest = worst(&springs, &shapes);
        assert!(
            deepest >= -1e-6,
            "{moving} frames: {deepest} m in the sofa at {index}"
        );
        let seen = springs.joints().iter().map(|joint| {
            let origin = joint.world.translation;
            let touching = solids
                .iter()
                .any(|solid| solid.along(origin, joint.tail).gap - 0.008 < 0.002);
            ((joint.tail - origin).normalize(), touching)
        });
        frames.push(seen.collect());
    }
    let tip_to = springs.joints().last().unwrap().tail;
    (frames, tip_from.distance(tip_to), from.distance(to))
}

#[test]
fn k19_a_made_up_hem_slid_onto_a_sofa_neither_reverses_nor_sinks_and_settles() {
    // Along the seat, and across its front edge and over it, as a coat is drawn while she shifts. A bone dragged
    // over the edge can turn back as the nearest clear side flips from the face to the top, so there reversals are
    // only refused once the hem is held.
    let along = (DVec3::new(0.3, 0.49, -0.15), DVec3::new(-0.5, 0.49, -0.15));
    let over = (DVec3::new(0.0, 0.49, -0.25), DVec3::new(0.0, 0.49, 0.1));
    for (moving, path, from) in [
        (60, along, 0),
        (240, along, 0),
        (60, over, 60),
        (240, over, 240),
    ] {
        let (frames, tip, root) = hem(moving, path);
        assert!(
            tip >= 0.8 * root,
            "{moving} frames: the hem followed {tip} of {root}"
        );
        for bone in 0..10 {
            let turns: Vec<(f64, DVec3, bool)> = frames
                .array_windows::<2>()
                .map(|[then, now]| {
                    let at = |frame: &Vec<(DVec3, bool)>| frame.get(bone).copied().unwrap();
                    let (then, now) = (at(then), at(now));
                    (
                        then.0.angle_between(now.0).to_degrees(),
                        then.0.cross(now.0),
                        then.1 && now.1,
                    )
                })
                .collect();
            for (index, turn) in turns.iter().enumerate().skip(from) {
                for later in turns.iter().skip(index + 1).take(2) {
                    let reversed =
                        turn.0 >= 2.0 && later.0 >= turn.0 * 0.5 && turn.1.dot(later.1) < 0.0;
                    assert!(
                        !(reversed && turn.2 && later.2),
                        "{moving} frames: bone {bone} reversed at {index}"
                    );
                }
            }
            let settled = turns.iter().rev().take(60).all(|turn| turn.0 < 0.1);
            assert!(
                settled,
                "{moving} frames: bone {bone} has not settled in two seconds"
            );
        }
    }
}

/// A one-bone chain of `length` and `radius` with `feel`, its origin `behind` the seat's front edge and `sunk` into its
/// top, the clip leaning it `lean` towards the edge and `yaw` aside, held still for three seconds; the reversals of its
/// direction after the first second, by the tool's rule.
fn held(
    feel: Tuning,
    length: f64,
    radius: f64,
    behind: f64,
    sunk: f64,
    lean: f64,
    yaw: f64,
) -> usize {
    let (mut rig, mut springs) = tuned(&[length], radius, feel);
    let origin = DVec3::new(0.0, 0.45 + radius - sunk, -behind);
    rig.placement = DAffine3::from_translation(origin + DVec3::Y * 0.01);
    if let Some(pose) = rig.poses.get_mut(1) {
        pose.rotation = DQuat::from_rotation_y(yaw) * DQuat::from_rotation_x(-lean);
    }
    let clip = rig.poses.get(1).map(|pose| pose.rotation).unwrap();
    let shapes = sofa();
    let mut turns: Vec<(f64, DVec3)> = Vec::new();
    let mut last: Option<DVec3> = None;
    for _ in 0..180 {
        if let Some(pose) = rig.poses.get_mut(1) {
            pose.rotation = clip;
        }
        frame(&mut rig, &mut springs, &shapes);
        let joint = springs.joints().first().unwrap();
        let direction = (joint.tail - joint.world.translation).normalize();
        if let Some(last) = last {
            turns.push((
                last.angle_between(direction).to_degrees(),
                last.cross(direction),
            ));
        }
        last = Some(direction);
    }
    let mut reversals = 0;
    for (index, turn) in turns.iter().enumerate().skip(60) {
        let back = turns.iter().skip(index + 1).take(2);
        reversals +=
            usize::from(back.into_iter().any(|later| {
                turn.0 >= 2.0 && later.0 >= turn.0 * 0.5 && turn.1.dot(later.1) < 0.0
            }));
    }
    reversals
}

#[test]
fn a_bone_held_resting_on_a_seat_by_its_front_edge_settles() {
    let tunings = [
        (Tuning::new(6.0, 0.9, 1.0), 0.06, 0.01),
        (Tuning::new(14.0, 0.9, 1.0), 0.04, 0.01),
        (Tuning::new(0.8, 0.3, 1.0), 0.0586, 0.008),
        (Tuning::new(4.0, 0.3, 1.0), 0.06, 0.008),
    ];
    for (feel, length, radius) in tunings {
        for behind in [0.012, 0.02, 0.03] {
            for sunk in [0.0, 0.001] {
                for (lean, yaw) in [(0.2, 0.1), (0.4, 0.3), (0.8, -0.2)] {
                    let reversals = held(feel, length, radius, behind, sunk, lean, yaw);
                    assert_eq!(
                        reversals, 0,
                        "{feel:?}, {length} m by {radius} m, {behind} m behind, {sunk} m in, lean {lean}, yaw {yaw}"
                    );
                }
            }
        }
    }
}

/// Furniture overlapping about a coat bone's root, with ids out of their gathered order.
fn overlapping() -> [Placed; 3] {
    let at = |x: f64, y: f64, z: f64| DAffine3::from_translation(DVec3::new(x, 0.95 + y, z));
    [
        Placed {
            placement: at(-0.018_656_8, -0.007_788_48, -0.031_182_48),
            shape: Shape::Sphere {
                radius: 0.019_627_87,
            },
            id: 4,
        },
        Placed {
            placement: at(-0.028_651_12, -0.017_862, 0.014_277_04),
            shape: Shape::Box {
                half: DVec3::new(0.021_959_08, 0.028_128_34, 0.027_217_36),
            },
            id: 7,
        },
        Placed {
            placement: at(-0.011_489_92, -0.005_366_88, -0.008_201_92),
            shape: Shape::Box {
                half: DVec3::new(0.018_481_99, 0.027_004_87, 0.038_409_76),
            },
            id: 2,
        },
    ]
}

#[test]
fn k15_the_same_furniture_gathered_in_any_order_steps_a_bone_to_the_same_bits() {
    let shapes = overlapping();
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let mut seen: Option<Vec<[u64; 3]>> = None;
    for order in orders {
        let gathered: Vec<Placed> = order
            .iter()
            .filter_map(|index| shapes.get(*index).copied())
            .collect();
        let (mut rig, mut springs) = chain(&[0.0586], 0.008, 2.0, false);
        rig.placement = DAffine3::from_translation(DVec3::new(0.0, 0.95, 0.0));
        for step in 0..40 {
            if let Some(pose) = rig.poses.get_mut(1) {
                pose.rotation = DQuat::from_rotation_x(-0.72)
                    * DQuat::from_rotation_z(-0.4 + f64::from(step) * 0.02);
            }
            frame(&mut rig, &mut springs, &gathered);
        }
        let bits: Vec<[u64; 3]> = springs
            .joints()
            .iter()
            .map(|joint| joint.tail.to_array().map(f64::to_bits))
            .collect();
        assert!(
            seen.as_ref().is_none_or(|seen| *seen == bits),
            "gathered as {order:?}, the bone stepped elsewhere"
        );
        seen = Some(bits);
    }
}

/// The fastest a coat bone `behind` the seat's front edge moves its tail over its third second resting on the seat,
/// the sofa shaken by `jitter` metres each frame, as a tracked room is, one step a frame.
fn shaken(jitter: f64, behind: f64, feel: Tuning) -> f64 {
    let (mut rig, mut springs) = tuned(&[0.0586], 0.008, feel);
    rig.placement = DAffine3::from_translation(DVec3::new(0.0, 0.458, -behind));
    let mut most: f64 = 0.0;
    for index in 0..720 {
        let sign = if index % 2 == 0 { 1.0 } else { -1.0 };
        let shift = DVec3::new(1.0, 1.0, -1.0) * (jitter * sign);
        let shapes = sofa().map(|mut placed| {
            placed.placement.translation += shift;
            placed
        });
        frame_of(
            crate::RATE.time_of(1).get(),
            &mut rig,
            &mut springs,
            &shapes,
        );
        let joint = springs.joints().first().unwrap();
        if index >= 240 {
            most = most.max(joint.velocity.length());
        }
    }
    most
}

#[test]
fn a_bone_resting_on_furniture_shaken_by_micrometres_settles() {
    for behind in [0.02, 0.04, 0.1] {
        for feel in [
            Tuning::new(6.0, 0.9, 1.0),
            Tuning::new(0.8, 0.3, 1.0),
            Tuning::new(4.0, 0.3, 2.0),
            Tuning::new(0.2, 0.05, 2.0),
        ] {
            // The sofa itself moves at about a millimetre a second; a tail at rest on it should move no faster.
            let jittered = shaken(2e-6, behind, feel);
            assert!(
                jittered < 0.002,
                "{behind} m back, {feel:?}: the tail moves at {jittered} m/s"
            );
        }
    }
}

/// A held bone: reversals after the first second, the largest drawn turn in the last held second, in radians, and
/// where its tail ends, of one bone of `length` and `radius` with `feel`, its origin at `origin` and its clip turned by
/// `clip`, held still against `shapes` for three seconds at 60 frames a second.
pub(super) fn holding(
    feel: Tuning,
    length: f64,
    radius: f64,
    origin: DVec3,
    clip: DQuat,
    shapes: &[Placed],
) -> (usize, f64, DVec3) {
    let (mut rig, mut springs) = tuned(&[length], radius, feel);
    rig.placement = DAffine3::from_translation(origin + DVec3::Y * 0.01);
    let mut drawn: Vec<DQuat> = Vec::new();
    for _ in 0..180 {
        rig.turn(1, clip);
        frame(&mut rig, &mut springs, shapes);
        drawn.push(
            rig.poses
                .get(1)
                .map_or(DQuat::IDENTITY, |pose| pose.rotation),
        );
    }
    let turns = turns(&drawn);
    let reversals = reversals(&turns);
    let largest = turns
        .iter()
        .rev()
        .take(60)
        .map(|turn| turn.0)
        .fold(0.0, f64::max);
    let tail = springs
        .joints()
        .first()
        .map_or(DVec3::NAN, |joint| joint.tail);
    (reversals, largest, tail)
}

#[test]
fn a_held_hair_bone_behind_a_seats_edge_settles_in_each_of_its_four_poses() {
    let seat = [slab(
        DVec3::new(0.0, 0.225, -0.5),
        DVec3::new(1.0, 0.225, 0.5),
        1,
    )];
    // At friction 0 and 0.4 (R12), each with its present bound.
    for (sunk, mu) in [(0.0005, 0.0), (0.001, 0.0), (0.0005, 0.4), (0.001, 0.4)] {
        for yaw in [1.9, -1.9] {
            let origin = DVec3::new(0.0, 0.45 + 0.01 - sunk, -0.003);
            let clip = DQuat::from_rotation_y(yaw) * DQuat::from_rotation_x(-0.2);
            let (reversals, largest, _) = holding(
                Tuning::new(6.0, 0.9, 5.0).with_friction(mu),
                0.06,
                0.01,
                origin,
                clip,
                &seat,
            );
            assert_eq!(
                reversals, 0,
                "sunk {sunk}, yaw {yaw}, mu {mu}: reversals after the first second"
            );
            assert!(
                largest <= 2.0 * 1e-5 / 0.06,
                "sunk {sunk}, yaw {yaw}, mu {mu}: still turning {largest} rad a frame"
            );
        }
    }
}

#[test]
fn long_hair_held_across_a_tables_front_edge_settles() {
    let table = [slab(
        DVec3::new(0.0, 0.74, -0.4),
        DVec3::new(0.8, 0.01, 0.4),
        1,
    )];
    let origin = DVec3::new(0.0, 0.76, -0.008);
    let clip = DQuat::from_rotation_y(0.5) * DQuat::from_rotation_x(-0.7);
    // A long-hair tuning at gravity 1, where the old stop cycled 0.16–0.36° every few frames.
    let (reversals, largest, _) =
        holding(Tuning::new(6.0, 0.9, 1.0), 0.06, 0.01, origin, clip, &table);
    assert_eq!(reversals, 0);
    assert!(
        largest <= 2.0 * 1e-5 / 0.06,
        "still turning {largest} rad a frame"
    );
}

#[test]
fn a_bone_held_half_a_degree_from_straight_down_onto_a_table_keeps_its_azimuth() {
    // Near straight down the nearest clear direction's azimuth is ill-conditioned; held, each step starts from the
    // last, so the azimuth must not flip from frame to frame.
    let table = [slab(
        DVec3::new(0.0, -0.225, 0.0),
        DVec3::new(1.0, 0.225, 1.0),
        1,
    )];
    // Hanging down, -Y, tipped half a degree towards -Z and then turned by its yaw: its azimuth along the table is
    // -Z turned by the yaw about +Y.
    let half = std::f64::consts::FRAC_1_SQRT_2;
    for (yaw, azimuth) in [
        (0.0_f64, DVec3::NEG_Z),
        (45.0, DVec3::new(-half, 0.0, -half)),
        (
            120.0,
            DVec3::new(
                -(120.0_f64.to_radians().sin()),
                0.0,
                -(120.0_f64.to_radians().cos()),
            ),
        ),
    ] {
        let origin = DVec3::new(0.0, 0.05, 0.0);
        let clip =
            DQuat::from_rotation_y(yaw.to_radians()) * DQuat::from_rotation_x(0.5_f64.to_radians());
        let (reversals, largest, tail) = holding(
            Tuning::new(6.0, 0.9, 5.0),
            0.06,
            0.008,
            origin,
            clip,
            &table,
        );
        // The rig stands its origin a centimetre higher; the table's top is y = 0.
        let along = (tail - (origin + DVec3::Y * 0.01)).with_y(0.0);
        assert!(
            along.normalize().distance(azimuth) < 1e-3,
            "yaw {yaw}: the tail lies along {along}"
        );
        assert!(tail.y >= 0.008 - 1e-4, "yaw {yaw}: the tail sits at {tail}");
        assert_eq!(reversals, 0, "yaw {yaw}");
        assert!(
            largest <= 2.0 * 1e-5 / 0.06,
            "yaw {yaw}: still turning {largest} rad a frame"
        );
    }
}

/// What a swaying held bone did after its first second, which places it: reversals, the largest drawn turn between
/// frames in radians, and the endings of its solves. One bone of `length` and `radius` with `feel`, its origin at
/// `origin` in the sofa's crease and its clip leaning it back 0.6 rad, its parent swaying a fifth of a
/// millimetre along (1, 0, 0.6) at half a cycle a second, held for ten seconds at 60 frames a second.
pub(super) fn swaying(
    feel: Tuning,
    length: f64,
    radius: f64,
    origin: DVec3,
) -> (usize, f64, Vec<Option<crate::contact::Settled>>) {
    let (clip, sway, shapes, seconds) = (DQuat::from_rotation_x(0.6), 0.0002, crease_sofa(), 10);
    let (mut rig, mut springs) = tuned(&[length], radius, feel);
    let mut drawn: Vec<DQuat> = Vec::new();
    let mut endings = Vec::new();
    for frame in 0..seconds * 60 {
        let time = f64::from(frame) / 60.0;
        let swing = DVec3::new(1.0, 0.0, 0.6) * (sway * (std::f64::consts::PI * time).sin());
        rig.placement = DAffine3::from_translation(origin + DVec3::Y * 0.01 + swing);
        rig.turn(1, clip);
        frame_of(STEP, &mut rig, &mut springs, &shapes);
        drawn.push(
            rig.poses
                .get(1)
                .map_or(DQuat::IDENTITY, |pose| pose.rotation),
        );
        if let Some(joint) = springs.joints().first()
            && frame >= 60
        {
            endings.extend(joint.records().iter().map(|record| record.settled));
        }
    }
    let turns = turns(&drawn);
    let reversals = reversals(&turns);
    let largest = turns.iter().skip(60).map(|turn| turn.0).fold(0.0, f64::max);
    (reversals, largest, endings)
}

/// Each turn from one of `drawn` to the next, the shorter way: its angle in radians and its rotation vector.
pub(super) fn turns(drawn: &[DQuat]) -> Vec<(f64, DVec3)> {
    drawn
        .array_windows::<2>()
        .map(|[then, now]| {
            let step = *now * then.inverse();
            let step = if step.w < 0.0 { -step } else { step };
            let (axis, angle) = step.to_axis_angle();
            (angle, axis * angle)
        })
        .collect()
}

/// The reversals after the first second among `turns`, by the snaps tool's rule: a turn of at least 2° followed within
/// two turns by one of at least half its angle the other way.
pub(super) fn reversals(turns: &[(f64, DVec3)]) -> usize {
    turns
        .iter()
        .enumerate()
        .skip(60)
        .filter(|(index, turn)| {
            turns.iter().skip(index + 1).take(2).any(|later| {
                turn.0 >= 2_f64.to_radians() && later.0 >= turn.0 * 0.5 && turn.1.dot(later.1) < 0.0
            })
        })
        .count()
}

/// A sofa: the seat and a backrest whose underside shares the seat's top, its front face at z = −0.4.
pub(super) fn crease_sofa() -> [Placed; 2] {
    [
        slab(DVec3::new(0.0, 0.225, -0.5), DVec3::new(1.0, 0.225, 0.5), 1),
        slab(DVec3::new(0.0, 0.7, -0.48), DVec3::new(1.0, 0.25, 0.08), 2),
    ]
}

#[test]
fn a_bone_in_a_seat_and_backrest_crease_swaying_by_a_fifth_of_a_millimetre_holds() {
    // A pose deep in the corner: 15 mm in front of the backrest, sunk 2 mm, leaning back into the corner; at friction 0 and
    // 0.4 (R12).
    for mu in [0.0, 0.4] {
        let origin = DVec3::new(0.0, 0.45 + 0.008 - 0.002, -0.40 + 0.015);
        let feel = Tuning::new(4.0, 0.3, 2.5).with_friction(mu);
        let (reversals, largest, endings) = swaying(feel, 0.06, 0.008, origin);
        assert_eq!(reversals, 0, "reversals after the first second");
        assert!(
            largest <= 2_f64.to_radians(),
            "a step jumped {}°",
            largest.to_degrees()
        );
        let other: Vec<_> = endings
            .iter()
            .filter(|ending| **ending != Some(crate::contact::Settled::Converged))
            .collect();
        assert!(
            other.is_empty(),
            "{} solves did not converge: {:?}",
            other.len(),
            other.first()
        );
    }
}

#[test]
fn the_swaying_crease_holds_for_each_tuning() {
    let tunings = [
        (Tuning::new(6.0, 0.9, 5.0), 0.06, 0.01),
        (Tuning::new(14.0, 0.9, 0.75), 0.04, 0.01),
        (Tuning::new(0.8, 0.3, 0.75), 0.0586, 0.008),
        (Tuning::new(4.0, 0.3, 2.5), 0.06, 0.008),
    ];
    for (feel, length, radius) in tunings {
        let origin = DVec3::new(0.0, 0.45 + radius - 0.002, -0.40 + 0.015);
        let (reversals, _, endings) = swaying(feel, length, radius, origin);
        assert_eq!(reversals, 0, "{feel:?}: reversals after the first second");
        let escaped = endings
            .iter()
            .filter(|ending| {
                ending.is_some_and(|ending| ending >= crate::contact::Settled::Escaped)
            })
            .count();
        assert_eq!(escaped, 0, "{feel:?}: held solves escaped");
    }
}
