//! Authored networking adapter. All packet validation, interpolation, session
//! identity and spatial/sample decisions run in Rust; browsers transport bytes.
#[path = "multiplayer_animation.rs"]
mod animation;
#[path = "multiplayer_world.rs"]
mod world;
use super::{EnginePlayback, Simulation, vehicle_world};
use atv_core::{
    Result,
    engine_audio::{
        EngineAudioConfig, EngineAudioState, OriginalRandom, PlaybackPosition, sample_index,
    },
    spatial_audio::{Listener, Source, command},
};
use atv_net::{Packet, PoseBuffer, RiderState, Snapshot, Transform};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};
pub use world::{collide, spawn, synchronize_train};

#[derive(Default)]
struct Client {
    rigs: BTreeMap<usize, animation::Rig>,
    rig_revision: u64,
    local_animations: BTreeMap<u8, animation::Local>,
    baseline: Option<Snapshot>,
    pending: Option<Snapshot>,
    last_sent_ms: u32,
    last_full_ms: u32,
    force_full: bool,
    sent_messages: u64,
    full_messages: u64,
    delta_messages: u64,
    suppressed: u64,
    last_packet_bytes: usize,
    peer: Option<u8>,
    code: String,
    lobby: Option<atv_net::Lobby>,
    quad: u16,
    rider: u16,
    sequence: u32,
    crash_serial: u32,
    wreck: bool,
    buffers: BTreeMap<u8, PoseBuffer>,
    engines: BTreeMap<u8, EnginePlayback>,
    heard_crash: BTreeMap<u8, u32>,
    last_sample: Option<u64>,
    contacts: u64,
    received: u64,
    sent: u64,
    rtt_ms: Option<u32>,
}
thread_local! {
    static CLIENT: RefCell<Client> = RefCell::default();
    static AUDIO_TABLE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static EFFECTS: RefCell<Option<atv_core::sound_effects::Config>> = const { RefCell::new(None) };
}
pub fn audio_table(bytes: &[u8]) {
    AUDIO_TABLE.with_borrow_mut(|v| {
        v.clear();
        v.extend_from_slice(bytes);
    });
}
pub fn effect_table(bytes: &[u8]) -> Result<()> {
    let config = atv_core::sound_effects::Config::parse(bytes)?;
    EFFECTS.with_borrow_mut(|v| *v = Some(config));
    Ok(())
}
pub fn observe_wreck(wreck: bool) {
    CLIENT.with_borrow_mut(|c| {
        if wreck && !c.wreck {
            c.crash_serial = c.crash_serial.wrapping_add(1);
        }
        c.wreck = wreck;
    });
}
fn mapped<T>(r: atv_net::Result<T>) -> Result<T> {
    r.map_err(|e| e.to_string())
}
#[cfg(test)]
fn matrix_nodes(matrices: Vec<[f32; 16]>) -> Result<Vec<Transform>> {
    matrices
        .into_iter()
        .map(|m| mapped(Transform::from_column_major(m)))
        .collect()
}
fn snapshot(c: &mut Client, sim: &Simulation, now: u32, paused: bool) -> Result<Vec<u8>> {
    let peer = c.peer.ok_or("join multiplayer first")?;
    if c.lobby.as_ref().is_some_and(|l| l.players.len() < 2) {
        c.suppressed += 1;
        return Ok(Vec::new());
    }
    let frame = sim.vehicle.as_ref().ok_or("vehicle is not ready")?.frame();
    let atv = mapped(Transform::from_column_major(vehicle_world(&frame)))?;
    let (rider, velocity, state) =
        if let Some(wreck) = sim.wreck_animator.as_ref().and_then(|a| a.frame()) {
            (
                mapped(Transform::from_column_major(wreck.world_matrix))?,
                wreck.linear_velocity,
                RiderState::Wreck,
            )
        } else {
            (
                atv,
                frame.linear_velocity,
                if frame.wreck.phase == atv_core::wreck::Phase::Recovering {
                    RiderState::Recovering
                } else if frame.grounded.iter().any(|v| *v) {
                    RiderState::Riding
                } else {
                    RiderState::Airborne
                },
            )
        };
    let recipe = sim
        .rider_animator
        .as_ref()
        .ok_or("rider pose missing")?
        .recipe();
    c.sequence = c.sequence.wrapping_add(1);
    let basis = atv_core::dynamics::basis(frame.orientation);
    let angular_velocity = std::array::from_fn(|i| {
        (0..3)
            .map(|j| basis[j][i] * frame.local_angular_velocity[j])
            .sum()
    });
    let s = Snapshot {
        peer,
        sequence: c.sequence,
        time_ms: now,
        atv_model: c.quad,
        rider_model: c.rider,
        atv,
        velocity: if paused {
            [0.; 3]
        } else {
            frame.linear_velocity
        },
        angular_velocity: if paused { [0.; 3] } else { angular_velocity },
        animation: Some(Box::new(atv_net::AnimationState {
            steering: frame.visual_steering_angle,
            wheels: frame.wheel_angles,
            compression: frame.wheel_compression,
            clips: recipe.clips,
            count: recipe.count,
            phase: recipe.phase,
            first_weight: recipe.first_weight,
            pair_weight: recipe.pair_weight,
            look_phase: recipe.look_phase,
            paused,
        })),
        atv_nodes: Vec::new(),
        rider_nodes: Vec::new(),
        rider_state: state,
        rider,
        rider_velocity: if paused { [0.; 3] } else { velocity },
        rpm: frame.rpm,
        throttle: if paused {
            0.
        } else {
            sim.resolved_input
                .as_ref()
                .and_then(|v| v["throttle"].as_f64())
                .unwrap_or(0.) as f32
        },
        speed: if paused { 0. } else { frame.speed_magnitude },
        crash_serial: c.crash_serial,
        train_time: sim.train.as_ref().map(|t| t.time),
        train_running: !paused,
        platform: world::platform(sim, &frame)?,
    };
    let nearest = c
        .buffers
        .values()
        .filter_map(|b| b.latest())
        .map(|(p, _)| {
            p.atv
                .position
                .iter()
                .zip(s.atv.position)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                .sqrt()
        })
        .fold(f32::INFINITY, f32::min);
    let transition = c.baseline.as_ref().is_none_or(|b| {
        b.rider_state != s.rider_state
            || b.crash_serial != s.crash_serial
            || b.animation.as_ref().is_some_and(|a| a.paused != paused)
            || b.platform.as_ref().map(|p| p.car) != s.platform.as_ref().map(|p| p.car)
            || b.atv
                .position
                .iter()
                .zip(s.atv.position)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                > 16. * 16.
    });
    let elapsed = now.wrapping_sub(c.last_sent_ms);
    let moving = s
        .velocity
        .iter()
        .chain(s.rider_velocity.iter())
        .map(|v| v * v)
        .sum::<f32>()
        > 0.25;
    let interval = if paused {
        5000
    } else if !moving && state == RiderState::Riding {
        if peer == 0 && s.train_time.is_some() {
            500
        } else {
            1000
        }
    } else if state == RiderState::Wreck
        || state == RiderState::Airborne
        || s.speed > 35.
        || nearest < 40.
    {
        50
    } else if nearest > 300. {
        200
    } else {
        100
    };
    if !c.force_full && !transition && elapsed < interval {
        c.suppressed += 1;
        return Ok(Vec::new());
    }
    let full = c.force_full
        || transition
        || now.wrapping_sub(c.last_full_ms) >= 2000
        || c.baseline.is_none();
    let packet = if full {
        Packet::Snapshot((s.clone()).into())
    } else {
        let patch = mapped(atv_net::delta(&s, c.baseline.as_ref().unwrap()))?;
        // An all-block patch is no smaller than independent full state. Avoid
        // baseline reconstruction work on the relay when it buys no bytes.
        if patch.mask == 15 {
            Packet::Snapshot(s.clone().into())
        } else {
            Packet::Delta(Box::new(patch))
        }
    };
    let output = mapped(atv_net::encode(&packet))?;
    c.pending = Some(s);
    c.last_packet_bytes = output.len();
    Ok(output)
}
fn status(c: &Client) -> Value {
    let environment = c
        .lobby
        .as_ref()
        .and_then(|lobby| atv_net::map_environment(lobby.map));
    let next_map = c
        .lobby
        .as_ref()
        .and_then(|lobby| atv_net::next_map(lobby.map));
    json!({"peer":c.peer,"code":c.code,"lobby":c.lobby,"environment":environment,"nextMap":next_map,"receivedBytes":c.received,"sentBytes":c.sent,"rttMs":c.rtt_ms,"contacts":c.contacts,"network":{"messages":c.sent_messages,"full":c.full_messages,"delta":c.delta_messages,"suppressed":c.suppressed,"lastPacketBytes":c.last_packet_bytes,"animation":"local","protocol":atv_net::VERSION,"rigKeys":c.rigs.keys().collect::<Vec<_>>(),"rigRevision":c.rig_revision}})
}
fn lobby(c: &mut Client, value: atv_net::Lobby) {
    if c.lobby.as_ref() != Some(&value) {
        c.force_full = true;
        c.baseline = None;
        c.local_animations.clear();
    }
    if c.lobby.as_ref().is_some_and(|old| {
        old.map != value.map || old.epoch != value.epoch || old.started != value.started
    }) {
        c.buffers.clear();
        c.engines.clear();
        c.heard_crash.clear();
    }
    c.buffers
        .retain(|peer, _| value.players.iter().any(|p| p.id == *peer));
    c.engines
        .retain(|peer, _| value.players.iter().any(|p| p.id == *peer));
    c.heard_crash
        .retain(|peer, _| value.players.iter().any(|p| p.id == *peer));
    c.lobby = Some(value);
}
#[derive(Deserialize, Default)]
struct Playing {
    peer: u8,
    #[serde(default)]
    duration: f32,
    #[serde(default)]
    playhead: f32,
}
#[derive(Deserialize)]
struct Presentation {
    listener: Listener,
    #[serde(default)]
    playing: Vec<Playing>,
    #[serde(default)]
    audible: bool,
}
fn presentation(c: &mut Client, input: &[u8], now: u64, sim: &Simulation) -> Result<Vec<u8>> {
    let req: Presentation = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    if req.playing.len() > 4
        || req.playing.iter().any(|p| {
            !p.duration.is_finite() || !p.playhead.is_finite() || p.duration < 0. || p.playhead < 0.
        })
    {
        return Err("invalid remote playback history".into());
    }
    let dt = c
        .last_sample
        .map_or(0.016, |last| now.saturating_sub(last) as f32 * 0.001)
        .clamp(0., 0.1);
    c.last_sample = Some(now);
    let mut peers = Vec::new();
    for (&id, buffer) in &c.buffers {
        let Some(mut pose) = buffer.sample(now) else {
            continue;
        };
        world::rebase(&mut pose.snapshot, sim)?;
        let s = &pose.snapshot;
        let spatial = command(
            Source {
                position: s.atv.position,
                velocity: s.velocity,
                min_distance: 15.,
                max_distance: 300.,
            },
            &[req.listener],
        )?;
        let mut engine = Value::Null;
        if req.audible && !pose.stale && !c.engines.contains_key(&id) {
            let config =
                AUDIO_TABLE.with_borrow(|t| EngineAudioConfig::parse(t, s.atv_model as u32))?;
            c.engines.insert(
                id,
                EnginePlayback {
                    config,
                    state: EngineAudioState::default(),
                    previous: [usize::MAX; 10],
                    random: OriginalRandom::with_seed(id as u64 + 1),
                },
            );
        }
        let playing = req.playing.iter().find(|p| p.peer == id);
        if let Some(player) = c
            .engines
            .get_mut(&id)
            .filter(|_| req.audible && !pose.stale)
        {
            let selection = player.state.step(
                &player.config,
                dt,
                s.throttle,
                s.rider_state == RiderState::Riding,
                s.speed,
                PlaybackPosition {
                    duration: playing.map_or(0., |p| p.duration),
                    playhead: playing.map_or(0., |p| p.playhead),
                },
            )?;
            if let Some(v) = selection {
                let kind = v.sound as usize;
                let samples = &player.config.samples[kind];
                if !samples.is_empty() {
                    let random = if samples.len() > 1 {
                        player.random.next_u31()
                    } else {
                        0
                    };
                    let choice = sample_index(samples.len(), &mut player.previous[kind], random)?;
                    engine =
                        json!({"sample":samples[choice],"looping":kind==1,"overlap":v.overlap});
                }
            } else if playing.is_none() && !player.config.samples[1].is_empty() {
                engine = json!({"sample":player.config.samples[1].last(),"looping":true,"overlap":false});
            }
        }
        let previous = c.heard_crash.insert(id, s.crash_serial);
        let crash = if req.audible && !pose.stale && previous.is_some_and(|v| v != s.crash_serial) {
            EFFECTS
                .with_borrow(|cfg| {
                    cfg.as_ref()
                        .map(|cfg| {
                            let index = s.crash_serial as usize % cfg.body_impact.len();
                            let gain = command(
                                Source {
                                    position: s.rider.position,
                                    velocity: s.rider_velocity,
                                    min_distance: cfg.min_distance,
                                    max_distance: cfg.max_distance,
                                },
                                &[req.listener],
                            );
                            gain.map(|gain| json!({"sample":cfg.body_impact[index],"spatial":gain}))
                        })
                        .transpose()
                })?
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let matrices = if s.animation.is_some() {
            let key = usize::from(s.atv_model) * 2 + usize::from(s.rider_model);
            if let Some(rig) = c.rigs.get(&key) {
                if let std::collections::btree_map::Entry::Vacant(e) = c.local_animations.entry(id)
                {
                    e.insert(animation::Local::new(rig)?);
                }
                Some(
                    c.local_animations
                        .get_mut(&id)
                        .unwrap()
                        .pose(rig, s, dt, pose.stale)?,
                )
            } else {
                None
            }
        } else {
            Some((
                s.atv_nodes
                    .iter()
                    .map(|t| t.to_column_major())
                    .collect::<Vec<_>>(),
                s.rider_nodes
                    .iter()
                    .map(|t| t.to_column_major())
                    .collect::<Vec<_>>(),
            ))
        };
        let (quad_matrices, rider_matrices) = matrices.unwrap_or_default();
        peers.push(json!({"peer":id,"atv_model":s.atv_model,"rider_model":s.rider_model,"world_matrix":s.atv.to_column_major(),"quad_node_matrices":quad_matrices,"rider_world_matrix":s.rider.to_column_major(),"rider_node_matrices":rider_matrices,"position":s.atv.position,"platform":s.platform.as_ref().map(|p|p.car),"rider_state":s.rider_state,"crash_serial":s.crash_serial,"rpm":s.rpm,"stale":pose.stale,"extrapolated_ms":pose.extrapolated_ms,"spatial":spatial,"engine":engine,"crash":crash}));
    }
    serde_json::to_vec(&json!({"state":status(c),"peers":peers})).map_err(|e| e.to_string())
}
pub fn dispatch(input: &[u8], op: u32, index: usize, sim: &Simulation) -> Result<Vec<u8>> {
    if input.len() > 65536 {
        return Err("multiplayer adapter input too large".into());
    }
    CLIENT.with_borrow_mut(|c| match op {
        80 => {
            let v: Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
            let quad = v["quad_id"].as_u64().ok_or("missing quad")?;
            let rider = v["rider_id"].as_u64().ok_or("missing rider")?;
            if quad > 26 || quad == 0 || rider > 1 {
                return Err("unsupported player model".into());
            }
            *c = Client {
                quad: quad as u16,
                rider: rider as u16,
                ..Default::default()
            };
            serde_json::to_vec(&status(c)).map_err(|e| e.to_string())
        }
        109 => {
            let bytes = input.len();
            if let Some(s) = c.pending.take() {
                let kind = input.get(5).copied().unwrap_or(0);
                if !matches!(kind, 9 | 10) || input.len() != c.last_packet_bytes {
                    return Err("invalid snapshot send receipt".into());
                }
                c.last_sent_ms = s.time_ms;
                if kind == 9 {
                    c.last_full_ms = s.time_ms;
                    c.full_messages += 1;
                } else {
                    c.delta_messages += 1;
                }
                c.force_full = false;
                c.baseline = Some(s);
                c.sent += bytes as u64;
                c.sent_messages += 1;
            }
            Ok(b"null".to_vec())
        }
        81 => snapshot(c, sim, index as u32, input.first() == Some(&1)),
        82 => {
            let packet = mapped(atv_net::decode(input))?;
            c.received += input.len() as u64;
            match &packet {
                Packet::Welcome {
                    peer,
                    code,
                    lobby: value,
                } => {
                    c.peer = Some(*peer);
                    c.code = String::from_utf8(code.to_vec()).map_err(|e| e.to_string())?;
                    lobby(c, value.clone());
                }
                Packet::Lobby(value) => lobby(c, value.clone()),
                Packet::Snapshot(s) => {
                    let allowed = c.lobby.as_ref().is_some_and(|l| {
                        l.started
                            && l.players.iter().any(|p| {
                                p.id == s.peer
                                    && p.atv_model == s.atv_model
                                    && p.rider_model == s.rider_model
                            })
                    });
                    if allowed && Some(s.peer) != c.peer {
                        mapped(
                            c.buffers
                                .entry(s.peer)
                                .or_default()
                                .push(s.as_ref().clone(), index as u64),
                        )?;
                    }
                }
                Packet::Leave { peer } => {
                    c.buffers.remove(peer);
                    c.engines.remove(peer);
                    c.heard_crash.remove(peer);
                }
                Packet::Pong { nonce } => c.rtt_ms = Some((index as u32).wrapping_sub(*nonce)),
                _ => return Err("unexpected relay packet".into()),
            }
            serde_json::to_vec(&json!({"state":status(c)})).map_err(|e| e.to_string())
        }
        83 => presentation(c, input, index as u64, sim),
        84 => {
            *c = Client::default();
            Ok(b"null".to_vec())
        }
        85 => {
            let packet: Packet = serde_json::from_slice(input).map_err(|e| e.to_string())?;
            mapped(atv_net::encode(&packet))
        }
        _ => Err("unknown multiplayer adapter operation".into()),
    })
}

pub fn install_rig(input: &[u8], key: usize) -> Result<Vec<u8>> {
    let rig = animation::load(input, key)?;
    CLIENT.with_borrow_mut(|c| {
        if c.rigs.len() >= 8 && !c.rigs.contains_key(&key) {
            let active = c
                .lobby
                .as_ref()
                .map(|l| {
                    l.players
                        .iter()
                        .map(|p| usize::from(p.atv_model) * 2 + usize::from(p.rider_model))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let victim = c
                .rigs
                .keys()
                .find(|k| !active.contains(k))
                .copied()
                .ok_or("remote rig cache limit")?;
            c.rigs.remove(&victim);
        }
        c.rigs.insert(key, rig);
        c.rig_revision += 1;
        serde_json::to_vec(&json!({"rigRevision":c.rig_revision})).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_net::{Lobby, Participant};

    fn roster(started: bool) -> Lobby {
        Lobby {
            host: 0,
            epoch: 1,
            map: 4,
            started,
            players: vec![
                Participant {
                    id: 0,
                    name: "Host".into(),
                    atv_model: 1,
                    rider_model: 0,
                },
                Participant {
                    id: 1,
                    name: "Guest".into(),
                    atv_model: 2,
                    rider_model: 1,
                },
            ],
        }
    }
    fn setup(started: bool) {
        CLIENT.with_borrow_mut(|c| {
            *c = Client {
                peer: Some(0),
                quad: 1,
                rider: 0,
                lobby: Some(roster(started)),
                ..Client::default()
            }
        });
        AUDIO_TABLE.with_borrow_mut(Vec::clear);
        EFFECTS.with_borrow_mut(|e| *e = None);
    }
    fn receive(packet: Packet, now: usize) -> Result<Vec<u8>> {
        dispatch(
            &atv_net::encode(&packet).unwrap(),
            82,
            now,
            &Simulation::default(),
        )
    }
    fn remote(sequence: u32, time_ms: u32) -> Snapshot {
        let mut s = Snapshot {
            peer: 1,
            sequence,
            time_ms,
            atv_model: 2,
            rider_model: 1,
            ..Snapshot::default()
        };
        s.atv.position = [10., 2., 0.];
        s.velocity = [5., 0., 0.];
        s.atv_nodes = vec![Transform::default(); 27];
        s.rider_nodes = vec![Transform::default(); 27];
        s
    }
    fn request(audible: bool) -> Vec<u8> {
        serde_json::to_vec(&json!({"listener":{"position":[0.,0.,0.],"forward":[0.,0.,1.],"velocity":[0.,0.,0.]},"audible":audible})).unwrap()
    }
    #[test]
    fn lobby_handshake_preserves_identity_and_female_model() {
        setup(false);
        dispatch(
            br#"{"quad_id":2,"rider_id":1}"#,
            80,
            0,
            &Simulation::default(),
        )
        .unwrap();
        receive(
            Packet::Welcome {
                peer: 1,
                code: *b"ABCD2345",
                lobby: roster(false),
            },
            0,
        )
        .unwrap();
        CLIENT.with_borrow(|c| {
            assert_eq!(c.peer, Some(1));
            assert_eq!(c.code, "ABCD2345");
            assert_eq!(c.quad, 2);
            assert_eq!(c.rider, 1);
            assert!(!c.lobby.as_ref().unwrap().started);
        });
    }
    #[test]
    fn ninth_lobby_map_keeps_its_rust_environment_and_advances_to_appended_maps() {
        setup(false);
        let output = receive(
            Packet::Welcome {
                peer: 0,
                code: *b"ABCD2345",
                lobby: Lobby {
                    map: 9,
                    ..roster(false)
                },
            },
            0,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["state"]["environment"], "free05");
        assert_eq!(value["state"]["nextMap"], 10);
        let output = receive(
            Packet::Lobby(Lobby {
                map: 1,
                epoch: 2,
                ..roster(false)
            }),
            10,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["state"]["environment"], "env01");
        assert_eq!(value["state"]["nextMap"], 2);
        let output = dispatch(
            br#"{"quad_id":1,"rider_id":0}"#,
            80,
            0,
            &Simulation::default(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["environment"], Value::Null);
        assert_eq!(value["nextMap"], Value::Null);
    }

    #[test]
    fn rejects_invalid_configuration_without_resetting_live_session() {
        setup(true);
        for input in [
            br#"{"quad_id":0,"rider_id":0}"#.as_slice(),
            br#"{"quad_id":27,"rider_id":0}"#,
            br#"{"quad_id":1,"rider_id":2}"#,
        ] {
            assert!(dispatch(input, 80, 0, &Simulation::default()).is_err());
            CLIENT.with_borrow(|c| assert_eq!(c.peer, Some(0)));
        }
    }
    #[test]
    fn snapshots_require_started_roster_identity_and_model_match() {
        setup(false);
        receive(Packet::Snapshot((remote(1, 0)).into()), 0).unwrap();
        CLIENT.with_borrow(|c| assert!(c.buffers.is_empty()));
        receive(Packet::Lobby(roster(true)), 0).unwrap();
        let mut wrong = remote(1, 0);
        wrong.atv_model = 3;
        receive(Packet::Snapshot((wrong).into()), 0).unwrap();
        let mut local = remote(1, 0);
        local.peer = 0;
        local.atv_model = 1;
        local.rider_model = 0;
        receive(Packet::Snapshot((local).into()), 0).unwrap();
        let mut unknown = remote(1, 0);
        unknown.peer = 3;
        receive(Packet::Snapshot((unknown).into()), 0).unwrap();
        CLIENT.with_borrow(|c| assert!(c.buffers.is_empty()));
        receive(Packet::Snapshot((remote(1, 100)).into()), 100).unwrap();
        receive(Packet::Snapshot((remote(1, 100)).into()), 120).unwrap();
        CLIENT.with_borrow(|c| assert_eq!(c.buffers.get(&1).unwrap().len(), 1));
    }
    #[test]
    fn leave_clears_slot_reuse_and_out_of_order_state() {
        setup(true);
        receive(Packet::Snapshot((remote(100, 1000)).into()), 1000).unwrap();
        CLIENT.with_borrow_mut(|c| {
            c.heard_crash.insert(1, 10);
        });
        receive(Packet::Leave { peer: 1 }, 1010).unwrap();
        CLIENT.with_borrow(|c| {
            assert!(!c.buffers.contains_key(&1));
            assert!(!c.heard_crash.contains_key(&1));
        });
        receive(Packet::Snapshot((remote(1, 1)).into()), 1020).unwrap();
        CLIENT.with_borrow(|c| {
            assert_eq!(
                c.buffers
                    .get(&1)
                    .unwrap()
                    .sample(1020)
                    .unwrap()
                    .snapshot
                    .sequence,
                1
            )
        });
    }
    #[test]
    fn map_start_and_epoch_changes_clear_remote_presentation() {
        for mode in 0..3 {
            setup(true);
            receive(Packet::Snapshot((remote(1, 0)).into()), 0).unwrap();
            let mut changed = roster(true);
            match mode {
                0 => changed.map = 5,
                1 => changed.started = false,
                _ => changed.epoch = 2,
            }
            receive(Packet::Lobby(changed), 10).unwrap();
            CLIENT.with_borrow(|c| assert!(c.buffers.is_empty()));
        }
    }
    #[test]
    fn silent_and_stale_players_render_without_audio_assets() {
        setup(true);
        receive(Packet::Snapshot((remote(1, 100)).into()), 100).unwrap();
        for (audible, now) in [(false, 100usize), (true, 1200usize)] {
            let bytes = dispatch(&request(audible), 83, now, &Simulation::default()).unwrap();
            let out: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(out["peers"].as_array().unwrap().len(), 1);
            assert_eq!(
                out["peers"][0]["rider_node_matrices"]
                    .as_array()
                    .unwrap()
                    .len(),
                27
            );
            assert_eq!(out["peers"][0]["engine"], Value::Null);
            assert_eq!(out["peers"][0]["crash"], Value::Null);
            assert_eq!(out["peers"][0]["stale"], now >= 1100);
        }
    }
    #[test]
    fn malformed_packets_and_playback_history_reject() {
        setup(true);
        let mut bytes = atv_net::encode(&Packet::Snapshot((remote(1, 100)).into())).unwrap();
        bytes[4] = 255;
        assert!(dispatch(&bytes, 82, 100, &Simulation::default()).is_err());
        CLIENT.with_borrow(|c| assert!(c.buffers.is_empty()));
        let mut req: Value = serde_json::from_slice(&request(false)).unwrap();
        req["playing"] = json!([{"peer":1,"duration":-1,"playhead":0}]);
        assert!(
            dispatch(
                &serde_json::to_vec(&req).unwrap(),
                83,
                100,
                &Simulation::default()
            )
            .is_err()
        );
        assert!(dispatch(&vec![0; 65537], 85, 0, &Simulation::default()).is_err());
    }
    #[test]
    fn ping_roundtrip_tracks_monotonic_milliseconds_rollover() {
        setup(true);
        receive(
            Packet::Pong {
                nonce: u32::MAX - 20,
            },
            10,
        )
        .unwrap();
        CLIENT.with_borrow(|c| assert_eq!(c.rtt_ms, Some(31)));
    }
    #[test]
    fn node_adapter_keeps_source_basis_and_shear() {
        let source = Transform {
            position: [1., 2., 3.],
            rotation: [0.5f32.sqrt(), 0., 0.5f32.sqrt(), 0.],
            scale: [2., 3., 4.],
            shear: [0.; 3],
        }
        .to_column_major();
        let decoded = matrix_nodes(vec![source]).unwrap()[0].to_column_major();
        for (a, b) in source.iter().zip(decoded) {
            assert!((a - b).abs() < 0.00001);
        }
        let mut source = source;
        source[4] += 1.;
        let output = matrix_nodes(vec![source]).unwrap()[0].to_column_major();
        for (a, b) in source.iter().zip(output) {
            assert!((a - b).abs() < 0.00001);
        }
    }
}
