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
// ENV08 train cars and locomotive wheels (web/train.js through the scene
// shader's useQuadPose path): rigid nodes posed by the bind pose or the
// Rust wheel matrices, provisional prop lighting, cutouts and GS fog.
@group(1) @binding(0) var base_texture: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;
struct Material {
  // alpha cutoff, fog enabled, use texture
  params: vec4<f32>,
};
struct Instance {
  model: mat4x4<f32>,
  // unlit
  flags: vec4<f32>,
  bones: array<mat4x4<f32>, 32>,
};
@group(2) @binding(0) var<uniform> m: Material;
@group(2) @binding(1) var<uniform> d: Instance;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) fog: vec2<f32>,
};

fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(g.fog.x + g.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) node: f32) -> Varyings {
  let pose = d.model * d.bones[u32(node)];
  let world = pose * vec4<f32>(position, 1.0);
  var out: Varyings;
  out.clip = g.view_projection * world;
  out.uv = uv;
  out.normal = (pose * vec4<f32>(normal, 0.0)).xyz;
  let fogged = g.fog.z > 0.5 && m.params.y > 0.5;
  let coverage = select(1.0, native_fog_coverage(length(world.xyz - g.eye.xyz)), fogged);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  var base = vec3<f32>(0.68, 0.70, 0.73);
  let sampled = textureSample(base_texture, base_sampler, in.uv);
  if (m.params.z > 0.5) {
    if (sampled.a < m.params.x) { discard; }
    base = sampled.rgb;
  }
  let n = normalize(in.normal);
  let sunlight = max(dot(n, normalize(g.sun_direction.xyz)), 0.0);
  let lit = select(base * (g.ambient.xyz + g.sun_color.xyz * sunlight), base, d.flags.x > 0.5);
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  return vec4<f32>(mix(g.fog_color.rgb, lit, coverage), 1.0);
}
