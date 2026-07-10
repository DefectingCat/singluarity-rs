use singularity_rs::physics;

#[test]
fn public_bcrt_constant_is_correct() {
    // 3*sqrt(3)/2 ≈ 2.598076
    let expected = 1.5 * 3.0_f32.sqrt();
    assert!((physics::BCRIT - expected).abs() < 1e-5);
}
