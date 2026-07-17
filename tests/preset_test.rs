use singularity_rs::params::BlackHoleParams;
use singularity_rs::ui::preset::{Preset, apply, canonical_hash, params_hash};

#[test]
fn canonical_hash_matches_just_applied_params() {
    // After applying a preset, params_hash of the result must equal that
    // preset's canonical_hash — otherwise the Custom-detection logic would
    // immediately flip a freshly-applied preset back to Custom.
    let mut p = BlackHoleParams::default();
    for preset in [Preset::Cinematic, Preset::Performance, Preset::Web] {
        apply(preset, &mut p);
        assert_eq!(
            canonical_hash(preset),
            params_hash(&p),
            "preset {:?}: apply() did not reproduce canonical_hash",
            preset
        );
    }
}

#[test]
fn non_preset_field_change_flips_to_custom() {
    // Editing a field that NO preset touches (disk_tilt is not in any preset bundle)
    // must NOT change params_hash. This guards the "hash only preset fields"
    // invariant: non-preset edits must not spuriously flip to Custom.
    let mut p = BlackHoleParams::default();
    let h0 = params_hash(&p);
    p.disk_tilt = 1.0; // not in any preset bundle
    assert_eq!(h0, params_hash(&p), "non-preset field leaked into hash");
}

#[test]
fn preset_field_change_differs_from_canonical() {
    // Editing a preset-touched field after applying must change the hash
    // away from the preset's canonical_hash (i.e. flip to Custom).
    let mut p = BlackHoleParams::default();
    apply(Preset::Cinematic, &mut p);
    let h_canonical = canonical_hash(Preset::Cinematic);
    p.steps = 299; // off by one from the Cinematic bundle
    assert_ne!(h_canonical, params_hash(&p));
}
