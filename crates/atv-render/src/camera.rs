//! Scene camera (sceneViewProjection/cameraAxes in web/render-support.js).

#[derive(Clone, Copy, Debug, Default)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
}

pub struct Axes {
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
}

impl Camera {
    pub fn axes(&self) -> Axes {
        let (sy, cy, sp, cp) = (
            self.yaw.sin(),
            self.yaw.cos(),
            self.pitch.sin(),
            self.pitch.cos(),
        );
        Axes {
            forward: [sy * cp, sp, cy * cp],
            right: [cy, 0., -sy],
            up: [-sy * sp, cp, -cy * sp],
        }
    }

    pub fn eye(&self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    /// Column-major view-projection with a 0..1 depth range. `focal_length`
    /// scales the vertical axis; the original frames a 60 degree view.
    pub fn view_projection(
        &self,
        aspect: f32,
        focal_length: f32,
        near: f32,
        far: f32,
    ) -> [f32; 16] {
        let Axes { forward, right, up } = self.axes();
        let dot = |v: [f32; 3]| v[0] * self.x + v[1] * self.y + v[2] * self.z;
        let view = [
            right[0],
            up[0],
            -forward[0],
            0.,
            right[1],
            up[1],
            -forward[1],
            0.,
            right[2],
            up[2],
            -forward[2],
            0.,
            -dot(right),
            -dot(up),
            dot(forward),
            1.,
        ];
        let f = focal_length;
        // GL projection, then z' = (z + w) / 2 so depth lands in wgpu's 0..1.
        let gl = [
            f / aspect,
            0.,
            0.,
            0.,
            0.,
            f,
            0.,
            0.,
            0.,
            0.,
            (far + near) / (near - far),
            -1.,
            0.,
            0.,
            2. * far * near / (near - far),
            0.,
        ];
        let mut projection = gl;
        for column in 0..4 {
            projection[column * 4 + 2] = 0.5 * gl[column * 4 + 2] + 0.5 * gl[column * 4 + 3];
        }
        multiply(&projection, &view)
    }
}

pub fn multiply(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum();
        }
    }
    out
}

/// The original 60 degree vertical framing, 1 / tan(30 degrees).
pub fn default_focal_length() -> f32 {
    1. / (std::f32::consts::PI / 6.).tan()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: &[f32; 16], p: [f32; 3]) -> [f32; 4] {
        let mut out = [0.; 4];
        for (row, value) in out.iter_mut().enumerate() {
            *value = m[row] * p[0] + m[4 + row] * p[1] + m[8 + row] * p[2] + m[12 + row];
        }
        out
    }

    #[test]
    fn near_and_far_planes_map_to_zero_and_one() {
        let camera = Camera {
            x: 10.,
            y: 5.,
            z: -3.,
            yaw: 0.7,
            pitch: -0.2,
        };
        let m = camera.view_projection(16. / 9., default_focal_length(), 1., 500.);
        let forward = camera.axes().forward;
        for (distance, depth) in [(1., 0.), (500., 1.)] {
            let p = apply(
                &m,
                [
                    10. + forward[0] * distance,
                    5. + forward[1] * distance,
                    -3. + forward[2] * distance,
                ],
            );
            assert!(
                (p[2] / p[3] - depth).abs() < 1e-4,
                "{distance}: {}",
                p[2] / p[3]
            );
            assert!(p[0].abs() < 1e-3 && p[1].abs() < 1e-3);
        }
    }
}
