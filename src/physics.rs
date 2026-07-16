//! CPU mirror of the shader physics, for unit-testing.
//! Natural units: Rs = 1.
//
// These functions exist to be exercised by the integration test crate
// (`tests/physics_test.rs`), not by the binary. The binary pulls this module
// in only for `kerr_isco` (UI) and `kerr_horizon` (UI); the rest are test-only
// mirrors of shader math. Silence the resulting dead-code noise at the module
// level rather than per-function.
#![allow(dead_code)]

use bevy::math::{Vec3, Vec4};

pub const RS: f32 = 1.0;
/// Critical impact parameter for a Schwarzschild hole: bcrit = (3*sqrt(3)/2) * Rs.
/// Written as a literal because `f32::sqrt` is not `const` on stable Rust.
/// (3 * sqrt(3) / 2 = 2.598076211353316...)
pub const BCRIT: f32 = 2.598_076;

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

/// Kerr 赤道 prograde 圆轨角速度 (Rs=1, M=0.5). Bardeen 1972 eqn 2.16.
/// `Ω_φ = 1 / (r^1.5 + a)`, a = χM = 0.5χ. χ=0 退化为牛顿 1/r^1.5.
pub fn kerr_orbital_frequency(r: f32, chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    1.0 / (r.powf(1.5) + a)
}

/// Kerr 赤道圆轨节点进动率 (Lense-Thirring, 强场精确). χ=0 返回 0.
///
/// `Ω_LT = Ω_φ - Ω_θ`, 其中 Ω_θ 是垂直 epicyclic 频率 (Okazaki 1987;
/// Kato/Fukue/Mineshige "Black-Hole Accretion Disks"):
/// `Ω_θ² = Ω_φ² · (1 − 4a√M/r^1.5 + 3a²/r²)`.
/// χ=0 时 a=0, 括号=1, 故 Ω_θ=Ω_φ, 进动为零 (Schwarzschild 球对称).
///
/// 注: 交叉项是 `a√M/r^1.5` (半径 -1.5 次幂), 不是 `a·Ω_φ/r`. 后者会让
/// Ω_θ 偏大、进动偏小, 且破坏"进动随 χ 单调增"的物理性质.
pub fn kerr_nodal_precession(r: f32, chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    let omega_phi = kerr_orbital_frequency(r, chi);
    // 垂直 epicyclic 频率比 (>=0; 极端 r/a 组合下数值精度可能略负, 钳位)
    let sqrt_m = m.sqrt();
    let ratio = (1.0 - 4.0 * a * sqrt_m / r.powf(1.5) + 3.0 * a * a / (r * r)).max(0.0);
    let omega_theta = omega_phi * ratio.sqrt();
    omega_phi - omega_theta
}

/// Kerr bending acceleration (CPU mirror of the shader `deriv` accel).
/// `chi = a/M ∈ [0,1]`. At chi=0 this equals `bending_accel`.
pub fn kerr_bending_accel(pos: Vec3, dir: Vec3, chi: f32) -> Vec3 {
    let r = pos.length();
    let m = 0.5;
    let a = chi * m;
    let h = pos.cross(dir);
    let h2 = h.dot(h);
    let r5 = (r * r * r * r * r).max(1e-6);
    let radial = -1.5 * RS * h2 / r5 * pos;
    let spin_axis = Vec3::Y;
    let r3 = (r * r * r).max(1e-6);
    let drag = 2.0 * m * a / r3 * spin_axis.cross(dir);
    radial + drag
}

// ---- RK45 (Dormand-Prince) adaptive integrator ----
// CPU mirror of the shader's `rk45_step` (black_hole.wgsl:260) and adaptive
// loop (black_hole.wgsl:320-390). Keep these in lockstep with the shader: the
// Butcher-tableau coefficients, the dt_min forced-accept floor, and the
// budget = accepted-steps-only semantics must all match, or the loop-level
// capture/escape tests below pass on code the shader contradicts.

/// Result of one Dormand-Prince step: the 5th-order solution advanced by `dt`,
/// plus the position error estimate `|y5 - y4|` used for step-size control.
/// Mirrors the shader's `RkStep` struct (black_hole.wgsl:254).
pub struct RkStep {
    pub pos: Vec3,
    pub dir: Vec3,
    pub err: f32,
}

/// One Dormand-Prince RK45 step against the Kerr geodesic. `chi = a/M ∈ [0,1]`.
/// At chi=0 the derivative reduces to `bending_accel`, so this is also the
/// Phase 1 integrator step. Mirrors `rk45_step` in black_hole.wgsl:260-285.
///
/// **Implemented order note:** the per-stage `normalize(dir + …)` projection
/// (faithful to the shader) makes the *realized* error estimate shrink between
/// 2nd and 4th order in dt depending on geometry, not a clean 5th order — see
/// the `rk45_step_error_shrinks_monotonically_with_dt` test. The adaptive loop
/// only depends on the error being monotone in dt, which holds. Naming follows
/// the shader ("RK45") for the tableau, not as a precision guarantee.
pub fn rk45_step(pos: Vec3, dir: Vec3, dt: f32, chi: f32) -> RkStep {
    // k_i = deriv(p_i, d_i); dpos = dir, ddir = kerr_bending_accel.
    let k1p = dir;
    let k1d = kerr_bending_accel(pos, dir, chi);

    let p2 = pos + k1p * (dt * 0.2);
    let d2 = (dir + k1d * (dt * 0.2)).normalize();
    let k2p = d2;
    let k2d = kerr_bending_accel(p2, d2, chi);

    let p3 = pos + (k1p * 0.075 + k2p * 0.225) * dt;
    let d3 = (dir + (k1d * 0.075 + k2d * 0.225) * dt).normalize();
    let k3p = d3;
    let k3d = kerr_bending_accel(p3, d3, chi);

    let p4 = pos + (k1p * 0.3 + k2p * -0.9 + k3p * 1.2) * dt;
    let d4 = (dir + (k1d * 0.3 + k2d * -0.9 + k3d * 1.2) * dt).normalize();
    let k4p = d4;
    let k4d = kerr_bending_accel(p4, d4, chi);

    let p5 = pos + (k1p * -11.0 / 54.0 + k2p * 2.5 + k3p * -70.0 / 27.0 + k4p * 35.0 / 27.0) * dt;
    let d5 = (dir + (k1d * -11.0 / 54.0 + k2d * 2.5 + k3d * -70.0 / 27.0 + k4d * 35.0 / 27.0) * dt)
        .normalize();
    let k5p = d5;
    let k5d = kerr_bending_accel(p5, d5, chi);

    let p6 = pos
        + (k1p * 1631.0 / 55296.0
            + k2p * 175.0 / 512.0
            + k3p * 575.0 / 13824.0
            + k4p * 44275.0 / 110592.0
            + k5p * 253.0 / 4096.0)
            * dt;
    let d6 = (dir
        + (k1d * 1631.0 / 55296.0
            + k2d * 175.0 / 512.0
            + k3d * 575.0 / 13824.0
            + k4d * 44275.0 / 110592.0
            + k5d * 253.0 / 4096.0)
            * dt)
        .normalize();
    let k6p = d6;
    let _k6d = kerr_bending_accel(p6, d6, chi); // 6th stage eval (6th-order weights k1..k5 only)

    // 5th-order solution (advances the state).
    let new_pos = pos
        + (k1p * 37.0 / 378.0
            + k3p * 250.0 / 621.0
            + k4p * 125.0 / 594.0
            + k5p * 512.0 / 1771.0)
            * dt;
    let new_dir = (dir
        + (k1d * 37.0 / 378.0
            + k3d * 250.0 / 621.0
            + k4d * 125.0 / 594.0
            + k5d * 512.0 / 1771.0)
            * dt)
        .normalize();
    // 4th-order solution (for the error estimate only).
    let pos4 = pos
        + (k1p * 2825.0 / 27648.0
            + k3p * 18575.0 / 48384.0
            + k4p * 13525.0 / 55296.0
            + k5p * 277.0 / 14336.0
            + k6p * 0.25)
            * dt;
    let err = (new_pos - pos4).length();
    RkStep {
        pos: new_pos,
        dir: new_dir,
        err,
    }
}

/// Classify a Kerr geodesic with the adaptive RK45 loop. Returns true if the
/// ray is captured (crosses r < r+(chi)). Mirrors the shader's integration
/// loop (black_hole.wgsl:320-390): budget = accepted steps, rejected steps
/// retry at smaller dt (down to dt_min, which is a forced-accept floor), and
/// the capture radius is the spin-dependent horizon r+(chi) (= Rs at chi=0).
///
/// `steps` is the hard cap on *accepted* steps (matches `uniforms.steps`).
pub fn is_captured_rk45(start_pos: Vec3, start_dir: Vec3, steps: u32, chi: f32) -> bool {
    let mut pos = start_pos;
    let mut dir = start_dir;

    // Same seeding as the shader: total_path from eye distance + escape radius.
    let eye_dist = pos.length();
    let escape_r = (eye_dist * 2.0).max(100.0);
    let total_path = eye_dist + escape_r;
    let dt_init = total_path / steps as f32;
    let dt_min = dt_init * 0.25;
    let dt_max = dt_init * 4.0;
    let tol = 1e-3;
    let r_plus = kerr_horizon(chi);

    let mut dt = dt_init;
    let mut budget = steps;

    while budget > 0 {
        let step = rk45_step(pos, dir, dt, chi);
        let err = step.err;

        if err > tol * 10.0 && dt > dt_min {
            // Reject: shrink to dt_min and retry (does not consume budget).
            dt = dt_min;
            continue;
        }
        // Accept: consume one budget unit.
        budget -= 1;
        if err <= tol * 10.0 {
            dt = (dt * (tol / err.max(1e-12)).powf(0.2)).clamp(dt_min, dt_max);
        }

        let new_pos = step.pos;
        let r = new_pos.length();
        if r < r_plus {
            return true;
        }
        if r > escape_r {
            return false;
        }
        pos = new_pos;
        dir = step.dir;
    }
    false
}

// Silence unused-import warning for Vec4 (legacy placeholder; retained).
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

    #[test]
    fn orbital_frequency_reduces_to_newton_at_zero_spin() {
        // χ=0: Ω = 1/r^1.5 (牛顿开普勒, Rs=1)
        for r in [4.0_f32, 6.0, 10.0, 20.0] {
            let newton = 1.0 / r.powf(1.5);
            let kerr = kerr_orbital_frequency(r, 0.0);
            assert!(
                (kerr - newton).abs() < 1e-6,
                "χ=0 at r={}: expected {} (newton), got {}",
                r, newton, kerr
            );
        }
    }

    #[test]
    fn orbital_frequency_decreases_with_spin_at_fixed_r() {
        // prograde 轨道 (a>0): Ω_φ 随 χ 减小 (分母 r^1.5+a 增大)
        let r = 8.0;
        let omega_0 = kerr_orbital_frequency(r, 0.0);
        let omega_1 = kerr_orbital_frequency(r, 1.0);
        assert!(omega_1 < omega_0, "prograde Ω should decrease with spin");
    }

    #[test]
    fn nodal_precession_vanishes_at_zero_spin() {
        // χ=0: 球对称 (Schwarzschild), 无节点进动
        for r in [4.0_f32, 6.0, 10.0, 20.0] {
            let prec = kerr_nodal_precession(r, 0.0);
            assert!(
                prec.abs() < 1e-6,
                "χ=0 at r={} should have zero precession, got {}",
                r, prec
            );
        }
    }

    #[test]
    fn nodal_precession_grows_with_spin() {
        // 固定 r, prograde 节点进动率随 χ 单调增
        let r = 6.0;
        let p_low = kerr_nodal_precession(r, 0.3);
        let p_high = kerr_nodal_precession(r, 0.9);
        assert!(p_high > p_low, "precession should grow with spin");
        assert!(p_low > 0.0, "prograde precession should be positive");
    }

    #[test]
    fn nodal_precession_strong_field_exceeds_weak_field() {
        // r<6 强场区: 精确 Ω_LT > 弱场近似 2aM/r³.
        // a = χM = 0.5χ, M = 0.5 → 2aM/r³ = 2·(0.5χ)·0.5/r³ = 0.5χ/r³.
        let r = 4.0_f32;
        let chi = 0.9;
        let weak = 0.5 * chi / r.powi(3);
        let strong = kerr_nodal_precession(r, chi);
        assert!(
            strong > weak,
            "strong-field precession at r={} should exceed weak approx {} , got {}",
            r, weak, strong
        );
    }
}
