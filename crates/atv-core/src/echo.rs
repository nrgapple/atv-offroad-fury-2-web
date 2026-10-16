//! Authored "Enhanced" sound option: one shared echo send per venue type.
//! Not original behavior; the original SPU2 reverb setup is unrecovered.
use crate::{Result, wheel::Vec3};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Echo {
    Open,
    Stadium,
    Canyon,
}
/// Fraction of sixteen headings where the terrain 150ft and 300ft away rises
/// at least 35ft above the ground below `origin`: walls on most sides read as
/// an enclosed canyon or bowl.
pub fn enclosure(height: impl Fn(f32, f32) -> Result<Option<f32>>, origin: Vec3) -> Result<f32> {
    let Some(ground) = height(origin[0], origin[2])? else {
        return Ok(0.);
    };
    let mut walls = 0;
    for i in 0..16 {
        let angle = i as f32 * std::f32::consts::TAU / 16.;
        let (s, c) = angle.sin_cos();
        let mut rise = f32::MIN;
        for radius in [150., 300.] {
            if let Some(h) = height(origin[0] + s * radius, origin[2] + c * radius)? {
                rise = rise.max(h - ground);
            }
        }
        if rise >= 35. {
            walls += 1;
        }
    }
    Ok(walls as f32 / 16.)
}
/// Stadium venues come from the original data (CrowdPresent, the indoor
/// arena, crowd stunt parks); otherwise at least 10 of 16 walls is a canyon.
pub fn classify(stadium: bool, enclosure: f32) -> Echo {
    if stadium {
        Echo::Stadium
    } else if enclosure >= 10. / 16. {
        Echo::Canyon
    } else {
        Echo::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bowl_reads_as_canyon_and_plain_as_open() {
        let bowl = |x: f32, z: f32| Ok(Some(((x * x + z * z).sqrt() / 4.).min(80.)));
        let e = enclosure(bowl, [0.; 3]).unwrap();
        assert_eq!(e, 1.);
        assert_eq!(classify(false, e), Echo::Canyon);
        let plain = |_: f32, _: f32| Ok(Some(5.));
        assert_eq!(
            classify(false, enclosure(plain, [0.; 3]).unwrap()),
            Echo::Open
        );
        assert_eq!(classify(true, 0.), Echo::Stadium);
        let off = |_: f32, _: f32| Ok(None);
        assert_eq!(enclosure(off, [0.; 3]).unwrap(), 0.);
    }
}
