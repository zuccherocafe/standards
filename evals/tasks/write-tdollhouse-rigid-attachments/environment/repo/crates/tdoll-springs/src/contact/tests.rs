use glam::{DAffine3, DMat2, DQuat, DVec2, DVec3};

use super::*;
use crate::furniture::{Placed, Shape};

pub(super) const EPSILON: f64 = TOLERANCE;
pub(super) const COAT: f64 = 0.0586;
pub(super) const RADIUS: f64 = 0.008;

pub(super) fn solid(placement: DAffine3, shape: Shape, id: u64) -> Solid {
    Solid::new(Placed {
        placement,
        shape,
        id,
    })
}

pub(super) fn big(placement: DAffine3, id: u64) -> Solid {
    solid(
        placement,
        Shape::Box {
            half: DVec3::splat(5.0),
        },
        id,
    )
}

/// A block whose top face lies at y = `top`, reaching far below and to every side.
pub(super) fn block(top: f64, id: u64) -> Solid {
    big(
        DAffine3::from_translation(DVec3::new(0.0, top - 5.0, 0.0)),
        id,
    )
}

/// A block whose top lies at y = 0 and whose front face lies at z = 0, the core behind and below the edge on X.
pub(super) fn edge(id: u64) -> Solid {
    big(DAffine3::from_translation(DVec3::new(0.0, -5.0, -5.0)), id)
}

/// A plane through `point` facing `normal`, solid behind it.
pub(super) fn wall(normal: DVec3, point: DVec3, id: u64) -> Solid {
    let normal = normal.normalize();
    solid(
        DAffine3::IDENTITY,
        Shape::Plane {
            normal,
            distance: normal.dot(point),
        },
        id,
    )
}

/// A thin board 1 cm thick through y = 0.
pub(super) fn board(id: u64) -> Solid {
    solid(
        DAffine3::IDENTITY,
        Shape::Box {
            half: DVec3::new(0.5, 0.005, 0.5),
        },
        id,
    )
}

/// A bone of `reach` turning about `origin` with the coat's hit radius, having begun the step along `start`.
pub(super) fn lever(origin: DVec3, reach: f64, start: DVec3, shapes: &Shapes) -> Lever<'_> {
    Lever {
        origin,
        reach,
        hit: RADIUS,
        start: start.normalize(),
        frame: [DVec3::Y, DVec3::X, DVec3::Z, DVec3::Y],
        solids: shapes.near(),
    }
}

/// Plain shapes as a solver holds them, in the order of the host's names for them, all near the bone.
pub(super) struct Shapes {
    all: Vec<Solid>,
    indices: Vec<usize>,
}

impl Shapes {
    pub(super) fn of(solids: &[&Solid]) -> Self {
        let mut all: Vec<Solid> = solids.iter().map(|solid| **solid).collect();
        all.sort_by_key(|solid| solid.id);
        let indices = (0..all.len()).collect();
        Self { all, indices }
    }

    /// The shapes as a joint's lever takes them.
    pub(super) fn near(&self) -> Near<'_> {
        Near {
            all: &self.all,
            indices: &self.indices,
        }
    }
}

impl Lever<'_> {
    /// The lever with each shape's origin recorded, and fresh buffers.
    pub(crate) fn problem(&self) -> Problem<'_> {
        self.problem_in(Spare::default())
    }

    /// The bone turned from `target` out of its furniture, starting from `target` or, when its tail's path from
    /// `before` meets a shape, from the last clear direction on the arc it came along, which keeps its side.
    pub(crate) fn resolve(&self, target: DVec3, before: Option<DVec3>) -> Solved {
        let problem = self.problem();
        let start = before.map(|_| problem.assess(self.start));
        problem.resolve(target, start.as_ref())
    }

    /// The physical constraint's least over the shapes with the bone along `direction`, in metres: each shape's
    /// capsule clearance less any depth its origin already has, and its tail's clearance. Negative where it lies in one.
    pub(crate) fn physical(&self, direction: DVec3) -> f64 {
        self.problem().physical(direction)
    }
}

/// The direction `angle` from straight down, leaning towards +X.
pub(super) fn down(angle: f64) -> DVec3 {
    DVec3::new(angle.sin(), -angle.cos(), 0.0)
}

/// Every pose a case returns is a unit direction within the tolerance of every shape, unless named unresolved.
pub(super) fn feasible(lever: &Lever<'_>, outcome: &Solved) {
    assert!(
        (outcome.direction.length() - 1.0).abs() < 1e-12,
        "not a unit direction"
    );
    if !outcome.unresolved() {
        let worst = lever.physical(outcome.direction);
        assert!(worst >= -EPSILON, "left {worst} m inside");
    }
}

#[test]
fn k1_a_tail_on_a_face_turns_by_the_exact_clearing_angle_as_its_origin_nears_it() {
    let floor = block(0.0, 1);
    let mut cases = vec![
        (0.01, 11.0_f64.to_radians(), 0.0001, Some(2.6811)),
        (COAT, 5.0_f64.to_radians(), 0.004_011_283_9, Some(16.9144)),
        (COAT, 5.0_f64.to_radians(), 0.004_011_285_9, Some(16.9144)),
        (COAT, 5.0_f64.to_radians(), 0.004, Some(16.885)),
    ];
    for reach in [0.01, COAT] {
        for degrees in [5.0_f64, 20.0, 45.0, 80.0] {
            cases.push((reach, degrees.to_radians(), reach * 0.05, None));
        }
    }
    for (reach, angle, nearer, stated) in cases {
        let origin = DVec3::new(0.0, RADIUS + reach * angle.cos() - nearer, 0.0);
        let shapes = Shapes::of(&[&floor]);
        let bone = lever(origin, reach, down(angle), &shapes);
        let outcome = bone.resolve(down(angle), None);
        feasible(&bone, &outcome);
        let turn = outcome.direction.angle_between(down(angle)).to_degrees();
        let exact = ((angle.cos() - nearer / reach).acos() - angle).to_degrees();
        assert!(
            (turn - exact).abs() <= 0.01 * exact,
            "{reach} m at {angle}: {turn}° not {exact}°"
        );
        if let Some(stated) = stated {
            // The stop leaves an active contact within ε of touching: ε over the tail's rise per radian, plus rounding.
            let band = (EPSILON / (reach * (angle + exact.to_radians()).sin())).to_degrees() + 5e-4;
            assert!(
                (turn - stated).abs() <= band,
                "{turn}° not the stated {stated}° within {band}°"
            );
        }
        assert!(bone.physical(outcome.direction) <= STILL, "it floats");
    }
}

#[test]
fn k2_an_origin_wrapping_a_rounded_edge_turns_the_bone_at_one_over_the_radius() {
    // At a tangent the constraint is flat to second order, so a direction within the tolerance of touching lies up to
    // sqrt(2ε/r), about 0.9°, short of it: the turn rate is measured over 10° of wrap, where that is under 10%.
    let block = edge(1);
    let around = |angle: f64| DVec3::new(0.0, RADIUS * angle.cos(), RADIUS * angle.sin());
    let slack = (2.0 * EPSILON / RADIUS).sqrt() * 1.001;
    let step = 10.0_f64.to_radians();
    let mut last: Option<DVec3> = None;
    for index in 1..9 {
        let angle = f64::from(index) * step;
        let shapes = Shapes::of(&[&block]);
        let bone = lever(around(angle), COAT, DVec3::NEG_Y, &shapes);
        let outcome = bone.resolve(DVec3::NEG_Y, None);
        feasible(&bone, &outcome);
        let tangent = DVec3::new(0.0, -angle.sin(), angle.cos());
        assert!(
            outcome.direction.angle_between(tangent) <= slack,
            "at {angle}: off the tangent"
        );
        if let Some(last) = last {
            let rate = last.angle_between(outcome.direction) / (RADIUS * step);
            assert!(
                (rate * RADIUS - 1.0).abs() <= 0.1,
                "at {angle}: {rate} rad a metre"
            );
        }
        last = Some(outcome.direction);
    }
}

#[test]
fn k3_an_origin_lifted_off_a_rounded_edge_lets_the_bone_droop_by_the_tangent_angle() {
    let block = edge(1);
    let out = DVec3::new(0.0, 1.0, 1.0).normalize();
    let along = DVec3::new(0.0, -1.0, 1.0).normalize();
    for step in 1..=200 {
        let lifted = f64::from(step) * 1e-5;
        let shapes = Shapes::of(&[&block]);
        let bone = lever(out * (RADIUS + lifted), COAT, DVec3::NEG_Y, &shapes);
        let outcome = bone.resolve(DVec3::NEG_Y, None);
        feasible(&bone, &outcome);
        let droop = outcome.direction.angle_between(along);
        let exact = (RADIUS / (RADIUS + lifted)).acos();
        assert!(
            (droop - exact).abs() <= 0.1 * exact,
            "lifted {lifted}: {droop} not {exact}"
        );
    }
}

#[test]
fn k4_a_target_walked_along_a_face_turns_the_resting_bone_no_faster_than_itself() {
    let floor = block(0.0, 1);
    let origin = DVec3::new(0.0, RADIUS, 0.0);
    let aimed = |yaw: f64| DVec3::new(yaw.cos(), -0.1, yaw.sin()).normalize();
    let mut last: Option<(DVec3, DVec3)> = None;
    for index in 0..100 {
        let target = aimed(f64::from(index) * 0.01);
        let shapes = Shapes::of(&[&floor]);
        let bone = lever(origin, COAT, target, &shapes);
        let outcome = bone.resolve(target, None);
        feasible(&bone, &outcome);
        if let Some((before, was)) = last {
            assert!(was.angle_between(outcome.direction) <= 1.1 * before.angle_between(target));
        }
        last = Some((target, outcome.direction));
    }
}

/// The nearest direction to `target` with neither its Y nor its Z below `-least`, by enumeration in closed form.
fn nearest_in_crease(target: DVec3, least: f64) -> DVec3 {
    let allowed =
        |direction: &DVec3| direction.y >= -least - 1e-12 && direction.z >= -least - 1e-12;
    let rim = |axis: DVec3| {
        let across = (target - axis * axis.dot(target)).normalize();
        -axis * least + across * (1.0 - least * least).sqrt()
    };
    let side = (1.0 - 2.0 * least * least).sqrt() * target.x.signum();
    [
        target,
        rim(DVec3::Y),
        rim(DVec3::Z),
        DVec3::new(side, -least, -least),
    ]
    .into_iter()
    .filter(allowed)
    .max_by(|one, other| one.dot(target).total_cmp(&other.dot(target)))
    .unwrap()
}

#[test]
fn k5_a_target_crossing_a_creases_bisector_lands_on_the_nearest_allowed_direction() {
    let seat = block(0.0, 1);
    let back = big(DAffine3::from_translation(DVec3::new(0.0, 0.0, -5.0)), 2);
    let clear = 0.01;
    let origin = DVec3::new(0.0, RADIUS + clear, RADIUS + clear);
    let bound = (EPSILON / COAT).asin() + (STILL / COAT).asin();
    for degrees in (10..=80).step_by(5) {
        let angle = f64::from(degrees).to_radians();
        let target = DVec3::new(0.3, -angle.cos(), -angle.sin()).normalize();
        let shapes = Shapes::of(&[&seat, &back]);
        let bone = lever(origin, COAT, target, &shapes);
        let outcome = bone.resolve(target, None);
        feasible(&bone, &outcome);
        let exact = nearest_in_crease(target, clear / COAT);
        let off = outcome.direction.angle_between(exact);
        assert!(off <= bound, "{degrees}°: {off} rad from the nearest");
    }
}

#[test]
fn k6_a_coat_bone_over_an_edge_lies_level_and_a_nudge_barely_moves_it() {
    let block = edge(1);
    let target = DVec3::new(0.0, -0.5, 1.0).normalize();
    for behind in [0.048_f64, 0.066] {
        let origin = DVec3::new(0.0, RADIUS, -behind);
        let shapes = Shapes::of(&[&block]);
        let bone = lever(origin, COAT, target, &shapes);
        let outcome = bone.resolve(target, None);
        feasible(&bone, &outcome);
        let tilt = outcome.direction.y.asin().to_degrees();
        assert!(tilt.abs() <= 0.1, "{behind} m behind: tilted {tilt}°");
        let nudged = DVec3::new(0.0, RADIUS, f64::from_bits((-behind).to_bits() + 1));
        let shapes = Shapes::of(&[&block]);
        let moved = lever(nudged, COAT, target, &shapes).resolve(target, None);
        assert!(
            moved
                .direction
                .angle_between(outcome.direction)
                .to_degrees()
                < 1e-6
        );
    }
}

#[test]
fn k7_an_origin_sinking_into_a_face_lifts_the_tail_by_its_depth_over_the_length() {
    let floor = block(0.0, 1);
    for step in 0..=60 {
        let depth = -0.001 + f64::from(step) * 1e-4;
        let shapes = Shapes::of(&[&floor]);
        let bone = lever(
            DVec3::new(0.0, RADIUS - depth, 0.0),
            COAT,
            DVec3::X,
            &shapes,
        );
        let outcome = bone.resolve(DVec3::X, None);
        feasible(&bone, &outcome);
        let tilt = outcome.direction.y.asin();
        let exact = (depth.max(0.0) / COAT).asin();
        assert!(
            (tilt - exact).abs() <= (EPSILON / COAT).asin(),
            "{depth}: {tilt} not {exact}"
        );
    }
}

#[test]
fn k8_a_resting_bone_is_left_alone_and_one_pressed_in_or_released_takes_the_exact_clearing_turn() {
    let floor = block(0.0, 1);
    let origin = DVec3::new(0.0, RADIUS + 0.02, 0.0);
    let resting = down((0.02 / COAT).acos());
    let shapes = Shapes::of(&[&floor]);
    let bone = lever(origin, COAT, resting, &shapes);
    assert_eq!(bone.resolve(resting, None).direction, resting, "resting");
    let pressed = bone.resolve(down((0.02 / COAT).acos() - 0.2), None);
    feasible(&bone, &pressed);
    assert!(pressed.direction.angle_between(resting) <= (STILL / (COAT * 0.6)).asin());
    let released = down((0.02 / COAT).acos() + 0.3);
    let before = Some(origin + resting * COAT);
    assert_eq!(
        bone.resolve(released, before).direction,
        released,
        "released"
    );
}

#[test]
fn k9_within_the_touching_band_nothing_turns_and_below_it_the_turn_is_bounded() {
    let floor = block(0.0, 1);
    let reach = 0.01;
    for start in [0.0, -EPSILON / 2.0, -2.0 * EPSILON, -5.0 * EPSILON] {
        let shapes = Shapes::of(&[&floor]);
        let bone = lever(
            DVec3::new(0.0, RADIUS + start, 0.0),
            reach,
            DVec3::X,
            &shapes,
        );
        let turn = bone
            .resolve(DVec3::X, None)
            .direction
            .angle_between(DVec3::X);
        if start >= -EPSILON {
            assert!(turn == 0.0, "{start}: turned {turn}");
        } else {
            let least = ((-start - EPSILON) / reach).asin();
            let most = (-start / reach).asin();
            assert!(
                least - 1e-12 <= turn && turn <= most + 1e-12,
                "{start}: {turn} outside [{least}, {most}]"
            );
        }
    }
}

#[test]
fn k10_a_tail_stepping_through_a_thin_board_or_into_a_corner_keeps_its_side() {
    // Beside the board's edge, the tail steps from over it to under it: both ends clear, only the path crosses.
    let thin = board(1);
    let origin = DVec3::new(0.53, 0.03, 0.0);
    let above = DVec3::new(-1.0, -0.2, 0.0).normalize();
    let through = DVec3::new(-0.4, -1.0, 0.0).normalize();
    let shapes = Shapes::of(&[&thin]);
    let bone = lever(origin, COAT, above, &shapes);
    assert!(
        bone.physical(above) > 0.0 && bone.physical(through) > 0.0,
        "an end of the step is not clear"
    );
    let problem = bone.problem();
    assert!(
        problem.crosses(&problem.assess(above), through),
        "the tail's path misses the board"
    );
    let kept = bone.resolve(through, Some(origin + above * COAT));
    feasible(&bone, &kept);
    let tail = origin + kept.direction * COAT;
    assert!(tail.y >= 0.005 + RADIUS - 1e-6, "it went through to {tail}");
    let corner = edge(2);
    let hanging = DVec3::new(0.0, -0.004, RADIUS + 0.002);
    let into = DVec3::new(0.0, -0.3, -1.0).normalize();
    let shapes = Shapes::of(&[&corner]);
    let front = lever(hanging, COAT, DVec3::NEG_Y, &shapes);
    let side = front.resolve(into, Some(hanging + DVec3::NEG_Y * COAT));
    feasible(&front, &side);
    assert!(
        side.direction.y < -0.9,
        "it flipped onto the top, along {}",
        side.direction
    );
}

#[test]
fn k10_a_tail_path_grazing_a_board_agrees_with_one_missing_it() {
    let thin = board(1);
    let grazing = |height: f64| {
        let origin = DVec3::new(0.6, 0.005 + RADIUS + TOUCH + height + COAT, 0.0);
        let shapes = Shapes::of(&[&thin]);
        let bone = lever(origin, COAT, DVec3::NEG_Y, &shapes);
        let before = DVec3::new(-0.6, 0.005 + RADIUS + TOUCH + height, 0.0);
        bone.resolve(DVec3::new(0.0, -1.0, 0.0), Some(before))
            .direction
    };
    // The board lies wholly to one side of the tail's target, straight down: a path that grazes it and one that
    // misses it both land on the target itself.
    for (name, direction) in [("miss", grazing(1e-7)), ("graze", grazing(-1e-7))] {
        assert!(
            direction.angle_between(DVec3::NEG_Y) <= 1e-9,
            "the {name} lands at {direction}"
        );
    }
}

#[test]
fn k11_with_no_motion_and_when_separating_nothing_changes_and_nothing_sticks() {
    let floor = block(0.0, 1);
    let origin = DVec3::new(0.0, RADIUS, 0.0);
    let shapes = Shapes::of(&[&floor]);
    let bone = lever(origin, COAT, DVec3::X, &shapes);
    let before = Some(origin + DVec3::X * COAT);
    assert_eq!(bone.resolve(DVec3::X, before).direction, DVec3::X);
    let away = DVec3::new(1.0, 0.5, 0.0).normalize();
    assert_eq!(bone.resolve(away, before).direction, away, "it stuck");
}

#[test]
fn k14_the_escape_finds_a_wedge_and_a_crease_slide_and_names_a_slot_unresolved() {
    let first = wall(DVec3::X, DVec3::new(-RADIUS - 0.001, 0.0, 0.0), 1);
    let tilted = DVec3::new(-0.5, 3.0_f64.sqrt() / 2.0, 0.0);
    let second = wall(tilted, -tilted * (RADIUS - 0.001), 2);
    let shapes = Shapes::of(&[&first, &second]);
    let wedge = lever(DVec3::ZERO, 0.03, DVec3::NEG_Y, &shapes);
    let found = wedge.resolve(DVec3::NEG_Y, None);
    feasible(&wedge, &found);
    assert!(!found.unresolved(), "the wedge has a way out");
    let seat = block(0.0, 3);
    let back = wall(DVec3::Z, DVec3::ZERO, 4);
    let corner = DVec3::new(0.0, RADIUS, RADIUS + 0.002);
    let shapes = Shapes::of(&[&seat, &back]);
    let slide = lever(corner, COAT, DVec3::new(0.3, -0.1, -1.0), &shapes);
    let slid = slide.resolve(DVec3::new(0.2, -1.0, -1.0).normalize(), None);
    feasible(&slide, &slid);
    assert!(
        !slid.unresolved() && slid.direction.x > 0.9,
        "it did not fold sideways: {}",
        slid.direction
    );
    let lower = wall(DVec3::Y, DVec3::new(0.0, -0.007, 0.0), 5);
    let upper = wall(DVec3::NEG_Y, DVec3::new(0.0, 0.007, 0.0), 6);
    let shapes = Shapes::of(&[&lower, &upper]);
    let slot = lever(DVec3::ZERO, COAT, DVec3::X, &shapes);
    let stuck = slot.resolve(DVec3::new(1.0, -0.2, 0.0).normalize(), None);
    feasible(&slot, &stuck);
    assert!(
        stuck.unresolved(),
        "a slot narrower than the bone is unresolved"
    );
    assert!(
        stuck.direction.y.abs() <= 0.001 / COAT + 1e-9,
        "the least wrong way lies within a millimetre of level along the slot"
    );
}

#[test]
fn k14_near_opposed_rows_walked_across_the_determinant_threshold_stay_feasible() {
    for tilt in [0.0, 1e-7, 1e-6, 1e-5, 1e-3, 1e-2] {
        let floor = wall(DVec3::new(tilt, 1.0, 0.0), DVec3::ZERO, 1);
        let ceiling = wall(DVec3::new(tilt, -1.0, 0.0), DVec3::Y * 0.03, 2);
        let origin = DVec3::new(0.0, 0.015, 0.0);
        let shapes = Shapes::of(&[&floor, &ceiling]);
        let bone = lever(origin, COAT, DVec3::X, &shapes);
        for target in [down(0.3), DVec3::new(0.3, 1.0, 0.0).normalize()] {
            let out = bone.resolve(target, None);
            feasible(&bone, &out);
            assert!(
                !out.unresolved(),
                "{tilt}: a slot wider than the bone has a way out"
            );
        }
    }
}

#[test]
fn k14_a_target_walked_from_two_rows_to_three_moves_the_bone_steadily() {
    let floor = block(0.0, 1);
    let back = wall(DVec3::Z, DVec3::ZERO, 2);
    let side = wall(DVec3::X, DVec3::ZERO, 3);
    let origin = DVec3::new(RADIUS + 0.004, RADIUS + 0.004, RADIUS + 0.004);
    let shapes = Shapes::of(&[&floor, &back, &side]);
    let bone = lever(origin, COAT, DVec3::new(1.0, 1.0, 1.0), &shapes);
    let mut last: Option<(DVec3, DVec3)> = None;
    for step in 0..=20 {
        let across = 0.6 - f64::from(step) * 0.06;
        let target = DVec3::new(across, -1.0, -1.0).normalize();
        let out = bone.resolve(target, None);
        feasible(&bone, &out);
        assert!(!out.unresolved());
        if let Some((was, answer)) = last {
            let moved = answer.angle_between(out.direction);
            assert!(
                moved <= 1.1 * was.angle_between(target) + 1e-3,
                "step {step}: the bone jumped"
            );
        }
        last = Some((target, out.direction));
    }
}

#[test]
fn k14_two_anchored_shapes_leave_a_feasible_direction() {
    let seat = block(0.0, 3);
    let back = wall(DVec3::Z, DVec3::ZERO, 4);
    let sunk = DVec3::new(0.0, RADIUS - 0.002, RADIUS - 0.003);
    let shapes = Shapes::of(&[&seat, &back]);
    let bone = lever(sunk, COAT, DVec3::X, &shapes);
    let outcome = bone.resolve(DVec3::new(0.5, -1.0, -1.0).normalize(), None);
    assert!(!outcome.unresolved(), "{outcome:?}");
    feasible(&bone, &outcome);
    // The seat's top is y = 0 and the back's face z = 0: the tail clears both by the bone's radius.
    let tail = sunk + outcome.direction * COAT;
    assert!(
        tail.y >= RADIUS - EPSILON && tail.z >= RADIUS - EPSILON,
        "{tail}"
    );
}

#[test]
fn k16_a_bone_starting_inside_comes_out_wherever_its_length_reaches() {
    let thin = board(1);
    let shapes = Shapes::of(&[&thin]);
    let mut inside = lever(DVec3::new(0.0, 0.005, 0.0), 0.1, DVec3::X, &shapes);
    inside.hit = 0.01;
    let out = inside.resolve(DVec3::X, None);
    feasible(&inside, &out);
    let tilt = out.direction.y.asin().to_degrees();
    assert!((tilt - 5.739).abs() < 0.01, "tilted {tilt}°");
    let tail = 0.005 + out.direction.y * 0.1;
    assert!(
        (-EPSILON..=STILL).contains(&(tail - 0.015)),
        "the tail is at {tail}, not within the band of 15 mm"
    );
    let cube = solid(
        DAffine3::IDENTITY,
        Shape::Box {
            half: DVec3::splat(0.02),
        },
        2,
    );
    let ball = solid(DAffine3::IDENTITY, Shape::Sphere { radius: 0.02 }, 3);
    for shape in [&cube, &ball] {
        let shapes = Shapes::of(&[shape]);
        let middle = lever(DVec3::ZERO, 0.05, DVec3::NEG_Y, &shapes);
        let outcome = middle.resolve(DVec3::NEG_Y, None);
        feasible(&middle, &outcome);
        assert!(
            !outcome.unresolved(),
            "a medial origin within its length comes out"
        );
    }
}

/// A bone straight down onto a floor turned by `turn`, starting straight down too, so the side a tie takes is the
/// parent frame's alone; its outcome turned back.
fn head_on(turn: DQuat) -> DVec3 {
    let placement = DAffine3::from_quat(turn) * DAffine3::from_translation(DVec3::NEG_Y * 5.0);
    let floor = big(placement, 1);
    let shapes = Shapes::of(&[&floor]);
    let mut bone = lever(turn * DVec3::Y * 0.03, COAT, turn * DVec3::NEG_Y, &shapes);
    bone.frame = [
        turn * DVec3::NEG_Y,
        turn * DVec3::X,
        turn * DVec3::Z,
        turn * DVec3::Y,
    ];
    let outcome = bone.resolve(turn * DVec3::NEG_Y, None);
    feasible(&bone, &outcome);
    turn.inverse() * outcome.direction
}

#[test]
fn k16_k17_a_head_on_tie_takes_the_characters_side_whatever_the_worlds_turn() {
    let turns = [
        DQuat::IDENTITY,
        DQuat::from_rotation_y(1.3),
        DQuat::from_euler(glam::EulerRot::XYZ, 0.4, -2.0, 0.9),
    ];
    let first = head_on(DQuat::IDENTITY);
    assert!(first.x > 0.9, "folded towards the frame's X: {first}");
    for turn in turns {
        let found = head_on(turn);
        assert!(found.distance(first) < 1e-9, "{found} against {first}");
    }
}

#[test]
fn k17_a_bone_turned_head_on_into_a_face_never_divides_by_zero_and_comes_out() {
    let floor = block(0.0, 1);
    for degrees in [20.0_f64, 5.0, 1.0, 0.1, 0.0] {
        let target = down(degrees.to_radians());
        let shapes = Shapes::of(&[&floor]);
        let bone = lever(DVec3::new(0.0, 0.03, 0.0), COAT, target, &shapes);
        let outcome = bone.resolve(target, None);
        assert!(!outcome.unresolved(), "{degrees}°: {outcome:?}");
        feasible(&bone, &outcome);
        // The floor's top is y = 0: the tail rests at least the bone's radius above it.
        let tail = DVec3::new(0.0, 0.03, 0.0) + outcome.direction * COAT;
        assert!(tail.y >= RADIUS - EPSILON, "{degrees}°: {tail}");
        assert!(
            outcome.direction.x >= 0.0,
            "{degrees}°: folded the wrong way"
        );
    }
}

#[test]
fn a_touching_origin_does_not_hide_a_turn_that_carries_the_bone_past_level() {
    let block = edge(1);
    let shapes = Shapes::of(&[&block]);
    let case = lever(DVec3::new(0.0, RADIUS, -0.105), 0.53, DVec3::NEG_Y, &shapes);
    let flipped = case.resolve(DVec3::new(0.56, -0.70, 0.44).normalize(), None);
    feasible(&case, &flipped);
    assert!(
        flipped.direction.y <= 1e-4,
        "carried past level to {}",
        flipped.direction
    );
    let floor = self::block(0.0, 2);
    let shapes = Shapes::of(&[&floor]);
    let lying = lever(DVec3::new(0.0, RADIUS, 0.0), COAT, DVec3::X, &shapes);
    for dip in [0.2, 0.8, 2.0] {
        let pressed = lying.resolve(DVec3::new(1.0, -dip, 0.3).normalize(), None);
        assert!(
            pressed.direction.y.abs() <= 1e-4,
            "pressed onto a face, it rose to {}",
            pressed.direction
        );
    }
}

/// The seat and backrest of the crease example, the seat's top at y = 0 and the backrest's front at z = 0.
pub(super) fn crease() -> (Solid, Solid) {
    let seat = solid(
        DAffine3::from_translation(DVec3::new(0.0, -0.225, -0.3)),
        Shape::Box {
            half: DVec3::new(0.5, 0.225, 0.5),
        },
        1,
    );
    let back = solid(
        DAffine3::from_translation(DVec3::new(0.0, 0.25, -0.08)),
        Shape::Box {
            half: DVec3::new(0.5, 0.25, 0.08),
        },
        2,
    );
    (seat, back)
}

#[test]
fn a_bone_touching_both_faces_of_a_crease_answers_neighbouring_targets_alike() {
    let (seat, back) = crease();
    let origin = DVec3::new(0.0, RADIUS, RADIUS);
    let shapes = Shapes::of(&[&seat, &back]);
    let bone = lever(origin, COAT, DVec3::X, &shapes);
    let solve = |degrees: f64| {
        let angle = degrees.to_radians();
        let target = DVec3::new(0.1, -angle.cos(), -angle.sin()).normalize();
        bone.resolve(target, Some(origin + DVec3::X * COAT))
    };
    let (one, other) = (solve(0.00818), solve(0.00819));
    feasible(&bone, &one);
    feasible(&bone, &other);
    let apart = one.direction.angle_between(other.direction).to_degrees();
    assert!(
        apart < 0.01,
        "targets 0.00001° apart are answered {apart}° apart"
    );
    assert!(
        one.direction.angle_between(DVec3::X) < 0.01,
        "not the nearest clear direction: {}",
        one.direction
    );
}

#[test]
fn a_coat_bone_sliding_on_the_seat_with_its_tails_path_meeting_it_keeps_its_targets_yaw() {
    let seat = solid(
        DAffine3::from_translation(DVec3::new(
            -2.0,
            0.224_999_994_039_535_52,
            -1.399_999_976_158_142,
        )),
        Shape::Box {
            half: DVec3::new(1.0, 0.224_999_994_039_535_52, 0.5),
        },
        1,
    );
    let origin = DVec3::new(
        -2.098_259_223_414_783,
        0.457_999_853_685_335,
        -0.958_270_931_497_955_5,
    );
    let start = DVec3::new(
        -0.151_053_976_149_479_46,
        0.000_047_033_216_979_534_74,
        0.988_525_515_137_221_4,
    );
    let target = DVec3::new(
        -0.223_805_849_754_523_67,
        -0.177_409_159_834_877_87,
        0.958_351_152_564_830_2,
    );
    let before = DVec3::new(
        -2.108_928_621_499_427_6,
        0.458_003_175_783_355_53,
        -0.888_448_391_855_634_8,
    );
    let shapes = Shapes::of(&[&seat]);
    let mut bone = lever(origin, 0.060_831_849_253_555_29, start, &shapes);
    bone.hit = 0.007_999_992_256_293_836;
    let out = bone.resolve(target, Some(before));
    feasible(&bone, &out);
    let yaw = |direction: DVec3| direction.x.atan2(direction.z).to_degrees();
    let off = (yaw(out.direction) - yaw(target)).abs();
    assert!(off < 0.06, "held {off}° off its target's yaw");
}

#[test]
fn k13_neighbouring_targets_across_an_edge_land_clear_and_move_together() {
    let block = edge(1);
    let origin = DVec3::new(0.0, RADIUS + 0.0005, -0.003);
    let aim = |across: f64| DVec3::new(-0.9, -1.0, across).normalize();
    // Targets stepped across the edge: each answer is clear of the block, and between neighbours it turns no more
    // than their targets do, and two degrees besides, however many passes each took.
    let mut last: Option<(DVec3, DVec3)> = None;
    for across in [-0.45, -0.4, -0.35, -0.3, -0.25, -0.2, -0.15] {
        let target = aim(across);
        let shapes = Shapes::of(&[&block]);
        let bone = lever(origin, 0.04, target, &shapes);
        let out = bone.resolve(target, None);
        feasible(&bone, &out);
        assert!(bone.physical(out.direction) >= -EPSILON, "{across}: inside");
        if let Some((answer, aimed)) = last {
            let moved = answer.angle_between(out.direction).to_degrees();
            let turned = aimed.angle_between(target).to_degrees();
            assert!(
                moved <= turned + 2.0,
                "{across}: the answer moved {moved}°, its target {turned}°"
            );
        }
        last = Some((out.direction, target));
    }
}

#[test]
fn crosses_a_touching_tail_crosses_only_a_shape_its_path_heads_into() {
    let floor = block(0.0, 1);
    // The tail resting on the floor: heading down crosses, heading up or along does not.
    let lying = (RADIUS / COAT).asin();
    let resting = DVec3::new(lying.cos(), -lying.sin(), 0.0);
    let origin = DVec3::new(0.0, 2.0 * RADIUS, 0.0);
    let shapes = Shapes::of(&[&floor]);
    let bone = lever(origin, COAT, resting, &shapes);
    let problem = bone.problem();
    let start = problem.assess(resting);
    assert!(
        start
            .readings
            .first()
            .is_some_and(|reading| reading.tail.value.abs() * COAT < 1e-9),
        "the tail is not touching"
    );
    let turned = |pitch: f64| DVec3::new(pitch.cos(), -pitch.sin(), 0.0);
    assert!(
        problem.crosses(&start, turned(lying + 0.3)),
        "heading in, not crossing"
    );
    assert!(
        !problem.crosses(&start, turned(lying - 0.3)),
        "heading out, crossing"
    );
    let across = DVec3::new(
        lying.cos() * 0.9,
        -lying.sin(),
        (1.0 - 0.81_f64).sqrt() * lying.cos(),
    )
    .normalize();
    let level = DVec3::new(across.x, -lying.sin(), across.z).normalize();
    assert!(
        !problem.crosses(&start, level),
        "sliding along the floor, crossing"
    );
}

#[test]
fn crosses_a_coat_bone_resting_on_a_boards_top_beyond_its_edge_stays_on_top() {
    // OF-S5: the origin 5 mm and 10 mm beyond a 1 cm board's edge, the bone lying on its top, its target swung below.
    let thin = board(1);
    for beyond in [0.005, 0.01] {
        let origin = DVec3::new(0.5 + beyond, 0.005 + RADIUS, 0.0);
        let lying = DVec3::NEG_X;
        let shapes = Shapes::of(&[&thin]);
        let bone = lever(origin, COAT, lying, &shapes);
        let before = origin + lying * COAT;
        for pitch in [0.3_f64, 0.8, 1.3] {
            let target = DVec3::new(-pitch.cos(), -pitch.sin(), 0.0);
            let solved = bone.resolve(target, Some(before));
            let tail = origin + solved.direction * COAT;
            assert!(
                tail.y >= 0.005 + RADIUS - EPSILON,
                "{beyond} m beyond, pitch {pitch}: the tail went through to {tail}"
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

    fn count(&mut self, below: u64) -> usize {
        self.next();
        usize::try_from(self.0 % below).unwrap_or(0)
    }

    fn within(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next()
    }

    fn vector(&mut self) -> DVec2 {
        DVec2::new(self.within(-1.0, 1.0), self.within(-1.0, 1.0))
    }
}

/// A row with `value` and `gradient` and nothing for the Hessian.
fn row(value: f64, gradient: DVec2) -> Row {
    Row {
        value,
        gradient,
        normal: DVec3::Y,
        share: 1.0,
        piece: 0,
    }
}

/// A symmetric positive definite matrix, not a multiple of the identity.
fn spd(draw: &mut Draw) -> DMat2 {
    let m = DMat2::from_cols(draw.vector(), draw.vector());
    m * m.transpose() + DMat2::IDENTITY * 0.05
}

/// Whether some point of a fine grid over the disc of `radius` satisfies every row to `slack`.
fn somewhere(rows: &[Row], radius: f64, slack: f64) -> bool {
    let steps: i32 = 400;
    (0..=steps).any(|i| {
        (0..=steps).any(|j| {
            let point = DVec2::new(f64::from(i), f64::from(j)) / f64::from(steps) * 2.0 * radius
                - DVec2::splat(radius);
            rows.iter()
                .all(|row| row.value + row.gradient.dot(point) >= -slack)
        })
    })
}

/// The QP's KKT conditions to rounding for `min ½pᵀHp − q·p`.
fn kkt(rows: &[Row], hessian: DMat2, towards: DVec2, step: DVec2, multipliers: &[f64]) {
    let mut stationarity = hessian * step - towards;
    for (row, multiplier) in rows.iter().zip(multipliers) {
        let value = row.value + row.gradient.dot(step);
        assert!(value >= -1e-10, "a row is broken by {value}");
        assert!(*multiplier >= -1e-10, "a multiplier is {multiplier}");
        assert!(
            *multiplier <= 1e-9 || value.abs() <= 1e-9,
            "complementarity fails: λ {multiplier}, row {value}"
        );
        stationarity -= row.gradient * *multiplier;
    }
    assert!(
        stationarity.length() <= 1e-9,
        "Hp − q − Aᵀλ = {stationarity}"
    );
}

#[test]
fn i2_the_qp_meets_its_kkt_conditions_and_is_empty_only_when_no_point_satisfies_its_rows() {
    let mut draw = Draw(0x9e37_79b9_7f4a_7c15);
    let mut empty = 0;
    for _ in 0..400 {
        let count = draw.count(6);
        let rows: Vec<Row> = (0..count)
            .map(|_| {
                let gradient = draw.vector();
                let gradient = gradient.normalize_or(DVec2::X) * draw.within(0.3, 1.5);
                row(draw.within(-0.3, 0.3), gradient)
            })
            .collect();
        let (hessian, towards) = (spd(&mut draw), draw.vector() * 2.0);
        let Some((step, active)) = projected(towards, &rows, &[], hessian) else {
            empty += 1;
            assert!(
                !somewhere(&rows, 3.0, 1e-9),
                "empty, but a point satisfies every row"
            );
            continue;
        };
        // The active rows' multipliers, each in its row's place; an inactive row's is zero.
        let mut multipliers = vec![0.0; rows.len()];
        for (index, value) in active.into_iter().flatten() {
            *multipliers.get_mut(index).unwrap() = value;
        }
        kkt(&rows, hessian, towards, step, &multipliers);
        // No point of the grid satisfying the rows has a lower objective.
        let objective = |p: DVec2| 0.5 * p.dot(hessian * p) - towards.dot(p);
        let best = objective(step);
        for i in -40..=40 {
            for j in -40..=40 {
                let p = step + DVec2::new(f64::from(i), f64::from(j)) * 0.01;
                if rows
                    .iter()
                    .all(|row| row.value + row.gradient.dot(p) >= 0.0)
                {
                    assert!(objective(p) >= best - 1e-9, "{p} beats the QP");
                }
            }
        }
    }
    assert!(empty > 0, "no empty case drawn");
}

/// A bone set up against each kind of shape: the shape, an origin near it and a direction across it.
fn kinds() -> Vec<(Solid, DVec3, DVec3)> {
    let turned = DQuat::from_rotation_y(0.4) * DQuat::from_rotation_x(0.2);
    let place = DAffine3::from_quat(turned);
    let shapes = [
        Shape::Box {
            half: DVec3::new(0.2, 0.05, 0.2),
        },
        Shape::Cylinder {
            radius: 0.1,
            height: 0.1,
        },
        Shape::Sphere { radius: 0.05 },
        Shape::Capsule {
            radius: 0.04,
            height: 0.2,
        },
        Shape::Plane {
            normal: DVec3::Y,
            distance: 0.0,
        },
    ];
    let set_ups = [
        (DVec3::new(-0.03, 0.08, 0.01), DVec3::new(0.6, -0.5, 0.1)),
        (DVec3::new(-0.02, 0.085, 0.02), DVec3::new(0.7, -0.4, 0.2)),
        (DVec3::new(-0.04, 0.07, 0.0), DVec3::new(0.8, -0.3, 0.1)),
        (DVec3::new(-0.06, 0.05, 0.03), DVec3::new(0.9, -0.2, 0.0)),
        (DVec3::new(0.0, 0.02, 0.0), DVec3::new(0.6, -0.3, 0.2)),
    ];
    shapes
        .into_iter()
        .zip(set_ups)
        .zip(1..)
        .map(|((shape, (origin, direction)), id)| (solid(place, shape, id), origin, direction))
        .collect()
}

/// Each piece's value against the least of its rows, after a step `step` from `direction`: the linearisation error.
fn errors(bone: &Lever<'_>, direction: DVec3, step: DVec2) -> Vec<f64> {
    let problem = bone.problem();
    let at = problem.assess(direction);
    let tangent = bone.tangent(direction);
    let moved = problem.assess(tangent.retract(step));
    at.pieces()
        .copied()
        .collect::<Vec<_>>()
        .iter()
        .zip(moved.pieces())
        .filter(|(piece, _)| piece.points.iter().any(Option::is_some))
        .map(|(piece, after)| {
            let linear = piece
                .points
                .iter()
                .flatten()
                .map(|point| {
                    let row = tangent.row(point);
                    row.value + row.gradient.dot(step)
                })
                .fold(f64::INFINITY, f64::min);
            after.value - linear
        })
        .collect()
}

#[test]
fn i1_each_piece_is_tangent_to_its_rows_to_second_order_on_every_kind_of_shape() {
    for (shape, origin, direction) in kinds() {
        for pinned in [false, true] {
            let (gap, normal) = shape.signed(origin);
            let origin = if pinned {
                origin - normal * (gap - RADIUS)
            } else {
                origin
            };
            let shapes = Shapes::of(&[&shape]);
            let bone = lever(origin, COAT, direction, &shapes);
            let direction = direction.normalize();
            for way in [DVec2::X, DVec2::Y, DVec2::new(0.6, -0.8)] {
                let (small, smaller) = (
                    errors(&bone, direction, way * 2e-3),
                    errors(&bone, direction, way * 1e-3),
                );
                for (big, little) in small.iter().zip(&smaller) {
                    // Second order: halving the step quarters the error, or the error is rounding.
                    assert!(
                        little.abs() <= 1e-11 || little.abs() <= 0.3 * big.abs() + 1e-12,
                        "{shape:?} pinned {pinned}: {big} then {little}"
                    );
                }
            }
        }
    }
}

#[test]
fn eigen_keeps_its_eigenvectors_when_the_off_diagonal_is_tiny() {
    // A matrix whose off-diagonal is far below a − d: `(b, λ − a)` cancels to rounding when b is far below a − d, so each branch takes the form that
    // does not.
    for (a, b, d) in [
        (1.34, 2e-19, 0.899),
        (0.899, 2e-19, 1.34),
        (1.0, 1e-12, 0.5),
        (2.0, -3e-17, 2.0 - 1e-9),
    ] {
        let matrix = DMat2::from_cols(DVec2::new(a, b), DVec2::new(b, d));
        let (values, vectors) = eigen(matrix);
        let rebuilt = vectors * DMat2::from_diagonal(values) * vectors.transpose();
        let error = (rebuilt - matrix)
            .to_cols_array()
            .iter()
            .fold(0.0_f64, |most, each| most.max(each.abs()));
        assert!(
            error <= 1e-12 * a.max(d),
            "({a}, {b}, {d}) rebuilds {error} off"
        );
    }
}

/// The angle between two directions, in degrees.
fn degrees(from: DVec3, to: DVec3) -> f64 {
    from.angle_between(to).to_degrees()
}

#[test]
fn pinned_origins_at_an_edge_converge_within_their_band_of_the_nearest_clear_direction() {
    let shape = edge(1);
    let target = DVec3::new(0.3, -0.8, 0.5).normalize();
    // On the rounded edge, touching: within acos(r/(r+ε)); 3 mm behind, sunk 0.5 mm: within asin(ε/ℓ) at 3 mm.
    let places = [
        (
            DVec3::new(0.0, 1.0, 1.0).normalize() * RADIUS,
            (RADIUS / (RADIUS + EPSILON)).acos(),
        ),
        (
            DVec3::new(0.0, RADIUS - 0.0005, -0.003),
            (EPSILON / 0.003).asin(),
        ),
    ];
    for (origin, allowance) in places {
        for start in [
            DVec3::new(0.3, 0.3, 0.9),
            DVec3::new(0.3, 0.05, 0.95),
            DVec3::new(0.6, 0.2, 0.75),
            DVec3::new(0.2, 0.5, 0.8),
        ] {
            let shapes = Shapes::of(&[&shape]);
            let bone = lever(origin, COAT, start, &shapes);
            let problem = bone.problem();
            let solved = problem.solve(target, problem.assess(start.normalize()));
            assert_eq!(solved.settled, Settled::Converged, "{origin} from {start}");
            lands_as_near_as_clear(&bone, target, start, &solved, allowance * COAT + EPSILON);
        }
    }
}

/// The gym's seat as the beside-sofa scenario gathered it, single precision and all.
fn beside_seat() -> Solid {
    solid(
        DAffine3::from_translation(DVec3::new(
            -2.0,
            0.224_999_994_039_535_52,
            -1.399_999_976_158_142,
        )),
        Shape::Box {
            half: DVec3::new(1.0, 0.224_999_994_039_535_52, 0.5),
        },
        1,
    )
}

#[test]
fn a_long_coat_bone_beside_the_seats_front_converges_without_reaching_the_steering_cap() {
    // Replayed from the beside-sofa scenario: a 0.53 m bone hanging past the seat's front face. Its plain QP meets its
    // rows only to rounding, which steering must read as no violation.
    let seat = beside_seat();
    let target = DVec3::new(
        -0.028_351_767_364_547_7,
        -0.999_191_082_159_237_5,
        -0.028_519_442_854_981_8,
    );
    let shapes = Shapes::of(&[&seat]);
    let bone = Lever {
        origin: DVec3::new(
            -2.028_059_012_917_187_6,
            0.732_991_633_244_400_2,
            -0.886_987_012_963_980_3,
        ),
        reach: 0.529_179_347_461_292_1,
        hit: 0.024_000_000_791_926_268,
        start: target,
        frame: [
            DVec3::new(
                -0.028_351_538_111_039_524,
                -0.999_191_057_836_806_7,
                -0.028_520_522_886_188_025,
            ),
            DVec3::new(
                0.979_377_816_236_035_1,
                -0.033_476_508_921_196_44,
                0.199_244_614_519_900_53,
            ),
            DVec3::new(
                0.200_038_204_477_200_86,
                0.022_283_468_666_520_277,
                -0.979_534_666_958_615_5,
            ),
            DVec3::new(
                -0.028_351_539_892_434_163,
                -0.999_191_057_993_733_8,
                -0.028_520_515_617_541_882,
            ),
        ],
        solids: shapes.near(),
    };
    let problem = bone.problem();
    let solved = problem.solve(target, problem.assess(target));
    assert_eq!(solved.settled, Settled::Converged);
    assert!(!solved.capped, "steering reached its cap");
}

/// Whether a solve from `start` towards `target` landed where a nearest clear direction can: clear, no further from
/// the target than its start, and either at the target or held off it by a shape it touches, within `band` metres.
fn lands_as_near_as_clear(
    bone: &Lever<'_>,
    target: DVec3,
    start: DVec3,
    solved: &Solved,
    band: f64,
) {
    let clearance = bone.physical(solved.direction);
    assert!(clearance >= -EPSILON, "{clearance} m inside");
    assert!(
        degrees(solved.direction, target) <= degrees(start.normalize(), target) + 1e-9,
        "further from the target than its start"
    );
    let at_target = degrees(solved.direction, target) <= 1e-6;
    assert!(
        at_target || clearance <= band,
        "held {clearance} m off every shape, short of its target"
    );
}

/// A seat 45 cm high and a hair bone held 9.5 mm above its top, 3 mm in from its front edge, turned by its clip to
/// hang down past the edge.
#[test]
fn a_hair_bone_held_over_a_seats_front_edge_lands_as_near_its_target_as_a_clear_direction_allows() {
    let seat = solid(
        DAffine3::from_translation(DVec3::new(0.0, 0.225, -0.5)),
        Shape::Box {
            half: DVec3::new(1.0, 0.225, 0.5),
        },
        1,
    );
    let start = DVec3::new(0.994_780_044_330_540_7, 0.0, 0.102_042_458_818_511_15);
    let shapes = Shapes::of(&[&seat]);
    let bone = Lever {
        origin: DVec3::new(0.0, 0.4595, -0.003),
        reach: 0.06,
        hit: 0.01,
        start,
        frame: [DVec3::Y, DVec3::X, DVec3::Z, DVec3::Y],
        solids: shapes.near(),
    };
    let target = DVec3::new(
        0.817_054_080_004_677_9,
        -0.574_530_292_697_120_1,
        0.048_348_455_208_733_75,
    );
    let solved = bone.resolve(target, Some(start));
    assert_eq!(solved.settled, Settled::Converged);
    feasible(&bone, &solved);
    lands_as_near_as_clear(&bone, target, start, &solved, EPSILON);
}

/// A long hair lying on a table 75 cm high lands this near, in degrees, to its target turned level: its tail rests
/// on the top, so the nearest clear direction to a target below the top is the target levelled, and a twentieth of a
/// degree is the most a hair along a table can be off level unseen.
const LEVEL_DEGREES: f64 = 0.05;

/// A table top 2 cm thick and a hair bone rooted 1 cm above it, 8 mm in from its front edge, turned by its clip down
/// into the top: it lies along the top, as near its target as the top lets it.
#[test]
fn long_hair_held_down_onto_a_table_lies_along_its_top_towards_the_target() {
    let table = solid(
        DAffine3::from_translation(DVec3::new(0.0, 0.74, -0.4)),
        Shape::Box {
            half: DVec3::new(0.8, 0.01, 0.4),
        },
        1,
    );
    let start = DVec3::new(0.475_875_061_816_663_4, 0.0, 0.879_512_891_060_152_1);
    let shapes = Shapes::of(&[&table]);
    let bone = Lever {
        origin: DVec3::new(0.0, 0.76, -0.008),
        reach: 0.06,
        hit: 0.01,
        start,
        frame: [DVec3::NEG_Y, DVec3::X, DVec3::Z, DVec3::Y],
        solids: shapes.near(),
    };
    let target = DVec3::new(
        0.456_137_999_702_329_16,
        -0.292_601_092_179_408_16,
        0.840_430_083_994_483_9,
    );
    let levelled = DVec3::new(target.x, 0.0, target.z).normalize();
    let solved = bone.resolve(target, Some(start));
    assert_eq!(solved.settled, Settled::Converged);
    feasible(&bone, &solved);
    let off = degrees(solved.direction, levelled);
    assert!(off <= LEVEL_DEGREES, "{off}° from the levelled target");
}

/// A solve of a crease lands this near its resting place, in degrees, from whichever start: a hundredth of a degree
/// cannot be seen.
const SAME_REST_DEGREES: f64 = 0.01;

/// A sofa's seat and backrest meeting in a crease, and a hair bone rooted in it, turned by its clip down and back into
/// both: solved from its target and from its start, it comes to rest in the same place.
#[test]
fn a_bone_in_a_sofas_crease_rests_in_the_same_place_from_its_target_or_its_start() {
    let seat = solid(
        DAffine3::from_translation(DVec3::new(0.0, 0.225, -0.5)),
        Shape::Box {
            half: DVec3::new(1.0, 0.225, 0.5),
        },
        1,
    );
    let back = solid(
        DAffine3::from_translation(DVec3::new(0.0, 0.7, -0.48)),
        Shape::Box {
            half: DVec3::new(1.0, 0.25, 0.08),
        },
        2,
    );
    let start = DVec3::new(
        -0.967_463_545_208_435_8,
        0.025_012_001_693_996_706,
        -0.251_771_103_314_070_04,
    );
    let shapes = Shapes::of(&[&seat, &back]);
    let bone = Lever {
        origin: DVec3::new(
            0.000_165_029_408_490_214_68,
            0.4585,
            -0.374_900_982_354_905_9,
        ),
        reach: 0.06,
        hit: 0.01,
        start,
        frame: [DVec3::NEG_Y, DVec3::X, DVec3::Z, DVec3::Y],
        solids: shapes.near(),
    };
    let target = DVec3::new(
        -0.829_422_620_811_814_8,
        -0.370_262_034_422_405_5,
        -0.418_287_152_505_359_75,
    );
    // Resting in the crease, the tail lies a hit's radius above the seat's top, y = 0.45, and in front of the
    // back's face, z = -0.40: y = 0.46 and z = -0.39, which from the root 6 cm away leaves x on the target's side.
    let origin = bone.origin;
    let (y, z) = ((0.46 - origin.y) / 0.06, (-0.39 - origin.z) / 0.06);
    let rest = DVec3::new(-(1.0 - y * y - z * z).sqrt(), y, z);
    let caught = bone.resolve(target, None);
    let problem = bone.problem();
    let from_start = problem.solve(target, problem.assess(start));
    for (name, solved) in [("the target", caught), ("the start", from_start)] {
        assert_eq!(solved.settled, Settled::Converged, "from {name}");
        let apart = degrees(solved.direction, rest);
        assert!(
            apart <= SAME_REST_DEGREES,
            "from {name}: {apart}° from the crease"
        );
    }
}

#[test]
fn i6_an_exhausted_solve_returns_its_best_clear_iterate_with_that_iterates_assessment() {
    // A bone rooted 5 cm above a table, its tail touching the top, its target half a degree from straight down: the
    // top holds it off, and the walk towards the pole runs out of steps before the stop is met.
    let top = solid(
        DAffine3::from_translation(DVec3::new(0.0, -0.225, 0.0)),
        Shape::Box {
            half: DVec3::new(1.0, 0.225, 1.0),
        },
        1,
    );
    let start = DVec3::new(0.0, -0.7, 0.714_142_842_854_285);
    let shapes = Shapes::of(&[&top]);
    let bone = lever(DVec3::new(0.0, 0.05, 0.0), 0.06, start, &shapes);
    let (depth, yaw) = (89.5_f64.to_radians(), 45.0_f64.to_radians());
    let target = DVec3::new(
        depth.cos() * yaw.sin(),
        -depth.sin(),
        depth.cos() * yaw.cos(),
    );
    let problem = bone.problem();
    let solved = problem.solve(target, problem.assess(start));
    assert_eq!(solved.settled, Settled::Exhausted);
    // Its own assessment, at its own direction, and clear.
    let own = problem.assess(solved.direction);
    assert_eq!(solved.assessment.direction, solved.direction);
    assert_eq!(solved.assessment.least, own.least);
    assert!(solved.assessment.feasible && own.feasible);
    // Held off the top: a unit direction whose tail rests at least the bone's radius above it, its azimuth turned
    // part of the way from the start's, along +Z, towards the target's 45 degrees, and nearer the target than its
    // start.
    assert!((solved.direction.length() - 1.0).abs() < 1e-12);
    let tail = bone.origin + solved.direction * 0.06;
    assert!(tail.y >= RADIUS - EPSILON, "{tail}");
    let azimuth = DVec3::new(solved.direction.x, 0.0, solved.direction.z).normalize();
    let (from_start, to_target) = (
        degrees(azimuth, DVec3::Z),
        degrees(azimuth, DVec3::new(yaw.sin(), 0.0, yaw.cos())),
    );
    assert!(
        azimuth.x > 0.0 && from_start < 45.0 && to_target < 45.0,
        "{azimuth}: {from_start}° from the start's, {to_target}° from the target's"
    );
    assert!(solved.direction.dot(target) > start.normalize().dot(target));
}

#[test]
fn i7_a_solve_started_inside_a_face_by_a_hair_comes_out_of_it() {
    // A pose 2 µm inside a floor, aimed at: the stop cannot accept it, so the solve ends clear of the floor, its
    // tail at least the bone's radius above the top.
    let floor = block(0.0, 1);
    let shapes = Shapes::of(&[&floor]);
    let origin = DVec3::new(0.0, RADIUS + 0.02, 0.0);
    let bone = lever(origin, COAT, DVec3::X, &shapes);
    let sunk = (0.02 + 2e-6) / COAT;
    let into = DVec3::new((1.0 - sunk.powi(2)).sqrt(), -sunk, 0.0);
    let problem = bone.problem();
    assert!(!problem.assess(into).feasible, "the pose is not inside");
    let solved = problem.solve(into, problem.assess(into));
    assert!(!solved.unresolved(), "{solved:?}");
    let tail = origin + solved.direction * COAT;
    assert!(tail.y >= RADIUS - EPSILON, "the tail rests at {tail}");
}

/// A bone of `reach` and hit radius `hit` turning about `origin`, having begun the step along `start`.
fn bone_of(origin: DVec3, reach: f64, hit: f64, start: DVec3, shapes: &Shapes) -> Lever<'_> {
    Lever {
        hit,
        ..lever(origin, reach, start, shapes)
    }
}

#[test]
fn a_caps_margin_is_how_far_a_direction_passes_its_least() {
    let cap = Cap {
        normal: DVec3::Y,
        least: 0.25,
    };
    assert!((cap.margin(DVec3::Y) - 0.75).abs() < 1e-15);
    assert!((cap.margin(DVec3::NEG_Y) + 1.25).abs() < 1e-15);
}

#[test]
fn an_origin_piece_tied_between_two_ways_out_takes_the_first() {
    let origin = Origin {
        clearance: 0.0,
        pinned: true,
        normals: [DVec3::X, DVec3::Y, DVec3::ZERO],
        count: 2,
    };
    let (value, normal) = origin.piece(DVec3::Z);
    assert!(value.abs() < 1e-15);
    assert_eq!(normal, DVec3::X);
}

/// Three planes clear of a bone heading towards the floor give six pieces, each shape's tail then its near piece,
/// and each row names its own; two friction bands follow as pieces six and seven.
#[test]
fn each_row_names_its_piece_in_the_order_of_the_shapes_tail_then_near_then_each_band() {
    let floor = wall(DVec3::Y, DVec3::ZERO, 1);
    let side = wall(DVec3::X, DVec3::NEG_X, 2);
    let back = wall(DVec3::Z, DVec3::NEG_Z, 3);
    let shapes = Shapes::of(&[&floor, &side, &back]);
    let bone = lever(DVec3::Y, 0.5, DVec3::NEG_Y, &shapes);
    let problem = bone.problem();
    let at = problem.assess(DVec3::NEG_Y);
    assert_eq!(at.piece_count(), 6);
    let mut rows = Vec::new();
    at.rows_into(&bone.tangent(DVec3::NEG_Y), &mut rows);
    let pieces: Vec<usize> = rows.iter().map(|row| row.piece).collect();
    assert_eq!(pieces, [0, 1, 2, 3, 4, 5]);
    let mut problem = bone.problem();
    bands_along_x(&mut problem);
    let mut walk = Walk {
        at: problem.assess(DVec3::NEG_Y),
        best: None,
        weight: 1.0,
        radius: LONGEST,
        accepted: 0,
        evaluations: 1,
    };
    let mut work = Work::default();
    let _ = problem.iteration(&mut walk, &mut work, DVec3::NEG_Y);
    let bands: Vec<usize> = work.rows.iter().skip(6).map(|row| row.piece).collect();
    assert_eq!(bands, [6, 6, 7, 7], "each band a piece after the shapes'");
}

#[test]
fn the_turn_towards_the_opposite_direction_is_half_a_turn_and_towards_itself_none() {
    let tangent = Tangent {
        direction: DVec3::Y,
        across: DVec3::X,
        along: DVec3::Y.cross(DVec3::X),
    };
    let opposed = tangent.towards(DVec3::NEG_Y);
    assert!(
        opposed.distance(DVec2::new(std::f64::consts::PI, 0.0)) < 1e-12,
        "{opposed}"
    );
    assert_eq!(tangent.towards(DVec3::Y), DVec2::ZERO);
}

#[test]
fn the_least_wrong_direction_has_the_greatest_margin_and_a_tie_goes_nearest_the_start() {
    let shapes = Shapes::of(&[]);
    let bone = lever(DVec3::ZERO, 1.0, DVec3::Y, &shapes);
    let caps = [Cap {
        normal: DVec3::X,
        least: 0.25,
    }];
    assert_eq!(
        bone.least_wrong(DVec3::Y, &caps, &[DVec3::X, DVec3::Y]),
        DVec3::X
    );
    for candidates in [[DVec3::NEG_Y, DVec3::Y], [DVec3::Y, DVec3::NEG_Y]] {
        assert_eq!(
            bone.least_wrong(DVec3::NEG_Z, &caps, &candidates),
            DVec3::Y,
            "{candidates:?}"
        );
    }
}

/// Whether `candidates` hold `expected`.
fn holds(candidates: &[DVec3], expected: DVec3) -> bool {
    candidates
        .iter()
        .any(|candidate| candidate.distance(expected) < 1e-12)
}

#[test]
fn escape_candidates_take_each_rims_nearest_point_and_where_every_pair_of_rims_cross() {
    let shapes = Shapes::of(&[]);
    let bone = lever(DVec3::ZERO, 1.0, DVec3::Y, &shapes);
    let cap = |normal: DVec3, least: f64| Cap { normal, least };
    let mut found = Vec::new();
    bone.candidates(DVec3::Z, &[cap(DVec3::X, 0.5)], &mut found);
    assert!(
        holds(&found, DVec3::new(0.5, 0.0, 0.75_f64.sqrt())),
        "{found:?}"
    );
    let crossing = |z: f64| DVec3::new(0.5, 0.5, z * 0.5_f64.sqrt());
    bone.candidates(
        DVec3::Z,
        &[cap(DVec3::X, 0.5), cap(DVec3::Y, 0.5)],
        &mut found,
    );
    assert!(
        holds(&found, crossing(1.0)) && holds(&found, crossing(-1.0)),
        "{found:?}"
    );
    let caps = [cap(DVec3::X, 0.5), cap(DVec3::Z, -0.9), cap(DVec3::Y, 0.5)];
    bone.candidates(DVec3::Z, &caps, &mut found);
    assert!(
        holds(&found, crossing(1.0)),
        "the first and third: {found:?}"
    );
}

/// An origin sunk half a reach into one wall and three tenths into another, whose caps both allow the way out of
/// the first: the escape takes the allowed direction nearest the target, not the last one it considered.
#[test]
fn the_escape_takes_the_allowed_direction_nearest_its_target() {
    let tilted = DVec3::new(1.0, 1.0, 0.0).normalize();
    let first = wall(DVec3::X, DVec3::X * (0.5 - RADIUS), 1);
    let second = wall(tilted, tilted * (0.3 - RADIUS), 2);
    let shapes = Shapes::of(&[&first, &second]);
    let bone = lever(DVec3::ZERO, 1.0, DVec3::Z, &shapes);
    let escaped = bone.problem().escape(DVec3::Z);
    assert_eq!(escaped.settled, Settled::Escaped);
    let nearest = DVec3::new(0.5, 0.0, 0.75_f64.sqrt());
    assert!(
        escaped.direction.distance(nearest) < 1e-4,
        "{}",
        escaped.direction
    );
}

/// A band along X keeping its coordinate from −0.1 to 0.2, twice over.
fn bands_along_x(problem: &mut Problem<'_>) {
    let grip = Grip {
        way: DVec3::X,
        low: -0.1,
        high: 0.2,
    };
    problem.grips.extend([grip, grip]);
}

#[test]
fn a_direction_slips_by_its_coordinates_excess_over_every_band() {
    let shapes = Shapes::of(&[]);
    let bone = lever(DVec3::ZERO, 1.0, DVec3::Y, &shapes);
    let mut problem = bone.problem();
    bands_along_x(&mut problem);
    let out = DVec3::new(0.5, 0.75_f64.sqrt(), 0.0);
    assert!((problem.slipped(out) - 0.6).abs() < 1e-12);
    assert!(problem.slipped(DVec3::Y).abs() < 1e-15);
}

/// A 2 m bone lying 0.0099995 m over a box's top with a 1 cm hit radius, half a micrometre in, along the flat from
/// a quarter to three quarters of its length: both ends of the flat give the piece a row, each with its clearance in
/// tail-radians; a bone 1 cm higher has only its witness.
#[test]
fn a_bone_lying_on_a_flat_within_the_tolerance_linearises_at_both_its_ends() {
    let top = solid(
        DAffine3::IDENTITY,
        Shape::Box {
            half: DVec3::new(0.5, 0.1, 0.5),
        },
        1,
    );
    let shapes = Shapes::of(&[&top]);
    let lying = |height: f64| {
        let bone = bone_of(
            DVec3::new(-1.0, 0.1 + height, 0.0),
            2.0,
            0.01,
            DVec3::X,
            &shapes,
        );
        let at = bone.problem().assess(DVec3::X);
        at.readings
            .first()
            .map(|reading| reading.near.points)
            .unwrap()
    };
    let [witness, near] = lying(0.009_999_5).map(Option::unwrap);
    assert!((witness.share - 0.75).abs() < 1e-8, "{}", witness.share);
    assert!((near.share - 0.25).abs() < 1e-8, "{}", near.share);
    for point in [witness, near] {
        assert!((point.value + 2.5e-7).abs() < 1e-12, "{}", point.value);
    }
    assert!(
        lying(0.02)[1].is_none(),
        "a separated flat has its witness alone"
    );
}

/// The physical constraint's least, in metres, for each made-up bone, wherever the whole is moved.
#[test]
fn a_residual_is_the_physical_clearance_in_metres_counting_the_origins_allowed_depth() {
    for offset in [DVec3::ZERO, DVec3::new(3.0, 2.0, 1.0)] {
        let floor = wall(DVec3::Y, offset, 1);
        let shapes = Shapes::of(&[&floor]);
        let sunk = bone_of(offset - DVec3::Y * 0.2, 1.0, 0.1, DVec3::Y, &shapes);
        assert!(
            sunk.physical(DVec3::Y).abs() < 1e-12,
            "out of its anchoring"
        );
        assert!(
            (sunk.physical(DVec3::NEG_Y) + 1.3).abs() < 1e-12,
            "its tail 1.3 m in"
        );
        let lying = bone_of(offset + DVec3::Y * 0.2, 2.0, 0.1, DVec3::X, &shapes);
        assert!((lying.physical(DVec3::X) - 0.1).abs() < 1e-12);
        let rising = DVec3::new(0.96_f64.sqrt(), 0.2, 0.0);
        assert!(
            (lying.physical(rising) - 0.1).abs() < 1e-12,
            "nearest at its origin"
        );
        let board = solid(
            DAffine3::from_translation(offset),
            Shape::Box {
                half: DVec3::new(5.0, 0.1, 5.0),
            },
            1,
        );
        let shapes = Shapes::of(&[&board]);
        let through = bone_of(offset + DVec3::Y * 0.05, 1.0, 0.01, DVec3::NEG_Y, &shapes);
        assert!(
            (through.physical(DVec3::NEG_Y) + 0.05).abs() < RESOLUTION,
            "the board's middle 0.05 m deeper than its anchoring, to the search's resolution: {}",
            through.physical(DVec3::NEG_Y)
        );
    }
}

/// A bone beside a thin wall's front edge, its tail caught in the wall's left face, aimed at a clear direction on
/// the wall's right: caught by a quarter of a millimetre it keeps its side, and by a millimetre it meets the wall
/// afresh from its target.
#[test]
fn a_tail_caught_no_more_than_half_a_millimetre_deep_keeps_its_side() {
    let plate = solid(
        DAffine3::from_translation(DVec3::new(0.0, 0.0, 0.5)),
        Shape::Box {
            half: DVec3::new(0.005, 1.0, 0.5),
        },
        1,
    );
    let shapes = Shapes::of(&[&plate]);
    let origin = DVec3::new(-0.1, 0.0, -0.1);
    let target = DVec3::new(0.95, 0.0, 0.31).normalize();
    let ends = |depth: f64| {
        let tail_x = -(0.005 + RADIUS - depth);
        let sine = (tail_x - origin.x) / 0.5;
        let start = DVec3::new(sine, 0.0, (1.0 - sine * sine).sqrt());
        let bone = lever(origin, 0.5, start, &shapes);
        let solved = bone.resolve(target, Some(origin + start * 0.5));
        (origin + solved.direction * 0.5).x
    };
    let shallow = ends(0.000_25);
    assert!(shallow < 0.0, "a shallow catch crossed to {shallow}");
    let deep = ends(0.001);
    assert!(deep > 0.3, "a deep catch stayed at {deep}");
}

/// With bands, a clear iterate is kept as the best when its full objective, half its squared angle to the target
/// and the band's weight times its slip, is less: 0.08 for one 0.4 rad off, 0.12 for one 0.2 rad off slipping
/// 0.025, and 0.02 for one 0.2 rad off without slipping.
#[test]
fn with_friction_the_best_iterate_is_the_one_of_least_full_objective() {
    let shapes = Shapes::of(&[]);
    let bone = lever(DVec3::ZERO, 1.0, DVec3::Y, &shapes);
    let mut problem = bone.problem();
    let far = DVec3::new(0.4_f64.sin(), 0.4_f64.cos(), 0.0);
    let slipping = DVec3::new(0.0, 0.2_f64.cos(), 0.2_f64.sin());
    let near = DVec3::new(-(0.2_f64.sin()), 0.2_f64.cos(), 0.0);
    problem.grips.push(Grip {
        way: DVec3::Z,
        low: -1.0,
        high: slipping.z - 0.025,
    });
    let walk = |first: DVec3| Walk {
        at: problem.assess(first),
        best: Some(problem.assess(first)),
        weight: 1.0,
        radius: LONGEST,
        accepted: 0,
        evaluations: 1,
    };
    let best = |walk: &Walk| walk.best.as_ref().map(|best| best.direction);
    let mut kept = walk(far);
    problem.accept(&mut kept, problem.assess(slipping), DVec3::Y);
    assert_eq!(best(&kept), Some(far));
    let mut replaced = walk(slipping);
    problem.accept(&mut replaced, problem.assess(far), DVec3::Y);
    assert_eq!(best(&replaced), Some(far));
    problem.accept(&mut replaced, problem.assess(near), DVec3::Y);
    assert_eq!(best(&replaced), Some(near));
}

#[test]
fn a_tails_path_across_a_thin_plate_crosses_it_wherever_the_whole_stands() {
    for (offset, scale) in [
        (DVec3::ZERO, 1.0),
        (DVec3::new(3.0, 2.0, 1.0), 1.0),
        (DVec3::ZERO, 2.0),
    ] {
        let plate = solid(
            DAffine3::from_translation(offset + DVec3::new(0.0, 0.95, 0.0) * scale),
            Shape::Box {
                half: DVec3::new(0.005, 0.2, 1.0) * scale,
            },
            1,
        );
        let shapes = Shapes::of(&[&plate]);
        let start = DVec3::new(-0.6, 0.8, 0.0);
        let bone = lever(offset, scale, start, &shapes);
        let problem = bone.problem();
        assert!(
            problem.crosses(&problem.assess(start), DVec3::new(0.6, 0.8, 0.0)),
            "{offset} at {scale}"
        );
    }
}

#[test]
fn a_tails_path_is_traced_only_as_far_as_its_target() {
    let beyond = wall(DVec3::NEG_X, DVec3::X * 2.0, 1);
    let shapes = Shapes::of(&[&beyond]);
    let bone = lever(DVec3::ZERO, 1.0, DVec3::Y, &shapes);
    let problem = bone.problem();
    assert!(!problem.crosses(&problem.assess(DVec3::Y), DVec3::X));
}

/// A tail touching a floor 2 m up crosses it heading down, and not heading up.
#[test]
fn a_touching_tail_crosses_where_it_heads_in_wherever_the_floor_stands() {
    let floor = wall(DVec3::Y, DVec3::Y * 2.0, 1);
    let shapes = Shapes::of(&[&floor]);
    let start = DVec3::new(0.3_f64.sin(), -(0.3_f64.cos()), 0.0);
    let origin = DVec3::new(0.0, 2.0 + RADIUS + 0.3_f64.cos(), 0.0);
    let bone = lever(origin, 1.0, start, &shapes);
    let problem = bone.problem();
    let touching = problem.assess(start);
    assert!(problem.crosses(&touching, DVec3::NEG_Y));
    assert!(!problem.crosses(&touching, DVec3::new(0.6_f64.sin(), -(0.6_f64.cos()), 0.0)));
}

#[test]
fn the_least_piece_is_the_least_clearance_over_the_length() {
    let floor = wall(DVec3::Y, DVec3::ZERO, 1);
    let shapes = Shapes::of(&[&floor]);
    let above = bone_of(DVec3::Y, 1.0, 0.0, DVec3::Y, &shapes);
    assert!((above.problem().least(DVec3::Y) - 1.0).abs() < 1e-12);
    let into = bone_of(DVec3::Y * 0.5, 1.0, 0.0, DVec3::Y, &shapes);
    assert!((into.problem().least(DVec3::NEG_Y) + 0.5).abs() < 1e-12);
}

/// A tail swung over from straight up to straight down meets a plate 0.6 mm thick on its way: the last clear direction
/// stops the tail on the plate's top within the tolerance, at reaches of 4 m, 2 m and 0.5 m, not past the plate
/// where the target is clear.
#[test]
fn the_last_clear_direction_stops_at_the_first_contact_within_the_tolerance() {
    for scale in [2.0, 1.0, 0.25] {
        let plate = solid(
            DAffine3::from_translation(DVec3::new(2.0, 0.5, 0.0) * scale),
            Shape::Box {
                half: DVec3::new(0.05 * scale, 0.0003, scale),
            },
            1,
        );
        let shapes = Shapes::of(&[&plate]);
        let reach = 2.0 * scale;
        let origin = DVec3::Y * 0.5 * scale;
        let bone = bone_of(origin, reach, 0.0, DVec3::Y, &shapes);
        let problem = bone.problem();
        let found = problem.last_clear(DVec3::NEG_Y, &problem.assess(DVec3::Y));
        let tail = origin + found.direction * reach;
        let clearance = found.least * reach;
        assert!((-1e-6..=0.0).contains(&clearance), "{scale}: {clearance}");
        assert!(tail.x > 1.9 * scale, "{scale}: passed the plate to {tail}");
    }
}

/// Two square directions, a quarter turn apart: the arc's end is the second, not one turned off its great circle by
/// the rounding of their dot product.
#[test]
fn a_quarter_turn_along_the_great_circle_ends_at_its_far_direction() {
    let from = DVec3::new(1.0, 2.0, 3.0).normalize();
    let to = DVec3::new(2.0, -1.0, 0.0).normalize();
    let axis = from.cross(to).normalize();
    let half_pi = std::f64::consts::FRAC_PI_2;
    assert!(turned(from, axis * half_pi, 1.0).distance(to) < 1e-12);
    let middle = (from + to).normalize();
    assert!(turned(from, axis * half_pi, 0.5).distance(middle) < 1e-12);
}
