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
            yaw: 0.0,
            pitch: 1.3,       // ~75 deg — slightly above the disk plane
            distance: 30.0,
            fov: 1.0,         // radians
        }
    }
}

impl OrbitCamera {
    /// Compute the camera eye position and an orthonormal basis (forward/right/up)
    /// in Bevy's right-handed Y-up coordinate system. The black hole sits at origin.
    /// Disk plane is the xz-plane (y=0); the disk tilt is applied in the shader
    /// via the params, so the camera basis here is in world space.
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
        let world_up = Vec3::Y;
        let right = forward.cross(world_up).normalize();
        let up = right.cross(forward).normalize();
        (eye, forward, right, up)
    }
}

pub fn orbit_controller(
    mut camera: ResMut<OrbitCamera>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
) {
    if mouse.pressed(MouseButton::Left) {
        for ev in motion.read() {
            camera.yaw -= ev.delta.x * 0.005;
            // Clamp pitch to avoid flipping.
            camera.pitch = (camera.pitch + ev.delta.y * 0.005).clamp(0.05, std::f32::consts::PI - 0.05);
        }
    }
    for ev in wheel.read() {
        // Zoom: multiply distance by a factor of the scroll amount.
        camera.distance = (camera.distance * (1.0 + ev.y * 0.1)).clamp(2.6, 500.0);
        // 2.6 ≈ bcrit; don't let the camera pass through the shadow.
    }
}
