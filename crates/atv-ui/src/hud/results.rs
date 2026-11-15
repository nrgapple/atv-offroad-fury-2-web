//! The race-over results: Race Results with the summary and the standings,
//! the original race-over menu (Continue opens Restart / Quit Event, Hide
//! Menu toggles the results while the riders drive on) and Unhide Menu.
//! Keys, controller buttons and pointers drive it as web/race.js did.
//!
//! The look is the exact-build `Overlays.pb [RaceOver Menu]` page and its
//! kin (`[ChampionshipRaceOver Menu]`, `[Objective Results Menu]`, the
//! `Quit?` page, `[NameEntry Menu]`): the pause overlay's panel, title strip
//! and navigation pill (crate::menu), the standings in its ruled columns.
use super::Viewport;
use crate::{
    color::Color,
    layout::{Content, El, Paint, margins, sides},
    menu,
    text::{Font, Fonts, TextStyle},
};
use taffy::prelude::*;

/// Hit ids of the buttons.
pub const CONTINUE: u32 = 30;
pub const HIDE: u32 = 31;
pub const RETRY: u32 = 32;
pub const QUIT: u32 = 33;
pub const SHOW: u32 = 34;
pub const NEXT_ROUND: u32 = 35;
pub const FAME_SELECT: u32 = 36;
/// Replay (#542, a port addition): the whole race again from its start.
pub const REPLAY: u32 = 37;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Menu {
    #[default]
    Results,
    Quit,
    /// Overlays.pb `[NameEntry Menu]`: the Hall of Fame name, before the
    /// results (`001f1740` → `00217e40`).
    NameEntry,
}

/// The Hall of Fame records the player broke and the name being entered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fame {
    /// "Fastest Lap Time", "2nd Fastest 5 Lap Race Time"… (`00333360`).
    pub broken: Vec<String>,
    pub name: String,
    /// The menu the entry returns to (Race Results, or an objective's).
    pub then: Menu,
    /// Split screen: the player (1-4) naming their records, after
    /// "Congratulations" (Text4, the seat's name `002deb30`).
    pub player: Option<u8>,
}

/// What the host runs for the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Restart: reload the event (a new roster draw, no pre-race camera).
    Retry,
    /// Quit Event: back to Single Player (or the Championship page).
    Quit,
    /// Replay: the instant replay over the results.
    Replay,
    /// Continue Championship: the series' next round.
    NextRound,
    /// The Hall of Fame name was entered (`Results::fame`).
    FameName,
}

/// Where a recorded championship round leaves the series.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Round {
    /// Short of the points: Restart replays it.
    Replay,
    /// Advanced: Continue Championship races the next round.
    Next,
    /// The series ended.
    Over,
}

/// race.js `raceTime`: M:SS.hh, dashes when unknown.
pub fn race_time(value: Option<f32>) -> String {
    match value.filter(|v| v.is_finite()) {
        None => "--:--.--".into(),
        Some(v) => format!(
            "{}:{:0>5}",
            (v / 60.).floor() as i64,
            format!("{:.2}", v.rem_euclid(60.))
        ),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Results {
    /// The results have been shown for this race (`resultShown`).
    pub shown: bool,
    /// Hide Menu: the Show Results button replaces the panel.
    pub hidden: bool,
    pub menu: Menu,
    /// The focused button shows its ring (keyboard or controller use).
    pub focus_visible: bool,
    /// The focused button among those shown (`setMenu` focuses the first).
    pub focus: usize,
    pad_armed: bool,
    /// Controller left/right held (focus moves on a new press).
    pad_steer: i32,
    /// The standings, fixed when the race ended: rider, total time, best
    /// lap, stunt score and Profile Points (the original RaceOver Menu rows).
    pub rows: Vec<[String; 5]>,
    pub player_row: Option<usize>,
    /// A failed restart's message (shown in the summary).
    pub error: Option<String>,
    /// The host can return to the menus (Quit Event shown).
    pub can_quit: bool,
    /// The race-over menu offers Replay (solo and split screen).
    pub replay: bool,
    /// The player's place, race stunt score and awarded Profile Points.
    pub award: Option<(usize, f32, i32)>,
    /// The profile's Profile Points after this race, once the host saved it.
    pub profile_total: Option<i64>,
    /// A championship round: its context and each slot's place (the
    /// player, then the CPUs).
    pub championship: Option<(atv_core::championship::Round, Vec<usize>)>,
    /// What the round did, once the host recorded it.
    pub championship_line: Option<String>,
    /// The recorded round's next step (the original ChampionshipRaceOver
    /// menu: Continue Championship / Quit to Championship Menu).
    pub round: Option<Round>,
    /// A Freestyle objective's results (Overlays.pb `[Objective Results
    /// Menu]`): the summary lines replace the standings; Restart Objective
    /// and Continue (back to Select Objective) replace the race choices.
    pub objective: Option<String>,
    /// Classic uses timed-event wording on the shared Freestyle results.
    pub classic: bool,
    /// A mini-game's over page (`HockeyOver Menu`): its title; the summary
    /// is `objective`'s, with Restart Game and Quit to Main Menu.
    pub game: Option<String>,
    /// A room game's page closes with Continue (riding on in the room)
    /// instead of Quit to Main Menu.
    pub game_continue: bool,
    /// No profile was loaded: the race earned nothing and nothing was saved.
    pub no_profile: bool,
    /// Hall of Fame records this race set, and their name.
    pub fame: Option<Fame>,
    /// The world boards' answer for this race ("World #3 Fastest Lap!").
    pub world_line: Option<String>,
    /// The best lap's course medal and whether it is new
    /// (research/GHOST_CHALLENGES.md), revealed over `medal_reveal` 0..1.
    pub medal: Option<(atv_core::medals::Medal, bool)>,
    pub medal_reveal: f32,
}

/// Seconds the results' medal takes to pop in.
pub const MEDAL_REVEAL_SECONDS: f32 = 0.45;

impl Results {
    /// The race finished: show the results once (`draw`'s first finished
    /// frame) with the standings of that moment. True the first time, when
    /// the host credits the award to the profile.
    pub fn finish(&mut self, frame: &atv_core::race::Frame) -> bool {
        use atv_core::profile::{CourseAwards, Event, race_award_with_combos};
        if self.shown {
            return false;
        }
        self.championship = frame.championship.map(|round| (round, frame.ranks.clone()));
        self.championship_line = None;
        self.round = None;
        self.no_profile = false;
        // A championship round's course table, already chosen for the
        // series and difficulty.
        let course = self.championship.as_ref().map(|(round, _)| CourseAwards {
            am_normal: round.awards,
            ..Default::default()
        });
        let event = match &course {
            Some(awards) => Event::Championship {
                awards,
                pro: false,
                hard: false,
            },
            None => Event::Single,
        };
        self.shown = true;
        self.hidden = false;
        self.menu = Menu::Results;
        self.focus = 0;
        self.pad_armed = false;
        self.player_row = frame.standings.iter().position(|s| s.player);
        // 001f1e30: only the human racer earns Profile Points.
        self.award = frame.standings.iter().find(|s| s.player).map(|s| {
            let combos = frame.player_stunts.combo_points;
            let award = race_award_with_combos(event, s.position, s.stunt, combos);
            (s.position, s.stunt, award)
        });
        self.profile_total = None;
        self.fame = None;
        self.world_line = None;
        self.medal = None;
        self.medal_reveal = 0.;
        self.rows = frame
            .standings
            .iter()
            .map(|s| {
                [
                    format!("{}. {}", s.position, s.name),
                    race_time(s.time),
                    race_time(s.best),
                    format!("{}", s.stunt.max(0.) as i64),
                    match self.award.filter(|_| s.player) {
                        Some((.., points)) => points.to_string(),
                        None => "-".into(),
                    },
                ]
            })
            .collect();
        true
    }

    /// Online competition earns no Profile Points (USA manual pp. 36–37):
    /// the panel shows none.
    pub fn no_award(&mut self) {
        self.award = None;
        if let Some(row) = self.player_row.and_then(|i| self.rows.get_mut(i)) {
            row[4] = "-".into();
        }
    }

    /// No profile is loaded: `001f1e30` gives the rider no Profile Points
    /// and nothing is recorded.
    pub fn mark_no_profile(&mut self) {
        self.no_profile = true;
        if let Some(award) = &mut self.award {
            award.2 = 0;
        }
        if let Some(row) = self.player_row.and_then(|i| self.rows.get_mut(i)) {
            row[4] = "0".into();
        }
    }

    /// The original "and %d Profile Points!" line after the summary.
    pub fn points_line(&self) -> String {
        if self.no_profile {
            // The original's alert (0x431440).
            return " · Remember, your progress is not being saved!".into();
        }
        let championship = self
            .championship_line
            .as_deref()
            .map_or(String::new(), |line| format!(" · {line}"));
        championship
            + &match (self.award, self.profile_total) {
                (Some((.., points)), Some(total)) => {
                    format!(" · +{points} Profile Points ({total} total)")
                }
                (Some((.., points)), None) => format!(" · +{points} Profile Points"),
                _ => String::new(),
            }
            + &self.world_suffix()
    }

    /// The world boards' line after a summary (a race's, or a Freestyle
    /// objective's).
    pub fn world_suffix(&self) -> String {
        self.world_line
            .as_deref()
            .map_or(String::new(), |line| format!(" · {line}"))
    }

    /// The championship line for a recorded round.
    pub fn describe_round(
        series: &atv_core::championship::Series,
        outcome: &atv_core::championship::Outcome,
    ) -> String {
        let name = series.name;
        let round = outcome.round + 1;
        match &outcome.finished {
            Some((Some(trophy), reward, opened)) => {
                let mut line = format!("{name} over: {} trophy", trophy.name());
                if *reward > 0 {
                    line += &format!(" and {reward} Profile Points!");
                }
                for key in opened {
                    line += &format!(" {} unlocked.", unlocked_name(key));
                }
                line
            }
            Some((None, ..)) => format!(
                "{name} over: not enough championship points for a trophy ({} pts).",
                outcome.total
            ),
            None if outcome.advanced => format!(
                "{name} round {round} of {}: +{} championship points ({} total).",
                outcome.rounds, outcome.race_points[0], outcome.total
            ),
            None => format!(
                "{name} round {round} of {}: {} of {} points needed to advance. Restart to try again.",
                outcome.rounds,
                outcome.total + outcome.race_points[0],
                outcome.required
            ),
        }
    }

    /// A finished Freestyle objective: its results with Restart Objective
    /// and Continue.
    pub fn show_objective(&mut self, summary: String) {
        *self = Self {
            shown: true,
            menu: Menu::Quit,
            can_quit: true,
            objective: Some(summary),
            ..Default::default()
        };
    }

    /// A finished mini-game (the original `HockeyOver Menu`/`TagOver
    /// Menu`): `title`, the summary lines, Restart Game and Quit to Main
    /// Menu.
    pub fn show_game(&mut self, title: &str, summary: String) {
        self.show_objective(summary);
        self.game = Some(title.into());
    }

    /// The race set Hall of Fame records: the name entry comes first,
    /// starting on `name`.
    pub fn enter_fame(&mut self, broken: Vec<String>, name: &str) {
        self.enter_player_fame(broken, name, None);
    }

    /// Split screen: player `player`'s (1-4) name entry; the players are
    /// asked one after another (`00217e40`, `002c8630`).
    pub fn enter_player_fame(&mut self, broken: Vec<String>, name: &str, player: Option<u8>) {
        if broken.is_empty() {
            return;
        }
        self.fame = Some(Fame {
            broken,
            name: name.into(),
            then: if self.menu == Menu::NameEntry {
                Menu::Results
            } else {
                self.menu
            },
            player,
        });
        self.menu = Menu::NameEntry;
        self.hidden = false;
        self.focus = 0;
    }

    /// The name entry's text: printable ASCII, at most eight characters
    /// (nine-byte fields).
    pub fn set_fame_name(&mut self, text: &str) {
        if let Some(fame) = &mut self.fame {
            fame.name = text
                .chars()
                .filter(|c| c.is_ascii_graphic() || *c == ' ')
                .take(atv_core::hall_of_fame::NAME_MAX)
                .collect();
        }
    }

    /// The panel or the Show Results button is up.
    pub fn active(&self) -> bool {
        self.shown
    }

    /// The buttons shown, in focus order.
    pub fn buttons(&self) -> Vec<u32> {
        if self.hidden {
            return vec![SHOW];
        }
        match (self.menu, self.round) {
            (Menu::NameEntry, _) => vec![FAME_SELECT],
            (Menu::Results, _) if self.replay => vec![CONTINUE, REPLAY, HIDE],
            (Menu::Results, _) => vec![CONTINUE, HIDE],
            // An advanced or finished round cannot be raced again.
            (Menu::Quit, Some(Round::Next)) if self.can_quit => vec![NEXT_ROUND, QUIT],
            (Menu::Quit, Some(Round::Next)) => vec![NEXT_ROUND],
            (Menu::Quit, Some(Round::Over)) if self.can_quit => vec![QUIT],
            (Menu::Quit, _) if self.can_quit => vec![RETRY, QUIT],
            (Menu::Quit, _) => vec![RETRY],
        }
    }

    /// The focused button.
    pub fn focused(&self) -> u32 {
        let buttons = self.buttons();
        buttons[self.focus.min(buttons.len() - 1)]
    }

    /// Tab and the arrows move between the buttons (the browser's focus
    /// order), wrapping.
    fn move_focus(&mut self, step: i32) {
        let n = self.buttons().len() as i32;
        self.focus = (self.focus.min(n as usize - 1) as i32 + step).rem_euclid(n) as usize;
    }

    /// race.js `act`; a new menu or the Show/Hide toggle focuses its
    /// first button.
    pub fn act(&mut self, id: u32) -> Option<Command> {
        match id {
            CONTINUE => self.menu = Menu::Quit,
            HIDE => self.hidden = true,
            SHOW => self.hidden = false,
            RETRY => return Some(Command::Retry),
            REPLAY if self.replay => return Some(Command::Replay),
            NEXT_ROUND => return Some(Command::NextRound),
            FAME_SELECT if self.menu == Menu::NameEntry => {
                self.menu = self.fame.as_ref().map_or(Menu::Results, |f| f.then);
                self.focus = 0;
                return Some(Command::FameName);
            }
            QUIT if self.can_quit => return Some(Command::Quit),
            _ => return None,
        }
        self.focus = 0;
        None
    }

    fn back(&mut self) {
        self.menu = Menu::Results;
        self.focus = 0;
    }

    /// Escape and Enter while the results are up (the focused button is the
    /// menu's first: Enter activates it); true when the key was taken.
    pub fn key(&mut self, code: &str) -> (bool, Option<Command>) {
        if !self.shown {
            return (false, None);
        }
        self.focus_visible = true;
        if self.menu == Menu::NameEntry {
            return self.name_key(code);
        }
        match code {
            "Escape" => {
                if self.menu == Menu::Quit {
                    self.back();
                } else {
                    self.act(if self.hidden { SHOW } else { HIDE });
                }
                (true, None)
            }
            "Enter" | "NumpadEnter" => {
                let id = self.focused();
                (true, self.act(id))
            }
            "Tab" => {
                self.move_focus(1);
                (true, None)
            }
            "ArrowLeft" | "ArrowUp" if !self.hidden => {
                self.move_focus(-1);
                (true, None)
            }
            "ArrowRight" | "ArrowDown" if !self.hidden => {
                self.move_focus(1);
                (true, None)
            }
            _ => (false, None),
        }
    }

    /// The controller letter picker (`dir` 1 up, 2 down, 3 add, 4 delete).
    fn pick_letter(&mut self, dir: i32) {
        const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-. ";
        let Some(fame) = &self.fame else { return };
        let mut name: Vec<char> = fame.name.chars().collect();
        match dir {
            3 => name.push('A'),
            4 => {
                name.pop();
            }
            _ => {
                let letters: Vec<char> = LETTERS.chars().collect();
                let last = name.pop().unwrap_or(' ');
                let at = letters.iter().position(|c| *c == last).unwrap_or(0) as i32;
                let step = if dir == 1 { 1 } else { -1 };
                let n = letters.len() as i32;
                name.push(letters[(at + step).rem_euclid(n) as usize]);
            }
        }
        let name: String = name.into_iter().collect();
        self.set_fame_name(&name);
    }

    /// Typing the Hall of Fame name: letters, digits, space and `-`,
    /// Backspace, and Enter to accept.
    fn name_key(&mut self, code: &str) -> (bool, Option<Command>) {
        let typed = if let Some(letter) = code.strip_prefix("Key") {
            letter.chars().next().filter(|_| letter.len() == 1)
        } else if let Some(digit) = code.strip_prefix("Digit") {
            digit.chars().next().filter(|_| digit.len() == 1)
        } else {
            match code {
                "Space" => Some(' '),
                "Minus" => Some('-'),
                "Period" => Some('.'),
                _ => None,
            }
        };
        if let Some(c) = typed {
            let name = self.fame.as_ref().map_or(String::new(), |f| f.name.clone());
            self.set_fame_name(&format!("{name}{c}"));
            return (true, None);
        }
        match code {
            "Backspace" => {
                if let Some(fame) = &mut self.fame {
                    fame.name.pop();
                }
                (true, None)
            }
            "Enter" | "NumpadEnter" => (true, self.act(FAME_SELECT)),
            "Escape" | "Tab" | "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => (true, None),
            _ => (false, None),
        }
    }

    /// Standard-layout A/B/X edges (race.js polls `activeGamepad`); the
    /// D-pad or left stick moves the focus and A activates it.
    pub fn pad(&mut self, buttons: &[f32], axes: &[f32]) -> Option<Command> {
        if !self.shown {
            return None;
        }
        let pressed = |i: usize| buttons.get(i).is_some_and(|v| *v > 0.5);
        let axis = |i: usize| axes.get(i).copied().unwrap_or(0.);
        if self.menu == Menu::NameEntry {
            // The letter picker: Up/Down step the last letter, Right adds
            // one, Left deletes one (authored; the original's entry cells
            // are not recovered).
            let dir = if pressed(12) || axis(1) < -0.55 {
                1
            } else if pressed(13) || axis(1) > 0.55 {
                2
            } else if pressed(15) || axis(0) > 0.55 {
                3
            } else if pressed(14) || axis(0) < -0.55 {
                4
            } else {
                0
            };
            if dir != 0 && dir != self.pad_steer {
                self.focus_visible = true;
                self.pick_letter(dir);
            }
            self.pad_steer = dir;
        }
        if self.menu != Menu::NameEntry {
            let steer = if pressed(14) || pressed(12) || axis(0) < -0.55 || axis(1) < -0.55 {
                -1
            } else if pressed(15) || pressed(13) || axis(0) > 0.55 || axis(1) > 0.55 {
                1
            } else {
                0
            };
            if steer != 0 && steer != self.pad_steer && !self.hidden {
                self.focus_visible = true;
                self.move_focus(steer);
            }
            // (The letter picker keeps its own held direction: a held
            // D-pad steps one letter, not one every other frame.)
            self.pad_steer = steer;
        }
        let held = pressed(0) || pressed(1) || pressed(2);
        if !held {
            self.pad_armed = true;
            return None;
        }
        if !self.pad_armed {
            return None;
        }
        self.pad_armed = false;
        self.focus_visible = true;
        if self.menu == Menu::NameEntry {
            // A accepts the name (B and X do nothing here).
            return pressed(0).then(|| self.act(FAME_SELECT)).flatten();
        }
        let id = if self.hidden {
            SHOW
        } else if pressed(2) {
            HIDE
        } else if pressed(1) {
            if self.menu == Menu::Quit {
                self.back();
                return None;
            }
            HIDE
        } else {
            self.focused()
        };
        self.act(id)
    }
}

/// "Pro Supercross" or "Amateur Supercross Expert" for a save key.
fn unlocked_name(key: &str) -> String {
    let (series, difficulty) = key.split_once('-').unwrap_or((key, "normal"));
    let name = atv_core::championship::series(series).map_or(series, |s| s.name);
    if difficulty == "expert" {
        format!("{name} Expert")
    } else {
        name.to_string()
    }
}

fn display(size: f32, weight: u16, italic: bool) -> Font {
    Font::display(size, weight, italic)
}

fn text(value: &str, style: TextStyle, line_height: f32) -> El {
    El::new(Style::default()).content(Content::Text {
        text: value.into(),
        style,
        line_height,
        wrap: false,
    })
}

/// The `[RaceOver Menu]` page's geometry in CSS px. The source lays it out
/// on a 640x448 screen; `s` scales its numbers, as Pause's overlay does.
#[derive(Clone, Copy)]
struct Look {
    s: f32,
    /// The panel's width (source 59..581: 522).
    panel: f32,
    /// A phone on its side.
    short: bool,
    /// The overlay lettering (`tradegothic_boldcondensed_22s`).
    font: f32,
    /// A table row (source rows 24 apart).
    row: f32,
    /// Where the lettering starts inside the panel (source x 75 - 59).
    inset: f32,
    /// The navigation pill's height: 32 at source scale, a touch target.
    nav: f32,
}

impl Look {
    fn new(view: &Viewport) -> Self {
        let (w, h) = (view.width, view.height);
        let room = w - view.safe[1] - view.safe[3];
        let narrow = h > w || room < 560.;
        let short = !narrow && h <= 500.;
        let s = (w / 640.).min(h / 448.).clamp(0.8, 1.6);
        let panel = if narrow {
            room - 20.
        } else {
            (522. * (h / 448.).clamp(0.8, 1.6)).min(room - 2. * (w * 0.032).clamp(16., 44.))
        };
        let font = (22. * s * 0.8).clamp(15., 24.);
        Self {
            s,
            panel,
            short,
            font,
            row: (24. * s).max(font * 1.25),
            inset: (16. * s).max(12.),
            nav: (32. * s).max(44.),
        }
    }
}

/// The overlay's unselected lettering: 200 grey with its drop shadow; black
/// on the capsule.
fn lettering(size: f32, color: Color, selected: bool) -> TextStyle {
    let style = TextStyle::new(
        display(size, 700, false),
        if selected { Color::BLACK } else { color },
    )
    .spacing(size * 0.05);
    if selected {
        style
    } else {
        style.shadow(1., 2., 0., Color::hex("#000c"))
    }
}

/// The overlay's gold lettering (the name entry's prompt, 218,192,31): the
/// player's own row, which the source does not mark.
const OVERLAY_GOLD: Color = Color::rgb(218, 192, 31);

/// The page title (`tradegothic_bci_neg1fs_32ds`) on `overlay_titlebkg`'s
/// faint strip, as Pause draws it.
fn heading(look: &Look, title: &str) -> El {
    let size = (32. * look.s).clamp(24., 46.) * if look.short { 0.85 } else { 1. };
    El::new(Style {
        align_self: Some(AlignSelf::START),
        flex_shrink: 0.,
        size: Size {
            width: percent(0.49),
            height: auto(),
        },
        min_size: Size {
            width: auto(),
            height: auto(),
        },
        margin: margins(0., 0., if look.short { 4. } else { 8. * look.s }, 0.),
        padding: sides(size * 0.06, 28. * look.s, size * 0.04, 14. * look.s),
        ..Default::default()
    })
    .paint(Paint::fill(menu::title_strip()))
    .content(Content::Text {
        text: title.into(),
        style: TextStyle::new(display(size, 800, true), Color::hex("#e0e0e0"))
            .spacing(size * 0.01)
            .shadow(2., 2., 0., Color::hex("#000d")),
        line_height: size * 1.05,
        wrap: false,
    })
}

/// The results' medal row: its coin popping in, the medal's name and NEW on
/// the gold pill when it beat the course's medal before. A fixed height, so
/// the panel does not move while the coin grows.
fn medal_row(look: &Look, medal: atv_core::medals::Medal, new: bool, reveal: f32) -> El {
    let size = 34. * crate::medal::pop(reveal);
    let color = Color::hex(medal.color());
    let mut row = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        flex_shrink: 0.,
        gap: Size {
            width: length(10.),
            height: length(0.),
        },
        min_size: Size {
            width: auto(),
            height: length(42.),
        },
        padding: sides(0., look.inset, 0., look.inset),
        margin: margins(0., 0., 4., 0.),
        ..Default::default()
    })
    .child(
        El::new(Style {
            size: Size {
                width: length(42.),
                height: length(42.),
            },
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            flex_shrink: 0.,
            ..Default::default()
        })
        .child(crate::medal::coin(medal, size)),
    )
    .child(text(
        medal.label(),
        TextStyle::new(display(22., 800, true), color).shadow(2., 2., 0., Color::hex("#000c")),
        24.,
    ));
    if new {
        row = row.child(
            El::new(Style {
                padding: sides(2., 8., 2., 8.),
                ..Default::default()
            })
            .paint(Paint::fill(menu::capsule()).radius(9.))
            .child(text(
                "NEW",
                TextStyle::new(display(13., 800, false), Color::BLACK).spacing(0.8),
                15.,
            )),
        );
    }
    row
}

/// The standings as `[RaceOver Menu]` sets them: the places (x 70) and
/// names (x 92), then Total Time, Best Lap, Stunt Score and Profile Points
/// centred in four columns under two-line labels, each column's left edge a
/// faint white rule (`white` at alpha 64, 2 px). Rows are 24 apart.
fn table(look: &Look, state: &Results, fonts: &mut Fonts) -> El {
    let s = look.s;
    let cell = lettering(look.font, menu::CLASSIC_TEXT, false);
    let mine = lettering(look.font, OVERLAY_GOLD, false);
    let header_line = look.font * 1.05;
    // Two label lines (source 146 and 166), then the rows from 198.
    let header = 2. * header_line + 8. * s;
    let labels: [[&str; 2]; 4] = [
        ["Total", "Time"],
        ["Best", "Lap"],
        ["Stunt", "Score"],
        ["Profile", "Points"],
    ];
    // The columns are 92 wide at source scale; a narrow panel shares what
    // the names leave, never less than the widest value.
    let inner = look.panel - 4. - 2. * (11. * s).max(8.);
    let mut widest = 0f32;
    for row in &state.rows {
        for value in &row[1..] {
            widest = widest.max(fonts.measure(value, &cell.font, cell.letter_spacing).width);
        }
    }
    for pair in labels {
        for label in pair {
            widest = widest.max(fonts.measure(label, &cell.font, cell.letter_spacing).width);
        }
    }
    let column = (92. * s).min((inner - 96.) / 4.).max(widest + 8.);
    let place = (22. * s).max(fonts.measure("5.", &cell.font, cell.letter_spacing).width + 4.);
    let row_box = || {
        El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            flex_shrink: 0.,
            size: Size {
                width: auto(),
                height: length(look.row),
            },
            ..Default::default()
        })
    };
    let style_for = |r: usize| {
        if state.player_row == Some(r) {
            mine.clone()
        } else {
            cell.clone()
        }
    };
    let mut names = El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        flex_grow: 1.,
        flex_shrink: 1.,
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        padding: sides(header, 6., 0., 0.),
        ..Default::default()
    });
    for (r, row) in state.rows.iter().enumerate() {
        let style = style_for(r);
        let (number, name) = row[0]
            .split_once(". ")
            .map_or((String::new(), row[0].as_str()), |(n, rest)| {
                (format!("{n}."), rest)
            });
        names = names.child(
            row_box()
                .child(
                    El::new(Style {
                        flex_shrink: 0.,
                        size: Size {
                            width: length(place),
                            height: auto(),
                        },
                        ..Default::default()
                    })
                    .child(text(&number, style.clone(), look.font)),
                )
                .child(super::classic::ellipsis(name, style, look.font)),
        );
    }
    let mut table = El::new(Style {
        display: Display::Flex,
        flex_shrink: 0.,
        padding: sides(0., (11. * s).max(8.), 0., (11. * s).max(8.)),
        margin: margins(0., 0., 4., 0.),
        ..Default::default()
    })
    .child(names);
    let centred = |child: El, height: f32| {
        El::new(Style {
            display: Display::Flex,
            justify_content: JustifyContent::CENTER,
            align_items: AlignItems::CENTER,
            flex_shrink: 0.,
            size: Size {
                width: auto(),
                height: length(height),
            },
            ..Default::default()
        })
        .child(child)
    };
    for (c, pair) in labels.iter().enumerate() {
        let mut col = El::new(Style {
            position: Position::Relative,
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.,
            size: Size {
                width: length(column),
                height: auto(),
            },
            padding: sides(0., 0., 0., 2.),
            ..Default::default()
        })
        .child(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(0.),
                    top: LengthPercentageAuto::length(-4. * s),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::length(-4. * s),
                },
                size: Size {
                    width: length(2.),
                    height: auto(),
                },
                ..Default::default()
            })
            .paint(Paint::fill(Color::hex("#ffffff40"))),
        );
        for label in pair {
            col = col.child(centred(text(label, cell.clone(), header_line), header_line));
        }
        col = col.child(El::new(Style {
            flex_shrink: 0.,
            size: Size {
                width: auto(),
                height: length(8. * s),
            },
            ..Default::default()
        }));
        for (r, row) in state.rows.iter().enumerate() {
            col = col.child(centred(
                text(&row[c + 1], style_for(r), look.font),
                look.row,
            ));
        }
        table = table.child(col);
    }
    table
}

/// A choice in the navigation pill (`overlay_navbar`, where the source
/// shows its X Continue and Square Hide Menu hints): 200 grey, on the
/// capsule while focused.
fn nav_item(look: &Look, id: u32, label: &str, focused: bool) -> El {
    let size = (22. * look.s * 0.8).clamp(15., 21.);
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        flex_shrink: 0.,
        min_size: Size {
            width: length(44.),
            height: length(look.nav),
        },
        padding: sides(0., 12. * look.s.max(1.), 0., 12. * look.s.max(1.)),
        ..Default::default()
    })
    .id(id)
    .paint(if focused {
        Paint::fill(menu::capsule()).radius(look.nav / 2.)
    } else {
        Paint::default()
    })
    .child(text(
        label,
        lettering(size, menu::CLASSIC_TEXT, focused),
        size,
    ))
}

/// The navigation pill: black, a grey outline, round ends.
fn nav(look: &Look, max: f32, items: Vec<El>) -> El {
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        flex_wrap: FlexWrap::Wrap,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: length(4.),
            height: length(0.),
        },
        flex_shrink: 0.,
        max_size: Size {
            width: length(max),
            height: auto(),
        },
        padding: sides(0., 12. * look.s, 0., 12. * look.s),
        border: sides(2., 2., 2., 2.),
        ..Default::default()
    })
    .paint(Paint {
        radii: [look.nav / 2. + 2.; 4],
        fill: Some(Color::hex("#000000f2").into()),
        border: [2.; 4],
        border_color: Color::hex("#a6a6a6"),
        ..Default::default()
    })
    .children(items)
}

/// A row of the `Quit?` page (Restart, Quit Event at x 134, 25 apart):
/// the gold `menuhighlight` capsule behind the focused one.
fn row(look: &Look, id: u32, label: &str, focused: bool) -> El {
    let height = look.nav.max(25. * look.s);
    let size = (22. * look.s * 0.85).clamp(16., 24.);
    let mut el = El::new(Style {
        position: Position::Relative,
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        flex_shrink: 0.,
        min_size: Size {
            width: length(0.),
            height: length(height),
        },
        padding: sides(0., 32. * look.s + 14., 0., 75. * look.s),
        ..Default::default()
    })
    .id(id);
    if focused {
        let pill = (size * 1.55).min(height - 2.);
        el = el.child(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(33. * look.s),
                    top: LengthPercentageAuto::length((height - pill) / 2.),
                    right: LengthPercentageAuto::length(32. * look.s),
                    bottom: LengthPercentageAuto::auto(),
                },
                size: Size {
                    width: auto(),
                    height: length(pill),
                },
                ..Default::default()
            })
            .paint(Paint::fill(menu::capsule()).radius(pill / 2.)),
        );
    }
    el.child(text(
        label,
        lettering(size, menu::CLASSIC_TEXT, focused),
        size,
    ))
}

/// A choice's label.
fn label(state: &Results, id: u32) -> &'static str {
    match id {
        CONTINUE => "Continue",
        HIDE => "Hide Menu",
        REPLAY => "Replay",
        SHOW => "Unhide Menu",
        RETRY if state.game.is_some() => "Restart Game",
        RETRY if state.classic => "Restart Event",
        RETRY if state.objective.is_some() => "Restart Objective",
        RETRY => "Restart",
        NEXT_ROUND => "Continue Championship",
        FAME_SELECT => "Select",
        _ if state.game_continue => "Continue",
        // Treasure Hunt / King of the Hills return to Games (Continue).
        _ if state.game.as_deref() == Some("Race Results") => "Continue",
        _ if state.game.is_some() => "Quit to Main Menu",
        _ if state.objective.is_some() => "Continue",
        _ if state.championship.is_some() => "Quit to Championship Menu",
        _ => "Quit Event",
    }
}

/// The overlay page (`[RaceOver Menu]` and its kin) over the riders who
/// drive on, or Unhide Menu's pill while it is hidden. The panel is the
/// pause overlay's (`overlay_bkg01` tinted 10,10,10 in a black rim); the
/// host centres it vertically once laid out.
pub fn element(view: &Viewport, state: &Results, summary: &str, fonts: &mut Fonts) -> El {
    let look = Look::new(view);
    let root = |child: El| {
        El::new(Style {
            size: Size {
                width: length(view.width),
                height: length(view.height),
            },
            ..Default::default()
        })
        .child(child)
    };
    let focused = state.focused();
    if state.hidden {
        // The source shows Square "Unhide Menu" where the pill was; the pill
        // keeps it a touch target over the ride.
        let mut pill = nav(
            &look,
            view.width - 16.,
            vec![nav_item(
                &look,
                SHOW,
                label(state, SHOW),
                state.focus_visible,
            )],
        );
        pill.style.position = Position::Absolute;
        pill.style.inset = taffy::Rect {
            left: LengthPercentageAuto::auto(),
            top: LengthPercentageAuto::auto(),
            right: LengthPercentageAuto::length(16f32.max(view.safe[1] + 12.)),
            bottom: LengthPercentageAuto::length(18f32.max(view.safe[2] + 12.)),
        };
        return root(pill);
    }
    let naming = state
        .fame
        .as_ref()
        .filter(|_| state.menu == Menu::NameEntry);
    let congratulations;
    let title = if let Some(player) = naming.and_then(|f| f.player) {
        congratulations = format!("Congratulations Player {player}");
        congratulations.as_str()
    } else if naming.is_some() {
        "Congratulations"
    } else if let Some(title) = &state.game {
        title.as_str()
    } else if state.classic {
        "Classic Freestyle"
    } else if state.objective.is_some() {
        "Objectives"
    } else if state.menu == Menu::Quit && state.championship.is_none() {
        // `[Quit Menu]`: "Quit?" over Restart / Quit Event.
        "Quit?"
    } else if state.championship.is_some() {
        "Championship Race Results"
    } else {
        "Race Results"
    };
    let s = look.s;
    // The race-over page keeps its choices in the navigation pill; the
    // `Quit?` page and the name entry list theirs as capsule rows.
    let in_nav = state.menu == Menu::Results;
    let half = look.nav / 2.;
    let summary_size = (16. * s * 0.85).clamp(14., 18.);
    let mut medal = state
        .medal
        .filter(|_| naming.is_none() && state.objective.is_none())
        .map(|(medal, new)| medal_row(&look, medal, new, state.medal_reveal));
    let mut panel =
        El::new(Style {
            position: Position::Absolute,
            inset: taffy::Rect {
                left: LengthPercentageAuto::length((view.width - look.panel) / 2.),
                top: LengthPercentageAuto::length(view.height / 2.),
                right: LengthPercentageAuto::auto(),
                bottom: LengthPercentageAuto::auto(),
            },
            size: Size {
                width: length(look.panel),
                height: auto(),
            },
            padding: sides(
                if look.short { 4. } else { 12. * s },
                0.,
                if in_nav { half + 8. } else { 10. * s },
                0.,
            ),
            border: sides(2., 2., 2., 2.),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..Default::default()
        })
        .paint(Paint {
            radii: [10. * s; 4],
            fill: Some(menu::PANEL_FILL.into()),
            border: [2.; 4],
            border_color: Color::BLACK,
            ..Default::default()
        })
        .child(if look.short && medal.is_some() {
            // A phone on its side: the medal beside the title strip, not on a
            // line of its own.
            let mut head = El::new(Style {
                display: Display::Flex,
                align_items: AlignItems::CENTER,
                justify_content: JustifyContent::SPACE_BETWEEN,
                flex_shrink: 0.,
                margin: margins(0., 0., 4., 0.),
                ..Default::default()
            })
            .child(heading(&look, title))
            .children(medal.take());
            head.children[0].style.margin = margins(0., 0., 0., 0.);
            head
        } else {
            heading(&look, title)
        })
        .children(naming.is_none().then(|| {
            El::new(Style {
                flex_shrink: 0.,
                margin: margins(0., 0., 6. * s, 0.),
                padding: sides(0., look.inset, 0., look.inset),
                ..Default::default()
            })
            .content(Content::Text {
                text: summary.into(),
                style: TextStyle::new(display(summary_size, 600, false), Color::hex("#dcdcdc"))
                    .shadow(1., 2., 0., Color::hex("#000c")),
                line_height: summary_size * 1.35,
                wrap: true,
            })
        }))
        .children(medal)
        .children(naming.into_iter().flat_map(|fame| fame_lines(&look, fame)))
        .children(naming.map(|fame| name_field(&look, &fame.name)));
    // The standings belong to Race Results; `Quit?` and the objectives'
    // pages show none.
    if state.objective.is_none() && naming.is_none() && state.menu == Menu::Results {
        panel = panel.child(table(&look, state, fonts));
    }
    // The focused choice sits on the capsule, as Pause's selection does.
    let focus = |id: u32| id == focused;
    if in_nav {
        let items = state
            .buttons()
            .into_iter()
            .map(|id| nav_item(&look, id, label(state, id), focus(id)))
            .collect();
        // Source 276..610 on a panel ending at 581: the pill runs past the
        // panel's right edge where the screen has room.
        let room = (view.width - view.safe[1] - view.safe[3] - look.panel) / 2.;
        let past = (29. * s).min(room - 6.).max(-14.);
        let mut pill = nav(&look, look.panel + past - 8., items);
        pill.style.position = Position::Absolute;
        pill.style.inset = taffy::Rect {
            left: LengthPercentageAuto::auto(),
            top: LengthPercentageAuto::auto(),
            right: LengthPercentageAuto::length(-past - 2.),
            bottom: LengthPercentageAuto::length(-half - 4.),
        };
        panel = panel.child(pill);
    } else {
        panel = panel.children(
            state
                .buttons()
                .into_iter()
                .map(|id| row(&look, id, label(state, id), focus(id))),
        );
    }
    root(panel)
}

/// `[NameEntry Menu]` Text5–Text11: the broken records and the prompt
/// (the prompt in the overlay's gold, 218,192,31).
fn fame_lines(look: &Look, fame: &Fame) -> Vec<El> {
    let size = look.font;
    let plain = lettering(size, menu::CLASSIC_TEXT, false);
    let record = lettering(size, Color::WHITE, false);
    let gold = lettering(size, OVERLAY_GOLD, false);
    let line = |child: El, top: f32| {
        El::new(Style {
            flex_shrink: 0.,
            margin: margins(top, 0., 0., 0.),
            padding: sides(0., look.inset, 0., look.inset),
            ..Default::default()
        })
        .child(child)
    };
    let mut lines = vec![line(
        text("You Have Broken These Records:", plain, size * 1.3),
        0.,
    )];
    lines.extend(
        fame.broken
            .iter()
            .take(5)
            .map(|l| line(text(&format!("  {l}"), record.clone(), size * 1.3), 0.)),
    );
    lines.push(line(
        text("Enter your name for the Hall of Fame:", gold, size * 1.3),
        8.,
    ));
    lines
}

/// The name cell (Text2, nine bytes) with its caret, on the overlay's
/// navigation pill.
fn name_field(look: &Look, name: &str) -> El {
    El::new(Style {
        flex_shrink: 0.,
        margin: margins(4., look.inset, 6., look.inset),
        padding: sides(6., 14., 6., 14.),
        align_self: Some(AlignSelf::START),
        min_size: Size {
            width: length(180.),
            height: auto(),
        },
        border: sides(2., 2., 2., 2.),
        ..Default::default()
    })
    .paint(Paint {
        radii: [20.; 4],
        fill: Some(Color::hex("#000000f2").into()),
        border: [2.; 4],
        border_color: Color::hex("#a6a6a6"),
        ..Default::default()
    })
    .child(text(
        &format!("{name}_"),
        TextStyle::new(display(22., 700, false), OVERLAY_GOLD).spacing(1.),
        26.,
    ))
}

/// The panel is centred with `translate(-50%, -50%)`: its height is only
/// known after layout, so the host lays it out once and shifts it up.
pub fn summary(frame: &atv_core::race::Frame, error: Option<&str>) -> String {
    if let Some(error) = error {
        return error.into();
    }
    format!(
        "Place {} of {} · {} · Best {}",
        frame.position,
        frame.field,
        race_time(Some(frame.elapsed)),
        race_time(frame.best)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hall_of_fame_name_entry_comes_first_and_takes_keys() {
        let mut r = Results {
            shown: true,
            ..Default::default()
        };
        r.enter_fame(vec![], "ACE");
        assert!(r.fame.is_none(), "no records, no name entry");
        r.enter_fame(vec!["Fastest Lap Time".into()], "ACE");
        assert_eq!(r.menu, Menu::NameEntry);
        assert_eq!(r.buttons(), [FAME_SELECT]);
        for code in ["Backspace", "Backspace", "KeyB", "Digit2", "Space", "KeyZ"] {
            assert_eq!(r.key(code), (true, None));
        }
        // Escape does not hide the entry; the name keeps 8 characters.
        r.key("Escape");
        assert_eq!(r.menu, Menu::NameEntry);
        for _ in 0..10 {
            r.key("KeyQ");
        }
        assert_eq!(r.fame.as_ref().unwrap().name, "AB2 ZQQQ");
        assert_eq!(r.key("Enter"), (true, Some(Command::FameName)));
        assert_eq!(r.menu, Menu::Results);
        assert_eq!(r.buttons(), [CONTINUE, HIDE]);
        // The controller's letter picker: Right adds A, Up steps it to B,
        // Down twice to the space before A, Left deletes it.
        r.enter_fame(vec!["Longest Jump".into()], "AC");
        let mut buttons = [0f32; 16];
        let mut tap = |r: &mut Results, i: usize| {
            buttons[i] = 1.;
            r.pad(&buttons, &[]);
            buttons[i] = 0.;
            r.pad(&buttons, &[]);
        };
        tap(&mut r, 15);
        tap(&mut r, 12);
        assert_eq!(r.fame.as_ref().unwrap().name, "ACB");
        tap(&mut r, 13);
        tap(&mut r, 13);
        assert_eq!(r.fame.as_ref().unwrap().name, "AC ");
        tap(&mut r, 14);
        assert_eq!(r.fame.as_ref().unwrap().name, "AC");
        // A held D-pad direction steps once.
        let mut held = [0f32; 16];
        held[14] = 1.;
        for _ in 0..4 {
            r.pad(&held, &[]);
        }
        assert_eq!(r.fame.as_ref().unwrap().name, "A");
        r.pad(&[0f32; 16], &[]);
        assert_eq!(r.menu, Menu::NameEntry);
        // The controller's A accepts too.
        r.enter_fame(vec!["Longest Jump".into()], "ACE");
        assert_eq!(r.pad(&[0.], &[]), None);
        assert_eq!(r.pad(&[1.], &[]), Some(Command::FameName));
    }
    #[test]
    fn split_name_entries_follow_one_another_with_the_player() {
        let mut r = Results {
            shown: true,
            ..Default::default()
        };
        r.enter_player_fame(vec!["Fastest Lap Time".into()], "ANN", Some(1));
        assert_eq!(r.fame.as_ref().unwrap().player, Some(1));
        assert_eq!(r.key("Enter"), (true, Some(Command::FameName)));
        assert_eq!(r.menu, Menu::Results);
        // The host opens player 2's entry next; it still returns to the
        // results, not to player 1's entry.
        r.enter_player_fame(vec!["Longest Jump".into()], "Player 2", Some(2));
        let fame = r.fame.as_ref().unwrap();
        assert_eq!((fame.player, fame.then), (Some(2), Menu::Results));
        assert_eq!(r.menu, Menu::NameEntry);
        assert_eq!(r.key("Enter"), (true, Some(Command::FameName)));
        assert_eq!(r.menu, Menu::Results);
    }

    #[test]
    fn race_time_matches_race_js() {
        assert_eq!(race_time(None), "--:--.--");
        assert_eq!(race_time(Some(65.432)), "1:05.43");
        assert_eq!(race_time(Some(108.96)), "1:48.96");
        assert_eq!(race_time(Some(5.)), "0:05.00");
    }

    #[test]
    fn menu_follows_race_js() {
        let mut r = Results {
            shown: true,
            can_quit: true,
            ..Default::default()
        };
        assert_eq!(r.key("Enter"), (true, None));
        assert_eq!(r.menu, Menu::Quit);
        assert_eq!(r.key("Escape"), (true, None));
        assert_eq!(r.menu, Menu::Results);
        r.key("Escape");
        assert!(r.hidden);
        r.key("Enter");
        assert!(!r.hidden);
        r.act(CONTINUE);
        assert_eq!(r.key("Enter"), (true, Some(Command::Retry)));
        // Controller: a press needs a release first; X hides, A shows.
        let mut p = Results {
            shown: true,
            ..Default::default()
        };
        assert_eq!(p.pad(&[1., 0., 0.], &[]), None);
        assert!(p.menu == Menu::Results);
        p.pad(&[0., 0., 0.], &[]);
        p.pad(&[0., 0., 1.], &[]);
        assert!(p.hidden);
        p.pad(&[0., 0., 0.], &[]);
        p.pad(&[1., 0., 0.], &[]);
        assert!(!p.hidden);
    }

    #[test]
    fn championship_rounds_offer_the_original_choices() {
        let mut r = Results {
            shown: true,
            can_quit: true,
            menu: Menu::Quit,
            round: Some(Round::Next),
            ..Default::default()
        };
        assert_eq!(r.buttons(), vec![NEXT_ROUND, QUIT]);
        assert_eq!(r.key("Enter"), (true, Some(Command::NextRound)));
        r.round = Some(Round::Over);
        assert_eq!(r.buttons(), vec![QUIT]);
        r.round = Some(Round::Replay);
        assert_eq!(r.buttons(), vec![RETRY, QUIT]);
        r.menu = Menu::Results;
        r.replay = true;
        assert_eq!(r.buttons(), vec![CONTINUE, REPLAY, HIDE]);
        assert_eq!(r.act(REPLAY), Some(Command::Replay));
        r.menu = Menu::Quit;
    }

    #[test]
    fn focus_reaches_every_button() {
        // Keyboard: Tab/arrows move the focus, Enter activates it.
        let mut r = Results {
            shown: true,
            can_quit: true,
            ..Default::default()
        };
        assert_eq!(r.key("ArrowRight"), (true, None));
        assert_eq!(r.focused(), HIDE);
        r.key("Enter");
        assert!(r.hidden);
        assert_eq!(r.focused(), SHOW);
        r.key("Enter");
        assert_eq!(r.focused(), CONTINUE);
        r.key("Enter");
        assert_eq!(r.menu, Menu::Quit);
        r.key("Tab");
        assert_eq!(r.focused(), QUIT);
        assert_eq!(r.key("Enter"), (true, Some(Command::Quit)));
        r.key("Tab");
        assert_eq!(r.key("Enter"), (true, Some(Command::Retry)));
        // Controller: D-pad right then A reaches Quit Event.
        let mut p = Results {
            shown: true,
            can_quit: true,
            menu: Menu::Quit,
            ..Default::default()
        };
        let mut buttons = [0.; 17];
        p.pad(&buttons, &[0.; 4]);
        buttons[15] = 1.;
        p.pad(&buttons, &[0.; 4]);
        assert_eq!(p.focused(), QUIT);
        buttons[15] = 0.;
        p.pad(&buttons, &[0.; 4]);
        buttons[0] = 1.;
        assert_eq!(p.pad(&buttons, &[0.; 4]), Some(Command::Quit));
        // Without a frontend's Quit, Restart is the only choice.
        p.can_quit = false;
        assert_eq!(p.buttons(), vec![RETRY]);
        assert_eq!(p.focused(), RETRY);
    }

    /// The race-over page on a desktop, a phone on its side and a phone held
    /// upright: every choice is a 44 px touch target on screen, the standings
    /// and the medal fit, and the panel (centred by the host, never above
    /// the top) ends above the screen's bottom with its pill.
    #[test]
    fn race_over_page_fits_phones_and_desktop() {
        use crate::paint::{DisplayList, Item};
        let rows: Vec<[String; 5]> = (1..=5)
            .map(|p| {
                [
                    format!("{p}. Rider With A Long Name {p}"),
                    "12:05.43".into(),
                    "1:48.96".into(),
                    "123456".into(),
                    if p == 2 { "1234".into() } else { "-".into() },
                ]
            })
            .collect();
        let summary = "Place 2 of 5 · 6:05.43 · Best 1:48.96 · +1234 Profile Points (98765 total) · World #3 Fastest Lap!";
        for (w, h, safe) in [
            (1280., 720., [0.; 4]),
            (844., 390., [0., 47., 21., 47.]),
            (390., 844., [47., 0., 34., 0.]),
            (360., 640., [0.; 4]),
        ] {
            let view = Viewport {
                width: w,
                height: h,
                scale: 2.,
                safe,
                ..Default::default()
            };
            for menu in [Menu::Results, Menu::Quit] {
                let state = Results {
                    shown: true,
                    can_quit: true,
                    replay: true,
                    rows: rows.clone(),
                    player_row: Some(1),
                    menu,
                    medal: Some((atv_core::medals::Medal::Gold, true)),
                    medal_reveal: 1.,
                    ..Default::default()
                };
                let el = element(&view, &state, summary, &mut Fonts::new());
                let (mut list, mut hits) = (DisplayList::default(), Vec::new());
                crate::layout::render(&el, &mut Fonts::new(), w, h, &mut list, &mut hits);
                let Some(Item::Box(panel)) = list.items.first() else {
                    panic!("the panel is drawn first");
                };
                let dy = (-panel.rect.h / 2.).max(safe[0] + 6. - panel.rect.y);
                let bottom = panel.rect.y + dy + panel.rect.h;
                for id in state.buttons() {
                    let (rect, _) = hits.iter().find(|(_, i)| *i == id).expect("choice drawn");
                    assert!(
                        rect.h >= 44. && rect.w >= 44.,
                        "{w}x{h} {menu:?} {id}: {rect:?}"
                    );
                    assert!(
                        rect.x >= safe[3] && rect.right() <= w - safe[1] + 0.5,
                        "{w}x{h} {menu:?} {id}: {rect:?}"
                    );
                    assert!(
                        rect.y + dy + rect.h <= h - safe[2] + 0.5,
                        "{w}x{h} {menu:?} {id} below the screen: {rect:?} dy {dy}"
                    );
                }
                assert!(
                    bottom <= h - safe[2],
                    "{w}x{h} {menu:?}: panel ends at {bottom}"
                );
            }
        }
    }
}
