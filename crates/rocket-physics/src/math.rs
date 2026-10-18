//! 3D vector and quaternion math for 6-DOF dynamics.
//!
//! A small, self-contained math module for the flight dynamics code.
//! We avoid pulling in `nalgebra` for Phase 3 because the diagonal-inertia
//! case doesn't need a 3×3 solve; Phase 4 (full inertia tensor, asymmetric
//! configs) will introduce `nalgebra` when the cost is justified.
//!
//! All types are `#[derive(Copy)]` and pass-by-value — vectors and
//! quaternions are small (3 or 4 `f64`s) and cheap to copy.

use serde::{Deserialize, Serialize};

/// A 3D vector in `f64`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    /// The zero vector.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// The unit vector along +x.
    pub const UNIT_X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };

    /// The unit vector along +y.
    pub const UNIT_Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    /// The unit vector along +z.
    pub const UNIT_Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    /// Create a vector from components.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Create from a tuple `(x, y, z)`.
    #[must_use]
    pub const fn from_tuple(t: (f64, f64, f64)) -> Self {
        Self {
            x: t.0,
            y: t.1,
            z: t.2,
        }
    }

    /// Convert to a tuple.
    #[must_use]
    pub const fn to_tuple(self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }

    /// Squared magnitude.
    #[must_use]
    pub fn mag_sq(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Magnitude.
    #[must_use]
    pub fn mag(self) -> f64 {
        self.mag_sq().sqrt()
    }

    /// Normalize to a unit vector. Returns `ZERO` if the magnitude is zero.
    #[must_use]
    pub fn normalize(self) -> Self {
        let m = self.mag();
        if m < 1e-12 {
            Self::ZERO
        } else {
            self / m
        }
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Cross product `self × other`.
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    /// Component-wise multiplication (Hadamard product).
    #[must_use]
    pub fn component_mul(self, other: Self) -> Self {
        Self {
            x: self.x * other.x,
            y: self.y * other.y,
            z: self.z * other.z,
        }
    }

    /// Rotate by a rotation matrix (row-major 3x3).
    #[must_use]
    pub fn rotate(self, r: &Rotation3) -> Self {
        Self {
            x: r.r[0][0] * self.x + r.r[0][1] * self.y + r.r[0][2] * self.z,
            y: r.r[1][0] * self.x + r.r[1][1] * self.y + r.r[1][2] * self.z,
            z: r.r[2][0] * self.x + r.r[2][1] * self.y + r.r[2][2] * self.z,
        }
    }

    /// Rotate by the transpose of a rotation matrix (inverse rotation).
    #[must_use]
    pub fn rotate_transpose(self, r: &Rotation3) -> Self {
        Self {
            x: r.r[0][0] * self.x + r.r[1][0] * self.y + r.r[2][0] * self.z,
            y: r.r[0][1] * self.x + r.r[1][1] * self.y + r.r[2][1] * self.z,
            z: r.r[0][2] * self.x + r.r[1][2] * self.y + r.r[2][2] * self.z,
        }
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

impl std::ops::Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}

impl std::ops::Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

impl std::ops::Div<f64> for Vec3 {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        Self {
            x: self.x / s,
            y: self.y / s,
            z: self.z / s,
        }
    }
}

/// A 3x3 rotation matrix (row-major).
///
/// Stored as `[[r00, r01, r02], [r10, r11, r12], [r20, r21, r22]]`.
/// For a rotation matrix, the inverse is the transpose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation3 {
    pub r: [[f64; 3]; 3],
}

impl Rotation3 {
    /// The identity matrix.
    pub const IDENTITY: Self = Self {
        r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    /// Construct from row-major 3x3 array.
    #[must_use]
    pub const fn from_rows(r: [[f64; 3]; 3]) -> Self {
        Self { r }
    }

    /// Transpose (which is also the inverse for a rotation matrix).
    #[must_use]
    pub fn transpose(self) -> Self {
        Self {
            r: [
                [self.r[0][0], self.r[1][0], self.r[2][0]],
                [self.r[0][1], self.r[1][1], self.r[2][1]],
                [self.r[0][2], self.r[1][2], self.r[2][2]],
            ],
        }
    }

    /// Matrix-matrix multiplication.
    #[must_use]
    pub fn mat_mul(self, other: Self) -> Self {
        let mut result = [[0.0_f64; 3]; 3];
        for (i, row) in result.iter_mut().enumerate() {
            for (j, slot) in row.iter_mut().enumerate() {
                let mut sum = 0.0;
                for k in 0..3 {
                    sum += self.r[i][k] * other.r[k][j];
                }
                *slot = sum;
            }
        }
        Self { r: result }
    }
}

/// A unit quaternion `q = w + x·i + y·j + z·k` representing a rotation.
///
/// The quaternion maps body-frame vectors to launch-frame vectors:
/// `v_launch = R(q) · v_body`. The inverse rotation (launch → body) is
/// the conjugate `q*`.
///
/// Conventions:
/// - Hamilton product (not JPL).
/// - Right-handed rotations.
/// - `q = (cos(θ/2), sin(θ/2)·axis)` for a rotation by θ about `axis`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Quaternion {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Quaternion {
    /// The identity quaternion (no rotation).
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Create from components.
    #[must_use]
    pub const fn new(w: f64, x: f64, y: f64, z: f64) -> Self {
        Self { w, x, y, z }
    }

    /// Create from a rotation about an axis by an angle (radians).
    ///
    /// The axis need not be normalized; it's normalized internally.
    /// A zero axis returns the identity.
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle: f64) -> Self {
        let n = axis.mag();
        if n < 1e-12 {
            return Self::IDENTITY;
        }
        let (s, c) = (angle / 2.0).sin_cos();
        let s = s / n;
        Self {
            w: c,
            x: axis.x * s,
            y: axis.y * s,
            z: axis.z * s,
        }
    }

    /// Create from Euler angles (roll, pitch, yaw) in radians, using the
    /// ZYX intrinsic convention (yaw, then pitch, then roll).
    ///
    /// This matches the aerospace convention where:
    /// - roll (φ) is rotation about x
    /// - pitch (θ) is rotation about y
    /// - yaw (ψ) is rotation about z
    #[must_use]
    pub fn from_euler_zyx(roll: f64, pitch: f64, yaw: f64) -> Self {
        let (sr, cr) = (roll / 2.0).sin_cos();
        let (sp, cp) = (pitch / 2.0).sin_cos();
        let (sy, cy) = (yaw / 2.0).sin_cos();
        Self {
            w: cr * cp * cy + sr * sp * sy,
            x: sr * cp * cy - cr * sp * sy,
            y: cr * sp * cy + sr * cp * sy,
            z: cr * cp * sy - sr * sp * cy,
        }
    }

    /// Squared norm.
    #[must_use]
    pub fn norm_sq(self) -> f64 {
        self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Norm.
    #[must_use]
    pub fn norm(self) -> f64 {
        self.norm_sq().sqrt()
    }

    /// Normalize to a unit quaternion. Returns identity if near-zero.
    #[must_use]
    pub fn normalize(self) -> Self {
        let n = self.norm();
        if n < 1e-12 {
            Self::IDENTITY
        } else {
            Self {
                w: self.w / n,
                x: self.x / n,
                y: self.y / n,
                z: self.z / n,
            }
        }
    }

    /// Conjugate (inverse for a unit quaternion).
    #[must_use]
    pub fn conjugate(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    /// Hamilton product `self ⊗ other`.
    ///
    /// The product `q1 ⊗ q2` represents the rotation `q2` followed by
    /// `q1` (i.e., `R(q1 ⊗ q2) = R(q1) · R(q2)`).
    #[must_use]
    pub fn hamilton(self, other: Self) -> Self {
        Self {
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
        }
    }

    /// Convert to a 3x3 rotation matrix.
    ///
    /// The resulting matrix `R` satisfies `v_launch = R · v_body`.
    #[must_use]
    pub fn to_rotation_matrix(self) -> Rotation3 {
        let Self { w, x, y, z } = self.normalize();
        let xx = x * x;
        let yy = y * y;
        let zz = z * z;
        let xy = x * y;
        let xz = x * z;
        let yz = y * z;
        let wx = w * x;
        let wy = w * y;
        let wz = w * z;
        Rotation3 {
            r: [
                [1.0 - 2.0 * (yy + zz), 2.0 * (xy - wz), 2.0 * (xz + wy)],
                [2.0 * (xy + wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz - wx)],
                [2.0 * (xz - wy), 2.0 * (yz + wx), 1.0 - 2.0 * (xx + yy)],
            ],
        }
    }

    /// Rotate a vector from body frame to launch frame.
    #[must_use]
    pub fn rotate_to_launch(self, v: Vec3) -> Vec3 {
        v.rotate(&self.to_rotation_matrix())
    }

    /// Rotate a vector from launch frame to body frame.
    #[must_use]
    pub fn rotate_to_body(self, v: Vec3) -> Vec3 {
        v.rotate_transpose(&self.to_rotation_matrix())
    }

    /// The exponential map: `exp(½·ω·t)` for a pure-rotation quaternion.
    ///
    /// Given an angular velocity `ω` (in body frame) and a time `t`,
    /// returns the quaternion representing the rotation accumulated over
    /// `t`. This is the closed-form solution to `q̇ = ½·q⊗ω`.
    #[must_use]
    pub fn exp(omega: Vec3, t: f64) -> Self {
        let half_omega_t = omega * (t / 2.0);
        let angle = half_omega_t.mag();
        let s = angle.sin();
        let c = angle.cos();
        if angle < 1e-12 {
            return Self::IDENTITY;
        }
        let axis = half_omega_t / angle;
        Self {
            w: c,
            x: axis.x * s,
            y: axis.y * s,
            z: axis.z * s,
        }
    }
}

impl std::ops::Sub for Quaternion {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self {
            w: self.w - other.w,
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

impl std::ops::Mul<f64> for Quaternion {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self {
            w: self.w * s,
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

/// Compute the angle of attack `α` and sideslip `β` from the relative
/// velocity in the body frame.
///
/// Per spec §14.3:
/// ```text
/// α = atan2(−v_rel.z,  v_rel.x)    // pitch AoA
/// β = asin (−v_rel.y / |v_rel|)    // sideslip
/// ```
///
/// The body x-axis points along the rocket's nose; z is "down" in the
/// pitch plane (positive z = nose below velocity vector = positive α).
///
/// Returns `(alpha, beta)` in radians. Returns `(0, 0)` for a zero
/// velocity.
#[must_use]
pub fn angle_of_attack_sideslip(v_rel_body: Vec3) -> (f64, f64) {
    let v_mag = v_rel_body.mag();
    if v_mag < 1e-9 {
        return (0.0, 0.0);
    }
    let alpha = (-v_rel_body.z).atan2(v_rel_body.x);
    let sin_beta = -v_rel_body.y / v_mag;
    let beta = sin_beta.clamp(-1.0, 1.0).asin();
    (alpha, beta)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]

    use super::*;

    const TOL: f64 = 1e-9;

    #[test]
    fn vec3_basic_operations() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(a - b, Vec3::new(-3.0, -3.0, -3.0));
        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(-a, Vec3::new(-1.0, -2.0, -3.0));
    }

    #[test]
    fn vec3_dot_and_cross() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        assert!((a.dot(b) - 0.0).abs() < TOL);
        assert_eq!(a.cross(b), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(b.cross(a), Vec3::new(0.0, 0.0, -1.0));
        // Right-hand rule: x × y = z, y × z = x, z × x = y.
        assert_eq!(Vec3::UNIT_X.cross(Vec3::UNIT_Y), Vec3::UNIT_Z);
        assert_eq!(Vec3::UNIT_Y.cross(Vec3::UNIT_Z), Vec3::UNIT_X);
        assert_eq!(Vec3::UNIT_Z.cross(Vec3::UNIT_X), Vec3::UNIT_Y);
    }

    #[test]
    fn vec3_magnitude_and_normalize() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert!((v.mag() - 5.0).abs() < TOL);
        let n = v.normalize();
        assert!((n.mag() - 1.0).abs() < TOL);
        assert!((n.x - 0.6).abs() < TOL);
        assert!((n.y - 0.8).abs() < TOL);
        // Zero vector normalizes to zero.
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
    }

    #[test]
    fn quaternion_identity_is_unit() {
        let q = Quaternion::IDENTITY;
        assert!((q.norm() - 1.0).abs() < TOL);
    }

    #[test]
    fn quaternion_from_axis_angle_is_unit() {
        let q = Quaternion::from_axis_angle(Vec3::UNIT_Z, std::f64::consts::FRAC_PI_2);
        assert!((q.norm() - 1.0).abs() < TOL, "q.norm() = {}", q.norm());
        // Rotating (1,0,0) by 90° about z gives (0,1,0).
        let v = q.rotate_to_launch(Vec3::UNIT_X);
        assert!((v - Vec3::UNIT_Y).mag() < TOL, "got {v:?}");
    }

    #[test]
    fn quaternion_round_trip_to_rotation_matrix() {
        let q = Quaternion::from_axis_angle(Vec3::new(1.0, 1.0, 1.0).normalize(), 0.7);
        let r = q.to_rotation_matrix();
        // R · R^T = I.
        let rr_t = r.mat_mul(r.transpose());
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (rr_t.r[i][j] - expected).abs() < TOL,
                    "R·R^T[{i}][{j}] = {}, expected {expected}",
                    rr_t.r[i][j]
                );
            }
        }
    }

    #[test]
    fn quaternion_multiplication_identity() {
        let q = Quaternion::from_axis_angle(Vec3::UNIT_X, 0.3);
        assert_eq!(q.hamilton(Quaternion::IDENTITY), q);
        assert_eq!(Quaternion::IDENTITY.hamilton(q), q);
    }

    #[test]
    fn quaternion_multiplication_composes_rotations() {
        // Rotate 90° about z, then 90° about x. The result should be a
        // 120° rotation about (1,1,1)/√3 (composition of two 90° face
        // rotations of a cube).
        let q_z = Quaternion::from_axis_angle(Vec3::UNIT_Z, std::f64::consts::FRAC_PI_2);
        let q_x = Quaternion::from_axis_angle(Vec3::UNIT_X, std::f64::consts::FRAC_PI_2);
        let q_combined = q_x.hamilton(q_z);
        // Apply to (1,0,0): first q_z gives (0,1,0), then q_x gives (0,0,1).
        let v = q_combined.rotate_to_launch(Vec3::UNIT_X);
        assert!(
            (v - Vec3::UNIT_Z).mag() < TOL,
            "expected (0,0,1), got {v:?}"
        );
    }

    #[test]
    fn quaternion_conjugate_is_inverse() {
        let q = Quaternion::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 1.2);
        let q_inv = q.conjugate();
        let product = q.hamilton(q_inv);
        assert!(
            (product - Quaternion::IDENTITY).norm() < TOL,
            "q ⊗ q* should be identity, got {product:?}"
        );
    }

    #[test]
    fn quaternion_normalize() {
        let q = Quaternion::new(2.0, 0.0, 0.0, 0.0);
        let n = q.normalize();
        assert!((n.norm() - 1.0).abs() < TOL);
        assert!((n.w - 1.0).abs() < TOL);
    }

    #[test]
    fn steady_spin_closed_form() {
        // A body spinning at constant ω with zero torque:
        // q(t) = q(0) ⊗ exp(½·ω·t).
        // The closed-form solution is q(0) ⊗ Quaternion::exp(ω, t).
        let omega = Vec3::new(0.0, 0.0, 1.0); // 1 rad/s about z.
        let q0 = Quaternion::from_axis_angle(Vec3::UNIT_X, 0.5);
        let t = 1.0;

        // Closed form:
        let q_exp = Quaternion::exp(omega, t);
        let q_closed = q0.hamilton(q_exp);

        // Numerical: integrate q̇ = ½·q⊗ω with small steps.
        let dt = 1e-6;
        let mut q = q0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n_steps = (t / dt).round() as usize;
        for _ in 0..n_steps {
            // q̇ = ½·q⊗ω_quat where ω_quat = (0, ωx, ωy, ωz).
            let omega_quat = Quaternion::new(0.0, omega.x, omega.y, omega.z);
            let q_dot = q.hamilton(omega_quat) * 0.5;
            q = Quaternion {
                w: q.w + q_dot.w * dt,
                x: q.x + q_dot.x * dt,
                y: q.y + q_dot.y * dt,
                z: q.z + q_dot.z * dt,
            }
            .normalize();
        }

        // Compare q (numerical) to q_closed.
        // Quaternions q and -q represent the same rotation; compare rotation
        // matrices to avoid the double-cover ambiguity.
        let r_num = q.to_rotation_matrix();
        let r_closed = q_closed.to_rotation_matrix();
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (r_num.r[i][j] - r_closed.r[i][j]).abs() < 1e-5,
                    "R[{i}][{j}]: numerical={}, closed={}",
                    r_num.r[i][j],
                    r_closed.r[i][j]
                );
            }
        }
    }

    #[test]
    fn angle_of_attack_pure_pitch() {
        // Body x-axis aligned with velocity → α=0.
        let v = Vec3::new(100.0, 0.0, 0.0);
        let (alpha, beta) = angle_of_attack_sideslip(v);
        assert!(alpha.abs() < TOL, "α should be 0, got {alpha}");
        assert!(beta.abs() < TOL, "β should be 0, got {beta}");

        // Body pitched up 10° relative to velocity: v_body has -z component.
        let alpha_in = 10.0_f64.to_radians();
        let v = Vec3::new(100.0 * alpha_in.cos(), 0.0, -100.0 * alpha_in.sin());
        let (alpha, beta) = angle_of_attack_sideslip(v);
        assert!(
            (alpha - alpha_in).abs() < TOL,
            "α should be {alpha_in}, got {alpha}"
        );
        assert!(beta.abs() < TOL);
    }

    #[test]
    fn sideslip_pure_yaw() {
        // Body yawed 10° to the right: v_body has -y component.
        let beta_in = 10.0_f64.to_radians();
        let v = Vec3::new(100.0 * beta_in.cos(), -100.0 * beta_in.sin(), 0.0);
        let (alpha, beta) = angle_of_attack_sideslip(v);
        assert!(alpha.abs() < TOL);
        assert!(
            (beta - beta_in).abs() < TOL,
            "β should be {beta_in}, got {beta}"
        );
    }

    #[test]
    fn rotation3_transpose_is_inverse() {
        let q = Quaternion::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 0.9);
        let r = q.to_rotation_matrix();
        let r_t = r.transpose();
        let product = r.mat_mul(r_t);
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((product.r[i][j] - expected).abs() < TOL);
            }
        }
    }

    #[test]
    fn vec3_rotate_round_trip() {
        // Rotate a vector to launch frame and back should recover it.
        let q = Quaternion::from_axis_angle(Vec3::new(1.0, 1.0, 0.0).normalize(), 1.3);
        let v = Vec3::new(1.0, 2.0, 3.0);
        let v_launch = q.rotate_to_launch(v);
        let v_back = q.rotate_to_body(v_launch);
        assert!(
            (v - v_back).mag() < TOL,
            "round-trip should recover v: got {v_back:?}, expected {v:?}"
        );
    }

    #[test]
    fn from_euler_zyx_zero_is_identity() {
        let q = Quaternion::from_euler_zyx(0.0, 0.0, 0.0);
        assert!((q - Quaternion::IDENTITY).norm() < TOL);
    }

    #[test]
    fn from_euler_zyx_yaw_90() {
        // Yaw 90° about z should rotate (1,0,0) to (0,1,0).
        let q = Quaternion::from_euler_zyx(0.0, 0.0, std::f64::consts::FRAC_PI_2);
        let v = q.rotate_to_launch(Vec3::UNIT_X);
        assert!((v - Vec3::UNIT_Y).mag() < TOL, "got {v:?}");
    }

    #[test]
    fn quaternion_exp_zero_omega_is_identity() {
        let q = Quaternion::exp(Vec3::ZERO, 1.0);
        assert!((q - Quaternion::IDENTITY).norm() < TOL);
    }
}
