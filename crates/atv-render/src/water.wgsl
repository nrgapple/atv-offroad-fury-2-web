// Projected water surface of web/water.js: the original WTT height signatures
// (shared with physics), Dx/Dz derivative maps with BumpScale and the
// environment reflection. Native PS2 VU tessellation and froth are not claimed.
struct Water {
  view_projection: mat4x4<f32>,
  eye: vec4<f32>,
  forward: vec4<f32>,
  right: vec4<f32>,
  up: vec4<f32>,
  // aspect / focal, 1 / focal, screen bounds (min, max)
  projection: vec4<f32>,
  // base height, far distance, channel count, gradient count
  surface: vec4<f32>,
  environment_color: vec4<f32>,
  fog_color: vec4<f32>,
  // encoded bias, coefficient, enabled, fresnel
  fog: vec4<f32>,
  // origin.xy, inverse width.xy
  origins: array<vec4<f32>, 8>,
  // width.xy, tiled
  widths: array<vec4<f32>, 8>,
  // dimension, scale, sign, layer
  channels: array<vec4<f32>, 8>,
  // origin channel, first layer, GS FIX / 128, dimension ratio
  waves: array<vec4<f32>, 4>,
  // second layer, GS FIX / 128
  second: array<vec4<f32>, 4>,
};
@group(0) @binding(0) var<uniform> w: Water;
@group(0) @binding(1) var height_maps: texture_2d_array<u32>;
@group(0) @binding(2) var environment_map: texture_2d<f32>;
@group(0) @binding(3) var gradient_maps: texture_2d_array<f32>;
@group(0) @binding(4) var clamp_sampler: sampler;
@group(0) @binding(5) var repeat_sampler: sampler;

fn inside(channel: i32, local: vec2<f32>) -> bool {
  return w.widths[channel].z > 0.5 || !(any(local < vec2<f32>(0.0)) || any(local > w.widths[channel].xy));
}

fn height_at(point: vec2<f32>) -> f32 {
  var sum = 0.0;
  for (var i = 0; i < 8; i++) {
    if (f32(i) >= w.surface.z) { break; }
    let local = point - w.origins[i].xy;
    if (!inside(i, local)) { continue; }
    let channel = w.channels[i];
    let dimension = u32(channel.x);
    // Native truncation then DIVU remainder; negative inputs wrap as unsigned.
    let index = bitcast<vec2<u32>>(vec2<i32>((w.origins[i].zw * channel.x) * local));
    let pixel = vec2<i32>(index % vec2<u32>(dimension));
    let value = textureLoad(height_maps, pixel, i32(channel.w), 0).r;
    sum = sum + channel.z * (channel.y * f32(value));
  }
  return w.surface.x + sum * (1.0 / 4096.0);
}

fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(w.fog.x + w.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) point: vec3<f32>,
  @location(1) fog: vec2<f32>,
};

@vertex
fn vs(@location(0) grid: vec2<f32>) -> Varyings {
  let screen = vec2<f32>(grid.x, mix(w.projection.z, w.projection.w, grid.y));
  let direction = w.forward.xyz + w.right.xyz * screen.x * w.projection.x + w.up.xyz * screen.y * w.projection.y;
  var denominator = direction.y;
  if (abs(direction.y) < 0.000001) { denominator = select(0.000001, -0.000001, direction.y < 0.0); }
  let distance = clamp((w.surface.x - w.eye.y) / denominator, 1.0, w.surface.y);
  var point = w.eye.xyz + direction * distance;
  point.y = height_at(point.xz);
  var out: Varyings;
  out.point = point;
  out.clip = w.view_projection * vec4<f32>(point, 1.0);
  let coverage = select(1.0, native_fog_coverage(length(point - w.eye.xyz)), w.fog.z > 0.5);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let point = in.point;
  // Explicit gradients keep sampling valid under the per-channel bounds test.
  let dx = dpdx(point.xz);
  let dy = dpdy(point.xz);
  var gradient = vec2<f32>(0.0);
  for (var i = 0; i < 4; i++) {
    if (f32(i) >= w.surface.w) { break; }
    let wave = w.waves[i];
    let channel = i32(wave.x);
    let local = point.xz - w.origins[channel].xy;
    let scale = w.origins[channel].zw * wave.w;
    let keep = select(0.0, 1.0, inside(channel, local));
    var d = textureSampleGrad(gradient_maps, repeat_sampler, local * scale, i32(wave.y), dx * scale, dy * scale);
    gradient += vec2<f32>(d.r - d.g, d.b - d.a) * wave.z * (255.0 / 128.0) * keep;
    if (w.second[i].y != 0.0) {
      d = textureSampleGrad(gradient_maps, repeat_sampler, local * scale, i32(w.second[i].x), dx * scale, dy * scale);
      gradient += vec2<f32>(d.r - d.g, d.b - d.a) * w.second[i].y * (255.0 / 128.0) * keep;
    }
  }
  let normal = normalize(vec3<f32>(gradient.x, 1.0, gradient.y));
  let reflected = reflect(normalize(point - w.eye.xyz), normal);
  let uv = reflected.xz * 0.5 + 0.5;
  let sampled = textureSampleGrad(environment_map, clamp_sampler, uv, dpdx(uv), dpdy(uv));
  var surface = sampled.rgb * w.environment_color.rgb;
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  var alpha = clamp(sampled.a * (255.0 / 128.0), 0.0, 1.0);
  if (w.fog.w > 0.5) {
    // Enhanced/High authored Fresnel (Schlick, F0 0.02).
    let facing = clamp(dot(normalize(w.eye.xyz - point), normal), 0.0, 1.0);
    let f = 0.02 + 0.98 * pow(1.0 - facing, 5.0);
    alpha = clamp(alpha * 0.55 + f * 0.75, 0.0, 1.0);
    surface = surface * (0.85 + 0.35 * f);
  }
  return vec4<f32>(mix(w.fog_color.rgb, surface, coverage), alpha);
}
