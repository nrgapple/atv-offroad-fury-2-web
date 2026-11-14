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
/// The Weekly Challenge page (research/GHOST_CHALLENGES.md).
pub const WEEKLY_PAGE: &str = "weekly";
/// Rows of a world board shown on a page.
pub const SHOWN: usize = 10;
/// Rows of each board where a page shows two (a race course's lap and race
/// boards): ten of each run off the panel under the first.
pub const PAIR_SHOWN: usize = 5;

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
    /// Unix milliseconds, from the host clock (the weekly's time left).
    pub now_ms: Option<f64>,
    /// The last weekly submission's answer: its week and percentile.
    pub weekly_percentile: Option<(String, u32)>,
    /// Lap Attack's ghost picker: the chosen row.
    pub ghost_pick: usize,
}

/// One choice of Lap Attack's ghost picker.
#[derive(Clone, Debug, PartialEq)]
pub struct GhostPick {
    /// "PB 1:02.33", "~Gold 0:45.70", "Rival R4 0:50.00"…
    pub label: String,
    /// The lap board, row name and lap ms of a world ghost; an empty board
    /// for this device's personal best ghost; `None` for Off.
    pub ghost: Option<(String, String, i64)>,
}

impl WorldView {
    /// Today's event, once the host set the clock.
    pub fn pick(&self) -> Option<DailyPick> {
        self.day.map(world_board::daily)
    }
    /// This week's challenge, once the host set the clock.
    pub fn weekly(&self) -> Option<atv_core::medals::WeeklyPick> {
        self.day
            .map(|d| atv_core::medals::weekly(atv_core::medals::week_index(d)))
    }
}

/// "2d 4h", "5h 12m" or "12m" until `ends_ms`.
fn time_left(now_ms: f64, ends_ms: i64) -> String {
    let minutes = ((ends_ms as f64 - now_ms) / 60_000.).max(0.) as i64;
    match (minutes / 1440, minutes / 60 % 24, minutes % 60) {
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, _) => format!("{d}d {h}h"),
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
        let past = || {
            self.world.weekly().map_or_else(Vec::new, |w| {
                self.medals
                    .unbadged(&w.week)
                    .into_iter()
                    .map(|week| format!("weekly:{week}"))
                    .collect()
            })
        };
        let wanted = match self.page.as_str() {
            "hall-of-fame" if self.profile_ui.fame_world => self.fame_boards(),
            DAILY_PAGE => self.world.pick().map_or_else(Vec::new, |p| vec![p.board()]),
            WEEKLY_PAGE => {
                let mut ids: Vec<String> =
                    self.world.weekly().map(|w| w.board()).into_iter().collect();
                ids.extend(past());
                ids
            }
            "profile-overview" => past(),
            "race" if self.lap_attack() => vec![self.lap_attack_board()],
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
        if matches!(
            self.page.as_str(),
            DAILY_PAGE | WEEKLY_PAGE | "hall-of-fame" | "race" | "profile-overview"
        ) {
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
            && (self.world.names.contains(&entry.name)
                || self
                    .profile_name()
                    .is_some_and(|p| p.eq_ignore_ascii_case(&entry.name)))
    }

    /// A board's lines: a title, its top `shown` rows (the rider's marked
    /// "▶") and the rider's own row below them when it ranks lower.
    fn board_lines(
        &self,
        title: &str,
        id: &str,
        score: bool,
        shown: usize,
    ) -> Vec<(String, String)> {
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
            Some(BoardState::Ready(board)) if board.entries.is_empty() => lines.push((
                if score {
                    "No scores yet. Be the first!"
                } else {
                    "No times yet. Be the first!"
                }
                .into(),
                String::new(),
            )),
            Some(BoardState::Ready(board)) => {
                let row = |e: &world_board::BoardEntry| {
                    let mark = if self.mine(e) { "▶ " } else { "" };
                    let ghost = if e.ghost { " · Ghost" } else { "" };
                    (
                        format!(
                            "{mark}{}. {} · {}{ghost}",
                            e.rank,
                            e.name,
                            self.quad_label(e.vehicle)
                        ),
                        value(e.value),
                    )
                };
                lines.extend(board.entries.iter().take(shown).map(row));
                if let Some(own) = board.entries.iter().skip(shown).find(|e| self.mine(e)) {
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
            lines.extend(self.board_lines("Highest Objective Scores", &boards[0], true, SHOWN));
            return lines;
        }
        let laps = world_board::default_laps(id);
        let difficulty = if self.profile_ui.fame_expert {
            "Expert"
        } else {
            "Normal"
        };
        let mut lines = vec![(self.course_name(id), format!("World · {difficulty}"))];
        lines.extend(self.board_lines("Fastest Lap", &boards[0], false, PAIR_SHOWN));
        lines.extend(self.board_lines(
            &format!("Fastest {laps}-Lap Race"),
            &boards[1],
            false,
            PAIR_SHOWN,
        ));
        lines
    }

    /// The World view's lap board and its rows that carry a ghost to race
    /// (none on a Freestyle course, in the local view or before the board
    /// loads).
    pub(super) fn ghost_rows(&self) -> Vec<(String, world_board::BoardEntry)> {
        if self.page != "hall-of-fame" || !self.profile_ui.fame_world {
            return Vec::new();
        }
        let Some(id) = self
            .fame_boards()
            .into_iter()
            .find(|id| id.starts_with("lap:"))
        else {
            return Vec::new();
        };
        let Some(BoardState::Ready(board)) = self.world.boards.get(&id) else {
            return Vec::new();
        };
        board
            .entries
            .iter()
            .filter(|e| e.ghost)
            .map(|e| (id.clone(), e.clone()))
            .collect()
    }

    /// The World view's ghost rows: the chosen ghost (Left/Right steps
    /// through the board's) and Race This Ghost.
    pub(super) fn ghost_choices(&self) -> Vec<(String, String)> {
        let rows = self.ghost_rows();
        let Some((_, entry)) = rows.get(self.profile_ui.fame_ghost.min(rows.len().max(1) - 1))
        else {
            return Vec::new();
        };
        vec![
            (
                "fameghost".into(),
                format!("Ghost: {} · {}", entry.name, ms_time(entry.value)),
            ),
            ("fameghostrace".into(), "Race This Ghost".into()),
        ]
    }

    /// Race This Ghost: Lap Attack on the board's course with the garage's
    /// vehicle, once the host has the ghost.
    fn ghost_race(&mut self, settings: &mut Settings) -> Option<Event> {
        let rows = self.ghost_rows();
        let (board, entry) = rows.get(self.profile_ui.fame_ghost.min(rows.len().max(1) - 1))?;
        let course = board.split(':').nth(1)?.to_string();
        if !self.race_courses.contains(&course) {
            self.notice = Some(format!("{} is not available.", self.course_name(&course)));
            return None;
        }
        if let Some(cost) = self.race_course_cost(&course) {
            self.notice = Some(Self::locked_course(cost));
            return None;
        }
        self.notice = self
            .enforce_unlocks(settings)
            .or_else(|| Some(format!("Loading {}'s ghost…", entry.name)));
        Some(Event::GhostRace {
            board: board.clone(),
            name: entry.name.clone(),
            value: entry.value,
            course,
            expert: self.profile_ui.fame_expert,
            quad: self.quad_id,
            rider: self.rider_id,
        })
    }

    /// Lap Attack is the race setup's event.
    pub(super) fn lap_attack(&self) -> bool {
        self.race_event_type() == atv_core::race_event::EventType::LapAttack
    }

    /// The lap board Lap Attack's world ghosts come from.
    fn lap_attack_board(&self) -> String {
        world_board::lap_board(&self.race.course, self.race_difficulty() == 2)
    }

    /// Lap Attack's ghost picker (research/GHOST_CHALLENGES.md): Off (the
    /// original's own best-lap ghost), then the choices that exist: this
    /// device's personal best, the next medal's ghost, the rival just
    /// above the rider and the board's #1.
    pub fn ghost_picks(&self) -> Vec<GhostPick> {
        use atv_core::medals;
        let course = &self.race.course;
        let mut picks = vec![GhostPick {
            label: "Off".into(),
            ghost: None,
        }];
        if let Some(ms) = self.medals.ghosts.get(course) {
            picks.push(GhostPick {
                label: format!("PB {}", ms_time(i64::from(*ms))),
                ghost: Some((String::new(), "PB".into(), i64::from(*ms))),
            });
        }
        let board = self.lap_attack_board();
        let Some(BoardState::Ready(rows)) = self.world.boards.get(&board) else {
            return picks;
        };
        let world = |label: String, e: &world_board::BoardEntry| GhostPick {
            label: format!("{label} {}", ms_time(e.value)),
            ghost: Some((board.clone(), e.name.clone(), e.value)),
        };
        let best = self.medals.best(course);
        if let Some(medal) = medals::next_medal(course, best)
            && let Some(target) = medals::medal_ms(course, medal)
            && let Some((entry, approx)) = medals::medal_ghost(&rows.entries, target)
        {
            let tilde = if approx { "~" } else { "" };
            picks.push(world(format!("{tilde}{}", medal.label()), entry));
        }
        // The rider's best: their board row, else this device's best lap.
        let mine = rows
            .entries
            .iter()
            .filter(|e| self.mine(e))
            .map(|e| e.value)
            .min()
            .or(best.map(i64::from));
        if let Some(rival) = medals::rival_ghost(&rows.entries, mine) {
            picks.push(world("Rival".into(), rival));
        }
        if let Some(top) = medals::top_ghost(&rows.entries) {
            picks.push(world("#1".into(), top));
        }
        picks
    }

    /// The race setup's Ghost row (Lap Attack only).
    pub(super) fn ghost_row(&self) -> Option<(String, String)> {
        if !self.lap_attack() {
            return None;
        }
        let picks = self.ghost_picks();
        let pick = &picks[self.world.ghost_pick.min(picks.len() - 1)];
        Some(("raceghost".into(), format!("Ghost: {}", pick.label)))
    }

    /// Left/Right on the Ghost row.
    pub(super) fn step_ghost(&mut self, direction: i32) {
        let n = self.ghost_picks().len() as i32;
        let at = (self.world.ghost_pick as i32).min(n - 1);
        self.world.ghost_pick = (at + direction).rem_euclid(n) as usize;
    }

    /// Start Race with a ghost picked: the host fetches it (or reads the
    /// personal best) and starts Lap Attack against it.
    pub(super) fn ghost_start(&self) -> Option<Event> {
        let picks = self.ghost_picks();
        let (board, name, value) = picks
            .get(self.world.ghost_pick.min(picks.len() - 1))?
            .ghost
            .clone()?;
        Some(Event::GhostRace {
            board,
            name,
            value,
            course: self.race.course.clone(),
            expert: self.race_difficulty() == 2,
            quad: self.quad_id,
            rider: self.rider_id,
        })
    }

    /// Single Player's Weekly Challenge row: once the clock is set and the
    /// week's course is loaded.
    pub(super) fn weekly_offered(&self) -> bool {
        self.world
            .weekly()
            .is_some_and(|p| self.race_courses.iter().any(|c| c == p.course))
    }

    pub(super) fn weekly_choices(&self) -> Vec<(String, String)> {
        vec![("weeklystart".into(), "Start Weekly".into())]
    }

    /// The Weekly Challenge page: the course, the time left, the rider's
    /// best with its live band, and the board's top ten.
    pub(super) fn weekly_details(&self) -> Vec<(String, String)> {
        let Some(pick) = self.world.weekly() else {
            return vec![(
                "The weekly challenge needs the clock.".into(),
                String::new(),
            )];
        };
        let left = self.world.now_ms.map_or(String::new(), |now| {
            format!("{} left", time_left(now, pick.ends_ms))
        });
        let mut lines = vec![
            (format!("Weekly · {}", pick.week), left),
            ("Track:".into(), self.course_name(pick.course)),
            (
                "Event:".into(),
                format!("Time Trial · {} Laps · No CPUs", pick.laps()),
            ),
            ("Vehicle:".into(), format!("Any {}", pick.class.label())),
        ];
        let board = pick.board();
        let ready = match self.world.boards.get(&board) {
            Some(BoardState::Ready(b)) => Some(b),
            _ => None,
        };
        if let Some(mine) = self.medals.weeks.get(&pick.week) {
            // The submission's exact percentile, else the band from the cuts.
            let percent = self
                .world
                .weekly_percentile
                .as_ref()
                .filter(|(week, _)| *week == pick.week)
                .map(|(_, p)| format!("Top {p}%"));
            let band = ready.filter(|b| !b.cuts.is_empty()).map(|b| {
                atv_core::medals::Badge::of(i64::from(mine.value), &b.cuts)
                    .label()
                    .to_string()
            });
            let riders = ready.map_or(String::new(), |b| {
                if b.riders > 0 {
                    format!(" · {} riders", b.riders)
                } else {
                    String::new()
                }
            });
            lines.push((
                "Your Best:".into(),
                match percent.or(band) {
                    Some(p) => format!("{} · {p}{riders}", ms_time(i64::from(mine.value))),
                    None => ms_time(i64::from(mine.value)),
                },
            ));
        }
        lines.extend(self.board_lines("This Week", &board, false, SHOWN));
        lines
    }

    /// The host could not get a ghost (or read it): the page says why.
    pub fn ghost_failed(&mut self, message: &str, view: &Viewport) {
        if self.page == "hall-of-fame" || self.page == "race" {
            self.notice = Some(message.to_string());
            self.refresh(view);
        }
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
        lines.extend(self.board_lines("Today's Board", &board, false, SHOWN));
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
            "fameghostrace" => Some(self.ghost_race(settings)),
            WEEKLY_PAGE => {
                self.show(WEEKLY_PAGE, view);
                Some(None)
            }
            "weeklystart" => {
                let pick = self.world.weekly()?;
                let quad = class_vehicle(pick.class, self.quad_id);
                self.notice = self.enforce_unlocks(settings);
                Some(Some(Event::Race {
                    environment: pick.course.to_string(),
                    quad,
                    rider: self.rider_id,
                    laps: pick.laps(),
                    cpu: 0,
                    difficulty: 1,
                    cheats: (false, false),
                    championship: None,
                    practice: false,
                    duel: false,
                    lap_attack: false,
                }))
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
                ghost: false,
                // A house rider named as the player is not the player's row.
                house: rank == 3,
            })
            .collect();
        entries[2].name = "ADA".into();
        let board = Board {
            board: wants[0].clone(),
            entries,
            yesterday: None,
            ..Default::default()
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
        // Two boards share the panel: five rows of each.
        assert!(details.iter().any(|l| l.0.starts_with("5. R5")));
        assert!(!details.iter().any(|l| l.0.starts_with("6. R6")));
        assert!(details.iter().any(|l| l.0.starts_with("Offline")));
        // Placing again refetches the boards.
        front.world_placed("ADA", &wants);
        assert_eq!(front.world_wants(), wants);
    }

    #[test]
    fn the_world_view_lists_a_freestyle_course_score_board() {
        let (mut front, view, mut settings) = front();
        front.run("halloffame", &view, &mut settings);
        front.run("fameworld", &view, &mut settings);
        let pro = front
            .fame_groups()
            .iter()
            .position(|(g, _)| *g == "Pro Freestyle")
            .unwrap();
        for _ in 0..pro {
            front.step_profile("fameevent", 1, &view);
        }
        front.step_profile("famecourse", 1, &view);
        // One board per series and course, whatever the difficulty row says.
        assert_eq!(front.world_wants(), ["score:spstunt:free04"]);
        front.step_profile("famedifficulty", 1, &view);
        let wants = front.world_wants();
        assert_eq!(wants, ["score:spstunt:free04"]);
        front.world_loading(&wants[0]);
        let empty = Board {
            board: wants[0].clone(),
            ..Default::default()
        };
        front.world_answer(&wants[0], BoardState::Ready(empty), &view);
        let details = front.profile_details();
        assert_eq!(details[0].1, "World · Pro Freestyle");
        assert_eq!(details[1].0, "Highest Objective Scores");
        assert_eq!(details[2].0, "No scores yet. Be the first!");
        // A placed score refetches the board; its rows show grouped scores.
        front.world_placed("ADA", &wants);
        assert_eq!(front.world_wants(), wants);
        let entries = vec![
            BoardEntry {
                rank: 1,
                name: "ACE".into(),
                value: 1_204_500,
                vehicle: 28,
                day: "2026-11-12".into(),
                ghost: false,
                house: false,
            },
            BoardEntry {
                rank: 2,
                name: "ADA".into(),
                value: 31_250,
                vehicle: 4,
                day: "2026-11-12".into(),
                ghost: false,
                house: false,
            },
        ];
        let board = Board {
            board: wants[0].clone(),
            entries,
            yesterday: None,
            ..Default::default()
        };
        front.world_answer(&wants[0], BoardState::Ready(board), &view);
        let details = front.profile_details();
        assert!(
            details
                .iter()
                .any(|l| l.0.starts_with("1. ACE") && l.1 == "1,204,500"),
            "{details:?}"
        );
        assert!(
            details
                .iter()
                .any(|l| l.0.starts_with("▶ 2. ADA") && l.1 == "31,250"),
            "{details:?}"
        );
    }

    #[test]
    fn the_world_view_offers_a_lap_board_row_s_ghost_to_race() {
        let (mut front, view, mut settings) = front();
        front.run("halloffame", &view, &mut settings);
        front.run("fameworld", &view, &mut settings);
        let wants = front.world_wants();
        let course = wants[0].split(':').nth(1).unwrap().to_string();
        let rows = |ghosts: &[u32]| Board {
            board: wants[0].clone(),
            entries: (1..=4)
                .map(|rank| BoardEntry {
                    rank,
                    name: format!("R{rank}"),
                    value: 60_000 + i64::from(rank) * 1230,
                    vehicle: 1,
                    day: "2026-11-12".into(),
                    ghost: ghosts.contains(&rank),
                    house: false,
                })
                .collect(),
            yesterday: None,
            ..Default::default()
        };
        // No ghost rows, no ghost choices.
        front.world_answer(&wants[0], BoardState::Ready(rows(&[])), &view);
        assert!(!front.choices.iter().any(|c| c.0.starts_with("fameghost")));
        front.world_answer(&wants[0], BoardState::Ready(rows(&[2, 4])), &view);
        let label = |front: &Frontend| {
            let at = front.choices.iter().position(|c| c.0 == "fameghost");
            at.map(|i| front.choices[i].1.clone())
        };
        assert_eq!(label(&front).as_deref(), Some("Ghost: R2 · 01:02.46"));
        assert!(front.choices.iter().any(|c| c.0 == "fameghostrace"));
        let details = front.profile_details();
        assert!(
            details
                .iter()
                .any(|l| l.0.starts_with("2. R2") && l.0.ends_with("· Ghost"))
        );
        assert!(
            details
                .iter()
                .any(|l| l.0.starts_with("1. R1") && !l.0.contains("Ghost"))
        );
        // Left/Right steps through the rows with a ghost.
        front.step_profile("fameghost", 1, &view);
        assert_eq!(label(&front).as_deref(), Some("Ghost: R4 · 01:04.92"));
        front.step_profile("fameghost", 1, &view);
        assert_eq!(label(&front).as_deref(), Some("Ghost: R2 · 01:02.46"));
        front.step_profile("fameghost", -1, &view);
        match front.run("fameghostrace", &view, &mut settings) {
            Some(Event::GhostRace {
                board,
                name,
                value,
                course: on,
                expert,
                ..
            }) => {
                assert_eq!(
                    (board, name, value),
                    (wants[0].clone(), "R4".into(), 64_920)
                );
                assert_eq!((on, expert), (course, false));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(front.notice.as_deref(), Some("Loading R4's ghost…"));
        // A ghost that cannot be used is reported on the page.
        front.ghost_failed("This ghost could not be read.", &view);
        assert_eq!(
            front.notice.as_deref(),
            Some("This ghost could not be read.")
        );
        // This device's view and Freestyle courses offer none.
        front.run("fameworld", &view, &mut settings);
        assert!(!front.choices.iter().any(|c| c.0.starts_with("fameghost")));
        assert_eq!(front.run("fameghostrace", &view, &mut settings), None);
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

    fn ghost_row(rank: u32, name: &str, value: i64, ghost: bool) -> BoardEntry {
        BoardEntry {
            rank,
            name: name.into(),
            value,
            vehicle: 1,
            day: "2026-11-14".into(),
            ghost,
            house: false,
        }
    }

    #[test]
    fn lap_attack_picks_a_pb_medal_rival_or_top_ghost() {
        use atv_core::race_event::EventType;
        let (mut front, view, mut settings) = front();
        front.race.course = "nat01".into();
        front.show("race", &view);
        assert!(
            !front.choices.iter().any(|c| c.0 == "raceghost"),
            "Single Race has none"
        );
        assert!(front.world_wants().is_empty());
        front.race.event = EventType::LapAttack;
        front.show("race", &view);
        let label = |f: &Frontend| {
            f.choices
                .iter()
                .find(|c| c.0 == "raceghost")
                .map(|c| c.1.clone())
        };
        assert_eq!(label(&front).as_deref(), Some("Ghost: Off"));
        assert_eq!(front.world_wants(), ["lap:nat01:n"]);
        // Silver on this device (51,000 ms) and its personal best ghost.
        front.medals.record_lap("nat01", 51_000);
        front.medals.ghosts.insert("nat01".into(), 51_000);
        let board = Board {
            board: "lap:nat01:n".into(),
            entries: vec![
                ghost_row(1, "TOP", 42_000, true),
                ghost_row(2, "GLD", 45_600, true),
                ghost_row(3, "MID", 49_000, true),
                ghost_row(4, "NOG", 50_000, false),
                ghost_row(5, "SLO", 60_000, true),
            ],
            ..Default::default()
        };
        front.world_answer("lap:nat01:n", BoardState::Ready(board), &view);
        let picks: Vec<String> = front.ghost_picks().into_iter().map(|p| p.label).collect();
        assert_eq!(
            picks,
            [
                "Off",
                "PB 00:51.00",
                "Gold 00:45.60",
                "Rival 00:49.00",
                "#1 00:42.00"
            ]
        );
        front.selection = front
            .choices
            .iter()
            .position(|c| c.0 == "raceghost")
            .unwrap();
        front.step_ghost(3);
        front.refresh(&view);
        assert_eq!(label(&front).as_deref(), Some("Ghost: Rival 00:49.00"));
        match front.run("racestart", &view, &mut settings) {
            Some(Event::GhostRace {
                board,
                name,
                value,
                course,
                ..
            }) => assert_eq!(
                (board.as_str(), name.as_str(), value, course.as_str()),
                ("lap:nat01:n", "MID", 49_000, "nat01")
            ),
            other => panic!("{other:?}"),
        }
        // PB starts against this device's ghost: an empty board.
        front.world.ghost_pick = 1;
        match front.run("racestart", &view, &mut settings) {
            Some(Event::GhostRace { board, value, .. }) => {
                assert_eq!((board.as_str(), value), ("", 51_000))
            }
            other => panic!("{other:?}"),
        }
        // Off is the original's Lap Attack.
        front.world.ghost_pick = 0;
        assert!(matches!(
            front.run("racestart", &view, &mut settings),
            Some(Event::Race {
                lap_attack: true,
                ..
            })
        ));
        // The course list shows the course's medal.
        let screen = front.screen(None, false, None, &|_| None, None, (None, None), 0.);
        assert_eq!(
            screen.medals.get("nat01"),
            Some(&atv_core::medals::Medal::Silver)
        );
    }

    #[test]
    fn the_weekly_challenge_offers_its_pick_and_the_rider_s_band() {
        let (mut front, view, mut settings) = front();
        front.show("single", &view);
        assert!(
            !front.choices.iter().any(|c| c.0 == WEEKLY_PAGE),
            "no clock yet"
        );
        let day = 20_771; // 2026-11-14
        front.world.day = Some(day);
        front.world.now_ms = Some(day as f64 * 86_400_000. + 12. * 3_600_000.);
        front.show("single", &view);
        let at = |f: &Frontend, a: &str| f.choices.iter().position(|c| c.0 == a);
        assert_eq!(
            at(&front, WEEKLY_PAGE),
            at(&front, DAILY_PAGE).map(|i| i + 1)
        );
        front.run(WEEKLY_PAGE, &view, &mut settings);
        assert_eq!(front.page, WEEKLY_PAGE);
        let pick = front.world.weekly().unwrap();
        assert_eq!(pick.week, "2026-W46");
        let details = front.profile_details();
        assert_eq!(
            details[0],
            ("Weekly · 2026-W46".into(), "1d 12h left".into())
        );
        assert!(details.iter().any(|l| l.1.contains("No CPUs")));
        assert_eq!(front.world_wants(), ["weekly:2026-W46"]);
        // A best time this week, a finished unbadged one before it.
        front.medals.record_week("2026-W46", 300_000);
        front.medals.record_week("2026-W45", 280_000);
        assert_eq!(front.world_wants(), ["weekly:2026-W46", "weekly:2026-W45"]);
        let cuts = [
            ("1", 250_000),
            ("10", 270_000),
            ("25", 290_000),
            ("50", 310_000),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let board = Board {
            board: pick.board(),
            riders: 40,
            cuts,
            ..Default::default()
        };
        front.world_answer(&pick.board(), BoardState::Ready(board), &view);
        assert!(
            front
                .profile_details()
                .iter()
                .any(|l| l.0 == "Your Best:" && l.1 == "05:00.00 · Top 50% · 40 riders"),
            "{:?}",
            front.profile_details()
        );
        // The submission's exact percentile wins over the band.
        front.world.weekly_percentile = Some(("2026-W46".into(), 37));
        assert!(
            front
                .profile_details()
                .iter()
                .any(|l| l.1 == "05:00.00 · Top 37% · 40 riders")
        );
        match front.run("weeklystart", &view, &mut settings) {
            Some(Event::Race {
                environment,
                quad,
                laps,
                cpu,
                difficulty,
                lap_attack,
                ..
            }) => {
                assert_eq!(environment, pick.course);
                assert!(pick.class.admits(quad));
                assert_eq!(
                    (laps, cpu, difficulty, lap_attack),
                    (pick.laps(), 0, 1, false)
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_profile_overview_shows_medal_and_badge_counts() {
        let (mut front, view, _) = front();
        front.medals.record_lap("nat01", 43_000);
        front.medals.record_lap("sx01", 100_000);
        front.medals.weeks.insert(
            "2026-W45".into(),
            atv_core::medals::WeekResult {
                value: 1,
                badge: Some(atv_core::medals::Badge::Top10),
            },
        );
        front.show("profile-overview", &view);
        let screen = front.screen(None, false, None, &|_| None, None, (None, None), 0.);
        assert_eq!(screen.medal_strip, Some(([1, 0, 0, 1], [0, 1, 0, 0, 0])));
    }
}
