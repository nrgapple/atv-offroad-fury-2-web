// Presentation only: not part of the deterministic simulation (clippy.toml).
#![allow(clippy::disallowed_methods)]
use atv_net::*;
fn state() -> Snapshot {
    Snapshot {
        animation: Some(Box::default()),
        rpm: 6000.,
        throttle: 0.7,
        speed: 43.25,
        velocity: [43.25, -2., 1.],
        angular_velocity: [0.1, 0.2, 0.3],
        sequence: 1,
        time_ms: 50,
        ..Default::default()
    }
}
fn unpack(s: &Snapshot) -> Snapshot {
    let Packet::Snapshot(out) =
        decode(&encode(&Packet::Snapshot((s.clone()).into())).unwrap()).unwrap()
    else {
        panic!("not snapshot")
    };
    *out
}
#[test]
fn compact_has_no_bone_poses_and_keeps_positions_exact() {
    for i in 0..1000 {
        let mut s = state();
        s.atv.position = [i as f32 * 53.123, -i as f32 * 0.234, 7777.777];
        s.rider.position = [-i as f32, 23.456, 103.05];
        let angle = i as f32 * 0.017;
        s.atv.rotation = [(angle * 0.5).cos(), 0., (angle * 0.5).sin(), 0.];
        let bytes = encode(&Packet::Snapshot((s.clone()).into())).unwrap();
        assert!(bytes.len() < 170);
        let out = unpack(&s);
        assert_eq!(out.atv.position, s.atv.position);
        assert_eq!(out.rider.position, s.rider.position);
        assert!(out.atv_nodes.is_empty() && out.rider_nodes.is_empty());
        for j in 0..4 {
            assert!((out.atv.rotation[j] - s.atv.rotation[j]).abs() < 0.00005);
        }
        for j in 0..3 {
            assert!((out.angular_velocity[j] - s.angular_velocity[j]).abs() <= 1. / 512.);
        }
    }
}
#[test]
fn delta_uses_only_changed_blocks_and_requires_correct_baseline() {
    let base = unpack(&state());
    let mut next = base.clone();
    next.sequence = 2;
    next.time_ms = 100;
    next.atv.position[0] += 3.;
    let d = delta(&next, &base).unwrap();
    assert_eq!(d.mask, 1);
    let wire = encode(&Packet::Delta(Box::new(d))).unwrap();
    assert!(wire.len() < 65);
    let Packet::Delta(d) = decode(&wire).unwrap() else {
        panic!("not delta")
    };
    let out = apply_delta(&d, &base).unwrap();
    assert_eq!(out.atv.position, next.atv.position);
    assert_eq!(out.animation, base.animation);
    assert_eq!(out.rider, base.rider);
    assert_eq!(out.sequence, 2);
    let mut wrong = base.clone();
    wrong.sequence = 0;
    assert!(apply_delta(&d, &wrong).is_err());
    wrong = base.clone();
    wrong.peer = 1;
    assert!(apply_delta(&d, &wrong).is_err());
    let mut unchanged = base.clone();
    unchanged.sequence = 3;
    unchanged.time_ms = 200;
    let d = delta(&unchanged, &base).unwrap();
    assert_eq!(d.mask, 0);
    assert_eq!(encode(&Packet::Delta(Box::new(d))).unwrap().len(), 26);
}
#[test]
fn malformed_compact_packets_and_non_rigid_transforms_are_rejected() {
    let bytes = encode(&Packet::Snapshot((state()).into())).unwrap();
    for i in 0..bytes.len() {
        assert!(decode(&bytes[..i]).is_err());
    }
    let mut bad = bytes.clone();
    bad[25] = 128;
    assert!(decode(&bad).is_err());
    for v in [f32::NAN, f32::INFINITY, -1.] {
        let mut s = state();
        s.animation.as_mut().unwrap().phase = v;
        assert!(encode(&Packet::Snapshot((s).into())).is_err());
    }
    let mut s = state();
    s.atv.scale[0] = 2.;
    assert!(encode(&Packet::Snapshot((s).into())).is_err());
    s = state();
    s.velocity[0] = 100_001.;
    assert!(encode(&Packet::Snapshot((s).into())).is_err());
}
#[test]
fn paused_state_stays_fresh_between_slow_heartbeats_but_eventually_expires() {
    let mut s = state();
    s.animation.as_mut().unwrap().paused = true;
    s.velocity = [0.; 3];
    s.rider_velocity = [0.; 3];
    let mut b = PoseBuffer::default();
    b.push(s, 100).unwrap();
    assert!(!b.sample(5100).unwrap().stale);
    assert!(b.sample(15100).unwrap().stale);
}
#[test]
fn linkage_rotation_interpolates_across_wrap_without_spinning_backwards() {
    let mut a = state();
    a.animation.as_mut().unwrap().wheels = [3.13; 4];
    let mut z = a.clone();
    z.sequence = 2;
    z.time_ms = 150;
    z.animation.as_mut().unwrap().wheels = [-3.13; 4];
    let mut b = PoseBuffer::new(SmoothingConfig {
        delay_ms: 0,
        extrapolation_ms: 100,
        stale_ms: 1000,
    });
    b.push(a, 50).unwrap();
    b.push(z, 150).unwrap();
    let p = b.sample(100).unwrap();
    assert!((p.snapshot.animation.unwrap().wheels[0] - std::f32::consts::PI).abs() < 0.02);
}

#[test]
fn valid_extreme_launch_uses_exact_escape_without_clipping_or_disconnect() {
    let mut s = state();
    s.velocity = [90_000., -50_000., 123.];
    s.speed = 95_000.;
    s.angular_velocity = [999., -500., 0.];
    let out = unpack(&s);
    assert_eq!(out.velocity, s.velocity);
    assert_eq!(out.speed, s.speed);
    assert_eq!(out.angular_velocity, s.angular_velocity);
}

fn bike(nodes: usize) -> Snapshot {
    let mut s = state();
    s.atv_model = 30;
    s.rider_model = 0;
    let a = s.animation.as_mut().unwrap();
    a.steering = 0.25;
    a.wheels = [1.5, -2.75, 0., 0.];
    a.compression = [0.125, -0.3, 0., 0.];
    s.rider_nodes = (0..nodes)
        .map(|i| {
            let angle = i as f32 * 0.31;
            Transform {
                position: [i as f32 * 0.1 - 1., 2.5 + i as f32 * 0.05, -0.75],
                rotation: [(angle * 0.5).cos(), 0., 0., (angle * 0.5).sin()],
                // MX rider motions scale and shear some bones; both are kept.
                scale: if i % 2 == 0 {
                    [0.95, 1., 1.03]
                } else {
                    [1.; 3]
                },
                shear: if i % 2 == 0 {
                    [0.04, 0., -0.02]
                } else {
                    [0.; 3]
                },
                ..Transform::default()
            }
        })
        .collect();
    s
}
#[test]
fn motorcycle_pose_carries_linkage_and_rider_bones_compactly() {
    let s = bike(26);
    let bytes = encode(&Packet::Snapshot((s.clone()).into())).unwrap();
    // 26 bones at 15 bytes each, half with 12 more for scale and shear,
    // plus the ATV pose's other blocks.
    assert!(bytes.len() < 700, "{}", bytes.len());
    let out = unpack(&s);
    assert_eq!(out.atv_model, 30);
    let (a, b) = (out.animation.unwrap(), s.animation.unwrap());
    assert!((a.steering - b.steering).abs() < 0.0002);
    for i in 0..2 {
        assert!((a.wheels[i] - b.wheels[i]).abs() < 0.0002);
        assert!((a.compression[i] - b.compression[i]).abs() < 0.001);
    }
    assert_eq!(out.rider_nodes.len(), 26);
    for (o, i) in out.rider_nodes.iter().zip(&s.rider_nodes) {
        for j in 0..3 {
            assert!((o.position[j] - i.position[j]).abs() <= 0.0005);
        }
        for j in 0..4 {
            assert!((o.rotation[j] - i.rotation[j]).abs() < 0.0001);
        }
        for j in 0..3 {
            assert!((o.scale[j] - i.scale[j]).abs() <= 0.00013);
            assert!((o.shear[j] - i.shear[j]).abs() <= 0.00013);
        }
        let (a, b) = (o.to_column_major(), i.to_column_major());
        assert!(
            a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 0.001),
            "{a:?} {b:?}"
        );
    }
    // The ATV pose is unchanged by the motorcycle block.
    let atv = encode(&Packet::Snapshot((state()).into())).unwrap();
    assert!(atv.len() < 170, "{}", atv.len());
}
#[test]
fn motorcycle_delta_replaces_rider_bones_and_keeps_unchanged_blocks() {
    let base = unpack(&bike(26));
    let mut next = base.clone();
    next.sequence = 2;
    next.time_ms = 100;
    next.rider_nodes[3].position[1] += 0.5;
    let d = delta(&next, &base).unwrap();
    assert_eq!(d.mask, 4);
    let Packet::Delta(d) = decode(&encode(&Packet::Delta(Box::new(d))).unwrap()).unwrap() else {
        panic!("not delta")
    };
    let out = apply_delta(&d, &base).unwrap();
    assert!((out.rider_nodes[3].position[1] - next.rider_nodes[3].position[1]).abs() < 0.001);
    assert_eq!(out.rider_nodes.len(), 26);
    assert_eq!(out.atv, base.atv);
    // A rider moving without bone changes sends no rider block.
    let mut moved = out.clone();
    moved.sequence = 3;
    moved.atv.position[0] += 1.;
    assert_eq!(delta(&moved, &out).unwrap().mask, 1);
}
#[test]
fn motorcycle_blocks_and_vehicle_ids_are_bounded() {
    assert!(encode(&Packet::Snapshot((bike(0)).into())).is_err());
    assert!(encode(&Packet::Snapshot((bike(MAX_NODES + 1)).into())).is_err());
    assert!(encode(&Packet::Snapshot((bike(MAX_NODES)).into())).is_ok());
    for model in [0, MAX_VEHICLE_MODEL + 1] {
        let mut s = state();
        s.atv_model = model;
        assert!(encode(&Packet::Snapshot((s).into())).is_err());
    }
    let mut s = state();
    s.rider_model = 2;
    assert!(encode(&Packet::Snapshot((s).into())).is_err());
    let bytes = encode(&Packet::Snapshot((bike(26)).into())).unwrap();
    for i in 0..bytes.len() {
        assert!(decode(&bytes[..i]).is_err());
    }
    // An ATV id cannot reinterpret a motorcycle block, and vice versa
    // (the vehicle id follows the header, peer, sequence and time).
    let mut swapped = bytes.clone();
    swapped[17] = 26;
    assert!(decode(&swapped).is_err());
    let atv = encode(&Packet::Snapshot((state()).into())).unwrap();
    let mut swapped = atv.clone();
    swapped[17] = 27;
    assert!(decode(&swapped).is_err());
    assert!(is_motorcycle(27) && is_motorcycle(46) && !is_motorcycle(26) && !is_motorcycle(47));
    assert!(valid_actor(46, 1) && !valid_actor(47, 0) && !valid_actor(0, 0));
}
#[test]
fn motorcycle_poses_travel_in_frames() {
    let pose = encode(&Packet::Snapshot((bike(26)).into())).unwrap();
    let frame = encode(&Packet::Frame {
        poses: vec![pose.clone(), pose.clone()],
    })
    .unwrap();
    let Packet::Frame { poses } = decode(&frame).unwrap() else {
        panic!("not a frame")
    };
    assert_eq!(poses, [pose.clone(), pose]);
}
#[test]
fn motorcycle_rider_bones_interpolate_between_poses() {
    let a = unpack(&bike(4));
    let mut z = a.clone();
    z.sequence = 2;
    z.time_ms = 150;
    z.rider_nodes[0].position[0] += 1.;
    let z = unpack(&z);
    let mut b = PoseBuffer::new(SmoothingConfig {
        delay_ms: 0,
        extrapolation_ms: 100,
        stale_ms: 1000,
    });
    b.push(a.clone(), 50).unwrap();
    b.push(z, 150).unwrap();
    let p = b.sample(100).unwrap();
    let x = p.snapshot.rider_nodes[0].position[0];
    assert!(
        (x - (a.rider_nodes[0].position[0] + 0.5)).abs() < 0.01,
        "{x}"
    );
}
