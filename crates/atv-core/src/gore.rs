//! Optional Gorn blood presentation driven by the ragdoll. Bounded pools of
//! droplets, ground splats and stump caps; deterministic local noise only. It
//! never touches gameplay RNG, contacts or state. Units are feet/seconds.
use crate::ragdoll::{Kind, Ragdoll, V};
use serde::Serialize;
const GRAVITY: f32 = 32.174;
pub const MAX_DROPS: usize = 120;
pub const MAX_SPLATS: usize = 64;
const MAX_REQUERIES: usize = 32;

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V, s: f32) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length(a: V) -> f32 {
    dot(a, a).sqrt()
}
fn unit_or(a: V, fallback: V) -> V {
    let l = length(a);
    if l > 1e-6 && l.is_finite() {
        scale(a, 1. / l)
    } else {
        fallback
    }
}
/// Two decimals keep the serialized frame compact without visible error.
fn quantize(v: f32) -> f32 {
    (v * 100.).round() / 100.
}

#[derive(Clone, Copy)]
struct Ground {
    origin: V,
    normal: V,
}
impl Ground {
    fn sample(floor: &impl Fn(V) -> Option<(f32, V)>, p: V) -> Option<Self> {
        let (h, n) = floor(p)?;
        let n = unit_or(if n[1] < 0. { scale(n, -1.) } else { n }, [0., 1., 0.]);
        (h.is_finite() && n[1] > 0.05).then_some(Self {
            origin: [p[0], h, p[2]],
            normal: n,
        })
    }
    fn distance(&self, p: V) -> f32 {
        dot(sub(p, self.origin), self.normal)
    }
}

struct Drop {
    p: V,
    v: V,
    radius: f32,
    ground: Option<Ground>,
}

struct Splat {
    p: V,
    normal: V,
    /// Horizontal elongation direction (x, z).
    direction: [f32; 2],
    width: f32,
    length: f32,
    pool: bool,
}

#[derive(PartialEq)]
enum Flow {
    /// Heart-driven pulses from the body side of a torn joint.
    Arterial,
    /// Short dribble from the torn limb itself.
    Dribble,
}

struct Emitter {
    body: usize,
    local: V,
    outward: V,
    radius: f32,
    flow: Flow,
    age: f32,
    carry: f32,
    pool: Option<usize>,
    trail: Option<V>,
}

#[derive(Serialize, Default)]
pub struct BloodFrame {
    /// x, y, z, vx, vy, vz, radius per droplet in flight.
    pub drops: Vec<f32>,
    /// x, y, z, nx, ny, nz, dx, dz, width, length per ground splat.
    pub splats: Vec<f32>,
    /// x, y, z, nx, ny, nz, radius per exposed stump face.
    pub stumps: Vec<f32>,
}
impl BloodFrame {
    pub fn count(&self) -> usize {
        self.drops.len() / 7 + self.splats.len() / 10
    }
}

#[derive(Default)]
pub struct Gore {
    drops: Vec<Drop>,
    splats: Vec<Splat>,
    emitters: Vec<Emitter>,
    cooldown: Vec<f32>,
    seed: u32,
    oldest_splat: usize,
}

impl Gore {
    pub fn clear(&mut self) {
        self.drops.clear();
        self.splats.clear();
        self.emitters.clear();
        self.cooldown.clear();
        self.oldest_splat = 0;
        self.seed = 0x9e37_79b9;
    }
    pub fn is_empty(&self) -> bool {
        self.drops.is_empty() && self.splats.is_empty() && self.emitters.is_empty()
    }
    fn random(&mut self) -> f32 {
        if self.seed == 0 {
            self.seed = 0x9e37_79b9;
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / 16_777_216.
    }
    fn signed(&mut self) -> f32 {
        self.random() * 2. - 1.
    }
    /// Direction inside a cone of `spread` radians around unit `axis`.
    fn cone(&mut self, axis: V, spread: f32) -> V {
        let side = unit_or(cross(axis, [0.31, 0.92, 0.23]), [1., 0., 0.]);
        let up = cross(axis, side);
        let (a, b) = (self.signed() * spread, self.signed() * spread);
        unit_or(add(axis, add(scale(side, a), scale(up, b))), axis)
    }
    fn spawn(&mut self, p: V, v: V, radius: f32, floor: &impl Fn(V) -> Option<(f32, V)>) {
        if self.drops.len() >= MAX_DROPS || !p.iter().chain(&v).all(|x| x.is_finite()) {
            return;
        }
        self.drops.push(Drop {
            p,
            v,
            radius,
            ground: Ground::sample(floor, p),
        });
    }
    fn splat(&mut self, p: V, normal: V, travel: V, width: f32, long: f32) {
        let direction = {
            let d = [travel[0], travel[2]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
            if l > 1e-3 {
                [d[0] / l, d[1] / l]
            } else {
                [1., 0.]
            }
        };
        // Nearby splats merge into larger stains instead of stacking quads.
        if let Some(s) = self
            .splats
            .iter_mut()
            .find(|s| !s.pool && length(sub(s.p, p)) < 0.45 * (s.width + width))
        {
            s.width = (s.width * s.width + 0.5 * width * width).sqrt().min(1.4);
            s.length = s.length.max(s.width).min(2.4);
            return;
        }
        let splat = Splat {
            p,
            normal,
            direction,
            width,
            length: long.max(width),
            pool: false,
        };
        if self.splats.len() < MAX_SPLATS {
            self.splats.push(splat);
            return;
        }
        // Recycle the oldest ordinary stain; pools persist.
        for _ in 0..MAX_SPLATS {
            let i = self.oldest_splat % MAX_SPLATS;
            self.oldest_splat += 1;
            if !self.splats[i].pool {
                self.splats[i] = splat;
                return;
            }
        }
    }

    /// A joint tore: spray from the open wound and start bleeding from both
    /// the body-side stump and the severed limb.
    pub fn sever(
        &mut self,
        ragdoll: &Ragdoll,
        joint: usize,
        floor: &impl Fn(V) -> Option<(f32, V)>,
    ) {
        let j = &ragdoll.joints[joint];
        let bone = ragdoll.joint_bone(joint);
        let radius = ragdoll.radius(j.child) * 0.9;
        let origin = ragdoll.point(j.parent, j.anchor);
        let outward = ragdoll.direction(j.parent, bone);
        let base = ragdoll.velocity_at(j.parent, origin);
        for _ in 0..22 {
            let d = self.cone(outward, 0.65);
            let speed = 5. + self.random() * 11.;
            let r = 0.04 + self.random() * 0.04;
            self.spawn(origin, add(base, scale(d, speed)), r, floor);
        }
        let limb = ragdoll.point(j.child, [0.; 3]);
        let limb_velocity = ragdoll.velocity_at(j.child, limb);
        for _ in 0..8 {
            let d = self.cone(scale(outward, -1.), 0.8);
            let speed = 2. + self.random() * 5.;
            self.spawn(limb, add(limb_velocity, scale(d, speed)), 0.04, floor);
        }
        self.emitters.push(Emitter {
            body: j.parent,
            local: j.anchor,
            outward: bone,
            radius,
            flow: Flow::Arterial,
            age: 0.,
            carry: 0.,
            pool: None,
            trail: None,
        });
        self.emitters.push(Emitter {
            body: j.child,
            local: [0.; 3],
            outward: scale(bone, -1.),
            radius,
            flow: Flow::Dribble,
            age: 0.,
            carry: 0.,
            pool: None,
            trail: None,
        });
    }

    pub fn update(
        &mut self,
        dt: f32,
        ragdoll: &Ragdoll,
        wounds: bool,
        floor: &impl Fn(V) -> Option<(f32, V)>,
    ) {
        if dt.is_nan() || dt <= 0. {
            return;
        }
        if self.cooldown.len() != ragdoll.body_count() {
            self.cooldown = vec![0.; ragdoll.body_count()];
        }
        for c in &mut self.cooldown {
            *c -= dt;
        }
        // Hard contacts open wounds where the body actually struck.
        if wounds {
            for b in 0..ragdoll.body_count() {
                let (impact, point, normal) = ragdoll.impact(b);
                if impact < 26. || self.cooldown[b] > 0. {
                    continue;
                }
                self.cooldown[b] = 0.3;
                let v = ragdoll.velocity_at(b, point);
                let slide = sub(v, scale(normal, dot(v, normal)));
                let count = ((impact - 20.) / 3.).clamp(4., 14.) as usize;
                for _ in 0..count {
                    let d = self.cone(normal, 0.9);
                    let speed = 3. + self.random() * (impact * 0.12);
                    let r = 0.03 + self.random() * 0.03;
                    self.spawn(
                        add(point, scale(normal, 0.05)),
                        add(scale(slide, 0.35), scale(d, speed)),
                        r,
                        floor,
                    );
                }
                let size = (0.3 + impact * 0.01).min(0.9);
                let stretch = 1. + (length(slide) / 15.).min(1.5);
                self.splat(
                    add(point, scale(normal, 0.02)),
                    normal,
                    slide,
                    size,
                    size * stretch,
                );
            }
        }
        let mut emitters = std::mem::take(&mut self.emitters);
        emitters.retain_mut(|e| {
            e.age += dt;
            let pressure = match e.flow {
                Flow::Arterial => (-e.age / 2.6).exp(),
                Flow::Dribble => 0.45 * (-e.age / 0.8).exp(),
            };
            let p = ragdoll.point(e.body, e.local);
            let outward = ragdoll.direction(e.body, e.outward);
            let base = ragdoll.velocity_at(e.body, p);
            let speed = ragdoll.speed(e.body);
            let clearance = ragdoll.clearance(e.body, p).unwrap_or(f32::INFINITY);
            // Pulses at an adrenaline heart rate; a weak ooze between beats.
            let phase = (e.age / 0.45).fract();
            let pulse = e.flow == Flow::Arterial && phase < 0.22;
            let rate = if pulse {
                95. * pressure
            } else {
                22. * pressure
            };
            e.carry += rate * dt;
            let n = (e.carry as usize).min(6);
            e.carry -= n as f32;
            for _ in 0..n {
                let (spread, jet) = if pulse {
                    (0.22, (6. + 10. * pressure) * (1. - phase * 2.))
                } else {
                    (0.6, 1.5)
                };
                let d = self.cone(outward, spread);
                let r = if pulse { 0.05 } else { 0.035 } + self.random() * 0.025;
                let lateral = scale(self.cone(outward, 1.4), e.radius * 0.6);
                self.spawn(add(p, lateral), add(base, scale(d, jet)), r, floor);
            }
            // A resting stump pools blood beneath it; a sliding one smears.
            if clearance < 0.75 && speed < 2.5 {
                let fresh = match e.pool {
                    Some(i) => i >= self.splats.len() || length(sub(self.splats[i].p, p)) > 1.2,
                    None => true,
                };
                if fresh {
                    e.pool = Ground::sample(floor, p).map(|g| {
                        let point = sub(p, scale(g.normal, g.distance(p) - 0.02));
                        let pool = Splat {
                            p: point,
                            normal: g.normal,
                            direction: [1., 0.],
                            width: 0.25,
                            length: 0.25,
                            pool: true,
                        };
                        if self.splats.len() < MAX_SPLATS {
                            self.splats.push(pool);
                            self.splats.len() - 1
                        } else {
                            let i = self.oldest_splat % MAX_SPLATS;
                            self.oldest_splat += 1;
                            self.splats[i] = pool;
                            i
                        }
                    });
                }
                if let Some(s) = e.pool.and_then(|i| self.splats.get_mut(i)) {
                    let max = if e.flow == Flow::Arterial { 2.1 } else { 0.8 };
                    let area =
                        std::f32::consts::PI * s.width * s.width + 2.6 * pressure.max(0.1) * dt;
                    s.width = (area / std::f32::consts::PI).sqrt().min(max);
                    s.length = s.width * 1.08;
                }
            } else if clearance < 0.5 && speed > 3. {
                let moved = e.trail.is_none_or(|t| length(sub(t, p)) > 0.4);
                if moved {
                    e.trail = Some(p);
                    if let Some(g) = Ground::sample(floor, p) {
                        let point = sub(p, scale(g.normal, g.distance(p) - 0.02));
                        self.splat(point, g.normal, base, 0.18, 0.18 + speed * 0.04);
                    }
                }
            }
            pressure > 0.04 || e.flow == Flow::Arterial && e.age < 8.
        });
        self.emitters = emitters;
        // Droplets: ballistic with light drag, landing as stretched splats.
        let mut requeries = 0;
        let mut landed = Vec::new();
        self.drops.retain_mut(|d| {
            d.v[1] -= GRAVITY * dt;
            d.v = scale(d.v, 1. / (1. + 0.35 * dt));
            d.p = add(d.p, scale(d.v, dt));
            let stale = d
                .ground
                .is_none_or(|g| (d.p[0] - g.origin[0]).abs() + (d.p[2] - g.origin[2]).abs() > 2.5);
            if stale && requeries < MAX_REQUERIES {
                requeries += 1;
                d.ground = Ground::sample(floor, d.p);
            }
            let Some(g) = d.ground else {
                return d.p[1] > -1000.;
            };
            let height = g.distance(d.p);
            if height > d.radius {
                return height < 400.;
            }
            landed.push((
                sub(d.p, scale(g.normal, height - 0.02)),
                g.normal,
                d.v,
                d.radius,
            ));
            false
        });
        for (p, normal, v, r) in landed {
            let vn = dot(v, normal);
            let slide = sub(v, scale(normal, vn));
            let speed = length(v);
            let width = (r * (3.2 + 0.07 * speed)).min(0.7);
            let stretch = 1. + (length(slide) / vn.abs().max(2.)).min(2.5);
            self.splat(p, normal, slide, width, width * stretch);
        }
    }

    pub fn frame(&self, ragdoll: Option<&Ragdoll>) -> BloodFrame {
        let mut frame = BloodFrame::default();
        for d in &self.drops {
            frame
                .drops
                .extend([d.p[0], d.p[1], d.p[2], d.v[0], d.v[1], d.v[2], d.radius].map(quantize));
        }
        for s in &self.splats {
            frame.splats.extend(
                [
                    s.p[0],
                    s.p[1],
                    s.p[2],
                    s.normal[0],
                    s.normal[1],
                    s.normal[2],
                    s.direction[0],
                    s.direction[1],
                    s.width,
                    s.length,
                ]
                .map(quantize),
            );
        }
        if let Some(r) = ragdoll {
            for (index, j) in r.joints.iter().enumerate() {
                if !j.broken {
                    continue;
                }
                let bone = r.joint_bone(index);
                let radius = r.radius(j.child)
                    * if r.kind(j.child) == Kind::Head {
                        0.55
                    } else {
                        0.95
                    };
                for (p, n) in [
                    (r.point(j.parent, j.anchor), r.direction(j.parent, bone)),
                    (
                        r.point(j.child, [0.; 3]),
                        scale(r.direction(j.child, bone), -1.),
                    ),
                ] {
                    frame
                        .stumps
                        .extend([p[0], p[1], p[2], n[0], n[1], n[2], radius].map(quantize));
                }
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ragdoll::tests::{axis_angle, posed, rider};
    fn flat(_: V) -> Option<(f32, V)> {
        Some((0., [0., 1., 0.]))
    }
    fn torn_neck() -> (Ragdoll, usize) {
        let nodes = rider();
        let pose = posed(
            &nodes,
            axis_angle([1., 0., 0.], std::f32::consts::PI),
            [0., 9., 0.],
        );
        let mut r = Ragdoll::new(&nodes, &pose, None, [0., -62., 0.]).unwrap();
        for _ in 0..30 {
            r.step(1. / 60., &flat);
            if let Some(t) = r.tear(1).first() {
                return (r, t.joint);
            }
        }
        panic!("no tear");
    }

    #[test]
    fn severed_neck_pulses_pools_and_stays_bounded() {
        let (mut r, joint) = torn_neck();
        let mut gore = Gore::default();
        gore.clear();
        gore.sever(&r, joint, &flat);
        let first = gore.frame(Some(&r));
        assert_eq!(first.stumps.len(), 14, "body and limb stump faces");
        assert!(first.drops.len() / 7 >= 25);
        let mut max_drops = 0;
        let mut pooled = 0f32;
        for _ in 0..600 {
            r.step(1. / 60., &flat);
            gore.update(1. / 60., &r, true, &flat);
            let f = gore.frame(Some(&r));
            max_drops = max_drops.max(f.drops.len() / 7);
            assert!(f.drops.len() / 7 <= MAX_DROPS && f.splats.len() / 10 <= MAX_SPLATS);
            assert!(
                f.drops
                    .iter()
                    .chain(&f.splats)
                    .chain(&f.stumps)
                    .all(|v| v.is_finite())
            );
            for d in f.drops.chunks(7) {
                assert!(d[1] > -0.05, "droplet sank below ground");
            }
            pooled = gore
                .splats
                .iter()
                .filter(|s| s.pool)
                .map(|s| s.width)
                .fold(pooled, f32::max);
        }
        assert!(max_drops > 10, "arterial pulses keep spraying");
        assert!(pooled > 1., "a resting stump pools blood: {pooled}");
        assert!(
            gore.splats
                .iter()
                .any(|s| !s.pool && s.length > s.width * 1.05),
            "angled hits streak"
        );
        // Arterial flow dies down; nothing is left in flight.
        assert!(gore.emitters.is_empty() && gore.drops.is_empty());
    }

    #[test]
    fn hard_contacts_open_wounds_only_where_the_body_hits() {
        let nodes = rider();
        let pose = posed(&nodes, axis_angle([1., 0., 0.], -1.5), [0., 3., 0.]);
        let mut r = Ragdoll::new(&nodes, &pose, None, [10., -40., 0.]).unwrap();
        let mut gore = Gore::default();
        let mut crazy = Gore::default();
        for _ in 0..60 {
            r.step(1. / 60., &flat);
            gore.update(1. / 60., &r, true, &flat);
            crazy.update(1. / 60., &r, false, &flat);
        }
        assert!(crazy.is_empty(), "wounds are opt-in");
        assert!(!gore.splats.is_empty());
        let x = r.center(0)[0];
        assert!(
            gore.splats
                .iter()
                .all(|s| (s.p[0] - x).abs() < 8. && s.p[1].abs() < 0.1)
        );
    }
}
