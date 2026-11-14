//! Free Ride hub packets (research/FREERIDE_HUB.md) round-trip at their
//! limits and refuse forged values.
use atv_net::{
    Challenge, Hub, HubRow, NO_PEER, Packet, challenge_end, challenge_op, decode, encode,
    event_kind, event_phase, hub_op, signal,
};

fn full_event() -> Hub {
    Hub {
        op: hub_op::EVENT,
        serial: 7,
        kind: event_kind::HILL,
        phase: event_phase::RESULTS,
        timer_ms: 5000,
        point: [10., 20., -30.],
        target: [-1000., 50., 2500.],
        radius: 40.,
        mask: (1 << 20) - 1,
        value: 0.,
        choice: atv_net::hub::SOLO,
        maps: Vec::new(),
        votes: Vec::new(),
        rows: (0..20)
            .map(|peer| HubRow {
                peer,
                value: f32::from(peer) * 1.5,
                points: 100 - u16::from(peer),
                finished: peer % 2 == 0,
            })
            .collect(),
    }
}

#[test]
fn hub_packets_round_trip_at_their_limits() {
    for hub in [
        full_event(),
        Hub {
            op: hub_op::VOTE,
            serial: u32::MAX,
            timer_ms: 60_000,
            choice: atv_net::hub::NO_CHOICE,
            maps: vec![1, 5, 8],
            votes: vec![3, 0, 17],
            ..Default::default()
        },
        Hub {
            op: hub_op::RIDERS,
            mask: 0b1011,
            ..Default::default()
        },
        Hub {
            op: hub_op::SCORE,
            serial: 3,
            value: 123_456.,
            ..Default::default()
        },
        Hub {
            op: hub_op::CAST,
            serial: 3,
            choice: 2,
            ..Default::default()
        },
    ] {
        let packet = Packet::Hub(Box::new(hub));
        let bytes = encode(&packet).unwrap();
        assert_eq!(bytes[5], 26);
        assert_eq!(decode(&bytes).unwrap(), packet);
    }
}

#[test]
fn hub_packets_refuse_forged_values() {
    let bad = [
        Hub {
            op: 3,
            ..Default::default()
        },
        Hub {
            kind: 3,
            ..full_event()
        },
        Hub {
            phase: 4,
            ..full_event()
        },
        Hub {
            point: [f32::NAN, 0., 0.],
            ..full_event()
        },
        Hub {
            target: [2.0e6, 0., 0.],
            ..full_event()
        },
        Hub {
            mask: 1 << 20,
            ..full_event()
        },
        Hub {
            radius: -1.,
            ..full_event()
        },
        Hub {
            timer_ms: 31 * 60 * 1000,
            ..full_event()
        },
        Hub {
            op: hub_op::CAST,
            choice: 3,
            ..Default::default()
        },
        Hub {
            op: hub_op::VOTE,
            maps: vec![1, 2, 3, 4],
            votes: vec![0; 4],
            ..Default::default()
        },
        Hub {
            op: hub_op::VOTE,
            maps: vec![41],
            votes: vec![0],
            ..Default::default()
        },
        Hub {
            op: hub_op::VOTE,
            maps: vec![1, 2],
            votes: vec![0],
            ..Default::default()
        },
        Hub {
            op: hub_op::SCORE,
            value: -1.,
            ..Default::default()
        },
        Hub {
            rows: vec![HubRow::default(), HubRow::default()],
            ..full_event()
        },
        Hub {
            rows: vec![HubRow {
                peer: 20,
                ..Default::default()
            }],
            ..full_event()
        },
    ];
    for hub in bad {
        assert!(
            encode(&Packet::Hub(Box::new(hub.clone()))).is_err(),
            "{hub:?}"
        );
    }
    // Hand-forged bytes: a truncated packet, trailing bytes and a bad flag.
    let mut bytes = encode(&Packet::Hub(Box::new(full_event()))).unwrap();
    let fix = |b: &mut Vec<u8>| {
        let len = (b.len() - 8) as u16;
        b[6..8].copy_from_slice(&len.to_le_bytes());
    };
    let mut truncated = bytes.clone();
    truncated.pop();
    fix(&mut truncated);
    assert!(decode(&truncated).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    fix(&mut trailing);
    assert!(decode(&trailing).is_err());
    let last = bytes.len() - 1;
    bytes[last] = 2;
    assert!(decode(&bytes).is_err());
}

#[test]
fn challenges_round_trip_and_refuse_forged_values() {
    for challenge in [
        Challenge {
            op: challenge_op::ASK,
            peer: 19,
            ..Default::default()
        },
        Challenge {
            op: challenge_op::START,
            serial: 9,
            peer: 2,
            timer_ms: 45_000,
            target: [100., -2., 300.],
            detail: 0,
        },
        Challenge {
            op: challenge_op::RESULT,
            serial: 9,
            peer: 2,
            detail: NO_PEER,
            ..Default::default()
        },
        Challenge {
            op: challenge_op::CLOSED,
            serial: 9,
            peer: 2,
            detail: challenge_end::UNAVAILABLE,
            ..Default::default()
        },
    ] {
        let packet = Packet::Challenge(challenge);
        let bytes = encode(&packet).unwrap();
        assert_eq!(bytes[5], 27);
        assert_eq!(decode(&bytes).unwrap(), packet);
    }
    for challenge in [
        Challenge {
            op: 8,
            ..Default::default()
        },
        Challenge {
            peer: 20,
            ..Default::default()
        },
        Challenge {
            op: challenge_op::CLOSED,
            detail: 6,
            ..Default::default()
        },
        Challenge {
            op: challenge_op::RESULT,
            detail: 30,
            ..Default::default()
        },
        Challenge {
            op: challenge_op::ASK,
            detail: 1,
            ..Default::default()
        },
        Challenge {
            target: [0., f32::INFINITY, 0.],
            ..Default::default()
        },
    ] {
        assert!(
            encode(&Packet::Challenge(challenge)).is_err(),
            "{challenge:?}"
        );
    }
}

#[test]
fn hello_takes_the_hub_bit() {
    let hello = Packet::Signal(atv_net::Signal {
        peer: 0,
        kind: signal::HELLO,
        data: vec![signal::FRAMES | signal::CPU_SOUNDS | signal::HUB],
    });
    let bytes = encode(&hello).unwrap();
    assert_eq!(decode(&bytes).unwrap(), hello);
}
