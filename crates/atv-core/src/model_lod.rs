//! Original static-model visibility policy (research/VISIBILITY.md).
//!
//! The PSG model pre-draw visitor `001b39e0` skips hidden models, asks the
//! model class's visibility method (`001b39d0`, which always answers yes for
//! PSG models: whole-object rejection is the spatial tree's job) and then
//! selects the level of detail in `001b3a90`. The draw `001b2390` hands the
//! chosen level's groups to the shader dispatch `001b90b0` with a "far" flag
//! (model distance above 130 ft) that limits a material to its first pass
//! unless the PSM's `[General] EnableMultiPassCull` (`001bf430`, default 1)
//! is cleared.
//!
//! Level selection (`001b3a90`): with `r` the bounds radius and `d` the
//! distance from the eye to the bounds centre, `s = F / max(d - r, 0.1)`
//! where `F` is the view's horizontal focal length in pixels (`0011b070`:
//! half the 640-pixel gameplay viewport over tan(FOV/2)). Each stored level
//! `i` except the last (whose threshold is FLT_MAX) is passed when
//! `scale * s * 1.0 * threshold_i < 1`; the drawn level is one past the last
//! passed level, or the finest when none passes.
use crate::{Result, mesh::Mesh};

/// The gameplay viewport width every `0011a810` caller passes (640x448).
pub const VIEW_WIDTH: f32 = 640.;
/// `001b3a90`'s global multiplier (DAT_00408350).
pub const LOD_FACTOR: f32 = 1.;
/// The geometry's own multiplier, initialised to 1 by `001b2610` (+0x14)
/// and not changed by any loader found.
pub const GEOMETRY_SCALE: f32 = 1.;
/// `001b2390`: beyond this model distance (DAT_00408348, feet) materials with
/// `EnableMultiPassCull` draw only their first pass (`001b90b0`).
pub const MULTIPASS_CULL_DISTANCE: f32 = 130.;
/// `001b3a90`'s lower bound on the distance beyond the bounds radius.
pub const NEAR_CLAMP: f32 = 0.1;

/// The original focal length in viewport pixels for a horizontal field of
/// view in degrees (`0011b070`).
pub fn focal_from_fov(horizontal_fov_degrees: f32) -> f32 {
    VIEW_WIDTH * 0.5 / (horizontal_fov_degrees.to_radians() * 0.5).tan()
}

/// The browser view's `focal_length` is `(4/3) / tan(FOV/2)` (the vertical
/// NDC focal of the 4:3 reference frame), so the original 640-pixel focal is
/// `320 / tan(FOV/2) = 240 * focal_length`.
pub fn focal_from_view(focal_length: f32) -> f32 {
    VIEW_WIDTH * 0.5 * 0.75 * focal_length
}

/// One model's LOD inputs: the stored thresholds (one per level) and the
/// bounds `001b3a90` measures from.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModelLod {
    pub thresholds: Vec<f32>,
    /// Model-local bounds centre: the root node's own centre (+0x100) for a
    /// one-node PSG, otherwise the aggregate box centre (+0x120, `001b3ed0`).
    pub center: [f32; 3],
    /// The root node's sphere radius (+0x154) for a one-node PSG, otherwise
    /// the length of the aggregate box's half extents (+0x158).
    pub radius: f32,
}

impl ModelLod {
    /// Scene and TOY models load with the box branch of `001b3ed0` (only
    /// vehicles, `001e3b80`, and keyframed actors, `0023cdc0`, set the sphere
    /// flag).
    pub fn from_mesh(mesh: &Mesh) -> Result<Self> {
        let thresholds = mesh.lods.iter().map(|l| l.threshold).collect();
        let root = mesh.nodes.first().ok_or("PSG has no root node")?;
        let (center, radius) = if mesh.nodes.len() == 1 {
            (
                [root.bounds[0], root.bounds[1], root.bounds[2]],
                root.bounds[6],
            )
        } else {
            let b = mesh.bind_bounds()?;
            let h = b.half_extents;
            (b.center, (h[2] * h[2] + h[0] * h[0] + h[1] * h[1]).sqrt())
        };
        Ok(Self {
            thresholds,
            center,
            radius,
        })
    }

    /// `001b3a90`'s distance (stored at +0x180): the local centre less the
    /// eye, plus the model translation. The centre offset is not rotated.
    pub fn distance(&self, translation: [f32; 3], eye: [f32; 3]) -> f32 {
        let d: [f32; 3] = std::array::from_fn(|a| (self.center[a] - eye[a]) + translation[a]);
        (d[2] * d[2] + d[0] * d[0] + d[1] * d[1]).sqrt()
    }

    pub fn select(&self, distance: f32, focal: f32) -> usize {
        select(&self.thresholds, self.radius, distance, focal)
    }

    /// The distance from which each coarser level is drawn (census aid):
    /// level `i + 1` starts once `d - r > threshold_i * F`.
    pub fn switch_distances(&self, focal: f32) -> Vec<f32> {
        let n = self.thresholds.len().saturating_sub(1);
        self.thresholds[..n]
            .iter()
            .map(|t| self.radius + t * GEOMETRY_SCALE * LOD_FACTOR * focal)
            .collect()
    }
}

/// `001b3a90`'s level choice for `thresholds` (finest first).
pub fn select(thresholds: &[f32], radius: f32, distance: f32, focal: f32) -> usize {
    let beyond = distance - radius;
    // A select on the comparison: values under 0.1 (not NaN) become 0.1.
    let beyond = if beyond < NEAR_CLAMP {
        NEAR_CLAMP
    } else {
        beyond
    };
    let s = focal / beyond;
    let mut level = 0;
    for (i, threshold) in thresholds
        .iter()
        .take(thresholds.len().saturating_sub(1))
        .enumerate()
    {
        if GEOMETRY_SCALE * s * LOD_FACTOR * threshold < 1. {
            level = i + 1;
        }
    }
    level
}

/// `001b2390`: the far flag passed to the shader dispatch.
pub fn multipass_far(distance: f32) -> bool {
    MULTIPASS_CULL_DISTANCE < distance
}

/// `[General] EnableMultiPassCull` of a PSM (`001bf430`, default 1), read
/// with the shared TOY/PSM flag reader (`001421b0`/`00141fc0`).
pub fn multipass_cull(psm: &str) -> bool {
    let mut general = false;
    for line in psm.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[') {
            general = header
                .split(']')
                .next()
                .is_some_and(|h| h.trim().eq_ignore_ascii_case("General"));
            continue;
        }
        if !general {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("EnableMultiPassCull") {
            return crate::scene::flag(value.trim());
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: f32 = f32::MAX;

    #[test]
    fn focal_follows_the_640_pixel_viewport() {
        // 85 degrees: the native follow camera's horizontal field of view.
        let f = focal_from_fov(85.);
        assert!((f - 349.21).abs() < 0.01, "{f}");
        // The browser's 4:3 reference focal maps to the same pixels.
        let view = (4. / 3.) / (85f32.to_radians() * 0.5).tan();
        assert!((focal_from_view(view) - f).abs() < 1e-3);
    }

    #[test]
    fn levels_coarsen_past_each_threshold() {
        // chalet01: 0.2, 0.4, FLT_MAX; radius 10, F = 300.
        let t = [0.2, 0.4, MAX];
        let (r, f) = (10., 300.);
        // Switch at r + 0.2 * 300 = 70 and r + 0.4 * 300 = 130 ft.
        assert_eq!(select(&t, r, 0., f), 0, "inside the bounds clamps to 0.1");
        assert_eq!(select(&t, r, 69.9, f), 0);
        assert_eq!(select(&t, r, 70.1, f), 1);
        assert_eq!(select(&t, r, 129.9, f), 1);
        assert_eq!(select(&t, r, 130.1, f), 2);
        // The last (FLT_MAX) level is never compared: it is reached by
        // passing every finer threshold, and no level beyond it exists.
        assert_eq!(select(&t, r, 1e9, f), 2);
        // Exactly at the switch distance 0.2 * F / (d - r) == 1: not passed.
        assert_eq!(select(&[0.5, MAX], 0., 150., 300.), 0);
        assert_eq!(select(&[0.5, MAX], 0., 150.01, 300.), 1);
    }

    #[test]
    fn single_level_models_never_change() {
        for d in [0., 50., 500., 5000.] {
            assert_eq!(select(&[MAX], 3., d, 349.), 0);
        }
        assert_eq!(select(&[], 3., 10., 349.), 0);
    }

    #[test]
    fn levels_are_counted_not_searched() {
        // 001b3a90 keeps the last passing index without breaking, so an
        // unsorted table selects one past the last passing entry.
        assert_eq!(select(&[0.5, 0.1, MAX], 0., 100., 300.), 2);
        assert_eq!(select(&[0.1, 0.5, MAX], 0., 100., 300.), 1);
    }

    #[test]
    fn distance_adds_the_unrotated_centre_to_the_translation() {
        let lod = ModelLod {
            thresholds: vec![0.2, MAX],
            center: [0., 3., 4.],
            radius: 1.,
        };
        assert_eq!(lod.distance([10., 0., 0.], [10., 3., 0.]), 4.);
        let switches = lod.switch_distances(300.);
        assert_eq!(switches, vec![1. + 0.2 * 300.]);
    }

    #[test]
    fn far_pass_cull_is_strictly_beyond_130_feet() {
        assert!(!multipass_far(130.));
        assert!(multipass_far(130.001));
    }

    #[test]
    fn multipass_cull_defaults_on_and_reads_the_general_flag() {
        assert!(multipass_cull(""));
        assert!(multipass_cull("[General]\nTotalShaderPasses = 2\n"));
        assert!(!multipass_cull(
            "[General] \nTotalShaderPasses=2\nEnableMultiPassCull       = 0 ; off\n"
        ));
        assert!(!multipass_cull("[general]\nenablemultipasscull=F\n"));
        assert!(multipass_cull("[General]\nEnableMultiPassCull=1\n"));
        // Only the General section is read.
        assert!(multipass_cull("[ShaderPass_1]\nEnableMultiPassCull=0\n"));
    }
}
