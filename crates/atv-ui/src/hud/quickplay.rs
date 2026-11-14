//! The Quick Play panel over a server-run room (research/QUICK_PLAY.md):
//! seats filling with a live countdown, the map vote, the picked round's
//! countdown and the podium with Rematch / Leave. A port addition; no text
//! walls (research/MULTIPLAYER_EXPERIENCE.md "UI rules"): short labels,
//! numbers, colors and timers. Touch targets are at least 48 px.
use super::Viewport;
use crate::{
    color::Color,
    layout::{Content, El, Paint, margins, sides},
    menu::gold_pill,
    paint::BoxShadow,
    text::{Font, TextStyle},
};
use taffy::prelude::*;

/// Hit ids: the four vote cards, then the buttons.
pub const CARD: u32 = 80;
pub const READY: u32 = 84;
pub const REMATCH: u32 = 85;
pub const LEAVE: u32 = 86;

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
    /// Watching this round, riding the next one.
    pub spectating: bool,
    /// Narrow (portrait phone) layout.
    pub narrow: bool,
}

/// The focusable buttons in order.
pub fn buttons(view: &View) -> Vec<u32> {
    match view.phase {
        Phase::Lobby => vec![READY],
        Phase::Vote => (0..view.cards.len() as u32).map(|i| CARD + i).collect(),
        Phase::Results => vec![REMATCH, LEAVE],
        _ => Vec::new(),
    }
}

fn font(size: f32, weight: u16) -> Font {
    Font::display(size, weight, false)
}

fn text(value: &str, size: f32, weight: u16, color: Color) -> El {
    El::new(Style::default()).content(Content::Text {
        text: value.into(),
        style: TextStyle::new(font(size, weight), color),
        line_height: size * 1.2,
        wrap: false,
    })
}

fn wrapped(value: &str, size: f32, weight: u16, color: Color) -> El {
    El::new(Style {
        max_size: Size {
            width: percent(1.),
            height: auto(),
        },
        ..Default::default()
    })
    .content(Content::Text {
        text: value.into(),
        style: TextStyle::new(font(size, weight), color),
        line_height: size * 1.15,
        wrap: true,
    })
}

fn row(gap: f32, wrap: bool) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        flex_wrap: if wrap {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        },
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: length(gap),
            height: length(gap),
        },
        ..Default::default()
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

fn focus_ring(paint: Paint) -> Paint {
    paint.shadow(BoxShadow {
        spread: 3.,
        ..BoxShadow::outer(0., 0., 0., Color::hex("#f6e061d9"))
    })
}

fn button(id: u32, label: &str, primary: bool, focused: bool) -> El {
    let mut paint = if primary {
        Paint::fill(gold_pill()).shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")))
    } else {
        Paint::fill(Color::hex("#ffffff1f"))
    }
    .radius(8.);
    if focused {
        paint = focus_ring(paint);
    }
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        min_size: Size {
            width: length(132.),
            height: length(52.),
        },
        padding: sides(0., 24., 0., 24.),
        ..Default::default()
    })
    .id(id)
    .paint(paint)
    .child(text(
        label,
        20.,
        800,
        if primary {
            Color::hex("#0d0b00")
        } else {
            Color::hex("#f2f2f2")
        },
    ))
}

/// Ping bars: green under 80 ms, yellow to 150, red above.
fn ping_bars(ping: u16) -> El {
    let (lit, color) = match ping {
        0..80 => (3, "#55d66b"),
        80..=150 => (2, "#f2c84b"),
        _ => (1, "#ef5b50"),
    };
    El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::END,
        gap: Size {
            width: length(2.),
            height: length(0.),
        },
        margin: margins(0., 0., 0., 6.),
        ..Default::default()
    })
    .children((0..3).map(move |i| {
        El::new(Style {
            size: Size {
                width: length(4.),
                height: length(5. + i as f32 * 4.),
            },
            ..Default::default()
        })
        .paint(Paint::fill(Color::hex(if i < lit { color } else { "#ffffff33" })).radius(1.))
    }))
}

fn chip(seat: Option<&SeatView>, narrow: bool) -> El {
    let width = if narrow { 150. } else { 136. };
    let style = Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::CENTER,
        size: Size {
            width: length(width),
            height: length(36.),
        },
        padding: sides(0., 10., 0., 10.),
        ..Default::default()
    };
    let Some(seat) = seat else {
        return El::new(style)
            .paint(Paint::fill(Color::hex("#ffffff0f")).radius(18.))
            .child(text("• • •", 16., 700, Color::hex("#ffffff59")));
    };
    let fill = if seat.me {
        Color::hex("#f6dc4a40")
    } else {
        Color::hex("#0000008c")
    };
    let mut name: String = seat.name.chars().take(11).collect();
    if seat.ready || seat.voted {
        name.push_str(" ✓");
    }
    let mut el = El::new(style).paint(Paint::fill(fill).radius(18.)).child(
        El::new(Style {
            flex_grow: 1.,
            min_size: Size {
                width: length(0.),
                height: auto(),
            },
            ..Default::default()
        })
        .child(text(&name, 16., 700, Color::hex("#f4f4f4"))),
    );
    if seat.cpu {
        el = el.child(
            El::new(Style {
                padding: sides(1., 6., 1., 6.),
                ..Default::default()
            })
            .paint(Paint::fill(Color::hex("#5b8def")).radius(4.))
            .child(text("CPU", 12., 800, Color::hex("#ffffff"))),
        );
    }
    if let Some(ping) = seat.ping {
        el = el.child(ping_bars(ping));
    }
    el
}

fn card(index: usize, card: &CardView, focused: bool, narrow: bool, big: bool) -> El {
    let width = if big {
        260.
    } else if narrow {
        150.
    } else {
        140.
    };
    let mode_color = match card.mode.as_str() {
        "Race" => "#f6dc4a",
        "Hills" => "#ef8b3c",
        "Treasure" => "#55d66b",
        "Stunts" => "#e86bd9",
        _ => "#9fb7ff",
    };
    let mut paint = Paint::fill(Color::hex(if card.chosen {
        "#f6dc4a33"
    } else {
        "#000000a6"
    }))
    .radius(10.);
    if card.chosen {
        paint.border = [2.; 4];
        paint.border_color = Color::hex("#f6dc4a");
    }
    if focused {
        paint = focus_ring(paint);
    }
    let mut el = El::new(Style {
        size: Size {
            width: length(width),
            height: auto(),
        },
        min_size: Size {
            width: auto(),
            height: length(if big { 110. } else { 96. }),
        },
        padding: sides(10., 10., 10., 10.),
        ..column(6.)
    })
    .paint(paint)
    .child(text(
        &card.mode,
        if big { 22. } else { 17. },
        800,
        Color::hex(mode_color),
    ))
    .child(wrapped(
        &card.track,
        if big { 20. } else { 15. },
        700,
        Color::hex("#f2f2f2"),
    ));
    if !big {
        el = el.id(CARD + index as u32).child(text(
            &if card.votes > 0 {
                format!("● {}", card.votes)
            } else {
                " ".into()
            },
            16.,
            800,
            Color::hex("#f6dc4a"),
        ));
    }
    el
}

fn podium(rows: &[RowView]) -> El {
    // 2nd, 1st, 3rd.
    let order = [1usize, 0, 2];
    let heights = [70., 96., 54.];
    let colors = ["#c9ced6", "#f6dc4a", "#d08a4b"];
    El::new(Style {
        align_items: AlignItems::END,
        ..row(8., false)
    })
    .children(order.iter().filter_map(|&place| {
        let r = rows.get(place)?;
        let name: String = r.name.chars().take(11).collect();
        Some(
            El::new(column(4.))
                .child(text(
                    &name,
                    16.,
                    800,
                    Color::hex(if r.me { "#f6dc4a" } else { "#f4f4f4" }),
                ))
                .child(text(&r.value, 15., 700, Color::hex("#dddddd")))
                .child(
                    El::new(Style {
                        size: Size {
                            width: length(92.),
                            height: length(heights[place]),
                        },
                        display: Display::Flex,
                        justify_content: JustifyContent::CENTER,
                        padding: sides(6., 0., 0., 0.),
                        ..Default::default()
                    })
                    .paint(Paint::fill(Color::hex(colors[place])).radius(6.))
                    .child(text(
                        &format!("{}", place + 1),
                        30.,
                        900,
                        Color::hex("#1a1a1a"),
                    )),
                ),
        )
    }))
}

/// The panel, or None when nothing shows (a round in play).
pub fn element(viewport: &Viewport, view: &View) -> Option<El> {
    let focused = |id: u32| view.focus_visible && buttons(view).get(view.focus) == Some(&id);
    let width = (viewport.width - 24.).clamp(0., 640.);
    let mut body = Vec::new();
    let header = |label: &str| {
        let mut el = El::new(Style {
            justify_content: JustifyContent::SPACE_BETWEEN,
            size: Size {
                width: percent(1.),
                height: auto(),
            },
            ..row(8., false)
        })
        .child(text(label, 22., 800, Color::hex("#f6f6f6")));
        if let Some(s) = view.seconds {
            el = el.child(text(&s.to_string(), 34., 900, Color::hex("#f6dc4a")));
        }
        el
    };
    match view.phase {
        Phase::Play => {
            if !view.spectating {
                return None;
            }
            body.push(
                El::new(Style {
                    padding: sides(6., 14., 6., 14.),
                    ..Default::default()
                })
                .paint(Paint::fill(Color::hex("#000000a6")).radius(16.))
                .child(text("Next round", 17., 800, Color::hex("#f6dc4a"))),
            );
        }
        Phase::Lobby => {
            body.push(header(&format!("{} · Waiting", view.playlist)));
            let chips = view
                .seats
                .iter()
                .map(|s| chip(Some(s), view.narrow))
                .chain((0..view.open).map(|_| chip(None, view.narrow)));
            body.push(El::new(row(6., true)).children(chips));
            body.push(button(
                READY,
                if view.ready { "Ready ✓" } else { "GO" },
                !view.ready,
                focused(READY),
            ));
        }
        Phase::Vote => {
            body.push(header("Vote"));
            body.push(
                El::new(row(8., true)).children(
                    view.cards
                        .iter()
                        .enumerate()
                        .map(|(i, c)| card(i, c, focused(CARD + i as u32), view.narrow, false)),
                ),
            );
            body.push(
                El::new(row(6., true))
                    .children(view.seats.iter().map(|s| chip(Some(s), view.narrow))),
            );
        }
        Phase::Countdown => {
            body.push(header("Next"));
            if let Some(picked) = &view.picked {
                body.push(card(0, picked, false, view.narrow, true));
            }
        }
        Phase::Results => {
            body.push(header("Results"));
            body.push(podium(&view.rows));
            let rest: Vec<El> = view
                .rows
                .iter()
                .enumerate()
                .skip(3)
                .map(|(i, r)| {
                    El::new(Style {
                        justify_content: JustifyContent::SPACE_BETWEEN,
                        size: Size {
                            width: length(260f32.min(width - 32.)),
                            height: auto(),
                        },
                        ..row(8., false)
                    })
                    .child(text(
                        &format!("{}  {}", i + 1, r.name.chars().take(14).collect::<String>()),
                        15.,
                        700,
                        Color::hex(if r.me { "#f6dc4a" } else { "#dddddd" }),
                    ))
                    .child(text(&r.value, 15., 700, Color::hex("#bbbbbb")))
                })
                .collect();
            if !rest.is_empty() {
                body.push(El::new(column(2.)).children(rest));
            }
            body.push(El::new(row(10., true)).children([
                button(REMATCH, "Rematch", !view.leaving, focused(REMATCH)),
                button(LEAVE, "Leave", view.leaving, focused(LEAVE)),
            ]));
        }
    }
    let panel = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length((viewport.width - width) / 2.),
            top: LengthPercentageAuto::length(56f32.max(viewport.safe[0] + 52.)),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(width),
            height: auto(),
        },
        padding: sides(12., 12., 14., 12.),
        ..column(12.)
    })
    .paint(if view.phase == Phase::Play {
        Paint::default()
    } else {
        Paint::fill(Color::hex("#101010c7")).radius(12.)
    })
    .children(body);
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
    Some(root.child(panel))
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
                    ..Default::default()
                })
                .collect(),
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
        // Nothing shows while a round runs, unless spectating.
        assert!(element(&view, &sample(Phase::Play)).is_none());
        let watching = View {
            spectating: true,
            ..sample(Phase::Play)
        };
        assert!(element(&view, &watching).is_some());
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
