//! Static friction at a step's start: one friction-cone QP over the start's rows decides which contacts carry load and
//! how far each lets the bone slide across it.
//!
//! A spring group's `friction` is the coefficient of static friction against plain furniture, one coefficient for
//! sticking and sliding, 0.4 unless the group sets one; at 0, the bone meets furniture as `contact` alone says. Above
//! 0, a step that does not start again is anchored where it began, its old tail. There one convex friction-cone
//! programme projects the step's net pull, under the bone's mass metric, onto each of the start's rows widened to its
//! dual cone `c + a·p ≥ μ·s·|ê·p|`, where s is the row's share and ê the way a turn slides the bone across that contact. It decides at once which contacts carry load and how far each lets the
//! bone slide: a contact the step does not reach carries none, so nothing switches; a contact holds while the pull
//! across it is within μ times its normal correction over sin φ, φ the angle between the bone and the contact's normal;
//! and a sliding contact slides (|t| − b)/(1 + k²), not Coulomb's |t| − b, with k = μ/sin φ, the relaxation's known
//! cost.
//!
//! Each loaded contact then holds its own coordinate along ê, from none to what the programme allowed, as a band in the
//! one solve's objective, weighted 4, more than the most the target can pull, while furniture stays the solve's only
//! constraint, so furniture wins where they conflict; a gripped step's solve starts from its anchor, and among clear
//! iterates keeps the one of least full objective. The chain's colliders and the joint box's hard limit move no pose:
//! each band also reaches as far as their push would take the anchor, so friction never resists them, a residue of
//! curvature under half a millimetre aside, and the box's cushion, already in the step's target, counts in each
//! contact's load. Where the colliders or a box want a bone where the seat is, furniture wins, as at friction 0.
//!
//! Three modelling choices are labelled as such: the anchor carries a contact at share s by (1 − s) of the root's move,
//! so near the root the bone counts as fabric attached to the body; a pinned origin's piece carries friction as a patch
//! on the plane through the origin; and the turn along the bone's own projection on the surface is free, giving up the
//! self-locking a rigid rod pressed nearly end-on would show. Friction is not remembered between steps, so a stuck bone
//! on a cyclically reshaping support can creep by its normal arcs.

use glam::{DMat2, DVec2, DVec3};
use tdoll_vocab::Radians;

use super::subproblem::{Buffers, Subproblem};
use super::{LEANS, LONGEST, Row, Tangent, projected};

/// A band's weight in the solve's objective: more than π, the most the target's `½θ²` can pull, so a band holds
/// against the target, while furniture's steered weight rises past it, so furniture wins.
pub(crate) const BAND: f64 = 4.0;

/// A loaded contact's band: the world way a turn at the start slides the bone across it, and the interval the bone's
/// coordinate along that way keeps, from none to what the frictional step allowed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Grip {
    pub(crate) way: DVec3,
    pub(crate) low: f64,
    pub(crate) high: f64,
}

impl Grip {
    /// How far `direction`'s coordinate along the way lies outside the interval.
    pub(crate) fn slipped(&self, direction: DVec3) -> f64 {
        let at = self.way.dot(direction);
        (self.low - at).max(at - self.high).max(0.0)
    }

    /// The grip's two rows in `tangent`, of piece `piece`, whose shortfall is how far it slipped: its coordinate's
    /// gap above the interval's low end and below its high end, each with the way's curvature on the sphere.
    pub(crate) fn rows(&self, tangent: &Tangent, piece: usize) -> [Row; 2] {
        let at = self.way.dot(tangent.direction);
        let gradient = DVec2::new(self.way.dot(tangent.across), self.way.dot(tangent.along));
        [
            Row {
                value: at - self.low,
                gradient,
                normal: self.way,
                share: 1.0,
                piece,
            },
            Row {
                value: self.high - at,
                gradient: -gradient,
                normal: -self.way,
                share: 1.0,
                piece,
            },
        ]
    }
}

/// What friction did at one bone-step: none loaded, every loaded contact stuck, or one let slide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Friction {
    /// No contact carried load, or friction was off.
    #[default]
    None,
    /// Every loaded contact held its tangential coordinate.
    Stuck,
    /// Some loaded contact let the bone slide across it.
    Slid,
}

/// What the frictional step gave a bone-step: what friction did and the largest tangential turn the step allowed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gripped {
    /// What friction did.
    pub friction: Friction,
    /// The largest tangential turn the frictional step allowed; none where the arithmetic left it not finite.
    pub allowed: Option<Radians>,
}

impl Default for Gripped {
    /// No friction and no turn.
    fn default() -> Self {
        Self {
            friction: Friction::default(),
            allowed: Some(Radians::ZERO),
        }
    }
}

/// Where the chain's colliders' push and then the joint box's hard limit would turn a bone's start, each none where it
/// would not move it: a band reaches both, since friction yields to them.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Yields {
    /// The colliders' push.
    pub pushed: Option<DVec3>,
    /// The hard limit, after the push.
    pub limited: Option<DVec3>,
}

/// One row's part in the frictional step, at the row's own place among the contacts: its tangential way in the
/// tangent plane, the load the step put on it and the tangential turn the step took across it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Contact {
    pub(crate) across: DVec2,
    pub(crate) load: f64,
    pub(crate) allowed: f64,
}

/// What the frictional step reuses: its cone rows, their multipliers, the elastic path's buffers and the contacts.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cones {
    rows: Vec<Row>,
    multipliers: Vec<f64>,
    buffers: Buffers,
    pub(crate) contacts: Vec<Contact>,
    /// The last frictional step.
    step: DVec2,
}

impl Cones {
    /// The frictional step over `rows`, the start's rows, under the pull `towards` at coefficient `mu`: the nearest
    /// step to the pull, under the bone's mass metric, within every row's dual friction cone
    /// `c + a·p ≥ μ·s·|ê·p|` (Anitescu's relaxation), two rows each. Each row's contact is written into `contacts`.
    pub(crate) fn step(&mut self, rows: &[Row], towards: DVec2, mu: f64) -> DVec2 {
        self.rows.clear();
        self.contacts.clear();
        for row in rows {
            let length = row.gradient.length();
            let across = if length > LEANS {
                row.gradient.perp() / length
            } else {
                DVec2::ZERO
            };
            let lean = across * (mu * row.share);
            self.rows.push(Row {
                gradient: row.gradient - lean,
                ..*row
            });
            self.rows.push(Row {
                gradient: row.gradient + lean,
                ..*row
            });
            self.contacts.push(Contact {
                across,
                ..Contact::default()
            });
        }
        let pieces = rows.iter().map(|row| row.piece + 1).max().unwrap_or(0);
        let step = self.solve(towards, pieces);
        for (contact, pair) in self.contacts.iter_mut().zip(self.multipliers.chunks(2)) {
            contact.load = pair.iter().sum();
            contact.allowed = contact.across.dot(step);
        }
        self.step = step;
        step
    }

    /// Each loaded contact's band in `tangent` after the last frictional step, put in `grips`: from none to the slide
    /// the step allowed across it, widened to where `yields` would take the start. What friction did is returned.
    pub(crate) fn bands(
        &self,
        tangent: &Tangent,
        yields: &Yields,
        grips: &mut Vec<Grip>,
    ) -> Gripped {
        grips.clear();
        let moved = tangent.retract(self.step);
        let mut widest = 0.0_f64;
        for contact in self.contacts.iter().filter(|contact| contact.load > LEANS) {
            let way = (tangent.across * contact.across.x + tangent.along * contact.across.y)
                .normalize_or_zero();
            // A slide within rounding is none: the band is then the great circle through the anchor.
            let allowed = Some(way.dot(moved))
                .filter(|slide| slide.abs() > LEANS)
                .unwrap_or(0.0);
            let reaches = [yields.pushed, yields.limited]
                .into_iter()
                .flatten()
                .map(|direction| way.dot(direction));
            let (low, high) = reaches
                .fold((allowed.min(0.0), allowed.max(0.0)), |(low, high), each| {
                    (low.min(each), high.max(each))
                });
            grips.push(Grip { way, low, high });
            widest = widest.max(contact.allowed.abs());
        }
        let friction = match grips.len() {
            0 => Friction::None,
            _ if widest > LEANS => Friction::Slid,
            _ => Friction::Stuck,
        };
        Gripped {
            friction,
            allowed: Radians::new(widest),
        }
    }

    /// The nearest step to `towards` within the cone rows, with each cone row's multiplier: the plain QP's, or where
    /// a start caught inside furniture leaves the cones admitting no step, the elastic step's, which removes what it
    /// can of `pieces` pieces' violation.
    fn solve(&mut self, towards: DVec2, pieces: usize) -> DVec2 {
        self.multipliers.clear();
        self.multipliers.resize(self.rows.len(), 0.0);
        if let Some((step, active)) = projected(towards, &self.rows, &[], DMat2::IDENTITY) {
            for (index, value) in active.into_iter().flatten() {
                if let Some(slot) = self.multipliers.get_mut(index) {
                    *slot = value.max(0.0);
                }
            }
            return step;
        }
        let problem = Subproblem {
            bands: 0,
            rows: &self.rows,
            pieces,
            towards,
            hessian: DMat2::IDENTITY,
            radius: LONGEST,
        };
        problem
            .steered(1.0, &mut self.buffers, &mut self.multipliers)
            .step
    }
}

#[cfg(test)]
mod tests;
