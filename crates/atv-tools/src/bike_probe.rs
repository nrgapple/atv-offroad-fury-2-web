//! Native motorcycle handling probe: scripted inputs on a real map, with
//! per-step telemetry and summary metrics. Reads the supplied private bike and
//! terrain archives; it writes measurements only, never game content.
use atv_core::{
    Result, dynamics,
    motorcycle::{Model, Motorcycle},
    resource::Resource,
    terrain::{self, Terrain},
    vehicle::VehicleInput,
    wreck::Phase,
};
use serde_json::{Value, json};

const DT: f32 = 1. / 60.;
const MPH: f32 = 1.46666;

/// A level 4096 ft test ground (height 0), isolating handling from map bumps.
fn flat() -> Terrain {
    Terrain {
        spacing: 256.,
        tiles: vec![terrain::TerrainTile {
            tag: 0,
            heights: [0; 289],
            attributes: [u32::from_le_bytes([0, 127, 0, 0]); 289],
        }],
        leaves: vec![terrain::TerrainLeaf {
            x: 0,
            z: 0,
            tile: 0,
            height_morph: 1.,
            height_scale: 1. / 64.,
            minimum_height: 0.,
            maximum_height: 100.,
        }],
        hierarchy_nodes: 1,
        width_cells: 16,
        depth_cells: 16,
    }
}
fn terrain_from(res: &[u8]) -> Result<Terrain> {
    if res == b"flat" {
        return Ok(flat());
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

type Script = fn(f32, &Motorcycle) -> VehicleInput;
fn input(throttle: f32, brake: f32, steer: f32, lean: f32) -> VehicleInput {
    VehicleInput {
        throttle,
        brake,
        steer,
        lean,
        ..VehicleInput::default()
    }
}
fn speed(b: &Motorcycle) -> f32 {
    b.frame().speed / MPH
}

fn scripts() -> Vec<(&'static str, f32, Script)> {
    vec![
        ("launch", 10., |_, _| input(1., 0., 0., 0.)),
        ("turn-30mph-half", 12., |t, b| {
            let hold = speed(b) < 30.;
            input(
                if hold { 1. } else { 0.35 },
                0.,
                if t > 5. { 0.5 } else { 0. },
                0.,
            )
        }),
        ("turn-30mph-full", 12., |t, b| {
            let hold = speed(b) < 30.;
            input(
                if hold { 1. } else { 0.35 },
                0.,
                if t > 5. { 1. } else { 0. },
                0.,
            )
        }),
        ("slalom-40mph", 14., |t, b| {
            let hold = speed(b) < 40.;
            let steer = if t > 5. { (t * 2.).sin() } else { 0. };
            input(if hold { 1. } else { 0.4 }, 0., steer, 0.)
        }),
        ("brake-from-45", 12., |t, b| {
            if t < 7. && speed(b) < 45. {
                input(1., 0., 0., 0.)
            } else if t < 7. {
                input(0.4, 0., 0., 0.)
            } else {
                input(0., 1., 0., 0.)
            }
        }),
        ("slow-full-lock", 10., |t, _| {
            input(0.25, 0., if t > 2. { 1. } else { 0. }, 0.)
        }),
        ("tap-steer-50mph", 14., |t, b| {
            let hold = speed(b) < 50.;
            let steer = if (8. ..8.4).contains(&t) { 1. } else { 0. };
            input(if hold { 1. } else { 0.45 }, 0., steer, 0.)
        }),
        ("wheelie", 10., |t, _| {
            input(1., 0., 0., if t > 4. { 1. } else { 0. })
        }),
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
    let (origin, yaw) = if map_res == b"flat" {
        ([2048., 2.5, 400.], 0.)
    } else {
        flat_start(&terrain)?
    };
    let mut results = vec![];
    for (name, duration, script) in scripts() {
        if filter.is_some_and(|f| f != name) {
            continue;
        }
        let model = Model::load(bike_res, id)?;
        let mut bike = Motorcycle::new(model, origin, yaw)?;
        bike.set_materials(materials);
        let mut rows = vec![];
        let mut wrecks = 0;
        let mut was_riding = true;
        let mut previous_yaw = None;
        let steps = (duration / DT) as usize;
        for i in 0..steps {
            let t = i as f32 * DT;
            let control = script(t, &bike);
            bike.step(&terrain, control, DT)?;
            let f = bike.frame();
            let riding = f.wreck.phase == Phase::Riding;
            if was_riding && !riding {
                wrecks += 1;
            }
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
            let lateral = f.linear_velocity[0] * b[0][0]
                + f.linear_velocity[1] * b[0][1]
                + f.linear_velocity[2] * b[0][2];
            rows.push(json!({
                "t": t, "steer": control.steer, "throttle": control.throttle, "brake": control.brake,
                "mph": f.speed / MPH, "lateral_fps": lateral,
                "roll_deg": roll.to_degrees(), "pitch_deg": pitch.to_degrees(),
                "yaw_rate": yaw_rate, "steer_rad": f.steering_angle,
                "grounded": f.grounded, "compression": f.wheel_compression,
                "rpm": f.rpm, "gear": f.gear, "riding": riding,
                "y": f.origin[1],
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
        let at = |v: &[f32], s: f32| v[((s / DT) as usize).min(v.len() - 1)];
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
        results.push(json!({
            "script": name,
            "wrecks": wrecks,
            "mph_at": [at(&mph,2.),at(&mph,4.),at(&mph,6.),at(&mph,duration-0.1)],
            "max_mph": mph.iter().cloned().fold(0., f32::max),
            "pitch_range_deg": [pitch.iter().cloned().fold(f32::MAX,f32::min), pitch.iter().cloned().fold(f32::MIN,f32::max)],
            "roll_range_deg": [roll.iter().cloned().fold(f32::MAX,f32::min), roll.iter().cloned().fold(f32::MIN,f32::max)],
            "yaw_rate_range": [rate.iter().cloned().fold(f32::MAX,f32::min), rate.iter().cloned().fold(f32::MIN,f32::max)],
            "pitch_jitter": jitter(&pitch), "roll_jitter": jitter(&roll), "yaw_rate_jitter": jitter(&rate),
            "airborne_fraction": airborne,
            "rows": rows,
        }));
    }
    Ok(json!({"bike": id, "start": origin, "yaw": yaw, "dt": DT, "results": results}))
}
