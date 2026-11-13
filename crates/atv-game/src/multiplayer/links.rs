//! Direct links between riders (research/MULTIPLAYER.md "Direct links").
//!
//! The relay keeps joining, the roster, the clock and room games. Poses,
//! which are nearly all of a room's traffic, go straight from rider to rider
//! over a link once the two have one, and the relay then carries only a slow
//! presence pose for them. This is the transport-independent half: who is
//! riding the course, which side offers, timeouts, retries and how often each
//! path needs a pose. The host owns the actual connections and follows the
//! [`Command`]s.
use atv_net::{LinkReport, RiderState, Signal, signal};
use std::collections::BTreeMap;

/// A handshake that has not opened by now has failed (blocked UDP or a
/// network that hands every destination a different address).
pub const CONNECT_MS: u64 = 12_000;
/// A linked rider sends at least once a second; a link this quiet is dead
/// even if the browser has not noticed yet.
pub const SILENCE_MS: u64 = 4_000;
/// Handshakes start one at a time, this far apart, so joining a full room
/// does not open nineteen connections in one frame.
const STAGGER_MS: u64 = 150;
/// The relay hears about changed links at most this often.
const REPORT_MS: u64 = 500;
/// A link that lasted this long was healthy: its next failure retries soon.
const STABLE_MS: u64 = 30_000;
/// Relay pose interval while every rider on the course is linked: it only
/// keeps the relay's copy (handed to the next arrival) roughly current.
pub const PRESENCE_MS: u32 = 3000;
/// The same, stationary or paused.
pub const PRESENCE_IDLE_MS: u32 = 5000;
/// The relay hears this client's [`LinkReport`] at most this often.
const LINK_REPORT_MS: u64 = 60_000;
/// Assumed distance (ft) to a rider whose pose has not arrived yet.
pub const UNKNOWN_FT: f32 = 100.;

/// What the host does with its connections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Open a connection to this rider and create an offer.
    Offer(u8),
    /// Open a connection from this rider's offer and create the answer.
    Accept(u8, Vec<u8>),
    /// Complete the connection we offered with this rider's answer.
    Answer(u8, Vec<u8>),
    /// Drop any connection to this rider.
    Close(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Connecting { since_ms: u64 },
    Open { since_ms: u64, heard_ms: u64 },
}

/// One other rider riding this course.
#[derive(Clone, Copy, Debug)]
struct Rider {
    /// Both ends can open direct links.
    direct: bool,
    /// This client arrived after the rider, so it creates the offer.
    offerer: bool,
    state: State,
    /// Failed handshakes in a row.
    failures: u8,
    /// When the offerer may try again.
    retry_ms: u64,
    /// The last pose sent over the link (session clock).
    sent_ms: Option<u32>,
}

/// The discrete part of a pose: a change goes to every linked rider at once.
type Key = (RiderState, u32, bool, Option<u8>);

#[derive(Default)]
pub struct Links {
    /// The relay was told this build understands signalling.
    announced: bool,
    /// This client can open direct links.
    direct: bool,
    riders: BTreeMap<u8, Rider>,
    commands: Vec<Command>,
    next_offer_ms: u64,
    reported: u32,
    report_ms: u64,
    key: Option<Key>,
    /// What the links did, for the relay's accounting.
    pub counts: LinkReport,
    counts_sent: LinkReport,
    counts_ms: Option<u64>,
}

fn bump(count: &mut u16) {
    *count = count.saturating_add(1);
}

fn backoff(failures: u8, stable: bool) -> u64 {
    match (failures, stable) {
        (0 | 1, true) => 3_000,
        (0 | 1, false) => 30_000,
        (2, _) => 120_000,
        _ => 300_000,
    }
}

impl Links {
    /// The relay now reports who rides the course (`direct`: and this
    /// client opens links).
    pub fn announce(&mut self, direct: bool) {
        self.announced = true;
        self.direct = direct;
    }

    /// Whether the relay reports the riders on this course. Without it
    /// (desktop builds) every rider in the room is assumed to need relayed
    /// poses, as before direct links.
    pub fn announced(&self) -> bool {
        self.announced
    }

    /// This client (re)loaded the course: every link starts over from the
    /// relay's next introductions.
    pub fn reset(&mut self) {
        let riders: Vec<u8> = self.riders.keys().copied().collect();
        for peer in riders {
            self.forget(peer);
        }
        self.reported = 0;
        self.key = None;
    }

    /// The rider left the course (or the room).
    pub fn forget(&mut self, peer: u8) {
        if let Some(rider) = self.riders.remove(&peer)
            && rider.state != State::Idle
        {
            self.commands.push(Command::Close(peer));
        }
    }

    /// Keeps only riders `keep` accepts.
    pub fn retain(&mut self, keep: impl Fn(u8) -> bool) {
        let gone: Vec<u8> = self.riders.keys().copied().filter(|p| !keep(*p)).collect();
        for peer in gone {
            self.forget(peer);
        }
    }

    /// One signalling message from the relay.
    pub fn signal(&mut self, message: Signal, now_ms: u64) {
        let peer = message.peer;
        match message.kind {
            signal::PRESENT | signal::ARRIVED => {
                // The relay's latest word decides who offers; any earlier
                // link to this slot belongs to a previous arrival.
                self.forget(peer);
                self.riders.insert(
                    peer,
                    Rider {
                        direct: self.direct && message.data[0] & signal::DIRECT != 0,
                        offerer: message.kind == signal::PRESENT,
                        state: State::Idle,
                        failures: 0,
                        retry_ms: 0,
                        sent_ms: None,
                    },
                );
            }
            signal::GONE => self.forget(peer),
            // The other end gave the link up (it heard nothing, or its
            // connection failed): fall back to the relay with it.
            signal::DROPPED => {
                if self
                    .riders
                    .get(&peer)
                    .is_some_and(|r| r.state != State::Idle)
                {
                    bump(&mut self.counts.dropped);
                }
                self.fail(peer, now_ms, true);
            }
            signal::OFFER => {
                // Only the later arrival offers; an offer from the other
                // side predates the relay's introduction and is stale.
                if let Some(rider) = self.riders.get_mut(&peer)
                    && rider.direct
                    && !rider.offerer
                {
                    rider.state = State::Connecting { since_ms: now_ms };
                    rider.sent_ms = None;
                    bump(&mut self.counts.attempts);
                    self.commands.push(Command::Accept(peer, message.data));
                }
            }
            signal::ANSWER => {
                if let Some(rider) = self.riders.get(&peer)
                    && rider.offerer
                    && matches!(rider.state, State::Connecting { .. })
                {
                    self.commands.push(Command::Answer(peer, message.data));
                }
            }
            _ => {}
        }
    }

    fn fail(&mut self, peer: u8, now_ms: u64, close: bool) {
        let Some(rider) = self.riders.get_mut(&peer) else {
            return;
        };
        let stable = match rider.state {
            State::Idle => return,
            State::Connecting { .. } => false,
            State::Open { since_ms, .. } => now_ms.saturating_sub(since_ms) >= STABLE_MS,
        };
        if stable {
            rider.failures = 0;
        }
        rider.failures = rider.failures.saturating_add(1);
        rider.retry_ms = now_ms + backoff(rider.failures, stable);
        rider.state = State::Idle;
        rider.sent_ms = None;
        if close {
            self.commands.push(Command::Close(peer));
        }
    }

    /// The host's connection to this rider opened.
    pub fn opened(&mut self, peer: u8, now_ms: u64) {
        if let Some(rider) = self.riders.get_mut(&peer)
            && matches!(rider.state, State::Connecting { .. })
        {
            rider.state = State::Open {
                since_ms: now_ms,
                heard_ms: now_ms,
            };
            rider.sent_ms = None;
            bump(&mut self.counts.opened);
        }
    }

    /// The host's connection to this rider closed or could not be made.
    pub fn closed(&mut self, peer: u8, now_ms: u64) {
        match self.riders.get(&peer).map(|r| r.state) {
            Some(State::Connecting { .. }) => bump(&mut self.counts.failed),
            Some(State::Open { .. }) => bump(&mut self.counts.lost),
            _ => {}
        }
        self.fail(peer, now_ms, false);
    }

    /// This client sent a session description: which kinds of address it
    /// offered say what the network allows (no public one: STUN is blocked).
    pub fn described(&mut self, description: &[u8]) {
        let text = String::from_utf8_lossy(description);
        let candidates = || text.lines().filter(|l| l.contains("candidate:"));
        bump(&mut self.counts.described);
        if candidates().any(|l| l.contains(" typ srflx")) {
            bump(&mut self.counts.public);
        }
        let host = || candidates().filter(|l| l.contains(" typ host"));
        if host().any(|l| l.contains(".local ")) {
            bump(&mut self.counts.hidden);
        }
        if host().any(|l| !l.contains(".local ")) {
            bump(&mut self.counts.local);
        }
    }

    /// The counts for the relay, when they changed and the last report is a
    /// minute old.
    pub fn link_report(&mut self, now_ms: u64) -> Option<LinkReport> {
        if !self.announced
            || self.counts == self.counts_sent
            || self
                .counts_ms
                .is_some_and(|at| now_ms.saturating_sub(at) < LINK_REPORT_MS)
        {
            return None;
        }
        self.counts_sent = self.counts;
        self.counts_ms = Some(now_ms);
        Some(self.counts)
    }

    /// A pose arrived over this rider's link. False when no link to the
    /// rider is expected (the pose is then ignored).
    pub fn heard(&mut self, peer: u8, now_ms: u64) -> bool {
        match self.riders.get_mut(&peer).map(|r| &mut r.state) {
            Some(State::Open { heard_ms, .. }) => {
                *heard_ms = now_ms;
                true
            }
            _ => false,
        }
    }

    /// Timeouts and due handshakes, then everything queued for the host.
    pub fn commands(&mut self, now_ms: u64) -> Vec<Command> {
        let expired: Vec<u8> = self
            .riders
            .iter()
            .filter(|(_, r)| match r.state {
                State::Idle => false,
                State::Connecting { since_ms } => now_ms.saturating_sub(since_ms) > CONNECT_MS,
                State::Open { heard_ms, .. } => now_ms.saturating_sub(heard_ms) > SILENCE_MS,
            })
            .map(|(peer, _)| *peer)
            .collect();
        for peer in expired {
            if let Some(rider) = self.riders.get(&peer) {
                if matches!(rider.state, State::Connecting { .. }) {
                    bump(&mut self.counts.timeouts);
                } else {
                    bump(&mut self.counts.silent);
                }
            }
            self.fail(peer, now_ms, true);
        }
        if now_ms >= self.next_offer_ms
            && let Some((&peer, rider)) = self.riders.iter_mut().find(|(_, r)| {
                r.direct && r.offerer && r.state == State::Idle && now_ms >= r.retry_ms
            })
        {
            rider.state = State::Connecting { since_ms: now_ms };
            self.next_offer_ms = now_ms + STAGGER_MS;
            bump(&mut self.counts.attempts);
            self.commands.push(Command::Offer(peer));
        }
        std::mem::take(&mut self.commands)
    }

    /// Whether the link to this rider is open.
    pub fn is_open(&self, peer: u8) -> bool {
        self.riders
            .get(&peer)
            .is_some_and(|r| matches!(r.state, State::Open { .. }))
    }

    /// One bit per rider with an open link.
    pub fn mask(&self) -> u32 {
        self.riders
            .iter()
            .filter(|(_, r)| matches!(r.state, State::Open { .. }))
            .fold(0, |mask, (peer, _)| mask | 1 << peer)
    }

    /// The link mask to report to the relay, when it changed and the last
    /// report is old enough.
    pub fn report(&mut self, now_ms: u64) -> Option<u32> {
        let mask = self.mask();
        if !self.announced
            || mask == self.reported
            || now_ms.saturating_sub(self.report_ms) < REPORT_MS
        {
            return None;
        }
        self.reported = mask;
        self.report_ms = now_ms;
        Some(mask)
    }

    /// Riders on the course who get this client's poses through the relay.
    pub fn relayed(&self) -> impl Iterator<Item = u8> + '_ {
        self.riders
            .iter()
            .filter(|(_, r)| !matches!(r.state, State::Open { .. }))
            .map(|(peer, _)| *peer)
    }

    /// Linked riders whose next pose is due. `every` gives a rider's
    /// interval; a change of `key` (state, crash, pause, platform) is due
    /// for everyone at once.
    pub fn due(&mut self, now: u32, key: Key, every: impl Fn(u8) -> u32) -> Vec<u8> {
        let changed = self.key != Some(key);
        let mut due = Vec::new();
        for (&peer, rider) in &mut self.riders {
            if matches!(rider.state, State::Open { .. })
                && (changed
                    || rider
                        .sent_ms
                        .is_none_or(|sent| now.wrapping_sub(sent) >= every(peer)))
            {
                rider.sent_ms = Some(now);
                due.push(peer);
            }
        }
        if !due.is_empty() {
            self.key = Some(key);
        }
        due
    }

    /// Each rider on the course and its link, for status lines and tests.
    pub fn states(&self) -> Vec<(u8, &'static str)> {
        self.riders
            .iter()
            .map(|(&peer, r)| {
                let state = match r.state {
                    State::Open { .. } => "direct",
                    State::Connecting { .. } => "connecting",
                    State::Idle if !r.direct => "relay",
                    State::Idle if r.failures > 0 => "retry",
                    State::Idle => "waiting",
                };
                (peer, state)
            })
            .collect()
    }
}

/// How often one path needs this rider's pose (ms). `feet` is the distance
/// to the rider it is for (the nearest, on the relay); `direct` is a link,
/// which costs nothing and so idles and fades out a little faster.
///
/// Up to 20 Hz near, fast, airborne or wrecked; 10 Hz moving; 5 Hz beyond
/// 300 ft; about 3 Hz beyond 1000 ft (still inside the smoothing buffer's
/// 400 ms cadence window); 1 Hz stationary; 0.2 Hz paused.
pub fn interval(
    paused: bool,
    moving: bool,
    state: RiderState,
    speed: f32,
    feet: f32,
    direct: bool,
) -> u32 {
    if paused {
        if direct { 1000 } else { 5000 }
    } else if !moving && state == RiderState::Riding {
        1000
    } else if feet >= 1000. {
        if direct { 300 } else { 350 }
    } else if feet > 300. {
        200
    } else if state == RiderState::Wreck
        || state == RiderState::Airborne
        || speed > 35.
        || feet < 40.
    {
        50
    } else {
        100
    }
}

/// [`interval`] for the relay path in a shared Free Ride room (research/
/// MULTIPLAYER.md "Free Ride relay intervals"). The relay is billed per
/// incoming message and those rooms are nearly all of its traffic, so each
/// tier waits as long as receivers already smooth: their playback delay
/// follows the cadence up to 220 ms and they call a rider stale 2.5 s after
/// a stationary pose and 15 s after a paused one. Builds from before this
/// table receive these poses too.
///
/// 10 Hz near, fast, airborne or wrecked; about 7 Hz moving; about 3 Hz
/// beyond 300 ft; 0.5 Hz stationary; 0.1 Hz paused.
pub fn freeride_interval(
    paused: bool,
    moving: bool,
    state: RiderState,
    speed: f32,
    feet: f32,
) -> u32 {
    if paused {
        10_000
    } else if !moving && state == RiderState::Riding {
        2000
    } else if feet >= 1000. {
        350
    } else if feet > 300. {
        300
    } else if state == RiderState::Wreck
        || state == RiderState::Airborne
        || speed > 35.
        || feet < 40.
    {
        100
    } else {
        150
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(peer: u8, kind: u8, flags: u8) -> Signal {
        Signal {
            peer,
            kind,
            data: vec![flags],
        }
    }
    fn sdp(peer: u8, kind: u8) -> Signal {
        Signal {
            peer,
            kind,
            data: b"v=0".to_vec(),
        }
    }
    fn linked() -> Links {
        let mut links = Links::default();
        links.announce(true);
        links
    }

    #[test]
    fn the_arrival_offers_and_the_rider_already_there_answers() {
        // This client arrives: riders 2 and 5 are there, 5 on a plain build.
        let mut arrival = linked();
        arrival.signal(notice(2, signal::PRESENT, signal::DIRECT), 0);
        arrival.signal(notice(5, signal::PRESENT, 0), 0);
        assert_eq!(arrival.commands(0), [Command::Offer(2)]);
        assert_eq!(arrival.relayed().collect::<Vec<_>>(), [2, 5]);
        // The rider already there waits for the offer.
        let mut resident = linked();
        resident.signal(notice(9, signal::ARRIVED, signal::DIRECT), 0);
        assert!(resident.commands(0).is_empty());
        resident.signal(sdp(9, signal::OFFER), 10);
        assert_eq!(resident.commands(10), [Command::Accept(9, b"v=0".to_vec())]);
        // An answer completes only an offer this client made.
        resident.signal(sdp(9, signal::ANSWER), 20);
        assert!(resident.commands(20).is_empty());
        arrival.signal(sdp(2, signal::ANSWER), 20);
        assert_eq!(arrival.commands(20), [Command::Answer(2, b"v=0".to_vec())]);
        // An offer from a rider this client must offer to is stale.
        arrival.signal(sdp(2, signal::OFFER), 30);
        assert!(arrival.commands(30).is_empty());
        // Open links leave the relay; the plain rider stays on it.
        arrival.opened(2, 40);
        assert_eq!(arrival.mask(), 1 << 2);
        assert_eq!(arrival.relayed().collect::<Vec<_>>(), [5]);
        assert_eq!(arrival.report(600), Some(1 << 2));
        assert_eq!(arrival.report(2000), None);
        assert_eq!(arrival.states(), [(2, "direct"), (5, "relay")]);
    }

    #[test]
    fn handshakes_are_staggered_time_out_and_back_off() {
        let mut links = linked();
        for peer in [1, 2, 3] {
            links.signal(notice(peer, signal::PRESENT, signal::DIRECT), 0);
        }
        assert_eq!(links.commands(0), [Command::Offer(1)]);
        assert!(links.commands(100).is_empty());
        assert_eq!(links.commands(150), [Command::Offer(2)]);
        assert_eq!(links.commands(300), [Command::Offer(3)]);
        links.opened(1, 400);
        links.opened(2, 400);
        // Rider 3 never answers: the attempt ends and waits half a minute.
        let failed = 300 + CONNECT_MS + 1;
        for time in (1000..failed).step_by(1000) {
            links.heard(1, time);
            links.heard(2, time);
            assert!(links.commands(time).is_empty());
        }
        assert_eq!(links.commands(failed), [Command::Close(3)]);
        assert_eq!(links.states()[2], (3, "retry"));
        links.heard(1, failed + 29_000);
        links.heard(2, failed + 29_000);
        assert!(links.commands(failed + 29_000).is_empty());
        links.heard(1, failed + 30_000);
        links.heard(2, failed + 30_000);
        assert_eq!(links.commands(failed + 30_000), [Command::Offer(3)]);
        // A second failure waits two minutes.
        links.closed(3, failed + 31_000);
        links.heard(1, failed + 150_000);
        links.heard(2, failed + 150_000);
        assert!(links.commands(failed + 150_000).is_empty());
        links.heard(1, failed + 151_000);
        links.heard(2, failed + 151_000);
        assert_eq!(links.commands(failed + 151_000), [Command::Offer(3)]);
    }

    #[test]
    fn a_quiet_or_closed_link_falls_back_and_a_healthy_one_retries_soon() {
        let mut links = linked();
        links.signal(notice(4, signal::PRESENT, signal::DIRECT), 0);
        links.commands(0);
        links.opened(4, 100);
        assert!(links.heard(4, 40_000));
        // Silence: closed, relayed again, and (it had been stable) retried
        // three seconds later.
        let quiet = 40_000 + SILENCE_MS + 1;
        assert_eq!(links.commands(quiet), [Command::Close(4)]);
        assert_eq!(links.mask(), 0);
        assert_eq!(links.relayed().collect::<Vec<_>>(), [4]);
        assert!(!links.heard(4, quiet + 10));
        assert!(links.commands(quiet + 2_900).is_empty());
        assert_eq!(links.commands(quiet + 3_000), [Command::Offer(4)]);
        // A link that drops right after opening is not hammered.
        links.opened(4, quiet + 3_100);
        links.closed(4, quiet + 3_200);
        assert!(links.commands(quiet + 3_200 + 119_000).is_empty());
        assert_eq!(links.commands(quiet + 3_200 + 120_000), [Command::Offer(4)]);
    }

    #[test]
    fn leaving_reloading_and_new_arrivals_drop_old_links() {
        let mut links = linked();
        links.signal(notice(4, signal::PRESENT, signal::DIRECT), 0);
        links.signal(notice(6, signal::PRESENT, signal::DIRECT), 0);
        links.commands(0);
        links.commands(200);
        links.opened(4, 300);
        links.opened(6, 300);
        // Rider 4's slot is taken by a new arrival: the old link goes and
        // the newcomer offers.
        links.signal(notice(4, signal::ARRIVED, signal::DIRECT), 400);
        assert_eq!(links.commands(400), [Command::Close(4)]);
        assert_eq!(links.mask(), 1 << 6);
        // Rider 6 loses its connection to the relay.
        links.signal(notice(6, signal::GONE, 0), 500);
        assert_eq!(links.commands(500), [Command::Close(6)]);
        assert_eq!(links.relayed().collect::<Vec<_>>(), [4]);
        // The roster drops rider 4.
        links.retain(|peer| peer != 4);
        assert_eq!(links.relayed().count(), 0);
        // Reloading the course starts over and reports no links.
        links.signal(notice(7, signal::PRESENT, signal::DIRECT), 600);
        links.commands(600);
        links.opened(7, 700);
        assert_eq!(links.report(1300), Some(1 << 7));
        links.reset();
        assert_eq!(links.commands(1400), [Command::Close(7)]);
        assert_eq!(links.report(5000), None);
    }

    #[test]
    fn a_client_without_links_still_learns_who_needs_the_relay() {
        let mut links = Links::default();
        links.announce(false);
        links.signal(notice(1, signal::PRESENT, signal::DIRECT), 0);
        links.signal(notice(2, signal::ARRIVED, signal::DIRECT), 0);
        assert!(links.commands(0).is_empty());
        links.signal(sdp(2, signal::OFFER), 10);
        assert!(links.commands(10).is_empty());
        assert_eq!(links.relayed().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(links.report(1000), None);
    }

    #[test]
    fn linked_riders_get_poses_by_distance_and_state_changes_at_once() {
        let mut links = linked();
        for peer in [1, 2] {
            links.signal(notice(peer, signal::PRESENT, signal::DIRECT), 0);
        }
        links.commands(0);
        links.commands(200);
        links.opened(1, 300);
        links.opened(2, 300);
        let key = (RiderState::Riding, 0, false, None);
        let every = |peer| if peer == 1 { 50 } else { 300 };
        // A new link gets a pose at once.
        assert_eq!(links.due(1000, key, every), [1, 2]);
        assert_eq!(links.due(1050, key, every), [1]);
        assert_eq!(links.due(1100, key, every), [1]);
        assert_eq!(links.due(1300, key, every), [1, 2]);
        // A crash goes to the far rider too.
        let crashed = (RiderState::Wreck, 1, false, None);
        assert_eq!(links.due(1310, crashed, every), [1, 2]);
        assert!(links.due(1320, crashed, every).is_empty());
    }

    #[test]
    fn links_count_what_happened_for_the_relay() {
        let mut links = linked();
        assert_eq!(links.link_report(0), None);
        for peer in [1, 2, 3] {
            links.signal(notice(peer, signal::PRESENT, signal::DIRECT), 0);
        }
        links.commands(0);
        links.commands(150);
        links.commands(300);
        links.described(
            b"a=candidate:1 1 udp 1 4f2c.local 5000 typ host\r\n\
              a=candidate:2 1 udp 1 203.0.113.9 6000 typ srflx raddr 0.0.0.0\r\n",
        );
        links.described(b"a=candidate:1 1 udp 1 192.168.1.4 5000 typ host\r\n");
        links.opened(1, 400);
        links.closed(2, 500);
        assert!(links.heard(1, 300 + CONNECT_MS));
        links.commands(300 + CONNECT_MS + 1);
        links.signal(notice(1, signal::DROPPED, 0), 20_000);
        let counts = links.link_report(20_000).unwrap();
        assert_eq!(
            counts,
            LinkReport {
                attempts: 3,
                opened: 1,
                timeouts: 1,
                failed: 1,
                dropped: 1,
                described: 2,
                public: 1,
                hidden: 1,
                local: 1,
                ..LinkReport::default()
            }
        );
        // At most once a minute, and only when something changed.
        assert_eq!(links.link_report(20_001), None);
        links.signal(notice(4, signal::ARRIVED, signal::DIRECT), 30_000);
        links.signal(sdp(4, signal::OFFER), 30_000);
        assert_eq!(links.link_report(79_999), None);
        assert_eq!(links.link_report(80_000).map(|c| c.attempts), Some(4));
        assert_eq!(links.link_report(200_000), None);
    }

    #[test]
    fn pose_intervals_follow_distance_then_activity() {
        use RiderState::{Airborne, Riding};
        let relay = |state, speed, feet| interval(false, true, state, speed, feet, false);
        assert_eq!(relay(Riding, 60., 20.), 50);
        assert_eq!(relay(Riding, 20., 20.), 50);
        assert_eq!(relay(Riding, 20., 120.), 100);
        assert_eq!(relay(Airborne, 20., 120.), 50);
        // Far riders cannot see the difference: distance wins over speed.
        assert_eq!(relay(Airborne, 90., 400.), 200);
        assert_eq!(relay(Riding, 90., 1500.), 350);
        assert_eq!(relay(Riding, 90., f32::INFINITY), 350);
        assert_eq!(interval(false, true, Riding, 90., 1500., true), 300);
        // Stationary and paused riders idle; a link keeps proving it is alive.
        assert_eq!(interval(false, false, Riding, 0., 20., false), 1000);
        assert_eq!(interval(true, false, Riding, 0., 20., false), 5000);
        assert_eq!(interval(true, false, Riding, 0., 20., true), 1000);
        assert!(u64::from(interval(true, false, Riding, 0., 0., true)) < SILENCE_MS);
    }

    #[test]
    fn free_ride_relay_intervals_stay_inside_what_receivers_smooth() {
        use RiderState::{Airborne, Recovering, Riding, Wreck};
        let moving = |state, speed, feet| freeride_interval(false, true, state, speed, feet);
        // Near, fast, airborne or wrecked: 10 Hz.
        assert_eq!(moving(Riding, 20., 20.), 100);
        assert_eq!(moving(Riding, 60., 120.), 100);
        assert_eq!(moving(Airborne, 20., 120.), 100);
        assert_eq!(moving(Wreck, 0., 120.), 100);
        // Ordinary riding, and a rider getting back on.
        assert_eq!(moving(Riding, 20., 120.), 150);
        assert_eq!(moving(Recovering, 0., 300.), 150);
        // Distance wins over activity.
        assert_eq!(moving(Airborne, 90., 400.), 300);
        assert_eq!(moving(Riding, 90., 1000.), 350);
        assert_eq!(moving(Wreck, 90., f32::INFINITY), 350);
        // Stationary and paused riders stay fresh on every receiver: a
        // stationary pose goes stale after 2.5 s, a paused one after 15 s.
        assert_eq!(freeride_interval(false, false, Riding, 0., 20.), 2000);
        assert_eq!(freeride_interval(true, false, Riding, 0., 20.), 10_000);
        assert_eq!(freeride_interval(true, true, Airborne, 90., 20.), 10_000);
        // A wrecked rider at rest is still shown as it falls and recovers.
        assert_eq!(freeride_interval(false, false, Wreck, 0., 20.), 100);
        // No tier is shorter than the same rider's in a private room, and
        // every moving tier is inside the receivers' 400 ms cadence window.
        for state in [Riding, Airborne, Wreck, Recovering] {
            for speed in [0., 20., 60.] {
                for feet in [10., 100., 301., 999., 1000., f32::INFINITY] {
                    for (paused, moves) in [(false, true), (false, false), (true, false)] {
                        let shared = freeride_interval(paused, moves, state, speed, feet);
                        assert!(shared >= interval(paused, moves, state, speed, feet, false));
                        if !paused && (moves || state != Riding) {
                            assert!(shared <= 350);
                        }
                    }
                }
            }
        }
    }
}
