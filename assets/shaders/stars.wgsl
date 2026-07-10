// Hash-based procedural stars on the unit sphere. Returns RGB radiance.
fn hash13(p: vec3<f32>) -> f32 {
    var q = vec3<f32>(dot(p, vec3<f32>(127.1, 311.7, 74.7)),
                      dot(p, vec3<f32>(269.5, 183.3, 246.1)),
                      dot(p, vec3<f32>(113.5, 271.9, 124.6)));
    let h = fract(sin(q) * 43758.5453);
    return h.x;
}

fn star_color(dir: vec3<f32>, intensity: f32) -> vec3<f32> {
    // Divide the sphere into cells; a cell gets a star if its hash passes a threshold.
    let scale = 80.0;
    let cell = floor(dir * scale);
    let h = hash13(cell);
    let threshold = 0.985; // ~1.5% of cells hold a star
    if (h > threshold) {
        // Brightness from the hash remainder.
        let b = (h - threshold) / (1.0 - threshold);
        let col = mix(vec3<f32>(0.6, 0.7, 1.0), vec3<f32>(1.0, 0.9, 0.7), b);
        // Soften the star with the fractional position inside the cell.
        let f = abs(dir * scale - cell);
        let d = max(f.x, max(f.y, f.z));
        let falloff = smoothstep(0.5, 0.0, d);
        return col * b * falloff * 3.0 * intensity;
    }
    return vec3<f32>(0.0);
}
