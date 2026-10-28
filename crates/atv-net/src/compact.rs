//! Independent compact state and baseline patches. World positions stay binary32;
//! bounded orientation/velocity/animation fields are explicitly quantized.
use crate::{Error, Platform, Result, RiderState, Snapshot, Transform};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationState {
    pub steering: f32,
    pub wheels: [f32; 4],
    pub compression: [f32; 4],
    pub clips: [u8; 4],
    pub count: u8,
    pub phase: f32,
    pub first_weight: f32,
    pub pair_weight: f32,
    pub look_phase: f32,
    pub paused: bool,
}
impl Default for AnimationState {
    fn default() -> Self {
        Self {
            steering: 0.,
            wheels: [0.; 4],
            compression: [0.; 4],
            clips: [4, 0, 0, 0],
            count: 1,
            phase: 0.5,
            first_weight: 0.,
            pair_weight: 0.,
            look_phase: -1.,
            paused: false,
        }
    }
}
/// Largest visual steering angle (rad) a pose may carry.
pub const STEERING_LIMIT: f32 = 3.9;
const STEERING_SCALE: f32 = 8192.;
/// The largest steering code whose decoded value stays within the limit
/// (3.9 * 8192 = 31948.8 rounds to 31949, which decodes as 3.900024).
fn steering_code_limit() -> f32 {
    (STEERING_LIMIT * STEERING_SCALE).floor()
}
impl AnimationState {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.count, 1 | 2 | 4)
            || self.clips.iter().any(|v| *v >= 64)
            || !self.steering.is_finite()
            || self.steering.abs() > STEERING_LIMIT
            || !self.wheels.iter().all(|v| v.is_finite())
            || !self
                .compression
                .iter()
                .all(|v| v.is_finite() && v.abs() < 32.)
            || ![self.phase, self.first_weight, self.pair_weight]
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
            || !self.look_phase.is_finite()
            || !(-1. ..=1.).contains(&self.look_phase)
        {
            return Err(Error::InvalidValue);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delta {
    pub base_sequence: u32,
    pub mask: u8,
    pub snapshot: Snapshot,
}
fn changed(a: &Snapshot, b: &Snapshot) -> Result<u8> {
    let mut mask = 0;
    for i in 0..4 {
        if block(a, i)? != block(b, i)? {
            mask |= 1 << i;
        }
    }
    Ok(mask)
}
pub fn delta(new: &Snapshot, base: &Snapshot) -> Result<Delta> {
    Ok(Delta {
        base_sequence: base.sequence,
        mask: changed(new, base)?,
        snapshot: new.clone(),
    })
}
pub fn apply_delta(p: &Delta, base: &Snapshot) -> Result<Snapshot> {
    if p.base_sequence != base.sequence
        || p.snapshot.peer != base.peer
        || p.snapshot.atv_model != base.atv_model
        || p.snapshot.rider_model != base.rider_model
        || p.mask & !15 != 0
    {
        return Err(Error::InvalidValue);
    }
    let mut out = base.clone();
    out.sequence = p.snapshot.sequence;
    out.time_ms = p.snapshot.time_ms;
    for i in 0..4 {
        if p.mask & (1 << i) != 0 {
            read_block(&mut out, i, &block(&p.snapshot, i)?)?;
        }
    }
    out.validate()?;
    Ok(out)
}
fn i16q(out: &mut Vec<u8>, v: f32, scale: f32) -> Result<()> {
    let n = (v * scale).round();
    if !v.is_finite() {
        return Err(Error::InvalidValue);
    }
    if !(-32767. ..=32767.).contains(&n) {
        // Rare, valid high-speed launches use an exact escape rather than
        // clipping motion or breaking the session. Normal fields stay compact.
        out.extend_from_slice(&i16::MIN.to_le_bytes());
        out.extend_from_slice(&v.to_le_bytes());
    } else {
        out.extend_from_slice(&(n as i16).to_le_bytes());
    }
    Ok(())
}
fn rigid(out: &mut Vec<u8>, t: Transform) -> Result<()> {
    t.validate()?;
    if t.scale.iter().any(|v| (v - 1.).abs() > 0.001) || t.shear.iter().any(|v| v.abs() > 0.001) {
        return Err(Error::InvalidTransform);
    }
    for v in t.position {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in t.rotation {
        i16q(out, v, 32767.)?;
    }
    Ok(())
}
fn vector(out: &mut Vec<u8>, v: [f32; 3]) -> Result<()> {
    for v in v {
        i16q(out, v, 8.)?;
    }
    Ok(())
}
fn block(s: &Snapshot, i: u8) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match i {
        0 => {
            rigid(&mut out, s.atv)?;
            vector(&mut out, s.velocity)?;
            for v in s.angular_velocity {
                i16q(&mut out, v, 256.)?;
            }
        }
        1 => {
            rigid(&mut out, s.rider)?;
            vector(&mut out, s.rider_velocity)?;
        }
        2 => {
            let a = s.animation.as_ref().ok_or(Error::InvalidValue)?;
            a.validate()?;
            // Clamp the code so the receiver's decoded value stays within
            // STEERING_LIMIT; rounding at the limit used to overshoot it.
            let limit = steering_code_limit();
            let code = (a.steering * STEERING_SCALE).round().clamp(-limit, limit);
            out.extend_from_slice(&(code as i16).to_le_bytes());
            for v in a.wheels {
                i16q(
                    &mut out,
                    (v + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI,
                    8192.,
                )?;
            }
            for v in a.compression {
                i16q(&mut out, v, 1024.)?;
            }
            out.extend_from_slice(&a.clips);
            out.push(a.count);
            i16q(&mut out, a.phase, 32767.)?;
            out.push((a.first_weight * 255.).round() as u8);
            out.push((a.pair_weight * 255.).round() as u8);
            i16q(&mut out, a.look_phase, 32767.)?;
            out.push(u8::from(a.paused));
        }
        3 => {
            i16q(&mut out, s.rpm, 0.25)?;
            i16q(&mut out, s.throttle, 32767.)?;
            i16q(&mut out, s.speed, 8.)?;
            out.extend_from_slice(&s.crash_serial.to_le_bytes());
            out.push(s.rider_state as u8);
            out.extend_from_slice(&s.train_time.unwrap_or(-1.).to_le_bytes());
            out.push(u8::from(s.train_running));
            out.push(s.platform.as_ref().map_or(255, |p| p.car));
            if let Some(p) = &s.platform {
                rigid(&mut out, p.transform)?;
            }
        }
        _ => return Err(Error::InvalidValue),
    }
    Ok(out)
}
struct Read<'a> {
    data: &'a [u8],
    at: usize,
}
impl Read<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let v = self
            .data
            .get(self.at..self.at + N)
            .ok_or(Error::Truncated)?;
        self.at += N;
        Ok(v.try_into().unwrap())
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    fn float(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take()?))
    }
    fn q(&mut self, scale: f32) -> Result<f32> {
        let n = i16::from_le_bytes(self.take()?);
        if n == i16::MIN {
            self.float()
        } else {
            Ok(f32::from(n) / scale)
        }
    }
    fn boolean(&mut self) -> Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidValue),
        }
    }
    fn rigid(&mut self) -> Result<Transform> {
        let mut t = Transform::default();
        for v in &mut t.position {
            *v = self.float()?;
        }
        for v in &mut t.rotation {
            *v = self.q(32767.)?;
        }
        let n = t.rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
        if !(0.98..=1.02).contains(&n) {
            return Err(Error::InvalidTransform);
        }
        for v in &mut t.rotation {
            *v /= n;
        }
        t.validate()?;
        Ok(t)
    }
    fn vector(&mut self) -> Result<[f32; 3]> {
        Ok([self.q(8.)?, self.q(8.)?, self.q(8.)?])
    }
}
fn read_block(s: &mut Snapshot, i: u8, data: &[u8]) -> Result<()> {
    let mut r = Read { data, at: 0 };
    match i {
        0 => {
            s.atv = r.rigid()?;
            s.velocity = r.vector()?;
            s.angular_velocity = [r.q(256.)?, r.q(256.)?, r.q(256.)?];
        }
        1 => {
            s.rider = r.rigid()?;
            s.rider_velocity = r.vector()?;
        }
        2 => {
            // Encoders before the clamp could round one code past the limit;
            // accept exactly that overshoot (clamped back) and nothing more.
            let n = i16::from_le_bytes(r.take()?);
            let limit = steering_code_limit();
            if n == i16::MIN || f32::from(n).abs() > limit + 1. {
                return Err(Error::InvalidValue);
            }
            let steering = (f32::from(n) / STEERING_SCALE).clamp(-STEERING_LIMIT, STEERING_LIMIT);
            let mut a = AnimationState {
                steering,
                ..Default::default()
            };
            for v in &mut a.wheels {
                *v = r.q(8192.)?;
            }
            for v in &mut a.compression {
                *v = r.q(1024.)?;
            }
            a.clips = r.take()?;
            a.count = r.byte()?;
            a.phase = r.q(32767.)?;
            a.first_weight = f32::from(r.byte()?) / 255.;
            a.pair_weight = f32::from(r.byte()?) / 255.;
            a.look_phase = r.q(32767.)?;
            a.paused = r.boolean()?;
            a.validate()?;
            s.animation = Some(Box::new(a));
        }
        3 => {
            s.rpm = r.q(0.25)?;
            s.throttle = r.q(32767.)?;
            s.speed = r.q(8.)?;
            s.crash_serial = u32::from_le_bytes(r.take()?);
            s.rider_state = match r.byte()? {
                0 => RiderState::Riding,
                1 => RiderState::Airborne,
                2 => RiderState::Wreck,
                3 => RiderState::Recovering,
                _ => return Err(Error::InvalidValue),
            };
            let time = r.float()?;
            s.train_time = if time == -1. { None } else { Some(time) };
            s.train_running = r.boolean()?;
            let car = r.byte()?;
            s.platform = if car == 255 {
                None
            } else {
                Some(Box::new(Platform {
                    car,
                    transform: r.rigid()?,
                }))
            };
        }
        _ => return Err(Error::InvalidValue),
    }
    if r.at != data.len() {
        return Err(Error::InvalidLength);
    }
    Ok(())
}
pub(crate) fn encode(s: &Snapshot, base: Option<&Delta>) -> Result<Vec<u8>> {
    s.validate()?;
    let mask = base.map_or(15, |d| d.mask);
    if mask & !15 != 0 {
        return Err(Error::InvalidValue);
    }
    let mut out = vec![s.peer];
    out.extend_from_slice(&s.sequence.to_le_bytes());
    out.extend_from_slice(&s.time_ms.to_le_bytes());
    out.extend_from_slice(&s.atv_model.to_le_bytes());
    out.extend_from_slice(&s.rider_model.to_le_bytes());
    out.extend_from_slice(&base.map_or(0, |d| d.base_sequence).to_le_bytes());
    out.push(mask);
    for i in 0..4 {
        if mask & (1 << i) != 0 {
            let v = block(s, i)?;
            out.push(v.len() as u8);
            out.extend_from_slice(&v);
        }
    }
    Ok(out)
}
pub(crate) fn decode(data: &[u8], patch: bool) -> Result<crate::Packet> {
    let mut r = Read { data, at: 0 };
    let mut s = Snapshot {
        peer: r.byte()?,
        sequence: u32::from_le_bytes(r.take()?),
        time_ms: u32::from_le_bytes(r.take()?),
        atv_model: u16::from_le_bytes(r.take()?),
        rider_model: u16::from_le_bytes(r.take()?),
        animation: Some(Box::default()),
        ..Default::default()
    };
    let base_sequence = u32::from_le_bytes(r.take()?);
    let mask = r.byte()?;
    if mask & !15 != 0 || (!patch && (mask != 15 || base_sequence != 0)) {
        return Err(Error::InvalidValue);
    }
    for i in 0..4 {
        if mask & (1 << i) != 0 {
            let len = usize::from(r.byte()?);
            let b = r.data.get(r.at..r.at + len).ok_or(Error::Truncated)?;
            r.at += len;
            read_block(&mut s, i, b)?;
        }
    }
    if r.at != data.len() {
        return Err(Error::InvalidLength);
    }
    s.validate()?;
    Ok(if patch {
        crate::Packet::Delta(Box::new(Delta {
            base_sequence,
            mask,
            snapshot: s,
        }))
    } else {
        crate::Packet::Snapshot((s).into())
    })
}
