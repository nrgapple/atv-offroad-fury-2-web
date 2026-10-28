//! Keyframed TOY actors: the `[AnimationN]` records of a venue's common and
//! mode TOYs with `AnimationType=KeyFramed` (crows, fish, windmills, blimps,
//! twisters...), their `.mcf` motion catalogs and playback clocks
//! (research/AMBIENT_ACTORS.md).
use crate::Result;
use crate::games::Toy;
use crate::math::Det;

/// One `[AnimationN]` sequence (`NumberInSequence%d`, `MotionSequence%d`,
/// `ProbabilityList%d`, `BlendSequence%d`, `LoopSequence%d`,
/// `SequenceActive%d`, `NameOfSequence%d`).
#[derive(Clone, Debug, PartialEq)]
pub struct Sequence {
    pub name: String,
    /// 1-based `MotionToPlay` indices played in order.
    pub motions: Vec<i32>,
    /// Per entry percentage (100 when absent).
    pub probabilities: Vec<i32>,
    pub blend: bool,
    pub looping: bool,
    pub active: bool,
}

/// One `[AnimationNLoopSoundM]` of a keyframed record (`NumberOfLoopingSounds`,
/// read by the shared loop-sound list reader `00205640`, format
/// `%sLoopSound%d`): a sample attached to a model node.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopSound {
    /// `SoundResourceFile` as authored (`soultree\\b\\blimp.vag`).
    pub sample: String,
    pub min_distance: f32,
    pub max_distance: f32,
    pub looping: bool,
    pub start_disabled: bool,
    /// `AttachToObject`: a PSG node name (`blimp`, `p_plane`, `chopper`).
    pub attach: String,
}

/// A keyframed `[AnimationN]` record.
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    /// N of `[AnimationN]`.
    pub index: usize,
    pub reference_name: String,
    /// The `.mcf` file name, lowercase, without directories.
    pub mcf: String,
    pub offset: [f32; 3],
    /// `Look` (the root's +Z axis); absent or zero selects `Yaw`/`Pitch`/
    /// `Roll` (degrees) instead.
    pub look: Option<[f32; 3]>,
    pub up: [f32; 3],
    pub yaw_pitch_roll: [f32; 3],
    pub use_lighting: bool,
    pub use_collision: bool,
    /// `MotionToPlay%d`: `[MOTION n] NAME`s of the MCF.
    pub motions: Vec<String>,
    pub sequences: Vec<Sequence>,
    /// `NumberOfLoopingSounds` sounds that follow the posed model.
    pub loop_sounds: Vec<LoopSound>,
}

impl Definition {
    /// The actor root (`0023cdc0`): `Offset` as its translation and the
    /// `00164dd0` basis from `Look`/`Up`. A zero or absent `Look` takes
    /// the `0x141150` basis from `Yaw`/`Pitch`/`Roll` instead (identity when
    /// all are zero, as in every shipped record without `Look`).
    /// `0023cdc0` → `0011e1d0`: a `UseCollision` actor loads
    /// `anim\<MCF stem>.col` (`%s.col`, with the `geometry\` prefix
    /// cleared), else none.
    pub fn collision_path(&self) -> Option<String> {
        if !self.use_collision || self.mcf.is_empty() {
            return None;
        }
        let name = self.mcf.replace('\\', "/").to_ascii_lowercase();
        let name = name.rsplit('/').next().unwrap_or(&name);
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        Some(format!("anim/{stem}.col"))
    }

    /// The collision body's world transform (`0011eca0` → `00330420`): the
    /// COL attaches to the model's engine root (`00330720` stores the node
    /// passed by `0011e1d0`), which carries only Offset/Look/Up. The MOT
    /// channels address nodes below that root, so the body follows the
    /// placement, not the animation, and its velocity history stays zero.
    pub fn collision_matrix(&self, local: &[f32; 12]) -> Result<[f32; 16]> {
        let root = self.matrix()?;
        let local = [
            local[0], local[1], local[2], 0., local[3], local[4], local[5], 0., local[6], local[7],
            local[8], 0., local[9], local[10], local[11], 1.,
        ];
        let mut out = [0.; 16];
        for c in 0..4 {
            for r in 0..4 {
                out[c * 4 + r] = (0..4).map(|k| root[k * 4 + r] * local[c * 4 + k]).sum();
            }
        }
        Ok(out)
    }

    pub fn matrix(&self) -> Result<[f32; 16]> {
        match self.look.filter(|l| *l != [0.; 3]) {
            Some(look) => crate::scene::placement_matrix(self.offset, look, self.up),
            None => {
                let (look, up) = yaw_pitch_roll(self.yaw_pitch_roll);
                crate::scene::placement_matrix(self.offset, look, up)
            }
        }
    }

    /// The starting sequence (`0023cdc0`): the first active one, else the
    /// first; and whether it plays (an inactive one holds its first
    /// motion's t=0 pose until a trigger starts it).
    pub fn start_sequence(&self) -> Option<(&Sequence, bool)> {
        match self.sequences.iter().find(|s| s.active) {
            Some(s) => Some((s, true)),
            None => self.sequences.first().map(|s| (s, false)),
        }
    }
}

/// `0x141150`: from +Z look and +Y up, roll turns Up about Z, then pitch
/// about X and yaw about Y turn both (degrees). The rotation senses are the
/// standard right-handed ones; no shipped record sets these keys.
fn yaw_pitch_roll([yaw, pitch, roll]: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (yaw, pitch, roll) = (yaw.to_radians(), pitch.to_radians(), roll.to_radians());
    let about_z = |v: [f32; 3], a: f32| {
        let (s, c) = a.sin_cos_d();
        [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]]
    };
    let about_x = |v: [f32; 3], a: f32| {
        let (s, c) = a.sin_cos_d();
        [v[0], v[1] * c - v[2] * s, v[1] * s + v[2] * c]
    };
    let about_y = |v: [f32; 3], a: f32| {
        let (s, c) = a.sin_cos_d();
        [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c]
    };
    let up = about_y(about_x(about_z([0., 1., 0.], roll), pitch), yaw);
    let look = about_y(about_x([0., 0., 1.], pitch), yaw);
    (look, up)
}

fn flag(toy: &Toy, section: &str, key: &str, default: bool) -> bool {
    toy.get(section, key)
        .and_then(|v| crate::train::integer(v).ok())
        .map_or(default, |v| v != 0)
}

fn int(toy: &Toy, section: &str, key: &str, default: i32) -> i32 {
    toy.get(section, key)
        .and_then(|v| crate::train::integer(v).ok())
        .unwrap_or(default)
}

/// A comma-separated integer list (`strtok(",")` + `atoi`), padded with
/// `default` to `count` entries; an empty value is `count` defaults.
fn list(value: Option<&str>, count: usize, default: i32) -> Vec<i32> {
    let mut out: Vec<i32> = match value {
        Some(v) if !v.trim().is_empty() => v
            .split(',')
            .filter(|t| !t.is_empty())
            .take(count)
            .map(atoi)
            .collect(),
        _ => vec![default; count],
    };
    // Entries the list does not reach keep their zeroed allocation.
    out.resize(count, 0);
    out
}

/// C `atoi`: leading blanks, an optional sign, then decimal digits.
fn atoi(text: &str) -> i32 {
    let t = text.trim_start();
    let (sign, digits) = match t.as_bytes().first() {
        Some(b'-') => (-1i64, &t[1..]),
        Some(b'+') => (1, &t[1..]),
        _ => (1, t),
    };
    let mut v = 0i64;
    for c in digits.bytes().take_while(u8::is_ascii_digit) {
        v = (v * 10 + i64::from(c - b'0')).min(i64::from(i32::MAX) + 1);
    }
    (sign * v).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// The keyframed `[AnimationN]` records of a TOY, in record order.
pub fn parse(text: &str) -> Vec<Definition> {
    let toy = Toy::parse(text);
    let count = int(&toy, "Animations", "NumberOfAnimations", 0).clamp(0, 4096) as usize;
    let mut out = Vec::new();
    for index in 1..=count {
        let section = format!("Animation{index}");
        let keyframed = toy
            .get(&section, "AnimationType")
            .is_some_and(|t| t.eq_ignore_ascii_case("KeyFramed"));
        if !keyframed {
            continue;
        }
        let Some(mcf) = toy.get(&section, "MCF") else {
            continue;
        };
        let mcf = mcf
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(mcf)
            .to_ascii_lowercase();
        let motions = (1..=int(&toy, &section, "NumberOfMotions", 0).clamp(0, 64))
            .map(|n| {
                toy.get(&section, &format!("MotionToPlay{n}"))
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        let sequences = (1..=int(&toy, &section, "NumberOfSequences", 1).clamp(0, 64))
            .map(|n| {
                let entries =
                    int(&toy, &section, &format!("NumberInSequence{n}"), 1).clamp(0, 256) as usize;
                Sequence {
                    name: toy
                        .get(&section, &format!("NameOfSequence{n}"))
                        .unwrap_or("")
                        .to_string(),
                    motions: list(toy.get(&section, &format!("MotionSequence{n}")), entries, 1),
                    probabilities: list(
                        toy.get(&section, &format!("ProbabilityList{n}")),
                        entries,
                        100,
                    ),
                    blend: flag(&toy, &section, &format!("BlendSequence{n}"), true),
                    looping: flag(&toy, &section, &format!("LoopSequence{n}"), true),
                    active: flag(&toy, &section, &format!("SequenceActive{n}"), true),
                }
            })
            .collect();
        out.push(Definition {
            index,
            reference_name: toy.get(&section, "ReferenceName").unwrap_or("").to_string(),
            mcf,
            offset: toy.vec3(&section, "Offset").unwrap_or([0.; 3]),
            look: toy.vec3(&section, "Look"),
            up: toy.vec3(&section, "Up").unwrap_or([0., 1., 0.]),
            yaw_pitch_roll: ["Yaw", "Pitch", "Roll"].map(|k| {
                toy.get(&section, k)
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0.)
            }),
            use_lighting: flag(&toy, &section, "UseLighting", true),
            // `0023bfc0` reads `UseCollision` with default 1 (`001421b0`).
            use_collision: flag(&toy, &section, "UseCollision", true),
            motions,
            sequences,
            loop_sounds: loop_sounds(&toy, &section),
        });
    }
    out
}

/// `[<section>LoopSoundM]` for M in 1..=`NumberOfLoopingSounds`. A sound
/// without a sample or with an invalid distance interval is skipped.
fn loop_sounds(toy: &Toy, section: &str) -> Vec<LoopSound> {
    let count = int(toy, section, "NumberOfLoopingSounds", 0).clamp(0, 32);
    (1..=count)
        .filter_map(|m| {
            let s = format!("{section}LoopSound{m}");
            let sample = toy.get(&s, "SoundResourceFile")?.trim();
            if sample.is_empty() || sample.contains("..") || sample.len() > 260 {
                return None;
            }
            let distance = |key: &str| -> Option<f32> {
                toy.get(&s, key)?
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite())
            };
            let (min_distance, max_distance) = (distance("MinDistance")?, distance("MaxDistance")?);
            if min_distance < 0. || max_distance <= min_distance {
                return None;
            }
            Some(LoopSound {
                sample: sample.to_string(),
                min_distance,
                max_distance,
                looping: flag(toy, &s, "Looping", false),
                start_disabled: flag(toy, &s, "StartDisabled", false),
                attach: toy
                    .get(&s, "AttachToObject")
                    .unwrap_or("body")
                    .trim()
                    .to_string(),
            })
        })
        .collect()
}

/// A TOY `[TriggerN]` with `ActionType=Animation` actions
/// (`AnimationTriggerAction`, `0023dec0`): touching it starts each named
/// actor's first sequence (`0020c910` → `0020d260`). SX10's stadium roofs
/// are the only shipped users.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationTrigger {
    pub name: String,
    pub position: [f32; 3],
    /// `Box` half extents about `position`.
    pub extents: [f32; 3],
    /// `StartEnabled` (default 1, `0023ff80`).
    pub enabled: bool,
    pub one_time: bool,
    /// `RandomTriggerPercent` (default 100, `0023ff80`).
    pub percent: f32,
    /// `AnimationNameRef`s: the actors' `ReferenceName`s.
    pub targets: Vec<String>,
    /// Whether the rider touched the box on the previous step.
    pub inside: bool,
}

impl AnimationTrigger {
    /// One step of contact: on entering the box an enabled trigger rolls
    /// `RandomTriggerPercent` with `random` (0..=0x7fff_ffff) and fires on
    /// success; a fired `OneTimeOnly` trigger disables itself. Returns
    /// whether it fired. Rolling once per entry is an inference; the
    /// original roll site was not decompiled (research/AMBIENT_ACTORS.md).
    pub fn contact(&mut self, point: [f32; 3], random: impl FnOnce() -> u32) -> bool {
        let inside = (0..3).all(|i| (point[i] - self.position[i]).abs() <= self.extents[i]);
        let entered = inside && !self.inside;
        self.inside = inside;
        if !entered || !self.enabled {
            return false;
        }
        let roll = random() as f32 / 0x8000_0000u32 as f32 * 100.;
        if roll >= self.percent {
            return false;
        }
        if self.one_time {
            self.enabled = false;
        }
        true
    }
}

/// The TOY's triggers that start actors (`[Triggers] NumberOfTriggers`,
/// only the first `NumberOfActions` actions of each are read).
pub fn animation_triggers(text: &str) -> Vec<AnimationTrigger> {
    let toy = Toy::parse(text);
    let count = int(&toy, "Triggers", "NumberOfTriggers", 0).clamp(0, 512);
    (1..=count)
        .filter_map(|n| {
            let section = format!("Trigger{n}");
            let position = toy.vec3(&section, "Position")?;
            let actions = int(&toy, &section, "NumberOfActions", 0).clamp(0, 16);
            let targets: Vec<String> = (1..=actions)
                .map(|a| format!("Trigger{n}Action{a}"))
                .filter(|action| {
                    toy.get(action, "ActionType")
                        .is_some_and(|k| k.trim().eq_ignore_ascii_case("Animation"))
                })
                .filter_map(|action| toy.get(&action, "AnimationNameRef"))
                .map(|name| name.trim().trim_matches('"').to_string())
                .collect();
            (!targets.is_empty()).then(|| AnimationTrigger {
                name: toy.get(&section, "Name").unwrap_or("").trim().to_string(),
                position,
                extents: toy.vec3(&section, "Box").unwrap_or([0.; 3]),
                enabled: flag(&toy, &section, "StartEnabled", true),
                one_time: flag(&toy, &section, "OneTimeOnly", false),
                percent: toy
                    .get(&section, "RandomTriggerPercent")
                    .and_then(|v| v.trim().parse::<f32>().ok())
                    .filter(|v| v.is_finite())
                    .unwrap_or(100.),
                targets,
                inside: false,
            })
        })
        .collect()
}

/// One `[MOTION n]` of an `.mcf` motion catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct McfMotion {
    /// `NAME`: the `.mot` stem.
    pub name: String,
    pub fps: Option<f32>,
    pub frame_count: Option<i32>,
    pub looping: bool,
}

/// An `.mcf` motion catalog: the model (`SLTFile`, whose stem names the
/// PSG) and its motions.
#[derive(Clone, Debug, PartialEq)]
pub struct Mcf {
    pub model: String,
    pub motions: Vec<McfMotion>,
}

impl Mcf {
    pub fn parse(text: &str) -> Result<Self> {
        let toy = Toy::parse(text);
        let slt = toy
            .get("General info", "SLTFile")
            .ok_or("MCF has no SLTFile")?;
        let slt = slt.rsplit(['\\', '/']).next().unwrap_or(slt);
        let model = slt
            .strip_suffix(".slt")
            .or_else(|| slt.strip_suffix(".SLT"))
            .unwrap_or(slt)
            .to_ascii_lowercase();
        let count = int(&toy, "General info", "NumberOfMotions", 0).clamp(0, 64);
        let motions = (1..=count)
            .filter_map(|n| {
                let section = format!("MOTION {n}");
                Some(McfMotion {
                    name: toy.get(&section, "NAME")?.to_ascii_lowercase(),
                    fps: toy.get(&section, "FPS").and_then(|v| v.trim().parse().ok()),
                    frame_count: toy
                        .get(&section, "FrameCount")
                        .and_then(|v| crate::train::integer(v).ok()),
                    looping: flag(&toy, &section, "Looping", false),
                })
            })
            .collect();
        Ok(Self { model, motions })
    }

    pub fn motion(&self, name: &str) -> Option<&McfMotion> {
        self.motions
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }
}

/// Playback time within a motion of `duration` seconds after `elapsed`
/// seconds: looping motions wrap, one-shot motions hold their last frame.
pub fn motion_time(elapsed: f64, duration: f32, looping: bool) -> f32 {
    if duration.is_nan() || duration <= 0. || !elapsed.is_finite() || elapsed <= 0. {
        return 0.;
    }
    if looping {
        (elapsed.rem_euclid(f64::from(duration)) as f32).min(duration)
    } else {
        (elapsed as f32).min(duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOY: &str = "[Animations]\nNumberOfAnimations=4\n\
        [Animation1]\nMCF=crow.mcf\nOffset=1,2,3\nLook=1,0,0\nUp=0,1,0\nUseLighting=T\nOneTimeOnly=T\n\
        UseBlendedKeys=T\nNumberOfMotions=1\nAnimationType=KeyFramed\nMotionToPlay1=crow\n\
        NumberInSequence=1\nMotionSequence1=1\nLoopSequence1=T\nSequenceActive1=T\nUseCollision=F\n\
        [Animation2]\nAnimationType=Procedural\nSLT=animgeometry\\engine.slt\n\
        [Animation3]\nMCF=anim\\stdmbroof.mcf\nReferenceName=southroof\nAnimationType=KeyFramed\n\
        NumberOfMotions=2\nMotionToPlay1=stdmbroofopen\nMotionToPlay2=stdmbroofclose\n\
        NumberOfSequences=2\nNameOfSequence1=open\nNumberInSequence1=1\nMotionSequence1=1\n\
        BlendSequence1=F\nProbabilityList1=100\nLoopSequence1=F\nSequenceActive1=F\n\
        NameOfSequence2=close\nNumberInSequence2=2\nMotionSequence2=2,1\nLoopSequence2=no\nSequenceActive2=F\n\
        [Animation4]\nMCF=windmill.mcf\nAnimationType=KeyFramed\nUseCollision=t\n";

    #[test]
    fn use_collision_actors_name_their_anim_col_and_follow_the_placement() {
        let actors = parse(TOY);
        let (crow, roof, windmill) = (&actors[0], &actors[1], &actors[2]);
        assert_eq!(crow.collision_path(), None);
        assert_eq!(roof.collision_path().as_deref(), Some("anim/stdmbroof.col"));
        assert_eq!(
            windmill.collision_path().as_deref(),
            Some("anim/windmill.col")
        );
        // An identity COL local leaves the placement (Offset/Look/Up).
        let identity = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
        assert_eq!(
            crow.collision_matrix(&identity).unwrap(),
            crow.matrix().unwrap()
        );
        // A local translation composes inside the placement basis.
        let shifted = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 2.];
        let m = crow.collision_matrix(&shifted).unwrap();
        let p = crow.matrix().unwrap();
        for i in 0..3 {
            assert!((m[12 + i] - (p[12 + i] + 2. * p[8 + i])).abs() < 1e-5);
        }
    }

    #[test]
    fn sx10_roof_trigger_reads_only_its_counted_actions_and_fires_on_entry() {
        let toy = "[Triggers]\nNumberOfTriggers=1\n[Trigger1]\nName=openroof\nNumberOfActions=2\n\
            Zone=1\nPosition=340,105,225\nBox=40,25,30\nStartEnabled = TRUE\nOneTimeOnly = TRUE\n\
            RandomTriggerPercent = 15.000000\n[Trigger1Action1]\nActionType=Animation\n\
            AnimationNameRef=southroof\n[Trigger1Action2]\nActionType=Animation\nAnimationNameRef=northroof\n\
            [Trigger1Action3]\nActionType = EnableTrigger\nTriggerNameRef = closeroof\n\
            [Trigger2]\nName=closeroof\nNumberOfActions=2\nPosition=420,105,680\nBox=30,25,25\n\
            [Trigger2Action1]\nActionType=ChangeAnimSequence\nAnimationNameRef=southroof\n";
        let mut triggers = animation_triggers(toy);
        assert_eq!(
            triggers.len(),
            1,
            "NumberOfTriggers=1 leaves closeroof unread"
        );
        let t = &mut triggers[0];
        assert_eq!(t.targets, ["southroof", "northroof"]);
        assert!(t.enabled && t.one_time && t.percent == 15.);
        let outside = [340., 105., 300.];
        let inside = [370., 120., 200.];
        assert!(!t.contact(outside, || 0));
        // A failed roll (>= 15%) leaves it armed; staying inside rolls no more.
        assert!(!t.contact(inside, || 0x2000_0000));
        assert!(!t.contact(inside, || panic!("no roll while inside")));
        assert!(!t.contact(outside, || 0));
        // The next entry succeeds and disarms the one-time trigger.
        assert!(t.contact(inside, || 0x1000_0000));
        assert!(!t.enabled);
        t.contact(outside, || 0);
        assert!(!t.contact(inside, || 0));
    }

    #[test]
    fn loop_sounds_attach_to_named_nodes() {
        let toy = "[Animations]\nNumberOfAnimations=1\n[Animation1]\nMCF=p_plane.mcf\n\
            AnimationType=KeyFramed\nNumberOfMotions=1\nMotionToPlay1=p_plane\n\n\
            NumberOfLoopingSounds=3\n[Animation1LoopSound1]\nSoundResourceFile=soultree\\p\\plane.vag\n\
            Looping=TRUE\nMaxDistance=850.0\nMinDistance=55.0\nStartDisabled=FALSE\nAttachToObject=p_plane\n\
            [Animation1LoopSound2]\nSoundResourceFile=x.vag\nMaxDistance=10\nMinDistance=20\n\
            [Animation1LoopSound3]\nSoundResourceFile=y.vag\nMaxDistance=20\nMinDistance=0\n";
        let actors = parse(toy);
        assert_eq!(
            actors[0].loop_sounds,
            vec![
                LoopSound {
                    sample: "soultree\\p\\plane.vag".into(),
                    min_distance: 55.,
                    max_distance: 850.,
                    looping: true,
                    start_disabled: false,
                    attach: "p_plane".into(),
                },
                // An inverted interval is skipped; absent keys take defaults.
                LoopSound {
                    sample: "y.vag".into(),
                    min_distance: 0.,
                    max_distance: 20.,
                    looping: false,
                    start_disabled: false,
                    attach: "body".into(),
                },
            ]
        );
        assert!(parse(TOY).iter().all(|a| a.loop_sounds.is_empty()));
    }

    #[test]
    fn keyframed_records_skip_procedural_and_use_reader_defaults() {
        let actors = parse(TOY);
        assert_eq!(actors.len(), 3);
        let crow = &actors[0];
        assert_eq!((crow.index, crow.mcf.as_str()), (1, "crow.mcf"));
        assert_eq!(crow.offset, [1., 2., 3.]);
        assert_eq!(crow.look, Some([1., 0., 0.]));
        let m = crow.matrix().unwrap();
        assert_eq!(
            (&m[8..11], &m[12..15]),
            (&[1., 0., 0.][..], &[1., 2., 3.][..])
        );
        assert_eq!(crow.motions, ["crow"]);
        // NumberOfSequences defaults to one; the unnumbered
        // `NumberInSequence` key is not `NumberInSequence1` (default 1).
        assert_eq!(crow.sequences.len(), 1);
        let s = &crow.sequences[0];
        assert_eq!(
            (s.motions.as_slice(), s.probabilities.as_slice()),
            (&[1][..], &[100][..])
        );
        assert!(s.looping && s.active && s.blend);
        assert!(!crow.use_collision);
        let roof = &actors[1];
        assert_eq!((roof.index, roof.mcf.as_str()), (3, "stdmbroof.mcf"));
        assert_eq!(roof.reference_name, "southroof");
        assert_eq!(roof.sequences.len(), 2);
        assert_eq!(roof.sequences[1].motions, [2, 1]);
        assert_eq!(roof.sequences[1].probabilities, [100, 100]);
        assert!(!roof.sequences[0].looping && !roof.sequences[1].looping);
        // No active sequence: the first holds its t=0 pose.
        let (start, playing) = roof.start_sequence().unwrap();
        assert_eq!((start.name.as_str(), playing), ("open", false));
        assert!(crow.start_sequence().unwrap().1);
        // Absent Offset/Look/Up: origin, +Z look, +Y up.
        let windmill = &actors[2];
        assert_eq!(windmill.offset, [0.; 3]);
        assert_eq!((windmill.look, windmill.up), (None, [0., 1., 0.]));
        let identity = windmill.matrix().unwrap();
        for (i, v) in identity.iter().enumerate() {
            assert!(
                (v - f32::from(u8::from(i % 5 == 0))).abs() < 1e-6,
                "{identity:?}"
            );
        }
        assert!(windmill.use_collision);
        // `UseCollision` absent: the reader's default 1 (`0023bfc0`).
        assert!(roof.use_collision);
        assert!(windmill.motions.is_empty());
    }

    #[test]
    fn yaw_turns_look_about_up() {
        let (look, up) = yaw_pitch_roll([90., 0., 0.]);
        assert!((look[0] - 1.).abs() < 1e-6 && look[2].abs() < 1e-6);
        assert!((up[1] - 1.).abs() < 1e-6);
        let (look, up) = yaw_pitch_roll([0., 0., 0.]);
        assert_eq!((look, up), ([0., 0., 1.], [0., 1., 0.]));
    }

    #[test]
    fn integer_lists_follow_strtok_atoi() {
        assert_eq!(list(Some("2,,1"), 3, 1), [2, 1, 0]);
        assert_eq!(list(Some(" 7x,-3"), 2, 1), [7, -3]);
        assert_eq!(list(None, 2, 100), [100, 100]);
        assert_eq!(list(Some(""), 1, 100), [100]);
    }

    #[test]
    fn mcf_names_model_and_motions() {
        let mcf = Mcf::parse(
            "[General info]\r\nNumberOfMotions=2\r\nContentDirectory=\r\nSLTFile=Monkey02.slt\r\n\
             [MOTION 1]\r\nNAME=Monkey03\r\nFPS=15\r\nFrameCount=216\r\nLooping=yes\r\n\
             [MOTION 2]\r\nNAME=close\r\nLooping=no\r\n",
        )
        .unwrap();
        assert_eq!(mcf.model, "monkey02");
        assert_eq!(mcf.motions.len(), 2);
        let m = mcf.motion("MONKEY03").unwrap();
        assert_eq!(
            (m.fps, m.frame_count, m.looping),
            (Some(15.), Some(216), true)
        );
        assert!(!mcf.motion("close").unwrap().looping);
        assert!(Mcf::parse("[General info]\nNumberOfMotions=0").is_err());
    }

    #[test]
    fn looping_wraps_and_one_shot_holds() {
        assert_eq!(motion_time(0., 4., true), 0.);
        assert_eq!(motion_time(5., 4., true), 1.);
        assert_eq!(motion_time(5., 4., false), 4.);
        assert_eq!(motion_time(-1., 4., true), 0.);
        assert_eq!(motion_time(3., 0., true), 0.);
    }
}
