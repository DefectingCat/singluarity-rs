mod lab;
pub mod loading;
pub mod preset;
mod probe;
mod sections;
mod stellar;
mod style;
mod trajectory;

use bevy::prelude::*;
use bevy_egui::egui::{self, Align, Button, Layout, RichText, Ui};

use crate::camera::{OrbitCamera, OrbitInteraction, SceneFraming, UiPointerActivity, WantsPointer};
use crate::params::BlackHoleParams;
use crate::scene::planets::PlanetSystemDirty;
use crate::ui::preset::{Preset, apply, canonical_hash, params_hash};
use crate::ui::style::{ACCENT, MUTED_TEXT, TEXT};

use lab::{LabLayout, TRANSITION_SECONDS, content_opacity, fit_distance, frame_scene, reveal};

/// Set the style once so subsequent frames can reuse egui's layout caches.
pub fn setup_egui_style(mut contexts: bevy_egui::EguiContexts, mut done: Local<bool>) {
    if !*done && let Ok(ctx) = contexts.ctx_mut() {
        style::setup(ctx);
        *done = true;
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq)]
enum ControlTab {
    #[default]
    Camera,
    Scene,
    Render,
    Effects,
}

impl ControlTab {
    const TABS: [(Self, &'static str); 4] = [
        (Self::Camera, "Camera"),
        (Self::Scene, "Scene"),
        (Self::Render, "Render"),
        (Self::Effects, "Effects"),
    ];
}

pub struct Controls {
    tab: ControlTab,
    collapsed: bool,
    preset: Preset,
    probe: probe::ProbeLab,
    stellar: stellar::StellarLab,
    tab_changed_at: Option<f64>,
    fit_pending: bool,
    distance_scale: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            tab: ControlTab::Camera,
            collapsed: true,
            preset: Preset::Custom,
            probe: probe::ProbeLab::default(),
            stellar: stellar::StellarLab::default(),
            tab_changed_at: None,
            fit_pending: false,
            distance_scale: 1.0,
        }
    }
}

const PRESETS: [(Preset, &str); 4] = [
    (Preset::Custom, "Custom"),
    (Preset::Cinematic, "Cinematic"),
    (Preset::Performance, "Performance"),
    (Preset::Web, "Web"),
];

#[allow(clippy::too_many_arguments)]
pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<BlackHoleParams>,
    mut camera: ResMut<OrbitCamera>,
    mut orbit: ResMut<OrbitInteraction>,
    mut framing: ResMut<SceneFraming>,
    mut wants: ResMut<WantsPointer>,
    mut ui_activity: ResMut<UiPointerActivity>,
    mut planet_dirty: ResMut<PlanetSystemDirty>,
    mut controls: Local<Controls>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        wants.0 = false;
        ui_activity.0 = false;
        *framing = SceneFraming::default();
        return;
    };

    let planets_before = planet_configuration(&params);
    let probe_active = controls.probe.is_active();
    let stellar_active = controls.stellar.is_active();
    let main = controls.show(
        ctx,
        &mut params,
        &mut camera,
        &mut orbit,
        &mut framing,
        !probe_active && !stellar_active,
    );
    let regions = if probe_active {
        controls.probe.show(ctx, &camera, &params, &mut framing)
    } else if stellar_active {
        controls.stellar.show(ctx, &camera, &params, &mut framing)
    } else {
        main
    };
    if planets_before != planet_configuration(&params) {
        planet_dirty.0 = true;
    }

    ui_activity.0 = ctx.egui_is_using_pointer();
    wants.0 = ui_activity.0 || pointer_over_controls(ctx, &regions);
}

fn planet_configuration(params: &BlackHoleParams) -> (bool, u32, f32, u32) {
    (
        params.planets_enabled,
        params.planet_count_target,
        params.planet_radius_factor,
        params.planet_seed,
    )
}

fn pointer_over_controls(ctx: &egui::Context, regions: &[egui::Rect]) -> bool {
    // The actual frame bounds cover hover and presses. Popups can extend
    // beyond those bounds. The decorative shadow does not capture input.
    ctx.input(|input| {
        input
            .pointer
            .interact_pos()
            .is_some_and(|pos| regions.iter().any(|rect| rect.contains(pos)))
    }) || (ctx.any_popup_open() && ctx.is_pointer_over_egui())
}

impl Controls {
    fn show(
        &mut self,
        ctx: &egui::Context,
        params: &mut BlackHoleParams,
        camera: &mut OrbitCamera,
        orbit: &mut OrbitInteraction,
        framing: &mut SceneFraming,
        visible: bool,
    ) -> [egui::Rect; 3] {
        if visible && !ctx.text_edit_focused() && !ctx.any_popup_open() {
            let tab_available = !ctx.egui_wants_keyboard_input();
            ctx.input_mut(|input| {
                if !self.collapsed && input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    self.collapsed = true;
                } else if input.consume_key(egui::Modifiers::NONE, egui::Key::H)
                    || (tab_available && input.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
                {
                    self.toggle();
                }
            });
        }

        let layout = LabLayout::new(ctx);
        let toolbar_reveal = reveal(ctx, "main_toolbar", visible);
        let panel_progress = reveal(ctx, "main_inspector", visible && !self.collapsed);
        if !visible && toolbar_reveal <= 0.0 && panel_progress <= 0.0 {
            return [egui::Rect::NOTHING; 3];
        }
        let toolbar = layout.toolbar(ctx, "controls_launcher", toolbar_reveal, visible, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                self.probe.launcher(ui, [103.0, 32.0]);
                self.stellar.launcher(ui, [80.0, 32.0]);
                let label = if self.collapsed {
                    "Controls"
                } else {
                    "Hide controls"
                };
                if ui
                    .add_sized([98.0, 32.0], Button::new(label).frame(false))
                    .on_hover_text("Toggle controls (H / Tab)")
                    .clicked()
                {
                    self.toggle();
                }
            });
        });

        let open = visible && !self.collapsed;
        let mut panel = egui::Rect::NOTHING;
        let mut target = SceneFraming::default();
        if panel_progress > 0.0 {
            let surface = layout.inspector(
                ctx,
                "controls_inspector",
                &toolbar,
                None,
                panel_progress,
                toolbar_reveal,
                open,
                |ui, _| self.inspector(ui, params, camera, orbit),
            );
            if open {
                panel = surface.rect;
                let canvas = ctx.viewport_rect();
                let region = layout.scene_region(&toolbar, None, Some(surface.settled));
                if std::mem::take(&mut self.fit_pending) {
                    let radius =
                        (crate::physics::kerr_isco(params.spin) + 1.5).max(4.0) * params.rs;
                    self.distance_scale =
                        fit_distance(canvas, region, region.center(), camera, radius);
                }
                target = frame_scene(canvas, region.center(), self.distance_scale);
            }
        }
        if visible && ctx.current_pass_index() == 0 {
            framing.approach(target, ctx.input(|input| input.stable_dt));
        }
        [toolbar.rect, panel, egui::Rect::NOTHING]
    }

    fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
        self.fit_pending = !self.collapsed;
    }

    fn inspector(
        &mut self,
        ui: &mut Ui,
        params: &mut BlackHoleParams,
        camera: &mut OrbitCamera,
        orbit: &mut OrbitInteraction,
    ) {
        self.tabs(ui);
        ui.separator();

        // Keep preset and reset controls available while the tab scrolls.
        let body_height = (ui.available_height() - 66.0).max(0.0);
        let tab = self.tab;
        egui::ScrollArea::vertical()
            .id_salt(tab)
            .max_height(body_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let opacity = content_opacity(ui.input(|input| input.time), self.tab_changed_at);
                ui.multiply_opacity(opacity);
                if opacity < 1.0 {
                    ui.ctx().request_repaint();
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(8, 8))
                    .show(ui, |ui| {
                        use crate::ui::sections::*;

                        match tab {
                            ControlTab::Camera => section_camera(ui, camera, orbit),
                            ControlTab::Scene => section_scene(ui, params),
                            ControlTab::Render => section_render(ui, params),
                            ControlTab::Effects => section_effects(ui, params),
                        }
                    });
            });
        ui.separator();
        self.preset_bar(ui, params);
    }

    fn tabs(&mut self, ui: &mut Ui) {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let width = ui.available_width() / ControlTab::TABS.len() as f32;
                    let mut first = egui::Rect::NOTHING;
                    let mut selected_index = 0.0;
                    for (index, (tab, name)) in ControlTab::TABS.into_iter().enumerate() {
                        let selected = self.tab == tab;
                        let label = RichText::new(name).strong().color(if selected {
                            TEXT
                        } else {
                            MUTED_TEXT
                        });
                        let response = ui.add_sized(
                            [width, 33.0],
                            Button::new(label)
                                .frame(true)
                                .frame_when_inactive(false)
                                .truncate(),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                selected,
                                name,
                            )
                        });
                        if response.clicked() && !selected {
                            self.tab = tab;
                            self.tab_changed_at = Some(ui.input(|input| input.time));
                        }
                        if index == 0 {
                            first = response.rect;
                        }
                        if self.tab == tab {
                            selected_index = index as f32;
                        }
                    }
                    let selected = ui.ctx().animate_value_with_time(
                        ui.id().with("active_tab"),
                        selected_index,
                        TRANSITION_SECONDS,
                    );
                    let underline = first.translate(egui::vec2(selected * width, 0.0));
                    ui.painter().hline(
                        underline.x_range(),
                        underline.bottom() - 1.0,
                        egui::Stroke::new(2.0, ACCENT),
                    );
                });
            });
    }

    fn preset_bar(&mut self, ui: &mut Ui, params: &mut BlackHoleParams) {
        if self.preset != Preset::Custom && params_hash(params) != canonical_hash(self.preset) {
            self.preset = Preset::Custom;
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(17, 11))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_height(28.0);
                    let label = ui.label(RichText::new("Preset").small().color(MUTED_TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let reset = ui
                            .add(
                                Button::new(RichText::new("Reset").color(MUTED_TEXT))
                                    .frame(true)
                                    .frame_when_inactive(false),
                            )
                            .on_hover_text("Reset scene and rendering parameters")
                            .clicked();

                        let previous = self.preset;
                        let selected = PRESETS
                            .iter()
                            .find_map(|&(preset, name)| (preset == previous).then_some(name));
                        egui::ComboBox::from_id_salt("preset")
                            .selected_text(selected.unwrap_or_default())
                            .width(ui.available_width())
                            .wrap_mode(egui::TextWrapMode::Truncate)
                            .show_ui(ui, |ui| {
                                for (preset, name) in PRESETS {
                                    ui.selectable_value(&mut self.preset, preset, name);
                                }
                            })
                            .response
                            .labelled_by(label.id);

                        if reset {
                            // The live count belongs to the GPU upload system.
                            let planet_count = params.planet_count;
                            *params = BlackHoleParams {
                                planet_count,
                                ..Default::default()
                            };
                            self.preset = Preset::Custom;
                        } else if self.preset != previous {
                            apply(self.preset, params);
                        }
                    });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_controls_only_capture_the_launcher() {
        let ctx = egui::Context::default();
        style::setup(&ctx);
        let mut controls = Controls {
            collapsed: false,
            tab: ControlTab::Effects,
            ..Default::default()
        };
        let mut camera = OrbitCamera::default();
        let mut orbit = OrbitInteraction::default();
        let mut params = BlackHoleParams::default();
        let mut framing = SceneFraming::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 650.0));
        let mut time = 0.0;
        let mut frame = |events| {
            time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(viewport),
                events,
                time: Some(time),
                ..Default::default()
            };
            let mut result = None;
            let mut output = ctx.run_ui(input, |_| {
                let rects = controls.show(
                    &ctx,
                    &mut params,
                    &mut camera,
                    &mut orbit,
                    &mut framing,
                    true,
                );
                result = Some((rects, pointer_over_controls(&ctx, &rects), framing.offset));
            });
            output.textures_delta.clear();
            result.unwrap()
        };
        let panel_point = egui::Event::PointerMoved(egui::pos2(981.0, 145.0));
        for _ in 0..28 {
            frame(Vec::new());
        }
        let ([launcher, inspector, _], over_inspector, expanded_offset) =
            frame(vec![panel_point.clone()]);
        assert!(over_inspector);
        assert!(viewport.contains_rect(inspector));
        assert!(inspector.height() < viewport.height());
        assert!(expanded_offset.x < -0.12);

        let effects_group = egui::pos2(672.0, 151.0);
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(effects_group),
                egui::Event::PointerButton {
                    pos: effects_group,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        for _ in 0..28 {
            frame(Vec::new());
        }
        let ([_, expanded_group, _], _, _) = frame(Vec::new());
        assert!(expanded_group.height() > inspector.height());
        assert!(viewport.contains_rect(expanded_group));

        let h = egui::Event::Key {
            key: egui::Key::H,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let (_, _, mid_offset) = frame(vec![h]);
        assert!(mid_offset.x > expanded_offset.x && mid_offset.x < 0.0);
        for _ in 0..28 {
            frame(Vec::new());
        }
        let ([_, closed_panel, _], over_collapsed_panel, closed_offset) = frame(vec![panel_point]);
        assert!(!closed_panel.is_positive());
        assert_eq!(closed_offset, Vec2::ZERO);
        assert!(launcher.height() <= 50.0);
        assert!(launcher.width() <= 302.0);
        assert!(!over_collapsed_panel);
        let ([next_launcher, _, _], over_launcher, _) =
            frame(vec![egui::Event::PointerMoved(launcher.center())]);
        assert!(
            over_launcher,
            "launcher moved from {launcher:?} to {next_launcher:?}"
        );

        let controls_button = egui::pos2(launcher.right() - 51.0, launcher.center().y);
        for pressed in [true, false] {
            frame(vec![egui::Event::PointerButton {
                pos: controls_button,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        for _ in 0..28 {
            frame(Vec::new());
        }
        let ([_, reopened, _], _, reopened_offset) = frame(Vec::new());
        assert!(reopened.is_positive());
        assert!((reopened.left() - inspector.left()).abs() < 1.0);
        assert!((reopened_offset.x - expanded_offset.x).abs() < 0.001);
    }
}
