//! Poses and the rotation operations the solver needs, each as the engine the solver was tuned in defines it, so a
//! port keeps its feel; nothing here names an engine type.

use glam::{DAffine3, DMat3, DQuat, DVec3};

/// The tolerance below which two values are taken as equal.
const EPSILON: f64 = 0.00001;

/// A bone's pose relative to its parent: where it stands, how it is turned and how it is scaled, in that order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Where the bone stands in its parent's space.
    pub translation: DVec3,
    /// The bone's turn, a unit quaternion.
    pub rotation: DQuat,
    /// The bone's scale along its own axes.
    pub scale: DVec3,
}

impl Pose {
    /// No translation, no turn and unit scale.
    pub const IDENTITY: Self = Self {
        translation: DVec3::ZERO,
        rotation: DQuat::IDENTITY,
        scale: DVec3::ONE,
    };

    /// The pose `weight` of the way to `to`: turned the shortest way, scaled and moved in a straight line.
    #[must_use]
    pub(crate) fn interpolate(self, to: Self, weight: f64) -> Self {
        Self {
            translation: self.translation.lerp(to.translation, weight),
            rotation: slerp(self.rotation, to.rotation, weight).normalize(),
            scale: self.scale.lerp(to.scale, weight),
        }
    }

    /// The pose as a placement: its turn, then its scale along its own axes, at its translation.
    #[must_use]
    pub fn affine(self) -> DAffine3 {
        DAffine3::from_mat3_translation(
            basis(self.rotation) * DMat3::from_diagonal(self.scale),
            self.translation,
        )
    }
}

impl From<DAffine3> for Pose {
    /// The placement's translation, turn and scale along its own axes, as glam splits them.
    fn from(placement: DAffine3) -> Self {
        let (scale, rotation, translation) = placement.to_scale_rotation_translation();
        Self {
            translation,
            rotation,
            scale,
        }
    }
}

/// An axis-aligned box: its least corner and its size, which is never negative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// The least corner.
    pub position: DVec3,
    /// The extent along each axis.
    pub size: DVec3,
}

impl Bounds {
    /// The cube of half side `reach` around `centre`.
    #[must_use]
    pub(crate) fn around(centre: DVec3, reach: f64) -> Self {
        Self {
            position: centre - DVec3::splat(reach),
            size: DVec3::splat(2.0 * reach),
        }
    }

    /// The box grown by `by` on every side.
    #[must_use]
    pub(crate) fn grow(self, by: f64) -> Self {
        Self {
            position: self.position - DVec3::splat(by),
            size: self.size + DVec3::splat(2.0 * by),
        }
    }

    /// Whether `point` lies in the box or on its faces.
    #[must_use]
    pub(crate) fn has_point(self, point: DVec3) -> bool {
        let end = self.position + self.size;
        point.cmpge(self.position).all() && point.cmple(end).all()
    }

    /// Whether the boxes overlap by more than a face.
    #[must_use]
    pub(crate) fn intersects(self, other: Self) -> bool {
        let end = self.position + self.size;
        let other_end = other.position + other.size;
        self.position.cmplt(other_end).all() && end.cmpgt(other.position).all()
    }

    /// The least box holding both.
    #[must_use]
    pub(crate) fn merge(self, other: Self) -> Self {
        let position = self.position.min(other.position);
        let end = (self.position + self.size).max(other.position + other.size);
        Self {
            position,
            size: end - position,
        }
    }

    /// The least axis-aligned box holding this one placed by `placement`.
    #[must_use]
    pub(crate) fn placed(self, placement: DAffine3) -> Self {
        let low = self.position;
        let high = self.position + self.size;
        let mut least = placement.translation;
        let mut most = placement.translation;
        for row in 0..3 {
            for column in 0..3 {
                let element = placement.matrix3.col(column)[row];
                let (one, other) = (element * low[column], element * high[column]);
                least[row] += one.min(other);
                most[row] += one.max(other);
            }
        }
        Self {
            position: least,
            size: most - least,
        }
    }
}

/// The rotation matrix of `turn`, which need not be of unit length.
#[must_use]
pub(crate) fn basis(turn: DQuat) -> DMat3 {
    let scale = 2.0 / turn.length_squared();
    let (xs, ys, zs) = (turn.x * scale, turn.y * scale, turn.z * scale);
    let (wx, wy, wz) = (turn.w * xs, turn.w * ys, turn.w * zs);
    let (xx, xy, xz) = (turn.x * xs, turn.x * ys, turn.x * zs);
    let (yy, yz, zz) = (turn.y * ys, turn.y * zs, turn.z * zs);
    DMat3::from_cols(
        DVec3::new(1.0 - (yy + zz), xy + wz, xz - wy),
        DVec3::new(xy - wz, 1.0 - (xx + zz), yz + wx),
        DVec3::new(xz + wy, yz - wx, 1.0 - (xx + yy)),
    )
}

/// The proper rotation in a matrix that may be scaled, reflections taken into the scale; see [`scale_of`].
#[must_use]
pub(crate) fn rotation_of(matrix: DMat3) -> DQuat {
    let mut rotation = orthonormalized(matrix);
    if rotation.determinant() < 0.0 {
        rotation = -rotation;
    }
    quaternion_of(rotation)
}

/// The scale along each column, negative throughout when the matrix reflects.
#[must_use]
pub(crate) fn scale_of(matrix: DMat3) -> DVec3 {
    sign_of(matrix.determinant())
        * DVec3::new(
            matrix.x_axis.length(),
            matrix.y_axis.length(),
            matrix.z_axis.length(),
        )
}

/// The matrix's columns made unit length and square to each other, the first first.
#[must_use]
pub(crate) fn orthonormalized(matrix: DMat3) -> DMat3 {
    let x = matrix.x_axis.normalize_or_zero();
    let y = (matrix.y_axis - x * x.dot(matrix.y_axis)).normalize_or_zero();
    let z =
        (matrix.z_axis - x * x.dot(matrix.z_axis) - y * y.dot(matrix.z_axis)).normalize_or_zero();
    DMat3::from_cols(x, y, z)
}

/// The placement with its matrix orthonormalized and its translation kept.
#[must_use]
pub(crate) fn without_scale(placement: DAffine3) -> DAffine3 {
    DAffine3::from_mat3_translation(orthonormalized(placement.matrix3), placement.translation)
}

/// The placement `weight` of the way to `to`, each decomposed into turn, scale and translation.
#[must_use]
pub(crate) fn interpolate(from: DAffine3, to: DAffine3, weight: f64) -> DAffine3 {
    let turn = slerp(rotation_of(from.matrix3), rotation_of(to.matrix3), weight).normalize();
    let scale = scale_of(from.matrix3).lerp(scale_of(to.matrix3), weight);
    DAffine3::from_mat3_translation(
        basis(turn) * DMat3::from_diagonal(scale),
        from.translation.lerp(to.translation, weight),
    )
}

/// The unit quaternion of a rotation matrix, by its trace.
fn quaternion_of(matrix: DMat3) -> DQuat {
    let row = |i: usize, j: usize| matrix.col(j)[i];
    let trace = row(0, 0) + row(1, 1) + row(2, 2);
    if trace > 0.0 {
        let root = (trace + 1.0).sqrt();
        let half = 0.5 / root;
        return DQuat::from_xyzw(
            (row(2, 1) - row(1, 2)) * half,
            (row(0, 2) - row(2, 0)) * half,
            (row(1, 0) - row(0, 1)) * half,
            root * 0.5,
        );
    }
    let i = if row(0, 0) < row(1, 1) {
        if row(1, 1) < row(2, 2) { 2 } else { 1 }
    } else if row(0, 0) < row(2, 2) {
        2
    } else {
        0
    };
    let (j, k) = ((i + 1) % 3, (i + 2) % 3);
    let root = (row(i, i) - row(j, j) - row(k, k) + 1.0).sqrt();
    let half = 0.5 / root;
    let mut axis = DVec3::ZERO;
    axis[i] = root * 0.5;
    axis[j] = (row(j, i) + row(i, j)) * half;
    axis[k] = (row(k, i) + row(i, k)) * half;
    DQuat::from_xyzw(axis.x, axis.y, axis.z, (row(k, j) - row(j, k)) * half)
}

/// The turn `weight` of the way from `from` to `to` along the shorter arc, at an even rate.
#[must_use]
pub(crate) fn slerp(from: DQuat, to: DQuat, weight: f64) -> DQuat {
    let mut cosine = from.dot(to);
    let mut end = to;
    if cosine < 0.0 {
        cosine = -cosine;
        end = -to;
    }
    let (start_share, end_share) = if 1.0 - cosine > EPSILON {
        let omega = cosine.acos();
        let sine = omega.sin();
        (
            ((1.0 - weight) * omega).sin() / sine,
            (weight * omega).sin() / sine,
        )
    } else {
        (1.0 - weight, weight)
    };
    DQuat::from_xyzw(
        start_share * from.x + end_share * end.x,
        start_share * from.y + end_share * end.y,
        start_share * from.z + end_share * end.z,
        start_share * from.w + end_share * end.w,
    )
}

/// The shortest turn taking the direction of `from` to that of `to`; none when they already agree.
#[must_use]
pub(crate) fn arc(from: DVec3, to: DVec3) -> DQuat {
    const ALMOST_ONE: f64 = 0.999_999_999_999_999;
    let start = from.normalize_or_zero();
    let end = to.normalize_or_zero();
    let cosine = start.dot(end);
    if cosine.abs() > ALMOST_ONE {
        if cosine >= 0.0 {
            return DQuat::IDENTITY;
        }
        let axis = start.cross(
            if start.x.abs() <= start.y.abs() && start.x.abs() <= start.z.abs() {
                DVec3::X
            } else {
                DVec3::Y
            },
        );
        let axis = axis.normalize_or_zero();
        return DQuat::from_xyzw(axis.x, axis.y, axis.z, 0.0);
    }
    let cross = start.cross(end);
    let root = ((1.0 + cosine) * 2.0).sqrt();
    DQuat::from_xyzw(cross.x / root, cross.y / root, cross.z / root, root * 0.5)
}

/// The angles of a rotation matrix about X, then Y, then Z, applied as X·Y·Z; a pure turn about Y reads as that alone.
#[must_use]
pub(crate) fn euler_xyz(matrix: DMat3) -> DVec3 {
    const NEAR_POLE: f64 = 0.000_000_25;
    let row = |i: usize, j: usize| matrix.col(j)[i];
    let sine_y = row(0, 2);
    if sine_y >= 1.0 - NEAR_POLE {
        return DVec3::new(row(2, 1).atan2(row(1, 1)), std::f64::consts::FRAC_PI_2, 0.0);
    }
    if sine_y <= -(1.0 - NEAR_POLE) {
        return DVec3::new(
            row(2, 1).atan2(row(1, 1)),
            -std::f64::consts::FRAC_PI_2,
            0.0,
        );
    }
    #[expect(
        clippy::float_cmp,
        reason = "the engine reads only an exact turn about Y as one"
    )]
    let pure_y = row(1, 0) == 0.0
        && row(0, 1) == 0.0
        && row(1, 2) == 0.0
        && row(2, 1) == 0.0
        && row(1, 1) == 1.0;
    if pure_y {
        return DVec3::new(0.0, row(0, 2).atan2(row(0, 0)), 0.0);
    }
    DVec3::new(
        (-row(1, 2)).atan2(row(2, 2)),
        sine_y.asin(),
        (-row(0, 1)).atan2(row(0, 0)),
    )
}

/// The rotation matrix turning about X, then Y, then Z by `angles`, applied as X·Y·Z.
#[must_use]
pub(crate) fn from_euler_xyz(angles: DVec3) -> DMat3 {
    DMat3::from_rotation_x(angles.x)
        * (DMat3::from_rotation_y(angles.y) * DMat3::from_rotation_z(angles.z))
}

/// Whether two vectors agree in each component within a tolerance relative to the first's magnitude.
#[must_use]
#[expect(
    clippy::float_cmp,
    reason = "equal infinities are equal, as the engine takes them"
)]
pub(crate) fn equal_approx(a: DVec3, b: DVec3) -> bool {
    (0..3).all(|axis| {
        let (left, right) = (a[axis], b[axis]);
        left == right || (left - right).abs() < (EPSILON * left.abs()).max(EPSILON)
    })
}

/// Whether every component lies within the tolerance of zero.
#[must_use]
pub(crate) fn zero_approx(vector: DVec3) -> bool {
    vector.abs().cmplt(DVec3::splat(EPSILON)).all()
}

/// The point of the segment from `start` to `end` nearest `point`.
#[must_use]
pub(crate) fn closest_on_segment(point: DVec3, start: DVec3, end: DVec3) -> DVec3 {
    let along = end - start;
    let length_squared = along.length_squared();
    if length_squared < 1e-20 {
        return start;
    }
    let share = along.dot(point - start) / length_squared;
    if share <= 0.0 {
        start
    } else if share >= 1.0 {
        end
    } else {
        start + along * share
    }
}

/// The smooth step from 0 to 1 over `share`, level at both ends.
#[must_use]
pub(crate) fn smoothstep(share: f64) -> f64 {
    let clamped = share.clamp(0.0, 1.0);
    clamped * clamped * (3.0 - 2.0 * clamped)
}

/// The sign of `value`, zero for zero.
#[must_use]
pub(crate) fn sign_of(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value.signum() }
}

/// The axis of the largest component, the earlier of any that tie.
#[must_use]
pub(crate) fn max_axis(vector: DVec3) -> usize {
    if vector.x < vector.y {
        if vector.y < vector.z { 2 } else { 1 }
    } else if vector.x < vector.z {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use glam::{DAffine3, DMat3, DQuat, DVec3};

    use super::*;

    fn near(a: DVec3, b: DVec3) -> bool {
        a.distance(b) < 1e-9
    }

    /// The angle between two turns, sign-blind.
    fn apart(a: DQuat, b: DQuat) -> f64 {
        let dot = a.normalize().dot(b.normalize()).abs().min(1.0);
        2.0 * dot.acos()
    }

    #[test]
    fn a_turns_matrix_turns_as_the_quaternion_does_and_reads_back() {
        let turn = DQuat::from_axis_angle(DVec3::new(0.3, -0.8, 0.5).normalize(), 1.1);
        let matrix = basis(turn);
        let point = DVec3::new(0.2, 0.7, -1.3);
        assert!(near(matrix * point, turn * point));
        assert!(apart(rotation_of(matrix), turn) < 1e-12);
        assert!(
            near(basis(turn * 3.0) * point, turn * point),
            "a longer quaternion turns the same way"
        );
    }

    #[test]
    fn a_scaled_or_mirrored_matrix_gives_its_turn_and_its_signed_scale() {
        let turn = DQuat::from_rotation_y(0.6);
        let scaled = basis(turn) * DMat3::from_diagonal(DVec3::new(2.0, 3.0, 0.5));
        assert!(apart(rotation_of(scaled), turn) < 1e-12);
        assert!(near(scale_of(scaled), DVec3::new(2.0, 3.0, 0.5)));
        let mirrored = basis(turn) * DMat3::from_diagonal(DVec3::new(-1.0, -1.0, -1.0));
        assert!(near(scale_of(mirrored), DVec3::splat(-1.0)));
        assert!(
            apart(rotation_of(mirrored), turn) < 1e-12,
            "the reflection is taken into the scale"
        );
    }

    #[test]
    fn a_placement_interpolates_turn_scale_and_translation_apart() {
        let from = DAffine3::IDENTITY;
        let to = DAffine3::from_scale_rotation_translation(
            DVec3::splat(3.0),
            DQuat::from_rotation_z(1.0),
            DVec3::X * 4.0,
        );
        let half = interpolate(from, to, 0.5);
        assert!(near(scale_of(half.matrix3), DVec3::splat(2.0)));
        assert!(apart(rotation_of(half.matrix3), DQuat::from_rotation_z(0.5)) < 1e-12);
        assert!(near(half.translation, DVec3::X * 2.0));
    }

    #[test]
    fn a_pose_interpolates_on_the_shorter_arc_and_places_as_turn_then_scale() {
        let from = Pose {
            translation: DVec3::ZERO,
            rotation: DQuat::from_rotation_x(0.2),
            scale: DVec3::ONE,
        };
        let to = Pose {
            translation: DVec3::Y,
            rotation: -DQuat::from_rotation_x(0.6),
            scale: DVec3::splat(3.0),
        };
        let half = from.interpolate(to, 0.5);
        assert!(apart(half.rotation, DQuat::from_rotation_x(0.4)) < 1e-12);
        assert!(near(half.translation, DVec3::Y * 0.5) && near(half.scale, DVec3::splat(2.0)));
        let placed = to.affine();
        assert!(near(
            placed.transform_point3(DVec3::Z),
            DVec3::Y + to.rotation * (DVec3::Z * 3.0)
        ));
    }

    #[test]
    fn slerp_keeps_its_ends_and_an_even_rate() {
        let from = DQuat::from_rotation_y(0.1);
        let to = DQuat::from_rotation_y(1.3);
        assert!(
            apart(slerp(from, to, 0.0), from) < 1e-12 && apart(slerp(from, to, 1.0), to) < 1e-12
        );
        assert!(apart(slerp(from, to, 0.25), DQuat::from_rotation_y(0.4)) < 1e-12);
        assert!(
            apart(slerp(from, from, 0.5), from) < 1e-12,
            "turns that agree interpolate linearly"
        );
    }

    #[test]
    fn an_arc_takes_one_direction_to_the_other() {
        let from = DVec3::new(0.0, -2.0, 0.0);
        let to = DVec3::new(0.5, 0.2, -0.3);
        assert!(near(arc(from, to) * from.normalize(), to.normalize()));
        assert_eq!(arc(from, from * 4.0), DQuat::IDENTITY);
        let half_turn = arc(DVec3::Y, DVec3::NEG_Y);
        assert!(near(half_turn * DVec3::Y, DVec3::NEG_Y));
        assert!(
            (half_turn * DVec3::X).dot(DVec3::Y).abs() < 1e-12,
            "a half turn is about an axis square to both"
        );
    }

    #[test]
    fn euler_angles_read_back_what_made_them() {
        for angles in [
            DVec3::new(0.3, -0.5, 1.2),
            DVec3::new(-1.0, 0.9, -2.5),
            DVec3::ZERO,
        ] {
            assert!(near(euler_xyz(from_euler_xyz(angles)), angles), "{angles}");
        }
        let x_then_y = DMat3::from_rotation_x(0.4)
            * DMat3::from_rotation_y(0.7)
            * DMat3::from_rotation_z(-0.2);
        assert!(
            near(euler_xyz(x_then_y), DVec3::new(0.4, 0.7, -0.2)),
            "applied as X·Y·Z"
        );
        assert!(
            near(
                euler_xyz(DMat3::from_rotation_y(2.5)),
                DVec3::new(0.0, 2.5, 0.0)
            ),
            "a pure turn about Y"
        );
        let pole = euler_xyz(from_euler_xyz(DVec3::new(0.3, FRAC_PI_2, 0.0)));
        assert!(
            (pole.y - FRAC_PI_2).abs() < 1e-12 && pole.z.abs() < 1e-12,
            "at the pole Z is taken as none"
        );
    }

    #[test]
    fn approximate_equality_is_relative_and_zero_is_absolute() {
        assert!(equal_approx(
            DVec3::new(1000.0, 0.0, 0.0),
            DVec3::new(1000.005, 0.0, 0.0)
        ));
        assert!(!equal_approx(
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(1.0001, 0.0, 0.0)
        ));
        assert!(equal_approx(DVec3::INFINITY, DVec3::INFINITY));
        assert!(zero_approx(DVec3::splat(9e-6)) && !zero_approx(DVec3::new(0.0, 2e-5, 0.0)));
    }

    #[test]
    fn the_nearest_point_of_a_segment_is_clamped_to_its_ends() {
        let (start, end) = (DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0));
        assert!(near(
            closest_on_segment(DVec3::new(0.5, 3.0, 0.0), start, end),
            DVec3::new(0.5, 0.0, 0.0)
        ));
        assert!(near(
            closest_on_segment(DVec3::new(-1.0, 1.0, 0.0), start, end),
            start
        ));
        assert!(near(
            closest_on_segment(DVec3::new(5.0, 0.0, 1.0), start, end),
            end
        ));
        assert!(
            near(closest_on_segment(DVec3::ONE, start, start), start),
            "a segment of no length is its start"
        );
    }

    #[test]
    fn the_largest_axis_is_the_earlier_of_any_that_tie() {
        assert_eq!(max_axis(DVec3::new(1.0, 3.0, 2.0)), 1);
        assert_eq!(max_axis(DVec3::new(2.0, 2.0, 1.0)), 0);
        assert_eq!(max_axis(DVec3::new(1.0, 2.0, 2.0)), 1);
        assert_eq!(max_axis(DVec3::new(2.0, 1.0, 2.0)), 0);
        assert_eq!(max_axis(DVec3::new(0.0, 1.0, 5.0)), 2);
        assert_eq!(
            [sign_of(-2.0), sign_of(0.0), sign_of(-0.0), sign_of(3.0)],
            [-1.0, 0.0, 0.0, 1.0]
        );
        assert!((smoothstep(0.5) - 0.5).abs() < 1e-12);
        assert!(smoothstep(-1.0).abs() < 1e-12 && (smoothstep(2.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn bounds_grow_test_points_and_hold_a_turned_box() {
        let unit = Bounds {
            position: DVec3::splat(-0.5),
            size: DVec3::ONE,
        };
        assert!(unit.has_point(DVec3::splat(0.5)) && !unit.has_point(DVec3::new(0.6, 0.0, 0.0)));
        assert!(unit.grow(0.2).has_point(DVec3::new(0.6, 0.0, 0.0)));
        let beside = Bounds {
            position: DVec3::new(0.5, -0.5, -0.5),
            size: DVec3::ONE,
        };
        assert!(
            !unit.intersects(beside),
            "boxes that share a face do not overlap"
        );
        assert!(unit.intersects(beside.grow(0.01)));
        let merged = unit.merge(beside);
        assert!(
            near(merged.position, DVec3::splat(-0.5))
                && near(merged.size, DVec3::new(2.0, 1.0, 1.0))
        );
        let turned = unit.placed(DAffine3::from_rotation_translation(
            DQuat::from_rotation_z(PI / 4.0),
            DVec3::X,
        ));
        let half_diagonal = 0.5 * 2.0_f64.sqrt();
        assert!(near(
            turned.position,
            DVec3::new(1.0 - half_diagonal, -half_diagonal, -0.5)
        ));
        assert!(near(Bounds::around(DVec3::ONE, 0.5).size, DVec3::ONE));
    }

    #[test]
    fn a_grown_box_moves_each_face_out_and_merged_boxes_are_the_least_holding_both() {
        let one = Bounds {
            position: DVec3::new(1.0, 2.0, 3.0),
            size: DVec3::new(2.0, 4.0, 6.0),
        };
        let grown = one.grow(0.25);
        assert!(near(grown.position, DVec3::new(0.75, 1.75, 2.75)));
        assert!(near(grown.size, DVec3::new(2.5, 4.5, 6.5)));
        let other = Bounds {
            position: DVec3::new(-1.0, 3.0, 4.0),
            size: DVec3::ONE,
        };
        let merged = one.merge(other);
        assert!(near(merged.position, DVec3::new(-1.0, 2.0, 3.0)));
        assert!(near(
            merged.position + merged.size,
            DVec3::new(3.0, 6.0, 9.0)
        ));
    }

    #[test]
    fn orthonormalising_removes_what_each_column_shares_with_the_earlier_ones() {
        let sheared = DMat3::from_cols(
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(1.0, 3.0, 0.0),
            DVec3::new(1.0, 2.0, 4.0),
        );
        let square = orthonormalized(sheared);
        assert!(near(square.x_axis, DVec3::X));
        assert!(near(square.y_axis, DVec3::Y));
        assert!(near(square.z_axis, DVec3::Z));
    }

    /// A half turn about each axis leading in turn, whose matrix's trace is −1, reads back as a turn that keeps the
    /// axis and reverses whatever is square to it.
    #[test]
    fn a_half_turn_about_a_slanted_axis_reads_back_whichever_axis_leads() {
        for axis in [
            DVec3::new(1.0, 2.0, 3.0),
            DVec3::new(3.0, 2.0, 1.0),
            DVec3::new(2.0, 3.0, 1.0),
        ] {
            let axis = axis.normalize();
            let matrix = DMat3::from_cols(axis * axis.x, axis * axis.y, axis * axis.z) * 2.0
                - DMat3::IDENTITY;
            let turn = rotation_of(matrix);
            let across = axis.any_orthonormal_vector();
            assert!(near(turn * axis, axis), "{axis}");
            assert!(near(turn * across, -across), "{axis}");
            assert!(
                near(turn * axis.cross(across), -axis.cross(across)),
                "{axis}"
            );
        }
    }

    #[test]
    fn slerp_to_a_negated_quaternion_takes_the_shorter_arc_at_an_even_rate() {
        let turn = DQuat::from_rotation_x(0.8);
        for weight in [0.0, 0.5, 1.0] {
            assert!(apart(slerp(turn, -turn, weight), turn) < 1e-9, "{weight}");
        }
        let quarter = slerp(
            DQuat::from_rotation_x(0.2),
            -DQuat::from_rotation_x(0.6),
            0.25,
        );
        assert!(apart(quarter, DQuat::from_rotation_x(0.3)) < 1e-9);
    }

    #[test]
    fn slerp_between_nearly_equal_turns_keeps_its_share() {
        let between = slerp(
            DQuat::from_rotation_y(0.2),
            DQuat::from_rotation_y(0.202),
            0.25,
        );
        assert!(apart(between, DQuat::from_rotation_y(0.2005)) < 1e-9);
    }

    #[test]
    fn a_quarter_turn_back_about_y_reads_as_minus_a_quarter() {
        let angles = euler_xyz(DMat3::from_rotation_y(-FRAC_PI_2));
        assert!(near(angles, DVec3::new(0.0, -FRAC_PI_2, 0.0)));
        assert!(near(from_euler_xyz(angles) * DVec3::Z, DVec3::NEG_X));
    }

    #[test]
    fn a_turn_about_x_or_z_alone_is_not_read_as_a_turn_about_y() {
        assert!(near(
            euler_xyz(DMat3::from_rotation_x(0.4)),
            DVec3::new(0.4, 0.0, 0.0)
        ));
        assert!(near(
            euler_xyz(DMat3::from_rotation_z(0.3)),
            DVec3::new(0.0, 0.0, 0.3)
        ));
    }

    #[test]
    fn values_a_whole_tolerance_apart_are_not_equal() {
        assert!(!equal_approx(DVec3::ZERO, DVec3::new(1e-5, 0.0, 0.0)));
        assert!(equal_approx(DVec3::ZERO, DVec3::new(0.9e-5, 0.0, 0.0)));
    }
}
