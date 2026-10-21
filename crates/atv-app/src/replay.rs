//! Native parity tooling: replay a scripted control sequence step by step and
//! write the vehicle/camera trace, for comparison with the browser game
//! (tools/compare_engine.mjs ride). Script: `[{"frames": N, "input": {...}}]`
//! with the browser's op 24 input fields.
use crate::riding::Riding;
use atv_game::ride::{Controls, Ride, bindings::TouchIntent};
use serde_json::{Value, json};

pub struct Script(Vec<(u32, Controls)>);

fn number(value: &Value, key: &str) -> f32 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.) as f32
}

impl Script {
    pub fn parse(text: &str) -> Result<Self, String> {
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

    pub fn steps(&self) -> impl Iterator<Item = &Controls> {
        self.0
            .iter()
            .flat_map(|(n, c)| std::iter::repeat_n(c, *n as usize))
    }

    pub fn run(&self, riding: &mut Riding) -> Result<(), String> {
        for controls in self.steps() {
            riding.run(controls, 1)?;
        }
        Ok(())
    }
}

fn controls(input: &Value) -> Controls {
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
    }
}

pub fn record(ride: &Ride, step: u64) -> Value {
    let frame = ride.frame();
    let (camera, mode) = ride.camera();
    json!({
        "step": step,
        "origin": frame.origin,
        "orientation": frame.orientation,
        "speed": frame.speed,
        "wheel_compression": frame.wheel_compression,
        "grounded": frame.grounded,
        "phase": format!("{:?}", frame.wreck.phase),
        "reset_count": frame.reset_count,
        "camera": {"position": camera.position, "target": camera.target, "fov": camera.fov,
            "mode": format!("{mode:?}")},
    })
}

/// Loads the venue and ride without a window, replays `script` and writes
/// one record per step (step 0 is the loaded state) to `out`.
#[cfg(not(target_arch = "wasm32"))]
pub fn trace(
    content: &std::path::Path,
    venue: &str,
    script: &Script,
    out: &std::path::Path,
) -> Result<(), String> {
    pollster::block_on(async {
        use atv_content::{Content, Store};
        let content = Content::open(Store::Directory(content.to_path_buf())).await?;
        let venue = atv_game::venue::load(&content, venue, false).await?;
        let mut ride = atv_game::ride::load(&content, &venue, 1, 0, |_| {}).await?;
        let mut records = vec![record(&ride, 0)];
        let started = std::time::Instant::now();
        for (index, controls) in script.steps().enumerate() {
            ride.step(controls, 0.)?;
            records.push(record(&ride, index as u64 + 1));
        }
        let elapsed = started.elapsed().as_secs_f64();
        eprintln!(
            "replayed {} steps in {:.1} ms ({:.3} ms/step)",
            records.len() - 1,
            elapsed * 1000.,
            elapsed * 1000. / (records.len() - 1).max(1) as f64
        );
        std::fs::write(
            out,
            serde_json::to_vec(&records).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    })
}
