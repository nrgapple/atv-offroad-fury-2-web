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
// Enhanced/High tire tracks (web/tire-tracks.js): modulate-2x marks that
// press, berm and tear the lit ground, faded by simulation age.
struct Track {
  // now, life
  clock: vec4<f32>,
};
@group(1) @binding(0) var<uniform> t: Track;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) v: vec2<f32>,
  @location(1) world: vec2<f32>,
  @location(2) mark: vec3<f32>,
};

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) track: vec4<f32>, @location(2) surface: vec2<f32>) -> Varyings {
  let age = t.clock.x - track.z;
  let fade = clamp(1.0 - age / t.clock.y, 0.0, 1.0) * step(0.0, age + 0.5);
  var out: Varyings;
  out.v = track.xy;
  out.world = position.xz;
  out.mark = vec3<f32>(track.w, surface.x, surface.y) * vec3<f32>(fade, fade, 1.0);
  // Unused ring slots are all zero; live vertices sit at across = +-1.
  out.clip = select(vec4<f32>(2.0, 2.0, 2.0, 1.0), g.view_projection * vec4<f32>(position, 1.0), abs(track.x) > 0.5);
  return out;
}

fn hash(p: vec2<f32>) -> f32 { return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453); }
fn noise(p: vec2<f32>) -> f32 {
  let i = floor(p);
  var f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), f.x), mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), f.x), f.y);
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let rut = in.mark.x;
  let scuff = in.mark.y;
  let wet = in.mark.z;
  let side = abs(in.v.x);
  // Ground is uneven: marks come and go along the path.
  let clump = smoothstep(0.15, 0.7, noise(in.world * 0.35) * 0.7 + noise(in.world * 1.7) * 0.3);
  let edge = 1.0 - smoothstep(0.6, 1.0, side);
  // Rolling: compacted tread with knobby lugs and a slightly raised, lighter berm.
  let lug = 0.75 + 0.25 * smoothstep(0.3, 0.7, abs(fract(in.v.y * 2.5) * 2.0 - 1.0));
  let press = rut * mix(0.45, 1.0, clump) * edge * lug;
  let berm = rut * (1.0 - wet) * smoothstep(0.55, 0.85, side) * (1.0 - smoothstep(0.85, 1.0, side));
  var m = 1.0 - press * mix(0.45, 0.55, wet) + berm * 0.08;
  // Sliding/spinning: torn grooves and loose ridges; mud only smears darker.
  let streak = noise(vec2<f32>(in.v.x * 3.0, in.v.y * 0.25) + in.world * 0.05);
  let tear = scuff * mix(0.4, 1.0, clump) * (1.0 - smoothstep(0.45, 1.0, side));
  m = m * (mix(mix(0.7, 1.14, smoothstep(0.25, 0.75, streak)), 0.55 + 0.15 * streak, wet) * tear + 1.0 - tear);
  // Modulate-2x blend: 0.5 leaves the ground unchanged.
  return vec4<f32>(vec3<f32>(0.5 * m), 1.0);
}
