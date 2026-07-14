use singularity_rs::physics;

#[test]
fn public_bcrt_constant_is_correct() {
    // 3*sqrt(3)/2 ≈ 2.598076
    let expected = 1.5 * 3.0_f32.sqrt();
    assert!((physics::BCRIT - expected).abs() < 1e-5);
}

#[test]
fn kerr_isco_at_zero_is_schwarzschild() {
    // spin=0 → ISCO = 6M = 3 Rs (Rs=1).
    let isco = physics::kerr_isco(0.0);
    assert!((isco - 3.0).abs() < 1e-3, "spin=0 ISCO should be 3.0, got {}", isco);
}

#[test]
fn kerr_isco_at_extremal_is_half_rs() {
    // spin=1 → ISCO = M = Rs/2 = 0.5.
    let isco = physics::kerr_isco(1.0);
    assert!((isco - 0.5).abs() < 1e-3, "spin=1 ISCO should be 0.5, got {}", isco);
}

#[test]
fn kerr_isco_is_monotonically_decreasing() {
    let a = physics::kerr_isco(0.3);
    let b = physics::kerr_isco(0.6);
    let c = physics::kerr_isco(0.9);
    assert!(a > b, "0.3 > 0.6: {} vs {}", a, b);
    assert!(b > c, "0.6 > 0.9: {} vs {}", b, c);
}

#[test]
fn kerr_horizon_at_zero_is_rs() {
    // spin=0 → r+ = Rs = 1.0.
    let r = physics::kerr_horizon(0.0);
    assert!((r - 1.0).abs() < 1e-3, "spin=0 horizon should be 1.0, got {}", r);
}

#[test]
fn kerr_horizon_at_extremal_is_half_rs() {
    // spin=1 → r+ = M = 0.5.
    let r = physics::kerr_horizon(1.0);
    assert!((r - 0.5).abs() < 1e-3, "spin=1 horizon should be 0.5, got {}", r);
}

#[test]
fn kerr_horizon_is_monotonically_decreasing() {
    let a = physics::kerr_horizon(0.3);
    let b = physics::kerr_horizon(0.6);
    let c = physics::kerr_horizon(0.9);
    assert!(a > b, "0.3 > 0.6: {} vs {}", a, b);
    assert!(b > c, "0.6 > 0.9: {} vs {}", b, c);
}

#[test]
fn kerr_bending_accel_degenerates_to_schwarzschild_at_zero_spin() {
    // At χ=0 the Kerr bending accel must equal the Schwarzschild one.
    let pos = bevy::math::Vec3::new(3.0, 1.0, 4.0);
    let dir = bevy::math::Vec3::new(0.2, -0.1, -0.97).normalize();
    let schw = physics::bending_accel(pos, dir);
    let kerr = physics::kerr_bending_accel(pos, dir, 0.0);
    let diff = (schw - kerr).length();
    assert!(diff < 1e-6, "spin=0 Kerr should match Schwarzschild; diff = {}", diff);
}

#[test]
fn kerr_bending_accel_nonzero_off_axis_at_nonzero_spin() {
    // At χ>0 the drag term must produce a different accel (frame-dragging exists).
    let pos = bevy::math::Vec3::new(3.0, 1.0, 4.0);
    let dir = bevy::math::Vec3::new(0.2, -0.1, -0.97).normalize();
    let schw = physics::bending_accel(pos, dir);
    let kerr = physics::kerr_bending_accel(pos, dir, 0.8);
    let diff = (schw - kerr).length();
    assert!(diff > 1e-4, "spin=0.8 Kerr should differ from Schwarzschild; diff = {}", diff);
}
