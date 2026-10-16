//! Separate two-wheel ABI integration. Shares scene resources/presentation with
//! the ATV game; does not invoke the four-wheel ATV solver for motorcycles.
use super::*;
use atv_core::motorcycle::{Model, Motorcycle};
pub fn menu(input: &[u8], id: usize) -> Result<Vec<u8>> {
    let model = Model::load(
        input,
        u16::try_from(id).map_err(|_| "invalid motorcycle id")?,
    )?;
    let mut clips = vec![];
    for idle in 0..=3 {
        let breathe = "ui_rider_select_breathing";
        clips.push((
            format!("{breathe}.mot"),
            model
                .motions
                .get(breathe)
                .ok_or("missing MX menu breathing")?
                .clone(),
        ));
        if idle > 0 {
            let name = format!("ui_rider_select_idle{idle}");
            clips.push((
                format!("{name}.mot"),
                model
                    .motions
                    .get(&name)
                    .ok_or("missing MX menu idle")?
                    .clone(),
            ));
        }
    }
    let rider = atv_core::menu_preview::Rider::from_clips(model.rider, clips)?;
    let (clip, pose) = rider.pose(0.)?;
    let output =
        serde_json::to_vec(&serde_json::json!({"mesh":rider.mesh,"pose":pose,"clip":clip}))
            .map_err(|e| e.to_string())?;
    MENU_RIDER.with_borrow_mut(|slot| *slot = Some(rider));
    Ok(output)
}
pub fn load(input: &[u8], id: usize, install: bool) -> Result<Vec<u8>> {
    let id = u16::try_from(id).map_err(|_| "invalid motorcycle id")?;
    let model = Model::load(input, id)?;
    let duration = model.motions["mx_turn_still_mid"].duration;
    let pose = model.rider_pose("mx_turn_still_mid", duration * 0.5)?;
    let output=serde_json::to_vec(&serde_json::json!({"mesh":model.mesh,"riderMesh":model.rider,"riderPose":{"world_matrices":pose},"class":model.size,"pro":model.pro,"particleConfig":model.effects_config})).map_err(|e|e.to_string())?;
    if install {
        SIMULATION.with_borrow_mut(|sim| sim.motorcycle_model = Some(model));
    }
    Ok(output)
}
/// The bike feeds the original ATV2 follow/wreck cameras.
fn camera_actor(sim: &Simulation) -> Result<CameraActor> {
    let bike = sim.motorcycle.as_ref().ok_or("start motorcycle first")?;
    let f = bike.frame();
    let rider = bike.rider_world();
    Ok(CameraActor {
        origin: f.origin,
        orientation: f.orientation,
        velocity: f.linear_velocity,
        yaw_rate: f.local_angular_velocity[1],
        heading: None,
        grounded: f.grounded.iter().any(|v| *v),
        rider: (f.wreck.phase != atv_core::wreck::Phase::Riding)
            .then(|| ([rider[12], rider[13], rider[14]], bike.rider_velocity())),
    })
}
fn frame(sim: &mut Simulation, raw: &serde_json::Value) -> Result<serde_json::Value> {
    let bike = sim.motorcycle.as_ref().ok_or("start motorcycle first")?;
    let f = bike.frame();
    let pose = bike.matrices();
    let rider = bike.rider_matrices()?;
    let world = bike.world();
    let rider_world = bike.rider_world();
    let mut value = serde_json::to_value(f).map_err(|e| e.to_string())?;
    value["quad_node_matrices"] = serde_json::json!(pose);
    value["rider_node_matrices"] = serde_json::json!(rider);
    value["rider_grip_errors"] = serde_json::json!(bike.grip_errors(&rider, &pose));
    value["rider_foot_errors"] = serde_json::json!(bike.foot_errors(&rider, &pose));
    value["front_wheel_local_position"] = serde_json::json!(bike.render_front_center(&pose));
    if f.wreck.phase != atv_core::wreck::Phase::Riding {
        value["rider_world_matrix"] = serde_json::json!(rider_world);
    }
    value["applied_input"] = raw.clone();
    value["rider_mode"] = serde_json::json!(bike.motion_name);
    value["hud"] = serde_json::to_value(sim.hud.frame()).map_err(|e| e.to_string())?;
    value["wheel_particles"] =
        serde_json::to_value(sim.motorcycle_effects.frame(bike.time, f.reset_count))
            .map_err(|e| e.to_string())?;
    value["motorcycle_tick"] = serde_json::json!(bike.tick);
    value["impact_sample"] = bike
        .model
        .audio_config
        .impact_samples
        .first()
        .filter(|_| f.crash_impact_speed >= 18.8)
        .map_or(serde_json::Value::Null, |name| serde_json::json!(name));
    value["rider_audio_position"] = serde_json::json!(f.origin);
    value["stunt"] = bike.stunt_name.as_ref().map_or(
        serde_json::Value::Null,
        |name| serde_json::json!({"name":name,"main_time":bike.stunt_timer}),
    );
    let terrain = sim.terrain.as_ref().ok_or("load terrain first")?;
    sim.funny_crashes.set_mode(
        raw.get("crash_mode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
        raw.get("funny_crashes")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
    );
    sim.funny_crashes.set_rider_velocity(bike.rider_velocity());
    value["funny_crash"] = serde_json::to_value(sim.funny_crashes.update(
        bike.tick,
        f.reset_count,
        1. / 60.,
        f.wreck.phase != atv_core::wreck::Phase::Riding,
        f.wreck.phase != atv_core::wreck::Phase::Riding,
        f.linear_velocity,
        f.crash_impact_speed,
        None,
        world,
        &pose,
        &bike.model.mesh.nodes,
        rider_world,
        &rider,
        &bike.model.rider.nodes,
        |p| {
            terrain
                .contact(p[0], p[2])
                .ok()
                .flatten()
                .map(|h| (h.height, h.normal))
        },
    ))
    .map_err(|e| e.to_string())?;
    Ok(value)
}
pub fn dispatch(input: &[u8], op: u32) -> Result<Vec<u8>> {
    let source: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    SIMULATION.with_borrow_mut(|sim| dispatch_sim(sim, &source, op))
}
/// The bike's own partitioned scene, built like the ATV's: static props,
/// vegetation proxies, mud categories and, once loaded, the moving train.
pub fn motorcycle_scene(
    sim: &Simulation,
    train: bool,
) -> Result<Option<atv_core::scene_collision::SceneCollision>> {
    let Some(config) = sim.scene_partition else {
        return Ok(None);
    };
    let mut scene = atv_core::scene_collision::SceneCollision::from_terrain(
        config,
        sim.obstacles.clone().unwrap_or_default(),
        sim.ecosystem.as_ref(),
        sim.terrain.as_ref().ok_or("load terrain first")?,
        false,
    )?;
    if sim.mud_obstacles.len() == sim.obstacles.as_ref().map_or(0, Vec::len) {
        scene.set_mud_obstacles(sim.mud_obstacles.clone())?;
    }
    if train {
        scene.install_dynamic(train_collision_objects(sim)?)?;
    }
    Ok(Some(scene))
}
fn dispatch_sim(sim: &mut Simulation, source: &serde_json::Value, op: u32) -> Result<Vec<u8>> {
    if op == 23 {
        if let Some(model) = sim.motorcycle_model.take() {
            let origin: [f32; 3] = serde_json::from_value(
                source
                    .get("origin")
                    .ok_or("missing motorcycle origin")?
                    .clone(),
            )
            .map_err(|e| e.to_string())?;
            let mut bike = Motorcycle::new(model, origin, json_number(source, "yaw")?)?;
            bike.set_scene(motorcycle_scene(sim, sim.train_collision_installed)?);
            sim.motorcycle = Some(bike);
        } else {
            let bike = sim.motorcycle.as_mut().ok_or("load motorcycle first")?;
            // Like the ATV start, an explicit pose places the bike there;
            // otherwise reset to the race checkpoint or the start.
            match source.get("origin") {
                Some(origin) => {
                    let origin: [f32; 3] =
                        serde_json::from_value(origin.clone()).map_err(|e| e.to_string())?;
                    bike.place(origin, json_number(source, "yaw")?)?;
                }
                None => bike.reset_now()?,
            }
        }
        let bike = sim.motorcycle.as_mut().ok_or("missing motorcycle")?;
        bike.set_obstacles(sim.obstacles.clone().unwrap_or_default());
        bike.set_materials(sim.materials);
        if let Some(config) = sim.boundary_config {
            bike.set_boundary(config, sim.extended_boundary)?;
        }
        for _ in 0..30 {
            bike.step(
                sim.terrain.as_ref().ok_or("load terrain first")?,
                VehicleInput::default(),
                1. / 60.,
            )?;
        }
        sim.placement_epoch = bike.frame().reset_count;
        if let Some(camera) = &mut sim.camera {
            camera.reset();
        }
        sim.wreck_camera = None;
        sim.motorcycle_effects = atv_core::motorcycle_effects::Effects::default();
        sim.vehicle = None;
        sim.hud.reset();
        sim.browser_drive.reset();
        sim.input_generation = None;
        return serde_json::to_vec(&frame(sim, &serde_json::json!({}))?).map_err(|e| e.to_string());
    }
    if op == 46 {
        let output = follow_camera(
            sim,
            camera_actor(sim)?,
            json_number(source, "orbit")?,
            1. / 60.,
        )?;
        return serde_json::to_vec(&output).map_err(|e| e.to_string());
    }
    if source
        .get("cycle")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        sim.camera
            .as_mut()
            .ok_or("load original camera presets first")?
            .cycle();
    }
    let count = source
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .filter(|n| (1..=4).contains(n))
        .ok_or("invalid motorcycle batch")?;
    let raw = source.get("input").ok_or("missing motorcycle input")?;
    if source
        .get("skip_intro")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        super::skip_race_intro(sim);
    }
    let mut frames = Vec::new();
    for _ in 0..count {
        let mut driving: VehicleInput =
            serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        let generation = raw
            .get("input_generation")
            .and_then(serde_json::Value::as_u64);
        if generation != sim.input_generation {
            sim.browser_drive.reset();
            sim.input_generation = generation;
            if let Some(bike) = &mut sim.motorcycle {
                bike.cancel_controls();
            }
        }
        let intent = |key: &str| {
            raw.get(key)
                .filter(|v| !v.is_null())
                .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
                .transpose()
        };
        let bike = sim.motorcycle.as_mut().ok_or("start motorcycle first")?;
        driving = sim.browser_drive.resolve(
            driving,
            intent("keyboard_drive")?,
            intent("touch_drive")?,
            bike.frame().grounded.iter().any(|x| *x),
            bike.frame().speed,
        )?;
        if let Some(brake) = driving.reverse_chord_brake
            && bike.frame().speed.abs() > 5.
        {
            driving.reverse = false;
            driving.brake = driving.brake.max(brake);
        }
        // The countdown brake and the post-finish CPU driver replace the
        // player's controls, including tricks and the clutch.
        let imposed = super::race_controls(sim)?;
        let bike = sim.motorcycle.as_mut().ok_or("start motorcycle first")?;
        {
            let (mask, axes, clutch) = if let Some(input) = imposed {
                driving = input;
                (0, [0.; 2], false)
            } else {
                let mask = raw
                    .get("stunt_modifiers")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0)
                    .try_into()
                    .map_err(|_| "invalid motorcycle stunt mask")?;
                let axis = |name| {
                    raw.get(name)
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.) as f32
                };
                (
                    mask,
                    [axis("stunt_x"), axis("stunt_y")],
                    raw.get("clutch")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                )
            };
            bike.spin_controls(mask, axes)?;
            bike.clutch_controls(clutch);
            if let Some(water) = &mut sim.water {
                water.advance(1. / 60.)?;
            }
            bike.step(
                sim.terrain.as_ref().ok_or("load terrain first")?,
                driving,
                1. / 60.,
            )?;
            bike.stunt_controls(mask, axes, 1. / 60.)?;
            if let Some(water) = &mut sim.water {
                water.prepare_surface()?;
            }
        }
        let f = bike.frame();
        if let Some(name) = &bike.stunt_name {
            sim.hud.name_event(name);
        } else if f.wheelie_active {
            sim.hud.name_event("Wheelie");
        } else if f.stoppie_active {
            sim.hud.name_event("Stoppie");
        }
        sim.hud.step(
            1. / 60.,
            f.speed.abs(),
            f.preload_armed,
            f.preload_strength,
            false,
            0.,
        )?;
        if sim.placement_epoch != f.reset_count {
            if let Some(camera) = &mut sim.camera {
                camera.reset();
            }
            sim.wreck_camera = None;
            sim.motorcycle_effects = atv_core::motorcycle_effects::Effects::default();
            sim.placement_epoch = f.reset_count;
        }
        sim.motorcycle_effects.step(
            &bike.model.effects_config,
            f,
            &bike.world(),
            1. / 60.,
            sim.terrain.as_ref().ok_or("missing terrain")?,
        )?;
        let race = step_race(sim)?;
        let train = step_train_scene(sim, 1. / 60.)?;
        let water = sim.water.as_ref().map(atv_core::water_render::Frame::new);
        let camera = follow_camera(
            sim,
            camera_actor(sim)?,
            json_number(source, "orbit")?,
            1. / 60.,
        )?;
        let view = |key: &str| -> Result<[f32; 3]> {
            serde_json::from_value(camera[key].clone()).map_err(|e| e.to_string())
        };
        let (position, target) = (view("position")?, view("target")?);
        let mut scenery_uv = std::collections::BTreeMap::new();
        for (name, scroll) in &mut sim.scenery_uv {
            scenery_uv.insert(name.clone(), scroll.step(1. / 60.)?);
        }
        let mut scenery_textures = std::collections::BTreeMap::new();
        let milliseconds = (sim.scenery_tick * 1000 / 60) as u32;
        sim.scenery_tick += 1;
        for (name, clock) in &mut sim.scenery_textures {
            scenery_textures.insert(name.clone(), clock.tick(milliseconds));
        }
        let mut resolved = raw.clone();
        resolved["throttle"] = serde_json::json!(driving.throttle);
        resolved["steer"] = serde_json::json!(driving.steer);
        resolved["brake"] = serde_json::json!(driving.brake);
        resolved["reverse"] = serde_json::json!(driving.reverse);
        let driving_frame = frame(sim, &resolved)?;
        let ambient = sim
            .ambient
            .as_mut()
            .map(|effects| effects.step(1. / 60.).cloned())
            .transpose()?;
        let snow = if let Some(snow) = sim.snow.as_mut() {
            Some(snow.step(position, std::array::from_fn(|i| target[i] - position[i]))?)
        } else {
            None
        };
        frames.push(serde_json::json!({"frame":driving_frame,"camera":camera,"water":water,"train":train,"race":race,"scenery_uv":scenery_uv,"scenery_textures":scenery_textures,"ambient":ambient,"snow":snow}));
    }
    serde_json::to_vec(&frames).map_err(|e| e.to_string())
}
