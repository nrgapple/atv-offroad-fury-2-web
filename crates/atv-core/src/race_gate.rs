//! National start gate (exact-build `[DropGate]` loader `00222170`, setup
//! `00222cd0`, DropGateAction `002212d0`/`002212f0`, reset `00221370`).
//!
//! Only the eight National race TOYs declare a drop gate: its own soultree
//! (`SLTName`, not a scene model) placed at `Position` with orientation rows
//! X = Up × Look, Y = Up, Z = Look made perpendicular to Up (`00164dd0`). At
//! GO it turns about `RotationAxisLocal` through `RotationAngleDegrees`,
//! linearly over `RotationRateInSeconds`, about its origin (the bottom edge),
//! and plays `gate.vag`. Restart stands it up again. It has no collision.
use crate::Result;
use crate::math::Det;
use serde::Serialize;

type V = [f32; 3];
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: V) -> V {
    let l = dot(a, a).sqrt();
    if l > 0. { a.map(|v| v / l) } else { a }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DropGate {
    /// Model name without `.slt` (geometry/<model>.psg).
    pub model: String,
    pub position: V,
    /// Orientation rows X, Y, Z.
    pub basis: [V; 3],
    pub axis: V,
    pub angle: f32,
    pub seconds: f32,
}
pub fn parse(toy: &str) -> Result<Option<DropGate>> {
    let mut in_gate = false;
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in toy.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_gate = name.trim().eq_ignore_ascii_case("DropGate");
        } else if in_gate && let Some((k, v)) = line.split_once('=') {
            fields.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let get = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let Some(slt) = get("sltname") else {
        return Ok(None);
    };
    let model = slt
        .trim_end_matches(".slt")
        .trim_end_matches(".SLT")
        .to_ascii_lowercase();
    if model.is_empty() || model.contains(['/', '\\', ':', '.']) || model.len() > 64 {
        return Err("invalid drop gate model".into());
    }
    let vector = |key: &str| -> Result<V> {
        let v: Vec<f32> = get(key)
            .ok_or_else(|| format!("drop gate needs {key}"))?
            .split(',')
            .map(|x| x.trim().parse::<f32>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| format!("invalid drop gate {key}"))?;
        match v[..] {
            [x, y, z] if [x, y, z].iter().all(|v| v.is_finite()) => Ok([x, y, z]),
            _ => Err(format!("invalid drop gate {key}")),
        }
    };
    let number = |key: &str, default: f32| -> Result<f32> {
        get(key).map_or(Ok(default), |v| {
            v.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("invalid drop gate {key}"))
        })
    };
    let (look, up) = (vector("lookvector")?, normalize(vector("upvector")?));
    let z = normalize([
        look[0] - dot(look, up) * up[0],
        look[1] - dot(look, up) * up[1],
        look[2] - dot(look, up) * up[2],
    ]);
    Ok(Some(DropGate {
        model,
        position: vector("position")?,
        basis: [normalize(cross(up, z)), up, z],
        axis: normalize(vector("rotationaxislocal").unwrap_or([1., 0., 0.])),
        angle: number("rotationangledegrees", 45.)?.to_radians(),
        seconds: number("rotationrateinseconds", 1.)?.max(1e-3),
    }))
}
impl DropGate {
    /// Swing after GO: linear from 0 to the full angle over the rate.
    pub fn angle_at(&self, since_go: Option<f32>) -> f32 {
        since_go.map_or(0., |t| {
            (self.angle * t / self.seconds).clamp(0., self.angle)
        })
    }
    /// Column-major world matrix with the local rotation M' = R · M about
    /// the gate's own axis through its origin (`00165fc0`).
    pub fn matrix(&self, angle: f32) -> [f32; 16] {
        let [k0, k1, k2] = self.axis;
        // 00165fc0 with axis −X gives Y' = cos·Y − sin·Z, Z' = sin·Y + cos·Z:
        // the transposed (inverse-sense) Rodrigues matrix of the axis.
        let (s, c) = (-angle).sin_cos_d();
        let t = 1. - c;
        let r = [
            [t * k0 * k0 + c, t * k0 * k1 - s * k2, t * k0 * k2 + s * k1],
            [t * k0 * k1 + s * k2, t * k1 * k1 + c, t * k1 * k2 - s * k0],
            [t * k0 * k2 - s * k1, t * k1 * k2 + s * k0, t * k2 * k2 + c],
        ];
        let b = self.basis;
        let mut m = [0.; 16];
        for (row, local) in r.iter().enumerate() {
            // New row = Σ R[row][j] · basis[j] (rows are the world axes).
            let axis: V = std::array::from_fn(|i| (0..3).map(|j| local[j] * b[j][i]).sum());
            m[row * 4..row * 4 + 3].copy_from_slice(&axis);
        }
        m[12..15].copy_from_slice(&self.position);
        m[15] = 1.;
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TOY: &str = "[DropGate]\r\nSLTName=nat01DropGate.slt\r\nPosition=2594.599609,132.566513,3002.570557\r\nLookVector=-0.104517,0.707107,-0.699340\r\nUpVector=0.104517,0.707107,0.699340\r\nRotationAxisLocal=-1.0,0.0,0.0\r\nRotationAngleDegrees=40.0\r\nRotationRateInSeconds=0.1\r\n[ChampionshipInfo]\r\n";
    #[test]
    fn parses_the_national_gate_and_swings_it_flat() {
        let gate = parse(TOY).unwrap().unwrap();
        assert_eq!(gate.model, "nat01dropgate");
        assert!((gate.angle - 40f32.to_radians()).abs() < 1e-6);
        assert_eq!(gate.angle_at(None), 0.);
        assert!((gate.angle_at(Some(0.05)) - gate.angle / 2.).abs() < 1e-6);
        assert_eq!(gate.angle_at(Some(5.)), gate.angle);
        // Standing: the panel's up axis leans 45° from vertical.
        let up = |m: [f32; 16]| m[5];
        assert!((up(gate.matrix(0.)) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        // Dropped through 40°: the up axis is ~5° above horizontal.
        let dropped = gate.matrix(gate.angle);
        assert!(up(dropped).abs() < 0.12, "{}", up(dropped));
        assert_eq!(&dropped[12..15], &gate.position);
        // The panel's width axis is unchanged by its own X rotation.
        assert_eq!(&dropped[0..3], &gate.matrix(0.)[0..3]);
        assert!(parse("[ParticleEmitters]\r\n").unwrap().is_none());
    }
}
