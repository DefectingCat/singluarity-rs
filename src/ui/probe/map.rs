use std::f64::consts::PI;

use bevy::prelude::{Quat, Vec3};
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use crate::camera::OrbitCamera;
use crate::params::BlackHoleParams;
use crate::physics::probe::{Launch, Sample, Trajectory, horizon_radius};
use crate::ui::style::{ACCENT, BORDER, EXTREME_BG, MUTED_TEXT};

use super::{RUN_STYLES, Run};

const MAX_EXTENT: f32 = 135.0;

enum Handle {
    Start,
    Aim,
}

pub(super) struct Map {
    extent: f32,
    drag: Option<Handle>,
}

impl Default for Map {
    fn default() -> Self {
        Self {
            extent: 16.0,
            drag: None,
        }
    }
}

impl Map {
    pub(super) fn fit(&mut self, launch: Launch, runs: &[Option<Run>]) {
        let radius = visible_runs(runs)
            .flat_map(|(run, _)| run.samples())
            .map(|sample| sample.radius)
            .fold(launch.radius, f64::max);
        self.extent = (radius as f32 * 1.1).clamp(2.0, MAX_EXTENT);
    }

    pub(super) fn focus_launch(&mut self, launch: Launch) {
        self.extent = (launch.radius as f32 * 1.4).clamp(2.0, MAX_EXTENT);
        self.end_drag();
    }

    pub(super) fn end_drag(&mut self) {
        self.drag = None;
    }

    fn zoom(&mut self, factor: f32) {
        self.extent = (self.extent * factor).clamp(1.4, MAX_EXTENT);
    }

    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        height: f32,
        launch: &mut Launch,
        runs: &[Option<Run>],
        time: f64,
        apsides: bool,
    ) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("Equatorial coordinates")
                    .small()
                    .color(MUTED_TEXT),
            );
            if ui.button("−").on_hover_text("Zoom map out").clicked() {
                self.zoom(1.35);
            }
            if ui.button("+").on_hover_text("Zoom map in").clicked() {
                self.zoom(1.0 / 1.35);
            }
            if ui.button("Fit paths").clicked() {
                self.fit(*launch, runs);
            }
        });
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::drag());
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            self.zoom((-0.004 * scroll).exp());
        }
        let projection = MapProjection {
            center: rect.center(),
            scale: 0.5 * rect.width().min(rect.height()) / self.extent,
        };
        let (start, tip) = handles(projection, *launch);
        if response.drag_started() {
            self.drag = ui
                .input(|input| input.pointer.press_origin())
                .and_then(|pos| {
                    if pos.distance(tip) <= 12.0 {
                        Some(Handle::Aim)
                    } else if pos.distance(start) <= 12.0 {
                        Some(Handle::Start)
                    } else {
                        None
                    }
                });
        }
        // The final move and button release can arrive in the same render frame.
        if (response.dragged() || response.drag_stopped())
            && let Some(pos) = response.interact_pointer_pos()
        {
            match self.drag {
                Some(Handle::Start) => {
                    let offset = pos - projection.center;
                    launch.radius = f64::from(offset.length() / projection.scale).clamp(
                        Launch::min_start_radius(launch.spin),
                        Launch::MAX_START_RADIUS,
                    );
                    if offset.length_sq() > 0.01 {
                        launch.bearing = f64::from((-offset.y).atan2(offset.x));
                    }
                }
                Some(Handle::Aim) if pos.distance(start) > 3.0 => {
                    let offset = pos - start;
                    let direction = f64::from((-offset.y).atan2(offset.x));
                    launch.aim = (direction - launch.bearing + PI).rem_euclid(2.0 * PI) - PI;
                }
                _ => {}
            }
        }
        if response.drag_stopped() {
            self.end_drag();
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 7, EXTREME_BG);
        grid(&painter, rect, projection, self.extent);

        let horizon = horizon_radius(launch.spin);
        painter.circle(
            projection.center,
            horizon as f32 * projection.scale,
            Color32::BLACK,
            Stroke::new(1.0, MUTED_TEXT),
        );
        for (run, color) in visible_runs(runs) {
            let run_horizon = horizon_radius(run.launch().spin);
            if (run_horizon - horizon).abs() > 0.005 {
                painter.circle_stroke(
                    projection.center,
                    run_horizon as f32 * projection.scale,
                    Stroke::new(1.0, color.gamma_multiply(0.65)),
                );
            }
            let project = |sample: Sample| Some(projection.project(sample.xy()));
            draw_track(&painter, run, color, time, project);
            if apsides {
                for sample in run.samples().windows(3).filter_map(|pair| {
                    (pair[1].radius < pair[0].radius && pair[1].radius < pair[2].radius)
                        .then_some(pair[1])
                }) {
                    painter.circle_filled(projection.project(sample.xy()), 2.6, color);
                }
            }
        }

        let (start, tip) = handles(projection, *launch);
        painter.arrow(start, tip - start, Stroke::new(1.7, ACCENT));
        painter.circle(start, 5.0, EXTREME_BG, Stroke::new(1.7, ACCENT));
        painter.circle_filled(tip, 2.5, ACCENT);
        response.on_hover_text("Drag the gold circle to set the start or the arrow tip to aim");
    }
}

#[derive(Clone, Copy)]
struct MapProjection {
    center: Pos2,
    scale: f32,
}

impl MapProjection {
    fn project(self, [x, y]: [f64; 2]) -> Pos2 {
        self.center + Vec2::new(x as f32, -y as f32) * self.scale
    }
}

fn handles(projection: MapProjection, launch: Launch) -> (Pos2, Pos2) {
    let (sin, cos) = launch.bearing.sin_cos();
    let start = projection.project([launch.radius * cos, launch.radius * sin]);
    let (sin, cos) = (launch.bearing + launch.aim).sin_cos();
    let tip = start + Vec2::new(cos as f32, -sin as f32) * 30.0;
    (start, tip)
}

fn grid(painter: &egui::Painter, rect: Rect, projection: MapProjection, extent: f32) {
    let grid_units = (extent / 3.0).log2().floor().exp2();
    let gap = (projection.scale * grid_units).max(10.0);
    let stroke = Stroke::new(1.0, BORDER.gamma_multiply(0.38));
    for i in -(rect.width() / gap * 0.5).ceil() as i32..=(rect.width() / gap * 0.5).ceil() as i32 {
        painter.vline(projection.center.x + i as f32 * gap, rect.y_range(), stroke);
    }
    for i in -(rect.height() / gap * 0.5).ceil() as i32..=(rect.height() / gap * 0.5).ceil() as i32
    {
        painter.hline(rect.x_range(), projection.center.y + i as f32 * gap, stroke);
    }
    let end = rect.right_bottom() - Vec2::new(9.0, 6.0);
    painter.line_segment(
        [end, end - Vec2::new(projection.scale * grid_units, 0.0)],
        Stroke::new(1.0, MUTED_TEXT),
    );
    painter.text(
        end - Vec2::new(0.0, 3.0),
        Align2::RIGHT_BOTTOM,
        format!("{grid_units} r_s"),
        FontId::proportional(11.0),
        MUTED_TEXT,
    );
}

fn visible_runs(runs: &[Option<Run>]) -> impl Iterator<Item = (&Trajectory, Color32)> {
    runs.iter()
        .zip(RUN_STYLES)
        .filter_map(|(slot, (_, color))| {
            slot.as_ref()
                .filter(|run| run.visible)
                .map(|run| (&run.path, color))
        })
}

pub(super) fn draw_scene(
    ctx: &egui::Context,
    camera: &OrbitCamera,
    params: &BlackHoleParams,
    runs: &[Option<Run>],
    time: f64,
) {
    let rect = ctx.content_rect();
    let painter = ctx
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(rect);
    let projection = SceneProjection::new(rect, camera, params);
    for (run, color) in visible_runs(runs) {
        draw_track(&painter, run, color, time, |sample| {
            projection.project(sample)
        });
    }
}

struct SceneProjection {
    center: Pos2,
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    focal_pixels: f32,
    tilt: Quat,
    radius_scale: f32,
}

impl SceneProjection {
    fn new(rect: Rect, camera: &OrbitCamera, params: &BlackHoleParams) -> Self {
        let (eye, forward, right, up) = camera.basis();
        let aspect = rect.width().max(1.0) / rect.height().max(1.0);
        // Invert the shader's ray generator, including the portrait FOV.
        let focal_pixels = rect.height() * 0.5 / (0.5 * camera.vertical_fov(aspect)).tan();
        Self {
            center: rect.center(),
            eye,
            forward,
            right,
            up,
            focal_pixels,
            tilt: Quat::from_rotation_x(params.disk_tilt),
            radius_scale: params.rs,
        }
    }

    fn project(&self, sample: Sample) -> Option<Pos2> {
        let [x, y] = sample.xy();
        // Increasing azimuth is right-handed about the shader's +Y spin axis.
        let point = self.tilt * Vec3::new(x as f32, 0.0, -y as f32) * self.radius_scale;
        let ray = point - self.eye;
        let depth = ray.dot(self.forward);
        (depth > 0.05).then(|| {
            self.center
                + Vec2::new(ray.dot(self.right), -ray.dot(self.up)) * (self.focal_pixels / depth)
        })
    }
}

fn draw_track(
    painter: &egui::Painter,
    run: &Trajectory,
    color: Color32,
    time: f64,
    project: impl Fn(Sample) -> Option<Pos2>,
) {
    let current = run.at(time);
    let samples = run.samples();
    let passed = samples.partition_point(|sample| sample.coordinate_time < time);
    draw_line(
        painter,
        samples.iter().copied(),
        Stroke::new(1.1, color.gamma_multiply(0.42)),
        &project,
    );
    draw_line(
        painter,
        samples[..passed].iter().copied().chain([current]),
        Stroke::new(2.0, color),
        &project,
    );
    if let Some(start) = project(samples[0]) {
        painter.circle_stroke(start, 3.0, Stroke::new(1.0, color));
    }
    if let Some(end) = project(run.end()) {
        painter.circle_stroke(end, 2.7, Stroke::new(1.0, color.gamma_multiply(0.7)));
    }
    if let Some(head) = project(current) {
        painter.circle(head, 4.5, color, Stroke::new(1.0, EXTREME_BG));
    }
}

fn draw_line(
    painter: &egui::Painter,
    samples: impl Iterator<Item = Sample>,
    stroke: Stroke,
    project: &impl Fn(Sample) -> Option<Pos2>,
) {
    let mut line = Vec::new();
    for point in samples.map(project).chain([None]) {
        if let Some(point) = point {
            if line
                .last()
                .is_none_or(|last: &Pos2| last.distance_sq(point) > 0.0625)
            {
                line.push(point);
            }
        } else if line.len() > 1 {
            painter.add(Shape::line(std::mem::take(&mut line), stroke));
        } else {
            line.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    #[test]
    fn dragging_map_handles_updates_only_the_corresponding_launch_parameters() {
        let ctx = egui::Context::default();
        let mut map = Map::default();
        let mut launch = Launch::default();
        let original = launch;
        let mut frame = |events| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(360.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                map.show(ui, 180.0, &mut launch, &[], 0.0, false);
            });
            output.textures_delta.clear();
            output
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let handle = |output: egui::FullOutput, radius: f32| {
            output
                .shapes
                .into_iter()
                .find_map(|shape| match shape.shape {
                    Shape::Circle(circle) if (circle.radius - radius).abs() < f32::EPSILON => {
                        Some(circle.center)
                    }
                    _ => None,
                })
                .unwrap()
        };

        let start = handle(frame(vec![]), 5.0);
        frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        let end = start + Vec2::new(-20.0, 25.0);
        let tip = handle(
            frame(vec![egui::Event::PointerMoved(end), button(end, false)]),
            2.5,
        );
        frame(vec![egui::Event::PointerMoved(tip), button(tip, true)]);
        let end = tip - Vec2::new(5.0, 32.0);
        frame(vec![egui::Event::PointerMoved(end), button(end, false)]);

        assert!(launch.radius < original.radius - 1.0);
        assert!(launch.bearing < -0.2);
        assert!(launch.aim < original.aim - 0.2);
        assert!((launch.spin - original.spin).abs() < f64::EPSILON);
        assert!(map.drag.is_none());
    }

    #[test]
    fn scene_projection_matches_pixel_rays_in_landscape_and_portrait() {
        for size in [Vec2::new(1100.0, 700.0), Vec2::new(320.0, 680.0)] {
            let camera = OrbitCamera::default();
            let params = BlackHoleParams {
                disk_tilt: 0.2,
                rs: 1.2,
                ..Default::default()
            };
            let rect = Rect::from_min_size(Pos2::new(2.0, 3.0), size);
            let projection = SceneProjection::new(rect, &camera, &params);
            let sample = Sample {
                radius: 5.0,
                azimuth: f64::from(TAU) * 0.125,
                coordinate_time: 0.0,
                proper_time: None,
            };
            let pos = projection.project(sample).unwrap();
            let uv = 2.0 * (pos - rect.min) / rect.size();
            let aspect = rect.width() / rect.height();
            let tan = (0.5 * camera.vertical_fov(aspect)).tan();
            let (eye, forward, right, up) = camera.basis();
            let direction =
                (forward + right * ((uv.x - 1.0) * aspect * tan) + up * ((1.0 - uv.y) * tan))
                    .normalize();
            let [x, y] = sample.xy();
            let world = Quat::from_rotation_x(params.disk_tilt)
                * Vec3::new(x as f32, 0.0, -y as f32)
                * params.rs;
            assert!(direction.distance((world - eye).normalize()) < 1e-6);
        }
    }

    #[test]
    fn points_behind_the_camera_are_hidden() {
        let projection = SceneProjection::new(
            Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0)),
            &OrbitCamera::default(),
            &BlackHoleParams::default(),
        );
        let behind = Sample {
            radius: 40.0,
            azimuth: -f64::from(TAU) * 0.25,
            coordinate_time: 0.0,
            proper_time: None,
        };
        assert!(projection.project(behind).is_none());
    }
}
