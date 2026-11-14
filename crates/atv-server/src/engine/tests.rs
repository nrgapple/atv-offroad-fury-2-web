use super::*;
use atv_net::{CpuMotion, CpuRider, GameClaim, Packet, Participant, decode, encode};

/// Deterministic randomness: counts up.
struct Counter(u8);
impl Entropy for Counter {
    fn fill(&mut self, out: &mut [u8]) {
        for b in out {
            self.0 = self.0.wrapping_add(1);
            *b = self.0;
        }
    }
}

const CODE: RoomCode = *b"ABCD2345";
const HOST_KEY: &str = "00112233445566778899aabbccddeeff";

fn player(name: &str) -> Participant {
    Participant {
        id: 0,
        name: name.into(),
        atv_model: 5,
        rider_model: 0,
        appearance: [0; 8],
    }
}

fn bytes(packet: &Packet) -> Bytes {
    Bytes::from(encode(packet).unwrap())
}

/// Packets sent to `to`, decoded.
fn sent(out: &[Out], to: ConnId) -> Vec<Packet> {
    out.iter()
        .filter_map(|o| match o {
            Out::Send(id, b) | Out::Pose(id, b) if *id == to => Some(decode(b).unwrap()),
            _ => None,
        })
        .collect()
}

fn closes(out: &[Out]) -> Vec<(ConnId, u16, &'static str)> {
    out.iter()
        .filter_map(|o| match o {
            Out::Close(id, code, reason) => Some((*id, *code, *reason)),
            _ => None,
        })
        .collect()
}

struct Fixture {
    room: Room,
    rng: Counter,
    now: u64,
}

impl Fixture {
    fn new(public: bool) -> Self {
        Fixture {
            room: Room::new(
                if public {
                    atv_net::public_room_code(1).unwrap()
                } else {
                    CODE
                },
                HOST_KEY.into(),
                public,
                1_000,
            ),
            rng: Counter(0),
            now: 1_000,
        }
    }
    /// Connects `id` with `key` and joins as `name`; returns the slot.
    fn join(&mut self, id: ConnId, key: Option<&str>, name: &str) -> (u8, Vec<Out>) {
        let mut out = Vec::new();
        let peer = self
            .room
            .admit(id, key, self.now, &mut self.rng, &mut out)
            .unwrap();
        let code = self.room.code();
        self.room.receive(
            id,
            bytes(&Packet::Join {
                code,
                player: player(name),
            }),
            self.now,
            &mut out,
        );
        (peer, out)
    }
    fn send(&mut self, id: ConnId, packet: &Packet) -> Vec<Out> {
        let mut out = Vec::new();
        self.room.receive(id, bytes(packet), self.now, &mut out);
        out
    }
    fn ready(&mut self, id: ConnId) -> Vec<Out> {
        let map = self.room.lobby().map;
        self.send(id, &Packet::Ready { map })
    }
    fn hello(&mut self, id: ConnId, flags: u8) {
        let peer = self.room.conns[self.room.index(id).unwrap()].peer;
        self.send(
            id,
            &Packet::Signal(atv_net::Signal {
                peer,
                kind: signal::HELLO,
                data: vec![flags],
            }),
        );
    }
    fn tick(&mut self) -> Vec<Out> {
        let mut out = Vec::new();
        self.room.tick(self.now, &mut out);
        out
    }
    fn housekeep(&mut self) -> (Option<&'static str>, Vec<Out>) {
        let mut out = Vec::new();
        let ended = self.room.housekeep(self.now, &mut out);
        (ended, out)
    }
}

fn pose(peer: u8, sequence: u32, x: f32, moving: bool) -> Snapshot {
    Snapshot {
        peer,
        sequence,
        time_ms: sequence * 50,
        atv_model: 5,
        rider_model: 0,
        animation: Some(Box::default()),
        velocity: if moving { [30., 0., 0.] } else { [0.; 3] },
        atv: atv_net::Transform {
            position: [x, 0., 0.],
            ..Default::default()
        },
        ..Default::default()
    }
}

fn snapshot(peer: u8, sequence: u32, x: f32) -> Packet {
    Packet::Snapshot(Box::new(pose(peer, sequence, x, true)))
}

#[test]
fn creator_joins_with_the_host_key_and_others_get_the_roster() {
    let mut f = Fixture::new(false);
    let (peer, out) = f.join(1, Some(HOST_KEY), "Host");
    assert_eq!(peer, 0);
    let Packet::Welcome {
        peer, lobby, key, ..
    } = &sent(&out, 1)[0]
    else {
        panic!("no welcome");
    };
    assert_eq!((*peer, lobby.host, lobby.players.len()), (0, 0, 1));
    assert_eq!(*key, key_bytes(HOST_KEY));
    let (peer, out) = f.join(2, None, "Guest");
    assert_eq!(peer, 1);
    // The host hears the slot cleared, then the roster with the guest.
    let to_host = sent(&out, 1);
    assert_eq!(to_host[0], Packet::Leave { peer: 1 });
    let Packet::Lobby(lobby) = &to_host[1] else {
        panic!("no lobby");
    };
    assert_eq!(lobby.players.len(), 2);
    assert!(matches!(sent(&out, 2)[0], Packet::Welcome { peer: 1, .. }));
    // Private rooms hold four riders.
    f.join(3, None, "C");
    f.join(4, None, "D");
    let mut out = Vec::new();
    assert_eq!(
        f.room.admit(5, None, f.now, &mut f.rng, &mut out),
        Err(Refused {
            status: 409,
            message: "Game is full!"
        })
    );
    assert_eq!(f.room.connections(), 4);
}

#[test]
fn a_dropped_rider_is_held_and_rejoins_its_slot_with_its_key() {
    let mut f = Fixture::new(false);
    f.join(1, Some(HOST_KEY), "Host");
    let (_, out) = f.join(2, None, "Guest");
    let Packet::Welcome { key, .. } = &sent(&out, 2)[0] else {
        panic!("no welcome");
    };
    let key: String = key.iter().map(|b| format!("{b:02x}")).collect();
    let mut out = Vec::new();
    f.room.disconnected(2, 1006, "", f.now, &mut out);
    // Still in the roster, no Leave.
    let to_host = sent(&out, 1);
    assert!(!to_host.contains(&Packet::Leave { peer: 1 }));
    assert_eq!(f.room.riders(), 2);
    // A stranger cannot take the held slot.
    let mut out = Vec::new();
    assert_eq!(f.room.admit(3, None, f.now, &mut f.rng, &mut out), Ok(2));
    f.room.disconnected(3, 1000, "", f.now, &mut out);
    f.now += 10_000;
    let (peer, out) = f.join(4, Some(&key), "Guest");
    assert_eq!(peer, 1);
    assert!(matches!(sent(&out, 4)[0], Packet::Welcome { peer: 1, .. }));
    // A key also replaces its own live connection.
    let mut out = Vec::new();
    assert_eq!(
        f.room.admit(5, Some(&key), f.now, &mut f.rng, &mut out),
        Ok(1)
    );
    assert_eq!(closes(&out), vec![(4, 1000, "Connection replaced")]);
    f.send(
        5,
        &Packet::Join {
            code: CODE,
            player: player("Guest"),
        },
    );
    // Held too long: released, everyone hears it.
    f.room.disconnected(5, 1001, "", f.now, &mut out);
    f.now += GRACE_MS;
    let (ended, out) = f.housekeep();
    assert_eq!(ended, None);
    let to_host = sent(&out, 1);
    assert_eq!(to_host[0], Packet::Leave { peer: 1 });
    assert_eq!(f.room.riders(), 1);
}

#[test]
fn the_host_role_passes_on_when_the_host_leaves() {
    let mut f = Fixture::new(false);
    f.join(1, Some(HOST_KEY), "Host");
    f.join(2, None, "B");
    f.join(3, None, "C");
    let out = f.send(1, &Packet::Leave { peer: 0 });
    assert_eq!(closes(&out), vec![(1, 1000, "Left room")]);
    let to_b = sent(&out, 2);
    assert_eq!(to_b[0], Packet::Leave { peer: 0 });
    let Packet::Lobby(lobby) = &to_b[1] else {
        panic!("no lobby");
    };
    assert_eq!(lobby.host, 1);
    // Only the host changes the map; a stale epoch is answered with the lobby.
    let epoch = lobby.epoch;
    let out = f.send(
        3,
        &Packet::SetLobby {
            epoch,
            map: 9,
            started: true,
        },
    );
    assert!(matches!(&sent(&out, 3)[0], Packet::Lobby(l) if l.map == 1));
    let out = f.send(
        2,
        &Packet::SetLobby {
            epoch: epoch + 7,
            map: 9,
            started: true,
        },
    );
    assert!(matches!(&sent(&out, 2)[0], Packet::Lobby(l) if l.map == 1));
    let out = f.send(
        2,
        &Packet::SetLobby {
            epoch,
            map: 9,
            started: true,
        },
    );
    assert!(matches!(&sent(&out, 3)[0], Packet::Lobby(l) if l.map == 9 && l.started));
}

#[test]
fn free_ride_claims_fill_a_shared_room_and_hold_unused_claims_briefly() {
    let mut f = Fixture::new(true);
    assert_eq!(f.room.lobby().map, PUBLIC_MAP);
    assert!(f.room.lobby().started);
    let mut keys = Vec::new();
    for _ in 1..PUBLIC_CAPACITY {
        keys.push(f.room.claim(f.now, &mut f.rng).unwrap());
    }
    assert_eq!(f.room.claim(f.now, &mut f.rng), None);
    // A claimant takes its own slot with its key.
    let (peer, _) = f.join(1, Some(&keys[4]), "Rider");
    assert_eq!(peer, 5);
    // Names are filtered for strangers; the map is fixed.
    let out = f.send(
        1,
        &Packet::SetLobby {
            epoch: f.room.lobby().epoch,
            map: 9,
            started: true,
        },
    );
    assert!(matches!(&sent(&out, 1)[0], Packet::Lobby(l) if l.map == PUBLIC_MAP));
    // Unused claims expire with the hold; the room keeps its rider.
    f.now += GRACE_MS;
    f.housekeep();
    assert!(f.room.claim(f.now, &mut f.rng).is_some());
    assert_eq!(f.room.riders(), 1);
}

#[test]
fn builds_without_frames_get_each_pose_and_frame_builds_get_one_frame_per_tick() {
    let mut f = Fixture::new(true);
    let k1 = f.room.claim(f.now, &mut f.rng).unwrap();
    let k2 = f.room.claim(f.now, &mut f.rng).unwrap();
    f.join(1, Some(HOST_KEY), "Old");
    f.join(2, Some(&k1), "New");
    f.join(3, Some(&k2), "Sender");
    f.hello(2, signal::FRAMES | signal::CPU_SOUNDS);
    for id in 1..=3 {
        f.ready(id);
    }
    let out = f.send(3, &snapshot(2, 10, 0.));
    // The old build gets the pose at once, stamped; the frame build waits.
    assert!(matches!(&sent(&out, 1)[0], Packet::Snapshot(s) if s.peer == 2 && s.sequence == 10));
    assert!(sent(&out, 2).is_empty());
    assert!(f.room.take_counters().poses[2] == 1);
    let out = f.tick();
    let frames = sent(&out, 2);
    let Packet::Frame { poses } = &frames[0] else {
        panic!("no frame: {frames:?}");
    };
    assert_eq!(poses.len(), 1);
    assert!(matches!(decode(&poses[0]).unwrap(), Packet::Snapshot(s) if s.peer == 2));
    // Only frame builds get frames.
    assert!(sent(&out, 1).is_empty() && sent(&out, 3).is_empty());
    // An unchanged pose is never sent again; an older one is ignored.
    f.now += 50;
    assert!(f.tick().is_empty());
    let out = f.send(3, &snapshot(2, 9, 5.));
    assert!(out.is_empty());
    // A far rider is due less often.
    f.send(2, &snapshot(1, 1, 500.));
    f.send(3, &snapshot(2, 11, 0.));
    f.now += 50;
    assert!(sent(&f.tick(), 2).is_empty());
    f.now += 150;
    assert_eq!(sent(&f.tick(), 2).len(), 1);
    assert_eq!(f.room.take_counters().frames, 2);
}

#[test]
fn arrivals_catch_up_on_every_rider_and_deltas_apply_to_the_baseline() {
    let mut f = Fixture::new(true);
    let k1 = f.room.claim(f.now, &mut f.rng).unwrap();
    f.join(1, Some(HOST_KEY), "A");
    f.ready(1);
    f.send(1, &snapshot(0, 5, 3.));
    // A delta before any baseline waits quietly for the next snapshot.
    f.join(2, Some(&k1), "B");
    let base = pose(1, 1, 0., true);
    let mut next = pose(1, 2, 7., true);
    let delta = atv_net::delta(&next, &base).unwrap();
    assert!(
        f.send(2, &Packet::Delta(Box::new(delta.clone())))
            .is_empty()
    );
    let out = f.ready(2);
    assert!(matches!(&sent(&out, 2)[0], Packet::Snapshot(s) if s.peer == 0 && s.sequence == 5));
    f.send(2, &Packet::Snapshot(Box::new(base)));
    let out = f.send(2, &Packet::Delta(Box::new(delta)));
    let Packet::Snapshot(s) = &sent(&out, 1)[0] else {
        panic!("delta not forwarded");
    };
    assert_eq!((s.sequence, s.atv.position[0]), (2, 7.));
    // A delta against the wrong baseline ends the session.
    next.sequence = 3;
    let mut bad = atv_net::delta(&next, &pose(1, 99, 0., true)).unwrap();
    bad.base_sequence = 77;
    let out = f.send(2, &Packet::Delta(Box::new(bad)));
    assert_eq!(closes(&out), vec![(2, 1008, "Invalid snapshot baseline")]);
}

#[test]
fn riders_that_never_got_a_baseline_get_whole_poses_built_from_deltas() {
    // A delta is relative to the sender's own last pose, which a rider that
    // joins (or comes into range) later never got. The server applies it
    // and sends the resulting compact snapshot, in catch-up and in frames.
    let mut f = Fixture::new(true);
    let k1 = f.room.claim(f.now, &mut f.rng).unwrap();
    let k2 = f.room.claim(f.now, &mut f.rng).unwrap();
    f.join(1, Some(HOST_KEY), "Sender");
    f.ready(1);
    let base = pose(0, 1, 0., true);
    f.send(1, &Packet::Snapshot(Box::new(base.clone())));
    let first = pose(0, 2, 7., true);
    f.send(
        1,
        &Packet::Delta(Box::new(atv_net::delta(&first, &base).unwrap())),
    );
    f.join(2, Some(&k1), "Late");
    f.hello(2, signal::FRAMES);
    let out = f.ready(2);
    let Some(Out::Pose(_, raw)) = out.iter().find(|o| matches!(o, Out::Pose(2, _))) else {
        panic!("no catch-up");
    };
    assert_eq!(raw[5], 9, "catch-up is a compact snapshot");
    assert!(
        matches!(decode(raw).unwrap(), Packet::Snapshot(s) if s.sequence == 2 && s.atv.position[0] == 7.)
    );
    f.join(3, Some(&k2), "Later");
    f.hello(3, signal::FRAMES);
    f.ready(3);
    let second = pose(0, 3, 9., true);
    f.send(
        1,
        &Packet::Delta(Box::new(atv_net::delta(&second, &first).unwrap())),
    );
    let out = f.tick();
    for id in [2, 3] {
        let Packet::Frame { poses } = &sent(&out, id)[0] else {
            panic!("no frame for {id}");
        };
        assert_eq!(
            poses[0][5], 9,
            "a frame carries a compact snapshot, not a delta"
        );
        assert!(
            matches!(decode(&poses[0]).unwrap(), Packet::Snapshot(s) if s.sequence == 3 && s.atv.position[0] == 9.)
        );
    }
}

#[test]
fn poses_from_a_previous_vehicle_or_before_ready_are_not_passed_on() {
    let mut f = Fixture::new(true);
    let k1 = f.room.claim(f.now, &mut f.rng).unwrap();
    f.join(1, Some(HOST_KEY), "A");
    f.join(2, Some(&k1), "B");
    f.ready(2);
    assert!(f.send(1, &snapshot(0, 1, 0.)).is_empty());
    f.ready(1);
    let mut other = pose(0, 2, 0., true);
    other.atv_model = 6;
    assert!(f.send(1, &Packet::Snapshot(Box::new(other))).is_empty());
    assert_eq!(sent(&f.send(1, &snapshot(0, 3, 0.)), 2).len(), 1);
    // A new vehicle: the rider reloads, and its old pose is gone.
    let mut profile = player("A");
    profile.atv_model = 6;
    f.send(1, &Packet::Profile(profile));
    assert!(f.send(1, &snapshot(0, 4, 0.)).is_empty());
}

#[test]
fn host_games_and_cpus_reach_the_riders_that_understand_them() {
    let mut f = Fixture::new(false);
    f.join(1, Some(HOST_KEY), "Host");
    f.join(2, None, "Old");
    f.join(3, None, "New");
    let epoch = f.room.lobby().epoch;
    f.send(
        1,
        &Packet::SetLobby {
            epoch,
            map: 23,
            started: true,
        },
    );
    for id in 1..=3 {
        f.ready(id);
    }
    // Rider 3 races and hears CPU sounds; rider 2 races without them.
    f.hello(3, signal::CPU_SOUNDS);
    for (id, peer) in [(2, 1), (3, 2)] {
        f.send(
            id,
            &Packet::Progress(atv_net::RaceProgress {
                peer,
                serial: 0,
                ..Default::default()
            }),
        );
    }
    let cpu = |index: u8| Snapshot {
        peer: atv_net::CPU_PEER + index,
        sequence: 3,
        animation: Some(Box::default()),
        ..Default::default()
    };
    let motion = CpuMotion {
        serial: 9,
        riders: vec![CpuRider {
            index: 0,
            lap: 1,
            distance: 10.,
            finish_ms: 0,
            pose: cpu(0),
            sound: Some(Default::default()),
        }],
    };
    let out = f.send(1, &Packet::Cpus(Box::new(motion.clone())));
    assert!(matches!(&sent(&out, 2)[0], Packet::Cpus(m) if m.riders[0].sound.is_none()));
    assert!(matches!(&sent(&out, 3)[0], Packet::Cpus(m) if m.riders[0].sound.is_some()));
    // A claim goes to the host alone, stamped with its sender.
    f.send(
        1,
        &Packet::Claim(GameClaim {
            peer: 0,
            serial: 0,
            kind: 0,
            index: 0,
            points: 0.,
            stunt: 0.,
        }),
    );
    let claim = GameClaim {
        peer: 9,
        serial: 4,
        kind: atv_net::claim_kind::STUNT,
        index: 0,
        points: 10.,
        stunt: 10.,
    };
    let out = f.send(2, &Packet::Claim(claim));
    assert!(matches!(&sent(&out, 1)[0], Packet::Claim(c) if c.peer == 1));
    assert!(sent(&out, 3).is_empty());
    // Only the host sends CPUs.
    let out = f.send(3, &Packet::Cpus(Box::new(motion)));
    assert_eq!(closes(&out), vec![(3, 1008, "Message not permitted")]);
}

#[test]
fn pings_answer_with_server_time_and_misbehaviour_closes() {
    let mut f = Fixture::new(false);
    f.join(1, Some(HOST_KEY), "Host");
    f.now = 1_234_567;
    let out = f.send(1, &Packet::Ping { nonce: 7 });
    assert_eq!(
        sent(&out, 1),
        vec![Packet::Pong {
            nonce: 7,
            server_ms: 1_234_567
        }]
    );
    // Link traffic from older builds is ignored, not refused.
    assert!(f.send(1, &Packet::Links { mask: 2 }).is_empty());
    // An older protocol version is told to reload.
    let mut out = Vec::new();
    let mut old = encode(&Packet::Ping { nonce: 1 }).unwrap();
    old[4] = atv_net::VERSION - 1;
    f.room.receive(1, Bytes::from(old), f.now, &mut out);
    assert_eq!(
        closes(&out),
        vec![(
            1,
            1008,
            "Game updated. Reload the page to join multiplayer."
        )]
    );
    // Unjoined sockets may only join.
    let mut out = Vec::new();
    f.room.admit(2, None, f.now, &mut f.rng, &mut out).unwrap();
    let out = f.send(2, &Packet::Ping { nonce: 1 });
    assert_eq!(closes(&out), vec![(2, 1008, "Message not permitted")]);
    // Floods close the connection.
    f.join(3, None, "Flood");
    let mut closed = Vec::new();
    for _ in 0..400 {
        closed.extend(closes(&f.send(3, &Packet::Ping { nonce: 1 })));
    }
    assert_eq!(closed, vec![(3, 1008, "Message rate exceeded")]);
}

#[test]
fn idle_sockets_are_held_and_empty_rooms_end() {
    let mut f = Fixture::new(false);
    f.join(1, Some(HOST_KEY), "Host");
    f.now += IDLE_MS + 1;
    let (ended, out) = f.housekeep();
    assert_eq!(ended, None);
    assert_eq!(closes(&out), vec![(1, 1000, "Connection timed out")]);
    assert_eq!(f.room.riders(), 1);
    f.now += GRACE_MS;
    assert_eq!(f.housekeep().0, None);
    f.now += IDLE_MS + 1;
    assert_eq!(f.housekeep().0, Some("Empty room expired"));
    assert!(f.room.closed());
    let mut out = Vec::new();
    assert_eq!(
        f.room
            .admit(9, None, f.now, &mut f.rng, &mut out)
            .unwrap_err()
            .status,
        410
    );
    // Private rooms also end at their age limit, riders or not.
    let mut f = Fixture::new(false);
    f.join(1, Some(HOST_KEY), "Host");
    f.now += MAX_AGE_MS + 1;
    let (ended, out) = f.housekeep();
    assert_eq!(ended, Some("Room expired"));
    assert_eq!(closes(&out), vec![(1, 1000, "Room expired")]);
}

#[test]
fn a_saved_room_comes_back_with_its_riders_held() {
    let mut f = Fixture::new(false);
    f.join(1, Some(HOST_KEY), "Host");
    let (_, out) = f.join(2, None, "Guest");
    let Packet::Welcome { key, .. } = &sent(&out, 2)[0] else {
        panic!("no welcome");
    };
    let guest_key: String = key.iter().map(|b| format!("{b:02x}")).collect();
    let epoch = f.room.lobby().epoch;
    f.send(
        1,
        &Packet::SetLobby {
            epoch,
            map: 12,
            started: true,
        },
    );
    let mut out = Vec::new();
    let saved = f.room.shut_down(f.now, &mut out);
    assert_eq!(
        closes(&out),
        vec![
            (1, 1012, "Server restarting"),
            (2, 1012, "Server restarting")
        ]
    );
    let saved: SavedRoom = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
    let later = f.now + 5_000;
    let mut room = Room::restore(saved, later).unwrap();
    let lobby = room.lobby();
    assert_eq!(
        (lobby.map, lobby.started, lobby.players.len(), lobby.host),
        (12, true, 2, 0)
    );
    // The guest rejoins its slot with its key; the hold runs from the restart.
    let mut out = Vec::new();
    assert_eq!(
        room.admit(7, Some(&guest_key), later, &mut f.rng, &mut out),
        Ok(1)
    );
    // The host is still held; the guest is back but has not joined yet.
    assert_eq!(room.riders(), 1);
    let mut out = Vec::new();
    room.housekeep(later + GRACE_MS - 1, &mut out);
    assert_eq!(room.riders(), 1);
    room.housekeep(later + GRACE_MS, &mut out);
    assert_eq!(room.riders(), 0);
}
