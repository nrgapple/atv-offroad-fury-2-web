//! Single Player → Games (research/GAMES.md): the original's Games Prep
//! (Track Select in mode 3, `0026a720`) as one page: Game, the course from
//! the game's Tracks.dat list, the time limit and Start Game. The original
//! reaches it from Multiplayer → Split Screen → Games; the port has no
//! split screen, so a single player starts it here.
//!
//! Profile Store gating (`0021add0`, `00323640`): King of the Hills and
//! Treasure Hunt start locked until bought (500 points) and each game list
//! opens its first two courses; the rest open with the Freestyle/Enduro
//! course unlocks or GAMEON.
use super::{Event, Frontend};
use crate::settings::Settings;
use atv_core::games::{DEFAULT_MINUTES, GameKind, step_minutes};

/// The page's working state.
#[derive(Clone, Debug, PartialEq)]
pub struct GamesUi {
    /// Index into `GameKind::ALL`.
    pub kind: usize,
    /// Index into the game's course list.
    pub course: usize,
    pub minutes: u32,
}

impl Default for GamesUi {
    fn default() -> Self {
        Self {
            // Treasure Hunt is the first game the port plays.
            kind: 3,
            course: 0,
            minutes: DEFAULT_MINUTES,
        }
    }
}

/// The game value rows.
pub fn game_row(action: &str) -> bool {
    matches!(action, "gametype" | "gamecourse" | "gametime")
}

impl Frontend {
    pub(super) fn game_kind(&self) -> GameKind {
        GameKind::ALL[self.games.kind % GameKind::ALL.len()]
    }

    /// The game's courses (scene, name) in Tracks.dat order.
    pub(super) fn game_courses(&self) -> Vec<(String, String)> {
        let kind = self.game_kind().event_type();
        self.store
            .courses
            .iter()
            .find(|l| l.kind == kind)
            .map(|l| {
                l.courses
                    .iter()
                    .map(|c| (c.scene.clone(), c.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Why the selected game or course cannot start, if it cannot.
    pub(super) fn game_lock(&self) -> Option<String> {
        let kind = self.game_kind();
        if !atv_core::store::event_open(kind.event_type(), &self.profile) {
            return Some(format!(
                "To unlock this game... Spend {} Profile Points in Unlock Items.",
                atv_core::store::grouped(atv_core::store::EVENT_COST)
            ));
        }
        if let Some(cost) = self.course_cost(kind.event_type(), self.games.course) {
            return Some(Self::locked_course(cost));
        }
        None
    }

    pub(super) fn game_choices(&self) -> Vec<(String, String)> {
        let kind = self.game_kind();
        let open = atv_core::store::event_open(kind.event_type(), &self.profile);
        let courses = self.game_courses();
        let course = courses
            .get(self.games.course)
            .map_or("None".to_string(), |(_, name)| name.clone());
        let course_locked = self
            .course_cost(kind.event_type(), self.games.course)
            .is_some();
        vec![
            (
                "gametype".into(),
                format!(
                    "Game: {}{}",
                    kind.name(),
                    if open { "" } else { " (Locked)" }
                ),
            ),
            (
                "gamecourse".into(),
                format!(
                    "Course: {course}{}",
                    if course_locked { " (Locked)" } else { "" }
                ),
            ),
            (
                "gametime".into(),
                format!("Time Limit: {}:00", self.games.minutes),
            ),
            ("gamestart".into(), "Start Game".into()),
            ("back".into(), "Back".into()),
        ]
    }

    /// The line under the rows: the lock, or what the game asks.
    pub(super) fn games_line(&self) -> Option<String> {
        if let Some(lock) = self.game_lock() {
            return Some(lock);
        }
        Some(match self.game_kind() {
            GameKind::TreasureHunt => {
                "Collect the treasures. Each group appears after the last is found.".into()
            }
            GameKind::KingOfTheHills => {
                "Jump off a hill and land a stunt that beats its score to capture it.".into()
            }
            GameKind::Hockey => "Score in the far goal before the time runs out.".into(),
            GameKind::Tag => "Keep the ball away; tag the carrier to take it. Plays in Multiplayer rooms (Pause → Play Tag).".into(),
        })
    }

    pub(super) fn step_game(&mut self, action: &str, direction: i32) {
        match action {
            "gametype" => {
                let n = GameKind::ALL.len() as i32;
                self.games.kind = (self.games.kind as i32 + direction).rem_euclid(n) as usize;
                self.games.course = 0;
            }
            "gamecourse" => {
                let n = self.game_courses().len().max(1) as i32;
                self.games.course = (self.games.course as i32 + direction).rem_euclid(n) as usize;
            }
            "gametime" => self.games.minutes = step_minutes(self.games.minutes, direction),
            _ => return,
        }
        self.notice = None;
    }

    /// Start Game: the selected game on its course with the garage choice.
    pub(super) fn game_event(&mut self, settings: &mut Settings) -> Option<Event> {
        let kind = self.game_kind();
        if !kind.playable() {
            // Tag needs opponents: it plays in multiplayer rooms (the
            // host's Pause → Play Tag, research/GAMES.md "Tag").
            self.notice = Some(format!(
                "{} needs other riders: host a Multiplayer room and choose Pause → Play Tag.",
                kind.name()
            ));
            return None;
        }
        if let Some(lock) = self.game_lock() {
            self.notice = Some(lock);
            return None;
        }
        let (venue, course) = self.game_courses().get(self.games.course)?.clone();
        // Hockey runs its own Coonce Arena session (`atv_core::hockey`).
        if kind == GameKind::Hockey {
            self.notice = self.enforce_unlocks(settings);
            self.save(settings);
            return Some(Event::Hockey {
                quad: self.quad_id,
                rider: self.rider_id,
            });
        }
        if !self.tracks.iter().any(|t| t.id == venue) {
            self.notice = Some(format!("{course} is not available."));
            return None;
        }
        self.notice = self.enforce_unlocks(settings);
        self.save(settings);
        Some(Event::Game {
            kind,
            venue,
            course,
            minutes: self.games.minutes,
            quad: self.quad_id,
            rider: self.rider_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{tracks, view};
    use super::*;
    use crate::front::assets::Track;

    #[test]
    fn games_page_gates_games_and_courses_and_starts_a_game() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut list = tracks();
        for id in ["env05", "env02", "env08"] {
            list.push(Track {
                id: id.into(),
                name: id.into(),
                image: String::new(),
            });
        }
        let mut front = Frontend::new(list, &settings, true);
        front.store.courses = atv_core::store::parse_courses(
            b"[th1]\nscn=env05\nname=\"Prospect Springs\"\naccess=1\npoints=0\n\
              [th2]\nscn=env08\nname=\"Arlington Stream\"\naccess=0\npoints=0\n\
              [koth1]\nscn=env02\nname=\"Tioga Valley\"\naccess=1\npoints=0\n",
        )
        .unwrap();
        front.show("single", &view);
        assert!(front.choices().iter().any(|(a, _)| a == "games"));
        front.run("games", &view, &mut settings);
        assert_eq!(front.page, "games");
        assert_eq!(front.choices()[0].1, "Game: Treasure Hunt (Locked)");
        assert!(front.run("gamestart", &view, &mut settings).is_none());
        assert!(
            front
                .notice
                .as_deref()
                .unwrap()
                .starts_with("To unlock this game")
        );
        front.profile.store.events = vec!["th".into(), "koth".into()];
        front.step("gamecourse", 1, &view, &mut settings);
        assert_eq!(front.choices()[1].1, "Course: Arlington Stream (Locked)");
        assert!(front.run("gamestart", &view, &mut settings).is_none());
        front.step("gamecourse", 1, &view, &mut settings);
        for _ in 0..4 {
            front.step("gametime", -1, &view, &mut settings);
        }
        assert_eq!(front.choices()[2].1, "Time Limit: 20:00");
        match front.run("gamestart", &view, &mut settings) {
            Some(Event::Game {
                kind,
                venue,
                minutes,
                ..
            }) => assert_eq!(
                (kind, venue.as_str(), minutes),
                (GameKind::TreasureHunt, "env05", 20)
            ),
            other => panic!("{other:?}"),
        }
        front.step("gametype", 1, &view, &mut settings);
        assert_eq!(front.choices()[0].1, "Game: King of the Hills");
        assert_eq!(front.choices()[1].1, "Course: Tioga Valley");
        front.step("gametype", 1, &view, &mut settings);
        assert!(
            front.run("gamestart", &view, &mut settings).is_none(),
            "Tag is not ported"
        );
        front.back(&view);
        assert_eq!(front.page, "single");
    }
}
