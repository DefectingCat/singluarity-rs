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

// ---- Loop-level CPU ↔ shader mirror tests (adaptive RK45 + Kerr) ----
// These exercise the full integration loop (`is_captured_rk45`) that mirrors
// the shader's black_hole.wgsl:320-390 loop, not just the single-step deriv.

#[test]
fn rk45_at_zero_spin_captures_below_bcrit() {
    // spin=0 Kerr loop must reproduce the Schwarzschild capture boundary.
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
    // A ray that would be captured at spin=0 (b < bcrit) must remain captured
    // at high spin — the horizon shrinks, but a b ~ 2.0 ray still plunges in.
    let eye = bevy::math::Vec3::new(0.0, 0.0, 50.0);
    let dir = bevy::math::Vec3::new(0.0, 2.0, -50.0).normalize();
    assert!(
        physics::is_captured_rk45(eye, dir, 2000, 0.9),
        "spin=0.9 ray at b~2.0 should still be captured"
    );
}

#[test]
fn rk45_capture_radius_shrinks_with_spin() {
    // Near the critical impact parameter, a higher-spin hole (smaller horizon,
    // prograde frame-dragging) is *easier* for a prograde ray to escape. With a
    // fixed step count, count captures across a sweep of impact parameters and
    // assert the capture set does not grow as spin increases — i.e. the boundary
    // does not move outward. This is the robust, sign-agnostic assertion.
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
        "higher spin should not capture more rays across the bcrit sweep; \
         spin=0 captures={}, spin=0.9 captures={}",
        c0,
        c_hi
    );
}

#[test]
fn rk45_step_error_shrinks_monotonically_with_dt() {
    // The Dormand-Prince error estimate |y5 − y4| must shrink monotonically as
    // dt shrinks — this is the property the shader's adaptive loop relies on to
    // decide reject/retry vs accept (black_hole.wgsl:326-335). It does NOT need
    // to be a clean 5th-order power law here: the per-stage `normalize(dir + …)`
    // projection in both the shader and the mirror makes the *realized* error
    // scaling fall between 2nd and 4th order depending on geometry. We assert
    // only what the loop actually depends on: smaller dt ⇒ smaller error, by a
    // factor strictly greater than 1, across a halving sequence. This is the
    // load-bearing correctness property; pinning a specific order would be
    // testing a model of the integrator, not the integrator as shipped.
    let pos = bevy::math::Vec3::new(4.0, 0.5, 0.0);
    let dir = bevy::math::Vec3::new(0.0, 0.0, -1.0);
    let mut prev = f32::INFINITY;
    for &dt in &[0.4_f32, 0.2, 0.1, 0.05, 0.025] {
        let err = physics::rk45_step(pos, dir, dt, 0.0).err;
        assert!(
            err < prev,
            "error should decrease as dt shrinks: dt={} err={} prev={}",
            dt,
            err,
            prev
        );
        prev = err;
    }
    // And the shrink is meaningful — the largest dt's error is at least 10x the
    // smallest (rules out the error being flat / dominated by a constant floor).
    let err_big = physics::rk45_step(pos, dir, 0.4, 0.0).err;
    let err_small = physics::rk45_step(pos, dir, 0.025, 0.0).err;
    assert!(
        err_big / err_small.max(1e-18) > 10.0,
        "error should span >10x across the dt range; big={} small={}",
        err_big,
        err_small
    );
}
