# Kerr 轨道行星系统（Phase 3.4）

**日期：** 2026-07-16
**状态：** 设计稿，待实现
**前置：** Phase 2（Kerr 光线积分）、Phase 3.1（体积盘）

## 目标

让 5–8 颗行星沿 Kerr 时空的**圆轨道**绕黑洞旋转，轨道面因 Lense–Thirring 效应绕黑洞自旋轴进动。行星位置每帧由 CPU 用闭式解析公式计算，写入既有 storage buffer；渲染端 shader **不改动**。

这是 AGENTS.md 里档次 3 的方案：Kerr 圆轨 + 强场节点进动。路径 A（CPU 算位置 + 现有上传机制），shader 零改动。

## 非目标

- **不做完整类时测地线**（偏心、倾轨、plunging）。理由：圆轨道下径向频率 `Ω_r = 0`，Mino 时间的三频率分解退化为纯 `Ω_φ`；上 Mino 时间需要椭圆函数（WGSL 与 std 都没有），破坏 `physics.rs` 单一可测试镜像的设计原则。这不是"测地线可视化"项目。
- **不改渲染端**。`planet_hit`、`SphereData`、storage buffer 布局全部不动。进动和轨道运动只在 CPU 端，最终位置照常上传。
- **不做行星-行星引力相互作用**。测试粒子近似。

## 物理模型

全部闭式公式，无数值积分。自然单位 `Rs = 1`，故 `M = 0.5`，`a = χM = 0.5χ`。

### 轨道角速度 `Ω_φ`

赤道 prograde 圆轨，[Bardeen 1972, eqn 2.16](https://physics.stackexchange.com/questions/502796/how-to-derive-the-angular-velocity-of-circular-orbits-in-kerr-geometry)：

```
Ω_φ(r, χ) = 1 / (r^1.5 + a)        // a = 0.5χ
```

`χ = 0` 时 `a = 0`，退化为牛顿开普勒 `Ω = 1/r^1.5`。

### 节点进动率 `Ω_LT`

轨道面绕黑洞自旋轴（Y 轴）的进动率。精确强场形式：

```
Ω_LT(r, χ) = Ω_φ(r, χ) − Ω_θ(r, χ)
```

其中 `Ω_θ` 是 Kerr 赤道圆轨的垂直 epicyclic 频率，闭式表达（[Caltech Ph236 lec27](http://www.tapir.caltech.edu/~chirata/ph236/2011-12/lec27.pdf)；[Okazaki、Kato 等的 标准 epicyclic 频率结果](https://arxiv.org/pdf/1304.6936)）：

```
Ω_θ² = Ω_φ² · (1 − 4a·Ω_φ/r + 3a²/r²)        // a = 0.5χ
Ω_θ  = Ω_φ · sqrt(1 − 4a·Ω_φ/r + 3a²/r²)
```

**退化验证：** `χ = 0` 时 `a = 0`，括号内 = 1，故 `Ω_θ = Ω_φ`，`Ω_LT = 0`——精确退化为"轨道面固定"（Schwarzschild 球对称），满足 AGENTS.md 的核心不变量。

**弱场极限交叉验证：** 大 `r` 时展开 `Ω_φ ≈ r^−1.5`，`Ω_θ ≈ Ω_φ(1 − 1.5·(2Ma/r³)/Ω_φ · ...)`，最终 `Ω_LT → 2Ma/r³ = χ/r³`（M=0.5）。此弱场极限用作测试断言，不用于渲染。

**实现：** `kerr_nodal_precession(r, chi)` 封装上述两式，返回 `Ω_φ - Ω_θ`。注意括号内可能因数值精度略负（极端 r/a 组合），`sqrt` 前用 `.max(0.0)` 钳位。

### 轨道半径

动态绑定 Kerr ISCO（`physics.rs:65` 已实现的 `kerr_isco`）：

```
r = k · kerr_isco(χ)
```

`k` 是 UI 可调的倍数（默认 2.5）。ISCO 从 `χ=0` 的 3 缩到 `χ=1` 的 0.5，故默认 `k=2.5` 给 `r ∈ [1.25, 7.5]`，**横跨强场区**——这是选强场 `Ω_LT` 而非弱场近似的物理理由。

### χ=0 退化验证表

| 量 | χ = 0 | 含义 |
|---|---|---|
| `Ω_φ` | `1/r^1.5` | 牛顿开普勒 |
| `Ω_LT` | `0` | 无进动，轨道面固定 |
| `r` | `k · 3` | Schwarzschild ISCO = 6M = 3 Rs |

## 组件与数据结构

### `OrbitParams`（轨道根数，启动时随机生成）

不可变，除非 UI 改种子重生整个系统。

```rust
#[derive(Component, Clone, Copy)]
pub struct OrbitParams {
    pub radius_factor: f32,     // k, 乘到 kerr_isco(χ) 上得实际半径
    pub inclination: f32,       // 轨道面法向与 Y 轴夹角 (rad)
    pub longitude_of_node: f32, // 升交点经度 (rad), 决定初始进动相位
    pub phase: f32,             // 轨道内初始相位 (rad)
}
```

### `Planet`（渲染状态，每帧重算 center）

字段不变，但 `center` 从"静态坐标"变成"每帧由 orbit_system 派生"。

```rust
#[derive(Component, Clone, Copy)]
pub struct Planet {
    pub center: Vec3,   // ← 每帧重算
    pub radius: f32,
    pub color: Vec3,
    pub emissive: bool,
}
```

**为什么拆两个组件：** `OrbitParams` 是不可变根数，`Planet.center` 是派生量。分开后 `orbit_system` 只写 `Planet.center`、读 `OrbitParams`——职责清晰，且轨道力学逻辑可独立测试，不依赖渲染数据结构。符合 AGENTS.md 的 deep-module 原则。

### `SphereData`（GPU 布局）— 不变

`render/material.rs:122` 的 `SphereData { center, radius, color, emissive, _pad0..2 }` **不改**。进动和轨道运动全在 CPU 算完，只把最终 `center` 写进 buffer。这是路径 A 的核心好处。

## 轨道几何

给定 `OrbitParams` 和当前时间 `t`，计算行星世界空间位置。

### 1. 轨道面法向与基向量

从倾角 `i` 和升交点 `Ω`（用 `longitude_of_node`）解析构造轨道面内两个正交单位基：

```rust
// 轨道面法向 (Y 轴为极轴的球坐标)
let n = Vec3::new(
    i.sin() * Omega.cos(),
    i.cos(),
    i.sin() * Omega.sin(),
);
// 轨道面内基: u 沿升节点方向, v = n × u
let u = Vec3::new(-Omega.sin(), 0.0, Omega.cos());
let v = n.cross(u);   // 已单位化 (u, n 均单位且正交)
```

### 2. 进动

整个 `(u, v)` 基绕 Y 轴旋转 `Ω_LT · t`。用旋转矩阵作用：

```rust
let prec_angle = Omega_LT * t;
let cos_p = prec_angle.cos();
let sin_p = prec_angle.sin();
// 绕 Y 轴: x' = x cos + z sin, z' = -x sin + z cos
let u_prec = Vec3::new(
    u.x * cos_p + u.z * sin_p,
    u.y,
    -u.x * sin_p + u.z * cos_p,
);
let v_prec = Vec3::new(
    v.x * cos_p + v.z * sin_p,
    v.y,
    -v.x * sin_p + v.z * cos_p,
);
```

### 3. 行星位置

```rust
let theta = phase + Omega_phi * t;
let r = radius_factor * kerr_isco(chi);
let center = r * (theta.cos() * u_prec + theta.sin() * v_prec);
```

**disk_tilt 的处理：** shader 的 `planet_hit`（`black_hole.wgsl:632`）会把世界空间球心用 `rot_x(center, -disk_tilt)` 转进盘局部空间。所以轨道平面定义在**世界空间**，倾斜交给 shader——轨道系统不读 `disk_tilt`。

## 随机生成

5–8 颗，全随机散布。用确定性 PRNG（`ChaCha8Rng` + UI 种子），改种子时整个系统可复现地重生。

```rust
pub fn spawn_planet_system(
    mut commands: Commands,
    params: Res<BlackHoleParams>,
    seed: Res<PlanetSeed>,
) {
    // 先 despawn 现有 (Planet, OrbitParams) — 由系统签名 query 完成
    let mut rng = ChaCha8Rng::seed_from_u64(seed.0);
    for _ in 0..params.planet_count_target {
        let inclination = rng.gen_range(0.0..PI);
        let longitude   = rng.gen_range(0.0..TAU);
        let phase       = rng.gen_range(0.0..TAU);
        let radius_factor = rng.gen_range(2.0..4.0);
        commands.spawn((
            OrbitParams { radius_factor, inclination, longitude_of_node: longitude, phase },
            Planet {
                center: Vec3::ZERO,  // 首帧由 orbit_system 填
                radius: rng.gen_range(0.8..1.6),
                color: random_planet_color(&mut rng),
                emissive: false,
            },
        ));
    }
}
```

`ChaCha8Rng`（不是 `thread_rng()`）的理由：
1. **可复现**——UI 改种子时整个系统重生，同样种子给同样布局，方便调试和截图对比。
2. **可测试**——CPU 测试能 seed 固定值断言生成的根数。
3. **跨平台一致**——web 与 desktop 同种子给同布局。

依赖：`rand`（通常已是 Bevy 间接依赖）+ `rand_chacha`。若 `rand` 未在 `Cargo.toml` 直接声明，需加上。

## 系统调度

### 新增系统

```rust
// Update: 读 OrbitParams + time + spin, 写 Planet.center
fn orbit_system(
    time: Res<Time>,
    params: Res<BlackHoleParams>,
    mut query: Query<(&OrbitParams, &mut Planet)>,
)
```

逻辑：对每个 `(orbit, planet)`，用上面的轨道几何公式算 `center`，写入 `planet.center`。

**调度顺序：** `orbit_system` 必须在 `upload_planets` 之前跑（否则上传的是上一帧位置）。用 Bevy 的 `.before(upload_planets)` 或同一个 `SystemSet` 排序。

### 改动的系统

- **`spawn_default_planet`（`planets.rs:66`）→ 删除**。由 `spawn_planet_system` 取代。
- **`upload_planets`（`planets.rs:30`）→ 不变**。它已经每帧全量重写 buffer，天然支持动态位置。
- **`mirror_params`（`plugin.rs:579`）→ 加几行**，把新参数镜像进 uniform（见下）。

### 插件注册（`plugin.rs`）

```rust
.add_systems(Startup, spawn_planet_system)       // 取代 spawn_default_planet
.add_systems(
    Update,
    orbit_system.before(crate::scene::planets::upload_planets),
)
```

## 参数镜像（end-to-end，4 处锁步）

按 AGENTS.md 的"Mirroring a new param"约定，每个新参数要改 4 处。这次新增的参数只影响 CPU 端的轨道计算，**不需要进 GPU uniform**——`Omega_φ`、`Omega_LT`、轨道几何全在 CPU 算，shader 只拿最终 `center`。

所以实际的锁步是 **3 处**（不是 4）：

1. **`BlackHoleParams`（`params.rs`）** — 加字段：
   ```rust
   pub planets_enabled: bool,        // 行星系统开关
   pub planet_count_target: u32,     // 5–8
   pub planet_radius_factor: f32,    // k, 默认 2.5
   pub planet_seed: u32,             // 随机种子
   pub planet_time_scale: f32,       // 时间加速 (进动很慢, 需放大才可见)
   ```
2. **`mirror_params`（`plugin.rs`）** — 把 `planet_count` 改为反映实际活动行星数（已有逻辑，保持），其余新字段不进 uniform。
3. **`ui.rs`** — 新增 "Planets" collapsing header（见下）。

**没有第 4 处 shader 改动**，因为 `SphereData` 和 `BlackHoleUniforms` 都不改。

### `PlanetSeed` 资源

```rust
#[derive(Resource, Clone, Copy)]
pub struct PlanetSeed(pub u32);
```

种子变更需要触发行星系统重生。两种做法：

- **方案 A（简单）：** UI 里把种子滑动条改成"改了就标记 dirty"，下一帧 `spawn_planet_system` 检测到 dirty 就 despawn + respawn。需要一个 `PlanetSystemDirty` 资源。
- **方案 B（事件）：** UI 发 `RespawnPlanets` 事件，专门系统消费。

推荐 **方案 A**——dirty flag 足够，事件系统对这个规模过度设计。

## UI（`ui.rs`）

新增 collapsing header，放在 "Accretion Disk" 之后：

```rust
egui::CollapsingHeader::new("Planets")
    .default_open(false)
    .show(ui, |ui| {
        ui.checkbox(&mut params.planets_enabled, "Enable");
        ui.add(egui::Slider::new(&mut params.planet_count_target, 0..=8).text("Count"));
        ui.add(egui::Slider::new(&mut params.planet_radius_factor, 1.5..=5.0).text("Radius factor k"));
        ui.label(format!("ISCO: {:.3} → r = {:.3}",
            crate::physics::kerr_isco(params.spin),
            params.planet_radius_factor * crate::physics::kerr_isco(params.spin)));
        ui.add(egui::Slider::new(&mut params.planet_seed, 0..=1000).text("Seed"));
        ui.add(egui::Slider::new(&mut params.planet_time_scale, 1.0..=200.0).text("Time scale"));
    });
```

**Time scale 的必要性：** `Ω_LT` 在 r=8 处约 `0.004 rad/s`，转一圈要 ~25 分钟。不放大根本看不见进动。`time_scale` 是让进动在合理时间内可见的必要旋钮，不是物理作弊——它等价于"模拟时间流速"。

## 测试策略

按 AGENTS.md，测试面是 `physics.rs` 的 CPU 镜像。新增的轨道公式应该进 `physics.rs` 并加测试。

### 进 `physics.rs` 的函数

```rust
/// Kerr 赤道 prograde 圆轨角速度 (Rs=1, M=0.5). Bardeen 1972 eqn 2.16.
/// χ=0 退化为牛顿 1/r^1.5.
pub fn kerr_orbital_frequency(r: f32, chi: f32) -> f32 {
    let a = 0.5 * chi;
    1.0 / (r.powf(1.5) + a)
}

/// Kerr 圆轨节点进动率 (Lense-Thirring, 强场精确). χ=0 返回 0.
pub fn kerr_nodal_precession(r: f32, chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    let omega_phi = kerr_orbital_frequency(r, chi);
    // 垂直 epicyclic 频率 (Caltech Ph236 lec27):
    // Ω_θ² = Ω_φ² · (1 − 4a·Ω_φ/r + 3a²/r²)
    let ratio = (1.0 - 4.0 * a * omega_phi / r + 3.0 * a * a / (r * r)).max(0.0);
    let omega_theta = omega_phi * ratio.sqrt();
    omega_phi - omega_theta
}
```

### 测试用例（`#[cfg(test)]` in `physics.rs`）

```rust
#[test]
fn orbital_frequency_reduces_to_newton_at_zero_spin() {
    // χ=0: Ω = 1/r^1.5
    let r = 8.0;
    let newton = 1.0 / r.powf(1.5);
    assert!((kerr_orbital_frequency(r, 0.0) - newton).abs() < 1e-6);
}

#[test]
fn nodal_precession_vanishes_at_zero_spin() {
    // χ=0: 球对称, 无进动
    for r in [4.0, 6.0, 10.0, 20.0] {
        assert!(kerr_nodal_precession(r, 0.0).abs() < 1e-6,
            "χ=0 at r={} should have zero precession", r);
    }
}

#[test]
fn nodal_precession_grows_with_spin() {
    // 固定 r, 进动率随 χ 单调增
    let r = 6.0;
    let p_low = kerr_nodal_precession(r, 0.3);
    let p_high = kerr_nodal_precession(r, 0.9);
    assert!(p_high > p_low, "precession should grow with spin");
}

#[test]
fn nodal_preception_strong_field_exceeds_weak_field() {
    // r<6 强场区: 精确 Ω_LT > 弱场近似 2Ma/r³
    let r = 4.0;
    let chi = 0.9;
    let weak = chi / r.powi(3);  // 2Ma/r³ = χ/r³ (M=0.5)
    let strong = kerr_nodal_precession(r, chi);
    assert!(strong > weak, "strong-field precession should exceed weak at r={}", r);
}

#[test]
fn orbital_radius_tracks_isco() {
    // r = k · isco(χ), 随 χ 收缩
    let k = 2.5;
    let r0 = k * kerr_isco(0.0);   // 7.5
    let r1 = k * kerr_isco(1.0);   // 1.25
    assert!(r1 < r0);
}
```

### 轨道几何测试

`u, v, n` 的正交性、绕 Y 轴旋转保持手性、`χ=0` 时位置退化为赤道圆——这些可以用 `ChaCha8Rng` 固定种子生成 `OrbitParams`，断言几何不变量。

## 性能分析

### GPU 开销：零

`planet_hit`（`black_hole.wgsl:626`）不变。`planet_count` 从 1 涨到 ~8，`planet_hit` 内循环（`:630`）从 1 轮涨到 8 轮——每步每像素多 7 次射线-球求交。对 300 steps × ~1.16M 像素（desktop），这是 ~24 亿次额外求交/帧，但每次求交是 ~10 次 FLOP，GPU 上微秒级。

实测验证项：`planet_count=8` vs `planet_count=0` 的帧时间差应 < 1ms。

### CPU 开销：可忽略

每帧每颗行星：1 次 `kerr_isco` + 1 次 `kerr_orbital_frequency` + 1 次 `kerr_nodal_precession` + 1 次 `sin` + 1 次 `cos` + ~20 次乘加。8 颗 = ~200 次 FLOP/帧，亚微秒。

### 上传开销：与现状相同

`upload_planets` 每帧全量重写 32 颗 `SphereData`（1.5KB）的逻辑不变——加轨道运动不增加它。

## 风险与缓解

1. **`Ω_θ` 闭式公式找错** → 用弱场极限 `2Ma/r³` 作交叉验证（测试用例已覆盖）。实现前先在 Python/KerrGeoPy 里算几个参考值对照。
2. **进动太慢看不见** → `time_scale` 旋钮（默认 ~50×）。这是必要的 UI 权宜，非物理错误。
3. **行星半径撞进吸积盘** → 默认 `k=2.5` 让 r ≥ ISCO×2.5，盘内边缘外。且随机倾角让多数行星不在盘面内。
4. **种子改动不触发重生** → `PlanetSystemDirty` flag + `spawn_planet_system` 检测。
5. **`orbit_system` 与 `upload_planets` 竞态** → `.before(upload_planets)` 强制顺序。

## 实现顺序（供 writing-plans 参考）

1. `physics.rs`：加 `kerr_orbital_frequency` + `kerr_nodal_precession` + 测试。先确保物理公式正确。
2. `scene/planets.rs`：加 `OrbitParams` 组件 + 轨道几何函数（纯函数，可测）+ `orbit_system`。
3. `scene/planets.rs`：加 `spawn_planet_system`（带 despawn 旧行星 + ChaCha8Rng）+ 删除 `spawn_default_planet`。
4. `params.rs`：加 5 个新字段 + Default。
5. `plugin.rs`：注册新系统 + `.before(upload_planets)` + `PlanetSeed` / `PlanetSystemDirty` 资源。
6. `ui.rs`：Planets header。
7. 视觉调参：`cargo run --release`，调 `k` / `time_scale` / 颜色直到好看。

## 参考资料

- [Bardeen, Press, Teukolsky 1972 — Kerr 圆轨角速度 eqn 2.16](https://physics.stackexchange.com/questions/502796/how-to-derive-the-angular-velocity-of-circular-orbits-in-kerr-geometry)
- [Chakraborty 2014 — Kerr 强场 Lense-Thirring 进动](https://arxiv.org/pdf/1304.6936)
- [Fujita & Hikida 2009 — Mino 时间解析解](https://arxiv.org/abs/0906.1420)（本文档解释为何**不用**它）
- [Costa & Natário 2021 — frame-dragging 三种含义](https://www.mdpi.com/2218-1997/7/10/388)
- [KerrGeoPy 文档](https://kerrgeopy.readthedocs.io/)（圆轨特例用同一组 Ω_φ）
- [Black Hole Perturbation Toolkit](http://bhptoolkit.org/toolkit.html)
