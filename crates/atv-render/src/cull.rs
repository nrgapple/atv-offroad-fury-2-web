//! Whole-object view culling (viewPlanes/outsideView in web/render-support.js).

/// Normalized clip planes of a 0..1-depth view-projection (Camera), the same
/// planes the browser derives from its -w..w matrix. Empty when the matrix is
/// degenerate: nothing is culled.
pub fn planes(m: &[f32; 16]) -> Vec<[f32; 4]> {
    let mut planes = Vec::with_capacity(6);
    for (row, w, sign) in [
        (0, 1., 1.),
        (0, 1., -1.),
        (1, 1., 1.),
        (1, 1., -1.),
        (2, 0., 1.),
        (2, 1., -1.),
    ] {
        {
            let p: [f32; 4] = std::array::from_fn(|c| w * m[c * 4 + 3] + sign * m[c * 4 + row]);
            let length = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            if length.is_nan() || length <= 0. || !p.iter().all(|v| v.is_finite()) {
                return Vec::new();
            }
            planes.push(p.map(|v| v / length));
        }
    }
    planes
}

pub fn outside(planes: &[[f32; 4]], min: [f32; 3], max: [f32; 3]) -> bool {
    planes.iter().any(|p| {
        // Most-positive box corner. A box intersecting any clip plane stays drawn.
        let (mut distance, mut magnitude) = (p[3], p[3].abs());
        for a in 0..3 {
            let v = p[a] * if p[a] >= 0. { max[a] } else { min[a] };
            distance += v;
            magnitude += v.abs();
        }
        // Allow for float32 GPU transforms at the original large world coordinates.
        distance < -(0.05 + magnitude * 1e-6)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Camera, default_focal_length};

    #[test]
    fn planes_cull_behind_beyond_far_and_keep_visible_boxes() {
        let camera = Camera {
            x: 0.,
            y: 0.,
            z: 0.,
            yaw: 0.,
            pitch: 0.,
        };
        let planes = planes(&camera.view_projection(16. / 9., default_focal_length(), 1., 500.));
        assert_eq!(planes.len(), 6);
        let at = |z: f32| ([-1., -1., z - 1.], [1., 1., z + 1.]);
        let (min, max) = at(100.);
        assert!(!outside(&planes, min, max), "ahead");
        let (min, max) = at(-100.);
        assert!(outside(&planes, min, max), "behind");
        let (min, max) = at(600.);
        assert!(outside(&planes, min, max), "beyond far");
        let (min, max) = at(500.5);
        assert!(!outside(&planes, min, max), "straddles far");
        assert!(super::planes(&[f32::NAN; 16]).is_empty());
    }
}
