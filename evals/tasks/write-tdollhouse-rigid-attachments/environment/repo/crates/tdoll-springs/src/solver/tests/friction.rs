//! Static friction through the whole solver: off at 0, the colliders and the box after the solve, recovery, and
//! a bone held on an armrest for a minute.

use glam::{DAffine3, DQuat, DVec3};

use super::contact::{STEP, Tuning, frame_of, reversals, rig, slab, tuned, turns};
use super::{Rig, angle, length, name};
use crate::chain::ChainSpec;
use crate::collider::{ColliderShape, ColliderSpec};
use crate::contact::{Friction, Gripped};
use crate::furniture::{Placed, Shape};
use crate::joint::Feel;
use crate::limit::{AxisLimit, JointBox, Limit};
use crate::solver::*;

/// An armrest: a 16 cm box turned under the bone, and the origin 8 mm above its top near its end.
fn armrest() -> ([Placed; 1], DVec3) {
    let place = DAffine3::from_translation(DVec3::new(0.0, 0.55, 0.0))
        * DAffine3::from_quat(DQuat::from_rotation_y(0.55) * DQuat::from_rotation_z(0.2));
    let arm = [Placed {
        placement: place,
        shape: Shape::Box {
            half: DVec3::new(0.08, 0.08, 0.4),
        },
        id: 1,
    }];
    (arm, place.transform_point3(DVec3::new(0.0, 0.088, 0.4)))
}

/// What a bone held on the armrest at `mu`, its root swaying 1 mm along X at 1 Hz, does over `seconds`: its drawn
/// rotations, its direction at each second's start, and the records of every step after the first second.
fn held_on_the_armrest(
    mu: f64,
    seconds: u32,
) -> (Vec<DQuat>, Vec<DVec3>, Vec<crate::joint::Record>) {
    let (arm, origin) = armrest();
    let feel = Tuning::new(4.0, 0.3, 2.5).with_friction(mu);
    let (mut rig, mut springs) = tuned(&[0.06], 0.008, feel);
    let clip = DQuat::from_rotation_y(0.1) * DQuat::from_rotation_x(-0.2);
    let (mut drawn, mut phases, mut records) = (Vec::new(), Vec::new(), Vec::new());
    for frame in 0..seconds * 60 {
        let time = f64::from(frame) / 60.0;
        let swing = DVec3::X * (0.001 * (std::f64::consts::TAU * time).sin());
        rig.placement = DAffine3::from_translation(origin + DVec3::Y * 0.01 + swing);
        rig.turn(1, clip);
        frame_of(STEP, &mut rig, &mut springs, &arm);
        drawn.push(
            rig.poses
                .get(1)
                .map_or(DQuat::IDENTITY, |pose| pose.rotation),
        );
        if let Some(joint) = springs.joints().first() {
            if frame % 60 == 0 {
                phases.push((joint.tail - joint.world.translation).normalize());
            }
            if frame >= 60 {
                records.extend_from_slice(joint.records());
            }
        }
    }
    (drawn, phases, records)
}

#[test]
fn r1_at_friction_0_no_step_grips() {
    let (_, _, records) = held_on_the_armrest(0.0, 2);
    assert_ne!(records.len(), 0, "the armrest is never met");
    for record in records {
        assert_eq!(record.gripped, Gripped::default(), "friction 0 gripped");
    }
}

#[test]
fn r10_a_bone_held_on_an_armrest_for_a_minute_neither_reverses_nor_creeps_and_sticks() {
    let (drawn, phases, records) = held_on_the_armrest(0.4, 60);
    assert_eq!(
        reversals(&turns(&drawn)),
        0,
        "reversals after the first second"
    );
    // Same phase, a cycle apart: about where a watcher of a still body would notice, 3° a minute.
    let creep = phases
        .array_windows::<2>()
        .skip(1)
        .map(|[then, now]| then.angle_between(*now).to_degrees())
        .fold(0.0, f64::max);
    assert!(creep < 0.05, "it creeps {creep}° a cycle");
    let contact: Vec<_> = records
        .iter()
        .filter(|record| record.settled.is_some())
        .collect();
    assert!(!contact.is_empty(), "the armrest is never met");
    for record in contact {
        assert_eq!(
            record.gripped.friction,
            Friction::Stuck,
            "step {} is not stuck",
            record.step
        );
    }
}

#[test]
fn r9_a_recovered_step_meets_furniture_as_friction_0_does() {
    let (arm, origin) = armrest();
    let mut tails = Vec::new();
    for mu in [0.0, 0.4] {
        let feel = Tuning::new(4.0, 0.3, 2.5).with_friction(mu);
        let (mut rig, mut springs) = tuned(&[0.06], 0.008, feel);
        rig.placement = DAffine3::from_translation(origin + DVec3::Y * 0.01);
        rig.turn(
            1,
            DQuat::from_rotation_y(0.1) * DQuat::from_rotation_x(-0.2),
        );
        // One step: the first starts the new bone again, whatever its friction.
        frame_of(crate::RATE.time_of(1).get(), &mut rig, &mut springs, &arm);
        assert!(
            springs.contacts().recovered >= 1,
            "the first step does not start again"
        );
        tails.push(
            springs
                .joints()
                .first()
                .map_or(DVec3::ZERO, |joint| joint.tail),
        );
    }
    assert_eq!(
        tails.first(),
        tails.last(),
        "friction moved a recovered step"
    );
}

/// One bone of 6 cm and an 8 mm hit radius on `springs`, stiffness 4, drag 0.3, with `gravity`, `cushion` and
/// `friction`, its box `limits` and the `colliders` it keeps out of.
fn one_bone(
    springs: &mut Springs,
    gravity: f64,
    cushion: f64,
    friction: f64,
    limits: Vec<Limit>,
    colliders: Vec<ColliderId>,
) {
    let chain = ChainSpec {
        bones: vec![name("link0")],
        tail: length(0.06),
        feel: Feel {
            stiffness: 4.0,
            drag: 0.3,
            gravity,
            radius: length(0.008),
            cushion,
            stiffness_tip: 1.0,
            drag_tip: 1.0,
            gravity_root: 1.0,
            friction,
        },
        limits,
        colliders,
    };
    springs.add_chain(&chain).unwrap();
}

/// A limb capsule, 3 cm thick along Z beside a bone lying along +Z on a floor with its origin pinned 8 mm up,
/// the bone pulled by `side` along X and into the floor; the limb, on its own bone, starts 7 mm clear and closes in on
/// the stuck bone. The least limb and floor clearances of the tail over two seconds.
fn limb(side: f64, mu: f64) -> (f64, f64) {
    let mut rig_now = Rig::new(&[
        ("parent", None, DVec3::ZERO),
        ("link0", Some(0), DVec3::NEG_Y * 0.01),
        ("limb", Some(0), DVec3::ZERO),
    ]);
    let mut springs = Springs::new(rig_now.bones.clone(), crate::RATE);
    rig_now.placement = DAffine3::from_translation(DVec3::new(0.0, 0.018, 0.0));
    let spec = ColliderSpec::new(
        name("limb"),
        DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2),
        DVec3::new(0.0, -0.018, 0.03),
        ColliderShape::Capsule {
            radius: length(0.03),
            half_length: length(0.1),
        },
    );
    let colliders = vec![springs.add_collider(&spec).unwrap()];
    one_bone(&mut springs, 2.5, 0.0, mu, Vec::new(), colliders);
    let floor = [slab(
        DVec3::new(0.0, -0.5, 0.0),
        DVec3::new(1.0, 0.5, 1.0),
        1,
    )];
    let lying = crate::pose::arc(DVec3::NEG_Y, DVec3::Z);
    let pulled = crate::pose::arc(
        DVec3::NEG_Y,
        (DVec3::Z + DVec3::X * side - DVec3::Y * 0.1).normalize(),
    );
    let (mut least_limb, mut least_floor) = (f64::INFINITY, f64::INFINITY);
    for frame in 0..120 {
        let axis = -0.045 + 0.0002 * f64::from(frame.min(60));
        if let Some(pose) = rig_now.poses.get_mut(2) {
            pose.translation = DVec3::new(axis, 0.0, 0.0);
        }
        rig_now.turn(1, if frame == 0 { lying } else { pulled });
        frame_of(STEP, &mut rig_now, &mut springs, &floor);
        let Some(joint) = springs.joints().first() else {
            continue;
        };
        let (a, b) = (DVec3::new(axis, 0.0, -0.04), DVec3::new(axis, 0.0, 0.10));
        let along = ((joint.tail - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
        least_limb = least_limb.min(joint.tail.distance(a + (b - a) * along) - 0.03 - 0.008);
        least_floor = least_floor.min(joint.tail.y - 0.008);
    }
    (least_limb, least_floor)
}

#[test]
fn r6_friction_yields_to_the_colliders_push_and_furniture_wins() {
    for side in [0.04, 0.06, -0.03] {
        for mu in [0.4, 0.8] {
            let (limb, floor) = limb(side, mu);
            // Friction reaches the push only linearly: the curved limb's residue, under a touch, is not visible.
            assert!(
                limb >= -crate::TOUCH,
                "pull {side}, mu {mu}: {limb} m into the limb"
            );
            assert!(
                floor >= -crate::TOLERANCE,
                "pull {side}, mu {mu}: {floor} m into the floor"
            );
        }
    }
}

/// The direction at `azimuth` and `elevation`, in degrees, in the model's frame, Y up from its floor and the box's
/// rest at azimuth 0, 10° up, turned onto the rig's, whose bone rests along −Y.
fn modelled(azimuth: f64, elevation: f64) -> DVec3 {
    let tilt = 10.0_f64.to_radians();
    let onto = DQuat::from_rotation_arc(DVec3::new(tilt.cos(), tilt.sin(), 0.0), DVec3::NEG_Y);
    let (azimuth, elevation) = (azimuth.to_radians(), elevation.to_radians());
    onto * DVec3::new(
        elevation.cos() * azimuth.cos(),
        elevation.sin(),
        elevation.cos() * azimuth.sin(),
    )
}

/// How far a bone along `direction` lies past `limit`, which holds X and Z to `most` either way, by its furthest angle.
fn excess(limit: &JointBox, most: f64, direction: DVec3) -> f64 {
    let angles = limit.angles(crate::pose::arc(DVec3::NEG_Y, direction));
    (angles.x.abs() - most).max(angles.z.abs() - most).max(0.0)
}

/// A joint box made oblique: a bone whose joint box allows 60° about its rest's X and Z, the rest 10° above a floor,
/// pulled past the box's limit and into the floor, started along `start`, at `mu`, for `seconds`, its twist about
/// the rest held by `twist`, with the whole scene, rig and floor, turned by `turn`; its directions are read back in
/// the unturned scene.
fn boxed(mu: f64, start: DVec3, seconds: u32, twist: AxisLimit, turn: DQuat) -> Vec<(DVec3, f64)> {
    let up = modelled(90.0, 0.0).cross(modelled(0.0, 0.0)).normalize();
    let mut rig_now = rig(&[0.06]);
    let origin = DVec3::new(0.0, 0.03, 0.0);
    rig_now.placement =
        DAffine3::from_quat(turn) * DAffine3::from_translation(origin + DVec3::Y * 0.01);
    let mut springs = Springs::new(rig_now.bones.clone(), crate::RATE);
    let most = 60.0_f64.to_radians();
    let held = AxisLimit::Held {
        min: angle(-most),
        max: angle(most),
    };
    let limits = vec![Limit {
        bone: name("link0"),
        frame: DQuat::IDENTITY,
        reversed: true,
        axes: [held, twist, held],
    }];
    let limit = limits.first().and_then(Limit::joint).unwrap();
    one_bone(&mut springs, 0.0, 0.3, mu, limits, Vec::new());
    let floor = [Placed {
        placement: DAffine3::from_quat(turn),
        shape: Shape::Plane {
            normal: up,
            distance: up.dot(origin) - 0.03,
        },
        id: 1,
    }];
    let mut out = Vec::new();
    for frame in 0..seconds * 60 {
        let aim = if frame == 0 {
            start
        } else {
            modelled(85.0, -30.0)
        };
        rig_now.turn(1, crate::pose::arc(DVec3::NEG_Y, aim));
        frame_of(STEP, &mut rig_now, &mut springs, &floor);
        if let Some(joint) = springs.joints().first() {
            let direction = turn.inverse() * (joint.tail - joint.world.translation).normalize();
            out.push((direction, excess(&limit, most, direction)));
        }
    }
    out
}

#[test]
fn r7_a_bone_its_box_holds_on_a_floor_stays_at_rest_and_never_moves_against_its_pull() {
    let start = modelled(20.0, -21.5);
    for mu in [0.4, 0.8] {
        // Held by its box and the floor for 7 s: once come to rest, by the fourth second, it stays there and inside
        // its box, never creeping along the floor.
        let held = boxed(mu, start, 7, AxisLimit::Free, DQuat::IDENTITY);
        let rest = held.get(4 * 60).map_or(start, |each| each.0);
        for (frame, (direction, excess)) in held.iter().enumerate().skip(4 * 60) {
            assert!(
                direction.angle_between(rest) <= crate::TOLERANCE / 0.06,
                "mu {mu}, frame {frame}: left its rest by {}°",
                direction.angle_between(rest).to_degrees()
            );
            assert!(
                *excess <= crate::TOLERANCE / 0.06,
                "mu {mu}: past its box by {excess}"
            );
        }
        // From 20° of azimuth, the bone moves only towards where its pull would take it, never back.
        let path = boxed(mu, start, 4, AxisLimit::Free, DQuat::IDENTITY);
        let up = modelled(90.0, 0.0).cross(modelled(0.0, 0.0)).normalize();
        let toward = |direction: DVec3| {
            (direction - up * up.dot(direction))
                .normalize()
                .dot((rest - up * up.dot(rest)).normalize())
        };
        for (frame, [(then, _), (now, excess)]) in path.array_windows::<2>().enumerate().skip(1) {
            {
                assert!(
                    toward(*now) >= toward(*then) - 1e-9,
                    "mu {mu}, frame {frame}: moved away from its pull"
                );
                assert!(
                    *excess <= crate::TOLERANCE / 0.06,
                    "mu {mu}: past its box by {excess}"
                );
            }
        }
    }
}

/// The box's hard limit and the floor hold a bone the same way wherever the scene is turned: friction yields to the
/// limit, its twist held too, in the bone's own frame, so turning the rig and the floor together turns the bone's
/// every direction with them.
#[test]
fn a_bone_its_box_and_friction_hold_moves_the_same_however_the_scene_is_turned() {
    let start = modelled(20.0, -21.5);
    let twist = AxisLimit::Held {
        min: angle(-0.1),
        max: angle(0.1),
    };
    let unturned = boxed(0.8, start, 2, twist, DQuat::IDENTITY);
    let turn = DQuat::from_axis_angle(DVec3::new(1.0, 2.0, -0.5).normalize(), 1.1);
    let turned = boxed(0.8, start, 2, twist, turn);
    for (frame, ((then, _), (now, _))) in unturned.iter().zip(&turned).enumerate() {
        assert!(
            then.angle_between(*now) < 1e-6,
            "frame {frame}: {then} unturned, {now} turned back"
        );
    }
}
