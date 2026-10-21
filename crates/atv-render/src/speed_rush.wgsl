// Authored speed-rush screen pass (web/speed-rush.js): a jittered radial
// streak toward the travel focus, wind lines and a vignette. Math runs in the
// browser's GL screen space (y up); the player's quad and rider are kept crisp
// by stencil.
struct Rush {
  // amount, aspect, time, taps
  params: vec4<f32>,
  // focus.xy, pixel.xy
  focus: vec4<f32>,
  // vehicle bounds: centre.xy, half.xy (uv)
  vehicle: vec4<f32>,
};
@group(0) @binding(0) var<uniform> r: Rush;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var scene_sampler: sampler;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) id: u32) -> Varyings {
  let p = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
  var out: Varyings;
  out.uv = p;
  out.clip = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
  return out;
}

fn hash(n: f32) -> f32 { return fract(sin(n * 127.1) * 43758.5453); }
// GL texture space (v up) to the wgpu texture (v down).
fn sample(p: vec2<f32>) -> vec3<f32> {
  return textureSampleLevel(scene, scene_sampler, vec2<f32>(p.x, 1.0 - p.y), 0.0).rgb;
}
// Samples that land on the player's own vehicle carry no weight.
fn open(p: vec2<f32>) -> f32 {
  return select(smoothstep(0.85, 1.1, length((p - r.vehicle.xy) / r.vehicle.zw)), 1.0, r.vehicle.z <= 0.0);
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
  let amount = r.params.x;
  let aspect = r.params.y;
  let pixel = r.focus.zw;
  let uv = in.uv;
  let ray = uv - r.focus.xy;
  // Unit radius reaches the nearer screen edge in landscape and portrait.
  let shaped = ray * vec2<f32>(aspect, 1.0) / min(aspect, 1.0);
  let radius = length(shaped);
  let base = sample(uv);
  let strength = amount * smoothstep(0.1, 0.7, radius);
  if (strength < 0.015) { return vec4<f32>(base, 1.0); }
  // gl_FragCoord (y up) for the interleaved gradient noise.
  let frag = vec2<f32>(in.clip.x, 1.0 / pixel.y - in.clip.y);
  let jitter = fract(52.9829189 * fract(dot(frag, vec2<f32>(0.06711056, 0.00583715))));
  let reach = ray * (0.16 * strength);
  var total = base;
  var weight = 1.0;
  let taps = i32(r.params.w);
  for (var i = 1; i <= 8; i++) {
    if (i > taps) { break; }
    let p = uv - reach * ((f32(i) - jitter) / f32(taps));
    let w = open(p);
    total += sample(clamp(p, pixel * 0.5, vec2<f32>(1.0) - pixel * 0.5)) * w;
    weight += w;
  }
  var c = total / weight;
  // Wind lines: polar cells around the travel focus moving outward in log-radius.
  if (radius > 0.24) {
    let cells = 84.0;
    let a = (atan2(shaped.y, shaped.x) / 6.2831853 + 0.5) * cells;
    let id = floor(a);
    let h1 = hash(id);
    let h2 = hash(id + 31.7);
    let h3 = hash(id + 77.1);
    let across = abs(fract(a) - 0.5 - (h1 - 0.5) * 0.6) * 6.2831853 / cells * radius / max(pixel.x, pixel.y);
    let width = mix(0.7, 2.4, smoothstep(0.3, 1.0, radius)) * (0.6 + 0.8 * h2);
    let line = 1.0 - smoothstep(width * 0.5, width * 0.5 + 1.0, across);
    let s = log(radius) * 1.7 - r.params.z * (1.1 + h2 * 1.4) + h3 * 9.0;
    let on = step(hash(id * 1.37 + floor(s) * 13.1), amount * 0.3);
    let f = fract(s);
    // Faint trailing tail, bright leading head.
    let body = smoothstep(0.05, 0.42, f) * (1.0 - smoothstep(0.42, 0.47, f));
    c = mix(c, vec3<f32>(1.0), line * body * on * smoothstep(0.24, 0.65, radius) * (0.2 + 0.3 * h3));
  }
  // Gentle peripheral vignette focuses the eye on the travel line.
  c = c * (1.0 - 0.2 * amount * smoothstep(0.55, 1.15, radius));
  return vec4<f32>(c, 1.0);
}
