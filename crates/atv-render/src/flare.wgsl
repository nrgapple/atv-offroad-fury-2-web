// Authored sun flares (research/SUN_FLARES.md): the flare texture drawn
// additively (GS Cs*As + Cd) over the scene, centred on the sun.
struct Globals {
  view_projection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var flare_texture: texture_2d<f32>;
@group(1) @binding(1) var flare_sampler: sampler;
struct Flare {
  // sun in NDC, then the sprite half size in NDC
  center: vec4<f32>,
  // GS vertex RGB over 128, then alpha over 128 times the visible share
  color: vec4<f32>,
};
struct Unused {
  value: vec4<f32>,
};
@group(2) @binding(0) var<uniform> f: Flare;
@group(2) @binding(1) var<uniform> unused: Unused;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_sprite(@builtin(vertex_index) vertex: u32) -> Varyings {
  var corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
  let c = corners[vertex];
  var out: Varyings;
  out.clip = vec4<f32>(f.center.xy + c * f.center.zw, 0.5, 1.0);
  out.uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
  return out;
}

@fragment
fn fs_sprite(in: Varyings) -> @location(0) vec4<f32> {
  let texel = textureSample(flare_texture, flare_sampler, in.uv);
  // GS MODULATE: texel alpha in 0-128 units times the vertex alpha.
  let alpha = clamp(texel.a * (255.0 / 128.0), 0.0, 1.0) * f.color.w;
  return vec4<f32>(texel.rgb * f.color.rgb, alpha);
}
