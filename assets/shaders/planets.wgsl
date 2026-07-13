#define_import_path singularity::planets

struct SphereData {
    center: vec4<f32>,   // xyz = center, w = radius
    color: vec4<f32>,    // xyz = color, w = emissive flag (u32 reinterpreted; we just check > 0.5)
};

// The storage binding is declared here as part of the planets module; it lives
// in the material's bind group (group 2 = #{MATERIAL_BIND_GROUP}).
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<storage, read> planets: array<SphereData>;

// Test the segment prev->cur against all planets. Returns hit color & alpha,
// or (0,0,0,0) if no hit. `dir` is the ray direction (for shading).
fn planet_hit(prev: vec3<f32>, cur: vec3<f32>, dir: vec3<f32>) -> vec4<f32> {
    var nearest_t = 1e9;
    var nearest_col = vec3<f32>(0.0);
    var found = false;
    for (var i: u32 = 0u; i < uniforms.planet_count; i = i + 1u) {
        let s = planets[i];
        let center = s.center.xyz;
        let radius = s.center.w;
        // Ray-sphere intersection for the segment.
        let seg = cur - prev;
        let oc = prev - center;
        let a = dot(seg, seg);
        let b = 2.0 * dot(oc, seg);
        let c = dot(oc, oc) - radius * radius;
        let disc = b * b - 4.0 * a * c;
        if (disc < 0.0) { continue; }
        let sq = sqrt(disc);
        var t = (-b - sq) / (2.0 * a);
        if (t < 0.0) { t = (-b + sq) / (2.0 * a); }
        if (t >= 0.0 && t <= 1.0 && t < nearest_t) {
            nearest_t = t;
            let hit_pos = prev + seg * t;
            let n = normalize(hit_pos - center);
            // Lambert shading from a fixed light direction.
            let light_dir = normalize(vec3<f32>(0.5, 0.8, 0.3));
            let ndl = max(dot(n, light_dir), 0.0);
            var col = s.color.xyz * (0.2 + 0.8 * ndl);
            if (s.color.w > 0.5) { col = s.color.xyz; } // emissive
            nearest_col = col;
            found = true;
        }
    }
    if (found) {
        return vec4<f32>(nearest_col, 0.95);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
