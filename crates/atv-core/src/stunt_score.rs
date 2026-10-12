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

/// settings+1878 is zero in constructor0021add0. The original GETONUP
/// cheat toggles it at0033ede0; regular wheelie torque has a separate gate.
pub const DEFAULT_WHEELIE_SCORING: bool = false;
pub const WHEELIE_PITCH_THRESHOLD: f32 = f32::from_bits(0x3edf66f1);

#[derive(Debug, Clone, Copy)]
pub struct WheelieInput {
    /// Native wheel ordering: front left/right, rear left/right (+158..164).
    pub grounded: [bool; 4],
    /// Body Euler pitch from00146850, not joystick or rider pose pitch.
    pub pitch: f32,
    /// Original context+13c must be zero. Caller supplies game-mode eligibility.
    pub context_allows: bool,
    /// The native GETONUP setting. Default is deliberately false.
    pub scoring_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WheelieNotification {
    /// 001e5260 refreshes the original "Wheelie" text on every scoring tick.
    Name,
    /// 001e70e0 adds these binary32 points directly to entity+104.
    Finished { points: f32 },
    /// 001e8a20 requests failure animation but credits no wheelie points.
    Failed,
}

/// Native actor+914/+918/+91c/+920/+7fc/+928. This is scoring eligibility,
/// distinct from the rider controller's physical wheelie-assist state.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct WheelieState {
    pub armed: bool,
    pub eligible: bool,
    pub elapsed: f32,
    pub points: f32,
    pub grace: f32,
    pub active_this_tick: bool,
}
impl Default for WheelieState {
    fn default() -> Self {
        // Exact constructor001e1b10, no inferred initial score or grace.
        Self {
            armed: false,
            eligible: false,
            elapsed: 0.,
            points: 1.,
            grace: 0.5,
            active_this_tick: false,
        }
    }
}
impl WheelieState {
    /// Wheelie section001e6450..001e6600, after the native actor update clears
    /// +928. Strict duration/pitch tests and exact grace comparison are retained.
    pub fn update(&mut self, dt: f32, input: WheelieInput) -> Result<Vec<WheelieNotification>> {
        if !dt.is_finite() || dt < 0. || !input.pitch.is_finite() {
            return Err("invalid wheelie scoring input/time step".into());
        }
        self.active_this_tick = false;
        if input.grounded[0] || input.grounded[1] {
            if !self.eligible {
                // Native does NOT reset elapsed or grace for an unscored attempt.
                self.armed = false;
            } else if self.grace <= 0. {
                return Ok(self.finish().into_iter().collect());
            } else {
                self.grace -= dt;
            }
        } else if self.grace == 0.5
            && input.grounded[2]
            && input.grounded[3]
            && input.pitch > WHEELIE_PITCH_THRESHOLD
            && input.context_allows
        {
            if !self.armed {
                self.points = 1.;
                self.armed = true;
            }
            if self.elapsed > 1. && input.scoring_enabled {
                self.eligible = true;
                self.points += 50. * dt;
                self.active_this_tick = true;
                self.elapsed += dt;
                return Ok(vec![WheelieNotification::Name]);
            }
            self.elapsed += dt;
        }
        Ok(vec![])
    }
    /// 001e70e0: front-contact grace expiry or accepted airborne stunt. The
    /// caller credits the shared stunt scoreboard and invokes native HUD logic.
    pub fn finish(&mut self) -> Option<WheelieNotification> {
        let event = (self.armed && self.eligible).then_some(WheelieNotification::Finished {
            points: self.points,
        });
        self.armed = false;
        self.eligible = false;
        self.elapsed = 0.;
        self.grace = 0.5;
        // +920 and +928 are left intact by the native finish function.
        event
    }
    /// 001e8a20 clears the wheelie attempt without adding its pending points.
    pub fn wreck(&mut self) -> Option<WheelieNotification> {
        let event = self.eligible.then_some(WheelieNotification::Failed);
        self.armed = false;
        self.eligible = false;
        self.elapsed = 0.;
        self.grace = 0.5;
        event
    }
}

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
    /// 001e70e0 bypasses generic stunt integer quantization and combo/repeat
    /// accounting. Merge its award into the same native entity+104 accumulator.
    pub fn credit_wheelie_points(&mut self, points: f32) -> Result<()> {
        let total = self.committed_points + points;
        if !points.is_finite() || points < 0. || !total.is_finite() || total >= 2_147_483_648. {
            return Err("invalid wheelie score award".into());
        }
        self.committed_points = total;
        Ok(())
    }
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
    fn wheelie_input() -> WheelieInput {
        WheelieInput {
            grounded: [false, false, true, true],
            pitch: 0.7,
            context_allows: true,
            scoring_enabled: true,
        }
    }
    #[test]
    fn native_default_does_not_award_wheelies() {
        let mut state = WheelieState::default();
        let input = WheelieInput {
            scoring_enabled: DEFAULT_WHEELIE_SCORING,
            ..wheelie_input()
        };
        for _ in 0..120 {
            assert!(state.update(1. / 60., input).unwrap().is_empty());
        }
        assert!(state.elapsed > 1. && state.armed);
        assert!(!state.eligible && !state.active_this_tick);
        assert_eq!(state.points, 1.);
        assert!(state.finish().is_none());
    }
    #[test]
    fn wheelie_pitch_and_previous_duration_tests_are_strict() {
        let mut state = WheelieState::default();
        let equal = WheelieInput {
            pitch: WHEELIE_PITCH_THRESHOLD,
            ..wheelie_input()
        };
        state.update(2., equal).unwrap();
        assert_eq!(state.elapsed, 0.);
        let above = WheelieInput {
            pitch: f32::from_bits(WHEELIE_PITCH_THRESHOLD.to_bits() + 1),
            ..wheelie_input()
        };
        assert!(state.update(1., above).unwrap().is_empty());
        assert!(state.update(0.125, above).unwrap().is_empty());
        assert_eq!(
            state.update(0.125, above).unwrap(),
            vec![WheelieNotification::Name]
        );
        assert_eq!(state.points, 7.25);
    }
    #[test]
    fn wheelie_grace_expires_on_next_front_contact_and_freezes_airborne() {
        let mut state = WheelieState {
            armed: true,
            eligible: true,
            elapsed: 2.,
            points: 20.25,
            ..WheelieState::default()
        };
        let front = WheelieInput {
            grounded: [true, false, true, true],
            ..wheelie_input()
        };
        assert!(state.update(0.5, front).unwrap().is_empty());
        assert_eq!(state.grace, 0.);
        assert!(
            state
                .update(
                    1.,
                    WheelieInput {
                        grounded: [false; 4],
                        ..front
                    }
                )
                .unwrap()
                .is_empty()
        );
        assert_eq!(state.grace, 0.);
        assert_eq!(
            state.update(0., front).unwrap(),
            vec![WheelieNotification::Finished { points: 20.25 }]
        );
        assert_eq!(state.grace, 0.5);
        assert_eq!(state.elapsed, 0.);
        assert!(!state.armed && !state.eligible);
    }
    #[test]
    fn unscored_front_contact_leaves_accumulated_duration_intact() {
        let mut state = WheelieState::default();
        state.update(0.8, wheelie_input()).unwrap();
        state
            .update(
                1.,
                WheelieInput {
                    grounded: [true; 4],
                    ..wheelie_input()
                },
            )
            .unwrap();
        assert_eq!(state.elapsed, 0.8);
        assert!(!state.armed);
        assert!(state.update(0.3, wheelie_input()).unwrap().is_empty());
        assert_eq!(
            state.update(0.25, wheelie_input()).unwrap(),
            vec![WheelieNotification::Name]
        );
        assert_eq!(state.points, 13.5);
    }
    #[test]
    fn wheelie_wreck_discards_award_without_clearing_native_point_or_activity_fields() {
        let mut state = WheelieState {
            armed: true,
            eligible: true,
            elapsed: 2.,
            points: 25.,
            active_this_tick: true,
            ..WheelieState::default()
        };
        assert_eq!(state.wreck(), Some(WheelieNotification::Failed));
        assert_eq!(state.points, 25.);
        assert!(state.active_this_tick);
        assert_eq!(state.elapsed, 0.);
        assert_eq!(state.grace, 0.5);
        assert!(state.finish().is_none());
    }
    #[test]
    fn wheelie_award_joins_shared_score_without_integer_quantization() {
        let mut state = State::default();
        state.credit_wheelie_points(10.25).unwrap();
        state.credit_wheelie_points(5.125).unwrap();
        assert_eq!(state.frame().committed_points, 15.375);
        assert!(!state.frame().active && state.events().is_empty());
    }
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
