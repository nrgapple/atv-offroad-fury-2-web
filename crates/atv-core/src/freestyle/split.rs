//! Split-screen Freestyle objectives (research/SPLIT_SCREEN.md "Split
//! objectives"): two to four players ride one objective on one clock.
//!
//! Recovered from the exact build (USA v2.00 `SCUS_972.11`):
//!
//! - Offline multiplayer keeps Objectives in Freestyle's event types (only
//!   online play drops it, `0026d010`/`0026a720`).
//! - One objective serves every player. Target hits are counted per player
//!   (`002fab40`: +0xac[player]); the hit's bonus points go to that
//!   player's score and its bonus seconds to the shared clock; the targets
//!   are shared (a taken target is gone for everyone).
//! - The players are ranked live (`+0x9c`: `002fb380` targets then score,
//!   `002fbbb0` score, `002fc450` combos then score; comparator `002fa120`
//!   orders by score, highest first) and each gets its place.
//! - A target run ends when the last target is taken: every player with a
//!   target completed it and the players with the most targets win
//!   (`002fab40`). A combo run ends when a player busts the last combo; that
//!   player wins (`002fc050`). Point levels do not complete a split run
//!   (`002fb720` completes only with one human); their signs show once per
//!   level, on the view of the player who passed it first.
//! - At time out the leader wins, with every player tied with it on the
//!   ranking (`002fb230`, `002fb960`, `002fc2e0`).
//! - A completed player's whole seconds left x 100 join its score
//!   (`002fd980`). Nothing is saved: completion bits, Profile Points and
//!   records need one human (`002fce00`).
use super::{
    Kind, Objective, Outcome, Run, TIME_BONUS_PER_SECOND, TIME_LIMIT, Target, TriggerBody,
};

/// One objective shared by the split-screen players.
#[derive(Clone, Debug)]
pub struct SplitRun {
    /// Each player's run: its score, combos and signs (its own clock and
    /// targets are not used).
    pub runs: Vec<Run>,
    /// The shared clock.
    pub time_left: f32,
    /// Which player took each target.
    pub taken: Vec<Option<usize>>,
    /// Point levels shown (`002fb720` +0xb0, one counter for everyone).
    pub levels: u32,
    /// Set when the run ends: the winners, in player order.
    pub winners: Option<Vec<usize>>,
}

impl SplitRun {
    /// A run for `committed.len()` players on their committed points.
    pub fn new(objective: Objective, index: usize, committed: &[f32]) -> Self {
        let targets = match &objective.kind {
            Kind::CherryPick { targets, .. } => *targets as usize,
            _ => 0,
        };
        Self {
            runs: committed
                .iter()
                .map(|c| Run::new(objective.clone(), index, *c))
                .collect(),
            time_left: TIME_LIMIT,
            taken: vec![None; targets],
            levels: 0,
            winners: None,
        }
    }

    pub fn objective(&self) -> &Objective {
        &self.runs[0].objective
    }

    pub fn finished(&self) -> bool {
        self.winners.is_some()
    }

    /// Targets the player took.
    pub fn targets_of(&self, player: usize) -> usize {
        self.taken.iter().filter(|t| **t == Some(player)).count()
    }

    /// The ranking key's first part: targets, combos or (Stunt) nothing.
    fn primary(&self, player: usize) -> u32 {
        match &self.objective().kind {
            Kind::CherryPick { .. } => self.targets_of(player) as u32,
            Kind::ComboMambo { .. } => self.runs[player].combos(),
            Kind::Stunt { .. } => 0,
        }
    }

    /// The players best first (`002fb380` / `002fbbb0` / `002fc450`):
    /// targets or combos, then score; ties keep player order.
    pub fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.runs.len()).collect();
        order.sort_by(|a, b| {
            self.primary(*b).cmp(&self.primary(*a)).then(
                self.runs[*b]
                    .score
                    .partial_cmp(&self.runs[*a].score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        order
    }

    /// Each player's place (1-based).
    pub fn places(&self) -> Vec<usize> {
        let mut places = vec![0; self.runs.len()];
        for (place, player) in self.order().into_iter().enumerate() {
            places[player] = place + 1;
        }
        places
    }

    /// Ends the run: the winners, completed players' seconds bonus.
    fn end(&mut self, mut winners: Vec<usize>, completed: &[usize]) {
        if self.finished() {
            return;
        }
        let bonus = self.time_left.max(0.).floor() * TIME_BONUS_PER_SECOND;
        for (player, run) in self.runs.iter_mut().enumerate() {
            if completed.contains(&player) {
                run.bonus += bonus;
                run.score += bonus;
                run.outcome = Some(Outcome::Completed);
            } else {
                run.outcome = Some(Outcome::Failed);
            }
            run.time_left = self.time_left;
        }
        winners.sort_unstable();
        self.winners = Some(winners);
    }

    /// Time out: the leader and every player tied with it on targets or
    /// combos and score (`002fb230`, `002fb960`, `002fc2e0`).
    fn time_out(&mut self) {
        let order = self.order();
        let lead = order[0];
        let tied = |p: usize| {
            self.primary(p) == self.primary(lead) && self.runs[p].score == self.runs[lead].score
        };
        let winners: Vec<usize> = order.iter().copied().filter(|p| tied(*p)).collect();
        self.end(winners, &[]);
    }

    /// Advances the shared clock and each player's committed points.
    pub fn tick(&mut self, dt: f32, committed: &[f32]) {
        for run in &mut self.runs {
            if let Some((_, left)) = &mut run.sign {
                *left -= dt.max(0.);
                if *left <= 0. {
                    run.sign = None;
                }
            }
        }
        if self.finished() {
            return;
        }
        for (run, committed) in self.runs.iter_mut().zip(committed) {
            run.score = (committed - run.baseline).max(0.) + run.bonus;
        }
        if let Kind::Stunt { levels } = &self.objective().kind {
            let levels = levels.clone();
            // `002fb720`: one level counter; the sign goes to the player
            // whose score passed it.
            while let Some(level) = levels.get(self.levels as usize).copied() {
                let Some(player) = (0..self.runs.len()).find(|p| self.runs[*p].score >= level)
                else {
                    break;
                };
                self.runs[player]
                    .messages
                    .push(format!("{} Point Level Reached", level as i64));
                self.levels += 1;
            }
        }
        self.time_left = (self.time_left - dt.max(0.)).max(0.);
        for run in &mut self.runs {
            run.time_left = self.time_left;
        }
        if self.time_left <= 0. {
            self.time_out();
        }
    }

    /// A player's finished stunt chain (`002fc050`): its own combos; the
    /// first to the goal wins and ends the run.
    pub fn chain(&mut self, player: usize, success: bool, ids: &[usize]) {
        if self.finished() || player >= self.runs.len() {
            return;
        }
        let Kind::ComboMambo { combos } = self.objective().kind else {
            return;
        };
        let run = &mut self.runs[player];
        if !success || ids.len() < 2 {
            return;
        }
        if run.combos.iter().any(|c| c == ids) {
            run.messages.push("Repeated Combos Do Not Count".into());
            return;
        }
        run.combos.push(ids.to_vec());
        if run.combos.len() as u32 >= combos {
            self.end(vec![player], &[player]);
        }
    }

    /// A player takes an untouched target (`002fab40`).
    pub fn hit(&mut self, player: usize, target: usize) {
        if self.finished() || player >= self.runs.len() {
            return;
        }
        let Kind::CherryPick {
            bonus_points,
            bonus_seconds,
            ..
        } = self.objective().kind
        else {
            return;
        };
        let Some(slot) = self.taken.get_mut(target).filter(|t| t.is_none()) else {
            return;
        };
        *slot = Some(player);
        let run = &mut self.runs[player];
        run.bonus += bonus_points;
        run.score += bonus_points;
        self.time_left += bonus_seconds;
        if self.taken.iter().all(Option::is_some) {
            for run in &mut self.runs {
                run.sign = Some(("All Targets Collected".into(), super::ALL_TARGETS_SIGN));
            }
            let most = (0..self.runs.len())
                .map(|p| self.targets_of(p))
                .max()
                .unwrap_or(0);
            let winners = (0..self.runs.len())
                .filter(|p| self.targets_of(*p) == most)
                .collect();
            let completed: Vec<usize> = (0..self.runs.len())
                .filter(|p| self.targets_of(*p) > 0)
                .collect();
            self.end(winners, &completed);
        } else {
            self.runs[player].sign = Some(("Target Collected".into(), super::TARGET_SIGN));
        }
    }

    /// A player's registered bodies touching untouched targets: each is
    /// taken once, by the first player to touch it. Returns the new hits.
    pub fn touch_bodies(
        &mut self,
        player: usize,
        targets: &[Target],
        bodies: &[TriggerBody],
    ) -> Vec<usize> {
        let mut hit = Vec::new();
        for (i, t) in targets.iter().enumerate() {
            if self.finished() || self.taken.get(i).is_none_or(Option::is_some) {
                continue;
            }
            if bodies.iter().any(|b| b.touches(t.position, t.radius)) {
                self.hit(player, i);
                hit.push(i);
            }
        }
        hit
    }

    /// Targets still standing.
    pub fn untouched(&self) -> impl Iterator<Item = usize> + '_ {
        self.taken
            .iter()
            .enumerate()
            .filter(|(_, t)| t.is_none())
            .map(|(i, _)| i)
    }

    /// A player's ranking figure for the HUD and results.
    pub fn stat(&self, player: usize) -> String {
        let run = &self.runs[player];
        match &self.objective().kind {
            Kind::CherryPick { .. } => format!(
                "{} {}",
                self.targets_of(player),
                if self.targets_of(player) == 1 {
                    "target"
                } else {
                    "targets"
                }
            ),
            Kind::ComboMambo { .. } => format!("{} combos", run.combos()),
            Kind::Stunt { .. } => format!("{} pts", run.score as i64),
        }
    }

    /// The shared progress line.
    pub fn progress(&self) -> String {
        match &self.objective().kind {
            Kind::Stunt { levels } => match levels.get(self.levels as usize) {
                Some(next) => format!(
                    "Level {} of {}: {} pts",
                    self.levels + 1,
                    levels.len(),
                    *next as i64
                ),
                None => format!("All {} Levels Reached", levels.len()),
            },
            Kind::ComboMambo { combos } => format!("First to {combos} combos"),
            Kind::CherryPick { targets, .. } => format!(
                "Targets {} of {targets}",
                self.taken.iter().filter(|t| t.is_some()).count()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn objective(kind: Kind) -> Objective {
        Objective {
            description: "Test".into(),
            kind,
            start: None,
        }
    }

    fn targets(n: usize) -> Kind {
        Kind::CherryPick {
            targets: n as u32,
            bonus_points: 500.,
            bonus_seconds: 10.,
            toy: "gcp".into(),
        }
    }

    #[test]
    fn shared_targets_credit_the_player_who_takes_them() {
        let mut run = SplitRun::new(objective(targets(3)), 0, &[0., 0.]);
        run.tick(20., &[0., 0.]);
        run.hit(1, 0);
        // The bonus seconds join the shared clock; the points the taker's.
        assert_eq!(run.time_left, TIME_LIMIT - 20. + 10.);
        assert_eq!((run.runs[0].score, run.runs[1].score), (0., 500.));
        // A taken target is gone for everyone.
        run.hit(0, 0);
        assert_eq!(run.targets_of(0), 0);
        run.hit(0, 1);
        // One target and 500 points each: player order breaks the tie.
        assert_eq!(run.places(), vec![1, 2]);
        run.tick(0., &[50., 0.]);
        assert_eq!(run.places(), vec![1, 2]);
        run.tick(0., &[0., 50.]);
        assert_eq!(run.places(), vec![2, 1], "targets equal, score decides");
    }

    #[test]
    fn the_last_target_ends_the_run_for_the_most_targets() {
        let mut run = SplitRun::new(objective(targets(3)), 0, &[0., 0., 0.]);
        run.hit(0, 0);
        run.hit(2, 1);
        assert!(!run.finished());
        run.hit(0, 2);
        assert_eq!(run.winners, Some(vec![0]));
        // Players with a target completed it (seconds bonus); player 2 did not.
        assert_eq!(run.runs[0].outcome, Some(Outcome::Completed));
        assert_eq!(run.runs[1].outcome, Some(Outcome::Failed));
        assert_eq!(run.runs[2].outcome, Some(Outcome::Completed));
        let seconds = ((TIME_LIMIT + 30.).floor()) * TIME_BONUS_PER_SECOND;
        assert_eq!(run.runs[2].score, 500. + seconds);
    }

    #[test]
    fn point_levels_never_end_a_split_run_and_time_out_picks_the_top_score() {
        let mut run = SplitRun::new(
            objective(Kind::Stunt {
                levels: vec![100., 200.],
            }),
            0,
            &[0., 0.],
        );
        run.tick(1., &[250., 120.]);
        assert_eq!(run.levels, 2);
        // Both level signs go to player 1, who passed them first.
        assert_eq!(run.runs[0].messages.len(), 2);
        assert!(run.runs[1].messages.is_empty());
        assert!(!run.finished());
        run.tick(TIME_LIMIT, &[250., 120.]);
        assert_eq!(run.winners, Some(vec![0]));
        assert_eq!(run.runs[0].score, 250., "no seconds bonus at time out");
    }

    #[test]
    fn a_tie_at_time_out_shares_the_win() {
        let mut run = SplitRun::new(
            objective(Kind::Stunt { levels: vec![1e6] }),
            0,
            &[0., 0., 0.],
        );
        run.tick(TIME_LIMIT, &[300., 100., 300.]);
        assert_eq!(run.winners, Some(vec![0, 2]));
        assert_eq!(run.places(), vec![1, 3, 2]);
    }

    #[test]
    fn the_first_to_the_combo_goal_wins() {
        let mut run = SplitRun::new(objective(Kind::ComboMambo { combos: 2 }), 0, &[0., 0.]);
        run.chain(1, true, &[1, 2]);
        run.chain(0, true, &[1, 2]);
        // Repeats are each player's own.
        run.chain(1, true, &[1, 2]);
        assert_eq!(run.runs[1].combos(), 1);
        assert_eq!(
            run.order(),
            vec![0, 1],
            "1-1 on combos, equal score: player order"
        );
        run.chain(1, true, &[2, 3]);
        assert_eq!(run.winners, Some(vec![1]));
        assert_eq!(run.runs[0].outcome, Some(Outcome::Failed));
    }
}
