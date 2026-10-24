//! Searches along an arc or a bone, narrowing a bracket by Illinois with Dekker's minimum step.

/// The most trials a level crossing spends; it typically needs four to six.
const CROSSING_TRIALS: usize = 20;
/// The most trials an ordered search spends: it halves at least every second trial, and 33 halvings narrow a bracket
/// to a hundred millionth of a share.
const ORDERED_TRIALS: usize = 72;

/// A bracket being narrowed: its low end, where the measure less its aim is at most zero, its high end, where it is
/// above, which end was kept by the last trial, and how many trials have passed since its width last halved.
struct Bracket {
    low: (f64, f64),
    high: (f64, f64),
    low_kept: Option<bool>,
    reference: f64,
    since: usize,
}

impl Bracket {
    fn new(low: (f64, f64), high: (f64, f64)) -> Self {
        Self {
            low,
            high,
            low_kept: None,
            reference: (high.0 - low.0).abs(),
            since: 0,
        }
    }

    fn width(&self) -> f64 {
        (self.high.0 - self.low.0).abs()
    }

    /// The next share to try: the secant, at least `step` off either end, or the middle when the last trial did not
    /// halve the bracket, so it at least halves every second trial.
    fn trial(&self, step: f64) -> f64 {
        let ((low, low_value), (high, high_value)) = (self.low, self.high);
        let middle = f64::midpoint(low, high);
        if self.since >= 1 {
            return middle;
        }
        let gap = high - low;
        let off = step.min(gap.abs() * 0.5).copysign(gap);
        let (first, last) = (low + off, high - off);
        let secant = low - low_value * gap / (high_value - low_value);
        if secant.is_finite() {
            secant.clamp(first.min(last), first.max(last))
        } else {
            middle
        }
    }

    /// The trial at `share`, whose measure less the aim is `value`, replaces the end on its side. An end kept twice
    /// running has its value halved, which is Illinois's cure for regula falsi's stall.
    fn take(&mut self, share: f64, value: f64) {
        if value <= 0.0 {
            self.low = (share, value);
            if self.low_kept == Some(false) {
                self.high.1 *= 0.5;
            }
            self.low_kept = Some(false);
        } else {
            self.high = (share, value);
            if self.low_kept == Some(true) {
                self.low.1 *= 0.5;
            }
            self.low_kept = Some(true);
        }
        let width = self.width();
        if width <= self.reference * 0.5 {
            self.reference = width;
            self.since = 0;
        } else {
            self.since += 1;
        }
    }
}

/// The last share where `test` is at most zero, going from `held`, a share where it is, towards `failed`, one where it
/// is not, each given with its value; `failed` may lie on either side. The predicate must be ordered: it holds up to a
/// share and fails beyond. The bracket narrows until it is narrower than `width`, and its satisfied end is returned
/// first, so the answer always satisfies the predicate; the values only place trials. Its failed end follows.
#[must_use]
pub(crate) fn ordered(
    test: &dyn Fn(f64) -> f64,
    held: (f64, f64),
    failed: (f64, f64),
    width: f64,
) -> (f64, f64) {
    let mut bracket = Bracket::new(held, failed);
    for _ in 0..ORDERED_TRIALS {
        if bracket.width() <= width {
            break;
        }
        let share = bracket.trial(width * 0.5);
        bracket.take(share, test(share));
    }
    (bracket.low.0, bracket.high.0)
}

/// The values a level crossing accepts, from `low` to `high`, and the one its trials aim at.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Band {
    /// The least value accepted.
    pub low: f64,
    /// The value trials aim at.
    pub aim: f64,
    /// The greatest value accepted.
    pub high: f64,
}

/// A share between `inside`, where `measure` lies below `band`, and `clear`, where it lies above, each given with its
/// measure, at which `measure` lies within `band`. Failing that, the clear end reached.
#[must_use]
pub(crate) fn crossing(
    measure: &dyn Fn(f64) -> f64,
    inside: (f64, f64),
    clear: (f64, f64),
    band: Band,
) -> f64 {
    let Band { low, aim, high } = band;
    let mut bracket = Bracket::new((inside.0, inside.1 - aim), (clear.0, clear.1 - aim));
    let step = bracket.width() * 1e-6;
    for _ in 0..CROSSING_TRIALS {
        let share = bracket.trial(step);
        let value = measure(share);
        if (low..=high).contains(&value) {
            return share;
        }
        // Below the band counts as inside, above as clear, whatever the aim.
        let side = if value < low {
            (value - aim).min(-f64::MIN_POSITIVE)
        } else {
            (value - aim).max(f64::MIN_POSITIVE)
        };
        bracket.take(share, side);
    }
    bracket.high.0
}

#[cfg(test)]
mod tests {
    //! Both searches on made-up measures with known answers.

    use super::*;

    /// The tolerance a contact search allows, in metres.
    const BAND: f64 = 1e-6;

    #[test]
    fn an_ordered_search_on_a_flat_returns_its_far_end_not_its_first_trial() {
        // A flat: the slope is exactly zero on [0.25, 0.75] and rises either side.
        let slope = |share: f64| {
            if share < 0.25 {
                share - 0.25
            } else if share <= 0.75 {
                0.0
            } else {
                share - 0.75
            }
        };
        let test = |share: f64| slope(share) - BAND;
        let (found, _) = ordered(&test, (0.0, test(0.0)), (1.0, test(1.0)), 1e-8);
        assert!(test(found) <= 0.0, "{found} does not satisfy the predicate");
        assert!(
            (found - 0.75).abs() <= BAND + 1e-8,
            "{found} is not the flat's far end"
        );
    }

    #[test]
    fn an_ordered_search_mirrored_finds_a_flats_near_end() {
        let slope = |share: f64| {
            if share < 0.25 {
                share - 0.25
            } else if share <= 0.75 {
                0.0
            } else {
                share - 0.75
            }
        };
        // The leftmost share whose slope is at least minus the tolerance: held on the right, failing on the left.
        let test = |share: f64| -BAND - slope(share);
        let (found, _) = ordered(&test, (0.75, test(0.75)), (0.0, test(0.0)), 1e-8);
        assert!(test(found) <= 0.0);
        assert!(
            (found - 0.25).abs() <= BAND + 1e-8,
            "{found} is not the flat's near end"
        );
    }

    #[test]
    fn an_ordered_search_across_a_slope_jump_returns_the_jump() {
        let test = |share: f64| if share <= 0.4 { -1.0 } else { 1.0 };
        let (found, _) = ordered(&test, (0.0, -1.0), (1.0, 1.0), 1e-8);
        assert!(
            found <= 0.4 && 0.4 - found <= 1e-8,
            "{found} is not the jump"
        );
    }

    #[test]
    fn an_ordered_search_on_a_smooth_slope_takes_few_trials() {
        let trials = std::cell::Cell::new(0);
        let test = |share: f64| {
            trials.set(trials.get() + 1);
            (share - 0.3) * (1.0 + share)
        };
        let (found, _) = ordered(&test, (0.0, test(0.0)), (1.0, test(1.0)), 1e-8);
        assert!(found <= 0.3 && 0.3 - found <= 1e-8);
        assert!(
            trials.get() <= 20,
            "{} trials, where bisection takes 27",
            trials.get()
        );
    }

    #[test]
    fn a_level_crossing_stays_within_its_bracket_and_lands_in_its_band() {
        let band = Band {
            low: -BAND,
            aim: 0.0,
            high: BAND,
        };
        // A linear measure, one convex near its touching end, a step and CB-S1's same-sign end within the tolerance.
        let cases: [&dyn Fn(f64) -> f64; 4] = [
            &|share| 0.002 - 0.004 * share,
            &|share| (1.0 - share).powi(3) * 0.01 - 1e-4 * share,
            &|share| if share < 0.37 { 0.003 } else { -0.003 },
            &|share| -0.5e-6 - 0.001 * share,
        ];
        for (index, measure) in cases.into_iter().enumerate() {
            let share = crossing(measure, (1.0, measure(1.0)), (0.0, measure(0.0)), band);
            assert!(
                (0.0..=1.0).contains(&share),
                "case {index}: {share} left the bracket"
            );
            let value = measure(share);
            let step = index == 2;
            assert!(
                step || (band.low..=band.high).contains(&value),
                "case {index}: {value} outside the band"
            );
            assert!(
                !step || ((0.37 - share).abs() < 1e-3 && value > 0.0),
                "case {index}: {share} is not clear at the step"
            );
        }
    }

    #[test]
    fn a_level_crossing_aiming_below_touching_lands_clear_of_the_aim_side_and_off_a_touching_end() {
        let measure = |share: f64| -0.001 * share;
        let band = Band {
            low: -BAND,
            aim: -BAND * 0.5,
            high: 0.0,
        };
        let share = crossing(&measure, (1.0, measure(1.0)), (0.0, 0.0), band);
        assert!(share > 0.0, "no move off the touching end");
        assert!((-BAND..=0.0).contains(&measure(share)));
    }
}
