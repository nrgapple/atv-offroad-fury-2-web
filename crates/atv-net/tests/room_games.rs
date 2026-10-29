//! Room King of the Hills / Treasure Hunt messages (research/MULTIPLAYER.md
//! "Room games"): the host's state and the riders' claims round-trip at
//! their limits, and out-of-range values are refused on both sides.
use atv_net::*;

fn state(game: u8, marks: usize) -> GameState {
    GameState {
        game,
        serial: u32::MAX,
        phase: game_phase::OVER,
        timer: 6.,
        clock: 1200.,
        scores: [MAX_MARKS as u32, 0, 3, 1],
        holder: NO_PEER,
        marks: GameMarks {
            venue: 13,
            players: 0b1111,
            stunts: [MAX_GAME_POINTS, 0., 1.5, 12345.],
            marks: (0..marks)
                .map(|i| {
                    (
                        if i % 5 == 4 { NO_PEER } else { (i % 4) as u8 },
                        (i % 2) as f32 * MAX_GAME_POINTS,
                    )
                })
                .collect(),
        },
        ..Default::default()
    }
}

fn roundtrip(packet: &Packet) -> Packet {
    let bytes = encode(packet).expect("encode");
    assert!(bytes.len() <= MAX_PACKET_BYTES);
    decode(&bytes).expect("decode")
}

#[test]
fn room_game_states_round_trip_at_their_limits() {
    for game in [game_kind::KING_OF_THE_HILLS, game_kind::TREASURE_HUNT] {
        for marks in [0, 1, 64, MAX_MARKS] {
            let s = state(game, marks);
            assert_eq!(
                roundtrip(&Packet::Game(Box::new(s.clone()))),
                Packet::Game(Box::new(s))
            );
        }
    }
    // Other games carry no marks on the wire.
    let tag = GameState {
        game: game_kind::TAG,
        ..Default::default()
    };
    assert_eq!(
        roundtrip(&Packet::Game(Box::new(tag.clone()))),
        Packet::Game(Box::new(tag))
    );
}

#[test]
fn room_game_states_refuse_out_of_range_values() {
    let bad = [
        GameState {
            phase: game_phase::OVER + 1,
            ..state(4, 1)
        },
        GameState {
            scores: [MAX_MARKS as u32 + 1, 0, 0, 0],
            ..state(4, 1)
        },
        GameState {
            holder: 4,
            ..state(5, 1)
        },
        GameState {
            game: 6,
            ..state(4, 1)
        },
        {
            let mut s = state(4, 1);
            s.marks.venue = 14;
            s
        },
        {
            let mut s = state(4, 1);
            s.marks.marks.push((4, 0.));
            s
        },
        {
            let mut s = state(4, 1);
            s.marks.marks.push((0, -1.));
            s
        },
        {
            let mut s = state(4, 1);
            s.marks.marks.push((0, f32::NAN));
            s
        },
        {
            let mut s = state(4, 1);
            s.marks.marks.push((0, MAX_GAME_POINTS * 2.));
            s
        },
        {
            let mut s = state(5, 1);
            s.marks.stunts[2] = f32::INFINITY;
            s
        },
        {
            let mut s = state(5, 1);
            s.marks.players = 1 << RACE_RIDERS;
            s
        },
        state(4, MAX_MARKS + 1),
    ];
    for s in bad {
        assert!(s.validate().is_err(), "{s:?}");
        assert!(encode(&Packet::Game(Box::new(s))).is_err());
    }
    // A hand-built packet claiming more marks than allowed is refused.
    let mut bytes = encode(&Packet::Game(Box::new(state(4, 0)))).unwrap();
    let n = bytes.len();
    bytes[n - 2..].copy_from_slice(&(MAX_MARKS as u16 + 1).to_le_bytes());
    assert!(decode(&bytes).is_err());
}

#[test]
fn game_claims_round_trip_and_refuse_out_of_range_values() {
    let claim = GameClaim {
        peer: (MAX_PLAYERS - 1) as u8,
        serial: u32::MAX,
        kind: claim_kind::CAPTURE,
        index: MAX_MARKS as u16,
        points: MAX_GAME_POINTS,
        stunt: MAX_GAME_POINTS,
    };
    assert_eq!(roundtrip(&Packet::Claim(claim)), Packet::Claim(claim));
    assert_eq!(
        roundtrip(&Packet::Claim(GameClaim::default())),
        Packet::Claim(GameClaim::default())
    );
    for bad in [
        GameClaim {
            peer: MAX_PLAYERS as u8,
            ..claim
        },
        GameClaim { kind: 2, ..claim },
        GameClaim {
            index: MAX_MARKS as u16 + 1,
            ..claim
        },
        GameClaim {
            points: -1.,
            ..claim
        },
        GameClaim {
            points: f32::NAN,
            ..claim
        },
        GameClaim {
            stunt: MAX_GAME_POINTS * 2.,
            ..claim
        },
    ] {
        assert!(encode(&Packet::Claim(bad)).is_err(), "{bad:?}");
    }
    // Hand-built claims: truncated, or with a bad kind, are refused.
    let bytes = encode(&Packet::Claim(claim)).unwrap();
    assert!(decode(&bytes[..bytes.len() - 1]).is_err());
    let mut forged = bytes.clone();
    let kind_at = bytes.len() - 4 - 4 - 2 - 1;
    forged[kind_at] = 2;
    assert!(decode(&forged).is_err());
    // A hand-built state whose player mask names a fifth slot is refused.
    let mut bytes = encode(&Packet::Game(Box::new(state(5, 0)))).unwrap();
    let players_at = bytes.len() - 2 - 4 * RACE_RIDERS - 1;
    assert_eq!(bytes[players_at], 0b1111);
    bytes[players_at] = 0b1_0000;
    assert!(decode(&bytes).is_err());
}
