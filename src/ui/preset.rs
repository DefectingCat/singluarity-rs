//! Preset bundles + Custom-edit detection (design spec §3).
//!
//! `params_hash` hashes ONLY the fields that presets touch — otherwise a
//! non-preset edit (e.g. disk_tilt) would spuriously flip the bar to Custom.
//! The field set below is the single source of truth; if you add a field to
//! a preset bundle, add it to `hashed_fields` too or the test
//! `non_preset_field_change_flips_to_custom` will not catch the leak.

use crate::params::{AaQuality, BlackHoleParams, BloomQuality, DiskQuality};

use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Preset {
    #[default]
    /// Read-only marker set when any preset-touched field is hand-edited
    /// away from a preset bundle. `apply(Custom, _)` is a no-op.
    Custom,
    Cinematic,
    Performance,
    Web,
}

/// The exact params each preset writes. `Custom` writes nothing.
fn bundle(p: Preset) -> Option<HashedParams> {
    // Defaults chosen to mirror cfg!(wasm32) dual-defaults already in the codebase:
    // Cinematic = desktop default, Web = wasm default, Performance = a low tier.
    Some(match p {
        Preset::Cinematic => HashedParams {
            steps: 300, render_scale: 0.75,
            bloom_quality: BloomQuality::High, disk_quality: DiskQuality::High, aa_quality: AaQuality::High,
        },
        Preset::Performance => HashedParams {
            steps: 150, render_scale: 0.5,
            bloom_quality: BloomQuality::Low, disk_quality: DiskQuality::Low, aa_quality: AaQuality::Off,
        },
        Preset::Web => HashedParams {
            steps: 200, render_scale: 0.5,
            bloom_quality: BloomQuality::Low, disk_quality: DiskQuality::Low, aa_quality: AaQuality::Off,
        },
        Preset::Custom => return None,
    })
}

/// Apply a preset's bundle to params. `Custom` is a no-op.
pub fn apply(p: Preset, params: &mut BlackHoleParams) {
    if let Some(b) = bundle(p) {
        params.steps = b.steps;
        params.render_scale = b.render_scale;
        params.bloom_quality = b.bloom_quality;
        params.disk_quality = b.disk_quality;
        params.aa_quality = b.aa_quality;
    }
}

/// Stable hash of a preset's canonical bundle. `Custom` returns 0.
/// (The Custom case is never fed to a `canonical_hash == params_hash`
/// comparison: `ui_system`'s preset-detection arm only checks
/// Cinematic | Performance | Web, so the 0 sentinel never risks matching.)
pub fn canonical_hash(p: Preset) -> u64 {
    match bundle(p) {
        Some(b) => b.hash(),
        None => 0,
    }
}

/// Hash of the preset-touched fields of a live params. Used by `ui_system`
/// to detect hand-edits and flip the bar to Custom.
pub fn params_hash(params: &BlackHoleParams) -> u64 {
    let h = HashedParams {
        steps: params.steps,
        render_scale: params.render_scale,
        bloom_quality: params.bloom_quality,
        disk_quality: params.disk_quality,
        aa_quality: params.aa_quality,
    };
    h.hash()
}

// --- internals ---

#[derive(Clone, Copy)]
struct HashedParams {
    steps: u32,
    render_scale: f32,
    bloom_quality: BloomQuality,
    disk_quality: DiskQuality,
    aa_quality: AaQuality,
}

impl HashedParams {
    fn hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.steps.hash(&mut hasher);
        self.render_scale.to_bits().hash(&mut hasher); // f32: hash bit pattern, not value
        self.bloom_quality.levels().hash(&mut hasher);
        self.disk_quality.as_u32().hash(&mut hasher);
        self.aa_quality.samples().hash(&mut hasher);
        hasher.finish()
    }
}
