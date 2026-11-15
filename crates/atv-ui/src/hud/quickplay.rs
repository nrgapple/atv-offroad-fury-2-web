//! The Quick Play panel over a server-run room (research/QUICK_PLAY.md):
//! seats filling with a live countdown, the map vote, the picked round's
//! countdown and the podium with Rematch / Leave. A private room's loop
//! uses the same panel: ready ticks, and for its host Start, the mode, CPU
//! fill and Skip; a rider watching the round cycles the riders it follows. A port addition; no text
//! walls (research/MULTIPLAYER_EXPERIENCE.md "UI rules"): short labels,
//! numbers, colors and timers. Touch targets are at least 48 px. Drawn with
//! the classic HUD pieces (research/UI_AUDIT_HUD.md).
use super::{Viewport, classic};
use crate::{
    color::Color,
    layout::{El, Paint, sides},
};
use taffy::prelude::*;

/// Hit ids: the four vote cards, then the buttons.
pub const CARD: u32 = 80;
pub const READY: u32 = 84;
pub const REMATCH: u32 = 85;
pub const LEAVE: u32 = 86;
/// Private rooms (slice C): the host's buttons, then the follow camera's.
pub const START: u32 = 90;
pub const MODE: u32 = 91;
pub const CPUS: u32 = 92;
pub const SKIP: u32 = 93;
pub const WATCH: u32 = 94;
/// The panel's hit ids are `CARD..=LAST`.
pub const LAST: u32 = WATCH;

/// What the panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Lobby,
    Vote,
    Countdown,
    Play,
    Results,
}

/// A seat chip.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SeatView {
    pub name: String,
    pub cpu: bool,
    pub me: bool,
    /// Others' round trip (ms); None for this rider or a CPU.
    pub ping: Option<u16>,
    pub ready: bool,
    pub voted: bool,
    /// A private room's host.
    pub host: bool,
}

/// A vote card.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CardView {
    /// The mode word ("Race", "Hills", "Treasure", "Stunts", "Random").
    pub mode: String,
    pub track: String,
    pub votes: u8,
    /// This rider's choice.
    pub chosen: bool,
}

/// A podium or results row.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RowView {
    pub name: String,
    /// The shown value (time or points, already counting up).
    pub value: String,
    pub cpu: bool,
    pub me: bool,
    pub dnf: bool,
    /// League points earned this round ("+35", counting up); None for
    /// CPUs, unverified rounds and riders without an online rider.
    pub points: Option<String>,
}

/// This rider's level after the round (research/ONLINE_IDENTITY.md).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LevelView {
    pub level: u16,
    /// 0-1 through the level, filling as the XP counts up.
    pub fill: f32,
    /// "+35 XP".
    pub gain: String,
    /// The weekly league tier's name.
    pub tier: String,
    /// The round reached a new tier / level (shown once counted).
    pub tier_up: bool,
    pub level_up: bool,
    /// 0-1 through the level-up beat (0 none).
    pub beat: f32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct View {
    pub phase: Phase,
    /// "Race" or "Stunts".
    pub playlist: String,
    /// Whole seconds left in the phase.
    pub seconds: Option<u32>,
    pub seats: Vec<SeatView>,
    /// Seats still open (drawn as empty chips).
    pub open: usize,
    pub cards: Vec<CardView>,
    /// The picked card (countdown).
    pub picked: Option<CardView>,
    pub rows: Vec<RowView>,
    pub ready: bool,
    /// Rematch chosen (the default focus on the podium).
    pub stay: bool,
    pub leaving: bool,
    /// Keyboard/controller focus: an index into [`buttons`].
    pub focus: usize,
    pub focus_visible: bool,
    /// 0-1: the win flash.
    pub flash: f32,
    /// The podium's level strip.
    pub level: Option<LevelView>,
    /// 0-1 through the level-up confetti.
    pub confetti: Option<f32>,
    /// Watching this round, riding the next one.
    pub spectating: bool,
    /// Narrow (portrait phone) layout.
    pub narrow: bool,
    /// A private room's loop (slice C).
    pub private: bool,
    /// This rider hosts it: Start, the mode, CPU fill and Skip.
    pub host: bool,
    /// The host's CPU fill (races).
    pub cpus: bool,
    /// The rider the follow camera watches while spectating.
    pub watching: Option<String>,
}

/// The focusable buttons in order.
pub fn buttons(view: &View) -> Vec<u32> {
    match view.phase {
        Phase::Lobby if view.host => {
            let mut ids = vec![START, MODE];
            if view.playlist == "Race" {
                ids.push(CPUS);
            }
            ids
        }
        Phase::Lobby => vec![READY],
        Phase::Vote => {
            let mut ids: Vec<u32> = (0..view.cards.len() as u32).map(|i| CARD + i).collect();
            if view.host {
                ids.push(SKIP);
            }
            ids
        }
        Phase::Results => vec![REMATCH, LEAVE],
        Phase::Play if view.spectating && view.watching.is_some() => vec![WATCH],
        _ => Vec::new(),
    }
}

fn column(gap: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: length(gap),
            height: length(gap),
        },
        ..Default::default()
    }
}

/// The phase's seconds as the race HUD letters its clock.
fn timer(seconds: Option<u32>) -> Option<El> {
    seconds.map(|s| classic::hud_text(s.to_string(), 30., classic::GOLD))
}

fn short(name: &str, max: usize) -> String {
    let mut out: String = name.chars().take(max).collect();
    if name.chars().count() > max {
        out.push('…');
    }
    out
}

/// The seats as Race Results rows: the rider, a HOST or CPU mark, the
/// ping and a tick once ready (or voted); open seats dimmed.
fn seats(view: &View) -> El {
    let tick = if view.phase == Phase::Vote {
        "VOTED"
    } else {
        "READY"
    };
    let mut lines: Vec<classic::Line> = view
        .seats
        .iter()
        .map(|s| classic::Line {
            cells: vec![
                s.name.clone(),
                if s.host {
                    "HOST".into()
                } else if s.cpu {
                    "CPU".into()
                } else {
                    String::new()
                },
                s.ping.map_or("-".into(), |p| p.to_string()),
                if s.ready || s.voted { "✓" } else { "" }.into(),
            ],
            me: s.me,
            dim: false,
        })
        .collect();
    lines.extend((0..view.open).map(|_| classic::Line {
        cells: vec!["Open".into()],
        me: false,
        dim: true,
    }));
    let column = |label, right| classic::Column { label, right };
    classic::standings(
        &[
            column("RIDER", false),
            column("", false),
            column("PING", true),
            column(tick, true),
        ],
        &lines,
    )
}

/// A vote card as a Select Track entry: the track, its mode in small gold
/// capitals and the votes in the HUD gold.
fn card(id: Option<u32>, card: &CardView, focused: bool) -> El {
    let votes = (card.votes > 0).then(|| {
        classic::hud_text(
            card.votes.to_string(),
            22.,
            if card.chosen {
                classic::INK
            } else {
                classic::GOLD
            },
        )
    });
    classic::course(id, &card.track, &card.mode, votes, card.chosen, focused)
}

/// The round's standings: place and rider, the time or points, and the
/// league points earned (Race Results' Profile Points column) when any are.
fn standings(view: &View) -> El {
    let league = view.rows.iter().any(|r| r.points.is_some());
    let lines: Vec<classic::Line> = view
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut cells = vec![format!("{}. {}", i + 1, r.name), r.value.clone()];
            if league {
                cells.push(r.points.clone().unwrap_or_else(|| "-".into()));
            }
            classic::Line {
                cells,
                me: r.me,
                dim: r.dnf,
            }
        })
        .collect();
    let mut columns = vec![
        classic::Column {
            label: "RIDER",
            right: false,
        },
        classic::Column {
            label: if view.playlist == "Race" {
                "TIME"
            } else {
                "POINTS"
            },
            right: true,
        },
    ];
    if league {
        columns.push(classic::Column {
            label: "LEAGUE",
            right: true,
        });
    }
    classic::standings(&columns, &lines)
}

/// Level n in the HUD gold, its bar filling (the menu sliders' track), the
/// XP gained and the league tier in small gold capitals.
fn level_strip(level: &LevelView, width: f32) -> El {
    let bar = 150f32.min(width - 200.).max(60.);
    let pulse = 1. + 0.25 * (level.beat * std::f32::consts::PI).sin().max(0.);
    let tier = format!("{}{}", level.tier, if level.tier_up { " ▲" } else { "" });
    El::new(Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: length(10.),
            height: length(4.),
        },
        padding: sides(2., 6., 2., 6.),
        ..Default::default()
    })
    .child(classic::hud_text(
        format!("Lv {}", level.level),
        22. * pulse,
        if level.level_up {
            classic::PLAYER
        } else {
            classic::GOLD
        },
    ))
    .child(classic::meter(level.fill, bar))
    .child(classic::hud_text(&level.gain, 17., classic::SILVER))
    .child(classic::caps(
        tier,
        if level.tier_up {
            classic::PLAYER
        } else {
            Color::hex("#e3c74a")
        },
    ))
    .children(
        level
            .level_up
            .then(|| classic::hud_text("LEVEL UP!", 20. * pulse, classic::PLAYER)),
    )
}

/// Gold and silver confetti falling over the screen, `t` 0-1 through the
/// beat. Pieces are placed by a fixed hash of their index, so the same frame
/// draws the same.
fn confetti(viewport: &Viewport, t: f32) -> El {
    const PIECES: u32 = 28;
    let colors = ["#f6dc4a", "#e5d360", "#fffaa1", "#b9b9b9", "#e9e9e9"];
    let fade = (1. - t).clamp(0., 1.);
    El::new(Style {
        position: Position::Absolute,
        size: Size {
            width: length(viewport.width),
            height: length(viewport.height),
        },
        ..Default::default()
    })
    .children((0..PIECES).map(move |i| {
        let h = i.wrapping_mul(2_654_435_761);
        let x = (h % 1000) as f32 / 1000.;
        let speed = 0.6 + ((h >> 10) % 100) as f32 / 250.;
        let sway = (t * 9. + i as f32).sin() * 14.;
        let y = -20. + t * speed * (viewport.height + 40.);
        let mut c = Color::hex(colors[i as usize % colors.len()]);
        c.a *= fade;
        El::new(Style {
            position: Position::Absolute,
            inset: taffy::Rect {
                left: LengthPercentageAuto::length(x * viewport.width + sway),
                top: LengthPercentageAuto::length(y),
                right: LengthPercentageAuto::auto(),
                bottom: LengthPercentageAuto::auto(),
            },
            size: Size {
                width: length(if i % 2 == 0 { 8. } else { 5. }),
                height: length(if i % 3 == 0 { 12. } else { 7. }),
            },
            ..Default::default()
        })
        .paint(Paint::fill(c).radius(1.5))
    }))
}

/// The panel, or None when nothing shows (a round in play). Built from the
/// Race Results panel, its standings and race-over buttons, Select Track
/// entries and the race HUD lettering (hud::classic).
pub fn element(viewport: &Viewport, view: &View) -> Option<El> {
    let focused = |id: u32| view.focus_visible && buttons(view).get(view.focus) == Some(&id);
    let button =
        |id: u32, label: &str, primary: bool| classic::button(id, label, primary, focused(id));
    let width = (viewport.width - 24.).clamp(0., 560.);
    // A portrait phone keeps its minimap (top right) clear when there is
    // room below it.
    let top = if view.narrow && viewport.height >= 640. {
        160f32.max(viewport.safe[0] + 150.)
    } else {
        56f32.max(viewport.safe[0] + 52.)
    };
    let panel =
        |title: &str| classic::panel(width).child(classic::heading(title, timer(view.seconds)));
    let content = match view.phase {
        Phase::Play => {
            if !view.spectating {
                return None;
            }
            // A race HUD sign, and the follow camera's rider.
            let mut el = El::new(Style {
                size: Size {
                    width: length(width),
                    height: auto(),
                },
                ..column(10.)
            })
            .child(classic::hud_text("NEXT ROUND", 22., classic::GOLD));
            if let Some(name) = &view.watching {
                el = el.child(button(WATCH, &format!("▸ {}", short(name, 11)), false));
            }
            el
        }
        Phase::Lobby => {
            let title = if view.private { "Lobby" } else { "Waiting" };
            let buttons = if view.host {
                let mut b = vec![
                    button(START, "Start", true),
                    button(MODE, &format!("{} ▸", view.playlist), false),
                ];
                if view.playlist == "Race" {
                    b.push(button(
                        CPUS,
                        if view.cpus { "CPUs ✓" } else { "No CPUs" },
                        false,
                    ));
                }
                b
            } else {
                let label = match (view.ready, view.private) {
                    (true, _) => "Ready ✓",
                    (false, true) => "Ready",
                    (false, false) => "GO",
                };
                vec![button(READY, label, !view.ready)]
            };
            panel(&format!("{} {title}", view.playlist))
                .child(seats(view))
                .child(classic::buttons(buttons))
        }
        Phase::Vote => {
            let mut el = panel("Vote").children(view.cards.iter().enumerate().map(|(i, c)| {
                let id = CARD + i as u32;
                card(Some(id), c, focused(id))
            }));
            // A phone shows who voted on the cards' counts alone.
            if !view.narrow {
                el = el.child(seats(view));
            }
            if view.host {
                el = el.child(classic::buttons(vec![button(SKIP, "Skip ▸", false)]));
            }
            el
        }
        Phase::Countdown => {
            panel("Next").children(view.picked.as_ref().map(|p| card(None, p, false)))
        }
        Phase::Results => panel("Results")
            .child(standings(view))
            .children(view.level.as_ref().map(|l| level_strip(l, width)))
            .child(classic::buttons(vec![
                // A choice made shows: ticked, on the gold pill with the
                // course list's ring (the podium also ends on its timer).
                classic::chosen(
                    button(
                        REMATCH,
                        if view.stay { "Rematch ✓" } else { "Rematch" },
                        !view.leaving,
                    ),
                    view.stay,
                ),
                classic::chosen(
                    button(
                        LEAVE,
                        if view.leaving { "Leave ✓" } else { "Leave" },
                        view.leaving,
                    ),
                    view.leaving,
                ),
            ])),
    };
    let mut placed = content;
    placed.style.position = Position::Absolute;
    placed.style.inset = taffy::Rect {
        left: LengthPercentageAuto::length((viewport.width - width) / 2.),
        top: LengthPercentageAuto::length(top),
        right: LengthPercentageAuto::auto(),
        bottom: LengthPercentageAuto::auto(),
    };
    let mut root = El::new(Style {
        size: Size {
            width: length(viewport.width),
            height: length(viewport.height),
        },
        ..Default::default()
    });
    if view.flash > 0. {
        root = root.child(
            El::new(Style {
                position: Position::Absolute,
                size: Size {
                    width: length(viewport.width),
                    height: length(viewport.height),
                },
                ..Default::default()
            })
            .paint(Paint::fill(Color::rgba(
                246,
                220,
                74,
                view.flash.clamp(0., 1.) * 0.45,
            ))),
        );
    }
    root = root.child(placed);
    if let Some(t) = view.confetti {
        root = root.child(confetti(viewport, t));
    }
    Some(root)
}

/// Keyboard, without taking the riding keys: Tab moves the focus, Enter
/// presses it, and 1-4 pick a vote card. Returns (taken, pressed button).
pub fn key(view: &mut View, code: &str) -> (bool, Option<u32>) {
    let ids = buttons(view);
    if ids.is_empty() {
        return (false, None);
    }
    if let Some(n) = code
        .strip_prefix("Digit")
        .and_then(|d| d.parse::<usize>().ok())
        .filter(|_| view.phase == Phase::Vote)
    {
        return match ids.get(n.wrapping_sub(1)) {
            Some(id) => {
                view.focus = n - 1;
                (true, Some(*id))
            }
            None => (false, None),
        };
    }
    match code {
        "Tab" => {
            view.focus_visible = true;
            view.focus = (view.focus + 1) % ids.len();
            (true, None)
        }
        "Enter" | "NumpadEnter" => (true, ids.get(view.focus.min(ids.len() - 1)).copied()),
        _ => (false, None),
    }
}

/// A controller beside the ride: D-pad left/right (14/15) move the focus
/// and A (0) presses it (never Leave), on each new press. `held` keeps the last state.
pub fn pad(view: &mut View, buttons_down: &[f32], held: &mut [bool; 3]) -> Option<u32> {
    let ids = buttons(view);
    let down = |i: usize| buttons_down.get(i).is_some_and(|v| *v > 0.5);
    let now = [down(14), down(15), down(0)];
    let fresh = |i: usize| now[i] && !held[i];
    let mut pressed = None;
    if !ids.is_empty() {
        if fresh(0) {
            view.focus_visible = true;
            view.focus = (view.focus + ids.len() - 1) % ids.len();
        }
        if fresh(1) {
            view.focus_visible = true;
            view.focus = (view.focus + 1) % ids.len();
        }
        // A is also the throttle: it never presses Leave.
        if fresh(2) && view.focus_visible {
            pressed = ids
                .get(view.focus.min(ids.len() - 1))
                .copied()
                .filter(|id| *id != LEAVE);
        }
    }
    *held = now;
    pressed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{paint::DisplayList, text::Fonts};

    fn viewport(width: f32, height: f32) -> Viewport {
        Viewport {
            width,
            height,
            scale: 1.,
            safe: [0.; 4],
            hover: false,
            coarse: true,
            touch_first: true,
        }
    }

    fn sample(phase: Phase) -> View {
        View {
            phase,
            playlist: "Race".into(),
            seconds: Some(12),
            seats: vec![
                SeatView {
                    name: "Dusty".into(),
                    me: true,
                    ..Default::default()
                },
                SeatView {
                    name: "Mud Hawk".into(),
                    ping: Some(120),
                    ..Default::default()
                },
                SeatView {
                    name: "Tony".into(),
                    cpu: true,
                    ..Default::default()
                },
            ],
            open: 1,
            cards: ["Race", "Hills", "Race", "Random"]
                .iter()
                .map(|m| CardView {
                    mode: m.to_string(),
                    track: "Prospect Springs".into(),
                    votes: 1,
                    chosen: *m == "Hills",
                })
                .collect(),
            rows: (0..5)
                .map(|i| RowView {
                    name: format!("R{i}"),
                    value: "1:02.50".into(),
                    points: (i % 2 == 0).then(|| "+45".into()),
                    ..Default::default()
                })
                .collect(),
            level: Some(LevelView {
                level: 12,
                fill: 0.4,
                gain: "+45 XP".into(),
                tier: "Platinum".into(),
                tier_up: true,
                level_up: true,
                beat: 0.5,
            }),
            confetti: Some(0.5),
            narrow: true,
            ..Default::default()
        }
    }

    /// Every button is a 48 px touch target and stays on a 360 px screen.
    #[test]
    fn buttons_are_big_enough_and_fit_a_portrait_phone() {
        let view = viewport(360., 740.);
        for phase in [Phase::Lobby, Phase::Vote, Phase::Countdown, Phase::Results] {
            let state = sample(phase);
            let el = element(&view, &state).unwrap();
            let (mut list, mut hits) = (DisplayList::default(), Vec::new());
            let mut fonts = Fonts::new();
            crate::layout::render(
                &el,
                &mut fonts,
                view.width,
                view.height,
                &mut list,
                &mut hits,
            );
            for id in buttons(&state) {
                let (rect, _) = hits.iter().find(|(_, h)| *h == id).expect("button drawn");
                assert!(rect.h >= 48. && rect.w >= 48., "{phase:?} {id}: {rect:?}");
                assert!(
                    rect.x >= 0. && rect.right() <= 360.5,
                    "{phase:?} {id}: {rect:?}"
                );
            }
        }
        // A private room's host and guest, and a spectator's follow camera.
        for phase in [Phase::Lobby, Phase::Vote, Phase::Play] {
            for host in [true, false] {
                let state = View {
                    private: true,
                    host,
                    spectating: true,
                    watching: Some("Mud Hawk".into()),
                    ..sample(phase)
                };
                let el = element(&view, &state).unwrap();
                let (mut list, mut hits) = (DisplayList::default(), Vec::new());
                let mut fonts = Fonts::new();
                crate::layout::render(
                    &el,
                    &mut fonts,
                    view.width,
                    view.height,
                    &mut list,
                    &mut hits,
                );
                let ids = buttons(&state);
                assert!(!ids.is_empty());
                for id in ids {
                    let (rect, _) = hits.iter().find(|(_, h)| *h == id).expect("button drawn");
                    assert!(rect.h >= 48. && rect.w >= 48., "{phase:?} {id}: {rect:?}");
                    assert!(
                        rect.x >= 0. && rect.right() <= 360.5,
                        "{phase:?} {id}: {rect:?}"
                    );
                }
            }
        }
        let host = View {
            private: true,
            host: true,
            ..sample(Phase::Lobby)
        };
        assert_eq!(buttons(&host), [START, MODE, CPUS]);
        assert_eq!(
            buttons(&View {
                phase: Phase::Vote,
                ..host.clone()
            })
            .last(),
            Some(&SKIP)
        );
        // Nothing shows while a round runs, unless spectating.
        assert!(element(&view, &sample(Phase::Play)).is_none());
        let watching = View {
            spectating: true,
            ..sample(Phase::Play)
        };
        assert!(element(&view, &watching).is_some());
    }

    /// Rematch shows the rider's choice once pressed.
    #[test]
    fn the_rematch_choice_shows() {
        let view = viewport(360., 740.);
        let labels = |state: &View| {
            let el = element(&view, state).unwrap();
            let (mut list, mut hits) = (DisplayList::default(), Vec::new());
            crate::layout::render(
                &el,
                &mut Fonts::new(),
                view.width,
                view.height,
                &mut list,
                &mut hits,
            );
            list.items
                .iter()
                .filter_map(|item| match item {
                    crate::paint::Item::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let podium = sample(Phase::Results);
        assert!(labels(&podium).iter().any(|t| t == "Rematch"));
        let stay = View {
            stay: true,
            ..podium
        };
        assert!(labels(&stay).iter().any(|t| t == "Rematch ✓"));
    }

    #[test]
    fn keys_and_pads_move_the_focus_and_press() {
        let mut state = sample(Phase::Vote);
        assert_eq!(key(&mut state, "Tab"), (true, None));
        assert_eq!(key(&mut state, "Enter"), (true, Some(CARD + 1)));
        assert_eq!(key(&mut state, "Digit4"), (true, Some(CARD + 3)));
        // Riding keys stay the ride's.
        assert_eq!(key(&mut state, "ArrowLeft"), (false, None));
        let mut play = sample(Phase::Play);
        assert_eq!(key(&mut play, "Enter"), (false, None));
        // A controller: right, then A (only after the focus shows).
        let mut state = sample(Phase::Results);
        let mut held = [false; 3];
        let mut b = [0f32; 16];
        b[0] = 1.;
        assert_eq!(pad(&mut state, &b, &mut held), None);
        let step = |b: &mut [f32; 16], i: usize, held: &mut [bool; 3], state: &mut View| {
            *b = [0.; 16];
            pad(state, b, held);
            b[i] = 1.;
            pad(state, b, held)
        };
        // Right onto Leave: A never presses it (A is the throttle).
        assert_eq!(step(&mut b, 15, &mut held, &mut state), None);
        assert_eq!(step(&mut b, 0, &mut held, &mut state), None);
        // Right again wraps to Rematch, which A presses.
        assert_eq!(step(&mut b, 15, &mut held, &mut state), None);
        assert_eq!(step(&mut b, 0, &mut held, &mut state), Some(REMATCH));
    }
}
