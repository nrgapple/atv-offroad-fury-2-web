//! Enhanced/High tire tracks (TrackBuilder in web/tire-tracks.js): a fixed
//! ring of terrain-draped quads behind each wheel, written per simulation
//! frame. Where they appear follows the original terrain-zone flags
//! (GenerateTireTrack zones rut; zones that dirty the quad press faintly);
//! what a wheel leaves follows its physics contact. Authored; presentation only.
use atv_core::math::Det;
use atv_core::wheel_effects::Input;

/// x, y, z, across, along, birth time, rut, scuff, wet.
pub const FLOATS: usize = 9;
pub const VERTICES: usize = 6;

/// Original `[TerrainZoneN]` presentation flags (parseDirtZones).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainZone {
    pub time: f32,
    pub amount: f32,
    pub tire_track: bool,
    pub dust: bool,
}

pub fn terrain_zones(toy: &str) -> [TerrainZone; 8] {
    let mut zones = [TerrainZone::default(); 8];
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
                .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| (1..=8).contains(n))
                .map(|n| n - 1);
            continue;
        }
        let (Some(zone), Some((key, value))) = (zone, line.split_once('=')) else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let flag = || match value.chars().next().map(|c| c.to_ascii_uppercase()) {
            Some('T') => Some(true),
            Some('F') => Some(false),
            _ => None,
        };
        if key.eq_ignore_ascii_case("GenerateTireTrack") {
            if let Some(v) = flag() {
                zones[zone].tire_track = v;
            }
        } else if key.eq_ignore_ascii_case("GenerateDust") {
            if let Some(v) = flag() {
                zones[zone].dust = v;
            }
        } else if key.eq_ignore_ascii_case("DirtAnimChangeTime")
            || key.eq_ignore_ascii_case("DirtAnimChangeAmount")
        {
            // /^-?[\d.]+$/ then Number(): reject anything else.
            let digits = value.strip_prefix('-').unwrap_or(value);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
                continue;
            }
            let Ok(number) = value.parse::<f32>() else {
                continue;
            };
            if key.eq_ignore_ascii_case("DirtAnimChangeTime") {
                zones[zone].time = number.max(0.);
            } else {
                zones[zone].amount = number;
            }
        }
    }
    zones
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// How soft the ground is: 1 mud ruts, .35 firm dirt/snow, 0 no marks.
pub fn track_strength(zone: Option<&TerrainZone>, mud: bool) -> f32 {
    if mud || zone.is_some_and(|z| z.tire_track) {
        return 1.;
    }
    if zone.is_some_and(|z| z.time > 0. && z.amount > 0.) {
        0.35
    } else {
        0.
    }
}

/// Per-wheel (rut, scuff) from the contact: speed deepens rolling ruts,
/// sideways scrub or spin/lockup tears wider scuffs.
pub fn wheel_mark(softness: f32, speed: f32, slip: [f32; 2]) -> (f32, f32) {
    if softness == 0. {
        return (0., 0.);
    }
    let scrub = smooth(3., 14., slip[0].abs());
    let spin = smooth(4., 18., slip[1].abs());
    let roll = softness
        * if softness >= 1. {
            0.35 + 0.65 * smooth(3., 25., speed)
        } else {
            0.45 * smooth(8., 40., speed)
        };
    (roll, scrub.max(spin) * (0.45 + 0.55 * softness))
}

#[derive(Clone, Copy)]
struct Wheel {
    x: f32,
    z: f32,
    along: f32,
    left: Option<[f32; 3]>,
    right: Option<[f32; 3]>,
    rut: f32,
    scuff: f32,
    peak_rut: f32,
    peak_scuff: f32,
}

pub struct TrackBuilder {
    pub capacity: usize,
    pub life: f32,
    pub data: Vec<f32>,
    next: usize,
    pub count: usize,
    wheels: [Option<Wheel>; 4],
    /// Segment range written since the last upload.
    pub dirty: Option<(usize, usize)>,
    pub total: u64,
}

/// What a frame contributes besides the op 75 snapshot.
pub struct Contact<'a> {
    pub radii: [f32; 4],
    pub slip: [[f32; 2]; 4],
    pub speed: f32,
    pub riding: bool,
    pub zones: &'a [TerrainZone; 8],
}

impl TrackBuilder {
    pub fn new(capacity: usize, life: f32) -> Self {
        Self {
            capacity,
            life,
            data: vec![0.; capacity * VERTICES * FLOATS],
            next: 0,
            count: 0,
            wheels: [None; 4],
            dirty: None,
            total: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.capacity, self.life);
        self.dirty = Some((0, self.capacity));
    }

    fn mark(&mut self, from: usize, to: usize) {
        self.dirty = Some(match self.dirty {
            Some((a, b)) => (a.min(from), b.max(to)),
            None => (from, to),
        });
    }

    /// One simulation frame; `height(x, z)` is the visible ground.
    pub fn step(&mut self, input: &Input, contact: &Contact, height: impl Fn(f32, f32) -> f32) {
        if !contact.riding {
            self.wheels = [None; 4];
            return;
        }
        let time = input.time;
        let speed = contact.speed.abs();
        for i in 0..4 {
            let center = input.centers[i];
            let radius = contact.radii[i];
            let wet = input.water_heights[i].is_some_and(|w| w > center[1] - radius);
            let softness = if input.grounded[i] && !wet && speed > 2. {
                track_strength(contact.zones.get(input.surfaces[i]), input.mud[i])
            } else {
                0.
            };
            if softness == 0. {
                self.wheels[i] = None;
                continue;
            }
            let (x, z) = (center[0], center[2]);
            let y = height(x, z);
            if !y.is_finite() {
                self.wheels[i] = None;
                continue;
            }
            let (rut, scuff) = wheel_mark(softness, speed, contact.slip[i]);
            let mud = if softness >= 1. { 1. } else { 0. };
            let fresh = |along| Wheel {
                x,
                z,
                along,
                left: None,
                right: None,
                rut: 0.,
                scuff: 0.,
                peak_rut: rut,
                peak_scuff: scuff,
            };
            let Some(mut last) = self.wheels[i] else {
                self.wheels[i] = Some(fresh(0.));
                continue;
            };
            // A short burst of slip between emitted segments still counts.
            last.peak_rut = last.peak_rut.max(rut);
            last.peak_scuff = last.peak_scuff.max(scuff);
            self.wheels[i] = Some(last);
            let (dx, dz) = (x - last.x, z - last.z);
            let d = dx.hypot_d(dz);
            if d > 4. {
                self.wheels[i] = Some(fresh(0.));
                continue;
            }
            if d < 0.6 {
                continue;
            }
            let (end_rut, end_scuff) = (last.peak_rut, last.peak_scuff);
            // Nothing worth drawing at either end: move the strip start along.
            if end_rut + end_scuff < 0.06 && last.rut + last.scuff < 0.06 {
                self.wheels[i] = Some(Wheel {
                    peak_rut: 0.,
                    peak_scuff: 0.,
                    ..fresh(last.along + d)
                });
                continue;
            }
            // Sliding smears a wider patch than the tread.
            let half = (radius * 0.45).max(0.25) * (1. + 0.8 * end_scuff);
            let (nx, nz) = (-dz / d * half, dx / d * half);
            let left = [x + nx, height(x + nx, z + nz) + 0.04, z + nz];
            let right = [x - nx, height(x - nx, z - nz) + 0.04, z - nz];
            let last_left = last.left.unwrap_or_else(|| {
                [
                    last.x + nx,
                    height(last.x + nx, last.z + nz) + 0.04,
                    last.z + nz,
                ]
            });
            let last_right = last.right.unwrap_or_else(|| {
                [
                    last.x - nx,
                    height(last.x - nx, last.z - nz) + 0.04,
                    last.z - nz,
                ]
            });
            if !left
                .iter()
                .chain(&right)
                .chain(&last_left)
                .chain(&last_right)
                .all(|v| v.is_finite())
            {
                self.wheels[i] = None;
                continue;
            }
            let along = last.along + d;
            let begin = |p: [f32; 3], across: f32| {
                [
                    p[0], p[1], p[2], across, last.along, time, last.rut, last.scuff, mud,
                ]
            };
            let end = |p: [f32; 3], across: f32| {
                [
                    p[0], p[1], p[2], across, along, time, end_rut, end_scuff, mud,
                ]
            };
            let quad = [
                begin(last_left, -1.),
                begin(last_right, 1.),
                end(left, -1.),
                begin(last_right, 1.),
                end(right, 1.),
                end(left, -1.),
            ];
            let at = self.next * VERTICES * FLOATS;
            for (k, vertex) in quad.iter().enumerate() {
                self.data[at + k * FLOATS..][..FLOATS].copy_from_slice(vertex);
            }
            self.mark(self.next, self.next + 1);
            self.next = (self.next + 1) % self.capacity;
            self.count = (self.count + 1).min(self.capacity);
            self.total += 1;
            self.wheels[i] = Some(Wheel {
                x,
                z,
                along,
                left: Some(left),
                right: Some(right),
                rut: end_rut,
                scuff: end_scuff,
                peak_rut: rut,
                peak_scuff: scuff,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_and_marks_follow_the_browser() {
        let zones = terrain_zones(
            "[TerrainZone1]\nGenerateTireTrack=T\n[TerrainZone2]\nDirtAnimChangeTime=2\nDirtAnimChangeAmount=0.5\nGenerateDust=T\n[TerrainZone3]\nDirtAnimChangeTime=x\n",
        );
        assert!(zones[0].tire_track && zones[1].dust && zones[1].time == 2.);
        assert_eq!(track_strength(Some(&zones[0]), false), 1.);
        assert_eq!(track_strength(Some(&zones[1]), false), 0.35);
        assert_eq!(track_strength(Some(&zones[2]), false), 0.);
        assert_eq!(track_strength(Some(&zones[2]), true), 1.);
        assert_eq!(wheel_mark(0., 30., [5., 5.]), (0., 0.));
        let (rut, scuff) = wheel_mark(1., 25., [14., 0.]);
        assert!((rut - 1.).abs() < 1e-6 && (scuff - 1.).abs() < 1e-6);
    }
}
