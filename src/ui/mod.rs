pub mod loading;
pub mod preset;
mod probe;
mod sections;
mod style;

use bevy::prelude::*;
use bevy_egui::egui::{self, Align, Align2, Button, Layout, RichText, Ui};

use crate::camera::{OrbitCamera, OrbitInteraction, UiPointerActivity, WantsPointer};
use crate::params::BlackHoleParams;
use crate::scene::planets::PlanetSystemDirty;
use crate::ui::preset::{Preset, apply, canonical_hash, params_hash};
use crate::ui::style::{ACCENT, MUTED_TEXT, TEXT, panel_frame};

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
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            tab: ControlTab::Camera,
            collapsed: true,
            preset: Preset::Custom,
            probe: probe::ProbeLab::default(),
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
    mut wants: ResMut<WantsPointer>,
    mut ui_activity: ResMut<UiPointerActivity>,
    mut planet_dirty: ResMut<PlanetSystemDirty>,
    mut controls: Local<Controls>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        wants.0 = false;
        ui_activity.0 = false;
        return;
    };

    let planets_before = planet_configuration(&params);
    let rect = controls.show(ctx, &mut params, &mut camera, &mut orbit);
    if planets_before != planet_configuration(&params) {
        planet_dirty.0 = true;
    }

    ui_activity.0 = ctx.egui_is_using_pointer();
    wants.0 = ui_activity.0 || pointer_over_controls(ctx, rect);
}

fn planet_configuration(params: &BlackHoleParams) -> (bool, u32, f32, u32) {
    (
        params.planets_enabled,
        params.planet_count_target,
        params.planet_radius_factor,
        params.planet_seed,
    )
}

fn pointer_over_controls(ctx: &egui::Context, rect: egui::Rect) -> bool {
    // The actual frame bounds cover hover and presses. Popups can extend
    // beyond those bounds. The decorative shadow does not capture input.
    ctx.input(|input| {
        input
            .pointer
            .interact_pos()
            .is_some_and(|pos| rect.contains(pos))
    }) || (ctx.any_popup_open() && ctx.is_pointer_over_egui())
}

impl Controls {
    fn show(
        &mut self,
        ctx: &egui::Context,
        params: &mut BlackHoleParams,
        camera: &mut OrbitCamera,
        orbit: &mut OrbitInteraction,
    ) -> egui::Rect {
        if self.probe.is_open() {
            return self.probe.show(ctx, camera, params);
        }
        if !ctx.text_edit_focused() {
            let tab_available = !ctx.egui_wants_keyboard_input();
            let toggle = ctx.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::H)
                    || (tab_available && input.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
            });
            self.collapsed ^= toggle;
        }

        let viewport = ctx.content_rect();
        let narrow = viewport.width() <= 680.0;
        let inset = if narrow { 12.0 } else { 20.0 };
        if self.collapsed {
            let size = egui::vec2(218.0, 38.0);
            return egui::Area::new(egui::Id::new("controls_launcher"))
                .fixed_pos(egui::pos2(
                    viewport.right() - inset - size.x,
                    viewport.top() + inset,
                ))
                .default_size(size)
                .fade_in(false)
                .show(ctx, |ui| {
                    panel_frame(9).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            self.probe.launcher(ui, [114.0, 36.0]);
                            if ui
                                .add_sized([102.0, 36.0], Button::new("Controls").frame(false))
                                .on_hover_text("Show controls (H / Tab)")
                                .clicked()
                            {
                                self.collapsed = false;
                            }
                        });
                    });
                })
                .response
                .rect;
        }

        let width = if narrow {
            (viewport.width() - 2.0 * inset).clamp(0.0, 300.0)
        } else {
            (viewport.width() * 0.34).clamp(252.0, 300.0)
        };
        let (anchor, vertical_offset, height) = if narrow {
            (
                Align2::RIGHT_BOTTOM,
                -inset,
                (viewport.height() * 0.62).min(400.0),
            )
        } else {
            (Align2::RIGHT_TOP, inset, viewport.height() - 2.0 * inset)
        };
        let rect = egui::Area::new(egui::Id::new("controls_inspector"))
            .anchor(anchor, egui::vec2(-inset, vertical_offset))
            .default_size(egui::vec2(width, height.max(0.0)))
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_width(width);
                self.inspector(ui, height, params, camera, orbit);
            })
            .response
            .rect;
        if !ctx.text_edit_focused()
            && !ctx.any_popup_open()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.collapsed = true;
        }
        rect
    }

    fn inspector(
        &mut self,
        ui: &mut Ui,
        max_height: f32,
        params: &mut BlackHoleParams,
        camera: &mut OrbitCamera,
        orbit: &mut OrbitInteraction,
    ) {
        panel_frame(16).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 18,
                    right: 14,
                    top: 14,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_height(31.0);
                        ui.label(RichText::new("Controls").heading().strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_sized(
                                    [31.0, 31.0],
                                    Button::new(RichText::new("×").size(21.0).color(MUTED_TEXT))
                                        .frame(true)
                                        .frame_when_inactive(false),
                                )
                                .on_hover_text("Hide controls (H / Tab / Esc)")
                                .clicked()
                            {
                                self.collapsed = true;
                            }
                            self.probe.launcher(ui, [108.0, 31.0]);
                        });
                    });
                });
            self.tabs(ui);
            ui.separator();

            // Leave room for the fixed preset and reset controls at the bottom.
            let body_height = (max_height - ui.min_size().y - 53.0).max(18.0);
            let tab = self.tab;
            egui::ScrollArea::vertical()
                .id_salt(tab)
                .max_height(body_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(18, 12))
                        .show(ui, |ui| {
                            use crate::ui::sections::*;

                            ui.spacing_mut().item_spacing.y = 5.0;
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
        });
    }

    fn tabs(&mut self, ui: &mut Ui) {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let width = ui.available_width() / ControlTab::TABS.len() as f32;
                    for (tab, name) in ControlTab::TABS {
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
                        if response.clicked() {
                            self.tab = tab;
                        }
                        if selected {
                            ui.painter().hline(
                                response.rect.x_range(),
                                response.rect.bottom() - 1.0,
                                egui::Stroke::new(2.0, ACCENT),
                            );
                        }
                    }
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
            ..Default::default()
        };
        let mut camera = OrbitCamera::default();
        let mut orbit = OrbitInteraction::default();
        let mut params = BlackHoleParams::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 650.0));
        let mut frame = |events| {
            let input = egui::RawInput {
                screen_rect: Some(viewport),
                events,
                ..Default::default()
            };
            let mut result = None;
            let mut output = ctx.run_ui(input, |_| {
                let rect = controls.show(&ctx, &mut params, &mut camera, &mut orbit);
                result = Some((rect, pointer_over_controls(&ctx, rect)));
            });
            output.textures_delta.clear();
            result.unwrap()
        };
        let panel_point = egui::Event::PointerMoved(egui::pos2(981.0, 145.0));
        frame(Vec::new());
        let (inspector, over_inspector) = frame(vec![panel_point.clone()]);
        assert!(over_inspector);
        assert!(inspector.height() < viewport.height());

        let h = egui::Event::Key {
            key: egui::Key::H,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![h]);
        let (launcher, over_collapsed_panel) = frame(vec![panel_point]);
        assert!(launcher.height() <= 40.0);
        assert!(launcher.width() <= 220.0);
        assert!(!over_collapsed_panel);
        let (next_launcher, over_launcher) =
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
        let (reopened, _) = frame(Vec::new());
        assert_eq!(reopened, inspector);
    }
}
