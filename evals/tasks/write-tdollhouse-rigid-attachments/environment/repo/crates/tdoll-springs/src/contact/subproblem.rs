//! Each step's subproblem: the trust-region elastic ℓ1 QP, its multipliers and its steered penalty weight.
//!
//! The model is the step towards its target under the Lagrangian's Hessian, its eigenvalues floored, plus a weight ν
//! times each piece's linearised shortfall, each piece counted once, within the trust region's box. It is never empty
//! and its multipliers never exceed ν. The plain programme over the rows is tried first and kept when its own
//! multipliers, each piece's summing to at most ν, make it the model's minimiser; otherwise the minimiser is found over
//! the faces of the arrangement of the rows' zero lines and the box, each face read from the cells on either side of it
//! however narrow, and its multipliers are fitted as `fit` says. ν is steered at every iterate, never read off the
//! multipliers: raised tenfold, up to 10⁸, until the step removes all the linearised shortfall that any step in the box
//! can, each piece to the plain programme's rounding, or at least a tenth of what it could, and the predicted reduction
//! is at least half ν times the shortfall it removes; a step either test still fails at the cap goes on as it is and is
//! counted as capped.

use glam::{DMat2, DVec2};

use super::friction::BAND;
use super::{Active, Row, box_rows, projected};

mod fit;

pub(super) use fit::Fitting;

/// Steering raises the penalty weight tenfold at a time up to this.
const MOST_WEIGHT: f64 = 1e8;
/// Linearised violation at most this is none: the rounding the plain QP checks its rows to.
const NONE: f64 = 1e-12;
/// The rows of one step's subproblem in a box `|p|∞ ≤ radius`, grouped by piece, with the objective's turn
/// `towards` and Hessian: `pieces` furniture pieces, then `bands` friction bands, each a piece of two rows whose
/// shortfall the objective weighs by the fixed weight `BAND`, outside the constraints.
#[derive(Clone, Copy, Debug)]
pub(super) struct Subproblem<'r> {
    pub(super) rows: &'r [Row],
    pub(super) pieces: usize,
    pub(super) bands: usize,
    pub(super) towards: DVec2,
    pub(super) hessian: DMat2,
    pub(super) radius: f64,
}

/// What a subproblem reuses from one step to the next: the arrangement's lines and a line's crossing shares.
#[derive(Clone, Debug, Default)]
pub(super) struct Buffers {
    lines: Vec<Line>,
    shares: Vec<f64>,
    pub(super) fitting: Fitting,
}

/// A steered step: the step, the weight it was taken at, the model's predicted reduction, the linearised violation at
/// no step and the least any step in the box reaches, and whether the weight hit its cap. Each row's multiplier is
/// written beside the rows.
#[derive(Clone, Copy, Debug)]
pub(super) struct Steered {
    pub(super) step: DVec2,
    pub(super) weight: f64,
    pub(super) predicted: f64,
    pub(super) violation: f64,
    pub(super) least: f64,
    pub(super) capped: bool,
}

/// A line `normal·p = offset` of the arrangement.
type Line = (DVec2, f64);

impl Subproblem<'_> {
    /// The linearised violation at `step`: each piece's largest row shortfall, counted once.
    pub(super) fn violation(&self, step: DVec2) -> f64 {
        (0..self.pieces)
            .map(|piece| self.shortfall(piece, step))
            .sum()
    }

    /// Whether every piece is met at `step` to the plain QP's rounding, piece by piece as it checks its rows.
    pub(super) fn met(&self, step: DVec2) -> bool {
        (0..self.pieces).all(|piece| self.shortfall(piece, step) <= NONE)
    }

    /// The bands' linearised shortfall at `step`: how far each band's tangential coordinate lies outside its interval.
    pub(super) fn band(&self, step: DVec2) -> f64 {
        (self.pieces..self.pieces + self.bands)
            .map(|piece| self.shortfall(piece, step))
            .sum()
    }

    /// The weight piece `piece`'s shortfall carries: `weight` for furniture, `BAND` for a band.
    fn weight_of(&self, piece: usize, weight: f64) -> f64 {
        if piece < self.pieces { weight } else { BAND }
    }

    /// Piece `piece`'s linearised shortfall at `step`, zero when met.
    fn shortfall(&self, piece: usize, step: DVec2) -> f64 {
        self.rows
            .iter()
            .filter(|row| row.piece == piece)
            .map(|row| -(row.value + row.gradient.dot(step)))
            .fold(0.0, f64::max)
    }

    /// The elastic model `−q·p + ½pᵀHp + weight·violation + BAND·band` at `step`.
    pub(super) fn model(&self, step: DVec2, weight: f64) -> f64 {
        0.5 * step.dot(self.hessian * step) - self.towards.dot(step)
            + weight * self.violation(step)
            + BAND * self.band(step)
    }

    /// The step steered by a test on the proposed step: if some step in the box removes the
    /// linearised violation, the step must; otherwise it must remove a tenth of what the best can. Then the model's
    /// reduction must be at least half the weight times the violation removed. The weight starts at `weight` and
    /// rises tenfold until both hold, up to its cap, where the step goes on as it is and is marked capped when either
    /// test still fails. Each row's multiplier at the step is written into `multipliers`.
    pub(super) fn steered(
        &self,
        weight: f64,
        buffers: &mut Buffers,
        multipliers: &mut Vec<f64>,
    ) -> Steered {
        let violation = self.violation(DVec2::ZERO);
        // A plain QP's answer meets every row inside the box, so the least violation is none without the scan.
        let plain = self.plain();
        let least = if violation <= NONE || plain.is_some() {
            0.0
        } else {
            self.violation(self.least_violation(buffers))
        };
        let mut weight = weight;
        let (mut step, mut fast) = self.minimiser(weight, plain, buffers, multipliers);
        while !self.needed(step, violation, least) && weight < MOST_WEIGHT {
            weight *= 10.0;
            (step, fast) = self.minimiser(weight, plain, buffers, multipliers);
        }
        while !self.rewarded(step, weight, violation) && weight < MOST_WEIGHT {
            weight *= 10.0;
            (step, fast) = self.minimiser(weight, plain, buffers, multipliers);
        }
        // The fast path's multipliers are the plain QP's own; a face's minimiser has its fitted here.
        if !fast {
            self.multipliers(step, weight, multipliers, &mut buffers.fitting);
        }
        Steered {
            step,
            weight,
            predicted: self.predicted(step, weight, violation),
            violation,
            least,
            capped: !self.needed(step, violation, least) || !self.rewarded(step, weight, violation),
        }
    }

    /// The plain QP's answer over the rows and the box, with its active rows' multipliers: over the rows alone when
    /// that lands inside the box, which is the usual case and the box then inactive, else with the box's edges among
    /// the rows, numbered after them.
    pub(super) fn plain(&self) -> Option<(DVec2, Active)> {
        let within = |found: &(DVec2, Active)| found.0.abs().max_element() <= self.radius;
        projected(self.towards, self.rows, &[], self.hessian)
            .filter(within)
            .or_else(|| {
                projected(
                    self.towards,
                    self.rows,
                    &box_rows(self.radius),
                    self.hessian,
                )
            })
    }

    /// The elastic model's minimiser in the box at `weight`. The plain QP over the rows and the box, `plain`, is it
    /// when multipliers whose sums per piece are at most the weight make it stationary, by the exact-penalty property;
    /// otherwise the least model over the faces of the line arrangement.
    pub(super) fn minimiser(
        &self,
        weight: f64,
        plain: Option<(DVec2, Active)>,
        buffers: &mut Buffers,
        multipliers: &mut Vec<f64>,
    ) -> (DVec2, bool) {
        if let Some((step, active)) = plain
            && self.certifies(active, weight, multipliers)
        {
            return (step, true);
        }
        (self.faces(weight, buffers), false)
    }

    /// Whether the plain QP's own multipliers, `active`, certify its step as the elastic model's minimiser at
    /// `weight`: none negative, and each piece's sum at most its weight, by the exact-penalty property. When they do,
    /// they are each row's multiplier, written into `multipliers`.
    fn certifies(&self, active: Active, weight: f64, multipliers: &mut Vec<f64>) -> bool {
        let active = active.map(|each| each.map(|found| self.flipped(found)));
        let mut sums: [Option<(usize, f64)>; 2] = [None; 2];
        for (index, value) in active.into_iter().flatten() {
            if value < -1e-12 * (1.0 + weight) {
                return false;
            }
            let Some(row) = self.rows.get(index) else {
                continue;
            };
            if let Some(slot) = sums
                .iter_mut()
                .find(|slot| slot.is_none_or(|(piece, _)| piece == row.piece))
            {
                let sum = slot.map_or(0.0, |(_, sum)| sum);
                *slot = Some((row.piece, sum + value.max(0.0)));
            }
        }
        let past =
            |&(piece, sum): &(usize, f64)| sum > self.weight_of(piece, weight) * (1.0 + 1e-12);
        if sums.iter().flatten().any(past) {
            return false;
        }
        multipliers.clear();
        multipliers.resize(self.rows.len(), 0.0);
        for (index, value) in active.into_iter().flatten() {
            if let Some(slot) = multipliers.get_mut(index) {
                *slot = value.max(0.0);
            }
        }
        true
    }

    /// A row's multiplier as the plain QP found it, moved to its band's other row where that row is the same line
    /// facing the other way, as a stuck band's two rows are: the QP picks either at their shared candidate, and a
    /// negative weight on one is the same weight on the other.
    fn flipped(&self, (index, value): (usize, f64)) -> (usize, f64) {
        let Some(row) = self
            .rows
            .get(index)
            .filter(|row| row.piece >= self.pieces && value < 0.0)
        else {
            return (index, value);
        };
        let partner = self.rows.iter().enumerate().find(|(other, each)| {
            *other != index
                && each.piece == row.piece
                && each.gradient == -row.gradient
                && (each.value + row.value).abs() <= NONE
        });
        partner.map_or((index, value), |(other, _)| (other, -value))
    }

    /// The rows that can be violated somewhere in the box; the rest never change the model.
    fn live(&self) -> impl Iterator<Item = &Row> + '_ {
        self.rows.iter().filter(|row| {
            row.value - self.radius * (row.gradient.x.abs() + row.gradient.y.abs()) < 0.0
        })
    }

    /// The arrangement's lines, into `lines`: each live row's zero line, the switch line between two live rows of one
    /// piece, and the box's four edges.
    fn lines(&self, lines: &mut Vec<Line>) {
        lines.clear();
        lines.extend(
            self.live()
                .filter(|row| row.gradient.length_squared() > 1e-24)
                .map(|row| (row.gradient, -row.value)),
        );
        for (index, first) in self.live().enumerate() {
            for second in self.live().skip(index + 1) {
                let normal = first.gradient - second.gradient;
                if first.piece == second.piece && normal.length_squared() > 1e-24 {
                    lines.push((normal, -(first.value - second.value)));
                }
            }
        }
        for axis in [DVec2::X, DVec2::Y] {
            lines.push((axis, self.radius));
            lines.push((axis, -self.radius));
        }
    }

    /// The step in the box with the least linearised violation: convex and piecewise linear, so least at a vertex of
    /// the arrangement or at no step.
    fn least_violation(&self, buffers: &mut Buffers) -> DVec2 {
        self.lines(&mut buffers.lines);
        let lines = &buffers.lines;
        let mut best = (DVec2::ZERO, self.violation(DVec2::ZERO));
        for (index, first) in lines.iter().enumerate() {
            for second in lines.iter().skip(index + 1) {
                if let Some(point) = self.crossing(*first, *second) {
                    let left = self.violation(point);
                    if left < best.1 {
                        best = (point, left);
                    }
                }
            }
        }
        best.0
    }

    /// Where two lines cross inside the box, if they do.
    fn crossing(&self, (first, at): Line, (second, by): Line) -> Option<DVec2> {
        let matrix = DMat2::from_cols(first, second).transpose();
        let determinant = matrix.determinant();
        if determinant.abs() <= 1e-12 * first.length() * second.length() {
            return None;
        }
        let point = matrix.inverse() * DVec2::new(at, by);
        (point.abs().max_element() <= self.radius * (1.0 + 1e-12)).then_some(point)
    }

    /// The model's gradient term in the cell around `point`: the turn plus the weight times each violated piece's
    /// attaining row.
    fn cell(&self, point: DVec2, weight: f64) -> DVec2 {
        let mut out = self.towards;
        for piece in 0..self.pieces + self.bands {
            let attaining = self
                .rows
                .iter()
                .filter(|row| row.piece == piece)
                .map(|row| (-(row.value + row.gradient.dot(point)), row.gradient))
                .fold((0.0, DVec2::ZERO), |best, each| {
                    if each.0 > best.0 { each } else { best }
                });
            out += attaining.1 * self.weight_of(piece, weight);
        }
        out
    }

    /// The model's least over the faces of the arrangement: walking each line clipped to the box, every vertex,
    /// and for each segment between crossings, its two neighbouring cells' free minimisers and their minimisers along
    /// the line, kept within the segment. Exact, since the minimiser lies in some face's relative interior.
    pub(super) fn faces(&self, weight: f64, buffers: &mut Buffers) -> DVec2 {
        self.lines(&mut buffers.lines);
        let Buffers { lines, shares, .. } = buffers;
        let mut best = (DVec2::ZERO, self.model(DVec2::ZERO, weight));
        let mut consider = |point: DVec2| {
            if point.abs().max_element() <= self.radius * (1.0 + 1e-12) {
                let value = self.model(point, weight);
                if value < best.1 {
                    best = (point, value);
                }
            }
        };
        for (normal, offset) in lines.iter() {
            let length = normal.length();
            let (normal, offset) = (*normal / length, offset / length);
            let along = normal.perp();
            let base = normal * offset;
            let Some((low, high)) = self.clip(base, along) else {
                continue;
            };
            shares.clear();
            shares.extend([low, high]);
            for (other, other_offset) in lines.iter() {
                let towards = other.dot(along);
                if towards.abs() > 1e-15 {
                    let share = (other_offset - other.dot(base)) / towards;
                    if low < share && share < high {
                        shares.push(share);
                    }
                }
            }
            shares.sort_by(f64::total_cmp);
            for pair in shares.windows(2) {
                if let (Some(&first), Some(&second)) = (pair.first(), pair.last()) {
                    self.segment(
                        (normal, offset),
                        first,
                        second,
                        weight,
                        lines,
                        &mut consider,
                    );
                }
            }
            if let Some(&last) = shares.last() {
                consider(base + along * last);
            }
        }
        best.0
    }

    /// One segment of the line `normal·p = offset` between the shares `first` and `second`: its first vertex, and
    /// when it has length, its two neighbouring cells' free minimisers and their minimisers along the line, kept
    /// within the segment, each handed to `consider`. The cells are read half way to the nearest of the other `lines`
    /// square to this one, so each probe lies in the cell beside the segment however narrow it is.
    fn segment(
        &self,
        (normal, offset): Line,
        first: f64,
        second: f64,
        weight: f64,
        lines: &[Line],
        consider: &mut dyn FnMut(DVec2),
    ) {
        let (along, base) = (normal.perp(), normal * offset);
        consider(base + along * first);
        if second - first <= 1e-15 {
            return;
        }
        let inverse = self.hessian.inverse();
        let middle = base + along * f64::midpoint(first, second);
        // No other line crosses this one inside the segment, so only lines through the middle itself, this line again,
        // are passed over.
        let side = 0.5
            * lines
                .iter()
                .filter_map(|(other, at)| {
                    let (across, gap) = (other.dot(normal), at - other.dot(middle));
                    (across.abs() > 1e-15 && gap.abs() > 1e-15 * (other.length() + at.abs()))
                        .then(|| (gap / across).abs())
                })
                .fold(2.0 * self.radius, f64::min);
        for sign in [1.0, -1.0] {
            let gradient = self.cell(middle + normal * (side * sign), weight);
            consider(inverse * gradient);
            let lift = (offset - normal.dot(inverse * gradient)) / normal.dot(inverse * normal);
            let on = inverse * (gradient + normal * lift);
            consider(base + along * (on - base).dot(along).clamp(first, second));
        }
    }

    /// The shares along `along` from `base` that lie in the box, if any do.
    fn clip(&self, base: DVec2, along: DVec2) -> Option<(f64, f64)> {
        let (mut low, mut high) = (f64::NEG_INFINITY, f64::INFINITY);
        for (start, way) in [(base.x, along.x), (base.y, along.y)] {
            if way.abs() < 1e-15 {
                if start.abs() > self.radius * (1.0 + 1e-12) {
                    return None;
                }
            } else {
                let (first, second) = ((-self.radius - start) / way, (self.radius - start) / way);
                low = low.max(first.min(second));
                high = high.min(first.max(second));
            }
        }
        (low <= high).then_some((low, high))
    }
}

impl Subproblem<'_> {
    /// Steering's first test: whether `step` removes all the violation a step in the box can, to the plain QP's
    /// rounding, or else a tenth of what the best can, from the iterate's linearised `violation` towards the `least`
    /// any step in the box reaches.
    fn needed(&self, step: DVec2, violation: f64, least: f64) -> bool {
        if least <= NONE {
            self.met(step)
        } else {
            violation - self.violation(step) >= 0.1 * (violation - least)
        }
    }

    /// The model's predicted reduction for `step` at `weight` from the iterate's linearised `violation`, the bands'
    /// included.
    fn predicted(&self, step: DVec2, weight: f64, violation: f64) -> f64 {
        self.towards.dot(step) - 0.5 * step.dot(self.hessian * step)
            + weight * (violation - self.violation(step))
            + BAND * (self.band(DVec2::ZERO) - self.band(step))
    }

    /// Steering's second test: whether the predicted reduction pays half the weight times the violation `step` removes
    /// from the iterate's linearised `violation`. A step removing none has a prediction no weight changes, so it holds.
    fn rewarded(&self, step: DVec2, weight: f64, violation: f64) -> bool {
        let removed = violation - self.violation(step);
        removed <= 0.0 || self.predicted(step, weight, violation) >= 0.5 * weight * removed - 1e-18
    }
}

#[cfg(test)]
mod tests;
