// Remastered rain (authored, not from the original game): falling streaks
// wrapped in a box around the camera, splash rings on the ground near it and
// beads of water on the lens. Original/Enhanced/High never build this pass.
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
  shadow_matrix: mat4x4<f32>,
  shadow: vec4<f32>,
  remaster: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var unused_texture: texture_2d<f32>;
@group(1) @binding(1) var unused_sampler: sampler;
struct Rain {
  // eye, then clock (seconds)
  eye: vec4<f32>,
  // fall velocity (ft/s), then streak opacity
  fall: vec4<f32>,
  // drop colour, then aspect (width / height)
  color: vec4<f32>,
  // box half width, box height, streak count, splash life
  shape: vec4<f32>,
};
struct Unused {
  value: vec4<f32>,
};
@group(2) @binding(0) var<uniform> r: Rain;
@group(2) @binding(1) var<uniform> unused: Unused;

struct Varyings {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) alpha: f32,
};

fn hash(n: u32) -> f32 {
  var x = n * 747796405u + 2891336453u;
  x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
  x = (x >> 22u) ^ x;
  return f32(x) / 4294967295.0;
}

// World-anchored wrap into [-size/2, size/2) around the camera.
fn wrap(value: f32, size: f32) -> f32 {
  return (fract(value / size) - 0.5) * size;
}

@vertex
fn vs_streak(@builtin(vertex_index) vertex: u32, @builtin(instance_index) id: u32) -> Varyings {
  let half_width = r.shape.x;
  let height = r.shape.y;
  let size = half_width * 2.0;
  let t = r.eye.w;
  let seed = vec3<f32>(hash(id * 3u), hash(id * 3u + 1u), hash(id * 3u + 2u));
  let speed = 0.8 + 0.4 * hash(id * 7u + 5u);
  let fall = r.fall.xyz * speed;
  let origin = seed * vec3<f32>(size, height, size) + fall * t;
  // Streaks span from 0.3 box heights below the eye to 0.7 above it.
  let top = r.eye.xyz + vec3<f32>(
    wrap(origin.x - r.eye.x, size),
    wrap(origin.y - r.eye.y - height * 0.2, height) + height * 0.2,
    wrap(origin.z - r.eye.z, size),
  );
  // A 1/20 s exposure gives the streak its length.
  let along = fall / 20.0;
  let corners = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
  );
  let corner = corners[vertex];
  let to_eye = r.eye.xyz - top;
  let distance = length(to_eye);
  let side_dir = normalize(cross(along, to_eye + vec3<f32>(0.0001, 0.0, 0.0)));
  // At least about a pixel wide at any distance.
  let width = max(0.01, distance * 0.0022);
  let point = top + along * corner.y + side_dir * corner.x * width;
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(point, 1.0);
  out.uv = corner;
  let near_fade = smoothstep(1.5, 4.0, distance);
  let far_fade = 1.0 - smoothstep(half_width * 0.5, half_width, distance);
  out.alpha = r.fall.w * near_fade * far_fade * (0.6 + 0.4 * seed.x);
  return out;
}

@fragment
fn fs_streak(in: Varyings) -> @location(0) vec4<f32> {
  let across = 1.0 - abs(in.uv.x);
  let ends = smoothstep(0.0, 0.25, in.uv.y) * smoothstep(1.0, 0.6, in.uv.y);
  return vec4<f32>(r.color.rgb, in.alpha * across * ends);
}

// Splash: x, y, z, birth (seconds on the rain clock); then the ground's rise
// per foot along x and z, so the ring lies on a slope.
@vertex
fn vs_splash(
  @builtin(vertex_index) vertex: u32,
  @location(0) splash: vec4<f32>,
  @location(1) slope: vec4<f32>,
) -> Varyings {
  let corners = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
  );
  let corner = corners[vertex];
  let age = (r.eye.w - splash.w) / r.shape.w;
  let radius = 0.15 + 0.55 * clamp(age, 0.0, 1.0);
  let flat = corner * radius;
  let point = splash.xyz + vec3<f32>(flat.x, 0.04 + dot(flat, slope.xy), flat.y);
  var out: Varyings;
  out.clip = g.view_projection * vec4<f32>(point, 1.0);
  out.uv = corner;
  out.alpha = select(0.0, 1.0 - age, age >= 0.0 && age < 1.0);
  if (out.alpha <= 0.0) { out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0); }
  return out;
}

@fragment
fn fs_splash(in: Varyings) -> @location(0) vec4<f32> {
  let d = length(in.uv);
  let ring = smoothstep(0.7, 0.9, d) * (1.0 - smoothstep(0.9, 1.0, d));
  return vec4<f32>(r.color.rgb, ring * in.alpha * 0.55);
}

// Lens bead: clip x, clip y, radius (fraction of the view height), opacity.
@vertex
fn vs_lens(@builtin(vertex_index) vertex: u32, @location(0) bead: vec4<f32>) -> Varyings {
  let corners = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
  );
  let corner = corners[vertex];
  var out: Varyings;
  // Beads are a little taller than wide, as water hangs on glass.
  let extent = vec2<f32>(bead.z * 2.0 / r.color.w, bead.z * 2.3);
  out.clip = vec4<f32>(bead.xy + corner * extent, 0.0, 1.0);
  out.uv = corner;
  out.alpha = bead.w;
  if (bead.w <= 0.0) { out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0); }
  return out;
}

@fragment
fn fs_lens(in: Varyings) -> @location(0) vec4<f32> {
  let d = length(in.uv);
  if (d >= 1.0) { discard; }
  // A dark meniscus rim, a clear body and a sky-lit highlight up and left.
  let rim = smoothstep(0.6, 0.95, d) * (1.0 - smoothstep(0.95, 1.0, d));
  let glint = 1.0 - smoothstep(0.0, 0.28, length(in.uv - vec2<f32>(-0.32, 0.38)));
  let body = 0.18 * (1.0 - d);
  let light = mix(r.color.rgb, vec3<f32>(1.0), 0.6);
  let color = mix(light * (0.55 + body), light, glint);
  let alpha = max(max(rim * 0.45, glint * 0.8), 0.12);
  return vec4<f32>(color * (1.0 - rim * 0.6), alpha * in.alpha);
}
