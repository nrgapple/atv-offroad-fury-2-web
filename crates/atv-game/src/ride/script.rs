//! Scripted replays for parity tooling: a control script
//! (`[{"frames": N, "input": {...}}]` with the browser's op 24 input
//! fields), one record per step, and headless traces of rides and races.
//! Shared by `fury --trace`, the browser build's `?traceAll=1` and the
//! cross-platform determinism example.
use super::{Controls, Ride, bindings::TouchIntent};
use crate::Result;
use atv_content::Content;
use serde_json::{Value, json};

pub struct Script(Vec<(u32, Controls)>);

fn number(value: &Value, key: &str) -> f32 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.) as f32
}

impl Script {
    pub fn parse(text: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let segments = value.as_array().ok_or("script must be an array")?;
        let mut out = Vec::new();
        for segment in segments {
            let frames = segment
                .get("frames")
                .and_then(Value::as_u64)
                .ok_or("segment needs frames")? as u32;
            let input = segment.get("input").cloned().unwrap_or(json!({}));
            out.push((frames, controls(&input)));
        }
        Ok(Self(out))
    }

    pub fn controls(&self) -> Vec<Controls> {
        self.steps().copied().collect()
    }

    pub fn steps(&self) -> impl Iterator<Item = &Controls> {
        self.0
            .iter()
            .flat_map(|(n, c)| std::iter::repeat_n(c, *n as usize))
    }
}

pub fn controls(input: &Value) -> Controls {
    let optional = |key: &str| input.get(key).and_then(Value::as_f64).map(|v| v as f32);
    Controls {
        throttle: number(input, "throttle"),
        brake: number(input, "brake"),
        steer: number(input, "steer"),
        lean: number(input, "lean"),
        reverse: input
            .get("reverse")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        reverse_chord_brake: optional("reverse_chord_brake"),
        arcade_spin: optional("arcade_spin"),
        // game.js flip_request / flip_launch / flip_hold.
        flip_request: input
            .get("flip_request")
            .and_then(Value::as_str)
            .and_then(|s| ["backflip", "left", "right"].into_iter().find(|f| *f == s))
            .map(|flip| {
                (
                    flip,
                    input
                        .get("flip_launch")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    optional("flip_hold").unwrap_or(0.),
                )
            }),
        preload_axis: optional("preload_axis"),
        keyboard_drive: input.get("keyboard_drive").map(|k| TouchIntent {
            generation: k.get("generation").and_then(Value::as_u64).unwrap_or(0),
            held: k.get("held").and_then(Value::as_bool).unwrap_or(false),
            axis: number(k, "axis"),
            grounded_only: false,
            forward_only: false,
            reverse_held: false,
        }),
        // The JavaScript game's op 50 `touch_drive` (web/touch.js sample).
        touch_drive: input.get("touch_drive").map(|k| {
            let flag = |name: &str| k.get(name).and_then(Value::as_bool).unwrap_or(false);
            TouchIntent {
                generation: k.get("generation").and_then(Value::as_u64).unwrap_or(0),
                held: flag("held"),
                axis: number(k, "axis"),
                grounded_only: flag("grounded_only"),
                forward_only: flag("forward_only"),
                reverse_held: flag("reverse_held"),
            }
        }),
        stunt_modifiers: input
            .get("stunt_modifiers")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8,
        stunt_x: number(input, "stunt_x"),
        stunt_y: number(input, "stunt_y"),
        generation: input
            .get("input_generation")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        clutch: input
            .get("clutch")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

pub fn record(ride: &Ride, step: u64) -> Value {
    let frame = ride.frame();
    // A motorcycle records its own two wheels (front, rear), as atv-web does.
    fn wheels<T: Into<Value> + Copy>(bike: bool, v: [T; 4]) -> Value {
        if bike {
            json!([v[0].into(), v[2].into()])
        } else {
            Value::Array(v.map(Into::into).to_vec())
        }
    }
    let bike = ride.is_motorcycle();
    let (camera, mode) = ride.camera();
    json!({
        "step": step,
        "origin": frame.origin,
        "orientation": frame.orientation,
        "speed": frame.speed,
        "wheel_compression": wheels(bike, frame.wheel_compression),
        "grounded": wheels(bike, frame.grounded),
        "phase": format!("{:?}", frame.wreck.phase),
        "reset_count": frame.reset_count,
        "camera": {"position": camera.position, "target": camera.target, "fov": camera.fov,
            "mode": format!("{mode:?}")},
        "imposed": ride.race().and_then(|r| r.imposed).map(|i| json!([i.throttle, i.brake, i.steer, i.lean])),
        "race": ride.race().and_then(|r| r.frame.as_ref()).map(|f| json!({
            "phase": f.phase, "elapsed": f.elapsed, "lap": f.lap, "position": f.position,
            "countdown": f.countdown, "contacts": f.contacts, "off_track": f.off_track,
            "grid_slots": f.grid_slots, "finish_at": f.finish_at, "track_flags": f.track_flags,
            "enduro_gate": f.enduro_gate, "target": f.target, "player_stunts": f.player_stunts,
            "opponents": f.opponents.iter().map(|o| json!({"origin": o.origin, "distance": o.distance, "finished": o.finished})).collect::<Vec<_>>(),
            "standings": f.standings.iter().map(|s| json!({"name": s.name, "time": s.time, "position": s.position,
                "laps": s.laps, "progress": s.progress, "stunt": s.stunt, "player": s.player})).collect::<Vec<_>>(),
        })),
    })
}

/// A traced replay: one record per step (step 0 is the loaded state) and,
/// with `autopilot`, the race path driver's imposed controls as a script.
pub struct Trace {
    pub records: Vec<Value>,
    pub autopilot: Vec<Value>,
    /// Race presentation per step (not fingerprinted): lamps, crowd cues,
    /// gate, firework commands, lamp sounds, music and the stadium flashes
    /// lit after the step (atv-web's op 50 `race` and `flashes`).
    pub events: Vec<Value>,
}

/// Loads the venue and ride headlessly and replays `script`. With
/// `autopilot` the race path driver also rides the player before the finish.
pub async fn trace(
    content: &Content,
    venue: &str,
    selection: super::Selection,
    race: Option<super::race::Settings>,
    script: &Script,
    autopilot: bool,
) -> Result<Trace> {
    // A race course id names the venue and its race (`env05ends`).
    let (venue, variant) = match race {
        Some(_) => crate::launch::split_race_course(venue)
            .unwrap_or((venue, crate::launch::RaceVariant::National)),
        None => (venue, crate::launch::RaceVariant::National),
    };
    let options = crate::venue::LoadOptions {
        // Race traces hold weather off (the browser draws it per load).
        weather: crate::venue::WeatherChoice::Active(false),
        race: variant,
        ..Default::default()
    };
    let venue = crate::venue::load_with(content, venue, race.is_some(), options).await?;
    let mut ride = super::load_with(
        content,
        &venue,
        selection.quad,
        selection.rider,
        race,
        |_| {},
    )
    .await?;
    if autopilot && let Some(state) = ride.race_mut() {
        state.autopilot_always = true;
    }
    let mut records = vec![record(&ride, 0)];
    let mut recorded: Vec<Value> = Vec::new();
    let mut events = Vec::new();
    // The stadium flash clock as the renderer runs it (op 115's seed).
    let mut flashes = venue
        .scenery
        .flashes
        .map(|(config, pool)| atv_core::race_crowd::Flashes::new(config, pool, 97211 + 8 * 47));
    for (index, controls) in script.steps().enumerate() {
        ride.step(controls, 0.)?;
        records.push(record(&ride, index as u64 + 1));
        if ride.race().is_some() {
            let step = ride.take_race_events();
            let lit = flashes.as_mut().map(|f| {
                let config = f.config();
                for burst in &step.flashes {
                    f.burst(match burst {
                        super::race::FlashBurst::PlayerWin => config.player_win,
                        super::race::FlashBurst::AiFinish => config.ai_finish,
                        super::race::FlashBurst::PlayerStunt => config.player_stunt,
                    });
                }
                f.step(1. / 60.).to_vec()
            });
            let frame = ride.race().and_then(|r| r.frame.as_ref());
            events.push(json!({
                "step": index + 1,
                "lights": frame.and_then(|f| f.lights),
                "crowd": step.crowd,
                "gate": step.gate,
                "fireworks": step.fireworks,
                "sounds": step.sounds,
                "music": step.music.map(|m| format!("{m:?}")),
                "bursts": step.flashes.iter().map(|b| format!("{b:?}")).collect::<Vec<_>>(),
                "flashes": lit,
            }));
        }
        // The imposed controls as a pad-style script step (f32 values
        // print exactly and read back exactly).
        let input = ride.race().and_then(|r| r.imposed).unwrap_or_default();
        let step = json!({"throttle": input.throttle, "brake": input.brake, "steer": input.steer, "lean": input.lean});
        match recorded.last_mut() {
            Some(last) if last["input"] == step => {
                last["frames"] = json!(last["frames"].as_u64().unwrap_or(0) + 1)
            }
            _ => recorded.push(json!({"frames": 1, "input": step})),
        }
    }
    Ok(Trace {
        records,
        autopilot: recorded,
        events,
    })
}

/// FNV-1a over every binary32 the trace records for the player and CPUs
/// (positions, orientation, speed, suspension, camera), for comparing
/// platforms without shipping traces.
pub fn fingerprint(records: &[Value]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut mix = |v: &Value| {
        if let Some(x) = v.as_f64() {
            hash ^= u64::from((x as f32).to_bits());
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    fn walk(v: &Value, mix: &mut dyn FnMut(&Value)) {
        match v {
            Value::Array(items) => items.iter().for_each(|i| walk(i, mix)),
            Value::Object(map) => map.values().for_each(|i| walk(i, mix)),
            other => mix(other),
        }
    }
    for record in records {
        walk(record, &mut mix);
    }
    hash
}
