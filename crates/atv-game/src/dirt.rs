//! ATV dirt buildup (DirtBuildup in web/graphics.js): one shared 0-255 level
//! for the quad and rider, drawn through the ATV2 dirt reveal (the vehicle
//! shader's `mudReveal` branch). The original `[TerrainZoneN]`
//! DirtAnimChangeTime/DirtAnimChangeAmount rule soils the quad while most
//! grounded wheels share a zone above 10 mph; MUD scenery counts as heavy
//! dirt, water washes it off and a player Reset (or a new ride) cleans it.
//! Wreck recovery keeps it. Authored adaptation; presentation only.
use crate::tire_tracks::TerrainZone;

const MPH: f32 = 5280. / 3600.;
pub const STEP: f64 = 1. / 60.;

/// What one accepted ATV simulation frame offers the dirt rule.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub centers: [[f32; 3]; 4],
    pub radii: [f32; 4],
    /// Water surface under each wheel (-256 without water).
    pub water_heights: [f32; 4],
    pub grounded: [bool; 4],
    pub surfaces: [usize; 4],
    pub mud: [bool; 4],
    /// `speed_magnitude`, ft/s.
    pub speed: f32,
}

#[derive(Clone, Debug)]
pub struct DirtBuildup {
    source: [TerrainZone; 8],
    zones: [TerrainZone; 8],
    rate: f64,
    heavy: bool,
    level: f64,
    timer: f64,
    zone: Option<usize>,
}

impl DirtBuildup {
    pub fn new(zones: [TerrainZone; 8]) -> Self {
        Self {
            source: zones,
            zones,
            rate: 1.,
            heavy: false,
            level: 0.,
            timer: 0.,
            zone: None,
        }
    }

    /// A player Reset (or a new ride) cleans the quad.
    pub fn reset(&mut self) {
        self.level = 0.;
        self.timer = 0.;
        self.zone = None;
    }

    /// The 0-255 level (`dirtLevel`).
    pub fn level(&self) -> f64 {
        self.level
    }

    /// The reveal amount, 0-1.
    pub fn amount(&self) -> f32 {
        (self.level / 255.) as f32
    }

    pub fn heavy(&self) -> bool {
        self.heavy
    }

    /// Alpha Heavy mud (hype.js `heavyMudZones`): faster soiling, dusty
    /// zones soil too, landings splat.
    pub fn set_heavy(&mut self, heavy: bool) {
        self.heavy = heavy;
        self.rate = if heavy { 3. } else { 1. };
        self.zones = if heavy {
            self.source.map(|z| {
                if z.time > 0. && z.amount != 0. {
                    TerrainZone {
                        time: z.time / 4.,
                        amount: z.amount * if z.amount > 0. { 1. } else { 2. },
                        ..z
                    }
                } else if z.dust {
                    TerrainZone {
                        time: 0.2,
                        amount: 1.,
                        ..z
                    }
                } else {
                    z
                }
            })
        } else {
            self.source
        };
    }

    /// The zones tracks and lingering dust read (heavy-adjusted).
    pub fn zones(&self) -> &[TerrainZone; 8] {
        &self.zones
    }

    /// A Heavy mud landing adds `steps` levels.
    pub fn splat(&mut self, steps: f64) {
        self.level = (self.level + steps).clamp(0., 255.);
    }

    /// Exactly one call per accepted 60 Hz simulation frame.
    pub fn step(&mut self, frame: &Sample, dt: f64) {
        let submerged = (0..4)
            .filter(|&i| frame.water_heights[i] > frame.centers[i][1] - frame.radii[i] * 0.5)
            .count();
        if submerged > 0 {
            // Authored wash: a fully submerged quad comes clean in about four seconds.
            self.level = (self.level - 255. * 0.25 * dt * submerged as f64 / 4.).max(0.);
            self.timer = 0.;
            return;
        }
        let mut counts = [0u8; 8];
        let mut any = false;
        for i in 0..4 {
            if frame.grounded[i] && frame.surfaces[i] < 8 {
                counts[frame.surfaces[i]] += 1;
                any = true;
            }
        }
        if frame.speed.abs() <= 10. * MPH || !any {
            return;
        }
        // The zone under most grounded wheels (lowest index on a tie).
        let zone = (0..8).fold(0, |best, z| if counts[z] > counts[best] { z } else { best });
        let mud = (0..4).any(|i| frame.mud[i] && frame.grounded[i]);
        let TerrainZone { time, amount, .. } = self.zones[zone];
        let (mut time, mut amount) = (f64::from(time), f64::from(amount));
        if mud {
            time = if time > 0. { time.min(0.1) } else { 0.1 };
            amount = amount.max(1.);
        }
        if self.zone != Some(zone) {
            self.zone = Some(zone);
            self.timer = 0.;
        }
        // NaN-free: zone times parse clamped to >= 0.
        if time <= 0. || amount == 0. {
            return;
        }
        self.timer += dt * self.rate;
        while self.timer >= time - 1e-6 {
            self.timer -= time;
            self.level = (self.level + amount).clamp(0., 255.);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zones() -> [TerrainZone; 8] {
        let mut zones = [TerrainZone::default(); 8];
        zones[1] = TerrainZone {
            time: 0.5,
            amount: 3.,
            tire_track: true,
            dust: true,
        };
        zones[2].dust = true;
        zones
    }

    fn riding(surface: usize) -> Sample {
        Sample {
            centers: [[0., 2., 0.]; 4],
            radii: [1.; 4],
            water_heights: [-256.; 4],
            grounded: [true; 4],
            surfaces: [surface; 4],
            mud: [false; 4],
            speed: 30.,
        }
    }

    // Expected levels from web/graphics.js DirtBuildup on the same frames.
    #[test]
    fn follows_the_browser_rule() {
        let mut dirt = DirtBuildup::new(zones());
        for _ in 0..120 {
            dirt.step(&riding(1), STEP);
        }
        assert_eq!(dirt.level(), 12.);
        // Slow: no change.
        let slow = Sample {
            speed: 10. * MPH,
            ..riding(1)
        };
        dirt.step(&slow, STEP);
        assert_eq!(dirt.level(), 12.);
        // MUD scenery on a clean zone: one level each 0.1 s.
        let mud = Sample {
            mud: [true, false, false, false],
            ..riding(0)
        };
        for _ in 0..60 {
            dirt.step(&mud, STEP);
        }
        assert_eq!(dirt.level(), 22.);
        // Two wheels in water wash 31.875 levels a second.
        let wet = Sample {
            water_heights: [2., 2., -256., -256.],
            ..riding(1)
        };
        for _ in 0..30 {
            dirt.step(&wet, STEP);
        }
        assert!((dirt.level() - (22. - 31.875 / 2.)).abs() < 1e-9);
        for _ in 0..30 {
            dirt.step(&wet, STEP);
        }
        assert_eq!(dirt.level(), 0.);
        // Heavy: the dusty zone soils at 0.2 s, three times as fast.
        dirt.reset();
        dirt.set_heavy(true);
        for _ in 0..60 {
            dirt.step(&riding(2), STEP);
        }
        assert_eq!(dirt.level(), 15.);
        dirt.splat(300.);
        assert_eq!(dirt.amount(), 1.);
    }
}
