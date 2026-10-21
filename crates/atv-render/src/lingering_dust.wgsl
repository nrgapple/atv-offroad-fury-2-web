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
// Enhanced/High lingering dust (web/lingering-dust.js): authored soft puffs
// that rise and spread slowly, faded by distance towards the fog colour.
@group(1) @binding(0) var unused_texture: texture_2d<f32>;
@group(1) @binding(1) var unused_sampler: sampler;
struct Dust {
  // camera right, then clock
  right: vec4<f32>,
  // camera up, then fog end
  up: vec4<f32>,
  light: vec4<f32>,
};
struct Unused {
  value: vec4<f32>,
};
@group(2) @binding(0) var<uniform> d: Dust;
@group(2) @binding(1) var<uniform> unused: Unused;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) v: vec2<f32>,
  @location(1) color: vec4<f32>,
};

@vertex
fn vs(@location(0) corner: vec2<f32>, @location(1) puff: vec4<f32>, @location(2) shape: vec2<f32>,
    @location(3) tint: vec3<f32>) -> Varyings {
  let age = d.right.w - puff.w;
  let t = age / shape.x;
  let alive = step(0.0, age) * step(t, 1.0) * step(0.001, shape.x);
  let size = shape.y * (0.45 + 1.1 * sqrt(clamp(t, 0.0, 1.0)));
  let center = puff.xyz + vec3<f32>(0.0, 0.6 + 1.4 * t, 0.0);
  let world = center + (d.right.xyz * corner.x + d.up.xyz * corner.y) * size;
  let fog_end = d.up.w;
  let fog = smoothstep(fog_end * 0.35, fog_end, length(center - g.eye.xyz));
  var out: Varyings;
  out.color = vec4<f32>(mix(tint * d.light.rgb, g.fog_color.rgb, fog),
    alive * 0.24 * smoothstep(0.0, 0.12, t) * (1.0 - t) * (1.0 - fog));
  out.v = corner;
  out.clip = select(vec4<f32>(2.0, 2.0, 2.0, 1.0), g.view_projection * vec4<f32>(world, 1.0), alive > 0.0);
  return out;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let r = dot(in.v, in.v);
  if (r >= 1.0) { discard; }
  let a = 1.0 - r;
  return vec4<f32>(in.color.rgb, in.color.a * a * a);
}
