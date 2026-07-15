use bevy::input::mouse::MouseMotion;
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
            fov: 1.0,         // radians
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
            camera.pitch =
                (camera.pitch - ev.delta.y * 0.005).clamp(-std::f32::consts::PI + 0.05, std::f32::consts::PI - 0.05);
        }
    }
    for ev in wheel.read() {
        // Scroll up zooms IN: wint's +y (scroll up) must shrink distance, hence
        // the negation. The old sign gave natural/inverted scrolling.
        camera.distance = (camera.distance / (1.0 + ev.y * 0.1)).clamp(2.6, 500.0);
        // 2.6 ≈ bcrit; don't let the camera pass through the shadow.
    }
}
