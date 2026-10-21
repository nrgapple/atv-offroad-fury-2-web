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
@group(1) @binding(1) var tile_sampler: sampler;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> Varyings {
  var out: Varyings;
  let clip = g.view_projection * vec4<f32>(position + g.sky.xyz, 1.0);
  out.clip = clip.xyww;
  out.uv = uv;
  return out;
}

// Catmull-Rom bicubic from nine bilinear taps (Enhanced/High smooth sky).
fn bicubic(uv: vec2<f32>) -> vec4<f32> {
  let size = vec2<f32>(textureDimensions(tile, 0));
  let position = uv * size;
  let center = floor(position - 0.5) + 0.5;
  let f = position - center;
  let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
  let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
  let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
  let w3 = f * f * (-0.5 + 0.5 * f);
  let w12 = w1 + w2;
  let t0 = (center - 1.0) / size;
  let t3 = (center + 2.0) / size;
  let t12 = (center + w2 / w12) / size;
  return textureSampleLevel(tile, tile_sampler, vec2<f32>(t0.x, t0.y), 0.0) * w0.x * w0.y
    + textureSampleLevel(tile, tile_sampler, vec2<f32>(t12.x, t0.y), 0.0) * w12.x * w0.y
    + textureSampleLevel(tile, tile_sampler, vec2<f32>(t3.x, t0.y), 0.0) * w3.x * w0.y
    + textureSampleLevel(tile, tile_sampler, vec2<f32>(t0.x, t12.y), 0.0) * w0.x * w12.y
    + textureSampleLevel(tile, tile_sampler, vec2<f32>(t12.x, t12.y), 0.0) * w12.x * w12.y
    + textureSampleLevel(tile, tile_sampler, vec2<f32>(t3.x, t12.y), 0.0) * w3.x * w12.y
    + textureSampleLevel(tile, tile_sampler, vec2<f32>(t0.x, t3.y), 0.0) * w0.x * w3.y
    + textureSampleLevel(tile, tile_sampler, vec2<f32>(t12.x, t3.y), 0.0) * w12.x * w3.y
    + textureSampleLevel(tile, tile_sampler, vec2<f32>(t3.x, t3.y), 0.0) * w3.x * w3.y;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let filtered = textureSample(tile, tile_sampler, in.uv);
  let sampled = select(filtered, clamp(bicubic(in.uv), vec4<f32>(0.0), vec4<f32>(1.0)), g.sky.w > 0.5);
  return vec4<f32>(sampled.rgb, 1.0);
}
