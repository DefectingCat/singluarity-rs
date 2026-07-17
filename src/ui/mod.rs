mod style;
pub mod preset;

use bevy::prelude::*;
use bevy_egui::egui;

pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut camera: ResMut<crate::camera::OrbitCamera>,
    mut wants: ResMut<crate::camera::WantsPointer>,
    mut planet_dirty: ResMut<crate::scene::planets::PlanetSystemDirty>,
) {
    if let Ok(ctx) = contexts.ctx_mut() {
        egui::Window::new("Controls")
            .collapsible(true)
            .resizable(true)
            .default_pos([16.0, 16.0])
            .default_width(300.0)
            .default_height(560.0)
            .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Camera")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add(egui::Slider::new(&mut camera.distance, 3.0..=200.0).text("Distance"));
                        ui.add(egui::Slider::new(&mut camera.yaw, -std::f32::consts::PI..=std::f32::consts::PI).text("Yaw"));
                        ui.add(egui::Slider::new(&mut camera.pitch, (-std::f32::consts::PI + 0.05)..=(std::f32::consts::PI - 0.05)).text("Pitch"));
                        ui.add(egui::Slider::new(&mut camera.fov, 0.3..=2.0).text("FOV"));
                    });
                egui::CollapsingHeader::new("Black Hole")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add(egui::Slider::new(&mut params.spin, 0.0..=1.0).text("Spin (χ)"));
                        ui.label(format!("ISCO (disk inner): {:.3}", crate::physics::kerr_isco(params.spin)));
                        ui.label(format!("Horizon r+: {:.3}", crate::physics::kerr_horizon(params.spin)));
                    });
                egui::CollapsingHeader::new("Accretion Disk")
                    .default_open(true)
                    .show(ui, |ui| {
                        // disk_inner removed — now spin-derived (see Black Hole section).
                        ui.add(egui::Slider::new(&mut params.disk_outer, 6.0..=50.0).text("Outer radius"));
                        ui.add(egui::Slider::new(&mut params.disk_tilt, 0.0..=std::f32::consts::PI).text("Tilt"));
                        ui.add(egui::Slider::new(&mut params.disk_brightness, 0.0..=3.0).text("Brightness"));
                        ui.add(egui::Slider::new(&mut params.disk_rotation_speed, 0.0..=3.0).text("Rotation speed"));
                        use crate::params::DiskColorMode;
                        let mut cm = params.disk_color_mode;
                        egui::ComboBox::from_label("Color model")
                            .selected_text(format!("{:?}", cm))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut cm, DiskColorMode::Gradient, "Gradient");
                                ui.selectable_value(&mut cm, DiskColorMode::Blackbody, "Blackbody");
                            });
                        params.disk_color_mode = cm;
                        ui.add_enabled(
                            cm == DiskColorMode::Blackbody,
                            egui::Slider::new(&mut params.disk_temp, 1000.0..=50000.0).text("Temperature (K)"),
                        );
                    });
                egui::CollapsingHeader::new("Planets")
                    .default_open(false)
                    .show(ui, |ui| {
                        // Record state before edits; if seed/count/k/enabled change,
                        // flag dirty so spawn_planet_system regenerates next frame.
                        // (time_scale excluded: it only scales orbit_system's time,
                        //  no respawn needed.)
                        let prev = (
                            params.planets_enabled,
                            params.planet_count_target,
                            params.planet_radius_factor,
                            params.planet_seed,
                        );
                        ui.checkbox(&mut params.planets_enabled, "Enable");
                        ui.add(egui::Slider::new(&mut params.planet_count_target, 0..=8).text("Count"));
                        ui.add(egui::Slider::new(&mut params.planet_radius_factor, 1.1..=2.0).text("Radius (× disk outer)"));
                        ui.label(format!("Orbit r = {:.2} (disk outer: {:.1})", params.planet_radius_factor * params.disk_outer, params.disk_outer));
                        ui.add(egui::Slider::new(&mut params.planet_seed, 0..=1000).text("Seed"));
                        ui.add(egui::Slider::new(&mut params.planet_time_scale, 1.0..=200.0).text("Time scale"));
                        let curr = (
                            params.planets_enabled,
                            params.planet_count_target,
                            params.planet_radius_factor,
                            params.planet_seed,
                        );
                        if curr != prev {
                            planet_dirty.0 = true;
                        }
                    });
                egui::CollapsingHeader::new("Disk Turbulence")
                    .default_open(true)
                    .show(ui, |ui| {
                        use crate::params::DiskQuality;
                        let mut q = params.disk_quality;
                        egui::ComboBox::from_label("Disk quality")
                            .selected_text(format!("{:?}", q))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut q, DiskQuality::Off, "Off");
                                ui.selectable_value(&mut q, DiskQuality::Low, "Low");
                                ui.selectable_value(&mut q, DiskQuality::Medium, "Medium");
                                ui.selectable_value(&mut q, DiskQuality::High, "High");
                            });
                        params.disk_quality = q;
                        let on = q != DiskQuality::Off;
                        ui.add_enabled(on, egui::Slider::new(&mut params.disk_half_thickness, 0.02..=0.3).text("Thickness (H/R)"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.filament_freq, 0.2..=4.0).text("Filament frequency"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.filament_sharpness, 1.0..=6.0).text("Filament sharpness"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.density_freq, 0.2..=3.0).text("Density frequency"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.density_strength, 0.0..=2.0).text("Density strength"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.arm_count, 0.0..=6.0).text("Arm count"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.arm_tightness, 0.0..=6.0).text("Arm tightness"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.arm_strength, 0.0..=1.0).text("Arm strength"));
                    });
                egui::CollapsingHeader::new("Doppler").show(ui, |ui| {
                    ui.checkbox(&mut params.doppler_enabled, "Enabled");
                    ui.add_enabled(params.doppler_enabled, egui::Slider::new(&mut params.doppler_strength, 0.0..=3.0).text("Strength"));
                });
                egui::CollapsingHeader::new("Jets").show(ui, |ui| {
                    ui.checkbox(&mut params.jets_enabled, "Enabled");
                    // Mirror the shader's spin gate (sample_jets in black_hole.wgsl):
                    // jets are a spin-powered (Blandford-Znajek) outflow and render
                    // only for χ ≥ 0.05. When the user enables them at low spin,
                    // explain the no-op so the checkbox doesn't look broken.
                    let jets_renderable = params.spin >= 0.05;
                    if params.jets_enabled && !jets_renderable {
                        ui.label("Spin (χ) too low — jets need χ ≥ 0.05.");
                    }
                    ui.add_enabled(
                        params.jets_enabled && jets_renderable,
                        egui::Slider::new(&mut params.jets_strength, 0.0..=3.0).text("Strength"),
                    );
                });
                egui::CollapsingHeader::new("Renderer").show(ui, |ui| {
                    ui.add(egui::Slider::new(&mut params.steps, 50..=600).text("Steps"));
                    ui.add(egui::Slider::new(&mut params.render_scale, 0.25..=1.0).text("Render scale"));
                });
                egui::CollapsingHeader::new("Background").show(ui, |ui| {
                    ui.add(egui::Slider::new(&mut params.star_intensity, 0.0..=3.0).text("Star intensity"));
                    ui.add(egui::Slider::new(&mut params.skybox_intensity, 0.0..=3.0).text("Skybox intensity"));
                });
                egui::CollapsingHeader::new("Grid").show(ui, |ui| {
                    ui.checkbox(&mut params.grid_enabled, "Enabled");
                    ui.add_enabled(params.grid_enabled, egui::Slider::new(&mut params.grid_density, 0.1..=4.0).text("Density"));
                });
                egui::CollapsingHeader::new("Quality")
                    .default_open(true)
                    .show(ui, |ui| {
                        use crate::params::BloomQuality;
                        let mut q = params.bloom_quality;
                        egui::ComboBox::from_label("Bloom quality")
                            .selected_text(format!("{:?}", q))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut q, BloomQuality::Off, "Off");
                                ui.selectable_value(&mut q, BloomQuality::Low, "Low");
                                ui.selectable_value(&mut q, BloomQuality::Medium, "Medium");
                                ui.selectable_value(&mut q, BloomQuality::High, "High");
                            });
                        params.bloom_quality = q;
                        ui.add_enabled(q != BloomQuality::Off, egui::Slider::new(&mut params.bloom_threshold, 0.0..=3.0).text("Bloom threshold"));
                        ui.add_enabled(q != BloomQuality::Off, egui::Slider::new(&mut params.bloom_strength, 0.0..=2.0).text("Bloom strength"));
                        ui.add(egui::Slider::new(&mut params.exposure, 0.5..=3.0).text("Exposure"));
                        ui.add(egui::Slider::new(&mut params.render_scale, 0.25..=1.0).text("Resolution scale"));
                        ui.checkbox(&mut params.star_aa, "Anti-aliased stars");
                        {
                            // Per-pixel supersampling: antialiases the higher-order
                            // lensed-image rings on the disk into a smooth gradient.
                            // Cost scales linearly with sample count (each sub-ray
                            // runs the full RK45 march).
                            use crate::params::AaQuality;
                            let mut a = params.aa_quality;
                            egui::ComboBox::from_label("Ring anti-alias")
                                .selected_text(format!("{:?} ({}×)", a, a.samples()))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut a, AaQuality::Off, "Off (1×)");
                                    ui.selectable_value(&mut a, AaQuality::Low, "Low (2×)");
                                    ui.selectable_value(&mut a, AaQuality::High, "High (4×)");
                                });
                            params.aa_quality = a;
                        }
                        ui.label("MSAA is decorative on a fullscreen shader (no geometry edges to sample).");
                    });
            });
            });
        // egui captures pointer when the cursor is over a window or being interacted with.
        wants.0 = ctx.egui_wants_pointer_input();
    } else {
        wants.0 = false;
    }
}
