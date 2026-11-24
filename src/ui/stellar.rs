//! Feed a star controls and shared playback across the active passes.

mod view;

use bevy_egui::egui::{self, Align, Align2, Button, Color32, Layout, RichText, Ui};

use crate::camera::{OrbitCamera, SceneFraming};
use crate::params::BlackHoleParams;
use crate::physics::stellar::{CRITICAL_RADIUS, Experiment, Input, Outcome, Pass};
use crate::ui::sections::slider;
use crate::ui::style::{ACCENT, MUTED_TEXT, PANEL_FILL, panel_frame};
use crate::ui::trajectory::clock;

const TRANSITION_SECONDS: f32 = 0.24;
const COLORS: [Color32; 4] = [
    Color32::from_rgb(108, 193, 249),
    Color32::from_rgb(130, 223, 159),
    Color32::from_rgb(255, 191, 109),
    Color32::from_rgb(242, 127, 142),
];
const RATES: [(f64, &str); 4] = [(0.25, "¼×"), (0.5, "½×"), (1.0, "1×"), (2.0, "2×")];
const TIDAL_RING: Color32 = Color32::from_rgb(176, 137, 222);

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
enum Mode {
    #[default]
    Comparison,
    Custom,
}

#[derive(Clone, Copy)]
enum Phase {
    Approach,
    Encounter,
    After,
}

impl Phase {
    fn time(self, pass: &Pass) -> Option<f64> {
        match self {
            Self::Approach => pass.events.approach,
            Self::Encounter => pass.events.encounter.map(|sample| sample.coordinate_time),
            Self::After => pass.events.after,
        }
    }
}

struct Playback {
    time: f64,
    playing: bool,
    rate: usize,
    jump: Option<TimeJump>,
}

struct TimeJump {
    from: f64,
    to: f64,
    elapsed: f64,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            time: 0.0,
            playing: false,
            rate: 2,
            jump: None,
        }
    }
}

impl Playback {
    fn advance(&mut self, dt: f64, duration: f64) {
        if let Some(jump) = &mut self.jump {
            jump.elapsed += dt;
            let t = (jump.elapsed / f64::from(TRANSITION_SECONDS)).min(1.0);
            self.time = jump.from + (jump.to - jump.from) * (t * t * (3.0 - 2.0 * t));
            if t >= 1.0 {
                self.time = jump.to;
                self.jump = None;
            }
            return;
        }
        if self.playing {
            self.time =
                (self.time + dt.min(0.1) * duration / 18.0 * RATES[self.rate].0).min(duration);
            self.playing = self.time < duration;
        }
    }

    fn seek(&mut self, time: f64) {
        self.time = time;
        self.playing = false;
        self.jump = None;
    }

    fn jump_to(&mut self, time: f64, play_after: bool) {
        self.playing = play_after;
        self.jump = (self.time != time).then_some(TimeJump {
            from: self.time,
            to: time,
            elapsed: 0.0,
        });
    }
}

struct Settings {
    mode: Mode,
    draft: Input,
    visible: [bool; 4],
    focus: Option<usize>,
    overlay: bool,
    zoom: f32,
    replay: Playback,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Comparison,
            draft: Input::default(),
            visible: [true; 4],
            focus: None,
            overlay: true,
            zoom: 1.0,
            replay: Playback::default(),
        }
    }
}

impl Settings {
    fn shows(&self, index: usize) -> bool {
        self.mode == Mode::Custom
            || (self.visible[index] && self.focus.is_none_or(|focus| index == focus))
    }

    fn toggle_focus(&mut self, index: usize) {
        self.focus = (self.focus != Some(index)).then_some(index);
        self.visible[index] = true;
    }

    fn reference(&self, experiment: &Experiment) -> usize {
        // Use the disruption as the reference in the comparison until a different
        // visible pass is selected. Visibility never changes the replay duration.
        let preferred = self.focus.unwrap_or(2).min(experiment.passes.len() - 1);
        if self.shows(preferred) {
            preferred
        } else {
            (0..experiment.passes.len())
                .find(|&index| self.shows(index))
                .unwrap_or(preferred)
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.zoom = 1.0;
            self.replay.seek(0.0);
        }
    }
}

struct Experiments {
    comparison: Experiment,
    custom: Experiment,
    settings: Settings,
}

impl Default for Experiments {
    fn default() -> Self {
        Self {
            comparison: Experiment::four_passes(),
            custom: Experiment::custom(Input::default()).expect("valid default stellar input"),
            settings: Settings::default(),
        }
    }
}

#[derive(Default)]
pub(super) struct StellarLab {
    open: bool,
    reveal: f32,
    experiments: Option<Experiments>,
    fit_pending: bool,
    distance_scale: f32,
    desktop_hidden: bool,
    compact_panel: Option<Panel>,
    last_panel: Option<Panel>,
    content_changed_at: Option<f64>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Panel {
    Desktop,
    Controls,
    Map,
}

fn reveal(ctx: &egui::Context, id: &'static str, shown: bool) -> f32 {
    let t = ctx.animate_bool_with_time(egui::Id::new(id), shown, TRANSITION_SECONDS);
    t * t * (3.0 - 2.0 * t)
}

impl StellarLab {
    pub(super) fn is_active(&self) -> bool {
        self.open || self.reveal > 0.0
    }

    pub(super) fn launcher(&mut self, ui: &mut Ui, size: [f32; 2]) {
        if ui
            .add_sized(
                size,
                Button::new(RichText::new("Feed a star").color(ACCENT)).frame(false),
            )
            .clicked()
        {
            self.experiments.get_or_insert_with(Experiments::default);
            reveal(ui.ctx(), "stellar_open", false);
            reveal(ui.ctx(), "stellar_panel", false);
            reveal(ui.ctx(), "stellar_overlay", false);
            self.open = true;
            self.fit_pending = true;
        }
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        camera: &OrbitCamera,
        params: &BlackHoleParams,
        framing: &mut SceneFraming,
    ) -> [egui::Rect; 3] {
        let viewport = ctx.content_rect();
        let compact = viewport.width() <= 700.0;
        let inset = if compact { 8.0 } else { 16.0 };
        if self.open && !ctx.text_edit_focused() && !ctx.any_popup_open() {
            ctx.input_mut(|input| {
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    self.open = false;
                } else if input.consume_key(egui::Modifiers::NONE, egui::Key::H) {
                    self.toggle_controls(compact);
                }
            });
        }

        self.reveal = reveal(ctx, "stellar_open", self.open);
        let dt = ctx.input(|input| input.stable_dt);
        if self.reveal <= 0.0 {
            framing.approach(SceneFraming::default(), dt);
            if !self.open {
                return [egui::Rect::NOTHING; 3];
            }
        }

        let toolbar_slide = egui::vec2(0.0, -24.0 * (1.0 - self.reveal));
        let toolbar = egui::Area::new(egui::Id::new("stellar_toolbar"))
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(viewport.right_top() + egui::vec2(-inset, inset) + toolbar_slide)
            .constrain(false)
            .fade_in(false)
            .interactable(self.open)
            .show(ctx, |ui| {
                ui.set_clip_rect(viewport);
                ui.multiply_opacity(self.reveal);
                panel_frame(9).inner_margin(7).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Feed a star");
                        if compact {
                            if choice(ui, self.compact_panel == Some(Panel::Controls), "Controls")
                                .clicked()
                            {
                                self.toggle_controls(compact);
                            }
                            if choice(ui, self.compact_panel == Some(Panel::Map), "Orbit map")
                                .clicked()
                            {
                                self.compact_panel =
                                    (self.compact_panel != Some(Panel::Map)).then_some(Panel::Map);
                            }
                        } else if ui
                            .button(if self.desktop_hidden {
                                "Show controls"
                            } else {
                                "Hide controls"
                            })
                            .on_hover_text("Toggle experiment controls (H)")
                            .clicked()
                        {
                            self.toggle_controls(compact);
                        }
                        if ui.button("Close").on_hover_text("Close (Esc)").clicked() {
                            self.open = false;
                        }
                    });
                });
            })
            .response
            .rect;

        let requested_panel = if compact {
            self.compact_panel
        } else {
            (!self.desktop_hidden).then_some(Panel::Desktop)
        };
        let panel_reveal = reveal(ctx, "stellar_panel", self.open && requested_panel.is_some());
        let now = ctx.input(|input| input.time);
        if let Some(panel) = requested_panel
            && self.last_panel != Some(panel)
        {
            if self.last_panel.is_some() && panel_reveal >= 1.0 {
                self.content_changed_at = Some(now);
            }
            self.last_panel = Some(panel);
        }

        let Experiments {
            comparison,
            custom,
            settings,
        } = self.experiments.as_mut().expect("active stellar lab");
        let experiment = match settings.mode {
            Mode::Comparison => &*comparison,
            Mode::Custom => &*custom,
        };
        if self.open {
            if ctx.current_pass_index() == 0 {
                settings.replay.advance(f64::from(dt), experiment.duration);
            }
        } else {
            settings.replay.seek(settings.replay.time);
        }

        let transport_slide = egui::vec2(0.0, 32.0 * (1.0 - self.reveal));
        let transport_width = (viewport.width() - 2.0 * inset).max(0.0);
        let transport = egui::Area::new(egui::Id::new("stellar_transport"))
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(viewport.left_bottom() + egui::vec2(inset, -inset) + transport_slide)
            .default_size(egui::vec2(transport_width, 82.0))
            .constrain(false)
            .fade_in(false)
            .interactable(self.open)
            .show(ctx, |ui| {
                ui.set_clip_rect(viewport);
                ui.set_width(transport_width);
                ui.multiply_opacity(self.reveal);
                panel_frame(12).inner_margin(8).show(ui, |ui| {
                    replay(ui, settings, experiment, &mut self.fit_pending);
                });
            })
            .response
            .rect;

        let top = toolbar.bottom() - toolbar_slide.y + 8.0;
        let bottom = transport.top() - transport_slide.y - 8.0;
        let mut panel_bounds = egui::Rect::NOTHING;
        let mut settled_panel = None;
        if let Some(panel) = self.last_panel
            && panel_reveal > 0.0
        {
            let width = transport_width.min(if compact { 480.0 } else { 376.0 });
            let available_height = (bottom - top).max(0.0);
            let (pivot, y, height, slide) = if compact {
                (
                    Align2::RIGHT_BOTTOM,
                    bottom,
                    (available_height * 0.55).min(370.0),
                    egui::vec2(0.0, 26.0 * (1.0 - panel_reveal)),
                )
            } else {
                (
                    Align2::RIGHT_TOP,
                    top,
                    available_height,
                    egui::vec2(28.0 * (1.0 - panel_reveal), 0.0),
                )
            };
            let mut clip = viewport;
            if compact {
                clip.max.y = clip.max.y.min(transport.top() - 1.0);
            }
            panel_bounds = egui::Area::new(egui::Id::new("stellar_inspector"))
                .pivot(pivot)
                .fixed_pos(egui::pos2(viewport.right() - inset, y) + slide)
                .default_size(egui::vec2(width, height))
                .constrain(false)
                .fade_in(false)
                .interactable(self.open && requested_panel.is_some())
                .show(ctx, |ui| {
                    ui.set_clip_rect(clip);
                    ui.set_width(width);
                    ui.set_max_height(height);
                    ui.multiply_opacity(panel_reveal * self.reveal);
                    panel_frame(12).inner_margin(10).show(ui, |ui| {
                        inspector(
                            ui,
                            panel,
                            height,
                            comparison,
                            custom,
                            settings,
                            &mut self.content_changed_at,
                        );
                    });
                })
                .response
                .rect;
            if self.open && requested_panel.is_some() {
                settled_panel = Some(panel_bounds.translate(-slide));
            }
        }

        let experiment = match settings.mode {
            Mode::Comparison => &*comparison,
            Mode::Custom => &*custom,
        };
        let canvas = ctx.viewport_rect();
        let region = scene_region(viewport, settled_panel, top, bottom, compact);
        let target = if self.open {
            let center = if settled_panel.is_some() {
                region.center()
            } else {
                canvas.center()
            };
            if self.fit_pending && (requested_panel.is_none() || settled_panel.is_some()) {
                self.fit_pending = false;
                self.distance_scale =
                    fit_distance(canvas, region, center, camera, params, experiment);
            }
            let offset = (center - canvas.center()) / canvas.size().max(egui::Vec2::splat(1.0));
            SceneFraming {
                offset: bevy::math::Vec2::new(offset.x, offset.y),
                distance_scale: self.distance_scale.max(1.0),
            }
        } else {
            SceneFraming::default()
        };
        if ctx.current_pass_index() == 0 {
            framing.approach(target, dt);
        }
        let overlay = reveal(ctx, "stellar_overlay", self.open && settings.overlay)
            * content_opacity(now, self.content_changed_at);
        if overlay > 0.0 {
            view::scene(ctx, camera, params, settings, experiment, *framing, overlay);
        }
        if self.open {
            [
                toolbar,
                transport,
                if requested_panel.is_some() {
                    panel_bounds
                } else {
                    egui::Rect::NOTHING
                },
            ]
        } else {
            [egui::Rect::NOTHING; 3]
        }
    }

    fn toggle_controls(&mut self, compact: bool) {
        if compact {
            self.compact_panel =
                (self.compact_panel != Some(Panel::Controls)).then_some(Panel::Controls);
        } else {
            self.desktop_hidden ^= true;
        }
    }
}

fn scene_region(
    mut region: egui::Rect,
    panel: Option<egui::Rect>,
    top: f32,
    bottom: f32,
    compact: bool,
) -> egui::Rect {
    const GAP: f32 = 16.0;
    region.max.y = (bottom - 8.0).clamp(region.top(), region.bottom());
    if compact {
        region.min.y = top.clamp(region.top(), region.bottom());
    }
    if let Some(panel) = panel {
        if compact {
            region.max.y = (panel.top() - GAP).clamp(region.top(), region.bottom());
        } else {
            region.max.x = (panel.left() - GAP).clamp(region.left(), region.right());
        }
    }
    region
}

fn fit_distance(
    canvas: egui::Rect,
    region: egui::Rect,
    center: egui::Pos2,
    camera: &OrbitCamera,
    params: &BlackHoleParams,
    experiment: &Experiment,
) -> f32 {
    let inner_disk = (crate::physics::kerr_isco(params.spin) + 1.5).max(4.0);
    // Frame the inner disk and the local encounter. Distant custom passes
    // remain available in the orbit map without making the black hole tiny.
    let encounter = experiment
        .passes
        .iter()
        .flat_map(|pass| [pass.events.encounter, pass.events.disruption])
        .flatten()
        .map(|sample| sample.radius as f32)
        .fold(inner_disk, f32::max)
        .min(12.0);
    let aspect = canvas.width().max(1.0) / canvas.height().max(1.0);
    let focal = canvas.height() * 0.5 / (camera.vertical_fov(aspect) * 0.5).tan();
    let clearance = (center - region.min).min(region.max - center).min_elem();
    let half_extent = (clearance * 0.9).max(1.0);
    // Distance at which a sphere of this radius subtends the available angle.
    let distance = encounter * params.rs * (focal / half_extent).hypot(1.0);
    (distance / camera.distance).max(1.0)
}

fn inspector(
    ui: &mut Ui,
    panel: Panel,
    height: f32,
    comparison: &Experiment,
    custom: &mut Experiment,
    settings: &mut Settings,
    changed_at: &mut Option<f64>,
) {
    let now = ui.input(|input| input.time);
    egui::ScrollArea::vertical()
        .id_salt(("stellar_inspector_content", panel, settings.mode))
        .auto_shrink([false, true])
        .max_height((height - 22.0).max(0.0))
        .min_scrolled_height(0.0)
        .show(ui, |ui| {
            if panel != Panel::Map {
                ui.horizontal_wrapped(|ui| {
                    for (mode, name) in [
                        (Mode::Comparison, "Four passes"),
                        (Mode::Custom, "Your experiment"),
                    ] {
                        if choice(ui, settings.mode == mode, name).clicked()
                            && settings.mode != mode
                        {
                            settings.set_mode(mode);
                            *changed_at = Some(now);
                        }
                    }
                });
                if settings.mode == Mode::Custom && custom_summary(ui, custom, settings) {
                    *changed_at = Some(now);
                }
            }
            let opacity = content_opacity(now, *changed_at);
            ui.multiply_opacity(opacity);
            if opacity < 1.0 {
                ui.ctx().request_repaint();
            }
            if panel != Panel::Controls {
                let experiment = match settings.mode {
                    Mode::Comparison => comparison,
                    Mode::Custom => &*custom,
                };
                view::map(ui, settings, experiment, (height * 0.52).clamp(65.0, 210.0));
            }
            if panel != Panel::Map {
                ui.separator();
                if settings.mode == Mode::Comparison {
                    comparison_controls(ui, comparison, settings);
                } else {
                    custom_controls(ui, &mut settings.draft);
                }
                ui.separator();
                note(
                    ui,
                    "Overlay uses the current scene. Stars are enlarged and debris paths are simplified.",
                );
            }
        });
}

fn content_opacity(now: f64, changed_at: Option<f64>) -> f32 {
    changed_at.map_or(1.0, |start| {
        let t = ((now - start) as f32 / TRANSITION_SECONDS).clamp(0.0, 1.0);
        0.45 + 0.55 * t
    })
}

fn choice(ui: &mut Ui, selected: bool, label: &str) -> egui::Response {
    let text = RichText::new(label);
    ui.selectable_label(
        selected,
        if selected {
            text.color(PANEL_FILL).strong()
        } else {
            text
        },
    )
}

fn note(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).small().color(MUTED_TEXT));
}

fn replay(ui: &mut Ui, settings: &mut Settings, experiment: &Experiment, fit_pending: &mut bool) {
    let reference = settings.reference(experiment);
    let pass = &experiment.passes[reference];
    let duration = experiment.duration;
    let mut controls = |ui: &mut Ui, settings: &mut Settings| {
        ui.add_enabled_ui(duration > 0.0, |ui| {
            let replay = &mut settings.replay;
            if ui
                .button(if replay.playing { "Pause" } else { "Play" })
                .clicked()
            {
                if replay.time >= duration && replay.jump.is_none() {
                    replay.jump_to(0.0, true);
                } else {
                    replay.playing = !replay.playing;
                }
            }
            if ui.button("Replay").clicked() {
                replay.jump_to(0.0, true);
            }
            egui::ComboBox::from_id_salt("stellar_rate")
                .width(38.0)
                .selected_text(RATES[replay.rate].1)
                .show_ui(ui, |ui| {
                    for (index, (_, name)) in RATES.iter().enumerate() {
                        if choice(ui, replay.rate == index, name).clicked() {
                            replay.rate = index;
                        }
                    }
                });
        });
        let seconds = experiment.input.seconds(settings.replay.time);
        let elapsed = if seconds > 0.0 {
            clock(seconds)
        } else {
            "0.00 s".into()
        };
        ui.label(RichText::new(elapsed).monospace()).on_hover_text(
            "Elapsed Schwarzschild coordinate time, as measured far from the black hole",
        );
        *fit_pending |= ui
            .button("Fit scene")
            .on_hover_text("Frame the inner disk and encounter")
            .clicked();
    };
    if ui.available_width() > 600.0 {
        ui.horizontal(|ui| {
            controls(ui, settings);
            timeline(ui, &mut settings.replay, duration);
        });
    } else {
        ui.horizontal_wrapped(|ui| controls(ui, settings));
        timeline(ui, &mut settings.replay, duration);
    }
    ui.horizontal_wrapped(|ui| {
        let encounter = match pass.outcome {
            Outcome::DirectCapture => "Horizon limit",
            Outcome::Critical => "Critical orbit",
            _ => "Closest point",
        };
        for (phase, label) in [
            (Phase::Approach, "Approach"),
            (Phase::Encounter, encounter),
            (Phase::After, "After encounter"),
        ] {
            let time = phase.time(pass);
            if ui
                .add_enabled(time.is_some(), Button::new(label))
                .on_hover_text(outcome_label(pass.outcome))
                .clicked()
                && let Some(time) = time
            {
                settings.replay.jump_to(time, false);
            }
        }
    });
}

fn timeline(ui: &mut Ui, replay: &mut Playback, duration: f64) {
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = ui.available_width();
        let mut time = replay.time;
        if ui
            .add_enabled(
                duration > 0.0,
                egui::Slider::new(&mut time, 0.0..=duration)
                    .clamping(egui::SliderClamping::Edits)
                    .show_value(false),
            )
            .on_hover_text("Shared coordinate time")
            .changed()
        {
            replay.seek(time);
        }
    });
}

fn comparison_controls(ui: &mut Ui, experiment: &Experiment, settings: &mut Settings) {
    note(
        ui,
        format!(
            "10 million M☉ Schwarzschild black hole · Sun-like star\nShared start at {:.1} r_s and {:.2} c. Only the entry direction changes.",
            experiment.launch_radius,
            experiment.launch_speed(),
        ),
    );
    for (index, pass) in experiment.passes.iter().enumerate() {
        ui.push_id(index, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut settings.visible[index],
                    RichText::new(outcome_label(pass.outcome)).color(COLORS[index]),
                )
                .on_hover_text(pass_detail(pass));
                if !settings.visible[index] && settings.focus == Some(index) {
                    settings.focus = None;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let focused = settings.focus == Some(index);
                    if choice(ui, focused, if focused { "Unfocus" } else { "Focus" })
                        .on_hover_text("Focus this pass in both views and use its phase times")
                        .clicked()
                    {
                        settings.toggle_focus(index);
                    }
                });
            });
        });
    }
}

fn custom_summary(ui: &mut Ui, experiment: &mut Experiment, settings: &mut Settings) -> bool {
    let dirty = settings.draft != experiment.input;
    let applied = ui
        .horizontal(|ui| {
            ui.strong(RichText::new(outcome_label(experiment.passes[0].outcome)).color(COLORS[2]));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        dirty && settings.draft.valid(),
                        Button::new("Apply parameters").frame(true),
                    )
                    .clicked()
                    && let Some(applied) = Experiment::custom(settings.draft)
                {
                    *experiment = applied;
                    settings.zoom = 1.0;
                    settings.replay.seek(0.0);
                    true
                } else {
                    false
                }
            })
            .inner
        })
        .inner;
    note(ui, pass_detail(&experiment.passes[0]));
    applied
}

fn custom_controls(ui: &mut Ui, draft: &mut Input) {
    // Use log mass for a usable slider across five orders of magnitude.
    slider(
        ui,
        "Black-hole mass",
        &mut draft.black_hole_mass,
        Input::BLACK_HOLE_MASS,
        0,
        " M☉",
        true,
    );
    slider(
        ui,
        "Stellar mass",
        &mut draft.star_mass,
        Input::STAR_MASS,
        2,
        " M☉",
        true,
    );
    slider(
        ui,
        "Stellar radius",
        &mut draft.star_radius,
        Input::STAR_RADIUS,
        2,
        " R☉",
        true,
    );
    slider(
        ui,
        "Encounter depth β",
        &mut draft.beta,
        Input::BETA,
        3,
        "",
        true,
    );
    if draft.valid() {
        let target = draft.tidal_radius() / draft.beta;
        note(
            ui,
            format!(
                "β = tidal radius / target radius · {:.3} r_s target{}",
                target,
                if target <= CRITICAL_RADIUS {
                    " (nominal)"
                } else {
                    ""
                }
            ),
        );
    }
}

fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::DistantPass => "Distant flyby",
        Outcome::GrazingPass => "Grazing flyby",
        Outcome::Disrupted => "Tidal disruption",
        Outcome::DirectCapture => "Direct capture",
        Outcome::Critical => "Critical orbit",
        Outcome::Unresolved => "Trajectory unresolved",
    }
}

fn pass_detail(pass: &Pass) -> String {
    match pass.outcome {
        Outcome::DirectCapture => format!(
            "Nominal target {:.3} r_s · Falls to the horizon limit",
            pass.target_radius
        ),
        Outcome::Critical => {
            "Nominal depth · Approaches the marginally bound orbit at 2 r_s".into()
        }
        Outcome::Unresolved => "The solver did not resolve this encounter.".into(),
        _ => {
            let radius = pass
                .events
                .encounter
                .map_or(pass.target_radius, |sample| sample.radius);
            let details = match pass.outcome {
                Outcome::Disrupted if pass.debris_unresolved() => " · Debris unresolved",
                Outcome::Disrupted => " · 48 debris particles",
                _ => "",
            };
            format!("Closest point {radius:.3} r_s{details}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_focus_and_jumps_keep_one_clock_for_every_pass() {
        let experiment = Experiment::four_passes();
        let mut settings = Settings::default();
        let encounter = Phase::Encounter
            .time(&experiment.passes[settings.reference(&experiment)])
            .unwrap();
        settings.replay.playing = true;
        settings.replay.seek(encounter);
        settings.replay.advance(0.05, experiment.duration);
        assert!((settings.replay.time - encounter).abs() < 1e-12);
        assert!(!settings.replay.playing);

        settings.visible[2] = false;
        assert!(!settings.shows(2));
        settings.toggle_focus(0);
        assert_eq!(settings.reference(&experiment), 0);
        assert!(!settings.shows(1));
        assert!((settings.replay.time - encounter).abs() < 1e-12);

        let after = Phase::After.time(&experiment.passes[0]).unwrap();
        settings.replay.seek(after);
        settings.toggle_focus(0);
        settings.visible[2] = true;
        assert!(settings.shows(1) && settings.shows(2));
        for pass in &experiment.passes {
            let path = pass.path.as_ref().unwrap();
            let sample = path.at(settings.replay.time);
            assert!((sample.coordinate_time - after.min(path.end().coordinate_time)).abs() < 1e-12);
        }

        settings.replay.playing = true;
        settings.replay.advance(0.05, experiment.duration);
        assert!(settings.replay.time > after);
    }

    #[test]
    fn transport_and_drawers_leave_room_for_the_scene() {
        let context = egui::Context::default();
        crate::ui::style::setup(&context);
        let mut lab = StellarLab {
            open: true,
            experiments: Some(Experiments::default()),
            fit_pending: true,
            ..Default::default()
        };
        let mut framing = SceneFraming::default();
        let mut now = 0.0;
        for size in [
            egui::vec2(320.0, 560.0),
            egui::vec2(360.0, 680.0),
            egui::vec2(690.0, 360.0),
            egui::vec2(1120.0, 720.0),
        ] {
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            for mode in [Mode::Comparison, Mode::Custom] {
                lab.experiments.as_mut().unwrap().settings.set_mode(mode);
                for panel in [None, Some(Panel::Controls), Some(Panel::Map)] {
                    lab.compact_panel = panel;
                    lab.desktop_hidden = panel.is_none();
                    let mut bounds = [egui::Rect::NOTHING; 3];
                    for _ in 0..28 {
                        now += 1.0 / 60.0;
                        let mut output = context.run_ui(
                            egui::RawInput {
                                screen_rect: Some(viewport),
                                time: Some(now),
                                ..Default::default()
                            },
                            |_| {
                                bounds = lab.show(
                                    &context,
                                    &OrbitCamera::default(),
                                    &BlackHoleParams::default(),
                                    &mut framing,
                                );
                            },
                        );
                        output.textures_delta.clear();
                    }
                    let [toolbar, transport, drawer] = bounds;
                    assert!(viewport.contains_rect(toolbar));
                    assert!(viewport.contains_rect(transport));
                    if panel.is_none() {
                        assert!(!drawer.is_positive());
                        assert_eq!(framing.offset, bevy::math::Vec2::ZERO);
                    } else {
                        assert!(viewport.contains_rect(drawer), "{drawer:?} in {viewport:?}");
                        assert!(toolbar.bottom() < drawer.top());
                        assert!(drawer.bottom() < transport.top());
                        if size.x <= 700.0 {
                            assert!(framing.offset.y < -0.05);
                        } else {
                            assert!(framing.offset.x < -0.12);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn phase_jumps_ease_and_scrubbing_cancels_the_animation() {
        let mut playback = Playback::default();
        let duration = 20.0;
        playback.jump_to(14.0, false);
        playback.advance(0.06, duration);
        assert!(playback.time > 0.0 && playback.time < 7.0);
        playback.advance(0.3, duration);
        assert_eq!(playback.time, 14.0);
        playback.jump_to(0.0, true);
        playback.advance(0.12, duration);
        assert!((playback.time - 7.0).abs() < 1e-6);
        playback.seek(10.0);
        playback.advance(0.3, duration);
        assert_eq!(playback.time, 10.0);
        assert!(!playback.playing);
    }
}
