//! Per-step sound inputs the simulation publishes for the audio layer: the
//! fields atv-web's simulation_frame adds to each frame for web/audio.js
//! (quad_audio, rider_audio_position, rider_wreck, applied_input).
use atv_core::{
    quad_sounds::{ChassisInput, DriveInput},
    spatial_audio::CameraPose,
    vehicle::{AudioEvents, VehicleFrame},
    wheel::Vec3,
    wreck::Phase,
};

/// `quad_frame` in crates/atv-web/src/quad_audio.rs (`001e5260` fields).
#[derive(Debug, Clone, Default)]
pub struct QuadFrame {
    pub events: AudioEvents,
    pub chassis: ChassisInput,
    pub drive: DriveInput,
    pub origin: Vec3,
    pub velocity: Vec3,
    /// Body origin minus the water surface (None without a water object).
    pub water_depth: Option<f32>,
    /// Front-left and front-right wheel world translations (side emitters).
    pub wheels: [Vec3; 2],
}

pub fn quad_frame(
    events: AudioEvents,
    frame: &VehicleFrame,
    surfaces: [usize; 4],
    ground: Option<f32>,
    water: Option<f32>,
    centers: &[Vec3],
) -> QuadFrame {
    let wrecked = frame.wreck.phase != Phase::Riding;
    // +994/+99c take the side's front wheel surface while grounded, 8 once
    // the quad is half a foot below the cached water surface, else -1.
    let terrain = std::array::from_fn(|side| {
        if !frame.grounded[side] {
            -1
        } else if water.is_some_and(|w| frame.origin[1] < w - 0.5) {
            8
        } else {
            surfaces[side] as i32
        }
    });
    QuadFrame {
        events,
        chassis: ChassisInput {
            impact: events.impact,
            wrecked,
            airborne: ground.is_some_and(|g| frame.origin[1] - g > 2.),
            chassis_contact: frame.chassis_contact_count + frame.obstacle_contact_count > 0,
        },
        drive: DriveInput {
            terrain,
            mph: (frame.speed_magnitude * atv_core::hud::MPH_FACTOR) as i32,
        },
        origin: frame.origin,
        velocity: frame.linear_velocity,
        water_depth: water.map(|w| frame.origin[1] - w),
        wheels: [
            centers.first().copied().unwrap_or(frame.origin),
            centers.get(1).copied().unwrap_or(frame.origin),
        ],
    }
}

/// Everything one fixed step hands the audio layer.
#[derive(Debug, Clone)]
pub struct AudioStep {
    pub quad: QuadFrame,
    /// The actor sound position (`001e7600`): the quad origin while riding
    /// or attached; detached phases keep the last attached point.
    pub rider_position: Vec3,
    /// The detached rider body's sampled contact delta (zero while riding).
    pub rider_impact: Vec3,
    pub rider_water: bool,
    pub riding: bool,
    /// Applied throttle, ground contact and speed for the engine model.
    pub throttle: f32,
    pub grounded: bool,
    pub speed: f32,
    /// The recovered camera frame of this step (the listener).
    pub camera: CameraPose,
    /// Increments on every reset; the audio layer resets its voices.
    pub placement: u32,
    /// A motorcycle's own cues (web/audio.js motorcycle branch); None on an ATV.
    pub bike: Option<BikeSound>,
    /// The ENV08 train's car matrices this step and the step before
    /// (captureSpatial's `cars`/`previous_cars`); empty without a train.
    pub train_cars: Vec<([f32; 16], [f32; 16])>,
}

/// The motorcycle frame fields web/audio.js reads besides the engine inputs.
#[derive(Debug, Clone, Default)]
pub struct BikeSound {
    /// `motorcycle_tick`: one impact per step at most.
    pub tick: u64,
    /// `impact_sample`: the bank's first wreck sample on a contact of 18.8
    /// ft/s or more this step.
    pub impact_sample: Option<String>,
    /// The clutch as the step applied it (the clutch sample plays on press).
    pub clutch: bool,
}
