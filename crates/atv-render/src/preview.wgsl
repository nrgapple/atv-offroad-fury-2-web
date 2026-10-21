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

@fragment
fn fs(v: Varyings) -> @location(0) vec4<f32> {
    let t = textureSample(image, image_sampler, v.tex);
    if (flags.x > .5 && t.a < .15) {
        discard;
    }
    var c = t.rgb * v.light;
    let l = dot(c, vec3(.299, .587, .114));
    // Studio grade: richer colour and a little contrast (ModelView).
    c = clamp(mix(vec3(l), c, 1.18), vec3(0.), vec3(1.));
    c = clamp((c - .5) * 1.1 + .47, vec3(0.), vec3(1.));
    return vec4(c, 1.);
}
