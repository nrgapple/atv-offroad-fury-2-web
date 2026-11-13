//! The World Hall of Fame view and the Daily Event page (research/
//! WORLD_BOARDS.md; port additions). The host fetches the boards these
//! pages want ([`Frontend::world_wants`]) and stores the answers in
//! [`WorldView`]; the pages show loading, offline and failed states, and
//! the rider's own names are marked.
use super::{Event, Frontend, Viewport};
use crate::settings::Settings;
use atv_core::world_board::{self, Board, DailyPick, VehicleClass};
use std::collections::BTreeMap;

/// The Daily Event page.
pub const DAILY_PAGE: &str = "daily";
/// Rows of a world board shown on a page.
pub const SHOWN: usize = 10;

/// One board's state in the menus.
#[derive(Clone, Debug, PartialEq)]
pub enum BoardState {
    Loading,
    Ready(Board),
    /// The server could not be reached.
    Offline,
    /// The server answered with an error.
    Failed,
}

/// The menus' copy of the world boards, filled by the host.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldView {
    /// Today's UTC day number (days since 1970-01-01), from the host clock.
    pub day: Option<i64>,
    pub boards: BTreeMap<String, BoardState>,
    /// Names this rider submitted under (their rows are marked).
    pub names: Vec<String>,
}

impl WorldView {
    /// Today's event, once the host set the clock.
    pub fn pick(&self) -> Option<DailyPick> {
        self.day.map(world_board::daily)
    }
}

fn ms_time(ms: i64) -> String {
    let centis = ms.max(0) / 10;
    format!(
        "{:02}:{:02}.{:02}",
        centis / 6000,
        centis / 100 % 60,
        centis % 100
    )
}

/// The default vehicle of a class when the garage's does not fit it: the
/// first ATV or the first motorcycle.
fn class_vehicle(class: VehicleClass, current: u16) -> u16 {
    if class.admits(current) {
        current
    } else if class == VehicleClass::Motorcycle {
        world_board::FIRST_MOTORCYCLE
    } else {
        1
    }
}

impl Frontend {
    /// The board ids the open page shows that the host has not fetched.
    pub fn world_wants(&self) -> Vec<String> {
        let wanted = match self.page.as_str() {
            "hall-of-fame" if self.profile_ui.fame_world => self.fame_boards(),
            DAILY_PAGE => self.world.pick().map_or_else(Vec::new, |p| vec![p.board()]),
            _ => Vec::new(),
        };
        wanted
            .into_iter()
            .filter(|id| !self.world.boards.contains_key(id))
            .collect()
    }

    /// The host started fetching `board`.
    pub fn world_loading(&mut self, board: &str) {
        self.world
            .boards
            .insert(board.to_string(), BoardState::Loading);
    }

    /// The host's answer for `board`; the page redraws when it shows it.
    pub fn world_answer(&mut self, board: &str, state: BoardState, view: &Viewport) {
        self.world.boards.insert(board.to_string(), state);
        if self.page == DAILY_PAGE || self.page == "hall-of-fame" {
            self.refresh(view);
        }
    }

    /// A submission placed on these boards under `name`: they are fetched
    /// again and the name's rows are marked.
    pub fn world_placed(&mut self, name: &str, boards: &[String]) {
        for board in boards {
            self.world.boards.remove(board);
        }
        if !self.world.names.iter().any(|n| n == name) {
            self.world.names.push(name.to_string());
        }
    }

    /// The world boards of the Hall of Fame's selection: the lap and race
    /// boards of a race course, the score board of a Freestyle course.
    fn fame_boards(&self) -> Vec<String> {
        let groups = self.fame_groups();
        let Some((group, courses)) = groups.get(self.profile_ui.fame_group) else {
            return Vec::new();
        };
        let Some(id) = courses.get(self.profile_ui.fame_course) else {
            return Vec::new();
        };
        if let Some(series) = atv_core::freestyle::SERIES
            .iter()
            .find(|s| s.name == *group)
        {
            return vec![world_board::score_board(series.key, id)];
        }
        let expert = self.profile_ui.fame_expert;
        vec![
            world_board::lap_board(id, expert),
            world_board::race_board(id, expert),
        ]
    }

    /// The rider's own row: a name they submitted under or the loaded
    /// profile's, never a house rider's row of the same name.
    fn mine(&self, entry: &world_board::BoardEntry) -> bool {
        !entry.house
            && (self.world.names.iter().any(|n| *n == entry.name)
                || self
                    .profile_name()
                    .is_some_and(|p| p.eq_ignore_ascii_case(&entry.name)))
    }

    /// A board's lines: a title, its top rows (the rider's marked "▶") and
    /// the rider's own row below them when it ranks lower.
    fn board_lines(&self, title: &str, id: &str, score: bool) -> Vec<(String, String)> {
        let mut lines = vec![(title.to_string(), String::new())];
        let value = |v: i64| {
            if score {
                super::profile_pages::grouped(v)
            } else {
                ms_time(v)
            }
        };
        match self.world.boards.get(id) {
            None | Some(BoardState::Loading) => lines.push(("Loading…".into(), String::new())),
            Some(BoardState::Offline) => lines.push((
                "Offline: the world boards need the network.".into(),
                String::new(),
            )),
            Some(BoardState::Failed) => lines.push((
                "The world boards could not be loaded.".into(),
                String::new(),
            )),
            Some(BoardState::Ready(board)) if board.entries.is_empty() => {
                lines.push(("No times yet. Be the first!".into(), String::new()))
            }
            Some(BoardState::Ready(board)) => {
                let row = |e: &world_board::BoardEntry| {
                    let mark = if self.mine(e) { "▶ " } else { "" };
                    (
                        format!(
                            "{mark}{}. {} · {}",
                            e.rank,
                            e.name,
                            self.quad_label(e.vehicle)
                        ),
                        value(e.value),
                    )
                };
                lines.extend(board.entries.iter().take(SHOWN).map(row));
                if let Some(own) = board.entries.iter().skip(SHOWN).find(|e| self.mine(e)) {
                    lines.push(row(own));
                }
            }
        }
        lines
    }

    /// The Hall of Fame's World view of the selection.
    pub(super) fn world_fame_details(&self) -> Vec<(String, String)> {
        let groups = self.fame_groups();
        let Some((group, courses)) = groups.get(self.profile_ui.fame_group) else {
            return vec![("No race courses are loaded.".into(), String::new())];
        };
        let Some(id) = courses.get(self.profile_ui.fame_course) else {
            return Vec::new();
        };
        let boards = self.fame_boards();
        if atv_core::freestyle::SERIES.iter().any(|s| s.name == *group) {
            let mut lines = vec![(self.course_name(id), format!("World · {group}"))];
            lines.extend(self.board_lines("Highest Objective Scores", &boards[0], true));
            return lines;
        }
        let laps = world_board::default_laps(id);
        let difficulty = if self.profile_ui.fame_expert {
            "Expert"
        } else {
            "Normal"
        };
        let mut lines = vec![(self.course_name(id), format!("World · {difficulty}"))];
        lines.extend(self.board_lines("Fastest Lap", &boards[0], false));
        lines.extend(self.board_lines(&format!("Fastest {laps}-Lap Race"), &boards[1], false));
        lines
    }

    /// Single Player's Daily Event row is offered once the clock is set and
    /// the day's course is loaded.
    pub(super) fn daily_offered(&self) -> bool {
        self.world
            .pick()
            .is_some_and(|p| self.race_courses.iter().any(|c| c == p.course))
    }

    pub(super) fn daily_choices(&self) -> Vec<(String, String)> {
        vec![("dailystart".into(), "Start Daily Event".into())]
    }

    pub(super) fn daily_details(&self) -> Vec<(String, String)> {
        let Some(pick) = self.world.pick() else {
            return vec![("The daily event needs the clock.".into(), String::new())];
        };
        let event =
            atv_game::launch::split_race_course(pick.course).map_or("Race", |(_, v)| match v {
                atv_game::launch::RaceVariant::EnduroShort => "Amateur Enduro",
                atv_game::launch::RaceVariant::EnduroLong => "Pro Enduro",
                _ if pick.course.starts_with("sx") => "Supercross",
                _ => "Nationals",
            });
        let mut lines = vec![
            (format!("Daily Event · {}", pick.date), "UTC".into()),
            ("Track:".into(), self.course_name(pick.course)),
            (
                "Event:".into(),
                format!("{event} · {} Laps · Normal", pick.laps()),
            ),
            ("Vehicle:".into(), format!("Any {}", pick.class.label())),
        ];
        let board = pick.board();
        if let Some(BoardState::Ready(b)) = self.world.boards.get(&board)
            && let Some(y) = &b.yesterday
        {
            lines.push((
                "Yesterday's Winner:".into(),
                match (&y.name, y.value) {
                    (Some(name), Some(ms)) => format!("{name} · {}", ms_time(ms)),
                    _ => "<Nobody>".into(),
                },
            ));
        }
        lines.extend(self.board_lines("Today's Board", &board, false));
        lines
    }

    /// Daily Event actions: open the page, start the race.
    pub(super) fn world_action(
        &mut self,
        action: &str,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Option<Event>> {
        match action {
            "daily" => {
                self.show(DAILY_PAGE, view);
                Some(None)
            }
            "dailystart" => {
                let pick = self.world.pick()?;
                let quad = class_vehicle(pick.class, self.quad_id);
                self.notice = self.enforce_unlocks(settings);
                Some(Some(Event::Race {
                    environment: pick.course.to_string(),
                    quad,
                    rider: self.rider_id,
                    laps: pick.laps(),
                    cpu: super::RACE_CPU[1],
                    difficulty: 1,
                    cheats: (false, false),
                    championship: None,
                    practice: false,
                    duel: false,
                    lap_attack: false,
                }))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_core::world_board::BoardEntry;

    fn front() -> (Frontend, Viewport, Settings) {
        let view = Viewport {
            width: 1280.,
            height: 720.,
            scale: 1.,
            ..Default::default()
        };
        let settings = Settings::memory();
        let mut front = Frontend::new(Vec::new(), &settings, true);
        front.race_courses = atv_core::world_board::DAILY_POOL
            .iter()
            .map(|c| c.to_string())
            .collect();
        front.show("main", &view);
        (front, view, settings)
    }

    #[test]
    fn the_hall_of_fame_world_view_loads_marks_and_reports_offline() {
        let (mut front, view, mut settings) = front();
        front.run("halloffame", &view, &mut settings);
        assert!(
            front.world_wants().is_empty(),
            "the local view fetches nothing"
        );
        front.run("fameworld", &view, &mut settings);
        let wants = front.world_wants();
        assert_eq!(wants.len(), 2);
        assert!(wants[0].starts_with("lap:") && wants[1].starts_with("race:"));
        for id in &wants {
            front.world_loading(id);
        }
        assert!(front.world_wants().is_empty());
        assert!(front.profile_details().iter().any(|l| l.0 == "Loading…"));
        front.world.names.push("ADA".into());
        let mut entries: Vec<BoardEntry> = (1..=12)
            .map(|rank| BoardEntry {
                rank,
                name: if rank == 12 {
                    "ADA".into()
                } else {
                    format!("R{rank}")
                },
                value: 60_000 + i64::from(rank) * 1000,
                vehicle: 1,
                day: "2026-11-11".into(),
                // A house rider named as the player is not the player's row.
                house: rank == 3,
            })
            .collect();
        entries[2].name = "ADA".into();
        let board = Board {
            board: wants[0].clone(),
            entries,
            yesterday: None,
        };
        front.world_answer(&wants[0], BoardState::Ready(board), &view);
        front.world_answer(&wants[1], BoardState::Offline, &view);
        let details = front.profile_details();
        assert!(
            details
                .iter()
                .any(|l| l.0.starts_with("1. R1") && l.1 == "01:01.00")
        );
        assert!(
            details.iter().any(|l| l.0.starts_with("▶ 12. ADA")),
            "{details:?}"
        );
        assert!(
            details.iter().any(|l| l.0.starts_with("3. ADA")),
            "a house row is an ordinary row: {details:?}"
        );
        assert!(details.iter().any(|l| l.0.starts_with("Offline")));
        // Placing again refetches the boards.
        front.world_placed("ADA", &wants);
        assert_eq!(front.world_wants(), wants);
    }

    #[test]
    fn the_daily_event_works_offline_and_starts_the_pick() {
        let (mut front, view, mut settings) = front();
        front.show("single", &view);
        assert!(
            !front.choices.iter().any(|c| c.0 == "daily"),
            "no clock yet"
        );
        front.world.day = Some(20_768);
        front.show("single", &view);
        assert!(front.choices.iter().any(|c| c.0 == "daily"));
        front.run("daily", &view, &mut settings);
        assert_eq!(front.page, DAILY_PAGE);
        let pick = world_board::daily(20_768);
        let details = front.profile_details();
        assert_eq!(details[0].0, format!("Daily Event · {}", pick.date));
        assert!(details.iter().any(|l| l.0 == "Loading…"));
        front.world_answer(&pick.board(), BoardState::Offline, &view);
        assert!(
            front
                .profile_details()
                .iter()
                .any(|l| l.0.starts_with("Offline"))
        );
        match front.run("dailystart", &view, &mut settings) {
            Some(Event::Race {
                environment,
                quad,
                laps,
                difficulty,
                ..
            }) => {
                assert_eq!(environment, pick.course);
                assert!(pick.class.admits(quad));
                assert_eq!((laps, difficulty), (pick.laps(), 1));
            }
            other => panic!("{other:?}"),
        }
    }
}
