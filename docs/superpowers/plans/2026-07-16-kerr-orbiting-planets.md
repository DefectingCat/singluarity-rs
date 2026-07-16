# Kerr 轨道行星系统 实现计划 (Phase 3.4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 5–8 颗行星沿 Kerr 圆轨道绕黑洞旋转,轨道面因 Lense-Thirring 进动绕自旋轴转动,位置每帧 CPU 闭式计算并上传既有 storage buffer。

**Architecture:** 路径 A — 物理/轨道全在 CPU,shader 零改动。新增 `OrbitParams`(不可变根数) + 复用 `Planet`(每帧派生 center)。闭式公式 `Ω_φ`(Bardeen 1972) 与 `Ω_θ`(垂直 epicyclic 频率) 给出精确强场节点进动 `Ω_LT = Ω_φ - Ω_θ`,χ=0 精确退化为牛顿。

**Tech Stack:** Bevy 0.19, Rust edition 2024, `rand` + `rand_chacha`(确定性 PRNG), egui 控制面板。

**Spec:** `docs/superpowers/specs/2026-07-16-kerr-orbiting-planets-design.md`

---

## 文件结构

| 文件 | 责任 | 改动 |
|---|---|---|
| `Cargo.toml` | 声明 `rand` + `rand_chacha` 直接依赖 | 新增 2 行 |
| `src/physics.rs` | `kerr_orbital_frequency` + `kerr_nodal_precession` + 测试 | 新增 ~80 行 |
| `src/scene/planets.rs` | `OrbitParams` 组件 + 轨道几何 + `orbit_system` + `spawn_planet_system` | 大改,删除 `spawn_default_planet` |
| `src/params.rs` | 5 个新 `BlackHoleParams` 字段 + Default | 新增 ~15 行 |
| `src/render/plugin.rs` | 系统注册 + 资源初始化 | 改 ~10 行 |
| `src/ui.rs` | Planets collapsing header | 新增 ~15 行 |

**不改动:** `src/render/material.rs`(`SphereData`/`BlackHoleUniforms` 不动), `assets/shaders/black_hole.wgsl`(shader 零改动)。

---

## Task 0: 添加 rand 依赖

**Files:**
- Modify: `Cargo.toml:10-12`

`rand` 当前只是 bevy 的传递依赖,未在 `Cargo.toml` 直接声明;`rand_chacha` 完全缺失。需要显式声明以便 `ChaCha8Rng` 稳定可用。

- [ ] **Step 1: 添加依赖**

修改 `Cargo.toml` 的 `[dependencies]` 段:

```toml
[dependencies]
bevy = "0.19"
bevy_egui = "0.41"
rand = "0.8"
rand_chacha = "0.3"
```

注:用 `rand 0.8` + `rand_chacha 0.3`(稳定 LTS,API 与 Bevy 0.19 生态兼容)。`seed_from_u64` + `gen_range` API 在 0.8 稳定。

- [ ] **Step 2: 验证编译**

Run: `cargo check`
Expected: 编译通过(可能下载新 crate)。若版本冲突,用 `cargo update -p rand` 或锁到与 bevy 兼容的版本。

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add rand + rand_chacha for deterministic planet seeding"
```

---

## Task 1: Kerr 轨道频率 + 进动率 (physics.rs) — TDD

核心物理公式,先写测试。这部分是整个方案物理正确性的基石。

**Files:**
- Modify: `src/physics.rs`(在文件末尾,`_phantom` 函数之前)
- Test: `src/physics.rs` 内联 `#[cfg(test)]` 模块

### 1a: `kerr_orbital_frequency`

- [ ] **Step 1: 写失败测试**

在 `src/physics.rs` 的 `#[cfg(test)]` mod 里(`fn _phantom` 之后,或现有测试 mod 内)加:

```rust
#[test]
fn orbital_frequency_reduces_to_newton_at_zero_spin() {
    // χ=0: Ω = 1/r^1.5 (牛顿开普勒, Rs=1)
    for r in [4.0_f32, 6.0, 10.0, 20.0] {
        let newton = 1.0 / r.powf(1.5);
        let kerr = kerr_orbital_frequency(r, 0.0);
        assert!(
            (kerr - newton).abs() < 1e-6,
            "χ=0 at r={}: expected {} (newton), got {}",
            r, newton, kerr
        );
    }
}

#[test]
fn orbital_frequency_decreases_with_spin_at_fixed_r() {
    // prograde 轨道 (a>0): Ω_φ 随 χ 减小 (分母 r^1.5+a 增大)
    let r = 8.0;
    let omega_0 = kerr_orbital_frequency(r, 0.0);
    let omega_1 = kerr_orbital_frequency(r, 1.0);
    assert!(omega_1 < omega_0, "prograde Ω should decrease with spin");
}
```

- [ ] **Step 2: 运行测试,确认失败**

Run: `cargo test orbital_frequency`
Expected: FAIL,编译错误 `cannot find function kerr_orbital_frequency`。

- [ ] **Step 3: 实现 `kerr_orbital_frequency`**

在 `src/physics.rs` 的 `kerr_horizon` 函数之后(`kerr_bending_accel` 之前)加:

```rust
/// Kerr 赤道 prograde 圆轨角速度 (Rs=1, M=0.5). Bardeen 1972 eqn 2.16.
/// `Ω_φ = 1 / (r^1.5 + a)`, a = χM = 0.5χ. χ=0 退化为牛顿 1/r^1.5.
pub fn kerr_orbital_frequency(r: f32, chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    1.0 / (r.powf(1.5) + a)
}
```

- [ ] **Step 4: 运行测试,确认通过**

Run: `cargo test orbital_frequency`
Expected: PASS,2 个测试通过。

- [ ] **Step 5: Commit**

```bash
git add src/physics.rs
git commit -m "feat(physics): Kerr equatorial orbital frequency Ω_φ (Bardeen 1972)"
```

### 1b: `kerr_nodal_precession`

- [ ] **Step 1: 写失败测试**

在 `src/physics.rs` 的测试 mod 加:

```rust
#[test]
fn nodal_precession_vanishes_at_zero_spin() {
    // χ=0: 球对称 (Schwarzschild), 无节点进动
    for r in [4.0_f32, 6.0, 10.0, 20.0] {
        let prec = kerr_nodal_precession(r, 0.0);
        assert!(
            prec.abs() < 1e-6,
            "χ=0 at r={} should have zero precession, got {}",
            r, prec
        );
    }
}

#[test]
fn nodal_precession_grows_with_spin() {
    // 固定 r, prograde 节点进动率随 χ 单调增
    let r = 6.0;
    let p_low = kerr_nodal_precession(r, 0.3);
    let p_high = kerr_nodal_precession(r, 0.9);
    assert!(p_high > p_low, "precession should grow with spin");
    assert!(p_low > 0.0, "prograde precession should be positive");
}

#[test]
fn nodal_precession_strong_field_exceeds_weak_field() {
    // r<6 强场区: 精确 Ω_LT > 弱场近似 2Ma/r³ = χ/r³ (M=0.5)
    let r = 4.0;
    let chi = 0.9;
    let weak = chi / r.powi(3); // 2Ma/r³ = (2·0.5·χ)/r³ = χ/r³
    let strong = kerr_nodal_precession(r, chi);
    assert!(
        strong > weak,
        "strong-field precession at r={} should exceed weak approx {} , got {}",
        r, weak, strong
    );
}
```

- [ ] **Step 2: 运行测试,确认失败**

Run: `cargo test nodal_precession`
Expected: FAIL,`cannot find function kerr_nodal_precession`。

- [ ] **Step 3: 实现 `kerr_nodal_precession`**

在 `kerr_orbital_frequency` 之后加:

```rust
/// Kerr 赤道圆轨节点进动率 (Lense-Thirring, 强场精确). χ=0 返回 0.
///
/// `Ω_LT = Ω_φ - Ω_θ`, 其中 Ω_θ 是垂直 epicyclic 频率:
/// `Ω_θ² = Ω_φ² · (1 − 4a·Ω_φ/r + 3a²/r²)` (Caltech Ph236 lec27).
/// χ=0 时 a=0, 括号=1, 故 Ω_θ=Ω_φ, 进动为零 (Schwarzschild 球对称).
pub fn kerr_nodal_precession(r: f32, chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    let omega_phi = kerr_orbital_frequency(r, chi);
    // 垂直 epicyclic 频率比 (>=0, 极端 r/a 组合下数值精度可能略负, 钳位)
    let ratio = (1.0 - 4.0 * a * omega_phi / r + 3.0 * a * a / (r * r)).max(0.0);
    let omega_theta = omega_phi * ratio.sqrt();
    omega_phi - omega_theta
}
```

- [ ] **Step 4: 运行测试,确认通过**

Run: `cargo test nodal_precession`
Expected: PASS,3 个测试通过。

- [ ] **Step 5: 运行全部测试确认无回归**

Run: `cargo test`
Expected: 全部通过(原有 capture/escape 测试 + 5 个新测试)。

- [ ] **Step 6: Commit**

```bash
git add src/physics.rs
git commit -m "feat(physics): Kerr nodal precession Ω_LT (strong-field Lense-Thirring)"
```

---

## Task 2: OrbitParams 组件 + 轨道几何纯函数

先把不可变根数和"根数 + 时间 → 位置"的纯函数定下来。纯函数可独立测试,不依赖 Bevy 系统。

**Files:**
- Modify: `src/scene/planets.rs`(文件顶部,`Planet` struct 之后)
- Test: `src/scene/planets.rs` 内联 `#[cfg(test)]` mod

- [ ] **Step 1: 加 `OrbitParams` 组件 + 轨道几何函数(含测试 mod)**

在 `src/scene/planets.rs` 的 `Planet` struct 定义之后(当前 `:8-13`)加:

```rust
use std::f32::consts::{PI, TAU};

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
```

- [ ] **Step 2: 写几何不变量测试**

在 `src/scene/planets.rs` 文件末尾加测试 mod:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

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
    fn orbit_position_zero_spin_keeps_plane_fixed() {
        // χ=0: 无进动, 倾角 0 (赤道面) 的行星应严格在 y=0 平面
        let orbit = OrbitParams {
            radius_factor: 3.0,
            inclination: 0.0, // 赤道面
            longitude_of_node: 0.0,
            phase: 0.0,
        };
        for t in [0.0_f32, 1.0, 10.0] {
            let pos = orbit_position(&orbit, t, 0.0);
            assert!(pos.y.abs() < 1e-5, "χ=0 equatorial orbit should stay in y=0 plane at t={}", t);
        }
    }

    #[test]
    fn orbit_position_advance_with_time() {
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
```

- [ ] **Step 3: 运行测试**

Run: `cargo test --lib scene::planets`
Expected: PASS,3 个几何测试通过。

注:若 `--lib` 选择器不工作,用 `cargo test orbit_position`。

- [ ] **Step 4: Commit**

```bash
git add src/scene/planets.rs
git commit -m "feat(planets): OrbitParams component + orbit_position geometry"
```

---

## Task 3: orbit_system (每帧更新 Planet.center)

把纯函数接进 Bevy 调度,每帧写 `Planet.center`。

**Files:**
- Modify: `src/scene/planets.rs`

- [ ] **Step 1: 加 `orbit_system`**

在 `orbit_position` 函数之后加。注:`Time` 已在 `bevy::prelude::*` 里(`planets.rs:1` 已 import prelude,参考 `plugin.rs:582` 的 `time: Res<Time>` 用法),无需额外 import。

```rust
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
```

- [ ] **Step 2: 验证编译**

Run: `cargo check`
Expected: 编译通过。会报 `planets_enabled` / `planet_time_scale` 字段不存在——这是 Task 4 要加的。**若如此,先做 Task 4 再回来验证。**

- [ ] **Step 3: Commit(字段未加前不 commit;待 Task 4 完成后一起验证再 commit)**

暂不 commit。

---

## Task 4: BlackHoleParams 新字段

**Files:**
- Modify: `src/params.rs:107-153`(struct 定义) + `:156-211`(Default impl)

- [ ] **Step 1: 加字段到 struct**

在 `src/params.rs` 的 `BlackHoleParams` struct 里,`aa_quality: AaQuality,`(`:153`)之后加:

```rust
    // Planets (Phase 3.4: Kerr orbiting planets)
    pub planets_enabled: bool,
    pub planet_count_target: u32,  // 0..=8
    pub planet_radius_factor: f32, // k, 乘到 kerr_isco(χ) 上
    pub planet_seed: u32,          // ChaCha8Rng 种子, 改了触发系统重生
    pub planet_time_scale: f32,    // 模拟时间放大 (进动很慢, 需放大才可见)
```

- [ ] **Step 2: 加 Default 值**

在 `impl Default for BlackHoleParams` 的 `aa_quality: ...`(`:208`)之后加:

```rust
            // Planets: 6 颗, k=2.5 (r ∈ [1.25, 7.5], 横跨强场区),
            // 种子 42, time_scale 50× (Ω_LT 在 r=8 转一圈 ~25 min, 放大才可见).
            planets_enabled: true,
            planet_count_target: 6,
            planet_radius_factor: 2.5,
            planet_seed: 42,
            planet_time_scale: 50.0,
```

- [ ] **Step 3: 验证编译 (回 Task 3 的待验证项)**

Run: `cargo check`
Expected: 编译通过,`orbit_system` 现在能找到 `planets_enabled` / `planet_time_scale`。

- [ ] **Step 4: Commit**

```bash
git add src/params.rs src/scene/planets.rs
git commit -m "feat(params): planet system params + orbit_system wiring"
```

---

## Task 5: spawn_planet_system (随机生成 + despawn 旧的)

取代 `spawn_default_planet`。用确定性 PRNG,改种子时整个系统重生。

**Files:**
- Modify: `src/scene/planets.rs`
- Modify: `src/render/plugin.rs:113`(注册) + 资源初始化

- [ ] **Step 1: 加 dirty-flag 资源 + spawn_planet_system**

在 `src/scene/planets.rs` 顶部 `use` 区加(`bevy::prelude::*` 已含 `Commands`/`Query`/`Entity`/`Resource`/`With`/`Vec3`):

```rust
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand::Rng;
```

在 `OrbitParams` struct 之后加 dirty flag 资源:

```rust
/// UI 改了种子/count/k 时置位, spawn_planet_system 检测到就重生整个行星系统.
#[derive(Resource, Default)]
pub struct PlanetSystemDirty(pub bool);
```

用新函数取代 `spawn_default_planet`(删除 `:66-73` 的整个 `spawn_default_planet`):

```rust
/// (重)生成行星系统. 检测 PlanetSystemDirty: 若置位, 先 despawn 所有现有
/// (Planet, OrbitParams), 再用 ChaCha8Rng + params.planet_seed 重新随机生成.
/// 确定性种子 → 同种子给同布局, 方便调试/截图/测试.
pub fn spawn_planet_system(
    mut commands: Commands,
    params: Res<crate::params::BlackHoleParams>,
    mut dirty: ResMut<PlanetSystemDirty>,
    existing: Query<Entity, With<Planet>>,
) {
    // 只在 dirty 时重生 (避免每帧重建). 首帧 dirty 默认 false → 需要初始 spawn.
    // 用 Resource Default 给的 false + 一个 startup 标记, 或始终在 Startup 调一次.
    // 简化: 此系统同时在 Startup 和 Update 注册; Update 路径靠 dirty 门控,
    // Startup 路径靠 "现有为零" 门控.
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
    for _ in 0..params.planet_count_target.min(crate::render::material::MAX_PLANETS as u32) {
        let inclination = rng.gen_range(0.0..PI);
        let longitude = rng.gen_range(0.0..TAU);
        let phase = rng.gen_range(0.0..TAU);
        let radius_factor = rng.gen_range(2.0..4.0);
        // 颜色: 暖色行星 (橙/红/黄系), 避开蓝色 (易与背景星混淆)
        let hue = rng.gen_range(0.02..0.13); // 橙红色相
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
```

注:`hsv_to_rgb` 的 `i32 % 6` 处理 h=1.0 边界;`hue 0.02..0.13` 给橙红色相,避免与背景星(蓝/白)混淆。

- [ ] **Step 2: 改 plugin.rs 注册**

`src/render/plugin.rs:96-98` 的 `init_resource` 链,在 `WantsPointer` 之后加 `PlanetSystemDirty`:

把:
```rust
        app.init_resource::<crate::camera::OrbitCamera>()
            .init_resource::<crate::camera::WantsPointer>()
            .init_resource::<crate::params::BlackHoleParams>()
```
改为(在 `BlackHoleParams` 之后加一行,注意 `init_resource::<BlackHoleParams>()` 后原本没有 `.` 链式调用——检查上下文,它可能用 `;` 结束。读 `plugin.rs:96-100` 确认):
```rust
        app.init_resource::<crate::camera::OrbitCamera>()
            .init_resource::<crate::camera::WantsPointer>()
            .init_resource::<crate::params::BlackHoleParams>()
            .init_resource::<crate::scene::planets::PlanetSystemDirty>();
```

若 `init_resource::<BlackHoleParams>()` 后是 `;` 而非 `.`(即链已断),把新行单独写:
```rust
        app.init_resource::<crate::scene::planets::PlanetSystemDirty>();
```
放在 `BlackHoleParams` init 之后。

- [ ] **Step 3: 改系统注册**

`src/render/plugin.rs:113`:
```rust
            .add_systems(Startup, crate::scene::planets::spawn_default_planet)
```
改为:
```rust
            .add_systems(Startup, crate::scene::planets::spawn_planet_system)
```

`:123`:
```rust
            .add_systems(Update, crate::scene::planets::upload_planets)
```
改为(加 orbit_system 在 upload_planets 前, 加 spawn_planet_system 在 Update):
```rust
            .add_systems(Update, crate::scene::planets::spawn_planet_system)
            .add_systems(
                Update,
                crate::scene::planets::orbit_system
                    .before(crate::scene::planets::upload_planets),
            )
            .add_systems(Update, crate::scene::planets::upload_planets)
```

注:`spawn_planet_system` 放 Update 是为了检测 dirty flag 重生。它内部靠 dirty + `existing.is_empty()` 门控,不会每帧重建。

- [ ] **Step 4: 验证编译**

Run: `cargo check`
Expected: 编译通过。`spawn_default_planet` 已删除,无悬空引用。

- [ ] **Step 5: 验证启动**

Run: `cargo run --release`
Expected: 应用启动,看到 ~6 颗行星在轨道上。可能位置/速度还需调(time_scale 50× 下进动应可见)。

- [ ] **Step 6: Commit**

```bash
git add src/scene/planets.rs src/render/plugin.rs
git commit -m "feat(planets): spawn_planet_system with deterministic ChaCha8Rng seeding"
```

---

## Task 6: UI 控制面板

**Files:**
- Modify: `src/ui.rs`(在 Accretion Disk header 之后,约 `:34` 段之后)

- [ ] **Step 1: 定位插入点**

读 `src/ui.rs`,找 "Accretion Disk" collapsing header 结束的位置(下一个 `egui::CollapsingHeader::new` 之前)。

- [ ] **Step 2: 加 Planets header**

在 Accretion Disk header 之后加(具体缩进对齐现有代码):

```rust
                egui::CollapsingHeader::new("Planets")
                    .default_open(false)
                    .show(ui, |ui| {
                        let was_enabled = params.planets_enabled;
                        ui.checkbox(&mut params.planets_enabled, "Enable");
                        ui.add(egui::Slider::new(&mut params.planet_count_target, 0..=8).text("Count"));
                        ui.add(egui::Slider::new(&mut params.planet_radius_factor, 1.5..=5.0).text("Radius factor k"));
                        let isco = crate::physics::kerr_isco(params.spin);
                        ui.label(format!("ISCO: {:.3} → r = {:.3}", isco, params.planet_radius_factor * isco));
                        ui.add(egui::Slider::new(&mut params.planet_seed, 0..=1000).text("Seed"));
                        ui.add(egui::Slider::new(&mut params.planet_time_scale, 1.0..=200.0).text("Time scale"));
                    });
```

- [ ] **Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过。

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs
git commit -m "feat(ui): Planets control panel header"
```

---

## Task 7: dirty flag 联动 (UI 改种子/count/k 时重生)

当前 UI 改 `planet_seed` / `planet_count_target` / `planets_enabled` 不会触发 `spawn_planet_system` 重生。需要在 `ui_system` 里检测变化并置 `PlanetSystemDirty`。

**Files:**
- Modify: `src/ui.rs`(`ui_system` 签名 + Planets header)

- [ ] **Step 1: 改 ui_system 签名加 PlanetSystemDirty**

读 `src/ui.rs:4-9`,当前签名:
```rust
pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut camera: ResMut<crate::camera::OrbitCamera>,
    mut wants: ResMut<crate::camera::WantsPointer>,
) {
```
加 `PlanetSystemDirty`:
```rust
pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut camera: ResMut<crate::camera::OrbitCamera>,
    mut wants: ResMut<crate::camera::WantsPointer>,
    mut planet_dirty: ResMut<crate::scene::planets::PlanetSystemDirty>,
) {
```

- [ ] **Step 2: 在 Planets header 里检测变化置 dirty**

把 Task 6 加的 Planets header 改为(记录改前值,改后对比):

```rust
                egui::CollapsingHeader::new("Planets")
                    .default_open(false)
                    .show(ui, |ui| {
                        let prev = (
                            params.planets_enabled,
                            params.planet_count_target,
                            params.planet_radius_factor,
                            params.planet_seed,
                        );
                        ui.checkbox(&mut params.planets_enabled, "Enable");
                        ui.add(egui::Slider::new(&mut params.planet_count_target, 0..=8).text("Count"));
                        ui.add(egui::Slider::new(&mut params.planet_radius_factor, 1.5..=5.0).text("Radius factor k"));
                        let isco = crate::physics::kerr_isco(params.spin);
                        ui.label(format!("ISCO: {:.3} → r = {:.3}", isco, params.planet_radius_factor * isco));
                        ui.add(egui::Slider::new(&mut params.planet_seed, 0..=1000).text("Seed"));
                        ui.add(egui::Slider::new(&mut params.planet_time_scale, 1.0..=200.0).text("Time scale"));
                        let curr = (
                            params.planets_enabled,
                            params.planet_count_target,
                            params.planet_radius_factor,
                            params.planet_seed,
                        );
                        if curr != prev {
                            planet_dirty.0 = true;
                        }
                    });
```

注:`planet_time_scale` 不触发重生(它只影响 orbit_system 的时间放大,不需重建实体)。

- [ ] **Step 3: spin 变化也触发重生**

Spin 改变 ISCO → 轨道半径变。读 `src/ui.rs` 的 "Black Hole" header(`:27-33`),在 spin slider 后加 dirty 标记。或者更简单:在 `ui_system` 末尾统一检测 spin 变化。

最简方案:在 `ui_system` 开头记录 `params.spin` 旧值,结尾对比。但这会污染整个函数。**推荐:** 在 "Black Hole" header 的 spin slider 后直接加:

读 `src/ui.rs:30` 附近:
```rust
                        ui.add(egui::Slider::new(&mut params.spin, 0.0..=1.0).text("Spin (χ)"));
```
改为(加 dirty 联动)——但这里需要 prev/curr。由于 spin 改变只影响半径(连续),不必重生实体(orbit_system 每帧读 spin)。**决定:spin 不触发重生**——orbit_system 已每帧读 `params.spin`,半径会平滑变化。只有种子/count/k 这些"根数"改变才需重生。

保持 Task 7 Step 2 的实现即可,spin 不加联动。

- [ ] **Step 4: 验证编译**

Run: `cargo check`
Expected: 编译通过。

- [ ] **Step 5: 验证 dirty 重生**

Run: `cargo run --release`
手动测试:在 UI 里拖动 Seed 滑条 → 行星布局应立即改变。拖动 Count → 行星数变化。拖动 k → 半径变化(可能需重生才体现新 k 的随机分布)。

- [ ] **Step 6: Commit**

```bash
git add src/ui.rs
git commit -m "feat(ui): planet dirty flag on seed/count/k change triggers respawn"
```

---

## Task 8: 集成验证 + 视觉调参

**Files:** 无代码改动(纯验证 + 可能微调默认值)

- [ ] **Step 1: 运行全部测试**

Run: `cargo test`
Expected: 所有测试通过:
- 原有 physics capture/escape 测试
- `orbital_frequency_*` (2)
- `nodal_precession_*` (3)
- `orbit_position_*` (3)

共 ~10+ 测试全绿。

- [ ] **Step 2: 桌面端视觉检查**

Run: `cargo run --release`
检查项:
- [ ] 启动后看到 ~6 颗行星
- [ ] 行星在轨道上运动(角速度可见)
- [ ] 拖动 Time Scale → 进动速率变化(高 time_scale 下轨道面绕 Y 轴转动可见)
- [ ] 拖动 Spin (χ) → 从 0 到 1:χ=0 时轨道面固定,χ>0 时进动出现
- [ ] 拖动 Seed → 行星布局重生
- [ ] 行星被引力透镜扭曲(爱因斯坦环/弧)——这是 shader 既有功能,验证位置上传正确
- [ ] 行星不会被吸积盘完全淹没(随机倾角应让多数行星偏离盘面)

- [ ] **Step 3: Web 端编译检查**

Run: `cargo check --target wasm32-unknown-unknown`
Expected: 编译通过。`ChaCha8Rng` 在 wasm 可用(纯计算,无平台依赖)。

- [ ] **Step 4: 微调默认值(若需要)**

若视觉不佳,调 `src/params.rs` 的 Default:
- 行星太小/大 → 调 `spawn_planet_system` 里 `radius: rng.gen_range(0.8..1.6)` 的范围
- 进动太慢/快 → 调 `planet_time_scale: 50.0`
- 行星太暗 → 调 `hsv_to_rgb` 的 value 范围,或让部分行星 `emissive: true`
- 颜色不好看 → 调 hue 范围 `0.02..0.13`

- [ ] **Step 5: 最终 commit(若有调参)**

```bash
git add -A
git commit -m "tune(planets): default visual parameters after visual check"
```

---

## Self-Review 结论

**1. Spec 覆盖:**
- ✅ 物理模型(Ω_φ, Ω_LT) → Task 1
- ✅ OrbitParams 组件 → Task 2
- ✅ 轨道几何 → Task 2 (`orbit_position`)
- ✅ orbit_system → Task 3
- ✅ BlackHoleParams 字段 → Task 4
- ✅ spawn_planet_system + ChaCha8Rng → Task 5
- ✅ UI 控制面板 → Task 6
- ✅ dirty flag 重生 → Task 7
- ✅ SphereData/shader 不动 → 全程未涉及
- ✅ 测试 → Task 1 (5 个) + Task 2 (3 个)
- ✅ χ=0 退化 → Task 1 测试覆盖

**2. 占位符扫描:** 无 TBD/TODO。所有代码块完整。

**3. 类型一致性:** `OrbitParams` 字段名(radius_factor, inclination, longitude_of_node, phase)在 Task 2/3/5 一致。`PlanetSystemDirty(pub bool)` 在 Task 5/7 一致。`planets_enabled` / `planet_time_scale` 在 Task 4/5/6/7 一致。

**4. 调度顺序:** orbit_system `.before(upload_planets)` (Task 5 Step 3) 保证上传最新位置。spawn_planet_system 在 Update 靠 dirty 门控。
