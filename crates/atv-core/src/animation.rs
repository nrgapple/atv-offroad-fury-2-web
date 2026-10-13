//! MOT v2/v3 animation streams, reconstructed from the exact build's loader.
//! Controller timing and game-side blending remain separate from file decoding.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Serialize)]
pub struct TranslationKey {
    pub time: f32,
    pub value: [f32; 3],
    /// Stored incoming/outgoing cubic tangents for Hermite controllers.
    pub tangents: Option<[[f32; 3]; 2]>,
}
#[derive(Debug, Serialize)]
pub struct RotationKey {
    pub time: f32,
    pub packed_angle: u16,
    pub packed_axis: [i16; 3],
    /// SLUS_211.04 angle byte plus three signed axis bytes.
    pub compressed_axis: Option<u32>,
}
#[derive(Debug, Serialize)]
pub struct Track<T> {
    pub bone: u32,
    pub keys: Vec<T>,
}
#[derive(Debug, Serialize)]
pub struct Animation {
    pub version: u32,
    pub frame_count: u32,
    pub duration: f32,
    /// Version 3 indices address this name table. Version 2 uses model indices.
    pub bone_names: Vec<String>,
    pub translation_kind: u32,
    pub translations: Vec<Track<TranslationKey>>,
    pub rotations: Vec<Track<RotationKey>>,
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn u32(&mut self) -> Result<u32> {
        let v = u32le(self.data, self.at)?;
        self.at += 4;
        Ok(v)
    }
    fn float(&mut self) -> Result<f32> {
        let value = f32::from_bits(self.u32()?);
        if !value.is_finite() {
            return Err("nonfinite MOT value".into());
        }
        Ok(value)
    }
    fn vector(&mut self) -> Result<[f32; 3]> {
        Ok([self.float()?, self.float()?, self.float()?])
    }
    fn key_time(&mut self, previous: f32, duration: f32) -> Result<f32> {
        let time = self.float()?;
        if time < 0. || time < previous || time > duration + 0.0001 {
            return Err("invalid MOT key time".into());
        }
        Ok(time)
    }
    fn track_header(&mut self, bone_names: usize, total: &mut usize) -> Result<(usize, u32)> {
        let count = self.u32()? as usize;
        let bone = self.u32()?;
        if count == 0
            || count > 1_000_000
            || bone >= 256
            || (bone_names != 0 && bone as usize >= bone_names)
        {
            return Err("invalid MOT key count or bone index".into());
        }
        *total += count;
        if *total > 1_000_000 {
            return Err("MOT exceeds one million keys".into());
        }
        Ok((count, bone))
    }
}

pub fn decode_mot(data: &[u8]) -> Result<Animation> {
    if data.len() > 128 * 1024 * 1024 {
        return Err("MOT exceeds 128 MiB limit".into());
    }
    let mut r = Reader { data, at: 0 };
    let version = match r.u32()? as i32 {
        -2 => 2,
        -3 => 3,
        -6 => 6,
        -7 => 7,
        _ => return Err("unsupported MOT version".into()),
    };
    if version >= 6 && r.u32()? != 1 {
        return Err("unsupported MOT v7 flags".into());
    }
    let frame_count = r.u32()?;
    if frame_count == 0 || frame_count > 1_000_000 {
        return Err("invalid MOT frame count".into());
    }
    let mut bone_names = Vec::new();
    if matches!(version, 3 | 6 | 7) {
        let count = r.u32()?;
        if count == 0 || count > 256 {
            return Err("invalid MOT bone-name count".into());
        }
        let mut seen = BTreeSet::new();
        for _ in 0..count {
            let length = r.u32()? as usize;
            if !(2..=256).contains(&length) {
                return Err("invalid MOT bone-name length".into());
            }
            let bytes = slice(data, r.at, length)?;
            r.at += length;
            if bytes[length - 1] != 0 || bytes[..length - 1].contains(&0) {
                return Err("invalid MOT bone-name terminator".into());
            }
            let name = std::str::from_utf8(&bytes[..length - 1])
                .map_err(|_| "non-UTF8 MOT bone name")?
                .to_owned();
            if !seen.insert(name.to_ascii_lowercase()) {
                return Err("duplicate MOT bone name".into());
            }
            bone_names.push(name);
        }
    }
    let duration = r.float()?;
    if !(0.0..=3600.0).contains(&duration) {
        return Err("invalid MOT duration".into());
    }
    let rotation_kind = r.u32()?;
    let translation_kind = r.u32()?;
    let translation_count = r.u32()?;
    let rotation_count = r.u32()?;
    if rotation_kind > 1 || translation_kind > 1 {
        return Err("unsupported MOT controller encoding".into());
    }
    if translation_count > 256 || rotation_count > 256 {
        return Err("too many MOT controllers".into());
    }
    if version >= 6 && (r.u32()? != 0 || r.u32()? != 0) {
        return Err("unsupported MOT event controllers".into());
    }
    let mut total_keys = 0;
    let mut translations = Vec::new();
    for _ in 0..translation_count {
        let (count, bone) = r.track_header(bone_names.len(), &mut total_keys)?;
        slice(
            data,
            r.at,
            count * if translation_kind == 0 { 16 } else { 40 },
        )?;
        let mut keys = Vec::with_capacity(count);
        let mut previous = 0.;
        for _ in 0..count {
            let time = r.key_time(previous, duration)?;
            previous = time;
            let value = r.vector()?;
            let tangents = if translation_kind == 1 {
                Some([r.vector()?, r.vector()?])
            } else {
                None
            };
            keys.push(TranslationKey {
                time,
                value,
                tangents,
            });
        }
        translations.push(Track { bone, keys });
    }
    let mut rotations = Vec::new();
    for _ in 0..rotation_count {
        let (count, bone) = r.track_header(bone_names.len(), &mut total_keys)?;
        slice(data, r.at, count * if rotation_kind == 0 { 8 } else { 12 })?;
        let mut keys = Vec::with_capacity(count);
        let mut previous = 0.;
        for _ in 0..count {
            let time = r.key_time(previous, duration)?;
            previous = time;
            if rotation_kind == 0 {
                let compressed_axis = Some(r.u32()?);
                keys.push(RotationKey {
                    time,
                    packed_angle: 0,
                    packed_axis: [0; 3],
                    compressed_axis,
                });
                continue;
            }
            let packed_angle = u16le(data, r.at)?;
            if packed_angle >= 4096 {
                return Err("MOT short-axis angle exceeds lookup table".into());
            }
            let packed_axis = [
                u16le(data, r.at + 2)? as i16,
                u16le(data, r.at + 4)? as i16,
                u16le(data, r.at + 6)? as i16,
            ];
            r.at += 8;
            keys.push(RotationKey {
                time,
                packed_angle,
                packed_axis,
                compressed_axis: None,
            });
        }
        rotations.push(Track { bone, keys });
    }
    if r.at != data.len() {
        return Err("MOT payload extent mismatch".into());
    }
    Ok(Animation {
        version,
        frame_count,
        duration,
        bone_names,
        translation_kind,
        translations,
        rotations,
    })
}

/// Original controller defaults interpolate both kinds; callers can reproduce
/// MCF overrides. Time is in the MOT stream's stored seconds, not game time.
#[derive(Debug, Clone, Copy)]
pub struct Sampling {
    pub translation_interpolation: bool,
    pub rotation_interpolation: bool,
}
impl Default for Sampling {
    fn default() -> Self {
        Self {
            translation_interpolation: true,
            rotation_interpolation: true,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Pose {
    pub local_matrices: Vec<[f32; 16]>,
    pub world_matrices: Vec<[f32; 16]>,
}

/// Local controller values before hierarchy composition. Missing channels keep
/// the corresponding model bind transform; they are not identity overrides.
#[derive(Debug, Clone)]
pub struct Channels {
    pub translations: Vec<Option<[f32; 3]>>,
    pub rotations: Vec<Option<[f32; 4]>>,
}

impl RotationKey {
    /// Scalar-first quaternion [w,x,y,z]. The original uses a precomputed table;
    /// these constants and operations match its initializer, with host sin/cos.
    /// PS2 transcendental results have not been established as bit-identical.
    pub fn quaternion(&self) -> [f32; 4] {
        if let Some(value) = self.compressed_axis {
            let bytes = value.to_le_bytes();
            // SLUS_211.04 table initializer 003b2924 uses 0x3c490fdb
            // (pi/256), and 0x3c010204 (1/127) for signed axis bytes.
            let half = bytes[0] as f32 * f32::from_bits(0x3c490fdb);
            let scale = half.sin() / 127.;
            return [
                half.cos(),
                bytes[1] as i8 as f32 * scale,
                bytes[2] as i8 as f32 * scale,
                bytes[3] as i8 as f32 * scale,
            ];
        }
        let half_angle = self.packed_angle as f32 * f32::from_bits(0x3a490fdb);
        let scale = half_angle.sin() * f32::from_bits(0x38000100);
        [
            half_angle.cos(),
            self.packed_axis[0] as f32 * scale,
            self.packed_axis[1] as f32 * scale,
            self.packed_axis[2] as f32 * scale,
        ]
    }
}

fn dot(a: [f32; 4], b: [f32; 4]) -> f32 {
    ((a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]) + a[3] * b[3]
}
fn sine_polynomial(angle: f32) -> f32 {
    let square = angle * angle;
    let c0 = f32::from_bits(0x39500d01);
    let c1 = f32::from_bits(0x3c088888);
    let c2 = f32::from_bits(0x3e2aaaad);
    angle * (1. + square * (-c2 + square * (c1 - square * c0)))
}
pub(crate) fn blend_rotation(a: [f32; 4], b: [f32; 4], weight: f32) -> [f32; 4] {
    let cosine = dot(a, b);
    // Exact-build 0x002f8650 uses this sine polynomial and |dot| < 1 guard.
    let mut q = a;
    if cosine.abs() < 1. {
        let theta = cosine.abs().acos();
        let left = sine_polynomial(theta - weight * theta);
        let right = sine_polynomial(weight * theta) * if cosine < 0. { -1. } else { 1. };
        let reciprocal = 1. / sine_polynomial(theta);
        q = std::array::from_fn(|i| (a[i] * left + b[i] * right) * reciprocal);
    }
    q
}
fn interpolate_rotation(a: [f32; 4], b: [f32; 4], weight: f32) -> [f32; 4] {
    let mut q = blend_rotation(a, b, weight);
    if dot(q, b) < 0. {
        q = q.map(|v| -v);
    }
    q
}
fn bracket<T>(
    keys: &[T],
    time: f32,
    key_time: impl Fn(&T) -> f32,
    interpolate: bool,
) -> (usize, usize, f32) {
    if time <= key_time(&keys[0]) {
        return (0, 0, 0.);
    }
    let next = keys.partition_point(|key| key_time(key) <= time);
    if next == keys.len() || !interpolate {
        return (next - 1, next - 1, 0.);
    }
    let previous = next - 1;
    (
        previous,
        next,
        (time - key_time(&keys[previous])) / (key_time(&keys[next]) - key_time(&keys[previous])),
    )
}
fn matrix_rotation(q: [f32; 4], matrix: &mut [f32; 16]) {
    let [w, x, y, z] = q;
    matrix[..12].copy_from_slice(&[
        1. - 2. * (y * y + z * z),
        2. * (x * y + z * w),
        2. * (x * z - y * w),
        0.,
        2. * (x * y - z * w),
        1. - 2. * (x * x + z * z),
        2. * (y * z + x * w),
        0.,
        2. * (x * z + y * w),
        2. * (y * z - x * w),
        1. - 2. * (x * x + y * y),
        0.,
    ]);
}

impl Animation {
    pub(crate) fn model_bone(&self, index: u32, nodes: &[crate::mesh::Node]) -> Result<usize> {
        if self.bone_names.is_empty() {
            if index as usize >= nodes.len() {
                return Err("MOT references missing model bone".into());
            }
            return Ok(index as usize);
        }
        let name = self
            .bone_names
            .get(index as usize)
            .ok_or("MOT references missing bone name")?;
        nodes
            .iter()
            .position(|node| node.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("MOT bone {name} absent from model"))
    }
    /// Sample the original local animation and compose the model hierarchy.
    /// Duplicate controllers apply in file order. Does not add game-side IK,
    /// cross-fades, event timing, rider physics, or dynamic female hair.
    pub fn sample_channels(
        &self,
        nodes: &[crate::mesh::Node],
        time: f32,
        sampling: Sampling,
    ) -> Result<Channels> {
        if !time.is_finite() || time < 0. || time > self.duration {
            return Err("MOT sample time outside clip".into());
        }
        if nodes.is_empty() || nodes.len() > 256 {
            return Err("invalid animation model node count".into());
        }
        let mut channels = Channels {
            translations: vec![None; nodes.len()],
            rotations: vec![None; nodes.len()],
        };
        for track in &self.translations {
            if track.keys.is_empty() {
                return Err("MOT translation controller has no keys".into());
            }
            let bone = self.model_bone(track.bone, nodes)?;
            let (a, b, t) = bracket(
                &track.keys,
                time,
                |key| key.time,
                sampling.translation_interpolation,
            );
            let first = &track.keys[a];
            let second = &track.keys[b];
            let last = track.keys.len() - 1;
            let value = if self.translation_kind == 1
                && sampling.translation_interpolation
                && last > 0
                && time > track.keys[last].time
            {
                let previous = &track.keys[last - 1];
                let end = &track.keys[last];
                let interval = end.time - previous.time;
                if interval <= 0. {
                    return Err("MOT terminal Hermite interval is empty".into());
                }
                let weight = (time - end.time) / interval;
                std::array::from_fn(|i| end.value[i] + weight * (end.value[i] - previous.value[i]))
            } else if a == b {
                first.value
            } else if self.translation_kind == 0 {
                std::array::from_fn(|i| first.value[i] + (second.value[i] - first.value[i]) * t)
            } else {
                let outgoing = first.tangents.ok_or("MOT Hermite tangent missing")?[1];
                let incoming = second.tangents.ok_or("MOT Hermite tangent missing")?[0];
                let square = t * t;
                let cube = t * square;
                let end = 3. * square - 2. * cube;
                let start = 1. - end;
                let end_tangent = cube - square;
                let start_tangent = t + (end_tangent - square);
                std::array::from_fn(|i| {
                    incoming[i] * end_tangent
                        + outgoing[i] * start_tangent
                        + second.value[i] * end
                        + first.value[i] * start
                })
            };
            channels.translations[bone] = Some(value);
        }
        for track in &self.rotations {
            if track.keys.is_empty() {
                return Err("MOT rotation controller has no keys".into());
            }
            let bone = self.model_bone(track.bone, nodes)?;
            let (a, b, t) = bracket(
                &track.keys,
                time,
                |key| key.time,
                sampling.rotation_interpolation,
            );
            let first = track.keys[a].quaternion();
            let q = if a == b {
                first
            } else {
                interpolate_rotation(first, track.keys[b].quaternion(), t)
            };
            channels.rotations[bone] = Some(q);
        }
        Ok(channels)
    }
    pub fn sample_pose(
        &self,
        nodes: &[crate::mesh::Node],
        time: f32,
        sampling: Sampling,
    ) -> Result<Pose> {
        self.sample_channels(nodes, time, sampling)?.pose(nodes)
    }
}

impl Channels {
    pub fn pose(&self, nodes: &[crate::mesh::Node]) -> Result<Pose> {
        if nodes.is_empty()
            || nodes.len() > 256
            || self.translations.len() != nodes.len()
            || self.rotations.len() != nodes.len()
        {
            return Err("animation channel/model size mismatch".into());
        }
        let mut local_matrices: Vec<_> = nodes.iter().map(|node| node.local_matrix).collect();
        for (index, matrix) in local_matrices.iter_mut().enumerate() {
            if let Some(value) = self.translations[index] {
                matrix[12..15].copy_from_slice(&value);
            }
            if let Some(value) = self.rotations[index] {
                matrix_rotation(value, matrix);
            }
        }
        let mut world_matrices: Vec<[f32; 16]> = Vec::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            if node.parent < -1 || node.parent >= index as i32 {
                return Err("invalid animation node hierarchy".into());
            }
            let local = local_matrices[index];
            let world = if node.parent == -1 {
                local
            } else {
                let parent = &world_matrices[node.parent as usize];
                std::array::from_fn(|i| {
                    (0..4)
                        .map(|k| parent[k * 4 + i % 4] * local[i / 4 * 4 + k])
                        .sum()
                })
            };
            if world.iter().any(|value| !value.is_finite()) {
                return Err("animation pose overflow".into());
            }
            world_matrices.push(world);
        }
        Ok(Pose {
            local_matrices,
            world_matrices,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(name: &str, parent: i32) -> crate::mesh::Node {
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        crate::mesh::Node {
            name: name.into(),
            parent,
            local_matrix: identity,
            world_matrix: identity,
            bounds: [0.; 7],
        }
    }
    #[test]
    fn mx_v7_compressed_axis_and_event_bounds() {
        let mut data = Vec::new();
        for value in [(-7i32) as u32, 1, 1, 1, 5] {
            data.extend(value.to_le_bytes());
        }
        data.extend(b"root\0");
        data.extend(1f32.to_le_bytes());
        for value in [0u32, 0, 0, 1, 0, 0, 1, 0] {
            data.extend(value.to_le_bytes());
        }
        data.extend(0f32.to_le_bytes());
        data.extend([64, 0, 127, 0]);
        let a = decode_mot(&data).unwrap();
        assert_eq!(a.version, 7);
        let q = a.rotations[0].keys[0].quaternion();
        assert!((q[0] * q[0] + q[2] * q[2] - 1.).abs() < 1e-6);
        assert_eq!(
            q[0].to_bits(),
            (std::f32::consts::FRAC_PI_4).cos().to_bits()
        );
        assert_eq!(
            q[2].to_bits(),
            (std::f32::consts::FRAC_PI_4).sin().to_bits()
        );
        let mut v6 = data.clone();
        v6[..4].copy_from_slice(&(-6i32).to_le_bytes());
        assert_eq!(decode_mot(&v6).unwrap().version, 6);
        for end in 0..v6.len() {
            assert!(decode_mot(&v6[..end]).is_err());
        }
        for end in 0..data.len() {
            assert!(decode_mot(&data[..end]).is_err());
        }
        data[4..8].copy_from_slice(&2u32.to_le_bytes());
        assert!(decode_mot(&data).is_err());
    }
    #[test]
    fn short_axis_quarter_turn_and_shortest_arc() {
        assert_eq!(
            blend_rotation([1., 0., 0., 0.], [-1., 0., 0., 0.], 0.8),
            [1., 0., 0., 0.],
            "raw blend retains first quaternion at the absolute-dot guard"
        );
        let q = RotationKey {
            time: 0.,
            packed_angle: 1024,
            packed_axis: [0, 32767, 0],
            compressed_axis: None,
        }
        .quaternion();
        let mut matrix = node("root", -1).local_matrix;
        matrix_rotation(q, &mut matrix);
        assert!(matrix[0].abs() < 0.000001);
        assert!((matrix[2] + 1.).abs() < 0.000001);
        let half = interpolate_rotation([1., 0., 0., 0.], q.map(|v| -v), 0.5);
        assert!((half[0].abs() - (std::f32::consts::PI / 8.).cos()).abs() < 0.00001);
        assert!((half[2].abs() - (std::f32::consts::PI / 8.).sin()).abs() < 0.00001);
    }
    #[test]
    fn named_pose_hierarchy_hermite_step_and_terminal_extrapolation() {
        let mut animation = decode_mot(&sample()).unwrap();
        animation.rotations.clear();
        animation.translation_kind = 1;
        animation.translations[0].keys[0].tangents = Some([[0.; 3], [4., 0., 0.]]);
        animation.translations[0].keys[1].tangents = Some([[0.; 3], [0.; 3]]);
        let mut nodes = [node("root", -1), node("Pelvis", 0)];
        nodes[0].local_matrix[12] = 10.;
        let p = animation
            .sample_pose(&nodes, 0.5, Sampling::default())
            .unwrap();
        assert_eq!(&p.local_matrices[1][12..15], &[2., 2.5, 3.5]);
        assert_eq!(&p.world_matrices[1][12..15], &[12., 2.5, 3.5]);
        let p = animation
            .sample_pose(
                &nodes,
                0.5,
                Sampling {
                    translation_interpolation: false,
                    rotation_interpolation: false,
                },
            )
            .unwrap();
        assert_eq!(&p.local_matrices[1][12..15], &[1., 2., 3.]);
        animation.duration = 2.;
        let p = animation
            .sample_pose(&nodes, 2., Sampling::default())
            .unwrap();
        assert_eq!(&p.local_matrices[1][12..15], &[3., 4., 5.]);
        assert!(
            animation
                .sample_pose(&nodes, f32::NAN, Sampling::default())
                .is_err()
        );
        nodes[1].name = "missing".into();
        assert!(
            animation
                .sample_pose(&nodes, 0., Sampling::default())
                .is_err()
        );
    }
    fn sample() -> Vec<u8> {
        let mut b = Vec::new();
        for value in [(-3i32) as u32, 2, 1, 7] {
            b.extend(value.to_le_bytes());
        }
        b.extend(b"Pelvis\0");
        for value in [1f32.to_bits(), 1, 0, 1, 1, 2, 0] {
            b.extend(value.to_le_bytes());
        }
        for key in [[0f32, 1., 2., 3.], [1., 2., 3., 4.]] {
            for value in key {
                b.extend(value.to_le_bytes());
            }
        }
        for value in [2u32, 0] {
            b.extend(value.to_le_bytes());
        }
        for (time, angle) in [(0f32, 0u16), (1., 1000)] {
            b.extend(time.to_le_bytes());
            b.extend(angle.to_le_bytes());
            for value in [0i16, 32767, 0] {
                b.extend(value.to_le_bytes());
            }
        }
        b
    }
    #[test]
    fn named_linear_and_packed_rotation_streams() {
        let bytes = sample();
        let animation = decode_mot(&bytes).unwrap();
        assert_eq!(animation.bone_names, ["Pelvis"]);
        assert_eq!(animation.translations[0].keys[1].value, [2., 3., 4.]);
        assert_eq!(animation.rotations[0].keys[1].packed_angle, 1000);
        assert_eq!(animation.rotations[0].keys[1].packed_axis, [0, 32767, 0]);
        for end in 0..bytes.len() {
            assert!(decode_mot(&bytes[..end]).is_err());
        }
    }
    #[test]
    fn trailing_payload_and_nonfinite_time_rejected() {
        let mut bytes = sample();
        bytes.push(0);
        assert!(decode_mot(&bytes).is_err());
        bytes.pop();
        // Named table ends at 23, controller header at 43; first key starts at 51.
        bytes[51..55].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(decode_mot(&bytes).unwrap_err().contains("nonfinite"));
    }
}
