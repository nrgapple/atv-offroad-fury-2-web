//! The Free Ride hub on a rider's page (research/FREERIDE_HUB.md): the
//! Challenge button beside a nearby rider, the Accept/Decline prompt, a
//! sprint's goal, pop-up event circles, the map vote and result toasts.
//! The server owns every timer and judges the results; this side shows
//! them, counts down locally between packets and sends the rider's
//! choices. Authored for the port.
use atv_game::multiplayer::HubMessage;
use atv_game::multiplayer::NO_PEER;
use atv_game::multiplayer::hub::{
    self as wire, Challenge, Hub as HubPacket, challenge_end, challenge_op, event_kind,
    event_phase, hub_op,
};
use atv_ui::hud::hub::{Banner, Button, Offer, Toast, View, VoteOption};
use atv_ui::hud::peers::Peer;
use std::sync::atomic::{AtomicU8, Ordering};

/// The Challenge button shows within this distance of a hub rider (ft).
pub const CHALLENGE_RANGE: f32 = 100.;
/// A toast stays this long (ms).
const TOAST_MS: f64 = 6_000.;
/// A stunt-off score goes to the server at most this often (ms).
const SCORE_MS: f64 = 500.;
/// Slot colour of hub markers: gold (`PLAYER_COLORS[19]`).
const MARKER_SLOT: u8 = 19;
/// The server's challenge answer time (ms) and sprint finish radius (ft),
/// for the ring timer and the goal circle.
const OFFER_MS: f64 = 8_000.;
const REACH: f32 = 30.;

/// Test hooks (`?hubTest=`, comma-separated): `challenge` asks the nearest
/// hub rider as soon as the button shows, `accept` accepts every offer,
/// `decline` declines it, `goal` places the ATV at a sprint's goal and
/// `join` in an event's join circle.
pub static TEST: AtomicU8 = AtomicU8::new(0);
pub const TEST_CHALLENGE: u8 = 1;
pub const TEST_ACCEPT: u8 = 2;
pub const TEST_DECLINE: u8 = 4;
pub const TEST_GOAL: u8 = 8;
pub const TEST_JOIN: u8 = 16;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn parse_test(value: &str) -> u8 {
    value.split(',').fold(0, |flags, word| {
        flags
            | match word {
                "challenge" => TEST_CHALLENGE,
                "accept" => TEST_ACCEPT,
                "decline" => TEST_DECLINE,
                "goal" => TEST_GOAL,
                "join" => TEST_JOIN,
                _ => 0,
            }
    })
}

fn test(flag: u8) -> bool {
    TEST.load(Ordering::Relaxed) & flag != 0
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// `M:SS`, rounded up.
fn clock(ms: f64) -> String {
    let s = (ms.max(0.) / 1000.).ceil() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[derive(Clone, Debug, PartialEq)]
enum Duel {
    Idle,
    /// Asked `peer`; waiting until.
    Waiting {
        serial: u32,
        peer: u8,
        until: f64,
    },
    /// `peer` asks us.
    Offered {
        serial: u32,
        peer: u8,
        until: f64,
    },
    Racing {
        serial: u32,
        peer: u8,
        target: [f32; 3],
        until: f64,
    },
}

/// A finished challenge or event, for the report and tests.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub kind: &'static str,
    pub serial: u32,
    /// Challenge: the winner; event: the top three slots.
    pub winners: Vec<u8>,
    pub points: u16,
}

#[derive(Default)]
pub struct Hub {
    /// This rider's slot (from [`Hub::update`]).
    me: Option<u8>,
    /// Riders whose builds take hub packets.
    riders: u32,
    duel: Option<Duel>,
    /// The latest event packet and when it came (local ms).
    event: Option<(HubPacket, f64)>,
    /// The ride's committed stunt points when a stunt-off began, and the
    /// last report.
    stunt_base: Option<f32>,
    score_sent: (f64, f32),
    vote: Option<(HubPacket, f64)>,
    /// This rider's vote option.
    voted: Option<u8>,
    toast: Option<(Toast, f64)>,
    /// Riders this player muted or blocked: their challenges are declined
    /// at once (research/QUICK_CHAT.md owns the list).
    pub blocked: u32,
    outbox: Vec<HubMessage>,
    /// The nearest hub rider in Challenge range.
    nearby: Option<u8>,
    /// A test placement (goal or circle) already made for this serial.
    placed: u32,
    pub outcomes: Vec<Outcome>,
    /// Hub points earned in this room.
    pub points: u32,
    /// Countdown beeps and result cues for the ride's sound (names).
    pub cues: Vec<&'static str>,
}

impl Hub {
    fn duel(&self) -> &Duel {
        self.duel.as_ref().unwrap_or(&Duel::Idle)
    }

    fn send_challenge(&mut self, op: u8, serial: u32, peer: u8) {
        self.outbox.push(HubMessage::Challenge(Challenge {
            op,
            serial,
            peer,
            ..Default::default()
        }));
    }

    /// Riders whose builds take hub packets (0 before the server names
    /// any, or without a hub).
    pub fn riders(&self) -> u32 {
        self.riders
    }

    /// Nothing to show.
    pub fn idle(&self) -> bool {
        self.nearby.is_none()
            && self.duel.is_none()
            && self.event.is_none()
            && self.vote.is_none()
            && self.toast.is_none()
    }

    /// Packets for the server, oldest first.
    pub fn take_outbox(&mut self) -> Vec<HubMessage> {
        std::mem::take(&mut self.outbox)
    }

    /// A new socket: the server sends the state again.
    pub fn reset(&mut self) {
        let keep = (
            self.blocked,
            std::mem::take(&mut self.outcomes),
            self.points,
        );
        *self = Hub {
            blocked: keep.0,
            outcomes: keep.1,
            points: keep.2,
            ..Default::default()
        };
    }

    fn show(&mut self, toast: Toast, now: f64) {
        self.toast = Some((toast, now + TOAST_MS));
    }

    /// One message from the server, at local time `now` (ms).
    pub fn receive(&mut self, message: HubMessage, now: f64, name: &dyn Fn(u8) -> String) {
        match message {
            HubMessage::Hub(hub) => match hub.op {
                hub_op::RIDERS => self.riders = hub.mask,
                hub_op::EVENT => self.event_message(hub, now, name),
                hub_op::VOTE => {
                    if hub.maps.is_empty() || hub.choice != wire::NO_CHOICE {
                        self.vote = None;
                        self.voted = None;
                    } else {
                        if self
                            .vote
                            .as_ref()
                            .is_none_or(|(v, _)| v.serial != hub.serial)
                        {
                            self.voted = None;
                        }
                        self.vote = Some((hub, now));
                    }
                }
                _ => {}
            },
            HubMessage::Challenge(c) => self.challenge_message(c, now, name),
        }
    }

    fn challenge_message(&mut self, c: Challenge, now: f64, name: &dyn Fn(u8) -> String) {
        let until = now + f64::from(c.timer_ms);
        match c.op {
            challenge_op::OFFER => {
                if self.blocked & (1 << c.peer) != 0 || test(TEST_DECLINE) {
                    return self.send_challenge(challenge_op::DECLINE, c.serial, c.peer);
                }
                if !matches!(self.duel(), Duel::Idle) {
                    return self.send_challenge(challenge_op::DECLINE, c.serial, c.peer);
                }
                self.duel = Some(Duel::Offered {
                    serial: c.serial,
                    peer: c.peer,
                    until,
                });
                self.cues.push("offer");
                if test(TEST_ACCEPT) {
                    self.press(Button::Accept, now);
                }
            }
            challenge_op::WAIT => {
                self.duel = Some(Duel::Waiting {
                    serial: c.serial,
                    peer: c.peer,
                    until,
                });
            }
            challenge_op::START => {
                self.duel = Some(Duel::Racing {
                    serial: c.serial,
                    peer: c.peer,
                    target: c.target,
                    until,
                });
                self.cues.push("go");
            }
            challenge_op::RESULT => {
                self.duel = None;
                let won = c.detail != NO_PEER && c.detail != c.peer;
                let title = if c.detail == NO_PEER {
                    "TIME UP"
                } else if won {
                    "YOU WIN!"
                } else {
                    "SO CLOSE"
                };
                let points = if won { 50 } else { 10 };
                self.points += u32::from(points);
                self.outcomes.push(Outcome {
                    kind: "challenge",
                    serial: c.serial,
                    winners: vec![c.detail],
                    points,
                });
                self.cues.push(if won { "win" } else { "lose" });
                self.show(
                    Toast {
                        title: title.into(),
                        rows: vec![(String::new(), format!("vs {}", name(c.peer)), points)],
                        win: won,
                    },
                    now,
                );
            }
            challenge_op::CLOSED => {
                let was_mine = matches!(self.duel(), Duel::Waiting { .. });
                self.duel = None;
                let title = match c.detail {
                    challenge_end::DECLINED if was_mine => "DECLINED",
                    challenge_end::EXPIRED if was_mine => "NO ANSWER",
                    challenge_end::BUSY => "BUSY",
                    challenge_end::COOLDOWN => "WAIT A BIT",
                    challenge_end::UNAVAILABLE => "TOO FAR",
                    challenge_end::LEFT => "RIDER LEFT",
                    _ => return,
                };
                self.show(
                    Toast {
                        title: title.into(),
                        rows: Vec::new(),
                        win: false,
                    },
                    now,
                );
            }
            _ => {}
        }
    }

    fn event_message(&mut self, hub: HubPacket, now: f64, name: &dyn Fn(u8) -> String) {
        match hub.phase {
            event_phase::RESULTS => {
                self.event = None;
                self.stunt_base = None;
                let mine = self.mine_in(&hub);
                let rows = hub
                    .rows
                    .iter()
                    .take(3)
                    .enumerate()
                    .map(|(i, r)| (format!("{}", i + 1), name(r.peer), r.points))
                    .collect();
                if let Some(points) = mine {
                    self.points += u32::from(points);
                    self.outcomes.push(Outcome {
                        kind: event_name(hub.kind),
                        serial: hub.serial,
                        winners: hub.rows.iter().take(3).map(|r| r.peer).collect(),
                        points,
                    });
                    self.cues.push("win");
                    let solo = hub.choice & wire::SOLO != 0;
                    self.show(
                        Toast {
                            title: if solo {
                                "SOLO SCORE".into()
                            } else {
                                format!("{} RESULTS", event_name(hub.kind).to_uppercase())
                            },
                            rows,
                            win: hub.rows.first().is_some_and(|r| Some(r.peer) == self.me),
                        },
                        now,
                    );
                }
            }
            event_phase::CANCELLED => {
                self.event = None;
                self.stunt_base = None;
            }
            _ => {
                if hub.phase == event_phase::RUN
                    && self
                        .event
                        .as_ref()
                        .is_some_and(|(e, _)| e.phase == event_phase::JOIN)
                    && self.joined(&hub)
                {
                    self.cues.push("go");
                }
                self.event = Some((hub, now));
            }
        }
    }

    /// Points this rider earned in a results packet, when it played.
    fn mine_in(&self, hub: &HubPacket) -> Option<u16> {
        let me = self.me?;
        hub.rows.iter().find(|r| r.peer == me).map(|r| r.points)
    }

    fn joined(&self, hub: &HubPacket) -> bool {
        self.me.is_some_and(|me| hub.mask & (1 << me) != 0)
    }

    /// A HUD button (or its key) at local time `now`.
    pub fn press(&mut self, button: Button, now: f64) {
        match (button, self.duel().clone()) {
            (Button::Challenge, Duel::Idle) => {
                if let Some(peer) = self.nearby {
                    self.send_challenge(challenge_op::ASK, 0, peer);
                    // Shown as waiting until the server answers.
                    self.duel = Some(Duel::Waiting {
                        serial: 0,
                        peer,
                        until: now + 2_000.,
                    });
                }
            }
            (Button::Accept, Duel::Offered { serial, peer, .. }) => {
                self.send_challenge(challenge_op::ACCEPT, serial, peer);
                self.duel = Some(Duel::Waiting {
                    serial,
                    peer,
                    until: now + 2_000.,
                });
            }
            (Button::Decline, Duel::Offered { serial, peer, .. }) => {
                self.send_challenge(challenge_op::DECLINE, serial, peer);
                self.duel = None;
            }
            (Button::Vote(choice), _) => {
                if let Some((vote, _)) = &self.vote
                    && usize::from(choice) < vote.maps.len()
                {
                    self.voted = Some(choice);
                    self.outbox.push(HubMessage::Hub(HubPacket {
                        op: hub_op::CAST,
                        serial: vote.serial,
                        choice,
                        ..Default::default()
                    }));
                }
            }
            _ => {}
        }
    }

    /// Per frame: the nearby rider, expired local timers, a stunt-off's
    /// score report. `me` is this rider's slot, `origin` its ATV, `others`
    /// the presented riders, `stunts` the ride's committed stunt points.
    /// Returns a test placement (origin) when a hook asks for one.
    pub fn update(
        &mut self,
        now: f64,
        me: Option<u8>,
        origin: Option<[f32; 3]>,
        others: &[Peer],
        stunts: f32,
    ) -> Option<[f32; 3]> {
        self.me = me;
        // A local timer ran out before the server's word: stop showing it.
        if let Some(Duel::Offered { until, .. } | Duel::Waiting { until, .. }) = self.duel
            && now > until + 1_500.
        {
            self.duel = None;
        }
        if self.toast.as_ref().is_some_and(|(_, until)| now > *until) {
            self.toast = None;
        }
        let in_event = self
            .event
            .as_ref()
            .is_some_and(|(e, _)| e.phase == event_phase::RUN && self.joined(e));
        self.nearby = None;
        if let Some(origin) = origin
            && matches!(self.duel(), Duel::Idle)
            && !in_event
            && self.vote.is_none()
        {
            self.nearby = others
                .iter()
                .filter(|p| !p.stale && Some(p.slot) != me)
                .filter(|p| self.riders & (1 << p.slot) != 0 && self.blocked & (1 << p.slot) == 0)
                .map(|p| (p.slot, flat(p.position, origin)))
                .filter(|(_, d)| *d <= CHALLENGE_RANGE)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(slot, _)| slot);
            if self.nearby.is_some() && test(TEST_CHALLENGE) {
                self.press(Button::Challenge, now);
            }
        }
        // Stunt-off: report the points since GO, twice a second at most.
        if let Some((event, _)) = &self.event
            && in_event
            && event.kind == event_kind::STUNTS
        {
            let base = *self.stunt_base.get_or_insert(stunts);
            let score = (stunts - base).max(0.);
            if score != self.score_sent.1 && now - self.score_sent.0 >= SCORE_MS {
                self.score_sent = (now, score);
                self.outbox.push(HubMessage::Hub(HubPacket {
                    op: hub_op::SCORE,
                    serial: event.serial,
                    value: score.min(wire::MAX_HUB_VALUE),
                    ..Default::default()
                }));
            }
        }
        // Test hooks: stand at the goal or in the circle, once per serial.
        if let Some(Duel::Racing { serial, target, .. }) = self.duel
            && test(TEST_GOAL)
            && self.placed != serial
        {
            self.placed = serial;
            return Some(target);
        }
        if let Some((event, _)) = &self.event
            && self.placed != event.serial
        {
            let place = match event.phase {
                event_phase::JOIN if test(TEST_JOIN) => Some(event.point),
                event_phase::RUN
                    if test(TEST_GOAL)
                        && self.joined(event)
                        && event.kind != event_kind::STUNTS =>
                {
                    Some(event.target)
                }
                _ => None,
            };
            if place.is_some() {
                self.placed = event.serial;
                return place;
            }
        }
        None
    }

    /// The HUD this frame.
    pub fn view(
        &self,
        now: f64,
        origin: Option<[f32; 3]>,
        name: &dyn Fn(u8) -> String,
        venue: &dyn Fn(u16) -> String,
    ) -> Option<View> {
        let mut view = View {
            challenge: self.nearby.map(name),
            ..Default::default()
        };
        let distance = |p: [f32; 3]| origin.map(|o| format!("{} ft", flat(o, p).round() as i32));
        match self.duel() {
            Duel::Offered { peer, until, .. } => {
                view.offer = Some(Offer {
                    name: name(*peer),
                    left: ((until - now) / OFFER_MS).clamp(0., 1.) as f32,
                    seconds: ((until - now) / 1000.).ceil().max(0.) as u32,
                });
            }
            Duel::Waiting { peer, until, .. } => {
                view.banner = Some(Banner {
                    title: "WAITING".into(),
                    line: name(*peer),
                    clock: clock(until - now),
                    low: false,
                });
            }
            Duel::Racing {
                peer,
                target,
                until,
                ..
            } => {
                view.banner = Some(Banner {
                    title: format!("RACE {}", name(*peer)),
                    line: distance(*target).unwrap_or_default(),
                    clock: clock(until - now),
                    low: until - now < 10_000.,
                });
            }
            Duel::Idle => {}
        }
        if view.banner.is_none()
            && let Some((event, at)) = &self.event
        {
            let left = f64::from(event.timer_ms) - (now - at);
            let joined = self.joined(event);
            let title = event_name(event.kind).to_uppercase();
            view.banner = Some(match event.phase {
                event_phase::JOIN => Banner {
                    title,
                    line: if joined {
                        format!("IN · {}", event.mask.count_ones())
                    } else {
                        distance(event.point).map_or("JOIN".into(), |d| format!("JOIN · {d}"))
                    },
                    clock: clock(left),
                    low: left < 10_000.,
                },
                _ if !joined => Banner {
                    title,
                    line: "LIVE".into(),
                    clock: clock(left),
                    low: false,
                },
                _ => Banner {
                    title,
                    line: match event.kind {
                        event_kind::STUNTS => {
                            let me = self
                                .me
                                .and_then(|me| event.rows.iter().position(|r| r.peer == me));
                            me.map_or(String::new(), |p| format!("PLACE {}", p + 1))
                        }
                        _ => distance(event.target).unwrap_or_default(),
                    },
                    clock: clock(left),
                    low: left < 10_000.,
                },
            });
        }
        if let Some((vote, at)) = &self.vote {
            let left = f64::from(vote.timer_ms) - (now - at);
            view.vote = Some((
                vote.maps
                    .iter()
                    .zip(&vote.votes)
                    .enumerate()
                    .map(|(i, (map, votes))| VoteOption {
                        name: venue(*map),
                        votes: *votes,
                        mine: self.voted == Some(i as u8),
                    })
                    .collect(),
                clock(left),
            ));
        }
        view.toast = self.toast.as_ref().map(|(t, _)| t.clone());
        (view != View::default()).then_some(view)
    }

    /// Map markers and screen tags: a sprint's goal, an event's circle and
    /// target.
    pub fn markers(&self) -> Vec<Peer> {
        let tag = |name: &str, p: [f32; 3]| Peer {
            slot: MARKER_SLOT,
            name: name.into(),
            // screen_marker lifts tags 7 ft.
            position: p,
            stale: false,
            pill: None,
            map_only: false,
        };
        let mut out = Vec::new();
        if let Duel::Racing { target, .. } = self.duel() {
            out.push(tag("GOAL", *target));
        }
        if let Some((event, _)) = &self.event {
            match event.phase {
                event_phase::JOIN => out.push(tag("JOIN", event.point)),
                _ if self.joined(event) && event.kind == event_kind::SPRINT => {
                    out.push(tag("GOAL", event.target))
                }
                _ if self.joined(event) && event.kind == event_kind::HILL => {
                    out.push(tag("HILL", event.target))
                }
                _ => {}
            }
        }
        out
    }

    /// The glowing ground circle to draw: centre and radius (ft).
    pub fn ring(&self) -> Option<([f32; 3], f32)> {
        if let Duel::Racing { target, .. } = self.duel() {
            return Some((*target, REACH));
        }
        let (event, _) = self.event.as_ref()?;
        match event.phase {
            event_phase::JOIN => Some((event.point, event.radius)),
            event_phase::RUN if self.joined(event) && event.kind != event_kind::STUNTS => {
                Some((event.target, event.radius))
            }
            _ => None,
        }
    }

    /// Status for tests (`window.furyMultiplayer.hub`).
    pub fn report(&self) -> serde_json::Value {
        let duel = match self.duel() {
            Duel::Idle => serde_json::json!({"state": "idle"}),
            Duel::Waiting { serial, peer, .. } => {
                serde_json::json!({"state": "waiting", "serial": serial, "peer": peer})
            }
            Duel::Offered { serial, peer, .. } => {
                serde_json::json!({"state": "offered", "serial": serial, "peer": peer})
            }
            Duel::Racing {
                serial,
                peer,
                target,
                ..
            } => serde_json::json!({
                "state": "racing", "serial": serial, "peer": peer, "target": target,
            }),
        };
        serde_json::json!({
            "riders": self.riders,
            "nearby": self.nearby,
            "duel": duel,
            "event": self.event.as_ref().map(|(e, _)| serde_json::json!({
                "serial": e.serial, "kind": event_name(e.kind), "phase": e.phase,
                "point": e.point, "target": e.target, "mask": e.mask,
                "solo": e.choice & wire::SOLO != 0,
            })),
            "vote": self.vote.as_ref().map(|(v, _)| serde_json::json!({
                "serial": v.serial, "maps": v.maps, "votes": v.votes, "mine": self.voted,
            })),
            "toast": self.toast.as_ref().map(|(t, _)| t.title.clone()),
            "points": self.points,
            "outcomes": self.outcomes.iter().map(|o| serde_json::json!({
                "kind": o.kind, "serial": o.serial, "winners": o.winners, "points": o.points,
            })).collect::<Vec<_>>(),
        })
    }
}

/// The rotation's venues by name before Tracks.dat's names have loaded
/// (research/FREESTYLE_MAPS.md "Environments").
pub fn venue_name(environment: &str) -> Option<&'static str> {
    Some(match environment {
        "env01" => "Caribou Peak",
        "env02" => "Tioga Valley",
        "env03" => "Sonora Dunes",
        "env04" => "Freeport Harbor",
        "env05" => "Prospect Springs",
        "env06" => "Rockbridge Park",
        "env07" => "San Jacinto Isles",
        "env08" => "Arlington Stream",
        _ => return None,
    })
}

/// Keyboard: G challenges, Y accepts, N declines, 1-3 vote.
pub fn key_button(code: &str) -> Option<Button> {
    Some(match code {
        "KeyG" => Button::Challenge,
        "KeyY" => Button::Accept,
        "KeyN" => Button::Decline,
        "Digit1" => Button::Vote(0),
        "Digit2" => Button::Vote(1),
        "Digit3" => Button::Vote(2),
        _ => return None,
    })
}

/// Short event names (1-2 words, research/MULTIPLAYER_EXPERIENCE.md "UI").
pub fn event_name(kind: u8) -> &'static str {
    match kind {
        event_kind::SPRINT => "Sprint",
        event_kind::HILL => "Hill King",
        _ => "Stunt-Off",
    }
}
