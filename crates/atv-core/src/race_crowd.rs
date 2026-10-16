//! Stadium crowd presentation for venues with `CrowdPresent` (every
//! Supercross course): crowd reaction cues and camera flashes.
//!
//! Exact-build evidence from private exports: crowd emitter `002cb9b0`
//! loading `Audio\Crowd\crowd.aud` (`002cbc20`), players `002cc070`–`002cc2f0`
//! with the no-repeat pick, triggers `001e8a20` (Boo), `001ea930` (Cheer),
//! `001e9a00` (Stomp), `00216190` (Win) and `002cc320` (stop Stomp); the
//! flash manager `0020f2c0`/`0020ee10` with `[CameraFlash]` from the venue
//! TOY (`0020a380`). The `Am` set is used: the setting selecting `Pro` is
//! unrecovered. The `Oh` trigger is unidentified and not reproduced. The
//! original libc `rand` stream is replaced by the port's own.
use crate::Result;
use serde::Serialize;

fn sections(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push((name.trim().to_ascii_lowercase(), Vec::new()));
        } else if let Some((key, value)) = line.split_once('=')
            && let Some((_, fields)) = out.last_mut()
        {
            fields.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    out
}
fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The crowd sound lists of `crowd.aud` for one prefix (`Am` or `Pro`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sounds {
    pub boo: Vec<String>,
    pub cheer: Vec<String>,
    pub win: Vec<String>,
    pub stomp: Vec<String>,
}
pub fn parse_crowd(text: &str, prefix: &str) -> Result<Sounds> {
    let all = sections(text);
    let list = |kind: &str| -> Result<Vec<String>> {
        let name = format!("{prefix}{kind}").to_ascii_lowercase();
        let Some((_, fields)) = all.iter().find(|(n, _)| *n == name) else {
            return Ok(Vec::new());
        };
        let count: usize = field(fields, "numsounds")
            .unwrap_or("0")
            .parse()
            .map_err(|_| "invalid crowd sound count")?;
        (1..=count.min(16))
            .map(|i| {
                field(fields, &format!("sound_{i}"))
                    .map(|s| s.replace('\\', "/").to_ascii_lowercase())
                    .ok_or_else(|| "missing crowd sound".to_string())
            })
            .collect()
    };
    Ok(Sounds {
        boo: list("Boo")?,
        cheer: list("Cheer")?,
        win: list("Win")?,
        stomp: list("Stomp")?,
    })
}

/// One crowd request. The single crowd voice drops a new clip while another
/// (non-Stomp) clip plays; a clip replaces a playing Stomp loop.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cue {
    pub sample: String,
    pub looping: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CrowdFrame {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue: Option<Cue>,
    pub stop_loop: bool,
}
/// Race events the crowd reacts to in one step.
#[derive(Default)]
pub struct Events {
    pub elapsed: f32,
    /// The player's rank this step and the leader being a CPU.
    pub player_rank: usize,
    pub player_wrecked: bool,
    /// A lap began that is the last one.
    pub final_lap: bool,
    /// Finishing places this step: (is the player, place).
    pub finishes: Vec<(bool, usize)>,
    pub single_player: bool,
}
pub struct Crowd {
    sounds: Sounds,
    last_rank: usize,
    last: [Option<usize>; 2],
    random: crate::engine_audio::OriginalRandom,
}
impl Crowd {
    pub fn new(sounds: Sounds, seed: u64) -> Self {
        Self {
            sounds,
            last_rank: 0,
            last: [None; 2],
            random: crate::engine_audio::OriginalRandom::with_seed(seed),
        }
    }
    /// `trunc(n · rand / 2³¹)`; Boo and Cheer step past a repeated pick.
    fn pick(&mut self, list: usize) -> Option<Cue> {
        let sounds = if list == 0 {
            &self.sounds.boo
        } else {
            &self.sounds.cheer
        };
        let n = sounds.len();
        if n == 0 {
            return None;
        }
        let mut i = (n as f32 * self.random.next_u31() as f32 * 4.656613e-10) as usize % n;
        if self.last[list] == Some(i) {
            i = (i + 1) % n;
        }
        self.last[list] = Some(i);
        Some(Cue {
            sample: sounds[i].clone(),
            looping: false,
        })
    }
    pub fn step(&mut self, events: &Events) -> CrowdFrame {
        let mut frame = CrowdFrame::default();
        let took_lead = self.last_rank > 1 && events.player_rank == 1;
        let lost_lead = self.last_rank == 1 && events.player_rank > 1;
        self.last_rank = events.player_rank;
        // A CPU in first, or any racer finishing, stops the Stomp loop.
        if lost_lead || !events.finishes.is_empty() {
            frame.stop_loop = true;
        }
        if events.player_wrecked && events.player_rank == 1 {
            frame.cue = self.pick(0);
        }
        if took_lead && events.elapsed >= 5. {
            frame.cue = self.pick(1);
        }
        if events.final_lap && events.single_player && events.player_rank == 2 {
            frame.cue = self.sounds.stomp.first().map(|s| Cue {
                sample: s.clone(),
                looping: true,
            });
        }
        if events
            .finishes
            .iter()
            .any(|&(player, place)| player && place == 1)
        {
            frame.cue = self.sounds.win.first().map(|s| Cue {
                sample: s.clone(),
                looping: false,
            });
        }
        frame
    }
}

/// `[CameraFlash]` from the venue TOY (`0020a380`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlashConfig {
    pub constant: f32,
    pub burst_duration: f32,
    pub player_win: f32,
    pub player_stunt: f32,
    pub ai_finish: f32,
}
impl Default for FlashConfig {
    fn default() -> Self {
        Self {
            constant: 0.01,
            burst_duration: 2.,
            player_win: 0.3,
            player_stunt: 0.4,
            ai_finish: 0.3,
        }
    }
}
pub fn parse_flash_config(toy: &str) -> Option<FlashConfig> {
    let all = sections(toy);
    let (_, fields) = all.iter().find(|(n, _)| n == "cameraflash")?;
    let d = FlashConfig::default();
    let number = |key: &str, default: f32| {
        field(fields, key)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    };
    Some(FlashConfig {
        constant: number("constantflashpercent", d.constant),
        burst_duration: number("burstduration", d.burst_duration),
        player_win: number("playerwinburstpercent", d.player_win),
        player_stunt: number("playerstuntburstpercent", d.player_stunt),
        ai_finish: number("aifinishburstpercent", d.ai_finish),
    })
}
/// Hidden `cameraflash.slt` scene models lit in random batches of
/// trunc(p · N) for 0.09s; a burst ramps p to its target over half the
/// burst time and back (`0020ee10`).
pub struct Flashes {
    config: FlashConfig,
    pool: usize,
    percent: f32,
    timer: f32,
    half: f32,
    rate: f32,
    batch: f32,
    lit: Vec<u16>,
    random: crate::engine_audio::OriginalRandom,
}
pub const FLASH_SECONDS: f32 = 0.09;
impl Flashes {
    pub fn new(config: FlashConfig, pool: usize, seed: u64) -> Self {
        Self {
            config,
            pool: pool.min(u16::MAX as usize),
            percent: config.constant,
            timer: 0.,
            half: 0.,
            rate: 0.,
            batch: 0.,
            lit: Vec::new(),
            random: crate::engine_audio::OriginalRandom::with_seed(seed),
        }
    }
    pub fn burst(&mut self, target: f32) {
        self.timer = self.timer.max(self.config.burst_duration);
        self.half = self.timer / 2.;
        self.rate = if self.half > 0. {
            (target - self.percent) / self.half
        } else {
            0.
        };
    }
    pub fn config(&self) -> FlashConfig {
        self.config
    }
    pub fn step(&mut self, dt: f32) -> &[u16] {
        if self.timer > 0. {
            self.percent = if self.timer > self.half {
                self.percent + self.rate * dt
            } else {
                self.percent - self.rate * dt
            }
            .clamp(0., 1.);
            self.timer -= dt;
        } else {
            self.percent = self.config.constant;
        }
        self.batch -= dt;
        if self.batch <= 0. {
            self.batch = FLASH_SECONDS;
            let count = ((self.percent * self.pool as f32) as usize).min(self.pool);
            self.lit.clear();
            for _ in 0..count {
                let i = (self.pool as f32 * self.random.next_u31() as f32 * 4.656613e-10) as usize;
                self.lit.push(i.min(self.pool.saturating_sub(1)) as u16);
            }
        }
        &self.lit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const AUD: &str = "[AmBoo]\r\nNumSounds = 1\r\nSound_1 = Crowd\\AmCrowd\\aboo001.vag\r\n\r\n[AmCheer]\r\nNumSounds = 2\r\nSound_1 = Crowd\\AmCrowd\\acheer01.vag\r\nSound_2 = Crowd\\AmCrowd\\acheer02.vag\r\n\r\n[AmWin]\r\nNumSounds = 1\r\nSound_1 = Crowd\\AmCrowd\\aracewin.vag\r\n\r\n[ProStomp]\r\nNumSounds = 1\r\nSound_1 = Crowd\\ProCrowd\\stomp.vag\r\n";
    #[test]
    fn parses_crowd_lists_by_prefix() {
        let am = parse_crowd(AUD, "Am").unwrap();
        assert_eq!(
            am.cheer,
            ["crowd/amcrowd/acheer01.vag", "crowd/amcrowd/acheer02.vag"]
        );
        assert!(am.stomp.is_empty(), "Stomp is a Pro list");
        assert_eq!(
            parse_crowd(AUD, "Pro").unwrap().stomp,
            ["crowd/procrowd/stomp.vag"]
        );
    }
    #[test]
    fn crowd_cheers_new_leaders_after_five_seconds_and_cheers_vary() {
        let mut crowd = Crowd::new(parse_crowd(AUD, "Am").unwrap(), 1);
        let step = |c: &mut Crowd, rank, elapsed| {
            c.step(&Events {
                elapsed,
                player_rank: rank,
                ..Events::default()
            })
        };
        step(&mut crowd, 2, 1.);
        assert!(step(&mut crowd, 1, 2.).cue.is_none(), "under 5s of racing");
        step(&mut crowd, 2, 6.);
        let a = step(&mut crowd, 1, 7.).cue.unwrap();
        assert!(step(&mut crowd, 2, 8.).stop_loop, "a CPU took the lead");
        let b = step(&mut crowd, 1, 9.).cue.unwrap();
        assert_ne!(a, b, "no immediate repeat");
        let win = crowd.step(&Events {
            player_rank: 1,
            finishes: vec![(true, 1)],
            ..Events::default()
        });
        assert_eq!(win.cue.unwrap().sample, "crowd/amcrowd/aracewin.vag");
        assert!(win.stop_loop);
        let boo = crowd.step(&Events {
            player_rank: 1,
            player_wrecked: true,
            ..Events::default()
        });
        assert_eq!(boo.cue.unwrap().sample, "crowd/amcrowd/aboo001.vag");
    }
    #[test]
    fn flashes_light_random_batches_and_burst_up_and_back() {
        let config = parse_flash_config("[CameraFlash]\r\nConstantFlashPercent=0.01\r\nBurstDuration=2.0\r\nPlayerWinBurstPercent=0.3\r\nAIStuntWinBurstPercent0.3\r\n").unwrap();
        assert_eq!(config, FlashConfig::default());
        let mut flashes = Flashes::new(config, 300, 1);
        assert_eq!(flashes.step(1. / 60.).len(), 3, "trunc(0.01 · 300)");
        flashes.burst(config.player_win);
        let mut peak = 0;
        for _ in 0..130 {
            peak = peak.max(flashes.step(1. / 60.).len());
        }
        assert!(peak > 70 && peak <= 90, "{peak}");
        for _ in 0..20 {
            flashes.step(1. / 60.);
        }
        assert_eq!(flashes.step(1. / 60.).len(), 3);
    }
}
