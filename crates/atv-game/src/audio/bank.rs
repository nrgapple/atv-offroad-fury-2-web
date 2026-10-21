//! Original sound samples for a ride, decoded from the AUDIO.RES bank and the
//! per-vehicle/rider/venue tables (GameAudio.loadBank, QuadSounds.configure
//! and loadVenue in web/audio.js and web/quad-sounds.js).
use super::mixer::Pcm;
use crate::{Result, archive::Archive};
use atv_content::Content;
use atv_core::{
    audio::decode_vag,
    engine_audio::EngineAudioConfig,
    quad_sounds::{self as qs, VenueAud},
    sound_effects, world_sounds,
};
use std::{collections::HashMap, sync::Arc};

/// The bank's member key for an authored sample name.
pub fn normal(name: &str) -> String {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    lower
        .strip_prefix("audio/")
        .or_else(|| lower.strip_prefix("aud/"))
        .unwrap_or(&lower)
        .to_string()
}

pub struct Bank {
    pub samples: HashMap<String, Arc<Pcm>>,
    /// Original sample rates (drive-on pitch is relative to them).
    pub rates: HashMap<String, u32>,
    pub engine: EngineAudioConfig,
    pub effects: Option<sound_effects::Config>,
    pub vocals: Option<[Vec<String>; 6]>,
    pub chassis: Option<qs::ChassisBanks>,
    pub drive: Vec<qs::DriveOnSound>,
    pub ambient: Vec<world_sounds::Ambient>,
    pub soultree: Vec<world_sounds::Soultree>,
    /// The venue's world sounds use crowd samples (Enhanced stadium echo).
    pub crowd_venue: bool,
    pub decode_ms: f64,
}

impl Bank {
    pub fn get(&self, name: &str) -> Option<&Arc<Pcm>> {
        self.samples.get(&normal(name))
    }
}

fn find(archive: &Archive, key: &str) -> Option<usize> {
    let with = format!("{key}.vag");
    archive.members.iter().position(|m| {
        let name = normal(&m.name);
        name == key || name == with
    })
}

/// Reads and decodes every sample the ride can play at `rate`. Missing
/// optional samples (older packages) leave those voices silent.
pub async fn load(
    content: &Content,
    course_archive: &str,
    environment: &str,
    quad: u32,
    female: bool,
    rate: u32,
) -> Result<Bank> {
    let table = Archive::open(content, "/AUDIOTBL.RES").await?;
    let vehicle_index = table
        .members
        .iter()
        .position(|m| m.name.to_ascii_lowercase().ends_with("vehicle.aud"))
        .ok_or("original vehicle audio table is missing")?;
    let vehicle = table.member(vehicle_index).await?;
    let engine = EngineAudioConfig::parse(&vehicle, quad)?;
    let rider = match table
        .members
        .iter()
        .position(|m| m.name.to_ascii_lowercase().ends_with("rider.aud"))
    {
        Some(index) => Some(table.member(index).await?),
        None => None,
    };
    let effects = rider
        .as_deref()
        .and_then(|bytes| sound_effects::Config::parse(bytes).ok());
    let vocals = rider
        .as_deref()
        .and_then(|bytes| qs::VocalBanks::parse(bytes).ok())
        .map(|banks| banks.lists(female).clone());
    let chassis = qs::ChassisBanks::parse(&vehicle).ok();

    let course = Archive::open(content, course_archive).await?;
    let (mut drive, mut ambient, mut soultree) = (Vec::new(), Vec::new(), Vec::new());
    if let Some(index) = course.named(&format!("{environment}.aud")) {
        let aud = VenueAud::parse(&course.member(index).await?)?;
        drive = aud.terrain_drive_on()?;
        (ambient, soultree) = world_sounds::parse(&aud)?;
    }

    let mut names: Vec<String> = engine
        .samples
        .iter()
        .flatten()
        .chain(&engine.impact_samples)
        .cloned()
        .collect();
    names.extend(effects.iter().flat_map(|e| e.body_impact.clone()));
    names.extend([qs::LANDING.to_string(), qs::SPLASH.to_string()]);
    names.extend(
        chassis
            .iter()
            .flat_map(|c| c.quad_to_quad.iter().chain(&c.wreck).cloned()),
    );
    names.extend(vocals.iter().flatten().flatten().cloned());
    names.extend(drive.iter().map(|d| d.sample.clone()));
    names.extend(ambient.iter().map(|a| a.sample.clone()));
    names.extend(soultree.iter().map(|s| s.sample.clone()));

    let sounds = Archive::open(content, "/AUDIO/AUDIO.RES").await?;
    let mut samples = HashMap::new();
    let mut rates = HashMap::new();
    let mut decode_ms = 0.;
    for name in names {
        let key = normal(&name);
        if samples.contains_key(&key) {
            continue;
        }
        let Some(index) = find(&sounds, &key) else {
            continue;
        };
        let vag = sounds.member(index).await?;
        let started = web_time::Instant::now();
        let audio = decode_vag(&vag)?;
        rates.insert(key.clone(), audio.info.sample_rate);
        samples.insert(key, Arc::new(Pcm::from_vag(&audio, rate)));
        decode_ms += started.elapsed().as_secs_f64() * 1000.;
    }
    let crowd_venue = ambient
        .iter()
        .map(|a| &a.sample)
        .chain(soultree.iter().map(|s| &s.sample))
        .any(|s| s.starts_with("crowd/"));
    // Sources whose sample an older package lacks are dropped.
    ambient.retain(|a| samples.contains_key(&normal(&a.sample)));
    soultree.retain(|s| samples.contains_key(&normal(&s.sample)));
    Ok(Bank {
        samples,
        rates,
        engine,
        effects,
        vocals,
        chassis,
        drive,
        ambient,
        soultree,
        crowd_venue,
        decode_ms,
    })
}
