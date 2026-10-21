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

    pub fn controls(&self) -> Vec<Controls> {
        self.steps().copied().collect()
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

/// Loads the ride and its sounds without a window or device, replays
/// `script` at one step per 60 Hz frame and writes the mixed output as a
/// 48 kHz stereo WAV, with a JSON sidecar of 50 ms RMS levels and voice
/// starts (`out` with `.json`).
#[cfg(not(target_arch = "wasm32"))]
pub fn render_audio(
    content: &std::path::Path,
    venue: &str,
    script: &Script,
    out: &std::path::Path,
    mode: atv_game::audio::ride::SoundMode,
) -> Result<(), String> {
    use atv_game::audio::{bank, mixer::Mixer, ride::RideAudio};
    const RATE: u32 = 48000;
    pollster::block_on(async {
        use atv_content::{Content, Store};
        let content = Content::open(Store::Directory(content.to_path_buf())).await?;
        let environment = venue;
        let venue = atv_game::venue::load(&content, environment, false).await?;
        let mut ride = atv_game::ride::load(&content, &venue, 1, 0, |_| {}).await?;
        let course = atv_game::launch::course_resource(environment, false)?;
        let started = std::time::Instant::now();
        let bank = bank::load(&content, &course.archive, environment, 1, false, RATE).await?;
        let load_ms = started.elapsed().as_secs_f64() * 1000.;
        let mut audio = RideAudio::new(bank, mode);
        let mut mixer = Mixer::new(RATE);
        mixer.log = Some(Vec::new());
        audio.apply_mode(&mut mixer);
        let mut pcm = Vec::new();
        let mut levels = Vec::new();
        let mut window = [0f64; 2];
        let mut window_frames = 0;
        let (mut logic, mut mix) = (0f64, 0f64);
        let frame_samples = (RATE / 60) as usize;
        let mut block = vec![0f32; frame_samples * 2];
        let mut live = Vec::new();
        for controls in script.steps() {
            ride.step(controls, 0.)?;
            let t = std::time::Instant::now();
            if let Some(step) = ride.sound() {
                audio.step(&mut mixer, step)?;
            }
            audio.frame(&mut mixer, ride.sound())?;
            logic += t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            live.push(
                mixer
                    .snapshot()
                    .into_iter()
                    .map(|(label, frames, rate, gains)| {
                        json!([label, frames as f64 / RATE as f64, rate, gains])
                    })
                    .collect::<Vec<_>>(),
            );
            mixer.render(&mut block);
            mix += t.elapsed().as_secs_f64();
            for frame in block.chunks_exact(2) {
                for c in 0..2 {
                    window[c] += f64::from(frame[c] * frame[c]);
                    pcm.push((frame[c].clamp(-1., 1.) * 32767.) as i16);
                }
                window_frames += 1;
                if window_frames == (RATE / 20) as usize {
                    levels.push(window.map(|w| (w / window_frames as f64).sqrt()));
                    window = [0.; 2];
                    window_frames = 0;
                }
            }
        }
        let steps = script.steps().count().max(1) as f64;
        eprintln!(
            "audio: bank {:.0} ms ({} samples, {:.0} ms decode), {:.3} ms logic + {:.3} ms mix per frame",
            load_ms,
            audio.bank.samples.len(),
            audio.bank.decode_ms,
            logic * 1000. / steps,
            mix * 1000. / steps
        );
        let mut wav = Vec::with_capacity(44 + pcm.len() * 2);
        let bytes = (pcm.len() * 2) as u32;
        wav.extend(b"RIFF");
        wav.extend((36 + bytes).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(RATE.to_le_bytes());
        wav.extend((RATE * 4).to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(bytes.to_le_bytes());
        for sample in pcm {
            wav.extend(sample.to_le_bytes());
        }
        std::fs::write(out, wav).map_err(|e| e.to_string())?;
        let log: Vec<Value> = mixer
            .log
            .take()
            .unwrap_or_default()
            .into_iter()
            .map(|(at, label, frames, rate)| {
                json!({"time": at as f64 / RATE as f64, "label": label, "seconds": frames as f64 / RATE as f64, "rate": rate})
            })
            .collect();
        let sidecar = json!({
            "rate": RATE, "window": 0.05, "levels": levels, "voices": log, "live": live,
            "stats": format!("{:?}", audio.stats),
            "bank_ms": load_ms, "logic_ms_per_frame": logic * 1000. / steps, "mix_ms_per_frame": mix * 1000. / steps,
        });
        std::fs::write(out.with_extension("json"), sidecar.to_string()).map_err(|e| e.to_string())
    })
}
