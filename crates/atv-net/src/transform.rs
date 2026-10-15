use crate::{Error, Result, bounded};
use serde::{Deserialize, Serialize};

/// Translation, W/X/Y/Z rotation, signed scale and upper-triangular shear.
/// QR decomposition preserves source linkage affine bases, including reflection.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    /// Upper triangular XY, XZ, YZ coefficients before rotation.
    #[serde(default)]
    pub shear: [f32; 3],
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.; 3],
            rotation: [1., 0., 0., 0.],
            scale: [1.; 3],
            shear: [0.; 3],
        }
    }
}
impl Transform {
    pub fn validate(&self) -> Result<()> {
        let norm: f32 = self.rotation.iter().map(|x| x * x).sum();
        if !bounded(&self.position, 1_000_000.)
            || !bounded(&self.rotation, 1.01)
            || !(0.98..=1.02).contains(&norm)
            || !bounded(&self.scale, 10_000.)
            || !bounded(&self.shear, 10_000.)
        {
            return Err(Error::InvalidTransform);
        }
        Ok(())
    }
    pub fn from_column_major(m: [f32; 16]) -> Result<Self> {
        if !m.iter().all(|v| v.is_finite())
            || [m[3], m[7], m[11], m[15] - 1.]
                .iter()
                .any(|v| v.abs() > 0.001)
        {
            return Err(Error::InvalidTransform);
        }
        let columns = [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]];
        let sx = dot(columns[0], columns[0]).sqrt();
        if sx < 0.00001 {
            return Err(Error::InvalidTransform);
        }
        let x = columns[0].map(|v| v / sx);
        let xy = dot(x, columns[1]);
        let remainder: [f32; 3] = std::array::from_fn(|i| columns[1][i] - x[i] * xy);
        let sy = dot(remainder, remainder).sqrt();
        if sy < 0.00001 {
            return Err(Error::InvalidTransform);
        }
        let y = remainder.map(|v| v / sy);
        let z = cross(x, y);
        let sz = dot(z, columns[2]);
        if sz.abs() < 0.00001 {
            return Err(Error::InvalidTransform);
        }
        let scales = [sx, sy, sz];
        let shear = [xy, dot(x, columns[2]), dot(y, columns[2])];
        let a = [x, y, z];
        let trace = a[0][0] + a[1][1] + a[2][2];
        let q = if trace > 0. {
            let s = (trace + 1.).sqrt() * 2.;
            [
                s * 0.25,
                (a[1][2] - a[2][1]) / s,
                (a[2][0] - a[0][2]) / s,
                (a[0][1] - a[1][0]) / s,
            ]
        } else if a[0][0] > a[1][1] && a[0][0] > a[2][2] {
            let s = (1. + a[0][0] - a[1][1] - a[2][2]).sqrt() * 2.;
            [
                (a[1][2] - a[2][1]) / s,
                s * 0.25,
                (a[1][0] + a[0][1]) / s,
                (a[2][0] + a[0][2]) / s,
            ]
        } else if a[1][1] > a[2][2] {
            let s = (1. + a[1][1] - a[0][0] - a[2][2]).sqrt() * 2.;
            [
                (a[2][0] - a[0][2]) / s,
                (a[1][0] + a[0][1]) / s,
                s * 0.25,
                (a[2][1] + a[1][2]) / s,
            ]
        } else {
            let s = (1. + a[2][2] - a[0][0] - a[1][1]).sqrt() * 2.;
            [
                (a[0][1] - a[1][0]) / s,
                (a[2][0] + a[0][2]) / s,
                (a[2][1] + a[1][2]) / s,
                s * 0.25,
            ]
        };
        let out = Self {
            position: [m[12], m[13], m[14]],
            rotation: normalize(q),
            scale: scales,
            shear,
        };
        out.validate()?;
        Ok(out)
    }
    pub fn to_column_major(self) -> [f32; 16] {
        let [w, qx, qy, qz] = normalize(self.rotation);
        let [sx, sy, sz] = self.scale;
        let [xy, xz, yz] = self.shear;
        let x = [
            1. - 2. * (qy * qy + qz * qz),
            2. * (qx * qy + w * qz),
            2. * (qx * qz - w * qy),
        ];
        let y = [
            2. * (qx * qy - w * qz),
            1. - 2. * (qx * qx + qz * qz),
            2. * (qy * qz + w * qx),
        ];
        let z = [
            2. * (qx * qz + w * qy),
            2. * (qy * qz - w * qx),
            1. - 2. * (qx * qx + qy * qy),
        ];
        let mut matrix = [0.; 16];
        for i in 0..3 {
            matrix[i] = x[i] * sx;
            matrix[i + 4] = x[i] * xy + y[i] * sy;
            matrix[i + 8] = x[i] * xz + y[i] * yz + z[i] * sz;
            matrix[i + 12] = self.position[i];
        }
        matrix[15] = 1.;
        matrix
    }
    pub(crate) fn blend(self, other: Self, t: f32) -> Self {
        Self {
            position: lerp(self.position, other.position, t),
            rotation: slerp(self.rotation, other.rotation, t),
            scale: lerp(self.scale, other.scale, t),
            shear: lerp(self.shear, other.shear, t),
        }
    }
}
pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn normalize(q: [f32; 4]) -> [f32; 4] {
    let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length < 0.00001 {
        return [1., 0., 0., 0.];
    }
    q.map(|v| v / length)
}
pub(crate) fn lerp<const N: usize>(a: [f32; N], b: [f32; N], t: f32) -> [f32; N] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
pub(crate) fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let a = normalize(a);
    let mut b = normalize(b);
    let mut d: f32 = a.iter().zip(b).map(|(a, b)| a * b).sum();
    if d < 0. {
        b = b.map(|v| -v);
        d = -d;
    }
    if d > 0.9995 {
        return normalize(lerp(a, b, t));
    }
    let angle = d.clamp(-1., 1.).acos();
    let divisor = angle.sin();
    let left = ((1. - t) * angle).sin() / divisor;
    let right = (t * angle).sin() / divisor;
    normalize(std::array::from_fn(|i| a[i] * left + b[i] * right))
}
pub(crate) fn advance(rotation: [f32; 4], angular: [f32; 3], dt: f32) -> [f32; 4] {
    let speed = dot(angular, angular).sqrt();
    if speed < 0.00001 {
        return rotation;
    }
    let half = speed * dt * 0.5;
    let [x, y, z] = angular.map(|v| v * half.sin() / speed);
    let w = half.cos();
    let [qw, qx, qy, qz] = rotation;
    normalize([
        w * qw - x * qx - y * qy - z * qz,
        w * qx + x * qw + y * qz - z * qy,
        w * qy - x * qz + y * qw + z * qx,
        w * qz + x * qy - y * qx + z * qw,
    ])
}
/// Hamilton product of W,X,Y,Z quaternions: `a` applied after `b`.
pub(crate) fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [aw, ax, ay, az] = a;
    let [bw, bx, by, bz] = b;
    [
        aw * bw - ax * bx - ay * by - az * bz,
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
    ]
}
pub(crate) fn conjugate([w, x, y, z]: [f32; 4]) -> [f32; 4] {
    [w, -x, -y, -z]
}
