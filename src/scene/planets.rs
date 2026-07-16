use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::render::material::{SphereData, MAX_PLANETS};

/// A planet rendered as a lensed sphere inside the geodesic shader.
#[derive(Component, Clone, Copy)]
pub struct Planet {
    pub center: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub emissive: bool,
}

/// 轨道根数 (不变量, 启动时随机生成, 运行时不变除非 UI 改种子重生).
#[derive(Component, Clone, Copy)]
pub struct OrbitParams {
    /// k, 乘到 kerr_isco(χ) 上得实际轨道半径.
    pub radius_factor: f32,
    /// 轨道面法向与 Y 轴(自旋轴)的夹角 (rad).
    pub inclination: f32,
    /// 升交点经度 (rad), 决定轨道面在方位上的初始取向.
    pub longitude_of_node: f32,
    /// 轨道内初始相位 (rad).
    pub phase: f32,
}

/// UI 改了种子/count/k 时置位, spawn_planet_system 检测到就重生整个行星系统.
#[derive(Resource, Default)]
pub struct PlanetSystemDirty(pub bool);

/// 由轨道根数 + 当前 (模拟)时间 + 自旋, 计算行星世界空间位置.
/// 纯函数: 无 Bevy 依赖, 可独立测试.
///
/// 物理:
/// - r = k · kerr_isco(χ)
/// - Ω_φ = kerr_orbital_frequency(r, χ)  (轨道角速度)
/// - Ω_LT = kerr_nodal_precession(r, χ)  (轨道面绕 Y 轴的进动率)
/// 轨道面基 (u, v) 由 inclination + longitude_of_node 构造, 然后绕 Y 轴
/// 整体旋转 Ω_LT·t (Lense-Thirring 进动).
pub fn orbit_position(orbit: &OrbitParams, t: f32, chi: f32) -> Vec3 {
    let r = orbit.radius_factor * crate::physics::kerr_isco(chi);
    let omega_phi = crate::physics::kerr_orbital_frequency(r, chi);
    let omega_lt = crate::physics::kerr_nodal_precession(r, chi);

    // 1. 轨道面法向 (Y 轴为极轴的球坐标)
    let inc = orbit.inclination;
    let lon = orbit.longitude_of_node;
    let sin_inc = inc.sin();
    let n = Vec3::new(
        sin_inc * lon.cos(),
        inc.cos(),
        sin_inc * lon.sin(),
    );
    // 2. 轨道面内正交基: u 沿升节点方向, v = n × u
    //    u 在 XZ 平面 (垂直于 Y 轴), 指向升节点
    let u = Vec3::new(-lon.sin(), 0.0, lon.cos());
    let v = n.cross(u);

    // 3. 进动: (u, v) 绕 Y 轴整体旋转 Ω_LT·t
    let pa = omega_lt * t;
    let cp = pa.cos();
    let sp = pa.sin();
    let u_p = Vec3::new(u.x * cp + u.z * sp, u.y, -u.x * sp + u.z * cp);
    let v_p = Vec3::new(v.x * cp + v.z * sp, v.y, -v.x * sp + v.z * cp);

    // 4. 行星在进动后的轨道面内的位置
    let theta = orbit.phase + omega_phi * t;
    r * (theta.cos() * u_p + theta.sin() * v_p)
}

/// 每帧读 OrbitParams + time + spin, 用闭式公式写 Planet.center.
/// 必须在 upload_planets 之前运行 (plugin.rs 用 .before() 保证).
pub fn orbit_system(
    time: Res<Time>,
    params: Res<crate::params::BlackHoleParams>,
    mut query: Query<(&OrbitParams, &mut Planet)>,
) {
    if !params.planets_enabled {
        return;
    }
    // time_scale 放大模拟时间, 让慢进动在合理时间内可见 (Ω_LT 在 r=8 转一圈 ~25 min)
    let t = time.elapsed_secs() * params.planet_time_scale;
    for (orbit, mut planet) in &mut query {
        planet.center = orbit_position(orbit, t, params.spin);
    }
}

/// Collects all Planet components, writes them into the shared MAX_PLANETS-sized
/// `ShaderBuffer` that the material already binds, and updates `planet_count`.
///
/// CRITICAL: we must NOT allocate a new `ShaderBuffer` (and a new handle) each
/// frame. The `#[storage(3, read_only)]` binding resolves the handle via
/// `RenderAssets<GpuShaderBuffer>::get(handle)` and returns
/// `AsBindGroupError::RetryNextUpdate` if the GPU asset for *that exact handle*
/// isn't ready yet. A freshly-added asset has no GPU asset yet, so reassigning
/// the handle every frame makes the fullscreen quad's draw get skipped every
/// frame — the screen shows only the camera clear color (grey).
///
/// Instead, mutate the existing asset in place. `GpuShaderBuffer::prepare_asset`
/// (bevy_render 0.19 `storage.rs`) sees the changed CPU data, reuses the same
/// GPU buffer, and `write_buffer`s the new contents — the handle stays stable,
/// the GPU asset stays ready, and the draw proceeds.
pub fn upload_planets(
    planets: Query<&Planet>,
    mut params: ResMut<crate::params::BlackHoleParams>,
    materials: Res<Assets<crate::render::material::BlackHoleMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let mut data: Vec<SphereData> = planets
        .iter()
        .take(MAX_PLANETS)
        .map(|p| SphereData {
            center: p.center,
            radius: p.radius,
            color: p.color,
            emissive: p.emissive as u32,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        })
        .collect();
    // Pad to MAX_PLANETS so the buffer size is constant (avoids reallocation churn).
    data.resize(MAX_PLANETS, SphereData::default());
    params.planet_count = planets.iter().count().min(MAX_PLANETS) as u32;

    // Write into the existing buffer asset(s) the materials already reference.
    // The startup system pre-creates exactly one such buffer; we find it by the
    // materials' handles and mutate in place — never reallocate the handle.
    // set_data moves a Vec<T> (encase treats Vec<T> as a runtime-sized array),
    // matching the official bevy 0.19 storage_buffer example.
    for (_, mat) in materials.iter() {
        if let Some(mut buffer) = buffers.get_mut(&mat.planets) {
            buffer.set_data(data.clone());
        }
    }
}

/// (重)生成行星系统. 检测 PlanetSystemDirty: 若置位, 先 despawn 所有现有
/// (Planet, OrbitParams), 再用 ChaCha8Rng + params.planet_seed 重新随机生成.
/// 确定性种子 → 同种子给同布局, 方便调试/截图/测试.
///
/// 同时注册在 Startup 和 Update: Startup 首帧靠 "existing 为空" 门控首次生成;
/// Update 路径靠 dirty flag 门控重生. 不会每帧重建.
pub fn spawn_planet_system(
    mut commands: Commands,
    params: Res<crate::params::BlackHoleParams>,
    mut dirty: ResMut<PlanetSystemDirty>,
    existing: Query<Entity, With<Planet>>,
) {
    // 只在 dirty, 或现有行星为零 (首帧/Startup) 时重生.
    if !dirty.0 && !existing.is_empty() {
        return;
    }
    // despawn 现有行星
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    dirty.0 = false;

    if !params.planets_enabled {
        return;
    }

    let mut rng = ChaCha8Rng::seed_from_u64(params.planet_seed as u64);
    let max = crate::render::material::MAX_PLANETS as u32;
    for _ in 0..params.planet_count_target.min(max) {
        let inclination = rng.gen_range(0.0..PI);
        let longitude = rng.gen_range(0.0..TAU);
        let phase = rng.gen_range(0.0..TAU);
        // 半径因子以 UI 的 planet_radius_factor 为中心 ±0.75 散布, 既让滑条
        // 真正控制轨道尺度, 又保留 per-planet 半径多样性 (避免全同半径).
        // 滑条范围 1.5..=5.0, 散布后实际 k ∈ [0.75, 5.75], 钳到 ISCO 外.
        let radius_factor = (params.planet_radius_factor + rng.gen_range(-0.75..0.75)).max(1.0);
        // 颜色: 暖色行星 (橙/红/黄系), 避开蓝色 (易与背景星混淆)
        let hue = rng.gen_range(0.02..0.13);
        let color = hsv_to_rgb(hue, rng.gen_range(0.5..0.9), rng.gen_range(0.7..1.0));
        commands.spawn((
            OrbitParams {
                radius_factor,
                inclination,
                longitude_of_node: longitude,
                phase,
            },
            Planet {
                center: Vec3::ZERO, // 首帧由 orbit_system 填
                radius: rng.gen_range(0.8..1.6),
                color,
                emissive: false,
            },
        ));
    }
}

/// HSV → RGB (h,s,v ∈ [0,1]). 行星颜色用.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Vec3 {
    let i = (h * 6.0).floor() as i32 % 6;
    let f = h * 6.0 - (h * 6.0).floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i {
        0 => Vec3::new(v, t, p),
        1 => Vec3::new(q, v, p),
        2 => Vec3::new(p, v, t),
        3 => Vec3::new(p, q, v),
        4 => Vec3::new(t, p, v),
        _ => Vec3::new(v, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_position_radius_is_preserved() {
        // 不管时间/相位, 行星到原点距离应恒等于 r = k·isco(χ)
        let orbit = OrbitParams {
            radius_factor: 2.5,
            inclination: 0.7,
            longitude_of_node: 1.3,
            phase: 0.5,
        };
        let chi = 0.8;
        let expected_r = 2.5 * crate::physics::kerr_isco(chi);
        for t in [0.0_f32, 1.0, 5.5, 100.0] {
            let pos = orbit_position(&orbit, t, chi);
            let dist = pos.length();
            assert!(
                (dist - expected_r).abs() < 1e-4,
                "t={}: dist {} != r {}",
                t, dist, expected_r
            );
        }
    }

    #[test]
    fn orbit_position_zero_spin_keeps_equatorial_plane() {
        // χ=0: 无进动, 倾角 0 (赤道面) 的行星应严格在 y=0 平面
        let orbit = OrbitParams {
            radius_factor: 3.0,
            inclination: 0.0,
            longitude_of_node: 0.0,
            phase: 0.0,
        };
        for t in [0.0_f32, 1.0, 10.0] {
            let pos = orbit_position(&orbit, t, 0.0);
            assert!(pos.y.abs() < 1e-5, "χ=0 equatorial orbit should stay in y=0 plane at t={}", t);
        }
    }

    #[test]
    fn orbit_position_advances_with_time() {
        // 不同时间应给不同位置 (除非极端巧合)
        let orbit = OrbitParams {
            radius_factor: 3.0,
            inclination: 0.5,
            longitude_of_node: 0.0,
            phase: 0.0,
        };
        let p0 = orbit_position(&orbit, 0.0, 0.5);
        let p1 = orbit_position(&orbit, 1.0, 0.5);
        assert!((p0 - p1).length() > 0.01, "planet should move over time");
    }
}
