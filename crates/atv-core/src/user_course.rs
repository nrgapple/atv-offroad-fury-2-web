//! User Created Enduro courses (the original Waypoint Editor's races; see
//! research/RACING.md "User Created Enduro").
//!
//! A course is a venue, an ordered gate list (each a ground position and the
//! race direction through it) and a lap count. It travels as a compact code:
//! the saved list keeps codes and a course link carries one (`wp=`), so a
//! saved course and the course its link opens are the same bits. Positions
//! are kept on a half-foot grid and headings in 1/65536 turns, which every
//! platform decodes to identical `f32` values.
//!
//! Codes are untrusted input: [`UserCourse::decode`] checks the version,
//! venue, laps, gate count, name and spacing, and [`UserCourse::validate`]
//! the gates against the venue's terrain once it is loaded.
use crate::enduro::Waypoint;
use crate::math::Det;

/// Fewest gates a course may have (authored: the original's minimum was
/// not recovered).
pub const MIN_GATES: usize = 3;
/// Most gates: the Waypoint Editor's record limit (`0022a240`).
pub const MAX_GATES: usize = crate::enduro::MAX_WAYPOINTS;
/// Saved races per venue: the original Waypoint Editor page lists "Race 1
/// (New)" to "Race 3 (New)" (PAGE09.RES labels).
pub const SLOTS_PER_VENUE: usize = 3;
/// Laps a course may ask for, as Single Race offers (1-5); the original
/// Enduro default is 3 (`00289d50`).
pub const LAPS: std::ops::RangeInclusive<u32> = 1..=5;
pub const DEFAULT_LAPS: u32 = 3;
/// Longest name, in characters (authored).
pub const MAX_NAME: usize = 24;
/// Closest two consecutive gates may stand (authored: keeps a lap from
/// counting twice through overlapping gate boxes).
pub const MIN_SPACING: f32 = 20.;
/// The Waypoint Editor's venues: the eight Environments (it reads the long
/// Enduro archive, `0021d8b0`).
pub const VENUES: usize = 8;

const VERSION: u8 = 1;
/// Half-foot position grid.
const GRID: f32 = 2.;
const TURN: f32 = 65536.;

#[derive(Clone, Debug, PartialEq)]
pub struct UserCourse {
    /// `env01`..`env08`.
    pub environment: String,
    pub name: String,
    pub laps: u32,
    /// In race order; the last is the finish gate.
    pub gates: Vec<Waypoint>,
}

/// The heading (radians, 0 = +z, clockwise towards +x) of a direction.
fn heading_units(direction: [f32; 2]) -> u16 {
    let turns = direction[0].atan2_d(direction[1]) / std::f32::consts::TAU;
    ((turns.rem_euclid(1.) * TURN).round() as u32 % 65536) as u16
}

fn direction(units: u16) -> [f32; 2] {
    let angle = f32::from(units) / TURN * std::f32::consts::TAU;
    [angle.sin_d(), angle.cos_d()]
}

/// A gate snapped to the code's grid: what a course keeps and a link
/// carries. `None` for a position outside the grid's range.
pub fn snap(x: f32, z: f32, direction_xz: [f32; 2]) -> Option<Waypoint> {
    let cell = |v: f32| {
        let units = (v * GRID).round();
        (v.is_finite() && (0. ..=f32::from(u16::MAX)).contains(&units)).then_some(units as u16)
    };
    let length = direction_xz[0].hypot_d(direction_xz[1]);
    if !(length.is_finite() && length > 1e-3) {
        return None;
    }
    Some(gate(
        cell(x)?,
        cell(z)?,
        heading_units([direction_xz[0] / length, direction_xz[1] / length]),
    ))
}

fn gate(x: u16, z: u16, heading: u16) -> Waypoint {
    Waypoint {
        x: f32::from(x) / GRID,
        z: f32::from(z) / GRID,
        direction: direction(heading),
    }
}

/// The venue id for a venue number (1-based).
pub fn environment(number: usize) -> Option<String> {
    (1..=VENUES)
        .contains(&number)
        .then(|| format!("env{number:02}"))
}

fn venue_number(environment: &str) -> Option<u8> {
    let n: u8 = environment.strip_prefix("env")?.parse().ok()?;
    (environment.len() == 5 && (1..=VENUES as u8).contains(&n)).then_some(n)
}

/// A name made safe to show: printable characters, trimmed, at most
/// [`MAX_NAME`].
pub fn clean_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .to_string()
}

impl UserCourse {
    pub fn new(environment: &str) -> Self {
        Self {
            environment: environment.to_string(),
            name: String::new(),
            laps: DEFAULT_LAPS,
            gates: Vec::new(),
        }
    }

    /// Whether another gate may go at (x, z): room left and far enough from
    /// the last gate.
    pub fn can_place(&self, x: f32, z: f32) -> bool {
        self.gates.len() < MAX_GATES
            && self
                .gates
                .last()
                .is_none_or(|g| (g.x - x).hypot_d(g.z - z) >= MIN_SPACING)
    }

    /// Adds a gate snapped to the grid; false when it cannot go there.
    pub fn place(&mut self, x: f32, z: f32, direction_xz: [f32; 2]) -> bool {
        match snap(x, z, direction_xz) {
            Some(g) if self.can_place(g.x, g.z) => {
                self.gates.push(g);
                true
            }
            _ => false,
        }
    }

    pub fn undo(&mut self) -> Option<Waypoint> {
        self.gates.pop()
    }

    pub fn complete(&self) -> bool {
        self.gates.len() >= MIN_GATES
    }

    /// The compact code (URL-safe base64 without padding).
    pub fn encode(&self) -> String {
        let name = clean_name(&self.name);
        let mut bytes = Vec::with_capacity(5 + name.len() + self.gates.len() * 6);
        bytes.push(VERSION);
        bytes.push(venue_number(&self.environment).unwrap_or(0));
        bytes.push(self.laps.min(255) as u8);
        bytes.push(self.gates.len().min(255) as u8);
        bytes.push(name.len() as u8);
        bytes.extend(name.as_bytes());
        for g in &self.gates {
            let cell = |v: f32| (v * GRID).round().clamp(0., f32::from(u16::MAX)) as u16;
            for v in [cell(g.x), cell(g.z), heading_units(g.direction)] {
                bytes.extend(v.to_le_bytes());
            }
        }
        base64url(&bytes)
    }

    /// Reads a code, checking everything but the terrain.
    pub fn decode(code: &str) -> Result<Self, String> {
        // 5 header bytes, a name and 50 gates fit in 400 characters.
        if code.len() > 512 {
            return Err("The course code is too long.".into());
        }
        let bytes = from_base64url(code).ok_or("The course code is not valid.")?;
        let [version, venue, laps, count, name_len, rest @ ..] = bytes.as_slice() else {
            return Err("The course code is too short.".into());
        };
        if *version != VERSION {
            return Err("The course code is from another version.".into());
        }
        let environment = environment(usize::from(*venue)).ok_or("The course venue is unknown.")?;
        let laps = u32::from(*laps);
        if !LAPS.contains(&laps) {
            return Err("The course lap count is invalid.".into());
        }
        let count = usize::from(*count);
        if !(MIN_GATES..=MAX_GATES).contains(&count) {
            return Err(format!("A course needs {MIN_GATES} to {MAX_GATES} gates."));
        }
        let name_len = usize::from(*name_len);
        if rest.len() != name_len + count * 6 {
            return Err("The course code has the wrong length.".into());
        }
        let name =
            std::str::from_utf8(&rest[..name_len]).map_err(|_| "The course name is not valid.")?;
        if name.chars().count() > MAX_NAME || clean_name(name) != name {
            return Err("The course name is not valid.".into());
        }
        let gates = rest[name_len..]
            .as_chunks::<6>()
            .0
            .iter()
            .map(|r| {
                let v = |i: usize| u16::from_le_bytes([r[i], r[i + 1]]);
                gate(v(0), v(2), v(4))
            })
            .collect();
        let course = Self {
            environment,
            name: name.to_string(),
            laps,
            gates,
        };
        course.check_spacing()?;
        Ok(course)
    }

    fn check_spacing(&self) -> Result<(), String> {
        let close = self
            .gates
            .windows(2)
            .any(|w| (w[0].x - w[1].x).hypot_d(w[0].z - w[1].z) < MIN_SPACING);
        if close {
            return Err("Two course gates stand too close together.".into());
        }
        Ok(())
    }

    /// Checks the gates against the venue's terrain: every gate inside
    /// `[0, size_x] × [0, size_z]` with a finite ground height.
    pub fn validate(
        &self,
        size_x: f32,
        size_z: f32,
        ground: impl Fn(f32, f32) -> f32,
    ) -> Result<(), String> {
        if !(MIN_GATES..=MAX_GATES).contains(&self.gates.len()) {
            return Err(format!("A course needs {MIN_GATES} to {MAX_GATES} gates."));
        }
        for g in &self.gates {
            if !(0. ..=size_x).contains(&g.x)
                || !(0. ..=size_z).contains(&g.z)
                || !ground(g.x, g.z).is_finite()
            {
                return Err("A course gate lies outside the venue.".into());
            }
        }
        self.check_spacing()
    }

    /// The menu label: its name, or "Race n" in its venue slot.
    pub fn label(&self, slot: usize) -> String {
        if self.name.is_empty() {
            format!("Race {}", slot + 1)
        } else {
            self.name.clone()
        }
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn from_base64url(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let v = ALPHABET.iter().position(|&a| a == c)? as u32;
            n |= v << (18 - 6 * i);
        }
        for i in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn course() -> UserCourse {
        let mut c = UserCourse::new("env05");
        c.name = "Ridge Run".into();
        assert!(c.place(100.2, 200.7, [0., 1.]));
        assert!(c.place(300., 200., [1., 0.]));
        assert!(c.place(300., 450.25, [-0.6, -0.8]));
        c
    }

    #[test]
    fn codes_round_trip_exactly() {
        let c = course();
        assert_eq!(c.gates[0].x, 100.);
        assert_eq!(c.gates[0].z, 200.5);
        let code = c.encode();
        assert!(code.len() < 40, "{code}");
        assert!(code.bytes().all(|b| ALPHABET.contains(&b)));
        let back = UserCourse::decode(&code).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.encode(), code);
        let [dx, dz] = back.gates[2].direction;
        assert!((dx + 0.6).abs() < 1e-4 && (dz + 0.8).abs() < 1e-4);
        // Fifty gates stay well inside a URL.
        let mut long = UserCourse::new("env08");
        long.name = "x".repeat(MAX_NAME);
        for i in 0..MAX_GATES {
            assert!(long.place(50. + 30. * i as f32, 4000., [0., -1.]));
        }
        assert!(!long.place(9000., 9000., [0., 1.]));
        assert!(long.encode().len() <= 420);
        assert_eq!(UserCourse::decode(&long.encode()).unwrap(), long);
    }

    #[test]
    fn placement_keeps_gates_apart_and_undoes() {
        let mut c = course();
        assert!(!c.place(305., 455., [0., 1.]));
        assert!(!c.place(f32::NAN, 0., [0., 1.]));
        assert!(!c.place(-5., 0., [0., 1.]));
        assert!(!c.place(500., 500., [0., 0.]));
        assert_eq!(c.undo().map(|g| g.z), Some(450.));
        assert_eq!(c.gates.len(), 2);
        assert!(!c.complete());
    }

    #[test]
    fn decode_rejects_untrusted_codes() {
        let good = course().encode();
        let bytes = from_base64url(&good).unwrap();
        let with = |f: &dyn Fn(&mut Vec<u8>)| {
            let mut b = bytes.clone();
            f(&mut b);
            UserCourse::decode(&base64url(&b))
        };
        assert!(with(&|_| {}).is_ok());
        assert!(with(&|b| b[0] = 2).is_err());
        assert!(with(&|b| b[1] = 9).is_err());
        assert!(with(&|b| b[1] = 0).is_err());
        assert!(with(&|b| b[2] = 0).is_err());
        assert!(with(&|b| b[2] = 6).is_err());
        assert!(with(&|b| b[3] = 2).is_err());
        assert!(with(&|b| b[3] = 51).is_err());
        assert!(with(&|b| b[3] = 4).is_err());
        assert!(
            with(&|b| {
                b.pop();
            })
            .is_err()
        );
        assert!(with(&|b| b[5] = 0x07).is_err());
        assert!(with(&|b| b[5] = 0xff).is_err());
        // The third gate moved onto the second.
        assert!(
            with(&|b| {
                let n = b.len();
                b.copy_within(n - 12..n - 8, n - 6);
            })
            .is_err()
        );
        for bad in ["", "A", "!!!!", "AAAA", &"A".repeat(600)] {
            assert!(UserCourse::decode(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn validate_keeps_gates_on_the_terrain() {
        let c = course();
        assert!(c.validate(1000., 1000., |_, _| 0.).is_ok());
        assert!(c.validate(250., 1000., |_, _| 0.).is_err());
        assert!(c.validate(1000., 1000., |_, _| f32::NAN).is_err());
        let mut short = c.clone();
        short.gates.truncate(2);
        assert!(short.validate(1000., 1000., |_, _| 0.).is_err());
    }

    #[test]
    fn base64url_matches_the_standard_alphabet() {
        assert_eq!(base64url(b"Man"), "TWFu");
        assert_eq!(base64url(b"Ma"), "TWE");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(from_base64url("-_8").unwrap(), vec![0xfb, 0xff]);
        assert_eq!(from_base64url("TWFu").unwrap(), b"Man");
    }
}
