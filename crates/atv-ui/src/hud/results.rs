//! The race-over results (web/race.js `RaceOverlay`; game.css + hud.css
//! `.race-results`, `.race-show`): Race Results with the summary and the
//! standings, the original race-over menu (Continue opens Restart / Quit
//! Event, Hide Menu toggles the results while the riders drive on) and the
//! Show Results button. Keys, controller buttons and pointers drive it as
//! race.js does.
use super::Viewport;
use crate::{
    color::{Color, Gradient},
    layout::{Content, El, Paint, margins, sides},
    menu::gold_pill,
    paint::BoxShadow,
    text::{Font, Fonts, TextStyle},
};
use taffy::prelude::*;

/// Hit ids of the buttons.
pub const CONTINUE: u32 = 30;
pub const HIDE: u32 = 31;
pub const RETRY: u32 = 32;
pub const QUIT: u32 = 33;
pub const SHOW: u32 = 34;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Menu {
    #[default]
    Results,
    Quit,
}

/// What the host runs for the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Restart: reload the event (a new roster draw, no pre-race camera).
    Retry,
    /// Quit Event: back to Single Player.
    Quit,
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
    /// The player's place, race stunt score and awarded Profile Points.
    pub award: Option<(usize, f32, i32)>,
    /// The profile's Profile Points after this race, once the host saved it.
    pub profile_total: Option<i64>,
    /// A championship round: its context and each slot's place (the
    /// player, then the CPUs).
    pub championship: Option<(atv_core::championship::Round, Vec<usize>)>,
    /// What the round did, once the host recorded it.
    pub championship_line: Option<String>,
}

impl Results {
    /// The race finished: show the results once (`draw`'s first finished
    /// frame) with the standings of that moment. True the first time, when
    /// the host credits the award to the profile.
    pub fn finish(&mut self, frame: &atv_core::race::Frame) -> bool {
        use atv_core::profile::{CourseAwards, Event, race_award};
        if self.shown {
            return false;
        }
        self.championship = frame.championship.map(|round| (round, frame.ranks.clone()));
        self.championship_line = None;
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
        self.award = frame
            .standings
            .iter()
            .find(|s| s.player)
            .map(|s| (s.position, s.stunt, race_award(event, s.position, s.stunt)));
        self.profile_total = None;
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

    /// The original "and %d Profile Points!" line after the summary.
    pub fn points_line(&self) -> String {
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

    /// The panel or the Show Results button is up.
    pub fn active(&self) -> bool {
        self.shown
    }

    /// The buttons shown, in focus order.
    pub fn buttons(&self) -> Vec<u32> {
        if self.hidden {
            return vec![SHOW];
        }
        match self.menu {
            Menu::Results => vec![CONTINUE, HIDE],
            Menu::Quit if self.can_quit => vec![RETRY, QUIT],
            Menu::Quit => vec![RETRY],
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

    /// Standard-layout A/B/X edges (race.js polls `activeGamepad`); the
    /// D-pad or left stick moves the focus and A activates it.
    pub fn pad(&mut self, buttons: &[f32], axes: &[f32]) -> Option<Command> {
        if !self.shown {
            return None;
        }
        let pressed = |i: usize| buttons.get(i).is_some_and(|v| *v > 0.5);
        let axis = |i: usize| axes.get(i).copied().unwrap_or(0.);
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
        self.pad_steer = steer;
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

fn button(id: u32, label: &str, primary: bool, focused: bool) -> El {
    let paint = if primary {
        Paint::fill(gold_pill())
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#fffbe0")))
            .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")))
    } else {
        Paint::fill(Color::hex("#ffffff14"))
    };
    let mut paint = paint.radius(5.);
    if focused {
        paint = paint.shadow(BoxShadow {
            spread: 3.,
            ..BoxShadow::outer(0., 0., 0., Color::hex("#f6e061b3"))
        });
    }
    let color = if primary {
        Color::hex("#0d0b00")
    } else {
        Color::hex("#f2f2f2")
    };
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        min_size: Size {
            width: auto(),
            height: length(46.),
        },
        margin: margins(6., 6., 0., 0.),
        padding: sides(0., 22., 0., 22.),
        ..Default::default()
    })
    .id(id)
    .paint(paint)
    .child(text(
        label,
        TextStyle::new(display(17., 700, false), color).spacing(0.51),
        17.,
    ))
}

/// The panel, or the Show Results button while hidden.
pub fn element(view: &Viewport, state: &Results, summary: &str, fonts: &mut Fonts) -> El {
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
    if state.hidden {
        let label = "Show Results";
        let style = TextStyle::new(display(16., 700, false), Color::hex("#f2f2f2")).spacing(0.48);
        let width = fonts
            .measure(label, &style.font, style.letter_spacing)
            .width
            + 40.;
        let mut paint = Paint::fill(Color::hex("#0c0c0cdb"))
            .radius(5.)
            .shadow(BoxShadow::outer(0., 4., 0., Color::hex("#0009")));
        if state.focus_visible {
            paint = paint.shadow(BoxShadow {
                spread: 3.,
                ..BoxShadow::outer(0., 0., 0., Color::hex("#f6e061b3"))
            });
        }
        return root(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length((view.width - width) / 2.),
                    bottom: LengthPercentageAuto::length(18f32.max(view.safe[2] + 12.)),
                    top: LengthPercentageAuto::auto(),
                    right: LengthPercentageAuto::auto(),
                },
                display: Display::Flex,
                align_items: AlignItems::CENTER,
                min_size: Size {
                    width: auto(),
                    height: length(44.),
                },
                padding: sides(0., 20., 0., 20.),
                ..Default::default()
            })
            .id(SHOW)
            .paint(paint)
            .child(text(label, style, 16.)),
        );
    }
    let width = 620f32.min(view.width - 32.);
    let heading = TextStyle::new(display(26., 800, true), Color::hex("#f6f6f6"))
        .spacing(0.39)
        .shadow(2., 3., 0., Color::hex("#000a"));
    // The table: CSS auto layout shares the width by each column's
    // max-content width (cells pad 6px each side).
    let cell = TextStyle::new(display(16., 700, false), Color::hex("#e8e8e8"));
    let head = TextStyle::new(display(13., 700, false), Color::hex("#bdbdbd")).spacing(0.52);
    let headers = [
        "RIDER",
        "TOTAL TIME",
        "BEST LAP",
        "STUNT SCORE",
        "PROFILE POINTS",
    ];
    let mut widths = [0f32; 5];
    for (c, label) in headers.iter().enumerate() {
        widths[c] = fonts.measure(label, &head.font, head.letter_spacing).width + 12.;
    }
    for row in &state.rows {
        for (c, value) in row.iter().enumerate() {
            widths[c] = widths[c].max(fonts.measure(value, &cell.font, 0.).width + 12.);
        }
    }
    let cell_el = |value: &str, style: TextStyle, line: f32, right: bool| {
        El::new(Style {
            display: Display::Flex,
            justify_content: if right {
                JustifyContent::END
            } else {
                JustifyContent::START
            },
            align_items: AlignItems::CENTER,
            padding: sides(0., 6., 0., 6.),
            min_size: Size {
                width: length(0.),
                height: auto(),
            },
            ..Default::default()
        })
        .child(text(value, style, line))
    };
    let mut cells = Vec::new();
    for (c, label) in headers.iter().enumerate() {
        cells.push(cell_el(label, head.clone(), 22.1, c > 0));
    }
    for (r, row) in state.rows.iter().enumerate() {
        let style = if state.player_row == Some(r) {
            TextStyle::new(cell.font, Color::hex("#f1d84e"))
        } else {
            cell.clone()
        };
        for (c, value) in row.iter().enumerate() {
            cells.push(cell_el(value, style.clone(), 27.2, c > 0));
        }
    }
    let table = El::new(Style {
        display: Display::Grid,
        grid_template_columns: widths.iter().map(|w| minmax(length(*w), fr(*w))).collect(),
        ..Default::default()
    })
    .children(cells);
    let focused = state.focus_visible.then(|| state.focused());
    let buttons: Vec<El> = state
        .buttons()
        .into_iter()
        .map(|id| {
            let label = match id {
                CONTINUE => "Continue",
                HIDE => "Hide Menu",
                RETRY => "Restart",
                _ => "Quit Event",
            };
            button(
                id,
                label,
                matches!(id, CONTINUE | RETRY),
                focused == Some(id),
            )
        })
        .collect();
    let menu = El::new(Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        margin: margins(10., 0., 0., 0.),
        ..Default::default()
    })
    .children(buttons);
    let summary_style = TextStyle::new(display(16., 600, false), Color::hex("#d8d8d8"));
    let panel = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length((view.width - width) / 2.),
            top: LengthPercentageAuto::length(view.height / 2.),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(width),
            height: auto(),
        },
        padding: sides(0., 20., 18., 20.),
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        ..Default::default()
    })
    .paint(
        Paint::fill(Color::hex("#0c0c0cdb"))
            .radius(5.)
            .shadow(BoxShadow::outer(0., 6., 0., Color::hex("#0009")))
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff1a"))),
    )
    .child(
        El::new(Style {
            margin: margins(0., -20., 12., -20.),
            padding: sides(6., 20., 8., 20.),
            ..Default::default()
        })
        .paint(Paint::fill(Gradient::new(
            90.,
            &[
                (0., Color::hex("#ffffff38")),
                (0.55, Color::hex("#ffffff14")),
                (1., Color::hex("#fff0")),
            ],
        )))
        .child(text("Race Results", heading, 28.6)),
    )
    .child(
        El::new(Style {
            // The paragraph's 16px margins, collapsed with the heading's 12px.
            margin: margins(4., 0., 16., 0.),
            ..Default::default()
        })
        .content(Content::Text {
            text: summary.into(),
            style: summary_style,
            line_height: 22.4,
            wrap: true,
        }),
    )
    .child(table)
    .child(menu);
    root(panel)
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
}
