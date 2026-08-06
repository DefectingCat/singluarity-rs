use bevy::input::mouse::MouseMotion;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

/// Orbit camera state. The black hole is at the origin (Rs=1 units).
/// `distance` is the camera radius; `yaw`/`pitch` orient it.
#[derive(Resource, Clone, Copy)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Vertical field of view in radians.
    pub fov: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: -1.065,
            // ~0.335 rad (19°) below the disk plane. The lensed far side of the
            // disk then arcs over the top of the shadow — the iconic framing. The
            // basis() has no gimbal pole, so any pitch is safe; this is just a nice
            // default angle, not a pole-avoidance choice.
            pitch: -0.335,
            distance: 30.0,
            fov: 1.0, // radians
        }
    }
}

/// Set by the UI system each frame; the orbit controller ignores input when true.
#[derive(Resource, Default)]
pub struct WantsPointer(pub bool);

impl OrbitCamera {
    /// Compute the camera eye position and an orthonormal basis (forward/right/up)
    /// in Bevy's right-handed Y-up coordinate system. The black hole sits at origin.
    /// Disk plane is the xz-plane (y=0); the disk tilt is applied in the shader
    /// via the params, so the camera basis here is in world space.
    ///
    /// `right` is derived from `yaw` alone (always a unit vector in the y=0 plane),
    /// NOT from `forward × world_up`. The old cross-product form degenerated to a
    /// zero vector when forward aligned with world-Y (pitch = ±π/2), flipping the
    /// image. Decoupling right from forward removes that singularity, so pitch can
    /// cross both poles with no roll/flip. This also keeps right exactly horizontal
    /// for all pitch, eliminating a subtle roll the old form introduced.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        // Eye position on a sphere around the origin.
        let eye = Vec3::new(
            self.distance * cp * sy,
            self.distance * sp,
            self.distance * cp * cy,
        );
        // Forward points from eye toward the origin.
        let forward = (-eye).normalize();
        // Right depends on yaw only: a unit vector in the y=0 plane, never zero.
        let right = Vec3::new(cy, 0.0, -sy);
        let up = right.cross(forward).normalize();
        (eye, forward, right, up)
    }
}

pub fn orbit_controller(
    wants: Res<WantsPointer>,
    mut camera: ResMut<OrbitCamera>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
) {
    if wants.0 {
        // egui is using the pointer: drain events so they don't pile up, and ignore.
        motion.clear();
        wheel.clear();
        return;
    }
    if mouse.pressed(MouseButton::Left) {
        for ev in motion.read() {
            // View-follows-cursor: dragging right rotates the view right,
            // dragging down tilts it down (inverts the old grab-the-scene mapping).
            camera.yaw += ev.delta.x * 0.005;
            // Keep yaw in (-π, π] so it never drifts out of the UI slider's range
            // (which would make touching the slider snap the view). cos/sin are
            // periodic, so the wrap is visually lossless.
            camera.yaw = (camera.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            // Full pitch range: the basis() has no gimbal pole now (right is yaw-only),
            // so crossing ±π/2 no longer flips. The ±0.05 margin keeps eye off the
            // exact origin-axis singularity where forward = ∓Y and yaw is undefined.
            camera.pitch = (camera.pitch - ev.delta.y * 0.005)
                .clamp(-std::f32::consts::PI + 0.05, std::f32::consts::PI - 0.05);
        }
    }
    for ev in wheel.read() {
        // Trackpads/Magic Mouse deliver PIXEL deltas (tens of px per event);
        // wheel mice deliver LINE deltas (±1 per notch). Normalize pixels to
        // notch-equivalents — treating raw pixels as lines made a single
        // smooth-scroll tick divide the distance several-fold.
        let lines = match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y / 50.0,
        };
        // Exponential zoom: +y (scroll up) shrinks distance, symmetric in/out,
        // and free of the old singularity — distance/(1 + y*0.1) blew up at
        // y ≤ -10 (÷0 → ∞, or negative → clamped to the 2.6 floor), so a fast
        // scroll-out teleported the camera to the CLOSEST point and macOS
        // momentum bursts jittered it between the clamp bounds.
        camera.distance = (camera.distance * 1.1f32.powf(-lines)).clamp(2.6, 500.0);
        // 2.6 ≈ bcrit; don't let the camera pass through the shadow.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::mouse::MouseScrollUnit;

    fn wheel_world(distance: f32) -> World {
        let mut world = World::new();
        world.insert_resource(WantsPointer(false));
        world.insert_resource(OrbitCamera {
            distance,
            ..Default::default()
        });
        world.insert_resource(ButtonInput::<MouseButton>::default());
        world.init_resource::<Messages<MouseMotion>>();
        world.init_resource::<Messages<MouseWheel>>();
        world
    }

    /// Feed wheel events (one batch = one frame), run the controller, return distance.
    fn scroll(world: &mut World, unit: MouseScrollUnit, ys: &[f32]) -> f32 {
        for &y in ys {
            world.write_message(MouseWheel {
                unit,
                x: 0.0,
                y,
                window: Entity::PLACEHOLDER,
                phase: bevy::input::touch::TouchPhase::Moved,
            });
        }
        world.run_system_once(orbit_controller).unwrap();
        world.resource::<OrbitCamera>().distance
    }

    #[test]
    fn wheel_line_notch_zooms_in_one_step() {
        let mut w = wheel_world(30.0);
        let d = scroll(&mut w, MouseScrollUnit::Line, &[1.0]);
        // One line-notch shrinks distance by exactly 1/1.1.
        assert!((d - 30.0 / 1.1).abs() < 1e-4, "got {d}");
    }

    #[test]
    fn wheel_pixel_tick_is_notch_sized_not_explosive() {
        // Trackpads/Magic Mouse deliver PIXEL deltas; |y| is tens of pixels per
        // event, not 1 line. One tick must feel like a notch, not divide the
        // distance by 4 (30 → 7.5) as the unit-blind formula does.
        let mut w = wheel_world(30.0);
        let d = scroll(&mut w, MouseScrollUnit::Pixel, &[30.0]);
        assert!(
            d > 30.0 * 0.7 && d < 30.0,
            "pixel tick overshot: {d} (started 30.0)"
        );
    }

    #[test]
    fn wheel_fast_scroll_out_does_not_teleport_inward() {
        // 1 + y*0.1 goes ≤ 0 at y ≤ -10: scrolling OUT fast made the divisor
        // negative → distance went negative → clamped to 2.6, i.e. the camera
        // teleported to the CLOSEST point. Distance must grow instead.
        let mut w = wheel_world(30.0);
        let d = scroll(&mut w, MouseScrollUnit::Pixel, &[-30.0]);
        assert!(
            (30.0..=500.0).contains(&d),
            "scroll-out moved camera inward/negative: {d}"
        );
    }

    #[test]
    fn wheel_alternating_momentum_burst_stays_bounded() {
        // macOS momentum phases emit alternating-sign bursts. Distance must
        // track the NET scroll, not slam between clamp bounds (the visible
        // jitter). Net here is +20 px → slightly zoomed in from 30.
        let mut w = wheel_world(30.0);
        let d = scroll(
            &mut w,
            MouseScrollUnit::Pixel,
            &[40.0, -30.0, 25.0, -20.0, 10.0, -5.0],
        );
        assert!(d > 10.0 && d < 30.0, "alternating burst jittered: {d}");
    }
}
