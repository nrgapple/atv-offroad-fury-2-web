//! Keyboard controls card (web/keyboard-help.js; game.css + hud.css
//! `#keyboard-help`). Opens on a player's first keyboard ride for 15 s and
//! toggles with H or the Keys button.
use super::{Viewport, tokens};
use crate::{
    color::{Color, Gradient},
    layout::{Content, El, Inline, Paint, margins, px, sides},
    paint::BoxShadow,
    text::{Font, TextStyle},
};
use taffy::prelude::*;

pub const SEEN_KEY: &str = "atv2-keyboard-help-seen-v1";
/// Seconds the card stays open on the first keyboard ride.
pub const FIRST_RIDE_SECONDS: f64 = 15.;

/// One row: action ids (paired rows group the n-th key of each) and text.
type Row = (&'static [&'static str], &'static str);

pub const SECTIONS: [(&str, &[Row]); 4] = [
    (
        "Drive",
        &[
            (&["throttle"], "Gas"),
            (&["brake"], "Brake · hold to reverse"),
            (&["steerLeft", "steerRight"], "Steer"),
        ],
    ),
    (
        "Rider",
        &[
            (&["preloadHold"], "Hold, release to jump"),
            (&["leanBack"], "Lean back · wheelie"),
            (&["leanForward"], "Lean forward"),
        ],
    ),
    (
        "Tricks",
        &[
            (
                &["stuntCircle", "stuntTriangle"],
                "In the air: hold, then tap a direction",
            ),
            (
                &["stuntL1"],
                "Add for combo tricks · alone + back/side: flip (Extras)",
            ),
        ],
    ),
    (
        "Other",
        &[
            (&["reset"], "Reset"),
            (&["camera"], "Camera"),
            (&["pause"], "Pause"),
        ],
    ),
];

/// keyboard-help.js `keyName`.
pub fn key_name(code: &str) -> String {
    let named = match code {
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "ShiftLeft" => "Shift",
        "ShiftRight" => "R Shift",
        "ControlLeft" => "Ctrl",
        "ControlRight" => "R Ctrl",
        "AltLeft" => "Alt",
        "AltRight" => "R Alt",
        "Escape" => "Esc",
        "Backspace" => "⌫",
        "Enter" => "Enter",
        "Space" => "Space",
        "Tab" => "Tab",
        "Slash" => "/",
        "Period" => ".",
        "Comma" => ",",
        "Semicolon" => ";",
        "Quote" => "'",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Minus" => "-",
        "Equal" => "=",
        "Backquote" => "`",
        _ => "",
    };
    if !named.is_empty() {
        return named.into();
    }
    if let Some(rest) = code.strip_prefix("Numpad") {
        return format!("Num {rest}");
    }
    code.strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))
        .unwrap_or(code)
        .into()
}

/// Each action's keyboard keys as card labels (keyboard-help.js `keysFor`'s
/// `(b.chord ?? [b]).map(a => keyName(a.key)).join('+')`), by action id.
pub fn action_keys(bindings: &atv_game::ride::bindings::Bindings) -> Vec<(String, Vec<String>)> {
    atv_game::ride::bindings::ACTIONS
        .iter()
        .map(|(action, id, _)| {
            let labels = bindings
                .keyboard_codes(*action)
                .iter()
                .map(|chord| {
                    chord
                        .iter()
                        .map(|c| key_name(c))
                        .collect::<Vec<_>>()
                        .join("+")
                })
                .collect();
            (id.to_string(), labels)
        })
        .collect()
}

/// keyboard-help.js `keysFor`: alternative key groups for `ids`.
pub fn keys_for(keys: &dyn Fn(&str) -> Vec<String>, ids: &[&str]) -> Vec<Vec<String>> {
    let per: Vec<Vec<String>> = ids.iter().map(|id| keys(id)).collect();
    let columns = per.iter().map(Vec::len).max().unwrap_or(0);
    (0..columns)
        .map(|i| per.iter().filter_map(|k| k.get(i).cloned()).collect())
        .collect()
}

fn body(size: f32, weight: u16) -> Font {
    Font::display(size, weight, false)
}

fn kbd(text: &str) -> Inline {
    Inline::Chip {
        text: text.into(),
        style: TextStyle::new(body(12., 700), tokens::HUD_GOLD),
        line_height: 15.6,
        padding: [2., 5., 2., 5.],
        min_width: 14.,
        margin: [0., 2.],
        paint: Paint::fill(Color::hex("#2e2e2e"))
            .radius(4.)
            .shadow(BoxShadow::inset(0., -2., 0., Color::hex("#000a")))
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff26"))),
    }
}

/// The card, or None while hidden.
pub fn element(view: &Viewport, key_names: &dyn Fn(&str) -> Vec<String>) -> El {
    let width = 640f32.min(view.width - 28.);
    let strut = (body(13., 400), 16.9);
    let heading = TextStyle::new(Font::display(20., 800, true), Color::hex("#f6f6f6"))
        .spacing(0.3)
        .shadow(2., 3., 0., Color::hex("#000a"));
    let note = TextStyle::new(body(12., 600), Color::hex("#c4c4c4"));
    let header = El::new(Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::BASELINE,
        justify_content: JustifyContent::SPACE_BETWEEN,
        gap: Size {
            width: px(12.),
            height: px(4.),
        },
        margin: margins(0., -16., 10., -16.),
        padding: sides(0., 16., 0., 0.),
        ..Default::default()
    })
    .child(
        El::new(Style {
            padding: sides(6., 20., 8., 16.),
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
        .content(Content::Text {
            text: "Keyboard controls".into(),
            style: heading,
            line_height: 22.,
            wrap: false,
        }),
    )
    .child(El::new(Style::default()).content(Content::Inline {
        strut: (body(12., 600), 15.6),
        runs: vec![
            Inline::Text {
                text: "Press ".into(),
                style: note.clone(),
                line_height: 15.6,
                margin: [0., 0.],
            },
            kbd("H"),
            Inline::Text {
                text: " to show or hide · remap in Pause → Settings → Controls".into(),
                style: note,
                line_height: 15.6,
                margin: [0., 0.],
            },
        ],
    }));
    let one_column = view.width <= 560.;
    let grid = El::new(Style {
        display: Display::Grid,
        grid_template_columns: if one_column {
            vec![minmax(length(0.), fr(1.))]
        } else {
            vec![minmax(length(0.), fr(1.)), minmax(length(0.), fr(1.))]
        },
        gap: Size {
            width: px(20.),
            height: px(8.),
        },
        ..Default::default()
    })
    .children(SECTIONS.iter().map(|(title, rows)| {
        let heading = El::new(Style {
            margin: margins(0., 0., 3., 0.),
            ..Default::default()
        })
        .content(Content::Text {
            text: title.to_uppercase(),
            style: TextStyle::new(body(12., 800), tokens::HUD_GOLD).spacing(1.68),
            line_height: 14.4,
            wrap: false,
        });
        let items = rows.iter().filter_map(|(ids, text)| {
            let groups = keys_for(key_names, ids);
            if groups.is_empty() {
                return None;
            }
            let mut runs = Vec::new();
            for (i, group) in groups.iter().enumerate() {
                if i > 0 {
                    runs.push(Inline::Text {
                        text: "/".into(),
                        style: TextStyle::new(body(11., 400), tokens::SILVER),
                        line_height: 14.3,
                        margin: [3., 3.],
                    });
                }
                runs.extend(group.iter().map(|k| kbd(k)));
            }
            Some(
                El::new(Style {
                    display: Display::Flex,
                    align_items: AlignItems::CENTER,
                    gap: Size {
                        width: px(10.),
                        height: px(10.),
                    },
                    padding: sides(2., 0., 2., 0.),
                    min_size: Size {
                        width: length(0.),
                        height: auto(),
                    },
                    ..Default::default()
                })
                .child(
                    El::new(Style {
                        flex_grow: 0.,
                        flex_shrink: 0.,
                        min_size: Size {
                            width: length(92.),
                            height: auto(),
                        },
                        ..Default::default()
                    })
                    .content(Content::Inline { strut, runs }),
                )
                .child(
                    El::new(Style {
                        min_size: Size {
                            width: length(0.),
                            height: auto(),
                        },
                        flex_shrink: 1.,
                        ..Default::default()
                    })
                    .content(Content::Text {
                        text: (*text).into(),
                        style: TextStyle::new(body(14., 600), Color::hex("#ececec")),
                        line_height: 18.2,
                        wrap: true,
                    }),
                ),
            )
        });
        El::new(Style {
            display: Display::Block,
            min_size: Size {
                width: length(0.),
                height: auto(),
            },
            ..Default::default()
        })
        .child(heading)
        .children(items)
    }));
    El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length((view.width - width) / 2.),
            bottom: LengthPercentageAuto::length(18f32.max(view.safe[2] + 12.)),
            top: LengthPercentageAuto::auto(),
            right: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(width),
            height: auto(),
        },
        padding: sides(0., 16., 14., 16.),
        display: Display::Block,
        ..Default::default()
    })
    .paint(
        Paint::fill(Color::hex("#0c0c0cdb"))
            .radius(5.)
            .shadow(BoxShadow::outer(0., 6., 0., Color::hex("#0009")))
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff1a"))),
    )
    .child(header)
    .child(grid)
}

#[cfg(test)]
mod tests {
    //! web/keyboard-help.test.mjs.
    use super::*;
    use atv_game::ride::bindings::{Action, Atom, Binding, Bindings};

    #[test]
    fn controls_card_names_keys_players_recognise() {
        let names: Vec<String> = [
            "KeyW",
            "ArrowUp",
            "ShiftLeft",
            "Space",
            "Escape",
            "Digit3",
            "Backspace",
        ]
        .into_iter()
        .map(key_name)
        .collect();
        assert_eq!(names, ["W", "↑", "Shift", "Space", "Esc", "3", "⌫"]);
        assert_eq!(key_name("Numpad7"), "Num 7");
    }

    #[test]
    fn controls_card_follows_live_remaps_and_pairs_steering_keys() {
        let mut bindings = Bindings::default();
        let groups = |bindings: &Bindings, ids: &[&str]| {
            let keys = action_keys(bindings);
            let lookup = |id: &str| {
                keys.iter()
                    .find(|(i, _)| i == id)
                    .map(|(_, k)| k.clone())
                    .unwrap_or_default()
            };
            keys_for(&lookup, ids)
        };
        let s = |rows: &[&[&str]]| -> Vec<Vec<String>> {
            rows.iter()
                .map(|r| r.iter().map(|k| k.to_string()).collect())
                .collect()
        };
        assert_eq!(groups(&bindings, &["throttle"]), s(&[&["W"], &["↑"]]));
        assert_eq!(
            groups(&bindings, &["steerLeft", "steerRight"]),
            s(&[&["A", "D"], &["←", "→"]])
        );
        assert_eq!(
            groups(&bindings, &["stuntCircle", "stuntTriangle"]),
            s(&[&["J", "K"]])
        );
        assert!(
            groups(&bindings, &["reverse"]).is_empty(),
            "Unbound rows are omitted"
        );
        bindings.set_binding(
            true,
            Action::Throttle,
            vec![
                Binding::One(Atom::Key("KeyT".into())),
                Binding::Chord(vec![Atom::Key("KeyG".into()), Atom::Key("KeyH".into())]),
            ],
        );
        assert_eq!(groups(&bindings, &["throttle"]), s(&[&["T"], &["G+H"]]));
    }
}
