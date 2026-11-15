//! The Free Ride hub HUD (research/FREERIDE_HUB.md): a Challenge button
//! beside a nearby rider, the Accept/Decline prompt with its ring timer,
//! one status chip with a live countdown, the map vote and result toasts,
//! plus the glowing ground circle of a join spot or goal. Authored for the
//! port, following research/MULTIPLAYER_EXPERIENCE.md "UI rules": labels
//! of 1-3 words, numbers and timers instead of sentences, 48 px targets;
//! drawn with the classic HUD pieces (research/UI_AUDIT_HUD.md).
use super::{Viewport, classic, peers};
use crate::{
    color::Color,
    layout::El,
    paint::{BoxPaint, BoxShadow, DisplayList, Item, Rect},
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

fn row(gap: f32) -> Style {
    Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        flex_wrap: FlexWrap::Wrap,
        gap: Size {
            width: LengthPercentage::length(gap),
            height: LengthPercentage::length(gap * 0.4),
        },
        ..Default::default()
    }
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
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::CENTER,
        ..Default::default()
    }
}

/// A name cut to fit a label.
fn short(name: &str, max: usize) -> String {
    let mut out: String = name.chars().take(max).collect();
    if name.chars().count() > max {
        out.push('…');
    }
    out
}

fn timer(clock: &str, low: bool) -> El {
    classic::hud_text(
        clock,
        24.,
        if low {
            Color::hex("#ff6b5b")
        } else {
            Color::WHITE
        },
    )
}

/// The hub HUD over the whole view; `top` clears the toolbar. Built from
/// the race HUD's lettering, the Race Results panel and the race-over
/// buttons (hud::classic); no cards of its own.
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
        // The race HUD's info line: gold title, silver detail, the clock.
        let line = El::new(row(10.))
            .child(classic::hud_text(&b.title, 18., classic::GOLD))
            .children(
                (!b.line.is_empty()).then(|| classic::hud_text(&b.line, 17., classic::SILVER)),
            )
            .child(timer(&b.clock, b.low));
        root = root.child(El::new(absolute(left, y, width)).child(line));
        y += 42.;
    }
    if let Some(offer) = &state.offer {
        let panel = classic::panel(width)
            .child(classic::heading(
                "Race?",
                Some(classic::hud_text(
                    offer.seconds.to_string(),
                    30.,
                    classic::GOLD,
                )),
            ))
            .child(classic::hud_text(
                short(&offer.name, 16),
                20.,
                classic::CELL,
            ))
            .child(classic::buttons(vec![
                classic::button(ACCEPT, "Accept", true, false),
                classic::button(DECLINE, "Decline", false, false),
            ]));
        root = root.child(El::new(absolute(left, y, width)).child(panel));
        y += 170.;
    } else if let Some(name) = &state.challenge {
        root = root.child(El::new(absolute(left, y, width)).child(classic::button(
            CHALLENGE,
            &format!("Challenge {}", short(name, 10)),
            true,
            false,
        )));
        y += 60.;
    }
    if let Some((options, clock)) = &state.vote {
        let mut panel =
            classic::panel(width).child(classic::heading("Next Map", Some(timer(clock, false))));
        for (i, option) in options.iter().enumerate() {
            let votes = classic::hud_text(
                option.votes.to_string(),
                22.,
                if option.mine {
                    classic::INK
                } else {
                    classic::GOLD
                },
            );
            panel = panel.child(classic::course(
                Some(VOTE + i as u32),
                &option.name,
                "",
                Some(votes),
                option.mine,
                false,
            ));
        }
        root = root.child(El::new(absolute(left, y, width)).child(panel));
        y += 90. + 56. * options.len() as f32;
    }
    if let Some(toast) = &state.toast {
        let at = y.max(view.height * 0.3);
        let color = if toast.win {
            classic::GOLD
        } else {
            Color::WHITE
        };
        let el = if toast.rows.is_empty() {
            // A one-word answer reads as a race HUD sign.
            classic::hud_text(&toast.title, 30., color)
        } else {
            let lines: Vec<classic::Line> = toast
                .rows
                .iter()
                .map(|(place, name, points)| classic::Line {
                    cells: vec![
                        if place.is_empty() {
                            name.clone()
                        } else {
                            format!("{place}. {name}")
                        },
                        format!("+{points}"),
                    ],
                    me: false,
                    dim: false,
                })
                .collect();
            classic::panel(width)
                .child(classic::heading_in(&toast.title, color, None))
                .child(classic::standings(
                    &[
                        classic::Column {
                            label: "RIDER",
                            right: false,
                        },
                        classic::Column {
                            label: "POINTS",
                            right: true,
                        },
                    ],
                    &lines,
                ))
        };
        root = root.child(El::new(absolute(left, at, width)).child(el));
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

    fn hits(view: &Viewport, state: &View) -> Vec<(Rect, u32)> {
        let el = element(view, state, 60.);
        let (mut list, mut hits) = (DisplayList::default(), Vec::new());
        crate::layout::render(
            &el,
            &mut Fonts::new(),
            view.width,
            view.height,
            &mut list,
            &mut hits,
        );
        hits
    }

    /// Every button is a 48 px target and stays on a 360 px portrait phone.
    #[test]
    fn buttons_are_touch_sized_and_fit_a_portrait_phone() {
        let vote = VoteOption {
            name: "Prospect Springs Long Name".into(),
            votes: 2,
            mine: false,
        };
        let states = [
            View {
                challenge: Some("Mud Hawk Rider".into()),
                ..Default::default()
            },
            View {
                challenge: None,
                banner: Some(Banner {
                    title: "HILL KING".into(),
                    line: "JOIN · 120 ft".into(),
                    clock: "0:12".into(),
                    low: false,
                }),
                offer: Some(Offer {
                    name: "Mud Hawk".into(),
                    left: 0.5,
                    seconds: 7,
                }),
                vote: Some((
                    vec![
                        vote.clone(),
                        VoteOption {
                            mine: true,
                            ..vote.clone()
                        },
                        vote,
                    ],
                    "0:42".into(),
                )),
                toast: Some(Toast {
                    title: "SPRINT RESULTS".into(),
                    rows: vec![("1".into(), "Ann".into(), 100)],
                    win: true,
                }),
            },
        ];
        for view in [viewport(360., 740.), viewport(1280., 720.)] {
            for state in &states {
                let hits = hits(&view, state);
                let mut ids: Vec<u32> = Vec::new();
                if state.challenge.is_some() && state.offer.is_none() {
                    ids.push(Button::Challenge.id());
                }
                if state.offer.is_some() {
                    ids.extend([Button::Accept.id(), Button::Decline.id()]);
                }
                if let Some((options, _)) = &state.vote {
                    ids.extend((0..options.len() as u8).map(|i| Button::Vote(i).id()));
                }
                for id in ids {
                    let (rect, _) = hits.iter().find(|(_, h)| *h == id).expect("button drawn");
                    assert!(rect.h >= 48. && rect.w >= 48., "{id}: {rect:?}");
                    assert!(
                        rect.x >= 0. && rect.right() <= view.width + 0.5,
                        "{id}: {rect:?} in {}",
                        view.width
                    );
                    assert_eq!(Button::from_id(id).map(Button::id), Some(id));
                }
            }
        }
    }
}
