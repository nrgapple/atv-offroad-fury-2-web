//! Small single-threaded Wasm ABI. Buffers are Rust-owned, initialized, and bounded.
//! JS copies into input_ptr only after input_resize and copies output before another call.
use atv_core::{
    Result, animation, audio, elf,
    engine_audio::{EngineAudioConfig, EngineAudioState, OriginalRandom, sample_index},
    iso,
    materials::{self, TerrainMaterial},
    mesh,
    physics_data::PhysicsData,
    resource::Resource,
    terrain, texture,
    vehicle::{Vehicle, VehicleInput},
};
use std::cell::RefCell;
mod motorcycles;
mod multiplayer;
thread_local! { static MENU_RIDER: RefCell<Option<atv_core::menu_preview::Rider>> = const { RefCell::new(None) }; }

#[derive(Default)]
struct Buffers {
    input: Vec<u8>,
    output: Vec<u8>,
}
thread_local! { static BUFFERS: RefCell<Buffers> = RefCell::default(); }

#[derive(Default)]
struct Simulation {
    funny_crashes: atv_core::crash_effects::Effects,
    terrain: Option<terrain::Terrain>,
    physics: Option<PhysicsData>,
    mesh: Option<mesh::Mesh>,
    materials: Option<[TerrainMaterial; 8]>,
    vehicle: Option<Vehicle>,
    motorcycle_model: Option<atv_core::motorcycle::Model>,
    motorcycle: Option<atv_core::motorcycle::Motorcycle>,
    motorcycle_effects: atv_core::motorcycle_effects::Effects,
    touch_drive: atv_core::browser_input::TouchDrive,
    input_generation: Option<u64>,
    resolved_input: Option<serde_json::Value>,
    rider_mesh: Option<mesh::Mesh>,
    rider_animation: Option<animation::Animation>,
    rider_catalog: Vec<atv_core::motion_catalog::Motion>,
    rider_motions: Vec<atv_core::rider_pose::Motion>,
    rider_animator: Option<atv_core::rider_pose::Animator>,
    rider_mode: Option<atv_core::rider_pose::Mode>,
    wreck_animator: Option<atv_core::wreck_animation::Animator>,
    chassis_points: Option<Vec<[f32; 3]>>,
    wreck_rays: Option<std::rc::Rc<atv_core::collision::CollisionRayTree>>,
    quad_pose: Option<std::rc::Rc<atv_core::quad_pose::QuadPose>>,
    obstacle_rays: Option<std::rc::Rc<atv_core::collision::CollisionRayTree>>,
    obstacle_meshes: Vec<std::rc::Rc<atv_core::collision::CollisionMesh>>,
    obstacles: Option<Vec<atv_core::obstacle::StaticObstacle>>,
    mud_obstacles: Vec<bool>,
    scene_partition: Option<atv_core::scene_partition::Config>,
    ecosystem: Option<atv_core::ecosystem::Ecosystem>,
    boundary_config: Option<atv_core::boundary_launch::Config>,
    crowd_present: bool,
    water: Option<atv_core::water_animation::LiveField>,
    camera: Option<atv_core::camera::FollowCamera>,
    train: Option<atv_core::train::Runtime>,
    scenery_uv: Vec<(String, atv_core::scenery_material::Scroll)>,
    scenery_textures: Vec<(String, atv_core::scenery_material::TextureClock)>,
    scenery_tick: u64,
    train_collision: Vec<TrainCollisionBinding>,
    train_collision_installed: bool,
    effects_time: f32,
    rider_audio_position: [f32; 3],
    effects_dt: f32,
    effects_tick: u64,
    placement_epoch: u32,
    wreck_camera: Option<atv_core::camera::WreckCamera>,
    hud: atv_core::hud::State,
    race_course: Option<atv_core::race::Course>,
    race: Option<atv_core::race::Race>,
    cpu_racers: Vec<CpuRacer>,
}
struct CpuRacer {
    vehicle: Vehicle,
    driver: atv_core::race_ai::Driver,
    rider: atv_core::rider_pose::Animator,
}
fn create_cpu_racer(
    sim: &Simulation,
    mut origin: [f32; 3],
    yaw: f32,
    index: usize,
    difficulty: u32,
) -> Result<CpuRacer> {
    let terrain = sim.terrain.as_ref().ok_or("CPU terrain missing")?;
    origin[1] = terrain
        .contact(origin[0], origin[2])?
        .ok_or("CPU grid outside terrain")?
        .height
        + 2.5;
    let mut vehicle = Vehicle::new(
        sim.mesh.as_ref().ok_or("CPU mesh missing")?,
        sim.physics.as_ref().ok_or("CPU physics missing")?,
        origin,
        yaw,
    )?;
    vehicle.set_linkage(sim.quad_pose.as_ref().ok_or("CPU linkage missing")?.clone());
    vehicle.set_materials(sim.materials.ok_or("CPU materials missing")?);
    vehicle.set_chassis_points(sim.chassis_points.as_ref().ok_or("CPU chassis missing")?)?;
    vehicle.set_obstacles(
        sim.obstacle_rays
            .as_ref()
            .ok_or("CPU rays missing")?
            .clone(),
        sim.obstacles.as_ref().ok_or("CPU scenery missing")?.clone(),
    )?;
    if let Some(config) = sim.scene_partition {
        vehicle.set_scene_partition(config, sim.ecosystem.as_ref(), terrain)?;
    }
    if let Some(scene) = vehicle.scene_collision_mut() {
        scene.set_mud_obstacles(sim.mud_obstacles.clone())?;
    }
    let rider = sim
        .rider_animator
        .as_ref()
        .ok_or("CPU rider motions missing")?
        .fork_normal(
            &sim.rider_mesh
                .as_ref()
                .ok_or("CPU rider mesh missing")?
                .nodes,
        )?;
    Ok(CpuRacer {
        vehicle,
        rider,
        driver: atv_core::race_ai::Driver::new(index, difficulty),
    })
}
struct TrainCollisionBinding {
    car: usize,
    mesh: std::rc::Rc<atv_core::collision::CollisionMesh>,
}
thread_local! { static SIMULATION: RefCell<Simulation> = RefCell::default(); }

struct EnginePlayback {
    config: EngineAudioConfig,
    state: EngineAudioState,
    previous: [usize; 10],
    random: OriginalRandom,
}
thread_local! { static WHEEL_EFFECTS: RefCell<Option<atv_core::wheel_effects::Effects>> = const { RefCell::new(None) }; }
fn configure_wheel_effects(input: &[u8]) -> Result<Vec<u8>> {
    if input.len() > 2 * 1024 * 1024 {
        return Err("wheel effect configuration exceeds limit".into());
    }
    let source: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    let resource = |key| {
        source
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("missing wheel effect resource {key}"))
    };
    let mut config = atv_core::wheel_effects::Config::parse(
        resource("prt")?.as_bytes(),
        resource("dust")?.as_bytes(),
        resource("chunk")?.as_bytes(),
        resource("spray")?.as_bytes(),
    )?;
    if let Some(scene) = source.get("scene").and_then(serde_json::Value::as_str) {
        let scene = atv_core::scene::parse_with_start(
            scene.as_bytes(),
            source
                .get("mode")
                .and_then(serde_json::Value::as_str)
                .map(str::as_bytes),
        )?;
        let ambient = scene
            .lights
            .iter()
            .find(|l| l.kind == "Ambient")
            .map(|l| l.color)
            .unwrap_or([0.; 3]);
        let directional = scene
            .lights
            .iter()
            .find(|l| l.kind == "Directional")
            .map(|l| l.color)
            .unwrap_or([0.; 3]);
        config.light_color = std::array::from_fn(|i| ambient[i] + directional[i] * 0.5);
    }
    let result = serde_json::to_vec(&config).map_err(|e| e.to_string())?;
    WHEEL_EFFECTS
        .with_borrow_mut(|slot| *slot = Some(atv_core::wheel_effects::Effects::new(config)));
    Ok(result)
}
fn wheel_effects_frame(
    frame: &serde_json::Value,
    terrain: Option<&terrain::Terrain>,
) -> Result<serde_json::Value> {
    WHEEL_EFFECTS.with_borrow_mut(|slot| {
        let Some(effects) = slot.as_mut() else {
            return Ok(serde_json::Value::Null);
        };
        let Some(mut snapshot) = frame.get("wheel_effects").filter(|v| !v.is_null()).cloned()
        else {
            return Ok(serde_json::Value::Null);
        };
        for key in [
            "grounded",
            "steering_angle",
            "linear_velocity",
            "reset_count",
        ] {
            snapshot[key] = frame[key].clone();
        }
        let input = serde_json::from_value(snapshot)
            .map_err(|e| format!("invalid wheel effect input: {e}"))?;
        let particles = effects.step_with_terrain(&input, |point| {
            terrain
                .map(|terrain| {
                    terrain
                        .contact(point[0], point[2])
                        .map(|hit| hit.map(|h| h.height))
                })
                .transpose()
                .map(Option::flatten)
        })?;
        serde_json::to_value(particles).map_err(|e| e.to_string())
    })
}
thread_local! { static ENGINE_AUDIO: RefCell<Option<EnginePlayback>> = const { RefCell::new(None) }; }

struct EffectPlayback {
    config: atv_core::sound_effects::Config,
    impact: atv_core::sound_effects::RiderImpactState,
    motion: atv_core::sound_effects::RiderMotionCache,
    random: OriginalRandom,
}
thread_local! { static SOUND_EFFECTS: RefCell<Option<EffectPlayback>> = const { RefCell::new(None) }; }
fn sound_effects(input: &[u8], op: u32) -> Result<Vec<u8>> {
    SOUND_EFFECTS.with_borrow_mut(|slot| {
        if op == 76 {
            multiplayer::effect_table(input)?;
            let config = atv_core::sound_effects::Config::parse(input)?;
            let result = serde_json::to_vec(&config).map_err(|e| e.to_string())?;
            *slot = Some(EffectPlayback {
                config,
                impact: Default::default(),
                motion: Default::default(),
                random: OriginalRandom::with_seed(1),
            });
            return Ok(result);
        }
        let player = slot.as_mut().ok_or("load original rider effects bank first")?;
        if op == 78 {
            player.impact = Default::default();
            return Ok(b"{}".to_vec());
        }
        if input.len() > 65536 {
            return Err("rider audio frame exceeds limit".into());
        }
        let batch: atv_core::sound_effects::BatchRequest =
            serde_json::from_slice(input).map_err(|e| e.to_string())?;
        if batch.frames.is_empty() || batch.frames.len() > 64 {
            return Err("invalid rider audio batch length".into());
        }
        let mut playing = batch.playing;
        let mut results = Vec::with_capacity(batch.frames.len());
        for mut request in batch.frames {
        request.impact.playing = playing;
        let listener = atv_core::spatial_audio::camera_listener(
            &request.camera,
            &request.previous_camera,
            request.dt,
        )?;
        let spatial = if let Some(position) = request.position {
            let velocity = player.motion.update(position, 1. / request.dt)?;
            Some(atv_core::spatial_audio::command(
                atv_core::spatial_audio::Source {
                    position,
                    velocity,
                    min_distance: player.config.min_distance,
                    max_distance: player.config.max_distance,
                },
                &[listener],
            )?)
        } else {
            None
        };
        let event = player.impact.advance(
            request.impact,
            player.config.body_impact.len(),
            || player.random.next_u31(),
        )?;
        if event.is_some_and(|event| event.play) { playing = true; }
        results.push(serde_json::json!({"event":event.map(|event|serde_json::json!({"sample":player.config.body_impact[event.sample],"magnitude":event.magnitude,"play":event.play})),"spatial":spatial,"latched":player.impact.latched}));
        }
        serde_json::to_vec(&results).map_err(|e|e.to_string())
    })
}

struct MusicPlayback {
    library: atv_core::music::MusicLibrary,
    shuffle: atv_core::music::MusicShuffle,
    // Initial RTC seed/global call order remain outside this reconstruction.
    random: OriginalRandom,
}
thread_local! {
    static MUSIC: RefCell<Option<MusicPlayback>> = const { RefCell::new(None) };
    static MUSIC_STREAM: RefCell<Option<audio::VagStream>> = const { RefCell::new(None) };
}
fn music(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    if op == 42 {
        let stream = audio::VagStream::new(input, index)?;
        let result = serde_json::to_vec(&stream.info).map_err(|e| e.to_string())?;
        MUSIC_STREAM.with_borrow_mut(|slot| *slot = Some(stream));
        return Ok(result);
    }
    if op == 43 {
        return MUSIC_STREAM.with_borrow_mut(|slot| {
            let samples = slot
                .as_mut()
                .ok_or("initialize music stream first")?
                .decode_chunk(input)?;
            Ok(samples.iter().flat_map(|s| s.to_le_bytes()).collect())
        });
    }
    MUSIC.with_borrow_mut(|slot| {
        if op == 40 {
            let library = atv_core::music::MusicLibrary::parse(input)?;
            let bytes = serde_json::to_vec(&library).map_err(|e| e.to_string())?;
            // Changing maps reloads the same catalog. Keep its original shuffle
            // history instead of restarting at the deterministic first track.
            if slot.as_ref().is_none_or(|music| music.library != library) {
                let shuffle = atv_core::music::MusicShuffle::new(&library);
                *slot = Some(MusicPlayback {
                    library,
                    shuffle,
                    random: OriginalRandom::with_seed(1),
                });
            }
            Ok(bytes)
        } else {
            let music = slot.as_mut().ok_or("load original music catalog first")?;
            let song = if input.is_empty() {
                music.shuffle.next_song(&mut music.random)?
            } else {
                let request: serde_json::Value =
                    serde_json::from_slice(input).map_err(|e| e.to_string())?;
                let fields = request.as_object().ok_or("invalid music selection")?;
                if fields.len() != 1 {
                    return Err("invalid music selection fields".into());
                }
                let selection = fields
                    .get("select")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or("invalid music selection index")?;
                music.shuffle.select(selection)?;
                Some(selection)
            };
            serde_json::to_vec(&song.map(|i| &music.library.songs[i])).map_err(|e| e.to_string())
        }
    })
}

fn engine_audio(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    ENGINE_AUDIO.with_borrow_mut(|slot| {
        if matches!(op,32|101) {
            if op==32 {multiplayer::audio_table(input);}
            let config = if op==101 {EngineAudioConfig::parse_mx(input)?}else{EngineAudioConfig::parse(input,index as u32)?};
            let bytes = serde_json::to_vec(&config).map_err(|e| e.to_string())?;
            *slot = Some(EnginePlayback { config, state: EngineAudioState::default(), previous: [usize::MAX; 10], random: OriginalRandom::with_seed(1) });
            return Ok(bytes);
        }
        let player = slot.as_mut().ok_or("load original engine audio table first")?;
        if op == 34 {
            player.state = EngineAudioState::default();
            player.previous = [usize::MAX; 10];
            return Ok(b"null".to_vec());
        }
        let value: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        let command = player.state.step(&player.config, json_number(&value,"dt")?,
            json_number(&value,"throttle")?, value.get("grounded").and_then(serde_json::Value::as_bool).ok_or("missing audio grounded")?,
            json_number(&value,"speed")?, atv_core::engine_audio::PlaybackPosition {duration: json_number(&value,"duration")?, playhead: json_number(&value,"playhead")?})?;
        if let Some(command) = command {
            let sound = command.sound as usize;
            let count = player.config.samples[sound].len();
            if count == 0 { return Ok(b"null".to_vec()); }
            let choice = if matches!(sound, 2..=6) {
                let random = if count >= 2 { player.random.next_u31() } else { 0 };
                sample_index(count, &mut player.previous[sound], random)?
            } else { 0 };
            serde_json::to_vec(&serde_json::json!({"command": command, "sample": player.config.samples[sound][choice]})).map_err(|e| e.to_string())
        } else { Ok(b"null".to_vec()) }
    })
}

fn json_number(value: &serde_json::Value, key: &str) -> Result<f32> {
    let number = value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| format!("missing simulation number {key}"))? as f32;
    if !number.is_finite() {
        return Err(format!("nonfinite simulation number {key}"));
    }
    Ok(number)
}

fn vehicle_world(frame: &atv_core::vehicle::VehicleFrame) -> [f32; 16] {
    let b = atv_core::dynamics::basis(frame.orientation);
    let p = frame.origin;
    [
        b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1], b[2][2],
        0., p[0], p[1], p[2], 1.,
    ]
}

fn simulation_frame(
    sim: &mut Simulation,
    frame: &atv_core::vehicle::VehicleFrame,
) -> Result<Vec<u8>> {
    multiplayer::observe_wreck(frame.wreck.phase != atv_core::wreck::Phase::Riding);
    let matrices = if let Some(matrices) = sim.vehicle.as_ref().and_then(|v| v.linkage_matrices()) {
        std::borrow::Cow::Borrowed(matrices)
    } else {
        std::borrow::Cow::Owned(
            sim.quad_pose
                .as_ref()
                .ok_or("load original quad linkage first")?
                .sample(
                    frame.visual_steering_angle,
                    frame.wheel_angles,
                    frame.wheel_compression,
                )?,
        )
    };
    let mut value = serde_json::to_value(frame).map_err(|error| error.to_string())?;
    if let (Some(vehicle), Some(pose), Some(terrain)) = (&sim.vehicle, &sim.quad_pose, &sim.terrain)
    {
        let mut effects =
            serde_json::to_value(vehicle.wheel_effect_state()).map_err(|e| e.to_string())?;
        effects["time"] = serde_json::json!(sim.effects_time);
        effects["dt"] = serde_json::json!(sim.effects_dt);
        effects["tick"] = serde_json::json!(sim.effects_tick);
        effects["placement_epoch"] = serde_json::json!(sim.placement_epoch);
        let world = vehicle_world(frame);
        let centers = pose
            .wheel_positions(&matrices)?
            .map(|point| atv_core::obstacle::transform_point(&world, point));
        let terrain_heights = centers
            .iter()
            .map(|point| {
                terrain
                    .contact(point[0], point[2])
                    .map(|hit| hit.map_or(point[1], |hit| hit.height))
            })
            .collect::<Result<Vec<_>>>()?;
        let water_heights = centers
            .iter()
            .map(|point| {
                sim.water
                    .as_ref()
                    .map_or(Ok(-256.), |water| water.height(*point, false))
            })
            .collect::<Result<Vec<_>>>()?;
        effects["centers"] = serde_json::json!(centers);
        effects["terrain_heights"] = serde_json::json!(terrain_heights);
        effects["water_heights"] = serde_json::json!(water_heights);
        value["wheel_effects"] = effects;
    }
    value["rider_audio_position"] = serde_json::json!(sim.rider_audio_position);
    value["wheel_particles"] = wheel_effects_frame(&value, sim.terrain.as_ref())?;
    value["hud"] = serde_json::to_value(sim.hud.frame()).map_err(|e| e.to_string())?;
    value["applied_input"] = sim
        .resolved_input
        .clone()
        .unwrap_or(serde_json::Value::Null);
    value["quad_node_matrices"] =
        serde_json::to_value(&matrices).map_err(|error| error.to_string())?;
    if let Some(animator) = &sim.rider_animator {
        let mesh = sim.rider_mesh.as_ref().ok_or("rider mesh missing")?;
        value["rider_node_matrices"] =
            serde_json::to_value(animator.pose(&mesh.nodes)?.world_matrices)
                .map_err(|e| e.to_string())?;
        value["rider_mode"] = serde_json::to_value(sim.rider_mode).map_err(|e| e.to_string())?;
        value["stunt"] = serde_json::to_value(animator.stunt_frame()).map_err(|e| e.to_string())?;
        value["stunt_score"] =
            serde_json::to_value(animator.score_frame()).map_err(|e| e.to_string())?;
        value["wheelie_score"] =
            serde_json::to_value(animator.wheelie_frame()).map_err(|e| e.to_string())?;
        value["wheelie_scoring_enabled"] =
            serde_json::json!(atv_core::stunt_score::DEFAULT_WHEELIE_SCORING);
        value["rider_landing"] =
            serde_json::to_value(animator.landing_frame()).map_err(|e| e.to_string())?;
    }
    if let Some(animator) = &sim.wreck_animator
        && let Some(wreck) = animator.frame()
    {
        let mesh = sim.rider_mesh.as_ref().ok_or("rider mesh missing")?;
        value["rider_node_matrices"] =
            serde_json::to_value(animator.pose(&mesh.nodes)?.world_matrices)
                .map_err(|e| e.to_string())?;
        value["rider_world_matrix"] =
            serde_json::to_value(wreck.world_matrix).map_err(|e| e.to_string())?;
        value["rider_wreck"] = serde_json::to_value(wreck).map_err(|e| e.to_string())?;
        value["rider_mode"] = serde_json::json!("wreck");
        value["stunt"] = serde_json::Value::Null;
    }
    if sim.funny_crashes.enabled()
        && let (Some(mesh), Some(rider_mesh), Some(terrain)) =
            (&sim.mesh, &sim.rider_mesh, &sim.terrain)
    {
        let rider_pose: Vec<[f32; 16]> =
            serde_json::from_value(value["rider_node_matrices"].clone())
                .map_err(|e| e.to_string())?;
        let rider_world = sim
            .wreck_animator
            .as_ref()
            .and_then(|a| a.frame())
            .map_or(vehicle_world(frame), |f| f.world_matrix);
        let detached = sim
            .wreck_animator
            .as_ref()
            .and_then(|a| a.frame())
            .is_some_and(|f| f.phase != atv_core::wreck_animation::Phase::Attached);
        value["funny_crash"] = serde_json::to_value(sim.funny_crashes.update(
            sim.effects_tick,
            frame.reset_count,
            sim.effects_dt,
            frame.wreck.phase != atv_core::wreck::Phase::Riding,
            detached,
            frame.linear_velocity,
            frame.crash_impact_speed,
            vehicle_world(frame),
            &matrices,
            &mesh.nodes,
            rider_world,
            &rider_pose,
            &rider_mesh.nodes,
            |p| terrain.contact(p[0], p[2]).ok().flatten().map(|h| h.height),
        ))
        .map_err(|e| e.to_string())?;
    }
    serde_json::to_vec(&value).map_err(|error| error.to_string())
}

///001e5260 scores wheel contact/body pitch after physics and wreck entry,
///including ongoing wreck frames, before the rider accepts airborne stunts.
fn update_wheelie(
    animator: Option<&mut atv_core::rider_pose::Animator>,
    frame: &atv_core::vehicle::VehicleFrame,
    dt: f32,
) -> Result<()> {
    if let Some(animator) = animator {
        animator.update_wheelie(
            dt,
            atv_core::stunt_score::WheelieInput {
                grounded: frame.grounded,
                pitch: atv_core::rider_control::body_angles(frame.orientation, f32::atan2)[0],
                //0020f370 initializes manager+13c to zero. The current active
                //Freestyle route retains that gate; end transitions are separate.
                context_allows: true,
                scoring_enabled: atv_core::stunt_score::DEFAULT_WHEELIE_SCORING,
            },
        )?;
    }
    Ok(())
}

fn update_hud(
    sim: &mut Simulation,
    frame: &atv_core::vehicle::VehicleFrame,
    dt: f32,
) -> Result<()> {
    let mut active = false;
    let mut points = 0.;
    if let Some(animator) = &mut sim.rider_animator {
        for event in animator.take_score_notifications() {
            match event {
                atv_core::stunt_score::Notification::Wheelie { event } => {
                    sim.hud.wheelie_event(event);
                }
                atv_core::stunt_score::Notification::WheelieClearedForStunt => {
                    sim.hud.clear_wheelie_for_stunt();
                }
                atv_core::stunt_score::Notification::DisplayUpdated { id, .. } => {
                    sim.hud.name_event(atv_core::stunt::NAMES[id]);
                }
                atv_core::stunt_score::Notification::ChainFinished {
                    success,
                    display_count,
                    ..
                } => sim.hud.finish_event(success, display_count),
                _ => {}
            }
        }
        let score = animator.score_frame();
        active = score.active;
        points = score.display_points;
        sim.hud
            .set_score_cache(points, score.display_count, active)?;
        let wheelie = animator.wheelie_frame();
        if wheelie.eligible {
            //002dd4b0 gives the wheelie cache precedence;002e3640 retains
            //its panel while eligible, independently of airborne chains.
            sim.hud.set_wheelie_points(wheelie.points)?;
            active = true;
            points = wheelie.points;
        }
    }
    sim.hud.step(
        dt,
        frame.speed_magnitude,
        frame.preload_available,
        frame.preload_strength,
        active,
        points,
    )
}

fn simulation(input: &[u8], op: u32) -> Result<Vec<u8>> {
    SIMULATION.with_borrow_mut(|sim| {
        match op {
            63 => {
                let document = atv_core::ui::Document::parse(input)?;
                sim.hud =
                    atv_core::hud::State::new(atv_core::hud::Config::from_document(&document)?);
            }
            20 => {
                let terrain = terrain::decode_tr2(input)?;
                sim.terrain = Some(terrain);
                sim.cpu_racers.clear();
                sim.race = None;

                sim.motorcycle = None;
                sim.motorcycle_model = None;
                sim.vehicle = None;
                sim.obstacle_rays = None;
                sim.obstacle_meshes.clear();
                sim.obstacles = None;
                sim.mud_obstacles.clear();
                sim.scene_partition = None;
                sim.ecosystem = None;
                sim.boundary_config = None;
                sim.crowd_present = false;
                sim.water = None;
                sim.train = None;
                sim.train_collision.clear();
                sim.train_collision_installed = false;
                sim.effects_time = 0.;
                sim.rider_audio_position = [0.; 3];
                sim.effects_dt = 0.;
                sim.effects_tick = 0;
                sim.placement_epoch = 0;
            }
            65 => {
                sim.ecosystem = if input.is_empty() {
                    None
                } else {
                    Some(atv_core::ecosystem::decode(input)?)
                };
                sim.vehicle = None;
                return serde_json::to_vec(&serde_json::json!({
                    "placements": sim.ecosystem.as_ref().map_or(0, |eco| eco.placements.len()),
                }))
                .map_err(|e| e.to_string());
            }
            21 => {
                let physics = PhysicsData::parse(input)?;
                sim.physics = Some(physics);
                sim.vehicle = None;
            }
            22 => {
                let mesh = mesh::decode_psg(input)?;
                sim.quad_pose = Some(std::rc::Rc::new(atv_core::quad_pose::QuadPose::new(&mesh)?));
                sim.mesh = Some(mesh);
                sim.chassis_points = None;
                sim.wreck_rays = None;
                sim.vehicle = None;
            }
            23 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|error| error.to_string())?;
                let array = value
                    .get("origin")
                    .and_then(serde_json::Value::as_array)
                    .ok_or("missing simulation origin")?;
                if array.len() != 3 {
                    return Err("simulation origin needs three coordinates".into());
                }
                let mut origin = [0.; 3];
                for (index, number) in array.iter().enumerate() {
                    origin[index] = number.as_f64().ok_or("invalid origin coordinate")? as f32;
                }
                if !origin.iter().all(|number| number.is_finite()) {
                    return Err("nonfinite simulation origin".into());
                }
                let mut vehicle = Vehicle::new(
                    sim.mesh.as_ref().ok_or("load original mesh first")?,
                    sim.physics.as_ref().ok_or("load original physics first")?,
                    origin,
                    json_number(&value, "yaw")?,
                )?;
                vehicle
                    .enable_wrecks(sim.physics.as_ref().ok_or("load original physics first")?)?;
                if let Some(config) = sim.boundary_config {
                    vehicle.set_boundary(config, sim.crowd_present)?;
                }
                vehicle.set_linkage(
                    sim.quad_pose
                        .as_ref()
                        .ok_or("load original quad linkage first")?
                        .clone(),
                );
                if let Some(wreck) = &mut sim.wreck_animator {
                    wreck.reset();
                    wreck.set_boundary_flight(false);
                    wreck.set_static_obstacles(sim.obstacles.clone().unwrap_or_default());
                }
                vehicle.set_materials(
                    sim.materials
                        .ok_or("load original terrain materials first")?,
                );
                vehicle.set_chassis_points(
                    sim.chassis_points
                        .as_ref()
                        .ok_or("load original chassis collision first")?,
                )?;
                vehicle.set_wreck_rays(
                    sim.wreck_rays
                        .as_ref()
                        .ok_or("load original wreck collision rays first")?
                        .clone(),
                )?;
                vehicle.set_obstacles(
                    sim.obstacle_rays
                        .as_ref()
                        .ok_or("load original actor collision rays first")?
                        .clone(),
                    sim.obstacles
                        .as_ref()
                        .ok_or("load original scene collision first")?
                        .clone(),
                )?;
                if let Some(config) = sim.scene_partition {
                    vehicle.set_scene_partition(
                        config,
                        sim.ecosystem.as_ref(),
                        sim.terrain.as_ref().ok_or("load original terrain first")?,
                    )?;
                }
                if let Some(scene) = vehicle.scene_collision_mut() {
                    scene.set_mud_obstacles(sim.mud_obstacles.clone())?;
                    if sim.train_collision_installed {
                        scene.install_dynamic(train_collision_objects(sim)?)?;
                    }
                }
                if let Some(animator) = &mut sim.rider_animator {
                    animator.reset(&sim.rider_mesh.as_ref().ok_or("rider mesh missing")?.nodes)?;
                    animator.reset_score_session();
                }
                sim.hud.reset();
                sim.rider_mode = None;
                sim.touch_drive.reset();
                sim.input_generation = None;
                sim.resolved_input = None;
                let frame = vehicle.frame();
                sim.placement_epoch = sim.placement_epoch.wrapping_add(1);
                sim.effects_dt = 0.;
                sim.wreck_camera = None;
                sim.vehicle = Some(vehicle);
                return simulation_frame(sim, &frame);
            }
            24 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|error| error.to_string())?;
                let raw = value.get("input").ok_or("missing simulation input")?;
                let generation = raw
                    .get("input_generation")
                    .map(|v| v.as_u64().ok_or("invalid input generation"))
                    .transpose()?;
                let input = VehicleInput {
                    throttle: json_number(raw, "throttle")?,
                    brake: json_number(raw, "brake")?,
                    steer: json_number(raw, "steer")?,
                    lean: json_number(raw, "lean")?,
                    preload_axis: raw
                        .get("preload_axis")
                        .filter(|v| !v.is_null())
                        .map(|_| json_number(raw, "preload_axis"))
                        .transpose()?,
                    reverse_chord_brake: raw
                        .get("reverse_chord_brake")
                        .filter(|v| !v.is_null())
                        .map(|_| json_number(raw, "reverse_chord_brake"))
                        .transpose()?,
                    reverse: raw
                        .get("reverse")
                        .and_then(serde_json::Value::as_bool)
                        .ok_or("missing reverse input")?,
                };
                sim.funny_crashes.set_enabled(
                    raw.get("funny_crashes")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                );
                let terrain = sim.terrain.as_ref().ok_or("load original terrain first")?;
                let dt = json_number(&value, "dt")?;
                if dt <= 0. || dt > 0.1 {
                    return Err("invalid simulation time step".into());
                }
                let intent = raw
                    .get("touch_drive")
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        serde_json::from_value(v.clone())
                            .map_err(|e| format!("invalid touch intent: {e}"))
                    })
                    .transpose()?;
                let current = sim
                    .vehicle
                    .as_ref()
                    .ok_or("start simulation first")?
                    .frame();
                let input = sim.touch_drive.resolve(
                    input,
                    intent,
                    current.grounded.iter().any(|c| *c),
                    current.speed,
                )?;
                let mut resolved = raw.clone();
                for (key, number) in [
                    ("throttle", input.throttle),
                    ("brake", input.brake),
                    ("steer", input.steer),
                    ("lean", input.lean),
                ] {
                    resolved[key] = serde_json::json!(number);
                }
                resolved["reverse"] = serde_json::json!(input.reverse);
                sim.resolved_input = Some(resolved);
                let safe = sim
                    .rider_animator
                    .as_ref()
                    .is_none_or(|a| a.stunt_landing_safe());
                let forced_landing = sim
                    .rider_animator
                    .as_ref()
                    .is_some_and(|a| a.landing_frame().is_some());
                let vehicle = sim.vehicle.as_mut().ok_or("start simulation first")?;
                if generation.is_some() && generation != sim.input_generation {
                    vehicle.cancel_preload_gesture();
                    sim.input_generation = generation;
                }
                vehicle.set_stunt_landing_safe(safe);
                vehicle.set_rider_mode(sim.rider_mode);
                vehicle.set_forced_landing(forced_landing);
                let wreck = sim
                    .wreck_animator
                    .as_mut()
                    .ok_or("load original wreck animations first")?;
                if let Some(water) = &mut sim.water {
                    water.advance(dt)?;
                }
                let frame =
                    vehicle.step_with_scene(terrain, input, dt, sim.water.as_ref(), || {
                        wreck.boundary_random_words()
                    })?;
                //Deep water can invoke001e35d0 before physics independently
                //of advance_wreck. Reset attached presentation/camera state too.
                if frame.reset_count != current.reset_count {
                    wreck.reset();
                    wreck.set_boundary_flight(false);
                    sim.rider_animator
                        .as_mut()
                        .ok_or("rider animator missing")?
                        .reset(&sim.rider_mesh.as_ref().ok_or("rider mesh missing")?.nodes)?;
                    if let Some(camera) = &mut sim.camera {
                        camera.reset();
                    }
                    sim.wreck_camera = None;
                    sim.rider_mode = None;
                    sim.touch_drive.reset();
                }
                //001e7600 publishes actor+290 before rider physics only while
                //Riding/Attached; detached audio retains the last actor point.
                if frame.wreck.phase == atv_core::wreck::Phase::Riding
                    || wreck.frame().is_none_or(|rider| {
                        rider.phase == atv_core::wreck_animation::Phase::Attached
                    })
                {
                    sim.rider_audio_position = frame.origin;
                }
                sim.effects_time += dt;
                sim.effects_dt = dt;
                sim.effects_tick = sim.effects_tick.wrapping_add(1);
                if let Some(input) = &mut sim.resolved_input {
                    let (throttle, brake, reverse) = vehicle.applied_drive_controls();
                    input["throttle"] = serde_json::json!(throttle);
                    input["brake"] = serde_json::json!(brake);
                    input["reverse"] = serde_json::json!(reverse);
                }
                if frame.wreck.phase != atv_core::wreck::Phase::Riding {
                    // Never carry takeoff throttle/direction through recovery.
                    sim.touch_drive.reset();
                    let nodes = &sim.rider_mesh.as_ref().ok_or("rider mesh missing")?.nodes;
                    let wreck = sim
                        .wreck_animator
                        .as_mut()
                        .ok_or("load original wreck animations first")?;
                    if !wreck.active() {
                        wreck.set_boundary_flight(frame.boundary_launch.active);
                        wreck.begin(
                            frame.wreck.kind.ok_or("missing wreck kind")?,
                            sim.rider_animator
                                .as_ref()
                                .ok_or("rider animator missing")?
                                .channels(),
                            vehicle_world(&frame),
                            nodes,
                        )?;
                        sim.rider_animator
                            .as_mut()
                            .ok_or("rider animator missing")?
                            .abort_wreck_scores()?;
                        sim.rider_animator
                            .as_mut()
                            .ok_or("rider animator missing")?
                            .reset(nodes)?;
                    }
                    update_wheelie(sim.rider_animator.as_mut(), &frame, dt)?;
                    wreck.step_with_scene(
                        dt,
                        atv_core::wreck_animation::VehicleMotion {
                            world: vehicle_world(&frame),
                            linear_velocity: frame.linear_velocity,
                            local_angular_velocity: frame.local_angular_velocity,
                        },
                        nodes,
                        terrain,
                        sim.materials.as_ref().ok_or("missing terrain materials")?,
                        sim.water.as_ref(),
                        vehicle.scene_collision_mut(),
                        frame.skip_vegetation_query,
                    )?;
                    let grounded = wreck.frame().is_some_and(|f| f.grounded);
                    if vehicle.advance_wreck(dt, grounded)? {
                        wreck.reset();
                        wreck.set_boundary_flight(false);
                        sim.rider_animator
                            .as_mut()
                            .ok_or("rider animator missing")?
                            .reset(nodes)?;
                        if let Some(camera) = &mut sim.camera {
                            camera.reset();
                        }
                        sim.wreck_camera = None;
                        sim.rider_mode = None;
                    }
                    let frame = vehicle.frame();
                    if let Some(water) = &mut sim.water {
                        water.prepare_surface()?;
                    }
                    update_hud(sim, &frame, dt)?;
                    return simulation_frame(sim, &frame);
                }
                if let Some(animator) = &mut sim.rider_animator {
                    update_wheelie(Some(animator), &frame, dt)?;
                    let nodes = &sim.rider_mesh.as_ref().ok_or("rider mesh missing")?.nodes;
                    let input = sim
                        .vehicle
                        .as_ref()
                        .ok_or("vehicle missing")?
                        .rider_pose_input();
                    let modifiers = raw
                        .get("stunt_modifiers")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    if modifiers > 7 {
                        return Err("invalid stunt modifiers".into());
                    }
                    let axis = |key| {
                        raw.get(key)
                            .filter(|v| !v.is_null())
                            .map(|_| json_number(raw, key))
                            .transpose()
                            .map(|v| v.unwrap_or(0.))
                    };
                    let stunt = atv_core::stunt::Input {
                        modifiers: modifiers as u8,
                        axes: [axis("stunt_x")?, axis("stunt_y")?],
                        grounded: frame.grounded.iter().any(|x| *x),
                        horizontal_speed_squared: frame.linear_velocity[0]
                            * frame.linear_velocity[0]
                            + frame.linear_velocity[2] * frame.linear_velocity[2],
                        crashed: false,
                        disabled: false,
                    };
                    if let Some(landing) = frame.landing_action {
                        animator.request_landing(landing)?;
                    }
                    sim.rider_mode = Some(animator.step_controls(
                        nodes,
                        input,
                        stunt,
                        json_number(&value, "dt")?,
                    )?);
                }
                if let Some(water) = &mut sim.water {
                    water.prepare_surface()?;
                }
                update_hud(sim, &frame, dt)?;
                return simulation_frame(sim, &frame);
            }
            26 => {
                let materials = materials::terrain_materials(input)?;
                sim.materials = Some(materials);
                sim.vehicle = None;
            }
            27 => {
                let mesh = mesh::decode_psg(input)?;
                let result = serde_json::to_vec(&mesh).map_err(|error| error.to_string())?;
                sim.rider_mesh = Some(mesh);
                sim.rider_animation = None;
                sim.rider_animator = None;
                sim.wreck_animator = None;
                sim.rider_catalog.clear();
                sim.rider_motions.clear();
                return Ok(result);
            }
            28 => {
                let clip = animation::decode_mot(input)?;
                sim.rider_animation = Some(clip);
            }
            29 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|error| error.to_string())?;
                let mesh = sim
                    .rider_mesh
                    .as_ref()
                    .ok_or("load original rider mesh first")?;
                let clip = sim
                    .rider_animation
                    .as_ref()
                    .ok_or("load original rider motion first")?;
                let pose = clip.sample_pose(
                    &mesh.nodes,
                    json_number(&value, "time")?,
                    animation::Sampling::default(),
                )?;
                return serde_json::to_vec(&pose).map_err(|error| error.to_string());
            }
            51 => {
                let catalog = atv_core::motion_catalog::MotionCatalog::parse(input)?;
                let prefix = if catalog.motions.iter().any(|m| m.name.starts_with("F_")) {
                    "F"
                } else {
                    "M"
                };
                let mut selected = atv_core::rider_pose::Clip::ALL
                    .iter()
                    .map(|clip| {
                        let name = format!("{prefix}_{}", clip.suffix());
                        catalog
                            .motions
                            .iter()
                            .find(|motion| motion.name.eq_ignore_ascii_case(&name))
                            .cloned()
                            .ok_or_else(|| format!("missing rider motion {name}"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                for kind in ["Stunt", "Hold"] {
                    for suffix in atv_core::stunt::SUFFIXES {
                        let name = format!("{prefix}_{kind}{suffix}");
                        selected.push(
                            catalog
                                .motions
                                .iter()
                                .find(|motion| motion.name.eq_ignore_ascii_case(&name))
                                .cloned()
                                .ok_or_else(|| format!("missing rider motion {name}"))?,
                        );
                    }
                }
                for suffix in atv_core::wreck_animation::SUFFIXES {
                    let name = format!("{prefix}_{suffix}");
                    selected.push(
                        catalog
                            .motions
                            .iter()
                            .find(|m| m.name.eq_ignore_ascii_case(&name))
                            .cloned()
                            .ok_or_else(|| format!("missing wreck motion {name}"))?,
                    );
                }
                for suffix in atv_core::rider_pose::Landing::SUFFIXES {
                    let name = format!("{prefix}_{suffix}");
                    selected.push(
                        catalog
                            .motions
                            .iter()
                            .find(|m| m.name.eq_ignore_ascii_case(&name))
                            .cloned()
                            .ok_or_else(|| format!("missing landing motion {name}"))?,
                    );
                }
                let output = serde_json::to_vec(&selected).map_err(|e| e.to_string())?;
                sim.rider_catalog = selected;
                sim.rider_motions.clear();
                sim.rider_animator = None;
                sim.wreck_animator = None;
                return Ok(output);
            }
            53 => {
                let nodes = &sim
                    .rider_mesh
                    .as_ref()
                    .ok_or("load rider mesh first")?
                    .nodes;
                if sim.rider_motions.len() != 78 {
                    return Err("load all normal, stunt, wreck and landing motions first".into());
                }
                let landings = sim.rider_motions.split_off(76);
                let wrecks = sim.rider_motions.split_off(62);
                sim.wreck_animator = Some(atv_core::wreck_animation::Animator::new(
                    wrecks,
                    sim.rider_catalog[62..76]
                        .iter()
                        .map(|m| m.looping)
                        .collect(),
                    nodes,
                )?);
                sim.wreck_animator
                    .as_mut()
                    .ok_or("wreck animator missing")?
                    .set_static_obstacles(sim.obstacles.clone().unwrap_or_default());
                let stunts = sim.rider_motions.split_off(14);
                let mut animator = atv_core::rider_pose::Animator::new(
                    std::mem::take(&mut sim.rider_motions),
                    nodes,
                )?;
                animator.set_landing_motions(landings)?;
                // 002de690 identifies mode 11 as Freeride/Freestyle. The current
                // browser route uses its ordinary stunt repetition penalties.
                animator.set_scoring_mode(Some(atv_core::stunt_score::FREESTYLE_MODE))?;
                animator.set_stunts(
                    stunts,
                    sim.rider_catalog[14..62]
                        .iter()
                        .map(|m| m.looping)
                        .collect(),
                )?;
                let output =
                    serde_json::to_vec(&animator.pose(nodes)?).map_err(|e| e.to_string())?;
                sim.rider_animator = Some(animator);
                return Ok(output);
            }
            44 => {
                let tree = atv_core::collision::decode_ray_tree(input)?;
                let points = tree.chassis_points()?;
                let result = serde_json::to_vec(
                    &serde_json::json!({"rays": tree.rays.len(), "points": points.len()}),
                )
                .map_err(|error| error.to_string())?;
                sim.chassis_points = Some(points);
                sim.wreck_rays = Some(std::rc::Rc::new(tree));
                sim.vehicle = None;
                return Ok(result);
            }
            45 => {
                let presets = atv_core::camera::presets(input, 0)?;
                let result = serde_json::to_vec(&presets).map_err(|e| e.to_string())?;
                //00319c10 ->003007b0 supplies70MPH and the original ft/s conversion.
                sim.camera = Some(atv_core::camera::FollowCamera::new(
                    presets,
                    70. * f32::from_bits(0x3fbbbb84),
                )?);
                return Ok(result);
            }
            46 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|e| e.to_string())?;
                let vehicle = sim
                    .vehicle
                    .as_ref()
                    .ok_or("start simulation first")?
                    .frame();
                let basis = atv_core::dynamics::basis(vehicle.orientation);
                let velocity = vehicle.linear_velocity;
                let local = atv_core::dynamics::inverse_transform(basis, velocity);
                let terrain = sim.terrain.as_ref().ok_or("load original terrain first")?;
                if let Some(wreck) = sim.wreck_animator.as_ref().and_then(|a| a.frame()) {
                    let actors = [
                        atv_core::camera::WreckActor {
                            origin: vehicle.origin,
                            velocity,
                        },
                        atv_core::camera::WreckActor {
                            origin: wreck.origin,
                            velocity: wreck.linear_velocity,
                        },
                    ];
                    if sim.wreck_camera.is_none() {
                        let mut camera = sim
                            .camera
                            .as_ref()
                            .ok_or("camera missing")?
                            .wreck_camera(actors)?;
                        camera.set_static_obstacles(sim.obstacles.clone().unwrap_or_default());
                        sim.wreck_camera = Some(camera);
                    }
                    let frame = sim
                        .wreck_camera
                        .as_mut()
                        .ok_or("wreck camera missing")?
                        .step_with_scene(
                            actors,
                            basis[2],
                            json_number(&value, "dt")?,
                            |point| {
                                Ok(terrain
                                    .contact(point[0], point[2])?
                                    .map_or(point[1], |c| c.height))
                            },
                            sim.vehicle.as_ref().and_then(Vehicle::scene_collision),
                        )?;
                    let mut output = serde_json::to_value(frame).map_err(|e| e.to_string())?;
                    output["mode"] = serde_json::json!("wreck");
                    output["motion_blocked"] = serde_json::json!(
                        sim.wreck_camera
                            .as_ref()
                            .is_some_and(|camera| camera.motion_blocked())
                    );
                    output["scene_query"] = serde_json::to_value(
                        sim.wreck_camera
                            .as_ref()
                            .and_then(|camera| camera.scene_query()),
                    )
                    .map_err(|error| error.to_string())?;
                    return serde_json::to_vec(&output).map_err(|e| e.to_string());
                }
                sim.wreck_camera = None;
                let frame = sim
                    .camera
                    .as_mut()
                    .ok_or("load original camera presets first")?
                    .step(
                        atv_core::camera::FollowInput {
                            origin: vehicle.origin,
                            forward: basis[2],
                            velocity,
                            speed: ((velocity[0] * velocity[0] + velocity[1] * velocity[1])
                                + velocity[2] * velocity[2])
                                .sqrt(),
                            local_forward_speed: local[2],
                            yaw_rate: vehicle.local_angular_velocity[1],
                            wheel_contact: vehicle.grounded.iter().any(|v| *v),
                            orbit: json_number(&value, "orbit")?,
                        },
                        json_number(&value, "dt")?,
                        |point| {
                            Ok(terrain
                                .contact(point[0], point[2])?
                                .map_or(point[1], |contact| contact.height))
                        },
                    )?;
                return serde_json::to_vec(&frame).map_err(|e| e.to_string());
            }
            47 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|e| e.to_string())?;
                let camera = sim
                    .camera
                    .as_mut()
                    .ok_or("load original camera presets first")?;
                if value
                    .get("cycle")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                {
                    camera.cycle();
                } else {
                    camera.reset();
                }
            }
            _ => return Err("unknown simulation operation".into()),
        }
        Ok(b"{}".to_vec())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn input_resize(len: usize) -> u32 {
    if len > 128 * 1024 * 1024 {
        return 0;
    }
    BUFFERS.with_borrow_mut(|b| {
        b.input.resize(len, 0);
    });
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn input_ptr() -> *const u8 {
    BUFFERS.with_borrow(|b| b.input.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn output_ptr() -> *const u8 {
    BUFFERS.with_borrow(|b| b.output.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn output_len() -> usize {
    BUFFERS.with_borrow(|b| b.output.len())
}

fn obstacle_input(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    SIMULATION.with_borrow_mut(|sim| {
        if op == 48 {
            if index == 0 {
                let rays = atv_core::collision::decode_ray_tree(input)?;
                let identity = [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.];
                if rays.transform != identity { return Err("unsupported actor collision header transform".into()); }
                let result = serde_json::json!({"rays":rays.rays.len()});
                sim.obstacle_rays = Some(std::rc::Rc::new(rays));
                sim.vehicle = None;
                return serde_json::to_vec(&result).map_err(|e| e.to_string());
            }
            if sim.obstacle_meshes.len() >= 4096 { return Err("too many collision meshes".into()); }
            let mesh = atv_core::collision::decode_col(input)?;
            if mesh.transform != [1.,0.,0.,0.,1.,0.,0.,0.,1.,0.,0.,0.] {
                return Err("unsupported static collision header transform".into());
            }
            let result = serde_json::json!({"mesh":sim.obstacle_meshes.len(),"triangles":mesh.triangles.len()});
            sim.obstacle_meshes.push(std::rc::Rc::new(mesh));
            sim.vehicle = None;
            return serde_json::to_vec(&result).map_err(|e| e.to_string());
        }
        let source: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        let placements = source.as_array().or_else(|| source.get("placements").and_then(serde_json::Value::as_array)).ok_or("collision placements need an array")?;
        let boundary_config: Option<atv_core::boundary_launch::Config> = source.get("boundary_launch").filter(|v| !v.is_null()).map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string())).transpose()?;
        let scene_partition: Option<atv_core::scene_partition::Config> = source.get("scene_partition").filter(|v| !v.is_null()).map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string())).transpose()?;
        if let Some(config) = scene_partition {
            atv_core::scene_partition::StaticPartition::new(config, &[])?;
        }
        if let Some(config) = boundary_config { config.outside([0.; 3], false)?; }
        let crowd_present = source.get("crowd_present").map(|v| v.as_bool().ok_or("invalid crowd flag")).transpose()?.unwrap_or(false);
        if placements.len() > 4096 { return Err("too many collision placements".into()); }
        let base = source.get("base_name").and_then(serde_json::Value::as_str).unwrap_or("");
        let mud_prefix = format!("{}mud", base.to_ascii_lowercase());
        let mut mud_obstacles = Vec::with_capacity(placements.len());
        let mut obstacles = Vec::with_capacity(placements.len());
        for placement in placements {
            let index = placement.get("mesh").and_then(serde_json::Value::as_u64).ok_or("missing collision mesh index")?;
            let mesh = sim.obstacle_meshes.get(usize::try_from(index).map_err(|_| "invalid collision mesh index")?).ok_or("collision mesh index out of bounds")?.clone();
            let values = placement.get("world_matrix").and_then(serde_json::Value::as_array).ok_or("missing collision matrix")?;
            if values.len() != 16 { return Err("collision matrix needs 16 elements".into()); }
            let mut world_matrix = [0.;16];
            for (output, value) in world_matrix.iter_mut().zip(values) {
                *output = value.as_f64().ok_or("invalid collision matrix element")? as f32;
                if !output.is_finite() { return Err("nonfinite collision matrix".into()); }
            }
            let name = placement.get("name").and_then(serde_json::Value::as_str).unwrap_or("");
            let stem = name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase();
            mud_obstacles.push(!base.is_empty() && stem.starts_with(&mud_prefix));
            obstacles.push(atv_core::obstacle::StaticObstacle { mesh, world_matrix });
        }
        let result = serde_json::json!({"instances":obstacles.len(),"meshes":sim.obstacle_meshes.len()});
        sim.obstacles = Some(obstacles);
        sim.mud_obstacles = mud_obstacles;
        sim.scene_partition = scene_partition;
        sim.boundary_config = boundary_config;
        sim.crowd_present = crowd_present;
        sim.vehicle = None;
        serde_json::to_vec(&result).map_err(|e| e.to_string())
    })
}

/// Bounded transport batch. Each step still runs the original vehicle operation,
/// then the original camera operation, with the same fixed 1/60 second input.
/// Retain intermediate frames so consumers such as audio see every step.
fn train_collision_objects(
    sim: &Simulation,
) -> Result<Vec<atv_core::scene_collision::DynamicObstacle>> {
    let train = sim.train.as_ref().ok_or("load original train first")?;
    sim.train_collision
        .iter()
        .map(|binding| {
            let motion = train
                .motion_state(binding.car)
                .ok_or("train collision body index out of range")?;
            Ok(atv_core::scene_collision::DynamicObstacle {
                obstacle: atv_core::obstacle::StaticObstacle {
                    mesh: binding.mesh.clone(),
                    world_matrix: motion.current,
                },
                motion,
            })
        })
        .collect::<Result<Vec<_>>>()
}

fn step_train_scene(sim: &mut Simulation, dt: f32) -> Result<Option<atv_core::train::Frame>> {
    let Some(train) = sim.train.as_mut() else {
        return Ok(None);
    };
    if !sim.train_collision_installed {
        return train.step(dt, |_| Ok(None)).map(Some);
    }
    let scene = sim
        .vehicle
        .as_mut()
        .and_then(Vehicle::scene_collision_mut)
        .ok_or("missing live train collision scene")?;
    let scene = RefCell::new(scene);
    let bindings = &sim.train_collision;
    train
        .step_with_scene(
            dt,
            |car, previous| {
                let own = bindings.iter().position(|binding| binding.car == car);
                atv_core::train::height_geometry(&scene.borrow(), own, previous)
            },
            |car, motion| {
                if let Some(index) = bindings.iter().position(|binding| binding.car == car) {
                    scene.borrow_mut().update_dynamic(index, motion)?;
                }
                Ok(())
            },
        )
        .map(Some)
}

/// Embed already serialized operations without allocating a second JSON tree.
/// RawValue validates the JSON and retains its original numeric representation.
fn serialized_operation(bytes: Vec<u8>) -> Result<Box<serde_json::value::RawValue>> {
    serde_json::value::RawValue::from_string(String::from_utf8(bytes).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct SimulationBatchFrame {
    #[serde(skip_serializing_if = "Option::is_none")]
    race: Option<atv_core::race::Frame>,
    frame: Box<serde_json::value::RawValue>,
    camera: Box<serde_json::value::RawValue>,
    water: Option<atv_core::water_render::Frame>,
    train: Option<atv_core::train::Frame>,
    scenery_uv: std::collections::BTreeMap<String, [f32; 2]>,
    scenery_textures: std::collections::BTreeMap<String, usize>,
}

fn simulation_batch(input: &[u8]) -> Result<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    let count = value
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .filter(|count| (1..=4).contains(count))
        .ok_or("simulation batch count must be 1 through 4")?;
    let driving = value.get("input").ok_or("missing simulation input")?;
    let orbit = json_number(&value, "orbit")?;
    let step = serde_json::to_vec(&serde_json::json!({"dt":1.0/60.0,"input":driving}))
        .map_err(|e| e.to_string())?;
    let camera = serde_json::to_vec(&serde_json::json!({"dt":1.0/60.0,"orbit":orbit}))
        .map_err(|e| e.to_string())?;
    if value
        .get("cycle")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        simulation(br#"{"cycle":true}"#, 47)?;
    }
    let mut frames = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let train = SIMULATION.with_borrow_mut(|sim| step_train_scene(sim, 1.0 / 60.0))?;
        let locked = SIMULATION.with_borrow(|sim| sim.race.as_ref().is_some_and(|r| r.locked()));
        let bytes = if locked {
            SIMULATION.with_borrow_mut(|sim| {
                let player = sim
                    .vehicle
                    .as_ref()
                    .ok_or("start simulation first")?
                    .frame();
                simulation_frame(sim, &player)
            })?
        } else {
            simulation(&step, 24)?
        };
        let frame = serialized_operation(bytes)?;
        let race = SIMULATION.with_borrow_mut(|sim| -> Result<Option<atv_core::race::Frame>> {
            if let Some(race) = &mut sim.race {
                let player = sim
                    .vehicle
                    .as_ref()
                    .ok_or("start simulation first")?
                    .frame();
                race.step(player.origin, 1. / 60.);
                let [w, x, y, z] = player.orientation;
                let yaw = (2. * (w * y + x * z)).atan2(1. - 2. * (y * y + x * x));
                let terrain = sim.terrain.as_ref().ok_or("missing terrain")?;
                if race.running() {
                    let nodes = &sim
                        .rider_mesh
                        .as_ref()
                        .ok_or("CPU rider mesh missing")?
                        .nodes;
                    for (index, cpu) in sim.cpu_racers.iter_mut().enumerate() {
                        if race.opponents()[index].finished {
                            continue;
                        }
                        let before = cpu.vehicle.frame();
                        let height = terrain
                            .contact(before.origin[0], before.origin[2])?
                            .map_or(before.origin[1], |hit| hit.height);
                        let (input, decisions) = cpu.driver.step(
                            &race.course,
                            &before,
                            &player,
                            race.progress_difference(index),
                            1. / 60.,
                            before.origin[1] - height,
                        )?;
                        let frame = cpu.vehicle.step(terrain, input, 1. / 60.)?;
                        let (_, pose) =
                            cpu.rider
                                .step(nodes, cpu.vehicle.rider_pose_input(), 1. / 60.)?;
                        let [w, x, y, z] = frame.orientation;
                        let cpu_yaw = (2. * (w * y + x * z)).atan2(1. - 2. * (y * y + x * x));
                        let physical = atv_core::race::CpuPose {
                            orientation: frame.orientation,
                            speed: frame.speed,
                            grounded: frame.grounded,
                            wheel_compression: frame.wheel_compression,
                            wheel_angles: frame.wheel_angles,
                            quad_matrices: cpu
                                .vehicle
                                .linkage_matrices()
                                .ok_or("CPU linkage not evaluated")?
                                .to_vec(),
                            rider_matrices: pose.world_matrices,
                            decisions,
                        };
                        race.observe_cpu(index, frame.origin, cpu_yaw, Some(physical));
                    }
                }
                let race_frame = race.frame(yaw);
                return Ok(Some(race_frame));
            }
            Ok(None)
        })?;
        let camera = serialized_operation(simulation(&camera, 46)?)?;
        let water = SIMULATION
            .with_borrow(|sim| sim.water.as_ref().map(atv_core::water_render::Frame::new));
        let scenery_uv = SIMULATION.with_borrow_mut(|sim| {
            sim.scenery_uv
                .iter_mut()
                .map(|(name, scroll)| scroll.step(1. / 60.).map(|offset| (name.clone(), offset)))
                .collect::<Result<std::collections::BTreeMap<_, _>>>()
        })?;
        let scenery_textures = SIMULATION.with_borrow_mut(|sim| {
            let milliseconds = (sim.scenery_tick * 1000 / 60) as u32;
            sim.scenery_tick += 1;
            sim.scenery_textures
                .iter_mut()
                .map(|(name, clock)| (name.clone(), clock.tick(milliseconds)))
                .collect::<std::collections::BTreeMap<_, _>>()
        });
        frames.push(SimulationBatchFrame {
            race,
            frame,
            camera,
            water,
            train,
            scenery_uv,
            scenery_textures,
        });
    }
    serde_json::to_vec(&frames).map_err(|e| e.to_string())
}

fn dispatch(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    if matches!(op, 100 | 102) {
        return motorcycles::load(input, index, op == 100);
    }
    if op == 101 {
        return engine_audio(input, op, index);
    }
    if matches!(op, 23 | 46 | 47 | 50)
        && SIMULATION.with_borrow(|sim| sim.motorcycle_model.is_some() || sim.motorcycle.is_some())
    {
        return motorcycles::dispatch(input, op);
    }
    if op == 95 {
        let course = atv_core::race::Course::parse(input)?;
        let result = serde_json::to_vec(&course).map_err(|e| e.to_string())?;
        SIMULATION.with_borrow_mut(|sim| {
            sim.race_course = Some(course);
            sim.race = None;
            sim.cpu_racers.clear();
        });
        return Ok(result);
    }
    if op == 96 {
        return SIMULATION.with_borrow_mut(|sim| {
            let value: serde_json::Value =
                serde_json::from_slice(input).map_err(|e| e.to_string())?;
            if value.get("active").and_then(serde_json::Value::as_bool) == Some(false) {
                sim.race = None;
                sim.race_course = None;
                sim.cpu_racers.clear();
                return Ok(b"null".to_vec());
            }
            let integer = |key| {
                value
                    .get(key)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| format!("invalid race {key}"))
            };
            let names: Vec<String> =
                serde_json::from_value(value.get("names").ok_or("missing CPU names")?.clone())
                    .map_err(|e| e.to_string())?;
            let mut race = atv_core::race::Race::new(
                sim.race_course.clone().ok_or("load race course first")?,
                integer("laps")?,
                names,
                integer("difficulty")?,
            )?;
            let racers = if sim.motorcycle.is_some() {
                vec![]
            } else {
                race.opponents()
                    .iter()
                    .enumerate()
                    .map(|(index, cpu)| {
                        create_cpu_racer(sim, cpu.origin, cpu.yaw, index, integer("difficulty")?)
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            for (index, cpu) in racers.iter().enumerate() {
                race.observe_cpu(
                    index,
                    cpu.vehicle.frame().origin,
                    race.opponents()[index].yaw,
                    None,
                );
            }
            sim.cpu_racers = racers;
            sim.race = Some(race);
            Ok(b"null".to_vec())
        });
    }

    if op == 90 {
        return serde_json::to_vec(&atv_core::scenery_material::parse(input)?)
            .map_err(|e| e.to_string());
    }
    if op == 91 {
        let sources: std::collections::BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(input).map_err(|e| e.to_string())?;
        if sources.len() > 4096 || sources.keys().any(|name| name.len() > 128) {
            return Err("scenery animation configuration exceeds limit".into());
        }
        let mut animations = Vec::new();
        let mut textures = Vec::new();
        for (name, source) in sources {
            let text = source
                .as_str()
                .or_else(|| source.get("source").and_then(serde_json::Value::as_str))
                .ok_or("missing scenery material source")?;
            let pass = atv_core::scenery_material::parse(text.as_bytes())?;
            if let Some(scroll) = pass.uv_scroll {
                animations.push((name.clone(), scroll));
            }
            if let Some(config) = pass.texture_animation {
                let count = source
                    .get("frames")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|count| (1..=512).contains(count))
                    .ok_or("missing scenery ATM count")?;
                textures.push((
                    name,
                    atv_core::scenery_material::TextureClock::new(config, count as usize)?,
                ));
            }
        }
        let result = serde_json::to_vec(&animations).map_err(|e| e.to_string())?;
        SIMULATION.with_borrow_mut(|sim| {
            sim.scenery_uv = animations;
            sim.scenery_textures = textures;
            sim.scenery_tick = 0;
        });
        return Ok(result);
    }
    if (80..=85).contains(&op) {
        return SIMULATION.with_borrow(|sim| multiplayer::dispatch(input, op, index, sim));
    }
    if op == 87 {
        return serde_json::to_vec(&EngineAudioConfig::parse(input, index as u32)?)
            .map_err(|e| e.to_string());
    }
    if op == 92 {
        if index > 1 {
            return Err("invalid menu rider".into());
        }
        let rider = atv_core::menu_preview::Rider::from_resource(input, index == 1)?;
        let (clip, pose) = rider.pose(0.)?;
        let output =
            serde_json::to_vec(&serde_json::json!({"mesh":rider.mesh,"pose":pose,"clip":clip}))
                .map_err(|e| e.to_string())?;
        MENU_RIDER.with_borrow_mut(|slot| *slot = Some(rider));
        return Ok(output);
    }
    if op == 93 {
        if input.len() != 4 {
            return Err("menu clock must be binary32".into());
        }
        let time = f32::from_le_bytes(input.try_into().map_err(|_| "invalid menu clock")?);
        return MENU_RIDER.with_borrow(|slot| {
            let (clip, pose) = slot.as_ref().ok_or("load menu rider first")?.pose(time)?;
            serde_json::to_vec(&serde_json::json!({"clip":clip,"pose":pose}))
                .map_err(|e| e.to_string())
        });
    }
    if op == 89 {
        return serde_json::to_vec(&atv_core::player_models::selection(
            index as u16,
            if input.is_empty() { 0 } else { input[0] as u16 },
        )?)
        .map_err(|e| e.to_string());
    }
    if (76..=78).contains(&op) {
        return sound_effects(input, op);
    }
    if op == 72 {
        return serde_json::to_vec(&atv_core::spatial_audio::train_definitions(input)?)
            .map_err(|e| e.to_string());
    }
    if op == 73 {
        let request = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        return serde_json::to_vec(&atv_core::spatial_audio::scene_commands(request)?)
            .map_err(|e| e.to_string());
    }
    if op == 67 {
        let train = if input.is_empty() {
            None
        } else {
            atv_core::train::Runtime::from_resource(&Resource::parse(input)?, index)?
        };
        let output = train
            .as_ref()
            .map(atv_core::train::Runtime::frame)
            .transpose()?;
        let mut bindings = Vec::new();
        if let Some(train) = &train {
            let resource = Resource::parse(input)?;
            let mut meshes = std::collections::BTreeMap::new();
            for (car, definition) in train.frame()?.cars.into_iter().enumerate() {
                if !definition.use_collision {
                    continue;
                }
                let path = format!(
                    "geometry/{}",
                    definition
                        .model
                        .replace('\\', "/")
                        .to_ascii_lowercase()
                        .strip_suffix(".slt")
                        .ok_or("unsupported train collision model suffix")?
                );
                let path = format!("{path}.col");
                let mesh = if let Some(mesh) = meshes.get(&path) {
                    std::rc::Rc::clone(mesh)
                } else {
                    let member = resource
                        .members
                        .iter()
                        .position(|member| {
                            member.name.replace('\\', "/").eq_ignore_ascii_case(&path)
                        })
                        .ok_or_else(|| {
                            format!("original train collision resource missing: {path}")
                        })?;
                    let mesh = atv_core::collision::decode_col(&resource.extract(member)?)?;
                    if mesh.transform != [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.] {
                        return Err("unsupported train COL local transform".into());
                    }
                    let mesh = std::rc::Rc::new(mesh);
                    meshes.insert(path, std::rc::Rc::clone(&mesh));
                    mesh
                };
                bindings.push(TrainCollisionBinding { car, mesh });
            }
        }
        SIMULATION.with_borrow_mut(|sim| {
            sim.train = train;
            sim.train_collision = bindings;
            sim.train_collision_installed = false;
        });
        return serde_json::to_vec(&output).map_err(|e| e.to_string());
    }
    if op == 68 {
        return SIMULATION.with_borrow_mut(|sim| {
            let frame = if input.is_empty() {
                sim.train
                    .as_ref()
                    .ok_or("load original train first")?
                    .frame()?
            } else {
                let request = serde_json::from_slice(input).map_err(|e| e.to_string())?;
                step_train_scene(sim, json_number(&request, "dt")?)?
                    .ok_or("load original train first")?
            };
            serde_json::to_vec(&frame).map_err(|e| e.to_string())
        });
    }
    if op == 69 {
        return SIMULATION.with_borrow_mut(|sim| {
            let objects = train_collision_objects(sim)?;
            let count = objects.len();
            sim.vehicle
                .as_mut()
                .and_then(Vehicle::scene_collision_mut)
                .ok_or("load original scene collision first")?
                .install_dynamic(objects)?;
            sim.train_collision_installed = true;
            serde_json::to_vec(&serde_json::json!({"bodies":count})).map_err(|e| e.to_string())
        });
    }
    if op == 66 {
        let resource = Resource::parse(input)?;
        let wtt = resource.extract(index)?;
        let directory = resource
            .members
            .get(index)
            .ok_or("water render member index out of range")?
            .name
            .rsplit_once('/')
            .ok_or("water render resource has no directory")?
            .0;
        let shader = atv_core::water_render::Material::shader_name(&wtt)?;
        let load = |path: &str| -> Result<Vec<u8>> {
            let member = resource
                .members
                .iter()
                .position(|member| member.name.eq_ignore_ascii_case(path))
                .ok_or_else(|| format!("missing water render resource {path}"))?;
            resource.extract(member)
        };
        let material = atv_core::water_render::Material::parse(
            &wtt,
            &load(&format!("textures/{shader}.psm"))?,
        )?;
        let environment = texture::decode_pst(&load(&format!(
            "textures/{}.pst",
            material.environment_texture
        ))?)?;
        let gradients =
            atv_core::water_render::GradientAtlas::new(&wtt, &format!("{directory}/Sigs"), load)?;
        return SIMULATION.with_borrow(|sim| {
            let field = sim.water.as_ref().ok_or("initialize shared water physics before rendering")?;
            for wave in &gradients.waves {
                if field.waves.get(wave.origin_channel).is_none_or(|channel| channel.dimension != gradients.textures[wave.layer].dimension) {
                    return Err("portable water requires matching height/derivative scroll dimensions".into());
                }
            }
            serde_json::to_vec(&serde_json::json!({"material":material,"height":atv_core::water_render::HeightAtlas::new(field)?,"gradients":gradients,
                "frame":atv_core::water_render::Frame::new(field),"environment":{"width":environment.info.width,
                "height":environment.info.height,"rgba":environment.rgba},"scope":"portable projected surface; native VU reflection/froth pending"}))
                .map_err(|error|error.to_string())
        });
    }
    if op == 52 {
        return SIMULATION.with_borrow_mut(|sim| {
            if index != sim.rider_motions.len() {
                return Err("load rider clips in catalog order".into());
            }
            let definition = sim
                .rider_catalog
                .get(index)
                .ok_or("rider motion index out of range")?;
            let motion = atv_core::rider_pose::Motion {
                animation: animation::decode_mot(input)?,
                sampling: animation::Sampling {
                    translation_interpolation: definition.interpolate_position,
                    rotation_interpolation: definition.interpolate_rotation,
                },
            };
            sim.rider_motions.push(motion);
            Ok(b"null".to_vec())
        });
    }
    if op == 50 {
        return simulation_batch(input);
    }
    if op == 64 {
        let resource = Resource::parse(input)?;
        let member = resource
            .members
            .get(index)
            .ok_or("water member index out of range")?;
        let (directory, filename) = member
            .name
            .rsplit_once('/')
            .ok_or("water resource has no directory")?;
        if !filename.to_ascii_lowercase().ends_with(".wtt") {
            return Err("water resource must be WTT".into());
        }
        let definition = atv_core::water_animation::Definition::parse(&resource.extract(index)?)?;
        let field = atv_core::water_animation::LiveField::new(
            definition,
            &format!("{directory}/Sigs"),
            |path| {
                let index = resource
                    .members
                    .iter()
                    .position(|member| member.name.eq_ignore_ascii_case(path))
                    .ok_or_else(|| format!("missing water height resource {path}"))?;
                resource.extract(index)
            },
        )?;
        let result = serde_json::to_vec(&serde_json::json!({"signature_channels":field.waves.len(), "base_height":field.definition.surface.base_height})).map_err(|e| e.to_string())?;
        SIMULATION.with_borrow_mut(|sim| {
            sim.water = Some(field);
            sim.vehicle = None;
        });
        return Ok(result);
    }
    if (48..=49).contains(&op) {
        return obstacle_input(input, op, index);
    }
    if (40..=43).contains(&op) {
        return music(input, op, index);
    }
    if (32..=34).contains(&op) {
        return engine_audio(input, op, index);
    }
    if (20..=29).contains(&op)
        || (44..=47).contains(&op)
        || op == 51
        || op == 53
        || op == 63
        || op == 65
    {
        return simulation(input, op);
    }
    if op == 74 {
        return configure_wheel_effects(input);
    }
    if op == 75 {
        let frame: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        return SIMULATION.with_borrow(|sim| {
            serde_json::to_vec(&wheel_effects_frame(&frame, sim.terrain.as_ref())?)
                .map_err(|e| e.to_string())
        });
    }
    let json = match op {
        60 => {
            let request: serde_json::Value =
                serde_json::from_slice(input).map_err(|e| e.to_string())?;
            let scene = request
                .get("scene")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing scene text")?;
            let mode = match request.get("mode") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(mode)) => Some(mode.as_bytes()),
                _ => return Err("invalid mode text".into()),
            };
            serde_json::to_vec(&atv_core::scene::parse_with_start(scene.as_bytes(), mode)?)
        }
        62 => {
            let request = serde_json::from_slice(input).map_err(|e| e.to_string())?;
            serde_json::to_vec(&atv_core::ui_menu::layout(request)?)
        }
        54 => serde_json::to_vec(&atv_core::ui::Document::parse(input)?),
        55 => serde_json::to_vec(&atv_core::ui::Font::decode(input)?),
        56 => {
            let (frames, texture) = texture::decode_atm(input, index)?;
            serde_json::to_vec(
                &serde_json::json!({"frames":frames,"width":texture.info.width,"height":texture.info.height}),
            )
        }
        57 => return Ok(texture::decode_atm(input, index)?.1.rgba),
        58 => serde_json::to_vec(&atv_core::ecosystem::decode(input)?),
        59 => {
            let mesh = mesh::decode_psg(input)?;
            serde_json::to_vec(&serde_json::json!({"bounds": mesh.bind_bounds()?, "mesh": mesh}))
        }
        0 => serde_json::to_vec(&iso::root(input)?),
        1 => serde_json::to_vec(&iso::directory(input)?),
        2 => serde_json::to_vec(&Resource::parse(input)?.members),
        3 => return Resource::parse(input)?.extract(index),
        4 => return Ok(audio::decode_vag(input)?.wav()),
        5 => serde_json::to_vec(&audio::decode_vag(input)?.info),
        6 => serde_json::to_vec(&elf::inspect(input)?),
        7 => serde_json::to_vec(&texture::decode_pst(input)?.info),
        8 => return Ok(texture::decode_pst(input)?.rgba),
        9 => {
            let terrain = terrain::decode_tr2(input)?;
            serde_json::to_vec(&serde_json::json!({
                "width_cells": terrain.width_cells,
                "depth_cells": terrain.depth_cells,
                "spacing": terrain.spacing,
                "tiles": terrain.tiles.len(),
                "leaves": terrain.leaves.len(),
            }))
        }
        10 => {
            let heights = terrain::decode_tr2(input)?.heightfield()?;
            return Ok(heights.iter().flat_map(|h| h.to_le_bytes()).collect());
        }
        11 => serde_json::to_vec(&match index {
            0 => mesh::decode_psg(input)?,
            1 => mesh::decode_psg_for_scene(input)?,
            _ => return Err("unknown PSG coordinate space".into()),
        }),
        30 => serde_json::to_vec(&terrain::texture_layout(input)?),
        35 => return Ok(atv_core::ipu::decode_ipum(input)?.rgba),
        36 => serde_json::to_vec(&atv_core::ipu::decode_ipum(input)?.info),
        37 => serde_json::to_vec(&atv_core::scene::parse(input)?),
        38 => serde_json::to_vec(&atv_core::sky::layout(input)?),
        31 => {
            let layout = terrain::texture_layout(input)?;
            let record = layout
                .records
                .get(index)
                .ok_or("terrain texture index exceeds table")?;
            let mip = record.mips.last().ok_or("terrain texture has no mip")?;
            return Ok(input[mip.offset..mip.offset + mip.bytes].to_vec());
        }
        _ => return Err("unknown operation".into()),
    };
    json.map_err(|e| e.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn run(op: u32, index: usize) -> u32 {
    BUFFERS.with_borrow_mut(|b| match dispatch(&b.input, op, index) {
        Ok(out) => {
            b.output = out;
            1
        }
        Err(e) => {
            b.output = e.into_bytes();
            0
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_embeds_operation_numbers_and_events_without_reencoding() {
        let bytes = br#"{"matrix":[-0.0,1.2345678901234567,1e-30],"event":{"sample":"a\\b\"c","play":true}}"#;
        let raw = serialized_operation(bytes.to_vec()).unwrap();
        assert_eq!(serde_json::to_vec(&raw).unwrap(), bytes);
        for invalid in [b"{} trailing".as_slice(), b"{\"number\":NaN}", &[255]] {
            assert!(serialized_operation(invalid.to_vec()).is_err());
        }
    }
    #[test]
    fn soundtrack_reload_preserves_shuffle_and_explicit_selection_is_validated() {
        let mut catalog = String::from(
            "[Songs]\nNumberOfSongs=3\nOrdered=1\n[Artists]\nNumberOfArtists=1\nArtist1=Artist\n[Genres]\nNumberOfGenres=1\nGenre1=Rock\n",
        );
        for i in 1..=3 {
            catalog.push_str(&format!(
                "[Song {i}]\nFilename=song{i}.vag\nTitle=Song{i}\nArtist=1\nGenre=1\nLocked=0\n"
            ));
        }
        music(catalog.as_bytes(), 40, 0).unwrap();
        let first = music(&[], 41, 0).unwrap();
        music(catalog.as_bytes(), 40, 0).unwrap();
        let second = music(&[], 41, 0).unwrap();
        assert_ne!(first, second, "map reload must not restart the shuffle");
        let selected: serde_json::Value =
            serde_json::from_slice(&music(br#"{"select":2}"#, 41, 0).unwrap()).unwrap();
        assert_eq!(selected["filename"], "song3.vag");
        assert!(music(br#"{"select":3}"#, 41, 0).is_err());
        assert!(music(br#"{"select":-1}"#, 41, 0).is_err());
        assert!(music(br#"{"select":1.5}"#, 41, 0).is_err());
        assert!(music(br#"{"select":1,"unexpected":true}"#, 41, 0).is_err());
    }
    #[test]
    fn batch_rejects_unbounded_or_fractional_step_counts_before_simulation() {
        for count in ["0", "5", "1.5", "-1", "18446744073709551615"] {
            let input = format!("{{\"count\":{count}}}");
            assert_eq!(
                simulation_batch(input.as_bytes()).unwrap_err(),
                "simulation batch count must be 1 through 4"
            );
        }
    }
}
