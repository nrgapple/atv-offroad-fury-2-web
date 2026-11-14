//! A connection's bounded send queue. The room actor pushes without
//! waiting; the socket's writer drains it. When the queue is over its bound
//! the oldest poses go first (a newer one follows); when only control
//! packets are left and it is still over, the connection is stuck and
//! closed.

use std::collections::VecDeque;
use std::sync::Mutex;

use bytes::Bytes;
use tokio::sync::Notify;

/// Most bytes queued for one connection.
pub const QUEUE_BYTES: usize = 256 * 1024;
/// Most packets queued for one connection.
pub const QUEUE_PACKETS: usize = 512;

#[derive(Debug, PartialEq)]
pub enum Next {
    Packet(Bytes),
    Close(u16, &'static str),
}

#[derive(Default)]
struct State {
    queue: VecDeque<(Bytes, bool)>,
    bytes: usize,
    close: Option<(u16, &'static str)>,
}

#[derive(Default)]
pub struct Outbox {
    state: Mutex<State>,
    notify: Notify,
}

/// What a push did.
#[derive(Debug, PartialEq, Eq)]
pub enum Pushed {
    Queued,
    /// Queued, after dropping this many older poses.
    Dropped(usize),
    /// Over the bound with nothing left to drop: close the connection.
    Stuck,
}

impl Outbox {
    /// Queues a packet; `pose` marks one a newer pose replaces.
    pub fn push(&self, bytes: Bytes, pose: bool) -> Pushed {
        let mut s = self.state.lock().expect("outbox lock");
        if s.close.is_some() {
            return Pushed::Queued;
        }
        s.bytes += bytes.len();
        s.queue.push_back((bytes, pose));
        let mut dropped = 0;
        while s.bytes > QUEUE_BYTES || s.queue.len() > QUEUE_PACKETS {
            let Some(i) = s.queue.iter().position(|(_, pose)| *pose) else {
                return Pushed::Stuck;
            };
            let (old, _) = s.queue.remove(i).expect("queued pose");
            s.bytes -= old.len();
            dropped += 1;
        }
        drop(s);
        self.notify.notify_one();
        if dropped > 0 {
            Pushed::Dropped(dropped)
        } else {
            Pushed::Queued
        }
    }

    /// Sends what is queued (poses excepted), then closes.
    pub fn close(&self, code: u16, reason: &'static str) {
        let mut s = self.state.lock().expect("outbox lock");
        s.close.get_or_insert((code, reason));
        s.queue.retain(|(_, pose)| !pose);
        drop(s);
        self.notify.notify_one();
    }

    fn take(&self) -> Option<Next> {
        let mut s = self.state.lock().expect("outbox lock");
        if let Some((bytes, _)) = s.queue.pop_front() {
            s.bytes -= bytes.len();
            return Some(Next::Packet(bytes));
        }
        s.close.map(|(code, reason)| Next::Close(code, reason))
    }

    /// The next packet to send, or the close once the queue is empty.
    pub async fn next(&self) -> Next {
        loop {
            if let Some(next) = self.take() {
                return next;
            }
            self.notify.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn slow_connections_lose_old_poses_first_then_close() {
        let outbox = Outbox::default();
        let pose = Bytes::from(vec![0; 1024]);
        assert_eq!(
            outbox.push(Bytes::from_static(b"lobby"), false),
            Pushed::Queued
        );
        for _ in 0..QUEUE_BYTES / 1024 - 1 {
            assert_eq!(outbox.push(pose.clone(), true), Pushed::Queued);
        }
        assert_eq!(outbox.push(pose.clone(), true), Pushed::Dropped(1));
        // The control packet is still first.
        assert_eq!(
            outbox.next().await,
            Next::Packet(Bytes::from_static(b"lobby"))
        );
        // Control packets alone over the bound: stuck.
        let stuck = Outbox::default();
        let big = Bytes::from(vec![0; QUEUE_BYTES / 2]);
        assert_eq!(stuck.push(big.clone(), false), Pushed::Queued);
        assert_eq!(stuck.push(big.clone(), false), Pushed::Queued);
        assert_eq!(stuck.push(big, false), Pushed::Stuck);
        // A close sends the queued control packets, drops poses, then closes.
        let closing = Outbox::default();
        closing.push(pose.clone(), true);
        closing.push(Bytes::from_static(b"leave"), false);
        closing.close(1012, "Server restarting");
        closing.push(Bytes::from_static(b"late"), false);
        assert_eq!(
            closing.next().await,
            Next::Packet(Bytes::from_static(b"leave"))
        );
        assert_eq!(closing.next().await, Next::Close(1012, "Server restarting"));
    }
}
