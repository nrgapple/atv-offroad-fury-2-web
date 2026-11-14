//! Counts only: live gauges and counters for `/healthz` and `/metrics`, and
//! the shared Free Ride rooms' daily usage the relay published
//! (research/MULTIPLAYER.md "Relay accounting", "Relay history").

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::engine::{Counters, POSE_KINDS};

pub const DAY_MS: u64 = 86_400_000;
/// Days of totals kept.
pub const HISTORY_DAYS: u32 = 30;
/// A room's report counts until this old. Live rooms report every few
/// seconds, so a vanished room drops out on its own.
pub const FRESH_MS: u64 = 90_000;

/// Process-wide gauges and counters, updated without locks.
#[derive(Default)]
pub struct Metrics {
    pub connections: AtomicU64,
    pub rooms: AtomicU64,
    pub riders: AtomicU64,
    pub rooms_created: AtomicU64,
    pub messages: AtomicU64,
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub poses_in: AtomicU64,
    pub frames: AtomicU64,
    pub frame_poses: AtomicU64,
    pub single_poses: AtomicU64,
    /// Stale poses a slow connection's queue dropped for newer ones.
    pub poses_dropped: AtomicU64,
    /// Connections closed because their queue stayed full.
    pub slow_closes: AtomicU64,
    pub refused_origin: AtomicU64,
    pub refused_admission: AtomicU64,
}

impl Metrics {
    pub fn add(&self, c: &Counters) {
        let add = |a: &AtomicU64, n: u64| {
            a.fetch_add(n, Ordering::Relaxed);
        };
        add(&self.messages, c.messages);
        add(&self.bytes_in, c.bytes_in);
        add(&self.poses_in, c.poses.iter().sum());
        add(&self.frames, c.frames);
        add(&self.frame_poses, c.frame_poses);
        add(&self.single_poses, c.single_poses);
    }

    /// Prometheus text exposition.
    pub fn prometheus(&self, uptime_seconds: u64) -> String {
        let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let mut out = String::new();
        let mut metric = |name: &str, kind: &str, help: &str, value: u64| {
            out.push_str(&format!(
                "# HELP fury_{name} {help}\n# TYPE fury_{name} {kind}\nfury_{name} {value}\n"
            ));
        };
        metric(
            "uptime_seconds",
            "gauge",
            "Seconds since start.",
            uptime_seconds,
        );
        metric(
            "connections",
            "gauge",
            "Open room sockets.",
            get(&self.connections),
        );
        metric("rooms", "gauge", "Live rooms.", get(&self.rooms));
        metric(
            "riders",
            "gauge",
            "Riders in rooms, held slots included.",
            get(&self.riders),
        );
        let counters = [
            ("rooms_created_total", "Rooms opened.", &self.rooms_created),
            (
                "messages_total",
                "Messages received from riders.",
                &self.messages,
            ),
            (
                "received_bytes_total",
                "Bytes received from riders.",
                &self.bytes_in,
            ),
            ("sent_bytes_total", "Bytes sent to riders.", &self.bytes_out),
            (
                "poses_total",
                "Poses accepted from riders on a course.",
                &self.poses_in,
            ),
            ("frames_total", "Frames sent.", &self.frames),
            (
                "frame_poses_total",
                "Poses carried in frames.",
                &self.frame_poses,
            ),
            (
                "single_poses_total",
                "Poses sent on their own.",
                &self.single_poses,
            ),
            (
                "dropped_poses_total",
                "Stale poses dropped for slow connections.",
                &self.poses_dropped,
            ),
            (
                "slow_closes_total",
                "Connections closed for a stuck send queue.",
                &self.slow_closes,
            ),
            (
                "refused_origin_total",
                "Requests refused for their Origin.",
                &self.refused_origin,
            ),
            (
                "refused_admission_total",
                "Requests refused by per-address limits.",
                &self.refused_admission,
            ),
        ];
        for (name, help, value) in counters {
            metric(name, "counter", help, get(value));
        }
        out
    }
}

/// A shared room's day: messages and poses by kind, added up from its
/// reports, and its riders as of the last report.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct RoomDay {
    pub day: u32,
    pub messages: u64,
    #[serde(default)]
    pub poses: [u64; 7],
    #[serde(default)]
    pub batches: u64,
    #[serde(default)]
    pub frames: u64,
    #[serde(default)]
    pub now: BTreeMap<String, u64>,
    #[serde(default)]
    pub at_ms: u64,
}

/// The shared rooms' day, and their riders at the report with the most.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct DayTotal {
    pub messages: u64,
    pub poses: [u64; 7],
    #[serde(default)]
    pub bundles: u64,
    #[serde(default)]
    pub batches: u64,
    #[serde(default)]
    pub frames: u64,
    pub busiest: BTreeMap<String, u64>,
}

/// Each shared room's counts for the current day and every day's totals for
/// [`HISTORY_DAYS`]; saved in the state directory.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Usage {
    pub rooms: BTreeMap<String, RoomDay>,
    pub days: BTreeMap<u32, DayTotal>,
}

impl Usage {
    /// Adds a shared room's report (its counts since its last one, and its
    /// riders now) at `now_ms`.
    pub fn report(
        &mut self,
        room: &str,
        counters: &Counters,
        riders: BTreeMap<String, u64>,
        now_ms: u64,
    ) {
        let day = (now_ms / DAY_MS) as u32;
        self.days
            .retain(|d, _| d.saturating_add(HISTORY_DAYS) > day);
        let total = self.days.entry(day).or_default();
        total.messages += counters.messages;
        total.batches += counters.batches;
        total.frames += counters.frames;
        for (sum, n) in total.poses.iter_mut().zip(counters.poses) {
            *sum += n;
        }
        self.rooms.retain(|_, d| d.day >= day);
        let entry = self
            .rooms
            .entry(room.to_string())
            .or_insert_with(|| RoomDay {
                day,
                ..RoomDay::default()
            });
        entry.messages += counters.messages;
        entry.batches += counters.batches;
        entry.frames += counters.frames;
        for (sum, n) in entry.poses.iter_mut().zip(counters.poses) {
            *sum += n;
        }
        entry.now = riders;
        entry.at_ms = now_ms;
        let live = self.live(day, now_ms);
        let count = |c: &BTreeMap<String, u64>| c.get("riders").copied().unwrap_or(0);
        let total = self.days.entry(day).or_default();
        if count(&live) > 0 && count(&live) >= count(&total.busiest) {
            total.busiest = live;
        }
    }

    /// The live shared rooms' rider counts, added up.
    pub fn live(&self, day: u32, now_ms: u64) -> BTreeMap<String, u64> {
        let mut riding = BTreeMap::new();
        for d in self.rooms.values() {
            if d.day == day && now_ms.saturating_sub(d.at_ms) < FRESH_MS {
                for (k, v) in &d.now {
                    *riding.entry(k.clone()).or_insert(0) += v;
                }
            }
        }
        riding
    }

    /// `/multiplayer/freeride/riders`: riders in shared rooms now, and
    /// today's counts.
    pub fn riders_json(&self, now_ms: u64) -> serde_json::Value {
        let day = (now_ms / DAY_MS) as u32;
        let live = self.live(day, now_ms);
        let today = self.days.get(&day).cloned().unwrap_or_default();
        serde_json::json!({
            "riders": live.get("riders").copied().unwrap_or(0) + live.get("held").copied().unwrap_or(0),
            "today": {
                "messages": today.messages,
                "poses": pose_counts(today.poses),
                "bundles": 0,
                "batches": today.batches,
                "frames": today.frames,
                "now": live,
            },
        })
    }

    /// `/multiplayer/freeride/days`: each recent day, newest first.
    pub fn days_json(&self) -> serde_json::Value {
        let days: Vec<_> = self
            .days
            .iter()
            .rev()
            .map(|(day, d)| {
                serde_json::json!({
                    "day": day, "messages": d.messages, "poses": pose_counts(d.poses),
                    "bundles": d.bundles, "batches": d.batches, "frames": d.frames,
                    "busiest": d.busiest,
                })
            })
            .collect();
        serde_json::json!({ "days": days })
    }
}

fn pose_counts(poses: [u64; 7]) -> BTreeMap<&'static str, u64> {
    POSE_KINDS.iter().copied().zip(poses).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_adds_up_reports_and_keeps_the_busiest_moment() {
        let mut usage = Usage::default();
        let counters = |messages, poses| Counters {
            messages,
            poses: [0, 0, 0, poses, 0, 0, 0],
            frames: 2,
            ..Counters::default()
        };
        let riders = |n: u64| BTreeMap::from([("riders".to_string(), n)]);
        let day = 20_000 * DAY_MS;
        usage.report("FREERD01", &counters(100, 10), riders(4), day);
        usage.report("FREERD02", &counters(20, 0), riders(3), day + 1000);
        usage.report("FREERD01", &counters(50, 5), riders(1), day + 31_000);
        let total = &usage.days[&20_000];
        assert_eq!((total.messages, total.poses[3], total.frames), (170, 15, 6));
        assert_eq!(total.busiest["riders"], 7);
        assert_eq!(usage.riders_json(day + 31_000)["riders"], 4);
        // A room that stopped reporting no longer counts.
        assert_eq!(usage.riders_json(day + 1000 + FRESH_MS)["riders"], 1);
        // A new day starts fresh; old days age out.
        usage.report("FREERD01", &counters(1, 0), riders(1), day + DAY_MS);
        assert_eq!(usage.rooms.len(), 1);
        assert_eq!(usage.days_json()["days"][0]["messages"], 1);
        assert_eq!(usage.days_json()["days"][1]["poses"]["senderNoLinks"], 15);
        usage.report(
            "FREERD01",
            &counters(1, 0),
            riders(1),
            day + DAY_MS * HISTORY_DAYS as u64,
        );
        assert!(!usage.days.contains_key(&20_000));
        let saved: Usage = serde_json::from_str(&serde_json::to_string(&usage).unwrap()).unwrap();
        assert_eq!(saved, usage);
    }

    #[test]
    fn metrics_render_as_prometheus_text() {
        let m = Metrics::default();
        m.connections.store(3, Ordering::Relaxed);
        m.add(&Counters {
            messages: 7,
            frames: 2,
            ..Counters::default()
        });
        let text = m.prometheus(9);
        assert!(text.contains("# TYPE fury_connections gauge\nfury_connections 3\n"));
        assert!(text.contains("fury_messages_total 7\n"));
        assert!(text.contains("fury_frames_total 2\n"));
        assert!(text.contains("fury_uptime_seconds 9\n"));
    }
}
