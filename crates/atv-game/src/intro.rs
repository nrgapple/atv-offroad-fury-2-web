//! Pre-race camera shots (`<venue>natN.vub`) for a race-course load, as the
//! browser's op 113 installs them: atv_core::race_intro plays the authored
//! shots, then swoops to the follow pose. Presentation only until the race
//! itself is ported; Snow follows this camera during the intro as it does in
//! the browser.
use crate::{Result, materials::member_named};
use atv_core::{
    race_intro::{Intro, Pose, Scene, ShotFrame, parse_vub},
    resource::Resource,
};

/// The race intro's shots in load order (nat1, nat2, ...).
pub fn shots(archive: &Resource, environment: &str) -> Result<Vec<Vec<ShotFrame>>> {
    let mut shots = Vec::new();
    for shot in 1..=4 {
        let Some(index) = member_named(archive, &format!("{environment}nat{shot}.vub")) else {
            break;
        };
        shots.push(parse_vub(&archive.extract(index)?)?);
    }
    Ok(shots)
}

pub struct IntroCamera {
    intro: Intro,
    rider: [f32; 3],
    forward: [f32; 3],
    pose: Option<Pose>,
}

impl IntroCamera {
    pub fn new(shots: Vec<Vec<ShotFrame>>, rider: [f32; 3], forward: [f32; 3]) -> Option<Self> {
        let intro = Intro::new(shots);
        intro.active().then_some(Self {
            intro,
            rider,
            forward,
            pose: None,
        })
    }

    /// One 60 Hz step. `follow` is the pose the swoop ends on.
    pub fn step(&mut self, follow: Pose, ground: &dyn Fn(f32, f32) -> f32) -> Result<Option<Pose>> {
        self.pose = if self.intro.active() {
            self.intro.step(
                1. / 60.,
                &Scene {
                    rider: self.rider,
                    forward: self.forward,
                    follow,
                    ground,
                },
            )?
        } else {
            None
        };
        Ok(self.pose)
    }

    /// The rider start and forward the swoop is built around.
    pub fn start(&self) -> ([f32; 3], [f32; 3]) {
        (self.rider, self.forward)
    }

    pub fn pose(&self) -> Option<Pose> {
        self.pose
    }
}
