//! Interest management and frame assembly (research/GAME_SERVER.md "Shape").
//!
//! Each tick a recipient gets the latest pose of every other rider that is
//! due for it: near riders every tick, farther riders less often, resting
//! riders only when something changed plus a slow keepalive, and never a
//! pose it already has. The tiers follow the relay-era batch tiers
//! (research/MULTIPLAYER.md "Free Ride pose batches") at the server's rates.

use atv_net::{MAX_FRAME, MAX_PACKET_BYTES, RiderState, Snapshot};
use bytes::Bytes;

/// Room tick (20 Hz).
pub const TICK_MS: u64 = 50;
/// Below this speed (ft/s) a rider in the ordinary state is at rest, as the
/// client's own pose cadence counts it.
pub const REST_SPEED: f32 = 0.5;
/// A resting rider whose pose moved less than this (ft) has not changed.
pub const REST_MOVE_FT: f32 = 0.25;
/// A resting rider's unchanged pose is sent again this often, so receivers
/// keep it fresh (the client holds a resting rider for 2.5 s).
pub const KEEPALIVE_MS: u64 = 2000;

/// Time between poses of a moving rider `distance` feet from the recipient:
/// under 60 ft every tick, to 300 ft about 5 Hz, to 1,000 ft about 2 Hz,
/// beyond that about 0.5 Hz.
pub fn tier_ms(distance: f32) -> u64 {
    if distance < 60. {
        0
    } else if distance < 300. {
        200
    } else if distance < 1000. {
        500
    } else {
        2000
    }
}

/// What the server knows of a rider's newest accepted pose: its encoded
/// bytes (decoded and validated once, on arrival) and the few fields
/// interest management reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub sequence: u32,
    /// An encoded `Snapshot` packet stamped with the rider's own slot.
    pub bytes: Bytes,
    /// A compact pose (kind 9), which a frame may carry; full snapshots
    /// (kind 4) are sent on their own.
    pub compact: bool,
    pub position: [f32; 3],
    pub moving: bool,
    pub state: RiderState,
    pub crash: u32,
    pub models: (u16, u16),
}

impl Pose {
    pub fn new(snapshot: &Snapshot, bytes: Bytes) -> Self {
        let [x, y, z] = snapshot.velocity;
        let moving = snapshot.rider_state != RiderState::Riding
            || x * x + y * y + z * z > REST_SPEED * REST_SPEED;
        Pose {
            sequence: snapshot.sequence,
            bytes,
            compact: snapshot.animation.is_some(),
            position: snapshot.atv.position,
            moving,
            state: snapshot.rider_state,
            crash: snapshot.crash_serial,
            models: (snapshot.atv_model, snapshot.rider_model),
        }
    }
}

/// The pose a recipient last got of one rider, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sent {
    pub sequence: u32,
    pub at_ms: u64,
    pub position: [f32; 3],
    pub moving: bool,
    pub state: RiderState,
    pub crash: u32,
}

impl Sent {
    pub fn of(pose: &Pose, at_ms: u64) -> Self {
        Sent {
            sequence: pose.sequence,
            at_ms,
            position: pose.position,
            moving: pose.moving,
            state: pose.state,
            crash: pose.crash,
        }
    }
}

pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Whether `pose` is due for a recipient `distance` feet away that last got
/// `sent` of this rider. A change of rider state, a crash, or setting off
/// from rest or coming to it is sent at once, at any distance.
pub fn due(sent: Option<&Sent>, pose: &Pose, distance: f32, now_ms: u64) -> bool {
    let Some(sent) = sent else {
        return true;
    };
    if sent.sequence == pose.sequence {
        return false;
    }
    if sent.state != pose.state || sent.crash != pose.crash || sent.moving != pose.moving {
        return true;
    }
    let waited = now_ms.saturating_sub(sent.at_ms);
    if !pose.moving {
        return self::distance(sent.position, pose.position) > REST_MOVE_FT
            || waited >= KEEPALIVE_MS;
    }
    // Ticks land within a few ms of their schedule: half a tick of slack
    // keeps a 200 ms tier at 200 ms rather than 250.
    waited + TICK_MS / 2 >= tier_ms(distance)
}

/// Frame packet overhead: the eight-byte header and the pose count.
const FRAME_HEADER: usize = 9;

/// Encoded `Packet::Frame`s holding `poses` (encoded compact snapshots, at
/// most one per rider), split where a frame would exceed [`MAX_FRAME`]
/// poses or [`MAX_PACKET_BYTES`]. Built from the bytes; nothing is decoded
/// or encoded again. A pose too large for any frame is left out (the codec
/// bounds a compact pose far below that).
pub fn frames(poses: &[Bytes]) -> Vec<Bytes> {
    let mut out = Vec::new();
    let mut frame: Vec<u8> = Vec::new();
    let mut count = 0usize;
    let finish = |frame: &mut Vec<u8>, count: &mut usize, out: &mut Vec<Bytes>| {
        if *count > 0 {
            frame[8] = *count as u8;
            let len = (frame.len() - 8) as u16;
            frame[6..8].copy_from_slice(&len.to_le_bytes());
            out.push(Bytes::from(std::mem::take(frame)));
            *count = 0;
        }
    };
    for pose in poses {
        if FRAME_HEADER + 2 + pose.len() > MAX_PACKET_BYTES {
            continue;
        }
        if count == MAX_FRAME || frame.len() + 2 + pose.len() > MAX_PACKET_BYTES {
            finish(&mut frame, &mut count, &mut out);
        }
        if count == 0 {
            frame.reserve(FRAME_HEADER + 2 + pose.len());
            frame.extend_from_slice(b"ATVM");
            frame.extend_from_slice(&[atv_net::VERSION, 21, 0, 0, 0]);
        }
        frame.extend_from_slice(&(pose.len() as u16).to_le_bytes());
        frame.extend_from_slice(pose);
        count += 1;
    }
    finish(&mut frame, &mut count, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_net::{Packet, decode, encode};

    fn compact(peer: u8, sequence: u32, x: f32) -> (Snapshot, Bytes) {
        let snapshot = Snapshot {
            peer,
            sequence,
            time_ms: sequence * 50,
            atv_model: 1,
            animation: Some(Box::default()),
            atv: atv_net::Transform {
                position: [x, 0., 0.],
                ..Default::default()
            },
            ..Default::default()
        };
        let bytes = encode(&Packet::Snapshot(Box::new(snapshot.clone()))).unwrap();
        (snapshot, Bytes::from(bytes))
    }

    #[test]
    fn frames_match_the_codec_and_split_by_count_and_size() {
        let poses: Vec<Bytes> = (0..20).map(|p| compact(p, 1, p as f32).1).collect();
        let built = frames(&poses);
        assert_eq!(built.len(), 1);
        let expected = encode(&Packet::Frame {
            poses: poses.iter().map(|p| p.to_vec()).collect(),
        })
        .unwrap();
        assert_eq!(built[0].as_ref(), expected.as_slice());
        // Many poses split on the packet size; every frame decodes.
        let many: Vec<Bytes> = (0..200).map(|i| compact((i % 20) as u8, i, 0.).1).collect();
        let built = frames(&many);
        assert!(built.len() > 1);
        let mut total = 0;
        for frame in &built {
            assert!(frame.len() <= MAX_PACKET_BYTES);
            let Packet::Frame { poses } = decode(frame).unwrap() else {
                panic!("not a frame");
            };
            assert!(poses.len() <= MAX_FRAME);
            total += poses.len();
        }
        assert_eq!(total, 200);
        assert!(frames(&[]).is_empty());
    }

    #[test]
    fn tiers_follow_distance_and_rest_waits_for_change_or_keepalive() {
        let (mut s, bytes) = compact(1, 10, 0.);
        s.velocity = [30., 0., 0.];
        let moving = Pose::new(&s, bytes.clone());
        assert!(moving.moving && moving.compact);
        // Never sent: due at once at any distance.
        assert!(due(None, &moving, 5000., 0));
        let sent = Sent::of(&moving, 1000);
        // The same pose is never sent twice.
        assert!(!due(Some(&sent), &moving, 0., 99_999));
        let mut next = moving.clone();
        next.sequence = 11;
        assert!(due(Some(&sent), &next, 10., 1050));
        assert!(!due(Some(&sent), &next, 100., 1100));
        assert!(due(Some(&sent), &next, 100., 1175));
        assert!(!due(Some(&sent), &next, 500., 1400));
        assert!(due(Some(&sent), &next, 500., 1500));
        assert!(!due(Some(&sent), &next, 2000., 2500));
        assert!(due(Some(&sent), &next, 2000., 3000));
        // A wreck goes out at once, however far.
        next.state = RiderState::Wreck;
        assert!(due(Some(&sent), &next, 5000., 1001));
        // At rest: only a change, or the keepalive.
        s.velocity = [0.; 3];
        let resting = Pose::new(&s, bytes);
        assert!(!resting.moving);
        let sent = Sent::of(&resting, 1000);
        let mut later = resting.clone();
        later.sequence = 11;
        assert!(!due(Some(&sent), &later, 0., 1500));
        assert!(due(Some(&sent), &later, 0., 1000 + KEEPALIVE_MS));
        later.position = [1., 0., 0.];
        assert!(due(Some(&sent), &later, 0., 1050));
        // Setting off from rest is sent at once.
        let mut off = resting.clone();
        off.sequence = 12;
        off.moving = true;
        assert!(due(Some(&sent), &off, 5000., 1001));
        assert_eq!(distance([0., 3., 0.], [4., 0., 0.]), 5.);
    }
}
