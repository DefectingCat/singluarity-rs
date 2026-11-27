//! In-session probe editor and replay state.

mod map;

use std::f64::consts::{FRAC_PI_2, PI};

use bevy_egui::egui::{self, Align, Button, Color32, Layout, RichText, Ui};

use crate::camera::{OrbitCamera, SceneFraming};
use crate::params::BlackHoleParams;
use crate::physics::probe::{IntegrationError, Launch, Outcome, ProbeKind, Trajectory, integrate};
use crate::ui::style::{ACCENT, MUTED_TEXT, PANEL_FILL};
use crate::ui::trajectory::clock;

use super::lab::{LabLayout, TimeJump, content_opacity, fit_distance, frame_scene, reveal};

const RUN_STYLES: [(char, Color32); 6] = [
    ('A', ACCENT),
    ('B', Color32::from_rgb(124, 199, 240)),
    ('C', Color32::from_rgb(163, 219, 160)),
    ('D', Color32::from_rgb(225, 159, 203)),
    ('E', Color32::from_rgb(244, 145, 119)),
    ('F', Color32::from_rgb(191, 175, 237)),
];
const RATES: [(f64, &str); 4] = [(0.25, "¼×"), (0.5, "½×"), (1.0, "1×"), (2.0, "2×")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Experiment {
    Capture,
    Clocks,
    FrameDragging,
    Precession,
}

impl Experiment {
    const ALL: [Self; 4] = [
        Self::Capture,
        Self::Clocks,
        Self::FrameDragging,
        Self::Precession,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Capture => "Capture / escape",
            Self::Clocks => "Two clocks",
            Self::FrameDragging => "Frame dragging",
            Self::Precession => "Orbit precession",
        }
    }

    fn explanation(self) -> &'static str {
        match self {
            Self::Capture => {
                "The launches differ by 8°. One pulse reaches the horizon limit and the other escapes."
            }
            Self::Clocks => {
                "Compare the time on two orbiting spacecraft at the same far-away time."
            }
            Self::FrameDragging => {
                "Both spacecraft start locally at rest. Spin carries one around the hole as it falls."
            }
            Self::Precession => {
                "The dots mark successive closest passes. Compare how their directions shift after each orbit."
            }
        }
    }

    fn launches(self) -> [Launch; 2] {
        let light = Launch::default();
        let spacecraft = Launch {
            kind: ProbeKind::Spacecraft,
            aim: FRAC_PI_2,
            ..light
        };
        match self {
            Self::Capture => [
                light,
                Launch {
                    aim: 162.0_f64.to_radians(),
                    ..light
                },
            ],
            Self::Clocks => [4.0, 10.0].map(|radius| Launch {
                radius,
                speed: Launch::circular_speed(radius, 0.0),
                ..spacecraft
            }),
            Self::FrameDragging => [0.0, 0.9].map(|spin| Launch {
                spin,
                radius: 4.0,
                ..spacecraft
            }),
            Self::Precession => [0.0, 0.8].map(|spin| Launch {
                spin,
                radius: 10.0,
                speed: 0.22,
                ..spacecraft
            }),
        }
    }
}

struct Run {
    path: Trajectory,
    visible: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Desktop,
    Controls,
    Map,
}

#[derive(Default)]
pub(super) struct ProbeLab {
    open: bool,
    reveal: f32,
    initialized: bool,
    draft: Launch,
    preset: Option<Experiment>,
    runs: [Option<Run>; RUN_STYLES.len()],
    error: Option<IntegrationError>,
    time: f64,
    playing: bool,
    jump: Option<TimeJump>,
    rate: usize,
    scene: bool,
    map: map::Map,
    desktop_hidden: bool,
    compact_panel: Option<Panel>,
    last_panel: Option<Panel>,
    fit_pending: bool,
    distance_scale: f32,
    content_changed_at: Option<f64>,
}

impl ProbeLab {
    pub(super) fn is_active(&self) -> bool {
        self.open || self.reveal > 0.0
    }

    pub(super) fn launcher(&mut self, ui: &mut Ui, narrow: bool) {
        let (label, width) = if narrow {
            ("Probe", 52.0)
        } else {
            ("Launch a probe", 103.0)
        };
        if ui
            .add_sized(
                [width, 32.0],
                Button::new(RichText::new(label).color(ACCENT)).frame(false),
            )
            .on_hover_text("Launch a probe")
            .clicked()
        {
            reveal(ui.ctx(), "probe_open", false);
            reveal(ui.ctx(), "probe_panel", false);
            reveal(ui.ctx(), "probe_overlay", false);
            self.open = true;
            self.fit_pending = true;
            if !self.initialized {
                self.load(Experiment::Capture);
                self.rate = 2;
                self.scene = true;
                self.initialized = true;
            }
        }
    }

    fn load(&mut self, experiment: Experiment) {
        let [a, b] = experiment.launches();
        self.clear();
        self.draft = a;
        self.add_run(a);
        self.add_run(b);
        self.preset = Some(experiment);
        self.map.focus_launch(self.draft);
    }

    fn clear(&mut self) {
        self.runs = Default::default();
        self.preset = None;
        self.time = 0.0;
        self.playing = false;
        self.jump = None;
        self.error = None;
    }

    fn add_run(&mut self, launch: Launch) -> bool {
        let Some(slot) = self.runs.iter_mut().find(|slot| slot.is_none()) else {
            return false;
        };
        match integrate(launch) {
            Ok(path) => {
                *slot = Some(Run {
                    path,
                    visible: true,
                });
                self.error = None;
                self.preset = None;
                self.time = 0.0;
                self.playing = false;
                self.jump = None;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    fn duration(&self) -> f64 {
        self.runs
            .iter()
            .flatten()
            .map(|run| run.path.end().coordinate_time)
            .fold(0.0, f64::max)
    }

    fn update_playback(&mut self, dt: f64) {
        let duration = self.duration();
        self.time = self.time.min(duration);
        if let Some(jump) = &mut self.jump {
            if jump.advance(dt, &mut self.time) {
                self.jump = None;
            }
            return;
        }
        if self.playing {
            // Give each comparison a readable playback length, regardless of its simulation span.
            self.time =
                (self.time + dt.min(0.1) * duration / 16.0 * RATES[self.rate].0).min(duration);
            self.playing = self.time < duration;
        }
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        camera: &OrbitCamera,
        params: &BlackHoleParams,
        isco: f32,
        framing: &mut SceneFraming,
    ) -> [egui::Rect; 3] {
        let layout = LabLayout::new(ctx);
        let compact = layout.compact;
        if self.open && !ctx.text_edit_focused() && !ctx.any_popup_open() {
            ctx.input_mut(|input| {
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    self.open = false;
                } else if input.consume_key(egui::Modifiers::NONE, egui::Key::H) {
                    self.toggle_controls(compact);
                }
            });
        }

        self.reveal = reveal(ctx, "probe_open", self.open);
        let dt = ctx.input(|input| input.stable_dt);
        if !self.open && self.reveal <= 0.0 {
            framing.approach(SceneFraming::default(), dt);
            return [egui::Rect::NOTHING; 3];
        }

        let toolbar = layout.toolbar(ctx, "probe_toolbar", self.reveal, self.open, |ui| {
            ui.horizontal(|ui| {
                ui.strong(if compact {
                    "Probe lab"
                } else {
                    "Probe experiments"
                });
                if compact {
                    let controls = self.compact_panel == Some(Panel::Controls);
                    if ui
                        .selectable_label(controls, choice("Controls", controls))
                        .clicked()
                    {
                        self.toggle_controls(compact);
                    }
                    let map = self.compact_panel == Some(Panel::Map);
                    if ui.selectable_label(map, choice("Orbit map", map)).clicked() {
                        self.compact_panel = (!map).then_some(Panel::Map);
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
        let requested_panel = if compact {
            self.compact_panel
        } else {
            (!self.desktop_hidden).then_some(Panel::Desktop)
        };
        let panel_open = self.open && requested_panel.is_some();
        let panel_reveal = reveal(ctx, "probe_panel", panel_open);
        let now = ctx.input(|input| input.time);
        if let Some(panel) = requested_panel
            && self.last_panel != Some(panel)
        {
            if self.last_panel.is_some() && panel_reveal >= 1.0 {
                self.content_changed_at = Some(now);
            }
            self.last_panel = Some(panel);
        }
        if !self.open {
            self.playing = false;
            self.jump = None;
        } else if ctx.current_pass_index() == 0 {
            self.update_playback(f64::from(dt));
        }
        if !panel_open || requested_panel == Some(Panel::Controls) {
            self.map.end_drag();
        }

        let transport = layout.transport(ctx, "probe_transport", self.reveal, self.open, |ui| {
            self.replay(ui);
        });
        let mut panel_bounds = egui::Rect::NOTHING;
        let mut settled_panel = None;
        if let Some(panel) = self.last_panel
            && panel_reveal > 0.0
        {
            let surface = layout.inspector(
                ctx,
                "probe_inspector",
                &toolbar,
                Some(&transport),
                panel_reveal,
                self.reveal,
                panel_open,
                |ui, height| self.inspector(ui, height, panel),
            );
            panel_bounds = surface.rect;
            settled_panel = panel_open.then_some(surface.settled);
        }
        let canvas = ctx.viewport_rect();
        let region = layout.scene_region(&toolbar, Some(&transport), settled_panel);
        let target = if self.open {
            let center = if settled_panel.is_some() {
                region.center()
            } else {
                canvas.center()
            };
            if self.fit_pending && (!panel_open || settled_panel.is_some()) {
                self.fit_pending = false;
                let inner_disk = (isco + 1.5).max(4.0);
                let radius = self
                    .runs
                    .iter()
                    .flatten()
                    .filter(|run| run.visible)
                    .map(|run| run.path.launch().radius as f32)
                    .fold(inner_disk, f32::max)
                    .min(12.0)
                    * params.rs;
                self.distance_scale = fit_distance(canvas, region, center, camera, radius);
            }
            frame_scene(canvas, center, self.distance_scale)
        } else {
            SceneFraming::default()
        };
        if ctx.current_pass_index() == 0 {
            framing.approach(target, dt);
        }
        let overlay = reveal(ctx, "probe_overlay", self.open && self.scene)
            * content_opacity(now, self.content_changed_at);
        if overlay > 0.0 {
            map::draw_scene(
                ctx, camera, params, &self.runs, self.time, *framing, overlay,
            );
        }
        if self.open {
            [
                toolbar.rect,
                transport.rect,
                if panel_open {
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

    fn inspector(&mut self, ui: &mut Ui, height: f32, panel: Panel) {
        let now = ui.input(|input| input.time);
        let opacity = content_opacity(now, self.content_changed_at);
        ui.multiply_opacity(opacity);
        if opacity < 1.0 {
            ui.ctx().request_repaint();
        }
        if panel != Panel::Map {
            self.experiment_selector(ui);
        }
        ui.checkbox(&mut self.scene, "Scene overlay")
            .on_hover_text("Project the paths over the current scene");
        if panel != Panel::Controls {
            let before = self.draft;
            let map_height =
                (height * if panel == Panel::Map { 0.61 } else { 0.36 }).clamp(45.0, 210.0);
            self.map.show(
                ui,
                map_height,
                &mut self.draft,
                &self.runs,
                self.time,
                self.preset == Some(Experiment::Precession),
            );
            if self.draft != before {
                self.error = None;
            }
        }
        if panel != Panel::Map {
            ui.separator();
            let before = self.run_revision();
            egui::ScrollArea::vertical()
                .id_salt("probe_settings")
                .auto_shrink([false, true])
                .max_height((height - ui.min_size().y - 22.0).max(22.0))
                .show(ui, |ui| {
                    self.launch_settings(ui);
                    ui.separator();
                    self.readings(ui);
                });
            if self.run_revision() != before {
                self.content_changed_at = Some(now);
            }
        }
    }

    fn run_revision(&self) -> [Option<Launch>; RUN_STYLES.len()] {
        self.runs
            .each_ref()
            .map(|slot| slot.as_ref().map(|run| run.path.launch()))
    }

    fn experiment_selector(&mut self, ui: &mut Ui) {
        let mut selection = None;
        egui::ComboBox::from_id_salt("experiment")
            .selected_text(self.preset.map_or("Custom experiment", Experiment::name))
            .width(132.0)
            .show_ui(ui, |ui| {
                for experiment in Experiment::ALL {
                    if ui
                        .selectable_label(
                            self.preset == Some(experiment),
                            choice(experiment.name(), self.preset == Some(experiment)),
                        )
                        .clicked()
                    {
                        selection = Some(experiment);
                    }
                }
            });
        if let Some(experiment) = selection
            && self.preset != Some(experiment)
        {
            self.load(experiment);
            self.content_changed_at = Some(ui.input(|input| input.time));
        }
        if let Some(preset) = self.preset {
            ui.label(
                RichText::new(preset.explanation())
                    .small()
                    .color(MUTED_TEXT),
            );
        }
    }

    fn replay(&mut self, ui: &mut Ui) {
        let duration = self.duration();
        let controls = |ui: &mut Ui, lab: &mut Self| {
            ui.add_enabled_ui(duration > 0.0, |ui| {
                if ui
                    .button(if lab.playing { "Pause" } else { "Play" })
                    .clicked()
                {
                    if lab.time >= duration && lab.jump.is_none() {
                        lab.jump = TimeJump::new(lab.time, 0.0);
                        lab.playing = true;
                    } else {
                        lab.playing = !lab.playing;
                    }
                }
                if ui.button("Rewind").clicked() {
                    lab.jump = TimeJump::new(lab.time, 0.0);
                    lab.playing = false;
                }
            });
            egui::ComboBox::from_id_salt("probe_rate")
                .width(42.0)
                .selected_text(RATES[lab.rate].1)
                .show_ui(ui, |ui| {
                    for (index, (_, label)) in RATES.iter().enumerate() {
                        let selected = lab.rate == index;
                        ui.selectable_value(&mut lab.rate, index, choice(label, selected));
                    }
                });
            ui.label(RichText::new(format!("{:.1} r_s/c", lab.time)).monospace());
            lab.fit_pending |= ui.button("Fit scene").clicked();
        };
        if ui.available_width() > 600.0 {
            ui.horizontal(|ui| {
                controls(ui, self);
                self.timeline(ui, duration);
            });
        } else {
            ui.horizontal_wrapped(|ui| controls(ui, self));
            self.timeline(ui, duration);
        }
    }

    fn timeline(&mut self, ui: &mut Ui, duration: f64) {
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = ui.available_width();
            let response = ui
                .add_enabled(
                    duration > 0.0,
                    egui::Slider::new(&mut self.time, 0.0..=duration)
                        .clamping(egui::SliderClamping::Edits)
                        .show_value(false),
                )
                .on_hover_text("Shared coordinate time");
            if response.changed() {
                self.playing = false;
                self.jump = None;
            }
        });
    }

    fn readings(&mut self, ui: &mut Ui) {
        let count = self.runs.iter().flatten().count();
        ui.horizontal(|ui| {
            ui.strong(format!("Runs {count}/{}", self.runs.len()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(count > 0, Button::new("Clear runs"))
                    .clicked()
                {
                    self.clear();
                }
            });
        });
        for (slot, (name, color)) in self.runs.iter_mut().zip(RUN_STYLES) {
            let Some(run) = slot else { continue };
            let launch = run.path.launch();
            let sample = run.path.at(self.time);
            let status = if self.time < run.path.end().coordinate_time {
                "In flight"
            } else {
                match run.path.outcome() {
                    Outcome::HorizonLimit => "At horizon limit",
                    Outcome::Escaped => "Escaped",
                    Outcome::TimeLimit => "Simulation time limit",
                }
            };
            let mut remove = false;
            ui.push_id(name, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut run.visible,
                        RichText::new(format!("Run {name}")).strong().color(color),
                    )
                    .on_hover_text(format!(
                        "{} · {:.3e} Suns\nStart {:.1}° · Local speed {:.3} c",
                        if launch.kind == ProbeKind::Light {
                            "Light pulse"
                        } else {
                            "Spacecraft"
                        },
                        launch.solar_masses,
                        launch.bearing.to_degrees(),
                        if launch.kind == ProbeKind::Light {
                            1.0
                        } else {
                            launch.speed
                        }
                    ));
                    ui.label(format!("{status} · {:.3} r_s", sample.radius));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        remove = ui
                            .button("×")
                            .on_hover_text(format!("Remove run {name}"))
                            .clicked();
                    });
                });
                ui.label(
                    RichText::new(format!(
                        "Far away {} · Onboard {}",
                        clock(launch.seconds(sample.coordinate_time)),
                        sample
                            .proper_time
                            .map_or_else(|| "—".into(), |t| clock(launch.seconds(t)))
                    ))
                    .small(),
                );
                ui.label(
                    RichText::new(format!(
                        "Closest {:.3} r_s · Start {:.2} r_s · Aim {:.1}° · χ {:+.3}",
                        run.path.closest_radius(),
                        launch.radius,
                        launch.aim.to_degrees(),
                        launch.spin
                    ))
                    .small()
                    .color(MUTED_TEXT),
                );
            });
            if remove {
                *slot = None;
                self.preset = None;
            }
        }
    }

    fn launch_settings(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.strong("Next launch");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let available = self.runs.iter().any(Option::is_none);
                if ui
                    .add_enabled(
                        available,
                        Button::new(RichText::new("Launch and compare").color(ACCENT)).frame(true),
                    )
                    .clicked()
                {
                    self.add_run(self.draft);
                }
            });
        });
        if let Some(error) = self.error {
            ui.colored_label(
                Color32::LIGHT_RED,
                match error {
                    IntegrationError::InvalidLaunch => {
                        "The launch settings are outside the supported range."
                    }
                    IntegrationError::StepLimit => {
                        "The solver reached its step limit before finishing this run."
                    }
                    IntegrationError::NumericalFailure => {
                        "The solver could not resolve this trajectory reliably."
                    }
                },
            );
        }
        let before = self.draft;
        ui.horizontal_wrapped(|ui| {
            let light = self.draft.kind == ProbeKind::Light;
            ui.selectable_value(
                &mut self.draft.kind,
                ProbeKind::Light,
                choice("Light pulse · c", light),
            );
            ui.selectable_value(
                &mut self.draft.kind,
                ProbeKind::Spacecraft,
                choice("Spacecraft", !light),
            );
        });
        super::sections::slider(
            ui,
            "Black-hole mass",
            &mut self.draft.solar_masses,
            1.0..=Launch::MAX_SOLAR_MASSES,
            0,
            " Suns",
            true,
        );
        super::sections::slider(
            ui,
            "Spin and direction (χ)",
            &mut self.draft.spin,
            -Launch::MAX_SPIN..=Launch::MAX_SPIN,
            3,
            "",
            false,
        );
        let min_radius = Launch::min_start_radius(self.draft.spin);
        self.draft.radius = self.draft.radius.max(min_radius);
        super::sections::slider(
            ui,
            "Start distance",
            &mut self.draft.radius,
            min_radius..=Launch::MAX_START_RADIUS,
            3,
            " r_s",
            true,
        );
        angle_slider(ui, "Start position", &mut self.draft.bearing);
        angle_slider(ui, "Aim · 0° out, ±180° in", &mut self.draft.aim);
        if self.draft.kind == ProbeKind::Spacecraft {
            super::sections::slider(
                ui,
                "Local speed",
                &mut self.draft.speed,
                0.0..=Launch::MAX_SPEED,
                3,
                " c",
                false,
            );
        }
        if self.draft != before {
            self.error = None;
        }
        ui.collapsing("Model and units", |ui| {
            ui.label("Light and spacecraft follow equatorial Kerr geodesics. Launch speed and direction use a local zero-angular-momentum frame. The solver stops 0.004 r_s outside the horizon, on an unbound outward crossing of 120 r_s, or at time 600 r_s/c. The unit r_s is the Schwarzschild radius for each run’s mass. Each run uses its own mass to convert coordinate and onboard times to seconds.");
        });
    }
}

fn choice(label: &str, selected: bool) -> RichText {
    let text = RichText::new(label);
    if selected {
        text.color(PANEL_FILL).strong()
    } else {
        text
    }
}

fn angle_slider(ui: &mut Ui, label: &str, angle: &mut f64) {
    let before = *angle;
    let wrapped = (before + PI).rem_euclid(2.0 * PI) - PI;
    let mut degrees = if wrapped == -PI && before > 0.0 {
        180.0
    } else {
        wrapped.to_degrees()
    };
    let initial = degrees;
    super::sections::slider(ui, label, &mut degrees, -180.0..=180.0, 1, "°", false);
    if degrees != initial {
        *angle = degrees.to_radians();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_load_two_runs_and_reset_replay() {
        let mut lab = ProbeLab::default();
        for experiment in Experiment::ALL {
            lab.time = 100.0;
            lab.playing = true;
            lab.load(experiment);
            assert!(lab.error.is_none());
            assert_eq!(lab.runs.iter().flatten().count(), 2);
            assert_eq!(lab.preset, Some(experiment));
            assert!(lab.time.abs() < f64::EPSILON && !lab.playing);
        }
    }

    #[test]
    fn runs_keep_their_launch_snapshot_and_reuse_empty_slots() {
        let mut lab = ProbeLab::default();
        lab.load(Experiment::Capture);
        let first = lab.draft;
        lab.draft.spin = 0.5;
        for _ in 2..RUN_STYLES.len() {
            assert!(lab.add_run(lab.draft));
        }
        assert!(!lab.add_run(lab.draft));
        assert_eq!(lab.runs[0].as_ref().unwrap().path.launch(), first);
        lab.runs[1] = None;
        assert!(lab.add_run(lab.draft));
        assert_eq!(lab.runs[1].as_ref().unwrap().path.launch(), lab.draft);
    }

    #[test]
    fn shorter_run_holds_its_final_state_while_the_comparison_continues() {
        let mut lab = ProbeLab::default();
        lab.load(Experiment::Capture);
        let captured = &lab.runs[0].as_ref().unwrap().path;
        let last = captured.end();
        let duration = lab.duration();
        assert!(duration > last.coordinate_time);
        let settled = captured.at(duration);
        assert!((settled.coordinate_time - last.coordinate_time).abs() < f64::EPSILON);
        assert!((settled.radius - last.radius).abs() < f64::EPSILON);
        let in_flight = captured.at(1.23);
        assert!((in_flight.coordinate_time - 1.23).abs() < f64::EPSILON);
        assert!(in_flight.radius < captured.launch().radius && in_flight.radius > last.radius);
    }

    #[test]
    fn narrow_probe_drawer_leaves_the_transport_and_scene_visible() {
        let ctx = egui::Context::default();
        crate::ui::style::setup(&ctx);
        let mut lab = ProbeLab {
            open: true,
            scene: true,
            ..Default::default()
        };
        lab.load(Experiment::Capture);
        let camera = OrbitCamera::default();
        let params = BlackHoleParams::default();
        let isco = crate::physics::kerr_isco(params.spin);
        let mut framing = SceneFraming::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 560.0));
        let mut time = 0.0;
        let mut frame = |lab: &mut ProbeLab| {
            let mut bounds = [egui::Rect::NOTHING; 3];
            for _ in 0..28 {
                time += 1.0 / 60.0;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(viewport),
                        ..Default::default()
                    },
                    |_| bounds = lab.show(&ctx, &camera, &params, isco, &mut framing),
                );
                output.textures_delta.clear();
            }
            (bounds, framing.offset)
        };
        let ([toolbar, transport, panel], offset) = frame(&mut lab);
        assert!(viewport.contains_rect(toolbar));
        assert!(viewport.contains_rect(transport));
        assert!(!panel.is_positive());
        assert_eq!(offset, bevy::math::Vec2::ZERO);

        lab.compact_panel = Some(Panel::Map);
        let ([_, transport, panel], offset) = frame(&mut lab);
        assert!(viewport.contains_rect(panel));
        assert!(panel.bottom() < transport.top());
        assert!(offset.y < -0.12);
    }
}
