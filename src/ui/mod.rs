mod sections;
mod style;
pub mod preset;

use bevy::prelude::*;
use bevy_egui::egui;
use egui::{LayerId, UiBuilder};

use crate::ui::preset::{Preset, apply, canonical_hash, params_hash};
use crate::ui::style::ACCENT_CYAN;

/// One-shot egui styling. Registered in `EguiPrimaryContextPass` with a
/// `Local<bool>` guard so it retries until `ctx_mut()` first succeeds, then
/// runs exactly once. Per-frame set_style/set_visuals would dirty layout
/// caches every frame; this avoids that.
pub fn setup_egui_style(
    mut contexts: bevy_egui::EguiContexts,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    if let Ok(ctx) = contexts.ctx_mut() {
        crate::ui::style::setup(&ctx);
        *done = true;
    }
}

/// An always-open framed card. Title in cyan `RichText::strong().small()`.
/// Used for the 4 most-tuned groups (Camera / Black Hole / Disk / Quality).
fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(8.0)
        .corner_radius(6.0)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().window_stroke.color,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .strong()
                    .small()
                    .color(ACCENT_CYAN),
            );
            ui.add_space(2.0);
            body(ui);
        });
}

/// A collapsible section (`default_open` controls initial state).
/// Used for the 6 secondary groups. The body closure is only invoked when open.
fn collapsing(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    egui::CollapsingHeader::new(title)
        .default_open(default_open)
        .id_salt(id)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}

/// Collapsible section whose header hosts an enable toggle on the right.
/// Used by Doppler / Jets / Grid / Planets (design §4.1). The body closure
/// receives the current `enabled` state so it can `add_enabled` its rows.
///
/// State persistence is handled by `HeaderResponse::body`, which calls
/// `CollapsingState::store` internally via `show_body_indented` (see egui
/// 0.35 `containers/collapsing_header.rs`). `show_header` consumes the
/// state, so there is no separate `store` call here.
fn collapsing_with_toggle(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    default_open: bool,
    enabled: &mut bool,
    body: impl FnOnce(&mut egui::Ui, bool),
) {
    let id = ui.make_persistent_id(id);
    let state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    state
        .show_header(ui, |ui| {
            ui.checkbox(enabled, title);
        })
        .body(|ui| {
            ui.set_width(ui.available_width());
            body(ui, *enabled);
        });
}

/// The top preset bar. `current` is the UI-layer state (which preset is
/// shown as selected); `just_applied` is set for one frame after a preset
/// is chosen, to skip the Custom-detection hash compare on that frame
/// (applying a preset changes params; that change must not flip to Custom).
fn preset_bar(
    ui: &mut egui::Ui,
    params: &mut crate::params::BlackHoleParams,
    current: &mut Preset,
    just_applied: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Preset:");
        let prev = *current;
        egui::ComboBox::from_id_salt("preset_combo")
            .selected_text(format!("{:?}", prev))
            .show_ui(ui, |ui| {
                ui.selectable_value(current, Preset::Cinematic, "Cinematic");
                ui.selectable_value(current, Preset::Performance, "Performance");
                ui.selectable_value(current, Preset::Web, "Web");
                ui.selectable_value(current, Preset::Custom, "Custom");
            });
        if *current != prev && *current != Preset::Custom {
            // User picked a concrete preset → apply its bundle.
            apply(*current, params);
            *just_applied = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Global reset-to-default for all params.
            if ui.button("↺ all").clicked() {
                *params = crate::params::BlackHoleParams::default();
                *current = Preset::Custom;
                *just_applied = true;
            }
        });
    });
    ui.separator();
}

pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut camera: ResMut<crate::camera::OrbitCamera>,
    mut wants: ResMut<crate::camera::WantsPointer>,
    mut planet_dirty: ResMut<crate::scene::planets::PlanetSystemDirty>,
    mut current_preset: Local<Preset>,
    mut just_applied: Local<bool>,
    mut last_hash: Local<Option<u64>>,
) {
    // Local<Preset> defaults to Custom (Preset::default()).
    // Local<Option<u64>> defaults to None — first-frame sentinel.

    let Ok(ctx) = contexts.ctx_mut() else {
        wants.0 = false;
        return;
    };

    // --- Custom-detection (skip on the frame a preset was just applied) ---
    let now_hash = params_hash(&params);
    if !*just_applied && last_hash.is_some() && now_hash != last_hash.unwrap() {
        // Some preset-touched field changed by hand. If it no longer matches
        // the current concrete preset's canonical bundle, flip to Custom.
        let matches_any = matches!(
            *current_preset,
            Preset::Cinematic | Preset::Performance | Preset::Web
        ) && canonical_hash(*current_preset) == now_hash;
        if !matches_any && *current_preset != Preset::Custom {
            *current_preset = Preset::Custom;
        }
    }
    *just_applied = false;
    *last_hash = Some(now_hash);

    // --- Chassis ---
    // egui 0.35's Panel::show needs a parent `&mut Ui`, but bevy_egui's
    // single-pass `EguiPrimaryContextPass` only hands us a `Context`. Create
    // the top-level Ui manually (matches bevy_egui 0.41's side_panel example).
    // `Context` is an `Arc`, so cloning it here is cheap.
    let mut root_ui = egui::Ui::new(
        ctx.clone(),
        "controls_root".into(),
        UiBuilder::new()
            .layer_id(LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::right("controls")
        .default_size(300.0)
        .size_range(260.0..=400.0)
        .resizable(true)
        .show(&mut root_ui, |ui| {
            // Top bar (fixed).
            preset_bar(ui, &mut params, &mut current_preset, &mut just_applied);

            // Section stack (scrolls).
            egui::ScrollArea::vertical().show(ui, |ui| {
                use crate::ui::sections::*;

                // --- Always-open cards ---
                group(ui, "Camera", |ui| section_camera(ui, &mut camera));
                group(ui, "Black Hole", |ui| section_black_hole(ui, &mut params));
                group(ui, "Accretion Disk", |ui| section_disk(ui, &mut params));
                group(ui, "Quality", |ui| section_quality(ui, &mut params));

                // --- Collapsing sections ---
                collapsing(ui, "turbulence", "Disk Turbulence", false,
                    |ui| section_turbulence(ui, &mut params));

                // collapsing_with_toggle takes `&mut bool` for the header
                // checkbox AND a body closure that mutates `params`. Passing
                // `&mut params.<field>` plus a closure borrowing `&mut params`
                // is a double-mutable-borrow; copy the field out, pass a local
                // ref, then write it back. (All four are `bool` → Copy.)
                let mut en = params.doppler_enabled;
                collapsing_with_toggle(ui, "doppler", "Doppler", false,
                    &mut en,
                    |ui, en| section_doppler(ui, &mut params, en));
                params.doppler_enabled = en;

                let mut en = params.jets_enabled;
                collapsing_with_toggle(ui, "jets", "Jets", false,
                    &mut en,
                    |ui, en| section_jets(ui, &mut params, en));
                params.jets_enabled = en;

                // Planets: snapshot dirty-relevant fields before rendering so
                // we can detect changes (relocated from the old Planets block).
                let prev_planet = (
                    params.planets_enabled,
                    params.planet_count_target,
                    params.planet_radius_factor,
                    params.planet_seed,
                );
                let mut en = params.planets_enabled;
                collapsing_with_toggle(ui, "planets", "Planets", false,
                    &mut en,
                    |ui, en| section_planets(ui, &mut params, en));
                params.planets_enabled = en;
                let curr_planet = (
                    params.planets_enabled,
                    params.planet_count_target,
                    params.planet_radius_factor,
                    params.planet_seed,
                );
                if curr_planet != prev_planet {
                    planet_dirty.0 = true;
                }

                collapsing(ui, "background", "Background", false,
                    |ui| section_background(ui, &mut params));

                let mut en = params.grid_enabled;
                collapsing_with_toggle(ui, "grid", "Grid", false,
                    &mut en,
                    |ui, en| section_grid(ui, &mut params, en));
                params.grid_enabled = en;
            });
        });

    // egui captures pointer when the cursor is over a window or being
    // interacted with. MUST stay last — load-bearing for orbit camera.
    wants.0 = ctx.egui_wants_pointer_input();
}
