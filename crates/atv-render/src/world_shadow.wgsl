// Remastered world shadow map (authored; the original casts no shadows):
// depth-only casters for the terrain, static scene models and near plant
// meshes, seen from the sun.
@group(0) @binding(0) var<uniform> light: mat4x4<f32>;
@group(1) @binding(0) var base_texture: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;
// scenery.wgsl's per-draw record.
struct Material {
  // use texture, vertex color, scenery blend, alpha-fail pass
  flags: vec4<f32>,
  // alpha cutoff, fog enabled, reflection plane
  params: vec4<f32>,
  uv_offset: vec4<f32>,
};
struct Instance {
  model: mat4x4<f32>,
  flags: vec4<f32>,
};
struct Record {
  m: Material,
  d: Instance,
};
@group(2) @binding(0) var<uniform> r: Record;

struct Cutout {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  // Texture alpha below this casts nothing; 0 for solid geometry.
  @location(1) cutoff: f32,
};

@vertex
fn vs_terrain(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
  return light * vec4<f32>(position, 1.0);
}

@vertex
fn vs_scenery(@location(0) position_u: vec4<f32>, @location(1) normal_v: vec4<f32>) -> Cutout {
  var out: Cutout;
  out.clip = light * (r.d.model * vec4<f32>(position_u.xyz, 1.0));
  out.uv = vec2<f32>(position_u.w, normal_v.w) + r.m.uv_offset.xy;
  // Decoded alpha is 128 for opaque, so a quarter clears solid texels.
  out.cutoff = select(0.0, max(r.m.params.x, 0.25), r.m.flags.x > 0.5);
  return out;
}

// vegetation.wgsl's vs_mesh placement, without the wind: the map is cached.
@vertex
fn vs_vegetation(@location(0) position: vec3<f32>, @location(2) uv: vec2<f32>,
    @location(4) eco_position: vec3<f32>, @location(5) eco_scale: vec2<f32>, @location(9) eco_state: vec3<f32>) -> Cutout {
  let rotation = eco_state.xy;
  let scaled = position * vec3<f32>(eco_scale.x, eco_scale.y, eco_scale.x);
  let world = eco_position + vec3<f32>(rotation.x * scaled.x - rotation.y * scaled.z, scaled.y,
    rotation.y * scaled.x + rotation.x * scaled.z);
  var out: Cutout;
  out.clip = light * vec4<f32>(world, 1.0);
  out.uv = uv;
  out.cutoff = 0.25;
  return out;
}

@fragment
fn fs_solid() {}

@fragment
fn fs_cutout(in: Cutout) {
  let alpha = textureSample(base_texture, base_sampler, in.uv).a;
  if (in.cutoff > 0.0 && alpha < in.cutoff) { discard; }
}
