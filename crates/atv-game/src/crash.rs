//! Optional Crazy/Gorn crashes (authored, off under the Original setting):
//! atv_core::crash_effects stepped after every ride step with atv-web's
//! effects clock, as its simulation_frame does for `funny_crash`.
use crate::{Result, ride::Ride};
use atv_core::crash_effects::{Effects, Frame};

/// Ride Effects → Crashes (RideEffectsSettings.mode in render-support.js).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrashMode {
    #[default]
    Original,
    Crazy,
    Gorn,
}

impl CrashMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "original" => Some(Self::Original),
            "crazy" => Some(Self::Crazy),
            "gorn" => Some(Self::Gorn),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Crazy => "crazy",
            Self::Gorn => "gorn",
        }
    }
}

pub struct Crashes {
    pub mode: CrashMode,
    effects: Effects,
    tick: u64,
    frame: Option<Frame>,
}

impl Crashes {
    pub fn new(mode: CrashMode) -> Self {
        let mut effects = Effects::default();
        effects.set_mode(mode.name(), false);
        Self {
            mode,
            effects,
            tick: 0,
            frame: None,
        }
    }

    /// One simulation step after `ride.step`.
    pub fn step(&mut self, ride: &Ride) -> Result<()> {
        self.tick = self.tick.wrapping_add(1);
        self.frame = ride.step_crash_effects(&mut self.effects, self.tick, 1. / 60.)?;
        Ok(())
    }

    /// The frame a Reset returns: zero step time, the same tick.
    pub fn reset(&mut self, ride: &Ride) -> Result<()> {
        self.frame = ride.step_crash_effects(&mut self.effects, self.tick, 0.)?;
        Ok(())
    }

    pub fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }
}
