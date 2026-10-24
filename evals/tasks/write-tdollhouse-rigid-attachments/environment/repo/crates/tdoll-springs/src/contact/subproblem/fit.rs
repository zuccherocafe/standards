//! The multipliers at a step: stationarity's weights, bounded per piece, by an active-set least squares holding each
//! piece's sum exactly.
//!
//! The multipliers are fitted to the full stationarity equation, with the box's active edges, each violated piece's
//! weights summing to ν and each other piece's to at most ν, with no limit on the rows.

use glam::{DMat2, DVec2};

use super::Subproblem;
use crate::contact::eigen;

/// A row is attaining, or a piece violated, past this rounding.
const ATTAINS: f64 = 1e-10;

/// What a fit reuses from one to the next: its columns, its pieces' equations and the least squares' work.
#[derive(Clone, Debug, Default)]
pub(in crate::contact) struct Fitting {
    columns: Vec<Column>,
    /// Each piece's equation, by piece.
    equations: Vec<Option<usize>>,
    pieces: Vec<Equation>,
    solved: Vec<f64>,
    work: Work,
}

/// A multiplier column: its gradient, and its row and that row's piece, or neither for a box edge.
#[derive(Clone, Copy, Debug)]
struct Column {
    gradient: DVec2,
    row: Option<(usize, usize)>,
}

/// One piece with attaining rows: whether it is violated, its columns' sum then being the weight, and how many it has.
#[derive(Clone, Copy, Debug)]
struct Equation {
    violated: bool,
    columns: usize,
    weight: f64,
}

/// One unknown of the bounded fit, in units of the weight: a column's weight, or a piece at zero's slack, with its
/// gradient and its piece's equation, or none for a box edge.
#[derive(Clone, Copy, Debug)]
struct Unknown {
    gradient: DVec2,
    equation: Option<usize>,
}

/// The active-set least squares' buffers: its unknowns, their values and a trial's, which are passive, each
/// equation's pivot and level, and the passive unknowns a solve moves freely with their reduced gradients.
#[derive(Clone, Debug, Default)]
struct Work {
    unknowns: Vec<Unknown>,
    values: Vec<f64>,
    trial: Vec<f64>,
    passive: Vec<bool>,
    pivots: Vec<Option<usize>>,
    levels: Vec<(f64, f64)>,
    free: Vec<(usize, DVec2)>,
}

impl Subproblem<'_> {
    /// Each row's multiplier at `step`, into `out`, from the complete stationarity equation `Hp − q = Σw·a + Σμ·g`
    /// over the box's active edges: only a piece's attaining rows carry weight, a violated piece's summing to the
    /// weight and a piece at zero's to at most it.
    pub(in crate::contact) fn multipliers(
        &self,
        step: DVec2,
        weight: f64,
        out: &mut Vec<f64>,
        fitting: &mut Fitting,
    ) {
        out.clear();
        out.resize(self.rows.len(), 0.0);
        fitting.gather(self, step, weight);
        let wanted = self.hessian * step - self.towards;
        fitting.bounded(wanted, weight);
        for (column, value) in fitting.columns.iter().zip(&fitting.solved) {
            if let Some((row, _)) = column.row
                && let Some(slot) = out.get_mut(row)
            {
                *slot = *value;
            }
        }
    }
}

impl Fitting {
    /// The columns at `step`: every row attaining its piece, and the box's active edges, with each piece's equation.
    fn gather(&mut self, problem: &Subproblem<'_>, step: DVec2, weight: f64) {
        self.columns.clear();
        self.pieces.clear();
        self.equations.clear();
        self.equations.resize(problem.pieces + problem.bands, None);
        for (index, row) in problem.rows.iter().enumerate() {
            let value = row.value + row.gradient.dot(step);
            let short = problem.shortfall(row.piece, step);
            let violated = short > ATTAINS;
            let attaining = if violated {
                (-value - short).abs() <= ATTAINS * short.max(1.0)
            } else {
                value.abs() <= ATTAINS
            };
            let Some(slot) = self.equations.get_mut(row.piece) else {
                continue;
            };
            if !attaining {
                continue;
            }
            let equation = *slot.get_or_insert_with(|| {
                self.pieces.push(Equation {
                    violated,
                    columns: 0,
                    weight: problem.weight_of(row.piece, weight),
                });
                self.pieces.len() - 1
            });
            if let Some(piece) = self.pieces.get_mut(equation) {
                piece.columns += 1;
            }
            self.columns.push(Column {
                gradient: row.gradient,
                row: Some((index, equation)),
            });
        }
        for (axis, way) in [(0, DVec2::X), (1, DVec2::Y)] {
            let at = step[axis];
            if (at.abs() - problem.radius).abs() <= 1e-12 * problem.radius.max(1.0) {
                self.columns.push(Column {
                    gradient: way * -at.signum(),
                    row: None,
                });
            }
        }
        self.solved.clear();
        self.solved.resize(self.columns.len(), 0.0);
    }

    /// The bounded least squares: non-negative weights, over the weight, nearest stationarity with each violated
    /// piece's summing to one and each other piece's, with a slack, to one, every sum held exactly.
    fn bounded(&mut self, wanted: DVec2, weight: f64) {
        let work = &mut self.work;
        work.unknowns.clear();
        let pieces = &self.pieces;
        let scale = |equation: Option<usize>| {
            equation
                .and_then(|equation| pieces.get(equation))
                .map_or(1.0, |piece| piece.weight / weight)
        };
        work.unknowns.extend(self.columns.iter().map(|column| {
            let equation = column.row.map(|(_, equation)| equation);
            Unknown {
                gradient: column.gradient * scale(equation),
                equation,
            }
        }));
        for (equation, piece) in self.pieces.iter().enumerate() {
            if !piece.violated {
                work.unknowns.push(Unknown {
                    gradient: DVec2::ZERO,
                    equation: Some(equation),
                });
            }
        }
        work.solve(self.pieces.len(), wanted / weight);
        for ((slot, value), column) in self.solved.iter_mut().zip(&work.values).zip(&self.columns) {
            *slot = value * weight * scale(column.row.map(|(_, equation)| equation));
        }
    }
}

impl Work {
    /// The active-set least squares (Lawson and Hanson, ch. 23, with equality constraints held exactly): from each
    /// equation's last unknown carrying its sum, unknowns enter by how far their gradient of the residual passes their
    /// equation's level, and leave when a solve over the passive set would turn them negative.
    fn solve(&mut self, equations: usize, target: DVec2) {
        let count = self.unknowns.len();
        self.values.clear();
        self.values.resize(count, 0.0);
        self.passive.clear();
        self.passive.resize(count, false);
        for equation in 0..equations {
            let last = self
                .unknowns
                .iter()
                .rposition(|unknown| unknown.equation == Some(equation));
            if let Some(index) = last {
                set(&mut self.values, index, 1.0);
                set(&mut self.passive, index, true);
            }
        }
        let largest = self
            .unknowns
            .iter()
            .fold(0.0, |most: f64, each| most.max(each.gradient.length()));
        let carried: f64 = self
            .unknowns
            .iter()
            .zip(&self.values)
            .map(|(unknown, value)| unknown.gradient.length() * value)
            .sum();
        let tolerance = 1e-13 * largest * (target.length() + largest * carried);
        for _ in 0..3 * count + 3 {
            let Some(entering) = self.entering(equations, target, tolerance) else {
                return;
            };
            set(&mut self.passive, entering, true);
            for _ in 0..3 * count + 3 {
                self.over_passive(equations, target);
                if self.step_back() {
                    break;
                }
            }
            // An unknown the solve would not keep positive cannot enter: its gradient was rounding.
            if !self.passive.get(entering).copied().unwrap_or(false) {
                return;
            }
        }
    }

    /// The unknown outside the passive set whose gradient of the residual most passes its equation's level, the
    /// gradient its passive unknowns share, if any does by more than `tolerance`.
    fn entering(&mut self, equations: usize, target: DVec2, tolerance: f64) -> Option<usize> {
        let left = self
            .unknowns
            .iter()
            .zip(&self.values)
            .fold(target, |left, (unknown, value)| {
                left - unknown.gradient * *value
            });
        self.levels.clear();
        self.levels.resize(equations, (0.0, 0.0));
        for ((unknown, inside), _) in self.unknowns.iter().zip(&self.passive).zip(&self.values) {
            if let (true, Some(level)) = (
                *inside,
                unknown
                    .equation
                    .and_then(|equation| self.levels.get_mut(equation)),
            ) {
                *level = (level.0 + unknown.gradient.dot(left), level.1 + 1.0);
            }
        }
        let level = |unknown: &Unknown| {
            unknown
                .equation
                .and_then(|equation| self.levels.get(equation))
                .map_or(
                    0.0,
                    |(sum, count)| if *count > 0.0 { sum / count } else { 0.0 },
                )
        };
        self.unknowns
            .iter()
            .enumerate()
            .filter(|(index, _)| !self.passive.get(*index).copied().unwrap_or(true))
            .map(|(index, unknown)| (index, unknown.gradient.dot(left) - level(unknown)))
            .filter(|(_, gain)| *gain > tolerance)
            .max_by(|first, second| first.1.total_cmp(&second.1))
            .map(|(index, _)| index)
    }

    /// The least squares over the passive unknowns alone, into `trial`, each equation's sum held exactly: its first
    /// passive unknown, the pivot, takes what the others leave, so the others and the box's edges move freely, by the
    /// least-norm solution of the 2-row system their reduced gradients make.
    fn over_passive(&mut self, equations: usize, target: DVec2) {
        self.choose_pivots(equations);
        self.free.clear();
        let mut right = target;
        for (index, unknown) in self.unknowns.iter().enumerate() {
            if !self.passive.get(index).copied().unwrap_or(false) {
                continue;
            }
            match unknown
                .equation
                .and_then(|equation| self.pivots.get(equation))
            {
                Some(Some(pivot)) if *pivot == index => right -= unknown.gradient,
                Some(Some(pivot)) => {
                    let base = self
                        .unknowns
                        .get(*pivot)
                        .map_or(DVec2::ZERO, |each| each.gradient);
                    self.free.push((index, unknown.gradient - base));
                }
                _ => self.free.push((index, unknown.gradient)),
            }
        }
        let solved = least_norm(&self.free, right);
        self.trial.clear();
        self.trial.resize(self.unknowns.len(), 0.0);
        for (index, way) in &self.free {
            let moved = way.dot(solved);
            set(&mut self.trial, *index, moved);
            let equation = self
                .unknowns
                .get(*index)
                .and_then(|unknown| unknown.equation);
            if let Some(Some(pivot)) = equation.and_then(|equation| self.pivots.get(equation))
                && let Some(slot) = self.trial.get_mut(*pivot)
            {
                *slot -= moved;
            }
        }
        for pivot in self.pivots.iter().flatten() {
            if let Some(slot) = self.trial.get_mut(*pivot) {
                *slot += 1.0;
            }
        }
    }

    /// Each equation's pivot: its largest passive value, so what the pivot takes back loses no precision.
    fn choose_pivots(&mut self, equations: usize) {
        self.pivots.clear();
        self.pivots.resize(equations, None);
        for (index, unknown) in self.unknowns.iter().enumerate() {
            let value = self.values.get(index).copied().unwrap_or(0.0);
            if let (true, Some(pivot)) = (
                self.passive.get(index).copied().unwrap_or(false),
                unknown
                    .equation
                    .and_then(|equation| self.pivots.get_mut(equation)),
            ) && pivot.is_none_or(|kept| self.values.get(kept).copied().unwrap_or(0.0) < value)
            {
                *pivot = Some(index);
            }
        }
    }

    /// Moves `values` towards `trial`: all the way when every passive value stays positive, true; else as far as the
    /// first that reaches zero, which leaves the passive set, false. Both meet every equation, so each step does.
    fn step_back(&mut self) -> bool {
        let inside = |passive: &[bool], index: usize| passive.get(index).copied().unwrap_or(false);
        let positive = self
            .trial
            .iter()
            .enumerate()
            .all(|(index, value)| !inside(&self.passive, index) || *value > 0.0);
        if positive {
            self.values.clone_from(&self.trial);
            return true;
        }
        let share = self
            .values
            .iter()
            .zip(&self.trial)
            .enumerate()
            .filter(|(index, (_, then))| inside(&self.passive, *index) && **then <= 0.0)
            .map(|(_, (now, then))| {
                if *now > *then {
                    now / (now - then)
                } else {
                    0.0
                }
            })
            .fold(1.0, f64::min);
        for ((now, then), passive) in self
            .values
            .iter_mut()
            .zip(&self.trial)
            .zip(self.passive.iter_mut())
        {
            *now = if *passive {
                *now + share * (then - *now)
            } else {
                0.0
            };
            if *now <= 1e-15 {
                *passive = false;
            }
        }
        false
    }
}

/// The `y` whose free unknowns, each moving by `way·y`, meet `right` at least norm: through the 2×2 Gram matrix of
/// their ways, dropping a direction none of them spans.
fn least_norm(free: &[(usize, DVec2)], right: DVec2) -> DVec2 {
    let gram = free.iter().fold(DMat2::ZERO, |sum, (_, way)| {
        sum + DMat2::from_cols(*way * way.x, *way * way.y)
    });
    let (values, vectors) = eigen(gram);
    let largest = values.x.max(values.y);
    let mut solved = DVec2::ZERO;
    for (value, vector) in [(values.x, vectors.x_axis), (values.y, vectors.y_axis)] {
        if value > largest * 1e-14 && value > 0.0 {
            solved += vector * (vector.dot(right) / value);
        }
    }
    solved
}

/// Sets `slice[index]` when it has one.
fn set<T>(slice: &mut [T], index: usize, value: T) {
    if let Some(slot) = slice.get_mut(index) {
        *slot = value;
    }
}
