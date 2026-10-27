//! King of the Hills (event type 9; `QuadzillaEventHillKing.cpp`, vtable
//! `004578b0`; research/GAMES.md).
//!
//! Exact-build evidence (private exports, bounded reading):
//!
//! - Setup (`00220640`, `002203d0`): `[KingOfTheHill] NumberOfHills`
//!   (+0x1c0) hills of 0x20 bytes: owner (-1), the score to beat (0), the
//!   flag object and the marker position, the `Hill%d` trigger's position
//!   raised 12 units. Each hill is a `Box=20,20,20` trigger (Arlington
//!   Stream `15,15,2`) with a `Hilltop` action (`HillNumber`,
//!   `SpecificObject=kohflag.slt`).
//! - Rider state +0x1e4 (`00220150`, the Hilltop callback): entering a
//!   hill with a wheel down makes the rider ready (-1), entering it in the
//!   air does not (-2). Leaving the hill in the air while ready arms that
//!   hill (its number); leaving any other way clears it (-2). An armed
//!   rider keeps the state until landing.
//! - Every step (`00220750`): a rider with a wheel down and no stunt open
//!   who is not ready loses the armed hill (-2).
//! - A finished stunt chain (vtable +0x184, `00221020`): a crash clears the
//!   state; a landed chain scores the sum of its stunts times the chain
//!   multiplier, and `00220b50` captures the armed hill when that beats
//!   the hill's score: the previous owner holds one hill fewer, the rider
//!   one more, the hill keeps the new score and the flag takes the rider's
//!   colour (red, blue, green, yellow at +0x210); the standings resort
//!   (`00220e70`) and "ATV %d captured hill %d" shows for 3 s (`002dc1b0`,
//!   `002e9510`).
//! - Restart Game (`00220d10`): every hill unowned at score 0 with a white
//!   flag, every rider cleared.
//! - The event ends on its time limit; the most hills wins, ties by stunt
//!   score.
use super::{GameKind, Marker, Notice, Rider, Rules, Toy, parse_triggers};
use serde_json::json;

/// `002e9510(3.0, ..)`: the capture sign's seconds.
pub const CAPTURE_SIGN: f32 = 3.;
/// `002203d0`: the hill marker sits this far above the trigger.
pub const MARKER_RAISE: f32 = 12.;
/// The ATV body's horizontal reach added to the trigger box (approximate).
pub const REACH: f32 = 3.;

/// +0x1e4 values.
pub const CLEAR: i32 = -2;
pub const READY: i32 = -1;

#[derive(Clone, Debug, PartialEq)]
pub struct Hill {
    pub number: usize,
    pub position: [f32; 3],
    pub extents: [f32; 3],
    pub owner: Option<usize>,
    pub score: f32,
}

pub struct KingOfTheHill {
    pub hills: Vec<Hill>,
    /// Per rider: CLEAR, READY or the armed hill number.
    pub state: Vec<i32>,
    /// Per rider: the hills touched last step.
    inside: Vec<Vec<bool>>,
    pub held: Vec<i32>,
    riders: usize,
}

impl KingOfTheHill {
    pub fn new(toy: &Toy) -> crate::Result<Self> {
        Self::with_riders(toy, 1)
    }

    pub fn with_riders(toy: &Toy, riders: usize) -> crate::Result<Self> {
        let count = toy.int("KingOfTheHill", "NumberOfHills").unwrap_or(0).clamp(0, 64) as usize;
        let mut hills: Vec<Hill> = parse_triggers(toy)
            .into_iter()
            .filter_map(|t| {
                let number: usize = t.action("Hilltop")?.get("hillnumber")?.trim().parse().ok()?;
                (1..=count).contains(&number).then_some(Hill {
                    number,
                    position: t.position,
                    extents: t.extents,
                    owner: None,
                    score: 0.,
                })
            })
            .collect();
        hills.sort_by_key(|h| h.number);
        hills.dedup_by_key(|h| h.number);
        if hills.is_empty() {
            return Err("the King of the Hills course lists no hills".into());
        }
        let riders = riders.max(1);
        Ok(Self {
            inside: vec![vec![false; hills.len()]; riders],
            hills,
            state: vec![CLEAR; riders],
            held: vec![0; riders],
            riders,
        })
    }

    fn touches(hill: &Hill, point: [f32; 3]) -> bool {
        (0..3).all(|i| {
            let grow = if i == 1 { REACH * 0.5 } else { REACH };
            (point[i] - hill.position[i]).abs() <= hill.extents[i] + grow
        })
    }

    /// `00220b50`: a landed chain's points against the armed hill.
    fn capture(&mut self, rider: usize, points: f32) -> Option<Notice> {
        let armed = self.state[rider];
        if armed <= 0 {
            return None;
        }
        let at = self.hills.iter().position(|h| h.number == armed as usize)?;
        let hill = &mut self.hills[at];
        if hill.score >= points {
            return None;
        }
        if let Some(previous) = hill.owner {
            self.held[previous] -= 1;
        }
        self.held[rider] += 1;
        hill.owner = Some(rider);
        hill.score = points;
        Some(Notice {
            text: format!("ATV {} captured hill {}", rider + 1, hill.number),
            seconds: CAPTURE_SIGN,
        })
    }
}

impl Rules for KingOfTheHill {
    fn kind(&self) -> GameKind {
        GameKind::KingOfTheHills
    }

    fn begin(&mut self) {}

    fn update(&mut self, _dt: f32, riders: &[Rider]) -> Vec<Notice> {
        let mut notices = Vec::new();
        for (r, rider) in riders.iter().enumerate().take(self.riders) {
            // Trigger enter/leave (`00172bc0` with 1, `001728e0` with 0).
            for h in 0..self.hills.len() {
                let now = Self::touches(&self.hills[h], rider.position);
                let was = self.inside[r][h];
                self.inside[r][h] = now;
                if now == was || self.state[r] >= 0 {
                    continue;
                }
                self.state[r] = if now {
                    if rider.grounded { READY } else { CLEAR }
                } else if self.state[r] == READY && !rider.grounded {
                    self.hills[h].number as i32
                } else {
                    CLEAR
                };
            }
            // The finished chain (`00221020`).
            match rider.chain {
                Some((true, points)) => notices.extend(self.capture(r, points)),
                Some((false, _)) => self.state[r] = CLEAR,
                None => {}
            }
            // `00220750`: back on the ground without a stunt open.
            if rider.grounded && !rider.stunting && self.state[r] != READY {
                self.state[r] = CLEAR;
            }
        }
        notices
    }

    fn complete(&self) -> bool {
        false
    }

    fn scores(&self) -> Vec<i32> {
        self.held.clone()
    }

    fn progress(&self, rider: usize) -> String {
        format!(
            "{} / {}",
            self.held.get(rider).copied().unwrap_or(0),
            self.hills.len()
        )
    }

    fn markers(&self) -> Vec<Marker> {
        self.hills
            .iter()
            .map(|h| Marker {
                position: [h.position[0], h.position[1] + MARKER_RAISE, h.position[2]],
                label: format!("Hill {}", h.number),
                owner: h.owner,
            })
            .collect()
    }

    fn restart(&mut self) {
        for h in &mut self.hills {
            h.owner = None;
            h.score = 0.;
        }
        self.state.iter_mut().for_each(|s| *s = CLEAR);
        self.held.iter_mut().for_each(|s| *s = 0);
        for inside in &mut self.inside {
            inside.iter_mut().for_each(|i| *i = false);
        }
    }

    fn state(&self) -> serde_json::Value {
        json!({
            "state": self.state,
            "held": self.held,
            "hills": self.hills.iter().map(|h| json!({
                "number": h.number, "position": h.position, "owner": h.owner, "score": h.score,
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> KingOfTheHill {
        let toy = Toy::parse(
            "[KingOfTheHill]\nNumberOfHills=2\n[Triggers]\nNumberOfTriggers=2\n\
             [Trigger1]\nName=Hill1\nNumberOfActions=1\nPosition=0,0,0\nBox=20,20,20\n\
             [Trigger1Action1]\nActionType=Hilltop\nHillNumber=1\nSpecificObject=kohflag.slt\n\
             [Trigger2]\nName=Hill2\nNumberOfActions=1\nPosition=500,0,0\nBox=20,20,20\n\
             [Trigger2Action1]\nActionType=Hilltop\nHillNumber=2\n",
        );
        KingOfTheHill::with_riders(&toy, 2).unwrap()
    }

    fn rider(x: f32, grounded: bool) -> Rider {
        Rider {
            position: [x, 0., 0.],
            grounded,
            ..Rider::default()
        }
    }

    /// Drives onto hill 1, jumps off it and lands a chain worth `points`.
    fn jump(g: &mut KingOfTheHill, r: usize, points: f32) -> Vec<Notice> {
        let idle = rider(1000., true);
        let mut riders = [idle, idle];
        riders[r] = rider(0., true);
        g.update(0.1, &riders);
        assert_eq!(g.state[r], READY);
        riders[r] = Rider {
            stunting: true,
            ..rider(40., false)
        };
        g.update(0.1, &riders);
        assert_eq!(g.state[r], 1, "left the hill in the air: armed");
        riders[r] = Rider {
            chain: Some((true, points)),
            ..rider(60., true)
        };
        let notices = g.update(0.1, &riders);
        assert_eq!(g.state[r], CLEAR, "landed");
        riders[r] = rider(1000., true);
        g.update(0.1, &riders);
        notices
    }

    #[test]
    fn a_stunt_off_the_hill_captures_it_and_must_beat_its_score() {
        let mut g = game();
        let notices = jump(&mut g, 0, 500.);
        assert_eq!(notices[0].text, "ATV 1 captured hill 1");
        assert_eq!((g.held.clone(), g.hills[0].owner), (vec![1, 0], Some(0)));
        assert!(jump(&mut g, 1, 400.).is_empty(), "a lower score keeps the hill");
        let notices = jump(&mut g, 1, 600.);
        assert_eq!(notices[0].text, "ATV 2 captured hill 1");
        assert_eq!(g.held, vec![0, 1]);
        assert_eq!(g.progress(1), "1 / 2");
        assert_eq!(g.markers()[0].position[1], MARKER_RAISE);
    }

    #[test]
    fn driving_off_or_entering_in_the_air_does_not_arm() {
        let mut g = game();
        let far = rider(1000., true);
        g.update(0.1, &[rider(0., true), far]);
        g.update(0.1, &[rider(40., true), far]);
        assert_eq!(g.state[0], CLEAR, "drove off");
        g.update(0.1, &[rider(0., false), far]);
        assert_eq!(g.state[0], CLEAR, "flew in");
        g.update(0.1, &[rider(40., false), far]);
        assert_eq!(g.state[0], CLEAR);
    }

    #[test]
    fn a_crash_clears_the_armed_hill_and_restart_resets() {
        let mut g = game();
        let far = rider(1000., true);
        g.update(0.1, &[rider(0., true), far]);
        g.update(0.1, &[rider(40., false), far]);
        assert_eq!(g.state[0], 1);
        g.update(
            0.1,
            &[
                Rider {
                    chain: Some((false, 0.)),
                    ..rider(60., false)
                },
                far,
            ],
        );
        assert_eq!(g.state[0], CLEAR);
        jump(&mut g, 0, 100.);
        g.restart();
        assert_eq!((g.held.clone(), g.hills[0].owner), (vec![0, 0], None));
    }
}
