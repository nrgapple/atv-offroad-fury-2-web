// Menu model previews: frontend.js ModelView's vertex and fragment shaders.
struct Globals {
    bones: array<mat4x4<f32>, 32>,
    // centre xyz, yaw
    center: vec4<f32>,
    // extent x, y
    extent: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> flags: vec4<f32>;
@group(1) @binding(1) var image: texture_2d<f32>;
@group(1) @binding(2) var image_sampler: sampler;

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) tex: vec2<f32>,
    @location(1) light: f32,
};

@vertex
fn vs(
    @location(0) p: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) node: f32,
) -> Varyings {
    let yaw = globals.center.w;
    let c = cos(yaw);
    let s = sin(yaw);
    let r = mat3x3<f32>(vec3(c, 0., -s), vec3(0., 1., 0.), vec3(s, 0., c));
    let bone = globals.bones[u32(node)];
    let v = r * ((bone * vec4(p, 1.)).xyz - globals.center.xyz);
    let pitch = .25;
    let y = v.y * cos(pitch) - v.z * sin(pitch);
    let z = v.y * sin(pitch) + v.z * cos(pitch);
    var out: Varyings;
    // GL clip z = -z / 40 mapped from [-1, 1] to wgpu's [0, 1].
    out.position = vec4(-v.x / globals.extent.x, y / globals.extent.y, .5 - z / 80., 1.);
    out.tex = uv;
    let nn = normalize(r * mat3x3<f32>(bone[0].xyz, bone[1].xyz, bone[2].xyz) * n);
    out.light = .36 + .7 * max(dot(nn, normalize(vec3(.4, .8, .6))), 0.)
        + .16 * max(dot(nn, normalize(vec3(-.7, .1, -.7))), 0.);
    return out;
}

// render-support.js paintGlsl (also in vehicle.wgsl): saturated body-atlas
// pixels take the hue, lift to white or darken to black.
fn paint_rgb2hsv(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.b, c.g, k.w, k.z), vec4<f32>(c.g, c.b, k.x, k.y), step(c.b, c.g));
    let q = mix(vec4<f32>(p.x, p.y, p.w, c.r), vec4<f32>(c.r, p.y, p.z, p.x), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + 1e-6)), d / (q.x + 1e-6), q.x);
}
fn paint_hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let p = abs(fract(vec3<f32>(c.x) + vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
    return c.z * mix(vec3<f32>(1.0), clamp(p - 1.0, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}
fn apply_paint(base: vec3<f32>, paint: vec2<f32>) -> vec3<f32> {
    if (paint.x < 0.5) {
        return base;
    }
    let h = paint_rgb2hsv(base);
    let w = smoothstep(0.22, 0.42, h.y) * smoothstep(0.05, 0.16, h.z);
    var painted = vec3<f32>(h.z * 0.2);
    if (paint.x < 1.5) {
        painted = paint_hsv2rgb(vec3<f32>(paint.y, max(h.y, 0.55), h.z));
    } else if (paint.x < 2.5) {
        painted = vec3<f32>(min(1.0, h.z * 1.1 + 0.18));
    }
    return mix(base, painted, w);
}

@fragment
fn fs(v: Varyings) -> @location(0) vec4<f32> {
    let t = textureSample(image, image_sampler, v.tex);
    if (flags.x > .5 && t.a < .15) {
        discard;
    }
    var base = t.rgb;
    if (flags.y > .5) {
        base = apply_paint(base, flags.zw);
    }
    var c = base * v.light;
    let l = dot(c, vec3(.299, .587, .114));
    // Studio grade: richer colour and a little contrast (ModelView).
    c = clamp(mix(vec3(l), c, 1.18), vec3(0.), vec3(1.));
    c = clamp((c - .5) * 1.1 + .47, vec3(0.), vec3(1.));
    return vec4(c, 1.);
}
