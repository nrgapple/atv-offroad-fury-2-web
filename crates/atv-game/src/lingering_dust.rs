//! Enhanced/High lingering dust (DustEmitter in web/lingering-dust.js): an
//! authored, capped pool of slow puffs behind the rear wheels on terrain zones
//! whose original TOY sets GenerateDust, tinted by the terrain texture under
//! the wheel. Not part of the Original preset; the recovered wheel dust is
//! unchanged.
use atv_core::wheel_effects::Input;

pub const FLOATS: usize = 9;

pub struct DustEmitter {
    pub capacity: usize,
    /// x, y, z, birth, life, size, r, g, b per puff.
    pub data: Vec<f32>,
    next: usize,
    pub live: usize,
    timers: [f32; 2],
    pub emitted: u64,
}

impl DustEmitter {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            data: vec![0.; capacity * FLOATS],
            next: 0,
            live: 0,
            timers: [0.; 2],
            emitted: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.capacity);
    }

    /// One simulation frame. `zones` are GenerateDust flags per terrain zone;
    /// `tint(x, z)` is the terrain colour under a point (0..1).
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        input: &Input,
        radii: [f32; 4],
        speed: f32,
        riding: bool,
        zones: &[bool; 8],
        tint: impl Fn(f32, f32) -> [f32; 3],
    ) {
        if !riding || self.capacity == 0 {
            return;
        }
        let speed = speed.abs();
        // Fewer, sparser puffs on a smaller pool so short rides never exhaust it.
        let interval = (12. / self.capacity as f32).max(0.08);
        for side in 0..2 {
            let i = 2 + side;
            let center = input.centers[i];
            let water = input.water_heights[i].unwrap_or(-256.);
            let dry = input.grounded[i]
                && zones.get(input.surfaces[i]).copied().unwrap_or(false)
                && !input.mud[i]
                && water <= center[1] - radii[i];
            if !dry || speed < 15. {
                self.timers[side] = 0.;
                continue;
            }
            self.timers[side] += 1. / 60.;
            if self.timers[side] < interval {
                continue;
            }
            self.timers[side] -= interval;
            let color = tint(center[0], center[2]);
            let jitter = ((self.emitted.wrapping_mul(2_654_435_761)) % 1000) as f32 / 1000.;
            let puff = [
                center[0],
                center[1] - radii[i] * 0.4,
                center[2],
                input.time,
                2.8 + jitter * 1.6,
                2.2 + speed.min(80.) / 40. + jitter,
                color[0],
                color[1],
                color[2],
            ];
            self.data[self.next * FLOATS..][..FLOATS].copy_from_slice(&puff);
            self.next = (self.next + 1) % self.capacity;
            self.live = (self.live + 1).min(self.capacity);
            self.emitted += 1;
        }
    }
}

/// `[TerrainZoneN] GenerateDust=T` flags (parseDirtZones in web/graphics.js).
pub fn dust_zones(toy: &str) -> [bool; 8] {
    let mut zones = [false; 8];
    let mut zone: Option<usize> = None;
    for raw in toy.lines() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.starts_with('[') {
            zone = line
                .strip_prefix('[')
                .and_then(|l| l.strip_suffix(']'))
                .map(str::trim)
                .and_then(|l| {
                    l.get(..11)
                        .filter(|p| p.eq_ignore_ascii_case("TerrainZone"))
                        .map(|_| l[11..].trim())
                })
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| (1..=8).contains(n))
                .map(|n| n - 1);
            continue;
        }
        let (Some(zone), Some((key, value))) = (zone, line.split_once('=')) else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("GenerateDust") {
            match value.trim().chars().next().map(|c| c.to_ascii_uppercase()) {
                Some('T') => zones[zone] = true,
                Some('F') => zones[zone] = false,
                _ => {}
            }
        }
    }
    zones
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generate_dust_per_zone() {
        let zones = dust_zones(
            "[TerrainZone1]\nGenerateDust=T\n[TerrainZone3]\nGenerateDust = F ; x\nGenerateTireTrack=T\n[Other]\nGenerateDust=T\n[TerrainZone8]\ngeneratedust=t\n",
        );
        assert_eq!(
            zones,
            [true, false, false, false, false, false, false, true]
        );
    }
}
