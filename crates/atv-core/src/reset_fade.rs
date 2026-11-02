//! The post-reset fade (research/RACING.md "Post-reset fade").
//!
//! Exact-build evidence (USA v2.00 `SCUS_972.11`, private exports):
//!
//! - `001e9050` starts the vehicle's two-second reset timer (+0x800), adds
//!   two collision categories to the quad's, rider's and wheels' collision
//!   objects and, when the vehicle's fade flag (+0x804) is set, draws the
//!   quad and rider objects at the scene's ResetOpacity (`00209680`:
//!   Environment "ResetOpacity", default 0.2).
//! - `001e5260` counts the timer down each step; once it is at or below
//!   0.75 s the opacity ramps back to 1. At 0 `001e92c0` restores opacity
//!   1 and the normal collision categories.
//! - Every wreck recovery (`001e8d10` with its fade argument set, outside
//!   Training) and every reset through `001e8f90` (the off-track reset)
//!   starts it, for CPUs and humans alike. The deep-water reset (`001e5260`
//!   straight to `001e35d0`) and the race grid (`001f80e0` restores at
//!   once) do not.
//! - In Lap Attack (session mode 2) `001e1fd0` clears the fade flag for a
//!   human and starts the timer, and `001e5260` never counts it down: the
//!   human stays opaque with the reset collision categories.

/// The reset timer's start (`001e9050`, 0x40000000).
pub const SECONDS: f32 = 2.;

/// The last stretch of the timer over which the opacity returns to 1
/// (`001e5260`).
pub const RAMP: f32 = 0.75;

/// ResetOpacity's default (`00209680`, world +0x510).
pub const OPACITY: f32 = 0.2;

/// The drawn opacity with `timer` seconds left at `reset_opacity`: the
/// reset opacity until the ramp, then
/// `reset + (1 − reset)(1 − timer / 0.75)`; 1 once the timer has run out.
pub fn opacity(timer: f32, reset_opacity: f32) -> f32 {
    if timer.is_nan() || timer <= 0. {
        return 1.;
    }
    let reset = reset_opacity.clamp(0., 1.);
    if timer > RAMP {
        return reset;
    }
    (reset + (1. - reset) * (1. - timer / RAMP)).clamp(0., 1.)
}

/// Whether a rider draws the fade (+0x804): every rider except a Lap
/// Attack human (`001e1fd0`).
pub fn draws(lap_attack: bool, human: bool) -> bool {
    !(lap_attack && human)
}

/// Whether the timer holds instead of counting down: Lap Attack
/// (`001e5260` skips the countdown in session mode 2).
pub fn holds(lap_attack: bool) -> bool {
    lap_attack
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_holds_then_ramps_back_over_the_last_three_quarters_second() {
        assert_eq!(opacity(SECONDS, OPACITY), OPACITY);
        assert_eq!(opacity(1.0, OPACITY), OPACITY);
        assert_eq!(opacity(RAMP + 1e-4, OPACITY), OPACITY);
        assert_eq!(opacity(RAMP, OPACITY), OPACITY);
        assert!((opacity(RAMP / 2., OPACITY) - 0.6).abs() < 1e-6);
        assert!((opacity(1e-6, OPACITY) - 1.).abs() < 1e-5);
        assert_eq!(opacity(0., OPACITY), 1.);
        assert_eq!(opacity(-0.01, OPACITY), 1.);
        assert_eq!(opacity(f32::NAN, OPACITY), 1.);
        let mut previous = 0.;
        for step in 0..=120 {
            let value = opacity(SECONDS - step as f32 / 60., OPACITY);
            assert!(value >= previous);
            previous = value;
        }
    }

    #[test]
    fn venue_opacity_is_clamped() {
        assert_eq!(opacity(1.5, 0.5), 0.5);
        assert_eq!(opacity(1.5, -1.), 0.);
        assert_eq!(opacity(1.5, 3.), 1.);
    }

    #[test]
    fn lap_attack_humans_stay_opaque_and_hold_the_timer() {
        assert!(draws(false, true));
        assert!(draws(false, false));
        assert!(draws(true, false));
        assert!(!draws(true, true));
        assert!(holds(true));
        assert!(!holds(false));
    }
}
