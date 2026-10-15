//! Presentation-only articulated rider ragdoll. Never feeds impulses, RNG or
//! contacts back into the recovered vehicle/rider/recovery simulation.
//!
//! Each skeletal segment is a capsule rigid body solved with extended
//! position-based dynamics (Müller et al. 2020, "Detailed Rigid Body Simulation
//! with Extended Position Based Dynamics"): small fixed substeps, one constraint
//! pass, then a velocity pass for restitution, friction and joint damping. Knees
//! and elbows are hinges that only flex the anatomical way; hips, shoulders,
//! neck, spine, wrists and ankles are swing/twist joints with asymmetric
//! elliptical swing cones. Contacts act at the touching point so the part that
//! lands receives the impulse and torque. Joint stress is measured from solver
//! impulses so optional dismemberment can happen where an impact actually
//! loads the body. Units are feet, seconds and pounds.
use crate::mesh::Node;
pub type V = [f32; 3];
type Q = [f32; 4];
type M = [f32; 16];
const GRAVITY: f32 = 32.174;
/// 480 Hz with two joint passes either side of contacts keeps hinge and cone
/// limits within a few degrees through hard tumbles.
pub const SUBSTEP: f32 = 1. / 480.;
const MAX_SUBSTEPS: usize = 24;
const UP: V = [0., 1., 0.];
const FORWARD: V = [0., 0., 1.];

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
fn clamp_length(a: V, max: f32) -> V {
    let l = length(a);
    if l > max { scale(a, max / l) } else { a }
}
/// Component of `a` perpendicular to unit `n`, normalized.
fn perpendicular(a: V, n: V, fallback: V) -> V {
    unit_or(sub(a, scale(n, dot(a, n))), fallback)
}
fn qmul(a: Q, b: Q) -> Q {
    let av = [a[0], a[1], a[2]];
    let bv = [b[0], b[1], b[2]];
    let v = add(add(scale(bv, a[3]), scale(av, b[3])), cross(av, bv));
    [v[0], v[1], v[2], a[3] * b[3] - dot(av, bv)]
}
fn qconj(q: Q) -> Q {
    [-q[0], -q[1], -q[2], q[3]]
}
fn qrot(q: Q, v: V) -> V {
    let u = [q[0], q[1], q[2]];
    let t = scale(cross(u, v), 2.);
    add(add(v, scale(t, q[3])), cross(u, t))
}
fn qnormalize(q: Q) -> Q {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l > 1e-6 && l.is_finite() {
        q.map(|x| x / l)
    } else {
        [0., 0., 0., 1.]
    }
}
/// q += ½·[ω,0]·q·s, the XPBD orientation update for a small rotation vector.
fn qturn(q: Q, w: V, s: f32) -> Q {
    let w = scale(w, 0.5 * s);
    let u = [q[0], q[1], q[2]];
    let v = add(scale(w, q[3]), cross(w, u));
    qnormalize([q[0] + v[0], q[1] + v[1], q[2] + v[2], q[3] - dot(w, u)])
}
fn column(m: &M, c: usize) -> V {
    [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]]
}
fn origin(m: &M) -> V {
    [m[12], m[13], m[14]]
}
/// Orthonormal orientation of a possibly scaled node matrix.
fn qfrom_matrix(m: &M) -> Q {
    let x = unit_or(column(m, 0), [1., 0., 0.]);
    let y = perpendicular(column(m, 1), x, [0., 1., 0.]);
    let z = cross(x, y);
    let (m00, m11, m22) = (x[0], y[1], z[2]);
    let trace = m00 + m11 + m22;
    let q = if trace > 0. {
        let s = (trace + 1.).sqrt() * 2.;
        [
            (y[2] - z[1]) / s,
            (z[0] - x[2]) / s,
            (x[1] - y[0]) / s,
            0.25 * s,
        ]
    } else if m00 > m11 && m00 > m22 {
        let s = (1. + m00 - m11 - m22).sqrt() * 2.;
        [
            0.25 * s,
            (y[0] + x[1]) / s,
            (z[0] + x[2]) / s,
            (y[2] - z[1]) / s,
        ]
    } else if m11 > m22 {
        let s = (1. + m11 - m00 - m22).sqrt() * 2.;
        [
            (y[0] + x[1]) / s,
            0.25 * s,
            (z[1] + y[2]) / s,
            (z[0] - x[2]) / s,
        ]
    } else {
        let s = (1. + m22 - m00 - m11).sqrt() * 2.;
        [
            (z[0] + x[2]) / s,
            (z[1] + y[2]) / s,
            0.25 * s,
            (x[1] - y[0]) / s,
        ]
    };
    qnormalize(q)
}
fn wrap(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    a - tau * ((a + std::f32::consts::PI) / tau).floor()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Pelvis,
    LowerTorso,
    UpperTorso,
    Head,
    UpperArm,
    LowerArm,
    Hand,
    UpperLeg,
    LowerLeg,
    Foot,
    Other,
}
impl Kind {
    fn of(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "pelvis" => Kind::Pelvis,
            "lwrtorso" => Kind::LowerTorso,
            "uprtorso" | "torso" => Kind::UpperTorso,
            "head" => Kind::Head,
            "uprarml" | "uprarmr" => Kind::UpperArm,
            "lwrarml" | "lwrarmr" => Kind::LowerArm,
            "handl" | "handr" => Kind::Hand,
            "uprlegl" | "uprlegr" => Kind::UpperLeg,
            "lwrlegl" | "lwrlegr" => Kind::LowerLeg,
            "footl" | "footr" => Kind::Foot,
            _ => return None,
        })
    }
    /// Share of a 170 lb rider (Dempster-style segment fractions).
    fn mass(self) -> f32 {
        170. * match self {
            Kind::Pelvis => 0.14,
            Kind::LowerTorso => 0.12,
            Kind::UpperTorso => 0.2,
            Kind::Head => 0.08,
            Kind::UpperArm => 0.028,
            Kind::LowerArm => 0.017,
            Kind::Hand => 0.007,
            Kind::UpperLeg => 0.1,
            Kind::LowerLeg => 0.047,
            Kind::Foot => 0.015,
            Kind::Other => 0.03,
        }
    }
    fn primary(self) -> Option<Kind> {
        Some(match self {
            Kind::Pelvis => Kind::LowerTorso,
            Kind::LowerTorso => Kind::UpperTorso,
            Kind::UpperTorso => Kind::Head,
            Kind::UpperArm => Kind::LowerArm,
            Kind::LowerArm => Kind::Hand,
            Kind::UpperLeg => Kind::LowerLeg,
            Kind::LowerLeg => Kind::Foot,
            _ => return None,
        })
    }
    /// Whether a torn joint can separate this segment from its parent.
    fn tearable(self) -> bool {
        matches!(
            self,
            Kind::Head | Kind::UpperArm | Kind::LowerArm | Kind::UpperLeg | Kind::LowerLeg
        )
    }
    /// Contact approach speed (ft/s) at which a strike on this segment tears
    /// its nearest tearable joint.
    fn tear_speed(self) -> f32 {
        match self {
            Kind::Head => 42.,
            Kind::Hand | Kind::Foot => 40.,
            Kind::LowerArm | Kind::LowerLeg => 37.,
            Kind::UpperArm => 40.,
            Kind::UpperLeg => 46.,
            _ => f32::INFINITY,
        }
    }
}

#[derive(Clone, Copy)]
struct Plane {
    origin: V,
    normal: V,
}

struct Body {
    node: usize,
    kind: Kind,
    parent: Option<usize>,
    x: V,
    q: Q,
    v: V,
    w: V,
    previous_x: V,
    previous_q: Q,
    start_v: V,
    start_w: V,
    inverse_mass: f32,
    mass: f32,
    inverse_perpendicular: f32,
    inverse_axial: f32,
    axis: V,
    com: V,
    segment: [V; 2],
    radius: f32,
    /// Bounding-sphere radius about the center of mass.
    reach: f32,
    scale: V,
    planes: [Option<Plane>; 2],
    impact: f32,
    impact_point: V,
    impact_normal: V,
    /// Substep (within the last step) at which `impact` occurred.
    impact_order: u32,
}
impl Body {
    fn inverse_inertia(&self, v: V) -> V {
        let local = qrot(qconj(self.q), v);
        let along = dot(local, self.axis) * (self.inverse_axial - self.inverse_perpendicular);
        qrot(
            self.q,
            add(
                scale(local, self.inverse_perpendicular),
                scale(self.axis, along),
            ),
        )
    }
    fn world(&self, local: V) -> V {
        add(self.x, qrot(self.q, sub(local, self.com)))
    }
    fn generalized_inverse_mass(&self, r: V, n: V) -> f32 {
        let rn = cross(r, n);
        self.inverse_mass + dot(rn, self.inverse_inertia(rn))
    }
    fn matrix(&self) -> M {
        let o = sub(self.x, qrot(self.q, self.com));
        let x = scale(qrot(self.q, [1., 0., 0.]), self.scale[0]);
        let y = scale(qrot(self.q, [0., 1., 0.]), self.scale[1]);
        let z = scale(qrot(self.q, [0., 0., 1.]), self.scale[2]);
        [
            x[0], x[1], x[2], 0., y[0], y[1], y[2], 0., z[0], z[1], z[2], 0., o[0], o[1], o[2], 1.,
        ]
    }
}

enum Limit {
    /// Quadrant-elliptical swing cone around `center` (+e1, −e1, +e2, −e2) and
    /// twist about the child's rest bone axis.
    Ball {
        center: V,
        e1: V,
        e2: V,
        swing: [f32; 4],
        bone: V,
        twist: [f32; 2],
    },
    /// One flexion axis; `bone` is the child's rest direction.
    Hinge { axis: V, bone: V, range: [f32; 2] },
}

pub struct Joint {
    pub parent: usize,
    pub child: usize,
    pub anchor: V,
    limit: Limit,
    tearable: bool,
    pub broken: bool,
}

struct Contact {
    body: usize,
    r: V,
    normal: V,
    lambda: f32,
    approach: f32,
}

pub struct Ragdoll {
    bodies: Vec<Body>,
    pub joints: Vec<Joint>,
    /// Per node: owning body and the node pose relative to that body's node.
    attachments: Vec<(usize, M)>,
    pairs: Vec<(usize, usize)>,
    contacts: Vec<Contact>,
    calm: f32,
    pub sleeping: bool,
    torn: usize,
    substeps: u32,
    pub age: f32,
}

pub struct Break {
    pub joint: usize,
    pub node: usize,
    pub load: f32,
}

fn mat_mul(a: &M, b: &M) -> M {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
/// Rigid inverse(to)·from with orthonormalized `to` axes; tolerates scaled poses.
fn relative(from: &M, to: &M) -> M {
    let q = qfrom_matrix(to);
    let inv = qconj(q);
    let o = origin(to);
    let mut out = [0.; 16];
    let s = [
        length(column(to, 0)).max(1e-6),
        length(column(to, 1)).max(1e-6),
        length(column(to, 2)).max(1e-6),
    ];
    for c in 0..3 {
        let v = qrot(inv, column(from, c));
        out[c * 4..c * 4 + 3].copy_from_slice(&[v[0] / s[0], v[1] / s[1], v[2] / s[2]]);
    }
    let p = qrot(inv, sub(origin(from), o));
    out[12..15].copy_from_slice(&[p[0] / s[0], p[1] / s[1], p[2] / s[2]]);
    out[15] = 1.;
    out
}
fn closest_segments(p1: V, q1: V, p2: V, q2: V) -> (V, V) {
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let (s, t) = if a <= 1e-8 && e <= 1e-8 {
        (0., 0.)
    } else if a <= 1e-8 {
        (0., (f / e).clamp(0., 1.))
    } else {
        let c = dot(d1, r);
        if e <= 1e-8 {
            ((-c / a).clamp(0., 1.), 0.)
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s = if denom > 1e-8 {
                ((b * f - c * e) / denom).clamp(0., 1.)
            } else {
                0.
            };
            let mut t = (b * s + f) / e;
            if t < 0. {
                t = 0.;
                s = (-c / a).clamp(0., 1.);
            } else if t > 1. {
                t = 1.;
                s = ((b - c) / a).clamp(0., 1.);
            }
            (s, t)
        }
    };
    (add(p1, scale(d1, s)), add(p2, scale(d2, t)))
}

impl Ragdoll {
    /// Builds bodies from a decoded rider skeleton. `pose` holds current world
    /// node matrices; `previous` (one presentation step earlier) supplies each
    /// segment's animated velocity so the hand-off from animation is seamless.
    pub fn new(
        nodes: &[Node],
        pose: &[M],
        previous: Option<(&[M], f32)>,
        launch: V,
    ) -> Option<Self> {
        if nodes.is_empty()
            || nodes.len() != pose.len()
            || nodes.len() > 48
            || nodes
                .iter()
                .enumerate()
                .any(|(i, n)| n.parent >= i as i32 || (i > 0 && n.parent < 0))
            || pose.iter().flatten().any(|v| !v.is_finite())
        {
            return None;
        }
        let kinds: Vec<Option<Kind>> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| Kind::of(&n.name).or((i == 0).then_some(Kind::Other)))
            .collect();
        let bind = |i: usize| origin(&nodes[i].world_matrix);
        let lateral = |i: usize| {
            let name = nodes[i].name.to_ascii_lowercase();
            if name.ends_with('l') {
                -1.
            } else if name.ends_with('r') {
                1.
            } else {
                bind(i)[0].signum()
            }
        };
        let mut body_of = vec![0usize; nodes.len()];
        let mut bodies: Vec<Body> = Vec::new();
        for (i, kind) in kinds.iter().enumerate() {
            let Some(kind) = *kind else {
                body_of[i] = body_of[nodes[i].parent as usize];
                continue;
            };
            body_of[i] = bodies.len();
            let q = qfrom_matrix(&pose[i]);
            bodies.push(Body {
                node: i,
                kind,
                parent: (i > 0).then(|| body_of[nodes[i].parent as usize]),
                x: [0.; 3],
                q,
                v: [0.; 3],
                w: [0.; 3],
                previous_x: [0.; 3],
                previous_q: q,
                start_v: [0.; 3],
                start_w: [0.; 3],
                inverse_mass: 0.,
                mass: kind.mass(),
                inverse_perpendicular: 0.,
                inverse_axial: 0.,
                axis: UP,
                com: [0.; 3],
                segment: [[0.; 3]; 2],
                radius: 0.2,
                reach: 0.,
                scale: [
                    length(column(&pose[i], 0)),
                    length(column(&pose[i], 1)),
                    length(column(&pose[i], 2)),
                ],
                planes: [None; 2],
                impact: 0.,
                impact_point: [0.; 3],
                impact_normal: UP,
                impact_order: 0,
            });
        }
        // Proportions come from the bind skeleton; 2.1 ft is the ATV rider's
        // pelvis-to-head span. Bind node rotations are identity in all decoded
        // rider models, so bind offsets are already node-local.
        let layout: Vec<(Kind, usize, Option<usize>)> =
            bodies.iter().map(|b| (b.kind, b.node, b.parent)).collect();
        let find = |kind: Kind, side: f32| {
            layout
                .iter()
                .position(|b| b.0 == kind && (side == 0. || lateral(b.1) == side))
        };
        let span = match (find(Kind::Pelvis, 0.), find(Kind::Head, 0.)) {
            (Some(p), Some(h)) => length(sub(bind(layout[h].1), bind(layout[p].1))),
            _ => 2.1,
        };
        let s = (span / 2.1).clamp(0.5, 2.);
        let offset = |from: usize, to: usize| sub(bind(to), bind(from));
        for b in 0..bodies.len() {
            let (kind, node, _) = layout[b];
            let children: Vec<usize> = (0..layout.len())
                .filter(|&c| layout[c].2 == Some(b))
                .collect();
            let primary = kind
                .primary()
                .and_then(|k| children.iter().copied().find(|&c| layout[c].0 == k))
                .or_else(|| {
                    children.iter().copied().max_by(|&a, &c| {
                        length(offset(node, layout[a].1))
                            .total_cmp(&length(offset(node, layout[c].1)))
                    })
                })
                .map(|c| offset(node, layout[c].1));
            let bounds = nodes[node].bounds;
            let center = [bounds[0], bounds[1], bounds[2]];
            let leaf = if length(center) > 0.02 {
                center
            } else {
                primary.unwrap_or(scale(UP, 0.3 * s))
            };
            let pair = |k: Kind, scale_x: f32| {
                let l = find(k, -1.)?;
                let r = find(k, 1.)?;
                Some((
                    scale(offset(node, layout[l].1), scale_x),
                    scale(offset(node, layout[r].1), scale_x),
                ))
            };
            let (segment, radius) = match kind {
                Kind::Pelvis => match pair(Kind::UpperLeg, 0.75) {
                    Some((l, r)) => ([l, r], 0.36 * s),
                    None => ([[0.; 3], scale(UP, 0.3 * s)], 0.36 * s),
                },
                Kind::UpperTorso => {
                    let neck = primary.unwrap_or(scale(UP, 1.1 * s));
                    let mid = [0., neck[1] * 0.42, neck[2] * 0.5];
                    match pair(Kind::UpperArm, 0.5) {
                        Some((l, r)) => {
                            ([[l[0], mid[1], mid[2]], [r[0], mid[1], mid[2]]], 0.46 * s)
                        }
                        None => ([[0.; 3], neck], 0.46 * s),
                    }
                }
                Kind::LowerTorso => ([[0.; 3], primary.unwrap_or(scale(UP, 0.6 * s))], 0.38 * s),
                Kind::Head => {
                    let r = bounds[3].min(bounds[5]).clamp(0.2 * s, 0.5 * s) * 0.95;
                    ([leaf, leaf], r)
                }
                Kind::UpperArm => ([[0.; 3], primary.unwrap_or(leaf)], 0.19 * s),
                Kind::LowerArm => ([[0.; 3], primary.unwrap_or(leaf)], 0.15 * s),
                Kind::Hand => ([leaf, leaf], 0.15 * s),
                Kind::UpperLeg => ([[0.; 3], primary.unwrap_or(leaf)], 0.26 * s),
                Kind::LowerLeg => ([[0.; 3], primary.unwrap_or(leaf)], 0.2 * s),
                Kind::Foot => ([scale(leaf, 0.3), scale(leaf, 1.45)], 0.14 * s),
                Kind::Other => ([leaf, leaf], 0.2 * s),
            };
            let body = &mut bodies[b];
            body.segment = segment;
            body.radius = radius;
            body.com = scale(add(segment[0], segment[1]), 0.5);
            let len = length(sub(segment[1], segment[0]));
            body.reach = 0.5 * len + radius;
            let m = body.mass;
            let (perpendicular, axial) = if len < 0.05 {
                (0.4 * m * radius * radius, 0.4 * m * radius * radius)
            } else {
                (
                    m * (3. * radius * radius + len * len) / 12.,
                    0.5 * m * radius * radius,
                )
            };
            body.axis = unit_or(sub(segment[1], segment[0]), UP);
            body.inverse_mass = 1. / m;
            body.inverse_perpendicular = 1. / perpendicular;
            body.inverse_axial = 1. / axial;
            body.x = add(origin(&pose[node]), qrot(body.q, body.com));
        }
        // Initial motion: animated segment velocity relative to the actor, on
        // top of the detached rider momentum, plus a forward pitch.
        let total: f32 = bodies.iter().map(|b| b.mass).sum();
        let center = scale(
            bodies
                .iter()
                .fold([0.; 3], |a, b| add(a, scale(b.x, b.mass))),
            1. / total,
        );
        let animated: Vec<(V, V)> = bodies
            .iter()
            .map(|b| {
                let Some((prev, dt)) = previous.filter(|(p, dt)| {
                    p.len() == pose.len() && *dt > 1e-4 && p.iter().flatten().all(|v| v.is_finite())
                }) else {
                    return ([0.; 3], [0.; 3]);
                };
                let q0 = qfrom_matrix(&prev[b.node]);
                let x0 = add(origin(&prev[b.node]), qrot(q0, b.com));
                let mut dq = qmul(b.q, qconj(q0));
                if dq[3] < 0. {
                    dq = dq.map(|x| -x);
                }
                (
                    scale(sub(b.x, x0), 1. / dt),
                    scale([dq[0], dq[1], dq[2]], 2. / dt),
                )
            })
            .collect();
        let mean = scale(
            bodies
                .iter()
                .zip(&animated)
                .fold([0.; 3], |a, (b, (v, _))| add(a, scale(*v, b.mass))),
            1. / total,
        );
        let travel = [launch[0], 0., launch[2]];
        let pitch = scale(
            unit_or(cross(UP, travel), [0.; 3]),
            (length(travel) / 22.).min(2.2) * 0.5,
        );
        for (b, (v, w)) in bodies.iter_mut().zip(&animated) {
            let relative = clamp_length(sub(*v, mean), 12.);
            b.w = add(clamp_length(*w, 10.), pitch);
            b.v = add(add(launch, relative), cross(pitch, sub(b.x, center)));
        }
        let mut joints = Vec::new();
        for c in 0..bodies.len() {
            let Some(p) = bodies[c].parent else { continue };
            let kind = bodies[c].kind;
            let side = lateral(bodies[c].node);
            let outward = [side, 0., 0.];
            let child_node = bodies[c].node;
            // Rest bone direction of the child (node-local == parent-local at bind).
            let bone = unit_or(
                if kind.primary().is_some() || matches!(kind, Kind::Head | Kind::Hand | Kind::Foot)
                {
                    sub(bodies[c].segment[1], [0.; 3])
                } else {
                    bodies[c].com
                },
                UP,
            );
            let bone = if length(bone) < 0.5 { UP } else { bone };
            let ball = |center: V, flex: V, swing: [f32; 4], twist: f32| {
                let center = unit_or(center, bone);
                let e1 = perpendicular(flex, center, perpendicular(FORWARD, center, UP));
                Limit::Ball {
                    center,
                    e1,
                    e2: cross(center, e1),
                    swing,
                    bone,
                    twist: [-twist, twist],
                }
            };
            let hinge = |flex: V, range: [f32; 2]| Limit::Hinge {
                axis: unit_or(cross(bone, flex), [1., 0., 0.]),
                bone,
                range,
            };
            let limit = match kind {
                Kind::LowerTorso => ball(UP, FORWARD, [0.5, 0.3, 0.3, 0.3], 0.35),
                Kind::UpperTorso => ball(UP, FORWARD, [0.45, 0.3, 0.3, 0.3], 0.4),
                Kind::Head => ball(
                    add(UP, scale(FORWARD, 0.15)),
                    FORWARD,
                    [0.8, 0.65, 0.6, 0.6],
                    1.1,
                ),
                Kind::UpperArm => {
                    let center = unit_or(add(scale(outward, 0.75), scale(FORWARD, 0.65)), outward);
                    // e1 raises the arm; the across-body direction is derived per side.
                    let e1 = perpendicular(UP, center, UP);
                    let toward_front = perpendicular(FORWARD, center, FORWARD);
                    let e2 = cross(center, e1);
                    let front = if dot(e2, toward_front) >= 0. {
                        1.15
                    } else {
                        1.25
                    };
                    let back = if front == 1.15 { 1.25 } else { 1.15 };
                    Limit::Ball {
                        center,
                        e1,
                        e2,
                        swing: [1.55, 1.6, front, back],
                        bone,
                        twist: [-1.2, 1.2],
                    }
                }
                Kind::LowerArm => hinge(FORWARD, [-0.05, 2.5]),
                Kind::Hand => ball(bone, FORWARD, [0.9, 0.9, 0.45, 0.45], 0.9),
                Kind::UpperLeg => {
                    let center = unit_or(
                        add(
                            add(scale(UP, -0.75), scale(FORWARD, 0.6)),
                            scale(outward, 0.25),
                        ),
                        bone,
                    );
                    let e1 = perpendicular(FORWARD, center, FORWARD);
                    let e2 = cross(center, e1);
                    let abduct = if dot(e2, outward) >= 0. { 0.8 } else { 0.55 };
                    let adduct = if abduct == 0.8 { 0.55 } else { 0.8 };
                    Limit::Ball {
                        center,
                        e1,
                        e2,
                        swing: [1.35, 0.95, abduct, adduct],
                        bone,
                        twist: [-0.6, 0.6],
                    }
                }
                Kind::LowerLeg => hinge(scale(FORWARD, -1.), [-0.05, 2.55]),
                Kind::Foot => ball(bone, UP, [0.5, 0.5, 0.3, 0.3], 0.3),
                _ => ball(bone, FORWARD, [0.6, 0.6, 0.6, 0.6], 0.4),
            };
            let local = relative(&pose[child_node], &pose[bodies[p].node]);
            joints.push(Joint {
                parent: p,
                child: c,
                anchor: origin(&local),
                limit,
                tearable: kind.tearable(),
                broken: false,
            });
        }
        let attachments = (0..nodes.len())
            .map(|i| {
                let b = body_of[i];
                (b, relative(&pose[i], &pose[bodies[b].node]))
            })
            .collect();
        // Self-collision between segments that are apart in the bind pose.
        let mut pairs = Vec::new();
        for i in 0..bodies.len() {
            for j in i + 1..bodies.len() {
                if bodies[i].parent == Some(j) || bodies[j].parent == Some(i) {
                    continue;
                }
                // A fully flexed elbow/knee folds the hand/foot onto its own
                // upper segment; the hinge limit, not contact, governs that.
                let grand =
                    |a: usize, b: usize| bodies[a].parent.and_then(|p| bodies[p].parent) == Some(b);
                let limb = |a: Kind, b: Kind| {
                    matches!(
                        (a, b),
                        (Kind::Hand, Kind::UpperArm) | (Kind::Foot, Kind::UpperLeg)
                    )
                };
                if (grand(i, j) && limb(bodies[i].kind, bodies[j].kind))
                    || (grand(j, i) && limb(bodies[j].kind, bodies[i].kind))
                {
                    continue;
                }
                let at = |b: &Body, k: usize| add(bind(b.node), b.segment[k]);
                let (a, c) = closest_segments(
                    at(&bodies[i], 0),
                    at(&bodies[i], 1),
                    at(&bodies[j], 0),
                    at(&bodies[j], 1),
                );
                if length(sub(a, c)) > bodies[i].radius + bodies[j].radius + 0.02 {
                    pairs.push((i, j));
                }
            }
        }
        Some(Self {
            bodies,
            joints,
            attachments,
            pairs,
            contacts: Vec::new(),
            calm: 0.,
            sleeping: false,
            torn: 0,
            substeps: 0,
            age: 0.,
        })
    }

    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }
    pub fn body_of_node(&self, node: usize) -> Option<usize> {
        self.attachments.get(node).map(|a| a.0)
    }
    pub fn node_of_body(&self, body: usize) -> usize {
        self.bodies[body].node
    }
    pub fn kind(&self, body: usize) -> Kind {
        self.bodies[body].kind
    }
    pub fn radius(&self, body: usize) -> f32 {
        self.bodies[body].radius
    }
    pub fn center(&self, body: usize) -> V {
        self.bodies[body].x
    }
    /// World position of a point given in the body's node-local frame.
    pub fn point(&self, body: usize, local: V) -> V {
        self.bodies[body].world(local)
    }
    pub fn direction(&self, body: usize, local: V) -> V {
        qrot(self.bodies[body].q, local)
    }
    pub fn velocity_at(&self, body: usize, world: V) -> V {
        let b = &self.bodies[body];
        add(b.v, cross(b.w, sub(world, b.x)))
    }
    pub fn speed(&self, body: usize) -> f32 {
        length(self.bodies[body].v)
    }
    /// Child bone rest direction in the parent's node-local frame.
    pub fn joint_bone(&self, joint: usize) -> V {
        match self.joints[joint].limit {
            Limit::Ball { bone, .. } | Limit::Hinge { bone, .. } => bone,
        }
    }
    /// Clearance below a world point using this frame's terrain planes.
    pub fn clearance(&self, body: usize, world: V) -> Option<f32> {
        self.bodies[body]
            .planes
            .iter()
            .flatten()
            .map(|p| dot(sub(world, p.origin), p.normal))
            .reduce(f32::min)
    }
    /// Strongest contact approach speed per body during the last step.
    pub fn impact(&self, body: usize) -> (f32, V, V) {
        let b = &self.bodies[body];
        (b.impact, b.impact_point, b.impact_normal)
    }

    pub fn matrices(&self) -> Vec<M> {
        let bodies: Vec<M> = self.bodies.iter().map(Body::matrix).collect();
        self.attachments
            .iter()
            .map(|(b, rel)| mat_mul(&bodies[*b], rel))
            .collect()
    }

    pub fn step(&mut self, dt: f32, floor: &impl Fn(V) -> Option<(f32, V)>) {
        for b in &mut self.bodies {
            b.impact = 0.;
        }
        if self.sleeping || dt.is_nan() || dt <= 0. {
            return;
        }
        // One terrain sample per capsule end per presentation step; the
        // substeps reuse each local plane.
        for b in &mut self.bodies {
            for k in 0..2 {
                b.planes[k] = if k == 1 && b.segment[0] == b.segment[1] {
                    None
                } else {
                    let p = b.world(b.segment[k]);
                    floor(p).and_then(|(h, n)| {
                        let n = unit_or(if n[1] < 0. { scale(n, -1.) } else { n }, UP);
                        (h.is_finite() && n[1] > 0.05).then_some(Plane {
                            origin: [p[0], h, p[2]],
                            normal: n,
                        })
                    })
                };
            }
        }
        // Hard tumbles need 480 Hz for stiff limits; a settling body does not.
        let energetic = self
            .bodies
            .iter()
            .any(|b| length(b.v) > 10. || length(b.w) > 6.);
        let h = if energetic { SUBSTEP } else { 2. * SUBSTEP };
        let steps = ((dt / h).round() as usize).clamp(1, MAX_SUBSTEPS);
        for _ in 0..steps {
            self.substep(h);
        }
        self.age += dt;
        let moving = self
            .bodies
            .iter()
            .any(|b| length(b.v) > 0.6 || length(b.w) > 0.8);
        self.calm = if moving { 0. } else { self.calm + dt };
        if self.calm > 0.75 {
            self.sleeping = true;
            for b in &mut self.bodies {
                b.v = [0.; 3];
                b.w = [0.; 3];
            }
        }
    }

    fn substep(&mut self, h: f32) {
        self.substeps += 1;
        for b in &mut self.bodies {
            b.start_v = b.v;
            b.start_w = b.w;
            b.v[1] -= GRAVITY * h;
            b.v = clamp_length(scale(b.v, 1. - 0.03 * h), 120.);
            b.w = clamp_length(b.w, 40.);
            b.previous_x = b.x;
            b.previous_q = b.q;
            b.x = add(b.x, scale(b.v, h));
            b.q = qturn(b.q, b.w, h);
        }
        self.solve_joints(false);
        self.solve_joints(false);
        self.contacts.clear();
        for b in 0..self.bodies.len() {
            for k in 0..2 {
                let Some(plane) = self.bodies[b].planes[k] else {
                    continue;
                };
                let body = &self.bodies[b];
                let p = body.world(body.segment[k]);
                let penetration = body.radius - dot(sub(p, plane.origin), plane.normal);
                if penetration <= 0. {
                    continue;
                }
                let touch = sub(p, scale(plane.normal, body.radius));
                let r = sub(touch, body.x);
                let approach = -dot(add(body.start_v, cross(body.start_w, r)), plane.normal);
                let lambda = self.translate(b, touch, None, scale(plane.normal, penetration));
                self.contacts.push(Contact {
                    body: b,
                    r: sub(touch, self.bodies[b].x),
                    normal: plane.normal,
                    lambda,
                    approach,
                });
            }
        }
        for k in 0..self.pairs.len() {
            let (i, j) = self.pairs[k];
            let (a, b) = (&self.bodies[i], &self.bodies[j]);
            let reach = a.reach + b.reach;
            let gap = sub(a.x, b.x);
            if dot(gap, gap) > reach * reach {
                continue;
            }
            let (pi, pj) = closest_segments(
                a.world(a.segment[0]),
                a.world(a.segment[1]),
                b.world(b.segment[0]),
                b.world(b.segment[1]),
            );
            let delta = sub(pi, pj);
            let d = length(delta);
            let penetration = a.radius + b.radius - d;
            if penetration <= 0. || d < 1e-5 {
                continue;
            }
            let n = scale(delta, 1. / d);
            let (ri, rj) = (a.radius, b.radius);
            self.translate(
                i,
                sub(pi, scale(n, ri)),
                Some((j, add(pj, scale(n, rj)))),
                scale(n, penetration),
            );
        }
        // Contacts and self-collision moved segments; close joints again so
        // limbs never visibly gap or bend past a limit at the shoulder or knee.
        self.solve_joints(true);
        self.solve_joints(true);
        for b in &mut self.bodies {
            b.v = scale(sub(b.x, b.previous_x), 1. / h);
            let dq = qmul(b.q, qconj(b.previous_q));
            let w = scale([dq[0], dq[1], dq[2]], 2. / h);
            b.w = if dq[3] < 0. { scale(w, -1.) } else { w };
        }
        for k in 0..self.contacts.len() {
            let (body, r, n, lambda, approach) = {
                let c = &self.contacts[k];
                (c.body, c.r, c.normal, c.lambda, c.approach)
            };
            let b = &self.bodies[body];
            let v = add(b.v, cross(b.w, r));
            let vn = dot(v, n);
            let vt = sub(v, scale(n, vn));
            let vt_len = length(vt);
            let mut dv = if vt_len > 1e-5 {
                scale(vt, -(0.75 * lambda / h).min(vt_len) / vt_len)
            } else {
                [0.; 3]
            };
            let restitution = if approach > 2. * GRAVITY * h {
                0.18
            } else {
                0.
            };
            dv = add(dv, scale(n, -vn + (restitution * approach).max(0.)));
            self.velocity_impulse(body, r, dv);
            let order = self.substeps;
            let b = &mut self.bodies[body];
            if approach > b.impact {
                b.impact = approach;
                b.impact_order = order;
                b.impact_point = add(b.x, r);
                b.impact_normal = n;
            }
        }
        // Tissue/joint friction keeps limbs from flailing like rope.
        let damping = (5.5 * h).min(1.);
        for j in 0..self.joints.len() {
            if self.joints[j].broken {
                continue;
            }
            let (p, c) = (self.joints[j].parent, self.joints[j].child);
            let relative = sub(self.bodies[c].w, self.bodies[p].w);
            let correction = scale(relative, -damping);
            let magnitude = length(correction);
            if magnitude < 1e-6 {
                continue;
            }
            let n = scale(correction, 1. / magnitude);
            let wc = dot(n, self.bodies[c].inverse_inertia(n));
            let wp = dot(n, self.bodies[p].inverse_inertia(n));
            let impulse = scale(n, magnitude / (wc + wp));
            let dc = self.bodies[c].inverse_inertia(impulse);
            let dp = self.bodies[p].inverse_inertia(impulse);
            self.bodies[c].w = add(self.bodies[c].w, dc);
            self.bodies[p].w = sub(self.bodies[p].w, dp);
        }
    }

    fn solve_joints(&mut self, limits_first: bool) {
        for j in 0..self.joints.len() {
            if self.joints[j].broken {
                continue;
            }
            let (p, c, anchor) = {
                let j = &self.joints[j];
                (j.parent, j.child, j.anchor)
            };
            if limits_first {
                self.solve_limit(j);
            }
            let pa = self.bodies[p].world(anchor);
            let pc = self.bodies[c].world([0.; 3]);
            self.translate(c, pc, Some((p, pa)), sub(pa, pc));
            if !limits_first {
                self.solve_limit(j);
            }
        }
    }

    /// Moves body `a` (point `pa`) by `correction` relative to optional body
    /// `b` (point `pb`); returns the positional multiplier Δλ.
    fn translate(&mut self, a: usize, pa: V, b: Option<(usize, V)>, correction: V) -> f32 {
        let c = length(correction);
        if c < 1e-7 || !c.is_finite() {
            return 0.;
        }
        let n = scale(correction, 1. / c);
        let ra = sub(pa, self.bodies[a].x);
        let wa = self.bodies[a].generalized_inverse_mass(ra, n);
        let (wb, rb) = match b {
            Some((b, pb)) => {
                let rb = sub(pb, self.bodies[b].x);
                (self.bodies[b].generalized_inverse_mass(rb, n), rb)
            }
            None => (0., [0.; 3]),
        };
        let lambda = c / (wa + wb);
        let impulse = scale(n, lambda);
        let body = &mut self.bodies[a];
        body.x = add(body.x, scale(impulse, body.inverse_mass));
        let turn = body.inverse_inertia(cross(ra, impulse));
        body.q = qturn(body.q, turn, 1.);
        if let Some((b, _)) = b {
            let body = &mut self.bodies[b];
            body.x = sub(body.x, scale(impulse, body.inverse_mass));
            let turn = body.inverse_inertia(cross(rb, impulse));
            body.q = qturn(body.q, turn, -1.);
        }
        lambda
    }

    /// Rotates `a` by `correction` (world rotation vector) relative to `b`.
    fn rotate(&mut self, a: usize, b: usize, correction: V) {
        let angle = length(correction);
        if angle < 1e-6 || !angle.is_finite() {
            return;
        }
        let n = scale(correction, 1. / angle);
        let wa = dot(n, self.bodies[a].inverse_inertia(n));
        let wb = dot(n, self.bodies[b].inverse_inertia(n));
        let impulse = scale(n, angle / (wa + wb));
        let ta = self.bodies[a].inverse_inertia(impulse);
        let tb = self.bodies[b].inverse_inertia(impulse);
        self.bodies[a].q = qturn(self.bodies[a].q, ta, 1.);
        self.bodies[b].q = qturn(self.bodies[b].q, tb, -1.);
    }

    fn solve_limit(&mut self, j: usize) {
        let (p, c) = (self.joints[j].parent, self.joints[j].child);
        match self.joints[j].limit {
            Limit::Hinge { axis, bone, range } => {
                let ap = qrot(self.bodies[p].q, axis);
                let ac = qrot(self.bodies[c].q, axis);
                self.rotate(c, p, cross(ac, ap));
                let ap = qrot(self.bodies[p].q, axis);
                let bp = qrot(self.bodies[p].q, bone);
                let bc = qrot(self.bodies[c].q, bone);
                let angle = dot(cross(bp, bc), ap).atan2(dot(bp, bc));
                let target = angle.clamp(range[0], range[1]);
                if target != angle {
                    self.rotate(c, p, scale(ap, target - angle));
                }
                // Re-align: the flexion correction must not leave axial twist.
                let ap = qrot(self.bodies[p].q, axis);
                let ac = qrot(self.bodies[c].q, axis);
                self.rotate(c, p, cross(ac, ap));
            }
            Limit::Ball {
                center,
                e1,
                e2,
                swing,
                bone,
                twist,
            } => {
                let relative = qmul(qconj(self.bodies[p].q), self.bodies[c].q);
                let v = qrot(relative, bone);
                let theta = dot(v, center).clamp(-1., 1.).acos();
                let (x, y) = (dot(v, e1), dot(v, e2));
                let psi = if x.abs() + y.abs() < 1e-6 {
                    0.
                } else {
                    y.atan2(x)
                };
                let (cos, sin) = (psi.cos(), psi.sin());
                let a1 = if cos >= 0. { swing[0] } else { swing[1] };
                let a2 = if sin >= 0. { swing[2] } else { swing[3] };
                let max = 1. / ((cos / a1).powi(2) + (sin / a2).powi(2)).sqrt();
                if theta > max {
                    let radial = add(scale(e1, cos), scale(e2, sin));
                    let target = add(scale(center, max.cos()), scale(radial, max.sin()));
                    let axis = unit_or(cross(v, target), radial);
                    let world = qrot(self.bodies[p].q, scale(axis, theta - max));
                    self.rotate(c, p, world);
                }
                let relative = qmul(qconj(self.bodies[p].q), self.bodies[c].q);
                // Swing-twist decomposition about the child's rest bone axis.
                let angle = wrap(
                    2. * dot([relative[0], relative[1], relative[2]], bone).atan2(relative[3]),
                );
                let target = angle.clamp(twist[0], twist[1]);
                if target != angle {
                    let axis = qrot(self.bodies[c].q, bone);
                    self.rotate(c, p, scale(axis, target - angle));
                }
            }
        }
    }

    fn velocity_impulse(&mut self, body: usize, r: V, dv: V) {
        let magnitude = length(dv);
        if magnitude < 1e-6 {
            return;
        }
        let n = scale(dv, 1. / magnitude);
        let b = &mut self.bodies[body];
        let impulse = scale(n, magnitude / b.generalized_inverse_mass(r, n));
        b.v = add(b.v, scale(impulse, b.inverse_mass));
        b.w = add(b.w, b.inverse_inertia(cross(r, impulse)));
    }

    /// Joints torn by this step's contacts. A strike loads the nearest
    /// tearable joint between the struck part and the torso: hand/forearm →
    /// elbow, foot/shin → knee, upper arm → shoulder, thigh → hip, head → neck.
    /// The torn limb keeps its momentum and springs slightly free.
    pub fn tear(&mut self, limit: usize) -> Vec<Break> {
        let mut direct = vec![0f32; self.joints.len()];
        let mut order = vec![u32::MAX; self.joints.len()];
        for (b, body) in self.bodies.iter().enumerate() {
            if body.impact <= 0. || self.age < 0.05 {
                continue;
            }
            let mut k = Some(b);
            while let Some(i) = k {
                let skip_wrist = i == b && matches!(body.kind, Kind::Hand | Kind::Foot);
                if !skip_wrist
                    && let Some(j) = self.joints.iter().position(|j| j.child == i && j.tearable)
                {
                    let load = body.impact / body.kind.tear_speed();
                    if load >= 1. {
                        order[j] = order[j].min(body.impact_order);
                    }
                    direct[j] = direct[j].max(load);
                    break;
                }
                k = self.bodies[i].parent;
            }
        }
        let mut candidates: Vec<(usize, f32)> = direct
            .iter()
            .enumerate()
            .filter(|&(j, &load)| !self.joints[j].broken && load >= 1.)
            .map(|(j, &load)| (j, load))
            .collect();
        // The part that struck first tears first; ties go to the harder hit.
        candidates.sort_by(|a, b| order[a.0].cmp(&order[b.0]).then(b.1.total_cmp(&a.1)));
        let mut torn: Vec<Break> = Vec::new();
        for (index, load) in candidates {
            // Tearing absorbs energy: each further tear in one crash needs a
            // clearly harder strike, so only the worst-hit parts give way.
            if torn.len() >= limit || load < 1. + 0.6 * self.torn as f32 {
                continue;
            }
            // Severed pieces do not dismember further; a joint and a joint
            // further out on the same limb never both tear.
            let within = |outer: usize, inner_body: usize| {
                let mut k = Some(inner_body);
                while let Some(i) = k {
                    if i == self.joints[outer].child {
                        return true;
                    }
                    k = self.bodies[i].parent;
                }
                false
            };
            let detached = self
                .joints
                .iter()
                .enumerate()
                .any(|(o, j)| j.broken && o != index && within(o, self.joints[index].parent));
            let nested = torn.iter().any(|t| {
                within(index, self.joints[t.joint].child)
                    || within(t.joint, self.joints[index].child)
            });
            if detached || nested {
                continue;
            }
            let (p, c, anchor) = {
                let j = &mut self.joints[index];
                j.broken = true;
                (j.parent, j.child, j.anchor)
            };
            let tear = unit_or(sub(self.bodies[c].x, self.bodies[p].world(anchor)), UP);
            self.bodies[c].v = add(self.bodies[c].v, scale(tear, 4.));
            self.bodies[c].w = add(self.bodies[c].w, scale(cross(tear, UP), 3.));
            self.sleeping = false;
            self.calm = 0.;
            self.torn += 1;
            torn.push(Break {
                joint: index,
                node: self.bodies[c].node,
                load,
            });
        }
        torn
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    const SKELETON: [(&str, i32, V); 18] = [
        ("Pelvis", -1, [0., 2.864, -0.085]),
        ("LwrTorso", 0, [0., 3.193, -0.067]),
        ("UprLegL", 0, [-0.357, 2.93, -0.022]),
        ("UprLegR", 0, [0.357, 2.93, -0.022]),
        ("UprTorso", 1, [0., 3.841, -0.125]),
        ("Head", 4, [0., 4.966, -0.183]),
        ("ShoulderL", 4, [-0.145, 4.646, -0.152]),
        ("ShoulderR", 4, [0.12, 4.641, -0.152]),
        ("UprArmL", 6, [-0.636, 4.549, -0.145]),
        ("LwrArmL", 8, [-1.266, 4.037, -0.143]),
        ("HandL", 9, [-1.957, 3.652, -0.08]),
        ("UprArmR", 7, [0.627, 4.591, -0.145]),
        ("LwrArmR", 11, [1.232, 4.081, -0.143]),
        ("HandR", 12, [1.932, 3.647, -0.08]),
        ("LwrLegL", 2, [-0.391, 1.403, -0.086]),
        ("FootL", 14, [-0.392, 0.303, -0.143]),
        ("LwrLegR", 3, [0.391, 1.403, -0.086]),
        ("FootR", 16, [0.392, 0.303, -0.143]),
    ];
    const BOUNDS: [[f32; 7]; 18] = [
        [0., 0.05, 0., 0.58, 0.46, 0.46, 0.87],
        [0., 0.14, 0., 0.55, 0.28, 0.42, 0.75],
        [0.01, -0.87, 0., 0.35, 1.14, 0.47, 1.33],
        [-0.01, -0.87, 0., 0.35, 1.14, 0.47, 1.33],
        [0., 0.18, 0., 0.61, 0.98, 0.5, 1.32],
        [0.02, 0.37, 0.14, 0.48, 0.7, 0.61, 1.18],
        [-0.29, -0.22, -0.01, 0.39, 0.46, 0.39, 0.73],
        [0.3, -0.22, -0.01, 0.4, 0.46, 0.39, 0.74],
        [-0.3, -0.17, 0., 0.47, 0.5, 0.38, 0.79],
        [-0.3, -0.2, -0.05, 0.49, 0.36, 0.26, 0.68],
        [-0.14, -0.15, 0.03, 0.36, 0.2, 0.31, 0.59],
        [0.31, -0.21, 0., 0.47, 0.5, 0.38, 0.79],
        [0.33, -0.24, -0.05, 0.49, 0.36, 0.26, 0.68],
        [0.17, -0.15, 0.03, 0.36, 0.2, 0.31, 0.59],
        [0.01, -0.51, -0.01, 0.3, 0.83, 0.35, 0.95],
        [0., -0.22, 0.27, 0.24, 0.2, 0.55, 0.66],
        [-0.01, -0.51, -0.01, 0.3, 0.83, 0.35, 0.95],
        [0., -0.22, 0.27, 0.24, 0.2, 0.55, 0.66],
    ];
    pub(crate) fn rider() -> Vec<Node> {
        SKELETON
            .iter()
            .zip(BOUNDS)
            .map(|((name, parent, p), bounds)| {
                let mut m = [0.; 16];
                m[0] = 1.;
                m[5] = 1.;
                m[10] = 1.;
                m[15] = 1.;
                m[12..15].copy_from_slice(p);
                Node {
                    name: (*name).into(),
                    parent: *parent,
                    local_matrix: m,
                    world_matrix: m,
                    bounds,
                }
            })
            .collect()
    }
    /// Bind pose rotated by `q` about the pelvis and lifted to `height`.
    pub(crate) fn posed(nodes: &[Node], q: Q, lift: V) -> Vec<M> {
        let pivot = origin(&nodes[0].world_matrix);
        nodes
            .iter()
            .map(|n| {
                let p = add(
                    add(pivot, qrot(q, sub(origin(&n.world_matrix), pivot))),
                    lift,
                );
                let x = qrot(q, [1., 0., 0.]);
                let y = qrot(q, [0., 1., 0.]);
                let z = qrot(q, [0., 0., 1.]);
                [
                    x[0], x[1], x[2], 0., y[0], y[1], y[2], 0., z[0], z[1], z[2], 0., p[0], p[1],
                    p[2], 1.,
                ]
            })
            .collect()
    }
    pub(crate) fn axis_angle(axis: V, angle: f32) -> Q {
        let a = unit_or(axis, UP);
        let (s, c) = (angle * 0.5).sin_cos();
        [a[0] * s, a[1] * s, a[2] * s, c]
    }
    fn flat(_: V) -> Option<(f32, V)> {
        Some((0., UP))
    }
    fn knee_and_elbow_angles(r: &Ragdoll) -> Vec<(f32, f32)> {
        r.joints
            .iter()
            .filter_map(|j| match j.limit {
                Limit::Hinge { axis, bone, .. } if !j.broken => {
                    let (p, c) = (&r.bodies[j.parent], &r.bodies[j.child]);
                    let ap = qrot(p.q, axis);
                    let ac = qrot(c.q, axis);
                    let bp = qrot(p.q, bone);
                    let bc = qrot(c.q, bone);
                    Some((
                        dot(cross(bp, bc), ap).atan2(dot(bp, bc)),
                        dot(ap, ac).clamp(-1., 1.).acos(),
                    ))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn builds_fifteen_segments_and_reproduces_the_input_pose() {
        let nodes = rider();
        let pose = posed(&nodes, axis_angle([0.3, 1., 0.2], 0.9), [10., 4., -3.]);
        let r = Ragdoll::new(&nodes, &pose, None, [0.; 3]).unwrap();
        assert_eq!(r.body_count(), 16);
        assert_eq!(r.joints.len(), 15);
        for (a, b) in r.matrices().iter().zip(&pose) {
            for k in 0..16 {
                assert!((a[k] - b[k]).abs() < 1e-4, "{a:?} {b:?}");
            }
        }
        // Clavicles ride rigidly on the chest; arms hang from them.
        assert_eq!(r.body_of_node(6), r.body_of_node(4));
        let arm = r.body_of_node(8).unwrap();
        assert_eq!(
            r.joints.iter().find(|j| j.child == arm).unwrap().parent,
            r.body_of_node(4).unwrap()
        );
    }

    #[test]
    fn hinges_never_bend_backwards_or_sideways_through_violent_tumbles() {
        let nodes = rider();
        for (k, (axis, angle, spin)) in [
            ([1., 0., 0.], 1.6, [9., 3., -4.]),
            ([0., 0., 1.], 3.0, [-6., 8., 12.]),
            ([1., 1., 0.], 2.2, [14., -9., 5.]),
            ([0., 1., 1.], -1.2, [-12., 15., -7.]),
        ]
        .into_iter()
        .enumerate()
        {
            let pose = posed(&nodes, axis_angle(axis, angle), [0., 6., 0.]);
            let mut r =
                Ragdoll::new(&nodes, &pose, None, [30., -20. - k as f32 * 10., 8.]).unwrap();
            for b in &mut r.bodies {
                b.w = spin;
            }
            let slope = |p: V| Some((p[0] * 0.25, unit_or([-0.25, 1., 0.], UP)));
            for _ in 0..360 {
                r.step(1. / 60., &slope);
                for (bend, misalignment) in knee_and_elbow_angles(&r) {
                    assert!((-0.2..=2.7).contains(&bend), "hinge bent to {bend}");
                    assert!(
                        misalignment < 0.2,
                        "hinge twisted off-axis by {misalignment}"
                    );
                }
                for j in &r.joints {
                    let gap = length(sub(
                        r.bodies[j.parent].world(j.anchor),
                        r.bodies[j.child].world([0.; 3]),
                    ));
                    assert!(gap < 0.12, "joint separated by {gap}");
                }
                for b in &r.bodies {
                    for s in b.segment {
                        let p = b.world(s);
                        assert!(p.iter().all(|v| v.is_finite()));
                        assert!(p[1] - p[0] * 0.25 > -0.2, "segment sank into terrain");
                    }
                }
            }
        }
    }

    #[test]
    fn swing_cones_hold_hips_shoulders_and_neck() {
        let nodes = rider();
        let pose = posed(&nodes, axis_angle([0., 0., 1.], 1.4), [0., 8., 0.]);
        let mut r = Ragdoll::new(&nodes, &pose, None, [0., -30., 0.]).unwrap();
        for b in &mut r.bodies {
            b.w = [20., -10., 15.];
        }
        let mut worst: f32 = 0.;
        for _ in 0..240 {
            r.step(1. / 60., &flat);
            for j in &r.joints {
                if let Limit::Ball {
                    center,
                    e1,
                    e2,
                    swing,
                    bone,
                    ..
                } = j.limit
                {
                    let rel = qmul(qconj(r.bodies[j.parent].q), r.bodies[j.child].q);
                    let v = qrot(rel, bone);
                    let theta = dot(v, center).clamp(-1., 1.).acos();
                    let psi = dot(v, e2).atan2(dot(v, e1));
                    let a1 = if psi.cos() >= 0. { swing[0] } else { swing[1] };
                    let a2 = if psi.sin() >= 0. { swing[2] } else { swing[3] };
                    let max = 1. / ((psi.cos() / a1).powi(2) + (psi.sin() / a2).powi(2)).sqrt();
                    worst = worst.max(theta - max);
                }
            }
        }
        assert!(worst < 0.2, "swing overshoot {worst}");
    }

    #[test]
    fn settles_and_sleeps_on_flat_ground() {
        let nodes = rider();
        let pose = posed(&nodes, axis_angle([1., 0., 0.], -1.5), [0., 3., 0.]);
        let mut r = Ragdoll::new(&nodes, &pose, None, [12., 0., 0.]).unwrap();
        for _ in 0..600 {
            r.step(1. / 60., &flat);
            if r.sleeping {
                break;
            }
        }
        assert!(r.sleeping, "ragdoll never came to rest");
        let before = r.matrices();
        r.step(1. / 60., &flat);
        assert_eq!(before, r.matrices());
    }

    #[test]
    fn animated_motion_carries_into_the_ragdoll() {
        let nodes = rider();
        let previous = posed(&nodes, [0., 0., 0., 1.], [0., 5., 0.]);
        let pose = posed(&nodes, axis_angle([1., 0., 0.], 0.05), [0.5, 5., 0.]);
        let r = Ragdoll::new(&nodes, &pose, Some((&previous, 1. / 60.)), [30., 0., 0.]).unwrap();
        let pelvis = r.body_of_node(0).unwrap();
        // Launch momentum, not the animated 30 ft/s, dominates the actor.
        assert!((r.bodies[pelvis].v[0] - 30.).abs() < 3.);
        // The rotating animation leaves a forward pitch on every segment.
        assert!(r.bodies.iter().all(|b| b.w[0] > 1.));
    }

    #[test]
    fn head_first_impact_tears_the_neck_and_flat_landings_tear_nothing() {
        let nodes = rider();
        let head = nodes.iter().position(|n| n.name == "Head").unwrap();
        let upside_down = posed(
            &nodes,
            axis_angle([1., 0., 0.], std::f32::consts::PI),
            [0., 9., 0.],
        );
        let mut r = Ragdoll::new(&nodes, &upside_down, None, [0., -62., 0.]).unwrap();
        let mut torn = Vec::new();
        for _ in 0..30 {
            r.step(1. / 60., &flat);
            torn.extend(r.tear(4));
            if !torn.is_empty() {
                break;
            }
        }
        assert_eq!(
            torn.first().map(|t| t.node),
            Some(head),
            "head-first slam takes the neck"
        );
        let flat_pose = posed(
            &nodes,
            axis_angle([1., 0., 0.], -std::f32::consts::FRAC_PI_2),
            [0., 1.2, 0.],
        );
        let mut r = Ragdoll::new(&nodes, &flat_pose, None, [6., -18., 0.]).unwrap();
        for _ in 0..120 {
            r.step(1. / 60., &flat);
            assert!(r.tear(4).is_empty(), "moderate flat landing tore a joint");
        }
    }

    #[test]
    fn a_limb_that_strikes_first_is_the_one_that_comes_off() {
        let nodes = rider();
        // Roll the rider onto the left side so the left arm meets the ground.
        let side = posed(&nodes, axis_angle([0., 0., 1.], 1.45), [0., 7., 0.]);
        let mut r = Ragdoll::new(&nodes, &side, None, [0., -60., 0.]).unwrap();
        let mut torn = Vec::new();
        for _ in 0..40 {
            r.step(1. / 60., &flat);
            torn.extend(r.tear(4));
        }
        assert!(!torn.is_empty(), "severe side slam tore nothing");
        assert!(torn.len() <= 2, "everything popped off: {}", torn.len());
        // The limb that met the ground goes first; a whipping far limb may follow.
        assert_eq!(nodes[torn[0].node].name, "LwrArmL");
    }
}
#[cfg(test)]
mod probe {
    use super::tests::*;
    use super::*;
    /// `cargo test --release -p atv-core ragdoll_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn ragdoll_cost() {
        let nodes = rider();
        let pose = posed(&nodes, axis_angle([0., 0., 1.], 2.0), [0., 6., 0.]);
        let slope = |p: V| Some((p[0] * 0.25, unit_or([-0.25, 1., 0.], UP)));
        let start = std::time::Instant::now();
        let mut frames = 0;
        for k in 0..200 {
            let mut r =
                Ragdoll::new(&nodes, &pose, None, [40., -20. - k as f32 * 0.1, 8.]).unwrap();
            for _ in 0..90 {
                r.step(1. / 60., &slope);
                frames += 1;
            }
        }
        eprintln!(
            "{:.1} us per 60 Hz frame",
            start.elapsed().as_secs_f64() * 1e6 / frames as f64
        );
    }
}
