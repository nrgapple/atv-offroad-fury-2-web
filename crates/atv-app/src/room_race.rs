//! Races in a multiplayer room (research/MULTIPLAYER.md "Room races").
//!
//! Every rider runs the original race rules (`atv_core::race`) for their
//! own ATV on their own machine: grid slot, 3-2-1-GO, checkpoints, laps,
//! finish gate and track window. Riders report their progress
//! (`Packet::Progress`); the room host owns the phase (grid, countdown,
//! racing, results) and the final order, which it broadcasts as
//! `GameState` game 3 so every peer shows the same results.
//!
//! [`Field`] is the participant list (rider ids with start slots) and the
//! host's state machine. It knows nothing about sockets or rendering, so a
//! local split-screen race can drive the same rules by reporting each local
//! player's progress under its own id.
use atv_game::multiplayer::{GameState, RACE_DNF, RACE_RIDERS, RaceInfo, RaceProgress, race_phase};
use std::collections::BTreeMap;

/// The host waits this long for every rider to load before starting the
/// countdown without them (ms, authored).
pub const LOAD_WAIT_MS: f64 = 30_000.;
/// After the first finisher, the race ends this long later for everyone
/// still riding (ms, authored; the original's online race-over timer is not
/// recovered).
pub const FINISH_WAIT_MS: f64 = 30_000.;
/// At the finish wait, a still-connected rider not heard from for this long
/// (ms) may have finished behind a slow link: the host waits for it up to
/// [`LATE_GRACE_MS`] longer before ranking it as not finished (authored).
pub const STALE_MS: f64 = 3_000.;
/// The longest extra wait for stale riders after the finish wait (ms).
pub const LATE_GRACE_MS: f64 = 30_000.;
/// The countdown the host broadcasts (`0020f780`: 4.0 s, live at the end).
pub const COUNTDOWN_S: f32 = 4.;

/// One results row, in finish order.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: u8,
    pub name: String,
    /// Finish time (s), None for a rider who did not finish.
    pub time: Option<f32>,
}

/// A rider's live standing input.
fn rank_key(p: &RaceProgress) -> (u32, i64) {
    if p.finish_ms > 0 {
        (p.finish_ms, 0)
    } else {
        (
            RACE_DNF,
            -(i64::from(p.lap) * 10_000_000 + (p.distance.max(0.) * 10.) as i64),
        )
    }
}

/// Riders in race order: finishers by time, then the rest by laps and
/// distance (ties by id so every peer sorts the same).
pub fn order(progress: &BTreeMap<u8, RaceProgress>, racers: &[u8]) -> Vec<u8> {
    let mut ids: Vec<u8> = racers.to_vec();
    ids.sort_by_key(|id| {
        let key = progress.get(id).map_or((RACE_DNF, i64::MAX), rank_key);
        (key, *id)
    });
    ids
}

/// The host's race: setup, phase and everyone's progress.
#[derive(Clone, Debug)]
pub struct Field {
    pub serial: u32,
    pub info: RaceInfo,
    pub phase: u8,
    /// Seconds to GO during the countdown.
    pub timer: f32,
    pub scores: [u32; RACE_RIDERS],
    phase_at: f64,
    first_finish: Option<f64>,
    /// Latest progress by participant id (this race's serial only).
    pub progress: BTreeMap<u8, RaceProgress>,
    /// Racers seen leaving the room. They stay out even if a late joiner
    /// later takes the same room slot id (that rider spectates).
    gone: [bool; RACE_RIDERS],
    /// A report changed since the last step / when it was last heard (ms).
    fresh: [bool; RACE_RIDERS],
    heard: [f64; RACE_RIDERS],
}

/// Start slots for the riders (ascending ids) from the original shuffle
/// (`001f6350`, `atv_core::race::start_slots`) on a per-race seed.
pub fn grid(ids: &[u8], seed: u64) -> [u8; RACE_RIDERS] {
    let mut out = [0; RACE_RIDERS];
    let riders: Vec<u8> = ids
        .iter()
        .copied()
        .filter(|id| (*id as usize) < RACE_RIDERS)
        .take(RACE_RIDERS)
        .collect();
    let mut random = atv_core::engine_audio::OriginalRandom::with_seed(seed);
    let slots = atv_core::race::start_slots(riders.len(), &mut random);
    for (id, slot) in riders.iter().zip(slots) {
        out[*id as usize] = slot as u8;
    }
    out
}

impl Field {
    /// A new race for `ids` (the room roster, ascending) at `now` (ms).
    pub fn new(serial: u32, variant: u8, laps: u8, ids: &[u8], seed: u64, now: f64) -> Self {
        Self {
            serial,
            info: RaceInfo {
                variant,
                laps: laps.clamp(1, 5),
                grid: grid(ids, seed),
            },
            phase: race_phase::GRID,
            timer: COUNTDOWN_S,
            scores: [0; RACE_RIDERS],
            phase_at: now,
            first_finish: None,
            progress: BTreeMap::new(),
            gone: [false; RACE_RIDERS],
            fresh: [false; RACE_RIDERS],
            heard: [f64::NEG_INFINITY; RACE_RIDERS],
        }
    }

    /// The race's riders (ids with a start slot), ascending.
    pub fn racers(&self) -> Vec<u8> {
        (0..RACE_RIDERS as u8)
            .filter(|id| self.info.grid[*id as usize] > 0)
            .collect()
    }

    /// Riders on the grid: their count, which sizes the start grid.
    pub fn riders(&self) -> usize {
        self.racers().len()
    }

    /// A rider's progress report; other races' reports are ignored.
    pub fn report(&mut self, progress: RaceProgress) {
        if progress.serial == self.serial
            && (progress.peer as usize) < RACE_RIDERS
            && self.info.grid[progress.peer as usize] > 0
            && !self.gone[progress.peer as usize]
            && self.phase != race_phase::RESULTS
        {
            let before = self.progress.get(&progress.peer).copied();
            if before.is_none_or(|b| b.distance != progress.distance || b.lap != progress.lap) {
                self.fresh[progress.peer as usize] = true;
            }
            let entry = self.progress.entry(progress.peer).or_insert(progress);
            // A finish time, once reported, stands.
            let finish = if entry.finish_ms > 0 {
                entry.finish_ms
            } else {
                progress.finish_ms
            };
            *entry = RaceProgress {
                finish_ms: finish,
                ..progress
            };
        }
    }

    /// A rider left the room: they stay in the results as not finished, and
    /// nobody waits for them (nor for a late joiner reusing their id).
    fn present(&mut self, roster: &[u8]) -> Vec<u8> {
        for id in self.racers() {
            if !roster.contains(&id) {
                self.gone[id as usize] = true;
            }
        }
        self.racers()
            .into_iter()
            .filter(|id| !self.gone[*id as usize])
            .collect()
    }

    /// Host: advance the phase at `now` (ms) for the current roster.
    /// Returns true when the phase changed.
    pub fn step(&mut self, now: f64, roster: &[u8]) -> bool {
        let present = self.present(roster);
        for id in 0..RACE_RIDERS {
            if std::mem::take(&mut self.fresh[id]) {
                self.heard[id] = now;
            }
        }
        let before = self.phase;
        match self.phase {
            race_phase::GRID => {
                let loaded = present.iter().all(|id| self.progress.contains_key(id));
                if loaded || now - self.phase_at >= LOAD_WAIT_MS {
                    self.phase = race_phase::COUNTDOWN;
                    self.phase_at = now;
                    self.timer = COUNTDOWN_S;
                }
            }
            race_phase::COUNTDOWN => {
                self.timer = (COUNTDOWN_S - ((now - self.phase_at) / 1000.) as f32).max(0.);
                if self.timer <= 0. {
                    self.phase = race_phase::RACING;
                    self.phase_at = now;
                }
            }
            race_phase::RACING => {
                let finished = |id: &u8| self.progress.get(id).is_some_and(|p| p.finish_ms > 0);
                if self.first_finish.is_none() && self.racers().iter().any(finished) {
                    self.first_finish = Some(now);
                }
                let all = present.iter().all(finished);
                // A rider still riding (heard lately) is ranked at the finish
                // wait; one gone quiet may have crossed the line behind a slow
                // link, so the host waits a little longer for its report.
                let quiet = present
                    .iter()
                    .any(|id| !finished(id) && now - self.heard[*id as usize] > STALE_MS);
                let timed_out = self.first_finish.is_some_and(|t| {
                    now - t >= FINISH_WAIT_MS
                        && (!quiet || now - t >= FINISH_WAIT_MS + LATE_GRACE_MS)
                });
                if all || timed_out || present.is_empty() {
                    self.finish();
                }
            }
            _ => {}
        }
        self.phase != before
    }

    /// Final order into `scores`.
    fn finish(&mut self) {
        let mut dnf = 0;
        for id in order(&self.progress, &self.racers()) {
            let score = match self.progress.get(&id) {
                Some(p) if p.finish_ms > 0 => p.finish_ms,
                _ => {
                    dnf += 1;
                    RACE_DNF + dnf
                }
            };
            self.scores[id as usize] = score;
        }
        self.phase = race_phase::RESULTS;
    }

    /// The broadcast state.
    pub fn state(&self) -> GameState {
        GameState {
            game: 3,
            serial: self.serial,
            phase: self.phase,
            timer: self.timer,
            scores: self.scores,
            holder: atv_game::multiplayer::NO_PEER,
            race: self.info,
            ..Default::default()
        }
    }

    /// A guest who becomes the room host mid-race (the relay hands the room
    /// to the lowest remaining rider when the host leaves) takes the race
    /// over from its mirror at `now` (ms): the countdown keeps its time
    /// left, a finish wait restarts, and the old host ranks as gone at the
    /// next step.
    pub fn adopt(&mut self, now: f64) {
        self.phase_at = match self.phase {
            race_phase::COUNTDOWN => now - f64::from(COUNTDOWN_S - self.timer) * 1000.,
            _ => now,
        };
        self.first_finish = self
            .progress
            .values()
            .any(|p| p.finish_ms > 0)
            .then_some(now);
        // Everyone gets a fresh hearing under the new host.
        self.heard = [now; RACE_RIDERS];
    }

    /// A guest's copy of the host's race.
    pub fn mirror(state: &GameState) -> Self {
        Self {
            serial: state.serial,
            info: state.race,
            phase: state.phase,
            timer: state.timer,
            scores: state.scores,
            phase_at: 0.,
            first_finish: None,
            progress: BTreeMap::new(),
            gone: [false; RACE_RIDERS],
            fresh: [false; RACE_RIDERS],
            heard: [f64::NEG_INFINITY; RACE_RIDERS],
        }
    }
}

/// The agreed results from a RESULTS state: rows in finish order.
pub fn results(state: &GameState, name: &dyn Fn(u8) -> String) -> Vec<Row> {
    let mut ids: Vec<u8> = (0..RACE_RIDERS as u8)
        .filter(|id| state.scores[*id as usize] > 0)
        .collect();
    ids.sort_by_key(|id| (state.scores[*id as usize], *id));
    ids.into_iter()
        .map(|id| {
            let s = state.scores[id as usize];
            Row {
                id,
                name: name(id),
                time: (s < RACE_DNF).then(|| s as f32 / 1000.),
            }
        })
        .collect()
}

/// `m:ss.cc` like the race HUD.
pub fn clock(seconds: f32) -> String {
    let cs = (seconds * 100.).round() as u32;
    format!("{}:{:02}.{:02}", cs / 6000, cs / 100 % 60, cs % 100)
}

/// The results line shown on every peer.
pub fn summary(rows: &[Row]) -> String {
    rows.iter()
        .enumerate()
        .map(|(place, r)| {
            format!(
                "{}. {} {}",
                place + 1,
                r.name,
                r.time.map_or_else(|| "DNF".to_string(), clock)
            )
        })
        .collect::<Vec<_>>()
        .join("   ")
}

/// What the app does after a room-race frame.
#[derive(Debug)]
pub enum Action {
    /// Reload the room's course as this race's ride.
    Load(atv_game::ride::race::Settings),
    /// The host's final results arrived: (title, summary).
    Results(String),
}

/// A room race beside a [`crate::multiplayer::Net`]: the host's [`Field`]
/// or a guest's mirror of it, and this rider's part.
pub struct RoomRace {
    pub field: Field,
    pub host: bool,
    /// This rider's ride is this race (loaded with its grid slot).
    pub loaded: bool,
    /// The results were shown.
    pub shown: bool,
    last_progress: f64,
    last_broadcast: f64,
    went: bool,
    rows: Vec<Row>,
    /// Racers' names as last seen in the room, so a leaver keeps its name
    /// in the results.
    names: BTreeMap<u8, String>,
}

/// Ride settings for this rider's slot in `field`.
fn settings(field: &Field, me: u8, seed: u64) -> Option<atv_game::ride::race::Settings> {
    let slot = *field.info.grid.get(me as usize)?;
    (slot > 0).then(|| atv_game::ride::race::Settings {
        laps: u32::from(field.info.laps),
        cpu: 0,
        seed,
        intro: false,
        room: Some((u32::from(slot), field.riders())),
        course: atv_game::launch::RaceVariant::ALL
            .get(field.info.variant as usize)
            .copied()
            .unwrap_or_default(),
        ..Default::default()
    })
}

impl RoomRace {
    /// The host starts a race for the room's roster.
    pub fn host(
        net: &mut crate::multiplayer::Net,
        previous: u32,
        variant: u8,
        laps: u8,
        seed: u64,
    ) -> Option<(Self, Action)> {
        let (Some(me), roster) = net.roster() else {
            return None;
        };
        let now = net.session_now() as f64;
        let field = Field::new(
            previous.wrapping_add(1).max(1),
            variant,
            laps,
            &roster,
            seed,
            now,
        );
        net.send_game(&field.state());
        let load = settings(&field, me, u64::from(field.serial))?;
        Some((
            Self {
                field,
                host: true,
                loaded: false,
                shown: false,
                last_progress: 0.,
                last_broadcast: now,
                went: false,
                rows: Vec::new(),
                names: BTreeMap::new(),
            },
            Action::Load(load),
        ))
    }

    /// A guest sees the host's new race: load it when it has a slot for
    /// this rider and the room's course ships the race (else spectate until
    /// the next one; an untrusted host cannot make a guest load a course
    /// that does not exist).
    pub fn guest(state: &GameState, me: u8, environment: &str) -> (Self, Option<Action>) {
        let field = Field::mirror(state);
        let ships = atv_game::launch::RaceVariant::ALL
            .get(state.race.variant as usize)
            .is_some_and(|v| v.exists(environment));
        // A rider first seeing a race already under way (a late joiner, maybe
        // reusing a leaver's slot id) spectates it and joins the next one.
        let load = (state.phase <= race_phase::COUNTDOWN && ships)
            .then(|| settings(&field, me, u64::from(state.serial)))
            .flatten()
            .map(Action::Load);
        (
            Self {
                field,
                host: false,
                loaded: false,
                shown: false,
                last_progress: 0.,
                last_broadcast: 0.,
                went: false,
                rows: Vec::new(),
                names: BTreeMap::new(),
            },
            load,
        )
    }

    /// The race's own results panel waits for the host's.
    pub fn pending(&self) -> bool {
        self.loaded && self.field.phase != race_phase::RESULTS
    }

    /// One frame: report progress, run (host) or mirror (guest) the phase,
    /// start the countdown, feed the standings to the HUD.
    pub fn frame(
        &mut self,
        ride: &mut atv_game::ride::Ride,
        net: &mut crate::multiplayer::Net,
    ) -> Option<Action> {
        let now = net.session_now() as f64;
        // The host left and the room passed to this rider: run the race on.
        if !self.host && net.is_host() {
            self.host = true;
            self.field.adopt(now);
            self.last_broadcast = 0.;
        }
        // The ride is this race once its reload carries the race's serial.
        self.loaded = ride.race().is_some_and(|r| {
            let s = r.settings();
            s.room.is_some() && s.seed == u64::from(self.field.serial)
        });
        let (me, roster) = net.roster();
        for (id, n) in roster.iter().zip(net.names(&roster)) {
            if self
                .field
                .info
                .grid
                .get(*id as usize)
                .is_some_and(|s| *s > 0)
            {
                self.names.entry(*id).or_insert(n);
            }
        }
        let known = self.names.clone();
        let name = |id: u8| {
            known
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("Rider {}", id + 1))
        };
        // This rider's progress.
        let mine = ride.race().filter(|_| self.loaded).map(|r| RaceProgress {
            peer: me.unwrap_or(0),
            serial: self.field.serial,
            lap: r.race.laps_completed(None).min(10) as u8,
            distance: r.race.player_distance().clamp(-10_000., 1_000_000.),
            finish_ms: r.race.player_finish().map_or(0, |t| {
                ((t * 1000.) as u32).clamp(1, atv_game::multiplayer::RACE_MAX_MS)
            }),
        });
        if now - self.last_progress >= 200. {
            self.last_progress = now;
            net.send_progress(mine.unwrap_or_default());
        }
        if self.host {
            if let (Some(p), Some(id)) = (mine, me) {
                self.field.report(RaceProgress { peer: id, ..p });
            }
            let reports: Vec<RaceProgress> = net.race_progress().values().copied().collect();
            for p in reports {
                self.field.report(p);
            }
            let changed = self.field.step(now, &roster);
            let every = if self.field.phase == race_phase::COUNTDOWN {
                250.
            } else {
                1000.
            };
            if changed || now - self.last_broadcast >= every {
                self.last_broadcast = now;
                net.send_game(&self.field.state());
            }
        } else if let Some(state) = net
            .game_state()
            .filter(|g| g.game == 3 && g.serial == self.field.serial)
        {
            let progress = std::mem::take(&mut self.field.progress);
            self.field = Field::mirror(state);
            self.field.progress = progress;
            for p in net.race_progress().values() {
                self.field.report(*p);
            }
            if let (Some(p), Some(id)) = (mine, me) {
                self.field.report(RaceProgress { peer: id, ..p });
            }
        }
        // GO together: the host's countdown less half the round trip.
        if self.loaded && !self.went {
            let half_rtt = if self.host {
                0.
            } else {
                net.rtt_ms().unwrap_or(0) as f32 / 2000.
            };
            match self.field.phase {
                race_phase::COUNTDOWN => {
                    ride.room_go(self.field.timer - half_rtt);
                    self.went = true;
                }
                race_phase::RACING | race_phase::RESULTS => {
                    ride.room_go(0.);
                    self.went = true;
                }
                _ => {}
            }
        }
        // Standings across the room for the HUD and the race's results.
        let racers = self.field.racers();
        let final_rows =
            (self.field.phase == race_phase::RESULTS).then(|| results(&self.field.state(), &name));
        let ids: Vec<u8> = match &final_rows {
            Some(rows) => rows.iter().map(|r| r.id).collect(),
            None => order(&self.field.progress, &racers),
        };
        if let Some(state) = ride.race_mut().filter(|_| self.loaded) {
            state.room = Some(
                ids.iter()
                    .map(|id| {
                        let p = self.field.progress.get(id).copied().unwrap_or_default();
                        let time = match &final_rows {
                            Some(rows) => rows.iter().find(|r| r.id == *id).and_then(|r| r.time),
                            None => (p.finish_ms > 0).then(|| p.finish_ms as f32 / 1000.),
                        };
                        atv_game::ride::race::RoomRow {
                            name: name(*id),
                            laps: u32::from(p.lap),
                            time,
                            progress: p.distance,
                            player: Some(*id) == me,
                        }
                    })
                    .collect(),
            );
        }
        if let Some(rows) = final_rows
            && !self.shown
        {
            self.shown = true;
            self.rows = rows;
            return Some(Action::Results(summary(&self.rows)));
        }
        None
    }

    /// Diagnostics (`#ui-state` ride.roomRace).
    pub fn state(&self) -> serde_json::Value {
        serde_json::json!({
            "host": self.host,
            "serial": self.field.serial,
            "phase": self.field.phase,
            "timer": self.field.timer,
            "laps": self.field.info.laps,
            "grid": self.field.info.grid,
            "scores": self.field.scores,
            "loaded": self.loaded,
            "went": self.went,
            "progress": self.field.progress.values().map(|p| serde_json::json!({
                "peer": p.peer, "lap": p.lap, "distance": p.distance, "finish": p.finish_ms,
            })).collect::<Vec<_>>(),
            "results": self.rows.iter().map(|r| serde_json::json!({
                "id": r.id, "name": r.name, "time": r.time,
            })).collect::<Vec<_>>(),
            "summary": summary(&self.rows),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(peer: u8, serial: u32, lap: u8, distance: f32, finish_ms: u32) -> RaceProgress {
        RaceProgress {
            peer,
            serial,
            lap,
            distance,
            finish_ms,
        }
    }

    #[test]
    fn grid_gives_each_rider_a_distinct_slot() {
        let g = grid(&[0, 2, 3], 7);
        let mut slots: Vec<u8> = g.iter().copied().filter(|s| *s > 0).collect();
        slots.sort_unstable();
        assert_eq!(slots, vec![1, 2, 3]);
        assert_eq!(g[1], 0);
        // The same seed gives every peer the same grid.
        assert_eq!(grid(&[0, 2, 3], 7), g);
        // Peers outside a private room's slots never race.
        assert_eq!(grid(&[0, 9], 1).iter().filter(|s| **s > 0).count(), 1);
    }

    #[test]
    fn host_runs_grid_countdown_racing_results() {
        let mut f = Field::new(5, 0, 2, &[0, 1], 3, 0.);
        assert_eq!(f.riders(), 2);
        // Waits for both riders to load.
        f.report(progress(0, 5, 0, 0., 0));
        assert!(!f.step(1000., &[0, 1]));
        assert_eq!(f.phase, race_phase::GRID);
        // Reports from an older race are ignored.
        f.report(progress(1, 4, 0, 0., 0));
        assert!(!f.step(1100., &[0, 1]));
        f.report(progress(1, 5, 0, 0., 0));
        assert!(f.step(1200., &[0, 1]));
        assert_eq!(f.phase, race_phase::COUNTDOWN);
        f.step(3200., &[0, 1]);
        assert!((f.timer - 2.).abs() < 1e-3);
        assert!(f.step(5200., &[0, 1]));
        assert_eq!(f.phase, race_phase::RACING);
        f.report(progress(1, 5, 2, 3000., 61_000));
        f.report(progress(0, 5, 1, 2500., 0));
        assert!(!f.step(6000., &[0, 1]));
        // A late zero cannot erase a finish.
        f.report(progress(1, 5, 2, 3010., 0));
        f.report(progress(0, 5, 2, 3000., 64_500));
        assert!(f.step(7000., &[0, 1]));
        assert_eq!(f.phase, race_phase::RESULTS);
        let state = f.state();
        state.validate().unwrap();
        let rows = results(&state, &|id| format!("R{id}"));
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 0]);
        assert_eq!(rows[0].time, Some(61.));
        assert_eq!(summary(&rows), "1. R1 1:01.00   2. R0 1:04.50");
        // Results are final.
        f.report(progress(0, 5, 2, 3000., 1));
        assert_eq!(f.state().scores, state.scores);
    }

    #[test]
    fn missing_riders_start_after_the_load_wait() {
        let mut f = Field::new(1, 0, 1, &[0, 1], 3, 0.);
        f.report(progress(0, 1, 0, 0., 0));
        assert!(!f.step(LOAD_WAIT_MS - 1., &[0, 1]));
        assert!(f.step(LOAD_WAIT_MS, &[0, 1]));
        assert_eq!(f.phase, race_phase::COUNTDOWN);
    }

    #[test]
    fn leavers_are_not_waited_for_and_rank_after_finishers() {
        let mut f = Field::new(2, 0, 1, &[0, 1, 2], 9, 0.);
        for id in 0..3 {
            f.report(progress(id, 2, 0, 0., 0));
        }
        f.step(0., &[0, 1, 2]);
        f.step(4000., &[0, 1, 2]);
        assert_eq!(f.phase, race_phase::RACING);
        f.report(progress(2, 2, 0, 900., 0));
        f.report(progress(1, 2, 0, 400., 0));
        f.report(progress(0, 2, 1, 2000., 50_000));
        // Rider 2 left; rider 1 is still riding.
        assert!(!f.step(5000., &[0, 1]));
        // Rider 1 leaves too: nobody left to wait for.
        assert!(f.step(5100., &[0]));
        let rows = results(&f.state(), &|id| format!("R{id}"));
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![0, 2, 1]);
        assert_eq!(rows[1].time, None);
        assert!(summary(&rows).ends_with("3. R1 DNF"));
    }

    #[test]
    fn a_late_joiner_reusing_a_leavers_id_is_not_waited_for() {
        let mut f = Field::new(4, 0, 1, &[0, 1], 9, 0.);
        f.report(progress(0, 4, 0, 0., 0));
        f.report(progress(1, 4, 0, 0., 0));
        f.step(0., &[0, 1]);
        f.step(4000., &[0, 1]);
        // Rider 1 leaves, a newcomer takes room slot 1.
        assert!(!f.step(5000., &[0]));
        f.report(progress(1, 4, 0, 50., 0));
        assert_eq!(
            f.progress[&1].distance, 0.,
            "a gone rider's id reports nothing"
        );
        f.report(progress(0, 4, 1, 2000., 45_000));
        assert!(
            f.step(6000., &[0, 1]),
            "the race ends once rider 0 finishes"
        );
        let rows = results(&f.state(), &|id| format!("R{id}"));
        assert_eq!(rows[1].time, None);
    }

    #[test]
    fn a_new_host_takes_over_mid_race() {
        let mut host = Field::new(7, 0, 1, &[0, 1, 2], 9, 0.);
        for id in 0..3 {
            host.report(progress(id, 7, 0, 0., 0));
        }
        host.step(0., &[0, 1, 2]);
        host.step(4000., &[0, 1, 2]);
        host.report(progress(1, 7, 1, 2000., 50_000));
        // Rider 1's mirror of the host's state, with the relayed progress.
        let mut mirror = Field::mirror(&host.state());
        mirror.progress = host.progress.clone();
        mirror.report(progress(2, 7, 0, 900., 0));
        // The host (0) leaves; rider 1 is the room's new host.
        mirror.adopt(100_000.);
        assert!(!mirror.step(100_001., &[1, 2]), "rider 2 is still racing");
        mirror.report(progress(2, 7, 1, 2000., 70_000));
        assert!(mirror.step(100_002., &[1, 2]));
        let rows = results(&mirror.state(), &|id| format!("R{id}"));
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 2, 0]);
        assert_eq!(rows[2].time, None, "the old host ranks DNF");
        // A countdown keeps its time left across the handoff.
        let mut f = Field::new(8, 0, 1, &[0, 1], 9, 0.);
        f.report(progress(0, 8, 0, 0., 0));
        f.report(progress(1, 8, 0, 0., 0));
        f.step(0., &[0, 1]);
        f.step(1000., &[0, 1]);
        let mut m = Field::mirror(&f.state());
        m.adopt(50_000.);
        m.step(50_500., &[1]);
        assert!((m.timer - 2.5).abs() < 1e-3);
    }

    #[test]
    fn a_quiet_rider_gets_a_grace_before_dnf() {
        let mut f = Field::new(9, 0, 1, &[0, 1], 9, 0.);
        f.report(progress(0, 9, 0, 0., 0));
        f.report(progress(1, 9, 0, 0., 0));
        f.step(0., &[0, 1]);
        f.step(4000., &[0, 1]);
        f.report(progress(1, 9, 0, 500., 0));
        f.step(5000., &[0, 1]);
        f.report(progress(0, 9, 1, 2000., 40_000));
        f.step(10_000., &[0, 1]);
        // Rider 1 has not been heard since 5 s: the wait is extended.
        assert!(!f.step(10_000. + FINISH_WAIT_MS, &[0, 1]));
        // Its delayed finish arrives and stands.
        f.report(progress(1, 9, 1, 2000., 39_000));
        assert!(f.step(10_000. + FINISH_WAIT_MS + 5_000., &[0, 1]));
        let rows = results(&f.state(), &|id| format!("R{id}"));
        assert_eq!(rows[0].id, 1, "the earlier time wins once it arrives");
        // A rider never heard again is ranked after the grace.
        let mut g = Field::new(10, 0, 1, &[0, 1], 9, 0.);
        g.report(progress(0, 10, 0, 0., 0));
        g.report(progress(1, 10, 0, 0., 0));
        g.step(0., &[0, 1]);
        g.step(4000., &[0, 1]);
        g.report(progress(0, 10, 1, 2000., 40_000));
        g.step(10_000., &[0, 1]);
        assert!(!g.step(10_000. + FINISH_WAIT_MS + LATE_GRACE_MS - 1., &[0, 1]));
        assert!(g.step(10_000. + FINISH_WAIT_MS + LATE_GRACE_MS, &[0, 1]));
    }

    #[test]
    fn guests_load_only_races_not_yet_under_way() {
        let mut f = Field::new(6, 0, 1, &[0, 1], 9, 0.);
        let mut state = f.state();
        assert!(RoomRace::guest(&state, 1, "sx01").1.is_some());
        // A rider without a slot spectates.
        assert!(RoomRace::guest(&state, 2, "sx01").1.is_none());
        f.report(progress(0, 6, 0, 0., 0));
        f.report(progress(1, 6, 0, 0., 0));
        f.step(0., &[0, 1]);
        f.step(4000., &[0, 1]);
        state = f.state();
        assert_eq!(state.phase, race_phase::RACING);
        assert!(RoomRace::guest(&state, 1, "sx01").1.is_none());
    }

    #[test]
    fn the_race_ends_after_the_finish_wait() {
        let mut f = Field::new(3, 0, 1, &[0, 1], 9, 0.);
        f.report(progress(0, 3, 0, 0., 0));
        f.report(progress(1, 3, 0, 0., 0));
        f.step(0., &[0, 1]);
        f.step(4000., &[0, 1]);
        f.report(progress(0, 3, 1, 2000., 40_000));
        f.step(10_000., &[0, 1]);
        // Rider 1 is still riding (heard lately), just not across the line.
        f.report(progress(1, 3, 0, 1500., 0));
        f.step(10_000. + FINISH_WAIT_MS - 1_000., &[0, 1]);
        assert!(!f.step(10_000. + FINISH_WAIT_MS - 1., &[0, 1]));
        assert!(f.step(10_000. + FINISH_WAIT_MS, &[0, 1]));
        let state = f.state();
        assert_eq!(state.scores[1], RACE_DNF + 1);
        // A guest's mirror shows the same rows.
        let mirror = Field::mirror(&state);
        assert_eq!(mirror.state().scores, state.scores);
    }

    #[test]
    fn standings_order_live_riders_by_lap_then_distance() {
        let mut p = BTreeMap::new();
        p.insert(0, progress(0, 1, 1, 100., 0));
        p.insert(1, progress(1, 1, 0, 900., 0));
        p.insert(2, progress(2, 1, 1, 300., 0));
        assert_eq!(order(&p, &[0, 1, 2, 3]), vec![2, 0, 1, 3]);
    }
}
