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
// Vegetation of web/game.js's scene shader (useEcosystem, ecoPerInstance):
// near meshes and atlas billboards with the per-instance alpha reference.
@group(1) @binding(0) var base_texture: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;
struct Material {
  // alpha cutoff, fog enabled, scenery blend, billboard
  params: vec4<f32>,
  // use texture
  flags: vec4<f32>,
};
struct Unused {
  value: vec4<f32>,
};
@group(2) @binding(0) var<uniform> m: Material;
@group(2) @binding(1) var<uniform> unused: Unused;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  // Authored RGBA times clip W, divided again per fragment: GS screen-linear.
  @location(1) color: vec4<f32>,
  @location(2) fog: vec2<f32>,
  @location(3) @interpolate(flat) reference: f32,
};

fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(g.fog.x + g.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

fn finish(world: vec3<f32>, uv: vec2<f32>, color: vec4<f32>, reference: f32) -> Varyings {
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(world, 1.0);
  out.uv = uv;
  let fogged = g.fog.z > 0.5 && m.params.y > 0.5;
  let coverage = select(1.0, native_fog_coverage(length(world - g.eye.xyz)), fogged);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  out.color = color * out.clip.w;
  out.reference = reference;
  return out;
}

fn place(origin: vec3<f32>, rotation: vec2<f32>, scaled: vec3<f32>) -> vec3<f32> {
  return origin + vec3<f32>(rotation.x * scaled.x - rotation.y * scaled.z, scaled.y,
    rotation.y * scaled.x + rotation.x * scaled.z);
}

@vertex
fn vs_mesh(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
    @location(4) eco_position: vec3<f32>, @location(5) eco_scale: vec2<f32>, @location(9) eco_state: vec3<f32>) -> Varyings {
  let rotation = eco_state.xy;
  let scaled = position * vec3<f32>(eco_scale.x, eco_scale.y, eco_scale.x);
  // TriStripPrologue transforms the light through the scaled instance basis.
  // DiffuseCompressed retains authored normal magnitude and bias quantization.
  let sun = g.eco_direction.xyz;
  let local_light = vec3<f32>((rotation.x * eco_scale.x) * sun.x + (rotation.y * eco_scale.x) * sun.z,
    sun.y * eco_scale.y, (-rotation.y * eco_scale.x) * sun.x + (rotation.x * eco_scale.x) * sun.z);
  let weight = max((local_light.x * normal.x + local_light.y * normal.y) + local_light.z * normal.z, 0.0);
  let alpha = min(64.0, 64.0 - (eco_state.z - 16.0) * 1.3333334);
  var biased = vec4<f32>(8388608.0) + vec4<f32>(g.eco_ambient.xyz, alpha);
  biased = min(biased + vec4<f32>(g.eco_sunlight.xyz, g.eco_direction.w) * weight, vec4<f32>(8388863.0));
  let color = vec4<f32>(bitcast<vec4<u32>>(biased) & vec4<u32>(255u));
  return finish(place(eco_position, rotation, scaled), uv, color, trunc(alpha) / 255.0);
}

@vertex
fn vs_billboard(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>,
    @location(4) eco_position: vec3<f32>, @location(5) eco_scale: vec2<f32>, @location(9) eco_state: vec3<f32>,
    @location(10) atlas_h: vec4<f32>, @location(11) atlas_v: vec4<f32>) -> Varyings {
  let rotation = g.eco_rotation.xy;
  let column = normal.x;
  var horizontal = atlas_h.y;
  var u = atlas_h.w;
  if (column < 0.5) { horizontal = atlas_h.x; u = atlas_h.z; } else if (column < 1.5) { horizontal = 0.0; u = atlas_v.x; }
  let scaled = vec3<f32>(horizontal * eco_scale.x, (position.y + atlas_v.w) * eco_scale.y, 0.0);
  let uv = vec2<f32>(u, select(atlas_v.z, atlas_v.y, position.y > 0.5));
  //002fea30 shades the left edge, center pivot and right edge separately.
  let look = vec3<f32>(-rotation.y, 0.0, rotation.x);
  var light_normal = vec3<f32>(look.z + look.x * 0.1, 0.0, -look.x + look.z * 0.1);
  if (column < 0.5) { light_normal = vec3<f32>(-look.z + look.x * 0.1, 0.0, look.x + look.z * 0.1); }
  else if (column < 1.5) { light_normal = -look; }
  let horizontal_sun = g.eco_direction.xz;
  let sun_length = length(horizontal_sun);
  var sun = vec3<f32>(0.0);
  if (sun_length > 0.0) { sun = vec3<f32>(horizontal_sun.x / sun_length, 0.0, horizontal_sun.y / sun_length); }
  let diffuse = max(dot(normalize(light_normal), sun), 0.0);
  let color = vec4<f32>(trunc(min(g.eco_ambient.xyz + g.eco_sunlight.xyz * diffuse, vec3<f32>(127.0))),
    min(64.0, trunc(eco_state.z * 1.3333334)));
  return finish(place(eco_position, rotation, scaled), uv, color, 0.0);
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  var sampled = textureSample(base_texture, base_sampler, in.uv);
  var base = vec3<f32>(0.68, 0.70, 0.73);
  var source_alpha = 1.0;
  if (m.flags.x > 0.5) {
    sampled = sampled * (in.color / in.fog.y) / 128.0;
    //002fe710 overrides TEST with GREATER/KEEP and a per-placement reference.
    if (sampled.a <= select(in.reference, m.params.x, m.params.w > 0.5)) { discard; }
    base = sampled.rgb;
    source_alpha = select(1.0, clamp(sampled.a * (255.0 / 128.0), 0.0, 1.0), m.params.z > 0.5);
  }
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  return vec4<f32>(mix(g.fog_color.rgb, base, coverage), source_alpha);
}
