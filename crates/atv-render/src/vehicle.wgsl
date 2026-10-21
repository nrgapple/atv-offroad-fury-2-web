// Player quad/rider pass of web/game.js's scene shader (useQuadPose, lit,
// useTexture, fog per material pass).
struct Globals {
  view_projection: mat4x4<f32>,
  eye: vec4<f32>,
  fog_color: vec4<f32>,
  // encoded bias, distance coefficient, enabled
  fog: vec4<f32>,
  ambient: vec4<f32>,
  sun_color: vec4<f32>,
  sun_direction: vec4<f32>,
  surface: vec4<f32>,
};
struct Bones {
  world: mat4x4<f32>,
  nodes: array<mat4x4<f32>, 32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var<uniform> bones: Bones;
// textured, fog
@group(2) @binding(0) var<uniform> material: vec4<f32>;
@group(2) @binding(1) var base_texture: texture_2d<f32>;
@group(2) @binding(2) var base_sampler: sampler;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) uv: vec2<f32>,
  // Coverage and clip W, so the ratio interpolates screen-linearly (web/fog.js).
  @location(2) fog: vec2<f32>,
};

fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(g.fog.x + g.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) uv: vec2<f32>, @location(3) node: f32) -> Varyings {
  var out: Varyings;
  let pose = bones.nodes[u32(node)];
  let world = bones.world * pose * vec4<f32>(position, 1.0);
  let w = bones.world;
  out.normal = mat3x3<f32>(w[0].xyz, w[1].xyz, w[2].xyz) * mat3x3<f32>(pose[0].xyz, pose[1].xyz, pose[2].xyz) * normal;
  out.uv = uv;
  out.clip = g.view_projection * world;
  let fogged = g.fog.z > 0.5 && material.y > 0.5;
  let coverage = select(1.0, native_fog_coverage(length(world.xyz - g.eye.xyz)), fogged);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let n = normalize(in.normal);
  let sampled = textureSample(base_texture, base_sampler, in.uv).rgb;
  let base = select(g.surface.rgb, sampled, material.x > 0.5);
  let sunlight = max(dot(n, normalize(g.sun_direction.xyz)), 0.0);
  let lit = base * (g.ambient.rgb + g.sun_color.rgb * sunlight);
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  return vec4<f32>(mix(g.fog_color.rgb, lit, coverage), 1.0);
}
