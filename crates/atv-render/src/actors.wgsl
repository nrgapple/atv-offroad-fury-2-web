// Keyframed TOY actors (research/AMBIENT_ACTORS.md): rigid nodes posed by
// their motions, with the scenery shader's material rules (texture,
// prelit vertex colour, cutout, GS source alpha), provisional prop
// lighting and GS fog.
struct Globals {
  view_projection: mat4x4<f32>,
  eye: vec4<f32>,
  fog_color: vec4<f32>,
  fog: vec4<f32>,
  ambient: vec4<f32>,
  sun_color: vec4<f32>,
  sun_direction: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var base_texture: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;
struct Material {
  // use texture, vertex colour, blend
  flags: vec4<f32>,
  // alpha cutoff, fog enabled
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
  @location(2) color: vec4<f32>,
  @location(3) fog: vec2<f32>,
};

fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(g.fog.x + g.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) node: f32, @location(4) color: vec4<f32>) -> Varyings {
  let pose = d.model * d.bones[min(u32(node), 31u)];
  let world = pose * vec4<f32>(position, 1.0);
  var out: Varyings;
  out.clip = g.view_projection * world;
  out.uv = uv;
  out.normal = (pose * vec4<f32>(normal, 0.0)).xyz;
  out.color = color * 255.0;
  let fogged = g.fog.z > 0.5 && m.params.y > 0.5;
  let coverage = select(1.0, native_fog_coverage(length(world.xyz - g.eye.xyz)), fogged);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  var base = vec3<f32>(0.68, 0.70, 0.73);
  var source_alpha = 1.0;
  var sampled = textureSample(base_texture, base_sampler, in.uv);
  if (m.flags.x > 0.5) {
    if (m.flags.y > 0.5) { sampled = sampled * (in.color / 128.0); }
    if (sampled.a < m.params.x) { discard; }
    base = sampled.rgb;
    // GS source alpha uses 128 for unity, while decoded PST bytes use 255.
    source_alpha = select(1.0, clamp(sampled.a * (255.0 / 128.0), 0.0, 1.0), m.flags.z > 0.5);
  }
  let n = normalize(in.normal);
  let sunlight = max(dot(n, normalize(g.sun_direction.xyz)), 0.0);
  let unlit = d.flags.x > 0.5 || m.flags.y > 0.5;
  let lit = select(base * (g.ambient.xyz + g.sun_color.xyz * sunlight), base, unlit);
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  return vec4<f32>(mix(g.fog_color.rgb, lit, coverage), source_alpha);
}
