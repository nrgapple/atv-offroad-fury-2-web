//! Browser transport for the recovered rider vocals, chassis/surface sounds
//! and venue world sounds (`atv_core::quad_sounds`, `atv_core::world_sounds`).
//! Rust chooses every sample, volume and pitch; the frontend only plays them.
use atv_core::{
    Result,
    engine_audio::OriginalRandom,
    quad_sounds::{self as qs, ChassisInput, DriveCommand, DriveInput, Voice},
    sound_effects::RiderMotionCache,
    spatial_audio::{self, CameraPose, Listener, Source},
    wheel::Vec3,
    world_sounds,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

/// Per-frame sound inputs gathered from the simulation (`001e5260` fields).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuadFrame {
    pub events: atv_core::vehicle::AudioEvents,
    pub chassis: ChassisInput,
    pub drive: DriveInput,
    pub origin: Vec3,
    pub velocity: Vec3,
    /// Body origin minus the water surface (None without a water object).
    pub water_depth: Option<f32>,
    /// Front-left and front-right wheel world translations (side emitters).
    pub wheels: [Vec3; 2],
}
/// Builds the frame record. `ground` is the terrain height below the origin.
pub fn quad_frame(
    events: atv_core::vehicle::AudioEvents,
    frame: &atv_core::vehicle::VehicleFrame,
    surfaces: [usize; 4],
    ground: Option<f32>,
    water: Option<f32>,
    centers: &[Vec3],
) -> QuadFrame {
    let wrecked = frame.wreck.phase != atv_core::wreck::Phase::Riding;
    // 001e5260: +994/+99c take the side's front wheel surface while grounded,
    // 8 once the quad is half a foot below the cached water surface, else -1.
    let terrain = std::array::from_fn(|side| {
        if !frame.grounded[side] {
            -1
        } else if water.is_some_and(|w| frame.origin[1] < w - 0.5) {
            8
        } else {
            surfaces[side] as i32
        }
    });
    QuadFrame {
        events,
        chassis: ChassisInput {
            impact: events.impact,
            wrecked,
            airborne: ground.is_some_and(|g| frame.origin[1] - g > 2.),
            chassis_contact: frame.chassis_contact_count + frame.obstacle_contact_count > 0,
        },
        drive: DriveInput {
            terrain,
            mph: (frame.speed_magnitude * atv_core::hud::MPH_FACTOR) as i32,
        },
        origin: frame.origin,
        velocity: frame.linear_velocity,
        water_depth: water.map(|w| frame.origin[1] - w),
        wheels: [
            centers.first().copied().unwrap_or(frame.origin),
            centers.get(1).copied().unwrap_or(frame.origin),
        ],
    }
}

struct QuadAudio {
    vocals: Option<qs::RiderVocals>,
    chassis: qs::Chassis,
    motion: RiderMotionCache,
    /// Position, velocity and volume of the clip the chassis voice started.
    chassis_source: Option<(Vec3, Vec3, i32)>,
    water: qs::WaterEntry,
    rider_water: qs::RiderWater,
    drive: [qs::DriveEmitter; 2],
    /// Latest (record, pitch, volume) of each side emitter.
    drive_level: [Option<(usize, i32, i32)>; 2],
    drive_random: OriginalRandom,
}
#[derive(Default)]
struct Venue {
    terrain: Vec<qs::DriveOnSound>,
    rates: HashMap<String, u32>,
    world: Option<world_sounds::World>,
}
thread_local! {
    static QUAD: RefCell<Option<QuadAudio>> = const { RefCell::new(None) };
    static VENUE: RefCell<Venue> = RefCell::default();
}

#[derive(Deserialize)]
struct Configure {
    rider: String,
    vehicle: String,
    female: bool,
}
#[derive(Deserialize)]
struct VenueRequest {
    aud: Option<String>,
    #[serde(default)]
    rates: HashMap<String, u32>,
}
#[derive(Deserialize)]
struct FrameRequest {
    dt: f32,
    camera: CameraPose,
    previous_camera: CameraPose,
    #[serde(default)]
    rider_position: Option<Vec3>,
    #[serde(default)]
    rider_impact: Vec3,
    #[serde(default)]
    rider_water: bool,
    #[serde(default)]
    quad: Option<QuadFrame>,
}
#[derive(Deserialize)]
struct Batch {
    frames: Vec<FrameRequest>,
    #[serde(default)]
    vocal_playing: bool,
    #[serde(default)]
    chassis_playing: bool,
    #[serde(default)]
    drive_playing: [bool; 2],
    #[serde(default)]
    ambient_playing: Vec<bool>,
    #[serde(default)]
    soultree_playing: Vec<bool>,
    #[serde(default)]
    world_dt: f32,
}
#[derive(Serialize, Default)]
struct FrameResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    vocal: Option<qs::VocalCue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chassis: Option<qs::ChassisCue>,
    #[serde(skip_serializing_if = "<[Vec<DriveCommand>; 2]>::is_empty_pair")]
    drive: [Vec<DriveCommand>; 2],
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stop_collisions: bool,
}
trait EmptyPair {
    fn is_empty_pair(&self) -> bool;
}
impl EmptyPair for [Vec<DriveCommand>; 2] {
    fn is_empty_pair(&self) -> bool {
        self[0].is_empty() && self[1].is_empty()
    }
}
/// Final stereo gains (0..1) and playback rate for one voice.
#[derive(Serialize, Clone, Copy)]
struct Gains {
    left: f32,
    right: f32,
    rate: f32,
}
fn gains(command: &spatial_audio::Command, volume: i32, rate: f32) -> Option<Gains> {
    if command.pitch < -1 || command.pitch >= 0x4000 {
        return None;
    }
    // 0014ed40 multiplies the sound volume into the attenuation before 002f4500.
    let (left, right) =
        spatial_audio::stereo(volume.clamp(0, 100) * command.volume / 100, command.pan);
    Some(Gains {
        left: left as f32 / 100.,
        right: right as f32 / 100.,
        rate: rate * command.pitch.max(0) as f32 / 4096.,
    })
}

pub fn operation(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    if input.len() > 512 * 1024 {
        return Err("quad audio request exceeds limit".into());
    }
    match op {
        // Enhanced-sound echo type for the loaded venue (authored).
        172 if index == 2 => {
            let request: serde_json::Value =
                serde_json::from_slice(input).map_err(|e| e.to_string())?;
            let stadium = request
                .get("stadium")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let enclosure = crate::SIMULATION.with_borrow(|sim| -> Result<f32> {
                let (Some(terrain), Some(vehicle)) = (&sim.terrain, &sim.vehicle) else {
                    return Ok(0.);
                };
                atv_core::echo::enclosure(
                    |x, z| terrain.contact(x, z).map(|hit| hit.map(|h| h.height)),
                    vehicle.frame().origin,
                )
            })?;
            serde_json::to_vec(&serde_json::json!({
                "echo": atv_core::echo::classify(stadium, enclosure),
                "enclosure": enclosure,
            }))
            .map_err(|e| e.to_string())
        }
        171 => {
            let request: Configure = serde_json::from_slice(input).map_err(|e| e.to_string())?;
            let banks = qs::VocalBanks::parse(request.rider.as_bytes())?;
            let chassis = qs::ChassisBanks::parse(request.vehicle.as_bytes())?;
            let lists = banks.lists(request.female).clone();
            let vocals: Vec<String> = lists.iter().flatten().cloned().collect();
            let mut samples = vec![qs::LANDING.to_string(), qs::SPLASH.to_string()];
            samples.extend(chassis.quad_to_quad.iter().chain(&chassis.wreck).cloned());
            QUAD.with_borrow_mut(|slot| {
                *slot = Some(QuadAudio {
                    vocals: (!vocals.is_empty())
                        .then(|| qs::RiderVocals::new(lists, 97211 + 9 * 47)),
                    chassis: qs::Chassis::new(chassis, true, 97211 + 10 * 47),
                    motion: RiderMotionCache::default(),
                    chassis_source: None,
                    water: Default::default(),
                    rider_water: Default::default(),
                    drive: Default::default(),
                    drive_level: [None; 2],
                    drive_random: OriginalRandom::with_seed(97211 + 11 * 47),
                })
            });
            serde_json::to_vec(&serde_json::json!({"vocals": vocals, "chassis": samples}))
                .map_err(|e| e.to_string())
        }
        172 => {
            let request: VenueRequest = serde_json::from_slice(input).map_err(|e| e.to_string())?;
            VENUE.with_borrow_mut(|venue| {
                if index == 1 {
                    venue.rates = request.rates;
                    let rates = &venue.rates;
                    if let Some(world) = &mut venue.world {
                        world.retain_samples(|sample| rates.contains_key(sample));
                    }
                    let (ambient, soultree) = venue
                        .world
                        .as_ref()
                        .map_or((&[][..], &[][..]), |w| (&w.ambient[..], &w.soultree[..]));
                    return serde_json::to_vec(
                        &serde_json::json!({"ambient": ambient, "soultree": soultree}),
                    )
                    .map_err(|e| e.to_string());
                }
                *venue = Venue::default();
                let Some(text) = request.aud else {
                    return Ok(br#"{"drive":[],"ambient":[],"soultree":[]}"#.to_vec());
                };
                let aud = qs::VenueAud::parse(text.as_bytes())?;
                venue.terrain = aud.terrain_drive_on()?;
                let (ambient, soultree) = world_sounds::parse(&aud)?;
                let result = serde_json::json!({
                    "drive": venue.terrain.iter().map(|s| s.sample.clone()).collect::<Vec<_>>(),
                    "ambient": ambient,
                    "soultree": soultree,
                });
                venue.world = Some(world_sounds::World::new(ambient, soultree, 97211 + 12 * 47));
                serde_json::to_vec(&result).map_err(|e| e.to_string())
            })
        }
        174 => {
            // Vehicle reset: forget latches, the frozen chassis source and the
            // current drive-on records (the frontend stops their voices).
            QUAD.with_borrow_mut(|slot| {
                if let Some(q) = slot {
                    q.chassis.reset();
                    if let Some(v) = &mut q.vocals {
                        v.reset();
                    }
                    q.chassis_source = None;
                    q.motion = RiderMotionCache::default();
                    q.drive = Default::default();
                    q.drive_level = [None; 2];
                }
            });
            Ok(b"null".to_vec())
        }
        173 => batch(input),
        _ => Err("unknown quad audio operation".into()),
    }
}

/// The final frame's listener, rider position and side wheel positions.
type LastFrame = (Listener, Option<Vec3>, Option<Vec3>, [Vec3; 2]);
#[allow(clippy::needless_range_loop)]
fn batch(input: &[u8]) -> Result<Vec<u8>> {
    let batch: Batch = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    if batch.frames.len() > 64 {
        return Err("invalid quad audio batch length".into());
    }
    VENUE.with_borrow_mut(|venue| {
        QUAD.with_borrow_mut(|slot| {
            let mut vocal = Voice {
                playing: batch.vocal_playing,
                in_range: true,
            };
            let mut chassis = Voice {
                playing: batch.chassis_playing,
                in_range: true,
            };
            let mut drive_playing = batch.drive_playing;
            let mut results = Vec::with_capacity(batch.frames.len());
            let mut last: Option<LastFrame> = None;
            let rates = &venue.rates;
            let native = |sample: &str| rates.get(sample).map(|&r| qs::native_pitch(r));
            for request in &batch.frames {
                let listener = spatial_audio::camera_listener(
                    &request.camera,
                    &request.previous_camera,
                    request.dt,
                )?;
                let mut result = FrameResult::default();
                let Some(q) = slot.as_mut() else {
                    results.push(result);
                    last = Some((listener, None, None, [[0.; 3]; 2]));
                    continue;
                };
                let distance = |p: Vec3| {
                    let d: Vec3 = std::array::from_fn(|i| p[i] - listener.position[i]);
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                };
                let rider_position = request
                    .rider_position
                    .or(request.quad.as_ref().map(|f| f.origin));
                if let Some(position) = rider_position {
                    q.motion.update(position, 1. / request.dt)?;
                    vocal.in_range = distance(position) <= qs::VOCAL_MAX;
                }
                if let Some(frame) = &request.quad {
                    chassis.in_range = distance(frame.origin) <= qs::CHASSIS_MAX;
                    let events = frame.events;
                    // 001eb6f0 during physics, then 001e8690 after it.
                    if events.quad_contact > 1. {
                        if let Some(v) = &mut q.vocals {
                            result.vocal = v
                                .request(qs::event::BANG, events.quad_contact, &mut vocal)
                                .or(result.vocal);
                        }
                        result.chassis = q.chassis.quad_contact(&mut chassis).or(result.chassis);
                    }
                    if let Some(v) = &mut q.vocals {
                        if let Some((event, magnitude)) =
                            events.wreck_entry.and_then(qs::WreckEntry::event)
                        {
                            result.vocal = v
                                .request(event, magnitude, &mut vocal)
                                .or(result.vocal.take());
                        } else if events.hard_landing {
                            result.vocal = v
                                .request(qs::event::JOSTLE, 0., &mut vocal)
                                .or(result.vocal.take());
                        }
                    }
                    if q.water.update(frame.water_depth, frame.velocity)
                        | q.rider_water.update(request.rider_water)
                    {
                        result.chassis = q.chassis.splash(&mut chassis).or(result.chassis.take());
                    }
                    let (cue, stop) = q.chassis.update(frame.chassis, &mut chassis);
                    result.chassis = cue.or(result.chassis.take());
                    result.stop_collisions = stop;
                    // 002cdb80 sets the voice position only when a clip starts.
                    if let Some(cue) = &result.chassis {
                        q.chassis_source = Some((frame.origin, frame.velocity, cue.volume));
                    }
                    if !venue.terrain.is_empty() {
                        result.drive = qs::drive_on(
                            &mut q.drive,
                            &venue.terrain,
                            &native,
                            frame.drive,
                            drive_playing,
                            &mut q.drive_random,
                        );
                        for side in 0..2 {
                            for command in &result.drive[side] {
                                match *command {
                                    DriveCommand::Stop => {
                                        drive_playing[side] = false;
                                        q.drive_level[side] = None;
                                    }
                                    DriveCommand::Start {
                                        record,
                                        pitch,
                                        volume,
                                        ..
                                    }
                                    | DriveCommand::Update {
                                        record,
                                        pitch,
                                        volume,
                                    } => {
                                        if matches!(command, DriveCommand::Start { .. }) {
                                            drive_playing[side] = true;
                                        }
                                        q.drive_level[side] = Some((record, pitch, volume));
                                    }
                                }
                            }
                        }
                    }
                    if let Some(v) = &mut q.vocals {
                        let impact = request.rider_impact;
                        let magnitude = ((impact[0] * impact[0] + impact[1] * impact[1])
                            + impact[2] * impact[2])
                            .sqrt();
                        let cue = v.update(magnitude, frame.chassis.wrecked, &mut vocal);
                        result.vocal = cue.or(result.vocal.take());
                    }
                }
                last = Some((
                    listener,
                    rider_position,
                    request.quad.as_ref().map(|f| f.origin),
                    request.quad.as_ref().map_or([[0.; 3]; 2], |f| f.wheels),
                ));
                results.push(result);
            }
            let mut spatial = serde_json::Map::new();
            if let (Some((listener, rider, _, wheels)), Some(q)) = (&last, slot.as_ref()) {
                let source = |position, velocity, min_distance, max_distance| Source {
                    position,
                    velocity,
                    min_distance,
                    max_distance,
                };
                if let Some(position) = rider {
                    let command = spatial_audio::command(
                        source(*position, q.motion.velocity, qs::VOCAL_MIN, qs::VOCAL_MAX),
                        std::slice::from_ref(listener),
                    )?;
                    spatial.insert(
                        "vocal".into(),
                        serde_json::to_value(gains(&command, 100, 1.))
                            .map_err(|e| e.to_string())?,
                    );
                }
                if let Some((position, velocity, volume)) = q.chassis_source {
                    let command = spatial_audio::command(
                        source(position, velocity, qs::CHASSIS_MIN, qs::CHASSIS_MAX),
                        std::slice::from_ref(listener),
                    )?;
                    spatial.insert(
                        "chassis".into(),
                        serde_json::to_value(gains(&command, volume, 1.))
                            .map_err(|e| e.to_string())?,
                    );
                }
                let mut drive = Vec::new();
                for side in 0..2 {
                    let level = q.drive_level[side].and_then(|(record, pitch, volume)| {
                        let sound = venue.terrain.get(record - 1)?;
                        let rate = *venue.rates.get(&sound.sample)? as f32;
                        if sound.max_distance <= sound.min_distance || rate <= 0. {
                            return None;
                        }
                        let command = spatial_audio::command(
                            source(
                                wheels[side],
                                [0.; 3],
                                sound.min_distance,
                                sound.max_distance,
                            ),
                            std::slice::from_ref(listener),
                        )
                        .ok()?;
                        // SPU pitch 4096 plays at 48kHz.
                        gains(&command, volume, pitch as f32 * 48000. / 4096. / rate)
                    });
                    drive.push(level);
                }
                spatial.insert(
                    "drive".into(),
                    serde_json::to_value(drive).map_err(|e| e.to_string())?,
                );
            }
            let world = match (&mut venue.world, &last) {
                (Some(world), Some((listener, ..))) if batch.world_dt > 0. => Some(world.step(
                    batch.world_dt.min(0.1),
                    listener,
                    &batch.ambient_playing,
                    &batch.soultree_playing,
                )?),
                _ => None,
            };
            serde_json::to_vec(
                &serde_json::json!({"frames": results, "spatial": spatial, "world": world}),
            )
            .map_err(|e| e.to_string())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn camera(x: f32) -> serde_json::Value {
        serde_json::json!({"position": [x, 5., 0.], "target": [x, 5., 1.]})
    }
    #[test]
    fn batch_orders_contact_wreck_and_world_commands() {
        let rider = "[MaleBang]\nNumSounds = 1\nSound_1 = Rider\\mgrunt\\mgrunt01.vag\n[MaleHardWreck]\nNumSounds = 1\nSound_1 = Rider\\mhurt\\mhurt10.vag\n";
        let vehicle = "[QuadToQuadImpact]\nNumSounds = 1\nSound_1 = Impact\\impact06.vag\n[Idle-250A]\nNumSounds = 0\nSound_1 =\n[Wreck]\nNumSounds = 1\nSound_1 = Impact\\wreck04.vag\n";
        let config: serde_json::Value = serde_json::from_slice(
            &operation(
                serde_json::json!({"rider": rider, "vehicle": vehicle, "female": false})
                    .to_string()
                    .as_bytes(),
                171,
                0,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            config["vocals"],
            serde_json::json!(["rider/mgrunt/mgrunt01.vag", "rider/mhurt/mhurt10.vag"])
        );
        let aud = "[Sounds]\nNumAmbientSounds=1\nNumTerrainDriveOnSounds=0\n[AmbientSound1]\nSoundResourceFile=Crowd\\Indoor\\icrowd01.VAG\nLooping=T\nCycleTime=0\nRandomTriggerPercent=100\n";
        let venue: serde_json::Value = serde_json::from_slice(
            &operation(
                serde_json::json!({"aud": aud}).to_string().as_bytes(),
                172,
                0,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(venue["ambient"][0]["sample"], "crowd/indoor/icrowd01.vag");
        let mut quad = QuadFrame::default();
        quad.events.quad_contact = 4.;
        quad.events.wreck_entry = Some(qs::WreckEntry {
            fall: false,
            impact: 55.,
        });
        let request = serde_json::json!({
            "frames": [{"dt": 1. / 60., "camera": camera(10.), "previous_camera": camera(10.), "rider_position": [0., 0., 0.], "quad": quad}],
            "world_dt": 1. / 60., "ambient_playing": [false],
        });
        let result: serde_json::Value =
            serde_json::from_slice(&operation(request.to_string().as_bytes(), 173, 0).unwrap())
                .unwrap();
        let frame = &result["frames"][0];
        // The contact's Bang takes the single vocal voice before the wreck entry.
        assert_eq!(frame["vocal"]["sample"], "rider/mgrunt/mgrunt01.vag");
        assert_eq!(frame["chassis"]["volume"], 60);
        assert!(result["spatial"]["vocal"]["left"].as_f64().unwrap() > 0.);
        // 002cdb80 scales the landing by its impact; the chassis voice gain
        // follows the cue volume instead of playing every clip at full scale.
        let landing = |impact: f32| {
            let mut airborne = QuadFrame::default();
            airborne.chassis.airborne = true;
            let mut lands = QuadFrame::default();
            lands.chassis.impact = impact;
            let frame = |quad: &QuadFrame| serde_json::json!({"dt": 1. / 60., "camera": camera(10.), "previous_camera": camera(10.), "quad": quad});
            let request = serde_json::json!({
                "frames": [frame(&airborne), frame(&lands)], "world_dt": 2. / 60.,
            });
            let result: serde_json::Value =
                serde_json::from_slice(&operation(request.to_string().as_bytes(), 173, 0).unwrap())
                    .unwrap();
            let chassis = &result["spatial"]["chassis"];
            (
                result["frames"][1]["chassis"]["volume"].as_i64().unwrap(),
                chassis["left"].as_f64().unwrap() + chassis["right"].as_f64().unwrap(),
            )
        };
        let (soft, soft_gain) = landing(15.);
        let (hard, hard_gain) = landing(60.);
        assert_eq!((soft, hard), (25, 100));
        assert!(
            soft_gain > 0. && soft_gain < hard_gain * 0.3,
            "{soft_gain} vs {hard_gain}"
        );
        assert_eq!(result["world"]["ambient"][0]["action"], "start");
        assert!(operation(b"{}", 175, 0).is_err());
    }
}
