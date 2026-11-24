//! Orbit map and camera projection shared by the trajectory experiments.

use bevy::prelude::{Quat, Vec3};
use bevy_egui::egui::{self, Align2, FontId, Pos2, Rect, Shape, Stroke, Vec2};

use crate::camera::OrbitCamera;
use crate::params::BlackHoleParams;
use crate::physics::probe::Sample;
use crate::ui::style::{BORDER, MUTED_TEXT};

#[derive(Clone, Copy)]
pub(super) struct MapProjection {
    pub(super) center: Pos2,
    pub(super) scale: f32,
}

impl MapProjection {
    pub(super) fn project(self, [x, y]: [f64; 2]) -> Pos2 {
        self.center + Vec2::new(x as f32, -y as f32) * self.scale
    }
}

pub(super) fn grid(painter: &egui::Painter, rect: Rect, projection: MapProjection, extent: f32) {
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

pub(super) struct SceneProjection {
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
    pub(super) fn new(rect: Rect, camera: &OrbitCamera, params: &BlackHoleParams) -> Self {
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

    pub(super) fn project(&self, sample: Sample) -> Option<Pos2> {
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

pub(super) fn draw_line(
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

pub(super) fn clock(seconds: f64) -> String {
    if seconds >= 86_400.0 {
        format!("{:.2} d", seconds / 86_400.0)
    } else if seconds >= 3_600.0 {
        format!("{:.2} h", seconds / 3600.0)
    } else if seconds >= 1.0 {
        format!("{seconds:.2} s")
    } else if seconds >= 0.001 {
        format!("{:.2} ms", seconds * 1000.0)
    } else {
        format!("{:.2} µs", seconds * 1e6)
    }
}
