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
  // untextured colour; w: the pass opacity (the Lap Attack ghost's
  // GhostOpacity, else 1)
  surface: vec4<f32>,
  // amount, reveal
  mud: vec4<f32>,
  // quad paint mode, hue (render-support.js PAINT_PALETTE)
  paint: vec4<f32>,
  // Remastered world shadows: world to shadow texture; receive, the texel in
  // texture space and in feet
  far_matrix: mat4x4<f32>,
  far: vec4<f32>,
};
struct Bones {
  world: mat4x4<f32>,
  nodes: array<mat4x4<f32>, 32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var world_shadow_map: texture_depth_2d;
@group(0) @binding(2) var world_shadow_sampler: sampler_comparison;
@group(1) @binding(0) var<uniform> bones: Bones;
// textured, fog, has mud map, paintable quad body
@group(2) @binding(0) var<uniform> material: vec4<f32>;
@group(2) @binding(1) var base_texture: texture_2d<f32>;
@group(2) @binding(2) var base_sampler: sampler;
@group(2) @binding(3) var mud_texture: texture_2d<f32>;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) uv: vec2<f32>,
  // Coverage and clip W, so the ratio interpolates screen-linearly (web/fog.js).
  @location(2) fog: vec2<f32>,
  @location(3) world: vec3<f32>,
};

// render-support.js paintGlsl: saturated body-atlas pixels take the hue
// (keeping their shading), lift to white or darken to black.
fn paint_rgb2hsv(c: vec3<f32>) -> vec3<f32> {
  let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
  let p = mix(vec4<f32>(c.b, c.g, k.w, k.z), vec4<f32>(c.g, c.b, k.x, k.y), step(c.b, c.g));
  let q = mix(vec4<f32>(p.x, p.y, p.w, c.r), vec4<f32>(c.r, p.y, p.z, p.x), step(p.x, c.r));
  let d = q.x - min(q.w, q.y);
  return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + 1e-6)), d / (q.x + 1e-6), q.x);
}
fn paint_hsv2rgb(c: vec3<f32>) -> vec3<f32> {
  let p = abs(fract(vec3<f32>(c.x) + vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
  return c.z * mix(vec3<f32>(1.0), clamp(p - 1.0, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}
fn apply_paint(base: vec3<f32>, paint: vec2<f32>) -> vec3<f32> {
  if (paint.x < 0.5) {
    return base;
  }
  let h = paint_rgb2hsv(base);
  let w = smoothstep(0.22, 0.42, h.y) * smoothstep(0.05, 0.16, h.z);
  var painted = vec3<f32>(h.z * 0.2);
  if (paint.x < 1.5) {
    painted = paint_hsv2rgb(vec3<f32>(paint.y, max(h.y, 0.55), h.z));
  } else if (paint.x < 2.5) {
    painted = vec3<f32>(min(1.0, h.z * 1.1 + 0.18));
  }
  return mix(base, painted, w);
}

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
  out.world = world.xyz;
  out.clip = g.view_projection * world;
  let fogged = g.fog.z > 0.5 && material.y > 0.5;
  let coverage = select(1.0, native_fog_coverage(length(world.xyz - g.eye.xyz)), fogged);
  out.fog = vec2<f32>(coverage * out.clip.w, out.clip.w);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let n = normalize(in.normal);
  let texel = textureSample(base_texture, base_sampler, in.uv);
  let sampled = texel.rgb;
  let flags = u32(material.x + 0.5);
  let textured = (flags & 1u) != 0u;
  let blended = (flags & 2u) != 0u;
  // GS source alpha: decoded bytes over 128 are full coverage. A texel with no
  // alpha at all draws nothing (no colour, no depth), as the cut-outs need.
  let source_alpha = select(1.0, clamp(texel.a * (255.0 / 128.0), 0.0, 1.0), blended);
  if (blended && texel.a < 0.5 / 255.0) { discard; }
  let mud = textureSample(mud_texture, base_sampler, in.uv);
  var base = select(g.surface.rgb, sampled, textured);
  if (textured && material.w > 0.5) {
    base = apply_paint(base, g.paint.xy);
  }
  // game.js motorcycleMudAmount: MX mud blends linearly by alpha; the ATV2
  // dirt pass reveals heavier-alpha texels first.
  let amount = g.mud.x;
  if (textured && material.z > 0.5 && amount > 0.0) {
    let strength = clamp(mud.a * (255.0 / 128.0), 0.0, 1.0);
    let weight = select(clamp(mud.a * (255.0 / 128.0) * amount, 0.0, 1.0),
      strength * smoothstep(1.0 - amount, 1.12 - amount, mud.a), g.mud.y > 0.5);
    base = mix(base, mud.rgb, weight);
  }
  var sunlight = max(dot(n, normalize(g.sun_direction.xyz)), 0.0);
  // Remastered: the world's shadows take the sun off the rider too.
  if (g.far.x > 0.5) { sunlight = sunlight * world_shadow(in.world, n); }
  let lit = base * (g.ambient.rgb + g.sun_color.rgb * sunlight);
  let coverage = select(1.0, clamp(in.fog.x / in.fog.y, 0.0, 1.0), in.fog.y != 0.0);
  return vec4<f32>(mix(g.fog_color.rgb, lit, coverage), source_alpha * g.surface.w);
}

// Remastered world shadow map (crate::world_shadows): the lit share of the
// sun at `p`, 1 outside the map and fading in over its last few percent.
// Four bilinear compare taps, stepped off the surface along its normal.
fn world_shadow(p: vec3<f32>, normal: vec3<f32>) -> f32 {
  let c = g.far_matrix * vec4<f32>(p + normal * g.far.z, 1.0);
  let s = c.xyz / c.w;
  let edge = min(min(s.x, s.y), min(1.0 - s.x, 1.0 - s.y));
  if (edge <= 0.0 || s.z >= 1.0) { return 1.0; }
  var lit = 0.0;
  for (var i = 0; i < 4; i++) {
    let o = vec2<f32>(select(-0.5, 0.5, i == 1 || i == 3), select(-0.5, 0.5, i >= 2)) * g.far.y;
    lit += textureSampleCompareLevel(world_shadow_map, world_shadow_sampler, s.xy + o, s.z - 0.0004);
  }
  return mix(1.0, lit * 0.25, smoothstep(0.0, 0.06, edge));
}
