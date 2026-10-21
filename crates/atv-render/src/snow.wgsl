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
// Ring-emitter Snow (web/weather.js): Rust owns camera-ring selection and
// source SnowEmitter births; this shader ages them. Unfogged, as the browser.
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
struct Snow {
  // camera right (horizontal), then clock
  right: vec4<f32>,
  // camera up, then min alpha
  up: vec4<f32>,
  // particle colour / 128, then max alpha
  color: vec4<f32>,
};
struct Unused {
  value: vec4<f32>,
};
@group(2) @binding(0) var<uniform> s: Snow;
@group(2) @binding(1) var<uniform> unused: Unused;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) alpha: f32,
};

@vertex
fn vs(@location(0) corner: vec2<f32>, @location(1) point_size: vec4<f32>,
    @location(2) velocity_birth: vec4<f32>, @location(3) life_mirror_half: vec3<f32>) -> Varyings {
  let age = s.right.w - velocity_birth.w;
  let life = life_mirror_half.x;
  var center = point_size.xyz + velocity_birth.xyz * age;
  // SourceSnowEmitter clears GPU gravity; its mode2 minimum plane is -10000.
  center.y = max(center.y, -10000.0);
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(center + (s.right.xyz * corner.x + s.up.xyz * corner.y) * point_size.w, 1.0);
  var mirrored = corner * 0.5 + 0.5;
  let bits = i32(life_mirror_half.y);
  if ((bits & 1) != 0) { mirrored.x = 1.0 - mirrored.x; }
  if ((bits & 2) != 0) { mirrored.y = 1.0 - mirrored.y; }
  out.uv = vec2<f32>((mirrored.x + life_mirror_half.z) * 0.5, mirrored.y);
  out.alpha = (s.up.w + (s.color.w - s.up.w) * clamp(1.0 - age / max(life, 0.000001), 0.0, 1.0)) / 128.0;
  if (life <= 0.0 || age < 0.0 || age > life) { out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0); }
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let sampled = textureSample(atlas, atlas_sampler, in.uv);
  return vec4<f32>(sampled.rgb * s.color.rgb, clamp(sampled.a * (255.0 / 128.0) * in.alpha, 0.0, 1.0));
}
