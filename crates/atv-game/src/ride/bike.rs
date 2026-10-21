//! The motorcycle player (MX bikes, selections 27-46): atv-web's separate
//! two-wheel integration (crates/atv-web/src/motorcycles.rs, ops 23/24/46 on
//! a motorcycle) driven through the shared ride, camera, HUD and race state.
//! The bike's original model carries its own rider animation and wreck, so
//! none of the ATV rider rig applies.
use super::{Controls, Mount, Ride, STEP, Simulation};
use crate::Result;
use atv_core::{
    motorcycle::{Frame, Model, Motorcycle},
    scene_collision::SceneCollision,
    vehicle::{VehicleFrame, VehicleInput},
};

/// Steps op 23 lets a new or placed motorcycle settle before the first frame.
const SETTLE_STEPS: usize = 30;

/// The bike's own partitioned scene (`motorcycle_scene`): static props,
/// vegetation proxies and mud categories.
fn scene(sim: &Simulation) -> Result<Option<SceneCollision>> {
    let Some(config) = sim.scene_partition else {
        return Ok(None);
    };
    let mut scene = SceneCollision::from_terrain(
        config,
        sim.obstacles.clone(),
        sim.ecosystem.as_ref(),
        &sim.terrain,
        false,
    )?;
    if sim.mud_obstacles.len() == sim.obstacles.len() {
        scene.set_mud_obstacles(sim.mud_obstacles.clone())?;
    }
    Ok(Some(scene))
}

/// Op 23 with a freshly loaded model (op 100): the bike at `start`.
pub(super) fn start(
    sim: &mut Simulation,
    model: Model,
    (origin, yaw): ([f32; 3], f32),
) -> Result<Motorcycle> {
    let mut bike = Motorcycle::new(model, origin, yaw)?;
    bike.set_scene(scene(sim)?);
    place(sim, &mut bike, None)?;
    Ok(bike)
}

/// Op 23: an explicit pose places the bike there (otherwise it resets in
/// place), then the venue's collision sources and 30 settling steps.
pub(super) fn place(
    sim: &Simulation,
    bike: &mut Motorcycle,
    start: Option<([f32; 3], f32)>,
) -> Result<()> {
    if let Some((origin, yaw)) = start {
        bike.place(origin, yaw)?;
    }
    bike.set_obstacles(sim.obstacles.clone());
    bike.set_materials(Some(sim.materials));
    if let Some(config) = sim.boundary {
        bike.set_boundary(config, sim.extended_boundary)?;
    }
    for _ in 0..SETTLE_STEPS {
        bike.step(&sim.terrain, VehicleInput::default(), STEP)?;
    }
    Ok(())
}

/// The bike's frame in the shared [`VehicleFrame`] shape the HUD, camera,
/// race, effects and traces read. Two wheels fill the four slots as front,
/// front, rear, rear; fields the bike has no counterpart for stay neutral.
pub(super) fn summary(f: &Frame) -> VehicleFrame {
    let four = |v: [f32; 2]| [v[0], v[0], v[1], v[1]];
    VehicleFrame {
        origin: f.origin,
        orientation: f.orientation,
        speed: f.speed,
        speed_magnitude: f.speed_magnitude,
        linear_velocity: f.linear_velocity,
        local_angular_velocity: f.local_angular_velocity,
        steering_angle: f.steering_angle,
        visual_steering_angle: f.visual_steering_angle,
        rpm: f.rpm,
        gear: f.gear,
        wheel_angles: four(f.wheel_angles),
        wheel_compression: four(f.wheel_compression),
        grounded: [f.grounded[0], f.grounded[0], f.grounded[1], f.grounded[1]],
        wheel_obstacle_support: [false; 4],
        suspension_stop_contacts: 0,
        chassis_contact_count: f.chassis_contact_count,
        obstacle_contact_count: 0,
        dynamic_contact_count: f.dynamic_contact_count,
        dynamic_secondary_contacts: 0,
        crash_impact_speed: f.crash_impact_speed,
        crash_impact_point: [0.; 3],
        preload_available: f.preload_available,
        preload_armed: f.preload_armed,
        preload_active: f.preload_active,
        preload_strength: f.preload_strength,
        wheelie_active: f.wheelie_active,
        wreck: f.wreck,
        boundary_launch: f.boundary_launch,
        skip_vegetation_query: false,
        water_height: -256.,
        water_failure: Default::default(),
        landing_action: None,
        reset_count: f.reset_count,
        scene_query: Default::default(),
        flip: None,
        flip_events: Vec::new(),
    }
}

/// A neutral frame until the first placement fills the real one.
pub(super) fn placeholder_frame() -> VehicleFrame {
    summary(&Frame::default())
}

impl Ride {
    /// The step's sound inputs for a motorcycle: the shared engine model's
    /// inputs and listener plus the bike's impact and clutch cues (atv-web's
    /// motorcycle `frame`: `impact_sample`, `rider_audio_position`).
    pub(super) fn capture_bike_sound(&self, controls: &Controls) -> super::sound::AudioStep {
        let Mount::Bike(bike) = &self.mount else {
            unreachable!("motorcycle sound without a motorcycle");
        };
        let f = bike.frame();
        let frame = &self.frame;
        super::sound::AudioStep {
            quad: super::sound::QuadFrame {
                origin: f.origin,
                velocity: f.linear_velocity,
                wheels: [f.origin; 2],
                ..Default::default()
            },
            rider_position: f.origin,
            rider_impact: [0.; 3],
            rider_water: false,
            riding: f.wreck.phase == super::Phase::Riding,
            throttle: self.applied.throttle,
            grounded: f.grounded.iter().any(|g| *g),
            speed: frame.speed_magnitude,
            camera: atv_core::spatial_audio::CameraPose {
                position: self.camera_frame.position,
                target: self.camera_frame.target,
            },
            placement: self.placement,
            bike: Some(super::sound::BikeSound {
                tick: bike.tick,
                impact_sample: bike
                    .model
                    .audio_config
                    .impact_samples
                    .first()
                    .filter(|_| f.crash_impact_speed >= 18.8)
                    .cloned(),
                clutch: controls.clutch,
            }),
        }
    }

    /// The motorcycle's op 24 for one step (atv-web `dispatch_sim`): the
    /// browser drive adapters, the race's imposed controls, spin/clutch
    /// intent, physics, the stunt controller, then the HUD.
    pub(super) fn step_bike(&mut self, controls: &Controls) -> Result<()> {
        let dt = STEP;
        let mut driving = VehicleInput {
            throttle: controls.throttle,
            brake: controls.brake,
            steer: controls.steer,
            lean: controls.lean,
            preload_axis: controls.preload_axis,
            reverse: controls.reverse,
            reverse_chord_brake: controls.reverse_chord_brake,
            lean_flick_rate: None,
        };
        let Mount::Bike(bike) = &mut self.mount else {
            return Err("not a motorcycle".into());
        };
        if self.input_generation != Some(controls.generation) {
            self.browser_drive.reset();
            self.input_generation = Some(controls.generation);
            bike.cancel_controls();
        }
        let current = bike.frame();
        driving = self.browser_drive.resolve(
            driving,
            controls.keyboard_drive,
            controls.touch_drive,
            current.grounded.iter().any(|g| *g),
            current.speed,
        )?;
        if let Some(brake) = driving.reverse_chord_brake
            && current.speed.abs() > 5.
        {
            driving.reverse = false;
            driving.brake = driving.brake.max(brake);
        }
        // The countdown brake and the post-finish CPU driver replace the
        // player's controls, including tricks and the clutch.
        let imposed = self.race_controls()?;
        let Mount::Bike(bike) = &mut self.mount else {
            return Err("not a motorcycle".into());
        };
        let (mask, axes, clutch) = match imposed {
            Some(input) => {
                driving = input;
                (0, [0.; 2], false)
            }
            None => {
                if controls.stunt_modifiers > 7 {
                    return Err("invalid motorcycle stunt mask".into());
                }
                (
                    controls.stunt_modifiers,
                    [controls.stunt_x, controls.stunt_y],
                    controls.clutch,
                )
            }
        };
        bike.spin_controls(mask, axes)?;
        bike.clutch_controls(clutch);
        let sim = &mut self.sim;
        if let Some(water) = &mut sim.water {
            water.advance(dt)?;
        }
        if let Some(race) = &mut self.race
            && std::mem::take(&mut race.autopilot_reset)
        {
            bike.reset_now()?;
        }
        bike.step(&sim.terrain, driving, dt)?;
        bike.stunt_controls(mask, axes, dt)?;
        if let Some(water) = &mut sim.water {
            water.prepare_surface()?;
        }
        let f = bike.frame();
        if let Some(name) = &bike.stunt_name {
            self.hud.name_event(name);
        } else if f.wheelie_active {
            self.hud.name_event("Wheelie");
        } else if f.stoppie_active {
            self.hud.name_event("Stoppie");
        }
        self.hud.step(
            dt,
            f.speed.abs(),
            f.preload_armed,
            f.preload_strength,
            false,
            0.,
        )?;
        // A recovery or deep-water reset restarts the follow camera.
        if f.reset_count != self.frame.reset_count {
            self.sim.camera.reset();
            self.wreck_camera = None;
        }
        self.applied = Controls {
            throttle: driving.throttle,
            brake: driving.brake,
            steer: driving.steer,
            lean: driving.lean,
            reverse: driving.reverse,
            ..*controls
        };
        self.frame = summary(f);
        Ok(())
    }
}
