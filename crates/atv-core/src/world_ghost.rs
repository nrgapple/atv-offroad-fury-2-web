//! World board ghosts (research/WORLD_BOARDS.md "Ghosts"; a port addition,
//! issues #543 and #542): a lap's motion, compact enough to travel with a
//! world lap board row and be raced in Lap Attack.
//!
//! A race that keeps records also keeps its best lap's motion here
//! ([`Recorder`]): one key (position and orientation) every [`STRIDE`]
//! simulation steps, 10 a second. The Lap Attack ghost's own recording
//! (`race_ghost`) keeps every drawn node matrix and is some hundred times
//! larger, so a world ghost carries the motion alone and is drawn in the
//! pose the downloading rider's own vehicle rests in.
//!
//! The bytes (version 1, little endian; `cloudflare/ghosts.mjs` checks the
//! same layout): a 32-byte header — `FRGH`, version, stride, vehicle `u16`,
//! lap milliseconds `u32`, lap steps `u32`, key count `u16`, the course id's
//! length and its bytes (zero padded to 10), three zero bytes — then 16
//! bytes a key: the position as three `i32` (1/32 ft) and the orientation
//! quaternion's three smaller components in 10 bits each with the index of
//! the largest. They travel as base64 text in JSON.
//!
//! A downloaded ghost is untrusted: [`decode`] and [`Rival::from_download`]
//! check every length and value and return an [`Error`], never panic.
use crate::race_ghost::Sample;
use serde::{Deserialize, Serialize};

pub const MAGIC: [u8; 4] = *b"FRGH";
pub const VERSION: u8 = 1;
pub const HEADER: usize = 32;
pub const KEY: usize = 16;
/// Simulation steps between keys (the step is 1/60 s).
pub const STRIDE: usize = 6;
/// A ghost's largest size: 4094 keys, a lap of 6 min 49 s. A longer lap
/// carries no ghost.
pub const MAX_BYTES: usize = 64 * 1024;
pub const MAX_KEYS: usize = (MAX_BYTES - HEADER) / KEY;
/// Rows of a lap board that keep a ghost (`GHOST_TOP` in ghosts.mjs).
pub const TOP: u32 = 10;
/// Position units a foot.
const UNITS: f32 = 32.;
/// The boards' world bound, feet.
const MAX_FEET: f32 = 1.0e6;
/// A key step longer than this (feet) is a rider reset, not ridden: the
/// ghost jumps instead of gliding across (twice the boards' fastest step).
const JUMP_FEET: f32 = 40.;
const COURSE_BYTES: usize = 10;
/// Steps of a packed orientation component: even, so zero is exact.
const QUANTA: f32 = 1022.;

/// Why a ghost could not be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not the JSON or base64 the server sends.
    Encoding,
    /// Not a ghost, or its sizes disagree.
    Format,
    /// A newer format than this build reads.
    Version,
    /// A position outside any course.
    Bounds,
    /// Another course's or another lap's ghost.
    Mismatch,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Version => "This ghost needs a newer version of the game.",
            Self::Mismatch => "This ghost belongs to another lap.",
            Self::Encoding | Self::Format | Self::Bounds => "This ghost could not be read.",
        })
    }
}

/// One lap's motion: a key every [`STRIDE`] steps from the lap's first.
#[derive(Clone, Debug, PartialEq)]
pub struct Lap {
    pub course: String,
    pub vehicle: u16,
    pub lap_ms: u32,
    /// Simulation steps the lap lasted.
    pub steps: u32,
    pub keys: Vec<Sample>,
}

/// Keys of a lap of `steps` steps.
pub fn keys_for(steps: u32) -> usize {
    if steps == 0 {
        0
    } else {
        (steps as usize - 1) / STRIDE + 1
    }
}

fn course_ok(course: &str) -> bool {
    (3..=COURSE_BYTES).contains(&course.len())
        && course
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// The orientation in 32 bits: the largest component's index, then the
/// other three (sign-normalized, each within ±1/√2) in 10 bits each.
fn pack(q: [f32; 4]) -> u32 {
    let length = q.iter().map(|c| c * c).sum::<f32>().sqrt();
    let q = if length.is_finite() && length > 1e-6 {
        q.map(|c| c / length)
    } else {
        [0., 0., 0., 1.]
    };
    let mut largest = 0;
    for i in 1..4 {
        if q[i].abs() > q[largest].abs() {
            largest = i;
        }
    }
    let sign = if q[largest] < 0. { -1. } else { 1. };
    let mut bits = largest as u32;
    for (i, c) in q.iter().enumerate() {
        if i != largest {
            let unit = c * sign * std::f32::consts::FRAC_1_SQRT_2 + 0.5;
            bits = bits << 10 | (unit * QUANTA).round().clamp(0., QUANTA) as u32;
        }
    }
    bits
}

fn unpack(bits: u32) -> [f32; 4] {
    let largest = (bits >> 30) as usize;
    let mut q = [0.; 4];
    let mut shift = 20;
    let mut rest = 0.;
    for (i, c) in q.iter_mut().enumerate() {
        if i != largest {
            *c = (((bits >> shift) & 1023) as f32 / QUANTA - 0.5) * std::f32::consts::SQRT_2;
            rest += *c * *c;
            shift -= 10.min(shift);
        }
    }
    q[largest] = (1. - rest).max(0.).sqrt();
    let length = q.iter().map(|c| c * c).sum::<f32>().sqrt();
    if length > 1e-6 {
        q.map(|c| c / length)
    } else {
        [0., 0., 0., 1.]
    }
}

/// The lap's bytes; none when it does not fit the format (too long, no
/// motion, a course id the boards do not know).
pub fn encode(lap: &Lap) -> Option<Vec<u8>> {
    let keys = lap.keys.len();
    if keys < 2 || keys > MAX_KEYS || keys != keys_for(lap.steps) || !course_ok(&lap.course) {
        return None;
    }
    let mut out = Vec::with_capacity(HEADER + keys * KEY);
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(STRIDE as u8);
    out.extend_from_slice(&lap.vehicle.to_le_bytes());
    out.extend_from_slice(&lap.lap_ms.to_le_bytes());
    out.extend_from_slice(&lap.steps.to_le_bytes());
    out.extend_from_slice(&(keys as u16).to_le_bytes());
    out.push(lap.course.len() as u8);
    out.extend_from_slice(lap.course.as_bytes());
    out.resize(HEADER, 0);
    for key in &lap.keys {
        for c in key.origin {
            let units = (c * UNITS).round();
            if !units.is_finite() || units.abs() > MAX_FEET * UNITS {
                return None;
            }
            out.extend_from_slice(&(units as i32).to_le_bytes());
        }
        out.extend_from_slice(&pack(key.orientation).to_le_bytes());
    }
    Some(out)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Reads a ghost's bytes, checking every size and value.
pub fn decode(bytes: &[u8]) -> Result<Lap, Error> {
    if bytes.len() < HEADER || bytes.len() > MAX_BYTES || bytes[..4] != MAGIC {
        return Err(Error::Format);
    }
    if bytes[4] != VERSION {
        return Err(Error::Version);
    }
    let header = || -> Option<(u16, u32, u32, usize, usize)> {
        Some((
            u16_at(bytes, 6)?,
            u32_at(bytes, 8)?,
            u32_at(bytes, 12)?,
            usize::from(u16_at(bytes, 16)?),
            usize::from(*bytes.get(18)?),
        ))
    };
    let (vehicle, lap_ms, steps, keys, name) = header().ok_or(Error::Format)?;
    let course = bytes
        .get(19..19 + name)
        .filter(|_| name <= COURSE_BYTES)
        .and_then(|b| std::str::from_utf8(b).ok())
        .filter(|c| course_ok(c))
        .ok_or(Error::Format)?;
    if usize::from(bytes[5]) != STRIDE
        || bytes[19 + name..HEADER].iter().any(|b| *b != 0)
        || keys < 2
        || keys > MAX_KEYS
        || keys != keys_for(steps)
        || bytes.len() != HEADER + keys * KEY
        || !(1..=3_600_000).contains(&lap_ms)
    {
        return Err(Error::Format);
    }
    let mut out = Vec::with_capacity(keys);
    for key in bytes[HEADER..].chunks_exact(KEY) {
        let mut origin = [0.; 3];
        for (i, c) in origin.iter_mut().enumerate() {
            let units = u32_at(key, i * 4).ok_or(Error::Format)? as i32;
            *c = units as f32 / UNITS;
            if c.abs() > MAX_FEET {
                return Err(Error::Bounds);
            }
        }
        out.push(Sample {
            origin,
            orientation: unpack(u32_at(key, 12).ok_or(Error::Format)?),
        });
    }
    Ok(Lap {
        course: course.to_string(),
        vehicle,
        lap_ms,
        steps,
        keys: out,
    })
}

impl Lap {
    /// The lap's motion at every simulation step, blended between keys (a
    /// reset's jump is not blended): what the Lap Attack ghost replays.
    pub fn samples(&self) -> Vec<Sample> {
        let Some(last) = self.keys.len().checked_sub(1) else {
            return Vec::new();
        };
        let steps = (self.steps as usize).min(self.keys.len() * STRIDE);
        let mut out = Vec::with_capacity(steps);
        for step in 0..steps {
            let key = (step / STRIDE).min(last);
            let (a, b) = (self.keys[key], self.keys[(key + 1).min(last)]);
            let t = (step % STRIDE) as f32 / STRIDE as f32;
            let apart = (0..3)
                .map(|i| (b.origin[i] - a.origin[i]).powi(2))
                .sum::<f32>()
                .sqrt();
            if t == 0. || apart > JUMP_FEET {
                out.push(a);
                continue;
            }
            // The shorter way round between the two orientations.
            let dot: f32 = (0..4).map(|i| a.orientation[i] * b.orientation[i]).sum();
            let side = if dot < 0. { -1. } else { 1. };
            let mut orientation: [f32; 4] = std::array::from_fn(|i| {
                a.orientation[i] + (b.orientation[i] * side - a.orientation[i]) * t
            });
            let length = orientation.iter().map(|c| c * c).sum::<f32>().sqrt();
            if length > 1e-6 {
                orientation = orientation.map(|c| c / length);
            } else {
                orientation = a.orientation;
            }
            out.push(Sample {
                origin: std::array::from_fn(|i| a.origin[i] + (b.origin[i] - a.origin[i]) * t),
                orientation,
            });
        }
        out
    }
}

/// Records each lap of a race and keeps the best judged lap's keys, in
/// fixed storage (no allocation while racing).
#[derive(Clone, Debug)]
pub struct Recorder {
    current: Vec<Sample>,
    steps: u32,
    /// The best judged lap's time and steps; its keys are `kept` (empty
    /// when the lap was too long for a ghost).
    best: Option<(f32, u32)>,
    kept: Vec<Sample>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            current: Vec::with_capacity(MAX_KEYS),
            steps: 0,
            best: None,
            kept: Vec::with_capacity(MAX_KEYS),
        }
    }
}

impl Recorder {
    /// One simulation step of the lap being ridden.
    pub fn record(&mut self, sample: Sample) {
        if (self.steps as usize).is_multiple_of(STRIDE) && self.current.len() < MAX_KEYS {
            self.current.push(sample);
        }
        self.steps = self.steps.saturating_add(1);
    }

    /// A lap completed in `time` seconds; `judged` when the race counts it
    /// for the best lap. The fastest judged lap's keys are kept. Returns
    /// whether this lap is the new best.
    pub fn lap(&mut self, time: f32, judged: bool) -> bool {
        let better =
            judged && self.steps > 0 && time > 0. && self.best.is_none_or(|(best, _)| time < best);
        if better {
            self.best = Some((time, self.steps));
            if self.current.len() == keys_for(self.steps) && self.current.len() >= 2 {
                std::mem::swap(&mut self.current, &mut self.kept);
            } else {
                // Too long for a ghost: the slower lap's must not stand in.
                self.kept.clear();
            }
        }
        self.current.clear();
        self.steps = 0;
        better
    }

    /// The best lap's time, seconds.
    pub fn best(&self) -> Option<f32> {
        self.best.map(|(time, _)| time)
    }

    /// The best lap as a ghost of `course` on `vehicle`, when it was kept.
    pub fn ghost(&self, course: &str, vehicle: u16) -> Option<Lap> {
        let (time, steps) = self.best?;
        (!self.kept.is_empty()).then(|| Lap {
            course: course.to_string(),
            vehicle,
            lap_ms: crate::world_board::millis(time),
            steps,
            keys: self.kept.clone(),
        })
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn to_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Canonical standard base64 (padded) to bytes.
pub fn from_base64(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let groups = text.len() / 4;
    for (g, group) in text.chunks_exact(4).enumerate() {
        let padding = group.iter().rev().take_while(|b| **b == b'=').count();
        if padding > 2 || (padding > 0 && g + 1 != groups) {
            return None;
        }
        let mut n = 0u32;
        for b in &group[..4 - padding] {
            let value = match b {
                b'A'..=b'Z' => b - b'A',
                b'a'..=b'z' => b - b'a' + 26,
                b'0'..=b'9' => b - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            };
            n = n << 6 | u32::from(value);
        }
        n <<= 6 * padding;
        out.extend_from_slice(&n.to_be_bytes()[1..4 - padding]);
    }
    Some(out)
}

/// What a ghost upload sends (`POST /api/ghosts/<board>`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Upload<'a> {
    pub name: &'a str,
    /// The ghost's bytes, base64.
    pub ghost: &'a str,
}

/// `POST /api/ghosts/<board>`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Kept {
    pub kept: bool,
}

/// `GET /api/ghosts/<board>/<name>/<ms>`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
struct Download {
    name: String,
    value: i64,
    ghost: String,
}

/// The path of a row's ghost: the name's spaces are percent-encoded (its
/// other characters, `A-Z 0-9 . _ ! -`, stand as they are).
pub fn download_path(board: &str, name: &str, value: i64) -> String {
    format!("/api/ghosts/{board}/{}/{value}", name.replace(' ', "%20"))
}

/// A board row's ghost to race in Lap Attack.
#[derive(Clone, Debug, PartialEq)]
pub struct Rival {
    /// The rider's board name.
    pub name: String,
    /// The lap, seconds.
    pub time: f32,
    pub lap: Lap,
}

impl Rival {
    /// The ghost in a download's body, which must be the lap of `ms` on
    /// `course` that was asked for.
    pub fn from_download(body: &str, course: &str, ms: i64) -> Result<Self, Error> {
        let download: Download = serde_json::from_str(body).map_err(|_| Error::Encoding)?;
        if download.ghost.len() > MAX_BYTES.div_ceil(3) * 4 {
            return Err(Error::Format);
        }
        let bytes = from_base64(&download.ghost).ok_or(Error::Encoding)?;
        let lap = decode(&bytes)?;
        if lap.course != course || i64::from(lap.lap_ms) != ms || download.value != ms {
            return Err(Error::Mismatch);
        }
        let name: String = download
            .name
            .chars()
            .filter(|c| c.is_ascii_graphic() || *c == ' ')
            .take(8)
            .collect();
        Ok(Self {
            name,
            time: lap.lap_ms as f32 / 1000.,
            lap,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lap round a circle of `radius` feet in `steps` steps.
    fn ride(recorder: &mut Recorder, steps: u32, radius: f32) {
        for step in 0..steps {
            let angle = std::f32::consts::TAU * step as f32 / steps as f32;
            let (sin, cos) = (angle / 2.).sin_cos();
            recorder.record(Sample {
                origin: [radius * angle.cos(), 12.5, radius * angle.sin()],
                orientation: [0., sin, 0., cos],
            });
        }
    }

    fn lap(steps: u32) -> Lap {
        let mut recorder = Recorder::default();
        ride(&mut recorder, steps, 600.);
        assert!(recorder.lap(steps as f32 / 60., true));
        recorder.ghost("nat01", 28).unwrap()
    }

    #[test]
    fn a_lap_round_trips_within_the_format_s_precision() {
        let lap = lap(3661);
        assert_eq!(lap.keys.len(), 611);
        assert_eq!(lap.lap_ms, 61_017);
        let bytes = encode(&lap).unwrap();
        assert_eq!(bytes.len(), HEADER + 611 * KEY);
        let back = decode(&bytes).unwrap();
        assert_eq!(
            (back.course.as_str(), back.vehicle, back.lap_ms, back.steps),
            ("nat01", 28, 61_017, 3661)
        );
        for (a, b) in lap.keys.iter().zip(&back.keys) {
            for i in 0..3 {
                assert!((a.origin[i] - b.origin[i]).abs() <= 1. / 64. + 1e-3);
            }
            // The same rotation (q and -q are one orientation).
            let dot: f32 = (0..4).map(|i| a.orientation[i] * b.orientation[i]).sum();
            assert!(dot.abs() > 0.999_99, "{a:?} {b:?}");
        }
        // Base64 and the download body.
        let text = to_base64(&bytes);
        assert_eq!(from_base64(&text).as_deref(), Some(&bytes[..]));
        let body = serde_json::json!({"board": "lap:nat01:n", "name": "ADA", "value": 61_017, "vehicle": 28, "ghost": text})
            .to_string();
        let rival = Rival::from_download(&body, "nat01", 61_017).unwrap();
        assert_eq!(rival.name, "ADA");
        assert!((rival.time - 61.017).abs() < 1e-4);
        assert_eq!(rival.lap, back);
        assert_eq!(
            Rival::from_download(&body, "nat02", 61_017),
            Err(Error::Mismatch)
        );
        assert_eq!(
            Rival::from_download(&body, "nat01", 61_000),
            Err(Error::Mismatch)
        );
    }

    #[test]
    fn a_typical_lap_is_a_few_kilobytes_and_the_cap_is_64() {
        // A 61 s lap: 611 keys.
        assert_eq!(encode(&lap(3661)).unwrap().len(), 9808);
        // The longest lap that keeps a ghost fills the cap exactly.
        let longest = (MAX_KEYS * STRIDE) as u32;
        assert_eq!(encode(&lap(longest)).unwrap().len(), MAX_BYTES);
        // One step longer: the lap is still the best, without a ghost.
        let mut long = Recorder::default();
        ride(&mut long, longest + 1, 600.);
        assert!(long.lap(410., true));
        assert_eq!(long.best(), Some(410.));
        assert!(long.ghost("env01endl", 3).is_none());
        // A long best lap after a short one: the short lap's ghost must not
        // travel with the long lap's time.
        let mut both = Recorder::default();
        ride(&mut both, longest + 1, 600.);
        assert!(both.lap(500., true));
        ride(&mut both, 3600, 600.);
        assert!(both.lap(60., true));
        assert_eq!(both.ghost("env01endl", 3).map(|g| g.lap_ms), Some(60_000));
    }

    #[test]
    fn the_recorder_keeps_the_fastest_judged_lap_without_growing() {
        let mut recorder = Recorder::default();
        let capacity = (recorder.current.capacity(), recorder.kept.capacity());
        ride(&mut recorder, 1200, 300.);
        // Supercross's first lap from the grid is not judged.
        assert!(!recorder.lap(20., false));
        assert!(recorder.ghost("sx01", 3).is_none());
        ride(&mut recorder, 1500, 300.);
        assert!(recorder.lap(25., true));
        ride(&mut recorder, 1800, 300.);
        assert!(!recorder.lap(30., true));
        ride(&mut recorder, 1380, 300.);
        assert!(recorder.lap(23., true));
        let ghost = recorder.ghost("sx01", 3).unwrap();
        assert_eq!(
            (ghost.lap_ms, ghost.steps, ghost.keys.len()),
            (23_000, 1380, 230)
        );
        assert_eq!(
            (recorder.current.capacity(), recorder.kept.capacity()),
            capacity
        );
    }

    #[test]
    fn playback_blends_between_keys_and_jumps_a_reset() {
        let mut lap = lap(600);
        let samples = lap.samples();
        assert_eq!(samples.len(), 600);
        assert_eq!(samples[0], lap.keys[0]);
        assert_eq!(samples[6], lap.keys[1]);
        // Halfway between two keys.
        let mid = samples[3];
        for i in 0..3 {
            let expected = (lap.keys[0].origin[i] + lap.keys[1].origin[i]) / 2.;
            assert!((mid.origin[i] - expected).abs() < 1e-3);
        }
        let length: f32 = mid.orientation.iter().map(|c| c * c).sum();
        assert!((length - 1.).abs() < 1e-4);
        // A reset between keys 10 and 11: no glide across the gap.
        for key in &mut lap.keys[11..] {
            key.origin[0] += 500.;
        }
        let samples = lap.samples();
        assert_eq!(samples[63].origin, lap.keys[10].origin);
        assert_eq!(samples[66].origin, lap.keys[11].origin);
    }

    #[test]
    fn malformed_ghosts_are_errors_never_panics() {
        let good = encode(&lap(600)).unwrap();
        assert!(decode(&good).is_ok());
        assert_eq!(decode(&[]), Err(Error::Format));
        assert_eq!(decode(&good[..HEADER]), Err(Error::Format));
        assert_eq!(decode(&good[..good.len() - 1]), Err(Error::Format));
        let mut newer = good.clone();
        newer[4] = 2;
        assert_eq!(decode(&newer), Err(Error::Version));
        let edit = |at: usize, value: u8| {
            let mut bytes = good.clone();
            bytes[at] = value;
            decode(&bytes)
        };
        assert_eq!(edit(0, b'X'), Err(Error::Format));
        assert_eq!(edit(5, 1), Err(Error::Format), "stride");
        assert_eq!(edit(16, 99), Err(Error::Format), "key count");
        assert_eq!(edit(18, 200), Err(Error::Format), "course length");
        assert_eq!(edit(19, b'/'), Err(Error::Format), "course id");
        assert_eq!(edit(30, 1), Err(Error::Format), "padding");
        // A position far outside any course.
        assert_eq!(edit(HEADER + 3, 0x7f), Err(Error::Bounds));
        // Every truncation and every single-byte change decodes or errors.
        for cut in 0..good.len().min(200) {
            let _ = decode(&good[..cut]);
        }
        for at in 0..good.len().min(400) {
            for value in [0u8, 1, 0x7f, 0x80, 0xff] {
                if let Ok(lap) = edit(at, value) {
                    assert_eq!(lap.samples().len(), lap.steps as usize);
                }
            }
        }
        // Download bodies.
        for body in [
            "",
            "{}",
            "[]",
            r#"{"ghost": 5}"#,
            r#"{"ghost": "!!!!", "value": 1}"#,
            r#"{"ghost": "AAA", "value": 1}"#,
            r#"{"ghost": "AA=A", "value": 1}"#,
            r#"{"ghost": "AAAA", "value": 1}"#,
        ] {
            assert!(Rival::from_download(body, "nat01", 1).is_err(), "{body}");
        }
        assert_eq!(from_base64("TWE="), Some(b"Ma".to_vec()));
        assert_eq!(from_base64("TQ=="), Some(b"M".to_vec()));
        assert_eq!(to_base64(b"Man"), "TWFu");
        assert_eq!(to_base64(b"M"), "TQ==");
        assert_eq!(from_base64("T=Q="), None);
    }

    #[test]
    fn orientations_pack_into_32_bits() {
        for q in [
            [0., 0., 0., 1.],
            [1., 0., 0., 0.],
            [0.5, -0.5, 0.5, -0.5],
            [0.1, 0.7, -0.2, 0.6],
            [-0.3, 0.2, 0.9, 0.1],
        ] {
            let length = q.iter().map(|c: &f32| c * c).sum::<f32>().sqrt();
            let unit = q.map(|c| c / length);
            let back = unpack(pack(q));
            let dot: f32 = (0..4).map(|i| unit[i] * back[i]).sum();
            assert!(dot.abs() > 0.999_99, "{q:?} {back:?}");
        }
        // No orientation at all reads as none.
        assert_eq!(unpack(pack([f32::NAN; 4])), [0., 0., 0., 1.]);
        assert_eq!(unpack(pack([0.; 4])), [0., 0., 0., 1.]);
    }

    #[test]
    fn a_row_s_ghost_path_encodes_the_name() {
        assert_eq!(
            download_path("lap:nat01:n", "A B.C!", 61_017),
            "/api/ghosts/lap:nat01:n/A%20B.C!/61017"
        );
    }
}
