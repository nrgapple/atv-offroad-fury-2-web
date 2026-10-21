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
// Enhanced/High soft contact shadows (web/vehicle-shadows.js): one
// ground-draped elliptical blob per caster, darkening what is beneath.
struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) shade: vec3<f32>,
};

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) shade: vec3<f32>) -> Varyings {
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(position, 1.0);
  out.shade = shade;
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let r = length(in.shade.xy);
  let a = in.shade.z * (1.0 - smoothstep(0.4, 1.0, r));
  return vec4<f32>(0.0, 0.0, 0.0, a * a * (3.0 - 2.0 * a));
}
