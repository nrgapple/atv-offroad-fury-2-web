//! Recovered ATV rider vocals, chassis impacts and terrain drive-on loops
//! (USA v2.00 `SCUS_972.11`).
//!
//! Exact-build evidence (private Ghidra exports): the shared sound table
//! `00213210` loads `audio\Rider\rider.aud` (Female/Male Bang, SmallWreck,
//! MedWreck, HardWreck, Jostle and Fall lists) and `audio\vehicle.aud`
//! (QuadToQuadImpact, Wreck). The rider vocal emitter (`002cc430`,
//! `002cc640`, `002cc7d0`, request `002cc920`, update `002ccc60`, start
//! `002ccdc0`) is driven by wreck entry `001e8a20`, the hard-landing action
//! `002daa50` and the quad-to-quad contact callback `001eb6f0`. The quad's
//! collision emitter (`002cd4e0`, `002cd6c0`, update `002cdb80`, contact
//! `002cd990`, water `002cdad0`) plays `impact\landing.vag`, the Wreck list,
//! QuadToQuadImpact and `impact\h2ohit02.vag`, and drives two material sound
//! emitters (`001c3330`; play/update `001c3a90`, pitch/volume `001c3eb0`)
//! with the venue's `TerrainDriveOnSoundN` records (`00204720`).
//! The original libc `rand` stream is replaced by the port's own streams.
use crate::{Result, engine_audio::OriginalRandom, wheel::Vec3};
use serde::{Deserialize, Serialize};

/// `rand() * 4.656613e-10`, the source's normalization of a 31-bit word.
const UNIT: f32 = 4.656613e-10;

fn safe_resource(name: &str) -> Result<String> {
    let name = name.trim().replace('\\', "/").to_ascii_lowercase();
    if name.is_empty() || name.len() > 260 || name.contains("..") || name.starts_with('/') {
        return Err("invalid original sound resource".into());
    }
    Ok(if name.ends_with(".vag") {
        name
    } else {
        format!("{name}.vag")
    })
}
fn sound_list(data: &VenueAud, section: &str) -> Result<Vec<String>> {
    let count = data.int(section, "NumSounds", 0);
    if !(0..=64).contains(&count) {
        return Err("original sound list exceeds limit".into());
    }
    (1..=count)
        .map(|i| {
            safe_resource(
                data.get(section, &format!("Sound_{i}"))
                    .ok_or("original sound missing")?,
            )
        })
        .collect()
}

/// Rider vocal list order of `002cc7d0` (emitter +1ec..+250).
pub const VOCAL_KINDS: [&str; 6] = [
    "Bang",
    "SmallWreck",
    "MedWreck",
    "HardWreck",
    "Jostle",
    "Fall",
];
#[derive(Debug, Clone, Default, Serialize)]
pub struct VocalBanks {
    pub female: [Vec<String>; 6],
    pub male: [Vec<String>; 6],
}
impl VocalBanks {
    pub fn parse(rider_aud: &[u8]) -> Result<Self> {
        let data = VenueAud::parse(rider_aud)?;
        let bank = |prefix: &str| -> Result<[Vec<String>; 6]> {
            let lists = VOCAL_KINDS
                .iter()
                .map(|kind| sound_list(&data, &format!("{prefix}{kind}")))
                .collect::<Result<Vec<_>>>()?;
            Ok(lists.try_into().unwrap())
        };
        Ok(Self {
            female: bank("Female")?,
            male: bank("Male")?,
        })
    }
    /// `002cc7d0`: rider +268 == 0 selects the Female lists.
    pub fn lists(&self, female: bool) -> &[Vec<String>; 6] {
        if female { &self.female } else { &self.male }
    }
}

/// One emitter voice as the original observes it: the current clip is
/// playing, and the listener manager's in-range bit (`001c12e0`).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Voice {
    pub playing: bool,
    #[serde(default = "in_range_default")]
    pub in_range: bool,
}
fn in_range_default() -> bool {
    true
}
impl Default for Voice {
    fn default() -> Self {
        Self {
            playing: false,
            in_range: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VocalCue {
    pub sample: String,
    /// `002ccc60` stops a playing Fall yell (emitter vtable +8c, `001c17d0`)
    /// before the wreck vocal starts.
    pub stop_current: bool,
}
/// Rider vocal events passed to `002cc920` by its callers.
pub mod event {
    /// Quad-to-quad contact (`001eb6f0`).
    pub const BANG: u32 = 1;
    /// Hard-landing action (`002daa50`).
    pub const JOSTLE: u32 = 2;
    /// Wreck entry or a detached rider contact (`001e8a20`, `002ccc60`).
    pub const WRECK: u32 = 3;
    /// Wreck entry high above the ground or after a boundary launch.
    pub const FALL: u32 = 4;
}

/// `002cc430`/`002cc920`/`002ccc60` rider vocal emitter state.
#[derive(Debug, Clone)]
pub struct RiderVocals {
    lists: [Vec<String>; 6],
    last_bang: i32,
    last_jostle: i32,
    last_fall: i32,
    last_event: u32,
    random: OriginalRandom,
}
impl RiderVocals {
    pub fn new(lists: [Vec<String>; 6], seed: u64) -> Self {
        Self {
            lists,
            last_bang: -1,
            last_jostle: -1,
            last_fall: -1,
            last_event: 0,
            random: OriginalRandom::with_seed(seed),
        }
    }
    pub fn reset(&mut self) {
        self.last_event = 0;
    }
    /// The no-repeat pick used for Bang, Jostle and Fall.
    fn pick(&mut self, list: usize, last: i32) -> i32 {
        let n = self.lists[list].len() as i32;
        let mut index = (n as f32 * self.random.next_u31() as f32 * UNIT) as i32;
        if n > 1 && index == last {
            index = (index + 1) % n;
        }
        index
    }
    /// `002ccdc0`: start only within range and while no vocal plays.
    fn start(&self, list: usize, index: i32, voice: &mut Voice, stop: bool) -> Option<VocalCue> {
        let sample = self.lists[list].get(index as usize)?;
        if !voice.in_range || voice.playing {
            return None;
        }
        voice.playing = true;
        Some(VocalCue {
            sample: sample.clone(),
            stop_current: stop,
        })
    }
    /// `002cc920(magnitude, emitter, event)`.
    pub fn request(&mut self, event: u32, magnitude: f32, voice: &mut Voice) -> Option<VocalCue> {
        self.request_inner(event, magnitude, voice, false)
    }
    fn request_inner(
        &mut self,
        event: u32,
        magnitude: f32,
        voice: &mut Voice,
        stop: bool,
    ) -> Option<VocalCue> {
        let cue = match event {
            event::FALL => {
                let index = self.pick(5, self.last_fall);
                let cue = self.start(5, index, voice, stop);
                self.last_fall = index;
                cue
            }
            event::JOSTLE => {
                let index = self.pick(4, self.last_jostle);
                let cue = self.start(4, index, voice, stop);
                self.last_jostle = index;
                cue
            }
            event::BANG => {
                let index = self.pick(0, self.last_bang);
                let cue = self.start(0, index, voice, stop);
                self.last_bang = index;
                cue
            }
            event::WRECK => {
                // Below 20 SmallWreck, below 40 MedWreck, otherwise HardWreck.
                // Unlike the other lists, a multi-sample pick always steps on.
                let list = if magnitude < 20. {
                    1
                } else if magnitude < 40. {
                    2
                } else {
                    3
                };
                let n = self.lists[list].len() as i32;
                let mut index = (n as f32 * self.random.next_u31() as f32 * UNIT) as i32;
                if n > 1 {
                    index = (index + 1) % n;
                }
                self.start(list, index, voice, stop)
            }
            _ => None,
        };
        self.last_event = event;
        cue
    }
    /// `002ccc60` once per frame. `rider_impact` is the rider body's sampled
    /// contact delta magnitude, `wrecked` the quad's +7f0 wreck flag.
    pub fn update(
        &mut self,
        rider_impact: f32,
        wrecked: bool,
        voice: &mut Voice,
    ) -> Option<VocalCue> {
        if self.last_event != 0 && !voice.playing {
            self.last_event = 0;
        }
        if !wrecked || rider_impact <= 0. || !rider_impact.is_finite() {
            return None;
        }
        if self.last_event == event::FALL {
            voice.playing = false;
            self.request_inner(event::WRECK, rider_impact, voice, true)
        } else if rider_impact > 3. {
            self.request(event::WRECK, rider_impact, voice)
        } else {
            None
        }
    }
}

/// `001e8a20` wreck entry: Fall when the quad is at least 30ft above the
/// ground below it or a boundary launch forced the wreck; otherwise a wreck
/// vocal sized by the local impact, only when the sampled impact is nonzero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WreckEntry {
    pub fall: bool,
    pub impact: f32,
}
impl WreckEntry {
    pub fn classify(height_above_ground: f32, boundary_launch: bool, impact: f32) -> Self {
        Self {
            fall: height_above_ground >= 30. || boundary_launch,
            impact,
        }
    }
    pub fn event(self) -> Option<(u32, f32)> {
        if self.fall {
            Some((event::FALL, 0.))
        } else if self.impact > 0. {
            Some((event::WRECK, self.impact))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ChassisBanks {
    pub quad_to_quad: Vec<String>,
    pub wreck: Vec<String>,
}
impl ChassisBanks {
    pub fn parse(vehicle_aud: &[u8]) -> Result<Self> {
        let data = VenueAud::parse(vehicle_aud)?;
        Ok(Self {
            quad_to_quad: sound_list(&data, "QuadToQuadImpact")?,
            wreck: sound_list(&data, "Wreck")?,
        })
    }
}
pub const LANDING: &str = "impact/landing.vag";
pub const SPLASH: &str = "impact/h2ohit02.vag";
/// `002cd6c0` chassis voice: min 30ft, max/cull 70ft.
pub const CHASSIS_MIN: f32 = 30.;
pub const CHASSIS_MAX: f32 = 70.;
/// `002cc640` vocal voice: min 30ft, max/cull 100ft.
pub const VOCAL_MIN: f32 = 30.;
pub const VOCAL_MAX: f32 = 100.;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChassisCue {
    pub sample: String,
    /// The sound object's 0..100 volume (`001c1740`); Wreck keeps its default.
    pub volume: i32,
}
/// Per-frame inputs of `002cdb80`.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct ChassisInput {
    /// Magnitude of the quad's sampled world impact (+d0).
    pub impact: f32,
    /// Quad +7f0 wreck flag.
    pub wrecked: bool,
    /// Quad +9dc: more than 2ft above the ground below its origin.
    pub airborne: bool,
    /// Chassis collider +88 contact this frame.
    pub chassis_contact: bool,
}
/// `002cd4e0`/`002cdb80` quad collision emitter.
#[derive(Debug, Clone)]
pub struct Chassis {
    banks: ChassisBanks,
    landing_latch: bool,
    wreck_latch: bool,
    last_contact: i32,
    wreck_divisor: i32,
    water: bool,
    random: OriginalRandom,
}
impl Chassis {
    /// `water` is the scene's water object (+404), which loads h2ohit02.
    pub fn new(banks: ChassisBanks, water: bool, seed: u64) -> Self {
        let wreck_divisor = if banks.wreck.is_empty() {
            0
        } else {
            60 / banks.wreck.len() as i32
        };
        Self {
            banks,
            landing_latch: false,
            wreck_latch: false,
            last_contact: -1,
            wreck_divisor,
            water,
            random: OriginalRandom::with_seed(seed),
        }
    }
    pub fn reset(&mut self) {
        self.landing_latch = false;
        self.wreck_latch = false;
    }
    fn cue(sample: &str, volume: i32, voice: &mut Voice) -> Option<ChassisCue> {
        if !voice.in_range {
            return None;
        }
        voice.playing = true;
        Some(ChassisCue {
            sample: sample.into(),
            volume,
        })
    }
    /// `002cd990`: QuadToQuadImpact at volume 60 when the voice is free.
    pub fn quad_contact(&mut self, voice: &mut Voice) -> Option<ChassisCue> {
        if voice.playing || self.banks.quad_to_quad.is_empty() {
            return None;
        }
        let n = self.banks.quad_to_quad.len() as i32;
        let mut index = (n as f32 * self.random.next_u31() as f32 * UNIT) as i32;
        if n > 1 && index == self.last_contact {
            index = (index + 1) % n;
        }
        self.last_contact = index;
        let sample = self.banks.quad_to_quad[index as usize].clone();
        Self::cue(&sample, 60, voice)
    }
    /// `002cdad0`: water entry at volume 100 when the voice is free.
    pub fn splash(&mut self, voice: &mut Voice) -> Option<ChassisCue> {
        if voice.playing || !self.water {
            return None;
        }
        Self::cue(SPLASH, 100, voice)
    }
    /// `002cdb80` landing and wreck parts. Returns the cue and whether the
    /// recovered quad stops its material collision sounds (+30 vtable +cc).
    pub fn update(&mut self, input: ChassisInput, voice: &mut Voice) -> (Option<ChassisCue>, bool) {
        let impact = if input.impact.is_finite() {
            input.impact
        } else {
            0.
        };
        let mut cue = None;
        if impact >= 10. && !input.wrecked && self.landing_latch && !voice.playing {
            let volume = ((impact.min(60.) / 60.) * 100.) as i32;
            cue = Self::cue(LANDING, volume, voice);
            self.landing_latch = false;
        }
        if input.airborne {
            self.landing_latch = true;
        }
        let mut stop_collisions = false;
        if !input.wrecked {
            if self.wreck_latch {
                stop_collisions = true;
                self.wreck_latch = false;
            }
        } else if input.chassis_contact
            && impact >= 10.
            && !voice.playing
            && !self.banks.wreck.is_empty()
        {
            self.wreck_latch = true;
            let random = self.random.next_u31();
            let index = (impact as i32) / self.wreck_divisor.max(1) - 1
                + ((random as f32 * UNIT) * 3.) as i32;
            let index = index.clamp(0, self.banks.wreck.len() as i32 - 1) as usize;
            let sample = self.banks.wreck[index].clone();
            cue = Self::cue(&sample, 100, voice).or(cue);
        }
        (cue, stop_collisions)
    }
}

/// `001d7280(quad, 4)`: the body model entering water while moving downward
/// (normalized velocity against world up below -0.3) requests the splash.
/// The +66c latch keeps the previous submerged state.
#[derive(Debug, Clone, Copy, Default)]
pub struct WaterEntry {
    submerged: bool,
}
impl WaterEntry {
    pub fn update(&mut self, depth: Option<f32>, velocity: Vec3) -> bool {
        let submerged = depth.is_some_and(|d| d < 0.);
        let mut splash = false;
        if submerged && !self.submerged {
            let length =
                (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2])
                    .sqrt();
            if length > 0. && length.is_finite() {
                splash = -velocity[1] / length > 0.3;
            }
        }
        self.submerged = submerged;
        splash
    }
}
/// `001f0860`: a rider entering water also plays the quad's splash.
#[derive(Debug, Clone, Copy, Default)]
pub struct RiderWater {
    touching: bool,
}
impl RiderWater {
    pub fn update(&mut self, touching: bool) -> bool {
        let entered = touching && !self.touching;
        self.touching = touching;
        entered
    }
}

/// One `TerrainDriveOnSoundN`/`MaterialDriveOnSoundN` record (`00204720`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriveOnSound {
    pub sample: String,
    pub sample_speed: i32,
    pub min_volume: i32,
    pub min_speed: i32,
    pub max_speed: i32,
    pub resonant_speed: i32,
    pub slope: f32,
    pub min_distance: f32,
    pub max_distance: f32,
}
/// `001c3eb0`: pitch from the sample's native SPU pitch and the speed in mph,
/// volume rising from MinVolume at MinSpeed to 100 at the resonant speed,
/// then falling back to MinVolume at MaxSpeed. `step` caches record +20.
pub fn drive_on_pitch_volume(
    sound: &DriveOnSound,
    native_pitch: i32,
    mph: i32,
    step: &mut Option<i32>,
    random: u32,
) -> (i32, i32) {
    let step = *step.get_or_insert_with(|| {
        let ratio = if sound.sample_speed == 0 {
            0
        } else {
            native_pitch / sound.sample_speed
        };
        (sound.slope * ratio as f32 + 0.5) as i32
    });
    let jitter = ((step + 1) as f32 * random as f32 * UNIT) as i32;
    let pitch = (native_pitch as f32
        + (step.wrapping_mul(mph - sound.sample_speed) - jitter) as f32) as i32;
    let volume = if mph <= sound.resonant_speed {
        let span = sound.resonant_speed - sound.min_speed;
        let slope = if span == 0 {
            0
        } else {
            (100 - sound.min_volume) / span
        };
        sound.min_volume + slope * (mph - sound.min_speed)
    } else {
        let span = sound.max_speed - sound.resonant_speed;
        let slope = if span == 0 {
            0
        } else {
            -(100 - sound.min_volume) / span
        };
        sound.min_volume + slope * (mph - sound.max_speed)
    };
    let volume = volume.min(100);
    (pitch, if volume < 1 { 0 } else { volume })
}
/// The SPU pitch of a sample rate (4096 plays at 48kHz).
pub fn native_pitch(sample_rate: u32) -> i32 {
    (sample_rate as u64 * 4096 / 48000) as i32
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum DriveCommand {
    Start {
        sample: String,
        record: usize,
        pitch: i32,
        volume: i32,
    },
    Update {
        record: usize,
        pitch: i32,
        volume: i32,
    },
    Stop,
}
/// One `001c3330` material sound emitter's terrain drive-on state.
#[derive(Debug, Clone, Default)]
pub struct DriveEmitter {
    current: Option<usize>,
    steps: Vec<Option<i32>>,
}
impl DriveEmitter {
    /// `001c4260`: stop every drive-on sound; the current id becomes -1.
    pub fn stop(&mut self) -> Option<DriveCommand> {
        self.current.take().map(|_| DriveCommand::Stop)
    }
    /// `001c3a90` for terrain record `id` (1-based). `playing` is whether the
    /// current record's sound still plays; a stopped one-shot is only updated.
    #[allow(clippy::too_many_arguments)]
    pub fn play(
        &mut self,
        sounds: &[DriveOnSound],
        native: &dyn Fn(&str) -> Option<i32>,
        id: usize,
        mph: i32,
        scale: i32,
        playing: bool,
        random: &mut OriginalRandom,
    ) -> Vec<DriveCommand> {
        let mut out = Vec::new();
        if id == 0 || id > sounds.len() {
            return out;
        }
        if self.steps.len() < sounds.len() {
            self.steps.resize(sounds.len(), None);
        }
        let changed = self.current != Some(id);
        if !changed || !playing {
            if let Some(current) = self.current {
                if let Some(base) = native(&sounds[current - 1].sample) {
                    let (pitch, volume) = drive_on_pitch_volume(
                        &sounds[current - 1],
                        base,
                        mph,
                        &mut self.steps[current - 1],
                        random.next_u31(),
                    );
                    out.push(DriveCommand::Update {
                        record: current,
                        pitch,
                        volume: volume * scale / 100,
                    });
                }
                return out;
            }
        } else {
            out.extend(self.stop());
        }
        let Some(base) = native(&sounds[id - 1].sample) else {
            return out;
        };
        self.current = Some(id);
        let (pitch, volume) = drive_on_pitch_volume(
            &sounds[id - 1],
            base,
            mph,
            &mut self.steps[id - 1],
            random.next_u31(),
        );
        out.push(DriveCommand::Start {
            sample: sounds[id - 1].sample.clone(),
            record: id,
            pitch,
            volume: volume * scale / 100,
        });
        out
    }
    pub fn current(&self) -> Option<usize> {
        self.current
    }
}
/// Per-side inputs from `001e5260`: terrain material byte of the side's
/// front wheel (-1 off the ground, 8 when the quad is half a foot below
/// water), and the speed in mph (body +34 × 0.68182).
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct DriveInput {
    pub terrain: [i32; 2],
    pub mph: i32,
}
/// `002cdb80` drive-on branch for the two side emitters (terrain records;
/// scene-object material records need object sound ids and stay silent).
pub fn drive_on(
    emitters: &mut [DriveEmitter; 2],
    sounds: &[DriveOnSound],
    native: &dyn Fn(&str) -> Option<i32>,
    input: DriveInput,
    playing: [bool; 2],
    random: &mut OriginalRandom,
) -> [Vec<DriveCommand>; 2] {
    let mut out: [Vec<DriveCommand>; 2] = Default::default();
    let id = |terrain: i32| terrain + 1;
    if input.terrain[0] != input.terrain[1] {
        for side in 0..2 {
            let i = id(input.terrain[side]);
            if i < 2 {
                out[side].extend(emitters[side].stop());
            } else {
                out[side] = emitters[side].play(
                    sounds,
                    native,
                    i as usize,
                    input.mph,
                    50,
                    playing[side],
                    random,
                );
            }
        }
    } else {
        let i = id(input.terrain[0]);
        if i < 2 {
            out[0].extend(emitters[0].stop());
        } else {
            out[0] = emitters[0].play(
                sounds, native, i as usize, input.mph, 100, playing[0], random,
            );
        }
        out[1].extend(emitters[1].stop());
    }
    out
}

/// Venue `<venue>.aud` parsing shared by drive-on, ambient and Soultree.
pub struct VenueAud {
    entries: Vec<(String, String, String)>,
}
impl VenueAud {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 512 * 1024 {
            return Err("venue sound configuration exceeds limit".into());
        }
        let text = String::from_utf8_lossy(bytes);
        let mut entries = Vec::new();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_ascii_lowercase();
            } else if let Some((key, value)) = line.split_once('=') {
                entries.push((
                    section.clone(),
                    key.trim().to_ascii_lowercase(),
                    value.trim().to_string(),
                ));
            }
        }
        Ok(Self { entries })
    }
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let (section, key) = (section.to_ascii_lowercase(), key.to_ascii_lowercase());
        self.entries
            .iter()
            .find(|(s, k, _)| *s == section && *k == key)
            .map(|(_, _, v)| v.as_str())
    }
    /// The integer reader's leading-integer (atoi) semantics.
    pub fn int(&self, section: &str, key: &str, default: i32) -> i32 {
        self.get(section, key).map_or(default, atoi)
    }
    pub fn float(&self, section: &str, key: &str, default: f32) -> f32 {
        self.get(section, key)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }
    /// T/F, TRUE/FALSE, ON/OFF, Y/N or a number, as `00141fc0` accepts.
    pub fn flag(&self, section: &str, key: &str) -> bool {
        self.get(section, key).is_some_and(flag)
    }
    pub fn terrain_drive_on(&self) -> Result<Vec<DriveOnSound>> {
        let count = self.int("Sounds", "NumTerrainDriveOnSounds", 0);
        if !(0..=32).contains(&count) {
            return Err("terrain drive-on count exceeds limit".into());
        }
        (1..=count)
            .map(|i| {
                let s = format!("TerrainDriveOnSound{i}");
                Ok(DriveOnSound {
                    sample: safe_resource(
                        self.get(&s, "SoundResourceFile")
                            .ok_or("drive-on sound missing")?,
                    )?,
                    sample_speed: self.int(&s, "MaterialSampleSpeed", 0),
                    min_volume: self.int(&s, "MaterialMinVolume", 0),
                    min_speed: self.int(&s, "MaterialMinSpeed", 0),
                    max_speed: self.int(&s, "MaterialMaxSpeed", 0),
                    resonant_speed: self.int(&s, "MaterialResonantFreqSpeed", 0),
                    slope: self.float(&s, "MaterialFreqSlope", 0.),
                    min_distance: self.float(&s, "MinDistance", 0.),
                    max_distance: self.float(&s, "MaxDistance", 0.),
                })
            })
            .collect()
    }
}
pub fn atoi(value: &str) -> i32 {
    let value = value.trim();
    let (sign, digits) = match value.as_bytes().first() {
        Some(b'-') => (-1, &value[1..]),
        Some(b'+') => (1, &value[1..]),
        _ => (1, value),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    sign * digits[..end].parse::<i32>().unwrap_or(0)
}
pub fn flag(value: &str) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "t" | "true" | "on" | "y" | "yes" => true,
        "f" | "false" | "off" | "n" | "no" => false,
        other => other.parse::<f32>().is_ok_and(|v| v != 0.),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vocals() -> RiderVocals {
        let list =
            |n: usize, tag: &str| (1..=n).map(|i| format!("{tag}{i}.vag")).collect::<Vec<_>>();
        RiderVocals::new(
            [
                list(3, "bang"),
                list(2, "small"),
                list(2, "med"),
                list(2, "hard"),
                list(3, "jostle"),
                list(2, "fall"),
            ],
            7,
        )
    }
    #[test]
    fn parses_original_rider_bank_shape() {
        let text = b"[FemaleBang]\nNumSounds = 2\nSound_1 = Rider\\fgrunt\\fgrunt01.vag\nSound_2 = Rider\\fgrunt\\fgrunt02.vag\n[MaleFall]\nNumSounds = 1\nSound_1 = Rider\\myell\\myell01.vag\n[BodyImpact]\nNumSounds=1\nSound_1 = Rider\\body04.vag\n";
        let banks = VocalBanks::parse(text).unwrap();
        assert_eq!(
            banks.female[0],
            ["rider/fgrunt/fgrunt01.vag", "rider/fgrunt/fgrunt02.vag"]
        );
        assert_eq!(banks.male[5], ["rider/myell/myell01.vag"]);
        assert!(banks.male[0].is_empty());
        assert!(VocalBanks::parse(b"[MaleBang]\nNumSounds=1\nSound_1=../x").is_err());
    }
    #[test]
    fn wreck_size_lists_and_one_voice() {
        let mut v = vocals();
        let mut voice = Voice::default();
        let cue = v.request(event::WRECK, 19.9, &mut voice).unwrap();
        assert!(cue.sample.starts_with("small"));
        assert!(voice.playing);
        // A playing vocal blocks the next one; the request still records it.
        assert!(v.request(event::WRECK, 45., &mut voice).is_none());
        let mut free = Voice::default();
        assert!(
            v.request(event::WRECK, 39.9, &mut free)
                .unwrap()
                .sample
                .starts_with("med")
        );
        let mut free = Voice::default();
        assert!(
            v.request(event::WRECK, 40., &mut free)
                .unwrap()
                .sample
                .starts_with("hard")
        );
        let mut far = Voice {
            playing: false,
            in_range: false,
        };
        assert!(v.request(event::BANG, 0., &mut far).is_none());
    }
    #[test]
    fn no_repeat_picks_and_unknown_events() {
        let mut v = vocals();
        let mut previous = None;
        for _ in 0..40 {
            let cue = v.request(event::JOSTLE, 0., &mut Voice::default()).unwrap();
            assert!(cue.sample.starts_with("jostle"));
            assert_ne!(Some(cue.sample.clone()), previous);
            previous = Some(cue.sample);
        }
        assert!(v.request(9, 0., &mut Voice::default()).is_none());
        assert_eq!(v.last_event, 9);
    }
    #[test]
    fn fall_is_cut_by_the_first_rider_contact() {
        let mut v = vocals();
        let mut voice = Voice::default();
        assert!(
            v.request(event::FALL, 0., &mut voice)
                .unwrap()
                .sample
                .starts_with("fall")
        );
        // Riding frames do nothing; a detached contact replaces the yell.
        assert!(v.update(5., false, &mut voice).is_none());
        let cue = v.update(0.5, true, &mut voice).unwrap();
        assert!(cue.stop_current && cue.sample.starts_with("small"));
        // Ordinary contacts need more than 3 and a free voice.
        let mut free = Voice::default();
        v.last_event = event::WRECK;
        assert!(v.update(3., true, &mut free).is_none());
        assert!(v.update(3.5, true, &mut free).is_some());
        // A finished clip clears the remembered event.
        let mut idle = Voice::default();
        v.update(0., false, &mut idle);
        assert_eq!(v.last_event, 0);
    }
    #[test]
    fn wreck_entry_classification() {
        assert_eq!(
            WreckEntry::classify(30., false, 0.).event(),
            Some((event::FALL, 0.))
        );
        assert_eq!(
            WreckEntry::classify(2., true, 9.).event(),
            Some((event::FALL, 0.))
        );
        assert_eq!(
            WreckEntry::classify(29.9, false, 12.).event(),
            Some((event::WRECK, 12.))
        );
        assert_eq!(WreckEntry::classify(1., false, 0.).event(), None);
    }
    fn chassis() -> Chassis {
        Chassis::new(
            ChassisBanks {
                quad_to_quad: (6..=10)
                    .map(|i| format!("impact/impact{i:02}.vag"))
                    .collect(),
                wreck: (4..=8).map(|i| format!("impact/wreck{i:02}.vag")).collect(),
            },
            true,
            3,
        )
    }
    #[test]
    fn landing_needs_airborne_latch_and_impact() {
        let mut c = chassis();
        let mut voice = Voice::default();
        let input = |impact, airborne| ChassisInput {
            impact,
            wrecked: false,
            airborne,
            chassis_contact: false,
        };
        assert!(c.update(input(30., false), &mut voice).0.is_none());
        assert!(c.update(input(0., true), &mut voice).0.is_none());
        assert!(c.update(input(9.9, false), &mut voice).0.is_none());
        let cue = c.update(input(30., false), &mut voice).0.unwrap();
        assert_eq!((cue.sample.as_str(), cue.volume), (LANDING, 50));
        assert!(
            c.update(input(80., false), &mut Voice::default())
                .0
                .is_none()
        );
        c.update(input(0., true), &mut Voice::default());
        assert_eq!(
            c.update(input(80., false), &mut Voice::default())
                .0
                .unwrap()
                .volume,
            100
        );
    }
    #[test]
    fn wreck_chassis_selection_and_recovery_stop() {
        let mut c = chassis();
        let wrecked = |impact, contact| ChassisInput {
            impact,
            wrecked: true,
            airborne: false,
            chassis_contact: contact,
        };
        assert!(
            c.update(wrecked(50., false), &mut Voice::default())
                .0
                .is_none()
        );
        let cue = c
            .update(wrecked(10., true), &mut Voice::default())
            .0
            .unwrap();
        assert!(cue.sample.starts_with("impact/wreck0"));
        // 10/12 - 1 = -1 plus at most 2 clamps into the first three.
        assert!(["impact/wreck04.vag", "impact/wreck05.vag"].contains(&cue.sample.as_str()));
        let high = c
            .update(wrecked(500., true), &mut Voice::default())
            .0
            .unwrap();
        assert_eq!(high.sample, "impact/wreck08.vag");
        let (_, stop) = c.update(ChassisInput::default(), &mut Voice::default());
        assert!(stop);
        assert!(!c.update(ChassisInput::default(), &mut Voice::default()).1);
    }
    #[test]
    fn quad_contact_and_splash_share_the_voice() {
        let mut c = chassis();
        let mut voice = Voice::default();
        assert_eq!(c.quad_contact(&mut voice).unwrap().volume, 60);
        assert!(c.splash(&mut voice).is_none());
        assert_eq!(c.splash(&mut Voice::default()).unwrap().sample, SPLASH);
        let mut dry = Chassis::new(ChassisBanks::default(), false, 1);
        assert!(dry.splash(&mut Voice::default()).is_none());
        assert!(dry.quad_contact(&mut Voice::default()).is_none());
    }
    #[test]
    fn water_entry_requires_downward_motion() {
        let mut w = WaterEntry::default();
        assert!(!w.update(Some(1.), [0., -10., 0.]));
        assert!(w.update(Some(-0.1), [3., -10., 0.]));
        assert!(!w.update(Some(-0.2), [0., -10., 0.]));
        assert!(!w.update(None, [0., -10., 0.]));
        assert!(!w.update(Some(-0.1), [10., -2., 0.]));
        let mut r = RiderWater::default();
        assert!(r.update(true) && !r.update(true) && !r.update(false) && r.update(true));
    }
    fn snow() -> DriveOnSound {
        DriveOnSound {
            sample: "impact/driveon/drv_snw2.vag".into(),
            sample_speed: 40,
            min_volume: 0,
            min_speed: 1,
            max_speed: 60,
            resonant_speed: 90,
            slope: 0.35,
            min_distance: 15.,
            max_distance: 30.,
        }
    }
    #[test]
    fn drive_on_pitch_and_volume_follow_integer_curve() {
        let base = native_pitch(16000);
        assert_eq!(base, 1365);
        let mut step = None;
        let (pitch, volume) = drive_on_pitch_volume(&snow(), base, 40, &mut step, 0);
        // 0.35 * (1365 / 40 = 34) + 0.5 = 12.4 -> 12; no jitter at random 0.
        assert_eq!((step, pitch, volume), (Some(12), 1365, 39));
        // Jitter trunc(13 * 0.5) = 6 below the speed-scaled pitch.
        let (pitch, _) = drive_on_pitch_volume(&snow(), base, 50, &mut step, 0x4000_0000);
        assert_eq!(pitch, 1365 + 120 - 6);
        let mut s = snow();
        s.resonant_speed = 50;
        let mut step = None;
        assert_eq!(drive_on_pitch_volume(&s, base, 50, &mut step, 0).1, 98);
        assert_eq!(drive_on_pitch_volume(&s, base, 51, &mut step, 0).1, 90);
        assert_eq!(drive_on_pitch_volume(&s, base, 70, &mut step, 0).1, 0);
        assert_eq!(drive_on_pitch_volume(&s, base, 0, &mut step, 0).1, 0);
    }
    #[test]
    fn drive_on_sides_scale_switch_and_stop() {
        let sounds: Vec<DriveOnSound> = (0..9)
            .map(|i| DriveOnSound {
                sample: format!("s{i}.vag"),
                ..snow()
            })
            .collect();
        let native = |_: &str| Some(1365);
        let mut emitters: [DriveEmitter; 2] = Default::default();
        let mut random = OriginalRandom::with_seed(1);
        // Same surface on both sides: one emitter at full volume.
        let out = drive_on(
            &mut emitters,
            &sounds,
            &native,
            DriveInput {
                terrain: [2, 2],
                mph: 40,
            },
            [false; 2],
            &mut random,
        );
        assert!(matches!(
            &out[0][..],
            [DriveCommand::Start {
                record: 3,
                volume: 39,
                ..
            }]
        ));
        assert!(out[1].is_empty());
        // Continuing updates in place.
        let out = drive_on(
            &mut emitters,
            &sounds,
            &native,
            DriveInput {
                terrain: [2, 2],
                mph: 41,
            },
            [true, false],
            &mut random,
        );
        assert!(matches!(
            &out[0][..],
            [DriveCommand::Update { record: 3, .. }]
        ));
        // Different sides play both at half volume, switching the left record.
        let out = drive_on(
            &mut emitters,
            &sounds,
            &native,
            DriveInput {
                terrain: [8, 2],
                mph: 40,
            },
            [true, false],
            &mut random,
        );
        assert!(matches!(
            &out[0][..],
            [DriveCommand::Stop, DriveCommand::Start { record: 9, .. }]
        ));
        assert!(matches!(
            &out[1][..],
            [DriveCommand::Start {
                record: 3,
                volume: 19,
                ..
            }]
        ));
        // Surface 0 and an airborne wheel are silent.
        let out = drive_on(
            &mut emitters,
            &sounds,
            &native,
            DriveInput {
                terrain: [-1, -1],
                mph: 40,
            },
            [true, true],
            &mut random,
        );
        assert_eq!(out, [vec![DriveCommand::Stop], vec![DriveCommand::Stop]]);
        // A finished non-looping clip is only updated until it is stopped.
        drive_on(
            &mut emitters,
            &sounds,
            &native,
            DriveInput {
                terrain: [3, 3],
                mph: 40,
            },
            [false; 2],
            &mut random,
        );
        let out = drive_on(
            &mut emitters,
            &sounds,
            &native,
            DriveInput {
                terrain: [4, 4],
                mph: 40,
            },
            [false; 2],
            &mut random,
        );
        assert!(matches!(
            &out[0][..],
            [DriveCommand::Update { record: 4, .. }]
        ));
    }
    #[test]
    fn venue_records_and_original_readers() {
        let aud = VenueAud::parse(b"[Sounds]\nENV01\nNumTerrainDriveOnSounds=1\n[TerrainDriveOnSound1]\nSoundResourceFile=impact\\driveon\\drv_snw1.vag\nMaterialSampleSpeed=50\nMaterialMinVolume=0\nMaterialMinSpeed=1\nMaterialMaxSpeed=60\nMaterialResonantFreqSpeed=50\nMaterialFreqSlope=0.75\nMaxDistance=70\nMinDistance=30\n").unwrap();
        let records = aud.terrain_drive_on().unwrap();
        assert_eq!(records[0].sample, "impact/driveon/drv_snw1.vag");
        assert_eq!(
            (
                records[0].resonant_speed,
                records[0].slope,
                records[0].max_distance
            ),
            (50, 0.75, 70.)
        );
        assert_eq!((atoi("12.5"), atoi("-3x"), atoi("x")), (12, -3, 0));
        assert!(flag("T") && flag("1") && flag("on") && !flag("F") && !flag("0"));
    }
}
