// Presentation only: not part of the deterministic simulation (clippy.toml).
#![allow(clippy::disallowed_methods)]
//! Property-style round trips of the compact pose codec: every value the
//! sender's validation accepts must survive quantization and still pass the
//! receiver's validation. Steering at the 3.9 limit used to decode as 3.900024
//! and fail validation (latent: ATV visual steering stays near 0.52 rad).
use atv_net::*;

/// Small deterministic xorshift generator (no dependencies).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

fn base() -> Snapshot {
    Snapshot {
        sequence: 7,
        time_ms: 1000,
        animation: Some(Box::default()),
        ..Snapshot::default()
    }
}

fn roundtrip(s: &Snapshot) -> Snapshot {
    s.validate().expect("sender-side value must be valid");
    let bytes = encode(&Packet::Snapshot(Box::new(s.clone()))).expect("encode");
    match decode(&bytes) {
        Ok(Packet::Snapshot(out)) => {
            out.validate().expect("decoded snapshot must validate");
            *out
        }
        other => panic!("decode failed for {s:?}: {other:?}"),
    }
}

/// Values at, just inside and one f32 ulp inside each validated limit.
fn near(limit: f32) -> Vec<f32> {
    let below = f32::from_bits(limit.to_bits() - 1);
    let mut v = vec![limit, below, limit - 1e-4, limit - 1e-3, limit * 0.999];
    v.extend(v.clone().iter().map(|x| -x));
    v.push(0.);
    v.push(-0.);
    v
}

#[test]
fn steering_at_its_limit_survives_quantization() {
    for steering in near(3.9) {
        let mut s = base();
        s.animation.as_mut().unwrap().steering = steering;
        let out = roundtrip(&s);
        let got = out.animation.unwrap().steering;
        assert!(got.abs() <= 3.9, "{steering} decoded as {got}");
        assert!((got - steering).abs() <= 1. / 8192., "{steering} -> {got}");
    }
}

#[test]
fn every_bounded_field_at_its_limits_roundtrips() {
    let edit: Vec<(&str, Vec<f32>, fn(&mut Snapshot, f32))> = vec![
        ("steering", near(3.9), |s, v| {
            s.animation.as_mut().unwrap().steering = v
        }),
        ("wheel", near(1000.), |s, v| {
            s.animation.as_mut().unwrap().wheels = [v; 4]
        }),
        ("compression", near(31.999_99), |s, v| {
            s.animation.as_mut().unwrap().compression = [v; 4]
        }),
        ("phase", near(1.), |s, v| {
            s.animation.as_mut().unwrap().phase = v.abs()
        }),
        ("first_weight", near(1.), |s, v| {
            s.animation.as_mut().unwrap().first_weight = v.abs()
        }),
        ("pair_weight", near(1.), |s, v| {
            s.animation.as_mut().unwrap().pair_weight = v.abs()
        }),
        ("look_phase", near(1.), |s, v| {
            s.animation.as_mut().unwrap().look_phase = v
        }),
        ("velocity", near(100_000.), |s, v| s.velocity = [v; 3]),
        ("rider_velocity", near(100_000.), |s, v| {
            s.rider_velocity = [v; 3]
        }),
        ("angular_velocity", near(1000.), |s, v| {
            s.angular_velocity = [v; 3]
        }),
        ("rpm", near(100_000.), |s, v| s.rpm = v.abs()),
        ("throttle", near(1.), |s, v| s.throttle = v),
        ("speed", near(100_000.), |s, v| s.speed = v),
        ("train_time", near(86_400.), |s, v| {
            s.train_time = Some(v.abs())
        }),
    ];
    for (name, values, set) in edit {
        // Also cover the low end of [0, x] ranges with tiny positives.
        let extra = [1e-7_f32, 1e-5, 0.5 / 32767., 0.5 / 8192.];
        for v in values.into_iter().chain(extra) {
            let mut s = base();
            set(&mut s, v);
            if s.validate().is_err() {
                continue; // e.g. negative extra on a signed field's sign flip
            }
            let bytes = encode(&Packet::Snapshot(Box::new(s.clone()))).unwrap();
            let out = decode(&bytes);
            assert!(
                matches!(&out, Ok(Packet::Snapshot(o)) if o.validate().is_ok()),
                "{name}={v}: {out:?}"
            );
        }
    }
}

#[test]
fn random_valid_poses_roundtrip_and_validate() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..20_000 {
        let mut s = base();
        let a = s.animation.as_mut().unwrap();
        a.steering = r.range(-3.9, 3.9);
        a.wheels = std::array::from_fn(|_| r.range(-50., 50.));
        a.compression = std::array::from_fn(|_| r.range(-31.99, 31.99));
        a.clips = std::array::from_fn(|_| (r.next() % 64) as u8);
        a.count = [1, 2, 4][(r.next() % 3) as usize];
        a.phase = r.unit();
        a.first_weight = r.unit();
        a.pair_weight = r.unit();
        a.look_phase = r.range(-1., 1.);
        a.paused = r.next() & 1 == 1;
        s.velocity = std::array::from_fn(|_| r.range(-6000., 6000.));
        s.rider_velocity = std::array::from_fn(|_| r.range(-6000., 6000.));
        s.angular_velocity = std::array::from_fn(|_| r.range(-200., 200.));
        s.rpm = r.range(0., 12_000.);
        s.throttle = r.range(-1., 1.);
        s.speed = r.range(-6000., 6000.);
        let q: [f32; 4] = std::array::from_fn(|_| r.range(-1., 1.));
        let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-3);
        s.atv.rotation = q.map(|v| v / n);
        s.atv.position = std::array::from_fn(|_| r.range(-50_000., 50_000.));
        if s.atv.validate().is_err() {
            s.atv.rotation = [1., 0., 0., 0.];
        }
        roundtrip(&s);
    }
}

#[test]
fn steering_beyond_one_quantization_step_is_still_rejected() {
    // Hand-built wire value: the largest legitimate old-sender overshoot
    // (31949 / 8192 = 3.900024) is tolerated and clamped, anything further
    // and non-finite escapes are rejected.
    let mut s = base();
    s.animation.as_mut().unwrap().steering = 3.;
    let bytes = encode(&Packet::Snapshot(Box::new(s))).unwrap();
    let steer_at = bytes.len() - animation_tail_from_steering();
    assert_eq!(
        i16::from_le_bytes([bytes[steer_at], bytes[steer_at + 1]]),
        3 * 8192
    );
    let with = |n: i16| {
        let mut b = bytes.clone();
        b[steer_at..steer_at + 2].copy_from_slice(&n.to_le_bytes());
        decode(&b)
    };
    match with(31949) {
        Ok(Packet::Snapshot(o)) => assert_eq!(o.animation.unwrap().steering, 3.9),
        other => panic!("{other:?}"),
    }
    match with(-31949) {
        Ok(Packet::Snapshot(o)) => assert_eq!(o.animation.unwrap().steering, -3.9),
        other => panic!("{other:?}"),
    }
    assert_eq!(with(31950), Err(Error::InvalidValue));
    assert_eq!(with(-31950), Err(Error::InvalidValue));
    assert_eq!(with(32767), Err(Error::InvalidValue));
}

/// Bytes from the steering field to the end of a full default snapshot:
/// block 2 (steering .. paused) + block 3 (len byte + payload).
fn animation_tail_from_steering() -> usize {
    let block2 = 2 + 4 * 2 + 4 * 2 + 4 + 1 + 2 + 1 + 1 + 2 + 1;
    let block3 = 1 + 2 + 2 + 2 + 4 + 1 + 4 + 1 + 1;
    block2 + block3
}
