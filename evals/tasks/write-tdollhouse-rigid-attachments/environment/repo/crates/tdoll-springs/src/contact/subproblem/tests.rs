//! The elastic subproblem on made-up rows: hand-worked minimisers, the multipliers' contract, steering, and the cases
//! the re-checks named.

use glam::{DMat2, DVec2, DVec3};

use super::*;

/// The face walk's minimiser, with fresh buffers.
fn faces_of(problem: &Subproblem<'_>, weight: f64) -> DVec2 {
    problem.faces(weight, &mut Buffers::default())
}

/// The multipliers at `step`, freshly.
fn multipliers_of(problem: &Subproblem<'_>, step: DVec2, weight: f64) -> Vec<f64> {
    let mut out = Vec::new();
    problem.multipliers(step, weight, &mut out, &mut Fitting::default());
    out
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

    fn within(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next()
    }

    fn vector(&mut self) -> DVec2 {
        DVec2::new(self.within(-1.0, 1.0), self.within(-1.0, 1.0))
    }
}

fn row(value: f64, gradient: DVec2, piece: usize) -> Row {
    Row {
        value,
        gradient,
        normal: DVec3::Y,
        share: 1.0,
        piece,
    }
}

/// The multipliers' contract at `step`: only attaining rows carry weight, each violated piece's sum to the weight and
/// each piece at zero's to at most it, and the complete stationarity equation, box edges included, holds.
fn contract(problem: &Subproblem<'_>, step: DVec2, weight: f64, multipliers: &[f64]) {
    let mut made = DVec2::ZERO;
    for piece in 0..problem.pieces {
        made += piece_contract(problem, step, piece, weight, multipliers);
    }
    let mut left = problem.hessian * step - problem.towards - made;
    for axis in 0..2 {
        // An active box edge takes any remainder pushing outwards along its axis.
        let active = (step[axis].abs() - problem.radius).abs() <= 1e-12 * problem.radius.max(1.0);
        if active && left[axis] * step[axis] < 0.0 {
            left[axis] = 0.0;
        }
    }
    assert!(
        left.length() <= 1e-7 * (1.0 + weight),
        "stationarity is left {left}"
    );
}

/// One piece's part of the contract at `step`, and its rows' weighted gradients.
fn piece_contract(
    problem: &Subproblem<'_>,
    step: DVec2,
    piece: usize,
    weight: f64,
    multipliers: &[f64],
) -> DVec2 {
    let rows: Vec<(&Row, f64, f64)> = problem
        .rows
        .iter()
        .zip(multipliers)
        .filter(|(row, _)| row.piece == piece)
        .map(|(row, multiplier)| (row, row.value + row.gradient.dot(step), *multiplier))
        .collect();
    let short = rows.iter().map(|(_, value, _)| -value).fold(0.0, f64::max);
    let (mut sum, mut made) = (0.0, DVec2::ZERO);
    for (row, value, multiplier) in rows {
        assert!(multiplier >= -1e-9, "a negative multiplier {multiplier}");
        let attaining = if short > 1e-10 {
            (-value - short).abs() <= 1e-9
        } else {
            value.abs() <= 1e-9
        };
        assert!(
            attaining || multiplier.abs() <= 1e-9,
            "a row not attaining its piece carries {multiplier}"
        );
        sum += multiplier;
        made += row.gradient * multiplier;
    }
    if short > 1e-10 {
        assert!(
            (sum - weight).abs() <= 1e-7 * weight,
            "a violated piece's weights sum to {sum}, not {weight}"
        );
    } else {
        assert!(
            sum <= weight * (1.0 + 1e-9),
            "a piece at zero's weights sum to {sum}, past {weight}"
        );
    }
    made
}

#[test]
fn i2_the_elastic_minimiser_is_exact_and_its_multipliers_meet_their_contract() {
    // ½|p|² plus w a unit short of x ≥ 0.5 and of y ≥ 0.3, each its own piece, with no pull. At w = 1 meeting both
    // costs less than its weight: p = (0.5, 0.3), each row priced at its own margin, 0.5 and 0.3. At w = 0.25 the
    // weight is cheaper: p = (0.25, 0.25), both rows short and priced at the weight.
    let rows = [row(-0.5, DVec2::X, 0), row(-0.3, DVec2::Y, 1)];
    let problem = Subproblem {
        bands: 0,
        rows: &rows,
        pieces: 2,
        towards: DVec2::ZERO,
        hessian: DMat2::IDENTITY,
        radius: std::f64::consts::FRAC_PI_4,
    };
    for (weight, wanted, prices) in [
        (1.0, DVec2::new(0.5, 0.3), [0.5, 0.3]),
        (0.25, DVec2::new(0.25, 0.25), [0.25, 0.25]),
    ] {
        let plain = problem.plain();
        let (step, _) = problem.minimiser(weight, plain, &mut Buffers::default(), &mut Vec::new());
        assert!(step.distance(wanted) < 1e-12, "at {weight}: {step}");
        assert!(
            faces_of(&problem, weight).distance(wanted) < 1e-12,
            "at {weight}"
        );
        let multipliers = multipliers_of(&problem, step, weight);
        assert!(
            multipliers
                .iter()
                .zip(prices)
                .all(|(found, price)| (found - price).abs() < 1e-9),
            "at {weight}: {multipliers:?}"
        );
        contract(&problem, step, weight, &multipliers);
    }
}

#[test]
fn i2_a_piece_of_two_rows_weights_its_attaining_row_and_three_pieces_at_zero_share_stationarity() {
    let grouped = [row(-0.5, DVec2::X, 0), row(-0.5, DVec2::X * 0.25, 0)];
    let problem = Subproblem {
        bands: 0,
        rows: &grouped,
        pieces: 1,
        towards: DVec2::ZERO,
        hessian: DMat2::IDENTITY,
        radius: std::f64::consts::FRAC_PI_4,
    };
    let step = faces_of(&problem, 1.0);
    assert!(step.distance(DVec2::new(0.25, 0.0)) < 1e-12, "{step}");
    let multipliers = multipliers_of(&problem, step, 1.0);
    assert!(
        (multipliers.first().copied().unwrap_or(1.0)).abs() < 1e-9
            && (multipliers.last().copied().unwrap_or(0.0) - 1.0).abs() < 1e-9,
        "{multipliers:?}"
    );
    let three: Vec<Row> = (0..3)
        .map(|piece| row(0.0, DVec2::X * 0.1, piece))
        .collect();
    let problem = Subproblem {
        bands: 0,
        rows: &three,
        pieces: 3,
        towards: DVec2::new(-0.25, 0.0),
        hessian: DMat2::IDENTITY,
        radius: std::f64::consts::FRAC_PI_4,
    };
    let step = faces_of(&problem, 1.0);
    assert!(step.length() < 1e-12, "{step}");
    let multipliers = multipliers_of(&problem, step, 1.0);
    contract(&problem, step, 1.0, &multipliers);
    assert!(
        (multipliers.iter().sum::<f64>() - 2.5).abs() < 1e-9,
        "{multipliers:?}"
    );
}

#[test]
fn i2_the_subproblem_is_never_empty() {
    // Rows no step can meet together, each with the violation it cannot avoid: x ≥ 1, y ≥ 1 and x + y ≤ 1 leave a
    // total of 1 however they are traded, and x ≥ 1e-5 against -x ≥ 1e-5 leave 2e-5. Steering still finds a step in
    // the box, leaving exactly that.
    let helly = [
        row(-1.0, DVec2::X, 0),
        row(-1.0, DVec2::Y, 1),
        row(1.0, DVec2::new(-1.0, -1.0), 2),
    ];
    let opposed = [row(-1e-5, DVec2::X, 0), row(-1e-5, -DVec2::X, 1)];
    for (rows, pieces, unavoidable) in [(&helly[..], 3, 1.0), (&opposed[..], 2, 2e-5)] {
        let problem = Subproblem {
            bands: 0,
            rows,
            pieces,
            towards: DVec2::new(0.3, -0.2),
            hessian: DMat2::IDENTITY,
            radius: std::f64::consts::FRAC_PI_4,
        };
        let steered = problem.steered(1.0, &mut Buffers::default(), &mut Vec::new());
        assert!(
            steered.step.is_finite()
                && steered.step.abs().max_element() <= problem.radius * (1.0 + 1e-12)
        );
        let left = problem.violation(steered.step);
        assert!(
            (left - unavoidable).abs() <= 1e-9,
            "{left} left, {unavoidable} unavoidable"
        );
    }
}

#[test]
fn i3_steering_removes_all_violation_any_step_in_the_box_can_and_pays_for_what_it_removes() {
    // Pulled away from x ≥ 0.3 and y ≥ 0.2, each its own piece: the step (0.3, 0.2) meets both inside the box, so
    // steering removes all the violation the plain step leaves, uncapped, and its prediction pays for it.
    let rows = [row(-0.3, DVec2::X, 0), row(-0.2, DVec2::Y, 1)];
    let problem = Subproblem {
        bands: 0,
        rows: &rows,
        pieces: 2,
        towards: DVec2::new(-1.0, -1.0),
        hessian: DMat2::IDENTITY,
        radius: std::f64::consts::FRAC_PI_4,
    };
    let steered = problem.steered(1.0, &mut Buffers::default(), &mut Vec::new());
    assert!(!steered.capped, "{steered:?}");
    assert!(problem.violation(steered.step) <= 1e-12, "{steered:?}");
    assert!(
        steered.step.distance(DVec2::new(0.3, 0.2)) < 1e-9,
        "{steered:?}"
    );
    let removed = steered.violation - problem.violation(steered.step);
    assert!(
        steered.predicted >= 0.5 * steered.weight * removed - 1e-12,
        "the prediction rewards too little: {steered:?}"
    );
}

/// The multipliers at `step` with the residual of their complete equation, freshly, the fit's work held to its bound.
///
/// The fit's loops bound its least-squares solves by construction at `(3U + 3)²` for `U` unknowns, at most twice the
/// columns: quadratic, never as their subsets. Its promise is linear: each column enters and leaves a few times, so a
/// fit takes at most four solves a column; one that takes more would cost a bone with many contacts its frame.
fn fitted(problem: &Subproblem<'_>, step: DVec2, weight: f64) -> (Vec<f64>, f64) {
    let mut out = Vec::new();
    let mut fitting = Fitting::default();
    problem.multipliers(step, weight, &mut out, &mut fitting);
    let residual = residual(problem, step, weight, &out);
    (out, residual)
}

/// The complete equation's residual at `step` for each row's `multipliers`, relative to the size of its terms and to
/// the weight, so its accuracy reads alike at every scale: stationarity `Hp − q = Σw·a` left once each active box edge
/// takes what pushes outwards along it, against the size of all its terms; and each negative weight, each violated
/// piece's sum off its weight and each other's past it, against the weight.
fn residual(problem: &Subproblem<'_>, step: DVec2, weight: f64, multipliers: &[f64]) -> f64 {
    let mut left = problem.hessian * step - problem.towards;
    let mut size = (problem.hessian * step).length() + problem.towards.length();
    let mut out = 0.0;
    for (row, value) in problem.rows.iter().zip(multipliers) {
        left -= row.gradient * *value;
        size += row.gradient.length() * value.abs();
        out += (-*value).max(0.0) / weight;
    }
    for (axis, way) in [(0, DVec2::X), (1, DVec2::Y)] {
        let at = step[axis];
        if (at.abs() - problem.radius).abs() <= 1e-12 * problem.radius.max(1.0) {
            let edge = way * -at.signum();
            let taken = left.dot(edge).max(0.0);
            left -= edge * taken;
            size += taken;
        }
    }
    out += left.length() / size.max(f64::MIN_POSITIVE);
    for piece in 0..problem.pieces + problem.bands {
        let sum: f64 = problem
            .rows
            .iter()
            .zip(multipliers)
            .filter(|(row, _)| row.piece == piece)
            .map(|(_, value)| *value)
            .sum();
        let cap = problem.weight_of(piece, weight);
        out += if problem.shortfall(piece, step) > 1e-10 {
            (sum - cap).abs()
        } else {
            (sum - cap).max(0.0)
        } / cap;
    }
    out
}

#[test]
fn fit_takes_every_attaining_row_and_the_boxs_edge_however_many() {
    // Thirteen violated pieces whose rows never change, and a turn the box stops: each piece's one row carries the
    // weight, and the box's edge takes the rest of stationarity.
    for count in [13, 20] {
        let rows: Vec<Row> = (0..count)
            .map(|piece| row(-1.0, DVec2::ZERO, piece))
            .collect();
        let problem = Subproblem {
            bands: 0,
            rows: &rows,
            pieces: count,
            towards: DVec2::new(1.0, 0.0),
            hessian: DMat2::IDENTITY,
            radius: 0.5,
        };
        let step = faces_of(&problem, 1.0);
        assert!(step.distance(DVec2::new(0.5, 0.0)) < 1e-12, "{step}");
        let (multipliers, residual) = fitted(&problem, step, 1.0);
        contract(&problem, step, 1.0, &multipliers);
        assert!(residual <= 1e-9, "{count}: left {residual}");
    }
    // Fourteen identical rows at zero, each its own piece, sharing a stationarity no thirteen could meet under the
    // cap, with a Hessian that is not the identity.
    let rows: Vec<Row> = (0..14)
        .map(|piece| row(0.0, DVec2::X * 0.1, piece))
        .collect();
    let problem = Subproblem {
        bands: 0,
        rows: &rows,
        pieces: 14,
        towards: DVec2::new(-1.35, 0.0),
        hessian: DMat2::from_cols(DVec2::new(2.0, 0.5), DVec2::new(0.5, 1.0)),
        radius: 0.5,
    };
    let step = faces_of(&problem, 1.0);
    assert!(step.length() < 1e-12, "{step}");
    let (multipliers, residual) = fitted(&problem, step, 1.0);
    contract(&problem, step, 1.0, &multipliers);
    assert!(residual <= 1e-9, "left {residual}");
}

#[test]
fn i3_a_step_the_cap_stopped_short_of_paying_for_what_it_removes_is_marked_capped() {
    // A row a nanometre violated: removing its nanometre of violation takes a step whose model cost the reward at the cap's weight
    // never repays, so steering stops at the cap on the reward and must say so.
    let rows = [row(-1e-9, DVec2::X * 1e-8, 0)];
    let problem = Subproblem {
        bands: 0,
        rows: &rows,
        pieces: 1,
        towards: DVec2::new(-0.75, 0.0),
        hessian: DMat2::IDENTITY,
        radius: 0.5,
    };
    let steered = problem.steered(1.0, &mut Buffers::default(), &mut Vec::new());
    let removed = steered.violation - problem.violation(steered.step);
    assert!(
        steered.predicted < 0.5 * steered.weight * removed,
        "the reward is met: {steered:?}"
    );
    assert!(steered.capped, "{steered:?}");
}

#[test]
fn faces_a_narrow_cells_free_minimiser_is_found() {
    // Cells 10 nanoradians wide under a stiff Hessian: the minimiser is the free one, inside the narrow
    // square, which a probe stepping further than the cell is wide reads from the wrong neighbour.
    let rows = [
        row(-5e-9, DVec2::X, 0),
        row(15e-9, -DVec2::X, 1),
        row(-5e-9, DVec2::Y, 2),
        row(15e-9, -DVec2::Y, 3),
        row(-0.1, DVec2::ZERO, 4),
    ];
    let problem = Subproblem {
        bands: 0,
        rows: &rows,
        pieces: 5,
        towards: DVec2::ONE,
        hessian: DMat2::from_diagonal(DVec2::splat(1e8)),
        radius: 0.5,
    };
    // The cell spans 5 to 15 nanoradians each way; the pull of 1 under 1e8 frees the step at 1e-8 both ways.
    let step = faces_of(&problem, 1.0);
    assert!(step.distance(DVec2::splat(1e-8)) < 1e-12, "{step}");
}

#[test]
fn i3_a_step_adding_violation_only_within_rounding_keeps_its_weight() {
    // Rows met at no step whose plain answer slides along them: rounding can leave the step a hair of violation, which
    // no weight rewards, so steering must not raise the weight for it, as the sit's capped steps did.
    let mut draw = Draw(0x6a09_e667_f3bc_c908);
    let mut rounded = 0;
    for _ in 0..4000 {
        let rows = [
            row(
                0.0,
                draw.vector().normalize_or(DVec2::X) * draw.within(0.1, 1.0),
                0,
            ),
            row(
                draw.within(0.0, 1e-3),
                draw.vector().normalize_or(DVec2::Y),
                1,
            ),
        ];
        let problem = Subproblem {
            bands: 0,
            rows: &rows,
            pieces: 2,
            towards: draw.vector() * draw.within(1e-12, 1e-9),
            hessian: DMat2::IDENTITY,
            radius: 0.5,
        };
        let steered = problem.steered(1.0, &mut Buffers::default(), &mut Vec::new());
        if problem.violation(steered.step) > steered.violation {
            rounded += 1;
            assert!(!steered.capped && steered.weight <= 1.0, "{steered:?}");
        }
    }
    assert!(rounded > 0, "no step added rounding");
}

/// The fit's certificate is met to this relative residual: rounding in a few two-by-two solves, far below any weight a
/// bone's step could feel.
const CERTIFIED: f64 = 1e-9;

#[test]
fn fit_meets_its_certificate_at_penalty_weights_up_to_1e8() {
    // Pieces at zero with rows along X and Y, and multipliers well under the weight: stationarity holds exactly
    // whatever weight steering has raised the penalty to, up to the largest it reaches.
    let one = [row(0.0, DVec2::X, 0), row(0.0, DVec2::Y, 0)];
    let two = [
        row(0.0, DVec2::X, 0),
        row(0.0, DVec2::Y, 0),
        row(0.0, DVec2::X, 1),
        row(0.0, DVec2::Y, 1),
    ];
    for (rows, pieces, wanted) in [
        (&one[..], 1, DVec2::new(0.3, 0.2)),
        (&two[..], 2, DVec2::new(0.03, 0.02)),
    ] {
        let problem = Subproblem {
            bands: 0,
            rows,
            pieces,
            towards: -wanted,
            hessian: DMat2::IDENTITY,
            radius: 0.5,
        };
        for weight in [1.0, 1e4, 1e7, MOST_WEIGHT] {
            let (multipliers, residual) = fitted(&problem, DVec2::ZERO, weight);
            contract(&problem, DVec2::ZERO, weight, &multipliers);
            assert!(
                residual <= CERTIFIED,
                "{pieces} pieces at weight {weight:e}: left {residual}, {multipliers:?}"
            );
        }
    }
}
