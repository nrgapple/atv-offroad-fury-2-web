//! Options → Accessibility → Tilt steering (authored; phones in the
//! browser). The page shell (web/engine.js `window.furyTilt`) turns
//! DeviceOrientation into a steering-wheel angle and asks iOS for motion
//! access; this reads that angle and makes it a steering axis.

/// Degrees either side of straight that steer nothing.
const DEAD: f32 = 3.;

/// The steering axis for a turn of `angle` degrees (positive right), full
/// lock at `lock` degrees.
pub fn steer(angle: f32, lock: f32) -> f32 {
    if !angle.is_finite() || angle.abs() <= DEAD || lock <= DEAD {
        return 0.;
    }
    ((angle.abs() - DEAD) / (lock - DEAD))
        .min(1.)
        .copysign(angle)
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static KEYS: [wasm_bindgen::JsValue; 3] = ["furyTilt".into(), "live".into(), "angle".into()];
}

#[cfg(target_arch = "wasm32")]
fn shell() -> Option<wasm_bindgen::JsValue> {
    let window = web_sys::window()?;
    let tilt = KEYS.with(|k| js_sys::Reflect::get(&window, &k[0])).ok()?;
    (tilt.is_object()).then_some(tilt)
}

/// The phone's latest steering-wheel angle, once readings arrive.
pub fn angle() -> Option<f32> {
    #[cfg(target_arch = "wasm32")]
    {
        let tilt = shell()?;
        KEYS.with(|k| {
            js_sys::Reflect::get(&tilt, &k[1])
                .ok()?
                .as_bool()?
                .then_some(())?;
            js_sys::Reflect::get(&tilt, &k[2])
                .ok()?
                .as_f64()
                .map(|a| a as f32)
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

/// Starts the readings: on iOS this asks for motion access, which must
/// come from a tap (the menu row's, or the shell's next one).
pub fn enable() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let Some(tilt) = shell() else {
            return;
        };
        if let Ok(function) = js_sys::Reflect::get(&tilt, &"enable".into())
            .and_then(|f| f.dyn_into::<js_sys::Function>())
        {
            let _ = function.call0(&tilt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::steer;

    #[test]
    fn tilt_steers_past_the_dead_zone_up_to_full_lock() {
        assert_eq!(steer(0., 28.), 0.);
        assert_eq!(steer(-2.5, 28.), 0.);
        assert!((steer(15.5, 28.) - 0.5).abs() < 1e-6);
        assert!((steer(-15.5, 28.) + 0.5).abs() < 1e-6);
        assert_eq!(steer(80., 28.), 1.);
        assert_eq!(steer(-80., 18.), -1.);
        assert_eq!(steer(f32::NAN, 28.), 0.);
        assert!(steer(10., 18.) > steer(10., 40.));
    }
}
