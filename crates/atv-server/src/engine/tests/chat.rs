//! Quick chat in a room (research/QUICK_CHAT.md).
use super::*;
use atv_net::chat::{Chat, EMOTE, HEARING_FT, HORN, LINE};

fn line(id: u8) -> Packet {
    Packet::Chat(Chat {
        peer: 0,
        kind: LINE,
        id,
    })
}

fn chats(out: &[Out], to: ConnId) -> Vec<Chat> {
    sent(out, to)
        .into_iter()
        .filter_map(|p| match p {
            Packet::Chat(c) => Some(c),
            _ => None,
        })
        .collect()
}

/// A shared room: 1 and 2 take chat, 3 is an older build.
fn room() -> (Fixture, String) {
    let mut f = Fixture::new(true);
    let k1 = f.room.claim(f.now, &mut f.rng).unwrap();
    let k2 = f.room.claim(f.now, &mut f.rng).unwrap();
    f.join(1, Some(HOST_KEY), "Ann");
    f.join(2, Some(&k1), "Bo");
    f.join(3, Some(&k2), "Cy");
    f.hello(1, signal::FRAMES | signal::CHAT);
    f.hello(2, signal::CHAT);
    for id in 1..=3 {
        f.ready(id);
    }
    (f, k1)
}

#[test]
fn lines_reach_chat_builds_stamped_and_echoed() {
    let (mut f, _) = room();
    let out = f.send(2, &line(4));
    let expected = Chat {
        peer: 1,
        kind: LINE,
        id: 4,
    };
    assert_eq!(chats(&out, 1), [expected]);
    assert_eq!(chats(&out, 2), [expected]);
    assert!(sent(&out, 3).is_empty());
    assert!(closes(&out).is_empty());
}

#[test]
fn senders_are_limited_and_unknown_ids_close() {
    let (mut f, _) = room();
    for _ in 0..3 {
        assert_eq!(chats(&f.send(1, &line(0)), 2).len(), 1);
        f.now += 100;
    }
    // A fourth line inside 5 s is dropped and starts the 5 s lockout.
    assert!(f.send(1, &line(0)).is_empty());
    f.now += 4_900;
    assert!(f.send(1, &line(0)).is_empty());
    f.now += 200;
    assert_eq!(chats(&f.send(1, &line(0)), 2).len(), 1);
    // Emotes keep their own 2 s.
    let emote = Packet::Chat(Chat {
        peer: 0,
        kind: EMOTE,
        id: 3,
    });
    assert_eq!(chats(&f.send(1, &emote), 2).len(), 1);
    assert!(f.send(1, &emote).is_empty());
    // Free text cannot be encoded, so a forged id is an invalid packet.
    let mut forged = encode(&line(0)).unwrap();
    forged[10] = 200;
    let mut out = Vec::new();
    f.room.receive(1, Bytes::from(forged), f.now, &mut out);
    assert_eq!(closes(&out), [(1, 1008, "Invalid multiplayer packet")]);
}

#[test]
fn the_horn_reaches_only_riders_within_hearing() {
    let (mut f, _) = room();
    let horn = Packet::Chat(Chat {
        peer: 0,
        kind: HORN,
        id: 0,
    });
    // Before any pose the sender only hears itself.
    let out = f.send(1, &horn);
    assert_eq!(chats(&out, 1).len(), 1);
    assert!(chats(&out, 2).is_empty());
    f.send(1, &snapshot(0, 1, 0.));
    f.send(2, &snapshot(1, 1, HEARING_FT - 10.));
    f.now += 1_000;
    assert_eq!(chats(&f.send(1, &horn), 2).len(), 1);
    f.send(2, &snapshot(1, 2, HEARING_FT + 10.));
    f.now += 1_000;
    let out = f.send(1, &horn);
    assert!(chats(&out, 2).is_empty());
    assert_eq!(chats(&out, 1).len(), 1);
}

#[test]
fn a_report_carries_the_recent_chat_and_needs_a_slot_key() {
    let (mut f, k1) = room();
    f.send(1, &line(1));
    f.now += 10_000;
    f.send(2, &line(7));
    let context = f.room.report_context(&k1, 0, f.now).unwrap();
    assert_eq!(
        (context.reporter, context.reporter_name.as_str()),
        (1, "Bo")
    );
    assert_eq!((context.target, context.target_name.as_str()), (0, "Ann"));
    assert_eq!(context.events.len(), 2);
    assert_eq!((context.events[0].peer, context.events[0].id), (0, 1));
    // Old chat leaves the ring.
    let later = f.now + super::super::chat::RING_MS - 5_000;
    assert_eq!(
        f.room.report_context(&k1, 0, later).unwrap().events.len(),
        1
    );
    // Not oneself, not an empty slot, not without a key of this room.
    assert!(f.room.report_context(&k1, 1, f.now).is_none());
    assert!(f.room.report_context(&k1, 9, f.now).is_none());
    assert!(f.room.report_context(&"0".repeat(32), 0, f.now).is_none());
}
