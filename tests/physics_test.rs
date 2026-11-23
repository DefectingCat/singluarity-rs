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
    assert!(
        (isco - 3.0).abs() < 1e-3,
        "spin=0 ISCO should be 3.0, got {}",
        isco
    );
}

#[test]
fn kerr_isco_at_extremal_is_half_rs() {
    // spin=1 → ISCO = M = Rs/2 = 0.5.
    let isco = physics::kerr_isco(1.0);
    assert!(
        (isco - 0.5).abs() < 1e-3,
        "spin=1 ISCO should be 0.5, got {}",
        isco
    );
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
    assert!(
        (r - 1.0).abs() < 1e-3,
        "spin=0 horizon should be 1.0, got {}",
        r
    );
}

#[test]
fn kerr_horizon_at_extremal_is_half_rs() {
    // spin=1 → r+ = M = 0.5.
    let r = physics::kerr_horizon(1.0);
    assert!(
        (r - 0.5).abs() < 1e-3,
        "spin=1 horizon should be 0.5, got {}",
        r
    );
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
    assert!(
        diff < 1e-6,
        "spin=0 Kerr should match Schwarzschild; diff = {}",
        diff
    );
}

#[test]
fn kerr_bending_accel_nonzero_off_axis_at_nonzero_spin() {
    // At χ>0 the drag term must produce a different accel (frame-dragging exists).
    let pos = bevy::math::Vec3::new(3.0, 1.0, 4.0);
    let dir = bevy::math::Vec3::new(0.2, -0.1, -0.97).normalize();
    let schw = physics::bending_accel(pos, dir);
    let kerr = physics::kerr_bending_accel(pos, dir, 0.8);
    let diff = (schw - kerr).length();
    assert!(
        diff > 1e-4,
        "spin=0.8 Kerr should differ from Schwarzschild; diff = {}",
        diff
    );
}

// ---- Adaptive Cash–Karp ray integration (CPU mirror of black_hole.wesl) ----

#[test]
fn rk45_at_zero_spin_captures_below_bcrit() {
    // Sample a zero-spin ray safely below the analytic Schwarzschild bcrit.
    let eye = bevy::math::Vec3::new(0.0, 0.0, 50.0);
    let dir = bevy::math::Vec3::new(0.0, 2.0, -50.0).normalize(); // b ~ 2.0 < bcrit
    let b = physics::impact_parameter(eye, dir);
    assert!(b < physics::BCRIT, "b {} should be < bcrit", b);
    assert!(
        physics::is_captured_rk45(eye, dir, 2000, 0.0),
        "spin=0 ray below bcrit should be captured by the RK45 loop"
    );
}

#[test]
fn rk45_at_zero_spin_escapes_above_bcrit() {
    let eye = bevy::math::Vec3::new(0.0, 0.0, 50.0);
    let dir = bevy::math::Vec3::new(0.0, 10.0, -50.0).normalize(); // b ~ 9.8 >> bcrit
    let b = physics::impact_parameter(eye, dir);
    assert!(b > physics::BCRIT);
    assert!(
        !physics::is_captured_rk45(eye, dir, 2000, 0.0),
        "spin=0 ray above bcrit should escape the RK45 loop"
    );
}

#[test]
fn rk45_higher_spin_still_captures_a_grazing_ray() {
    // Sample a ray that plunges into the smaller horizon at high spin.
    let eye = bevy::math::Vec3::new(0.0, 0.0, 50.0);
    let dir = bevy::math::Vec3::new(0.0, 2.0, -50.0).normalize();
    assert!(
        physics::is_captured_rk45(eye, dir, 2000, 0.9),
        "spin=0.9 ray at b~2.0 should still be captured"
    );
}

#[test]
fn rk45_capture_count_does_not_increase_for_sampled_rays_at_high_spin() {
    // Compare captures for this camera and sweep of impact parameters.
    let eye = bevy::math::Vec3::new(0.0, 0.0, 50.0);
    let count_captures = |chi: f32| -> usize {
        (0..=40)
            .map(|i| {
                let y = 1.6 + (i as f32) * 0.06; // b sweeps ~1.6 .. ~4.0, straddling bcrit
                let dir = bevy::math::Vec3::new(0.0, y, -50.0).normalize();
                physics::is_captured_rk45(eye, dir, 400, chi) as usize
            })
            .sum()
    };
    let c0 = count_captures(0.0);
    let c_hi = count_captures(0.9);
    assert!(
        c_hi <= c0,
        "expected at most as many captures for the high-spin sample: \
         spin=0 captures={}, spin=0.9 captures={}",
        c0,
        c_hi
    );
}

#[test]
fn rk45_step_matches_cash_karp_reference() {
    use bevy::math::Vec3;

    let pos = Vec3::new(1.6, 0.5, 0.7);
    let step = physics::rk45_step(pos, Vec3::NEG_Z, 0.8, 0.7);

    // Independent f64 evaluation of the GSL tableau, with the renderer's
    // direction normalization at each stage. Allow for rounding in f32.
    let expected_pos = Vec3::new(1.459_637_9, 0.462_374_4, -0.081_622_79);
    let expected_dir = Vec3::new(-0.364_355_62, -0.097_988_88, -0.926_090_24);
    assert!(step.pos.distance(expected_pos) < 1e-6, "pos={:?}", step.pos);
    assert!(step.dir.distance(expected_dir) < 1e-6, "dir={:?}", step.dir);
    assert!((step.err - 2.430_624_5e-5).abs() < 1e-6, "err={}", step.err);
}

#[test]
fn rk45_step_error_shrinks_monotonically_with_dt() {
    // Use a curved ray and step sizes that keep the position error above f32
    // roundoff while checking that it decreases with dt.
    let pos = bevy::math::Vec3::new(1.6, 0.5, 0.7);
    let dir = bevy::math::Vec3::new(0.0, 0.0, -1.0);
    let [large, medium, small] =
        [1.6, 0.8, 0.4].map(|dt| physics::rk45_step(pos, dir, dt, 0.7).err);
    assert!(
        large > medium && medium > small && small > 0.0,
        "expected decreasing positive errors: {large}, {medium}, {small}"
    );
    assert!(
        large > 10.0 * small,
        "error should span >10x across the dt range: {large}, {small}"
    );
}
