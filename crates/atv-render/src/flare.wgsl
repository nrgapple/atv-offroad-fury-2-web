// Authored sun flares (research/SUN_FLARES.md): the flare render target
// drawn additively (GS Cs*As + Cd) over the scene, centred on the sun. Its
// alpha is the visible share of a depth-tested square around the sun.
struct Globals {
  view_projection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var flare_texture: texture_2d<f32>;
@group(1) @binding(1) var flare_sampler: sampler;
struct Flare {
  // sun in NDC, then the sprite half size in NDC
  center: vec4<f32>,
  // GS vertex RGB over 128, then the GS vertex alpha (0-255)
  color: vec4<f32>,
  // the sample square in NDC: centre, half size
  probe: vec4<f32>,
  // x: query results are current; y: the terrain rays' visible share
  occlusion: vec4<f32>,
};
// One u64 per occlusion query: 16 cells, then the always-passing sentinel.
struct Results {
  query: array<vec4<u32>, 9>,
};
@group(2) @binding(0) var<uniform> f: Flare;
@group(2) @binding(1) var<uniform> results: Results;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  // GS As over 128 (up to 255/128)
  @location(1) strength: f32,
};

fn corner(vertex: u32) -> vec2<f32> {
  var corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
  return corners[vertex % 6u];
}

fn passed(i: u32) -> bool {
  let q = results.query[i / 2u];
  let v = select(q.xy, q.zw, i % 2u == 1u);
  return (v.x | v.y) != 0u;
}

// The visible share: the cells that passed, once the sentinel has reported
// (WebGL releases results only between frames); else the terrain rays.
fn visible_share() -> f32 {
  if (f.occlusion.x < 0.5 || !passed(16u)) {
    return f.occlusion.y;
  }
  var seen = 0u;
  for (var i = 0u; i < 16u; i++) {
    seen += select(0u, 1u, passed(i));
  }
  return f32(seen) / 16.0;
}

@vertex
fn vs_sprite(@builtin(vertex_index) vertex: u32) -> Varyings {
  let c = corner(vertex);
  var out: Varyings;
  out.clip = vec4<f32>(f.center.xy + c * f.center.zw, 0.5, 1.0);
  out.uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
  // The target's alpha is the averaged z-test (0 or 0xff per sample);
  // MODULATE scales it by the vertex alpha over 128, clamped to 255.
  let target_alpha = floor(visible_share() * 255.0);
  out.strength = min(255.0, floor(target_alpha * f.color.w / 128.0)) / 128.0;
  return out;
}

@fragment
fn fs_sprite(in: Varyings) -> @location(0) vec4<f32> {
  let texel = textureSample(flare_texture, flare_sampler, in.uv);
  // As can exceed 1 on the GS, so it scales the colour and the blend adds
  // it whole (SrcAlpha with alpha 1).
  return vec4<f32>(texel.rgb * f.color.rgb * in.strength, 1.0);
}

// Probes: vertices 6i..6i+5 cover cell i of a 4x4 grid over the sample
// square at the far plane, passing where nothing was drawn in front;
// cell 16 is the whole square at the near plane, which always passes.
@vertex
fn vs_probe(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
  let cell = vertex / 6u;
  let c = corner(vertex);
  if (cell >= 16u) {
    return vec4<f32>(f.probe.xy + c * f.probe.zw, 0.0, 1.0);
  }
  let grid = vec2<f32>(f32(cell % 4u), f32(cell / 4u));
  let low = f.probe.xy - f.probe.zw + grid * f.probe.zw * 0.5;
  let p = low + (c * 0.5 + 0.5) * f.probe.zw * 0.5;
  return vec4<f32>(p, 0.9999999, 1.0);
}

@fragment
fn fs_probe() -> @location(0) vec4<f32> {
  return vec4<f32>(0.0);
}
