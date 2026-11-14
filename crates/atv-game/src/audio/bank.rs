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
    /// Both genders' vocal lists (race CPUs ride with either).
    pub vocal_banks: Option<qs::VocalBanks>,
    /// Each quad's `vehicle.aud` `EngineSets` (set, stroke), quad 1 first;
    /// empty for a motorcycle bank.
    pub engine_sets: Vec<(u32, u32)>,
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

    /// The output rate samples are decoded at.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn decoded_vocals(&self) -> usize {
        self.vocal_cache.decoded.len()
    }

    /// Decodes further AUDIO.RES samples (race lamps, IntOut stingers, the
    /// stadium crowd); missing ones stay silent like an older package.
    pub async fn load_samples(&mut self, content: &Content, names: &[String]) -> Result<()> {
        let missing: Vec<String> = names
            .iter()
            .map(|name| normal(name))
            .filter(|key| !self.samples.contains_key(key))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        let sounds = Archive::open(content, "/AUDIO/AUDIO.RES").await?;
        let (keys, vags) = read_samples(&sounds, missing).await?;
        // Decoded together off the page's thread on the web (and kept in the
        // decoded cache, which a menu preload fills): race samples decode
        // while the ride runs, so its frames are kept.
        let started = web_time::Instant::now();
        let inputs: Vec<&[u8]> = vags.iter().map(Vec::as_slice).collect();
        for (key, (source, pcm)) in keys
            .into_iter()
            .zip(crate::offload::pcms(&inputs, self.rate).await?)
        {
            self.rates.insert(key.clone(), source);
            self.samples.insert(key, Arc::new(pcm));
        }
        self.decode_ms += started.elapsed().as_secs_f64() * 1000.;
        Ok(())
    }
}

/// Another rider's ATV engine (GameAudio's remote banks): its `vehicle.aud`
/// table and the engine samples, decoded at `rate`.
pub async fn load_remote_engine(
    content: &Content,
    key: super::remote::EngineKey,
    rate: u32,
) -> Result<(EngineAudioConfig, HashMap<String, Arc<Pcm>>)> {
    if let super::remote::EngineKey::Quad(quad) = key
        && quad >= 27
    {
        return load_remote_motorcycle_engine(content, quad, rate).await;
    }
    let table = Archive::open(content, "/AUDIOTBL.RES").await?;
    let index = table
        .members
        .iter()
        .position(|m| m.name.to_ascii_lowercase().ends_with("vehicle.aud"))
        .ok_or("original vehicle audio table is missing")?;
    let bytes = table.member(index).await?;
    let engine = match key {
        super::remote::EngineKey::Quad(quad) => EngineAudioConfig::parse(&bytes, u32::from(quad))?,
        super::remote::EngineKey::Set(..) => {
            EngineAudioConfig::parse_set(&bytes, &key.set_name().unwrap_or_default())?
        }
    };
    let sounds = Archive::open(content, "/AUDIO/AUDIO.RES").await?;
    let mut samples = HashMap::new();
    for name in engine.samples.iter().flatten() {
        let key = normal(name);
        if samples.contains_key(&key) {
            continue;
        }
        let Some(index) = find(&sounds, &key) else {
            continue;
        };
        let audio = decode_vag(&sounds.member(index).await?)?;
        crate::yield_frame().await;
        // Resampled in pieces across frames: a bank never holds up the ride.
        samples.insert(key, Arc::new(Pcm::from_vag_spread(&audio, rate).await));
    }
    Ok((engine, samples))
}

/// A motorcycle's MX archive and its class's engine table
/// (`vehicle_<size>a.aud`).
async fn mx_engine(content: &Content, quad: u16) -> Result<(Archive, EngineAudioConfig)> {
    let (size, _) = atv_core::motorcycle::class(quad)?;
    let selection = atv_core::player_models::selection(quad, 0)?;
    let table = Archive::open(content, &selection.quad).await?;
    let name = format!("vehicle_{size}a.aud");
    let index = table
        .members
        .iter()
        .position(|m| {
            m.name
                .replace('\\', "/")
                .rsplit('/')
                .next()
                .is_some_and(|last| last.eq_ignore_ascii_case(&name))
        })
        .ok_or("original vehicle audio table is missing")?;
    let engine = EngineAudioConfig::parse_mx(&table.member(index).await?)?;
    Ok((table, engine))
}

/// A remote motorcycle's engine: its MX archive's audio table and samples,
/// as the player's own bike reads them ([`load_motorcycle`]).
async fn load_remote_motorcycle_engine(
    content: &Content,
    quad: u16,
    rate: u32,
) -> Result<(EngineAudioConfig, HashMap<String, Arc<Pcm>>)> {
    let (table, engine) = mx_engine(content, quad).await?;
    let mut samples = HashMap::new();
    for name in engine.samples.iter().flatten() {
        let key = normal(name);
        if samples.contains_key(&key) {
            continue;
        }
        let Some(index) = find(&table, &key) else {
            continue;
        };
        let audio = decode_vag(&table.member(index).await?)?;
        crate::yield_frame().await;
        samples.insert(key, Arc::new(Pcm::from_vag_spread(&audio, rate).await));
    }
    Ok((engine, samples))
}

fn find(archive: &Archive, key: &str) -> Option<usize> {
    let with = format!("{key}.vag");
    archive.members.iter().position(|m| {
        let name = normal(&m.name);
        name == key || name == with
    })
}

/// An ATV bank's tables: the vehicle and rider tables and the venue's sounds.
struct AtvTables {
    engine: EngineAudioConfig,
    effects: Option<sound_effects::Config>,
    vocal_banks: Option<qs::VocalBanks>,
    chassis: Option<qs::ChassisBanks>,
    engine_sets: Vec<(u32, u32)>,
    drive: Vec<qs::DriveOnSound>,
    ambient: Vec<world_sounds::Ambient>,
    soultree: Vec<world_sounds::Soultree>,
}

async fn atv_tables(
    content: &Content,
    course_archive: &str,
    environment: &str,
    quad: u32,
) -> Result<AtvTables> {
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
    let vocal_banks = rider
        .as_deref()
        .and_then(|bytes| qs::VocalBanks::parse(bytes).ok());
    let chassis = qs::ChassisBanks::parse(&vehicle).ok();
    let engine_sets = atv_core::engine_audio::engine_sets(&vehicle).unwrap_or_default();
    let (drive, ambient, soultree) = venue_sounds(content, course_archive, environment).await?;
    Ok(AtvTables {
        engine,
        effects,
        vocal_banks,
        chassis,
        engine_sets,
        drive,
        ambient,
        soultree,
    })
}

/// The samples an ATV bank decodes up front (vocals stay encoded).
fn atv_sample_names(
    engine: &EngineAudioConfig,
    effects: &Option<sound_effects::Config>,
    chassis: &Option<qs::ChassisBanks>,
    drive: &[qs::DriveOnSound],
    ambient: &[world_sounds::Ambient],
    soultree: &[world_sounds::Soultree],
) -> Vec<String> {
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
    names
}

/// Each named sample's bank key and VAG, once, in order; ones the bank
/// lacks (an older package) are left out.
async fn read_samples(
    sounds: &Archive<'_>,
    names: Vec<String>,
) -> Result<(Vec<String>, Vec<Vec<u8>>)> {
    let mut keys = Vec::new();
    let mut vags = Vec::new();
    for name in names {
        let key = normal(&name);
        if keys.contains(&key) {
            continue;
        }
        let Some(index) = find(sounds, &key) else {
            continue;
        };
        vags.push(sounds.member(index).await?);
        keys.push(key);
    }
    Ok((keys, vags))
}

/// The VAGs [`load`] decodes up front for an ATV bank, plus the `extra`
/// samples a ride adds with [`Bank::load_samples`]: the inputs of the
/// [`crate::offload::VAG_PCM`] jobs a menu preload (`crate::preload`) runs
/// ahead of the ride so its bank reads them from the decoded cache. A
/// motorcycle's bank decodes on the page's thread uncached: none.
pub async fn job_inputs(
    content: &Content,
    course_archive: &str,
    environment: &str,
    quad: u32,
    extra: &[&str],
) -> Result<Vec<Vec<u8>>> {
    if quad >= 27 {
        return Ok(Vec::new());
    }
    let tables = atv_tables(content, course_archive, environment, quad).await?;
    let mut names = atv_sample_names(
        &tables.engine,
        &tables.effects,
        &tables.chassis,
        &tables.drive,
        &tables.ambient,
        &tables.soultree,
    );
    names.extend(extra.iter().map(|name| name.to_string()));
    let sounds = Archive::open(content, "/AUDIO/AUDIO.RES").await?;
    Ok(read_samples(&sounds, names).await?.1)
}

/// About how many bytes a VAG's decode at `rate` holds (its frames at the
/// output rate, 32-bit samples per channel); 0 when the header is invalid.
pub fn pcm_bytes(vag: &[u8], rate: u32) -> u64 {
    atv_core::audio::VagStream::new(vag, vag.len()).map_or(0, |stream| {
        let info = stream.info;
        let frames = info.frames_max as u64 * u64::from(rate) / u64::from(info.sample_rate) + 2;
        frames * u64::from(info.channels) * 4
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
    if quad >= 27 {
        return load_motorcycle(content, course_archive, environment, quad, rate).await;
    }
    let AtvTables {
        engine,
        effects,
        vocal_banks,
        chassis,
        engine_sets,
        drive,
        mut ambient,
        mut soultree,
    } = atv_tables(content, course_archive, environment, quad).await?;
    let vocals = vocal_banks
        .as_ref()
        .map(|banks| banks.lists(female).clone());
    let names = atv_sample_names(&engine, &effects, &chassis, &drive, &ambient, &soultree);

    let sounds = Archive::open(content, "/AUDIO/AUDIO.RES").await?;
    let mut samples = HashMap::new();
    let mut rates = HashMap::new();
    // Read every sample, then decode them together (in parallel off the
    // page's thread on the web: the bank loads while the ride draws).
    let (keys, vags) = read_samples(&sounds, names).await?;
    let started = web_time::Instant::now();
    let inputs: Vec<&[u8]> = vags.iter().map(Vec::as_slice).collect();
    for (key, (source, pcm)) in keys
        .into_iter()
        .zip(crate::offload::pcms(&inputs, rate).await?)
    {
        rates.insert(key.clone(), source);
        samples.insert(key, Arc::new(pcm));
    }
    let decode_ms = started.elapsed().as_secs_f64() * 1000.;
    let mut vocal_cache = VocalCache::default();
    // Both genders' clips stay encoded: race CPUs use either list.
    let all_vocals = vocal_banks
        .iter()
        .flat_map(|b| b.female.iter().chain(&b.male))
        .flatten();
    for name in all_vocals {
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
        vocal_banks,
        engine_sets,
        chassis,
        drive,
        ambient,
        soultree,
        crowd_venue,
        decode_ms,
    })
}

/// The venue's world sound sources (`loadVenue`): ambient emitters and
/// soultrees, plus the terrain drive-on table the ATV uses.
async fn venue_sounds(
    content: &Content,
    course_archive: &str,
    environment: &str,
) -> Result<(
    Vec<qs::DriveOnSound>,
    Vec<world_sounds::Ambient>,
    Vec<world_sounds::Soultree>,
)> {
    let course = Archive::open(content, course_archive).await?;
    match course.named(&format!("{environment}.aud")) {
        Some(index) => {
            let aud = VenueAud::parse(&course.member(index).await?)?;
            let (ambient, soultree) = world_sounds::parse(&aud)?;
            Ok((aud.terrain_drive_on()?, ambient, soultree))
        }
        None => Ok(Default::default()),
    }
}

/// A motorcycle's bank (GameAudio.loadBank for quad ids 27+): the MX
/// archive's `vehicle_<size>A.aud` engine table (op 101) and its samples;
/// samples the bike archive lacks (the venue's) come from AUDIO.RES. No
/// rider effects, vocals, chassis or drive-on sounds.
async fn load_motorcycle(
    content: &Content,
    course_archive: &str,
    environment: &str,
    quad: u32,
    rate: u32,
) -> Result<Bank> {
    let id = u16::try_from(quad).map_err(|_| "invalid motorcycle id")?;
    let (table, engine) = mx_engine(content, id).await?;
    let (_, mut ambient, mut soultree) = venue_sounds(content, course_archive, environment).await?;
    let names: Vec<String> = engine
        .samples
        .iter()
        .flatten()
        .chain(&engine.impact_samples)
        .cloned()
        .chain(ambient.iter().map(|a| a.sample.clone()))
        .chain(soultree.iter().map(|s| s.sample.clone()))
        .collect();
    let mut scene = None;
    let mut samples = HashMap::new();
    let mut rates = HashMap::new();
    let mut decode_ms = 0.;
    for name in names {
        let key = normal(&name);
        if samples.contains_key(&key) {
            continue;
        }
        let vag = match find(&table, &key) {
            Some(index) => table.member(index).await?,
            None => {
                if scene.is_none() {
                    scene = Some(Archive::open(content, "/AUDIO/AUDIO.RES").await?);
                }
                let bank = scene.as_ref().unwrap();
                let Some(index) = find(bank, &key) else {
                    continue;
                };
                bank.member(index).await?
            }
        };
        let started = web_time::Instant::now();
        let audio = decode_vag(&vag)?;
        rates.insert(key.clone(), audio.info.sample_rate);
        samples.insert(key, Arc::new(Pcm::from_vag(&audio, rate)));
        decode_ms += started.elapsed().as_secs_f64() * 1000.;
        // The bank decodes after the ride is drawn: keep its frames (web).
        crate::yield_frame().await;
    }
    let crowd_venue = ambient
        .iter()
        .map(|a| &a.sample)
        .chain(soultree.iter().map(|s| &s.sample))
        .any(|s| s.starts_with("crowd/"));
    ambient.retain(|a| samples.contains_key(&normal(&a.sample)));
    soultree.retain(|s| samples.contains_key(&normal(&s.sample)));
    Ok(Bank {
        samples,
        vocal_cache: VocalCache::default(),
        rate,
        rates,
        engine,
        effects: None,
        vocals: None,
        vocal_banks: None,
        engine_sets: Vec::new(),
        chassis: None,
        drive: Vec::new(),
        ambient,
        soultree,
        crowd_venue,
        decode_ms,
    })
}

/// A bank of the given samples for voice tests (no engine, vocal, chassis
/// or venue tables).
#[cfg(test)]
pub(crate) fn test_bank(
    samples: &[(&str, Arc<Pcm>)],
    effects: Option<sound_effects::Config>,
) -> Bank {
    Bank {
        samples: samples
            .iter()
            .map(|(name, pcm)| (normal(name), pcm.clone()))
            .collect(),
        vocal_cache: VocalCache::default(),
        rate: 48000,
        rates: HashMap::new(),
        engine: EngineAudioConfig {
            engine_set: String::new(),
            stroke: 4,
            low_torque_speed: 0.,
            mid_torque_speed: 0.,
            airborne_time_to_decel: 0.,
            overlap_time: 0.,
            min_distance: 0.,
            max_distance: 1.,
            doppler_factor: 0.,
            samples: Default::default(),
            impact_samples: Vec::new(),
        },
        effects,
        vocals: None,
        vocal_banks: None,
        engine_sets: Vec::new(),
        chassis: None,
        drive: Vec::new(),
        ambient: Vec::new(),
        soultree: Vec::new(),
        crowd_venue: false,
        decode_ms: 0.,
    }
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
    fn pcm_estimates_cover_the_decode() {
        // A mono VAG: 10 ADPCM blocks of 28 frames at 22050 Hz.
        let mut vag = b"VAGp".to_vec();
        vag.extend_from_slice(&[0; 8]);
        vag.extend_from_slice(&160u32.to_be_bytes());
        vag.extend_from_slice(&22050u32.to_be_bytes());
        vag.resize(48 + 160, 0);
        let pcm = Pcm::from_vag(&decode_vag(&vag).unwrap(), 48000);
        let estimate = pcm_bytes(&vag, 48000);
        let actual = (pcm.data.len() * 4) as u64;
        assert!(
            estimate >= actual && estimate <= actual + 64,
            "{estimate} vs {actual}"
        );
        assert_eq!(pcm_bytes(b"not a vag", 48000), 0);
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
