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
// Wheel dust, dirt chunks, spray (CPU sprites) and wet births (GPU ring) of
// web/wheel-effects.js. Unfogged, depth-tested, no depth writes.
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
struct Effect {
  // camera right (horizontal), then clock
  right: vec4<f32>,
  // camera up, then wet (1) or dry (0)
  up: vec4<f32>,
  light: vec4<f32>,
};
struct Unused {
  value: vec4<f32>,
};
@group(2) @binding(0) var<uniform> e: Effect;
@group(2) @binding(1) var<uniform> unused: Unused;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) alpha: f32,
};

@vertex
fn vs(@location(0) corner: vec2<f32>, @location(1) point_size: vec4<f32>, @location(2) rectangle: vec4<f32>,
    @location(3) particle_alpha: f32, @location(4) velocity_birth: vec4<f32>, @location(5) lifetime: f32) -> Varyings {
  let wet = e.up.w > 0.5;
  var center = point_size.xyz;
  let age = e.right.w - velocity_birth.w;
  if (wet) {
    center += velocity_birth.xyz * age + vec3<f32>(0.0, -16.1, 0.0) * age * age;
    center.y = max(center.y, particle_alpha);
  }
  let point = center + (e.right.xyz * corner.x + e.up.xyz * corner.y) * point_size.w;
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(point, 1.0);
  out.uv = mix(rectangle.xy, rectangle.zw, corner * 0.5 + 0.5);
  out.alpha = select(particle_alpha, (64.0 + 63.0 * clamp(1.0 - age / lifetime, 0.0, 1.0)) / 128.0, wet);
  if (wet && age > lifetime) { out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0); }
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let sampled = textureSample(atlas, atlas_sampler, in.uv);
  if (e.up.w < 0.5 && sampled.a * 255.0 < 3.0) { discard; }
  // 001451a0 packet alpha 64, GS 128 unity; recovered PST stays in source units.
  return vec4<f32>(sampled.rgb * e.light.rgb, clamp(sampled.a * (255.0 / 128.0) * in.alpha, 0.0, 1.0));
}
