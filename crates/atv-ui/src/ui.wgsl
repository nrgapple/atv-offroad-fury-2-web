// UI pass: CSS boxes (rounded, gradient, border, outer/inset shadows) as signed
// distance fields, and premultiplied textured quads (text, shapes, images).
// Coordinates are physical pixels, origin top-left.

struct Globals {
    viewport: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) shape: vec4<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) border: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) uv: vec4<f32>,
    // kind, blur sigma, angle (gradient direction or rotation), gradient flag
    @location(7) params: vec4<f32>,
    @location(8) cut: vec4<f32>,
    @location(9) cut_radii: vec4<f32>,
    @location(10) clip: vec4<f32>,
    // clip radius, rotation pivot x/y
    @location(11) extra: vec4<f32>,
};

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) corner: vec2<f32>,
    @location(2) @interpolate(flat) shape: vec4<f32>,
    @location(3) @interpolate(flat) radii: vec4<f32>,
    @location(4) @interpolate(flat) color: vec4<f32>,
    @location(5) @interpolate(flat) border: vec4<f32>,
    @location(6) @interpolate(flat) border_color: vec4<f32>,
    @location(7) @interpolate(flat) uv: vec4<f32>,
    @location(8) @interpolate(flat) params: vec4<f32>,
    @location(9) @interpolate(flat) cut: vec4<f32>,
    @location(10) @interpolate(flat) cut_radii: vec4<f32>,
    @location(11) @interpolate(flat) clip: vec4<f32>,
    @location(12) @interpolate(flat) extra: vec4<f32>,
    @location(13) screen: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) index: u32, i: Instance) -> Varyings {
    var corners = array<vec2<f32>, 6>(
        vec2(0., 0.), vec2(1., 0.), vec2(0., 1.),
        vec2(0., 1.), vec2(1., 0.), vec2(1., 1.),
    );
    let corner = corners[index];
    let local = i.rect.xy + corner * i.rect.zw;
    var screen = local;
    if (i.params.z != 0.) {
        let pivot = i.extra.yz;
        let c = cos(i.params.z);
        let s = sin(i.params.z);
        let d = local - pivot;
        screen = pivot + vec2(d.x * c - d.y * s, d.x * s + d.y * c);
    }
    var out: Varyings;
    let ndc = screen / globals.viewport.xy * 2. - 1.;
    out.position = vec4(ndc.x, -ndc.y, 0., 1.);
    out.local = local;
    out.corner = corner;
    out.screen = screen;
    out.shape = i.shape;
    out.radii = i.radii;
    out.color = i.color;
    out.border = i.border;
    out.border_color = i.border_color;
    out.uv = i.uv;
    out.params = i.params;
    out.cut = i.cut;
    out.cut_radii = i.cut_radii;
    out.clip = i.clip;
    out.extra = i.extra;
    return out;
}

// Signed distance to a rounded box (radii tl, tr, br, bl).
fn rounded(p: vec2<f32>, b: vec4<f32>, r: vec4<f32>) -> f32 {
    let half = b.zw * .5;
    let q0 = p - (b.xy + half);
    var radius = r.x;
    if (q0.x > 0. && q0.y < 0.) { radius = r.y; }
    if (q0.x > 0. && q0.y > 0.) { radius = r.z; }
    if (q0.x < 0. && q0.y > 0.) { radius = r.w; }
    radius = min(radius, min(half.x, half.y));
    let q = abs(q0) - half + radius;
    return min(max(q.x, q.y), 0.) + length(max(q, vec2(0.))) - radius;
}

fn coverage(d: f32) -> f32 {
    return clamp(.5 - d, 0., 1.);
}

fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let t = 1. / (1. + .3275911 * a);
    let y = 1. - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - .284496736) * t + .254829592) * t * exp(-a * a);
    return s * y;
}

// Coverage of a Gaussian-blurred shape at signed distance d.
fn blurred(d: f32, sigma: f32) -> f32 {
    if (sigma < .5) { return coverage(d); }
    return .5 - .5 * erf(d / (sigma * 1.41421356));
}

fn gradient(p: vec2<f32>, v: Varyings) -> vec4<f32> {
    let b = v.shape;
    var t = 0.;
    if (v.params.w > 1.5) {
        // radial-gradient(circle at …): farthest-corner radius.
        let c = b.xy + b.zw * v.cut.yz;
        let far = max(abs(c - b.xy), abs(b.xy + b.zw - c));
        if (v.cut.w > .5) {
            // Ellipse through the farthest corner: farthest-side radii x sqrt 2.
            t = clamp(length((p - c) / max(far * 1.41421356, vec2(1e-3))), 0., 1.);
        } else {
            t = clamp(length(p - c) / max(length(far), 1e-3), 0., 1.);
        }
    } else {
        let a = v.cut.x;
        let dir = vec2(sin(a), -cos(a));
        let span = abs(b.z * sin(a)) + abs(b.w * cos(a));
        t = clamp(dot(p - (b.xy + b.zw * .5), dir) / max(span, 1e-3) + .5, 0., 1.);
    }
    let u = mix(v.uv.x, v.uv.z, t);
    return textureSampleLevel(image, image_sampler, vec2(u, (v.uv.y + v.uv.w) * .5), 0.);
}

@fragment
fn fs(v: Varyings) -> @location(0) vec4<f32> {
    let p = v.local;
    // Clip in screen space: a rounded rectangle or plain rectangle.
    var clip = 1.;
    let c = v.clip;
    if (v.extra.x > 0.) {
        clip = coverage(rounded(v.screen, vec4(c.xy, c.zw - c.xy), vec4(v.extra.x)));
    } else {
        let e = min(min(v.screen.x - c.x, c.z - v.screen.x), min(v.screen.y - c.y, c.w - v.screen.y));
        clip = clamp(e + .5, 0., 1.);
    }
    if (clip <= 0.) { discard; }
    let kind = v.params.x;
    var out = vec4(0.);
    if (kind < .5) {
        let d = rounded(p, v.shape, v.radii);
        let outer = coverage(d);
        var fill = v.color;
        if (v.params.w > .5) { fill = gradient(p, v) * v.color.a; }
        let w = v.border;
        let inner_box = vec4(v.shape.x + w.w, v.shape.y + w.x, v.shape.z - w.y - w.w, v.shape.w - w.x - w.z);
        let inner_radii = max(v.radii - vec4(max(w.x, w.w), max(w.x, w.y), max(w.z, w.y), max(w.z, w.w)), vec4(0.));
        var ring = 0.;
        if (dot(w, vec4(1.)) > 0.) {
            ring = max(outer - coverage(rounded(p, inner_box, inner_radii)), 0.);
        }
        if (v.params.y > .5) {
            out = fill * ring;
        } else {
            let border = v.border_color * ring;
            out = border + fill * outer * (1. - v.border_color.a * ring);
        }
    } else if (kind < 1.5) {
        let shadow = blurred(rounded(p, v.shape, v.radii), v.params.y);
        let cut = coverage(rounded(p, v.cut, v.cut_radii));
        out = v.color * shadow * (1. - cut);
    } else if (kind < 2.5) {
        let element = coverage(rounded(p, v.shape, v.radii));
        let hole = blurred(rounded(p, v.cut, v.cut_radii), v.params.y);
        out = v.color * element * (1. - hole);
    } else {
        let uv = mix(v.uv.xy, v.uv.zw, v.corner);
        out = textureSampleLevel(image, image_sampler, uv, 0.) * v.color.a;
    }
    return out * clip;
}
