//!Native packed-eight-byte rotation keys used by Arlington's locomotive MOT.
use crate::{
    Result,
    animation::{Channels, Track, TranslationKey},
    mesh::Node,
};
#[derive(Debug)]
struct RotationKey {
    time: f32,
    angle: u8,
    axis: [i8; 3],
}
impl RotationKey {
    fn quaternion(&self) -> [f32; 4] {
        //001cb7e0 constructs the256-entry table;001cbb10 does no additional
        //normalization. Host sin/cos is the same documented backend limitation
        //as the existing ShortAxis MOT reader.
        let angle = f32::from(self.angle) * f32::from_bits(0x3c490fdb);
        let scale = angle.sin() * f32::from_bits(0x3c010204);
        [
            angle.cos(),
            f32::from(self.axis[0]) * scale,
            f32::from(self.axis[1]) * scale,
            f32::from(self.axis[2]) * scale,
        ]
    }
}
pub struct Motion {
    pub duration: f32,
    names: Vec<String>,
    translations: Vec<Track<TranslationKey>>,
    rotations: Vec<Track<RotationKey>>,
}
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn word(&mut self) -> Result<u32> {
        let v = crate::u32le(self.data, self.at)?;
        self.at += 4;
        Ok(v)
    }
    fn float(&mut self) -> Result<f32> {
        let v = f32::from_bits(self.word()?);
        if !v.is_finite() {
            return Err("nonfinite train MOT value".into());
        }
        Ok(v)
    }
    fn point(&mut self) -> Result<[f32; 3]> {
        Ok([self.float()?, self.float()?, self.float()?])
    }
    fn header(&mut self, names: usize) -> Result<(usize, u32)> {
        let count = self.word()? as usize;
        let bone = self.word()?;
        if count == 0 || count > 65536 || bone as usize >= names {
            return Err("invalid train MOT controller".into());
        }
        Ok((count, bone))
    }
}
impl Motion {
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() > 16 * 1024 * 1024 {
            return Err("train MOT exceeds limit".into());
        }
        let mut r = Reader { data, at: 0 };
        if r.word()? != (-3_i32) as u32 {
            return Err("train wheels require named MOT v3".into());
        }
        let count = r.word()?;
        let names_count = r.word()? as usize;
        if count == 0 || names_count == 0 || names_count > 256 {
            return Err("invalid train MOT dimensions".into());
        }
        let mut names = Vec::new();
        for _ in 0..names_count {
            let n = r.word()? as usize;
            if !(2..=256).contains(&n) {
                return Err("invalid train MOT bone length".into());
            }
            let b = crate::slice(data, r.at, n)?;
            r.at += n;
            if b[n - 1] != 0 || b[..n - 1].contains(&0) {
                return Err("invalid train MOT name".into());
            }
            names.push(
                std::str::from_utf8(&b[..n - 1])
                    .map_err(|_| "train MOT name is not UTF8")?
                    .to_owned(),
            );
        }
        let duration = r.float()?;
        let rotation = r.word()?;
        let translation = r.word()?;
        let nt = r.word()? as usize;
        let nr = r.word()? as usize;
        if duration <= 0.
            || duration > 3600.
            || rotation != 0
            || translation != 0
            || nt > 256
            || nr > 256
        {
            return Err("unsupported train wheel MOT encoding".into());
        }
        let mut translations = Vec::new();
        for _ in 0..nt {
            let (count, bone) = r.header(names_count)?;
            let mut keys = Vec::new();
            for _ in 0..count {
                let time = r.float()?;
                if time < 0.
                    || time > duration
                    || keys.last().is_some_and(|k: &TranslationKey| k.time > time)
                {
                    return Err("invalid train translation time".into());
                }
                keys.push(TranslationKey {
                    time,
                    value: r.point()?,
                    tangents: None,
                });
            }
            translations.push(Track { bone, keys });
        }
        let mut rotations = Vec::new();
        for _ in 0..nr {
            let (count, bone) = r.header(names_count)?;
            let mut keys = Vec::new();
            for _ in 0..count {
                let time = r.float()?;
                if time < 0.
                    || time > duration
                    || keys.last().is_some_and(|k: &RotationKey| k.time > time)
                {
                    return Err("invalid train rotation time".into());
                }
                let packed = r.word()?.to_le_bytes();
                keys.push(RotationKey {
                    time,
                    angle: packed[0],
                    axis: [packed[1] as i8, packed[2] as i8, packed[3] as i8],
                });
            }
            rotations.push(Track { bone, keys });
        }
        if r.at != data.len() {
            return Err("unexpected train MOT trailing data".into());
        }
        Ok(Self {
            duration,
            names,
            translations,
            rotations,
        })
    }
    pub fn pose(&self, nodes: &[Node], time: f32) -> Result<Vec<[f32; 16]>> {
        if !time.is_finite() {
            return Err("nonfinite train wheel time".into());
        }
        let time = time % self.duration;
        let mut channels = Channels {
            translations: vec![None; nodes.len()],
            rotations: vec![None; nodes.len()],
        };
        let bone = |index: u32| {
            nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(&self.names[index as usize]))
                .ok_or("train MOT bone absent from wheel model")
        };
        for track in &self.translations {
            let (a, b, t) = bracket(&track.keys, time, |k| k.time);
            channels.translations[bone(track.bone)?] = Some(std::array::from_fn(|i| {
                track.keys[a].value[i] + t * (track.keys[b].value[i] - track.keys[a].value[i])
            }));
        }
        for track in &self.rotations {
            let (a, b, t) = bracket(&track.keys, time, |k| k.time);
            let qa = track.keys[a].quaternion();
            let qb = track.keys[b].quaternion();
            let mut q = if a == b {
                qa
            } else {
                crate::animation::blend_rotation(qa, qb, t)
            };
            if q.iter().zip(qb).map(|(a, b)| a * b).sum::<f32>() < 0. {
                q = q.map(|v| -v)
            }
            channels.rotations[bone(track.bone)?] = Some(q);
        }
        Ok(channels.pose(nodes)?.world_matrices)
    }
}
fn bracket<T>(keys: &[T], time: f32, at: impl Fn(&T) -> f32) -> (usize, usize, f32) {
    if time <= at(&keys[0]) {
        return (0, 0, 0.);
    }
    let b = keys.partition_point(|k| at(k) <= time);
    if b == keys.len() {
        return (b - 1, b - 1, 0.);
    }
    let a = b - 1;
    (a, b, (time - at(&keys[a])) / (at(&keys[b]) - at(&keys[a])))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_packed_axis_bytes_are_signed_and_not_renormalized() {
        let q = RotationKey {
            time: 0.,
            angle: 0,
            axis: [127, -128, 0],
        }
        .quaternion();
        assert_eq!(q, [1., 0., -0., 0.]);
        let q = RotationKey {
            time: 0.,
            angle: 128,
            axis: [-128, 0, 0],
        }
        .quaternion();
        assert!(q[0] < 0. && q[0].abs() < 0.000001);
        assert_eq!(q[1], -128_f32 * f32::from_bits(0x3c010204));
        assert!(q[1].abs() > 1.);
    }
    #[test]
    fn named_eight_byte_keys_validate_every_truncated_prefix() {
        let mut bytes = Vec::new();
        for word in [(-3_i32) as u32, 2, 1, 5] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(b"Root\0");
        for word in [
            1_f32.to_bits(),
            0,
            0,
            0,
            1,
            2,
            0,
            0,
            0x00007f00,
            1_f32.to_bits(),
            0x00007f40,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        let motion = Motion::decode(&bytes).unwrap();
        assert_eq!(motion.rotations.len(), 1);
        assert_eq!(motion.rotations[0].keys.len(), 2);
        for prefix in 0..bytes.len() {
            assert!(Motion::decode(&bytes[..prefix]).is_err(), "prefix{prefix}");
        }
    }
}
