//! Original wheel dust, dirt chunks, spray and wet births (WheelEffectsRenderer
//! in web/wheel-effects.js, fed by atv-web's op 74/75): atv_core::wheel_effects
//! simulates the PRT/EMT emitters each 60 Hz step from the ride's wheel state.
//! A motorcycle's wheel, exhaust and wreck particles come from its own MX
//! emitters (atv_core::motorcycle_effects, stepped in atv-web's motorcycle
//! batch) and draw through the same renderer.
use crate::{
    Result,
    ambient::{gpu_atlas, image},
    materials::last_component,
    ride::Ride,
    venue::{Image, Venue},
};
use atv_content::Content;
use atv_core::{
    player_models,
    resource::Resource,
    wheel_effects::{AtlasFrame, Config, Effects, Frame},
};

/// The emitters behind the drawn particles.
pub enum Source {
    /// The ATV's op 74/75 wheel effects.
    Quad(Box<Effects>),
    /// The motorcycle's MX emitters; `epoch` is the bike's reset count the
    /// particles belong to (a recovery restarts them).
    Bike {
        effects: atv_core::motorcycle_effects::Effects,
        epoch: u32,
    },
}

pub struct WheelEffects {
    pub source: Source,
    /// Atlas frame rectangles and the wet tint the renderer reads.
    pub frames: Vec<AtlasFrame>,
    pub light_color: [f32; 3],
    /// Particles.prt atlas (dust, chunks, spray).
    pub atlas: Image,
    /// Wet-particle GPU atlas (particle_gpu TextureMap_1, 128x64 or 128x256).
    pub wet_atlas: Image,
    /// atv-web's effects clock: summed step time, step, tick; reset on load.
    time: f32,
    tick: u64,
    frame: Frame,
    /// Wet births of every step since the renderer last took them (the
    /// browser accepts each frame of a batch).
    wet_pending: Vec<atv_core::wheel_effects::WetBirth>,
    /// Enhanced/High lingering dust and what it reads from the venue.
    pub dust: Option<LingeringDust>,
    /// Enhanced/High tire tracks and the venue's terrain zones.
    pub tracks: Option<(
        crate::tire_tracks::TrackBuilder,
        [crate::tire_tracks::TerrainZone; 8],
        atv_core::terrain::Heightfield,
    )>,
}

pub struct LingeringDust {
    pub emitter: crate::lingering_dust::DustEmitter,
    zones: [bool; 8],
    tint: DustTint,
}

/// dustTint in game.js: the mean colour of the terrain layer under a point.
#[derive(Clone, Debug)]
pub struct DustTint {
    /// Mean colour per terrain layer and the 64-cell layer mapping.
    colors: Vec<[f32; 3]>,
    mapping: Vec<u16>,
    mapping_size: [usize; 2],
    tile: f32,
}

impl DustTint {
    pub fn new(venue: &Venue) -> Self {
        let t = &venue.terrain;
        // uploadTerrainTextures: every 16th texel of each layer, averaged.
        let colors = t
            .layers
            .iter()
            .map(|image| {
                let (mut sum, mut n) = ([0f64; 3], 0f64);
                for pixel in image.rgba.as_chunks::<64>().0 {
                    for c in 0..3 {
                        sum[c] += f64::from(pixel[c]);
                    }
                    n += 1.;
                }
                sum.map(|v| (v / (n * 255.)) as f32)
            })
            .collect();
        Self {
            colors,
            mapping: t.mapping.clone(),
            mapping_size: [t.mapping_width as usize, t.mapping_height as usize],
            tile: t.spacing * 64.,
        }
    }

    pub fn at(&self, x: f32, z: f32) -> [f32; 3] {
        tint(
            &self.colors,
            &self.mapping,
            self.mapping_size,
            self.tile,
            x,
            z,
        )
    }
}

/// The terrain layer's mean colour under a point (dustTint in game.js).
fn tint(
    colors: &[[f32; 3]],
    mapping: &[u16],
    [w, h]: [usize; 2],
    tile: f32,
    x: f32,
    z: f32,
) -> [f32; 3] {
    let tx = ((x / tile).floor().max(0.) as usize).min(w.saturating_sub(1));
    let tz = ((z / tile).floor().max(0.) as usize).min(h.saturating_sub(1));
    mapping
        .get(tz * w + tx)
        .and_then(|layer| colors.get(usize::from(*layer)))
        .map_or([0.62, 0.55, 0.45], |c| c.map(|v| v.min(1.)))
}

fn named(archive: &Resource, name: &str) -> Result<Vec<u8>> {
    let index = archive
        .members
        .iter()
        .position(|m| last_component(&m.name).eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("Original wheel effect resource is missing: {name}"))?;
    archive.extract(index)
}

/// loadWheelEffects + op 74 for the player's `quad` on a loaded venue.
/// None when the course has no Particles.prt.
pub async fn load(content: &Content, venue: &Venue, quad: u16) -> Result<Option<WheelEffects>> {
    let course = crate::launch::course_resource(&venue.environment, false)?;
    let course_bytes = content.file(&course.archive).await?;
    let archive = Resource::parse(&course_bytes)?;
    let Some(prt) = archive
        .members
        .iter()
        .position(|m| m.name.to_ascii_lowercase().ends_with("particles.prt"))
    else {
        return Ok(None);
    };
    let prt = archive.extract(prt)?;
    let models = player_models::selection(quad, 0)?;
    let quad_bytes = content.file(&models.quad).await?;
    let quad_archive = Resource::parse(&quad_bytes)?;
    let mut config = Config::parse(
        &prt,
        &named(&quad_archive, "quaddust.emt")?,
        &named(&quad_archive, "quaddirtchunk.emt")?,
        &named(&quad_archive, "quaddirtspray.emt")?,
    )?;
    // Op 74 tints wet particles by the scene's ambient plus half its sun.
    let light = |kind: &str| {
        venue
            .scene
            .lights
            .iter()
            .find(|l| l.kind == kind)
            .map_or([0.; 3], |l| l.color)
    };
    let (ambient, sun) = (light("Ambient"), light("Directional"));
    config.light_color = std::array::from_fn(|i| ambient[i] + sun[i] * 0.5);
    let stem = config.atlas.texture.to_ascii_lowercase();
    let stem = stem.strip_suffix(".tga").unwrap_or(&stem);
    let atlas = image(&archive, &format!("{stem}.pst"))?;
    if config
        .atlas
        .frames
        .iter()
        .any(|f| f.rect[2] > atlas.width as f32 || f.rect[3] > atlas.height as f32)
    {
        return Err("Original particle rectangle exceeds atlas.".into());
    }
    let wet_atlas = gpu_atlas(&archive)?;
    if wet_atlas.width != 128 || ![64, 256].contains(&wet_atlas.height) {
        return Err("Unsupported original wet particle atlas.".into());
    }
    Ok(Some(WheelEffects {
        frames: config.atlas.frames.clone(),
        light_color: config.light_color,
        source: Source::Quad(Box::new(Effects::new(config))),
        atlas,
        wet_atlas,
        time: 0.,
        tick: 0,
        frame: Frame::default(),
        wet_pending: Vec::new(),
        dust: None,
        tracks: None,
    }))
}

/// loadMotorcycleEffects: the riding motorcycle's particle atlas from its MX
/// archive (FS01particles.prt's TextureMap); its wet particles share it.
pub async fn load_motorcycle(content: &Content, ride: &Ride) -> Result<WheelEffects> {
    let bike = ride.motorcycle().ok_or("not a motorcycle")?;
    let config = &bike.model.effects_config;
    let models = player_models::selection(ride.selection().quad, 0)?;
    let bytes = content.file(&models.quad).await?;
    let archive = Resource::parse(&bytes)?;
    let stem = config.atlas.texture.to_ascii_lowercase();
    let stem = stem.strip_suffix(".tga").unwrap_or(&stem);
    let atlas = image(&archive, &format!("{stem}.pst"))?;
    if config
        .atlas
        .frames
        .iter()
        .any(|f| f.rect[2] > atlas.width as f32 || f.rect[3] > atlas.height as f32)
    {
        return Err("Motorcycle particle rectangle exceeds atlas.".into());
    }
    Ok(WheelEffects {
        source: Source::Bike {
            effects: Default::default(),
            epoch: bike.frame().reset_count,
        },
        frames: config.atlas.frames.clone(),
        light_color: config.light_color,
        wet_atlas: Image {
            width: atlas.width,
            height: atlas.height,
            rgba: atlas.rgba.clone(),
        },
        atlas,
        time: 0.,
        tick: 0,
        frame: Frame::default(),
        wet_pending: Vec::new(),
        dust: None,
        tracks: None,
    })
}

impl WheelEffects {
    /// Turns on tire tracks: `capacity` segments living `life` seconds
    /// (Enhanced 600/15, High 3600/75); 0 turns them off.
    pub fn set_tire_tracks(&mut self, capacity: usize, life: f32, venue: &Venue) {
        // The browser leaves motorcycles without tire tracks.
        if capacity == 0 || matches!(self.source, Source::Bike { .. }) {
            self.tracks = None;
            return;
        }
        let t = &venue.terrain;
        self.tracks = Some((
            crate::tire_tracks::TrackBuilder::new(capacity, life),
            venue.terrain_zones,
            atv_core::terrain::Heightfield {
                width: t.width,
                depth: t.depth,
                spacing: t
                    .spacing
                    .to_string()
                    .parse()
                    .unwrap_or(f64::from(t.spacing)),
                heights: t.heights.clone(),
            },
        ));
    }

    /// The terrain zones the tire tracks read (Alpha Heavy mud adjusts them,
    /// as game.js passes `dirtBuildup.zones`).
    pub fn set_track_zones(&mut self, zones: [crate::tire_tracks::TerrainZone; 8]) {
        if let Some((_, current, _)) = &mut self.tracks {
            *current = zones;
        }
    }

    /// Turns on lingering dust with `capacity` puffs (Enhanced 24, High 96);
    /// 0 turns it off.
    pub fn set_lingering_dust(&mut self, capacity: usize, venue: &Venue) {
        // The browser leaves motorcycles without lingering dust.
        if capacity == 0 || matches!(self.source, Source::Bike { .. }) {
            self.dust = None;
            return;
        }
        if self
            .dust
            .as_ref()
            .is_some_and(|d| d.emitter.capacity == capacity)
        {
            return;
        }
        self.dust = Some(LingeringDust {
            emitter: crate::lingering_dust::DustEmitter::new(capacity),
            zones: venue.dust_zones,
            tint: DustTint::new(venue),
        });
    }

    /// One simulation step after `ride.step` (simulation_frame then op 75).
    pub fn step(&mut self, ride: &Ride) -> Result<()> {
        self.time += 1. / 60.;
        self.tick = self.tick.wrapping_add(1);
        self.run(ride, 1. / 60.)
    }

    /// The frame a Reset returns (a new placement epoch with zero step time
    /// and the same tick), right after `ride.reset`.
    pub fn reset(&mut self, ride: &Ride) -> Result<()> {
        if let Some(dust) = &mut self.dust {
            dust.emitter.reset();
        }
        if let Some((tracks, ..)) = &mut self.tracks {
            tracks.reset();
        }
        self.run(ride, 0.)
    }

    fn run(&mut self, ride: &Ride, dt: f32) -> Result<()> {
        let effects = match &mut self.source {
            Source::Quad(effects) => effects,
            Source::Bike { effects, epoch } => {
                let bike = ride.motorcycle().ok_or("not a motorcycle")?;
                let f = bike.frame();
                // A placement or recovery restarts the particles; atv-web
                // then steps them once per motorcycle step.
                if dt == 0. || *epoch != f.reset_count {
                    *effects = Default::default();
                    *epoch = f.reset_count;
                }
                if dt > 0. {
                    effects.step(
                        &bike.model.effects_config,
                        f,
                        &bike.world(),
                        dt,
                        ride.terrain(),
                    )?;
                }
                self.frame = effects.frame(bike.time, f.reset_count);
                return Ok(());
            }
        };
        let mut input = ride.wheel_effect_input()?;
        input.time = self.time;
        input.dt = dt;
        input.tick = self.tick;
        self.frame = effects
            .step_with_terrain(&input, |point| Ok(ride.ground(point[0], point[2])))?
            .clone();
        self.wet_pending
            .extend(self.frame.wet_births.iter().cloned());
        // acceptFrame steps tire tracks, then lingering dust, every frame.
        if let Some((tracks, zones, ground)) = &mut self.tracks {
            let frame = ride.frame();
            let state = ride.wheel_effect_state().ok_or("tire tracks need an ATV")?;
            tracks.step(
                &input,
                &crate::tire_tracks::Contact {
                    radii: state.radii,
                    slip: state.slip,
                    speed: frame.speed_magnitude,
                    riding: frame.wreck.phase == atv_core::wreck::Phase::Riding,
                    zones,
                },
                |x, z| ground.height(f64::from(x), f64::from(z)) as f32,
            );
        }
        if let Some(dust) = &mut self.dust {
            let frame = ride.frame();
            let riding = frame.wreck.phase == atv_core::wreck::Phase::Riding;
            let LingeringDust {
                emitter,
                zones,
                tint,
            } = dust;
            let tint = |x: f32, z: f32| tint.at(x, z);
            emitter.step(
                &input,
                ride.wheel_radii(),
                frame.speed_magnitude,
                riding,
                zones,
                tint,
            );
        }
        Ok(())
    }

    /// Wet births since the last call, in birth order.
    pub fn take_wet_births(&mut self) -> Vec<atv_core::wheel_effects::WetBirth> {
        std::mem::take(&mut self.wet_pending)
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }
}
