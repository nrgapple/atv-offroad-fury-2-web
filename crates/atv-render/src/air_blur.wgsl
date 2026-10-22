// Alpha Air dark blur (AirBlurRenderer in web/hype-gl.js): the finished frame
// blurred radially towards the centre, desaturated and darkened at the edges
// in big air; the rider in the middle stays sharp. Radially symmetric, so the
// wgpu screen orientation needs no flip.
struct Blur {
  // resolution.xy, amount, unused
  params: vec4<f32>,
};
@group(0) @binding(0) var<uniform> b: Blur;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var scene_sampler: sampler;

@vertex
fn vs(@builtin(vertex_index) id: u32) -> @builtin(position) vec4<f32> {
  let p = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
  return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
  let resolution = b.params.xy;
  let amount = b.params.z;
  let uv = frag.xy / resolution;
  let d = (uv - 0.5) * vec2<f32>(resolution.x / resolution.y, 1.0);
  let r = length(d);
  let edge = smoothstep(0.18, 0.85, r) * amount;
  // Zoom blur: ten taps stepping towards the centre, longer at the edges.
  let step = (uv - 0.5) * edge * 0.035;
  var c = vec3<f32>(0.0);
  for (var i = 0; i < 10; i++) {
    c += textureSampleLevel(scene, scene_sampler, uv - step * f32(i), 0.0).rgb;
  }
  c /= 10.0;
  let grey = dot(c, vec3<f32>(0.299, 0.587, 0.114));
  c = mix(c, vec3<f32>(grey), 0.45 * edge);
  c *= 1.0 - 0.7 * smoothstep(0.25, 1.0, r) * amount;
  return vec4<f32>(c, 1.0);
}
