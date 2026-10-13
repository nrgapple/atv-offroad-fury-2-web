use crate::transform::{advance, lerp};
use crate::{Error, Result, Snapshot, sequence_newer};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const MAX_SAMPLES: usize = 32;
#[derive(Debug, Clone, Copy)]
pub struct SmoothingConfig {
    pub delay_ms: u32,
    pub extrapolation_ms: u32,
    pub stale_ms: u32,
}
impl Default for SmoothingConfig {
    fn default() -> Self {
        Self {
            delay_ms: 80,
            extrapolation_ms: 100,
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
/// One bounded buffer per remote peer. The low-watermark clock offset removes
/// sender clock origin differences; it does not require synchronized wall clocks.
/// Reset this buffer after reconnect or reusing a participant slot.
pub struct PoseBuffer {
    config: SmoothingConfig,
    samples: VecDeque<TimedSnapshot>,
    offset_ms: f64,
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
            extrapolation_ms: config.extrapolation_ms.min(100),
            stale_ms: config.stale_ms.clamp(100, 10_000),
        };
        Self {
            config,
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            offset_ms: 0.,
        }
    }
    pub fn clear(&mut self) {
        self.samples.clear();
        self.offset_ms = 0.;
    }
    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
    /// Returns false for duplicates/out-of-order sequence or sender time. Invalid
    /// input never mutates the buffer. Receipt time must use a monotonic clock.
    pub fn push(&mut self, snapshot: Snapshot, received_ms: u64) -> Result<bool> {
        snapshot.validate()?;
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
                    .saturating_add(5000)
            {
                return Err(Error::InvalidValue);
            }
            last.time_ms + u64::from(delta)
        } else {
            u64::from(snapshot.time_ms)
        };
        let observed_offset = received_ms as f64 - extended as f64;
        if self.samples.is_empty() {
            self.offset_ms = observed_offset;
        } else {
            self.offset_ms = self.offset_ms.min(observed_offset);
        }
        if self.samples.len() == MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(TimedSnapshot {
            snapshot,
            time_ms: extended,
            received_ms,
        });
        Ok(true)
    }
    /// No allocations beyond the returned bounded pose. Pauses and packet loss
    /// freeze after the extrapolation limit and mark the peer stale for UI/audio.
    pub fn latest(&self) -> Option<(&Snapshot, u64)> {
        self.samples.back().map(|s| (&s.snapshot, s.received_ms))
    }
    pub fn sample_current(&self, now_ms: u64) -> Option<SampledPose> {
        self.sample_delay(now_ms, 0)
    }
    pub fn sample(&self, now_ms: u64) -> Option<SampledPose> {
        self.sample_delay(now_ms, self.config.delay_ms)
    }
    fn sample_delay(&self, now_ms: u64, delay_ms: u32) -> Option<SampledPose> {
        let first = self.samples.front()?;
        let last = self.samples.back()?;
        let stale = now_ms.saturating_sub(last.received_ms) >= u64::from(self.config.stale_ms);
        let target = now_ms as f64 - self.offset_ms - f64::from(delay_ms);
        if target <= first.time_ms as f64 {
            return Some(SampledPose {
                snapshot: first.snapshot.clone(),
                stale,
                extrapolated_ms: 0,
            });
        }
        for index in 1..self.samples.len() {
            let left = &self.samples[index - 1];
            let right = &self.samples[index];
            if target <= right.time_ms as f64 {
                let alpha =
                    ((target - left.time_ms as f64) / (right.time_ms - left.time_ms) as f64) as f32;
                // Discontinuous gameplay state/model transitions must never blend
                // attached rider poses into separated wreck poses or a new model.
                let snapshot = if left.snapshot.atv_model != right.snapshot.atv_model
                    || left.snapshot.rider_model != right.snapshot.rider_model
                    || left.snapshot.rider_state != right.snapshot.rider_state
                    || left.snapshot.crash_serial != right.snapshot.crash_serial
                    || left.snapshot.platform.as_ref().map(|p| p.car)
                        != right.snapshot.platform.as_ref().map(|p| p.car)
                    || distance_squared(left.snapshot.atv.position, right.snapshot.atv.position)
                        > 100. * 100.
                {
                    if alpha < 1. {
                        left.snapshot.clone()
                    } else {
                        right.snapshot.clone()
                    }
                } else {
                    blend(&left.snapshot, &right.snapshot, alpha)
                };
                return Some(SampledPose {
                    snapshot,
                    stale,
                    extrapolated_ms: 0,
                });
            }
        }
        let extra_ms =
            (target - last.time_ms as f64).clamp(0., f64::from(self.config.extrapolation_ms));
        let mut snapshot = last.snapshot.clone();
        let dt = extra_ms as f32 / 1000.;
        for i in 0..3 {
            if snapshot.platform.is_some() {
                break;
            }
            snapshot.atv.position[i] += snapshot.velocity[i] * dt;
            snapshot.rider.position[i] += snapshot.rider_velocity[i] * dt;
        }
        if snapshot.platform.is_none() {
            snapshot.atv.rotation = advance(snapshot.atv.rotation, snapshot.angular_velocity, dt);
        }
        Some(SampledPose {
            snapshot,
            stale,
            extrapolated_ms: extra_ms as u32,
        })
    }
}
fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| (a - b) * (a - b)).sum()
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
    out.rpm = a.rpm + (b.rpm - a.rpm) * t;
    out.throttle = a.throttle + (b.throttle - a.throttle) * t;
    out.speed = a.speed + (b.speed - a.speed) * t;
    out
}
