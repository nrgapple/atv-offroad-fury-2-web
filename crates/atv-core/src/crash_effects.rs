//! Optional authored slapstick presentation. Never feeds impulses, RNG or contacts
//! back into original vehicle/rider/recovery simulation. Units are feet/seconds.
use crate::{
    mesh::Node,
    obstacle::{relative_transform, transform_point},
};
use serde::Serialize;
type V = [f32; 3];
type M = [f32; 16];
const ID: M = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
fn add(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn scale(a: V, s: f32) -> V {
    a.map(|x| x * s)
}
fn dot(a: V, b: V) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: V) -> V {
    scale(a, 1. / dot(a, a).sqrt().max(1e-6))
}
fn pos(m: &M) -> V {
    [m[12], m[13], m[14]]
}
fn mul(a: &M, b: &M) -> M {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
fn rotation(axis: V, angle: f32) -> M {
    let [x, y, z] = unit(axis);
    let (s, c) = angle.sin_cos();
    let t = 1. - c;
    [
        c + x * x * t,
        y * x * t + z * s,
        z * x * t - y * s,
        0.,
        x * y * t - z * s,
        c + y * y * t,
        z * y * t + x * s,
        0.,
        x * z * t + y * s,
        y * z * t - x * s,
        c + z * z * t,
        0.,
        0.,
        0.,
        0.,
        1.,
    ]
}
fn align(a: V, b: V) -> M {
    let a = unit(a);
    let b = unit(b);
    let axis = cross(a, b);
    let cosine = dot(a, b).clamp(-1., 1.);
    if dot(axis, axis) < 1e-8 {
        if cosine < 0. {
            rotation(
                if a[0].abs() < 0.8 {
                    cross(a, [1., 0., 0.])
                } else {
                    cross(a, [0., 1., 0.])
                },
                std::f32::consts::PI,
            )
        } else {
            ID
        }
    } else {
        rotation(axis, cosine.acos())
    }
}
struct Particle {
    p: V,
    previous: V,
    radius: f32,
    floor_height: Option<f32>,
    settled: bool,
}
impl Particle {
    fn new(p: V, v: V, radius: f32) -> Self {
        Self {
            p,
            previous: sub(p, scale(v, 1. / 120.)),
            radius,
            floor_height: None,
            settled: false,
        }
    }
    fn integrate(&mut self) {
        let raw = sub(self.p, self.previous);
        // At 120 Hz, one foot per substep caps presentation speed at 120 ft/s.
        let velocity = scale(raw, 0.995 / dot(raw, raw).sqrt().max(1.));
        self.previous = self.p;
        self.p = add(add(self.p, velocity), [0., -32.174 / 14400., 0.]);
    }
    fn advance(&mut self, floor: &impl Fn(V) -> Option<f32>) {
        self.integrate();
        self.contact(floor, true);
    }
    fn contact(&mut self, floor: &impl Fn(V) -> Option<f32>, bounce: bool) -> bool {
        self.contact_height(floor(self.p), bounce)
    }
    fn contact_height(&mut self, height: Option<f32>, bounce: bool) -> bool {
        self.floor_height = height;
        if let Some(h) = height {
            let h = h + self.radius;
            if self.p[1] < h {
                let v = sub(self.p, self.previous);
                self.p[1] = h;
                if bounce {
                    self.previous = [
                        self.p[0] - v[0] * 0.76,
                        h + v[1].min(0.) * 0.36,
                        self.p[2] - v[2] * 0.76,
                    ];
                }
                return true;
            }
        }
        false
    }
}
struct Joint {
    body: Particle,
    initial: M,
    child: Option<usize>,
    parent: Option<usize>,
}
struct Link {
    a: usize,
    b: usize,
    min: f32,
    max: f32,
}
struct Part {
    node: usize,
    body: Particle,
    initial: M,
    axis: V,
    angle: f32,
    spin: f32,
}
#[derive(Default)]
pub struct Effects {
    enabled: bool,
    gorn: bool,
    worst_impact: f32,
    severed: Vec<usize>,
    blood: Vec<Particle>,
    tick: u64,
    reset: u32,
    age: f32,
    joints: Vec<Joint>,
    links: Vec<Link>,
    parts: Vec<Part>,
    rider_velocity: Option<V>,
}
#[derive(Serialize)]
pub struct Blood {
    pub position: V,
    pub size: f32,
    pub landed: bool,
}
#[derive(Serialize)]
pub struct Frame {
    pub age: f32,
    pub severed_nodes: Vec<usize>,
    pub blood: Vec<Blood>,
    pub detached_parts: usize,
    pub rider_node_matrices: Vec<M>,
    pub quad_node_matrices: Vec<M>,
}
impl Effects {
    /// Detached actor momentum is independent of the already stopped vehicle.
    pub fn set_rider_velocity(&mut self, velocity: V) {
        self.rider_velocity = velocity.iter().all(|v| v.is_finite()).then_some(velocity);
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.clear();
            self.enabled = enabled;
        }
    }
    pub fn set_mode(&mut self, mode: &str, legacy: bool) {
        let enabled = if mode.is_empty() {
            legacy
        } else {
            matches!(mode, "crazy" | "gorn")
        };
        let gorn = mode == "gorn";
        if self.gorn != gorn {
            self.clear();
            self.gorn = gorn;
        }
        self.set_enabled(enabled);
    }
    fn clear(&mut self) {
        self.joints.clear();
        self.links.clear();
        self.parts.clear();
        self.severed.clear();
        self.blood.clear();
        self.age = 0.;
        self.worst_impact = 0.;
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        tick: u64,
        reset: u32,
        dt: f32,
        wreck: bool,
        detached: bool,
        velocity: V,
        impact_speed: f32,
        world: M,
        quad: &[M],
        quad_nodes: &[Node],
        rider_world: M,
        rider: &[M],
        rider_nodes: &[Node],
        floor: impl Fn(V) -> Option<f32>,
    ) -> Option<Frame> {
        if !self.enabled {
            return None;
        }
        if quad.len() != quad_nodes.len()
            || rider.len() != rider_nodes.len()
            || quad_nodes.len() > 32
            || rider_nodes.len() > 32
            || rider_nodes
                .iter()
                .enumerate()
                .any(|(i, n)| n.parent >= i as i32)
        {
            self.clear();
            return None;
        }
        if self.reset != reset || !wreck {
            self.clear();
            self.reset = reset;
        }
        if tick != self.tick {
            self.tick = tick;
            if wreck && impact_speed.is_finite() {
                self.worst_impact = self.worst_impact.max(impact_speed);
            }
            // Rider separation can happen in flight. Mechanical damage waits for
            // a real ATV contact, measured by inward speed rather than travel speed.
            if wreck && impact_speed >= 25. {
                let severe = impact_speed >= 50.;
                for (i, node) in quad_nodes.iter().enumerate() {
                    let name = node.name.to_ascii_lowercase();
                    let wheel = name.starts_with("wheel") && !name.starts_with("wheelholder")
                        || matches!(name.as_str(), "tirefront" | "tirerear");
                    let selected = wheel && (severe || name.contains("front"))
                        || severe
                            && (matches!(name.as_str(), "handlebars" | "swingarm" | "rearaxle"));
                    if selected && self.parts.len() < 8 && !self.parts.iter().any(|p| p.node == i) {
                        let initial = mul(&world, &quad[i]);
                        let side = if pos(&quad[i])[0] < 0. { -1. } else { 1. };
                        let kick = transform_point(
                            &world,
                            [
                                side * 11.,
                                10. + (i % 3) as f32 * 2.,
                                (i % 2) as f32 * 8. - 4.,
                            ],
                        );
                        let v = add(scale(velocity, 0.7), sub(kick, pos(&world)));
                        self.parts.push(Part {
                            node: i,
                            body: Particle::new(pos(&initial), v, if wheel { 0.65 } else { 0.22 }),
                            initial,
                            axis: unit([1., 0.3 + side, 0.6]),
                            angle: 0.,
                            spin: side * (8. + (i % 3) as f32),
                        });
                    }
                }
            }
            if detached && self.joints.is_empty() {
                for (i, node) in rider_nodes.iter().enumerate() {
                    let initial = mul(&rider_world, &rider[i]);
                    let side = if pos(&initial)[0] < pos(&rider_world)[0] {
                        -1.
                    } else {
                        1.
                    };
                    let launch = self.rider_velocity.unwrap_or(velocity);
                    let speed = dot(launch, launch).sqrt();
                    // One coherent cartwheel impulse instead of unrelated limp limbs.
                    let axis = unit([rider_world[0], rider_world[1], rider_world[2]]);
                    let center = pos(&mul(&rider_world, &rider[0]));
                    let spin = cross(
                        scale(axis, 4.5 + (speed / 30.).min(3.)),
                        sub(pos(&initial), center),
                    );
                    let v = add(add(scale(launch, 0.9), spin), [side * 1.5, 12., 0.]);
                    self.joints.push(Joint {
                        body: Particle::new(
                            pos(&initial),
                            v,
                            if node.name.starts_with("Hair") {
                                0.025
                            } else if node.name == "Head" {
                                0.38
                            } else if node.name.contains("Torso") || node.name == "Pelvis" {
                                0.24
                            } else {
                                0.16
                            },
                        ),
                        initial,
                        parent: usize::try_from(node.parent).ok(),
                        child: rider_nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| n.parent == i as i32 && !n.name.starts_with("Hair"))
                            .max_by(|(a, _), (b, _)| {
                                let a = sub(pos(&rider[*a]), pos(&rider[i]));
                                let b = sub(pos(&rider[*b]), pos(&rider[i]));
                                dot(a, a).total_cmp(&dot(b, b))
                            })
                            .map(|(i, _)| i),
                    });
                }
                for i in 0..self.joints.len() {
                    if let Some(p) = self.joints[i].parent {
                        self.link(i, p, false);
                        // Firmer grandparent braces preserve readable limbs through tumbling.
                        if let Some(g) = self.joints[p].parent {
                            self.link(i, g, true);
                        }
                    }
                }
            }
            if self.gorn && detached && self.worst_impact >= 50. {
                self.sever(rider_nodes);
            }
            if !self.joints.is_empty() || !self.parts.is_empty() {
                let steps = (dt * 120.).round().clamp(1., 12.) as usize;
                for _ in 0..steps {
                    for joint in &mut self.joints {
                        joint.body.integrate();
                    }
                    let incoming: [V; 32] = std::array::from_fn(|i| {
                        self.joints
                            .get(i)
                            .map_or([0.; 3], |j| sub(j.body.p, j.body.previous))
                    });
                    let mut contacts = [false; 32];
                    // Reuse each joint's floor during solver projections. Requery
                    // at the final corrected X/Z so slope contact stays exact.
                    let heights: [Option<f32>; 32] =
                        std::array::from_fn(|i| self.joints.get(i).and_then(|j| floor(j.body.p)));
                    for iteration in 0..10 {
                        for link in &self.links {
                            let delta = sub(self.joints[link.b].body.p, self.joints[link.a].body.p);
                            let d = dot(delta, delta).sqrt().max(1e-6);
                            let target = d.clamp(link.min, link.max);
                            let correction = scale(delta, 0.5 * (d - target) / d);
                            self.joints[link.a].body.p =
                                add(self.joints[link.a].body.p, correction);
                            self.joints[link.b].body.p =
                                sub(self.joints[link.b].body.p, correction);
                        }
                        for (i, joint) in self.joints.iter_mut().enumerate() {
                            let height = if iteration == 9 {
                                floor(joint.body.p)
                            } else {
                                heights[i]
                            };
                            contacts[i] |= joint.body.contact_height(height, false);
                        }
                    }
                    // Apply restitution AFTER constraints. Earlier contact response
                    // was consumed by subsequent joint/floor projections.
                    for (i, joint) in self.joints.iter_mut().enumerate() {
                        if contacts[i] {
                            let v = incoming[i];
                            let rebound = if v[1] < -0.045 { -v[1] * 0.62 } else { 0. };
                            joint.body.previous =
                                sub(joint.body.p, [v[0] * 0.9, rebound, v[2] * 0.9]);
                        }
                    }
                    // A rider's hard landing can be severe even when the ATV misses it.
                    if self.gorn
                        && contacts
                            .iter()
                            .enumerate()
                            .any(|(i, hit)| *hit && incoming[i][1] < -40. / 120.)
                    {
                        self.sever(rider_nodes);
                    }
                    for droplet in &mut self.blood {
                        if !droplet.settled {
                            droplet.advance(&floor);
                            let motion = sub(droplet.p, droplet.previous);
                            if droplet
                                .floor_height
                                .is_some_and(|h| droplet.p[1] <= h + droplet.radius + 0.01)
                                && dot(motion, motion) < 0.0001
                            {
                                droplet.previous = droplet.p;
                                droplet.settled = true;
                            }
                        }
                    }
                    for part in &mut self.parts {
                        part.body.advance(&floor);
                        part.angle += part.spin / 120.;
                        part.spin *= 0.996;
                    }
                }
                self.age += dt;
            }
        }
        if self.joints.is_empty() && self.parts.is_empty() {
            return None;
        }
        let rider_node_matrices = self
            .joints
            .iter()
            .enumerate()
            .map(|(i, j)| {
                let edge = j.child.map(|c| (i, c)).or_else(|| j.parent.map(|p| (p, i)));
                let mut m = j.initial;
                if let Some((a, b)) = edge {
                    let old = sub(pos(&self.joints[b].initial), pos(&self.joints[a].initial));
                    let new = sub(self.joints[b].body.p, self.joints[a].body.p);
                    m = mul(&align(old, new), &j.initial);
                }
                m[12..15].copy_from_slice(&j.body.p);
                m
            })
            .collect();
        let mut quad_node_matrices = quad.to_vec();
        for part in &self.parts {
            let mut m = mul(&rotation(part.axis, part.angle), &part.initial);
            m[12..15].copy_from_slice(&part.body.p);
            quad_node_matrices[part.node] = relative_transform(&m, &world);
        }
        // The GPU receives world-relative bone matrices, so moving a parent
        // marker alone cannot move its geometry children. Preserve each child's
        // original relative pose beneath its nearest detached ancestor. A rear
        // wheel detached inside a detached swingarm keeps its own body.
        for i in 0..quad_nodes.len() {
            if self.parts.iter().any(|p| p.node == i) {
                continue;
            }
            let mut parent = quad_nodes[i].parent;
            let mut depth = 0;
            while let Ok(index) = usize::try_from(parent) {
                if index >= quad_nodes.len() || depth >= quad_nodes.len() {
                    break;
                }
                if self.parts.iter().any(|p| p.node == index) {
                    quad_node_matrices[i] = mul(
                        &quad_node_matrices[index],
                        &relative_transform(&quad[i], &quad[index]),
                    );
                    break;
                }
                parent = quad_nodes[index].parent;
                depth += 1;
            }
        }
        Some(Frame {
            age: self.age,
            severed_nodes: self.severed.clone(),
            blood: self
                .blood
                .iter()
                .map(|b| Blood {
                    position: b.p,
                    size: b.radius,
                    landed: b
                        .floor_height
                        .is_some_and(|h| b.p[1] <= h + b.radius + 0.01),
                })
                .collect(),
            detached_parts: self.parts.len(),
            rider_node_matrices,
            quad_node_matrices,
        })
    }
    fn sever(&mut self, nodes: &[Node]) {
        if !self.severed.is_empty() || self.joints.is_empty() {
            return;
        }
        self.severed = nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                matches!(
                    n.name.to_ascii_lowercase().as_str(),
                    "head" | "uprarml" | "uprarmr" | "uprlegl" | "uprlegr"
                )
                .then_some(i)
            })
            .collect();
        let component = |mut i: usize| {
            loop {
                if self.severed.contains(&i) {
                    return i + 1;
                }
                if let Some(p) = self.joints[i].parent {
                    i = p;
                } else {
                    return 0;
                }
            }
        };
        self.links.retain(|l| component(l.a) == component(l.b));
        for &i in &self.severed {
            let p = self.joints[i].parent;
            if let Some(p) = p
                && self.joints[p].child == Some(i)
            {
                self.joints[p].child = None;
            }
            self.joints[i].parent = None;
            let origin = self.joints[i].body.p;
            let side = if i % 2 == 0 { -1. } else { 1. };
            self.joints[i].body.previous =
                sub(self.joints[i].body.previous, [side * 0.055, 0.035, 0.025]);
            // Deterministic, bounded spray; no gameplay RNG calls.
            for k in 0..18 {
                let angle = k as f32 * 2.39996 + i as f32;
                let velocity = [
                    angle.cos() * (5. + (k % 5) as f32 * 2.),
                    4. + (k % 7) as f32 * 2.,
                    angle.sin() * 12.,
                ];
                self.blood.push(Particle::new(
                    origin,
                    velocity,
                    0.045 + (k % 3) as f32 * 0.02,
                ));
            }
        }
    }
    fn link(&mut self, a: usize, b: usize, angular: bool) {
        let d = sub(self.joints[a].body.p, self.joints[b].body.p);
        let length = dot(d, d).sqrt();
        let (min, max) = if angular {
            let p = self.joints[a].parent.unwrap();
            let one = sub(self.joints[a].body.p, self.joints[p].body.p);
            let two = sub(self.joints[p].body.p, self.joints[b].body.p);
            (
                length * 0.82,
                ((dot(one, one).sqrt() + dot(two, two).sqrt()) * 0.98)
                    .min(length * 1.18)
                    .max(length),
            )
        } else {
            (length, length)
        };
        self.links.push(Link {
            a,
            b,
            min,
            max: max.max(min),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(name: &str, parent: i32, p: V) -> Node {
        let mut m = ID;
        m[12..15].copy_from_slice(&p);
        Node {
            name: name.into(),
            parent,
            local_matrix: m,
            world_matrix: m,
            bounds: [0.; 7],
        }
    }
    #[test]
    fn solver_bounds_terrain_queries_and_keeps_final_slope_contact_exact() {
        use std::cell::Cell;
        let rider = [
            node("Pelvis", -1, [0., 1., 0.]),
            node("Head", 0, [0., 2., 0.]),
        ];
        let pose: Vec<_> = rider.iter().map(|n| n.world_matrix).collect();
        let mut effects = Effects::default();
        effects.set_mode("crazy", true);
        let queries = Cell::new(0);
        for tick in 1..120 {
            queries.set(0);
            let frame = effects
                .update(
                    tick,
                    0,
                    1. / 60.,
                    true,
                    true,
                    [8., -5., 0.],
                    0.,
                    ID,
                    &[],
                    &[],
                    ID,
                    &pose,
                    &rider,
                    |p| {
                        queries.set(queries.get() + 1);
                        Some(p[0] * 0.2)
                    },
                )
                .unwrap();
            assert_eq!(
                queries.get(),
                8,
                "two terrain queries per joint per 120 Hz substep"
            );
            for m in frame.rider_node_matrices {
                assert!(m[13] >= m[12] * 0.2 + 0.16 - 0.001);
            }
        }
    }
    #[test]
    fn gorn_requires_severe_contact_and_clears_on_mode_recovery_and_reset() {
        let rider = vec![
            node("Pelvis", -1, [0., 20., 0.]),
            node("UprTorso", 0, [0., 21., 0.]),
            node("Head", 1, [0., 22., 0.]),
            node("UprArmL", 1, [-1., 21., 0.]),
            node("LwrArmL", 3, [-2., 21., 0.]),
            node("HandL", 4, [-3., 21., 0.]),
            node("UprArmR", 1, [1., 21., 0.]),
            node("UprLegL", 0, [-0.5, 19., 0.]),
            node("UprLegR", 0, [0.5, 19., 0.]),
        ];
        let pose: Vec<_> = rider.iter().map(|n| n.world_matrix).collect();
        let mut e = Effects::default();
        e.set_mode("gorn", false);
        let step = |e: &mut Effects, tick, reset, wreck, impact| {
            e.update(
                tick,
                reset,
                1. / 60.,
                wreck,
                wreck,
                [30., 0., 0.],
                impact,
                ID,
                &[],
                &[],
                ID,
                &pose,
                &rider,
                |_| None,
            )
        };
        for tick in 1..8 {
            let f = step(&mut e, tick, 0, true, 49.).unwrap();
            assert!(f.severed_nodes.is_empty() && f.blood.is_empty());
        }
        let severe = step(&mut e, 8, 0, true, 65.).unwrap();
        assert_eq!(severe.severed_nodes.len(), 5);
        assert_eq!(severe.blood.len(), 90);
        assert!(!e.links.iter().any(|l| l.a == 3 && (l.b == 1 || l.b == 0)));
        assert!(e.links.iter().any(|l| l.a == 4 && l.b == 3));
        assert_eq!(e.joints[3].parent, None);
        for tick in 9..300 {
            let f = step(&mut e, tick, 0, true, 65.).unwrap();
            assert_eq!(f.blood.len(), 90);
            assert!(
                f.rider_node_matrices
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite())
            );
            assert!(
                f.blood
                    .iter()
                    .all(|b| b.position.iter().all(|v| v.is_finite()))
            );
        }
        e.set_mode("crazy", true);
        assert!(e.blood.is_empty());
        assert!(
            step(&mut e, 300, 0, true, 65.)
                .unwrap()
                .severed_nodes
                .is_empty()
        );
        e.set_mode("gorn", true);
        step(&mut e, 301, 0, true, 65.);
        assert!(!e.blood.is_empty());
        assert!(step(&mut e, 302, 1, false, 0.).is_none());
        assert!(e.blood.is_empty());
        step(&mut e, 303, 1, true, 65.);
        e.set_mode("original", true);
        assert!(!e.enabled() && e.blood.is_empty());
    }
    #[test]
    fn severe_rider_terrain_landing_creates_bounded_blood_without_quad_damage() {
        let rider = vec![
            node("Pelvis", -1, [0., 2., 0.]),
            node("Head", 0, [0., 3., 0.]),
        ];
        let pose: Vec<_> = rider.iter().map(|n| n.world_matrix).collect();
        let mut e = Effects::default();
        e.set_mode("gorn", true);
        e.set_rider_velocity([0., -70., 0.]);
        let mut landed = false;
        for tick in 1..120 {
            let f = e
                .update(
                    tick,
                    0,
                    1. / 60.,
                    true,
                    true,
                    [0.; 3],
                    0.,
                    ID,
                    &[],
                    &[],
                    ID,
                    &pose,
                    &rider,
                    |_| Some(0.),
                )
                .unwrap();
            assert_eq!(f.detached_parts, 0);
            assert!(f.blood.len() <= 18);
            assert!(f.blood.iter().all(|b| b.position[1] >= b.size - 0.001));
            landed |= f.blood.iter().any(|b| b.landed);
        }
        assert_eq!(e.severed.len(), 1);
        assert!(landed);
    }
    #[test]
    fn severe_contact_before_rider_separation_is_retained_only_for_this_crash() {
        let rider = [
            node("Pelvis", -1, [0., 8., 0.]),
            node("Head", 0, [0., 9., 0.]),
        ];
        let pose: Vec<_> = rider.iter().map(|n| n.world_matrix).collect();
        let mut e = Effects::default();
        e.set_mode("gorn", true);
        assert!(
            e.update(
                1,
                0,
                1. / 60.,
                true,
                false,
                [0.; 3],
                65.,
                ID,
                &[],
                &[],
                ID,
                &pose,
                &rider,
                |_| None
            )
            .is_none()
        );
        let f = e
            .update(
                2,
                0,
                1. / 60.,
                true,
                true,
                [0.; 3],
                0.,
                ID,
                &[],
                &[],
                ID,
                &pose,
                &rider,
                |_| None,
            )
            .unwrap();
        assert_eq!(f.severed_nodes.len(), 1);
        assert_eq!(f.blood.len(), 18);
        e.update(
            3,
            1,
            1. / 60.,
            false,
            false,
            [0.; 3],
            0.,
            ID,
            &[],
            &[],
            ID,
            &pose,
            &rider,
            |_| None,
        );
        let soft = e
            .update(
                4,
                1,
                1. / 60.,
                true,
                true,
                [0.; 3],
                10.,
                ID,
                &[],
                &[],
                ID,
                &pose,
                &rider,
                |_| None,
            )
            .unwrap();
        assert!(soft.blood.is_empty() && soft.severed_nodes.is_empty());
    }
    #[test]
    fn rider_retains_detached_momentum_and_rebounds_after_constraint_solving() {
        let rider = vec![
            node("Pelvis", -1, [0., 4., 0.]),
            node("Torso", 0, [0., 5., 0.]),
            node("Head", 1, [0., 6., 0.]),
        ];
        let pose: Vec<_> = rider.iter().map(|n| n.world_matrix).collect();
        let mut effects = Effects::default();
        effects.set_enabled(true);
        // The vehicle has stopped; the independent detached rider is still moving.
        effects.set_rider_velocity([30., -50., 0.]);
        let mut rebounded = false;
        for tick in 1..180 {
            let frame = effects
                .update(
                    tick,
                    0,
                    1. / 60.,
                    true,
                    true,
                    [0.; 3],
                    0.,
                    ID,
                    &[],
                    &[],
                    ID,
                    &pose,
                    &rider,
                    |_| Some(0.),
                )
                .unwrap();
            assert!(
                frame
                    .rider_node_matrices
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite())
            );
            for j in &effects.joints {
                assert!(j.body.p[1] >= j.body.radius - 1e-5);
                if j.body.p[1] < 1. && j.body.p[1] - j.body.previous[1] > 0.08 {
                    rebounded = true;
                }
            }
            if tick == 1 {
                assert!(
                    effects.joints[0].body.p[0] > 0.3,
                    "use detached momentum instead of stopped vehicle"
                );
                let brace = effects.links.iter().find(|l| l.a == 2 && l.b == 0).unwrap();
                assert!((brace.min - 1.64).abs() < 1e-5);
                assert!(brace.max <= 2.36);
            }
        }
        assert!(
            rebounded,
            "joint projections must preserve readable terrain rebound"
        );
    }
    #[test]
    fn baseline_and_repeated_reads_never_advance_effects() {
        let nodes = vec![
            node("root", -1, [0., 3., 0.]),
            node("wheelfrontleft", 0, [-1., 1., 0.]),
            node("wheelfrontright", 0, [1., 1., 0.]),
        ];
        let rider = vec![
            node("pelvis", -1, [0., 5., 0.]),
            node("spine", 0, [0., 6., 0.]),
            node("head", 1, [0., 7., 0.]),
        ];
        let q: Vec<_> = nodes.iter().map(|n| n.world_matrix).collect();
        let r: Vec<_> = rider.iter().map(|n| n.world_matrix).collect();
        let mut effects = Effects::default();
        assert!(
            effects
                .update(
                    1,
                    0,
                    1. / 60.,
                    true,
                    true,
                    [40., 0., 0.],
                    40.,
                    ID,
                    &q,
                    &nodes,
                    ID,
                    &r,
                    &rider,
                    |_| Some(0.)
                )
                .is_none()
        );
        effects.set_enabled(true);
        assert!(
            effects
                .update(
                    2,
                    0,
                    1. / 60.,
                    false,
                    false,
                    [40., 0., 0.],
                    40.,
                    ID,
                    &q,
                    &nodes,
                    ID,
                    &r,
                    &rider,
                    |_| Some(0.)
                )
                .is_none()
        );
        let first = effects
            .update(
                3,
                0,
                1. / 60.,
                true,
                true,
                [40., 0., 0.],
                40.,
                ID,
                &q,
                &nodes,
                ID,
                &r,
                &rider,
                |_| Some(0.),
            )
            .unwrap();
        assert_eq!(first.detached_parts, 2);
        let repeated = effects
            .update(
                3,
                0,
                1. / 60.,
                true,
                true,
                [40., 0., 0.],
                40.,
                ID,
                &q,
                &nodes,
                ID,
                &r,
                &rider,
                |_| Some(0.),
            )
            .unwrap();
        assert_eq!(first.rider_node_matrices, repeated.rider_node_matrices);
        assert_eq!(first.age, repeated.age);
        for tick in 4..500 {
            let f = effects
                .update(
                    tick,
                    0,
                    1. / 60.,
                    true,
                    true,
                    [0.; 3],
                    0.,
                    ID,
                    &q,
                    &nodes,
                    ID,
                    &r,
                    &rider,
                    |_| Some(0.),
                )
                .unwrap();
            assert!(
                f.rider_node_matrices
                    .iter()
                    .chain(&f.quad_node_matrices)
                    .flatten()
                    .all(|x| x.is_finite())
            );
            assert!(
                effects
                    .joints
                    .iter()
                    .all(|j| j.body.p[1] >= j.body.radius - 1e-5)
            );
            assert!(effects.links.iter().all(|l| {
                let d = sub(effects.joints[l.a].body.p, effects.joints[l.b].body.p);
                dot(d, d).sqrt() >= l.min - 0.08 && dot(d, d).sqrt() <= l.max + 0.08
            }));
        }
        assert!(
            effects
                .update(
                    500,
                    1,
                    1. / 60.,
                    false,
                    false,
                    [0.; 3],
                    0.,
                    ID,
                    &q,
                    &nodes,
                    ID,
                    &r,
                    &rider,
                    |_| Some(0.)
                )
                .is_none()
        );
        effects.set_enabled(false);
        assert!(effects.joints.is_empty() && effects.parts.is_empty());
    }
    #[test]
    fn severe_wreck_sheds_all_wheels_and_mechanical_pieces() {
        let names = [
            "body",
            "wheelfrontleft",
            "wheelfrontright",
            "wheelrearleft",
            "wheelrearright",
            "handlebars",
            "swingarm",
            "rearaxle",
            "wheelholderleft",
            "iknullhandL",
        ];
        let nodes: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, n)| node(n, if i == 0 { -1 } else { 0 }, [i as f32, 3., 0.]))
            .collect();
        let q: Vec<_> = nodes.iter().map(|n| n.world_matrix).collect();
        let mut e = Effects::default();
        e.set_enabled(true);
        e.update(
            1,
            0,
            1. / 60.,
            false,
            false,
            [60., 0., 0.],
            0.,
            ID,
            &q,
            &nodes,
            ID,
            &[],
            &[],
            |_| None,
        );
        let f = e
            .update(
                2,
                0,
                1. / 60.,
                true,
                false,
                [10., 0., 0.],
                60.,
                ID,
                &q,
                &nodes,
                ID,
                &[],
                &[],
                |_| None,
            )
            .unwrap();
        assert_eq!(f.detached_parts, 7);
        assert_eq!(f.quad_node_matrices[0], q[0]);
        assert_eq!(f.quad_node_matrices[8], q[8]);
        assert_eq!(f.quad_node_matrices[9], q[9]);
        e.set_enabled(false);
        assert!(e.parts.is_empty());
    }
    #[test]
    fn aerial_bailout_keeps_atv_intact_until_contact_and_can_escalate() {
        let names = [
            "body",
            "wheelfrontleft",
            "wheelfrontright",
            "wheelrearleft",
            "wheelrearright",
            "handlebars",
            "swingarm",
            "rearaxle",
        ];
        let nodes: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, name)| node(name, if i == 0 { -1 } else { 0 }, [i as f32, 20., 0.]))
            .collect();
        let quad: Vec<_> = nodes.iter().map(|n| n.world_matrix).collect();
        let rider = [
            node("Pelvis", -1, [0., 22., 0.]),
            node("Torso", 0, [0., 23., 0.]),
        ];
        let pose: Vec<_> = rider.iter().map(|n| n.world_matrix).collect();
        let mut effects = Effects::default();
        effects.set_enabled(true);
        let mut tick = 0;
        let mut step = |impact| {
            tick += 1;
            effects
                .update(
                    tick,
                    0,
                    1. / 60.,
                    true,
                    true,
                    [80., -40., 0.],
                    impact,
                    ID,
                    &quad,
                    &nodes,
                    ID,
                    &pose,
                    &rider,
                    |_| None,
                )
                .unwrap()
        };
        for _ in 0..30 {
            let frame = step(0.);
            assert_eq!(
                frame.detached_parts, 0,
                "high-speed airborne bailout is not an impact"
            );
            assert_eq!(frame.quad_node_matrices, quad);
            assert_eq!(frame.rider_node_matrices.len(), 2);
        }
        assert_eq!(
            step(24.).detached_parts,
            0,
            "soft contact keeps the ATV intact"
        );
        assert_eq!(
            step(40.).detached_parts,
            2,
            "hard contact can break front wheels later"
        );
        assert_eq!(
            step(40.).detached_parts,
            2,
            "repeated contact must not duplicate pieces"
        );
        assert_eq!(
            step(60.).detached_parts,
            7,
            "a later severe contact adds remaining pieces"
        );
        assert_eq!(step(0.).detached_parts, 7);
    }
    #[test]
    fn motorcycle_markers_shed_once_and_geometry_follows_nearest_piece() {
        let nodes = vec![
            node("Bike", -1, [0., 3., 0.]),
            node("Handlebars", 0, [0., 5., 2.]),
            node("SwingArm", 0, [0., 2., -1.]),
            node("TireFront", 1, [0., 1., 3.]),
            node("Tire_Front", 3, [0.1, 1., 3.]),
            node("TireRear", 2, [0., 1., -3.]),
            node("Tire_Rear", 5, [0.1, 1., -3.]),
            node("Front_Fender", 1, [0., 4., 3.]),
            node("Frame", 0, [0., 3., 0.]),
        ];
        let q: Vec<_> = nodes.iter().map(|n| n.world_matrix).collect();
        let mut e = Effects::default();
        assert!(
            e.update(
                1,
                0,
                1. / 60.,
                true,
                false,
                [60., 0., 0.],
                60.,
                ID,
                &q,
                &nodes,
                ID,
                &[],
                &[],
                |_| None
            )
            .is_none()
        );
        e.set_enabled(true);
        let f = e
            .update(
                2,
                0,
                1. / 60.,
                true,
                false,
                [60., 0., 0.],
                60.,
                ID,
                &q,
                &nodes,
                ID,
                &[],
                &[],
                |_| None,
            )
            .unwrap();
        assert_eq!(f.detached_parts, 4);
        for (child, parent) in [(4, 3), (6, 5), (7, 1)] {
            let expected = mul(
                &f.quad_node_matrices[parent],
                &relative_transform(&q[child], &q[parent]),
            );
            assert_eq!(f.quad_node_matrices[child], expected);
            assert_ne!(f.quad_node_matrices[child], q[child]);
        }
        assert_eq!(f.quad_node_matrices[8], q[8]);
        assert!(
            e.update(
                3,
                1,
                1. / 60.,
                false,
                false,
                [0.; 3],
                0.,
                ID,
                &q,
                &nodes,
                ID,
                &[],
                &[],
                |_| None
            )
            .is_none()
        );
        assert!(e.parts.is_empty());
    }
    #[test]
    fn part_transform_stays_world_fixed_while_quad_moves() {
        let mut world = rotation([0., 1., 0.], 1.2);
        world[12] = 15.;
        world[13] = 2.;
        let mut part = ID;
        part[12] = -3.;
        part[13] = 5.;
        part[14] = 8.;
        let local = relative_transform(&part, &world);
        let recovered = mul(&world, &local);
        for i in 0..16 {
            assert!((recovered[i] - part[i]).abs() < 1e-5);
        }
    }
}
