//! Source water initialization and signature animation (00198ca0,0030acb0,
//! 00197410,0030af00,00197f10,00188680). Runtime bytes remain caller supplied.
use crate::{
    Result,
    physics_data::PhysicsData,
    slice, u32le,
    water_surface::{self, Config, SignatureWave},
    wheel::Vec3,
};

#[derive(Debug, Clone)]
pub struct WaveDefinition {
    pub filename: String,
    pub position: [f32; 2],
    pub width: [f32; 2],
    pub velocity: [f32; 2],
    pub tiled: bool,
    pub frames: usize,
    pub frames_per_second: f32,
    pub scale: f32,
    pub height_channel: bool,
    pub bump_only: bool,
}
#[derive(Debug, Clone)]
pub struct Definition {
    pub surface: Config,
    pub world_width: f32,
    pub swell_height: f32,
    pub swell_period: f32,
    pub waves: Vec<WaveDefinition>,
}
impl Definition {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() > 1024 * 1024 {
            return Err("water text exceeds limit".into());
        }
        // WTTs use inline // annotations, in addition to the .phy grammar.
        let text = std::str::from_utf8(data).map_err(|_| "water text is not UTF-8")?;
        let cleaned = text
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let data = PhysicsData::parse(cleaned.as_bytes())?;
        let scalar = |section: &str, key: &str, default: f32| -> Result<f32> {
            match data
                .entries
                .iter()
                .find(|entry| entry.section == section && entry.key.eq_ignore_ascii_case(key))
            {
                Some(_) => data.first_scalar(section, key),
                None => Ok(default),
            }
        };
        let general = "General Information";
        let base = scalar(general, "WaterBaseHeight", 20.)?;
        let count = scalar(general, "NumSignatureWaves", 0.)?;
        if count.fract() != 0. || !(0. ..=64.).contains(&count) {
            return Err("unsupported water signature count".into());
        }
        let mut waves = Vec::new();
        for index in 1..=count as usize {
            let section = format!("Signature Wave {index}");
            let filename = data
                .first(&section, "filename")?
                .value
                .trim_matches('"')
                .to_owned();
            if filename.is_empty() || filename.contains(['/', '\\', ':']) || filename.contains("..")
            {
                return Err("invalid water wave filename".into());
            }
            let frames = scalar(&section, "numFrames", 0.)?;
            if frames.fract() != 0. || !(1. ..=512.).contains(&frames) {
                return Err("unsupported water wave frame count".into());
            }
            let width = [
                scalar(&section, "widthX", 0.)?,
                scalar(&section, "widthZ", 0.)?,
            ];
            if width.iter().any(|v| *v <= 0.) {
                return Err("invalid water signature width".into());
            }
            waves.push(WaveDefinition {
                filename,
                position: [
                    scalar(&section, "positionX", 0.)?,
                    scalar(&section, "positionZ", 0.)?,
                ],
                width,
                velocity: [
                    scalar(&section, "velocityX", 0.)?,
                    scalar(&section, "velocityZ", 0.)?,
                ],
                tiled: scalar(&section, "tile", 1.)? != 0.,
                frames: frames as usize,
                frames_per_second: scalar(&section, "fps", 0.)?,
                scale: scalar(&section, "scale", 0.)? * 16.,
                height_channel: scalar(&section, "channel", 0.)? == 0.,
                bump_only: scalar(&section, "BumpOnly", 0.)? != 0.,
            });
        }
        let result = Self {
            surface: Config {
                base_height: base,
                authored_base_height: base,
                ripple_scale: scalar(general, "WakeScale", 10.)? * 16.,
                displacement_scale: scalar(general, "WaterHeightRange", 40.)? * 16.,
                maximum_height_difference: scalar(general, "PoolDepth", 100.)?,
                ripples_enabled: true,
                infinite_plane: scalar(general, "RenderInfinitePlane", 1.)? != 0.,
            },
            world_width: scalar(general, "WorldWidthHeightInFeet", 2304.)?,
            swell_height: scalar(general, "WaterBaseSwellHeight", 0.)?,
            swell_period: scalar(general, "WaterBaseSwellPeriod", 5.)?,
            waves,
        };
        if ![
            result.surface.base_height,
            result.surface.ripple_scale,
            result.surface.displacement_scale,
            result.surface.maximum_height_difference,
            result.world_width,
            result.swell_height,
            result.swell_period,
        ]
        .iter()
        .all(|v| v.is_finite())
            || result.waves.iter().any(|wave| !wave.scale.is_finite())
        {
            return Err("water configuration overflow".into());
        }
        if result.world_width <= 0. || result.swell_period <= 0. {
            return Err("invalid water world width/swell period".into());
        }
        Ok(result)
    }
    /// Exact native filename pattern %s%04dHeightNeg/HeightPos, plus .pst.
    /// `directory` is the resource's authored Sigs directory, e.g. tracks/env04/Sigs.
    pub fn required_height_resources(&self, directory: &str) -> Vec<String> {
        let mut result = Vec::new();
        for wave in self
            .waves
            .iter()
            .filter(|wave| wave.height_channel && !wave.bump_only)
        {
            for frame in 0..wave.frames {
                for channel in ["HeightNeg", "HeightPos"] {
                    let name = format!(
                        "{}/{}{:04}{channel}.pst",
                        directory.trim_end_matches(['/', '\\']),
                        wave.filename,
                        frame
                    );
                    if !result.contains(&name) {
                        result.push(name);
                    }
                }
            }
        }
        result
    }
}

/// 001ab180's binary32 sine polynomial, valid for the caller's [-pi,pi] range.
pub fn height_sine(angle: f32) -> f32 {
    let squared = angle * angle;
    let mut p = f32::from_bits(0xab57_3fa0);
    for coefficient in [
        0x2f30_9231,
        0xb2d7_322b,
        0x3638_ef1d,
        0xb950_0d01,
        0x3c08_8889,
        0xbe2a_aaab,
    ] {
        p = f32::from_bits(coefficient) + squared * p;
    }
    angle * (1. + squared * p)
}
/// Base-swell elapsed time and height are independent of the signature clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SwellState {
    pub elapsed: f32,
}
impl SwellState {
    pub fn advance(
        &mut self,
        authored_base: f32,
        amplitude: f32,
        period: f32,
        dt: f32,
    ) -> Result<f32> {
        if ![self.elapsed, authored_base, amplitude, period, dt]
            .iter()
            .all(|v| v.is_finite())
            || period <= 0.
            || dt < 0.
        {
            return Err("invalid water swell state".into());
        }
        self.elapsed += dt;
        let tau = f32::from_bits(0x40c9_0fdb);
        let pi = f32::from_bits(0x4049_0fdb);
        let mut angle = (self.elapsed / period) % tau;
        if angle > pi {
            angle -= tau;
        } else if angle < -pi {
            angle += tau;
        }
        let height = authored_base + amplitude * height_sine(angle);
        if !height.is_finite() {
            return Err("water swell overflow".into());
        }
        Ok(height)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WaveState {
    pub elapsed: f32,
    pub scroll: [f32; 2],
    pub velocity: [f32; 2],
    pub first_frame: usize,
    pub second_frame: usize,
    pub first_frame_weight: f32,
    /// Published only during the native render-preparation phase 00197f10.
    pub sampled_origin: [f32; 2],
}
impl WaveState {
    /// Constructor starts dimension=1; 00197410 divides its rates by loaded width.
    pub fn new(definition: &WaveDefinition, dimension: usize) -> Result<Self> {
        if dimension == 0
            || dimension > 4096
            || definition.frames == 0
            || definition.frames > 512
            || !definition.frames_per_second.is_finite()
            || definition.frames_per_second < 0.
            || definition.width.iter().any(|v| !v.is_finite() || *v <= 0.)
            || definition
                .velocity
                .iter()
                .chain(definition.position.iter())
                .any(|v| !v.is_finite())
        {
            return Err("invalid water signature animation".into());
        }
        Ok(Self {
            elapsed: 0.,
            scroll: [0.; 2],
            velocity: std::array::from_fn(|i| {
                (definition.velocity[i] * definition.width[i]) / dimension as f32
            }),
            first_frame: 0,
            second_frame: 0,
            first_frame_weight: 1.,
            sampled_origin: [0.; 2],
        })
    }
    /// 0030af00's clock and signed fmod scrolling. Source type3 channels are
    /// skipped when their external feature gate is off; callers provide that gate.
    pub fn advance(
        &mut self,
        width: [f32; 2],
        fps: f32,
        frames: usize,
        dt: f32,
        enabled: bool,
    ) -> Result<()> {
        if ![self.elapsed, self.first_frame_weight, fps, dt]
            .iter()
            .chain(width.iter())
            .chain(self.scroll.iter())
            .chain(self.velocity.iter())
            .all(|v| v.is_finite())
            || dt < 0.
            || fps < 0.
            || frames == 0
            || frames > 512
            || width.iter().any(|v| *v <= 0.)
        {
            return Err("invalid water signature update".into());
        }
        if !enabled {
            return Ok(());
        }
        self.elapsed += dt;
        if fps == 0. {
            self.first_frame = 0;
            self.second_frame = 0;
            self.first_frame_weight = 1.;
        } else {
            let frame = self.elapsed * fps;
            if !frame.is_finite() || frame > i32::MAX as f32 {
                return Err("water signature clock overflow".into());
            }
            let whole = frame as i32;
            self.first_frame_weight = 1. - (frame - whole as f32);
            self.first_frame = (whole as u32 % frames as u32) as usize;
            self.second_frame = (self.first_frame + 1) % frames;
        }
        for (i, extent) in width.into_iter().enumerate() {
            self.scroll[i] = (self.scroll[i] + self.velocity[i] * dt) % extent;
        }
        if !self.elapsed.is_finite() || self.scroll.iter().any(|v| !v.is_finite()) {
            return Err("water signature state overflow".into());
        }
        Ok(())
    }
    pub fn prepare_surface(&mut self, position: [f32; 2]) -> Result<()> {
        if position
            .iter()
            .chain(self.scroll.iter())
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite water signature origin".into());
        }
        self.sampled_origin = std::array::from_fn(|i| position[i] + self.scroll[i]);
        if self.sampled_origin.iter().any(|v| !v.is_finite()) {
            return Err("water signature origin overflow".into());
        }
        Ok(())
    }
}

/// Raw palette indexes of the largest native eight-bit PST level. Physics height
/// reads these bytes; the CSM1 display palette must not be applied.
pub fn decode_height_pst(data: &[u8]) -> Result<(usize, Vec<u8>)> {
    if data.len() > 128 * 1024 * 1024 || u32le(data, 0)? != 1 || u32le(data, 4)? != 1 {
        return Err("unsupported water height PST".into());
    }
    let levels = u32le(data, 8)?;
    if levels == 0 || levels > 16 {
        return Err("invalid water height PST levels".into());
    }
    let mut at = 12 + 1024;
    let mut result = None;
    for _ in 0..levels {
        let h = u32le(data, at)? as usize;
        let w = u32le(data, at + 4)? as usize;
        if h == 0 || w != h || w > 4096 || u32le(data, at + 8)? != 0 || u32le(data, at + 12)? != 0 {
            return Err("unsupported water height PST dimensions".into());
        }
        at += 16;
        let bytes = slice(data, at, w * h)?;
        if result.as_ref().is_none_or(|(dimension, _)| w > *dimension) {
            result = Some((w, bytes.to_vec()));
        }
        at += w * h;
    }
    if at != data.len() {
        return Err("water height PST payload extent mismatch".into());
    }
    result.ok_or_else(|| "missing water height pixels".into())
}

/// Live surface for the connected scenes' one-frame, zero-wake/displacement
/// profiles. Other authored profiles fail rather than silently losing their field.
#[derive(Debug, Clone)]
pub struct LiveField {
    pub definition: Definition,
    pub swell: SwellState,
    pub states: Vec<WaveState>,
    pub waves: Vec<SignatureWave>,
}
impl LiveField {
    pub fn new(
        definition: Definition,
        directory: &str,
        mut load: impl FnMut(&str) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        if definition.surface.ripple_scale != 0. || definition.surface.displacement_scale != 0. {
            return Err("live water profile requires ripple/base-map integration".into());
        }
        let mut states = Vec::new();
        let mut waves = Vec::new();
        for def in definition
            .waves
            .iter()
            .filter(|wave| wave.height_channel && !wave.bump_only)
        {
            if def.frames != 1 || def.frames_per_second != 0. {
                return Err("live water profile requires animated frame-blend integration".into());
            }
            for (channel, positive) in [("HeightNeg", false), ("HeightPos", true)] {
                let path = format!(
                    "{}/{}0000{channel}.pst",
                    directory.trim_end_matches(['/', '\\']),
                    def.filename
                );
                let (dimension, pixels) = decode_height_pst(&load(&path)?)?;
                let state = WaveState::new(def, dimension)?;
                waves.push(SignatureWave {
                    origin: state.sampled_origin,
                    width: def.width,
                    inverse_width: def.width.map(|v| 1. / v),
                    dimension,
                    row_stride: dimension,
                    pixel_stride: 1,
                    tiled: def.tiled,
                    height_channel: true,
                    positive,
                    scale: def.scale,
                    first_frame: pixels.clone(),
                    second_frame: pixels,
                });
                states.push(state);
            }
        }
        if waves.is_empty() {
            return Err("live water profile has no height signatures".into());
        }
        Ok(Self {
            definition,
            swell: SwellState::default(),
            states,
            waves,
        })
    }
    /// Runs native update-stage clocks. The prepared signature origins remain
    /// from the last completed render-preparation stage until prepare_surface().
    pub fn advance(&mut self, dt: f32) -> Result<()> {
        self.definition.surface.base_height = self.swell.advance(
            self.definition.surface.authored_base_height,
            self.definition.swell_height,
            self.definition.swell_period,
            dt,
        )?;
        for (state, wave) in self.states.iter_mut().zip(&self.waves) {
            state.advance(wave.width, 0., 1, dt, true)?;
        }
        Ok(())
    }
    pub fn prepare_surface(&mut self) -> Result<()> {
        let definitions = self
            .definition
            .waves
            .iter()
            .filter(|wave| wave.height_channel && !wave.bump_only)
            .flat_map(|wave| [wave, wave]);
        for ((state, wave), def) in self.states.iter_mut().zip(&mut self.waves).zip(definitions) {
            state.prepare_surface(def.position)?;
            wave.origin = state.sampled_origin;
        }
        Ok(())
    }
    pub fn height(&self, point: Vec3, base_only: bool) -> Result<f32> {
        water_surface::height(
            self.definition.surface,
            point,
            base_only,
            &[],
            None,
            |point| water_surface::signature_height(&self.waves, [point[0], point[2]], 1., 0.),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_scroll_wraps_without_publishing_early() {
        let def = WaveDefinition {
            filename: "synthetic".into(),
            position: [5., 7.],
            width: [30., 30.],
            velocity: [-40., 3.],
            tiled: true,
            frames: 1,
            frames_per_second: 0.,
            scale: 16.,
            height_channel: true,
            bump_only: false,
        };
        let mut state = WaveState::new(&def, 128).unwrap();
        assert_eq!(state.velocity, [-9.375, 0.703125]);
        state.advance(def.width, 0., 1, 1., true).unwrap();
        assert_eq!(state.scroll, [-9.375, 0.703125]);
        assert_eq!(state.sampled_origin, [0.; 2]);
        state.prepare_surface(def.position).unwrap();
        assert_eq!(state.sampled_origin, [-4.375, 7.703125]);
    }
    #[test]
    fn no_height_bytes_are_optional_in_live_field() {
        let definition=Definition::parse(b"[General Information]\nWakeScale=0\nWaterHeightRange=0\nNumSignatureWaves=1\n[Signature Wave 1]\nfilename=\"sample\"\nwidthX=15\nwidthZ=15\nnumFrames=1\nscale=1 // source annotation\n").unwrap();
        assert_eq!(
            definition.required_height_resources("tracks/synthetic/Sigs"),
            [
                "tracks/synthetic/Sigs/sample0000HeightNeg.pst",
                "tracks/synthetic/Sigs/sample0000HeightPos.pst"
            ]
        );
        assert!(
            LiveField::new(definition, "tracks/synthetic/Sigs", |_| Err(
                "missing pixels".into()
            ))
            .is_err()
        );
    }
}
