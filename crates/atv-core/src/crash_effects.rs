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
}
impl Particle {
    fn new(p: V, v: V, radius: f32) -> Self {
        Self {
            p,
            previous: sub(p, scale(v, 1. / 120.)),
            radius,
        }
    }
    fn advance(&mut self, floor: &impl Fn(V) -> Option<f32>) {
        let raw = sub(self.p, self.previous);
        // At 120 Hz, one foot per substep caps presentation speed at 120 ft/s.
        let velocity = scale(raw, 0.995 / dot(raw, raw).sqrt().max(1.));
        self.previous = self.p;
        self.p = add(add(self.p, velocity), [0., -32.174 / 14400., 0.]);
        self.contact(floor, true);
    }
    fn contact(&mut self, floor: &impl Fn(V) -> Option<f32>, bounce: bool) {
        if let Some(h) = floor(self.p) {
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
            }
        }
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
    tick: u64,
    reset: u32,
    was_wreck: bool,
    speed: f32,
    age: f32,
    joints: Vec<Joint>,
    links: Vec<Link>,
    parts: Vec<Part>,
}
#[derive(Serialize)]
pub struct Frame {
    pub age: f32,
    pub detached_parts: usize,
    pub rider_node_matrices: Vec<M>,
    pub quad_node_matrices: Vec<M>,
}
impl Effects {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.clear();
            self.enabled = enabled;
            self.was_wreck = false;
        }
    }
    fn clear(&mut self) {
        self.joints.clear();
        self.links.clear();
        self.parts.clear();
        self.age = 0.;
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
            self.was_wreck = false;
        }
        if tick != self.tick {
            self.tick = tick;
            if !wreck {
                self.speed = dot(velocity, velocity).sqrt();
                self.was_wreck = false;
            }
            if wreck && !self.was_wreck {
                self.was_wreck = true;
                if self.speed.max(dot(velocity, velocity).sqrt()) >= 25. {
                    let severe = self.speed.max(dot(velocity, velocity).sqrt()) >= 50.;
                    for (i, node) in quad_nodes.iter().enumerate() {
                        let name = node.name.to_ascii_lowercase();
                        // Unleashed attaches visible Tire_Front/Tire_Rear meshes
                        // below the two TireFront/TireRear articulation markers.
                        let wheel = name.starts_with("wheel") && !name.starts_with("wheelholder")
                            || matches!(name.as_str(), "tirefront" | "tirerear");
                        let selected = wheel && (severe || name.contains("front"))
                            || severe
                                && (matches!(
                                    name.as_str(),
                                    "handlebars" | "swingarm" | "rearaxle"
                                ));
                        if selected && self.parts.len() < 8 {
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
                                body: Particle::new(
                                    pos(&initial),
                                    v,
                                    if wheel { 0.65 } else { 0.22 },
                                ),
                                initial,
                                axis: unit([1., 0.3 + side, 0.6]),
                                angle: 0.,
                                spin: side * (8. + (i % 3) as f32),
                            });
                        }
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
                    let v = add(
                        scale(velocity, 0.65),
                        [
                            side * (i % 3) as f32 * 2.,
                            8. + (i % 4) as f32 * 1.5,
                            ((i % 5) as f32 - 2.) * 2.,
                        ],
                    );
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
                        // A grandparent brace limits folding, while leaving arms/legs floppy.
                        if let Some(g) = self.joints[p].parent {
                            self.link(i, g, true);
                        }
                    }
                }
            }
            if !self.joints.is_empty() || !self.parts.is_empty() {
                let steps = (dt * 120.).round().clamp(1., 12.) as usize;
                for _ in 0..steps {
                    for joint in &mut self.joints {
                        joint.body.advance(&floor);
                    }
                    for _ in 0..10 {
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
                        for joint in &mut self.joints {
                            joint.body.contact(&floor, false);
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
            detached_parts: self.parts.len(),
            rider_node_matrices,
            quad_node_matrices,
        })
    }
    fn link(&mut self, a: usize, b: usize, angular: bool) {
        let d = sub(self.joints[a].body.p, self.joints[b].body.p);
        let length = dot(d, d).sqrt();
        let (min, max) = if angular {
            let p = self.joints[a].parent.unwrap();
            let one = sub(self.joints[a].body.p, self.joints[p].body.p);
            let two = sub(self.joints[p].body.p, self.joints[b].body.p);
            (
                length * 0.55,
                (dot(one, one).sqrt() + dot(two, two).sqrt()).max(length) * 0.98,
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
