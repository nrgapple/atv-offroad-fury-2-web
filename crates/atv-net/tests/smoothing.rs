// Presentation only: not part of the deterministic simulation (clippy.toml).
#![allow(clippy::disallowed_methods)]
use atv_net::*;

const FRAME_MS: u64 = 16;

fn moving(sequence: u32, time_ms: u32, position: [f32; 3], velocity: [f32; 3]) -> Snapshot {
    let mut s = Snapshot {
        sequence,
        time_ms,
        ..Snapshot::default()
    };
    s.atv.position = position;
    s.velocity = velocity;
    s.rider.position = [position[0], position[1] + 1., position[2]];
    s.rider_velocity = velocity;
    s
}
/// Deterministic uniform values in [-1, 1].
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2. - 1.
    }
}
/// Packets (snapshot, arrival time) for a straight 1D path at `speed` ft/s.
fn stream(
    cadence: u32,
    count: u32,
    jitter_ms: f64,
    velocity_noise: f32,
    speed: f32,
    seed: u64,
) -> Vec<(Snapshot, u64)> {
    let mut rng = Lcg(seed);
    let mut out: Vec<(Snapshot, u64)> = (0..count)
        .map(|n| {
            let t = n * cadence;
            let x = speed * t as f32 / 1000.;
            let v = speed * (1. + velocity_noise * rng.next() as f32);
            let arrival = (5000. + f64::from(t) + 30. + jitter_ms * rng.next()).round() as u64;
            (moving(n + 1, t, [x, 0., 0.], [v, 0., 0.]), arrival)
        })
        .collect();
    out.sort_by_key(|(_, arrival)| *arrival);
    out
}

#[test]
fn adaptive_delay_converges_for_common_cadences_and_stays_bounded() {
    for cadence in [50u32, 100] {
        let packets = stream(cadence, 8000 / cadence, 5., 0., 20., 7);
        let mut buffer = PoseBuffer::default();
        let mut next = 0;
        let mut previous_x = f32::MIN;
        let mut now = 5000;
        while now < 13_000 {
            while next < packets.len() && packets[next].1 <= now {
                buffer
                    .push(packets[next].0.clone(), packets[next].1)
                    .unwrap();
                next += 1;
            }
            if let Some(pose) = buffer.present(now, 0.) {
                let delay = buffer.presented_delay_ms();
                assert!((45. ..=220.).contains(&delay), "{cadence}: {delay}");
                let x = pose.snapshot.atv.position[0];
                assert!(x >= previous_x - 1e-4, "{cadence}: {x} < {previous_x}");
                previous_x = x;
            }
            now += FRAME_MS;
        }
        let target = buffer.target_delay_ms();
        println!("{cadence} ms cadence: target delay {target:.1} ms");
        let expected = cadence as f32 + 8.;
        assert!(
            (expected..=expected + 25.).contains(&target),
            "{cadence}: target {target}"
        );
        assert!(
            (buffer.presented_delay_ms() - target).abs() < 5.,
            "{cadence}: presented {} target {target}",
            buffer.presented_delay_ms()
        );
    }
    // Extreme jitter and very fast cadence stay inside the clamp.
    for (cadence, jitter, bound) in [(100u32, 200., 220f32), (20, 0., 45.)] {
        let packets = stream(cadence, 6000 / cadence, jitter, 0., 20., 3);
        let mut buffer = PoseBuffer::default();
        let mut previous = None;
        for (snapshot, arrival) in packets {
            // Late reordered packets are rejected, exactly as on the wire.
            let _ = buffer.push(snapshot, arrival);
            let pose = buffer.present(arrival, 0.).unwrap();
            assert!((45. ..=220.).contains(&buffer.presented_delay_ms()));
            if let Some(previous) = previous {
                assert!(pose.snapshot.atv.position[0] >= previous - 1e-3);
            }
            previous = Some(pose.snapshot.atv.position[0]);
        }
        assert!((buffer.target_delay_ms() - bound).abs() < 1., "{cadence}");
    }
}

#[test]
fn jittered_ten_hertz_presentation_is_monotonic_and_far_smoother_than_fixed_delay() {
    // Straight constant-speed path at 10 Hz with +-30 ms arrival jitter. The
    // reported velocity carries +-25% noise, as physics velocity does on bumpy
    // terrain; with exact velocities both paths extrapolate a line perfectly.
    let speed = 30.;
    let packets = stream(100, 120, 30., 0.25, speed, 42);
    let mut adaptive = PoseBuffer::default();
    let mut fixed = PoseBuffer::default();
    let (mut next, mut now) = (0, 5000);
    let (mut last_adaptive, mut last_fixed) = (None::<f32>, None::<f32>);
    let (mut worst_adaptive, mut worst_fixed) = (0f32, 0f32);
    let step = speed * FRAME_MS as f32 / 1000.;
    while now < 16_000 {
        while next < packets.len() && packets[next].1 <= now {
            let (snapshot, arrival) = &packets[next];
            assert!(adaptive.push(snapshot.clone(), *arrival).unwrap());
            assert!(fixed.push(snapshot.clone(), *arrival).unwrap());
            next += 1;
        }
        if let (Some(a), Some(f)) = (adaptive.present(now, 0.), fixed.sample(now)) {
            let (a, f) = (a.snapshot.atv.position, f.snapshot.atv.position);
            if let (Some(la), Some(lf)) = (last_adaptive, last_fixed) {
                assert!(a[0] >= la - 1e-4, "adaptive moved backwards at {now}");
                if now >= 8000 {
                    worst_adaptive = worst_adaptive.max((a[0] - la - step).abs());
                    worst_fixed = worst_fixed.max((f[0] - lf - step).abs());
                }
            }
            last_adaptive = Some(a[0]);
            last_fixed = Some(f[0]);
        }
        now += FRAME_MS;
    }
    println!(
        "max 16 ms step error: present {worst_adaptive:.4} ft, fixed sample {worst_fixed:.4} ft"
    );
    assert!(worst_fixed > 0.2, "fixed {worst_fixed}");
    assert!(
        worst_adaptive * 3. < worst_fixed,
        "present {worst_adaptive} vs fixed {worst_fixed}"
    );
}

#[test]
fn hermite_follows_ballistic_parabola_where_chords_do_not() {
    let gravity = -32.17f32;
    let v0 = [25f32, 40., -3.];
    let at = |t: f32| -> ([f32; 3], [f32; 3]) {
        (
            [v0[0] * t, v0[1] * t + 0.5 * gravity * t * t, v0[2] * t],
            [v0[0], v0[1] + gravity * t, v0[2]],
        )
    };
    let mut buffer = PoseBuffer::new(SmoothingConfig {
        delay_ms: 0,
        ..SmoothingConfig::default()
    });
    for n in 0..4u32 {
        let (p, v) = at(n as f32 * 0.1);
        let mut s = moving(n + 1, n * 100, p, v);
        s.rider_state = RiderState::Airborne;
        buffer.push(s, u64::from(n * 100)).unwrap();
    }
    for ms in [125u32, 150, 175] {
        let t = ms as f32 / 1000.;
        let (truth, _) = at(t);
        let pose = buffer.sample(u64::from(ms)).unwrap().snapshot;
        for i in 0..3 {
            assert!(
                (pose.atv.position[i] - truth[i]).abs() < 2e-3,
                "{ms} {i}: {:?} vs {truth:?}",
                pose.atv.position
            );
        }
        assert!((pose.rider.position[1] - truth[1] - 1.).abs() < 2e-3);
        // A chord would sit gravity * dt^2 * alpha * (1 - alpha) / 2 above the arc.
        let alpha = (t - 0.1) / 0.1;
        let (a, _) = at(0.1);
        let (b, _) = at(0.2);
        let chord = a[1] + (b[1] - a[1]) * alpha;
        assert!((chord - truth[1]).abs() > 0.02);
    }
}

#[test]
fn correction_offset_decays_and_resets_on_crash_or_teleport() {
    for reset in 0..2 {
        let mut buffer = PoseBuffer::default();
        let velocity = [20., 0., 0.];
        let mut now = 1000;
        let mut sequence = 0;
        let mut send = |buffer: &mut PoseBuffer, now: u64, z: f32| -> Snapshot {
            sequence += 1;
            let t = now as f32 / 1000.;
            let s = moving(sequence, now as u32, [20. * t, 0., z], velocity);
            buffer.push(s.clone(), now).unwrap();
            s
        };
        let mut last = None;
        // near = 1: present the current estimate, so trajectory changes land on
        // the frame being shown.
        while now < 2000 {
            if now % 100 == 0 {
                send(&mut buffer, now, 0.);
            }
            last = buffer.present(now, 1.).map(|p| p.snapshot);
            now += 4;
        }
        assert!(buffer.correction_ft() < 1e-3);
        // The rider swerved 2 ft sideways since the last packet.
        let before = last.unwrap().atv.position;
        send(&mut buffer, now, 2.);
        let shown = buffer.present(now, 1.).unwrap().snapshot.atv.position;
        assert!(
            (shown[2] - before[2]).abs() < 0.2,
            "{before:?} -> {shown:?}"
        );
        let initial = buffer.correction_ft();
        assert!(initial > 1., "{initial}");
        let mut previous = initial;
        for _ in 0..3 {
            now += 4;
            buffer.present(now, 1.).unwrap();
            assert!(buffer.correction_ft() < previous);
            previous = buffer.correction_ft();
        }
        assert!(previous > 0.5);
        let mut s = moving(sequence + 1, now as u32 + 4, [0.; 3], velocity);
        s.atv.position = [20. * (now + 4) as f32 / 1000., 0., 2.];
        if reset == 0 {
            s.crash_serial = 1;
            s.rider_state = RiderState::Wreck;
        } else {
            s.atv.position[0] += 50.;
        }
        now += 4;
        buffer.push(s.clone(), now).unwrap();
        let pose = buffer.present(now, 1.).unwrap().snapshot;
        assert_eq!(buffer.correction_ft(), 0., "reset mode {reset}");
        assert_eq!(pose.crash_serial, s.crash_serial);
    }
    // Decay without further surprises.
    let mut buffer = PoseBuffer::default();
    let mut now = 0;
    let mut sent = 0u32;
    while now < 1600 {
        while u64::from(sent * 100) <= now {
            let t = sent * 100;
            let z = if t >= 1000 { 3. } else { 0. };
            let x = 20. * t as f32 / 1000.;
            buffer
                .push(moving(sent + 1, t, [x, 0., z], [20., 0., 0.]), u64::from(t))
                .unwrap();
            sent += 1;
        }
        let before = buffer.correction_ft();
        buffer.present(now, 1.);
        if now < 1000 {
            assert!(buffer.correction_ft() < 1e-3);
        } else if now < 1016 {
            assert!(buffer.correction_ft() > 2., "{}", buffer.correction_ft());
        } else {
            assert!(buffer.correction_ft() < before || before == 0.);
        }
        now += FRAME_MS;
    }
    assert!(buffer.correction_ft() < 0.02, "{}", buffer.correction_ft());
}

#[test]
fn near_rider_presents_current_estimate_instead_of_delayed_history() {
    let packets = stream(100, 40, 10., 0., 25., 11);
    let mut far = PoseBuffer::default();
    let mut close = PoseBuffer::default();
    let (mut next, mut now) = (0, 5000);
    let (mut far_pose, mut close_pose) = (None, None);
    while now < 8500 {
        while next < packets.len() && packets[next].1 <= now {
            far.push(packets[next].0.clone(), packets[next].1).unwrap();
            close
                .push(packets[next].0.clone(), packets[next].1)
                .unwrap();
            next += 1;
        }
        far_pose = far.present(now, 0.);
        close_pose = close.present(now, 1.);
        now += FRAME_MS;
    }
    now -= FRAME_MS;
    let current = close.sample_current(now).unwrap().snapshot.atv.position[0];
    let close_x = close_pose.unwrap().snapshot.atv.position[0];
    let far_x = far_pose.unwrap().snapshot.atv.position[0];
    assert!(close.presented_delay_ms() < 1.);
    assert!(far.presented_delay_ms() > 100.);
    assert!((close_x - current).abs() < 0.1, "{close_x} vs {current}");
    assert!(current - far_x > 2., "{far_x} vs {current}");
}

#[test]
fn near_changes_are_slew_limited_and_never_reverse_time() {
    let packets = stream(100, 60, 20., 0., 25., 5);
    let mut buffer = PoseBuffer::default();
    let (mut next, mut now) = (0, 5000);
    let mut previous: Option<f32> = None;
    while now < 11_000 {
        while next < packets.len() && packets[next].1 <= now {
            buffer
                .push(packets[next].0.clone(), packets[next].1)
                .unwrap();
            next += 1;
        }
        // Rider repeatedly snaps between far and touching.
        let near = if (now / 700) % 2 == 0 { 0. } else { 1. };
        let Some(pose) = buffer.present(now, near) else {
            now += FRAME_MS;
            continue;
        };
        let x = pose.snapshot.atv.position[0];
        if let Some(previous) = previous {
            let moved = x - previous;
            let step = 25. * FRAME_MS as f32 / 1000.;
            assert!(moved >= -1e-4 && moved <= step * 1.6, "{now}: {moved}");
        }
        previous = Some(x);
        now += FRAME_MS;
    }
}

#[test]
fn extrapolation_is_longer_but_bounded_and_rotates() {
    let mut s = moving(1, 100, [0.; 3], [10., 0., 0.]);
    s.angular_velocity = [0., 1., 0.];
    let mut buffer = PoseBuffer::default();
    buffer.push(s.clone(), 100).unwrap();
    let current = buffer.sample_current(300).unwrap();
    assert_eq!(current.extrapolated_ms, 200);
    assert!((current.snapshot.atv.position[0] - 2.).abs() < 1e-4);
    assert!((current.snapshot.atv.rotation[2] - 0.1f32.sin()).abs() < 1e-4);
    let capped = buffer.sample_current(2000).unwrap();
    assert_eq!(capped.extrapolated_ms, 250);
    assert!(capped.stale);
    let mut wide = PoseBuffer::new(SmoothingConfig {
        extrapolation_ms: 5000,
        ..SmoothingConfig::default()
    });
    wide.push(s, 100).unwrap();
    assert_eq!(wide.sample_current(5000).unwrap().extrapolated_ms, 300);
    let mut extrapolated = 0;
    for n in 0..100 {
        extrapolated = wide
            .present(100 + n * FRAME_MS, 1.)
            .unwrap()
            .extrapolated_ms;
        assert!(extrapolated <= 300);
    }
    assert_eq!(extrapolated, 300);
}

#[test]
fn clear_resets_presentation_and_platform_poses_never_keep_offsets() {
    let mut buffer = PoseBuffer::default();
    let mut s = moving(1, 0, [12., 0., 0.], [20., 0., 0.]);
    s.train_time = Some(1.);
    s.platform = Some(Box::new(Platform {
        car: 1,
        transform: Transform::default(),
    }));
    buffer.push(s.clone(), 0).unwrap();
    for n in 0..10 {
        let pose = buffer.present(n * FRAME_MS, 1.).unwrap();
        assert_eq!(pose.snapshot.atv.position, s.atv.position);
    }
    s.sequence = 2;
    s.time_ms = 100;
    s.atv.position[0] = 40.;
    buffer.push(s.clone(), 160).unwrap();
    buffer.present(176, 1.).unwrap();
    assert_eq!(buffer.correction_ft(), 0.);
    buffer.clear();
    assert!(buffer.present(200, 0.).is_none());
    assert!((buffer.presented_delay_ms() - buffer.target_delay_ms()).abs() < 1e-6);
}
