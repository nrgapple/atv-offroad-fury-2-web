//! Native motorcycle handling probe: scripted inputs on a synthetic course
//! (level ground, whoops and a jump) or a real map, with per-step telemetry
//! and summary metrics. Reads the supplied private bike and terrain archives;
//! it writes measurements only, never game content.
//!
//! `BIKE_PROBE_CLASSIC=1` rides the recovered MX side-force model instead of
//! the authored single-track handling; `BIKE_PROBE_MODE=sport|simulation`
//! selects the handling preset.
use atv_core::{
    Result, dynamics,
    motorcycle::{Model, Motorcycle},
    resource::Resource,
    terrain::{self, Terrain},
    tire_wear::Handling,
    vehicle::VehicleInput,
    wreck::Phase,
};
use serde_json::{Value, json};

const DT: f32 = 1. / 60.;
const MPH: f32 = 1.46666;
const CELL: f32 = 8.;
const LEAVES: u32 = 32;
/// Lanes of the synthetic course (x of the centre line).
const FLAT: f32 = 2048.;
const WHOOPS: f32 = 1600.;
const JUMP: f32 = 1200.;
const ROUGH: f32 = 600.;
const START_Z: f32 = 400.;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
/// Height (ft above the base) of the synthetic course.
fn course_height(x: f32, z: f32) -> f32 {
    if (x - WHOOPS).abs() < 100. {
        // Eighteen whoops, 0.9 ft high and 14 ft apart.
        let along = z - 700.;
        if (0. ..252.).contains(&along) {
            return 0.45 * (1. - (along * std::f32::consts::TAU / 14.).cos());
        }
    } else if (x - ROUGH).abs() < 500. && z > 500. {
        // Rough ground: bumps a few inches high, a few feet apart.
        return 0.45 + 0.25 * (x / 3.1).sin() * (z / 4.3).sin() + 0.15 * (z / 1.9 + x / 2.3).sin();
    } else if (x - JUMP).abs() < 100. {
        // A 9 ft kicker, a gap, then a 7 ft landing that falls away.
        if (790. ..840.).contains(&z) {
            return 9. * smooth((z - 790.) / 50.);
        } else if (840. ..848.).contains(&z) {
            return 9. * (1. - (z - 840.) / 8.);
        } else if (862. ..876.).contains(&z) {
            return 7. * smooth((z - 862.) / 14.);
        } else if (876. ..986.).contains(&z) {
            return 7. * (1. - smooth((z - 876.) / 110.));
        }
    }
    0.
}
/// The synthetic course as original-format terrain: 8 ft cells, one tile per
/// 16-cell leaf, packed normals from the height gradient.
fn course() -> Terrain {
    const BASE: f32 = 100.;
    let scale = 1. / 256.;
    let mut tiles = Vec::new();
    let mut leaves = Vec::new();
    for lz in 0..LEAVES {
        for lx in 0..LEAVES {
            let mut heights = [0u16; 289];
            let mut attributes = [0u32; 289];
            for i in 0..289 {
                let x = (lx * 16 + (i % 17) as u32) as f32 * CELL;
                let z = (lz * 16 + (i / 17) as u32) as f32 * CELL;
                let h = course_height(x, z);
                heights[i] = ((BASE + h) / (scale * CELL)).round() as u16;
                let e = 1.;
                let dx = (course_height(x + e, z) - course_height(x - e, z)) / (2. * e);
                let dz = (course_height(x, z + e) - course_height(x, z - e)) / (2. * e);
                let l = (dx * dx + 1. + dz * dz).sqrt();
                let byte = |v: f32| (v / l * 127.).round() as i8 as u8;
                attributes[i] = u32::from_le_bytes([byte(-dx), byte(1.), byte(-dz), 0]);
            }
            leaves.push(terrain::TerrainLeaf {
                x: lx * 16,
                z: lz * 16,
                tile: tiles.len() as u16,
                height_morph: 1.,
                height_scale: scale,
                minimum_height: 0.,
                maximum_height: 400.,
            });
            tiles.push(terrain::TerrainTile {
                tag: 0,
                heights,
                attributes,
            });
        }
    }
    Terrain {
        spacing: CELL,
        tiles,
        leaves,
        hierarchy_nodes: 1,
        width_cells: LEAVES * 16,
        depth_cells: LEAVES * 16,
        leaf_index: Default::default(),
    }
}
/// A vertical test wall facing -z at `z`, 200 ft wide and 60 ft tall.
fn wall(z: f32, base: f32) -> atv_core::obstacle::StaticObstacle {
    use atv_core::collision::{CollisionMesh, RayTreeNode, Triangle};
    let vertices = vec![[2148., base, z], [1948., base, z], [2048., base + 60., z]];
    atv_core::obstacle::StaticObstacle {
        mesh: std::rc::Rc::new(CollisionMesh {
            transform: [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.],
            vertices,
            vertex_normal_bits: None,
            triangles: vec![Triangle {
                indices: [0, 1, 2],
                normal: [0., 0., -1.],
                distance: z,
                invalid_plane_bits: None,
            }],
            broad_phase_nodes: 1,
            nodes: vec![RayTreeNode {
                center: [2048., base + 30., z],
                half_extents: [100., 30., 0.],
                bound_measure: 100. * 100. + 30. * 30.,
                children: [0x8000, 0x8000],
            }],
            flags: 0,
        }),
        world_matrix: [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ],
    }
}
fn terrain_from(res: &[u8]) -> Result<Terrain> {
    if res == b"flat" {
        return Ok(course());
    }
    let resource = Resource::parse(res)?;
    let index = resource
        .members
        .iter()
        .position(|m| m.name.to_ascii_lowercase().ends_with(".tr2"))
        .ok_or("terrain archive has no TR2 member")?;
    terrain::decode_tr2(&resource.extract(index)?)
}

/// Flattest open 120 ft run near the map centre facing `yaw`.
fn flat_start(terrain: &Terrain) -> Result<([f32; 3], f32)> {
    let extent = terrain.width_cells.max(terrain.depth_cells) as f32 * terrain.spacing;
    let mut best = (f32::MAX, [0.; 3], 0.);
    for gx in 4..28 {
        for gz in 4..28 {
            let x = extent * gx as f32 / 32.;
            let z = extent * gz as f32 / 32.;
            for yaw in [0., 1., 2., 3.].map(|q: f32| q * std::f32::consts::FRAC_PI_2) {
                let mut heights = vec![];
                for s in 0..=24 {
                    let d = s as f32 * 10.;
                    match terrain.contact(x + yaw.sin() * d, z + yaw.cos() * d)? {
                        Some(h) => heights.push(h.height),
                        None => break,
                    }
                }
                if heights.len() < 25 {
                    continue;
                }
                let rough: f32 = heights.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
                if rough < best.0 {
                    best = (rough, [x, heights[0] + 2.5, z], yaw);
                }
            }
        }
    }
    if best.0 == f32::MAX {
        return Err("no open start found".into());
    }
    Ok((best.1, best.2))
}

fn angles(q: [f32; 4]) -> (f32, f32, f32) {
    let b = dynamics::basis(q);
    let pitch = (-b[2][1]).atan2((b[2][0] * b[2][0] + b[2][2] * b[2][2]).sqrt());
    let roll = b[0][1].atan2(b[1][1]);
    let yaw = b[2][0].atan2(b[2][2]);
    (roll, pitch, yaw)
}

/// Riding controls, the trick button mask and the trick stick.
type Controls = (VehicleInput, u8, [f32; 2]);
type Script = fn(f32, &Motorcycle) -> Controls;
fn input(throttle: f32, brake: f32, steer: f32, lean: f32) -> Controls {
    (
        VehicleInput {
            throttle,
            brake,
            steer,
            lean,
            ..VehicleInput::default()
        },
        0,
        [0.; 2],
    )
}
fn speed(b: &Motorcycle) -> f32 {
    b.frame().speed / MPH
}
fn airborne(b: &Motorcycle) -> bool {
    !b.frame().grounded.iter().any(|g| *g)
}

fn cruise(b: &Motorcycle, mph: f32) -> Controls {
    let s = speed(b);
    if s < mph - 1. {
        input(1., 0., 0., 0.)
    } else if s > mph + 0.5 {
        input(0., 0.3, 0., 0.)
    } else {
        input(0.25, 0., 0., 0.)
    }
}
/// Hold `mph`, steering `steer` from `from` seconds.
fn corner(t: f32, b: &Motorcycle, mph: f32, from: f32, steer: f32) -> Controls {
    let hold = speed(b) < mph;
    input(
        if hold { 1. } else { 0.3 },
        0.,
        if t > from { steer } else { 0. },
        0.,
    )
}
struct Scenario {
    name: &'static str,
    seconds: f32,
    lane: f32,
    script: Script,
}
fn scenarios() -> Vec<Scenario> {
    let flat = |name, seconds, script: Script| Scenario {
        name,
        seconds,
        lane: FLAT,
        script,
    };
    vec![
        flat("launch", 10., |_, _| input(1., 0., 0., 0.)),
        flat("turn-20mph-full", 10., |t, b| corner(t, b, 20., 5., 1.)),
        flat("turn-30mph-half", 12., |t, b| corner(t, b, 30., 5., 0.5)),
        flat("turn-30mph-full", 12., |t, b| corner(t, b, 30., 5., 1.)),
        flat("turn-45mph-full", 14., |t, b| corner(t, b, 45., 7., 1.)),
        flat("turn-60mph-full", 16., |t, b| corner(t, b, 60., 10., 1.)),
        flat("slalom-40mph", 14., |t, b| {
            let hold = speed(b) < 40.;
            let steer = if t > 5. { (t * 2.).sin() } else { 0. };
            input(if hold { 1. } else { 0.4 }, 0., steer, 0.)
        }),
        flat("flick-40mph", 12., |t, b| {
            // Hard left, hard right, release: how quickly the bike changes
            // direction and settles.
            let hold = speed(b) < 40.;
            let steer = match t {
                t if (6. ..7.).contains(&t) => 1.,
                t if (7. ..8.).contains(&t) => -1.,
                _ => 0.,
            };
            input(if hold { 1. } else { 0.4 }, 0., steer, 0.)
        }),
        flat("throttle-in-turn", 12., |t, b| {
            // Roll into a turn at 25 mph, then pin the throttle.
            if t < 6. {
                corner(t, b, 25., 4.5, 0.9)
            } else {
                input(1., 0., 0.9, 0.)
            }
        }),
        flat("brake-slide", 12., |t, b| {
            // Brake hard while turning in at 45 mph, then drive out.
            if t < 7. {
                corner(t, b, 45., 99., 0.)
            } else if t < 8.2 {
                input(0., 1., 0.9, 0.)
            } else {
                input(1., 0., 0.4, 0.)
            }
        }),
        flat("brake-from-45", 12., |t, b| {
            if t < 7. && speed(b) < 45. {
                input(1., 0., 0., 0.)
            } else if t < 7. {
                input(0.4, 0., 0., 0.)
            } else {
                input(0., 1., 0., 0.)
            }
        }),
        flat("slow-full-lock", 10., |t, _| {
            input(0.25, 0., if t > 2. { 1. } else { 0. }, 0.)
        }),
        flat("tap-steer-50mph", 14., |t, b| {
            let hold = speed(b) < 50.;
            let steer = if (8. ..8.4).contains(&t) { 1. } else { 0. };
            input(if hold { 1. } else { 0.45 }, 0., steer, 0.)
        }),
        flat("wall-15mph", 14., |_, b| cruise(b, 15.)),
        flat("wall-20mph", 14., |_, b| cruise(b, 20.)),
        flat("wall-25mph", 14., |_, b| cruise(b, 25.)),
        flat("wall-35mph", 14., |_, b| cruise(b, 35.)),
        flat("wheelie", 10., |t, _| {
            input(1., 0., 0., if t > 4. { 1. } else { 0. })
        }),
        flat("free-run", 30., |t, _| {
            // Flat out with a gentle weave: what a real map does to the bike.
            input(1., 0., 0.35 * (t / 1.5).sin(), 0.)
        }),
        flat("wheelie-turn", 10., |t, _| {
            input(
                1.,
                0.,
                if t > 6. { 0.6 } else { 0. },
                if t > 4. { 1. } else { 0. },
            )
        }),
        Scenario {
            name: "whoops-35mph",
            seconds: 14.,
            lane: WHOOPS,
            script: |_, b| cruise(b, 35.),
        },
        Scenario {
            name: "whoops-flat-out",
            seconds: 14.,
            lane: WHOOPS,
            script: |_, _| input(1., 0., 0., 0.),
        },
        Scenario {
            name: "rough-straight-45mph",
            seconds: 14.,
            lane: ROUGH,
            script: |_, b| cruise(b, 45.),
        },
        Scenario {
            name: "rough-turn-30mph-full",
            seconds: 12.,
            lane: ROUGH,
            script: |t, b| corner(t, b, 30., 5., 1.),
        },
        Scenario {
            name: "rough-slalom-40mph",
            seconds: 14.,
            lane: ROUGH,
            script: |t, b| {
                let hold = speed(b) < 40.;
                let steer = if t > 5. { (t * 2.).sin() } else { 0. };
                input(if hold { 1. } else { 0.4 }, 0., steer, 0.)
            },
        },
        Scenario {
            name: "jump",
            seconds: 14.,
            lane: JUMP,
            script: |_, b| cruise(b, 42.),
        },
        Scenario {
            name: "jump-whip",
            seconds: 14.,
            lane: JUMP,
            script: |_, b| {
                let mut c = cruise(b, 42.);
                if airborne(b) && b.frame().origin[2] > 845. {
                    c.0.steer = 1.;
                }
                c
            },
        },
        Scenario {
            name: "jump-trick-tap",
            seconds: 14.,
            lane: JUMP,
            script: |_, b| {
                // Tap Circle+Up for a quarter second after take-off.
                let mut c = cruise(b, 42.);
                if airborne(b) && (845. ..865.).contains(&b.frame().origin[2]) {
                    c.1 = 1;
                    c.2 = [0., -1.];
                }
                c
            },
        },
        Scenario {
            name: "jump-trick-held",
            seconds: 14.,
            lane: JUMP,
            script: |_, b| {
                // Hold Triangle+Left until just before the landing.
                let mut c = cruise(b, 42.);
                if airborne(b) && (845. ..868.).contains(&b.frame().origin[2]) {
                    c.1 = 2;
                    c.2 = [-1., 0.];
                }
                c
            },
        },
        Scenario {
            name: "jump-backflip",
            seconds: 14.,
            lane: JUMP,
            script: |_, b| {
                // The airborne shortcut: trick button plus pull back.
                let mut c = cruise(b, 58.);
                if airborne(b) && (845. ..865.).contains(&b.frame().origin[2]) {
                    c.1 = 4;
                    c.2 = [0., 1.];
                }
                c
            },
        },
        Scenario {
            name: "jump-360",
            seconds: 14.,
            lane: JUMP,
            script: |_, b| {
                let mut c = cruise(b, 58.);
                if airborne(b) && (845. ..865.).contains(&b.frame().origin[2]) {
                    c.1 = 4;
                    c.2 = [1., 0.];
                }
                c
            },
        },
    ]
}

pub fn run(
    bike_res: &[u8],
    id: u16,
    map_res: &[u8],
    materials: Option<[atv_core::materials::TerrainMaterial; 8]>,
    filter: Option<&str>,
) -> Result<Value> {
    let terrain = terrain_from(map_res)?;
    let synthetic = map_res == b"flat";
    let classic = std::env::var("BIKE_PROBE_CLASSIC").is_ok_and(|v| v == "1");
    let mode = match std::env::var("BIKE_PROBE_MODE").as_deref() {
        Ok("sport") => Handling::Sport,
        Ok("simulation") => Handling::Simulation,
        _ => Handling::Original,
    };
    let mut results = vec![];
    for scenario in scenarios() {
        let name = scenario.name;
        if filter.is_some_and(|f| !name.contains(f)) {
            continue;
        }
        if !synthetic && scenario.lane != FLAT {
            continue;
        }
        let (origin, yaw) = if synthetic {
            let ground = terrain
                .contact(scenario.lane, START_Z)?
                .ok_or("course start is off the terrain")?
                .height;
            ([scenario.lane, ground + 2.5, START_Z], 0.)
        } else {
            flat_start(&terrain)?
        };
        let model = Model::load(bike_res, id)?;
        let mut bike = Motorcycle::new(model, origin, yaw)?;
        bike.set_classic(classic);
        bike.tires.mode = mode;
        bike.set_materials(materials);
        if name.starts_with("wall-") {
            bike.set_obstacles(vec![wall(origin[2] + 300., origin[1] - 2.5)]);
        }
        let mut rows = vec![];
        let mut wrecks = 0;
        let mut kinds = Vec::new();
        let mut stunts: Vec<String> = Vec::new();
        let mut peak_impact = 0f32;
        let mut previous_mph = 0f32;
        let mut was_riding = true;
        let mut previous_yaw = None;
        let mut flights: Vec<f32> = Vec::new();
        let mut flight = 0.;
        let steps = (scenario.seconds / DT) as usize;
        for i in 0..steps {
            let t = i as f32 * DT;
            let (control, mask, axes) = (scenario.script)(t, &bike);
            bike.spin_controls(mask, axes)?;
            bike.step(&terrain, control, DT)?;
            bike.stunt_controls(mask, axes, DT)?;
            let f = bike.frame();
            let riding = f.wreck.phase == Phase::Riding;
            if was_riding && !riding {
                wrecks += 1;
                kinds.push(format!("{:?}@{t:.2}s/{:.1}mph", f.wreck.kind, previous_mph));
            }
            if let Some(stunt) = &bike.stunt_name
                && stunts.last() != Some(stunt)
            {
                stunts.push(stunt.clone());
            }
            peak_impact = peak_impact.max(f.crash_impact_speed);
            previous_mph = f.speed / MPH;
            was_riding = riding;
            let (roll, pitch, heading) = angles(f.orientation);
            let yaw_rate = previous_yaw.map_or(0., |p: f32| {
                let mut d = heading - p;
                if d > std::f32::consts::PI {
                    d -= std::f32::consts::TAU;
                }
                if d < -std::f32::consts::PI {
                    d += std::f32::consts::TAU;
                }
                d / DT
            });
            previous_yaw = Some(heading);
            let b = dynamics::basis(f.orientation);
            let v = f.linear_velocity;
            // Sideslip: the angle between where the bike points and where it
            // is going, both on the ground plane.
            let flat_heading = [b[2][0], b[2][2]];
            let along = v[0] * flat_heading[0] + v[2] * flat_heading[1];
            let across = v[0] * flat_heading[1] - v[2] * flat_heading[0];
            let grounded = f.grounded.iter().any(|g| *g);
            let slip = if grounded && riding && along.abs() > 5. {
                across.atan2(along.abs()).to_degrees()
            } else {
                0.
            };
            if grounded {
                if flight > 0.2 {
                    flights.push(flight);
                }
                flight = 0.;
            } else if riding {
                flight += DT;
            }
            rows.push(json!({
                "t": t, "steer": control.steer, "throttle": control.throttle, "brake": control.brake,
                "mph": f.speed / MPH, "slip_deg": slip,
                "roll_deg": roll.to_degrees(), "pitch_deg": pitch.to_degrees(),
                "lean_deg": f.lean.to_degrees(), "rear_slide": f.rear_slide,
                "yaw_rate": yaw_rate, "steer_rad": f.steering_angle,
                "lateral_g": f.speed * yaw_rate / 32.2,
                "grounded": f.grounded, "compression": f.wheel_compression,
                "rpm": f.rpm, "gear": f.gear, "riding": riding,
                "x": f.origin[0], "y": f.origin[1], "z": f.origin[2],
                "stunt_timer": if bike.stunt_name.is_some() { bike.stunt_timer } else { 0. },
                "pelvis": f.rider_pelvis_offset,
                "feet": f.rider_foot_pins,
            }));
        }
        let col = |k: &str| -> Vec<f32> {
            rows.iter()
                .map(|r| r[k].as_f64().unwrap_or(0.) as f32)
                .collect()
        };
        let mph = col("mph");
        let pitch = col("pitch_deg");
        let roll = col("roll_deg");
        let rate = col("yaw_rate");
        let slip = col("slip_deg");
        let lateral = col("lateral_g");
        let range = |v: &[f32]| {
            [
                (v.iter().cloned().fold(f32::MAX, f32::min) * 10.).round() / 10.,
                (v.iter().cloned().fold(f32::MIN, f32::max) * 10.).round() / 10.,
            ]
        };
        let at = |v: &[f32], s: f32| (v[((s / DT) as usize).min(v.len() - 1)] * 10.).round() / 10.;
        let jitter = |v: &[f32]| {
            v.windows(3)
                .map(|w| (w[2] - 2. * w[1] + w[0]).abs())
                .sum::<f32>()
                / v.len() as f32
        };
        let airborne = rows
            .iter()
            .filter(|r| r["grounded"] == json!([false, false]))
            .count() as f32
            / rows.len() as f32;
        // Steady-state numbers over the last two seconds.
        let tail = rows.len().saturating_sub(120);
        let mean = |v: &[f32]| v[tail..].iter().sum::<f32>() / (v.len() - tail).max(1) as f32;
        results.push(json!({
            "script": name,
            "wrecks": wrecks,
            "wreck_kinds": kinds,
            "stunts": stunts,
            "peak_impact": peak_impact.round(),
            "mph_at": [at(&mph,2.),at(&mph,4.),at(&mph,6.),at(&mph,scenario.seconds-0.1)],
            "max_mph": mph.iter().cloned().fold(0., f32::max).round(),
            "slip_deg_range": range(&slip),
            "roll_deg_range": range(&roll),
            "pitch_deg_range": range(&pitch),
            "yaw_rate_range": range(&rate),
            "lateral_g_range": range(&lateral),
            "steady": {
                "mph": (mean(&mph) * 10.).round() / 10.,
                "yaw_rate": (mean(&rate) * 100.).round() / 100.,
                "roll_deg": (mean(&roll) * 10.).round() / 10.,
                "slip_deg": (mean(&slip) * 10.).round() / 10.,
                "lateral_g": (mean(&lateral) * 100.).round() / 100.,
            },
            "pitch_jitter": jitter(&pitch), "roll_jitter": jitter(&roll), "yaw_rate_jitter": jitter(&rate),
            "airborne_fraction": (airborne * 1000.).round() / 1000.,
            "flights_s": flights.iter().map(|f| (f * 100.).round() / 100.).collect::<Vec<_>>(),
            "rows": rows,
        }));
    }
    Ok(json!({"bike": id, "classic": classic, "mode": mode.name(), "dt": DT, "results": results}))
}
