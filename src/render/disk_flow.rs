use std::f64::consts::TAU;

use crate::params::MAX_DISK_CLUMPS;

use super::material::DiskClump;

/// Builds the same disk-space sources for all rays in a frame. Each source
/// fades out before its position is reassigned, on its own clock.
pub(super) fn disk_clumps(time: f64, count: usize) -> [DiskClump; MAX_DISK_CLUMPS] {
    let mut clumps = [DiskClump::default(); MAX_DISK_CLUMPS];
    for (index, clump) in clumps.iter_mut().take(count).enumerate() {
        let slot = index as f64;
        let lifetime = 7.0 + slot * 0.8;
        let clock = time + 1.4 + slot * 2.37;
        let generation = (clock / lifetime).floor();
        let age = clock - generation * lifetime;
        let life = age / lifetime;
        let fade_in = smooth_unit(life / 0.17);
        let fade_out = smooth_unit((1.0 - life) / 0.34);
        let seed = slot + generation * MAX_DISK_CLUMPS as f64;
        *clump = DiskClump {
            radius_fraction: (0.11 + 0.46 * (0.27 + seed * 0.618_033_988_75).fract()) as f32,
            phase_at_birth: (TAU * (0.17 + seed * 0.754_877_666_25).fract()) as f32,
            age: age as f32,
            amplitude: (3.0 * fade_in * fade_out) as f32,
        };
    }
    clumps
}

fn smooth_unit(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clumps_fade_before_reseeding_and_live_on_separate_clocks() {
        let reset = 7.0 - 1.4;
        let before = disk_clumps(reset - 1e-3, 2);
        let after = disk_clumps(reset + 1e-3, 2);

        assert!(before[0].amplitude < 1e-4);
        assert!(after[0].amplitude < 1e-4);
        assert!((before[0].radius_fraction - after[0].radius_fraction).abs() > 0.01);
        assert!(after[1].amplitude > 0.1);
        assert!(after[2..].iter().all(|clump| clump.amplitude == 0.0));
    }
}
