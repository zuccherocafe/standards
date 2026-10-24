//! The frictional step on made-up rows and on a bone against a floor and a wall: its Coulomb bound, its slide law, a contact it
//! does not reach, and that it never slides further than the frictionless step.

use glam::{DAffine3, DQuat};
use glam::{DMat2, DVec2, DVec3};

use super::super::tests::{Shapes, block, edge, lever, solid, wall};
use super::super::{Lever, Row, Solid, Solved, projected};
use super::*;
use crate::furniture::Shape;

/// The rule at the contact for a bone whose start is `bone.start`: the start's frictional step at `mu`, its bands in
/// the one solve towards `target`, as `meet` runs it once the colliders and the box have placed the anchor.
fn gripped(bone: &Lever<'_>, target: DVec3, mu: f64) -> (Solved, Gripped) {
    let mut problem = bone.problem();
    let start = problem.assess(bone.start);
    let gripped = if mu > 0.0 {
        problem.grip(&start, target, &Yields::default(), mu)
    } else {
        Gripped::default()
    };
    (problem.resolve(target, Some(&start)), gripped)
}

/// The angle between two directions, in degrees.
fn degrees(from: DVec3, to: DVec3) -> f64 {
    from.angle_between(to).to_degrees()
}

/// A floor, the only shape of `shapes`: the origin 67.088 mm up, the bone 10° off hanging with its tail touching,
/// reach 60 mm.
fn floor_bone(shapes: &Shapes) -> (Lever<'_>, DVec3, DVec3) {
    let tilt = 10.0_f64.to_radians();
    let start = DVec3::new(tilt.sin(), -tilt.cos(), 0.0);
    let normal = DVec3::new(tilt.cos(), tilt.sin(), 0.0);
    let bone = lever(
        DVec3::new(0.0, 0.067_088_465_180_732_47, 0.0),
        0.06,
        start,
        shapes,
    );
    (bone, start, normal)
}

/// `start` turned by `0.2` into the floor along `normal` and by `tau` across it, along Z.
fn pulled(start: DVec3, normal: DVec3, tau: f64) -> DVec3 {
    let way = normal * -0.2 + DVec3::Z * tau;
    let angle = way.length();
    (start * angle.cos() + way / angle * angle.sin()).normalize()
}

/// The start's rows and the pull towards `target` in its tangent plane.
fn at_start(bone: &Lever<'_>, start: DVec3, target: DVec3) -> (Vec<Row>, DVec2) {
    let problem = bone.problem();
    let tangent = bone.tangent(start);
    let mut rows = Vec::new();
    problem.assess(start).rows_into(&tangent, &mut rows);
    (rows, tangent.towards(target))
}

/// Each loaded contact's Coulomb bound after `cones`' frictional step over `rows` at `mu`: `μ·s·w`, its row's share
/// times its load.
fn bounds<'c>(cones: &'c Cones, rows: &'c [Row], mu: f64) -> impl Iterator<Item = f64> + 'c {
    cones
        .contacts
        .iter()
        .zip(rows)
        .filter(|(each, _)| each.load > 1e-12)
        .map(move |(each, row)| mu * row.share * each.load)
}

#[test]
fn r2_a_contacts_load_is_its_normal_correction_over_sin_phi_whatever_mu() {
    let floor = block(0.0, 1);
    let shapes = Shapes::of(&[&floor]);
    let (bone, start, normal) = floor_bone(&shapes);
    let (rows, towards) = at_start(&bone, start, pulled(start, normal, 0.0));
    for mu in [0.1, 0.4, 0.8] {
        let mut cones = Cones::default();
        cones.step(&rows, towards, mu);
        let loaded: Vec<&Contact> = cones
            .contacts
            .iter()
            .filter(|each| each.load > 1e-12)
            .collect();
        assert!(!loaded.is_empty(), "nothing loaded");
        let load: f64 = loaded.iter().map(|each| each.load).sum();
        let sin = 10.0_f64.to_radians().sin();
        // The normal correction in angle is the load times |a| = sin φ: the pull's 0.2 into the floor.
        assert!(
            (load * sin - 0.2).abs() < 1e-12,
            "mu {mu}: the load {load} corrects {}",
            load * sin
        );
    }
}

#[test]
fn r3_the_frictional_step_sticks_to_its_bound_then_slides_by_the_relaxations_law() {
    let floor = block(0.0, 1);
    let shapes = Shapes::of(&[&floor]);
    let (bone, start, normal) = floor_bone(&shapes);
    for mu in [0.1, 0.4] {
        let sin = 10.0_f64.to_radians().sin();
        let (bound, k) = (mu * 0.2 / sin, mu / sin);
        let mut last: Option<f64> = None;
        let mut tau = 0.0;
        while tau <= bound + 0.087 {
            tau += 0.0005;
            let (rows, towards) = at_start(&bone, start, pulled(start, normal, tau));
            let mut cones = Cones::default();
            let step = cones.step(&rows, towards, mu);
            let tangent = bone.tangent(start);
            let slide = tangent.retract(step).dot(DVec3::Z).asin();
            let law = (tau - bound).max(0.0) / (1.0 + k * k);
            assert!(
                (slide - law).abs() <= crate::TOLERANCE / 0.06,
                "mu {mu}, tau {tau}: slid {slide}, the law {law}"
            );
            if let Some(last) = last {
                assert!(
                    (slide - last).abs() <= 1.2 * 0.0005,
                    "mu {mu}, tau {tau}: jumped"
                );
            }
            last = Some(slide);
        }
    }
}

/// A floor and a side wall: the bone dipping along +X from 30 mm up, its tail on the floor, the wall `gap` clear.
fn floor_and_wall(gap: f64) -> (Solid, Solid) {
    (
        block(0.0, 1),
        wall(DVec3::NEG_Z, DVec3::new(0.0, 0.0, gap + 0.008), 2),
    )
}

#[test]
fn r5_a_second_contact_the_frictional_step_does_not_reach_carries_no_load() {
    let (floor, side) = floor_and_wall(0.003);
    let origin = DVec3::new(0.0, 0.03, 0.0);
    let start = DVec3::new((0.06_f64.powi(2) - 0.022_f64.powi(2)).sqrt(), -0.022, 0.0).normalize();
    let shapes = Shapes::of(&[&floor, &side]);
    let bone = lever(origin, 0.06, start, &shapes);
    let normal = (DVec3::Y - start * start.y).normalize();
    let mut steps = Vec::new();
    for tau in [0.049_99, 0.05, 0.050_01] {
        let (rows, towards) = at_start(&bone, start, pulled(start, normal, tau));
        let mut cones = Cones::default();
        steps.push(cones.step(&rows, towards, 0.4));
        let wall_rows: Vec<&Contact> = cones
            .contacts
            .iter()
            .zip(&rows)
            .filter(|(_, row)| row.normal.z < -0.5)
            .map(|(each, _)| each)
            .collect();
        assert!(!wall_rows.is_empty(), "the wall gives no row");
        for each in wall_rows {
            assert!(
                each.load.abs() < 1e-12,
                "tau {tau}: the wall carries {}",
                each.load
            );
        }
    }
    for pair in steps.windows(2) {
        if let [first, second] = pair {
            assert!(
                first.distance(*second) <= 2e-5,
                "the step jumps across the old switch: {first} to {second}"
            );
        }
    }
}

/// A small generator, so the random cases are the same every run.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        f64::from(u32::try_from(self.0 % 1_000_000).unwrap_or(0)) / 1_000_000.0
    }
}

#[test]
fn r11_the_frictional_step_never_slides_further_than_the_frictionless_one() {
    // A single contact of each kind as its rows give it: a tail on a plane or a ball (share 1), an edge's two faces
    // (a piece of two rows), a pinned origin (share 1, steep) and a capsule witness near the root (share 0.02).
    let mut draw = Draw(0x9e37_79b9_7f4a_7c15);
    for case in 0..2000 {
        let share = [1.0, 1.0, 0.02].get(case % 3).copied().unwrap_or(1.0);
        let angle = |draw: &mut Draw| draw.next() * std::f64::consts::TAU;
        let way = |angle: f64| DVec2::new(angle.cos(), angle.sin());
        let sin = 0.05 + 0.95 * draw.next();
        let first = angle(&mut draw);
        let mut rows = vec![Row {
            value: 0.0,
            gradient: way(first) * sin * share,
            normal: DVec3::Y,
            share,
            piece: 0,
        }];
        if case % 5 == 0 {
            rows.push(Row {
                value: 0.0,
                gradient: way(first + 0.3 * (draw.next() - 0.5)) * sin * share,
                normal: DVec3::Y,
                share,
                piece: 0,
            });
        }
        let towards = way(angle(&mut draw)) * 0.5 * draw.next();
        let mu = [0.1, 0.4, 0.8].get(case % 3).copied().unwrap_or(0.4);
        let free = projected(towards, &rows, &[], DMat2::IDENTITY).map_or(towards, |found| found.0);
        let mut cones = Cones::default();
        let step = cones.step(&rows, towards, mu);
        for contact in &cones.contacts {
            let (slid, could) = (contact.across.dot(step), contact.across.dot(free));
            assert!(
                slid * could >= -1e-15 && slid.abs() <= could.abs() + 1e-12,
                "case {case}: slid {slid} where the frictionless step slides {could}"
            );
        }
    }
}

#[test]
fn r3_the_rule_sticks_to_its_bound_then_slides_by_the_relaxations_law() {
    // The floor bone through the one solve: the bands carry the frictional step's slide to the exact answer.
    let floor = block(0.0, 1);
    let shapes = Shapes::of(&[&floor]);
    let (bone, start, normal) = floor_bone(&shapes);
    let sin = 10.0_f64.to_radians().sin();
    for mu in [0.1, 0.4] {
        let (bound, k) = (mu * 0.2 / sin, mu / sin);
        let mut last: Option<DVec3> = None;
        let mut tau = 0.0;
        while tau <= bound + 0.087 {
            let (solved, _) = gripped(&bone, pulled(start, normal, tau), mu);
            let slide = solved.direction.dot(DVec3::Z).asin();
            let law = (tau - bound).max(0.0) / (1.0 + k * k);
            assert!(
                (slide - law).abs() <= crate::TOLERANCE / 0.06,
                "mu {mu}, tau {tau}: slid {slide}, the law {law}"
            );
            if let Some(last) = last {
                let moved = last.angle_between(solved.direction);
                assert!(
                    moved <= 1.2 * 0.0005 + 2.0 * crate::TOLERANCE / 0.06,
                    "mu {mu}, tau {tau}: moved {moved} for 0.0005 of pull"
                );
            }
            last = Some(solved.direction);
            tau += 0.0005;
        }
    }
}

#[test]
fn r4_a_bone_against_a_floor_and_a_side_wall_sticks_inside_the_cone_and_moves_steadily() {
    let origin = DVec3::new(0.0, 0.03, 0.0);
    let start = DVec3::new((0.06_f64.powi(2) - 0.022_f64.powi(2)).sqrt(), -0.022, 0.0).normalize();
    let (floor, side) = (
        block(0.0, 1),
        wall(DVec3::NEG_Z, DVec3::new(0.0, 0.0, 0.008), 2),
    );
    let shapes = Shapes::of(&[&floor, &side]);
    let bone = lever(origin, 0.06, start, &shapes);
    let tangent = bone.tangent(start);
    let way = |normal: DVec3| {
        DVec2::new(normal.dot(tangent.across), normal.dot(tangent.along)).normalize()
    };
    let (onto, against) = (way(DVec3::Y), way(DVec3::NEG_Z));
    let sin = (DVec3::Y - start * start.y).length();
    for mu in [0.1, 0.4] {
        let onset = -(mu / sin).atan().to_degrees();
        let mut last: Option<DVec3> = None;
        for step in 0..=480 {
            let angle = (-60.0 + f64::from(step) * 0.25_f64).to_radians();
            let pull = -(onto * angle.cos() + against * angle.sin()) * 0.15;
            let (solved, _) = gripped(&bone, tangent.retract(pull), mu);
            let moved = degrees(solved.direction, start);
            if angle.to_degrees() >= onset + 0.25 {
                assert!(
                    moved <= (crate::TOLERANCE / 0.06).to_degrees(),
                    "mu {mu}, {angle}: moved {moved}°"
                );
            }
            if let Some(last) = last {
                let change = degrees(solved.direction, last) / 0.25;
                assert!(
                    change <= 0.25,
                    "mu {mu}, {:.2}°: {change}° per degree of pull",
                    angle.to_degrees()
                );
            }
            last = Some(solved.direction);
        }
    }
}

#[test]
fn r5_a_second_contact_reached_only_as_its_wall_comes_in_moves_the_bone_no_faster_than_the_wall() {
    let origin = DVec3::new(0.0, 0.03, 0.0);
    let start = DVec3::new((0.06_f64.powi(2) - 0.022_f64.powi(2)).sqrt(), -0.022, 0.0).normalize();
    let normal = (DVec3::Y - start * start.y).normalize();
    let (floor, side) = floor_and_wall(0.003);
    let shapes = Shapes::of(&[&floor, &side]);
    let bone = lever(origin, 0.06, start, &shapes);
    let answers: Vec<DVec3> = [0.049_99, 0.05, 0.050_01]
        .into_iter()
        .map(|tau| gripped(&bone, pulled(start, normal, tau), 0.4).0.direction)
        .collect();
    for pair in answers.windows(2) {
        if let [first, second] = pair {
            assert!(
                first.angle_between(*second) <= 1e-9,
                "the answer moves across the old switch"
            );
        }
    }
    for tau in [0.05, 0.09] {
        let mut last: Option<DVec3> = None;
        for step in 0..=340 {
            let gap = 0.003 - f64::from(step) * 0.000_01;
            let (floor, side) = floor_and_wall(gap);
            let shapes = Shapes::of(&[&floor, &side]);
            let bone = lever(origin, 0.06, start, &shapes);
            let (solved, _) = gripped(&bone, pulled(start, normal, tau), 0.4);
            let tail = origin + solved.direction * 0.06;
            // Each answer is the solve's to within the tolerance, so a step of the wall moves the tail by at most
            // that step and twice the tolerance.
            if let Some(last) = last {
                let moved = tail.distance(last);
                assert!(
                    moved <= 0.000_01 + 2.0 * crate::TOLERANCE,
                    "tau {tau}, gap {gap}: the tail moves {moved} m for 1e-5 m of wall"
                );
            }
            last = Some(tail);
        }
    }
}

#[test]
fn r8_a_pull_turned_from_pressing_to_leaving_moves_the_bone_steadily_and_frees_it_once_it_leaves() {
    let floor = block(0.0, 1);
    let shapes = Shapes::of(&[&floor]);
    let (bone, start, normal) = floor_bone(&shapes);
    let tangent = bone.tangent(start);
    let out = DVec2::new(normal.dot(tangent.across), normal.dot(tangent.along)).normalize();
    let across = DVec2::new(DVec3::Z.dot(tangent.across), DVec3::Z.dot(tangent.along)).normalize();
    let mut last: Option<DVec3> = None;
    // From pressing straight in, through sliding along the floor, to leaving straight out.
    for step in 0..=720 {
        let angle = (-90.0 + f64::from(step) * 0.25_f64).to_radians();
        let pull = (out * angle.sin() + across * angle.cos()) * 0.2;
        let target = tangent.retract(pull);
        let (solved, _) = gripped(&bone, target, 0.4);
        if let Some(last) = last {
            let change = degrees(solved.direction, last) / 0.25;
            assert!(
                change <= 0.25,
                "{:.2}°: {change}° per degree of pull",
                angle.to_degrees()
            );
        }
        // Leaving, the pull lies inside the contact's dual friction cone: it lifts off by at least μ times its slide.
        let sin = 10.0_f64.to_radians().sin();
        if sin * out.dot(pull) >= 0.4 * across.dot(pull).abs() + 1e-9 {
            let (free, _) = gripped(&bone, target, 0.0);
            assert!(
                solved.direction.angle_between(free.direction) <= crate::TOLERANCE / 0.06,
                "{:.2}°: leaving, yet held",
                angle.to_degrees()
            );
        }
        last = Some(solved.direction);
    }
}

#[test]
fn r9_a_step_caught_deeper_than_a_touch_meets_the_slide_law_from_its_own_load() {
    // The start 0.6 mm inside the floor: the solve puts it out from the anchor, and the bands hold the frictional
    // step's slide, now against a threshold the depth raises.
    let floor = block(0.0006, 1);
    let shapes = Shapes::of(&[&floor]);
    let (bone, start, normal) = floor_bone(&shapes);
    let sin = 10.0_f64.to_radians().sin();
    let k = 0.4 / sin;
    // The stick threshold is the bound with no tangential pull: the pull's normal correction, the depth's included.
    let (rows, towards) = at_start(&bone, start, pulled(start, normal, 0.0));
    let mut cones = Cones::default();
    cones.step(&rows, towards, 0.4);
    let stuck = bounds(&cones, &rows, 0.4).fold(0.0, f64::max);
    let tangent = bone.tangent(start);
    for tau in [0.0, 0.2, 0.5, 0.7, 0.9] {
        let target = pulled(start, normal, tau);
        let (rows, towards) = at_start(&bone, start, target);
        let step = Cones::default().step(&rows, towards, 0.4);
        let across = DVec2::new(DVec3::Z.dot(tangent.across), DVec3::Z.dot(tangent.along));
        let law = (tau - stuck).max(0.0) / (1.0 + k * k);
        assert!(
            (across.dot(step) - law).abs() <= 1e-9,
            "tau {tau}: the cone slid {}, the law {law}",
            across.dot(step)
        );
        // The band carries the cone's slide to the answer, along the great circle its step lies on.
        let (solved, _) = gripped(&bone, target, 0.4);
        let slid = solved.direction.dot(DVec3::Z) - tangent.retract(step).dot(DVec3::Z);
        assert!(
            slid.abs() <= crate::TOLERANCE / 0.06,
            "tau {tau}: the answer is {slid} off its band"
        );
    }
}

/// Case `case`'s single contact, of one of five kinds: a tail touching a plane or a ball, an origin on a box's rounded
/// edge, an origin pinned on a floor, and a capsule witness near the root; its shape, of `floor`, `ball` and `corner`,
/// its origin and its direction.
fn single_contact<'s>(
    case: usize,
    draw: &mut Draw,
    floor: &'s Solid,
    ball: &'s Solid,
    corner: &'s Solid,
) -> (&'s Solid, DVec3, DVec3) {
    let spin = DQuat::from_rotation_y(draw.next() * std::f64::consts::TAU);
    let dip = 0.2 + 1.2 * draw.next();
    let way = spin * DVec3::new(dip.sin(), -dip.cos(), 0.0);
    match case % 5 {
        0 => (floor, DVec3::new(0.0, 0.008 + 0.06 * dip.cos(), 0.0), way),
        1 => (ball, -way * (0.058 + 0.06), way),
        2 => (
            corner,
            DVec3::new(0.0, 0.008, 0.001),
            (spin * DVec3::new(0.3, 0.2, 1.0)).normalize(),
        ),
        3 => (
            floor,
            DVec3::new(0.0, 0.008, 0.0),
            (spin * DVec3::X).normalize(),
        ),
        _ => (
            ball,
            way * -0.0012 - way.any_orthonormal_vector() * 0.058,
            way,
        ),
    }
}

#[test]
fn r11_the_rule_never_slides_a_single_contact_further_than_the_frictionless_linear_step() {
    let mut draw = Draw(0x2545_f491_4f6c_dd1d);
    let ball = solid(DAffine3::IDENTITY, Shape::Sphere { radius: 0.05 }, 1);
    let (floor, corner) = (block(0.0, 1), edge(1));
    for case in 0..2000 {
        let (shape, origin, direction) = single_contact(case, &mut draw, &floor, &ball, &corner);
        let shapes = Shapes::of(&[shape]);
        let bone = lever(origin, 0.06, direction, &shapes);
        let tangent = bone.tangent(direction);
        let pull = DVec2::new(draw.next() - 0.5, draw.next() - 0.5) * 0.4;
        let target = tangent.retract(pull);
        let problem = bone.problem();
        let mut rows = Vec::new();
        problem.assess(direction).rows_into(&tangent, &mut rows);
        let free = projected(pull, &rows, &[], DMat2::IDENTITY).map_or(pull, |found| found.0);
        let mut cones = Cones::default();
        cones.step(&rows, pull, 0.4);
        let (solved, _) = gripped(&bone, target, 0.4);
        let loaded = cones
            .contacts
            .iter()
            .filter(|contact| contact.load > 1e-12 && contact.across != DVec2::ZERO);
        for contact in loaded {
            let way =
                (tangent.across * contact.across.x + tangent.along * contact.across.y).normalize();
            let (slid, could) = (way.dot(solved.direction), way.dot(tangent.retract(free)));
            let slack = crate::TOLERANCE / 0.06;
            assert!(
                slid * could >= -slack * slack && slid.abs() <= could.abs() + slack,
                "case {case}: slid {slid} where the frictionless linear step slides {could}"
            );
        }
    }
}

/// The tangent plane at +Y with +X across it.
fn at_up() -> Tangent {
    Tangent {
        direction: DVec3::Y,
        across: DVec3::X,
        along: DVec3::Y.cross(DVec3::X),
    }
}

/// A band along +X keeping its coordinate from −0.1 to 0.2.
fn along_x() -> Grip {
    Grip {
        way: DVec3::X,
        low: -0.1,
        high: 0.2,
    }
}

#[test]
fn a_band_counts_how_far_a_coordinate_leaves_its_interval_on_either_side() {
    let band = along_x();
    let with_x = |x: f64| DVec3::new(x, (1.0 - x * x).sqrt(), 0.0);
    for (x, slip) in [(-0.4, 0.3), (0.0, 0.0), (0.5, 0.3)] {
        assert!((band.slipped(with_x(x)) - slip).abs() < 1e-12, "{x}");
    }
}

#[test]
fn a_bands_rows_hold_its_low_end_from_below_and_its_high_end_from_above() {
    let [low, high] = along_x().rows(&at_up(), 3);
    assert!((low.value - 0.1).abs() < 1e-12 && (high.value - 0.2).abs() < 1e-12);
    assert_eq!((low.gradient, high.gradient), (DVec2::X, DVec2::NEG_X));
    assert_eq!((low.normal, high.normal), (DVec3::X, DVec3::NEG_X));
    assert_eq!((low.piece, high.piece), (3, 3));
}

/// Two contacts asking for x ≥ 0.2 and x ≤ −0.2, with no pull: their cones admit no step, and the elastic step,
/// weighing both pieces alike, stays at the middle; dropping the second piece would push the bone 0.2 one way.
#[test]
fn contacts_no_step_satisfies_weigh_each_piece_alike() {
    let row = |gradient: DVec2, piece| Row {
        value: -0.2,
        gradient,
        normal: DVec3::ZERO,
        share: 1.0,
        piece,
    };
    let rows = [row(DVec2::X, 0), row(DVec2::NEG_X, 1)];
    let mut cones = Cones::default();
    let step = cones.step(&rows, DVec2::ZERO, 0.4);
    assert!(step.length() < 1e-9, "{step}");
}
