//! USA v2.00 speed/preload and stunt text controllers. Resource layouts and
//! bitmap glyphs stay in the private runtime archives; no race state is invented.
use crate::{Result, engine_audio::OriginalRandom, ui::Document};
use serde::Serialize;

pub const MPH_FACTOR: f32 = 0.68182;

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub scale_up: f32,
    pub scale_stay: f32,
    pub scale_down: f32,
    pub scale_size: f32,
    pub color_time: f32,
    pub failure_color: [f32; 3],
    pub alpha_time: f32,
}
impl Default for Config {
    fn default() -> Self {
        // OverlayObject-12 in the exact build's Overlays.pb, including overrides
        // of the executable's generic StuntText loader defaults.
        Self {
            scale_up: 0.05,
            scale_stay: 0.25,
            scale_down: 0.05,
            scale_size: 2.,
            color_time: 0.25,
            failure_color: [180., 0., 0.],
            alpha_time: 1.,
        }
    }
}
impl Config {
    pub fn from_document(doc: &Document) -> Result<Self> {
        let mut c = Self::default();
        for (key, out) in [
            ("ScaleUpTime", &mut c.scale_up),
            ("ScaleStayTime", &mut c.scale_stay),
            ("ScaleDownTime", &mut c.scale_down),
            ("ScaleSize", &mut c.scale_size),
            ("ColorChangeTime", &mut c.color_time),
            ("AlphaOutChangeTime", &mut c.alpha_time),
        ] {
            if let Some(v) = doc.value("OverlayObject-12", key) {
                *out = v.parse().map_err(|_| format!("invalid HUD {key}"))?;
            }
            if !out.is_finite() || *out <= 0. {
                return Err(format!("invalid HUD {key}"));
            }
        }
        if let Some(v) = doc.value("OverlayObject-12", "ColorChange") {
            let rgb: Vec<f32> = v
                .split(',')
                .map(|s| s.trim().parse::<f32>())
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| "invalid HUD ColorChange")?;
            if rgb.len() != 3
                || rgb
                    .iter()
                    .any(|x| !x.is_finite() || !(0. ..=255.).contains(x))
            {
                return Err("invalid HUD ColorChange".into());
            }
            c.failure_color.copy_from_slice(&rgb);
        }
        Ok(c)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Frame {
    pub speed_text: String,
    pub units: &'static str,
    pub preload_visible: bool,
    /// Orange icon height / original height. Blue alpha uses a distinct timer.
    pub preload_fill: f32,
    pub preload_blue_alpha: i16,
    pub stunt_visible: bool,
    pub stunt_name: String,
    pub stunt_points: String,
    pub stunt_scale: f32,
    pub stunt_color: [i32; 3],
    pub stunt_alpha: i16,
    pub stunt_offsets: [[f32; 2]; 2],
    pub scale_phase: u8,
}

pub struct State {
    config: Config,
    speed: i32,
    factor: f32,
    preload_timer: f32,
    preload_amount: f32,
    preload_previous: bool,
    preload_visible: bool,
    preload_fill: f32,
    preload_blue_alpha: i16,
    visible: bool,
    name: String,
    points: i32,
    lifetime: f32,
    phase: u8,
    phase_timer: f32,
    scale: f32,
    scale_rate: f32,
    scale_return: bool,
    color: [f32; 3],
    base_color: [f32; 3],
    color_rate: [f32; 3],
    color_timer: f32,
    alpha: f32,
    base_alpha: i16,
    alpha_rate: f32,
    alpha_timer: f32,
    shake_timer: f32,
    offsets: [[f32; 2]; 2],
    random: OriginalRandom,
}
impl Default for State {
    fn default() -> Self {
        Self::new(Config::default())
    }
}
impl State {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            speed: 0,
            factor: MPH_FACTOR,
            preload_timer: 0.,
            preload_amount: 0.,
            preload_previous: false,
            preload_visible: false,
            preload_fill: 0.,
            preload_blue_alpha: 255,
            visible: false,
            name: String::new(),
            points: 0,
            lifetime: 0.,
            phase: 3,
            phase_timer: 0.,
            scale: 1.,
            scale_rate: 1.,
            scale_return: false,
            color: [218., 192., 31.],
            base_color: [218., 192., 31.],
            color_rate: [0.; 3],
            color_timer: 0.,
            alpha: 255.,
            base_alpha: 255,
            alpha_rate: 0.,
            alpha_timer: 0.,
            shake_timer: 0.,
            offsets: [[0.; 2]; 2],
            // Original libc generator, local stream. Shared game RNG draw order
            // is still unknown; this does not establish pixel-exact shake parity.
            random: OriginalRandom::with_seed(1),
        }
    }
    /// Begin a new drive using the already loaded original HUD configuration
    /// and chosen units. No score, preload tail or text animation crosses reset.
    pub fn reset(&mut self) {
        let factor = self.factor;
        *self = Self::new(self.config);
        self.factor = factor;
    }
    pub fn set_speed_factor(&mut self, factor: f32) -> Result<()> {
        if !factor.is_finite() || factor <= 0. {
            return Err("invalid HUD speed units".into());
        }
        self.factor = factor;
        Ok(())
    }
    /// 001eaa70 caches pending sum/count before native clears the linked list.
    /// 002dd4b0 truncates the sum FIRST, then multiplies only when inactive.
    pub fn set_score_cache(&mut self, sum: f32, count: usize, active: bool) -> Result<()> {
        if !sum.is_finite() || count > crate::stunt_score::EVENT_CAPACITY {
            return Err("invalid HUD stunt score cache".into());
        }
        self.points = if active {
            sum as i32
        } else {
            (sum as i32).wrapping_mul(count as i32)
        };
        Ok(())
    }
    /// 002dd4b0 gives eligible wheelie points priority over stunt cache text.
    /// Apply after ordinary cache updates, only while actor+918 is nonzero.
    pub fn set_wheelie_points(&mut self, points: f32) -> Result<()> {
        if !points.is_finite() {
            return Err("invalid HUD wheelie score".into());
        }
        self.points = points as i32;
        Ok(())
    }
    pub fn wheelie_event(&mut self, event: crate::stunt_score::WheelieNotification) {
        use crate::stunt_score::WheelieNotification;
        match event {
            WheelieNotification::Name => {
                self.name_event("Wheelie");
            }
            WheelieNotification::Finished { .. } => {
                // 001e70e0 pulses only if overlay flag+29 bit0 is set; an
                // invisible overlay is reset, without showing a new panel.
                if self.visible {
                    self.finish_event(true, 1);
                } else {
                    self.reset_stunt();
                }
            }
            WheelieNotification::Failed => self.finish_event(false, 1),
        }
    }
    /// 002da6c0 hides/resets the wheelie panel before naming an accepted stunt.
    pub fn clear_wheelie_for_stunt(&mut self) {
        self.hide();
    }
    /// 002e3b70 changes the event name only while scale phase == 3.
    pub fn name_event(&mut self, name: &str) -> bool {
        if self.phase != 3 {
            return false;
        }
        self.lifetime = 2.;
        self.name = name.into();
        if !self.visible {
            self.reset_stunt();
            self.visible = true;
        }
        true
    }
    /// 002e3c70, after the caller establishes event/chain eligibility.
    pub fn finish_event(&mut self, success: bool, chain_count: usize) {
        if success {
            if chain_count > 1 {
                self.name = format!("COMBO x {chain_count}");
            }
            self.phase_timer = self.config.scale_up;
            self.scale_rate = (self.config.scale_size - 1.) / self.phase_timer;
            self.scale = 1.;
            self.phase = 0;
            self.scale_return = true;
        } else {
            self.base_color = self.color;
            self.color_rate = std::array::from_fn(|i| {
                (self.config.failure_color[i] - self.color[i]) / self.config.color_time
            });
            self.color_timer = self.config.color_time;
            self.base_alpha = self.alpha as i16;
            self.alpha_rate = -f32::from(self.base_alpha) / self.config.alpha_time;
            self.alpha_timer = self.config.alpha_time;
            self.shake_timer = self.config.alpha_time;
        }
    }
    /// 002e4000/4490. Native reset preserves the captured starting color/alpha.
    pub fn hide(&mut self) {
        self.reset_stunt();
        self.visible = false;
    }
    fn reset_stunt(&mut self) {
        self.phase_timer = 0.;
        self.phase = 3;
        self.scale = 1.;
        self.color_timer = 0.;
        self.color = self.base_color;
        self.alpha_timer = 0.;
        self.alpha = f32::from(self.base_alpha);
        self.shake_timer = 0.;
        self.offsets = [[0.; 2]; 2];
    }
    /// Advance native update once per simulation tick. Speed is feet/second;
    /// preload status/strength come from the recovered rider controller, not
    /// browser button state. chain_active prevents expiration while chaining.
    pub fn step(
        &mut self,
        dt: f32,
        speed: f32,
        preload_status: bool,
        preload_strength: f32,
        chain_active: bool,
        points: f32,
    ) -> Result<()> {
        if !dt.is_finite()
            || dt < 0.
            || !speed.is_finite()
            || !preload_strength.is_finite()
            || !points.is_finite()
        {
            return Err("nonfinite HUD input/time step".into());
        }
        self.speed = ((speed * self.factor) as i32).max(0);
        if chain_active {
            self.points = points as i32;
        }
        // Virtual update 002e25b0 precedes the render-side 002e2290.
        if self.preload_timer >= 0. {
            self.preload_timer -= dt;
        }
        if preload_status {
            if !self.preload_previous {
                self.preload_visible = true;
                self.preload_amount = 0.;
            }
            self.preload_timer = 2.;
            self.preload_blue_alpha = 255;
            if preload_strength != 0. {
                self.preload_amount = preload_strength;
                self.preload_timer = 2.;
            }
            self.preload_fill = self.preload_amount;
        }
        if self.preload_timer < 0. || self.preload_amount <= 0. {
            if !preload_status {
                self.preload_visible = false;
                self.preload_timer = 0.;
            }
        } else if self.preload_timer <= 1. {
            self.preload_fill = self.preload_timer * self.preload_amount;
            self.preload_blue_alpha = (255. * self.preload_timer) as i16;
        }
        self.preload_previous = preload_status;
        self.lifetime -= dt;
        if self.lifetime <= 0. && self.phase == 3 && !chain_active {
            self.hide();
            return Ok(());
        }
        match self.phase {
            0 => {
                self.phase_timer -= dt;
                if self.phase_timer <= 0. {
                    self.phase = 1;
                    self.phase_timer = self.config.scale_stay;
                } else {
                    self.scale += self.scale_rate * dt;
                }
            }
            1 => {
                self.phase_timer -= dt;
                if self.phase_timer <= 0. {
                    if self.scale_return {
                        self.phase_timer = self.config.scale_down;
                        self.scale_rate = (self.scale - 1.) / self.phase_timer;
                        self.phase = 2;
                    } else {
                        self.phase_timer = 0.;
                        self.phase = 3;
                        self.scale = 1.;
                    }
                }
            }
            2 => {
                self.phase_timer -= dt;
                if self.phase_timer <= 0. {
                    self.phase_timer = 0.;
                    self.phase = 3;
                    self.scale = 1.;
                } else {
                    self.scale -= self.scale_rate * dt;
                }
            }
            _ => {}
        }
        // The original does not snap color/alpha to the target on expiry.
        if self.color_timer > 0. {
            self.color_timer -= dt;
            if self.color_timer <= 0. {
                self.color_timer = 0.;
            } else {
                for i in 0..3 {
                    self.color[i] += dt * self.color_rate[i];
                }
            }
        }
        if self.alpha_timer > 0. {
            self.alpha_timer -= dt;
            if self.alpha_timer <= 0. {
                self.alpha_timer = 0.;
            } else {
                self.alpha += self.alpha_rate * dt;
            }
        }
        if self.shake_timer > 0. {
            self.shake_timer -= dt;
            if self.shake_timer <= 0. {
                self.shake_timer = 0.;
                self.offsets = [[0.; 2]; 2];
            } else {
                // 002e3640 applies integer deltas to current text positions.
                for text in &mut self.offsets {
                    for coordinate in text {
                        *coordinate +=
                            ((self.random.next_u31() as f32 * 4.656613e-10 * 6.) as i32 - 3) as f32;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn frame(&self) -> Frame {
        Frame {
            speed_text: format!("{:02}", self.speed),
            units: if self.factor == MPH_FACTOR {
                "Mph"
            } else {
                "Kph"
            },
            preload_visible: self.preload_visible,
            preload_fill: self.preload_fill,
            preload_blue_alpha: self.preload_blue_alpha,
            stunt_visible: self.visible,
            stunt_name: self.name.clone(),
            stunt_points: if self.points == 0 {
                String::new()
            } else {
                self.points.to_string()
            },
            stunt_scale: self.scale,
            stunt_color: self.color.map(|x| x as i32),
            stunt_alpha: self.alpha as i16,
            stunt_offsets: self.offsets,
            scale_phase: self.phase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn speed_has_original_truncation_clamp_and_two_digit_minimum() {
        let mut s = State::default();
        for (speed, text) in [
            (-40., "00"),
            (1.46, "00"),
            (1.47, "01"),
            (100., "68"),
            (160., "109"),
        ] {
            s.step(0., speed, false, 0., false, 0.).unwrap();
            assert_eq!(s.frame().speed_text, text);
        }
    }
    #[test]
    fn preload_persists_then_shrinks_and_fades_for_final_second() {
        let mut s = State::default();
        s.step(0., 0., true, 0.75, false, 0.).unwrap();
        s.step(0.5, 0., false, 0., false, 0.).unwrap();
        assert_eq!(s.frame().preload_fill, 0.75);
        s.step(0.75, 0., false, 0., false, 0.).unwrap();
        assert_eq!(s.frame().preload_fill, 0.5625);
        assert_eq!(s.frame().preload_blue_alpha, 191);
        s.step(0.75, 0., false, 0., false, 0.).unwrap();
        assert!(s.frame().preload_visible); // strict < 0 expiry
        s.step(0.01, 0., false, 0., false, 0.).unwrap();
        assert!(!s.frame().preload_visible);
    }
    #[test]
    fn stunt_pulse_keeps_native_discrete_boundary_and_name_gate() {
        let mut s = State::default();
        s.name_event("Nac Nac");
        s.finish_event(true, 2);
        s.step(0.025, 0., false, 0., true, 500.).unwrap();
        assert_eq!(s.frame().stunt_scale, 1.5);
        assert!(!s.name_event("Can Can"));
        s.step(0.025, 0., false, 0., true, 500.).unwrap();
        assert_eq!(s.frame().stunt_scale, 1.5); // no exact-end snap to 2
        assert_eq!(s.frame().stunt_name, "COMBO x 2");
        s.step(0.25, 0., false, 0., true, 500.).unwrap();
        s.step(0.05, 0., false, 0., true, 500.).unwrap();
        assert_eq!(s.frame().stunt_scale, 1.);
        assert!(s.name_event("Can Can"));
    }
    #[test]
    fn failures_do_not_snap_color_or_alpha_at_timer_expiration() {
        let mut s = State::default();
        s.name_event("Nac Nac");
        s.finish_event(false, 1);
        s.step(0.125, 0., false, 0., true, 500.).unwrap();
        assert_eq!(s.frame().stunt_color, [199, 96, 15]);
        s.step(0.125, 0., false, 0., true, 500.).unwrap();
        assert_eq!(s.frame().stunt_color, [199, 96, 15]);
        s.step(0.75, 0., false, 0., true, 500.).unwrap();
        assert_eq!(s.frame().stunt_alpha, 191);
    }
    #[test]
    fn cached_score_truncates_before_chain_multiplier_and_survives_list_clear() {
        let mut s = State::default();
        s.set_score_cache(500.75, 3, true).unwrap();
        assert_eq!(s.frame().stunt_points, "500");
        s.set_score_cache(500.75, 3, false).unwrap();
        s.step(0., 0., false, 0., false, 0.).unwrap();
        assert_eq!(s.frame().stunt_points, "1500");
        s.set_score_cache(0., 0, false).unwrap();
        assert_eq!(s.frame().stunt_points, "");
    }
    #[test]
    fn reset_preserves_resource_tuning_and_units() {
        let mut s = State::new(Config {
            scale_size: 3.,
            ..Config::default()
        });
        s.set_speed_factor(1.1).unwrap();
        s.name_event("Nac Nac");
        s.set_score_cache(500., 2, false).unwrap();
        s.step(0., 50., true, 1., true, 500.).unwrap();
        s.reset();
        assert_eq!(s.frame().units, "Kph");
        assert!(!s.frame().preload_visible && !s.frame().stunt_visible);
        assert_eq!(s.frame().stunt_points, "");
        s.name_event("Nac Nac");
        s.finish_event(true, 1);
        s.step(0.025, 0., false, 0., true, 500.).unwrap();
        assert_eq!(s.frame().stunt_scale, 2.);
    }
    #[test]
    fn wheelie_uses_same_panel_with_priority_points_and_single_event_pulse() {
        use crate::stunt_score::WheelieNotification;
        let mut s = State::default();
        s.set_score_cache(500., 2, true).unwrap();
        s.wheelie_event(WheelieNotification::Name);
        s.set_wheelie_points(20.75).unwrap();
        assert_eq!(s.frame().stunt_name, "Wheelie");
        assert_eq!(s.frame().stunt_points, "20");
        s.wheelie_event(WheelieNotification::Finished { points: 20.75 });
        assert_eq!(s.frame().stunt_name, "Wheelie");
        assert_eq!(s.frame().scale_phase, 0);
        s.clear_wheelie_for_stunt();
        assert!(!s.frame().stunt_visible);
        s.name_event("Nac Nac");
        assert_eq!(s.frame().stunt_name, "Nac Nac");
    }
    #[test]
    fn finished_hidden_wheelie_does_not_show_or_pulse_overlay() {
        let mut s = State::default();
        s.wheelie_event(crate::stunt_score::WheelieNotification::Finished { points: 30. });
        assert!(!s.frame().stunt_visible);
        assert_eq!(s.frame().scale_phase, 3);
    }
}
