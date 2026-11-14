//! Quick chat in a room (research/QUICK_CHAT.md): preset ids only, rate
//! limited per sender, relayed to builds that said [`signal::CHAT`], the
//! horn only to riders within [`chat::HEARING_FT`]. Each room keeps the
//! last two minutes of events so a report can carry them.

use std::collections::VecDeque;

use atv_net::{Packet, chat, signal};
use serde::Serialize;

use super::{ConnId, Out, Room};

/// Events older than this leave the ring.
pub const RING_MS: u64 = 120_000;
/// Most events a ring holds (a full room at its limits stays under this).
pub const RING_EVENTS: usize = 256;

/// One relayed chat event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Event {
    pub t: u64,
    pub peer: u8,
    pub name: String,
    pub kind: u8,
    pub id: u8,
}

/// The room's recent chat, oldest first.
#[derive(Default)]
pub struct Ring(VecDeque<Event>);

impl Ring {
    pub fn push(&mut self, event: Event) {
        let now = event.t;
        if self.0.len() == RING_EVENTS {
            self.0.pop_front();
        }
        self.0.push_back(event);
        self.prune(now);
    }
    pub fn prune(&mut self, now: u64) {
        while self
            .0
            .front()
            .is_some_and(|e| now.saturating_sub(e.t) > RING_MS)
        {
            self.0.pop_front();
        }
    }
    pub fn recent(&self, now: u64) -> Vec<Event> {
        self.0
            .iter()
            .filter(|e| now.saturating_sub(e.t) <= RING_MS)
            .cloned()
            .collect()
    }
}

/// What a report carries about its room (research/QUICK_CHAT.md
/// "Reports"): slots and shown names, and the recent chat.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReportContext {
    pub room: String,
    pub public: bool,
    pub reporter: u8,
    pub reporter_name: String,
    pub target: u8,
    pub target_name: String,
    pub events: Vec<Event>,
}

fn squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| (a - b) * (a - b)).sum()
}

impl Room {
    /// A chat event from connection `i`: dropped unless it passes the
    /// sender's limits, then relayed (and echoed to the sender, so it shows
    /// only what others see).
    pub(super) fn chat(&mut self, i: usize, message: chat::Chat, now: u64, out: &mut Vec<Out>) {
        if !self.conns[i].chat.take(message.kind, now) {
            return;
        }
        let conn = &self.conns[i];
        let message = chat::Chat {
            peer: conn.peer,
            ..message
        };
        let name = conn
            .player
            .as_ref()
            .map_or_else(String::new, |p| p.name.clone());
        let heard_from = (message.kind == chat::HORN).then(|| {
            (self.started && conn.ready_map == self.map)
                .then(|| conn.current_pose().map(|p| p.position))
                .flatten()
        });
        let Ok(bytes) = atv_net::encode(&Packet::Chat(message)) else {
            return;
        };
        let bytes = bytes::Bytes::from(bytes);
        let reach = chat::HEARING_FT * chat::HEARING_FT;
        let sender: ConnId = conn.id;
        for c in &self.conns {
            if c.player.is_none() || !c.chat_on {
                continue;
            }
            if c.id != sender
                && let Some(from) = heard_from
            {
                let near = from.is_some_and(|from| {
                    c.ready_map == self.map
                        && c.current_pose()
                            .is_some_and(|p| squared(p.position, from) <= reach)
                });
                if !near {
                    continue;
                }
            }
            out.push(Out::Send(c.id, bytes.clone()));
        }
        self.chat_ring.push(Event {
            t: now,
            peer: message.peer,
            name,
            kind: message.kind,
            id: message.id,
        });
    }

    /// A report from the rider holding `key` about slot `target`, with the
    /// room's recent chat; None when the key holds no slot here or the
    /// target is the reporter or no rider.
    pub fn report_context(&mut self, key: &str, target: u8, now: u64) -> Option<ReportContext> {
        let name = |peer: u8| {
            self.conns
                .iter()
                .find(|c| c.peer == peer)
                .and_then(|c| c.player.as_ref())
                .or_else(|| {
                    self.reserved
                        .iter()
                        .find(|r| r.peer == peer)
                        .and_then(|r| r.player.as_ref())
                })
                .map(|p| p.name.clone())
        };
        let reporter = self
            .conns
            .iter()
            .find(|c| c.key == key)
            .map(|c| c.peer)
            .or_else(|| self.reserved.iter().find(|r| r.key == key).map(|r| r.peer))?;
        if reporter == target {
            return None;
        }
        let target_name = name(target)?;
        let reporter_name = name(reporter).unwrap_or_default();
        self.chat_ring.prune(now);
        Some(ReportContext {
            room: String::from_utf8_lossy(&self.code).into_owned(),
            public: self.public,
            reporter,
            reporter_name,
            target,
            target_name,
            events: self.chat_ring.recent(now),
        })
    }
}

/// Whether a HELLO's flags take chat.
pub(super) fn said_chat(flags: u8) -> bool {
    flags & signal::CHAT != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(t: u64) -> Event {
        Event {
            t,
            peer: 0,
            name: "A".into(),
            kind: chat::LINE,
            id: 0,
        }
    }

    #[test]
    fn ring_keeps_two_minutes_and_a_bounded_count() {
        let mut ring = Ring::default();
        ring.push(event(0));
        ring.push(event(60_000));
        assert_eq!(ring.recent(120_000).len(), 2);
        assert_eq!(ring.recent(150_000).len(), 1);
        ring.push(event(200_000));
        assert_eq!(ring.0.len(), 1);
        for t in 0..RING_EVENTS as u64 * 2 {
            ring.push(event(200_000 + t));
        }
        assert_eq!(ring.0.len(), RING_EVENTS);
        assert_eq!(ring.0.front().unwrap().t, 200_000 + RING_EVENTS as u64);
    }
}
