// The 3D scene, drawn below the display's density, stretched over the
// full-density surface (bilinear) before the UI draws on top at full density.
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) id: u32) -> Varyings {
  let p = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
  var out: Varyings;
  // Clip space has y up; the texture has v down.
  out.uv = vec2<f32>(p.x, 1.0 - p.y);
  out.clip = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  return vec4<f32>(textureSampleLevel(scene, scene_sampler, in.uv, 0.0).rgb, 1.0);
}
