//! Room actors and the room registry. Each room runs in one task that owns
//! its [`Room`] engine, fed by a channel: no locks on the message path. The
//! registry maps codes to running rooms and is touched only to open, find,
//! claim or end a room.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use atv_net::RoomCode;
use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use crate::engine::chat::ReportContext;
use crate::engine::{ConnId, Counters, Entropy, Out, Refused, Room, SavedRoom};
use crate::interest::TICK_MS;
use crate::outbox::{Outbox, Pushed};
use crate::stats::Metrics;
use crate::{Shared, now_ms};

/// Messages queued for one room before senders wait.
const ROOM_QUEUE: usize = 4096;
const HOUSEKEEP_MS: u64 = 1000;
const REPORT_MS: u64 = 5000;

pub struct OsEntropy;
impl Entropy for OsEntropy {
    fn fill(&mut self, out: &mut [u8]) {
        getrandom::fill(out).expect("system random source");
    }
}

pub enum RoomMsg {
    Admit {
        conn: ConnId,
        key: Option<String>,
        /// False for a client at an uncounted address.
        counted: bool,
        /// The socket's verified player id (`Players::verify` of `?rider=`).
        pid: Option<String>,
        outbox: Arc<Outbox>,
        reply: oneshot::Sender<Result<u8, Refused>>,
    },
    Message {
        conn: ConnId,
        bytes: Bytes,
    },
    Closed {
        conn: ConnId,
        code: u16,
        reason: String,
    },
    Claim {
        reply: oneshot::Sender<Option<String>>,
    },
    /// A Quick Play seat (research/QUICK_PLAY.md).
    QuickClaim {
        any_phase: bool,
        reply: oneshot::Sender<Option<String>>,
    },
    Riders {
        reply: oneshot::Sender<usize>,
    },
    Shutdown {
        reply: oneshot::Sender<SavedRoom>,
    },
    /// A rider's report (crate::reports): its context, if the key holds a
    /// slot here.
    Report {
        key: String,
        target: u8,
        reply: oneshot::Sender<Option<ReportContext>>,
    },
}

#[derive(Clone)]
pub struct RoomHandle {
    pub tx: mpsc::Sender<RoomMsg>,
    generation: u64,
}

impl RoomHandle {
    pub fn alive(&self) -> bool {
        !self.tx.is_closed()
    }
    async fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> RoomMsg) -> Option<T> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(make(reply)).await.ok()?;
        answer.await.ok()
    }
    pub async fn admit(
        &self,
        conn: ConnId,
        key: Option<String>,
        counted: bool,
        pid: Option<String>,
        outbox: Arc<Outbox>,
    ) -> Option<Result<u8, Refused>> {
        self.ask(|reply| RoomMsg::Admit {
            conn,
            key,
            counted,
            pid,
            outbox,
            reply,
        })
        .await
    }
    pub async fn claim(&self) -> Option<Option<String>> {
        self.ask(|reply| RoomMsg::Claim { reply }).await
    }
    pub async fn claim_quickplay(&self, any_phase: bool) -> Option<Option<String>> {
        self.ask(|reply| RoomMsg::QuickClaim { any_phase, reply })
            .await
    }
    pub async fn riders(&self) -> Option<usize> {
        self.ask(|reply| RoomMsg::Riders { reply }).await
    }
    pub async fn shut_down(&self) -> Option<SavedRoom> {
        self.ask(|reply| RoomMsg::Shutdown { reply }).await
    }
    pub async fn report(&self, key: String, target: u8) -> Option<ReportContext> {
        self.ask(|reply| RoomMsg::Report { key, target, reply })
            .await
            .flatten()
    }
}

#[derive(Default)]
pub struct Registry {
    rooms: Mutex<HashMap<RoomCode, RoomHandle>>,
    generation: std::sync::atomic::AtomicU64,
}

impl Registry {
    pub fn get(&self, code: &RoomCode) -> Option<RoomHandle> {
        let rooms = self.rooms.lock().expect("registry lock");
        rooms.get(code).filter(|h| h.alive()).cloned()
    }

    /// Starts `room` unless a live room has its code; the new handle, or
    /// None when the code is taken.
    pub fn open(&self, shared: &Arc<Shared>, room: Room) -> Option<RoomHandle> {
        let mut rooms = self.rooms.lock().expect("registry lock");
        let code = room.code();
        if rooms.get(&code).is_some_and(RoomHandle::alive) {
            return None;
        }
        let (tx, rx) = mpsc::channel(ROOM_QUEUE);
        let handle = RoomHandle {
            tx,
            generation: self.generation.fetch_add(1, Ordering::Relaxed),
        };
        rooms.insert(code, handle.clone());
        shared
            .metrics
            .rooms_open
            .store(rooms.len() as u64, Ordering::Relaxed);
        shared.metrics.rooms_created.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(run(shared.clone(), room, handle.generation, rx));
        Some(handle)
    }

    fn remove(&self, metrics: &Metrics, code: &RoomCode, generation: u64) {
        let mut rooms = self.rooms.lock().expect("registry lock");
        if rooms.get(code).is_some_and(|h| h.generation == generation) {
            rooms.remove(code);
        }
        metrics
            .rooms_open
            .store(rooms.len() as u64, Ordering::Relaxed);
    }

    pub fn all(&self) -> Vec<RoomHandle> {
        let rooms = self.rooms.lock().expect("registry lock");
        rooms.values().cloned().collect()
    }
}

/// Delivers what the engine returned; connections whose queues are stuck
/// are handed back to the engine as closed, which may return more.
fn deliver(
    room: &mut Room,
    out: &mut Vec<Out>,
    outboxes: &mut HashMap<ConnId, Arc<Outbox>>,
    metrics: &Metrics,
) {
    while !out.is_empty() {
        let mut stuck = Vec::new();
        for action in out.drain(..) {
            let (id, bytes, pose) = match action {
                Out::Close(id, code, reason) => {
                    if let Some(outbox) = outboxes.remove(&id) {
                        outbox.close(code, reason);
                    }
                    continue;
                }
                Out::Send(id, bytes) => (id, bytes, false),
                Out::Pose(id, bytes) => (id, bytes, true),
            };
            let Some(outbox) = outboxes.get(&id).filter(|_| !stuck.contains(&id)) else {
                continue;
            };
            metrics
                .bytes_out
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            match outbox.push(bytes, pose) {
                Pushed::Stuck => stuck.push(id),
                Pushed::Dropped(n) => {
                    metrics.poses_dropped.fetch_add(n as u64, Ordering::Relaxed);
                }
                Pushed::Queued => {}
            }
        }
        for id in stuck {
            metrics.slow_closes.fetch_add(1, Ordering::Relaxed);
            if let Some(outbox) = outboxes.remove(&id) {
                outbox.close(1013, "Connection too slow");
            }
            room.disconnected(id, 1013, "Connection too slow", now_ms(), out);
        }
    }
}

/// What a room last added to the rider and room gauges.
#[derive(Clone, Copy, Default, PartialEq)]
struct Reported {
    riders: u64,
    counted_riders: u64,
    counted_room: u64,
}

/// Reports a room's counts to the metrics, and a shared room's to the
/// daily usage, with its riders now. Only riders from counted addresses
/// reach the public gauges, and a room is public only while one of them is
/// in it; a room without one adds nothing to the daily usage.
fn report(shared: &Shared, room: &mut Room, reported: &mut Reported) {
    let counters = room.take_counters();
    shared.metrics.add(&counters);
    let now = if room.closed() {
        Reported::default()
    } else {
        let counted_riders = room.counted_riders() as u64;
        Reported {
            riders: room.riders() as u64,
            counted_riders,
            counted_room: u64::from(counted_riders > 0),
        }
    };
    if now != *reported {
        let m = &shared.metrics;
        for (gauge, new, old) in [
            (&m.riders_all, now.riders, reported.riders),
            (&m.riders, now.counted_riders, reported.counted_riders),
            (&m.rooms, now.counted_room, reported.counted_room),
        ] {
            gauge.fetch_add(new, Ordering::Relaxed);
            gauge.fetch_sub(old, Ordering::Relaxed);
        }
        *reported = now;
    }
    if room.public() {
        let mut counts = room.rider_counts();
        let counters = if now.counted_room == 0 {
            counts.clear();
            Counters::default()
        } else {
            counters
        };
        let code = String::from_utf8_lossy(&room.code()).into_owned();
        shared
            .usage
            .lock()
            .expect("usage lock")
            .report(&code, &counters, counts, now_ms());
    }
}

async fn run(
    shared: Arc<Shared>,
    mut room: Room,
    generation: u64,
    mut rx: mpsc::Receiver<RoomMsg>,
) {
    let code = room.code();
    let mut outboxes: HashMap<ConnId, Arc<Outbox>> = HashMap::new();
    let mut out = Vec::new();
    let mut rng = OsEntropy;
    let mut tick = tokio::time::interval(Duration::from_millis(TICK_MS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut housekeep_at = now_ms() + HOUSEKEEP_MS;
    let mut report_at = now_ms() + REPORT_MS;
    let mut reported = Reported::default();
    tracing::info!(event = "room_opened", room = %String::from_utf8_lossy(&code), public = room.public());
    let ended = loop {
        tokio::select! {
            msg = rx.recv() => {
                let Some(msg) = msg else { break "Room ended" };
                let now = now_ms();
                match msg {
                    RoomMsg::Admit { conn, key, counted, pid, outbox, reply } => {
                        let admitted = room.admit(conn, key.as_deref(), now, &mut rng, &mut out);
                        if let Ok(peer) = admitted {
                            room.set_counted(peer, counted);
                            room.set_pid(peer, pid);
                            outboxes.insert(conn, outbox);
                        }
                        let _ = reply.send(admitted);
                    }
                    RoomMsg::Message { conn, bytes } => room.receive(conn, bytes, now, &mut out),
                    RoomMsg::Closed { conn, code, reason } => {
                        outboxes.remove(&conn);
                        room.disconnected(conn, code, &reason, now, &mut out);
                    }
                    RoomMsg::Claim { reply } => {
                        let _ = reply.send(room.claim(now, &mut rng));
                    }
                    RoomMsg::QuickClaim { any_phase, reply } => {
                        let _ = reply.send(room.claim_quickplay(now, &mut rng, any_phase));
                    }
                    RoomMsg::Riders { reply } => {
                        let _ = reply.send(room.riders());
                    }
                    RoomMsg::Report { key, target, reply } => {
                        let _ = reply.send(room.report_context(&key, target, now));
                    }
                    RoomMsg::Shutdown { reply } => {
                        let saved = room.shut_down(now, &mut out);
                        deliver(&mut room, &mut out, &mut outboxes, &shared.metrics);
                        let _ = reply.send(saved);
                        break "Server restarting";
                    }
                }
            }
            _ = tick.tick() => {
                let now = now_ms();
                room.tick(now, &mut out);
                crate::queue::save_records(&shared, room.take_records());
                if now >= housekeep_at {
                    housekeep_at = now + HOUSEKEEP_MS;
                    if let Some(reason) = room.housekeep(now, &mut out) {
                        deliver(&mut room, &mut out, &mut outboxes, &shared.metrics);
                        break reason;
                    }
                }
                if now >= report_at {
                    report_at = now + REPORT_MS;
                    report(&shared, &mut room, &mut reported);
                }
            }
        }
        deliver(&mut room, &mut out, &mut outboxes, &shared.metrics);
    };
    // Senders still holding the handle find the channel closed from here.
    rx.close();
    shared.registry.remove(&shared.metrics, &code, generation);
    if !room.closed() {
        room.finish("Room ended", &mut out);
        deliver(&mut room, &mut out, &mut outboxes, &shared.metrics);
    }
    report(&shared, &mut room, &mut reported);
    tracing::info!(event = "room_closed", room = %String::from_utf8_lossy(&code), reason = ended);
}
