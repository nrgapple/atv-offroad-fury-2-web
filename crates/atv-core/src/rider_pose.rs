//! Riding and stunt animation selection from USA v2.00 routines 0x002d8ba0/8e50.
//! Original clips, blend order and stunt main/hold clocks are reconstructed.
//! Wrecks, scoring, events and look-target selection remain separate systems.
use crate::Result;

/// Vertical-motion crouch controller at the end of USA v2.00 001e5260.
/// These are animation blend weights, never additional suspension forces.
#[derive(Debug, Clone, Copy)]
pub struct Crouch {
    demand: f32,
    previous_vertical_speed: f32,
    pub blend: f32,
}
impl Default for Crouch {
    fn default() -> Self {
        Self {
            demand: 0.2,
            previous_vertical_speed: 0.0,
            blend: 0.5,
        }
    }
}
impl Crouch {
    pub fn step(&mut self, local_vertical_speed: f32, dt: f32) -> Result<()> {
        if !local_vertical_speed.is_finite() || !dt.is_finite() || dt <= 0.0 {
            return Err("invalid crouch motion input".into());
        }
        self.demand += ((0.2 - self.blend) * 0.5).clamp(-2.25 * dt, 2.25 * dt);
        let change = local_vertical_speed - self.previous_vertical_speed;
        if change > 0.0 {
            self.demand = (self.demand + change / 38.0).min(1.5);
        } else if self.demand <= 0.25 {
            self.demand = (self.demand + change * 0.15).max(0.0);
        }
        self.blend += ((self.demand.min(1.0) - self.blend) * 0.5).clamp(-6.0 * dt, 6.0 * dt);
        self.previous_vertical_speed = local_vertical_speed;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Mode {
    Reverse,
    Turn,
    Lean,
    Air,
    Stunt,
    Hold,
    Landing,
}

/// Forced single-clip actions selected by 002daa50 and loaded by 002d6cc0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[repr(u8)]
pub enum Landing {
    Front,
    Vertical,
}
impl Landing {
    pub const SUFFIXES: [&'static str; 2] = ["LandHardF", "LandHard"];
}

/// 001e8690 uses collision velocity corrections in ATV local space. The
/// hard-landing branches follow all five impact-wreck branches, before angle
/// wreck checks; the caller must discard an action if those later checks wreck.
#[derive(Debug, Clone, Copy)]
pub struct LandingConfig {
    pub wreck_impact: [f32; 3],
    pub vertical: f32,
    pub longitudinal: f32,
}
impl LandingConfig {
    pub fn from_physics(data: &crate::physics_data::PhysicsData, upgrade: f32) -> Result<Self> {
        let rules = crate::wreck::Config::from_physics(data, upgrade)?;
        let get = |name| data.first_scalar("Wrecks", name);
        Ok(Self {
            wreck_impact: rules.impact_limit,
            vertical: get("VerticalHardLandingFps")?
                + get("TweakVerticalHardLandingFps")? * upgrade,
            longitudinal: get("LongitudinalHardLandingFps")?,
        })
    }
    pub fn evaluate(self, local_impact: [f32; 3], vertical_limit_scale: f32) -> Option<Landing> {
        let [x, y, z] = local_impact;
        let [lateral, vertical, longitudinal] = self.wreck_impact;
        if z > longitudinal
            || z < -longitudinal
            || x > lateral
            || x < -lateral
            || y > vertical * vertical_limit_scale
        {
            None
        } else if z < -self.longitudinal {
            Some(Landing::Front)
        } else if y > self.vertical {
            Some(Landing::Vertical)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Clip {
    TurnFwd,
    TurnMid,
    TurnBck,
    LeanFwd,
    LeanMid,
    LeanBck,
    LeanFwdD,
    LeanMidD,
    LeanBckD,
    CrosFwd,
    CrosMid,
    CrosBck,
    Reverse,
    LookLr,
}
impl Clip {
    pub const ALL: [Self; 14] = [
        Self::TurnFwd,
        Self::TurnMid,
        Self::TurnBck,
        Self::LeanFwd,
        Self::LeanMid,
        Self::LeanBck,
        Self::LeanFwdD,
        Self::LeanMidD,
        Self::LeanBckD,
        Self::CrosFwd,
        Self::CrosMid,
        Self::CrosBck,
        Self::Reverse,
        Self::LookLr,
    ];
    /// Catalog suffix; its original rider-specific M_/F_ prefix is separate.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::TurnFwd => "TurnFwd",
            Self::TurnMid => "TurnMid",
            Self::TurnBck => "TurnBck",
            Self::LeanFwd => "LeanFwd",
            Self::LeanMid => "LeanMid",
            Self::LeanBck => "LeanBck",
            Self::LeanFwdD => "LeanFwdD",
            Self::LeanMidD => "LeanMidD",
            Self::LeanBckD => "LeanBckD",
            Self::CrosFwd => "CrosFwd",
            Self::CrosMid => "CrosMid",
            Self::CrosBck => "CrosBck",
            Self::Reverse => "Reverse",
            Self::LookLr => "Look_LR",
        }
    }
}

/// Original normal-riding controller evaluation. Tricks, IK, look-target
/// acquisition and the crouch signal are intentionally supplied separately.
#[derive(Clone)]
pub struct Motion {
    pub animation: crate::animation::Animation,
    pub sampling: crate::animation::Sampling,
}
/// Small presentation recipe; poses are reconstructed from already loaded clips.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recipe {
    pub clips: [u8; 4],
    pub count: u8,
    pub phase: f32,
    pub first_weight: f32,
    pub pair_weight: f32,
    pub look_phase: f32,
}
impl Default for Recipe {
    fn default() -> Self {
        Self {
            clips: [4, 0, 0, 0],
            count: 1,
            phase: 0.5,
            first_weight: 0.,
            pair_weight: 0.,
            look_phase: -1.,
        }
    }
}
pub struct Animator {
    recipe: Recipe,

    motions: Vec<Motion>,
    state: State,
    current: crate::animation::Channels,
    saved: crate::animation::Channels,
    head: usize,
    stunt_motions: Vec<Motion>,
    stunt_looping: Vec<bool>,
    stunt_state: crate::stunt::State,
    stunt_frame: Option<crate::stunt::Frame>,
    hold_noise: crate::stunt::HoldNoise,
    landing_motions: Vec<Motion>,
    landing: Option<Landing>,
    landing_phase: f32,
    score: crate::stunt_score::State,
    wheelie: crate::stunt_score::WheelieState,
    scoring_mode: Option<u8>,
    hand_ik: Option<crate::rider_ik::Hands>,
}
impl Animator {
    /// Motions must follow Clip::ALL. Reject controller layouts that the original
    /// indexed pair/four-clip blend cannot safely combine.
    pub fn new(motions: Vec<Motion>, nodes: &[crate::mesh::Node]) -> Result<Self> {
        if motions.len() != Clip::ALL.len() {
            return Err("normal riding requires fourteen original motions".into());
        }
        let layout = |motion: &Motion| -> Result<(Vec<usize>, Vec<usize>)> {
            let a = &motion.animation;
            Ok((
                a.translations
                    .iter()
                    .map(|t| a.model_bone(t.bone, nodes))
                    .collect::<Result<_>>()?,
                a.rotations
                    .iter()
                    .map(|t| a.model_bone(t.bone, nodes))
                    .collect::<Result<_>>()?,
            ))
        };
        let expected = layout(&motions[0])?;
        for indices in [&expected.0, &expected.1] {
            if indices
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != indices.len()
            {
                return Err("duplicate normal riding controllers".into());
            }
        }
        // Reverse is sampled alone and may use a different controller order.
        for motion in &motions[..12] {
            if layout(motion)? != expected {
                return Err("incompatible normal riding controller order".into());
            }
        }
        let initial = &motions[Clip::TurnMid as usize];
        let current = initial.animation.sample_channels(
            nodes,
            initial.animation.duration * 0.5,
            initial.sampling,
        )?;
        let head = nodes
            .iter()
            .position(|node| node.name.eq_ignore_ascii_case("Head"))
            .ok_or("rider Head node is missing")?;
        Ok(Self {
            recipe: Recipe::default(),
            motions,
            state: State::default(),
            saved: current.clone(),
            current,
            head,
            stunt_motions: Vec::new(),
            stunt_looping: Vec::new(),
            stunt_state: crate::stunt::State::default(),
            stunt_frame: None,
            hold_noise: crate::stunt::HoldNoise::new(
                &mut crate::engine_audio::OriginalRandom::with_seed(1),
            ),
            landing_motions: Vec::new(),
            landing: None,
            landing_phase: 0.,
            score: crate::stunt_score::State::default(),
            wheelie: crate::stunt_score::WheelieState::default(),
            scoring_mode: None,
            hand_ik: None,
        })
    }
    /// Independent normal-pose controller sharing the same recovered motion data.
    /// Gameplay/trick/scoring clocks are initialized for the new actor.
    pub fn fork_normal(&self, nodes: &[crate::mesh::Node]) -> Result<Self> {
        Self::new(self.motions.clone(), nodes)
    }
    /// Independent race actor with the same recovered stunt/landing catalog.
    /// Controller clocks and scores belong to the new rider.
    pub fn fork_racer(&self, nodes: &[crate::mesh::Node]) -> Result<Self> {
        let mut rider = self.fork_normal(nodes)?;
        if !self.stunt_motions.is_empty() {
            rider.set_stunts(self.stunt_motions.clone(), self.stunt_looping.clone())?;
        }
        if !self.landing_motions.is_empty() {
            rider.set_landing_motions(self.landing_motions.clone())?;
        }
        if self.hand_ik.is_some() {
            rider.configure_native_hand_ik(nodes)?;
        }
        Ok(rider)
    }
    pub fn recipe(&self) -> Recipe {
        self.recipe
    }
    pub fn channels(&self) -> crate::animation::Channels {
        self.current.clone()
    }
    pub fn pose(&self, nodes: &[crate::mesh::Node]) -> Result<crate::animation::Pose> {
        self.current.pose(nodes)
    }
    /// Prepare original shoulder/elbow/hand chains without activating them.
    /// Native callers supply the mirrored local hinge axis separately.
    pub fn configure_hand_ik(
        &mut self,
        nodes: &[crate::mesh::Node],
        left_axis: [f32; 3],
    ) -> Result<()> {
        self.hand_ik = Some(crate::rider_ik::Hands::bind(nodes, left_axis)?);
        Ok(())
    }
    /// Bind the recovered rider-local +Y/-Y hand hinges without enabling IK.
    pub fn configure_native_hand_ik(&mut self, nodes: &[crate::mesh::Node]) -> Result<()> {
        self.hand_ik = Some(crate::rider_ik::Hands::bind_native(nodes)?);
        Ok(())
    }
    /// Existing normal/stunt/landing states request None, matching002d8e50's
    /// cleared hand gate. Explicit targets share the sampled pose's frame.
    pub fn pose_with_hand_targets(
        &self,
        nodes: &[crate::mesh::Node],
        targets: Option<[[f32; 3]; 2]>,
    ) -> Result<crate::animation::Pose> {
        let mut pose = self.pose(nodes)?;
        if targets.is_some() {
            self.hand_ik
                .as_ref()
                .ok_or("rider hand IK is not configured")?
                .apply(nodes, &mut pose, targets)?;
        }
        Ok(pose)
    }
    pub fn reset(&mut self, nodes: &[crate::mesh::Node]) -> Result<()> {
        let initial = &self.motions[Clip::TurnMid as usize];
        self.current = initial.animation.sample_channels(
            nodes,
            initial.animation.duration * 0.5,
            initial.sampling,
        )?;
        self.saved = self.current.clone();
        self.state = State::default();
        self.stunt_state.reset();
        self.stunt_frame = None;
        self.landing = None;
        self.landing_phase = 0.;
        self.abort_score_chain()?;
        Ok(())
    }
    /// Caller supplies the exact original game-mode id and eligibility. Mode7
    /// bypasses repeat penalties; None disables scoring for this rider.
    pub fn set_scoring_mode(&mut self, native_mode: Option<u8>) -> Result<()> {
        if native_mode.is_none() {
            self.score.abort_rider()?;
        }
        self.scoring_mode = native_mode;
        Ok(())
    }
    pub fn score_frame(&self) -> crate::stunt_score::Frame {
        self.score.frame()
    }
    /// 001e70e0 credits wheelie awards to the same entity+104 scoreboard as
    /// airborne stunts, without integer quantization or a combo multiplier.
    pub fn credit_wheelie_points(&mut self, points: f32) -> Result<()> {
        self.score.credit_wheelie_points(points)
    }
    pub fn wheelie_frame(&self) -> crate::stunt_score::WheelieState {
        self.wheelie
    }
    /// Actor001e5260 wheelie section follows physics/wreck gates and precedes
    /// rider controls. Supply native body Euler pitch and individual contacts.
    /// The original scoring flag remains false unless GETONUP is enabled.
    pub fn update_wheelie(
        &mut self,
        dt: f32,
        input: crate::stunt_score::WheelieInput,
    ) -> Result<()> {
        for event in self.wheelie.update(dt, input)? {
            self.score.record_wheelie_event(event)?;
        }
        Ok(())
    }
    pub fn take_score_notifications(&mut self) -> Vec<crate::stunt_score::Notification> {
        self.score.take_notifications()
    }
    /// Starting/restarting an event clears all score and repetition history.
    /// Ordinary ATV reset/recovery instead calls reset and aborts only the chain.
    pub fn reset_score_session(&mut self) {
        self.score = crate::stunt_score::State::default();
        self.wheelie = crate::stunt_score::WheelieState::default();
    }
    pub fn abort_score_chain(&mut self) -> Result<()> {
        self.score.abort_rider()
    }
    /// True wreck entry001e8a20 differs from generic vehicle reset001dd440,
    /// which does not clear any wheelie scoring fields.
    pub fn abort_wreck_scores(&mut self) -> Result<()> {
        // 001e8a20 aborts airborne stunt accounting before wheelie HUD failure.
        self.score.abort_rider()?;
        if let Some(event) = self.wheelie.wreck() {
            self.score.record_wheelie_event(event)?;
        }
        Ok(())
    }
    pub fn stunt_duration(&self, id: usize) -> Option<f32> {
        self.stunt_motions
            .get(id)
            .filter(|_| id < 24)
            .map(|motion| motion.animation.duration)
    }
    pub fn set_landing_motions(&mut self, motions: Vec<Motion>) -> Result<()> {
        if motions.len() != 2
            || motions
                .iter()
                .any(|m| !m.animation.duration.is_finite() || m.animation.duration <= 0.)
        {
            return Err("landing actions require LandHardF and LandHard".into());
        }
        self.landing_motions = motions;
        Ok(())
    }
    /// Repeated requests do not restart a clip: native 002daa50 changes only
    /// the action id; the phase is reset by the animation mode transition.
    pub fn request_landing(&mut self, landing: Landing) -> Result<()> {
        if self.landing_motions.is_empty() {
            return Err("landing motions are not loaded".into());
        }
        self.landing = Some(landing);
        Ok(())
    }
    pub fn landing_frame(&self) -> Option<(Landing, f32)> {
        self.landing.map(|action| (action, self.landing_phase))
    }
    /// Original 24 main clips followed by their 24 held extensions.
    pub fn set_stunts(&mut self, motions: Vec<Motion>, looping: Vec<bool>) -> Result<()> {
        if motions.len() != 48
            || looping.len() != 48
            || motions
                .iter()
                .any(|m| !m.animation.duration.is_finite() || m.animation.duration <= 0.)
        {
            return Err("stunts require 24 original main and hold motions".into());
        }
        self.stunt_motions = motions;
        self.stunt_looping = looping;
        Ok(())
    }
    pub fn request_cpu_stunt(&mut self, id: usize, input: crate::stunt::Input) -> bool {
        !self.stunt_motions.is_empty() && self.stunt_state.request_direct(id, input)
    }
    pub fn stunt_frame(&self) -> Option<crate::stunt::Frame> {
        self.stunt_frame
    }
    pub fn stunt_landing_safe(&self) -> bool {
        self.stunt_frame
            .is_none_or(|frame| frame.landing_safe(self.stunt_motions[frame.id].animation.duration))
    }
    pub fn step_controls(
        &mut self,
        nodes: &[crate::mesh::Node],
        input: Input,
        stunt: crate::stunt::Input,
        dt: f32,
    ) -> Result<Mode> {
        self.step_controls_with_buttons(nodes, input, stunt, 0, dt)
    }
    pub fn step_controls_with_buttons(
        &mut self,
        nodes: &[crate::mesh::Node],
        input: Input,
        stunt: crate::stunt::Input,
        direction_buttons: u8,
        dt: f32,
    ) -> Result<Mode> {
        if self.scoring_mode.is_some() {
            // Game rules00248020 execute0024dd00 before their child update
            // traversal. This uses the previous rider hold flag for that tick.
            // Original scene sibling/parent attachment order remains separate.
            self.score.tick(dt)?;
        }
        if self.landing_phase >= 1. || stunt.crashed || stunt.disabled {
            self.landing = None;
        }
        if self.stunt_motions.is_empty() && self.landing.is_none() {
            return Ok(self.step(nodes, input, dt)?.0.mode);
        }
        if !self.stunt_motions.is_empty() {
            let durations = std::array::from_fn(|i| self.stunt_motions[i].animation.duration);
            self.stunt_frame = self.stunt_state.step_controls(
                stunt,
                direction_buttons,
                dt,
                &durations,
                self.landing.is_none(),
            )?;
        }
        if let Some(native_mode) = self.scoring_mode {
            if stunt.crashed || stunt.disabled {
                self.abort_wreck_scores()?;
            } else {
                if self.stunt_frame.is_some_and(|frame| frame.started) && self.wheelie.eligible {
                    // 002da6c0 invokes001e70e0 and hides the old wheelie HUD
                    // before it finalizes/allocates any regular stunt records.
                    if let Some(event) = self.wheelie.finish() {
                        self.score.record_wheelie_event(event)?;
                    }
                    self.score.clear_wheelie_for_stunt();
                }
                self.score.observe_rider(
                    self.stunt_frame,
                    self.stunt_frame
                        .and_then(|frame| self.stunt_duration(frame.id)),
                    stunt.grounded,
                    dt,
                    native_mode,
                )?;
            }
        }
        if let Some(landing) = self.landing {
            let snapshot = self.select_single_mode(
                Mode::Landing,
                input,
                self.stunt_frame.is_some_and(|f| f.started),
                dt,
            )?;
            if snapshot {
                self.landing_phase = 0.;
            }
            let motion = &self.landing_motions[landing as usize];
            self.landing_phase = unit(self.landing_phase + dt * (1. / motion.animation.duration));
            let target = motion.animation.sample_channels(
                nodes,
                self.landing_phase * motion.animation.duration,
                motion.sampling,
            )?;
            self.recipe = Recipe {
                clips: [62 + landing as u8, 0, 0, 0],
                count: 1,
                phase: self.landing_phase,
                ..Recipe::default()
            };
            self.apply(&target, self.state.snapshot_weight, None)?;
            return Ok(Mode::Landing);
        }
        let Some(frame) = self.stunt_frame else {
            return Ok(self.step(nodes, input, dt)?.0.mode);
        };
        let mode = if frame.holding {
            Mode::Hold
        } else {
            Mode::Stunt
        };
        let snapshot = self.select_single_mode(mode, input, frame.started, dt)?;
        let index = frame.id + if frame.holding { 24 } else { 0 };
        let motion = &self.stunt_motions[index];
        let phase = frame.phase(
            motion.animation.duration,
            self.stunt_looping[index],
            snapshot,
            self.hold_noise.sample(1.25 * frame.hold_time),
        );
        self.landing_phase = phase;
        let target = motion.animation.sample_channels(
            nodes,
            phase.clamp(0., 1.) * motion.animation.duration,
            motion.sampling,
        )?;
        self.recipe = Recipe {
            clips: [14 + index as u8, 0, 0, 0],
            count: 1,
            phase: phase.clamp(0., 1.),
            ..Recipe::default()
        };
        self.apply(&target, self.state.snapshot_weight, None)?;
        Ok(mode)
    }
    fn select_single_mode(
        &mut self,
        mode: Mode,
        input: Input,
        started: bool,
        dt: f32,
    ) -> Result<bool> {
        if !dt.is_finite() || dt < 0. {
            return Err("invalid rider pose time step".into());
        }
        self.state.snapshot_weight = (self.state.snapshot_weight - dt * 4.).clamp(0., 1.);
        let snapshot = !input.suppress_snapshot
            && (mode != self.state.previous_mode || started || input.force_snapshot);
        if snapshot {
            self.saved = self.current.clone();
            self.state.snapshot_weight = 1.;
            self.state.previous_mode = mode;
        }
        Ok(snapshot)
    }
    pub fn step(
        &mut self,
        nodes: &[crate::mesh::Node],
        input: Input,
        dt: f32,
    ) -> Result<(Plan, crate::animation::Pose)> {
        let plan = self.state.step(input, dt)?;
        // Native +0x3d8 is shared between normal, stunt and forced actions.
        self.landing_phase = plan.phase;
        if plan.snapshot {
            self.saved = self.current.clone();
        }
        let first = plan.blend.clips()[0];
        let samples = plan.samples(self.motions[first as usize].animation.duration)?;
        let mut sampled = Vec::new();
        for sample in samples {
            let motion = &self.motions[sample.clip as usize];
            sampled.push(
                motion
                    .animation
                    .sample_channels(nodes, sample.time, motion.sampling)?,
            );
        }
        let target = match plan.blend {
            Blend::Single(_) => sampled.remove(0),
            Blend::Pair { first_weight, .. } => pair(&sampled[0], &sampled[1], first_weight)?,
            Blend::Four {
                first_weight,
                pair_weight,
                ..
            } => pair(
                &pair(&sampled[0], &sampled[1], first_weight)?,
                &pair(&sampled[2], &sampled[3], first_weight)?,
                pair_weight,
            )?,
        };
        let mut clips = [0; 4];
        for (out, clip) in clips.iter_mut().zip(plan.blend.clips()) {
            *out = *clip as u8;
        }
        let (first_weight, pair_weight) = match plan.blend {
            Blend::Single(_) => (0., 0.),
            Blend::Pair { first_weight, .. } => (first_weight, 0.),
            Blend::Four {
                first_weight,
                pair_weight,
                ..
            } => (first_weight, pair_weight),
        };
        self.recipe = Recipe {
            clips,
            count: plan.blend.clips().len() as u8,
            phase: plan.phase,
            first_weight,
            pair_weight,
            look_phase: plan.head_look_phase.unwrap_or(-1.),
        };
        self.apply(&target, plan.snapshot_weight, None)?;
        if let Some(phase) = plan.head_look_phase {
            let motion = &self.motions[Clip::LookLr as usize];
            let look = motion.animation.sample_channels(
                nodes,
                phase * motion.animation.duration,
                motion.sampling,
            )?;
            self.apply(&look, plan.snapshot_weight, Some(self.head))?;
        }
        Ok((plan, self.pose(nodes)?))
    }
    pub fn present(
        &mut self,
        nodes: &[crate::mesh::Node],
        recipe: Recipe,
        dt: f32,
    ) -> Result<crate::animation::Pose> {
        if !matches!(recipe.count, 1 | 2 | 4)
            || recipe.clips.iter().any(|c| *c >= 64)
            || ![recipe.phase, recipe.first_weight, recipe.pair_weight]
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
            || !recipe.look_phase.is_finite()
            || !(-1. ..=1.).contains(&recipe.look_phase)
            || !dt.is_finite()
            || dt < 0.
        {
            return Err("invalid remote animation recipe".into());
        }
        if recipe.clips != self.recipe.clips || recipe.count != self.recipe.count {
            self.saved = self.current.clone();
            self.state.snapshot_weight = 1.;
        }
        self.state.snapshot_weight = (self.state.snapshot_weight - dt * 4.).max(0.);
        let motion = |id: u8| -> Result<&Motion> {
            match id {
                0..=13 => self.motions.get(id as usize),
                14..=61 => self.stunt_motions.get(id as usize - 14),
                62..=63 => self.landing_motions.get(id as usize - 62),
                _ => None,
            }
            .ok_or_else(|| "remote motion is unavailable".into())
        };
        let time = motion(recipe.clips[0])?.animation.duration * recipe.phase;
        let samples = recipe.clips[..recipe.count as usize]
            .iter()
            .map(|id| {
                let m = motion(*id)?;
                m.animation.sample_channels(nodes, time, m.sampling)
            })
            .collect::<Result<Vec<_>>>()?;
        let target = match recipe.count {
            1 => samples[0].clone(),
            2 => pair(&samples[0], &samples[1], recipe.first_weight)?,
            _ => pair(
                &pair(&samples[0], &samples[1], recipe.first_weight)?,
                &pair(&samples[2], &samples[3], recipe.first_weight)?,
                recipe.pair_weight,
            )?,
        };
        self.apply(&target, self.state.snapshot_weight, None)?;
        if recipe.look_phase >= 0. {
            let m = &self.motions[Clip::LookLr as usize];
            let look = m.animation.sample_channels(
                nodes,
                recipe.look_phase * m.animation.duration,
                m.sampling,
            )?;
            self.apply(&look, self.state.snapshot_weight, Some(self.head))?;
        }
        self.recipe = recipe;
        self.pose(nodes)
    }
    fn apply(
        &mut self,
        target: &crate::animation::Channels,
        weight: f32,
        only: Option<usize>,
    ) -> Result<()> {
        for index in 0..self.current.translations.len() {
            if only.is_some_and(|node| node != index) {
                continue;
            }
            if let Some(new) = target.translations[index] {
                let old = self.saved.translations[index]
                    .ok_or("rider snapshot translation is missing")?;
                self.current.translations[index] = Some(if weight > 0. {
                    std::array::from_fn(|i| new[i] + weight * (old[i] - new[i]))
                } else {
                    new
                });
            }
            if let Some(new) = target.rotations[index] {
                let old =
                    self.saved.rotations[index].ok_or("rider snapshot rotation is missing")?;
                self.current.rotations[index] = Some(if weight > 0. {
                    crate::animation::blend_rotation(new, old, weight)
                } else {
                    new
                });
            }
        }
        Ok(())
    }
}
fn pair(
    a: &crate::animation::Channels,
    b: &crate::animation::Channels,
    weight: f32,
) -> Result<crate::animation::Channels> {
    if a.translations.len() != b.translations.len() || a.rotations.len() != b.rotations.len() {
        return Err("rider blend channel count mismatch".into());
    }
    let mut result = a.clone();
    for (index, channel) in result.translations.iter_mut().enumerate() {
        if let Some(first) = *channel {
            let second = b.translations[index].ok_or("rider blend translation is missing")?;
            *channel = Some(std::array::from_fn(|i| {
                second[i] - weight * (second[i] - first[i])
            }));
        }
    }
    for (index, channel) in result.rotations.iter_mut().enumerate() {
        if let Some(first) = *channel {
            let second = b.rotations[index].ok_or("rider blend rotation is missing")?;
            *channel = Some(crate::animation::blend_rotation(second, first, weight));
        }
    }
    Ok(result)
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Blend {
    Single(Clip),
    /// Interpolate second toward first, preserving this order.
    Pair {
        clips: [Clip; 2],
        first_weight: f32,
    },
    /// Blend (1,2) and (3,4) by first_weight, then second pair toward first
    /// pair by pair_weight. Flattening weights changes quaternion blending.
    Four {
        clips: [Clip; 4],
        first_weight: f32,
        pair_weight: f32,
    },
}
impl Blend {
    pub fn clips(&self) -> &[Clip] {
        match self {
            Self::Single(c) => std::slice::from_ref(c),
            Self::Pair { clips, .. } => clips,
            Self::Four { clips, .. } => clips,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Input {
    /// Smoothed original joystick axes, not raw browser axes.
    pub axes: [f32; 2],
    pub local_forward_speed: f32,
    pub stand_up_speed: f32,
    pub wheel_contact: bool,
    pub reverse_gear: bool,
    /// Original actor +0x860 gate; its wider gameplay meaning is not assumed.
    pub reverse_override: bool,
    pub crouch: f32,
    /// Original look-target controller output. Constructor value is 0.5.
    pub look_phase: f32,
    pub force_snapshot: bool,
    pub suppress_snapshot: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct Plan {
    pub mode: Mode,
    pub blend: Blend,
    pub phase: f32,
    pub snapshot: bool,
    /// Weight of the saved old pose, applied after the current clip blend.
    pub snapshot_weight: f32,
    /// Turn mode's final Look_LR overlay affects only the Head node.
    pub head_look_phase: Option<f32>,
}
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub clip: Clip,
    pub time: f32,
}
impl Plan {
    /// All body clips use FIRST clip duration × phase, even if their durations
    /// differ. The caller must reject incompatible controller layouts.
    pub fn samples(&self, first_duration: f32) -> Result<Vec<Sample>> {
        if !first_duration.is_finite() || first_duration < 0.0 {
            return Err("invalid rider motion duration".into());
        }
        let time = first_duration * self.phase;
        Ok(self
            .blend
            .clips()
            .iter()
            .map(|&clip| Sample { clip, time })
            .collect())
    }
}
#[derive(Debug, Clone, Copy)]
pub struct State {
    pub previous_mode: Mode,
    pub snapshot_weight: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            previous_mode: Mode::Air,
            snapshot_weight: 0.0,
        }
    }
}
fn unit(value: f32) -> f32 {
    // Native branch order also canonicalizes negative zero to positive zero.
    if value >= 1.0 {
        1.0
    } else if value > 0.0 {
        value
    } else {
        0.0
    }
}
impl State {
    pub fn step(&mut self, input: Input, dt: f32) -> Result<Plan> {
        if [
            input.axes[0],
            input.axes[1],
            input.local_forward_speed,
            input.stand_up_speed,
            input.crouch,
            input.look_phase,
            self.snapshot_weight,
            dt,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || dt < 0.0
        {
            return Err("invalid rider pose input".into());
        }
        let mode = if input.reverse_gear || input.reverse_override {
            Mode::Reverse
        } else if !input.wheel_contact {
            Mode::Air
        } else if input.local_forward_speed < input.stand_up_speed {
            Mode::Turn
        } else {
            Mode::Lean
        };
        self.snapshot_weight = (self.snapshot_weight - dt * 4.0).clamp(0.0, 1.0);
        let snapshot =
            !input.suppress_snapshot && (mode != self.previous_mode || input.force_snapshot);
        if snapshot {
            self.snapshot_weight = 1.0;
            self.previous_mode = mode;
        }
        let phase = unit(input.axes[0] * -0.5 + 0.5);
        let fore_aft = (input.axes[1] + 1.0) * 0.5;
        let back = fore_aft >= 0.5;
        let first_weight = unit(if back {
            (fore_aft - 0.5) * 2.0
        } else {
            fore_aft * 2.0
        });
        use Clip::*;
        let blend = match mode {
            Mode::Stunt | Mode::Hold | Mode::Landing => {
                unreachable!("stunts use their own original single-clip path")
            }
            Mode::Reverse => Blend::Single(Reverse),
            Mode::Turn => Blend::Pair {
                clips: if back {
                    [TurnBck, TurnMid]
                } else {
                    [TurnMid, TurnFwd]
                },
                first_weight,
            },
            Mode::Lean => Blend::Four {
                clips: if back {
                    [LeanBck, LeanMid, LeanBckD, LeanMidD]
                } else {
                    [LeanMid, LeanFwd, LeanMidD, LeanFwdD]
                },
                first_weight,
                pair_weight: unit(input.crouch),
            },
            Mode::Air => Blend::Pair {
                clips: if back {
                    [CrosBck, CrosMid]
                } else {
                    [CrosMid, CrosFwd]
                },
                first_weight,
            },
        };
        Ok(Plan {
            mode,
            blend,
            phase,
            snapshot,
            snapshot_weight: self.snapshot_weight,
            head_look_phase: if mode == Mode::Turn {
                Some(input.look_phase)
            } else {
                None
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stunt_animator() -> (Animator, Vec<crate::mesh::Node>) {
        let (motions, nodes) = fixture();
        let mut animator = Animator::new(motions, &nodes).unwrap();
        let motions = (0..48)
            .map(|_| {
                let mut motion = fixture().0.remove(0);
                motion.animation.duration = 3.;
                motion
            })
            .collect();
        animator.set_stunts(motions, vec![true; 48]).unwrap();
        animator
            .set_landing_motions(
                (0..2)
                    .map(|_| {
                        let mut motion = fixture().0.remove(0);
                        motion.animation.duration = 2.;
                        motion
                    })
                    .collect(),
            )
            .unwrap();
        (animator, nodes)
    }
    fn stunt_input() -> crate::stunt::Input {
        crate::stunt::Input {
            modifiers: 1,
            axes: [0., -1.],
            horizontal_speed_squared: 100.,
            ..Default::default()
        }
    }
    #[test]
    fn scoring_advances_once_through_both_control_paths_and_rejected_chain_keeps_event() {
        let (mut animator, nodes) = stunt_animator();
        animator
            .set_scoring_mode(Some(crate::stunt_score::FREESTYLE_MODE))
            .unwrap();
        animator
            .step_controls(&nodes, input(), stunt_input(), 0.125)
            .unwrap();
        assert_eq!(animator.score_frame().current.unwrap().id, 0);
        animator
            .step_controls_with_buttons(&nodes, input(), stunt_input(), 0, 0.125)
            .unwrap();
        assert_eq!(animator.score_frame().chain_seconds, 0.125);
        animator
            .step_controls(&nodes, input(), stunt_input(), 0.125)
            .unwrap();
        assert_eq!(animator.score_frame().chain_seconds, 0.25);
        let mut s = stunt_input();
        s.modifiers = 0;
        animator.step_controls(&nodes, input(), s, 0.125).unwrap();
        s.modifiers = 2;
        animator.step_controls(&nodes, input(), s, 0.125).unwrap();
        assert!(!animator.stunt_frame().unwrap().started);
        assert_eq!(animator.stunt_frame().unwrap().id, 0);
        assert_eq!(animator.score_frame().current.unwrap().id, 0);
        let notifications = animator.take_score_notifications();
        assert_eq!(notifications.len(), 5);
        assert!(matches!(
            notifications[0],
            crate::stunt_score::Notification::Started { id: 0, .. }
        ));
        assert!(notifications[1..].iter().all(|event| matches!(
            event,
            crate::stunt_score::Notification::DisplayUpdated { id: 0, .. }
        )));
        assert!(animator.take_score_notifications().is_empty());
    }
    #[test]
    fn scoring_survives_atv_reset_and_session_restart_clears_history() {
        let (mut animator, nodes) = stunt_animator();
        animator
            .set_scoring_mode(Some(crate::stunt_score::FREESTYLE_MODE))
            .unwrap();
        animator
            .step_controls(&nodes, input(), stunt_input(), 0.125)
            .unwrap();
        let mut s = stunt_input();
        s.modifiers = 0;
        for _ in 0..30 {
            animator.step_controls(&nodes, input(), s, 0.125).unwrap();
        }
        assert!(animator.stunt_frame().is_none());
        assert_eq!(animator.score_frame().pending_points, 500.);
        s.grounded = true;
        for _ in 0..5 {
            animator.step_controls(&nodes, input(), s, 0.125).unwrap();
        }
        assert_eq!(animator.score_frame().committed_points, 500.);
        animator.reset(&nodes).unwrap();
        assert_eq!(animator.score_frame().committed_points, 500.);
        assert_eq!(animator.score.repetition_weight(0), Some(0.85));
        animator.reset_score_session();
        assert_eq!(animator.score_frame().committed_points, 0.);
        assert_eq!(animator.score.repetition_weight(0), Some(1.));
    }
    #[test]
    fn wheelie_credit_preserves_fraction_and_does_not_change_pending_stunt_chain() {
        let (mut animator, nodes) = stunt_animator();
        animator
            .set_scoring_mode(Some(crate::stunt_score::FREESTYLE_MODE))
            .unwrap();
        animator
            .step_controls(&nodes, input(), stunt_input(), 0.125)
            .unwrap();
        animator.credit_wheelie_points(10.25).unwrap();
        animator.credit_wheelie_points(5.125).unwrap();
        assert_eq!(animator.score_frame().committed_points, 15.375);
        assert_eq!(animator.score_frame().chain_count, 1);
        assert_eq!(animator.score_frame().current.unwrap().id, 0);
        assert_eq!(animator.score.repetition_weight(0), Some(0.85));
        animator.abort_score_chain().unwrap();
        animator.reset(&nodes).unwrap();
        //0024d690 quantizes the total even on an aborted regular stunt chain.
        assert_eq!(animator.score_frame().committed_points, 15.);
        animator.reset_score_session();
        assert_eq!(animator.score_frame().committed_points, 0.);
    }
    fn wheelie_input(enabled: bool) -> crate::stunt_score::WheelieInput {
        crate::stunt_score::WheelieInput {
            grounded: [false, false, true, true],
            pitch: 0.7,
            context_allows: true,
            scoring_enabled: enabled,
        }
    }
    #[test]
    fn accepted_stunt_credits_then_clears_wheelie_before_allocating_event() {
        let (mut animator, nodes) = stunt_animator();
        animator
            .set_scoring_mode(Some(crate::stunt_score::FREESTYLE_MODE))
            .unwrap();
        animator.update_wheelie(1.125, wheelie_input(true)).unwrap();
        animator.update_wheelie(0.125, wheelie_input(true)).unwrap();
        animator.take_score_notifications();
        animator
            .step_controls(&nodes, input(), stunt_input(), 0.125)
            .unwrap();
        let events = animator.take_score_notifications();
        assert!(matches!(
            events.as_slice(),
            [
                crate::stunt_score::Notification::Wheelie {
                    event: crate::stunt_score::WheelieNotification::Finished { points: 7.25 }
                },
                crate::stunt_score::Notification::WheelieClearedForStunt,
                crate::stunt_score::Notification::Started { id: 0, .. }
            ]
        ));
        assert_eq!(animator.score_frame().committed_points, 7.25);
        assert!(!animator.wheelie_frame().eligible);
        assert_eq!(animator.wheelie_frame().grace, 0.5);
    }
    #[test]
    fn wreck_aborts_stunt_before_wheelie_failure_and_session_reset_reinitializes() {
        let (mut animator, nodes) = stunt_animator();
        animator
            .set_scoring_mode(Some(crate::stunt_score::FREESTYLE_MODE))
            .unwrap();
        animator
            .step_controls(&nodes, input(), stunt_input(), 0.125)
            .unwrap();
        animator.update_wheelie(1.125, wheelie_input(true)).unwrap();
        animator.update_wheelie(0.125, wheelie_input(true)).unwrap();
        animator.take_score_notifications();
        animator.abort_wreck_scores().unwrap();
        let events = animator.take_score_notifications();
        assert!(matches!(
            events.as_slice(),
            [
                crate::stunt_score::Notification::StuntFinished { success: false, .. },
                crate::stunt_score::Notification::ChainFinished { success: false, .. },
                crate::stunt_score::Notification::Wheelie {
                    event: crate::stunt_score::WheelieNotification::Failed
                }
            ]
        ));
        assert_eq!(animator.score_frame().committed_points, 0.);
        assert_eq!(animator.wheelie_frame().points, 7.25);
        assert!(animator.wheelie_frame().active_this_tick);
        assert_eq!(animator.wheelie_frame().elapsed, 0.);
        animator.reset_score_session();
        assert_eq!(animator.wheelie_frame().points, 1.);
        assert!(!animator.wheelie_frame().active_this_tick);
    }
    #[test]
    fn generic_pose_reset_preserves_wheelie_fields_until_actual_wreck_or_new_session() {
        let (mut animator, nodes) = stunt_animator();
        animator.update_wheelie(1.125, wheelie_input(true)).unwrap();
        animator.update_wheelie(0.125, wheelie_input(true)).unwrap();
        animator.take_score_notifications();
        let before = animator.wheelie_frame();
        animator.reset(&nodes).unwrap();
        let after = animator.wheelie_frame();
        assert_eq!(after.elapsed, before.elapsed);
        assert_eq!(after.points, before.points);
        assert_eq!(after.grace, before.grace);
        assert!(after.armed && after.eligible && after.active_this_tick);
        assert!(animator.take_score_notifications().is_empty());
    }
    #[test]
    fn baseline_wheelie_updates_duration_without_points_or_hud_notifications() {
        let (mut animator, _) = stunt_animator();
        for _ in 0..16 {
            animator
                .update_wheelie(
                    0.125,
                    wheelie_input(crate::stunt_score::DEFAULT_WHEELIE_SCORING),
                )
                .unwrap();
        }
        assert_eq!(animator.wheelie_frame().elapsed, 2.);
        assert!(animator.wheelie_frame().armed);
        assert!(!animator.wheelie_frame().eligible);
        assert!(animator.take_score_notifications().is_empty());
        assert_eq!(animator.score_frame().committed_points, 0.);
    }
    #[test]
    fn landing_phase_and_snapshot_flags_match_original_instructions() {
        // Five runs through 002d8e50 mode6. Native pose sampler and snapshot
        // call were hooked; all phase/flag/weight instructions were executed.
        for (previous, force, suppress, phase, weight) in [
            (Mode::Air, false, false, 0x3d000000, 0x3f800000),
            (Mode::Landing, false, false, 0x3f080000, 0x3f000000),
            (Mode::Landing, true, false, 0x3d000000, 0x3f800000),
            (Mode::Air, false, true, 0x3f080000, 0x3f000000),
            (Mode::Landing, true, true, 0x3f080000, 0x3f000000),
        ] {
            let (mut animator, nodes) = stunt_animator();
            animator.state.previous_mode = previous;
            animator.state.snapshot_weight = 0.75;
            animator.landing_phase = 0.5;
            animator.request_landing(Landing::Front).unwrap();
            let mut i = input();
            i.force_snapshot = force;
            i.suppress_snapshot = suppress;
            assert_eq!(
                animator
                    .step_controls(&nodes, i, crate::stunt::Input::default(), 0.0625)
                    .unwrap(),
                Mode::Landing
            );
            assert_eq!(animator.landing_phase.to_bits(), phase);
            assert_eq!(animator.state.snapshot_weight.to_bits(), weight);
            assert_eq!(
                animator.state.previous_mode,
                if suppress { previous } else { Mode::Landing }
            );
        }
    }
    #[test]
    fn landing_preempts_stunt_pauses_clock_and_repeated_requests_do_not_restart() {
        let (mut animator, nodes) = stunt_animator();
        animator
            .step_controls(&nodes, input(), stunt_input(), 0.125)
            .unwrap();
        let time = animator.stunt_frame().unwrap().main_time;
        animator.request_landing(Landing::Front).unwrap();
        for frame in 0..16 {
            assert_eq!(
                animator
                    .step_controls(&nodes, input(), stunt_input(), 0.125)
                    .unwrap(),
                Mode::Landing
            );
            assert_eq!(animator.stunt_frame().unwrap().main_time, time);
            assert_eq!(animator.landing_phase, (frame + 1) as f32 / 16.);
            if frame == 4 {
                animator.request_landing(Landing::Vertical).unwrap();
            }
        }
        assert_eq!(
            animator
                .step_controls(&nodes, input(), stunt_input(), 0.125)
                .unwrap(),
            Mode::Stunt
        );
        assert!(animator.stunt_frame().unwrap().main_time > time);
        assert_eq!(animator.landing_frame(), None);
        animator.request_landing(Landing::Front).unwrap();
        assert_eq!(
            animator
                .step_controls(&nodes, input(), stunt_input(), 0.125)
                .unwrap(),
            Mode::Landing
        );
        assert_eq!(animator.landing_phase, 0.0625);
        animator.reset(&nodes).unwrap();
        assert_eq!(animator.landing_frame(), None);
    }
    #[test]
    fn stunt_snapshot_suppression_defers_transition_and_force_saves_current_pose() {
        let (mut animator, nodes) = stunt_animator();
        let mut i = input();
        i.suppress_snapshot = true;
        animator
            .step_controls(&nodes, i, stunt_input(), 0.125)
            .unwrap();
        assert_eq!(animator.state.previous_mode, Mode::Air);
        assert_eq!(animator.state.snapshot_weight, 0.);
        i.suppress_snapshot = false;
        animator
            .step_controls(&nodes, i, stunt_input(), 0.125)
            .unwrap();
        assert_eq!(animator.state.previous_mode, Mode::Stunt);
        assert_eq!(animator.state.snapshot_weight, 1.);
        animator
            .step_controls(&nodes, i, stunt_input(), 0.125)
            .unwrap();
        assert_eq!(animator.state.snapshot_weight, 0.5);
        i.force_snapshot = true;
        animator
            .step_controls(&nodes, i, stunt_input(), 0.125)
            .unwrap();
        assert_eq!(animator.state.snapshot_weight, 1.);
    }
    #[test]
    fn hard_landing_uses_collision_delta_with_native_strict_thresholds_and_precedence() {
        let config = LandingConfig {
            wreck_impact: [19., 84., 21.],
            vertical: 60.,
            longitudinal: 15.,
        };
        assert_eq!(config.evaluate([0., 60., -15.], 1.), None);
        assert_eq!(
            config.evaluate([0., 60.01, 0.], 1.),
            Some(Landing::Vertical)
        );
        assert_eq!(config.evaluate([0., 70., -15.01], 1.), Some(Landing::Front));
        assert_eq!(config.evaluate([19.01, 70., -16.], 1.), None);
        assert_eq!(config.evaluate([0., 85., -16.], 1.), None);
        assert_eq!(config.evaluate([0., 85., -16.], 1.25), Some(Landing::Front));
        assert_eq!(config.evaluate([0., 70., -21.01], 1.), None);
    }
    #[test]
    fn crouch_matches_original_instruction_sequence() {
        // Synthetic local velocities through001e6b88..001e6d58. Only the
        // world-to-local transform was hooked; all blend math ran as EE ops.
        let cases = [
            (0.0, 0x3ecccccc),
            (0.0, 0x3e999998),
            (38.0, 0x3ecccccc),
            (-20.0, 0x3f000000),
            (-20.0, 0x3f19999a),
            (0.0, 0x3f333334),
            (0.0, 0x3f4cccce),
            (50.0, 0x3f666667),
            (0.0, 0x3f733334),
        ];
        let mut crouch = Crouch::default();
        for (velocity, expected) in cases {
            crouch.step(velocity, 1.0 / 60.0).unwrap();
            assert_eq!(crouch.blend.to_bits(), expected);
        }
    }
    #[test]
    fn crouch_settles_and_reacts_to_vertical_motion_without_changing_physics() {
        let mut crouch = Crouch::default();
        for _ in 0..300 {
            crouch.step(0.0, 1.0 / 60.0).unwrap();
        }
        assert!((crouch.blend - 0.2).abs() < 1e-5);
        let settled = crouch.blend;
        crouch.step(38.0, 1.0 / 60.0).unwrap();
        assert!(crouch.blend > settled);
        assert!(crouch.blend - settled <= 0.100001);
        for _ in 0..300 {
            crouch.step(0.0, 1.0 / 60.0).unwrap();
        }
        assert!((crouch.blend - 0.2).abs() < 1e-5);
        assert!(crouch.step(f32::NAN, 0.01).is_err());
    }
    fn fixture() -> (Vec<Motion>, Vec<crate::mesh::Node>) {
        use crate::animation::{Animation, RotationKey, Sampling, Track, TranslationKey};
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        let nodes = ["Root", "Head", "Hand"]
            .iter()
            .enumerate()
            .map(|(i, name)| crate::mesh::Node {
                name: (*name).into(),
                parent: if i == 0 { -1 } else { 0 },
                local_matrix: identity,
                world_matrix: identity,
                bounds: [0.; 7],
            })
            .collect();
        let motions = Clip::ALL
            .iter()
            .map(|clip| Motion {
                sampling: Sampling::default(),
                animation: Animation {
                    version: 2,
                    frame_count: 2,
                    duration: 1.,
                    bone_names: vec![],
                    translation_kind: 0,
                    translations: if *clip == Clip::LookLr {
                        vec![]
                    } else {
                        vec![Track {
                            bone: 0,
                            keys: vec![TranslationKey {
                                time: 0.,
                                value: [0.; 3],
                                tangents: None,
                            }],
                        }]
                    },
                    rotations: (0..3)
                        .filter(|bone| *clip != Clip::LookLr || *bone == 1)
                        .map(|bone| Track {
                            bone,
                            keys: vec![RotationKey {
                                time: 0.,
                                packed_angle: if *clip == Clip::LookLr { 1024 } else { 0 },
                                packed_axis: [0, 32767, 0],
                                compressed_axis: None,
                            }],
                        })
                        .collect(),
                },
            })
            .collect();
        (motions, nodes)
    }
    #[test]
    fn head_overlay_is_masked_and_reset_restores_initial_pose() {
        let (motions, nodes) = fixture();
        let mut animator = Animator::new(motions, &nodes).unwrap();
        let initial = animator.pose(&nodes).unwrap();
        let (_, first) = animator.step(&nodes, input(), 1. / 60.).unwrap();
        assert_eq!(
            first.local_matrices, initial.local_matrices,
            "mode change snapshots before applying look"
        );
        let (_, next) = animator.step(&nodes, input(), 0.3).unwrap();
        assert_eq!(next.local_matrices[0], initial.local_matrices[0]);
        assert_eq!(next.local_matrices[2], initial.local_matrices[2]);
        assert!(next.local_matrices[1][0].abs() < 0.00001);
        animator.reset(&nodes).unwrap();
        assert_eq!(
            animator.pose(&nodes).unwrap().local_matrices,
            initial.local_matrices
        );
    }
    #[test]
    fn indexed_blends_reject_reordered_controllers_but_single_reverse_allows_them() {
        let (mut motions, nodes) = fixture();
        motions[Clip::Reverse as usize]
            .animation
            .rotations
            .swap(0, 1);
        assert!(Animator::new(motions, &nodes).is_ok());
        let (mut motions, nodes) = fixture();
        motions[Clip::LeanFwd as usize]
            .animation
            .rotations
            .swap(0, 1);
        assert!(Animator::new(motions, &nodes).is_err());
    }
    #[test]
    fn transmitted_recipe_reconstructs_steady_normal_poses_without_advancing_scores() {
        let (motions, nodes) = fixture();
        let mut source = Animator::new(motions, &nodes).unwrap();
        let mut remote = source.fork_normal(&nodes).unwrap();
        for (reverse, contact, speed) in [
            (false, true, 0.),
            (false, true, 50.),
            (false, false, 20.),
            (true, true, 2.),
        ] {
            let mut controls = input();
            controls.reverse_gear = reverse;
            controls.wheel_contact = contact;
            controls.local_forward_speed = speed;
            controls.axes = [0.35, -0.2];
            controls.crouch = 0.7;
            source.step(&nodes, controls, 0.25).unwrap();
            let (_, expected) = source.step(&nodes, controls, 0.25).unwrap();
            let actual = remote.present(&nodes, source.recipe(), 0.25).unwrap();
            for (a, b) in actual
                .world_matrices
                .iter()
                .flatten()
                .zip(expected.world_matrices.iter().flatten())
            {
                assert!((a - b).abs() < 0.0001);
            }
            assert_eq!(remote.score_frame().committed_points, 0.);
        }
        let mut invalid = source.recipe();
        invalid.clips[0] = 255;
        assert!(remote.present(&nodes, invalid, 0.01).is_err());
    }
    fn input() -> Input {
        Input {
            axes: [0.0; 2],
            local_forward_speed: 0.0,
            stand_up_speed: 10.0,
            wheel_contact: true,
            reverse_gear: false,
            reverse_override: false,
            crouch: 0.5,
            look_phase: 0.5,
            force_snapshot: false,
            suppress_snapshot: false,
        }
    }
    #[test]
    fn transition_saves_pose_then_fades_without_restarting() {
        let mut state = State::default();
        let p = state.step(input(), 1.0 / 60.0).unwrap();
        assert!(p.snapshot);
        assert_eq!(p.snapshot_weight, 1.0);
        let p = state.step(input(), 0.125).unwrap();
        assert!(!p.snapshot);
        assert_eq!(p.snapshot_weight, 0.5);
        assert_eq!(p.head_look_phase, Some(0.5));
        let p = state.step(input(), 0.5).unwrap();
        assert_eq!(p.snapshot_weight, 0.0);
    }
    #[test]
    fn reverse_precedes_air_and_lean_threshold_is_inclusive() {
        let mut state = State::default();
        let mut i = input();
        i.local_forward_speed = 10.0;
        assert_eq!(state.step(i, 0.0).unwrap().mode, Mode::Lean);
        i.wheel_contact = false;
        assert_eq!(state.step(i, 0.0).unwrap().mode, Mode::Air);
        i.reverse_override = true;
        assert_eq!(state.step(i, 0.0).unwrap().mode, Mode::Reverse);
    }
    #[test]
    fn four_clips_preserve_pair_order_and_shared_absolute_time() {
        let mut i = input();
        i.local_forward_speed = 20.0;
        i.axes = [-0.5, -0.25];
        i.crouch = 0.75;
        let p = State::default().step(i, 0.0).unwrap();
        assert_eq!(
            p.blend,
            Blend::Four {
                clips: [Clip::LeanMid, Clip::LeanFwd, Clip::LeanMidD, Clip::LeanFwdD],
                first_weight: 0.75,
                pair_weight: 0.75
            }
        );
        assert!(p.samples(2.0).unwrap().iter().all(|s| s.time == 1.5));
    }
    #[test]
    fn suppressed_transition_does_not_consume_mode_change() {
        let mut state = State::default();
        let mut i = input();
        i.suppress_snapshot = true;
        assert!(!state.step(i, 0.0).unwrap().snapshot);
        assert_eq!(state.previous_mode, Mode::Air);
        i.suppress_snapshot = false;
        assert!(state.step(i, 0.0).unwrap().snapshot);
    }
}
