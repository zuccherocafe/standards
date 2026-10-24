//! A bone meets plain furniture as a capsule of its hit radius turned about its origin: trust-region SQP on the sphere
//! of its directions, each step an elastic ℓ1 QP.
//!
//! Each shape's constraint is min(D − min(a, 0), g(T)): the capsule's least clearance along the bone, less any depth
//! its origin already has, and its tail's clearance, so a bone never goes deeper than its origin and its tail comes
//! out. The bone turns about its origin to the nearest direction that meets every constraint, by trust-region
//! sequential quadratic programming on the sphere of its directions, each step an elastic ℓ1 quadratic programme.
//!
//! Each shape gives two pieces, each a clearance over the bone's length that must be at least zero: its tail's, and its
//! near piece. The near piece is the capsule's least clearance, wherever along the bone its witness lies, the tail
//! included, when the origin is more than 1 µm clear of the shape. Otherwise, the origin pinned, it is the origin piece
//! u·n, the bone's direction against the way out at its origin, which by convexity has the capsule's feasible set when
//! the origin is in, and for an origin 0–1 µm clear forbids at most acos(r/(r + 1 µm)) of dip, 0.91° at a hit radius of
//! 8 mm, a deliberate regularisation. An origin on or in a shape's core takes the largest over its faces' ways out.
//! Each iteration linearises the pieces in the bone's own tangent plane, at the witness along the bone and, on a flat
//! within 1 µm, at the flat's near end too, a flat ending at the tail included, at a crease inside the core by the two
//! faces' slope-weighted mix, and minimises the model `subproblem` describes within a box of half-width Δ in the
//! tangent coordinates, Δ starting at an eighth of a turn.
//!
//! The merit is half the squared angle to the target plus ν times the pieces' total shortfall. A trial is taken when
//! the merit falls by at least a tenth of the predicted reduction, Δ doubling, up to its start, when the step was boxed
//! and the fall exceeded three quarters of the prediction; a rejected trial still inside gets one correction, the least
//! change onto its own rows within the box, while the budget allows, and otherwise Δ halves to half the step. A solve
//! converges when its pose meets every piece within 1 µm, its step, which certifies stationarity since the model's
//! minimiser is exact, lies inside the box and would move the tail no more than 10 µm, and every row the step leans on
//! lies within 1 µm of touching. Otherwise it returns the clear iterate nearest its target: exhausted after sixteen
//! accepted steps or sixty-four assessments, stalled when the box shrinks until it would move the tail no more than 10
//! µm, infeasible when the iterate is inside and no step within the box reduces the linearised shortfall. With no clear
//! iterate, an escape takes the nearest direction that each shape's tangent cap at the origin allows, which suffices by
//! convexity, or else the least wrong direction, marked unresolved.
//!
//! A tail whose path from its last position meets a plain shape it started clear of first advances only to its last
//! clear direction before that meeting, even when both ends are clear, so a step does not carry a tail through a thin
//! board and the bone keeps its side; a tail touching a shape crosses it only when its path heads in, since by
//! convexity a path leaving a touching point outwards never comes nearer. A step that begins inside a plain shape,
//! whether the shape moved into it or measured furniture turned it there, has no path to keep a side along: caught no
//! more than half a millimetre deep, its solve starts where the step began, which keeps its side, and deeper, it meets
//! its furniture afresh from its target. A shape's own motion is met as contact and kept in the velocity it gives; a
//! bone that is new, reset or rescaled, near a shape that is new or jumped further than a tail can travel in a frame,
//! or left unresolved starts again from its present pose and keeps none of furniture's push in its velocity. The
//! contact is discrete, not continuous: each accepted turn is checked only where it ends, so between steps a glancing
//! pass can dip into the radius, and a bone's middle can sweep through a thin feature that both poses and the tail's
//! path miss.
//!
//! Each step records, per bone, how far furniture turned it, how its solve ended, the physical constraint's least at
//! the answer and how deep the step finally left it, after measured furniture, with its origin's anchoring apart, and
//! what friction did: none, stuck or slid, with its largest allowed slide.

use std::cell::Cell;
use std::f64::consts::FRAC_PI_4;

use glam::{DMat2, DVec2, DVec3};

use crate::furniture::{Near, Solid};
use crate::roots::{self, Band};
use crate::{RESOLUTION, TOLERANCE, TOUCH, TRACES};

/// A feasible step that would move the tail no further than this, in metres, with every contact it leans on touching,
/// finishes a solve.
const STILL: f64 = 1e-5;
/// The most accepted iterations a solve takes; a bone resting on a face needs two or three, one sliding over an edge a
/// dozen.
const ITERATIONS: usize = 16;
/// The most directions a solve assesses, rejected trials and restorations included.
const BUDGET: usize = 64;
/// The trust region's largest radius, in tangent radians on each axis, and its first.
const LONGEST: f64 = FRAC_PI_4;
/// A trial whose merit falls by less than this share of the predicted reduction is rejected.
const ACCEPT: f64 = 0.1;
/// A trial that reached the box and falls by more than this share of the prediction doubles the radius.
const EXPAND: f64 = 0.75;
/// The least eigenvalue the step's Hessian keeps, so the step descends.
const FLOOR: f64 = 0.05;
/// A row leans on a step, for the stop's complementarity and the next Hessian alike, with a multiplier past this.
const LEANS: f64 = 1e-12;

/// A bone this step as its furniture sees it: where it turns about, how long and thick it is, which way it began the
/// step, its parent's frame for the tie-break, and the plain shapes it could meet.
#[derive(Clone, Debug)]
pub(crate) struct Lever<'a> {
    /// The bone's origin, where its parent put it.
    pub origin: DVec3,
    /// The bone's length.
    pub reach: f64,
    /// Its hit radius.
    pub hit: f64,
    /// The unit direction the bone began the step with.
    pub start: DVec3,
    /// The parent bone's direction, then its frame's X, Z and Y, which break a tie where no direction is defined.
    pub frame: [DVec3; 4],
    /// The plain shapes within its reach, in the order of the host's names for them, so rows and ties do not depend
    /// on the order the furniture was gathered in.
    pub solids: Near<'a>,
}

/// What each bone-step reuses from the last: the near shapes' indices and its problem's spare buffers. Each buffer
/// is cleared, never freed, so once warm a solve allocates nothing.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scratch {
    /// The joint's near shapes within its reach, by index.
    pub(crate) near: Vec<usize>,
    /// The problem's buffers.
    pub(crate) spare: Spare,
}

/// A problem's buffers: the origin records, spare assessments, a walk's work, the escape's caps and candidates, and
/// friction's bands and cone step.
#[derive(Clone, Debug, Default)]
pub(crate) struct Spare {
    origins: Vec<Origin>,
    assessments: Vec<Assessment>,
    work: Work,
    caps: Vec<Cap>,
    candidates: Vec<DVec3>,
    grips: Vec<Grip>,
    cones: Cones,
}

/// A shape's cap at the bone's origin: the way out of the shape there, and the least the dot of the bone's direction
/// with it may be for the bone to clear the shape, by convexity.
#[derive(Clone, Copy, Debug)]
struct Cap {
    normal: DVec3,
    least: f64,
}

impl Cap {
    /// How far `direction` lies inside the cap, negative outside it.
    fn margin(&self, direction: DVec3) -> f64 {
        self.normal.dot(direction) - self.least
    }
}

/// What a walk works in: the iterate's rows and their multipliers, a trial's rows for restoration, the rows the last
/// step leaned on with their multipliers, and the subproblem's buffers.
#[derive(Clone, Debug, Default)]
struct Work {
    rows: Vec<Row>,
    onto: Vec<Row>,
    multipliers: Vec<f64>,
    leaned: Vec<(Row, f64)>,
    buffers: Buffers,
}

/// Where a bone's furniture leaves it: its unit direction, how the solve ended, and its assessment there.
#[derive(Clone, Debug)]
pub(crate) struct Solved {
    /// The bone's unit direction.
    pub direction: DVec3,
    /// How the solve ended.
    pub settled: Settled,
    /// The pieces at that direction, which every reading of its residual takes.
    pub assessment: Assessment,
    /// Whether steering reached its weight cap at some iteration.
    pub capped: bool,
}

/// How a bone's solve against its furniture ended, from best to worst.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Settled {
    /// A feasible iterate passed the full stopping test.
    Converged,
    /// The iterations ran out; the clear iterate nearest the target.
    Exhausted,
    /// No step was accepted; the clear iterate nearest the target.
    Stalled,
    /// At an iterate inside, no step within the box reduced the linearised violation; the clear iterate nearest the
    /// target is returned.
    Infeasible,
    /// No clear iterate was reached, so the escape placed the bone clear.
    Escaped,
    /// No direction clears every shape; the bone points the least wrong way.
    Unresolved,
}

impl Settled {
    /// Whether the escape placed the bone, cleared or not.
    #[must_use]
    pub(crate) const fn escaped(self) -> bool {
        match self {
            Self::Escaped | Self::Unresolved => true,
            Self::Converged | Self::Exhausted | Self::Stalled | Self::Infeasible => false,
        }
    }

    /// Whether no direction cleared every shape.
    #[must_use]
    pub(crate) const fn unresolved(self) -> bool {
        match self {
            Self::Unresolved => true,
            Self::Converged
            | Self::Exhausted
            | Self::Stalled
            | Self::Infeasible
            | Self::Escaped => false,
        }
    }
}

impl Solved {
    /// Whether the escape placed the bone, cleared or not.
    #[must_use]
    pub(crate) const fn escaped(&self) -> bool {
        self.settled.escaped()
    }

    /// Whether no direction cleared every shape.
    #[must_use]
    pub(crate) const fn unresolved(&self) -> bool {
        self.settled.unresolved()
    }
}

/// What a shape's origin is to the bone this step, whatever its direction: its clearance beyond the hit radius,
/// whether that pins it (at most the tolerance), and the ways out of the core there, more than one only in the core.
#[derive(Clone, Copy, Debug)]
struct Origin {
    clearance: f64,
    pinned: bool,
    normals: [DVec3; 3],
    count: usize,
}

impl Origin {
    /// The origin piece `u·n` at `direction`, the largest over the ways out, and the way that attains it; the first
    /// of any that tie.
    fn piece(&self, direction: DVec3) -> (f64, DVec3) {
        self.normals.iter().take(self.count).fold(
            (f64::NEG_INFINITY, DVec3::ZERO),
            |best, normal| {
                let value = direction.dot(*normal);
                if value > best.0 {
                    (value, *normal)
                } else {
                    best
                }
            },
        )
    }
}

/// A point that linearises a piece: its value, in tail-radians, the way out there, and its share of the bone, which
/// scales how a turn moves it.
#[derive(Clone, Copy, Debug)]
struct Linear {
    value: f64,
    normal: DVec3,
    share: f64,
}

/// One piece of a shape's constraint at a direction, which must be at least zero: its value, in tail-radians, and up
/// to two points that linearise it, a flat's two ends.
#[derive(Clone, Copy, Debug)]
struct Piece {
    value: f64,
    points: [Option<Linear>; 2],
}

/// A direction assessed against every shape: per shape its tail piece and its near piece, the origin's when pinned and
/// the capsule's otherwise, each in tail-radians, a clearance over the bone's length.
#[derive(Debug, Default)]
pub(crate) struct Assessment {
    /// The bone's unit direction.
    pub(crate) direction: DVec3,
    readings: Vec<Reading>,
    /// The least piece, negative where one is not met.
    pub(crate) least: f64,
    /// Every piece's shortfall, each counted once.
    pub(crate) violation: f64,
    /// Whether every piece is met within the tolerance.
    pub(crate) feasible: bool,
}

impl Clone for Assessment {
    fn clone(&self) -> Self {
        let mut out = Self::default();
        out.clone_from(self);
        out
    }

    /// The copy into this assessment's own buffer, which a solve's spare assessments are kept for.
    fn clone_from(&mut self, source: &Self) {
        self.direction = source.direction;
        self.readings.clone_from(&source.readings);
        self.least = source.least;
        self.violation = source.violation;
        self.feasible = source.feasible;
    }
}

/// One shape's two pieces at a direction: its tail's, and its near piece, the origin's when the origin is pinned and
/// the capsule's otherwise.
#[derive(Clone, Copy, Debug)]
struct Reading {
    tail: Piece,
    near: Piece,
    pinned: bool,
}

/// One linear row of a piece in a tangent plane: its value, how a step there changes it, and what the Hessian needs.
#[derive(Clone, Copy, Debug)]
struct Row {
    value: f64,
    gradient: DVec2,
    normal: DVec3,
    share: f64,
    /// The piece it linearises, its place among the assessment's pieces.
    piece: usize,
}

impl Assessment {
    /// Every piece in order, each shape's tail then its near piece.
    fn pieces(&self) -> impl Iterator<Item = &Piece> + '_ {
        self.readings
            .iter()
            .flat_map(|reading| [&reading.tail, &reading.near])
    }

    /// How many pieces there are.
    fn piece_count(&self) -> usize {
        self.readings.len() * 2
    }

    /// The rows of every piece in `tangent`, the plane they were assessed in, into `rows`.
    fn rows_into(&self, tangent: &Tangent, rows: &mut Vec<Row>) {
        rows.clear();
        for (index, piece) in self.pieces().enumerate() {
            rows.extend(piece.points.iter().flatten().map(|point| Row {
                piece: index,
                ..tangent.row(point)
            }));
        }
    }
}

/// The plane of turns at a direction: the direction, and two unit axes square to it and to each other.
#[derive(Clone, Copy, Debug)]
struct Tangent {
    direction: DVec3,
    across: DVec3,
    along: DVec3,
}

impl Tangent {
    /// A point's row here: its value, and its share times its way out along each axis.
    fn row(&self, point: &Linear) -> Row {
        Row {
            value: point.value,
            gradient: DVec2::new(point.normal.dot(self.across), point.normal.dot(self.along))
                * point.share,
            normal: point.normal,
            share: point.share,
            piece: 0,
        }
    }

    /// The turn towards `target`, its log map in this plane: its length is the angle.
    fn towards(&self, target: DVec3) -> DVec2 {
        let angle = self.direction.angle_between(target);
        let off = target - self.direction * self.direction.dot(target);
        let off = if off.length_squared() > 1e-30 {
            off.normalize()
        } else if angle > 1.0 {
            self.across
        } else {
            return DVec2::ZERO;
        };
        DVec2::new(off.dot(self.across), off.dot(self.along)) * angle
    }

    /// The direction a step `step` in this plane reaches along its great circle.
    fn retract(&self, step: DVec2) -> DVec3 {
        let angle = step.length();
        if angle == 0.0 {
            return self.direction;
        }
        let way = (self.across * step.x + self.along * step.y) / angle;
        (self.direction * angle.cos() + way * angle.sin()).normalize()
    }
}

/// A lever with each shape's origin recorded once: what a solve, its assessments and its diagnostics read.
#[derive(Debug)]
pub(crate) struct Problem<'l> {
    lever: &'l Lever<'l>,
    origins: Vec<Origin>,
    /// The loaded contacts' bands the solve's objective carries; none without friction.
    grips: Vec<Grip>,
    spare: std::cell::RefCell<Spare>,
    capped: Cell<bool>,
}

impl Lever<'_> {
    /// The lever with each shape's origin recorded, reusing `spare`'s buffers; `Problem::finish` hands them back.
    #[must_use]
    pub(crate) fn problem_in(&self, mut spare: Spare) -> Problem<'_> {
        let mut grips = std::mem::take(&mut spare.grips);
        grips.clear();
        let mut origins = std::mem::take(&mut spare.origins);
        origins.clear();
        origins.extend(self.solids.solids().map(|solid| {
            let (gap, normal) = solid.signed(self.origin);
            let clearance = gap - self.hit;
            let (normals, count) = if gap <= 0.0 {
                solid.supports(self.origin)
            } else {
                (
                    [self.defined(normal, self.start), DVec3::ZERO, DVec3::ZERO],
                    1,
                )
            };
            let (normals, count) = if count == 0 {
                ([self.fold(DVec3::ZERO), DVec3::ZERO, DVec3::ZERO], 1)
            } else {
                (normals, count)
            };
            Origin {
                clearance,
                pinned: clearance <= TOLERANCE,
                normals,
                count,
            }
        }));
        Problem {
            lever: self,
            origins,
            grips,
            spare: std::cell::RefCell::new(spare),
            capped: Cell::new(false),
        }
    }

    /// `normal`, or where a round shape's centre or axis leaves it undefined, a way square to the bone.
    fn defined(&self, normal: DVec3, direction: DVec3) -> DVec3 {
        if normal == DVec3::ZERO {
            self.fold(direction)
        } else {
            normal
        }
    }

    /// The tangent plane at `direction`, its first axis the tie helper's.
    fn tangent(&self, direction: DVec3) -> Tangent {
        let across = self.fold(direction);
        Tangent {
            direction,
            across,
            along: direction.cross(across),
        }
    }

    /// A unit vector square to `axis`, any for none: the first of the start direction, the parent's direction and its
    /// frame's axes that stands far enough off it. One of three square axes always does, and all of them turn with
    /// the character, so the side a tie takes does not depend on the world.
    #[must_use]
    pub(crate) fn fold(&self, axis: DVec3) -> DVec3 {
        let axis = axis.normalize_or_zero();
        std::iter::once(self.start)
            .chain(self.frame)
            .map(|each| each - axis * axis.dot(each))
            .find(|off| off.length() > 1e-3)
            .map_or(DVec3::X, DVec3::normalize)
    }

    /// The direction `share` of the way along the great arc from `from` to `to`, across `fold` where they are opposed.
    fn arc(&self, from: DVec3, to: DVec3, share: f64) -> DVec3 {
        let axis = from.cross(to);
        let angle = from.angle_between(to);
        let axis = if axis.length_squared() < 1e-24 {
            from.cross(self.fold(from))
        } else {
            axis.normalize()
        };
        turned(from, axis * angle, share)
    }

    /// Each shape's cap at the origin: the way out there and the least its dot with the bone's direction may be, over
    /// the shapes whose cap leaves any direction.
    fn caps(&self, origins: &[Origin], caps: &mut Vec<Cap>) {
        caps.clear();
        caps.extend(
            origins
                .iter()
                .map(|record| {
                    let normal = record.normals.first().copied().unwrap_or_default();
                    let normal = if normal == DVec3::ZERO {
                        self.fold(DVec3::ZERO)
                    } else {
                        normal
                    };
                    Cap {
                        normal,
                        least: -record.clearance / self.reach,
                    }
                })
                .filter(|cap| cap.least > -1.0),
        );
    }

    /// Of `candidates`, the direction the caps' least margin is greatest at, which for one shape is the way straight
    /// out at the origin; a tie within 1e-9 goes to the one nearest the step's start, so it holds from frame to frame.
    fn least_wrong(&self, target: DVec3, caps: &[Cap], candidates: &[DVec3]) -> DVec3 {
        let margin = |direction: DVec3| {
            caps.iter()
                .map(|cap| cap.margin(direction))
                .fold(f64::INFINITY, f64::min)
        };
        candidates.iter().fold(target, |best, each| {
            let (mine, theirs) = (margin(*each), margin(best));
            let tie = (mine - theirs).abs() <= 1e-9;
            if (tie && each.dot(self.start) > best.dot(self.start)) || (!tie && mine > theirs) {
                *each
            } else {
                best
            }
        })
    }

    /// The directions where the nearest allowed one to `target` can lie: the target, each cap's own axis and the
    /// nearest point of its rim, and where each pair of rims cross.
    fn candidates(&self, target: DVec3, caps: &[Cap], out: &mut Vec<DVec3>) {
        out.clear();
        out.push(target);
        for (index, &Cap { normal, least }) in caps.iter().enumerate() {
            let height = least.clamp(-1.0, 1.0);
            let across = target - normal * normal.dot(target);
            let across = if across.length() > 1e-9 {
                across.normalize()
            } else {
                self.fold(normal)
            };
            out.push(normal);
            out.push(normal * height + across * (1.0 - height * height).max(0.0).sqrt());
            for other in caps.iter().skip(index + 1) {
                let square = normal.cross(other.normal);
                let cosine = normal.dot(other.normal);
                if square.length_squared() < 1e-12 {
                    continue;
                }
                let share = (least - cosine * other.least) / (1.0 - cosine * cosine);
                let other_share = (other.least - cosine * least) / (1.0 - cosine * cosine);
                let within = normal * share + other.normal * other_share;
                let left = 1.0 - within.length_squared();
                if left >= 0.0 {
                    let off = square * (left.sqrt() / square.length());
                    out.extend([within + off, within - off]);
                }
            }
        }
    }
}

impl Problem<'_> {
    /// The pieces with the bone along `direction`: per shape the tail's clearance, and the origin's piece when it is
    /// pinned or else the capsule's least clearance, with its witness and, on a flat at most the tolerance clear, the
    /// flat's near end, both of which linearise the one piece.
    #[must_use]
    pub(crate) fn assess(&self, direction: DVec3) -> Assessment {
        let lever = self.lever;
        let (origin, reach, hit) = (lever.origin, lever.reach, lever.hit);
        let tail = origin + direction * reach;
        let mut out = self.fresh();
        out.readings.clear();
        for (solid, record) in lever.solids.solids().zip(&self.origins) {
            let (gap, normal) = solid.signed(tail);
            let point = Linear {
                value: (gap - hit) / reach,
                normal: lever.defined(normal, direction),
                share: 1.0,
            };
            out.readings.push(Reading {
                tail: Piece {
                    value: point.value,
                    points: [Some(point), None],
                },
                near: self.near(solid, record, direction),
                pinned: record.pinned,
            });
        }
        let least = out
            .pieces()
            .map(|piece| piece.value)
            .fold(f64::INFINITY, f64::min);
        out.violation = out.pieces().map(|piece| (-piece.value).max(0.0)).sum();
        out.direction = direction;
        out.least = least.min(1.0);
        out.feasible = least >= -TOLERANCE / reach;
        out
    }

    /// A spare assessment to fill, its buffers kept from an earlier one.
    fn fresh(&self) -> Assessment {
        self.spare
            .borrow_mut()
            .assessments
            .pop()
            .unwrap_or_default()
    }

    /// `assessment`'s buffers, kept for a later one.
    pub(crate) fn recycle(&self, assessment: Assessment) {
        // One emptied by being taken holds no buffer worth keeping.
        if assessment.readings.capacity() > 0 {
            self.spare.borrow_mut().assessments.push(assessment);
        }
    }

    /// A copy of `assessment` in a spare one's buffers.
    fn copy(&self, assessment: &Assessment) -> Assessment {
        let mut out = self.fresh();
        out.clone_from(assessment);
        out
    }

    /// The problem's buffers, for the next bone-step.
    #[must_use]
    pub(crate) fn finish(self) -> Spare {
        let mut spare = self.spare.into_inner();
        spare.origins = self.origins;
        spare.grips = self.grips;
        spare
    }

    /// Static friction at the start assessed as `start`, at coefficient `mu`: the frictional step towards `target`
    /// decides which contacts carry load, and each loaded contact's band, from none to the tangential turn it allowed
    /// and to where `yields` would take the start, goes into the solve's objective. What friction did is returned.
    pub(crate) fn grip(
        &mut self,
        start: &Assessment,
        target: DVec3,
        yields: &Yields,
        mu: f64,
    ) -> Gripped {
        let tangent = self.lever.tangent(start.direction);
        let spare = self.spare.get_mut();
        let rows = &mut spare.work.rows;
        start.rows_into(&tangent, rows);
        spare.cones.step(rows, tangent.towards(target), mu);
        spare.cones.bands(&tangent, yields, &mut self.grips)
    }

    /// How far `direction` slipped out of every band, summed.
    fn slipped(&self, direction: DVec3) -> f64 {
        self.grips.iter().map(|grip| grip.slipped(direction)).sum()
    }

    /// A shape's near piece along `direction`: the origin's when it is pinned, else the capsule's least clearance
    /// with its witness and, on a flat at most the tolerance clear, the flat's near end. The capsule's piece keeps its
    /// scalar wherever its witness lies, at the tail too, so the set of pieces the merit counts never changes.
    fn near(&self, solid: &Solid, record: &Origin, direction: DVec3) -> Piece {
        let (origin, reach, hit) = (self.lever.origin, self.lever.reach, self.lever.hit);
        if record.pinned {
            let (value, normal) = record.piece(direction);
            let point = Linear {
                value,
                normal,
                share: 1.0,
            };
            return Piece {
                value,
                points: [Some(point), None],
            };
        }
        let tail = origin + direction * reach;
        let witness = solid.along(origin, tail);
        let value = (witness.gap - hit) / reach;
        if witness.share <= 1e-9 {
            // The least is at the origin, at least the tolerance clear: a piece with no lever and no row.
            return Piece {
                value,
                points: [None, None],
            };
        }
        let near = (witness.gap - hit <= TOLERANCE)
            .then(|| solid.near_end(origin, tail, witness.share))
            .flatten()
            .filter(|end| end.share > 1e-9 && end.share < witness.share - RESOLUTION / reach)
            .map(|end| Linear {
                value: (end.gap - hit) / reach,
                normal: end.normal,
                share: end.share,
            });
        let point = Linear {
            value,
            normal: witness.normal,
            share: witness.share,
        };
        Piece {
            value,
            points: [Some(point), near],
        }
    }

    /// The physical constraint's least, in metres, at the direction `at` was assessed at, per shape
    /// `min(D − min(a, 0), g(T))`, read from its pieces: an unpinned shape's capsule piece is its clearance, and a
    /// pinned origin whose piece is met leaves the capsule its origin's clearance, by convexity; only a pinned origin's
    /// piece not met needs the search along the bone.
    #[must_use]
    pub(crate) fn residual(&self, at: &Assessment) -> f64 {
        let lever = self.lever;
        let tail = lever.origin + at.direction * lever.reach;
        let shapes = lever.solids.solids().zip(&self.origins).zip(&at.readings);
        shapes
            .map(|((solid, record), reading)| {
                let near = if !reading.pinned {
                    reading.near.value * lever.reach
                } else if reading.near.value >= 0.0 {
                    record.clearance.max(0.0)
                } else {
                    solid.along(lever.origin, tail).gap - lever.hit - record.clearance.min(0.0)
                };
                near.min(reading.tail.value * lever.reach)
            })
            .fold(lever.reach, f64::min)
    }

    /// How deep the deepest origin is anchored in its shape, in metres: the allowed embedding.
    #[must_use]
    pub(crate) fn sunk(&self) -> f64 {
        self.origins
            .iter()
            .map(|record| (-record.clearance).max(0.0))
            .fold(0.0, f64::max)
    }

    /// The physical constraint's least over the shapes with the bone along `direction`, in metres: the residual of
    /// its assessment.
    #[must_use]
    pub(crate) fn physical(&self, direction: DVec3) -> f64 {
        let at = self.assess(direction);
        let out = self.residual(&at);
        self.recycle(at);
        out
    }

    /// The bone turned from `target` out of its furniture: from `target` when the step starts again, with no `start`;
    /// else, `start` being the assessment of the direction it began the step along, from `target` or, when its tail's
    /// path from there meets a shape, from the last clear direction on the arc it came along.
    #[must_use]
    pub(crate) fn resolve(&self, target: DVec3, start: Option<&Assessment>) -> Solved {
        let first = match start {
            // A step friction holds starts from where it began: its bands are built around it, and a start caught
            // inside furniture is put out by the solve along the contact normal they leave free.
            Some(start) if !self.grips.is_empty() => self.copy(start),
            // A start caught inside furniture by no more than a touch is the first iterate; only a deeper one, which
            // keeps nothing of its side, starts again from the target.
            Some(start) if !start.feasible => {
                if self.residual(start) >= -TOUCH {
                    self.copy(start)
                } else {
                    self.assess(target)
                }
            }
            Some(start) if self.crosses(start, target) => self.last_clear(target, start),
            Some(start) if target == start.direction => self.copy(start),
            _ => self.assess(target),
        };
        self.solve(target, first)
    }

    /// Trust-region SQP from `first` towards `target`, for at most [`ITERATIONS`] accepted steps and the evaluation
    /// budget: each step the elastic ℓ1 QP in a box, its weight steered, taken when the merit falls by enough of the
    /// predicted reduction, after one restoration from a rejected infeasible trial's own rows, else the box halves.
    /// Finished by the full KKT test with the box inactive, else the best clear iterate.
    fn solve(&self, target: DVec3, first: Assessment) -> Solved {
        self.capped.set(false);
        let mut walk = self.walk(first);
        let mut work = std::mem::take(&mut self.spare.borrow_mut().work);
        work.leaned.clear();
        let mut solved = loop {
            if walk.accepted >= ITERATIONS || walk.evaluations >= BUDGET {
                break self.end(walk.best.take(), Settled::Exhausted, target);
            }
            if let Some(ending) = self.iteration(&mut walk, &mut work, target) {
                break ending;
            }
        };
        self.recycle(walk.at);
        if let Some(best) = walk.best {
            self.recycle(best);
        }
        self.spare.borrow_mut().work = work;
        solved.capped = self.capped.get();
        solved
    }

    /// A walk from `first`.
    fn walk(&self, first: Assessment) -> Walk {
        Walk {
            best: first.feasible.then(|| self.copy(&first)),
            at: first,
            weight: 1.0,
            radius: LONGEST,
            accepted: 0,
            evaluations: 1,
        }
    }

    /// One iteration from `walk`'s iterate, in `work`: the steered step, the stop and the infeasible test at the
    /// iterate, then the trial, which moves the walk on or halves its box. The solve's ending when it ends here.
    fn iteration(&self, walk: &mut Walk, work: &mut Work, target: DVec3) -> Option<Solved> {
        let tangent = self.lever.tangent(walk.at.direction);
        walk.at.rows_into(&tangent, &mut work.rows);
        let pieces = walk.at.piece_count();
        for (index, grip) in self.grips.iter().enumerate() {
            work.rows.extend(grip.rows(&tangent, pieces + index));
        }
        let steered = self.steer(walk, work, &tangent, target);
        let step = steered.step;
        let boxed = step.abs().max_element() >= walk.radius * (1.0 - 1e-9);
        if walk.at.feasible
            && !boxed
            && self.stops(&walk.at, &steered, &work.rows, &work.multipliers)
        {
            return Some(self.found(std::mem::take(&mut walk.at), Settled::Converged));
        }
        if !walk.at.feasible && steered.violation - steered.least <= 1e-14 {
            return Some(self.end(walk.best.take(), Settled::Infeasible, target));
        }
        let attempt = Attempt {
            target,
            weight: walk.weight,
            predicted: steered.predicted,
            radius: walk.radius,
        };
        let (taken, ratio) = self.attempt(walk, &mut work.onto, &tangent, step, attempt);
        let Some(next) = taken else {
            walk.radius = 0.5 * step.abs().max_element();
            let stalled = walk.radius * self.lever.reach <= STILL;
            return stalled.then(|| self.end(walk.best.take(), Settled::Stalled, target));
        };
        if ratio > EXPAND && boxed {
            walk.radius = (2.0 * walk.radius).min(LONGEST);
        }
        leaning(&work.rows, &work.multipliers, &mut work.leaned);
        self.accept(walk, next, target);
        None
    }

    /// The walk moved on to `next`, kept as the best when it is clear and nearer the target.
    fn accept(&self, walk: &mut Walk, next: Assessment, target: DVec3) {
        walk.accepted += 1;
        let was = std::mem::replace(&mut walk.at, next);
        self.recycle(was);
        // Without bands the objective is the angle to the target alone; with them, the full objective decides.
        let objective = |each: &Assessment| {
            0.5 * each.direction.angle_between(target).powi(2) + BAND * self.slipped(each.direction)
        };
        let nearer = |best: &Assessment| {
            if self.grips.is_empty() {
                walk.at.direction.dot(target) > best.direction.dot(target)
            } else {
                objective(&walk.at) < objective(best)
            }
        };
        if walk.at.feasible && walk.best.as_ref().is_none_or(nearer) {
            let copy = self.copy(&walk.at);
            if let Some(old) = walk.best.replace(copy) {
                self.recycle(old);
            }
        }
    }

    /// The steered step over `work`'s rows from the walk's iterate towards `target`; the walk takes the weight steering
    /// reached.
    fn steer(&self, walk: &mut Walk, work: &mut Work, tangent: &Tangent, target: DVec3) -> Steered {
        let towards = tangent.towards(target);
        let hessian = hessian(towards, walk.at.direction, &work.leaned);
        let problem = Subproblem {
            rows: &work.rows,
            pieces: walk.at.piece_count(),
            bands: self.grips.len(),
            towards,
            hessian,
            radius: walk.radius,
        };
        let steered = problem.steered(walk.weight, &mut work.buffers, &mut work.multipliers);
        walk.weight = steered.weight;
        if steered.capped {
            self.capped.set(true);
        }
        steered
    }

    /// The trial at `step` from the walk's iterate, taken when the merit falls by at least a tenth of the predicted
    /// reduction; a rejected infeasible trial gets one restoration, the least change onto its own rows within the
    /// box, its rows in `onto`, while the budget allows. What was taken, if anything, and the full trial's ratio.
    fn attempt(
        &self,
        walk: &mut Walk,
        onto: &mut Vec<Row>,
        tangent: &Tangent,
        step: DVec2,
        attempt: Attempt,
    ) -> (Option<Assessment>, f64) {
        let merit = |each: &Assessment| {
            0.5 * each.direction.angle_between(attempt.target).powi(2)
                + attempt.weight * each.violation
                + BAND * self.slipped(each.direction)
        };
        let base = merit(&walk.at);
        let ratio = |each: &Assessment| {
            if attempt.predicted > 0.0 {
                (base - merit(each)) / attempt.predicted
            } else {
                -1.0
            }
        };
        let trial = self.assess(tangent.retract(step));
        walk.evaluations += 1;
        let full = ratio(&trial);
        if full >= ACCEPT {
            return (Some(trial), full);
        }
        if trial.violation > 0.0 && walk.evaluations < BUDGET {
            let there = self.lever.tangent(trial.direction);
            trial.rows_into(&there, onto);
            if let Some((change, _)) = projected(
                DVec2::ZERO,
                onto,
                &box_rows(attempt.radius),
                DMat2::IDENTITY,
            ) && change != DVec2::ZERO
            {
                let restored = self.assess(there.retract(change));
                walk.evaluations += 1;
                if ratio(&restored) >= ACCEPT {
                    self.recycle(trial);
                    return (Some(restored), full);
                }
                self.recycle(restored);
            }
        }
        self.recycle(trial);
        (None, full)
    }

    /// The full KKT test at `at`: feasible, its step, the certificate of stationarity, moving the tail no more than the
    /// stopping step, and every row the step leans on within the tolerance of touching.
    fn stops(&self, at: &Assessment, steered: &Steered, rows: &[Row], multipliers: &[f64]) -> bool {
        let reach = self.lever.reach;
        // A band furniture has loaded to its weight gives way, so only one it holds must touch where it leans.
        let pieces = at.piece_count();
        let holds =
            |row: &Row, multiplier: f64| row.piece < pieces || multiplier < BAND * (1.0 - 1e-9);
        let mut active = rows
            .iter()
            .zip(multipliers)
            .filter(|(row, multiplier)| holds(row, **multiplier) && **multiplier > LEANS);
        at.feasible
            && steered.step.length() <= STILL / reach
            && active.all(|(row, _)| row.value.abs() <= TOLERANCE / reach)
    }

    /// The best clear iterate under `settled`, or with none, the escape.
    fn end(&self, best: Option<Assessment>, settled: Settled, target: DVec3) -> Solved {
        match best {
            Some(best) => self.found(best, settled),
            None => self.escape(target),
        }
    }

    /// `at` returned under `settled`.
    #[expect(clippy::unused_self, reason = "every ending is the problem's")]
    fn found(&self, at: Assessment, settled: Settled) -> Solved {
        Solved {
            direction: at.direction,
            settled,
            assessment: at,
            capped: false,
        }
    }

    /// Whether the tail's straight path from the start, assessed as `start`, to the target's tail heads into a shape:
    /// for a shape the tail touches, whether the path leaves it inwards, since by convexity a path leaving with
    /// `n·d ≥ 0` never comes nearer; for one it is clear of, whether the path meets it, traced by its clearance.
    fn crosses(&self, start: &Assessment, target: DVec3) -> bool {
        let lever = self.lever;
        let from = lever.origin + start.direction * lever.reach;
        let to = lever.origin + target * lever.reach;
        let (way, length) = (to - from, from.distance(to));
        if length <= 0.0 {
            return false;
        }
        lever
            .solids
            .solids()
            .zip(&start.readings)
            .any(|(solid, reading)| {
                let gap = reading.tail.value * lever.reach;
                if gap <= TOUCH {
                    let normal = reading
                        .tail
                        .points
                        .first()
                        .copied()
                        .flatten()
                        .map_or(DVec3::ZERO, |point| point.normal);
                    return normal.dot(way) < 0.0;
                }
                let mut share = gap / length;
                for _ in 0..TRACES {
                    if share > 1.0 {
                        return false;
                    }
                    let left = solid.signed(from.lerp(to, share)).0 - lever.hit;
                    if left <= TOUCH {
                        return true;
                    }
                    share += left / length;
                }
                false
            })
    }

    /// The least piece at `direction`, its assessment's buffers kept.
    fn least(&self, direction: DVec3) -> f64 {
        let at = self.assess(direction);
        let least = at.least;
        self.recycle(at);
        least
    }

    /// The last clear direction on the arc from the start, assessed as `start`, towards `target` before its first
    /// contact, with its assessment: the start itself when it is not clear.
    fn last_clear(&self, target: DVec3, start: &Assessment) -> Assessment {
        let span = start.direction.angle_between(target);
        if !start.feasible || span <= 0.0 {
            return if start.feasible {
                self.assess(target)
            } else {
                self.copy(start)
            };
        }
        let along = |share: f64| self.lever.arc(start.direction, target, share);
        // No point of the bone moves further than the tail, so a turn of the least piece, a clearance over the length,
        // cannot pass through anything: marching by it meets the first contact rather than stepping over a board.
        let mut clear = (0.0, self.copy(start));
        for _ in 0..TRACES {
            let share = (clear.0 + clear.1.least.max(TOUCH / self.lever.reach) / span).min(1.0);
            let next = self.assess(along(share));
            if !next.feasible {
                let ends = [(share, next.least), (clear.0, clear.1.least)];
                self.recycle(next);
                self.recycle(clear.1);
                return self.assess(along(self.crossing(&|each| self.least(each), &along, ends)));
            }
            if share >= 1.0 {
                self.recycle(clear.1);
                return next;
            }
            let was = std::mem::replace(&mut clear, (share, next));
            self.recycle(was.1);
        }
        clear.1
    }

    /// A share between `ends`, one inside and one clear, each with its least piece, at the furthest clear share within
    /// the tolerance below touching; failing that, the clear end reached.
    fn crossing(
        &self,
        measure: &dyn Fn(DVec3) -> f64,
        along: &dyn Fn(f64) -> DVec3,
        ends: [(f64, f64); 2],
    ) -> f64 {
        let [inside, clear] = ends;
        let tolerance = TOLERANCE / self.lever.reach;
        let band = Band {
            low: -tolerance,
            aim: -tolerance * 0.5,
            high: 0.0,
        };
        roots::crossing(&|share| measure(along(share)), inside, clear, band)
    }

    /// The direction nearest `target` that the shapes' caps at the origin allow, each sufficient for its shape by
    /// convexity, approached along the arc from `target`; with none, the one least wrong, marked unresolved.
    fn escape(&self, target: DVec3) -> Solved {
        let lever = self.lever;
        let (mut caps, mut candidates) = {
            let mut spare = self.spare.borrow_mut();
            (
                std::mem::take(&mut spare.caps),
                std::mem::take(&mut spare.candidates),
            )
        };
        lever.caps(&self.origins, &mut caps);
        lever.candidates(target, &caps, &mut candidates);
        let allowed = |direction: &DVec3| {
            caps.iter()
                .all(|cap| cap.normal.dot(*direction) >= cap.least - 1e-12)
        };
        let nearest = candidates
            .iter()
            .filter(|direction| allowed(direction))
            .fold(None, |best: Option<DVec3>, each| match best {
                Some(best) if best.dot(target) >= each.dot(target) => Some(best),
                _ => Some(*each),
            });
        let solved = match nearest.map(|direction| (direction, self.assess(direction))) {
            Some((allowed, there)) if there.feasible => {
                let along = |share: f64| lever.arc(target, allowed, share);
                let from = self.assess(target);
                let found = if from.feasible {
                    from
                } else {
                    let ends = [(0.0, from.least), (1.0, there.least)];
                    self.recycle(from);
                    self.assess(along(self.crossing(&|each| self.least(each), &along, ends)))
                };
                self.recycle(there);
                self.found(found, Settled::Escaped)
            }
            other => {
                if let Some((_, there)) = other {
                    self.recycle(there);
                }
                let direction = lever.least_wrong(target, &caps, &candidates);
                self.found(self.assess(direction), Settled::Unresolved)
            }
        };
        let mut spare = self.spare.borrow_mut();
        spare.caps = caps;
        spare.candidates = candidates;
        solved
    }
}

/// Where a solve has got to: the iterate, the best clear one, the penalty weight, the box, and the accepted steps and
/// evaluations spent.
#[derive(Debug)]
struct Walk {
    at: Assessment,
    best: Option<Assessment>,
    weight: f64,
    radius: f64,
    accepted: usize,
    evaluations: usize,
}

/// What a trial is judged by: the target and penalty weight of the merit, the reduction the model predicted, and the
/// box.
#[derive(Clone, Copy, Debug)]
struct Attempt {
    target: DVec3,
    weight: f64,
    predicted: f64,
    radius: f64,
}

/// The box `|p|∞ ≤ radius` as four rows of a plain QP.
fn box_rows(radius: f64) -> [Row; 4] {
    [DVec2::X, -DVec2::X, DVec2::Y, -DVec2::Y].map(|gradient| Row {
        value: radius,
        gradient,
        normal: DVec3::ZERO,
        share: 0.0,
        piece: usize::MAX,
    })
}

/// The rows a step leaned on, with their multipliers, for the next Hessian, into `leaned`.
fn leaning(rows: &[Row], multipliers: &[f64], leaned: &mut Vec<(Row, f64)>) {
    leaned.clear();
    leaned.extend(
        rows.iter()
            .zip(multipliers)
            .filter(|(_, multiplier)| **multiplier > LEANS)
            .map(|(row, multiplier)| (*row, *multiplier)),
    );
}

/// The step's Hessian at `direction`: the objective's exact one on the sphere for the turn `towards`, plus the
/// curvature `−λ·∇²c = λ·share·(u·n)·I` of each row the last iteration leaned on, its eigenvalues floored.
fn hessian(towards: DVec2, direction: DVec3, leaned: &[(Row, f64)]) -> DMat2 {
    let angle = towards.length();
    let objective = if angle > 1e-9 {
        let way = towards / angle;
        let square = way.perp();
        let bend = angle / angle.tan();
        outer(way) + outer(square) * bend
    } else {
        DMat2::IDENTITY
    };
    let curvature: f64 = leaned
        .iter()
        .map(|(row, multiplier)| multiplier * row.share * direction.dot(row.normal))
        .sum();
    let (values, vectors) = eigen(objective + DMat2::IDENTITY * curvature);
    vectors * DMat2::from_diagonal(values.max(DVec2::splat(FLOOR))) * vectors.transpose()
}

/// `vector · vectorᵀ`.
fn outer(vector: DVec2) -> DMat2 {
    DMat2::from_cols(vector * vector.x, vector * vector.y)
}

/// A symmetric 2×2 matrix's eigenvalues and its eigenvectors as columns.
fn eigen(matrix: DMat2) -> (DVec2, DMat2) {
    let (a, b, d) = (matrix.x_axis.x, matrix.y_axis.x, matrix.y_axis.y);
    let middle = f64::midpoint(a, d);
    let spread = ((a - d) * 0.5).hypot(b);
    let values = DVec2::new(middle + spread, middle - spread);
    // Each form of the first eigenvector cancels to rounding on one side of a = d; take the other.
    let first = if b.abs() <= 1e-300 {
        if a >= d { DVec2::X } else { DVec2::Y }
    } else if a >= d {
        DVec2::new(values.x - d, b).normalize()
    } else {
        DVec2::new(b, values.x - a).normalize()
    };
    (values, DMat2::from_cols(first, first.perp()))
}

/// The QP `min ½pᵀHp − towards·p` over `rows` and `extra`, each `value + gradient·p ≥ 0`: the step and the one or
/// two rows its candidate lies on, numbered across both slices, with their multipliers from `Hp − towards =
/// Σλ·gradient`. With `y = H^½p` it is the nearest point to `H^−½·towards` in a polygon, so the optimum is that point,
/// a row's projection or a pair's vertex, and enumerating them, streamed, is exact. Feasibility is to rounding. None
/// when no candidate satisfies every row.
fn projected(
    towards: DVec2,
    rows: &[Row],
    extra: &[Row],
    hessian: DMat2,
) -> Option<(DVec2, Active)> {
    let (values, vectors) = eigen(hessian);
    let scaled = DVec2::new(values.x.powf(-0.5), values.y.powf(-0.5));
    let inverse = vectors * DMat2::from_diagonal(scaled) * vectors.transpose();
    let centre = inverse * towards;
    let turned = Turned {
        rows,
        extra,
        inverse,
    };
    let (point, on) = nearest(turned, centre)?;
    Some((inverse * point, active(turned, point - centre, on)))
}

/// A QP's rows and extra rows in the turned variables `y = H^½p`, each turned as it is read, so no row is stored.
#[derive(Clone, Copy)]
struct Turned<'r> {
    rows: &'r [Row],
    extra: &'r [Row],
    inverse: DMat2,
}

impl Turned<'_> {
    /// How many rows there are, both slices together.
    fn len(&self) -> usize {
        self.rows.len() + self.extra.len()
    }

    /// Row `index`, numbered across both slices: its value and its gradient in the turned variables.
    fn row(&self, index: usize) -> (f64, DVec2) {
        self.rows
            .get(index)
            .or_else(|| self.extra.get(index.wrapping_sub(self.rows.len())))
            .map_or((0.0, DVec2::ZERO), |row| {
                (row.value, self.inverse * row.gradient)
            })
    }

    /// Whether `point`, in the turned variables, meets every row to rounding.
    fn feasible(&self, point: DVec2) -> bool {
        let step = self.inverse * point;
        self.rows
            .iter()
            .chain(self.extra)
            .all(|row| row.value + row.gradient.dot(step) >= -1e-12)
    }
}

/// The feasible candidate nearest `centre` among `turned` rows' QP candidates, the centre, each row's projection and
/// each pair's vertex, with the rows it lies on. None when no candidate satisfies every row.
fn nearest(turned: Turned<'_>, centre: DVec2) -> Option<(DVec2, [Option<usize>; 2])> {
    let row = |index: usize| turned.row(index);
    let feasible = |point: DVec2| turned.feasible(point);
    let mut best: Option<(DVec2, [Option<usize>; 2])> = None;
    let mut consider = |point: DVec2, on: [Option<usize>; 2]| {
        let nearer =
            best.is_none_or(|(kept, _)| point.distance(centre) < kept.distance(centre) - 1e-15);
        if nearer && feasible(point) {
            best = Some((point, on));
        }
    };
    consider(centre, [None, None]);
    for index in 0..turned.len() {
        let (value, gradient) = row(index);
        let length = gradient.length_squared();
        if length > 1e-12 {
            consider(
                centre - gradient * ((value + gradient.dot(centre)) / length),
                [Some(index), None],
            );
        }
        for other in index + 1..turned.len() {
            let (other_value, other_gradient) = row(other);
            let matrix = DMat2::from_cols(gradient, other_gradient).transpose();
            if matrix.determinant().abs() > 1e-6 * gradient.length() * other_gradient.length() {
                let point = matrix.inverse() * DVec2::new(-value, -other_value);
                consider(point, [Some(index), Some(other)]);
            }
        }
    }
    best
}

/// The multipliers of the one or two `turned` rows the answer lies `on`, from `moved`, its offset from the centre.
fn active(turned: Turned<'_>, moved: DVec2, on: [Option<usize>; 2]) -> Active {
    let gradient = |index: usize| turned.row(index).1;
    match on {
        [Some(only), None] => {
            let along = gradient(only);
            [
                Some((only, along.dot(moved) / along.length_squared())),
                None,
            ]
        }
        [Some(first), Some(second)] => {
            let solved = DMat2::from_cols(gradient(first), gradient(second)).inverse() * moved;
            [Some((first, solved.x)), Some((second, solved.y))]
        }
        _ => [None, None],
    }
}

/// The one or two rows a QP's answer lies on, each with its multiplier.
type Active = [Option<(usize, f64)>; 2];

/// `direction` turned by `share` of the turn `turn`, an axis scaled by its angle.
fn turned(direction: DVec3, turn: DVec3, share: f64) -> DVec3 {
    let angle = turn.length() * share;
    if angle == 0.0 {
        return direction;
    }
    let axis = turn.normalize();
    (direction * angle.cos()
        + axis.cross(direction) * angle.sin()
        + axis * axis.dot(direction) * (1.0 - angle.cos()))
    .normalize()
}

mod friction;

pub(crate) use friction::Yields;
use friction::{BAND, Cones, Grip};
pub use friction::{Friction, Gripped};

mod subproblem;

use subproblem::{Buffers, Steered, Subproblem};

#[cfg(test)]
mod tests;
