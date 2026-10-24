//! Furniture as it stands: boxes, spheres, capsules and cylinders along their own Y, and floors' planes, each measured
//! by its core's exact signed distance and along a bone; and furniture whose shape only a host's physics measures, a
//! convex hull, a mesh or a height map, asked about through a ball the host moves, the host leaving out every body but
//! that furniture, and the sweep against it.
//!
//! Furniture has any shape. Boxes, spheres, capsules, cylinders and planes are gathered in script, by
//! `GFSpringFurniture`, each named by its body and shape index, and measured here by their cores' exact signed
//! distance. Any other shape is measured by the host's physics, which the solver asks: a ball of the bone's hit radius
//! is swept along the tail's path from its last position and along the bone, the tail turns out of the plane of the
//! first surface it meets, and then out of whatever the ball still rests in, however the tail moved, only for the bones
//! the host finds, once a frame, within a frame's travel of that shape around either end of the bone. A ball centred
//! inside a one-sided mesh does not see it, which is why both ends are asked about, and a mesh has no inside to leave
//! once both ends are in it. A tail inside measured furniture's surface turns to the nearest direction that clears that
//! surface's plane, the way it came from when it meets the surface head on. Each frame the host finds the furniture
//! near the character body by body, a page of hits at a time with the bodies already met left out, so a mesh's many
//! triangles are not listed one by one, and reads each body's enabled shapes.
//!
//! Bodies that can move, and the character's own bodies, which hang from its skeleton, are not furniture. Animated
//! static bodies, such as doors and lifts, move, so they are not furniture either: the tests follow only the tail's
//! motion, and a moving body could pass through a tail.

use glam::{DAffine3, DVec2, DVec3};

use crate::joint::Swing;
use crate::pose::{Bounds, equal_approx, max_axis, scale_of, sign_of, without_scale};
use crate::roots::ordered;
use crate::{PUSHES, RESOLUTION, TOLERANCE, TOUCH};

/// A furniture shape as the host's physics holds it, in the shape's own space before the placement's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A box of these half extents.
    Box {
        /// Half the box's size along each axis.
        half: DVec3,
    },
    /// A ball.
    Sphere {
        /// Its radius.
        radius: f64,
    },
    /// A capsule along Y, `height` from cap to cap.
    Capsule {
        /// Its radius.
        radius: f64,
        /// Its length from cap to cap.
        height: f64,
    },
    /// A cylinder along Y.
    Cylinder {
        /// Its radius.
        radius: f64,
        /// Its length from face to face.
        height: f64,
    },
    /// The plane `normal · point = distance`, solid on the side away from the normal.
    Plane {
        /// The plane's unit normal.
        normal: DVec3,
        /// Its distance from the origin along the normal.
        distance: f64,
    },
}

/// A furniture shape and where it stands: the body's placement times the shape's own, scale included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Where the shape stands in the world.
    pub placement: DAffine3,
    /// The shape.
    pub shape: Shape,
    /// The host's name for the shape, the same every frame it stands.
    pub id: u64,
}

/// The furniture a host gathered around the chains for a frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Furniture {
    /// The plain shapes the solver measures itself.
    pub shapes: Vec<Placed>,
    /// Whether furniture the host's physics alone measures stands around the chains.
    pub measured: bool,
}

/// A plain shape's core in its own space, its placement's scale taken into its sizes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Core {
    /// A box of these half extents.
    Box { half: DVec3 },
    /// The points within `radius` of the segment along Y from `-straight` to `straight`: a ball, or a capsule.
    Round { radius: f64, straight: f64 },
    /// A cylinder along Y.
    Cylinder { radius: f64, half_height: f64 },
    /// The plane `normal · point = distance`, solid on the side away from the normal.
    Plane { normal: DVec3, distance: f64 },
}

/// A plain shape as it stands, measured by the solver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Solid {
    /// The host's name for the shape.
    pub(crate) id: u64,
    /// Whether the shape is new around the chains this frame, or jumped since the last further than a tail can travel.
    pub(crate) changed: bool,
    core: Core,
    placed: DAffine3,
    inverse: DAffine3,
    /// The shape's bounds in the world, or none for a plane, which has no bounds.
    pub(crate) bounds: Option<Bounds>,
}

impl Solid {
    /// The shape as it stands, its placement's scale taken into its size.
    #[must_use]
    pub(crate) fn new(placed: Placed) -> Self {
        let scale = scale_of(placed.placement.matrix3);
        let placement = without_scale(placed.placement);
        let core = match placed.shape {
            Shape::Box { half } => Core::Box { half: half * scale },
            Shape::Sphere { radius } => Core::Round {
                radius: radius * scale.x,
                straight: 0.0,
            },
            Shape::Capsule { radius, height } => Core::Round {
                radius: radius * scale.x,
                straight: (height * 0.5 - radius).max(0.0) * scale.y,
            },
            Shape::Cylinder { radius, height } => Core::Cylinder {
                radius: radius * scale.x,
                half_height: height * 0.5 * scale.y,
            },
            Shape::Plane { normal, distance } => Core::Plane { normal, distance },
        };
        let extent = match core {
            Core::Box { half } => Some(half),
            Core::Round {
                radius,
                straight: along,
            }
            | Core::Cylinder {
                radius,
                half_height: along,
            } => Some(DVec3::new(radius, radius + along, radius)),
            Core::Plane { .. } => None,
        };
        let bounds = extent.map(|extent| {
            Bounds {
                position: -extent,
                size: extent * 2.0,
            }
            .placed(placement)
        });
        Self {
            id: placed.id,
            changed: false,
            core,
            placed: placement,
            inverse: placement.inverse(),
            bounds,
        }
    }
}

/// Where a bone's centreline comes nearest a shape's core: the least signed distance, where along the bone, and the
/// way out there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Approach {
    /// The core's signed distance at the witness, negative inside.
    pub gap: f64,
    /// The witness's share of the way from the bone's origin to its tail, where the distance's slope along the bone
    /// first exceeds the tolerance: on a flat the furthest point within the tolerance of the least, so at an edge the
    /// edge; past a strict least by up to the tolerance over the slope's rate of change, a share of about ε·d/L² for a
    /// bone of length L passing d from an edge, its distance short of the least by the square of that.
    pub share: f64,
    /// The witness on the bone.
    pub point: DVec3,
    /// How the distance there moves with the witness: the way out of the core, a unit vector, or zero where none is
    /// defined. At a crease inside the core it is the crossing's derivative, the slope-weighted mix of the two faces'
    /// ways out, which is zero between opposite faces.
    pub normal: DVec3,
}

impl Solid {
    /// How far `point` lies from the shape's core, negative inside, and the way out there: a unit vector in the world,
    /// or zero on a round shape's centre or axis, where none is defined. Inside a box, it leaves by the nearest face,
    /// the lowest axis of any that tie.
    #[must_use]
    pub(crate) fn signed(&self, point: DVec3) -> (f64, DVec3) {
        let at = self.inverse.transform_point3(point);
        let (gap, out) = match self.core {
            Core::Box { half } => {
                let over = at.abs() - half;
                let outside = over.max(DVec3::ZERO);
                if outside.length_squared() > 0.0 {
                    (outside.length(), outside * at.map(sign_of))
                } else {
                    let axis = max_axis(over);
                    let mut out = DVec3::ZERO;
                    out[axis] = if at[axis] >= 0.0 { 1.0 } else { -1.0 };
                    (over[axis], out)
                }
            }
            Core::Round { radius, straight } => {
                let out = at - DVec3::new(0.0, at.y.clamp(-straight, straight), 0.0);
                (out.length() - radius, out)
            }
            Core::Cylinder {
                radius,
                half_height,
            } => {
                let side = DVec3::new(at.x, 0.0, at.z).normalize_or_zero();
                let over = DVec2::new(
                    DVec2::new(at.x, at.z).length() - radius,
                    at.y.abs() - half_height,
                );
                let end = DVec3::new(0.0, if at.y >= 0.0 { 1.0 } else { -1.0 }, 0.0);
                if over.x > 0.0 || over.y > 0.0 {
                    let outside = over.max(DVec2::ZERO);
                    (outside.length(), side * outside.x + end * outside.y)
                } else if over.x > over.y {
                    (over.x, side)
                } else {
                    (over.y, end)
                }
            }
            Core::Plane { normal, distance } => (normal.dot(at) - distance, normal),
        };
        (gap, self.placed.matrix3 * out.normalize_or_zero())
    }

    /// Where the segment from `origin` to `tail` comes nearest the core: in closed form for planes and round shapes,
    /// and for boxes and cylinders by an ordered search on the slope of the distance, which is convex along a segment,
    /// until the interval is shorter than the resolution.
    #[must_use]
    pub(crate) fn along(&self, origin: DVec3, tail: DVec3) -> Approach {
        let span = tail - origin;
        let at = |share: f64| self.approach(origin, span, share);
        match self.core {
            Core::Plane { .. } => {
                let (start, end) = (at(0.0), at(1.0));
                if end.gap <= start.gap + TOLERANCE {
                    end
                } else {
                    start
                }
            }
            Core::Round { straight, .. } => {
                let axis = self.placed.matrix3.y_axis * straight;
                let centre = self.placed.translation;
                at(nearest_share(origin, span, centre - axis, axis * 2.0))
            }
            Core::Box { .. } | Core::Cylinder { .. } => self.halved(origin, span),
        }
    }

    /// The near end of a flat the segment from `origin` to `tail` lies along, whose far end is at `share`: the least
    /// share whose slope along the segment is at least minus the tolerance; none where that is the origin itself.
    #[must_use]
    pub(crate) fn near_end(&self, origin: DVec3, tail: DVec3, share: f64) -> Option<Approach> {
        let span = tail - origin;
        let test = |each: f64| -TOLERANCE - self.signed(origin + span * each).1.dot(span);
        let (start, far) = (test(0.0), test(share));
        if start <= 0.0 || far > 0.0 {
            return None;
        }
        let (low, _) = ordered(
            &test,
            (share, far),
            (0.0, start),
            RESOLUTION / span.length(),
        );
        Some(self.approach(origin, span, low))
    }

    /// The ways out of the core at a point on or in it, each face within the tolerance of the nearest, in the order
    /// the tie rule gives: up to three faces of a box, a cylinder's side and an end; the one way out of any other shape.
    #[must_use]
    pub(crate) fn supports(&self, point: DVec3) -> ([DVec3; 3], usize) {
        let at = self.inverse.transform_point3(point);
        let mut out = [DVec3::ZERO; 3];
        let mut count = 0;
        let mut push = |way: DVec3| {
            if let Some(slot) = out.get_mut(count)
                && way != DVec3::ZERO
            {
                *slot = self.placed.matrix3 * way;
                count += 1;
            }
        };
        match self.core {
            Core::Box { half } => {
                let over = at.abs() - half;
                let most = over.max_element();
                for axis in 0..3 {
                    if over[axis] >= most - TOLERANCE {
                        let mut way = DVec3::ZERO;
                        way[axis] = if at[axis] >= 0.0 { 1.0 } else { -1.0 };
                        push(way);
                    }
                }
            }
            Core::Cylinder {
                radius,
                half_height,
            } => {
                let side = DVec2::new(at.x, at.z).length() - radius;
                let end = at.y.abs() - half_height;
                if side >= end.max(side) - TOLERANCE {
                    push(DVec3::new(at.x, 0.0, at.z).normalize_or_zero());
                }
                if end >= side.max(end) - TOLERANCE {
                    push(DVec3::new(0.0, if at.y >= 0.0 { 1.0 } else { -1.0 }, 0.0));
                }
            }
            Core::Round { .. } | Core::Plane { .. } => {
                let (_, way) = self.signed(point);
                if let Some(slot) = out.first_mut() {
                    *slot = way;
                }
                return (out, usize::from(way != DVec3::ZERO));
            }
        }
        (out, count)
    }

    /// The approach at `share` of the way along `span` from `origin`.
    fn approach(&self, origin: DVec3, span: DVec3, share: f64) -> Approach {
        let point = origin + span * share;
        let (gap, normal) = self.signed(point);
        Approach {
            gap,
            share,
            point,
            normal,
        }
    }

    /// A box's or a cylinder's nearest approach along `span` from `origin`: the furthest share whose slope is within
    /// the tolerance, by an ordered search on the slope of its distance.
    fn halved(&self, origin: DVec3, span: DVec3) -> Approach {
        let at = |share: f64| self.approach(origin, span, share);
        let length = span.length();
        // The slope only rises along a convex distance: one level or falling at the tail, or rising from the origin,
        // puts the least at that end, as the search would find it.
        let (start, end) = (at(0.0), at(1.0));
        if end.normal.dot(span) <= TOLERANCE {
            return end;
        }
        if start.normal.dot(span) > TOLERANCE {
            return start;
        }
        let slope = |share: f64| self.signed(origin + span * share).1.dot(span) - TOLERANCE;
        let held = (0.0, start.normal.dot(span) - TOLERANCE);
        let failed = (1.0, end.normal.dot(span) - TOLERANCE);
        let (low, high) = ordered(&slope, held, failed, RESOLUTION / length);
        let mut found = at(low);
        // A slope that jumps inside the core is a crease: the least of the two faces' distances moves with both.
        let out = self.signed(origin + span * high).1;
        let (rising, falling) = (out.dot(span), found.normal.dot(span));
        if high < 1.0 && found.gap <= 0.0 && found.normal.dot(out) < 1.0 - 1e-9 && rising > falling
        {
            found.normal = (found.normal * rising - out * falling) / (rising - falling);
        }
        found
    }
}

/// The share along the segment from `start` by `span` nearest the segment from `core` by `along`: where several are
/// equally near, the parallel case, the furthest of them.
fn nearest_share(start: DVec3, span: DVec3, core: DVec3, along: DVec3) -> f64 {
    let (bone, length) = (span.length_squared(), along.length_squared());
    if bone < 1e-24 {
        return 0.0;
    }
    let apart = start - core;
    let (cross, bone_apart, core_apart) = (span.dot(along), span.dot(apart), along.dot(apart));
    let denominator = bone * length - cross * cross;
    let (first, last) = (-bone_apart / bone, (cross - bone_apart) / bone);
    if length < 1e-24 {
        return first.clamp(0.0, 1.0);
    }
    if denominator <= 1e-12 * bone * length {
        // Parallel: the shares whose nearest core point lies within the core, the furthest of them.
        return first.max(last).clamp(0.0, 1.0);
    }
    let share = ((cross * core_apart - bone_apart * length) / denominator).clamp(0.0, 1.0);
    let other = (cross * share + core_apart) / length;
    if other < 0.0 {
        first.clamp(0.0, 1.0)
    } else if other > 1.0 {
        last.clamp(0.0, 1.0)
    } else {
        share
    }
}

/// The furniture a bone tests: the solver's plain shapes, by index.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Near<'a> {
    /// Every plain shape around the chains.
    pub(crate) all: &'a [Solid],
    /// The ones near the bone.
    pub(crate) indices: &'a [usize],
}

impl<'a> Near<'a> {
    /// The plain shapes near the bone.
    pub(crate) fn solids(self) -> impl Iterator<Item = &'a Solid> {
        self.indices.iter().filter_map(|index| self.all.get(*index))
    }
}

/// Where a ball rests against measured furniture: the point it touches and the way out there, a unit vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// The point touched, in the world.
    pub point: DVec3,
    /// The way out of the furniture there.
    pub normal: DVec3,
}

/// The host's physics, asked about the furniture it alone measures, in metres in the world.
pub trait Measured {
    /// Whether that furniture lies within `reach` of `centre`.
    fn within(&mut self, centre: DVec3, reach: f64) -> bool;
    /// What a ball of `radius` at `point` rests against, if anything.
    fn rest(&mut self, point: DVec3, radius: f64) -> Option<Contact>;
    /// The share of the move `motion` from `start` at which a ball of `radius` first touches, one or more for none.
    fn cast(&mut self, start: DVec3, motion: DVec3, radius: f64) -> f64;
}

/// No furniture the host measures, for a solver with no physics around it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Unmeasured;

impl Measured for Unmeasured {
    fn within(&mut self, _centre: DVec3, _reach: f64) -> bool {
        false
    }

    fn rest(&mut self, _point: DVec3, _radius: f64) -> Option<Contact> {
        None
    }

    fn cast(&mut self, _start: DVec3, _motion: DVec3, _radius: f64) -> f64 {
        1.0
    }
}

/// The tail turned out of the furniture only the host measures, as the solver turns it out of the rest: out of the
/// first surface a ball of the bone's hit radius meets on its way from where the step began, then from the bone's
/// origin, until neither moves it, then out of any it rests in.
pub(crate) fn clear_measured(swing: &Swing, to: DVec3, server: &mut dyn Measured) -> DVec3 {
    let radius = swing.hit.max(0.001);
    let mut out = to;
    for _ in 0..PUSHES {
        let mut moved = false;
        for start in [swing.before, swing.origin] {
            let Some(touch) = measured_contact(server, start, out, radius) else {
                continue;
            };
            let cleared = swing.clear_of(out, touch.point + touch.normal * radius, touch.normal);
            if !equal_approx(cleared, out) {
                out = cleared;
                moved = true;
                break;
            }
        }
        if !moved {
            break;
        }
    }
    for _ in 0..PUSHES {
        let Some(touch) = server.rest(out, radius + TOUCH) else {
            break;
        };
        let cleared = swing.clear_of(out, touch.point + touch.normal * radius, touch.normal);
        if equal_approx(cleared, out) {
            break;
        }
        out = cleared;
    }
    out
}

/// What a ball of `radius` rests on at `start` when it moves into it, else the first measured furniture it touches on
/// its way to `to`. A ball resting on a surface does not overlap it, so a slightly larger one looks.
fn measured_contact(
    server: &mut dyn Measured,
    start: DVec3,
    to: DVec3,
    radius: f64,
) -> Option<Contact> {
    if let Some(at_start) = server.rest(start, radius + TOUCH)
        && at_start.normal.dot(to - start) < 0.0
    {
        return Some(at_start);
    }
    let share = server.cast(start, to - start, radius);
    if share >= 1.0 {
        None
    } else {
        server.rest(start.lerp(to, share), radius + TOUCH)
    }
}

#[cfg(test)]
mod tests {
    use glam::{DAffine3, DQuat, DVec3};

    use super::*;
    use crate::joint::aside;

    fn near(a: DVec3, b: DVec3) -> bool {
        a.distance(b) < 1e-9
    }

    fn at(placement: DAffine3, shape: Shape) -> Solid {
        Solid::new(Placed {
            placement,
            shape,
            id: 0,
        })
    }

    /// A placement turned about Y and moved, so a slip in the shape's own axes shows.
    fn turned() -> DAffine3 {
        DAffine3::from_rotation_translation(DQuat::from_rotation_y(0.7), DVec3::new(1.0, 2.0, -0.5))
    }

    /// For each made-up point of `solid`'s own space: its distance outside the shape grown by `grown`, and the way out.
    fn expect(solid: &Solid, placement: DAffine3, cases: &[(DVec3, f64, DVec3)], grown: f64) {
        for (local, gap, out) in cases {
            let (core, normal) = solid.signed(placement.transform_point3(*local));
            let measured = core - grown;
            assert!(
                (measured - gap).abs() < 1e-9,
                "{local}: {measured}, not {gap}"
            );
            assert!(
                near(normal, placement.matrix3 * *out),
                "{local}: out along {normal}"
            );
        }
    }

    #[test]
    fn a_box_measures_round_corners_and_leaves_by_the_nearest_face() {
        let placement = turned();
        let solid = at(
            placement,
            Shape::Box {
                half: DVec3::new(0.5, 0.1, 0.3),
            },
        );
        let corner = DVec3::new(0.08, 0.09, 0.0);
        expect(
            &solid,
            placement,
            &[
                (DVec3::new(0.0, 0.3, 0.0), 0.15, DVec3::Y),
                (DVec3::new(0.0, 0.0, -0.4), 0.05, DVec3::NEG_Z),
                (
                    DVec3::new(0.58, 0.19, 0.1),
                    corner.length() - 0.05,
                    corner.normalize(),
                ),
                (DVec3::new(0.1, 0.08, 0.0), -0.07, DVec3::Y),
                (DVec3::new(-0.45, 0.0, 0.0), -0.1, DVec3::NEG_X),
            ],
            0.05,
        );
        assert!(solid.bounds.is_some_and(|bounds| {
            bounds.has_point(placement.transform_point3(DVec3::new(0.49, 0.09, 0.29)))
        }));
    }

    #[test]
    fn a_placements_scale_is_taken_into_the_size() {
        let placement = DAffine3::from_scale_rotation_translation(
            DVec3::new(2.0, 3.0, 1.0),
            DQuat::IDENTITY,
            DVec3::ZERO,
        );
        let solid = at(
            placement,
            Shape::Box {
                half: DVec3::splat(0.5),
            },
        );
        assert!((solid.signed(DVec3::new(0.0, 2.0, 0.0)).0 - 0.5).abs() < 1e-12);
        let ball = at(placement, Shape::Sphere { radius: 0.5 });
        assert!(
            (ball.signed(DVec3::new(3.0, 0.0, 0.0)).0 - 2.0).abs() < 1e-12,
            "a ball takes the X scale"
        );
    }

    #[test]
    fn a_sphere_and_a_capsule_measure_from_their_core_and_leave_straight_out() {
        let placement = turned();
        let ball = at(placement, Shape::Sphere { radius: 0.2 });
        expect(
            &ball,
            placement,
            &[
                (DVec3::new(0.0, 0.0, 0.5), 0.25, DVec3::Z),
                (DVec3::new(0.1, 0.0, 0.0), -0.15, DVec3::X),
            ],
            0.05,
        );
        let capsule = at(
            placement,
            Shape::Capsule {
                radius: 0.1,
                height: 0.6,
            },
        );
        expect(
            &capsule,
            placement,
            &[
                (DVec3::new(0.3, 0.15, 0.0), 0.15, DVec3::X),
                (DVec3::new(0.0, 0.5, 0.0), 0.15, DVec3::Y),
                (DVec3::new(0.0, -0.1, 0.05), -0.1, DVec3::Z),
            ],
            0.05,
        );
    }

    #[test]
    fn a_cylinder_leaves_by_its_side_or_its_ends_whichever_is_nearer() {
        let placement = turned();
        let solid = at(
            placement,
            Shape::Cylinder {
                radius: 0.2,
                height: 0.4,
            },
        );
        let rim = DVec3::new(0.15, 0.15, 0.0);
        expect(
            &solid,
            placement,
            &[
                (DVec3::new(0.0, 0.0, 0.4), 0.15, DVec3::Z),
                (DVec3::new(0.05, 0.35, 0.0), 0.1, DVec3::Y),
                (
                    DVec3::new(0.35, 0.35, 0.0),
                    rim.length() - 0.05,
                    rim.normalize(),
                ),
                (DVec3::new(0.15, 0.0, 0.0), -0.1, DVec3::X),
                (DVec3::new(0.0, -0.18, 0.0), -0.07, DVec3::NEG_Y),
            ],
            0.05,
        );
    }

    #[test]
    fn a_plane_stands_everywhere_and_leaves_along_its_normal() {
        let placement = turned();
        let solid = at(
            placement,
            Shape::Plane {
                normal: DVec3::Y,
                distance: 0.1,
            },
        );
        assert!(matches!(solid.core, Core::Plane { .. }) && solid.bounds.is_none());
        expect(
            &solid,
            placement,
            &[
                (DVec3::new(5.0, 0.4, -9.0), 0.25, DVec3::Y),
                (DVec3::new(0.0, 0.0, 0.0), -0.15, DVec3::Y),
            ],
            0.05,
        );
    }

    #[test]
    fn a_push_turned_aside_is_square_to_what_the_box_took_back_and_to_the_bone() {
        let bone = DVec3::new(0.0, -1.0, 0.0);
        let pushed = aside(DVec3::new(1.0, 0.2, 1.0).normalize(), DVec3::X, bone);
        assert!(
            pushed.dot(DVec3::X).abs() < 1e-12 && pushed.dot(bone).abs() < 1e-12 && pushed.z > 0.0
        );
        let across = aside(DVec3::X, DVec3::X * 0.5, bone);
        assert!(
            near(across, DVec3::Z) || near(across, DVec3::NEG_Z),
            "nearly all taken, it goes across"
        );
        assert!(across.dot(DVec3::X) >= 0.0 - 1e-12);
        assert!(
            near(aside(DVec3::Y, DVec3::Y, bone), DVec3::Y),
            "a push along the bone stays as it was"
        );
    }

    /// A bone hanging 0.1 m from `origin`, its tail beginning the step at `before`, with a 5 mm hit radius.
    fn swing(origin: DVec3, before: DVec3) -> Swing {
        Swing {
            origin,
            reach: 0.1,
            hit: 0.005,
            before,
        }
    }

    /// Furniture the host measures: a 2 cm board through y = 0, measured as a host's physics would, the calls it answered
    /// counted.
    #[derive(Default)]
    struct Board {
        calls: usize,
    }

    impl Measured for Board {
        fn within(&mut self, centre: DVec3, reach: f64) -> bool {
            self.calls += 1;
            centre.y.abs() <= 0.01 + reach
        }

        fn rest(&mut self, point: DVec3, radius: f64) -> Option<Contact> {
            self.calls += 1;
            let side = if point.y >= 0.0 { 1.0 } else { -1.0 };
            let contact = Contact {
                point: DVec3::new(point.x, 0.01 * side, point.z),
                normal: DVec3::Y * side,
            };
            (point.y.abs() <= 0.01 + radius).then_some(contact)
        }

        fn cast(&mut self, start: DVec3, motion: DVec3, radius: f64) -> f64 {
            self.calls += 1;
            if motion.y >= 0.0 || start.y < 0.01 + radius {
                return 1.0;
            }
            (start.y - 0.01 - radius) / -motion.y
        }
    }

    #[test]
    fn a_tail_kept_out_of_measured_furniture_is_turned_out_of_its_path_and_where_it_rests() {
        let mut board = Board::default();
        let bone = swing(DVec3::new(0.0, 0.06, 0.0), DVec3::new(0.08, 0.04, 0.0));
        let out = clear_measured(&bone, DVec3::new(0.0, -0.04, 0.0), &mut board);
        assert!(
            out.y >= 0.015 - 1e-6 && board.calls > 0,
            "it went through the board, to {}",
            out.y
        );
        let mut untouched = Board::default();
        let free = clear_measured(&bone, DVec3::new(0.06, 0.06, 0.0), &mut untouched);
        assert!(
            near(free, DVec3::new(0.06, 0.06, 0.0)),
            "a tail clear of it is left where it is"
        );
    }

    #[test]
    fn a_box_and_a_cylinder_measure_their_exact_distance_with_round_corners() {
        let placement = turned();
        let cube = at(
            placement,
            Shape::Box {
                half: DVec3::new(0.5, 0.1, 0.3),
            },
        );
        let corner = placement.transform_point3(DVec3::new(0.53, 0.14, 0.0));
        let (gap, normal) = cube.signed(corner);
        assert!(
            (gap - 0.05).abs() < 1e-12,
            "the corner is 5 cm off, not {gap}"
        );
        assert!(near(normal, placement.matrix3 * DVec3::new(0.6, 0.8, 0.0)));
        let inside = cube.signed(placement.transform_point3(DVec3::new(0.0, 0.0, 0.0)));
        assert!((inside.0 + 0.1).abs() < 1e-12 && near(inside.1, placement.matrix3 * DVec3::Y));
        let can = at(
            placement,
            Shape::Cylinder {
                radius: 0.2,
                height: 0.4,
            },
        );
        let (rim, out) = can.signed(placement.transform_point3(DVec3::new(0.23, 0.24, 0.0)));
        assert!(
            (rim - 0.05).abs() < 1e-12 && near(out, placement.matrix3 * DVec3::new(0.6, 0.8, 0.0))
        );
        let (axis, none) = can.signed(placement.transform_point3(DVec3::new(0.0, 0.05, 0.0)));
        assert!((axis + 0.15).abs() < 1e-12 && near(none, placement.matrix3 * DVec3::Y));
        let (centre, undefined) =
            at(placement, Shape::Sphere { radius: 0.2 }).signed(placement.translation);
        assert!(
            (centre + 0.2).abs() < 1e-12 && undefined == DVec3::ZERO,
            "a ball's centre has no way out"
        );
    }

    /// A made-up approach: the bone's ends in the shape's axes, the least distance, its share and the way out there, and
    /// for a halving search the distance to the centre of the curve the bone passes, which sets how far past the least it
    /// stops.
    struct Analytic {
        ends: (DVec3, DVec3),
        least: f64,
        share: f64,
        out: DVec3,
        curve: f64,
    }

    /// The shapes of the made-up approaches: a box, a cylinder, a capsule, a ball and a floor.
    fn analytic_shapes() -> [Shape; 5] {
        [
            Shape::Box {
                half: DVec3::new(0.3, 0.1, 0.2),
            },
            Shape::Cylinder {
                radius: 0.2,
                height: 0.4,
            },
            Shape::Capsule {
                radius: 0.1,
                height: 0.5,
            },
            Shape::Sphere { radius: 0.12 },
            Shape::Plane {
                normal: DVec3::Y,
                distance: -0.1,
            },
        ]
    }

    /// The made-up approaches to those shapes: past the box's corner, along the cylinder's side, beside the capsule's
    /// straight part, over the ball and down to the floor.
    fn analytic() -> [Analytic; 5] {
        let case = |ends, least, share, out, curve| Analytic {
            ends,
            least,
            share,
            out,
            curve,
        };
        let corner = DVec3::new(1.0, 1.0, 0.0).normalize();
        let side = |a: f64, b: f64, c: f64| DVec3::new(a, b, c);
        [
            case(
                (side(0.5, 0.2, 0.0), side(0.2, 0.5, 0.0)),
                0.3 / 2.0_f64.sqrt(),
                1.0 / 6.0,
                corner,
                0.2122,
            ),
            case(
                (side(-0.05, 0.0, 0.21), side(0.05, 0.0, 0.21)),
                0.01,
                0.5,
                DVec3::Z,
                0.21,
            ),
            case(
                (side(0.3, -0.1, 0.0), side(0.3, 0.1, 0.0)),
                0.2,
                1.0,
                DVec3::X,
                0.0,
            ),
            case(
                (side(-0.2, 0.3, 0.0), side(0.2, 0.3, 0.0)),
                0.18,
                0.5,
                DVec3::Y,
                0.0,
            ),
            case(
                (side(0.0, 0.3, 0.0), side(0.4, 0.1, 0.0)),
                0.2,
                1.0,
                DVec3::Y,
                0.0,
            ),
        ]
    }

    #[test]
    fn the_nearest_approach_along_a_bone_meets_its_analytic_least() {
        let placement = turned();
        for (shape, expected) in analytic_shapes().into_iter().zip(analytic()) {
            let solid = at(placement, shape);
            let origin = placement.transform_point3(expected.ends.0);
            let tail = placement.transform_point3(expected.ends.1);
            let found = solid.along(origin, tail);
            let length = origin.distance(tail);
            let slack = 1.01 * TOLERANCE * expected.curve / (length * length) + 1e-12;
            assert!(
                (found.share - expected.share).abs() <= slack,
                "{shape:?}: share {}",
                found.share
            );
            assert!(
                (found.gap - expected.least).abs() <= 1e-10,
                "{shape:?}: {}",
                found.gap
            );
            assert!(
                found.normal.distance(placement.matrix3 * expected.out) < 1e-4,
                "{shape:?}: {}",
                found.normal
            );
            assert!(near(found.point, origin.lerp(tail, found.share)));
        }
    }

    #[test]
    fn a_bone_lying_on_a_flat_finds_its_witness_furthest_along_it() {
        let cube = at(
            DAffine3::IDENTITY,
            Shape::Box {
                half: DVec3::new(0.5, 0.1, 0.5),
            },
        );
        let found = cube.along(DVec3::new(0.45, 0.108, 0.0), DVec3::new(0.55, 0.108, 0.0));
        assert!(
            (found.share - 0.5).abs() < 1e-6,
            "over the edge, the witness is the edge: {}",
            found.share
        );
        assert!((found.gap - 0.008).abs() < 1e-9 && found.normal.distance(DVec3::Y) < 1e-4);
        let lying = cube.along(DVec3::new(0.1, 0.108, 0.0), DVec3::new(0.2, 0.108, 0.0));
        assert!(
            (lying.share - 1.0).abs() < 1e-6,
            "wholly on the face, the tail"
        );
        let floor = at(
            DAffine3::IDENTITY,
            Shape::Plane {
                normal: DVec3::Y,
                distance: 0.0,
            },
        );
        assert!(
            (floor
                .along(DVec3::new(0.0, 0.1, 0.0), DVec3::new(1.0, 0.1, 0.0))
                .share
                - 1.0)
                .abs()
                < 1e-12
        );
        let rod = at(
            DAffine3::IDENTITY,
            Shape::Capsule {
                radius: 0.05,
                height: 0.5,
            },
        );
        let beside = rod.along(DVec3::new(0.1, -0.5, 0.0), DVec3::new(0.1, 0.5, 0.0));
        assert!(
            (beside.share - 0.7).abs() < 1e-12,
            "beside the straight part, its far end: {}",
            beside.share
        );
        assert!((beside.gap - 0.05).abs() < 1e-12 && near(beside.normal, DVec3::X));
    }

    #[test]
    fn a_point_on_a_cylinders_side_or_end_has_no_gap_and_leaves_through_that_face() {
        let can = at(
            DAffine3::IDENTITY,
            Shape::Cylinder {
                radius: 0.2,
                height: 0.4,
            },
        );
        for (point, out) in [
            (DVec3::new(0.2, 0.0, 0.0), DVec3::X),
            (DVec3::new(0.0, 0.2, 0.0), DVec3::Y),
        ] {
            let (gap, normal) = can.signed(point);
            assert!(
                gap.abs() < 1e-12 && near(normal, out),
                "{point}: {gap}, {normal}"
            );
        }
    }

    fn slab() -> DAffine3 {
        DAffine3::from_translation(DVec3::new(2.0, 3.0, 4.0))
    }

    /// A 2 m bone 1 cm above the top of a box 1 m wide lies along its face from a quarter to three quarters of its
    /// length.
    #[test]
    fn a_bone_along_a_flat_finds_where_the_flat_begins() {
        for placement in [DAffine3::IDENTITY, slab()] {
            let cube = at(
                placement,
                Shape::Box {
                    half: DVec3::new(0.5, 0.1, 0.5),
                },
            );
            let origin = placement.transform_point3(DVec3::new(-1.0, 0.11, 0.0));
            let tail = placement.transform_point3(DVec3::new(1.0, 0.11, 0.0));
            let start = cube.near_end(origin, tail, 0.75).unwrap();
            let edge = placement.transform_point3(DVec3::new(-0.5, 0.11, 0.0));
            assert!(
                start.point.distance(edge) <= RESOLUTION,
                "{}",
                start.point - edge
            );
            assert!((start.gap - 0.01).abs() < 1e-9 && start.normal.distance(DVec3::Y) < 1e-4);
            let inside = placement.transform_point3(DVec3::new(0.0, 0.11, 0.0));
            assert_eq!(
                cube.near_end(inside, tail, 0.5),
                None,
                "it begins on the flat"
            );
        }
    }

    #[test]
    fn a_box_or_cylinder_supports_a_point_by_every_nearest_face_facing_out() {
        let placement = turned();
        let world = |local: DVec3| placement.transform_point3(local);
        let ways = |solid: &Solid, local: DVec3| {
            let (found, count) = solid.supports(world(local));
            found.into_iter().take(count).collect::<Vec<_>>()
        };
        let expect = |found: Vec<DVec3>, local: &[DVec3]| {
            assert_eq!(found.len(), local.len(), "{found:?}");
            for (way, out) in found.iter().zip(local) {
                assert!(near(*way, placement.matrix3 * *out), "{found:?}");
            }
        };
        let cube = at(
            placement,
            Shape::Box {
                half: DVec3::new(1.0, 2.0, 3.0),
            },
        );
        expect(ways(&cube, DVec3::ZERO), &[DVec3::X]);
        expect(
            ways(&cube, DVec3::new(-0.9, 1.9, 0.0)),
            &[DVec3::NEG_X, DVec3::Y],
        );
        expect(
            ways(&cube, DVec3::new(0.9, 1.9, 2.9)),
            &[DVec3::X, DVec3::Y, DVec3::Z],
        );
        let can = at(
            placement,
            Shape::Cylinder {
                radius: 1.0,
                height: 2.0,
            },
        );
        expect(
            ways(&can, DVec3::new(0.9, -0.9, 0.0)),
            &[DVec3::X, DVec3::NEG_Y],
        );
        expect(ways(&can, DVec3::new(0.9, 0.0, 0.0)), &[DVec3::X]);
        expect(ways(&can, DVec3::new(0.1, -0.9, 0.0)), &[DVec3::NEG_Y]);
        let floor = at(
            placement,
            Shape::Plane {
                normal: DVec3::Y,
                distance: 0.0,
            },
        );
        expect(ways(&floor, DVec3::new(0.3, -0.2, 0.1)), &[DVec3::Y]);
        let ball = at(placement, Shape::Sphere { radius: 0.5 });
        expect(ways(&ball, DVec3::ZERO), &[]);
    }

    #[test]
    fn a_bone_moving_away_from_a_box_is_nearest_at_its_origin_and_towards_it_at_its_tail() {
        let cube = at(slab(), Shape::Box { half: DVec3::ONE });
        let (low, high) = (DVec3::new(2.0, 4.5, 4.0), DVec3::new(2.5, 5.5, 4.0));
        let away = cube.along(low, high);
        assert!(away.share.abs() < 1e-12 && (away.gap - 0.5).abs() < 1e-12);
        let towards = cube.along(high, low);
        assert!((towards.share - 1.0).abs() < 1e-12 && (towards.gap - 0.5).abs() < 1e-12);
    }

    /// The flat a 2 m bone 1 cm over a box's top lies along ends over the box's edge, which the search finds within its
    /// resolution, past by at most the share at which the corner's slope reaches the tolerance.
    #[test]
    fn the_witness_of_a_bone_over_a_flat_is_its_far_end_within_the_resolution() {
        let cube = at(
            slab(),
            Shape::Box {
                half: DVec3::new(0.5, 0.1, 0.5),
            },
        );
        let origin = slab().transform_point3(DVec3::new(-1.0, 0.11, 0.0));
        let tail = slab().transform_point3(DVec3::new(1.0, 0.11, 0.0));
        let found = cube.along(origin, tail);
        let edge = slab().translation.x + 0.5;
        assert!(
            found.point.x >= edge - RESOLUTION && found.point.x <= edge + RESOLUTION + 1e-8,
            "{}",
            found.point.x - edge
        );
    }

    /// Inside a box a bone crossing from where one face is nearest to where another is turns at a crease, where the
    /// way out is the mix of both faces' ways weighted by how fast the distance runs to each: zero between opposite
    /// faces, and five sixths of the rising face and a sixth of the falling one when the falling one runs five times
    /// as fast, or nine elevenths of the rising face and two elevenths of the falling one when it runs four and a
    /// half times as fast. The mix is read just past the crease, so a bone that also runs towards a third face,
    /// here +Z, still mixes only the two faces meeting at its least.
    #[test]
    fn a_crease_inside_a_box_leaves_by_the_slope_weighted_mix_of_its_faces() {
        let cube = at(slab(), Shape::Box { half: DVec3::ONE });
        let world = |local: DVec3| slab().transform_point3(local);
        let opposite = cube.along(
            world(DVec3::new(-0.5, 0.0, 0.0)),
            world(DVec3::new(0.5, 0.0, 0.0)),
        );
        assert!((opposite.share - 0.5).abs() < 1e-6 && (opposite.gap + 1.0).abs() < 1e-6);
        assert!(opposite.normal.length() < 1e-6, "{}", opposite.normal);
        let corner = cube.along(
            world(DVec3::new(0.1, 0.9, 0.0)),
            world(DVec3::new(0.3, -0.1, 0.0)),
        );
        assert!((corner.share - 2.0 / 3.0).abs() < 1e-6, "{}", corner.share);
        assert!(
            corner.normal.distance(DVec3::new(5.0, 1.0, 0.0) / 6.0) < 1e-6,
            "{}",
            corner.normal
        );
        let shallower = cube.along(
            world(DVec3::new(0.1, 0.9, 0.0)),
            world(DVec3::new(0.3, 0.0, 0.0)),
        );
        assert!(
            (shallower.share - 0.8 / 1.1).abs() < 1e-6,
            "{}",
            shallower.share
        );
        assert!(
            shallower.normal.distance(DVec3::new(9.0, 2.0, 0.0) / 11.0) < 1e-6,
            "{}",
            shallower.normal
        );
        let rising_in_z = cube.along(
            world(DVec3::new(0.1, 0.9, -0.9)),
            world(DVec3::new(0.3, -0.1, 0.5)),
        );
        assert!(
            (rising_in_z.share - 2.0 / 3.0).abs() < 1e-6,
            "{}",
            rising_in_z.share
        );
        assert!(
            rising_in_z.normal.distance(DVec3::new(5.0, 1.0, 0.0) / 6.0) < 1e-6,
            "{}",
            rising_in_z.normal
        );
    }

    /// The share along a bone from `start` by `span` nearest a core from `core` by `along`, each moved and turned the
    /// same way, or both scaled down ten thousand times.
    fn nearest_everywhere(start: DVec3, span: DVec3, core: DVec3, along: DVec3) -> [f64; 3] {
        let moved = DAffine3::from_rotation_translation(
            DQuat::from_axis_angle(DVec3::new(1.0, -2.0, 0.5).normalize(), 0.9),
            DVec3::new(2.0, 3.0, 4.0),
        );
        let point = |at: DVec3| moved.transform_point3(at);
        let way = |by: DVec3| moved.transform_vector3(by);
        [
            nearest_share(start, span, core, along),
            nearest_share(point(start), way(span), point(core), way(along)),
            nearest_share(start * 1e-4, span * 1e-4, core * 1e-4, along * 1e-4),
        ]
    }

    #[test]
    fn a_bone_is_nearest_a_skew_core_where_their_common_perpendicular_meets_it() {
        let (start, span) = (DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0));
        let cases = [
            (DVec3::new(0.5, 1.0, -1.0), DVec3::new(0.0, 0.0, 2.0), 0.25),
            (DVec3::new(0.5, 1.0, -1.0), DVec3::new(1.0, 0.0, 2.0), 0.5),
            (DVec3::new(0.5, 1.0, 1.0), DVec3::new(1.0, 0.0, 2.0), 0.25),
            (DVec3::new(0.5, 1.0, -3.0), DVec3::new(1.0, 0.0, 2.0), 0.75),
        ];
        for (core, along, share) in cases {
            for found in nearest_everywhere(start, span, core, along) {
                assert!((found - share).abs() < 1e-9, "{core} by {along}: {found}");
            }
        }
    }

    #[test]
    fn no_measured_furniture_stands_anywhere() {
        let mut none = Unmeasured;
        let centre = DVec3::new(1.0, -2.0, 3.0);
        assert!(!none.within(centre, 5.0));
        assert_eq!(none.rest(centre, 0.5), None);
        assert!((none.cast(centre, DVec3::new(0.0, -4.0, 0.0), 0.5) - 1.0).abs() < 1e-12);
    }

    /// A board of measured furniture: across `axis` from `low` to `high`, and along the other of X and Y from `from`
    /// to `to`.
    struct Slab {
        axis: usize,
        low: f64,
        high: f64,
        from: f64,
        to: f64,
    }

    /// Measured boards, as a host's physics would answer about them, its sweeps missing every board when `swept` is
    /// false and a sweep that starts touching one meeting nothing, with the radius of every ball asked where it
    /// rests.
    struct Boards {
        slabs: Vec<Slab>,
        swept: bool,
        radii: Vec<f64>,
    }

    impl Boards {
        fn new(slabs: Vec<Slab>, swept: bool) -> Self {
            Self {
                slabs,
                swept,
                radii: Vec::new(),
            }
        }
    }

    impl Measured for Boards {
        fn within(&mut self, _centre: DVec3, _reach: f64) -> bool {
            true
        }

        fn rest(&mut self, point: DVec3, radius: f64) -> Option<Contact> {
            self.radii.push(radius);
            self.slabs.iter().find_map(|slab| {
                let other = 1 - slab.axis;
                if !(slab.from..=slab.to).contains(&point[other]) {
                    return None;
                }
                let (face, side) = if point[slab.axis] >= f64::midpoint(slab.low, slab.high) {
                    (slab.high, 1.0)
                } else {
                    (slab.low, -1.0)
                };
                let mut touched = point;
                touched[slab.axis] = face;
                let mut normal = DVec3::ZERO;
                normal[slab.axis] = side;
                ((point[slab.axis] - face) * side < radius).then_some(Contact {
                    point: touched,
                    normal,
                })
            })
        }

        fn cast(&mut self, start: DVec3, motion: DVec3, radius: f64) -> f64 {
            if !self.swept {
                return 1.0;
            }
            let mut first = 1.0_f64;
            for slab in &self.slabs {
                let (axis, other) = (slab.axis, 1 - slab.axis);
                let (top, bottom) = (slab.high + radius, slab.low - radius);
                let share = if start[axis] > top && motion[axis] < 0.0 {
                    (start[axis] - top) / -motion[axis]
                } else if start[axis] < bottom && motion[axis] > 0.0 {
                    (bottom - start[axis]) / motion[axis]
                } else {
                    continue;
                };
                let across = start[other] + motion[other] * share;
                if (0.0..1.0).contains(&share) && (slab.from..=slab.to).contains(&across) {
                    first = first.min(share);
                }
            }
            first
        }
    }

    /// A bone 0.1 m long hanging 5 cm over a board whose top is at y = 2, its tail swung from level to straight down.
    fn over_the_board() -> (Swing, DVec3) {
        let origin = DVec3::new(3.0, 2.05, 0.0);
        let swing = Swing {
            origin,
            reach: 0.1,
            hit: 0.01,
            before: origin + DVec3::X * 0.1,
        };
        (swing, origin - DVec3::Y * 0.1)
    }

    /// The tail is swept through a 2 cm board, or comes to rest in a block 1 m deep that the host's sweep misses.
    #[test]
    fn a_tail_swept_or_resting_into_measured_furniture_turns_out_to_its_hit_radius_at_its_length() {
        for swept in [true, false] {
            let (swing, down) = over_the_board();
            let board = Slab {
                axis: 1,
                low: if swept { 1.98 } else { 1.0 },
                high: 2.0,
                from: -10.0,
                to: 10.0,
            };
            let mut boards = Boards::new(vec![board], swept);
            let out = clear_measured(&swing, down, &mut boards);
            assert!(
                (out.distance(swing.origin) - 0.1).abs() < 1e-9,
                "{swept}: {out}"
            );
            assert!((out.y - 2.01).abs() < 1e-9, "{swept}: {out}");
            assert!(
                out.x > swing.origin.x,
                "{swept}: turned the way it came, {out}"
            );
            assert!(
                boards
                    .radii
                    .iter()
                    .all(|radius| (radius - 0.0105).abs() < 1e-12),
                "a ball a touch larger than the hit radius looks: {:?}",
                boards.radii
            );
        }
    }

    #[test]
    fn a_ball_resting_on_measured_furniture_meets_it_moving_in_and_not_moving_out_or_along() {
        let board = || Slab {
            axis: 1,
            low: 1.98,
            high: 2.0,
            from: -10.0,
            to: 10.0,
        };
        let start = DVec3::new(3.0, 2.01, 0.0);
        let mut boards = Boards::new(vec![board()], true);
        let inward = measured_contact(&mut boards, start, DVec3::new(3.0, 1.99, 0.0), 0.01);
        assert_eq!(inward.map(|touch| touch.normal), Some(DVec3::Y));
        for to in [DVec3::new(3.0, 2.05, 0.0), DVec3::new(3.05, 2.01, 0.0)] {
            let mut boards = Boards::new(vec![board()], true);
            assert_eq!(measured_contact(&mut boards, start, to, 0.01), None, "{to}");
        }
    }

    /// A unit bone swung from level to straight down meets a board below it, and turned out of that, a second board
    /// standing in its new path: it is turned out of both, not left past the second.
    #[test]
    fn a_tail_turned_out_of_one_measured_board_into_another_is_turned_out_of_that_too() {
        let swing = Swing {
            origin: DVec3::ZERO,
            reach: 1.0,
            hit: 0.01,
            before: DVec3::X,
        };
        let floor = Slab {
            axis: 1,
            low: -0.62,
            high: -0.6,
            from: -10.0,
            to: 10.0,
        };
        let wall = Slab {
            axis: 0,
            low: 0.83,
            high: 0.85,
            from: -0.48,
            to: -0.40,
        };
        let mut boards = Boards::new(vec![floor, wall], true);
        let out = clear_measured(&swing, DVec3::NEG_Y, &mut boards);
        assert!(out.x >= 0.86 - 1e-9, "left past the second board, at {out}");
        assert!(
            out.y >= -0.59 - 1e-9 && (out.length() - 1.0).abs() < 1e-9,
            "{out}"
        );
    }
}
