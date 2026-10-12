//! USA v2.00 stunt event accounting: 0024da00/db30/dcb0/dd00/d690.
//! The caller owns original game-mode eligibility, animation callbacks and
//! ground/wreck finalization. HORSE comparisons and virtual overrides are separate.
use crate::Result;
use serde::Serialize;

pub const BASE_POINTS: [f32; 24] = [
    500., 500., 500., 500., 500., 500., 500., 500., 1000., 1000., 1000., 1000., 1500., 1500.,
    1500., 1500., 1500., 1500., 1500., 1500., 2000., 2000., 2000., 2000.,
];
pub const HOLD_POINTS_PER_SECOND: f32 = 750.;
/// 00247570 allocates 100 records shared by all original participants. A
/// single-rider scorer uses that bound; multiplayer must supply a shared pool.
pub const EVENT_CAPACITY: usize = 100;
/// 002de690 identifies mode11 as Freestyle/Freeride. The timed/untimed variant
/// also depends on settings+0; this constant does not recreate that session flow.
pub const FREESTYLE_MODE: u8 = 11;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Notification {
    DisplayUpdated {
        id: usize,
        points: f32,
        count: usize,
    },
    Started {
        id: usize,
        name: &'static str,
    },
    StuntFinished {
        id: usize,
        success: bool,
    },
    ChainFinished {
        success: bool,
        chain_count: usize,
        points: f32,
        sum_points: f32,
        display_points: f32,
        display_count: usize,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Event {
    pub id: usize,
    pub valid: bool,
    pub base_points: f32,
    pub points: f32,
    pub repetition_weight: f32,
    pub duration: f32,
    pub held_seconds: f32,
}
#[derive(Debug, Clone, Serialize)]
pub struct Frame {
    pub active: bool,
    pub holding: bool,
    pub current: Option<Event>,
    pub chain_count: usize,
    pub chain_seconds: f32,
    pub pending_points: f32,
    pub committed_points: f32,
    /// 001eaa70 caches entity+108/count before the chain records are recycled.
    /// 002dd4b0 displays int(points) while active, int(points)*count afterwards.
    pub display_points: f32,
    pub display_count: usize,
    pub last_event_id: Option<usize>,
}
pub struct State {
    active: bool,
    current_open: bool,
    holding: bool,
    // Native inserts each event at the linked-list head. Keep chronological
    // storage but iterate in reverse for its binary32 accumulation order.
    events: Vec<Event>,
    repetition: [f32; 24],
    successful_repetition: [f32; 24],
    chain_seconds: f32,
    pending_points: f32,
    committed_points: f32,
    landing_timer: f32,
    notifications: Vec<Notification>,
    display_points: f32,
    display_count: usize,
}
impl Default for State {
    fn default() -> Self {
        Self {
            active: false,
            current_open: false,
            holding: false,
            events: Vec::new(),
            repetition: [1.; 24],
            successful_repetition: [1.; 24],
            chain_seconds: 0.,
            pending_points: 0.,
            committed_points: 0.,
            landing_timer: 0.,
            notifications: Vec::new(),
            display_points: 0.,
            display_count: 0,
        }
    }
}
fn integer_points(points: f32) -> Result<f32> {
    // Original positive scores use cvt.w.S -> cvt.s.W at a quantum of one.
    if !points.is_finite() || !(0. ..2_147_483_648.).contains(&points) {
        return Err("stunt score exceeds original finite integer range".into());
    }
    Ok((points as i32) as f32)
}
impl State {
    /// Consumers drain once per simulation frame, including batched updates.
    pub fn take_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.notifications)
    }
    /// Rider callbacks from 002da6c0/002d9530/002d8ba0 after animation control
    /// acceptance. This dispatches events; tick remains separate so the caller
    /// can preserve game-rules versus child-entity update order.
    pub fn observe_rider(
        &mut self,
        frame: Option<crate::stunt::Frame>,
        main_duration: Option<f32>,
        wheel_contact: bool,
        dt: f32,
        native_mode: u8,
    ) -> Result<()> {
        if !dt.is_finite() || dt < 0. {
            return Err("invalid stunt rider event time step".into());
        }
        if let Some(frame) = frame {
            if frame.started {
                let duration = main_duration.ok_or("missing scored stunt duration")?;
                if frame.id >= 24 || !duration.is_finite() || duration <= 0. {
                    return Err("invalid scored stunt duration".into());
                }
                // Native accepts very brief contact as the same chain. Beyond
                // 0.07s of the 0.5s timer, a new stunt commits the old chain first.
                if self.landing_timer < 0.43 {
                    self.finish_current(true)?;
                    self.finish_chain(true)?;
                    self.landing_timer = 0.5;
                }
                if !self.active {
                    self.begin_chain();
                }
                self.finish_current(true)?;
                self.start_stunt(frame.id, duration, native_mode)?;
            }
            self.set_holding(frame.holding);
        } else {
            self.finish_current(true)?;
        }
        if self.active && wheel_contact {
            if self.landing_timer > 0. {
                self.landing_timer -= dt;
            } else {
                self.landing_timer = 0.5;
                self.finish_current(true)?;
                self.finish_chain(true)?;
            }
        }
        Ok(())
    }
    /// 002daa90 cancels the current event and chain on wreck/reset. Actor
    /// callback001eaad0 restores repetition to the last successful chain.
    pub fn abort_rider(&mut self) -> Result<()> {
        self.finish_current(false)?;
        self.finish_chain(false)?;
        self.landing_timer = 0.5;
        Ok(())
    }
    pub fn frame(&self) -> Frame {
        Frame {
            active: self.active,
            holding: self.holding,
            current: self.current_open.then(|| *self.events.last().unwrap()),
            chain_count: self.events.len(),
            chain_seconds: self.chain_seconds,
            pending_points: self.pending_points,
            committed_points: self.committed_points,
            display_points: self.display_points,
            display_count: self.display_count,
            last_event_id: self.events.last().map(|event| event.id),
        }
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn repetition_weight(&self, id: usize) -> Option<f32> {
        self.repetition.get(id).copied()
    }
    /// 0024d5c0 after the caller has established game/entity eligibility.
    pub fn begin_chain(&mut self) -> bool {
        if self.active {
            return false;
        }
        self.active = true;
        self.chain_seconds = 0.;
        self.events.clear();
        true
    }
    /// Returns whether an original event record was available. Native animation
    /// acceptance and repeat decay still occur when the shared event pool is empty.
    /// The caller finalizes any previous current event before requesting this one.
    pub fn start_stunt(&mut self, id: usize, duration: f32, native_mode: u8) -> Result<bool> {
        if id >= 24 || !duration.is_finite() || duration <= 0. {
            return Err("invalid stunt score event".into());
        }
        let base_points = BASE_POINTS[id];
        let repetition_weight = self.repetition[id];
        let allocated = self.active && self.events.len() < EVENT_CAPACITY;
        if allocated {
            self.events.push(Event {
                id,
                valid: true,
                base_points,
                points: base_points * repetition_weight,
                repetition_weight,
                duration,
                held_seconds: 0.,
            });
            self.current_open = true;
            self.notifications.push(Notification::Started {
                id,
                name: crate::stunt::NAMES[id],
            });
        }
        // Rider 002da6c0, after event creation. Mode7 bypasses this penalty.
        if native_mode != 7 {
            self.repetition[id] = (self.repetition[id] * 0.85).max(1. / base_points);
        }
        Ok(allocated)
    }
    /// 0024dcb0 can enter held accumulation only with a current event open.
    pub fn set_holding(&mut self, holding: bool) {
        self.holding = holding && self.current_open;
    }
    /// 0024dd00: advances chain elapsed time each tick, and held event time only
    /// while its hold flag is set. Completion clocks remain in the animator.
    pub fn tick(&mut self, dt: f32) -> Result<()> {
        if !dt.is_finite() || dt < 0. {
            return Err("invalid stunt score time step".into());
        }
        if !self.active {
            return Ok(());
        }
        let elapsed = self.chain_seconds + dt;
        if !elapsed.is_finite() {
            return Err("invalid stunt chain elapsed time".into());
        }
        if self.holding {
            let current = self
                .events
                .last_mut()
                .ok_or("held stunt event is missing")?;
            let held = current.held_seconds + dt;
            let points = integer_points(
                (current.base_points + held * HOLD_POINTS_PER_SECOND) * current.repetition_weight,
            )?;
            current.held_seconds = held;
            current.points = points;
        }
        self.chain_seconds = elapsed;
        self.pending_points = integer_points(
            self.events
                .iter()
                .rev()
                .filter(|event| event.valid)
                .fold(0., |sum, event| sum + event.points),
        )?;
        self.display_points = self.pending_points;
        self.display_count = self.events.len();
        if let Some(event) = self.events.last() {
            self.notifications.push(Notification::DisplayUpdated {
                id: event.id,
                points: self.display_points,
                count: self.display_count,
            });
        }
        Ok(())
    }
    /// 0024db30: success keeps/floors current points; failure clears its valid
    /// flag. Either outcome ends both current and held flags without ending chain.
    pub fn finish_current(&mut self, success: bool) -> Result<bool> {
        if !self.active || !self.current_open {
            return Ok(false);
        }
        let event = self
            .events
            .last_mut()
            .ok_or("current stunt event is missing")?;
        if success {
            event.points = integer_points(event.points)?;
        } else {
            event.valid = false;
        }
        self.current_open = false;
        self.holding = false;
        self.notifications.push(Notification::StuntFinished {
            id: event.id,
            success,
        });
        Ok(true)
    }
    /// 0024d690 with the rider caller's multiplier = chain event count. The
    /// final native sum includes all records; a failed chain commits none.
    /// Actor callback001eaad0 checkpoints penalties on success, restores them
    /// on failure, before the built-in scorer recycles the chain.
    pub fn finish_chain(&mut self, success: bool) -> Result<bool> {
        if !self.active {
            return Ok(false);
        }
        if success {
            self.successful_repetition = self.repetition;
        } else {
            self.repetition = self.successful_repetition;
        }
        let sum = self
            .events
            .iter()
            .rev()
            .fold(0., |sum, event| sum + event.points);
        let committed = if success {
            self.committed_points + sum * self.events.len() as f32
        } else {
            self.committed_points
        };
        let points = integer_points(committed)?;
        // Native entity finalize callback precedes the built-in re-sum. Retain
        // its pre-finalization pending score, including a failed current record.
        self.notifications.push(Notification::ChainFinished {
            success,
            chain_count: self.events.len(),
            points: points - self.committed_points,
            sum_points: sum,
            display_points: self.display_points,
            display_count: self.display_count,
        });
        self.committed_points = points;
        self.active = false;
        self.current_open = false;
        self.holding = false;
        self.pending_points = 0.;
        self.events.clear();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rider_frame(id: usize, started: bool, holding: bool) -> crate::stunt::Frame {
        crate::stunt::Frame {
            id,
            name: crate::stunt::NAMES[id],
            holding,
            main_time: 1.1,
            hold_time: 0.,
            started,
        }
    }
    #[test]
    fn rider_contact_grace_keeps_quick_chain_but_splits_after_native_threshold() {
        let mut state = State::default();
        state
            .observe_rider(Some(rider_frame(0, true, false)), Some(1.8), false, 0.01, 7)
            .unwrap();
        state.observe_rider(None, None, true, 0.05, 7).unwrap();
        state
            .observe_rider(Some(rider_frame(8, true, false)), Some(1.6), false, 0.01, 7)
            .unwrap();
        assert_eq!(state.frame().chain_count, 2);
        assert_eq!(state.frame().committed_points, 0.);
        state.observe_rider(None, None, true, 0.03, 7).unwrap();
        // Timer is now 0.42: another accepted stunt first commits the old chain.
        state
            .observe_rider(
                Some(rider_frame(20, true, false)),
                Some(1.8),
                false,
                0.01,
                7,
            )
            .unwrap();
        assert_eq!(state.frame().chain_count, 1);
        assert_eq!(state.frame().committed_points, 3000.);
        assert_eq!(state.frame().current.unwrap().id, 20);
    }
    #[test]
    fn rider_settled_landing_commits_on_tick_after_timer_expires_and_wreck_aborts() {
        let mut state = State::default();
        state
            .observe_rider(Some(rider_frame(0, true, false)), Some(1.8), false, 0.01, 0)
            .unwrap();
        for _ in 0..4 {
            state.observe_rider(None, None, true, 0.125, 0).unwrap();
        }
        assert!(state.frame().active);
        assert_eq!(state.frame().committed_points, 0.);
        state.observe_rider(None, None, true, 0.01, 0).unwrap();
        assert!(!state.frame().active);
        assert_eq!(state.frame().committed_points, 500.);
        state
            .observe_rider(
                Some(rider_frame(20, true, false)),
                Some(1.8),
                false,
                0.01,
                0,
            )
            .unwrap();
        state
            .observe_rider(Some(rider_frame(20, false, true)), None, false, 0.01, 0)
            .unwrap();
        state.tick(0.5).unwrap();
        state.abort_rider().unwrap();
        assert_eq!(state.frame().pending_points, 0.);
        assert_eq!(state.frame().committed_points, 500.);
        assert!(!state.frame().active);
        assert_eq!(state.repetition_weight(20), Some(1.));
    }
    #[test]
    fn failed_chain_restores_last_successful_repetition_checkpoint_and_display_cache() {
        let mut state = State::default();
        state.begin_chain();
        state.start_stunt(0, 1.8, FREESTYLE_MODE).unwrap();
        state.tick(0.01).unwrap();
        state.finish_current(true).unwrap();
        state.finish_chain(true).unwrap();
        assert_eq!(state.repetition_weight(0), Some(0.85));
        state.take_notifications();
        state.begin_chain();
        state.start_stunt(0, 1.8, FREESTYLE_MODE).unwrap();
        state.tick(0.01).unwrap();
        assert!(state.repetition_weight(0).unwrap() < 0.85);
        state.abort_rider().unwrap();
        assert_eq!(state.repetition_weight(0), Some(0.85));
        assert_eq!(state.frame().display_points, 425.);
        assert_eq!(state.frame().display_count, 1);
        assert_eq!(state.frame().committed_points, 500.);
        assert!(matches!(
            state.take_notifications().last().unwrap(),
            Notification::ChainFinished {
                success: false,
                points: 0.,
                display_points: 425.,
                display_count: 1,
                ..
            }
        ));
    }
    #[test]
    fn held_points_chain_multiplier_and_abort_preserve_committed_score() {
        let mut state = State::default();
        state.begin_chain();
        state.start_stunt(0, 1.8, 0).unwrap();
        state.tick(0.5).unwrap();
        assert_eq!(state.frame().pending_points, 500.);
        state.set_holding(true);
        state.tick(0.5).unwrap();
        assert_eq!(state.frame().current.unwrap().held_seconds, 0.5);
        assert_eq!(state.frame().pending_points, 875.);
        state.finish_current(true).unwrap();
        state.start_stunt(8, 1.6, 0).unwrap();
        state.tick(0.25).unwrap();
        assert_eq!(state.frame().pending_points, 1875.);
        state.finish_current(true).unwrap();
        state.finish_chain(true).unwrap();
        assert_eq!(state.frame().committed_points, 3750.);
        state.begin_chain();
        state.start_stunt(23, 2.2, 0).unwrap();
        state.set_holding(true);
        state.tick(1.).unwrap();
        assert_eq!(state.frame().pending_points, 2750.);
        state.finish_current(false).unwrap();
        state.tick(0.).unwrap();
        assert_eq!(state.frame().pending_points, 0.);
        state.finish_chain(false).unwrap();
        assert_eq!(state.frame().committed_points, 3750.);
        assert_eq!(state.repetition_weight(23), Some(1.));
    }
    #[test]
    fn repeat_penalty_uses_pre_decay_weight_and_reaches_native_minimum() {
        let mut state = State::default();
        for turn in 0..100 {
            state.begin_chain();
            let weight = state.repetition_weight(0).unwrap();
            state.start_stunt(0, 1.8, 0).unwrap();
            assert_eq!(state.frame().current.unwrap().points, 500. * weight);
            state.finish_current(true).unwrap();
            state.finish_chain(true).unwrap();
            if turn == 0 {
                assert_eq!(state.repetition_weight(0), Some(0.85));
            }
        }
        assert_eq!(state.repetition_weight(0), Some(1. / 500.));
        assert_eq!(state.repetition_weight(1), Some(1.));
        let before = state.repetition_weight(0);
        state.begin_chain();
        state.start_stunt(0, 1.8, 7).unwrap();
        assert_eq!(state.repetition_weight(0), before);
    }
    #[test]
    fn completion_stops_hold_gain_and_records_recycle_after_chain_finish() {
        let mut state = State::default();
        state.begin_chain();
        for id in 0..EVENT_CAPACITY {
            assert!(state.start_stunt(id % 24, 2., 7).unwrap());
            state.finish_current(true).unwrap();
        }
        assert!(!state.start_stunt(0, 2., 0).unwrap());
        assert_eq!(state.frame().chain_count, 100);
        assert_eq!(state.repetition_weight(0), Some(0.85));
        state.finish_chain(false).unwrap();
        state.begin_chain();
        assert!(state.start_stunt(0, 2., 7).unwrap());
        state.set_holding(true);
        state.tick(0.3).unwrap();
        state.finish_current(true).unwrap();
        let points = state.events()[0].points;
        state.set_holding(true);
        state.tick(0.3).unwrap();
        assert_eq!(state.events()[0].points, points);
        assert!(!state.frame().holding);
    }
    #[test]
    fn invalid_score_inputs_are_rejected_and_no_chain_has_no_pending_score() {
        let mut state = State::default();
        assert!(state.tick(f32::NAN).is_err());
        assert!(state.tick(-0.01).is_err());
        assert!(state.start_stunt(24, 1., 0).is_err());
        assert!(state.start_stunt(0, 0., 0).is_err());
        state.set_holding(true);
        state.tick(1.).unwrap();
        assert!(!state.frame().holding);
        assert_eq!(state.frame().pending_points, 0.);
        assert!(!state.finish_current(true).unwrap());
        assert!(!state.finish_chain(true).unwrap());
    }
}
