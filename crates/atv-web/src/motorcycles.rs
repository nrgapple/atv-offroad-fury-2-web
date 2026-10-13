//! Separate two-wheel ABI integration. Shares scene resources/presentation with
//! the ATV game; does not invoke the four-wheel ATV solver for motorcycles.
use super::*;
use atv_core::motorcycle::{Model, Motorcycle};
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
    sim.funny_crashes.set_enabled(
        raw.get("funny_crashes")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
    );
    value["funny_crash"] = serde_json::to_value(sim.funny_crashes.update(
        bike.tick,
        f.reset_count,
        1. / 60.,
        f.wreck.phase != atv_core::wreck::Phase::Riding,
        f.wreck.phase != atv_core::wreck::Phase::Riding,
        f.linear_velocity,
        f.crash_impact_speed,
        world,
        &pose,
        &bike.model.mesh.nodes,
        rider_world,
        &rider,
        &bike.model.rider.nodes,
        |p| terrain.contact(p[0], p[2]).ok().flatten().map(|h| h.height),
    ))
    .map_err(|e| e.to_string())?;
    Ok(value)
}
pub fn dispatch(input: &[u8], op: u32) -> Result<Vec<u8>> {
    let source: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    SIMULATION.with_borrow_mut(|sim| dispatch_sim(sim, &source, op))
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
            sim.motorcycle = Some(Motorcycle::new(model, origin, json_number(source, "yaw")?)?);
        } else {
            sim.motorcycle
                .as_mut()
                .ok_or("load motorcycle first")?
                .reset()?;
        }
        let bike = sim.motorcycle.as_mut().ok_or("missing motorcycle")?;
        bike.set_obstacles(sim.obstacles.clone().unwrap_or_default());
        for _ in 0..30 {
            bike.step(
                sim.terrain.as_ref().ok_or("load terrain first")?,
                VehicleInput::default(),
                1. / 60.,
            )?;
        }
        sim.placement_epoch = bike.frame().reset_count;
        sim.motorcycle_effects = atv_core::motorcycle_effects::Effects::default();
        sim.vehicle = None;
        sim.hud.reset();
        sim.touch_drive.reset();
        sim.input_generation = None;
        return serde_json::to_vec(&frame(sim, &serde_json::json!({}))?).map_err(|e| e.to_string());
    }
    if op == 46 {
        return serde_json::to_vec(
            &sim.motorcycle
                .as_ref()
                .ok_or("start motorcycle first")?
                .camera(json_number(source, "orbit")?),
        )
        .map_err(|e| e.to_string());
    }
    if op == 47 {
        sim.motorcycle
            .as_mut()
            .ok_or("start motorcycle first")?
            .cycle_camera(
                source
                    .get("cycle")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            );
        return Ok(b"{}".to_vec());
    }
    if source
        .get("cycle")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        sim.motorcycle
            .as_mut()
            .ok_or("start motorcycle first")?
            .cycle_camera(true);
    }
    let count = source
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .filter(|n| (1..=4).contains(n))
        .ok_or("invalid motorcycle batch")?;
    let raw = source.get("input").ok_or("missing motorcycle input")?;
    let mut frames = Vec::new();
    for _ in 0..count {
        let mut driving: VehicleInput =
            serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        let generation = raw
            .get("input_generation")
            .and_then(serde_json::Value::as_u64);
        if generation != sim.input_generation {
            sim.touch_drive.reset();
            sim.input_generation = generation;
            if let Some(bike) = &mut sim.motorcycle {
                bike.cancel_controls();
            }
        }
        let intent = raw
            .get("touch_drive")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
            .transpose()?;
        let bike = sim.motorcycle.as_mut().ok_or("start motorcycle first")?;
        driving = sim.touch_drive.resolve(
            driving,
            intent,
            bike.frame().grounded.iter().any(|x| *x),
            bike.frame().speed,
        )?;
        if let Some(brake) = driving.reverse_chord_brake
            && bike.frame().speed.abs() > 5.
        {
            driving.reverse = false;
            driving.brake = driving.brake.max(brake);
        }
        let locked = sim.race.as_ref().is_some_and(|r| r.locked());
        if !locked {
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
            let axes = [axis("stunt_x"), axis("stunt_y")];
            bike.spin_controls(mask, axes)?;
            bike.clutch_controls(
                raw.get("clutch")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            );
            bike.step(
                sim.terrain.as_ref().ok_or("load terrain first")?,
                driving,
                1. / 60.,
            )?;
            bike.stunt_controls(mask, axes, 1. / 60.)?;
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
        let race = if let Some(race) = &mut sim.race {
            race.step(f.origin, 1. / 60.);
            let b = atv_core::dynamics::basis(f.orientation);
            let mut state = race.frame(b[2][0].atan2(b[2][2]));
            for cpu in &mut state.opponents {
                cpu.origin[1] = sim
                    .terrain
                    .as_ref()
                    .ok_or("missing terrain")?
                    .contact(cpu.origin[0], cpu.origin[2])?
                    .map_or(f.origin[1], |h| h.height + 0.1);
            }
            Some(state)
        } else {
            None
        };
        let train = step_train_scene(sim, 1. / 60.)?;
        let water = sim.water.as_ref().map(atv_core::water_render::Frame::new);
        let camera = sim
            .motorcycle
            .as_ref()
            .ok_or("missing motorcycle")?
            .camera(json_number(source, "orbit")?);
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
        resolved["throttle"] = serde_json::json!(if locked { 0. } else { driving.throttle });
        resolved["brake"] = serde_json::json!(driving.brake);
        resolved["reverse"] = serde_json::json!(driving.reverse);
        let driving_frame = frame(sim, &resolved)?;
        frames.push(serde_json::json!({"frame":driving_frame,"camera":camera,"water":water,"train":train,"race":race,"scenery_uv":scenery_uv,"scenery_textures":scenery_textures}));
    }
    serde_json::to_vec(&frames).map_err(|e| e.to_string())
}
