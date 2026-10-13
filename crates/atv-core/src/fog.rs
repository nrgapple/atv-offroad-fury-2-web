//! Scene visibility/haziness and renderer fog parameters from 001ffda0,
//! 0012b2a0 and 00157c20. GS interpolation and clipping remain separate gates.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Fog {
    pub visibility: f32,
    pub haziness: f32,
    pub start: f32,
    pub end: f32,
    pub coefficient: f32,
    pub encoded_bias: f32,
}
impl Fog {
    pub fn new(visibility: f32, haziness: f32, far: f32, near: f32) -> Result<Self> {
        if ![visibility, haziness, far, near]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err("nonfinite original fog parameters".into());
        }
        let far = if far == -1. { 768. } else { far };
        let near = if near == -1. { 64. } else { near };
        // The original only caps haziness above one, preserving negative values.
        let haziness = haziness.min(1.);
        let end = near + visibility * (far - near);
        let start = (1. - haziness) * (end - 64.);
        let coefficient = if start == end {
            0.
        } else {
            255. / (start - end)
        };
        let encoded_bias = 524288. - end * coefficient;
        if end <= 0.
            || ![start, end, coefficient, encoded_bias]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err("unsupported original fog range".into());
        }
        Ok(Self {
            visibility,
            haziness,
            start,
            end,
            coefficient,
            encoded_bias,
        })
    }
    /// Compressed prelit VU packing: 524288 bias, sixteenth-unit float steps,
    /// followed by extraction of the GS eight-bit visibility component.
    pub fn coverage(&self, distance: f32) -> Result<u8> {
        if !distance.is_finite() || distance < 0. {
            return Err("invalid original fog distance".into());
        }
        let encoded = (self.encoded_bias + self.coefficient * distance).clamp(524288., 524543.);
        Ok(((encoded.to_bits() >> 4) & 255) as u8)
    }
}

pub fn parse(data: &PhysicsData) -> Result<Option<Fog>> {
    if !data
        .entries
        .iter()
        .any(|entry| entry.section.eq_ignore_ascii_case("Fog"))
    {
        return Ok(None);
    }
    let number = |key: &str, default: f32| -> Result<f32> {
        data.entries
            .iter()
            .find(|entry| {
                entry.section.eq_ignore_ascii_case("Fog") && entry.key.eq_ignore_ascii_case(key)
            })
            .map(|entry| {
                entry
                    .value
                    .parse()
                    .map_err(|_| format!("invalid original fog {key}"))
            })
            .transpose()
            .map(|v| v.unwrap_or(default))
    };
    Fog::new(
        number("Visibility", 1.)?,
        number("Haziness", 0.5)?,
        -1.,
        -1.,
    )
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_defaults_upper_only_haze_cap_and_zero_width() {
        let fog = Fog::new(1., 0.5, -1., -1.).unwrap();
        assert_eq!([fog.start, fog.end], [352., 768.]);
        assert_eq!(fog.coverage(fog.start).unwrap(), 255);
        assert_eq!(fog.coverage(fog.end).unwrap(), 0);
        assert_eq!(Fog::new(1., 1.2, -1., -1.).unwrap().start, 0.);
        assert_eq!(Fog::new(1., -0.5, -1., -1.).unwrap().start, 1056.);
        let degenerate = Fog::new(0.5, -1., 192., 64.).unwrap();
        assert_eq!(degenerate.start, degenerate.end);
        assert_eq!(degenerate.coverage(0.).unwrap(), 0);
        assert!(Fog::new(f32::NAN, 0.5, -1., -1.).is_err());
    }
}
