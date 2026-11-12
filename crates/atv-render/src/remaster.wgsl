// Remastered preset finish (authored, not from the original game): the
// finished frame gains bloom from its brightest parts, then a gentle film
// grade, vignette and dither. The 8-bit frame stays display-referred, so the
// bloom is a soft-knee threshold over it and nothing here needs float targets.
struct Remaster {
  // threshold, knee, intensity, exposure
  bloom: vec4<f32>,
  // contrast, vignette, saturation, aspect
  grade: vec4<f32>,
};
@group(0) @binding(0) var<uniform> r: Remaster;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var glow: texture_2d<f32>;
@group(0) @binding(3) var linear_sampler: sampler;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) id: u32) -> Varyings {
  let p = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
  var out: Varyings;
  out.clip = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
  out.uv = vec2<f32>(p.x, 1.0 - p.y);
  return out;
}

// Four bilinear taps around `uv`, `reach` source texels apart.
fn box(uv: vec2<f32>, reach: f32) -> vec3<f32> {
  let texel = reach / vec2<f32>(textureDimensions(source));
  return (textureSampleLevel(source, linear_sampler, uv + vec2<f32>(-texel.x, -texel.y), 0.0).rgb
    + textureSampleLevel(source, linear_sampler, uv + vec2<f32>(texel.x, -texel.y), 0.0).rgb
    + textureSampleLevel(source, linear_sampler, uv + vec2<f32>(-texel.x, texel.y), 0.0).rgb
    + textureSampleLevel(source, linear_sampler, uv + vec2<f32>(texel.x, texel.y), 0.0).rgb) * 0.25;
}

// The frame's brightest parts at half size, with a soft knee so the glow
// fades in instead of cutting on at the threshold.
@fragment
fn fs_prefilter(in: Varyings) -> @location(0) vec4<f32> {
  let c = box(in.uv, 1.0);
  let brightness = max(c.r, max(c.g, c.b));
  let knee = r.bloom.y;
  var soft = clamp(brightness - r.bloom.x + knee, 0.0, 2.0 * knee);
  soft = soft * soft / (4.0 * knee + 0.0001);
  let kept = max(soft, brightness - r.bloom.x) / max(brightness, 0.0001);
  return vec4<f32>(c * kept, 1.0);
}

@fragment
fn fs_down(in: Varyings) -> @location(0) vec4<f32> {
  return vec4<f32>(box(in.uv, 1.0), 1.0);
}

// Drawn additively into the next larger level.
@fragment
fn fs_up(in: Varyings) -> @location(0) vec4<f32> {
  return vec4<f32>(box(in.uv, 0.75), 1.0);
}

@fragment
fn fs_composite(in: Varyings) -> @location(0) vec4<f32> {
  let scene = textureSampleLevel(source, linear_sampler, in.uv, 0.0).rgb;
  let bloom = textureSampleLevel(glow, linear_sampler, in.uv, 0.0).rgb;
  var c = (scene + bloom * r.bloom.z) * r.bloom.w;
  // Highlights roll off instead of clipping where the glow adds up.
  let over = max(c - vec3<f32>(0.8), vec3<f32>(0.0));
  c = min(c, vec3<f32>(0.8)) + 0.2 * (1.0 - exp(-over * 5.0));
  // A mild S-curve, then a touch more colour.
  c = mix(c, c * c * (3.0 - 2.0 * c), r.grade.x);
  let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
  c = mix(vec3<f32>(luma), c, r.grade.z);
  // Highlights lean warm; the dark end keeps its authored colour.
  c = c + max(luma - 0.5, 0.0) * vec3<f32>(0.03, 0.006, -0.035);
  let d = (in.uv - 0.5) * vec2<f32>(r.grade.w, 1.0);
  c = c * (1.0 - r.grade.y * smoothstep(0.45, 1.1, length(d)));
  // One-step dither hides banding in skies and fog.
  let noise = fract(sin(dot(in.clip.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453);
  c = c + (noise - 0.5) / 255.0;
  return vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
