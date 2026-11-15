//! The Free Ride hub HUD (research/FREERIDE_HUB.md): a Challenge button
//! beside a nearby rider, the Accept/Decline prompt with its ring timer,
//! one status chip with a live countdown, the map vote and result toasts,
//! plus the glowing ground circle of a join spot or goal. Authored for the
//! port, following research/MULTIPLAYER_EXPERIENCE.md "UI rules": labels
//! of 1-3 words, numbers and timers instead of sentences, 48 px targets.
use super::{Viewport, peers};
use crate::{
    color::Color,
    layout::{Content, El, Paint, sides},
    paint::{BoxPaint, BoxShadow, DisplayList, Item, Rect},
    text::{Font, TextStyle},
};
use taffy::prelude::*;

/// Hit targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Challenge,
    Accept,
    Decline,
    /// A rotation vote option (0-2).
    Vote(u8),
}

const CHALLENGE: u32 = 80;
const ACCEPT: u32 = 81;
const DECLINE: u32 = 82;
const VOTE: u32 = 83;

impl Button {
    pub fn id(self) -> u32 {
        match self {
            Button::Challenge => CHALLENGE,
            Button::Accept => ACCEPT,
            Button::Decline => DECLINE,
            Button::Vote(i) => VOTE + u32::from(i),
        }
    }
    pub fn from_id(id: u32) -> Option<Self> {
        match id {
            CHALLENGE => Some(Button::Challenge),
            ACCEPT => Some(Button::Accept),
            DECLINE => Some(Button::Decline),
            VOTE..=85 => Some(Button::Vote((id - VOTE) as u8)),
            _ => None,
        }
    }
}

/// The status chip: what runs, one short line and the time left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Banner {
    pub title: String,
    pub line: String,
    /// M:SS.
    pub clock: String,
    /// Ten seconds or fewer left.
    pub low: bool,
}

/// Another rider's challenge.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offer {
    pub name: String,
    /// Share of the answer time left (0-1), for the ring.
    pub left: f32,
    pub seconds: u32,
}

/// Results: a title and up to three rows (place, name, +points).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toast {
    pub title: String,
    pub rows: Vec<(String, String, u16)>,
    /// The player won: a gold flash.
    pub win: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoteOption {
    pub name: String,
    pub votes: u8,
    /// This player's choice.
    pub mine: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// The nearby rider a Challenge goes to.
    pub challenge: Option<String>,
    pub offer: Option<Offer>,
    pub banner: Option<Banner>,
    /// The venues on offer and the time left.
    pub vote: Option<(Vec<VoteOption>, String)>,
    pub toast: Option<Toast>,
}

fn style(size: f32, weight: u16, color: Color) -> TextStyle {
    TextStyle::new(Font::display(size, weight, false), color)
        .spacing(0.4)
        .shadow(1., 2., 0., Color::hex("#000c"))
}

fn text(value: impl Into<String>, size: f32, weight: u16, color: &str) -> El {
    El::new(Style::default()).content(Content::Text {
        text: value.into(),
        style: style(size, weight, Color::hex(color)),
        line_height: size * 1.2,
        wrap: false,
    })
}

fn row(gap: f32) -> Style {
    Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: LengthPercentage::length(gap),
            height: LengthPercentage::length(gap),
        },
        ..Default::default()
    }
}

fn column(gap: f32) -> Style {
    Style {
        flex_direction: FlexDirection::Column,
        ..row(gap)
    }
}

fn panel(fill: &str) -> Paint {
    Paint::fill(Color::hex(fill))
        .radius(10.)
        .shadow(BoxShadow::outer(0., 3., 0., Color::hex("#0008")))
}

/// A 48 px button.
fn button(id: u32, label: El, fill: &str, width: f32) -> El {
    El::new(Style {
        min_size: Size {
            width: LengthPercentageAuto::length(width),
            height: LengthPercentageAuto::length(48.),
        },
        padding: sides(4., 14., 4., 14.),
        ..column(0.)
    })
    .id(id)
    .paint(panel(fill))
    .child(label)
}

/// A countdown ring: 20 dots around `seconds`, lit for the share left.
fn ring(left: f32, seconds: u32) -> El {
    const SIZE: f32 = 56.;
    const DOTS: usize = 20;
    let lit = (left * DOTS as f32).ceil() as usize;
    let mut el = El::new(Style {
        size: Size {
            width: length(SIZE),
            height: length(SIZE),
        },
        flex_shrink: 0.,
        ..column(0.)
    })
    .child(text(seconds.to_string(), 22., 900, "#ffd166"));
    for i in 0..DOTS {
        let a = i as f32 / DOTS as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let (x, y) = (
            SIZE / 2. + a.cos() * 24. - 3.,
            SIZE / 2. + a.sin() * 24. - 3.,
        );
        let on = i < lit;
        el = el.child(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(x),
                    top: LengthPercentageAuto::length(y),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                size: Size {
                    width: length(6.),
                    height: length(6.),
                },
                ..Default::default()
            })
            .paint(Paint::fill(Color::hex(if on { "#ffd166" } else { "#fff3" })).radius(3.)),
        );
    }
    el
}

fn absolute(left: f32, top: f32, width: f32) -> Style {
    Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(left),
            top: LengthPercentageAuto::length(top),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(width),
            height: auto(),
        },
        ..column(6.)
    }
}

/// The hub HUD over the whole view; `top` clears the toolbar.
pub fn element(view: &Viewport, state: &View, top: f32) -> El {
    let width = 340f32.min(view.width - 24.);
    let left = (view.width - width) / 2.;
    let mut root = El::new(Style {
        size: Size {
            width: length(view.width),
            height: length(view.height),
        },
        ..Default::default()
    });
    let mut y = top;
    if let Some(b) = &state.banner {
        let chip = El::new(Style {
            padding: sides(6., 14., 6., 14.),
            ..row(10.)
        })
        .paint(panel("#0c0c0cc8"))
        .child(text(&b.title, 15., 800, "#f1d84e"))
        .child(text(&b.line, 15., 700, "#f6f6f6"))
        .child(text(
            &b.clock,
            20.,
            900,
            if b.low { "#ff6b5b" } else { "#ffffff" },
        ));
        root = root.child(El::new(absolute(left, y, width)).child(chip));
        y += 46.;
    }
    if let Some(offer) = &state.offer {
        let card = El::new(Style {
            padding: sides(10., 12., 12., 12.),
            ..column(8.)
        })
        .paint(panel("#101820e0"))
        .child(
            El::new(row(12.))
                .child(ring(offer.left, offer.seconds))
                .child(
                    El::new(column(2.))
                        .child(text(&offer.name, 18., 800, "#ffffff"))
                        .child(text("RACE?", 16., 800, "#ffd166")),
                ),
        )
        .child(
            El::new(row(10.))
                .child(button(
                    ACCEPT,
                    text("ACCEPT", 16., 900, "#0b1a0b"),
                    "#7cff6b",
                    120.,
                ))
                .child(button(
                    DECLINE,
                    text("NO", 16., 900, "#ffffff"),
                    "#3a3a3aee",
                    90.,
                )),
        );
        root = root.child(El::new(absolute(left, y, width)).child(card));
        y += 150.;
    } else if let Some(name) = &state.challenge {
        let label = El::new(column(0.))
            .child(text("CHALLENGE", 16., 900, "#1a1200"))
            .child(text(name, 12., 700, "#3a2a00"));
        root = root.child(
            El::new(absolute(left, y, width)).child(button(CHALLENGE, label, "#ffd166", 150.)),
        );
        y += 60.;
    }
    if let Some((options, clock)) = &state.vote {
        let mut card = El::new(Style {
            padding: sides(8., 10., 10., 10.),
            ..column(6.)
        })
        .paint(panel("#0c0c0cd8"))
        .child(
            El::new(row(8.))
                .child(text("NEXT MAP", 15., 800, "#f1d84e"))
                .child(text(clock, 18., 900, "#ffffff")),
        );
        let mut buttons = El::new(Style {
            flex_wrap: FlexWrap::Wrap,
            ..row(6.)
        });
        for (i, option) in options.iter().enumerate() {
            let label = El::new(column(0.))
                .child(text(&option.name, 13., 800, "#ffffff"))
                .child(text(format!("{}", option.votes), 13., 900, "#ffd166"));
            buttons = buttons.child(button(
                VOTE + i as u32,
                label,
                if option.mine {
                    "#2f6d2fee"
                } else {
                    "#2a2a2aee"
                },
                ((width - 32.) / 3.).max(90.),
            ));
        }
        card = card.child(buttons);
        root = root.child(El::new(absolute(left, y, width)).child(card));
        y += 120.;
    }
    if let Some(toast) = &state.toast {
        let mut card = El::new(Style {
            padding: sides(10., 16., 12., 16.),
            ..column(4.)
        })
        .paint(if toast.win {
            panel("#2a2000e8").shadow(BoxShadow::outer(0., 0., 18., Color::hex("#ffd166aa")))
        } else {
            panel("#0c0c0ce0")
        })
        .child(text(
            &toast.title,
            22.,
            900,
            if toast.win { "#ffd166" } else { "#ffffff" },
        ));
        for (place, name, points) in &toast.rows {
            let mut line = El::new(row(10.));
            if !place.is_empty() {
                line = line.child(text(place, 16., 900, "#f1d84e"));
            }
            card = card.child(line.child(text(name, 16., 700, "#f6f6f6")).child(text(
                format!("+{points}"),
                16.,
                900,
                "#7cff6b",
            )));
        }
        root = root.child(El::new(absolute(left, y.max(view.height * 0.3), width)).child(card));
    }
    root
}

/// The glowing circle on the ground around `centre` (`radius` ft): dots
/// projected with the scene's column-major view-projection; those behind
/// the camera are skipped.
pub fn paint_ring(
    list: &mut DisplayList,
    view: &Viewport,
    centre: [f32; 3],
    radius: f32,
    vp: &[f32; 16],
    pulse: f32,
) {
    const DOTS: usize = 28;
    let gold = peers::color(19);
    for i in 0..DOTS {
        let a = i as f32 / DOTS as f32 * std::f32::consts::TAU;
        let point = [
            centre[0] + radius * a.cos(),
            centre[1] + 1.,
            centre[2] + radius * a.sin(),
            1.,
        ];
        let clip: [f32; 4] =
            std::array::from_fn(|r| (0..4).map(|k| vp[k * 4 + r] * point[k]).sum());
        if !clip.iter().all(|v| v.is_finite()) || clip[3] <= 0.5 {
            continue;
        }
        let (x, y) = (clip[0] / clip[3], -clip[1] / clip[3]);
        if x.abs() > 1.2 || y.abs() > 1.2 {
            continue;
        }
        // Nearer dots are bigger.
        let size = (220. / clip[3]).clamp(3., 14.) * (0.85 + 0.15 * pulse);
        let (sx, sy) = ((x + 1.) * view.width / 2., (y + 1.) * view.height / 2.);
        list.push(Item::Box(BoxPaint {
            rect: Rect::new(sx - size / 2., sy - size / 2., size, size),
            radii: [size / 2.; 4],
            fill: Some(gold.into()),
            shadows: vec![BoxShadow::outer(0., 0., size, Color::hex("#ffd166cc"))],
            ..Default::default()
        }));
    }
}
