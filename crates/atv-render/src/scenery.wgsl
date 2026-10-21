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
// Static scene models: web/game.js's scene shader for sceneDraws (useTexture,
// useVertexColor, sceneryBlend, alphaFailPass, sceneryReflectionPlane,
// sceneryUvOffset) with the provisional prop lighting and GS fog.
@group(1) @binding(0) var base_texture: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;
struct Material {
  // use texture, vertex color, scenery blend, alpha-fail pass
  flags: vec4<f32>,
  // alpha cutoff, fog enabled, reflection plane
  params: vec4<f32>,
  uv_offset: vec4<f32>,
};
struct Instance {
  model: mat4x4<f32>,
  // unlit
  flags: vec4<f32>,
};
@group(2) @binding(0) var<uniform> m: Material;
@group(2) @binding(1) var<uniform> d: Instance;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) color: vec4<f32>,
  // Coverage and clip W, carried so the ratio interpolates screen-linearly.
  @location(3) fog: vec2<f32>,
};

fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(g.fog.x + g.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

fn finish(world: vec4<f32>, normal: vec3<f32>, uv: vec2<f32>, color: vec4<f32>) -> Varyings {
  var out: Varyings;
  out.clip = g.view_projection * world;
  out.uv = uv;
  out.normal = (d.model * vec4<f32>(normal, 0.0)).xyz;
  out.color = color * 255.0;
  let fogged = g.fog.z > 0.5 && m.params.y > 0.5;
  let coverage = select(1.0, native_fog_coverage(length(world.xyz - g.eye.xyz)), fogged);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  return out;
}

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) color: vec4<f32>) -> Varyings {
  return finish(d.model * vec4<f32>(position, 1.0), normal, uv + m.uv_offset.xy, color);
}

@vertex
fn vs_reflect(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) color: vec4<f32>, @location(4) local_position: vec3<f32>, @location(5) local_normal: vec3<f32>,
      @location(6) n0: vec3<f32>, @location(7) n1: vec3<f32>, @location(8) n2: vec3<f32>, @location(9) n3: vec3<f32>) -> Varyings {
  // TriStripPrologue transforms the world camera into each rigid node's
  // coordinates. The compressed reflection programs retain the authored
  // scaled normal; normalizing it would change the original UVs.
  let node = d.model * mat4x4<f32>(vec4<f32>(n0, 0.0), vec4<f32>(n1, 0.0), vec4<f32>(n2, 0.0), vec4<f32>(n3, 1.0));
  let basis = mat3x3<f32>(node[0].xyz, node[1].xyz, node[2].xyz);
  let local_eye = transpose(basis) * (g.eye.xyz - node[3].xyz);
  let direction = normalize(local_eye - local_position);
  let reflected = vec3<f32>(0.5) - 0.5 * direction + local_normal * dot(local_normal, direction);
  let plane = i32(m.params.z + 0.5);
  var texcoord = reflected.yz;
  if (plane == 1) { texcoord = reflected.xy; } else if (plane == 2) { texcoord = reflected.xz; }
  return finish(d.model * vec4<f32>(position, 1.0), normal, texcoord, color);
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  var base = vec3<f32>(0.68, 0.70, 0.73);
  var source_alpha = 1.0;
  var sampled = textureSample(base_texture, base_sampler, in.uv);
  if (m.flags.x > 0.5) {
    if (m.flags.y > 0.5) { sampled = sampled * (in.color / 128.0); }
    let cutoff = m.params.x;
    let fails = (sampled.a < cutoff);
    let accepted_fails = (sampled.a >= cutoff);
    if (select(fails, accepted_fails, m.flags.w > 0.5)) { discard; }
    base = sampled.rgb;
    // GS source alpha uses 128 for unity, while decoded PST bytes use 255.
    source_alpha = select(1.0, clamp(sampled.a * (255.0 / 128.0), 0.0, 1.0), m.flags.z > 0.5);
  }
  let n = normalize(in.normal);
  let sunlight = max(dot(n, normalize(g.sun_direction.xyz)), 0.0);
  let lit = select(base * (g.ambient.xyz + g.sun_color.xyz * sunlight), base, d.flags.x > 0.5);
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  return vec4<f32>(mix(g.fog_color.rgb, lit, coverage), source_alpha);
}
