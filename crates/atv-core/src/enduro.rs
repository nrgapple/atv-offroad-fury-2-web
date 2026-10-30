//! Enduro waypoint courses (EnduroRaceEvent, exact-build `0022a340`).
//!
//! An Enduro race is a sequential gate race: `<venue><ends|endl>E.wpt` lists
//! the gates in order (x, z and race direction in world feet). Every gate
//! but the last is a `<venue>endwgate` model; the last is the
//! `<venue>endfgate` finish gate. RaceStatus type 1 (`0024a490`) tests only
//! each rider's next gate; crossing the last one counts a lap. See
//! research/RACING.md "Enduro".
use crate::math::Det;
use crate::race::Gate;
use crate::{Result, slice, u32le};

/// The Waypoint Editor's record limit (`0022a240`); shipped files are padded
/// to it.
pub const MAX_WAYPOINTS: usize = 50;
/// `001f8410` Enduro grid: one row this far past the last gate.
pub const GRID_PAST_FINISH: f32 = 10.;
/// `0022a340` raises each gate model 2ft above the ground snap.
pub const MODEL_LIFT: f32 = 2.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waypoint {
    pub x: f32,
    pub z: f32,
    /// Race direction through the gate (unit length in the shipped files).
    pub direction: [f32; 2],
}

/// `.wpt`: 0x18 bytes of names, a u32 count at 0x18, then `count` records
/// of f32 x, z, dirx, dirz.
pub fn parse_wpt(bytes: &[u8]) -> Result<Vec<Waypoint>> {
    let count = u32le(bytes, 0x18)? as usize;
    if !(2..=MAX_WAYPOINTS).contains(&count) {
        return Err("invalid waypoint count".into());
    }
    let records = slice(bytes, 0x1c, count * 16)?;
    records
        .as_chunks::<16>()
        .0
        .iter()
        .map(|r| {
            let f = |i: usize| f32::from_le_bytes(r[i * 4..i * 4 + 4].try_into().unwrap());
            let (x, z, dx, dz) = (f(0), f(1), f(2), f(3));
            let length = (dx * dx + dz * dz).sqrt();
            if ![x, z].iter().all(|v| v.is_finite() && v.abs() < 1_000_000.)
                || !length.is_finite()
                || !(0.5..=2.).contains(&length)
            {
                return Err("invalid waypoint".into());
            }
            Ok(Waypoint {
                x,
                z,
                direction: [dx / length, dz / length],
            })
        })
        .collect()
}

/// The race box of each gate (`00248600`, flags 5): a line `half_width`
/// either side of the waypoint across its direction, at the ground height.
pub fn gates(
    waypoints: &[Waypoint],
    half_width: f32,
    ground: impl Fn(f32, f32) -> f32,
) -> Result<Vec<Gate>> {
    if !(half_width.is_finite() && (1. ..=200.).contains(&half_width)) {
        return Err("invalid Enduro gate width".into());
    }
    waypoints
        .iter()
        .map(|w| {
            let [dx, dz] = w.direction;
            let y = ground(w.x, w.z);
            if !y.is_finite() {
                return Err("invalid Enduro gate height".into());
            }
            Ok(Gate {
                center: [w.x, y, w.z],
                forward: [dx, 0., dz],
                facing: [dx, 0., dz],
                half_width: [dz * half_width, 0., -dx * half_width],
                width: 2. * half_width,
            })
        })
        .collect()
}

/// Gate model world matrix (`0022a340`): right (dz, 0, −dx), up, forward
/// (dx, 0, dz), at the waypoint raised [`MODEL_LIFT`].
pub fn model_matrix(gate: &Gate) -> [f32; 16] {
    let [dx, _, dz] = gate.forward;
    let [x, y, z] = gate.center;
    [
        dz,
        0.,
        -dx,
        0.,
        0.,
        1.,
        0.,
        0.,
        dx,
        0.,
        dz,
        0.,
        x,
        y + MODEL_LIFT,
        z,
        1.,
    ]
}

/// Gate banner textures (`00229e90`, `00229670`): the turn to the next gate
/// (the last gate points at gate 0), θ = atan2(D·right, D·forward) with
/// right (dz, 0, −dx), picks the front-face `<venue>arrowN` suffix and the
/// back face's (the same turn seen riding the other way).
pub fn arrow_textures(gates: &[Gate], index: usize) -> (&'static str, &'static str) {
    let gate = &gates[index];
    let next = &gates[(index + 1) % gates.len()];
    let d = [
        next.center[0] - gate.center[0],
        next.center[2] - gate.center[2],
    ];
    let [dx, _, dz] = gate.forward;
    let theta = (d[0] * dz - d[1] * dx).atan2_d(d[0] * dx + d[1] * dz);
    use std::f32::consts::FRAC_PI_8;
    const BINS: [f32; 4] = [FRAC_PI_8, 3. * FRAC_PI_8, 5. * FRAC_PI_8, 7. * FRAC_PI_8];
    let bin = BINS.iter().take_while(|&&b| theta.abs() > b).count();
    match (theta >= 0., bin) {
        (true, 0) => ("arrow1", "arrow5r"),
        (true, 1) => ("arrow2", "arrow6"),
        (true, 2) => ("arrow3", "arrow7"),
        (true, 3) => ("arrow4", "arrow8"),
        (true, _) => ("arrow5r", "arrow1"),
        (false, 0) => ("arrow1", "arrow5l"),
        (false, 1) => ("arrow8", "arrow4"),
        (false, 2) => ("arrow7", "arrow3"),
        (false, 3) => ("arrow6", "arrow2"),
        (false, _) => ("arrow5l", "arrow1"),
    }
}

/// `00228ea0`: every gate but the rider's next one draws at this alpha.
pub const FADED_GATE_ALPHA: f32 = 0.4;

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn wpt(points: &[[f32; 4]]) -> Vec<u8> {
        let mut out = b"stu0101.wpt\0".to_vec();
        out.resize(0x18, 0);
        out.extend((points.len() as u32).to_le_bytes());
        for p in points {
            for v in p {
                out.extend(v.to_le_bytes());
            }
        }
        out.resize(0x1c + MAX_WAYPOINTS * 16, 0);
        out
    }

    #[test]
    fn parses_padded_waypoint_files() {
        let bytes = wpt(&[
            [10., 20., 0., 1.],
            [30., 40., 2f32.sqrt() / 2., 2f32.sqrt() / 2.],
        ]);
        assert_eq!(bytes.len(), 828);
        let w = parse_wpt(&bytes).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].x, w[0].z, w[0].direction), (10., 20., [0., 1.]));
        let g = gates(&w, 15., |_, _| 5.).unwrap();
        assert_eq!(g[0].center, [10., 5., 20.]);
        assert_eq!(g[0].half_width, [15., 0., -0.]);
        assert_eq!(model_matrix(&g[0])[12..15], [10., 7., 20.]);
        assert!(parse_wpt(&bytes[..0x1c + 20]).is_err());
        assert!(parse_wpt(&wpt(&[[0., 0., 0., 1.]])).is_err());
        assert!(parse_wpt(&wpt(&[[0., 0., 0., 0.], [1., 1., 0., 1.]])).is_err());
        assert!(gates(&w, 0., |_, _| 0.).is_err());
    }

    #[test]
    fn banners_point_at_the_next_gate() {
        let w = |x: f32, z: f32| Waypoint {
            x,
            z,
            direction: [0., 1.],
        };
        // Heading +z: right is +x (right = (dz, 0, −dx)).
        let at = |x: f32, z: f32| {
            let g = gates(&[w(0., 0.), w(x, z)], 10., |_, _| 0.).unwrap();
            arrow_textures(&g, 0)
        };
        assert_eq!(at(0., 100.), ("arrow1", "arrow5r"));
        assert_eq!(at(100., 100.), ("arrow2", "arrow6"));
        assert_eq!(at(100., 0.), ("arrow3", "arrow7"));
        assert_eq!(at(100., -100.), ("arrow4", "arrow8"));
        assert_eq!(at(1., -100.), ("arrow5r", "arrow1"));
        assert_eq!(at(-100., 100.), ("arrow8", "arrow4"));
        assert_eq!(at(-100., 0.), ("arrow7", "arrow3"));
        assert_eq!(at(-1., -100.), ("arrow5l", "arrow1"));
        assert_eq!(at(-1., 100.), ("arrow1", "arrow5l"));
    }
}
