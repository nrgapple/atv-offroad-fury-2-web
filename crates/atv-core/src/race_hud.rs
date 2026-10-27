//! Original single-player race overlays (Overlays.pb OverlaySet1): signage,
//! infotext, lap history and the position tree. Exact-build evidence from
//! private exports: lap handler `001e9a00`, place handler `001e9f40`, lap
//! history `002e85c0`, position tree `002e5150`/`002e5440`, signage priority
//! and the 5s off-track / too-deep clocks from `00247570`. The browser draws
//! the original art from this frame; the clocks live here.
use serde::Serialize;

/// Signage overlay (OverlayObject-6) ids and priorities.
pub const TOO_DEEP: u8 = 1;
pub const OFF_TRACK: u8 = 2;
pub const WRONG_WAY: u8 = 3;
pub const FINAL_LAP: u8 = 12;
pub const FIRST_PLACE: u8 = 13;
fn priority(sign: u8) -> u8 {
    match sign {
        TOO_DEEP => 2,
        OFF_TRACK => 3,
        _ => 1,
    }
}
fn persistent(sign: u8) -> bool {
    matches!(sign, TOO_DEEP | OFF_TRACK | WRONG_WAY)
}
const SIGN_SECONDS: f32 = 3.;
const LAP_TIME_SECONDS: f32 = 2.;
const HISTORY_SECONDS: f32 = 5.;
const SLIDE_SECONDS: f32 = 0.3;
/// Seconds off the track (or too deep in water) before the rider resets.
pub const RESET_SECONDS: f32 = 5.;

/// Everything the overlays read from the race each step.
pub struct Input<'a> {
    pub racing: bool,
    pub finished: bool,
    pub laps: u32,
    /// Player's completed lap times, oldest first.
    pub lap_times: &'a [f32],
    pub best_lap: Option<usize>,
    pub place: usize,
    /// Rank (1-based) of each rider slot: the player, then each CPU.
    pub ranks: &'a [usize],
    pub wrong_way: bool,
    pub off_track: Option<f32>,
    /// Remaining deep-water seconds while submerged.
    pub too_deep: Option<f32>,
}
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Sign {
    pub id: u8,
    /// Off Track / Too Deep countdown digit.
    pub digit: Option<i32>,
}
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HistoryRow {
    pub lap: usize,
    pub time: f32,
    pub best: bool,
}
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct History {
    pub rows: Vec<HistoryRow>,
    /// Horizontal slide in overlay units (in from +200, out to +200).
    pub offset: f32,
}
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Frame {
    pub sign: Option<Sign>,
    pub infotext: Option<String>,
    pub history: Option<History>,
    /// BEST/TOTAL/LEAD alpha (0–1); the lap history fades it out.
    pub listing_alpha: f32,
    /// Animated y offset (0, 23, 46…) of each rider slot's name.
    pub tree: Vec<f32>,
    /// Scale of the player's place number (pulses on arriving at a new rank).
    pub place_scale: f32,
}
#[derive(Default)]
pub struct RaceHud {
    sign: Option<(u8, f32)>,
    waiting: Vec<u8>,
    active: Vec<u8>,
    infotext: Vec<(String, f32)>,
    history: Option<f32>,
    laps_seen: usize,
    finished: bool,
    tree: Vec<f32>,
    player_rank: usize,
    pulse_pending: bool,
    pulse: Option<f32>,
}
fn time_text(t: f32) -> String {
    let hundredths = (t * 100. + 0.5) as i64;
    if t == 0. {
        return "--:--.--".into();
    }
    if t > 5999.99 {
        return "99:99.99".into();
    }
    format!(
        "{:02}:{:02}.{:02}",
        hundredths / 6000,
        hundredths / 100 % 60,
        hundredths % 100
    )
}
impl RaceHud {
    fn show(&mut self, sign: u8) {
        let current = self.sign.map_or(0, |(s, _)| priority(s));
        if self.sign.is_none() || priority(sign) >= current {
            if let Some((old, _)) = self.sign
                && persistent(old)
                && !self.waiting.contains(&old)
            {
                self.waiting.push(old);
            }
            self.sign = Some((sign, if persistent(sign) { -1. } else { SIGN_SECONDS }));
        } else if persistent(sign) && !self.waiting.contains(&sign) {
            self.waiting.push(sign);
        }
    }
    /// Hiding a sign brings back the highest-priority waiting sign.
    fn hide(&mut self) {
        self.sign = None;
        self.waiting.retain(|s| self.active.contains(s));
        if let Some(next) = self.waiting.iter().copied().max_by_key(|s| priority(*s)) {
            self.waiting.retain(|s| *s != next);
            self.sign = Some((next, -1.));
        }
    }
    /// Queues an infotext message (`002e2cf0`), such as a record's.
    pub fn say(&mut self, text: String, seconds: f32) {
        if self.infotext.len() < 3 {
            self.infotext.push((text, seconds));
        }
    }
    pub fn step(&mut self, dt: f32, input: &Input<'_>) -> Frame {
        // Persistent signs follow their conditions.
        let mut active = Vec::new();
        if input.racing {
            if input.too_deep.is_some() {
                active.push(TOO_DEEP);
            }
            if input.off_track.is_some() {
                active.push(OFF_TRACK);
            }
            if input.wrong_way {
                active.push(WRONG_WAY);
            }
        }
        for sign in &active {
            if !self.active.contains(sign) {
                self.show(*sign);
            }
        }
        self.active = active;
        // Lap events: "Lap Time" infotext, lap history and Final Lap!.
        let laps = input.lap_times.len();
        if laps > self.laps_seen {
            self.laps_seen = laps;
            if let Some(time) = input.lap_times.last() {
                self.say(format!("Lap Time {}", time_text(*time)), LAP_TIME_SECONDS);
            }
            self.history = Some(HISTORY_SECONDS);
            if laps as u32 + 1 == input.laps {
                self.show(FINAL_LAP);
            }
        }
        if input.finished && !self.finished {
            self.finished = true;
            if (1..=3).contains(&input.place) {
                self.show(FIRST_PLACE + input.place as u8 - 1);
            }
        }
        // Timed sign expiry, and persistent signs whose condition ended.
        match self.sign {
            Some((sign, _)) if persistent(sign) && !self.active.contains(&sign) => self.hide(),
            Some((sign, t)) if !persistent(sign) => {
                if t - dt <= 0. {
                    self.hide();
                } else {
                    self.sign = Some((sign, t - dt));
                }
            }
            _ => {}
        }
        let sign = self.sign.map(|(id, _)| Sign {
            id,
            digit: match id {
                OFF_TRACK => input
                    .off_track
                    .map(|t| (RESET_SECONDS - t).max(0.) as i32 + 1),
                TOO_DEEP => input.too_deep.map(|t| t.max(0.) as i32 + 1),
                _ => None,
            },
        });
        // Infotext queue: when more wait, the current message is cut to 1s.
        if let Some((_, t)) = self.infotext.first_mut() {
            *t -= dt;
        }
        if self.infotext.len() > 1 && self.infotext[0].1 > 1. {
            self.infotext[0].1 = 1.;
        }
        if self.infotext.first().is_some_and(|(_, t)| *t <= 0.) {
            self.infotext.remove(0);
        }
        // Lap history: slide in 1s, hold, slide out 1s; listing fades 0.5s.
        let (history, listing_alpha) = match self.history {
            Some(t) => {
                let t = (t - dt).max(0.);
                self.history = (t > 0.).then_some(t);
                let first = laps.saturating_sub(5);
                let rows = (first..laps)
                    .map(|i| HistoryRow {
                        lap: i + 1,
                        time: input.lap_times[i],
                        best: input.best_lap == Some(i),
                    })
                    .collect();
                let offset = if t > 4. {
                    (t - 4.) * 200.
                } else if t < 1. {
                    (1. - t) * 200.
                } else {
                    0.
                };
                let alpha = if t > 4.5 {
                    (t - 4.5) / 0.5
                } else if t < 0.5 {
                    1. - t / 0.5
                } else {
                    0.
                };
                match self.history {
                    Some(_) => (Some(History { rows, offset }), alpha.clamp(0., 1.)),
                    None => (None, 1.),
                }
            }
            None => (None, 1.),
        };
        // Position tree: each rider's name slides to its rank in 0.3s.
        self.tree.resize(input.ranks.len(), f32::NAN);
        for (y, rank) in self.tree.iter_mut().zip(input.ranks) {
            let target = (*rank as f32 - 1.) * 23.;
            *y = if y.is_nan() {
                target
            } else {
                *y + dt.min(SLIDE_SECONDS) / SLIDE_SECONDS * (target - *y)
            };
        }
        if let Some(&rank) = input.ranks.first() {
            if self.player_rank != 0 && rank != self.player_rank {
                self.pulse_pending = true;
            }
            self.player_rank = rank;
            let arrived = self
                .tree
                .first()
                .is_some_and(|y| (y - (rank as f32 - 1.) * 23.).abs() < 0.5);
            if self.pulse_pending && arrived {
                self.pulse_pending = false;
                self.pulse = Some(0.);
            }
        }
        // Up to 1.6 over 0.1s, hold 0.3s, back over 0.1s.
        let place_scale = match self.pulse {
            Some(t) => {
                let t = t + dt;
                self.pulse = (t < 0.5).then_some(t);
                if t < 0.1 {
                    1. + 0.6 * t / 0.1
                } else if t < 0.4 {
                    1.6
                } else {
                    1. + 0.6 * ((0.5 - t) / 0.1).max(0.)
                }
            }
            None => 1.,
        };
        Frame {
            sign,
            infotext: self.infotext.first().map(|(text, _)| text.clone()),
            history,
            listing_alpha,
            tree: self.tree.clone(),
            place_scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input<'a>(laps: &'a [f32], ranks: &'a [usize]) -> Input<'a> {
        Input {
            racing: true,
            finished: false,
            laps: 3,
            lap_times: laps,
            best_lap: None,
            place: ranks[0],
            ranks,
            wrong_way: false,
            off_track: None,
            too_deep: None,
        }
    }
    #[test]
    fn formats_original_times() {
        assert_eq!(time_text(0.), "--:--.--");
        assert_eq!(time_text(75.678), "01:15.68");
        assert_eq!(time_text(7000.), "99:99.99");
    }
    #[test]
    fn off_track_outranks_wrong_way_and_counts_down() {
        let mut hud = RaceHud::default();
        let mut i = input(&[], &[1]);
        i.wrong_way = true;
        assert_eq!(hud.step(0.1, &i).sign.unwrap().id, WRONG_WAY);
        i.off_track = Some(0.6);
        let f = hud.step(0.1, &i);
        assert_eq!(
            f.sign,
            Some(Sign {
                id: OFF_TRACK,
                digit: Some(5)
            })
        );
        i.off_track = Some(4.2);
        assert_eq!(hud.step(0.1, &i).sign.unwrap().digit, Some(1));
        // Back on track the waiting Wrong Way sign returns.
        i.off_track = None;
        assert_eq!(hud.step(0.1, &i).sign.unwrap().id, WRONG_WAY);
        i.wrong_way = false;
        assert!(hud.step(0.1, &i).sign.is_none());
    }
    #[test]
    fn lap_events_show_lap_time_history_and_final_lap() {
        let mut hud = RaceHud::default();
        let mut f = hud.step(0.1, &input(&[], &[1, 2]));
        assert!(f.history.is_none() && f.infotext.is_none());
        let laps = [62.5, 60.25];
        let mut i = input(&laps, &[1, 2]);
        i.best_lap = Some(1);
        f = hud.step(0.1, &i);
        assert_eq!(f.infotext.as_deref(), Some("Lap Time 01:00.25"));
        assert_eq!(f.sign.as_ref().unwrap().id, FINAL_LAP, "lap 3 of 3 begins");
        let history = f.history.clone().unwrap();
        assert_eq!(history.rows.len(), 2);
        assert!(history.rows[1].best && !history.rows[0].best);
        assert!((history.offset - 180.).abs() < 0.01, "slides in from +200");
        for _ in 0..20 {
            f = hud.step(0.1, &i);
        }
        assert_eq!(f.history.as_ref().unwrap().offset, 0.);
        assert_eq!(f.listing_alpha, 0.);
        assert!(f.infotext.is_none(), "Lap Time shows 2s");
        assert!(f.sign.is_some(), "Final Lap! shows 3s");
        for _ in 0..10 {
            f = hud.step(0.1, &i);
        }
        assert!(f.sign.is_none());
        for _ in 0..20 {
            f = hud.step(0.1, &i);
        }
        assert!(f.history.is_none());
        assert_eq!(f.listing_alpha, 1.);
    }
    #[test]
    fn finish_place_sign_and_sliding_names() {
        let mut hud = RaceHud::default();
        let f = hud.step(0.1, &input(&[], &[2, 1]));
        assert_eq!(f.tree, vec![23., 0.]);
        let mut i = input(&[], &[1, 2]);
        let f = hud.step(0.1, &i);
        assert!(f.tree[0] > 0. && f.tree[0] < 23.);
        let mut scale = 1.;
        for _ in 0..150 {
            scale = hud.step(1. / 60., &i).place_scale.max(scale);
        }
        assert!(scale > 1.5, "the player's place number pulses on arrival");
        i.finished = true;
        i.place = 1;
        assert_eq!(hud.step(0.1, &i).sign.unwrap().id, FIRST_PLACE);
    }
}
