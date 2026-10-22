// Alpha landing burst (web/hype-gl.js BurstRenderer): dust puffs shoved out
// with heavy drag that rise and spread, and ballistic dirt clods, faded by
// distance towards the fog colour; puffs next to the lens thin out.
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
@group(1) @binding(0) var unused_texture: texture_2d<f32>;
@group(1) @binding(1) var unused_sampler: sampler;
struct Burst {
  // camera right, then clock
  right: vec4<f32>,
  // camera up, then fog end
  up: vec4<f32>,
  light: vec4<f32>,
};
struct Unused {
  value: vec4<f32>,
};
@group(2) @binding(0) var<uniform> b: Burst;
@group(2) @binding(1) var<uniform> unused: Unused;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) v: vec2<f32>,
  @location(1) color: vec4<f32>,
};

@vertex
fn vs(@location(0) corner: vec2<f32>, @location(1) seed: vec4<f32>, @location(2) motion: vec4<f32>,
    @location(3) shape: vec2<f32>, @location(4) tint: vec3<f32>) -> Varyings {
  let age = b.right.w - seed.w;
  let t = age / motion.w;
  let alive = step(0.0, age) * step(t, 1.0) * step(0.001, motion.w);
  var center: vec3<f32>;
  var size: f32;
  var alpha: f32;
  if (shape.y < 0.5) {
    let k = 2.6;
    center = seed.xyz + motion.xyz * (1.0 - exp(-k * age)) / k + vec3<f32>(0.0, 0.9 * age, 0.0);
    size = shape.x * (0.5 + 1.2 * sqrt(clamp(t, 0.0, 1.0)));
    alpha = 0.3 * smoothstep(0.0, 0.05, t) * pow(max(1.0 - t, 0.0), 1.6);
  } else {
    center = seed.xyz + motion.xyz * age + vec3<f32>(0.0, -16.0 * age * age, 0.0);
    size = shape.x;
    alpha = 1.0 - smoothstep(0.75, 1.0, t);
  }
  let world = center + (b.right.xyz * corner.x + b.up.xyz * corner.y) * size;
  let fog_end = b.up.w;
  let distance = length(center - g.eye.xyz);
  let fog = smoothstep(fog_end * 0.35, fog_end, distance);
  let near = select(1.0, smoothstep(4.0, 14.0, distance), shape.y < 0.5);
  var out: Varyings;
  out.color = vec4<f32>(mix(tint * b.light.rgb, g.fog_color.rgb, fog), alive * alpha * near * (1.0 - fog));
  out.v = corner;
  out.clip = select(vec4<f32>(2.0, 2.0, 2.0, 1.0), g.view_projection * vec4<f32>(world, 1.0), alive > 0.0);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let r = dot(in.v, in.v);
  if (r >= 1.0) { discard; }
  let a = 1.0 - r;
  return vec4<f32>(in.color.rgb, in.color.a * a * a);
}
