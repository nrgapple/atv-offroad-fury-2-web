//! Alpha landing burst (BurstEmitter in web/hype-gl.js): an authored ring of
//! dust puffs plus thrown dirt clods at a hard landing, scaled by its
//! severity (ft/s) and tinted by the terrain under it. Presentation only.
use atv_core::math::Det;

/// x, y, z, birth, vx, vy, vz, life, size, kind, r, g, b per particle.
pub const FLOATS: usize = 13;

/// Deterministic jitter so tests and replays see the same burst.
fn hash(n: f64) -> f64 {
    let x = (n * 12.9898).sin_d() * 43758.5453;
    x - x.floor()
}

pub struct BurstEmitter {
    pub capacity: usize,
    pub data: Vec<f32>,
    next: usize,
    pub live: usize,
    pub emitted: u64,
    bursts: u32,
}

impl BurstEmitter {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            data: vec![0.; capacity * FLOATS],
            next: 0,
            live: 0,
            emitted: 0,
            bursts: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.capacity);
    }

    fn push(&mut self, values: [f64; FLOATS]) {
        let at = self.next * FLOATS;
        for (slot, v) in self.data[at..at + FLOATS].iter_mut().zip(values) {
            *slot = v as f32;
        }
        self.next = (self.next + 1) % self.capacity;
        self.live = (self.live + 1).min(self.capacity);
        self.emitted += 1;
    }

    /// The particles one landing adds (0 below 10 ft/s).
    pub fn burst(
        &mut self,
        position: [f32; 3],
        severity: f32,
        time: f64,
        tint: [f32; 3],
        heading: [f32; 2],
    ) -> usize {
        let power = ((f64::from(severity) - 10.) / 45.).clamp(0., 1.);
        if power <= 0. || self.capacity == 0 {
            return 0;
        }
        let seed = f64::from(self.bursts) * 7.31;
        self.bursts += 1;
        let [x, y, z] = position.map(f64::from);
        let [r, g, b] = tint.map(f64::from);
        let [hx, hz] = heading.map(f64::from);
        let puffs = (12. + 14. * power).round() as usize;
        let clods = (8. + 18. * power).round() as usize;
        let tau = std::f64::consts::TAU;
        for i in 0..puffs {
            let n = i as f64;
            let angle = n / puffs as f64 * tau + hash(seed + n) * 0.5;
            let speed = (9. + 18. * power) * (0.7 + 0.5 * hash(seed + n * 3.1));
            let (s, c) = angle.sin_cos_d();
            self.push([
                x,
                y + 0.4,
                z,
                time,
                c * speed + hx * 0.25,
                1.5 + 3. * hash(seed + n * 5.7),
                s * speed + hz * 0.25,
                1.2 + 0.9 * power + 0.5 * hash(seed + n * 2.3),
                1.1 + 1.2 * power,
                0.,
                r,
                g,
                b,
            ]);
        }
        for i in 0..clods {
            let n = i as f64;
            let angle = hash(seed + n * 9.1) * tau;
            let speed = (5. + 13. * power) * (0.5 + hash(seed + n * 4.4));
            let (s, c) = angle.sin_cos_d();
            self.push([
                x,
                y + 0.3,
                z,
                time,
                c * speed + hx * 0.35,
                9. + 16. * power * hash(seed + n * 6.6),
                s * speed + hz * 0.35,
                0.7 + 0.5 * hash(seed + n * 8.8),
                0.22 + 0.3 * hash(seed + n * 1.9),
                1.,
                r * 0.55,
                g * 0.5,
                b * 0.45,
            ]);
        }
        puffs + clods
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Values from web/hype-gl.js BurstEmitter on the same bursts (float32).
    #[test]
    fn follows_the_browser() {
        let mut e = BurstEmitter::new(192);
        assert_eq!(e.burst([1., 2., 3.], 9., 0., [0.5; 3], [0., 0.]), 0);
        assert_eq!(
            e.burst([1., 2., 3.], 40., 5., [0.6, 0.5, 0.4], [4., -2.]),
            41
        );
        assert_eq!(
            e.burst([1., 2., 3.], 70., 6., [0.6, 0.5, 0.4], [0., 0.]),
            52
        );
        assert_eq!(e.live, 93);
        let expected: [[f32; FLOATS]; 2] = [EXPECTED_0, EXPECTED_30];
        for (row, want) in [0usize, 30].into_iter().zip(expected) {
            let got = &e.data[row * FLOATS..(row + 1) * FLOATS];
            for (a, b) in got.iter().zip(want) {
                assert!(
                    (a - b).abs() <= 1e-4 * b.abs().max(1.),
                    "row {row}: {got:?} vs {want:?}"
                );
            }
        }
    }

    const EXPECTED_0: [f32; FLOATS] = [
        1., 2.4, 3., 5., 15.7, 1.5, -0.5, 1.8, 1.9, 0., 0.6, 0.5, 0.4,
    ];
    const EXPECTED_30: [f32; FLOATS] = [
        1.,
        2.3,
        3.,
        5.,
        19.212257,
        9.596178,
        -4.807996,
        0.917_634_1,
        0.271_064_4,
        1.,
        0.33,
        0.25,
        0.18,
    ];
}
