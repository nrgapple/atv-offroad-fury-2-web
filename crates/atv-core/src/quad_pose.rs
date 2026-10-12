//! Original quad linkage update (001d5290 initialization,001d65c0 pose).
//! Coordinates are relative to the loaded body root; host sin/cos/sqrt replace
//! original scalar/VU helpers and have not been established bit-exact.
use crate::{Result, mesh::Mesh};
pub type Matrix = [f32; 16];
type V = [f32; 3];
const X: V = [1., 0., 0.];
const Y: V = [0., 1., 0.];
const Z: V = [0., 0., 1.];
const IDENTITY: Matrix = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: V) -> Result<V> {
    let n = dot(a, a).sqrt();
    if n < 1e-8 || !n.is_finite() {
        return Err("degenerate quad linkage vector".into());
    }
    Ok(a.map(|x| x / n))
}
fn col(m: &Matrix, i: usize) -> V {
    [m[i * 4], m[i * 4 + 1], m[i * 4 + 2]]
}
fn direction(m: &Matrix, v: V) -> V {
    std::array::from_fn(|i| m[i] * v[0] + m[4 + i] * v[1] + m[8 + i] * v[2])
}
fn point(m: &Matrix, v: V) -> V {
    let d = direction(m, v);
    std::array::from_fn(|i| d[i] + m[12 + i])
}
fn inverse_direction(m: &Matrix, v: V) -> V {
    std::array::from_fn(|i| dot(col(m, i), v))
}
fn inverse_point(m: &Matrix, v: V) -> V {
    inverse_direction(m, sub(v, col(m, 3)))
}
fn basis(look: V, up: V) -> Matrix {
    let right = cross(up, look);
    [
        right[0], right[1], right[2], 0., up[0], up[1], up[2], 0., look[0], look[1], look[2], 0.,
        0., 0., 0., 1.,
    ]
}
fn ortho(look: V, up: V, preserve_look: bool) -> Result<Matrix> {
    let right = cross(up, look);
    let (look, up) = if preserve_look {
        (look, cross(look, right))
    } else {
        (cross(right, up), up)
    };
    Ok(basis(unit(look)?, unit(up)?))
}
fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| {
        let (r, c) = (i % 4, i / 4);
        (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum()
    })
}
fn rotation(axis: V, angle: f32) -> Matrix {
    let c = angle.cos();
    let s = angle.sin();
    let t = 1. - c;
    let [x, y, z] = axis;
    [
        c + t * x * x,
        t * x * y + s * z,
        t * x * z - s * y,
        0.,
        t * y * x - s * z,
        c + t * y * y,
        t * y * z + s * x,
        0.,
        t * z * x + s * y,
        t * z * y - s * x,
        c + t * z * z,
        0.,
        0.,
        0.,
        0.,
        1.,
    ]
}
#[derive(Clone)]
struct Front {
    wheel: usize,
    holder: usize,
    top: usize,
    bottom: usize,
    tie: usize,
    shock_top: usize,
    shock_bottom: usize,
    up: V,
    reference: Matrix,
    offset: V,
}
pub struct QuadPose {
    bind: Vec<Matrix>,
    parents: Vec<i32>,
    wheels: [usize; 4],
    handlebars: usize,
    bar_up: V,
    bar_reference: Matrix,
    front: [Front; 2],
    swing: usize,
    axle: usize,
    rear_reference: Matrix,
    rear_offsets: [V; 2],
}
struct Pose<'a> {
    local: Vec<Matrix>,
    parents: &'a [i32],
}
impl Pose<'_> {
    fn world(&self, n: usize) -> Matrix {
        let p = self.parents[n];
        if p < 0 {
            self.local[n]
        } else {
            mul(&self.world(p as usize), &self.local[n])
        }
    }
    fn pos(&self, n: usize) -> V {
        col(&self.world(n), 3)
    }
    fn set_orientation(&mut self, n: usize, m: Matrix) {
        let p = col(&self.local[n], 3);
        self.local[n] = m;
        self.local[n][12..15].copy_from_slice(&p);
    }
    fn set_world_orientation(&mut self, n: usize, m: Matrix) {
        let mut local = m;
        let parent = self.parents[n];
        if parent >= 0 {
            let p = self.world(parent as usize);
            for c in 0..3 {
                local[c * 4..c * 4 + 3].copy_from_slice(&inverse_direction(&p, col(&m, c)));
            }
        }
        self.set_orientation(n, local);
    }
    fn set_world_position(&mut self, n: usize, p: V) {
        let parent = self.parents[n];
        let p = if parent >= 0 {
            inverse_point(&self.world(parent as usize), p)
        } else {
            p
        };
        self.local[n][12..15].copy_from_slice(&p);
    }
    fn rotate(&mut self, n: usize, axis: V, angle: f32) {
        let m = mul(&self.local[n], &rotation(axis, angle));
        self.set_orientation(n, m);
    }
}
impl QuadPose {
    pub fn new(mesh: &Mesh) -> Result<Self> {
        if mesh.nodes.is_empty() || mesh.nodes.len() > 128 {
            return Err("unsupported quad node count".into());
        }
        for (i, n) in mesh.nodes.iter().enumerate() {
            if n.parent >= i as i32 {
                return Err("quad hierarchy must precede its children".into());
            }
        }
        // The supported in-race ATV mesh has an identity body root. Keep this
        // assumption explicit until another authored root convention is recovered.
        if mesh.nodes[0]
            .local_matrix
            .iter()
            .zip(IDENTITY)
            .any(|(a, b)| (a - b).abs() > 1e-5)
        {
            return Err("unsupported quad body root transform".into());
        }
        let find = |name: &str| {
            mesh.nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("missing quad linkage node {name}"))
        };
        let bind: Vec<_> = mesh.nodes.iter().map(|n| n.local_matrix).collect();
        let parents: Vec<_> = mesh.nodes.iter().map(|n| n.parent).collect();
        let pose = Pose {
            local: bind.clone(),
            parents: &parents,
        };
        let wheels = [
            find("wheelfrontleft")?,
            find("wheelfrontright")?,
            find("wheelrearleft")?,
            find("wheelrearright")?,
        ];
        let handlebars = find("handlebars")?;
        let bar_up = col(&bind[handlebars], 1);
        let bar_reference = ortho(Z, bar_up, false)?;
        let mut fronts = Vec::new();
        for (side, suffix) in ["left", "right"].iter().enumerate() {
            let holder = find(&format!("wheelholder{suffix}"))?;
            let tie = find(&format!("tierod{suffix}"))?;
            let v = if side == 0 {
                sub(pose.pos(tie), pose.pos(holder))
            } else {
                sub(pose.pos(holder), pose.pos(tie))
            };
            fronts.push(Front {
                wheel: wheels[side],
                holder,
                top: find(&format!("armtop{suffix}"))?,
                bottom: find(&format!("armbottom{suffix}"))?,
                tie,
                shock_top: find(&format!("shocktop{suffix}"))?,
                shock_bottom: find(&format!("shockbottom{suffix}"))?,
                up: col(&pose.world(holder), 1),
                reference: basis(Z, unit(cross(Z, unit(v)?))?),
                offset: inverse_point(&pose.world(holder), pose.pos(wheels[side])),
            });
        }
        let swing = find("swingarm")?;
        let axle = find("rearaxle")?;
        let rear_reference = ortho(unit(sub(pose.pos(swing), pose.pos(axle)))?, Y, true)?;
        let rear_offsets = [
            inverse_point(&pose.world(axle), pose.pos(wheels[2])),
            inverse_point(&pose.world(axle), pose.pos(wheels[3])),
        ];
        Ok(Self {
            bind,
            parents,
            wheels,
            handlebars,
            bar_up,
            bar_reference,
            front: fronts.try_into().map_err(|_| "quad front count")?,
            swing,
            axle,
            rear_reference,
            rear_offsets,
        })
    }
    /// Body-relative axle directions from the completed 001d65c0 pose.
    /// 001ecc40 consumes these at the next vehicle update, including front
    /// camber and the unscaled linkage steering angle. Wheel spin leaves X
    /// unchanged. Callers supply the full body-relative sample matrix array.
    pub fn wheel_right_axes(&self, matrices: &[Matrix]) -> Result<[[f32; 3]; 4]> {
        if matrices.len() != self.bind.len()
            || matrices.iter().flatten().any(|value| !value.is_finite())
        {
            return Err("invalid quad pose matrix array".into());
        }
        Ok(self.wheels.map(|wheel| col(&matrices[wheel], 0)))
    }

    pub fn sample(
        &self,
        steering: f32,
        angles: [f32; 4],
        compression: [f32; 4],
    ) -> Result<Vec<Matrix>> {
        if !steering.is_finite()
            || angles
                .iter()
                .chain(compression.iter())
                .any(|v| !v.is_finite())
        {
            return Err("nonfinite quad pose input".into());
        }
        let mut pose = Pose {
            local: self.bind.clone(),
            parents: &self.parents,
        };
        //001edcd0 writes rest - (0,-1,0)*compression before visual linkage update.
        for (i, &wheel) in self.wheels.iter().enumerate() {
            pose.local[wheel][13] += compression[i];
        }
        let look = direction(&self.bar_reference, [steering.sin(), 0., steering.cos()]);
        pose.set_orientation(self.handlebars, basis(look, self.bar_up));
        for (side, f) in self.front.iter().enumerate() {
            let mut v = if side == 0 {
                sub(pose.pos(f.tie), pose.pos(f.wheel))
            } else {
                sub(pose.pos(f.wheel), pose.pos(f.tie))
            };
            v[2] = 0.;
            let arm_up = cross(Z, inverse_direction(&f.reference, unit(v)?));
            pose.set_orientation(f.top, basis(Z, arm_up));
            pose.set_orientation(f.bottom, basis(Z, arm_up));
            pose.set_world_orientation(f.tie, basis(Z, arm_up));
            let shock_up = unit(sub(pose.pos(f.shock_top), pose.pos(f.shock_bottom)))?;
            pose.set_world_orientation(f.shock_top, basis(Z, shock_up));
            pose.set_world_orientation(f.shock_bottom, basis(Z, shock_up));
            pose.set_world_orientation(f.holder, basis(Z, Y));
            let wheel_pos = point(&pose.world(f.holder), f.offset);
            pose.set_world_position(f.wheel, wheel_pos);
            pose.set_orientation(f.wheel, basis(Z, f.up));
            pose.rotate(f.wheel, Y, steering);
            //Both front nodes use the front-left angle in the exact executable.
            pose.rotate(f.wheel, X, angles[0]);
        }
        let rear_y = pose.local[self.wheels[2]][13].max(pose.local[self.wheels[3]][13]);
        let swing_pos = col(&pose.local[self.swing], 3);
        let v = unit([
            0.,
            swing_pos[1] - rear_y,
            swing_pos[2] - pose.local[self.wheels[3]][14],
        ])?;
        let look = inverse_direction(&self.rear_reference, v);
        pose.set_orientation(self.swing, ortho(look, Y, true)?);
        for (i, &wheel) in self.wheels[2..].iter().enumerate() {
            let p = point(&pose.world(self.axle), self.rear_offsets[i]);
            pose.set_world_position(wheel, p);
        }
        let mut rear = angles[2].max(angles[3]);
        if rear > std::f32::consts::PI {
            rear -= std::f32::consts::TAU;
        } else if rear < -std::f32::consts::PI {
            rear += std::f32::consts::TAU;
        }
        for n in [self.wheels[2], self.wheels[3], self.axle] {
            pose.set_orientation(n, rotation(X, rear));
        }
        for f in &self.front {
            pose.rotate(f.holder, Y, steering);
        }
        let result: Vec<_> = (0..pose.local.len()).map(|i| pose.world(i)).collect();
        if result.iter().flatten().any(|v| !v.is_finite()) {
            return Err("nonfinite quad linkage matrix".into());
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn axis_rotation_order_and_preserved_position() {
        let mut pose = Pose {
            local: vec![IDENTITY],
            parents: &[-1],
        };
        pose.local[0][12..15].copy_from_slice(&[3., 4., 5.]);
        pose.rotate(0, Y, std::f32::consts::FRAC_PI_2);
        pose.rotate(0, X, std::f32::consts::FRAC_PI_2);
        let p = point(&pose.world(0), Y);
        assert!((p[0] - 4.).abs() < 1e-6);
        assert!((p[1] - 4.).abs() < 1e-6);
        assert!((p[2] - 5.).abs() < 1e-6);
    }
    #[test]
    fn world_orientation_removes_parent_rotation() {
        let mut parent = rotation(X, 0.4);
        parent[13] = 2.;
        let mut pose = Pose {
            local: vec![parent, IDENTITY],
            parents: &[-1, 0],
        };
        pose.set_world_orientation(1, basis(Z, Y));
        for (a, b) in pose.world(1)[..12].iter().zip(IDENTITY[..12].iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
