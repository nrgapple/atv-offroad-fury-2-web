//! Browser presentation of the recovered camera frame (chasePresentation in
//! web/render-support.js and presentCameraFrame/matrix in web/game.js) with
//! the default settings: Chase camera Original (no uphill assist), crash
//! camera without the optional crash layer, and the speed-rush FOV kick.
use atv_core::camera::CameraFrame;
use atv_core::math::Det;

/// Eye, heading and focal length for the scene camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub focal_length: f32,
}

/// The recovered follow/wreck frame as the browser frames it. Portrait
/// screens pull back and widen, as on phones.
pub fn chase(frame: &CameraFrame, aspect: f32) -> View {
    let aspect = f64::from(aspect);
    let portrait = ((1. - aspect) / 0.5).clamp(0., 1.);
    let distance_scale = 1. + 0.35 * portrait;
    let position = frame.position.map(f64::from);
    let target = frame.target.map(f64::from);
    let offset: [f64; 3] = std::array::from_fn(|i| target[i] - position[i]);
    let yaw = offset[0].atan2_d(offset[2]);
    let position: [f64; 3] =
        std::array::from_fn(|i| position[i] - offset[i] * (distance_scale - 1.));
    let view: [f64; 3] = std::array::from_fn(|i| target[i] - position[i]);
    let pitch = view[1].atan2_d(view[0].hypot_d(view[2])) + 0.12 * portrait;
    let focal_length = (4. / 3.) / (f64::from(frame.fov) * std::f64::consts::PI / 360.).tan_d()
        * (1. - 0.12 * portrait);
    View {
        position: position.map(|v| v as f32),
        yaw: yaw as f32,
        pitch: pitch as f32,
        focal_length: focal_length as f32,
    }
}

/// Authored camera shots (the pre-race camera and the after-finish
/// trackside cameras) keep their pose; only the portrait focal adaptation of
/// the chase view applies (cinematicPresentation in render-support.js).
pub fn cinematic(frame: &CameraFrame, aspect: f32) -> View {
    let portrait = ((1. - f64::from(aspect)) / 0.5).clamp(0., 1.);
    let view: [f64; 3] = std::array::from_fn(|i| f64::from(frame.target[i] - frame.position[i]));
    let focal_length = (4. / 3.) / (f64::from(frame.fov) * std::f64::consts::PI / 360.).tan_d()
        * (1. - 0.12 * portrait);
    View {
        position: frame.position,
        yaw: view[0].atan2_d(view[2]) as f32,
        pitch: view[1].atan2_d(view[0].hypot_d(view[2])) as f32,
        focal_length: focal_length as f32,
    }
}

/// Authored speed presentation's FOV kick (web/speed-rush.js, Normal):
/// above 40 MPH the view widens by up to about six degrees.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpeedRush {
    amount: f64,
}

impl SpeedRush {
    pub fn reset(&mut self) {
        self.amount = 0.;
    }

    /// `speed` in ft/s; `active` while riding, focused and unpaused.
    pub fn update(&mut self, speed: f32, active: bool, dt: f32) {
        let t = ((f64::from(speed).abs() * 3600. / 5280. - 40.) / 35.).clamp(0., 1.);
        let desired = if active && speed.is_finite() {
            t * t * (3. - 2. * t) * 0.75
        } else {
            0.
        };
        if desired == 0. {
            self.reset();
            return;
        }
        let step = f64::from(dt).max(0.);
        self.amount += (desired - self.amount) * (1. - (-step / 0.25).exp_d());
    }

    /// The smoothed amount (0..0.75 at Normal) the screen pass shares.
    pub fn amount(&self) -> f32 {
        self.amount as f32
    }

    pub fn focal_scale(&self) -> f32 {
        (1. - 0.09 * self.amount.clamp(0., 1.)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_keeps_the_recovered_eye_and_aim() {
        let frame = CameraFrame {
            position: [10., 5., 0.],
            target: [10., 4., 10.],
            fov: 60.,
            preset: 0,
        };
        let view = chase(&frame, 16. / 9.);
        assert_eq!(view.position, [10., 5., 0.]);
        assert!(view.yaw.abs() < 1e-6);
        assert!((view.pitch - (-0.1f64).atan_d() as f32).abs() < 1e-6);
        let focal = (4. / 3.) / (std::f64::consts::PI / 6.).tan_d();
        assert!((view.focal_length - focal as f32).abs() < 1e-6);
    }

    #[test]
    fn speed_rush_starts_above_forty_mph() {
        let mut rush = SpeedRush::default();
        rush.update(40. * 5280. / 3600., true, 0.1);
        assert_eq!(rush.focal_scale(), 1.);
        for _ in 0..100 {
            rush.update(80. * 5280. / 3600., true, 0.05);
        }
        assert!((rush.focal_scale() - (1. - 0.09 * 0.75)).abs() < 1e-4);
    }
}
