use crate::transform::{advance, conjugate, dot, lerp, multiply, normalize, slerp};
use crate::{Error, Result, RiderState, Snapshot, sequence_newer};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const MAX_SAMPLES: usize = 32;
/// `sample` keeps the original fixed-delay contract, including its 100 ms
/// extrapolation bound. `present` and `sample_current` use the configured bound.
const LEGACY_EXTRAPOLATION_MS: u32 = 100;
const MAX_EXTRAPOLATION_MS: u32 = 300;
/// A sender clock running this far ahead of receipt is a discontinuity (a
/// stale relayed copy or a clock jump), not motion to smooth.
pub const SENDER_JUMP_MS: u64 = 5000;

// Adaptive playback delay: interval + 2 * jitter + margin, clamped.
const MIN_DELAY_MS: f64 = 45.;
const MAX_DELAY_MS: f64 = 220.;
const DELAY_MARGIN_MS: f64 = 8.;
/// Sender gaps beyond this are idle heartbeats (1 Hz) or pauses, not cadence.
const IDLE_GAP_MS: u64 = 400;
/// Assumed ordinary 10 Hz cadence until two packets have been seen.
const SEED_INTERVAL_MS: f64 = 100.;
/// Cadence slowdowns are risky (extrapolation), so they are adopted faster.
const INTERVAL_RISE: f64 = 0.25;
const INTERVAL_FALL: f64 = 0.08;
const JITTER_RISE: f64 = 0.15;
const JITTER_FALL: f64 = 0.04;
const MAX_LATENESS_MS: f64 = 250.;
/// Lets the low-watermark clock offset follow up to 200 ppm of clock drift.
const CLOCK_CREEP: f64 = 0.0002;
/// Presented delay changes at most 5% of elapsed time (0.95x..1.05x playback).
const DELAY_SLEW: f64 = 0.05;
/// Proximity blend moves at most 2.0 per second (full swing in 0.5 s).
const NEAR_RATE: f64 = 0.002;
/// Final guard on presentation time: playback rate stays within 0.5x..1.5x,
/// so it never runs backwards or jumps.
const PLAY_SLEW: f64 = 0.5;
/// Frame stalls (hidden tab) or timeline errors beyond this resynchronize.
const RESYNC_MS: f64 = 1000.;

// Error-correction offsets absorbing trajectory changes in `present`.
const CORRECTION_TAU_MS: f64 = 110.;
const TELEPORT_FT: f32 = 30.;

// Hermite tangents are trusted only when consistent with the segment chord.
const HERMITE_SLACK_FT: f32 = 0.5;
const HERMITE_FULL_RATIO: f32 = 0.2;
const HERMITE_LINEAR_RATIO: f32 = 0.5;

const IDENTITY: [f32; 4] = [1., 0., 0., 0.];

#[derive(Debug, Clone, Copy)]
pub struct SmoothingConfig {
    /// Fixed delay used by `sample`; `present` adapts its own delay.
    pub delay_ms: u32,
    /// Velocity extrapolation bound for `present`/`sample_current` (max 300).
    pub extrapolation_ms: u32,
    pub stale_ms: u32,
}
impl Default for SmoothingConfig {
    fn default() -> Self {
        Self {
            delay_ms: 80,
            extrapolation_ms: 250,
            stale_ms: 1000,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampledPose {
    pub snapshot: Snapshot,
    pub stale: bool,
    pub extrapolated_ms: u32,
}
struct TimedSnapshot {
    snapshot: Snapshot,
    time_ms: u64,
    received_ms: u64,
}
#[derive(Clone, Copy)]
enum Locate {
    First,
    Segment {
        right: usize,
        alpha: f32,
        continuous: bool,
    },
    Last {
        extra_ms: f64,
    },
}
/// Discrete presentation state; any change forbids blending or corrections.
#[derive(Clone, Copy, PartialEq)]
struct Key {
    rider_state: RiderState,
    crash_serial: u32,
    atv_model: u16,
    rider_model: u16,
    platform: Option<u8>,
}
impl Key {
    fn of(s: &Snapshot) -> Self {
        Self {
            rider_state: s.rider_state,
            crash_serial: s.crash_serial,
            atv_model: s.atv_model,
            rider_model: s.rider_model,
            platform: s.platform.as_ref().map(|p| p.car),
        }
    }
}
/// Allocation-free evaluation of the root poses at one presentation time.
#[derive(Clone, Copy)]
struct Core {
    atv: [f32; 3],
    rider: [f32; 3],
    atv_rotation: [f32; 4],
    rider_rotation: [f32; 4],
    key: Key,
}
impl Core {
    fn of(s: &Snapshot) -> Self {
        Self {
            atv: s.atv.position,
            rider: s.rider.position,
            atv_rotation: s.atv.rotation,
            rider_rotation: s.rider.rotation,
            key: Key::of(s),
        }
    }
}
#[derive(Clone, Copy)]
struct View {
    active: bool,
    now_ms: u64,
    /// Slewed adaptive delay before the proximity blend.
    delay_ms: f64,
    near: f64,
    /// Presented sender-timeline time.
    play_ms: f64,
    version: u64,
    /// Raw (uncorrected) pose shown last frame.
    shown: Core,
    atv_offset: [f32; 3],
    rider_offset: [f32; 3],
    atv_rotation_offset: [f32; 4],
    rider_rotation_offset: [f32; 4],
}
impl View {
    fn reset_offsets(&mut self) {
        self.atv_offset = [0.; 3];
        self.rider_offset = [0.; 3];
        self.atv_rotation_offset = IDENTITY;
        self.rider_rotation_offset = IDENTITY;
    }
}
impl Default for View {
    fn default() -> Self {
        Self {
            active: false,
            now_ms: 0,
            delay_ms: 0.,
            near: 0.,
            play_ms: 0.,
            version: 0,
            shown: Core::of(&Snapshot::default()),
            atv_offset: [0.; 3],
            rider_offset: [0.; 3],
            atv_rotation_offset: IDENTITY,
            rider_rotation_offset: IDENTITY,
        }
    }
}
/// One bounded buffer per remote peer. The low-watermark clock offset removes
/// sender clock origin differences; it does not require synchronized wall clocks.
/// Reset this buffer after reconnect or reusing a participant slot.
///
/// `present` is the renderer path: adaptive delay, Hermite interpolation,
/// bounded extrapolation and decaying error correction. `sample` keeps the
/// original fixed-delay behavior; `sample_current` is the stateless current
/// estimate used for contact physics.
pub struct PoseBuffer {
    config: SmoothingConfig,
    samples: VecDeque<TimedSnapshot>,
    /// Pure low watermark used by `sample`/`sample_current`.
    offset_ms: f64,
    /// Low watermark that creeps upward to follow clock drift; used by `present`.
    clock_ms: f64,
    interval_ms: Option<f64>,
    jitter_ms: f64,
    version: u64,
    view: View,
}
impl Default for PoseBuffer {
    fn default() -> Self {
        Self::new(SmoothingConfig::default())
    }
}
impl PoseBuffer {
    pub fn new(config: SmoothingConfig) -> Self {
        // Bound custom settings so stale peers never extrapolate indefinitely.
        let config = SmoothingConfig {
            delay_ms: config.delay_ms.min(500),
            extrapolation_ms: config.extrapolation_ms.min(MAX_EXTRAPOLATION_MS),
            stale_ms: config.stale_ms.clamp(100, 10_000),
        };
        Self {
            config,
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            offset_ms: 0.,
            clock_ms: 0.,
            interval_ms: None,
            jitter_ms: 0.,
            version: 0,
            view: View::default(),
        }
    }
    pub fn clear(&mut self) {
        self.samples.clear();
        self.offset_ms = 0.;
        self.clock_ms = 0.;
        self.interval_ms = None;
        self.jitter_ms = 0.;
        self.version = self.version.wrapping_add(1);
        self.view = View::default();
    }
    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
    /// Returns false for duplicates/out-of-order sequence or sender time. Invalid
    /// input never mutates the buffer; a sender clock that runs more than
    /// [`SENDER_JUMP_MS`] ahead of receipt restarts the timeline at the new
    /// pose. Receipt time must use a monotonic clock.
    pub fn push(&mut self, snapshot: Snapshot, received_ms: u64) -> Result<bool> {
        snapshot.validate()?;
        let mut gap = None;
        let extended = if let Some(last) = self.samples.back() {
            if snapshot.peer != last.snapshot.peer {
                return Err(Error::InvalidValue);
            }
            if received_ms < last.received_ms {
                return Err(Error::InvalidValue);
            }
            if !sequence_newer(snapshot.sequence, last.snapshot.sequence) {
                return Ok(false);
            }
            let delta = snapshot.time_ms.wrapping_sub(last.snapshot.time_ms);
            if delta == 0 || delta >= 0x8000_0000 {
                return Ok(false);
            }
            if u64::from(delta)
                > received_ms
                    .saturating_sub(last.received_ms)
                    .saturating_add(SENDER_JUMP_MS)
            {
                // The sender's clock ran ahead of receipt by more than five
                // seconds: the previous sample was a stale copy (the relay
                // replays each rider's last stored pose to a rider who reports
                // Ready, which can be seconds old while that rider was paused
                // or throttled) or the sender's clock jumped. Neither may end
                // the session; restart this peer's timeline from the new pose,
                // as a teleport would.
                self.clear();
                return self.push(snapshot, received_ms);
            }
            gap = Some((u64::from(delta), received_ms - last.received_ms));
            last.time_ms + u64::from(delta)
        } else {
            u64::from(snapshot.time_ms)
        };
        let observed_offset = received_ms as f64 - extended as f64;
        if let Some((delta, elapsed)) = gap {
            self.offset_ms = self.offset_ms.min(observed_offset);
            let crept = self.clock_ms + CLOCK_CREEP * elapsed as f64;
            if observed_offset < crept {
                // Earlier lateness was measured against a baseline that was too
                // high; carry that correction into the jitter estimate.
                self.jitter_ms = (self.jitter_ms + (crept - observed_offset).min(MAX_LATENESS_MS))
                    .min(MAX_LATENESS_MS);
                self.clock_ms = observed_offset;
            } else {
                self.clock_ms = crept;
            }
            let lateness = (observed_offset - self.clock_ms).min(MAX_LATENESS_MS);
            let rate = if lateness > self.jitter_ms {
                JITTER_RISE
            } else {
                JITTER_FALL
            };
            self.jitter_ms += (lateness - self.jitter_ms) * rate;
            if delta <= IDLE_GAP_MS {
                let delta = delta as f64;
                self.interval_ms = Some(match self.interval_ms {
                    None => delta,
                    Some(interval) => {
                        let rate = if delta > interval {
                            INTERVAL_RISE
                        } else {
                            INTERVAL_FALL
                        };
                        interval + (delta - interval) * rate
                    }
                });
            }
        } else {
            self.offset_ms = observed_offset;
            self.clock_ms = observed_offset;
        }
        if self.samples.len() == MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(TimedSnapshot {
            snapshot,
            time_ms: extended,
            received_ms,
        });
        self.version = self.version.wrapping_add(1);
        Ok(true)
    }
    pub fn latest(&self) -> Option<(&Snapshot, u64)> {
        self.samples.back().map(|s| (&s.snapshot, s.received_ms))
    }
    /// Delay `present` is converging toward: interval + 2 * jitter + 8 ms,
    /// clamped to 45..=220 ms.
    pub fn target_delay_ms(&self) -> f32 {
        let interval = self.interval_ms.unwrap_or(SEED_INTERVAL_MS);
        (interval + 2. * self.jitter_ms + DELAY_MARGIN_MS).clamp(MIN_DELAY_MS, MAX_DELAY_MS) as f32
    }
    /// Effective delay of the last `present` call (after the proximity blend),
    /// or the target before the first call.
    pub fn presented_delay_ms(&self) -> f32 {
        if self.view.active {
            (self.view.delay_ms * (1. - self.view.near)) as f32
        } else {
            self.target_delay_ms()
        }
    }
    /// Magnitude in feet of the ATV error-correction offset still being blended out.
    pub fn correction_ft(&self) -> f32 {
        dot(self.view.atv_offset, self.view.atv_offset).sqrt()
    }
    /// Stateless current estimate (no delay), extrapolated up to the configured
    /// bound. Suitable for contact physics; renderers should call `present`.
    pub fn sample_current(&self, now_ms: u64) -> Option<SampledPose> {
        let target = now_ms as f64 - self.offset_ms;
        self.sample_at(now_ms, target, self.config.extrapolation_ms)
    }
    /// Original fixed-delay sampler (`SmoothingConfig::delay_ms`, at most
    /// 100 ms extrapolation).
    pub fn sample(&self, now_ms: u64) -> Option<SampledPose> {
        let target = now_ms as f64 - self.offset_ms - f64::from(self.config.delay_ms);
        let limit = self.config.extrapolation_ms.min(LEGACY_EXTRAPOLATION_MS);
        self.sample_at(now_ms, target, limit)
    }
    /// Per-frame renderer sample. `near` in 0..=1 blends the playback delay
    /// toward zero (current, extrapolated estimate) when the remote rider is
    /// close to the local player, so visuals line up with contact physics.
    /// Delay and proximity changes are slew-limited, so presentation time never
    /// runs backwards or jumps; trajectory corrections from late packets decay
    /// over ~110 ms instead of snapping. Call once per rendered frame.
    pub fn present(&mut self, now_ms: u64, near: f32) -> Option<SampledPose> {
        if self.samples.is_empty() {
            return None;
        }
        let near = if near.is_finite() {
            f64::from(near.clamp(0., 1.))
        } else {
            0.
        };
        let target_delay = f64::from(self.target_delay_ms());
        let limit = self.config.extrapolation_ms;
        let mut v = self.view;
        let fresh = !v.active;
        let dt = if fresh {
            0.
        } else {
            now_ms.saturating_sub(v.now_ms) as f64
        };
        if fresh {
            v.delay_ms = target_delay;
            v.near = near;
            v.now_ms = now_ms;
        } else {
            let step = DELAY_SLEW * dt;
            v.delay_ms += (target_delay - v.delay_ms).clamp(-step, step);
            let step = NEAR_RATE * dt;
            v.near += (near - v.near).clamp(-step, step);
            v.now_ms = v.now_ms.max(now_ms);
        }
        let desired = now_ms as f64 - self.clock_ms - v.delay_ms * (1. - v.near);
        let previous_play = v.play_ms;
        let expected = v.play_ms + dt;
        let resync = fresh || dt > RESYNC_MS || (desired - expected).abs() > RESYNC_MS;
        if resync {
            v.play_ms = desired;
            v.reset_offsets();
        } else {
            let step = PLAY_SLEW * dt;
            v.play_ms = expected + (desired - expected).clamp(-step, step);
            if v.version != self.version {
                // A new packet may have changed the trajectory under the frame
                // already shown; absorb that jump into the visual offsets.
                let before = self.core(self.locate(previous_play, limit));
                let last = v.shown;
                if before.key == last.key
                    && before.key.platform.is_none()
                    && distance_squared(before.atv, last.atv) <= TELEPORT_FT * TELEPORT_FT
                    && distance_squared(before.rider, last.rider) <= TELEPORT_FT * TELEPORT_FT
                {
                    for i in 0..3 {
                        v.atv_offset[i] += last.atv[i] - before.atv[i];
                        v.rider_offset[i] += last.rider[i] - before.rider[i];
                    }
                    v.atv_rotation_offset = absorb(
                        v.atv_rotation_offset,
                        last.atv_rotation,
                        before.atv_rotation,
                    );
                    v.rider_rotation_offset = absorb(
                        v.rider_rotation_offset,
                        last.rider_rotation,
                        before.rider_rotation,
                    );
                } else {
                    v.reset_offsets();
                }
            }
            let keep = (-dt / CORRECTION_TAU_MS).exp() as f32;
            v.atv_offset = v.atv_offset.map(|o| o * keep);
            v.rider_offset = v.rider_offset.map(|o| o * keep);
            v.atv_rotation_offset = slerp(IDENTITY, v.atv_rotation_offset, keep);
            v.rider_rotation_offset = slerp(IDENTITY, v.rider_rotation_offset, keep);
        }
        let locate = self.locate(v.play_ms, limit);
        let core = self.core(locate);
        if core.key != v.shown.key
            || core.key.platform.is_some()
            || distance_squared(core.atv, v.shown.atv) > TELEPORT_FT * TELEPORT_FT
            || distance_squared(core.rider, v.shown.rider) > TELEPORT_FT * TELEPORT_FT
            || dot(v.atv_offset, v.atv_offset) > TELEPORT_FT * TELEPORT_FT
            || dot(v.rider_offset, v.rider_offset) > TELEPORT_FT * TELEPORT_FT
        {
            v.reset_offsets();
        }
        settle(&mut v.atv_offset, &mut v.atv_rotation_offset);
        settle(&mut v.rider_offset, &mut v.rider_rotation_offset);
        v.shown = core;
        v.version = self.version;
        v.active = true;
        self.view = v;
        let mut snapshot = self.build(locate, &core);
        for i in 0..3 {
            snapshot.atv.position[i] += v.atv_offset[i];
            snapshot.rider.position[i] += v.rider_offset[i];
        }
        if v.atv_rotation_offset != IDENTITY {
            snapshot.atv.rotation = normalize(multiply(v.atv_rotation_offset, core.atv_rotation));
        }
        if v.rider_rotation_offset != IDENTITY {
            snapshot.rider.rotation =
                normalize(multiply(v.rider_rotation_offset, core.rider_rotation));
        }
        Some(SampledPose {
            snapshot,
            stale: self.stale(now_ms),
            extrapolated_ms: extrapolated(locate),
        })
    }
    fn stale(&self, now_ms: u64) -> bool {
        let Some(last) = self.samples.back() else {
            return true;
        };
        let freshness = if last.snapshot.animation.as_ref().is_some_and(|a| a.paused) {
            15_000
        } else if last.snapshot.animation.is_some()
            && last.snapshot.velocity.iter().all(|v| v.abs() < 0.5)
        {
            2500
        } else {
            u64::from(self.config.stale_ms)
        };
        now_ms.saturating_sub(last.received_ms) >= freshness
    }
    fn sample_at(&self, now_ms: u64, target: f64, limit_ms: u32) -> Option<SampledPose> {
        if self.samples.is_empty() {
            return None;
        }
        let locate = self.locate(target, limit_ms);
        let core = self.core(locate);
        Some(SampledPose {
            snapshot: self.build(locate, &core),
            stale: self.stale(now_ms),
            extrapolated_ms: extrapolated(locate),
        })
    }
    /// Callers ensure the buffer is not empty.
    fn locate(&self, target: f64, limit_ms: u32) -> Locate {
        let index = self
            .samples
            .partition_point(|s| (s.time_ms as f64) < target);
        if index == 0 {
            return Locate::First;
        }
        if index == self.samples.len() {
            let last = self.samples[index - 1].time_ms as f64;
            return Locate::Last {
                extra_ms: (target - last).clamp(0., f64::from(limit_ms)),
            };
        }
        let left = &self.samples[index - 1];
        let right = &self.samples[index];
        let alpha = ((target - left.time_ms as f64) / (right.time_ms - left.time_ms) as f64) as f32;
        // Discontinuous gameplay state/model transitions must never blend
        // attached rider poses into separated wreck poses or a new model.
        let continuous = Key::of(&left.snapshot) == Key::of(&right.snapshot)
            && distance_squared(left.snapshot.atv.position, right.snapshot.atv.position)
                <= 100. * 100.;
        Locate::Segment {
            right: index,
            alpha,
            continuous,
        }
    }
    fn core(&self, locate: Locate) -> Core {
        match locate {
            Locate::First => Core::of(&self.samples[0].snapshot),
            Locate::Segment {
                right,
                alpha,
                continuous,
            } => {
                let left = &self.samples[right - 1];
                let right_sample = &self.samples[right];
                let (a, b) = (&left.snapshot, &right_sample.snapshot);
                if !continuous {
                    return Core::of(if alpha < 1. { a } else { b });
                }
                let mut core = Core::of(if alpha < 1. { a } else { b });
                if a.platform.is_some() {
                    // Platform poses are re-based onto the moving car with the
                    // blended platform transform; keep them linear.
                    core.atv = lerp(a.atv.position, b.atv.position, alpha);
                    core.rider = lerp(a.rider.position, b.rider.position, alpha);
                } else {
                    let dt = (right_sample.time_ms - left.time_ms) as f32 / 1000.;
                    core.atv = hermite(
                        a.atv.position,
                        a.velocity,
                        b.atv.position,
                        b.velocity,
                        dt,
                        alpha,
                    );
                    core.rider = hermite(
                        a.rider.position,
                        a.rider_velocity,
                        b.rider.position,
                        b.rider_velocity,
                        dt,
                        alpha,
                    );
                }
                core.atv_rotation = slerp(a.atv.rotation, b.atv.rotation, alpha);
                core.rider_rotation = slerp(a.rider.rotation, b.rider.rotation, alpha);
                core
            }
            Locate::Last { extra_ms } => {
                let s = &self.samples[self.samples.len() - 1].snapshot;
                let mut core = Core::of(s);
                if s.platform.is_none() {
                    let dt = extra_ms as f32 / 1000.;
                    for i in 0..3 {
                        core.atv[i] += s.velocity[i] * dt;
                        core.rider[i] += s.rider_velocity[i] * dt;
                    }
                    core.atv_rotation = advance(s.atv.rotation, s.angular_velocity, dt);
                }
                core
            }
        }
    }
    /// The single clone per call; root poses come from `core`.
    fn build(&self, locate: Locate, core: &Core) -> Snapshot {
        let mut snapshot = match locate {
            Locate::First => return self.samples[0].snapshot.clone(),
            Locate::Segment {
                right,
                alpha,
                continuous,
            } => {
                let a = &self.samples[right - 1].snapshot;
                let b = &self.samples[right].snapshot;
                if !continuous {
                    return if alpha < 1. { a.clone() } else { b.clone() };
                }
                blend(a, b, alpha)
            }
            Locate::Last { .. } => self.samples[self.samples.len() - 1].snapshot.clone(),
        };
        snapshot.atv.position = core.atv;
        snapshot.rider.position = core.rider;
        snapshot.atv.rotation = core.atv_rotation;
        snapshot.rider.rotation = core.rider_rotation;
        snapshot
    }
}
fn extrapolated(locate: Locate) -> u32 {
    match locate {
        Locate::Last { extra_ms } => extra_ms as u32,
        _ => 0,
    }
}
/// New rotation offset that keeps `offset * last` on screen while the raw
/// rotation becomes `now`.
fn absorb(offset: [f32; 4], last: [f32; 4], now: [f32; 4]) -> [f32; 4] {
    normalize(multiply(offset, multiply(last, conjugate(normalize(now)))))
}
/// Drops imperceptible residual offsets so presentation returns to raw output.
fn settle(position: &mut [f32; 3], rotation: &mut [f32; 4]) {
    if dot(*position, *position) < 1e-8 {
        *position = [0.; 3];
    }
    if 1. - rotation[0].abs() < 1e-7 {
        *rotation = IDENTITY;
    }
}
fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| (a - b) * (a - b)).sum()
}
/// Cubic Hermite position with tangents `velocity * dt`, faded back to linear
/// when the endpoint velocities disagree with the chord (missing or noisy
/// velocity). Constant-acceleration (ballistic) segments are reproduced exactly.
fn hermite(p0: [f32; 3], v0: [f32; 3], p1: [f32; 3], v1: [f32; 3], dt: f32, t: f32) -> [f32; 3] {
    let linear = lerp(p0, p1, t);
    let chord: [f32; 3] = std::array::from_fn(|i| p1[i] - p0[i]);
    let m0 = v0.map(|v| v * dt);
    let m1 = v1.map(|v| v * dt);
    let mismatch: [f32; 3] = std::array::from_fn(|i| (m0[i] + m1[i]) * 0.5 - chord[i]);
    let ratio = dot(mismatch, mismatch).sqrt() / (dot(chord, chord).sqrt() + HERMITE_SLACK_FT);
    let weight = ((HERMITE_LINEAR_RATIO - ratio) / (HERMITE_LINEAR_RATIO - HERMITE_FULL_RATIO))
        .clamp(0., 1.);
    if weight <= 0. {
        return linear;
    }
    let t2 = t * t;
    let t3 = t2 * t;
    let h10 = t3 - 2. * t2 + t;
    let h11 = t3 - t2;
    // hermite - linear == h10 * (m0 - chord) + h11 * (m1 - chord)
    std::array::from_fn(|i| {
        linear[i] + weight * (h10 * (m0[i] - chord[i]) + h11 * (m1[i] - chord[i]))
    })
}
fn blend(a: &Snapshot, b: &Snapshot, t: f32) -> Snapshot {
    let mut out = if t < 1. { a.clone() } else { b.clone() };
    out.atv = a.atv.blend(b.atv, t);
    out.rider = a.rider.blend(b.rider, t);
    out.velocity = lerp(a.velocity, b.velocity, t);
    out.angular_velocity = lerp(a.angular_velocity, b.angular_velocity, t);
    out.rider_velocity = lerp(a.rider_velocity, b.rider_velocity, t);
    if a.atv_nodes.len() == b.atv_nodes.len() {
        for ((out, a), b) in out.atv_nodes.iter_mut().zip(&a.atv_nodes).zip(&b.atv_nodes) {
            *out = a.blend(*b, t);
        }
    }
    if a.rider_nodes.len() == b.rider_nodes.len() {
        for ((out, a), b) in out
            .rider_nodes
            .iter_mut()
            .zip(&a.rider_nodes)
            .zip(&b.rider_nodes)
        {
            *out = a.blend(*b, t);
        }
    }
    if let (Some(a), Some(b), Some(out)) = (&a.platform, &b.platform, &mut out.platform) {
        out.transform = a.transform.blend(b.transform, t);
    }
    if let (Some(a), Some(b), Some(out)) = (&a.animation, &b.animation, &mut out.animation) {
        out.steering = a.steering + (b.steering - a.steering) * t;
        for i in 0..4 {
            let arc = (b.wheels[i] - a.wheels[i] + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            out.wheels[i] = a.wheels[i] + arc * t;
            out.compression[i] = a.compression[i] + (b.compression[i] - a.compression[i]) * t;
        }
        if a.clips == b.clips && a.count == b.count {
            out.phase = a.phase + (b.phase - a.phase) * t;
            out.first_weight = a.first_weight + (b.first_weight - a.first_weight) * t;
            out.pair_weight = a.pair_weight + (b.pair_weight - a.pair_weight) * t;
            if a.look_phase >= 0. && b.look_phase >= 0. {
                out.look_phase = a.look_phase + (b.look_phase - a.look_phase) * t;
            }
        }
    }
    out.rpm = a.rpm + (b.rpm - a.rpm) * t;
    out.throttle = a.throttle + (b.throttle - a.throttle) * t;
    out.speed = a.speed + (b.speed - a.speed) * t;
    out
}
