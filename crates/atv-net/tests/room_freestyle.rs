//! Room Freestyle messages (research/MULTIPLAYER.md "Room Freestyle"): the
//! host's score table (game 6) and the riders' Freestyle reports (claim
//! kind 2) round-trip at their limits, and out-of-range or inconsistent
//! values are refused on both sides.
use atv_net::*;

fn table() -> GameState {
    GameState {
        game: game_kind::FREESTYLE,
        serial: u32::MAX,
        phase: game_phase::PLAY,
        timer: 3.,
        clock: 1200.,
        scores: [MAX_MARKS as u32, 0, 3, 1],
        holder: NO_PEER,
        marks: GameMarks {
            venue: 0,
            players: 0b1011,
            stunts: [MAX_GAME_POINTS, 0., 1500., 12345.],
            marks: vec![(0, MAX_GAME_POINTS), (1, 0.), (NO_PEER, 0.), (3, 12345.)],
        },
        ..Default::default()
    }
}

fn report() -> GameClaim {
    GameClaim {
        peer: (MAX_PLAYERS - 1) as u8,
        serial: u32::MAX,
        kind: claim_kind::FREESTYLE,
        index: MAX_MARKS as u16,
        points: 4000.,
        stunt: MAX_GAME_POINTS,
    }
}

fn roundtrip(packet: &Packet) -> Packet {
    let bytes = encode(packet).expect("encode");
    assert!(bytes.len() <= MAX_PACKET_BYTES);
    decode(&bytes).expect("decode")
}

#[test]
fn freestyle_tables_round_trip_at_their_limits() {
    for phase in [
        game_phase::COUNTDOWN,
        game_phase::PLAY,
        game_phase::COMPLETE,
        game_phase::OVER,
    ] {
        for marks in 0..=FREESTYLE_MARKS {
            let mut s = table();
            s.phase = phase;
            s.marks.marks.truncate(marks);
            assert_eq!(
                roundtrip(&Packet::Game(Box::new(s.clone()))),
                Packet::Game(Box::new(s))
            );
        }
    }
}

#[test]
fn freestyle_tables_refuse_out_of_range_values() {
    let edit = |f: fn(&mut GameState)| {
        let mut s = table();
        f(&mut s);
        s
    };
    let bad = [
        edit(|s| s.phase = game_phase::OVER + 1),
        // The venue byte is unused: the room's map names the course.
        edit(|s| s.marks.venue = 1),
        edit(|s| s.marks.players = 1 << RACE_RIDERS),
        // One mark per slot, and only that slot's.
        edit(|s| s.marks.marks.push((NO_PEER, 0.))),
        edit(|s| s.marks.marks[1].0 = 2),
        edit(|s| s.marks.marks[1].0 = 4),
        // A best combo above the slot's session score.
        edit(|s| s.marks.marks[3].1 = 12346.),
        edit(|s| s.marks.marks[1].1 = f32::NAN),
        edit(|s| s.marks.marks[1].1 = -1.),
        edit(|s| s.marks.stunts[2] = f32::INFINITY),
        edit(|s| s.marks.stunts[2] = MAX_GAME_POINTS * 2.),
        edit(|s| s.scores[1] = MAX_MARKS as u32 + 1),
        edit(|s| s.holder = 0),
        edit(|s| s.timer = f32::NAN),
    ];
    for s in bad {
        assert!(s.validate().is_err(), "{s:?}");
        assert!(encode(&Packet::Game(Box::new(s))).is_err());
    }
    // Hand-built bytes: a fifth mark, or a mark owned by another slot.
    let bytes = encode(&Packet::Game(Box::new(table()))).unwrap();
    let mut extra = bytes.clone();
    let n = extra.len();
    let count_at = n - 2 - 5 * FREESTYLE_MARKS;
    assert_eq!(
        &extra[count_at..count_at + 2],
        &(FREESTYLE_MARKS as u16).to_le_bytes()
    );
    extra[count_at..count_at + 2].copy_from_slice(&5u16.to_le_bytes());
    extra.extend_from_slice(&[NO_PEER, 0, 0, 0, 0]);
    assert!(decode(&extra).is_err());
    let mut forged = bytes.clone();
    forged[count_at + 2 + 5] = 0; // slot 1's mark claims slot 0
    assert!(decode(&forged).is_err());
    assert!(decode(&bytes[..n - 1]).is_err());
}

#[test]
fn freestyle_reports_round_trip_and_refuse_out_of_range_values() {
    let claim = report();
    assert_eq!(roundtrip(&Packet::Claim(claim)), Packet::Claim(claim));
    // The serial-0 announcement of room Freestyle support.
    let announce = GameClaim {
        kind: claim_kind::FREESTYLE,
        ..Default::default()
    };
    assert_eq!(roundtrip(&Packet::Claim(announce)), Packet::Claim(announce));
    for bad in [
        GameClaim {
            points: MAX_GAME_POINTS,
            stunt: 10.,
            ..claim
        },
        GameClaim {
            stunt: f32::NAN,
            ..claim
        },
        GameClaim {
            stunt: -1.,
            ..claim
        },
        GameClaim {
            index: MAX_MARKS as u16 + 1,
            ..claim
        },
        GameClaim {
            kind: claim_kind::FREESTYLE + 1,
            ..claim
        },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
        assert!(encode(&Packet::Claim(bad)).is_err(), "{bad:?}");
    }
}
