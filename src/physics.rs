//! CPU mirror of the shader physics, for unit-testing.
//! Natural units: Rs = 1.

use bevy::math::{Vec3, Vec4};

pub const RS: f32 = 1.0;
/// Critical impact parameter for a Schwarzschild hole: bcrit = (3*sqrt(3)/2) * Rs.
/// Written as a literal because `f32::sqrt` is not `const` on stable Rust.
/// (3 * sqrt(3) / 2 = 2.598076211353316...)
pub const BCRIT: f32 = 2.5980762;

/// One Euler step of the discretized geodesic (mirrors the shader's `deriv`).
pub fn bending_accel(pos: Vec3, dir: Vec3) -> Vec3 {
    let r = pos.length();
    let h = pos.cross(dir);
    let h2 = h.dot(h);
    let r5 = (r * r * r * r * r).max(1e-6);
    -1.5 * RS * h2 / r5 * pos
}

/// Classify a ray by integrating it. Returns true if captured (r < Rs).
/// Uses RK4 like the shader. `dt` is the step size, `steps` the count.
pub fn is_captured(start_pos: Vec3, start_dir: Vec3, steps: u32, dt: f32) -> bool {
    let mut pos = start_pos;
    let mut dir = start_dir;
    const R_ESCAPE: f32 = 1000.0;
    for _ in 0..steps {
        let r = pos.length();
        if r < RS {
            return true;
        }
        if r > R_ESCAPE {
            return false;
        }
        let k1p = dir;
        let k1d = bending_accel(pos, dir);
        let k2p = dir + k1d * dt * 0.5;
        let k2d = bending_accel(pos + k1p * dt * 0.5, (dir + k1d * dt * 0.5).normalize());
        let k3p = dir + k2d * dt * 0.5;
        let k3d = bending_accel(pos + k2p * dt * 0.5, (dir + k2d * dt * 0.5).normalize());
        let k4p = dir + k3d * dt;
        let k4d = bending_accel(pos + k3p * dt, (dir + k3d * dt).normalize());
        pos += (k1p + 2.0 * k2p + 2.0 * k3p + k4p) * dt / 6.0;
        dir = (dir + (k1d + 2.0 * k2d + 2.0 * k3d + k4d) * dt / 6.0).normalize();
    }
    false
}

/// Compute the impact parameter of a ray given eye position and direction:
/// b = |eye x dir| / |dir|  (since dir is unit, b = |eye x dir|).
pub fn impact_parameter(eye: Vec3, dir: Vec3) -> f32 {
    eye.cross(dir).length()
}

/// Prograde Kerr ISCO in Rs units (Rs=1, so M=0.5). `chi = a/M ∈ [0,1]`.
/// Bardeen-Press-Teukolsky (1972) closed form. Returns 6M=3.0 at chi=0,
/// M=0.5 at chi=1.
pub fn kerr_isco(chi: f32) -> f32 {
    let m = 0.5;
    let cbrt_pos = (1.0 + chi).cbrt();
    let cbrt_neg = (1.0 - chi).cbrt();
    let z1 = 1.0 + (1.0 - chi * chi).cbrt() * (cbrt_pos + cbrt_neg);
    let z2 = (3.0 * chi * chi + z1 * z1).sqrt();
    m * (3.0 + z2 - ((3.0 - z1) * (3.0 + z1 + 2.0 * z2)).sqrt())
}

/// Kerr event-horizon radius r+ in Rs units (Rs=1, M=0.5). `chi = a/M ∈ [0,1]`.
/// Returns Rs=1.0 at chi=0, M=0.5 at chi=1.
pub fn kerr_horizon(chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    m + (m * m - a * a).max(0.0).sqrt()
}

// Silence unused-import warning for Vec4 if not used; kept for future expansion.
#[allow(dead_code)]
fn _phantom(_v: Vec4) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcrit_value() {
        assert!((BCRIT - 2.598).abs() < 0.01, "bcrit should be ~2.598, got {}", BCRIT);
    }

    #[test]
    fn ray_below_bcrit_is_captured() {
        // Eye far on the z-axis; aim slightly off-center with b < bcrit.
        let eye = Vec3::new(0.0, 0.0, 50.0);
        let dir = Vec3::new(0.0, 2.0, -50.0).normalize(); // b ~ 2.0 < 2.598
        let b = impact_parameter(eye, dir);
        assert!(b < BCRIT, "b {} should be < bcrit {}", b, BCRIT);
        assert!(is_captured(eye, dir, 2000, 0.1), "ray below bcrit should be captured");
    }

    #[test]
    fn ray_above_bcrit_escapes() {
        let eye = Vec3::new(0.0, 0.0, 50.0);
        let dir = Vec3::new(0.0, 10.0, -50.0).normalize(); // b ~ 9.8 >> bcrit
        let b = impact_parameter(eye, dir);
        assert!(b > BCRIT);
        assert!(!is_captured(eye, dir, 2000, 0.1), "ray above bcrit should escape");
    }
}
