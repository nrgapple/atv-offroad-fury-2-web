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
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

/// Match QuadSounds' bounded decoded-vocal cache; a gender bank has about 50 clips.
pub const MAX_DECODED_VOCALS: usize = 24;

#[derive(Default)]
struct VocalCache {
    encoded: HashMap<String, Vec<u8>>,
    /// Least recently used first. Playing voices retain their own Arc on eviction.
    decoded: VecDeque<(String, Arc<Pcm>)>,
}

impl VocalCache {
    fn get(&mut self, key: &str, rate: u32) -> Result<Option<Arc<Pcm>>> {
        if let Some(index) = self.decoded.iter().position(|(name, _)| name == key) {
            let entry = self.decoded.remove(index).unwrap();
            let pcm = entry.1.clone();
            self.decoded.push_back(entry);
            return Ok(Some(pcm));
        }
        let Some(bytes) = self.encoded.get(key) else {
            return Ok(None);
        };
        let pcm = Arc::new(Pcm::from_vag(&decode_vag(bytes)?, rate));
        self.decoded.push_back((key.to_string(), pcm.clone()));
        if self.decoded.len() > MAX_DECODED_VOCALS {
            self.decoded.pop_front();
        }
        Ok(Some(pcm))
    }
}

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
    vocal_cache: VocalCache,
    rate: u32,
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

    /// Decode a selected rider vocal on first use, retaining only the last 24 clips.
    /// Missing optional vocals stay silent, just like an older browser package.
    pub fn vocal(&mut self, name: &str) -> Result<Option<Arc<Pcm>>> {
        let key = normal(name);
        // A clip also used by a non-vocal bank need not be decoded twice.
        if let Some(pcm) = self.samples.get(&key) {
            return Ok(Some(pcm.clone()));
        }
        self.vocal_cache.get(&key, self.rate)
    }

    pub fn decoded_vocals(&self) -> usize {
        self.vocal_cache.decoded.len()
    }
}

fn find(archive: &Archive, key: &str) -> Option<usize> {
    let with = format!("{key}.vag");
    archive.members.iter().position(|m| {
        let name = normal(&m.name);
        name == key || name == with
    })
}

/// Decodes the engine and world samples at `rate`, keeping rider vocals encoded
/// until selected. Missing optional samples (older packages) stay silent.
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
    let mut vocal_cache = VocalCache::default();
    for name in vocals.iter().flatten().flatten() {
        let key = normal(name);
        if samples.contains_key(&key) || vocal_cache.encoded.contains_key(&key) {
            continue;
        }
        if let Some(index) = find(&sounds, &key) {
            vocal_cache.encoded.insert(key, sounds.member(index).await?);
        }
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
        vocal_cache,
        rate,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut bytes = vec![0; 48 + 16];
        bytes[..4].copy_from_slice(b"VAGp");
        bytes[12..16].copy_from_slice(&16u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&48000u32.to_be_bytes());
        bytes[48] = 12;
        bytes[49] = 1;
        bytes[50..].fill(0x21);
        bytes
    }

    #[test]
    fn vocals_stay_encoded_until_selected_and_reuse_decoded_pcm() {
        let bytes = sample();
        let expected = Pcm::from_vag(&decode_vag(&bytes).unwrap(), 48000);
        let mut cache = VocalCache::default();
        cache.encoded.insert("voice".into(), bytes);
        assert!(cache.decoded.is_empty());
        assert!(cache.get("missing", 48000).unwrap().is_none());
        let first = cache.get("voice", 48000).unwrap().unwrap();
        assert_eq!(first.data, expected.data);
        let second = cache.get("voice", 48000).unwrap().unwrap();
        assert!(
            Arc::ptr_eq(&first, &second),
            "cache hit must not decode again"
        );
        assert_eq!(cache.decoded.len(), 1);
    }

    #[test]
    fn vocal_cache_evicts_least_recently_used_without_invalidating_playback() {
        let mut cache = VocalCache::default();
        for index in 0..=MAX_DECODED_VOCALS {
            cache.encoded.insert(index.to_string(), sample());
        }
        let mut voices = Vec::new();
        for index in 0..MAX_DECODED_VOCALS {
            voices.push(cache.get(&index.to_string(), 48000).unwrap().unwrap());
        }
        let refreshed = cache.get("0", 48000).unwrap().unwrap();
        assert!(Arc::ptr_eq(&refreshed, &voices[0]));
        cache.get(&MAX_DECODED_VOCALS.to_string(), 48000).unwrap();
        assert_eq!(cache.decoded.len(), MAX_DECODED_VOCALS);
        assert!(!cache.decoded.iter().any(|(name, _)| name == "1"));
        assert_eq!(
            Arc::strong_count(&voices[1]),
            1,
            "only the playing voice remains"
        );
        assert!(voices[1].data.iter().any(|sample| *sample != 0.));
        let reloaded = cache.get("1", 48000).unwrap().unwrap();
        assert_eq!(cache.decoded.len(), MAX_DECODED_VOCALS);
        assert!(!Arc::ptr_eq(&reloaded, &voices[1]));
        assert_eq!(reloaded.data, voices[1].data);
    }

    #[test]
    fn invalid_optional_vocal_does_not_enter_the_decoded_cache() {
        let mut cache = VocalCache::default();
        cache.encoded.insert("bad".into(), vec![0; 4]);
        assert!(cache.get("bad", 48000).is_err());
        assert!(cache.decoded.is_empty());
        cache.encoded.insert("good".into(), sample());
        assert!(cache.get("good", 48000).unwrap().is_some());
    }
}
