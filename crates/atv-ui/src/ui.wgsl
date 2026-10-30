// UI pass: CSS boxes (rounded, gradient, border, outer/inset shadows) as signed
// distance fields, and premultiplied textured quads (text, shapes, images).
// Coordinates are physical pixels, origin top-left.

struct Globals {
    viewport: vec4<f32>,
    // Tilted images' sway: yaw, pitch (radians).
    tilt: vec4<f32>,
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
    // vertical radii of elliptical corners (0: circular)
    @location(12) radii_y: vec4<f32>,
};

struct Varyings {
    @builtin(position) position: vec4<f32>,
    // local position (xy) and quad corner (zw)
    @location(0) local: vec4<f32>,
    @location(1) @interpolate(flat) radii_y: vec4<f32>,
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
    if (i.params.x < 2.5 && i.extra.w != 0.) {
        // skewX about the pivot (boxes only; images use extra.w = 0).
        screen.x += (local.y - i.extra.z) * i.extra.w;
    }
    if (i.params.z != 0.) {
        let pivot = i.extra.yz;
        let c = cos(i.params.z);
        let s = sin(i.params.z);
        let d = local - pivot;
        screen = pivot + vec2(d.x * c - d.y * s, d.x * s + d.y * c);
    }
    var out: Varyings;
    // Tilted images (border = perspective, centre, on): rotate the quad in
    // 3D about its centre and project it; w makes the texture coordinates
    // perspective-correct.
    var w = 1.;
    if (i.params.x > 2.5 && i.border.w > .5) {
        let d = screen - i.border.yz;
        let cy = cos(globals.tilt.x);
        let sy = sin(globals.tilt.x);
        let cx = cos(globals.tilt.y);
        let sx = sin(globals.tilt.y);
        // Yaw about the vertical axis, then pitch about the horizontal.
        let x1 = d.x * cy;
        let z1 = d.x * sy;
        let y2 = d.y * cx - z1 * sx;
        let z2 = d.y * sx + z1 * cx;
        w = max((i.border.x - z2) / i.border.x, .1);
        screen = i.border.yz + vec2(x1, y2) / w;
    }
    let ndc = screen / globals.viewport.xy * 2. - 1.;
    out.position = vec4(ndc.x * w, -ndc.y * w, 0., w);
    out.local = vec4(local, corner);
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
    out.radii_y = i.radii_y;
    return out;
}

// Signed distance to a rounded box (radii tl, tr, br, bl; `ry` gives
// elliptical corners their vertical radii, 0 for circular).
//
// Each corner owns the rectangle its radii span (CSS lets a corner be wider
// than half the box, e.g. `border-radius: 75% / 56px`), not the quadrant it
// sits in. Choosing corners by quadrant cut a wide elliptical corner off at
// the centre line, where the straight edge took over a step lower: the
// portrait stage's white rim broke in two halfway across the screen.
fn rounded(p: vec2<f32>, b: vec4<f32>, r: vec4<f32>, ry: vec4<f32>) -> f32 {
    let lo = b.xy;
    let hi = b.xy + b.zw;
    // Vertical radii: 0 means a circular corner.
    var vy = select(r, ry, ry > vec4(0.));
    var rx = r;
    // CSS overlapping-curves rule: every radius scales by one factor when
    // two corners on a side need more than its length.
    var f = 1.;
    if (rx.x + rx.y > b.z) { f = min(f, b.z / (rx.x + rx.y)); }
    if (rx.w + rx.z > b.z) { f = min(f, b.z / (rx.w + rx.z)); }
    if (vy.x + vy.w > b.w) { f = min(f, b.w / (vy.x + vy.w)); }
    if (vy.y + vy.z > b.w) { f = min(f, b.w / (vy.y + vy.z)); }
    rx = rx * f;
    vy = vy * f;
    var q = vec2(lo.x + rx.x - p.x, lo.y + vy.x - p.y);
    if (q.x > 0. && q.y > 0.) { return corner(q, rx.x, vy.x); }
    q = vec2(p.x - (hi.x - rx.y), lo.y + vy.y - p.y);
    if (q.x > 0. && q.y > 0.) { return corner(q, rx.y, vy.y); }
    q = vec2(p.x - (hi.x - rx.z), p.y - (hi.y - vy.z));
    if (q.x > 0. && q.y > 0.) { return corner(q, rx.z, vy.z); }
    q = vec2(lo.x + rx.w - p.x, p.y - (hi.y - vy.w));
    if (q.x > 0. && q.y > 0.) { return corner(q, rx.w, vy.w); }
    let edge = max(lo - p, p - hi);
    return max(edge.x, edge.y);
}

// Signed distance from a corner's curve; `q` is the offset outward from the
// corner's centre (both components positive inside its rectangle).
fn corner(q: vec2<f32>, rx: f32, ry: f32) -> f32 {
    if (rx <= 0. || ry <= 0.) { return length(q); }
    if (abs(rx - ry) <= .01) { return length(q) - rx; }
    // Ellipse distance estimate f / |grad f| (exact on the curve).
    let e = vec2(rx, ry);
    let k0 = length(q / e);
    let k1 = length(q / (e * e));
    return k0 * (k0 - 1.) / max(k1, 1e-6);
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
    let p = v.local.xy;
    // Clip in screen space: a rounded rectangle or plain rectangle.
    var clip = 1.;
    let c = v.clip;
    if (v.extra.x > 0.) {
        clip = coverage(rounded(v.screen, vec4(c.xy, c.zw - c.xy), vec4(v.extra.x), vec4(0.)));
    } else {
        let e = min(min(v.screen.x - c.x, c.z - v.screen.x), min(v.screen.y - c.y, c.w - v.screen.y));
        clip = clamp(e + .5, 0., 1.);
    }
    if (clip <= 0.) { discard; }
    let kind = v.params.x;
    var out = vec4(0.);
    if (kind < .5) {
        let d = rounded(p, v.shape, v.radii, v.radii_y);
        let outer = coverage(d);
        var fill = v.color;
        if (v.params.w > .5) { fill = gradient(p, v) * v.color.a; }
        let w = v.border;
        let inner_box = vec4(v.shape.x + w.w, v.shape.y + w.x, v.shape.z - w.y - w.w, v.shape.w - w.x - w.z);
        let inner_radii = max(v.radii - vec4(max(w.x, w.w), max(w.x, w.y), max(w.z, w.y), max(w.z, w.w)), vec4(0.));
        var ring = 0.;
        if (dot(w, vec4(1.)) > 0.) {
            let inner_y = max(v.radii_y - vec4(max(w.x, w.w), max(w.x, w.y), max(w.z, w.y), max(w.z, w.w)), vec4(0.));
            ring = max(outer - coverage(rounded(p, inner_box, inner_radii, inner_y)), 0.);
        }
        if (v.params.y > .5) {
            out = fill * ring;
        } else {
            let border = v.border_color * ring;
            out = border + fill * outer * (1. - v.border_color.a * ring);
        }
    } else if (kind < 1.5) {
        let shadow = blurred(rounded(p, v.shape, v.radii, v.radii_y), v.params.y);
        let cut = coverage(rounded(p, v.cut, v.cut_radii, v.radii_y));
        out = v.color * shadow * (1. - cut);
    } else if (kind < 2.5) {
        let element = coverage(rounded(p, v.shape, v.radii, v.radii_y));
        let hole = blurred(rounded(p, v.cut, v.cut_radii, v.radii_y), v.params.y);
        out = v.color * element * (1. - hole);
    } else {
        let uv = mix(v.uv.xy, v.uv.zw, v.local.zw);
        if (v.params.w > .5) {
            // drop-shadow: a 5x5 Gaussian of the image alpha, tinted.
            var a = 0.;
            var total = 0.;
            for (var i = -2; i <= 2; i++) {
                for (var j = -2; j <= 2; j++) {
                    let w = exp(-f32(i * i + j * j) / 2.);
                    let o = vec2(f32(i), f32(j)) * v.cut.xy;
                    a += textureSampleLevel(image, image_sampler, uv + o, 0.).a * w;
                    total += w;
                }
            }
            out = v.color * (a / total);
        } else {
            out = textureSampleLevel(image, image_sampler, uv, 0.) * v.color.a;
        }
    }
    return out * clip;
}
