mod scene;

use bevy_egui::egui::{self, Align, Align2, Layout, RichText, Ui, Vec2, vec2};

use super::lab::{LabLayout, TRANSITION_SECONDS, fit_distance, frame_scene, reveal};
use super::style::{ACCENT, BORDER, MUTED_TEXT, SURFACE, TEXT};
use crate::camera::{OrbitCamera, OrbitInteraction, SceneFraming};
use crate::params::{BlackHoleParams, KerrGeometry};
use scene::{CHAPTERS, Playback};

#[derive(Default)]
pub(super) struct Tour {
    session: Option<Playback>,
    panel_open: bool,
    previous_chapter: usize,
    changed_at: Option<f64>,
}

#[derive(Clone, Copy)]
enum Action {
    Chapter(usize),
    TogglePlayback,
    Scrub(f32),
    Exit,
}

// A stable hit area keeps a moving button under the pointer while its fill
// and inset respond to hover, keyboard focus and press.
fn button(ui: &mut Ui, label: &str, width: f32, enabled: bool) -> egui::Response {
    ui.add_enabled(enabled, |ui: &mut Ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 29.0), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        let hover = ui.ctx().animate_bool_with_time(
            response.id.with("hover"),
            response.hovered() || response.has_focus(),
            0.14,
        );
        let press = ui.ctx().animate_bool_with_time(
            response.id.with("press"),
            response.is_pointer_button_down_on(),
            0.09,
        );
        ui.painter().rect_filled(
            rect.shrink(press * 1.5),
            7,
            SURFACE.lerp_to_gamma(BORDER, hover),
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            egui::TextStyle::Button.resolve(ui.style()),
            MUTED_TEXT.lerp_to_gamma(TEXT, hover),
        );
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    })
}

impl Tour {
    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    pub fn launcher(
        &mut self,
        ui: &mut Ui,
        camera: &mut OrbitCamera,
        p: &BlackHoleParams,
        orbit: &mut OrbitInteraction,
        framing: SceneFraming,
    ) {
        if button(ui, "Guided tour", 81.0, true).clicked() {
            reveal(ui.ctx(), "tour_open", false);
            reveal(ui.ctx(), "tour_panel", false);
            *orbit = OrbitInteraction::default();
            self.session = Some(Playback::new(camera, p, framing));
            self.panel_open = true;
            self.changed_at = None;
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        camera: &mut OrbitCamera,
        p: &mut BlackHoleParams,
        geometry: KerrGeometry,
        orbit: &mut OrbitInteraction,
        framing: &mut SceneFraming,
    ) -> [egui::Rect; 3] {
        let Some(session) = &mut self.session else {
            return [egui::Rect::NOTHING; 3];
        };
        let dt = ctx.input(|i| i.stable_dt.min(0.1));
        let now = ctx.input(|i| i.time);
        if ctx.current_pass_index() == 0 && session.advance(dt, camera, p) {
            self.session = None;
            return [egui::Rect::NOTHING; 3];
        }
        let open = !session.returning;
        let mut action = None;
        if open && !ctx.text_edit_focused() && !ctx.any_popup_open() {
            let navigation_keys = !ctx.egui_wants_keyboard_input();
            ctx.input_mut(|i| {
                let mut key = |key| i.consume_key(egui::Modifiers::NONE, key);
                if key(egui::Key::Escape) {
                    action = Some(Action::Exit);
                } else if key(egui::Key::H) {
                    self.panel_open = !self.panel_open;
                } else if navigation_keys {
                    if session.chapter > 0 && key(egui::Key::ArrowLeft) {
                        action = Some(Action::Chapter(session.chapter - 1));
                    } else if session.chapter + 1 < CHAPTERS.len() && key(egui::Key::ArrowRight) {
                        action = Some(Action::Chapter(session.chapter + 1));
                    } else if key(egui::Key::Space) {
                        action = Some(Action::TogglePlayback);
                    }
                }
            });
        }

        let layout = LabLayout::new(ctx);
        let visibility = reveal(ctx, "tour_open", open);
        let toolbar = layout.toolbar(ctx, "tour_toolbar", visibility, open, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Guided tour");
                let toggle = if self.panel_open {
                    "Hide story"
                } else {
                    "Show story"
                };
                if button(ui, toggle, 79.0, open)
                    .on_hover_text("Toggle the chapter (H)")
                    .clicked()
                {
                    self.panel_open = !self.panel_open;
                }
                if button(ui, "Exit", 46.0, open)
                    .on_hover_text("Return to your view (Esc)")
                    .clicked()
                {
                    action = Some(Action::Exit);
                }
            });
        });
        let chapter = &CHAPTERS[session.chapter];
        let transport = layout.transport(ctx, "tour_transport", visibility, open, |ui| {
            let (name, reading) = chapter.reading(&framing.camera(camera), p, geometry);
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(MUTED_TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(reading).color(ACCENT));
                });
            });
            ui.horizontal(|ui| {
                let label = if session.playing {
                    "Pause"
                } else if session.progress >= 1.0 {
                    "Replay"
                } else {
                    "Play"
                };
                if button(ui, label, 58.0, open).clicked() {
                    action = Some(Action::TogglePlayback);
                }
                let mut position = session.slider_position();
                ui.spacing_mut().slider_width = ui.available_width();
                if ui
                    .add_enabled(
                        open,
                        egui::Slider::new(&mut position, 0.0..=1.0)
                            .show_value(false)
                            .clamping(egui::SliderClamping::Edits),
                    )
                    .on_hover_text(format!("Scrub the demonstration: {name}"))
                    .changed()
                {
                    action = Some(Action::Scrub(position));
                }
            });
        });

        let panel_open = open && self.panel_open;
        let panel_visibility = reveal(ctx, "tour_panel", panel_open);
        let panel = (panel_visibility > 0.0).then(|| {
            layout.inspector(
                ctx,
                "tour_inspector",
                &toolbar,
                Some(&transport),
                panel_visibility,
                visibility,
                panel_open,
                |ui, height| {
                    let height = (height - 22.0).clamp(0.0, 176.0);
                    ui.set_min_height(height);
                    ui.set_max_height(height);
                    if let Some(index) = navigation(ui, session.chapter, panel_open) {
                        action = Some(Action::Chapter(index));
                    }
                    ui.separator();
                    let (index, opacity) =
                        story_frame(session.chapter, self.previous_chapter, self.changed_at, now);
                    egui::ScrollArea::vertical()
                        .id_salt(("tour_text", index))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.multiply_opacity(opacity);
                            ui.label(
                                RichText::new(format!(
                                    "Chapter {} of {}",
                                    index + 1,
                                    CHAPTERS.len()
                                ))
                                .small()
                                .color(ACCENT),
                            );
                            ui.heading(CHAPTERS[index].title);
                            ui.label(CHAPTERS[index].text);
                        });
                },
            )
        });

        if let Some(action) = action {
            match action {
                Action::Exit => {
                    *orbit = OrbitInteraction::default();
                    session.exit(camera, p);
                }
                Action::Chapter(index) => {
                    *orbit = OrbitInteraction::default();
                    self.previous_chapter =
                        story_frame(session.chapter, self.previous_chapter, self.changed_at, now).0;
                    self.changed_at = Some(now);
                    session.begin(index, camera, p);
                }
                Action::TogglePlayback if session.progress >= 1.0 => {
                    *orbit = OrbitInteraction::default();
                    session.begin(session.chapter, camera, p);
                }
                Action::TogglePlayback => session.playing = !session.playing,
                Action::Scrub(progress) => {
                    *orbit = OrbitInteraction::default();
                    session.scrub(progress, camera);
                    session.advance(0.0, camera, p);
                }
            }
        }
        if session.playing
            || session.returning
            || self
                .changed_at
                .is_some_and(|t| now - t < f64::from(TRANSITION_SECONDS))
        {
            ctx.request_repaint();
        }
        let target = if session.returning {
            session.original_framing
        } else if let Some(surface) = &panel
            && panel_open
        {
            let canvas = ctx.viewport_rect();
            let region = layout.scene_region(&toolbar, Some(&transport), Some(surface.settled));
            // Fit against a fixed reference shot so the framing never cancels a dolly or a zoom.
            let scale = fit_distance(
                canvas,
                region,
                region.center(),
                &OrbitCamera::default(),
                4.0,
            );
            frame_scene(canvas, region.center(), scale)
        } else {
            SceneFraming::default()
        };
        if ctx.current_pass_index() == 0 {
            framing.approach(target, dt);
        }
        if session.returning {
            [egui::Rect::NOTHING; 3]
        } else {
            [
                toolbar.rect,
                transport.rect,
                panel
                    .filter(|_| panel_open)
                    .map_or(egui::Rect::NOTHING, |s| s.rect),
            ]
        }
    }
}

fn story_frame(current: usize, previous: usize, changed_at: Option<f64>, now: f64) -> (usize, f32) {
    let t = changed_at.map_or(1.0, |start| {
        ((now - start) as f32 / TRANSITION_SECONDS).clamp(0.0, 1.0)
    });
    if t < 0.5 {
        (previous, 1.0 - 2.0 * t)
    } else {
        (current, 2.0 * t - 1.0)
    }
}

fn navigation(ui: &mut Ui, current: usize, enabled: bool) -> Option<usize> {
    let mut selected = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        if button(ui, "←", 29.0, enabled && current > 0)
            .on_hover_text("Previous chapter")
            .clicked()
        {
            selected = Some(current - 1);
        }
        let gap = ui.spacing().item_spacing.x;
        let width = (ui.available_width() - 29.0) / CHAPTERS.len() as f32 - gap;
        let highlight = ui.ctx().animate_value_with_time(
            ui.id().with("chapter_indicator"),
            current as f32,
            TRANSITION_SECONDS,
        );
        let mut first = egui::Rect::NOTHING;
        for (i, chapter) in CHAPTERS.iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(vec2(width.max(1.0), 29.0), egui::Sense::click());
            if i == 0 {
                first = rect;
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    enabled,
                    current == i,
                    chapter.title,
                )
            });
            let hover = ui.ctx().animate_bool_with_time(
                response.id.with("hover"),
                response.hovered() || response.has_focus(),
                0.14,
            );
            let passed = (highlight - i as f32).clamp(0.0, 1.0);
            let color = BORDER.lerp_to_gamma(ACCENT, 0.48 * passed + 0.45 * hover);
            let rail =
                egui::Rect::from_center_size(rect.center(), vec2(rect.width(), 3.0 + 2.0 * hover));
            ui.painter().rect_filled(rail, 3, color);
            if response
                .on_hover_text(format!("{}. {}", i + 1, chapter.title))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                && enabled
                && i != current
            {
                selected = Some(i);
            }
        }
        let center = first.center() + Vec2::X * (first.width() + gap) * highlight;
        ui.painter().rect_filled(
            egui::Rect::from_center_size(center, vec2(first.width(), 4.0)),
            2,
            ACCENT,
        );
        if button(ui, "→", 29.0, enabled && current + 1 < CHAPTERS.len())
            .on_hover_text("Next chapter")
            .clicked()
        {
            selected = Some(current + 1);
        }
    });
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Controls, pointer_over_controls, style};

    struct Window {
        ctx: egui::Context,
        viewport: egui::Rect,
        controls: Controls,
        camera: OrbitCamera,
        orbit: OrbitInteraction,
        params: BlackHoleParams,
        framing: SceneFraming,
        time: f64,
    }

    impl Window {
        fn new(size: Vec2) -> Self {
            let ctx = egui::Context::default();
            style::setup(&ctx);
            Self {
                ctx,
                viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, size),
                controls: Controls::default(),
                camera: OrbitCamera::default(),
                orbit: OrbitInteraction::default(),
                params: BlackHoleParams::default(),
                framing: SceneFraming::default(),
                time: 0.0,
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> [egui::Rect; 3] {
            self.time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(self.viewport),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let mut bounds = [egui::Rect::NOTHING; 3];
            let mut output = self.ctx.run_ui(input, |_| {
                let geometry = KerrGeometry::from_spin(self.params.spin);
                bounds = self.controls.show(
                    &self.ctx,
                    &mut self.params,
                    geometry,
                    &mut self.camera,
                    &mut self.orbit,
                    &mut self.framing,
                );
            });
            output.textures_delta.clear();
            bounds
        }

        fn wait(&mut self, frames: usize) -> [egui::Rect; 3] {
            for _ in 1..frames {
                self.frame(vec![]);
            }
            self.frame(vec![])
        }

        fn click(&mut self, pos: egui::Pos2) {
            self.frame(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                self.frame(vec![egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
        }

        fn key(&mut self, key: egui::Key) {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }

    #[test]
    fn tour_navigation_and_camera_framing_work_from_the_launcher_at_both_widths() {
        for size in [vec2(1024.0, 650.0), vec2(320.0, 560.0)] {
            let mut w = Window::new(size);
            let launcher = w.wait(24)[0];
            assert!(w.viewport.contains_rect(launcher));
            w.click(egui::pos2(launcher.left() + 46.0, launcher.center().y));
            assert!(w.controls.tour.is_active());
            w.wait(5);
            let intermediate = w.framing.offset;
            let [toolbar, transport, panel] = w.wait(40);
            for rect in [toolbar, transport, panel] {
                assert!(
                    rect.is_positive() && w.viewport.contains_rect(rect),
                    "{rect:?} in {:?}",
                    w.viewport
                );
            }
            assert!(toolbar.bottom() < panel.top() && panel.bottom() < transport.top());
            let axis = usize::from(size.x < 700.0);
            assert!(w.framing.offset[axis] < intermediate[axis] && intermediate[axis] < 0.0);
            let rects = w.frame(vec![egui::Event::PointerMoved(panel.center())]);
            assert!(pointer_over_controls(&w.ctx, &rects));
            let scene = w.viewport.center()
                + vec2(w.framing.offset.x * size.x, w.framing.offset.y * size.y);
            let rects = w.frame(vec![egui::Event::PointerMoved(scene)]);
            assert!(!pointer_over_controls(&w.ctx, &rects));

            let nav_y = panel.top() + 25.0;
            w.click(egui::pos2(panel.right() - 55.0, nav_y));
            assert_eq!(w.controls.tour.session.as_ref().unwrap().chapter, 8);
            w.click(egui::pos2(panel.right() - 25.0, nav_y));
            assert_eq!(w.controls.tour.session.as_ref().unwrap().chapter, 8);
            w.click(egui::pos2(panel.left() + 25.0, nav_y));
            assert_eq!(w.controls.tour.session.as_ref().unwrap().chapter, 7);
            w.click(egui::pos2(panel.right() - 25.0, nav_y));
            assert_eq!(w.controls.tour.session.as_ref().unwrap().chapter, 8);
            w.key(egui::Key::H);
            let [_, bar, hidden] = w.wait(32);
            assert!(bar.is_positive() && !hidden.is_positive());
            w.click(egui::pos2(bar.center().x + 25.0, bar.bottom() - 23.0));
            let player = w.controls.tour.session.as_ref().unwrap();
            assert!(!player.playing && (0.0..1.0).contains(&player.progress));
            assert!(w.params.spin > 0.0 && w.params.spin < 0.95);
            w.key(egui::Key::Escape);
            let main = w.wait(60);
            assert!(!w.controls.tour.is_active());
            assert!(main[0].is_positive());
            assert_eq!(w.params.spin, 0.0);
            assert!(w.camera == OrbitCamera::default());
        }
    }
}
