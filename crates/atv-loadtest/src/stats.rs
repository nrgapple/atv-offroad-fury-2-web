//! Counters shared by every rider, and the JSON report over the measured
//! window (after ramp-up).
use crate::Args;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Default)]
pub struct Stats {
    /// Riders whose socket was admitted (Welcome), and those that never were.
    pub connect_ok: AtomicU64,
    pub connect_failed: AtomicU64,
    /// Admitted sockets that closed before the end.
    pub disconnects: AtomicU64,
    /// Riders connected now.
    pub live: AtomicU64,
    pub messages_out: AtomicU64,
    pub bytes_out: AtomicU64,
    pub messages_in: AtomicU64,
    pub bytes_in: AtomicU64,
    /// Other riders' poses received, however they came (single, batch,
    /// bundle, frame).
    pub poses_in: AtomicU64,
    pub frames_in: AtomicU64,
    pub batches_in: AtomicU64,
    /// Round trips (ms) during the window.
    rtts: Mutex<Vec<u32>>,
    measuring: AtomicBool,
    /// Failure and close reasons by count.
    reasons: Mutex<BTreeMap<String, u64>>,
    /// What the first room claim said (links, batches, frames).
    pub claim: Mutex<Option<Value>>,
    /// Riders that said FRAMES in their hello.
    pub said_frames: AtomicU64,
}

/// Counter values at one moment.
pub struct Mark {
    at: Instant,
    values: [u64; 9],
}

impl Stats {
    fn mark(&self) -> Mark {
        Mark {
            at: Instant::now(),
            values: [
                &self.messages_out,
                &self.bytes_out,
                &self.messages_in,
                &self.bytes_in,
                &self.poses_in,
                &self.frames_in,
                &self.batches_in,
                &self.disconnects,
                &self.live,
            ]
            .map(|c| c.load(Ordering::Relaxed)),
        }
    }

    pub fn window_start(&self) -> Mark {
        self.measuring.store(true, Ordering::Relaxed);
        self.mark()
    }

    pub fn window_end(&self) -> Mark {
        self.measuring.store(false, Ordering::Relaxed);
        self.mark()
    }

    pub fn rtt(&self, ms: u32) {
        if self.measuring.load(Ordering::Relaxed)
            && let Ok(mut rtts) = self.rtts.lock()
        {
            rtts.push(ms);
        }
    }

    pub fn reason(&self, reason: impl Into<String>) {
        if let Ok(mut reasons) = self.reasons.lock() {
            *reasons.entry(reason.into()).or_default() += 1;
        }
    }

    pub fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }

    pub fn report(&self, args: &Args, first: &Mark, last: &Mark) -> Value {
        let seconds = last.at.duration_since(first.at).as_secs_f64().max(1e-3);
        let delta = |i: usize| last.values[i].saturating_sub(first.values[i]);
        let rate = |i: usize| round(delta(i) as f64 / seconds);
        // Riders connected when the window ended (they leave once it has).
        let live = last.values[8];
        let mut rtts = self.rtts.lock().map(|r| r.clone()).unwrap_or_default();
        rtts.sort_unstable();
        json!({
            "url": args.url,
            "origin": args.origin,
            "riders": args.riders,
            "rooms": if args.rooms == 0 { json!("freeride") } else { json!(args.rooms) },
            "rampSeconds": args.ramp,
            "measuredSeconds": round(seconds),
            "claim": self.claim.lock().ok().and_then(|c| c.clone()),
            "saidFrames": self.said_frames.load(Ordering::Relaxed),
            "connections": {
                "ok": self.connect_ok.load(Ordering::Relaxed),
                "failed": self.connect_failed.load(Ordering::Relaxed),
                "liveAtEnd": live,
                "disconnects": self.disconnects.load(Ordering::Relaxed),
                "disconnectsMeasured": delta(7),
            },
            "out": { "messagesPerSecond": rate(0), "bytesPerSecond": rate(1) },
            "in": {
                "messagesPerSecond": rate(2),
                "bytesPerSecond": rate(3),
                "posesPerSecond": rate(4),
                "framesPerSecond": rate(5),
                "batchesPerSecond": rate(6),
            },
            "posesReceivedPerRiderPerSecond": round(delta(4) as f64 / seconds / live.max(1) as f64),
            "rttMs": {
                "samples": rtts.len(),
                "p50": percentile(&rtts, 0.50),
                "p95": percentile(&rtts, 0.95),
                "p99": percentile(&rtts, 0.99),
                "max": rtts.last(),
            },
            "reasons": *self.reasons.lock().unwrap_or_else(|e| e.into_inner()),
        })
    }
}

fn round(value: f64) -> f64 {
    (value * 100.).round() / 100.
}

/// Nearest-rank percentile of sorted samples.
pub fn percentile(sorted: &[u32], p: f64) -> Option<u32> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, sorted.len()) - 1])
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentiles_take_the_nearest_rank() {
        let samples: Vec<u32> = (1..=100).collect();
        assert_eq!(percentile(&samples, 0.5), Some(50));
        assert_eq!(percentile(&samples, 0.95), Some(95));
        assert_eq!(percentile(&samples, 0.99), Some(99));
        assert_eq!(percentile(&[7], 0.99), Some(7));
        assert_eq!(percentile(&[], 0.5), None);
    }
}
