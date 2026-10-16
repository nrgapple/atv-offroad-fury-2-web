//! Motocross chassis collision recovered from SLUS_211.04 (vehicle world
//! detection 002051f0, response 00205730: restitution impulse 00206320,
//! spring 00205f00, friction 00206c70) with the bike's COL v2 whisker rays
//! (`bikem.col`, `WreckBikem.col`; 50cc `bike50m.col`/`WreckBike50m.col`).
//! Contacts are found once per frame from the start-of-frame pose and
//! re-evaluated each substep. Impulses feed the per-frame impact vector that
//! the wreck chain compares with the Front/Back/Side/Down limits.
//!
//! Adapters: ATV2 terrain and the partitioned ATV2 scene (static props, plant
//! proxies, moving train bodies as zero-mass kinematic targets) stand in for
//! the MX collision world; host trigonometry; bike-to-bike chassis contact
//! uses the separate peer-contact proxy.
use super::*;

/// Riding and wrecked whisker rays in model space (start, end).
#[derive(Debug, Clone, Default)]
pub struct Chassis {
    pub riding: Vec<(V, V)>,
    pub wreck: Vec<(V, V)>,
    /// Ray ends above this model height are the rider's "head" rays.
    pub head_height: f32,
    pub source: bool,
}

impl Chassis {
    pub(super) fn load(resource: &Resource<'_>, size: u16) -> Option<Self> {
        let (riding, wreck) = if size == 50 {
            ("bike50m.col", "WreckBike50m.col")
        } else {
            ("bikem.col", "WreckBikem.col")
        };
        let rays = |leaf| -> Option<Vec<(V, V)>> {
            let tree = crate::collision::decode_mx_ray_tree(&member(resource, leaf).ok()?).ok()?;
            Some(tree.rays.iter().map(|r| (r.start, r.end)).collect())
        };
        Some(Self {
            riding: rays(riding)?,
            wreck: rays(wreck)?,
            head_height: if size == 50 { 2.9 } else { 4.5 },
            source: true,
        })
    }
    /// Authored fallback while a hosted package predates the source COL
    /// members: whiskers from the chassis centre to the grips, wheel fronts,
    /// fenders and pegs of this bike's own model, plus wheel bottoms when
    /// wrecked. Replaced by the source rays once the package carries them.
    pub(super) fn fallback(mesh: &Mesh, wheels: [V; 2], size: u16) -> Self {
        let node = |name: &str| {
            mesh.nodes
                .iter()
                .find(|n| n.name.eq_ignore_ascii_case(name))
                .map(|n| [n.world_matrix[12], n.world_matrix[13], n.world_matrix[14]])
        };
        let scale_y = if size == 50 { 0.62 } else { 1. };
        let center = [0., 2.8 * scale_y, (wheels[0][2] + wheels[1][2]) * 0.5];
        let mut ends = vec![
            add(wheels[0], [0., -0.2, 0.84 * scale_y]),
            add(wheels[1], [0., -0.15, -0.83 * scale_y]),
            add(wheels[0], [0., 1.85 * scale_y, 0.5]),
            add(wheels[1], [0., 2.6 * scale_y, -0.3]),
        ];
        ends.extend(
            [
                "Dummy_Hand_L",
                "Dummy_Hand_R",
                "Dummy_Foot_L",
                "Dummy_Foot_R",
            ]
            .iter()
            .filter_map(|n| node(n)),
        );
        let riding: Vec<_> = ends.iter().map(|e| (center, *e)).collect();
        let mut wreck = riding.clone();
        for w in wheels {
            wreck.push((center, [w[0], 0.05, w[2]]));
        }
        Self {
            riding,
            wreck,
            head_height: if size == 50 { 2.9 } else { 4.5 },
            source: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Contact {
    end: V,
    point: V,
    normal: V,
    motion: Option<crate::train::MotionState>,
    pub head: bool,
}

/// Chassis spring/impulse/friction coefficients from [Mass Properties].
#[derive(Debug, Clone, Copy)]
pub(super) struct Response {
    stiffness: f32,
    damping: f32,
    max_penetration: f32,
    restitution: f32,
    moment_scale: V,
    friction: f32,
}
impl Response {
    pub fn parse(data: &PhysicsData, weight: f32) -> Self {
        let get =
            |key: &str, default: f32| data.first_scalar("Mass Properties", key).unwrap_or(default);
        let mass = weight / 32.2;
        let stiffness = (get("SpringStiffness", 6.) * weight).abs();
        Self {
            stiffness,
            damping: get("SpringDamping", 0.1) * 2. * (mass * stiffness).sqrt(),
            max_penetration: get("MaxPenetration", 0.5),
            restitution: get("Restitution", 0.),
            moment_scale: data
                .vector3("Mass Properties", "SpringMomentScale")
                .unwrap_or([0., 1., 1.]),
            friction: get("Friction", 0.5),
        }
    }
}

impl Motorcycle {
    fn chassis_world(&self, local: V) -> V {
        add(
            self.body.origin(&self.body_config),
            dynamics::transform(self.body.basis(), local),
        )
    }

    /// Once per frame: downward whiskers against terrain, every whisker
    /// against the nearby scene meshes (including moving train bodies).
    pub(super) fn detect_chassis(&mut self, terrain: &Terrain) -> Result<()> {
        self.chassis_contacts.clear();
        let wreck = self.frame.wreck.phase != crate::wreck::Phase::Riding;
        let rays = if wreck {
            &self.model.chassis.wreck
        } else {
            &self.model.chassis.riding
        };
        let up = dynamics::inverse_transform(self.body.basis(), [0., 1., 0.]);
        let head = self.model.chassis.head_height;
        let mut found = Vec::new();
        let mut world_rays = Vec::with_capacity(rays.len());
        for &(start, end) in rays {
            let (s, e) = (self.chassis_world(start), self.chassis_world(end));
            world_rays.push(crate::collision::CollisionRay { start: s, end: e });
            if dot(sub(end, start), up) >= 0. {
                continue;
            }
            if let Some(hit) = terrain.contact(e[0], e[2])?
                && e[1] < hit.height
            {
                let depth = (e[1] - hit.height) * hit.normal[1];
                found.push(Contact {
                    end,
                    point: sub(e, scale(hit.normal, depth)),
                    normal: hit.normal,
                    motion: None,
                    head: end[1] > head,
                });
            }
        }
        let mut plants = 0;
        for (obstacle, motion, plant) in &self.contacts {
            for hit in crate::wreck::query_static_segments(
                &world_rays,
                std::slice::from_ref(obstacle),
                60usize.saturating_sub(found.len()),
            )? {
                plants += usize::from(*plant);
                let end = rays[hit.ray_index].1;
                found.push(Contact {
                    end,
                    point: hit.world_point,
                    normal: hit.world_normal,
                    motion: *motion,
                    head: end[1] > head,
                });
            }
            if found.len() >= 60 {
                break;
            }
        }
        self.drive.chassis_hit |= !found.is_empty();
        self.drive.head_hit |= found.iter().any(|c| c.head);
        self.frame.chassis_contact_count = found.len();
        self.frame.vegetation_contact_count = plants;
        self.frame.dynamic_contact_count = found.iter().filter(|c| c.motion.is_some()).count();
        self.chassis_contacts = found;
        Ok(())
    }

    /// Each substep before controls and tires: a hard contact (closing faster
    /// than 10 ft/s) takes a zero-restitution impulse with angular terms;
    /// otherwise a spring with SpringMomentScale; friction always.
    pub(super) fn respond_chassis(&mut self) {
        let r = self.chassis_response;
        let mass = self.body_config.mass;
        let inertia = self.body_config.inertia;
        for i in 0..self.chassis_contacts.len() {
            let c = self.chassis_contacts[i];
            let basis = self.body.basis();
            let cg = self.body.center;
            let arm = sub(c.point, cg);
            let other = c.motion.map_or([0.; 3], |m| m.point_velocity(c.point));
            let relative = sub(self.body.velocity_at(c.point), other);
            let vn = dot(relative, c.normal);
            if vn < -10. {
                let rb = dynamics::inverse_transform(basis, arm);
                let nb = dynamics::inverse_transform(basis, c.normal);
                let w: V = std::array::from_fn(|k| crate::wheel::cross(rb, nb)[k] / inertia[k]);
                let k = 1. / mass + dot(nb, crate::wheel::cross(w, rb));
                let j = -(1. + r.restitution) * vn / k;
                let dv = scale(c.normal, j / mass);
                self.body.linear_velocity = add(self.body.linear_velocity, dv);
                self.drive.substep_dv = add(self.drive.substep_dv, dv);
                let dw = crate::wheel::cross(rb, scale(nb, j));
                for (k, w) in self.body.local_angular_velocity.iter_mut().enumerate() {
                    *w += dw[k] / inertia[k];
                }
                self.frame.crash_impact_speed = self.frame.crash_impact_speed.max(-vn);
            } else {
                let end = self.chassis_world(c.end);
                let depth = dot(sub(c.point, end), c.normal);
                if depth > 0. {
                    let f = (r.stiffness * depth.min(r.max_penetration) - r.damping * vn).max(0.);
                    let force = scale(c.normal, f);
                    self.body.force = add(self.body.force, force);
                    let torque = dynamics::inverse_transform(
                        basis,
                        crate::wheel::cross(sub(end, cg), force),
                    );
                    for (k, t) in self.body.local_torque.iter_mut().enumerate() {
                        *t += r.moment_scale[k] * torque[k];
                    }
                }
            }
            // Friction: per contact, load-independent, ramping to full at
            // 10 ft/s of slip; FrictionMomentScale is zero for the bike.
            let relative = sub(
                self.body.velocity_at(c.point),
                c.motion.map_or([0.; 3], |m| m.point_velocity(c.point)),
            );
            let tangent = sub(scale(c.normal, dot(relative, c.normal)), relative);
            let slip = dot(tangent, tangent).sqrt();
            if slip > 1e-4 {
                let f = r.friction * mass * 32.2 * (0.1 * slip).min(1.);
                self.body.force = add(self.body.force, scale(tangent, f / slip));
            }
        }
    }
}
