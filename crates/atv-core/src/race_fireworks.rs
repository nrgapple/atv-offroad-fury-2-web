//! Supercross race fireworks (exact-build loader `00222170`, crossing handler
//! `00223c60`, controller `002217f0`/`00221730`, start `00221840`, countdown
//! Deactivate `0020ec20`).
//!
//! The race TOY's `[PreRaceFireworks]` emitters run their authored cycles
//! from load (through the pre-race camera) and stop when the countdown
//! begins. Each `[FireworkTriggerN]` is an upright gate (position, look,
//! width, height). A racer crossing it fires the trigger's emitter groups in
//! turn, `TimeBetweenGroups` apart: on the lap equal to `TriggerOnLap`
//! (clamped to the last lap) once, otherwise on every lap when
//! `TriggerEveryLap` is set. `RaceOrderMatters` limits a non-leader to group
//! 0; `DontTriggerForAI` ignores CPUs. The gate test is an authored plane
//! crossing; the original RaceStatus gate (`002487c0`) is not ported exactly.
use crate::Result;

type V = [f32; 3];
fn sections(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push((name.trim().to_ascii_lowercase(), Vec::new()));
        } else if let Some((key, value)) = line.split_once('=')
            && let Some((_, fields)) = out.last_mut()
        {
            fields.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub position: V,
    pub look: V,
    pub width: f32,
    pub height: f32,
    pub every_lap: bool,
    pub on_lap: u32,
    pub interval: f32,
    pub groups: usize,
    /// (emitter number, instance number, group).
    pub emitters: Vec<(usize, usize, usize)>,
    pub order_matters: bool,
    pub ignore_ai: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fireworks {
    pub pre_race: Vec<(usize, usize)>,
    pub triggers: Vec<Trigger>,
}
pub fn parse(toy: &str) -> Result<Fireworks> {
    let all = sections(toy);
    let section = |name: &str| {
        all.iter()
            .find(|(n, _)| n == name)
            .map(|(_, f)| f.as_slice())
            .unwrap_or(&[])
    };
    let get = |fields: &[(String, String)], key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim().to_string())
    };
    let number = |fields: &[(String, String)], key: &str, default: f32| -> Result<f32> {
        get(fields, key).map_or(Ok(default), |v| {
            v.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("invalid firework {key}"))
        })
    };
    let flag = |fields: &[(String, String)], key: &str| {
        get(fields, key)
            .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "t" | "true"))
    };
    let vector = |fields: &[(String, String)], key: &str| -> Result<V> {
        let v: Vec<f32> = get(fields, key)
            .ok_or_else(|| format!("missing firework {key}"))?
            .split(',')
            .map(|x| x.trim().parse::<f32>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| format!("invalid firework {key}"))?;
        let [x, y, z] = v[..] else {
            return Err(format!("invalid firework {key}"));
        };
        Ok([x, y, z])
    };
    let emitter = |fields: &[(String, String)]| -> Result<(usize, usize)> {
        Ok((
            number(fields, "emitternumber", 0.)? as usize,
            number(fields, "instancenumber", 1.)? as usize,
        ))
    };
    let pre = section("preracefireworks");
    let count = (number(pre, "numberofemitters", 0.)? as usize).min(256);
    let pre_race = (1..=count)
        .map(|i| emitter(section(&format!("preracefireworkemitter{i}"))))
        .collect::<Result<Vec<_>>>()?;
    let count = (number(section("fireworktriggers"), "numberoftriggers", 0.)? as usize).min(32);
    let mut triggers = Vec::with_capacity(count);
    for t in 1..=count {
        let fields = section(&format!("fireworktrigger{t}"));
        let emitters = (1..=(number(fields, "numberofemitters", 0.)? as usize).min(64))
            .map(|i| {
                let f = section(&format!("fireworktrigger{t}emitter{i}"));
                let (e, n) = emitter(f)?;
                Ok((e, n, number(f, "groupnumber", 0.)? as usize))
            })
            .collect::<Result<Vec<_>>>()?;
        triggers.push(Trigger {
            position: vector(fields, "triggerposition")?,
            look: vector(fields, "triggerlook")?,
            width: number(fields, "triggerwidth", 0.)?,
            height: number(fields, "triggerheight", 0.)?,
            every_lap: number(fields, "triggereverylap", 0.)? != 0.,
            on_lap: number(fields, "triggeronlap", i32::MAX as f32)?.max(0.) as u32,
            interval: number(fields, "timebetweengroups", 0.)?.max(0.),
            groups: (number(fields, "totalgroups", 1.)? as usize).clamp(1, 64),
            emitters,
            order_matters: flag(fields, "raceordermatters"),
            ignore_ai: flag(fields, "donttriggerforai"),
        });
    }
    Ok(Fireworks { pre_race, triggers })
}

/// One racer this step: position before and after, laps completed before
/// this step, place, and whether it is a CPU.
#[derive(Clone, Copy, Debug)]
pub struct Racer {
    pub previous: V,
    pub current: V,
    pub laps: u32,
    pub place: usize,
    pub cpu: bool,
}
/// Emitter commands for the ambient effects: (emitter, instance, fire).
pub type Command = (usize, usize, bool);

struct Sequence {
    trigger: usize,
    groups: usize,
    next: usize,
    timer: f32,
}
pub struct Controller {
    fireworks: Fireworks,
    laps: u32,
    disabled: Vec<bool>,
    sequence: Option<Sequence>,
    pre_race_stopped: bool,
}
impl Controller {
    pub fn new(fireworks: Fireworks, laps: u32) -> Self {
        Self {
            disabled: vec![false; fireworks.triggers.len()],
            fireworks,
            laps,
            sequence: None,
            pre_race_stopped: false,
        }
    }
    fn crossed(t: &Trigger, r: &Racer) -> bool {
        let rel = |p: V| {
            [
                p[0] - t.position[0],
                p[1] - t.position[1],
                p[2] - t.position[2],
            ]
        };
        let (a, b) = (rel(r.previous), rel(r.current));
        let side = |p: V| p[0] * t.look[0] + p[2] * t.look[2];
        let across = [t.look[2], 0., -t.look[0]];
        let lateral = (b[0] * across[0] + b[2] * across[2]).abs();
        side(a) < 0. && side(b) >= 0. && lateral <= t.width / 2. && b[1].abs() <= t.height
    }
    fn group(&self, trigger: usize, group: usize, fire: bool, out: &mut Vec<Command>) {
        for &(e, i, g) in &self.fireworks.triggers[trigger].emitters {
            if g == group {
                out.push((e, i, fire));
            }
        }
    }
    pub fn step(&mut self, dt: f32, counting_down: bool, racers: &[Racer]) -> Vec<Command> {
        let mut out = Vec::new();
        // The countdown deactivates the pre-race fireworks.
        if counting_down && !self.pre_race_stopped {
            self.pre_race_stopped = true;
            out.extend(self.fireworks.pre_race.iter().map(|&(e, i)| (e, i, false)));
        }
        for t in 0..self.fireworks.triggers.len() {
            let trigger = &self.fireworks.triggers[t];
            let on_lap = trigger.on_lap.min(self.laps.saturating_sub(1));
            for r in racers {
                if self.disabled[t] || (r.cpu && trigger.ignore_ai) || !Self::crossed(trigger, r) {
                    continue;
                }
                let fire = if r.laps == on_lap {
                    self.disabled[t] = true;
                    true
                } else {
                    trigger.every_lap
                };
                // The controller only starts when idle.
                if fire && self.sequence.is_none() {
                    let groups = if trigger.order_matters && r.place != 1 {
                        1
                    } else {
                        trigger.groups
                    };
                    self.sequence = Some(Sequence {
                        trigger: t,
                        groups,
                        next: 0,
                        timer: 0.,
                    });
                }
            }
        }
        if let Some(mut s) = self.sequence.take() {
            s.timer -= dt;
            if s.timer <= 0. {
                if s.next > 0 {
                    self.group(s.trigger, s.next - 1, false, &mut out);
                }
                if s.next < s.groups {
                    self.group(s.trigger, s.next, true, &mut out);
                    s.next += 1;
                    s.timer = self.fireworks.triggers[s.trigger].interval;
                    self.sequence = Some(s);
                }
            } else {
                self.sequence = Some(s);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TOY: &str = "[PreRaceFireworks]\r\nNumberOfEmitters=2\r\n[PreRaceFireworkEmitter1]\r\nEmitterNumber=1\r\nInstanceNumber=1\r\n[PreRaceFireworkEmitter2]\r\nEmitterNumber=7\r\nInstanceNumber=2\r\n[FireworkTriggers]\r\nNumberOfTriggers=1\r\n \r\n[FireworkTrigger1]\r\nTriggerPosition=0,0,0\r\nTriggerLook=0,0,1\r\nTriggerWidth=40.0\r\nTriggerHeight=100.0\r\nTriggerEveryLap=0\r\nTriggerOnLap=0\r\nTimeBetweenGroups=0.15\r\nTotalGroups=2\r\nNumberOfEmitters=2\r\nRaceOrderMatters = FALSE\r\nDontTriggerForAI = FALSE\r\n[FireworkTrigger1Emitter1]\r\nEmitterNumber=7\r\nInstanceNumber=1\r\nGroupNumber=0\r\n[FireworkTrigger1Emitter2]\r\nEmitterNumber=8\r\nInstanceNumber=1\r\nGroupNumber=1\r\n";
    fn racer(z0: f32, z1: f32, laps: u32) -> Racer {
        Racer {
            previous: [0., 0., z0],
            current: [0., 0., z1],
            laps,
            place: 2,
            cpu: true,
        }
    }
    #[test]
    fn parses_pre_race_and_trigger_groups() {
        let f = parse(TOY).unwrap();
        assert_eq!(f.pre_race, vec![(1, 1), (7, 2)]);
        let t = &f.triggers[0];
        assert_eq!((t.on_lap, t.groups, t.interval), (0, 2, 0.15));
        assert_eq!(t.emitters, vec![(7, 1, 0), (8, 1, 1)]);
        assert!(!t.order_matters && !t.every_lap);
    }
    #[test]
    fn countdown_stops_pre_race_and_the_gate_fires_groups_in_turn() {
        let mut c = Controller::new(parse(TOY).unwrap(), 3);
        assert!(c.step(0.1, false, &[]).is_empty());
        assert_eq!(c.step(0.1, true, &[]), vec![(1, 1, false), (7, 2, false)]);
        assert!(c.step(0.1, true, &[]).is_empty(), "stopped once");
        // Beside the gate does not count; crossing it on lap 0 fires.
        assert!(
            c.step(
                0.1,
                false,
                &[Racer {
                    previous: [50., 0., -1.],
                    current: [50., 0., 1.],
                    ..racer(0., 0., 0)
                }]
            )
            .is_empty()
        );
        assert_eq!(c.step(0.1, false, &[racer(-1., 1., 0)]), vec![(7, 1, true)]);
        assert!(c.step(0.1, false, &[]).is_empty());
        assert_eq!(c.step(0.1, false, &[]), vec![(7, 1, false), (8, 1, true)]);
        assert_eq!(c.step(0.2, false, &[]), vec![(8, 1, false)]);
        // TriggerOnLap fires once; another crossing does not refire.
        assert!(c.step(0.1, false, &[racer(-1., 1., 0)]).is_empty());
    }
    #[test]
    fn non_leaders_fire_only_group_zero_when_order_matters() {
        let toy = TOY.replace("RaceOrderMatters = FALSE", "RaceOrderMatters = TRUE");
        let mut c = Controller::new(parse(&toy).unwrap(), 3);
        assert_eq!(c.step(0.1, false, &[racer(-1., 1., 0)]), vec![(7, 1, true)]);
        assert_eq!(c.step(0.2, false, &[]), vec![(7, 1, false)]);
        assert!(c.step(0.2, false, &[]).is_empty());
    }
}
