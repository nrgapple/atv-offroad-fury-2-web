// Terrain pass of web/game.js's scene shader (useTerrainTexture, unlit).
struct Globals {
  view_projection: mat4x4<f32>,
  eye: vec4<f32>,
  fog_color: vec4<f32>,
  // encoded bias, distance coefficient, enabled, terrain spacing
  fog: vec4<f32>,
  detail_scale: vec4<f32>,
  shadow_matrix: mat4x4<f32>,
  // receive, texel size
  shadow: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var terrain_colors: texture_2d_array<f32>;
@group(0) @binding(2) var terrain_mapping: texture_2d<u32>;
@group(0) @binding(3) var terrain_detail: texture_2d<f32>;
@group(0) @binding(4) var clamp_sampler: sampler;
@group(0) @binding(5) var repeat_sampler: sampler;
@group(0) @binding(6) var vehicle_shadow_map: texture_depth_2d;
@group(0) @binding(7) var vehicle_shadow_sampler: sampler_comparison;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) world_xz: vec2<f32>,
  // Coverage and clip W, both carried so the ratio interpolates screen-linearly
  // like the GS fog (see web/fog.js).
  @location(1) fog: vec2<f32>,
  @location(2) world: vec3<f32>,
};

// Original eight-bit GS visibility from the packed bias/coefficient.
fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(g.fog.x + g.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> Varyings {
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(position, 1.0);
  out.world_xz = position.xz;
  out.world = position;
  let coverage = select(1.0, native_fog_coverage(length(position - g.eye.xyz)), g.fog.z > 0.5);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let spacing = g.fog.w;
  let grid = in.world_xz / (spacing * 64.0);
  let size = vec2<i32>(textureDimensions(terrain_mapping));
  let tile = clamp(vec2<i32>(floor(grid)), vec2<i32>(0), size - 1);
  let layer = textureLoad(terrain_mapping, tile, 0).r;
  // 001325c0 selects the 64-cell subtexture; 00131790 submits its 8px border.
  let coord = fract(grid) * 0.9375 + 0.03125;
  // Derivatives precede fract() so 64-cell seams keep their mip level.
  var base = textureSampleGrad(terrain_colors, clamp_sampler, coord, layer, dpdx(grid) * 0.9375, dpdy(grid) * 0.9375).rgb;
  // 00168490 computes (8*256/textureWidth)/64 per terrain cell.
  let detail = textureSample(terrain_detail, repeat_sampler, in.world_xz / spacing * g.detail_scale.xy);
  // Near terrain VU RGB=127, A=64; GS MODULATE divides by 128.
  base = mix(base, detail.rgb, detail.a * (255.0 / 256.0)) * (127.0 / 128.0);
  base = base * vehicle_shadow(in.world);
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  return vec4<f32>(mix(g.fog_color.rgb, base, coverage), 1.0);
}

// High preset vehicle shadow map (shadowReceiverGlsl in web/shadow-map.js):
// 1 outside the light box, four bilinear compare taps, soft box edges.
fn vehicle_shadow(p: vec3<f32>) -> f32 {
  // Off (every preset but High): no taps, as the browser's shadowReceive.
  if (g.shadow.x < 0.5) { return 1.0; }
  let c = g.shadow_matrix * vec4<f32>(p, 1.0);
  let s = c.xyz / c.w;
  let edge = min(min(s.x, s.y), min(1.0 - s.x, 1.0 - s.y));
  let texel = g.shadow.y;
  var lit = 0.0;
  for (var i = 0; i < 4; i++) {
    let o = vec2<f32>(select(-1.0, 1.0, i == 1 || i == 3), select(-1.0, 1.0, i >= 2)) * texel;
    lit += textureSampleCompareLevel(vehicle_shadow_map, vehicle_shadow_sampler, s.xy + o, s.z - 0.0015);
  }
  let factor = mix(1.0, mix(0.55, 1.0, lit * 0.25), smoothstep(0.0, 0.08, edge));
  return select(factor, 1.0, edge <= 0.0 || s.z >= 1.0);
}
