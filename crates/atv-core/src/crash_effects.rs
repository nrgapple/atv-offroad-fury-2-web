//! Optional authored crash presentation: an articulated rider ragdoll, ATV
//! breakaway pieces and opt-in Gorn dismemberment/blood. Never feeds impulses,
//! RNG or contacts back into original vehicle/rider/recovery simulation.
//! Units are feet/seconds.
use crate::math::Det;
use crate::{
    gore::{BloodFrame, Gore},
    mesh::Node,
    obstacle::{relative_transform, transform_point},
    ragdoll::Ragdoll,
};
use serde::Serialize;
type V = [f32; 3];
type M = [f32; 16];
#[cfg(test)]
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
    let (s, c) = angle.sin_cos_d();
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
struct Particle {
    p: V,
    previous: V,
}
impl Particle {
    fn new(p: V, v: V) -> Self {
        Self {
            p,
            previous: sub(p, scale(v, 1. / 120.)),
        }
    }
    /// 120 Hz Verlet step with a readable terrain bounce for loose pieces.
    fn advance(&mut self, radius: f32, floor: &impl Fn(V) -> Option<f32>) {
        let raw = sub(self.p, self.previous);
        // One foot per substep caps presentation speed at 120 ft/s.
        let velocity = scale(raw, 0.995 / dot(raw, raw).sqrt().max(1.));
        self.previous = self.p;
        self.p = add(add(self.p, velocity), [0., -32.174 / 14400., 0.]);
        if let Some(h) = floor(self.p).map(|h| h + radius)
            && self.p[1] < h
        {
            let v = sub(self.p, self.previous);
            self.p[1] = h;
            self.previous = [
                self.p[0] - v[0] * 0.76,
                h + v[1].min(0.) * 0.36,
                self.p[2] - v[2] * 0.76,
            ];
        }
    }
}
struct Part {
    node: usize,
    body: Particle,
    radius: f32,
    initial: M,
    axis: V,
    angle: f32,
    spin: f32,
}
/// Most torn joints in one crash; escalating thresholds usually stop sooner.
const MAX_TEARS: usize = 4;
#[derive(Default)]
pub struct Effects {
    enabled: bool,
    gorn: bool,
    severed: Vec<usize>,
    gore: Gore,
    tick: u64,
    reset: u32,
    age: f32,
    ragdoll: Option<Ragdoll>,
    unsupported_rider: bool,
    previous_pose: Vec<M>,
    parts: Vec<Part>,
    rider_velocity: Option<V>,
    /// Vehicle velocity on the last riding step, before the crash bled it off.
    riding_velocity: V,
    rider_impact: f32,
}
#[derive(Serialize)]
pub struct Frame {
    pub age: f32,
    pub severed_nodes: Vec<usize>,
    pub blood: BloodFrame,
    pub detached_parts: usize,
    /// Hardest rider contact approach speed (ft/s) so far in this crash.
    pub rider_impact: f32,
    pub rider_node_matrices: Vec<M>,
    pub quad_node_matrices: Vec<M>,
}
fn to_local(world: &M, p: V) -> V {
    let d = sub(p, pos(world));
    [
        dot(d, [world[0], world[1], world[2]]),
        dot(d, [world[4], world[5], world[6]]),
        dot(d, [world[8], world[9], world[10]]),
    ]
}
fn breakaway(name: &str) -> Option<bool> {
    let name = name.to_ascii_lowercase();
    let wheel = name.starts_with("wheel") && !name.starts_with("wheelholder")
        || matches!(name.as_str(), "tirefront" | "tirerear");
    (wheel || matches!(name.as_str(), "handlebars" | "swingarm" | "rearaxle")).then_some(wheel)
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
        self.ragdoll = None;
        self.unsupported_rider = false;
        self.parts.clear();
        self.severed.clear();
        self.gore.clear();
        self.age = 0.;
        self.rider_impact = 0.;
    }
    /// Mechanical pieces nearest the actual contact give way first. Without a
    /// contact point (motorcycles), severity alone selects the classic set.
    fn break_parts(
        &mut self,
        impact_speed: f32,
        impact_point: Option<V>,
        velocity: V,
        world: &M,
        quad: &[M],
        quad_nodes: &[Node],
    ) {
        let severe = impact_speed >= 50.;
        let hit = impact_point
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .map(|p| to_local(world, p));
        let mut candidates: Vec<(usize, bool, f32)> = quad_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                let wheel = breakaway(&n.name)?;
                let distance = hit.map_or(0., |h| {
                    let d = sub(pos(&quad[i]), h);
                    dot(d, d).sqrt()
                });
                Some((i, wheel, distance))
            })
            .collect();
        let selected: Vec<usize> = match hit {
            None => candidates
                .iter()
                .filter(|(i, wheel, _)| {
                    let name = quad_nodes[*i].name.to_ascii_lowercase();
                    *wheel && (severe || name.contains("front")) || severe && !*wheel
                })
                .map(|c| c.0)
                .collect(),
            Some(_) => {
                candidates.sort_by(|a, b| a.2.total_cmp(&b.2));
                if impact_speed >= 80. {
                    candidates.iter().map(|c| c.0).collect()
                } else if severe {
                    let reach = 2.2 + (impact_speed - 50.) * 0.12;
                    candidates
                        .iter()
                        .enumerate()
                        .filter(|(k, c)| *k < 2 || c.2 <= reach)
                        .map(|(_, c)| c.0)
                        .collect()
                } else {
                    let wheels = if impact_speed >= 37. { 2 } else { 1 };
                    candidates
                        .iter()
                        .filter(|c| c.1)
                        .take(wheels)
                        .map(|c| c.0)
                        .collect()
                }
            }
        };
        for i in selected {
            if self.parts.len() >= 8 || self.parts.iter().any(|p| p.node == i) {
                continue;
            }
            let wheel = breakaway(&quad_nodes[i].name).unwrap_or(false);
            let initial = mul(world, &quad[i]);
            let side = if pos(&quad[i])[0] < 0. { -1. } else { 1. };
            // Pieces fly away from the struck point, or outward without one.
            let local_kick = match hit {
                Some(h) => {
                    let away = sub(pos(&quad[i]), h);
                    let l = dot(away, away).sqrt().max(0.3);
                    let push = 8. + impact_speed * 0.08;
                    [
                        away[0] / l * push,
                        9. + (i % 3) as f32 * 2.,
                        away[2] / l * push,
                    ]
                }
                None => [
                    side * 11.,
                    10. + (i % 3) as f32 * 2.,
                    (i % 2) as f32 * 8. - 4.,
                ],
            };
            let kick = sub(transform_point(world, local_kick), pos(world));
            let v = add(scale(velocity, 0.7), kick);
            self.parts.push(Part {
                node: i,
                body: Particle::new(pos(&initial), v),
                radius: if wheel { 0.65 } else { 0.22 },
                initial,
                axis: unit([1., 0.3 + side, 0.6]),
                angle: 0.,
                spin: side * (8. + (i % 3) as f32),
            });
        }
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
        impact_point: Option<V>,
        world: M,
        quad: &[M],
        quad_nodes: &[Node],
        rider_world: M,
        rider: &[M],
        rider_nodes: &[Node],
        floor: impl Fn(V) -> Option<(f32, V)>,
    ) -> Option<Frame> {
        if !self.enabled {
            return None;
        }
        if quad.len() != quad_nodes.len()
            || rider.len() != rider_nodes.len()
            || quad_nodes.len() > 32
            || rider_nodes.len() > 48
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
        if !wreck && velocity.iter().all(|v| v.is_finite()) {
            self.riding_velocity = velocity;
        }
        if tick != self.tick {
            self.tick = tick;
            // Rider separation can happen in flight. Mechanical damage waits for
            // a real ATV contact, measured by inward speed rather than travel speed.
            if wreck && impact_speed >= 25. {
                self.break_parts(
                    impact_speed,
                    impact_point,
                    velocity,
                    &world,
                    quad,
                    quad_nodes,
                );
            }
            if self.ragdoll.is_none() && !rider.is_empty() {
                let pose: Vec<M> = rider.iter().map(|m| mul(&rider_world, m)).collect();
                if detached && !self.unsupported_rider {
                    let previous = (self.previous_pose.len() == pose.len())
                        .then_some((self.previous_pose.as_slice(), dt));
                    // An ejected rider keeps most of the momentum the vehicle
                    // had before the impact stopped it.
                    let native = self.rider_velocity.unwrap_or(velocity);
                    let carried = sub(self.riding_velocity, native);
                    let launch =
                        if dot(self.riding_velocity, self.riding_velocity) > dot(native, native) {
                            add(native, scale(carried, 0.65))
                        } else {
                            native
                        };
                    let speed = dot(launch, launch).sqrt();
                    let launch = scale(launch, 1. / (speed / 90.).max(1.));
                    self.ragdoll = Ragdoll::new(rider_nodes, &pose, previous, launch);
                    self.unsupported_rider = self.ragdoll.is_none();
                } else {
                    self.previous_pose = pose;
                }
            }
            let floor =
                |p: V| floor(p).filter(|(h, n)| h.is_finite() && n.iter().all(|v| v.is_finite()));
            if let Some(ragdoll) = &mut self.ragdoll {
                ragdoll.step(dt, &floor);
                for b in 0..ragdoll.body_count() {
                    self.rider_impact = self.rider_impact.max(ragdoll.impact(b).0);
                }
                if self.gorn {
                    let budget = MAX_TEARS.saturating_sub(self.severed.len());
                    for torn in ragdoll.tear(budget) {
                        self.severed.push(torn.node);
                        self.gore.sever(ragdoll, torn.joint, &floor);
                    }
                    self.gore.update(dt, ragdoll, true, &floor);
                }
            }
            let height = |p: V| floor(p).map(|(h, _)| h);
            for part in &mut self.parts {
                let steps = (dt * 120.).round().clamp(1., 12.) as usize;
                for _ in 0..steps {
                    part.body.advance(part.radius, &height);
                    part.angle += part.spin / 120.;
                    part.spin *= 0.996;
                }
            }
            if self.ragdoll.is_some() || !self.parts.is_empty() {
                self.age += dt;
            }
        }
        if self.ragdoll.is_none() && self.parts.is_empty() {
            return None;
        }
        let rider_node_matrices = self
            .ragdoll
            .as_ref()
            .map_or_else(Vec::new, Ragdoll::matrices);
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
            blood: if self.gorn {
                self.gore.frame(self.ragdoll.as_ref())
            } else {
                BloodFrame::default()
            },
            detached_parts: self.parts.len(),
            rider_impact: self.rider_impact,
            rider_node_matrices,
            quad_node_matrices,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ragdoll::tests::{axis_angle, posed, rider};
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
    fn flat(_: V) -> Option<(f32, V)> {
        Some((0., [0., 1., 0.]))
    }
    const ATV: [&str; 10] = [
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
    /// Rough ATV layout: +Z forward, −X left, wheels at the corners.
    fn atv() -> (Vec<Node>, Vec<M>) {
        let at = [
            [0., 2., 0.],
            [-1.5, 1., 2.],
            [1.5, 1., 2.],
            [-1.5, 1., -2.],
            [1.5, 1., -2.],
            [0., 3.5, 1.5],
            [0., 1.2, -1.2],
            [0., 1., -2.],
            [-1.2, 1., 2.],
            [-0.6, 3.5, 1.6],
        ];
        let nodes: Vec<_> = ATV
            .iter()
            .zip(at)
            .enumerate()
            .map(|(i, (n, p))| node(n, if i == 0 { -1 } else { 0 }, p))
            .collect();
        let q = nodes.iter().map(|n| n.world_matrix).collect();
        (nodes, q)
    }
    #[allow(clippy::too_many_arguments)]
    fn run(
        e: &mut Effects,
        tick: u64,
        wreck: bool,
        detached: bool,
        impact: f32,
        point: Option<V>,
        quad: (&[Node], &[M]),
        rider: (&[Node], &[M]),
    ) -> Option<Frame> {
        e.update(
            tick,
            0,
            1. / 60.,
            wreck,
            detached,
            [40., 0., 0.],
            impact,
            point,
            ID,
            quad.1,
            quad.0,
            ID,
            rider.1,
            rider.0,
            flat,
        )
    }
    fn names(nodes: &[Node], f: &Frame) -> Vec<String> {
        let mut parts: Vec<_> = (0..nodes.len())
            .filter(|&i| f.quad_node_matrices[i] != nodes[i].world_matrix)
            .map(|i| nodes[i].name.clone())
            .collect();
        parts.sort();
        parts
    }

    #[test]
    fn baseline_and_repeated_reads_never_advance_effects() {
        let (nodes, q) = atv();
        let rider = rider();
        let r = posed(&rider, [0., 0., 0., 1.], [0., 2., 0.]);
        let mut e = Effects::default();
        assert!(run(&mut e, 1, true, true, 40., None, (&nodes, &q), (&rider, &r)).is_none());
        e.set_enabled(true);
        assert!(
            run(
                &mut e,
                2,
                false,
                false,
                40.,
                None,
                (&nodes, &q),
                (&rider, &r)
            )
            .is_none()
        );
        let first = run(&mut e, 3, true, true, 40., None, (&nodes, &q), (&rider, &r)).unwrap();
        assert_eq!(first.detached_parts, 2);
        let repeated = run(&mut e, 3, true, true, 40., None, (&nodes, &q), (&rider, &r)).unwrap();
        assert_eq!(first.rider_node_matrices, repeated.rider_node_matrices);
        assert_eq!(first.age, repeated.age);
        for tick in 4..400 {
            let f = run(
                &mut e,
                tick,
                true,
                true,
                0.,
                None,
                (&nodes, &q),
                (&rider, &r),
            )
            .unwrap();
            assert!(
                f.rider_node_matrices
                    .iter()
                    .chain(&f.quad_node_matrices)
                    .flatten()
                    .all(|x| x.is_finite())
            );
            // Crazy mode never bleeds or dismembers.
            assert!(f.severed_nodes.is_empty() && f.blood.count() == 0);
        }
        assert!(
            run(
                &mut e,
                400,
                false,
                false,
                0.,
                None,
                (&nodes, &q),
                (&rider, &r)
            )
            .is_none()
        );
        e.set_enabled(false);
        assert!(e.ragdoll.is_none() && e.parts.is_empty());
    }

    #[test]
    fn severe_wreck_without_contact_point_sheds_the_classic_set() {
        let (nodes, q) = atv();
        let mut e = Effects::default();
        e.set_enabled(true);
        run(&mut e, 1, false, false, 0., None, (&nodes, &q), (&[], &[]));
        let f = run(&mut e, 2, true, false, 60., None, (&nodes, &q), (&[], &[])).unwrap();
        assert_eq!(f.detached_parts, 7);
        assert_eq!(f.quad_node_matrices[0], q[0]);
        assert_eq!(f.quad_node_matrices[8], q[8]);
        assert_eq!(f.quad_node_matrices[9], q[9]);
    }

    #[test]
    fn breakaway_follows_where_the_atv_was_struck() {
        let (nodes, q) = atv();
        let mut e = Effects::default();
        e.set_enabled(true);
        // A moderate hit at the front-left corner loses only that wheel.
        let f = run(
            &mut e,
            1,
            true,
            false,
            30.,
            Some([-1.8, 0.4, 2.3]),
            (&nodes, &q),
            (&[], &[]),
        )
        .unwrap();
        assert_eq!(names(&nodes, &f), ["wheelfrontleft"]);
        // A harder hit on the same corner adds its axle partner, never the rear.
        let f = run(
            &mut e,
            2,
            true,
            false,
            40.,
            Some([-1.8, 0.4, 2.3]),
            (&nodes, &q),
            (&[], &[]),
        )
        .unwrap();
        assert_eq!(names(&nodes, &f), ["wheelfrontleft", "wheelfrontright"]);
        e.set_enabled(false);
        e.set_enabled(true);
        // A severe rear impact tears off the rear assembly, not the bars.
        let f = run(
            &mut e,
            3,
            true,
            false,
            56.,
            Some([0., 0.8, -2.6]),
            (&nodes, &q),
            (&[], &[]),
        )
        .unwrap();
        let parts = names(&nodes, &f);
        assert!(parts.contains(&"rearaxle".into()) && parts.contains(&"swingarm".into()));
        assert!(!parts.contains(&"handlebars".into()) && !parts.contains(&"wheelfrontleft".into()));
        // Pieces fly away from the struck point.
        for p in &e.parts {
            let v = sub(p.body.p, p.body.previous);
            let away = sub(pos(&q[p.node]), [0., 0.8, -2.6]);
            assert!(
                v[2] * away[2] > 0.,
                "{} flew toward the hit",
                nodes[p.node].name
            );
        }
        // An extreme impact anywhere shreds the whole set.
        let f = run(
            &mut e,
            30,
            true,
            false,
            85.,
            Some([0., 0.8, -2.6]),
            (&nodes, &q),
            (&[], &[]),
        )
        .unwrap();
        assert_eq!(f.detached_parts, 7);
    }

    #[test]
    fn aerial_bailout_keeps_atv_intact_until_contact_and_can_escalate() {
        let (nodes, quad) = atv();
        let rider = rider();
        let pose = posed(&rider, [0., 0., 0., 1.], [0., 20., 0.]);
        let mut e = Effects::default();
        e.set_enabled(true);
        for tick in 1..30 {
            let f = run(
                &mut e,
                tick,
                true,
                true,
                0.,
                None,
                (&nodes, &quad),
                (&rider, &pose),
            )
            .unwrap();
            assert_eq!(
                f.detached_parts, 0,
                "high-speed airborne bailout is not an impact"
            );
            assert_eq!(f.quad_node_matrices, quad);
            assert_eq!(f.rider_node_matrices.len(), rider.len());
        }
        let step = |e: &mut Effects, tick, impact| {
            run(
                e,
                tick,
                true,
                true,
                impact,
                None,
                (&nodes, &quad),
                (&rider, &pose),
            )
            .unwrap()
            .detached_parts
        };
        assert_eq!(
            step(&mut e, 30, 24.),
            0,
            "soft contact keeps the ATV intact"
        );
        assert_eq!(
            step(&mut e, 31, 40.),
            2,
            "hard contact can break front wheels later"
        );
        assert_eq!(
            step(&mut e, 32, 40.),
            2,
            "repeated contact must not duplicate pieces"
        );
        assert_eq!(
            step(&mut e, 33, 60.),
            7,
            "a later severe contact adds remaining pieces"
        );
    }

    #[test]
    fn gorn_dismembers_only_where_the_rider_hits_and_clears_on_mode_and_reset() {
        let rider = rider();
        let head = rider.iter().position(|n| n.name == "Head").unwrap();
        // Head-first plunge: the wreck animation hands over a falling rider.
        let pose = posed(
            &rider,
            axis_angle([1., 0., 0.], std::f32::consts::PI),
            [0., 9., 0.],
        );
        let mut e = Effects::default();
        e.set_mode("gorn", true);
        e.set_rider_velocity([0., -62., 0.]);
        let mut frame = None;
        for tick in 1..120 {
            frame = run(
                &mut e,
                tick,
                true,
                true,
                0.,
                None,
                (&[], &[]),
                (&rider, &pose),
            );
        }
        let f = frame.unwrap();
        assert_eq!(
            f.severed_nodes.first(),
            Some(&head),
            "the neck takes a head-first slam"
        );
        assert!(
            f.severed_nodes.len() <= 2,
            "not every limb pops off: {:?}",
            f.severed_nodes
        );
        assert_eq!(f.blood.stumps.len(), 14 * f.severed_nodes.len());
        assert!(f.blood.count() > 0);
        assert!(
            f.blood
                .drops
                .iter()
                .chain(&f.blood.splats)
                .all(|v| v.is_finite())
        );
        // Same crash in Crazy: ragdoll only.
        e.set_mode("crazy", true);
        assert!(e.gore.is_empty() && e.severed.is_empty());
        let mut crazy = None;
        for tick in 120..240 {
            crazy = run(
                &mut e,
                tick,
                true,
                true,
                0.,
                None,
                (&[], &[]),
                (&rider, &pose),
            );
        }
        let f = crazy.unwrap();
        assert!(f.severed_nodes.is_empty() && f.blood.count() == 0 && f.blood.stumps.is_empty());
        e.set_mode("gorn", true);
        run(
            &mut e,
            240,
            true,
            true,
            0.,
            None,
            (&[], &[]),
            (&rider, &pose),
        );
        assert!(
            run(
                &mut e,
                241,
                false,
                false,
                0.,
                None,
                (&[], &[]),
                (&rider, &pose)
            )
            .is_none()
        );
        assert!(e.gore.is_empty() && e.ragdoll.is_none());
        e.set_mode("original", true);
        assert!(!e.enabled());
    }

    #[test]
    fn gorn_moderate_tumble_keeps_every_limb() {
        let rider = rider();
        let pose = posed(&rider, axis_angle([0., 0., 1.], 0.3), [0., 4., 0.]);
        let mut e = Effects::default();
        e.set_mode("gorn", true);
        e.set_rider_velocity([25., -8., 0.]);
        for tick in 1..240 {
            let f = run(
                &mut e,
                tick,
                true,
                true,
                30.,
                None,
                (&[], &[]),
                (&rider, &pose),
            )
            .unwrap();
            assert!(
                f.severed_nodes.is_empty(),
                "moderate crash tore {:?}",
                f.severed_nodes
            );
            for m in &f.rider_node_matrices {
                assert!(m.iter().all(|v| v.is_finite()) && m[13] > -0.5);
            }
        }
    }

    #[test]
    fn unsupported_rider_skeleton_keeps_the_native_pose() {
        let bad = vec![
            node("Pelvis", -1, [0., 3., 0.]),
            node("Head", 5, [0., 4., 0.]),
        ];
        let pose: Vec<_> = bad.iter().map(|n| n.world_matrix).collect();
        let mut e = Effects::default();
        e.set_enabled(true);
        assert!(run(&mut e, 1, true, true, 0., None, (&[], &[]), (&bad, &pose)).is_none());
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
        assert!(run(&mut e, 1, true, false, 60., None, (&nodes, &q), (&[], &[])).is_none());
        e.set_enabled(true);
        let f = run(&mut e, 2, true, false, 60., None, (&nodes, &q), (&[], &[])).unwrap();
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
                None,
                ID,
                &q,
                &nodes,
                ID,
                &[],
                &[],
                flat
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
        assert!(
            to_local(&world, pos(&part))
                .iter()
                .zip(pos(&local))
                .all(|(a, b)| (a - b).abs() < 1e-5)
        );
    }
}
