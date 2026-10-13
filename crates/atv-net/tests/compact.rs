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
