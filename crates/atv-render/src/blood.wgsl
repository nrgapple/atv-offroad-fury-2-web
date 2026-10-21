// Sky pass of web/game.js's scene shader (skyPass): unlit, unfogged cube
// tiles at the far plane (xyww) with depth testing disabled.
struct Globals {
  view_projection: mat4x4<f32>,
  eye: vec4<f32>,
  fog_color: vec4<f32>,
  fog: vec4<f32>,
  ambient: vec4<f32>,
  sun_color: vec4<f32>,
  sun_direction: vec4<f32>,
  eco_ambient: vec4<f32>,
  eco_sunlight: vec4<f32>,
  eco_direction: vec4<f32>,
  eco_rotation: vec4<f32>,
  sky: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tile: texture_2d<f32>;
// Gorn blood (BloodRenderer in web/wheel-effects.js): droplet streaks, lobed
// ground stains and stump caps, one instanced draw. Authored.
struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) @interpolate(flat) kind: f32,
  @location(2) @interpolate(flat) seed: f32,
  @location(3) shade: f32,
};

@vertex
fn vs(@location(0) corner: vec2<f32>, @location(1) a0: vec4<f32>, @location(2) a1: vec4<f32>,
    @location(3) a2: vec4<f32>) -> Varyings {
  var out: Varyings;
  out.uv = corner;
  out.kind = a0.w;
  out.seed = a1.w;
  var p = a0.xyz;
  var x: vec3<f32>;
  var y: vec3<f32>;
  if (a0.w < 0.5) {
    // Droplet: a camera-facing streak along its velocity, tail behind.
    let speed = length(a1.xyz);
    let along = select(vec3<f32>(0.0, 1.0, 0.0), a1.xyz / speed, speed > 0.01);
    let view = normalize(g.eye.xyz - p);
    var side = cross(along, view);
    side = select(normalize(cross(vec3<f32>(0.0, 1.0, 0.0), view) + vec3<f32>(1e-4, 0.0, 0.0)), normalize(side), length(side) > 1e-3);
    let up = normalize(cross(view, side));
    p = p - along * (a2.w - a2.z) * 0.5;
    x = side * a2.z;
    y = up * a2.w * sign(dot(up, along) + 1e-4);
    out.shade = 1.0;
  } else {
    let n = normalize(a1.xyz);
    var t: vec3<f32>;
    if (a0.w < 1.5) {
      var d = vec3<f32>(a2.x, 0.0, a2.y);
      d.y = -(n.x * d.x + n.z * d.z) / max(n.y, 0.05);
      t = normalize(d);
    } else {
      t = normalize(select(cross(n, vec3<f32>(1.0, 0.0, 0.0)), cross(n, vec3<f32>(0.0, 1.0, 0.0)), abs(n.y) < 0.9));
    }
    let b = cross(n, t);
    x = t * select(a2.z, a2.w, a0.w < 1.5);
    y = b * a2.z;
    p = p + n * select(0.015, 0.03, a0.w < 1.5);
    out.shade = 0.55 + 0.45 * max(dot(n, normalize(vec3<f32>(0.35, 0.85, 0.4))), 0.0);
  }
  out.clip = g.view_projection * vec4<f32>(p + x * corner.x + y * corner.y, 1.0);
  return out;
}

fn hash(n: f32) -> f32 { return fract(sin(n) * 43758.5453); }

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let uv = in.uv;
  let r = length(uv);
  if (in.kind < 0.5) {
    if (r > 1.0) { discard; }
    var c = mix(vec3<f32>(0.30, 0.004, 0.01), vec3<f32>(0.62, 0.02, 0.03), 1.0 - r);
    c += vec3<f32>(0.3, 0.15, 0.15) * (1.0 - smoothstep(0.0, 0.35, length(uv - vec2<f32>(-0.3, 0.4))));
    return vec4<f32>(c, (1.0 - smoothstep(0.55, 1.0, r)) * 0.95);
  }
  if (in.kind < 1.5) {
    // Irregular lobed stain with satellite droplets beyond the rim.
    let a = atan2(uv.y, uv.x);
    let seed = in.seed;
    let edge = 0.74 + 0.08 * sin(a * 3.0 + seed * 6.283) + 0.05 * sin(a * 7.0 + seed * 17.0) + 0.025 * sin(a * 13.0 + seed * 41.0);
    let body = 1.0 - smoothstep(edge - 0.06, edge, r);
    let cell = floor(uv * 5.0);
    let f = fract(uv * 5.0) - 0.5;
    let h = hash(dot(cell, vec2<f32>(12.9898, 78.233)) + seed * 91.0);
    let satellite = select(0.0, 1.0 - smoothstep(0.12, 0.28, length(f)), h > 0.87 && r > edge && r < 1.0);
    let alpha = max(body, satellite);
    if (alpha < 0.02) { discard; }
    var wet = mix(vec3<f32>(0.15, 0.0, 0.008), vec3<f32>(0.40, 0.012, 0.02), smoothstep(edge, 0.0, r));
    wet += vec3<f32>(0.16, 0.07, 0.07) * (1.0 - smoothstep(0.0, 0.18, abs(r - 0.4 * edge)));
    return vec4<f32>(wet * in.shade, alpha * 0.92);
  }
  // Stump cap: torn muscle around a bone and marrow.
  if (r > 1.0) { discard; }
  let meat = mix(vec3<f32>(0.56, 0.07, 0.07), vec3<f32>(0.30, 0.01, 0.02), smoothstep(0.35, 1.0, r));
  let b = uv - vec2<f32>(0.08, -0.05);
  var c = mix(meat, vec3<f32>(0.86, 0.82, 0.72), 1.0 - smoothstep(0.26, 0.32, length(b)));
  c = mix(c, vec3<f32>(0.45, 0.12, 0.08), 1.0 - smoothstep(0.09, 0.13, length(b)));
  return vec4<f32>(c * in.shade, 1.0 - smoothstep(0.88, 1.0, r));
}
