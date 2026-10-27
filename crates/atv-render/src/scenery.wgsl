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
  shadow_matrix: mat4x4<f32>,
  // receive, texel size
  shadow: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var vehicle_shadow_map: texture_depth_2d;
@group(0) @binding(2) var vehicle_shadow_sampler: sampler_comparison;
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
// One record per draw and pass: its material, then its instance (one
// dynamic offset per draw).
struct Record {
  m: Material,
  d: Instance,
};
@group(2) @binding(0) var<uniform> r: Record;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) color: vec4<f32>,
  // Coverage and clip W, carried so the ratio interpolates screen-linearly.
  @location(3) fog: vec2<f32>,
  @location(4) world: vec3<f32>,
};

fn native_fog_coverage(distance: f32) -> f32 {
  let encoded = clamp(g.fog.x + g.fog.y * distance, 524288.0, 524543.0);
  return f32((bitcast<u32>(encoded) >> 4u) & 255u) / 255.0;
}

fn finish(world: vec4<f32>, normal: vec3<f32>, uv: vec2<f32>, color: vec4<f32>) -> Varyings {
  var out: Varyings;
  out.clip = g.view_projection * world;
  out.world = world.xyz;
  out.uv = uv;
  out.normal = (r.d.model * vec4<f32>(normal, 0.0)).xyz;
  out.color = color * 255.0;
  let fogged = g.fog.z > 0.5 && r.m.params.y > 0.5;
  let coverage = select(1.0, native_fog_coverage(length(world.xyz - g.eye.xyz)), fogged);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  return out;
}

@vertex
// Position with U and normal with V share attributes (fewer GL attribute
// setups per pipeline change); the values are unchanged.
fn vs(@location(0) position_u: vec4<f32>, @location(1) normal_v: vec4<f32>,
      @location(3) color: vec4<f32>) -> Varyings {
  let position = position_u.xyz;
  let normal = normal_v.xyz;
  let uv = vec2<f32>(position_u.w, normal_v.w);
  return finish(r.d.model * vec4<f32>(position, 1.0), normal, uv + r.m.uv_offset.xy, color);
}

@vertex
fn vs_reflect(@location(0) position_u: vec4<f32>, @location(1) normal_v: vec4<f32>,
      @location(3) color: vec4<f32>, @location(4) local_position: vec3<f32>, @location(5) local_normal: vec3<f32>,
      @location(6) n0: vec3<f32>, @location(7) n1: vec3<f32>, @location(8) n2: vec3<f32>, @location(9) n3: vec3<f32>) -> Varyings {
  let position = position_u.xyz;
  let normal = normal_v.xyz;
  // TriStripPrologue transforms the world camera into each rigid node's
  // coordinates. The compressed reflection programs retain the authored
  // scaled normal; normalizing it would change the original UVs.
  let node = r.d.model * mat4x4<f32>(vec4<f32>(n0, 0.0), vec4<f32>(n1, 0.0), vec4<f32>(n2, 0.0), vec4<f32>(n3, 1.0));
  let basis = mat3x3<f32>(node[0].xyz, node[1].xyz, node[2].xyz);
  let local_eye = transpose(basis) * (g.eye.xyz - node[3].xyz);
  let direction = normalize(local_eye - local_position);
  let reflected = vec3<f32>(0.5) - 0.5 * direction + local_normal * dot(local_normal, direction);
  let plane = i32(r.m.params.z + 0.5);
  var texcoord = reflected.yz;
  if (plane == 1) { texcoord = reflected.xy; } else if (plane == 2) { texcoord = reflected.xz; }
  return finish(r.d.model * vec4<f32>(position, 1.0), normal, texcoord, color);
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  var base = vec3<f32>(0.68, 0.70, 0.73);
  var source_alpha = 1.0;
  var sampled = textureSample(base_texture, base_sampler, in.uv);
  if (r.m.flags.x > 0.5) {
    if (r.m.flags.y > 0.5) { sampled = sampled * (in.color / 128.0); }
    let cutoff = r.m.params.x;
    let fails = (sampled.a < cutoff);
    let accepted_fails = (sampled.a >= cutoff);
    if (select(fails, accepted_fails, r.m.flags.w > 0.5)) { discard; }
    base = sampled.rgb;
    // GS source alpha uses 128 for unity, while decoded PST bytes use 255.
    source_alpha = select(1.0, clamp(sampled.a * (255.0 / 128.0), 0.0, 1.0), r.m.flags.z > 0.5);
  }
  let n = normalize(in.normal);
  let sunlight = max(dot(n, normalize(g.sun_direction.xyz)), 0.0);
  let lit = select(base * (g.ambient.xyz + g.sun_color.xyz * sunlight), base, r.d.flags.x > 0.5)
    * vehicle_shadow(in.world);
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  // The model's colour alpha (Enduro gates fade, 00228ea0).
  return vec4<f32>(mix(g.fog_color.rgb, lit, coverage), source_alpha * r.d.flags.y);
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
