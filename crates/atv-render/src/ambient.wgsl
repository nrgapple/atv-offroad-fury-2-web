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
// Placed TOY ambient effects (web/ambient-effects.js, ambient-streams.js):
// back-to-front CPU sprites in alpha and additive families, and the GPU
// fountain ring aged by the original clock. Both camera-facing, depth-tested.
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
struct Effect {
  // camera right (horizontal), then stream clock
  right: vec4<f32>,
  // camera up, then far distance
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
  @location(1) modulation: vec4<f32>,
  @location(2) fog: vec2<f32>,
  // lit (sprites)
  @location(3) lit: f32,
};

fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(g.fog.x + g.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

fn fogged(out: ptr<function, Varyings>, point: vec3<f32>) {
  let coverage = select(1.0, native_fog_coverage(length(point - g.eye.xyz)), g.fog.z > 0.5);
  (*out).fog = vec2<f32>(coverage * (*out).clip.w, (*out).clip.w);
}

@vertex
fn vs_sprite(@location(0) corner: vec2<f32>, @location(1) point_size: vec4<f32>,
    @location(2) rectangle: vec4<f32>, @location(3) alpha_lit: vec2<f32>) -> Varyings {
  let point = point_size.xyz + (e.right.xyz * corner.x + e.up.xyz * corner.y) * point_size.w;
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(point, 1.0);
  out.uv = mix(rectangle.xy, rectangle.zw, corner * 0.5 + 0.5);
  out.modulation = vec4<f32>(1.0, 1.0, 1.0, alpha_lit.x);
  out.lit = alpha_lit.y;
  fogged(&out, point);
  return out;
}

@fragment
fn fs_sprite(in: Varyings) -> @location(0) vec4<f32> {
  let sampled = textureSample(atlas, atlas_sampler, in.uv);
  if (sampled.a * 255.0 < 3.0) { discard; }
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  let rgb = sampled.rgb * mix(vec3<f32>(1.0), e.light.rgb, in.lit);
  return vec4<f32>(mix(g.fog_color.rgb, rgb, coverage), clamp(sampled.a * (255.0 / 128.0) * in.modulation.a, 0.0, 1.0));
}

@vertex
fn vs_stream(@location(0) corner: vec2<f32>, @location(1) point_size: vec4<f32>,
    @location(2) velocity_birth: vec4<f32>, @location(3) life_growth_alpha: vec4<f32>,
    @location(4) rectangle: vec4<f32>, @location(5) color_floor: vec4<f32>) -> Varyings {
  let now = e.right.w;
  let age = now - velocity_birth.w;
  let life = life_growth_alpha.x;
  var center = point_size.xyz + velocity_birth.xyz * age + vec3<f32>(0.0, -16.1, 0.0) * age * age;
  center.y = max(center.y, color_floor.w);
  let size = max(0.0, point_size.w + life_growth_alpha.y * age);
  let point = center + (e.right.xyz * corner.x + e.up.xyz * corner.y) * size;
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(point, 1.0);
  out.uv = mix(rectangle.xy, rectangle.zw, corner * 0.5 + 0.5);
  out.modulation = vec4<f32>(color_floor.rgb / 128.0,
    mix(life_growth_alpha.z, life_growth_alpha.w, clamp(1.0 - age / max(life, 0.000001), 0.0, 1.0)) / 128.0);
  out.lit = 0.0;
  fogged(&out, point);
  if (life <= 0.0 || age < 0.0 || age > life || distance(center, g.eye.xyz) > e.up.w + size) {
    out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
  }
  return out;
}

@fragment
fn fs_stream(in: Varyings) -> @location(0) vec4<f32> {
  let sampled = textureSample(atlas, atlas_sampler, in.uv);
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  return vec4<f32>(mix(g.fog_color.rgb, sampled.rgb * in.modulation.rgb, coverage),
    clamp(sampled.a * (255.0 / 128.0) * in.modulation.a, 0.0, 1.0));
}
