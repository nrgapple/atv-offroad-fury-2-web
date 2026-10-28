//! Treasure Hunt (event type 12; `QuadzillaEventTreasure.cpp`, vtable
//! `0045e4e0`; research/GAMES.md).
//!
//! Exact-build evidence (private exports, bounded reading):
//!
//! - Setup (`002d0750`): the mode TOY's `[TreasureHunt] NumberOfTargets`
//!   (+0x1d0, the treasures to collect) and `NumberOfGroups`; each
//!   `[TreasureGroup%d]` names `NumberOfTriggers` triggers (`Trigger%d`).
//!   Each named trigger is a one-time `Box=4,6,4` trigger whose actions
//!   hide its `bomb.mcf` animation, run the `Treasure` action and start
//!   the `bomb_explosion.emt` burst.
//! - Start (`002d0590`, `002d11e0`, `002d1280`): every treasure trigger is
//!   disabled and its bomb hidden, then group 1 is shown and enabled and
//!   marked used. Collecting only counts once the event has started
//!   (+0x218 = 1, `002d15c0`).
//! - Collect (`00208b10` → `002d0210` → `002d0d70`): the rider's count
//!   (+0x1c0) rises, the remaining total (+0x1d4) and the group's remaining
//!   (+0x1d8) fall, `iconhit.vag` plays and the standings resort
//!   (`002d0f20`).
//! - Update (`002d0a20`): no treasures left shows "All Treasures
//!   Collected" for 5 s and ends the event (`002d10d0`). A group with none
//!   left opens a 6 s wait ("More Treasures in", Overlays.pb sign 16, value
//!   58); then a random group not yet used (`0036e7a0`, stepping on past
//!   used ones) is shown and enabled.
//! - Restart Game (`002d1450` → `002d1550`): every treasure and count
//!   resets and group 1 is shown again.
use super::{GameKind, Marker, Notice, Rider, Rules, Toy, parse_triggers};

/// `002d0a20`: the wait before the next group, seconds.
pub const NEXT_GROUP_DELAY: f32 = 6.;
/// The ATV body's horizontal reach added to the trigger box (approximate).
pub const REACH: f32 = 3.;

#[derive(Clone, Debug, PartialEq)]
pub struct Treasure {
    pub name: String,
    pub position: [f32; 3],
    pub extents: [f32; 3],
    pub group: usize,
    pub collected_by: Option<usize>,
}

pub struct TreasureHunt {
    pub treasures: Vec<Treasure>,
    /// `NumberOfTargets`.
    pub targets: usize,
    pub groups: usize,
    /// The shown group.
    pub group: usize,
    used: Vec<bool>,
    /// +0x218 = 1.
    pub active: bool,
    /// Seconds left before the next group (+0x220).
    pub wait: f32,
    pub collected: Vec<i32>,
    riders: usize,
    seed: u32,
    rng: u32,
    pub finished: bool,
    /// Collected treasures' trigger names not yet taken.
    triggered: Vec<String>,
}

impl TreasureHunt {
    pub fn new(toy: &Toy, riders: usize) -> crate::Result<Self> {
        let groups = toy
            .int("TreasureHunt", "NumberOfGroups")
            .unwrap_or(1)
            .clamp(1, 64) as usize;
        let triggers = parse_triggers(toy);
        let mut treasures = Vec::new();
        for g in 0..groups {
            let section = format!("TreasureGroup{}", g + 1);
            let count = toy
                .int(&section, "NumberOfTriggers")
                .unwrap_or(0)
                .clamp(0, 256);
            for n in 1..=count {
                let Some(name) = toy.get(&section, &format!("Trigger{n}")) else {
                    continue;
                };
                let Some(t) = triggers
                    .iter()
                    .find(|t| t.name.eq_ignore_ascii_case(name) && t.action("Treasure").is_some())
                else {
                    continue;
                };
                treasures.push(Treasure {
                    name: t.name.clone(),
                    position: t.position,
                    extents: t.extents,
                    group: g,
                    collected_by: None,
                });
            }
        }
        if treasures.is_empty() {
            return Err("the Treasure Hunt course lists no treasures".into());
        }
        let targets = toy
            .int("TreasureHunt", "NumberOfTargets")
            .map_or(treasures.len(), |n| n.clamp(1, 4096) as usize);
        let mut hunt = Self {
            treasures,
            targets,
            groups,
            group: 0,
            used: vec![false; groups],
            active: false,
            wait: 0.,
            collected: vec![0; riders.max(1)],
            riders: riders.max(1),
            seed: 0x1234_5678,
            rng: 0x1234_5678,
            finished: false,
            triggered: Vec::new(),
        };
        hunt.reset();
        Ok(hunt)
    }

    fn reset(&mut self) {
        for t in &mut self.treasures {
            t.collected_by = None;
        }
        self.used.iter_mut().for_each(|u| *u = false);
        self.group = 0;
        self.used[0] = true;
        self.active = false;
        self.wait = 0.;
        self.collected.iter_mut().for_each(|c| *c = 0);
        self.rng = self.seed;
        self.finished = false;
        self.triggered.clear();
    }

    /// Treasures still to collect (+0x1d4).
    pub fn remaining(&self) -> usize {
        self.targets
            .saturating_sub(self.collected.iter().sum::<i32>().max(0) as usize)
    }

    /// The shown group's treasures still standing (+0x1d8).
    pub fn group_remaining(&self) -> usize {
        self.treasures
            .iter()
            .filter(|t| t.group == self.group && t.collected_by.is_none())
            .count()
    }

    fn random(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (self.rng >> 1) & 0x7fff_ffff
    }

    /// `002d0a20`: a random group, stepping past used ones.
    fn next_group(&mut self) -> Option<usize> {
        if self.used.iter().all(|u| *u) {
            return None;
        }
        let r = self.random() as f32 * 4.656_613e-10;
        let mut g = ((self.groups as f32 * r) as usize).min(self.groups - 1);
        while self.used[g] {
            g = (g + 1) % self.groups;
        }
        Some(g)
    }
}

impl Rules for TreasureHunt {
    fn kind(&self) -> GameKind {
        GameKind::TreasureHunt
    }

    fn seed(&mut self, seed: u32) {
        self.seed = seed | 1;
        self.rng = self.seed;
    }

    fn begin(&mut self) {
        self.active = true;
    }

    fn update(&mut self, dt: f32, riders: &[Rider]) -> Vec<Notice> {
        let mut notices = Vec::new();
        if self.finished {
            return notices;
        }
        if self.active && self.wait <= 0. {
            for (r, rider) in riders.iter().enumerate().take(self.riders) {
                for t in &mut self.treasures {
                    if t.group == self.group
                        && t.collected_by.is_none()
                        && touches(t.position, t.extents, rider.position)
                    {
                        t.collected_by = Some(r);
                        self.collected[r] += 1;
                        self.triggered.push(t.name.clone());
                    }
                }
            }
        }
        if self.remaining() == 0 {
            self.finished = true;
            notices.push(Notice {
                text: "All Treasures Collected".into(),
                seconds: super::FINISH_HOLD,
            });
            return notices;
        }
        if self.group_remaining() == 0 {
            if self.wait <= 0. {
                self.wait = NEXT_GROUP_DELAY;
            } else {
                self.wait -= dt;
                if self.wait <= 0. {
                    self.wait = 0.;
                    match self.next_group() {
                        Some(g) => {
                            self.group = g;
                            self.used[g] = true;
                        }
                        // Every group shown with treasures still owed (no
                        // shipped course does this): nothing is left.
                        None => self.finished = true,
                    }
                }
            }
        }
        notices
    }

    fn complete(&self) -> bool {
        self.finished
    }

    fn scores(&self) -> Vec<i32> {
        self.collected.clone()
    }

    fn progress(&self, rider: usize) -> String {
        format!(
            "{} / {}",
            self.collected.get(rider).copied().unwrap_or(0),
            self.targets
        )
    }

    fn countdown(&self) -> Option<f32> {
        (self.wait > 0.).then_some(self.wait)
    }

    fn markers(&self) -> Vec<Marker> {
        if self.wait > 0. {
            return Vec::new();
        }
        self.treasures
            .iter()
            .filter(|t| t.group == self.group && t.collected_by.is_none())
            .map(|t| Marker {
                position: t.position,
                label: "Treasure".into(),
                owner: None,
                score: 0.,
            })
            .collect()
    }

    fn restart(&mut self) {
        self.reset();
    }

    fn take_triggered(&mut self) -> Vec<String> {
        std::mem::take(&mut self.triggered)
    }

    fn diagnostics(&self) -> Vec<(&'static str, f32)> {
        vec![
            ("targets", self.targets as f32),
            ("groups", self.groups as f32),
            ("group", self.group as f32),
            (
                "usedGroups",
                self.used.iter().filter(|u| **u).count() as f32,
            ),
            ("active", f32::from(u8::from(self.active))),
            ("wait", self.wait),
            ("remaining", self.remaining() as f32),
            ("groupRemaining", self.group_remaining() as f32),
        ]
    }
}

/// The trigger box grown by the ATV's reach.
fn touches(position: [f32; 3], extents: [f32; 3], point: [f32; 3]) -> bool {
    (0..3).all(|i| {
        let grow = if i == 1 { REACH * 0.5 } else { REACH };
        (point[i] - position[i]).abs() <= extents[i] + grow
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toy(groups: &[&[&str]]) -> Toy {
        let mut text = format!(
            "[TreasureHunt]\nNumberOfTargets={}\nNumberOfGroups={}\n",
            groups.iter().map(|g| g.len()).sum::<usize>(),
            groups.len()
        );
        let mut triggers = Vec::new();
        for (g, names) in groups.iter().enumerate() {
            text += &format!(
                "[TreasureGroup{}]\nNumberOfTriggers={}\n",
                g + 1,
                names.len()
            );
            for (i, n) in names.iter().enumerate() {
                text += &format!("Trigger{}={n}\n", i + 1);
                triggers.push(*n);
            }
        }
        text += &format!("[Triggers]\nNumberOfTriggers={}\n", triggers.len());
        for (i, n) in triggers.iter().enumerate() {
            let k = i + 1;
            text += &format!(
                "[Trigger{k}]\nName={n}\nNumberOfActions=2\nOneTimeOnly=T\nPosition={},0,0\nBox=4,6,4\n\
                 [Trigger{k}Action1]\nActionType=HideAnimation\n[Trigger{k}Action2]\nActionType=Treasure\n",
                k * 100
            );
        }
        Toy::parse(&text)
    }

    fn at(x: f32) -> Rider {
        Rider {
            position: [x, 0., 0.],
            ..Rider::default()
        }
    }

    #[test]
    fn collects_only_once_started_and_only_the_shown_group() {
        let mut hunt = TreasureHunt::new(&toy(&[&["a", "b"], &["c"]]), 1).unwrap();
        assert_eq!(hunt.markers().len(), 2);
        hunt.update(0.1, &[at(100.)]);
        assert_eq!(hunt.collected[0], 0, "not before the start");
        hunt.begin();
        hunt.update(0.1, &[at(300.)]);
        assert_eq!(hunt.collected[0], 0, "group 2 is hidden");
        assert!(hunt.take_triggered().is_empty(), "no trigger fired yet");
        hunt.update(0.1, &[at(100.)]);
        hunt.update(0.1, &[at(100.)]);
        assert_eq!(
            (hunt.collected[0], hunt.remaining()),
            (1, 2),
            "one time only"
        );
        // The collect runs the trigger's other actions (its explosion) once.
        assert_eq!(hunt.take_triggered(), ["a"]);
        assert!(hunt.take_triggered().is_empty());
        assert_eq!(hunt.progress(0), "1 / 3");
    }

    #[test]
    fn an_emptied_group_waits_six_seconds_then_opens_an_unused_one() {
        let mut hunt = TreasureHunt::new(&toy(&[&["a"], &["b"], &["c"]]), 1).unwrap();
        hunt.seed(7);
        hunt.begin();
        hunt.update(0.1, &[at(100.)]);
        assert_eq!(hunt.countdown(), Some(NEXT_GROUP_DELAY));
        assert!(hunt.markers().is_empty());
        for _ in 0..59 {
            hunt.update(0.1, &[at(0.)]);
        }
        assert!(hunt.countdown().is_some());
        hunt.update(0.11, &[at(0.)]);
        assert_eq!(hunt.countdown(), None);
        assert_ne!(hunt.group, 0);
        let second = hunt.group;
        hunt.update(0.1, &[at((second + 1) as f32 * 100.)]);
        for _ in 0..61 {
            hunt.update(0.1, &[at(0.)]);
        }
        assert!(hunt.group != 0 && hunt.group != second);
        let notices = hunt.update(0.1, &[at((hunt.group + 1) as f32 * 100.)]);
        assert!(hunt.complete());
        assert_eq!(notices[0].text, "All Treasures Collected");
        assert_eq!(hunt.scores(), vec![3]);
    }

    #[test]
    fn restart_resets_everything() {
        let mut hunt = TreasureHunt::new(&toy(&[&["a"], &["b"]]), 2).unwrap();
        hunt.begin();
        hunt.update(0.1, &[at(0.), at(100.)]);
        assert_eq!(hunt.scores(), vec![0, 1]);
        hunt.restart();
        assert_eq!(
            (hunt.scores(), hunt.group, hunt.active),
            (vec![0, 0], 0, false)
        );
        assert_eq!(hunt.markers().len(), 1);
    }

    #[test]
    fn split_screen_players_collect_and_rank_separately() {
        // Two players in one Session (split screen): each treasure counts
        // for whoever touches it, the HUD reads each player's own count,
        // and the standings rank both (ties by stunt score).
        let rules = TreasureHunt::new(&toy(&[&["a", "b", "c"]]), 2).unwrap();
        let mut session = crate::games::Session::new(Box::new(rules), 5, 2);
        session.begin();
        session.tick(0.1, &[at(100.), at(200.)], &[0., 50.]);
        assert_eq!(session.rules.scores(), vec![1, 1]);
        assert_eq!(session.rules.progress(1), "1 / 3");
        // Equal counts: player 2's higher stunt score ranks it first.
        assert_eq!(session.place(1), 1);
        session.tick(0.1, &[at(300.), at(0.)], &[0., 50.]);
        assert_eq!(session.rules.scores(), vec![2, 1]);
        assert_eq!(session.rules.progress(0), "2 / 3");
        assert_eq!((session.place(0), session.place(1)), (1, 2));
        assert_eq!(session.end, Some(crate::games::End::Complete));
        let rows: Vec<_> = session
            .standings()
            .iter()
            .map(|s| (s.rider, s.score))
            .collect();
        assert_eq!(rows, [(0, 2), (1, 1)]);
    }
}
