//! The Jukebox player (web/jukebox.js + jukebox.css): the original Jukebox
//! page art (PAGE14: the red snow backdrop and the 20-frame spinning CD),
//! now playing, seek, transport and volume, and the scrolling list of the
//! original soundtrack. atv_core::ui_menu owns the actions, labels and
//! keyboard order; the host's music player supplies the live view.
use super::{CONTENT, PANEL, ROW, State, gold_pill};
use crate::{
    color::{Color, Fill, Gradient},
    hud::{Viewport, tokens},
    layout::{Content, El, Fit, Paint, margins, px, sides},
    paint::{BoxShadow, ImageId},
    text::{Font, Shape, TextStyle},
};
use taffy::prelude::*;

/// Seconds a Left/Right step seeks (JUKEBOX_SEEK_STEP).
pub const SEEK_STEP: f32 = 10.;

/// The music player's live state (MusicStream `snapshot`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicView {
    pub ready: bool,
    pub enabled: bool,
    pub paused: bool,
    pub playing: bool,
    pub volume: f32,
    /// Title, artist, genre.
    pub songs: Vec<(String, String, String)>,
    pub playing_index: Option<usize>,
    pub position: f32,
    pub duration: f32,
    pub seekable: bool,
    /// `stream` (hosted AAC) or `decoder` (original VAG).
    pub mode: &'static str,
}

/// A Jukebox control the host's player performs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JukeboxAction {
    Select(usize),
    Toggle,
    Next,
    Previous,
    Seek(f32),
    /// Leaving the Jukebox ends its pause (`stopJukebox`).
    Leave,
}

/// Original Jukebox artwork registered with the UI renderer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Art {
    pub backdrop: Option<(ImageId, [f32; 2])>,
    /// The CD's frames and their size.
    pub disc: Vec<ImageId>,
    pub disc_size: [f32; 2],
}

/// `clock`: M:SS.
pub fn clock(seconds: f32) -> String {
    let whole = if seconds.is_finite() {
        seconds.max(0.).floor() as u64
    } else {
        0
    };
    format!("{}:{:02}", whole / 60, whole % 60)
}

fn display(size: f32, weight: u16, italic: bool) -> Font {
    Font::display(size, weight, italic)
}

fn text(value: &str, style: TextStyle, line: f32, wrap: bool) -> El {
    El::new(Style {
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        ..Default::default()
    })
    .content(Content::Text {
        text: value.into(),
        style,
        line_height: line,
        wrap,
    })
}

fn icon(path: &str, size: f32, view: f32, color: Color) -> El {
    El::new(Style {
        flex_shrink: 0.,
        size: Size {
            width: length(size),
            height: length(size),
        },
        ..Default::default()
    })
    .content(Content::Shape(Shape {
        path: path.into(),
        bounds: [0., 0., view, view],
        unit: size / view,
        fill: color,
        stroke: None,
        drop_shadow: None,
    }))
}

const PLAY: &str = "M8 5v14l11-7z";
const PAUSE: &str = "M7 5h4v14H7zM13 5h4v14h-4z";
const PREV: &str = "M6 5h2.5v14H6zM20 5v14L9.5 12z";
const NEXT: &str = "M15.5 5H18v14h-2.5zM4 5v14l10.5-7z";
const BACK: &str = "M13 3 5 10l8 7z";
const VOLUME: &str = "M4 9v6h4l5 4V5L8 9zm12.5 3a4.5 4.5 0 0 0-2.5-4v8a4.5 4.5 0 0 0 2.5-4z";

/// The Jukebox panel for the pause layer.
pub fn element(
    view: &Viewport,
    state: &State,
    music: &MusicView,
    art: Option<&Art>,
    frame: usize,
) -> El {
    let (w, h) = (view.width, view.height);
    let compact = w <= 560. || h > w;
    let short = h <= 520. && w > h;
    let index = |action: &str| state.rows.choices.iter().position(|c| c == action);
    let id = |action: &str| index(action).map(|i| ROW + i as u32);
    let selected = |action: &str| index(action) == Some(state.selection);
    let playing = music.playing && music.enabled;
    let song = music.playing_index.and_then(|i| music.songs.get(i));

    // Header: Back, title, source (`.pause-panel button` type size).
    let back_size = (w * 0.02).clamp(17., 21.);
    let back_label = state.rows.label("back").unwrap_or("Back");
    let mut back = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: px(8.),
            height: px(8.),
        },
        min_size: Size {
            width: length(44.),
            height: length(44.),
        },
        padding: sides(0., 14., 0., 10.),
        border: sides(1., 1., 1., 1.),
        ..Default::default()
    })
    .paint(Paint {
        radii: [4.; 4],
        border: [1.; 4],
        border_color: Color::hex("#ffffff24"),
        fill: selected("back").then(gold_pill),
        ..Default::default()
    })
    .child(icon(
        BACK,
        16.,
        20.,
        if selected("back") {
            Color::hex("#0d0b00")
        } else {
            Color::hex("#e8e8e8")
        },
    ))
    .child(text(
        back_label,
        TextStyle::new(
            display(back_size, 700, false),
            if selected("back") {
                Color::hex("#0d0b00")
            } else {
                Color::hex("#e8e8e8")
            },
        ),
        back_size,
        false,
    ));
    if let Some(i) = id("back") {
        back = back.id(i);
    }
    let title_size = if short {
        24.
    } else {
        (w * 0.034).clamp(26., 40.)
    };
    let source = if !music.ready {
        "Music unavailable"
    } else if music.mode == "stream" {
        "Streaming"
    } else {
        "Original decoder"
    };
    let mut header = El::new(Style {
        display: Display::Grid,
        grid_template_columns: vec![auto(), minmax(length(0.), fr(1.)), auto()],
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(12.),
            height: px(12.),
        },
        margin: margins(
            0.,
            0.,
            if compact {
                10.
            } else if short {
                6.
            } else {
                16.
            },
            0.,
        ),
        padding: sides(
            0.,
            0.,
            if compact {
                8.
            } else if short {
                4.
            } else {
                12.
            },
            0.,
        ),
        flex_shrink: 0.,
        ..Default::default()
    })
    .child(back)
    .child(text(
        "Jukebox",
        TextStyle::new(display(title_size, 800, true), Color::hex("#f6f6f6"))
            .spacing(title_size * 0.015)
            .shadow(2., 3., 0., Color::hex("#000a")),
        title_size * 1.1,
        false,
    ));
    if w > 420. {
        header = header.child(text(
            &source.to_uppercase(),
            TextStyle::new(display(12., 400, false), Color::hex("#bbb7b2")).spacing(0.72),
            14.,
            false,
        ));
    }

    // Art: the backdrop, a vignette and the turning disc.
    let art_size = if compact {
        Size {
            width: length(92.),
            height: length(92.),
        }
    } else {
        Size {
            width: percent(1.),
            height: length(if short {
                (h * 0.24).clamp(70., 130.)
            } else {
                (h * 0.34).clamp(120., 340.)
            }),
        }
    };
    let mut art_el = El::new(Style {
        position: Position::Relative,
        size: art_size,
        flex_shrink: 0.,
        border: sides(if compact { 2. } else { 3. }, 0., 0., 0.),
        overflow: taffy::Point {
            x: taffy::Overflow::Hidden,
            y: taffy::Overflow::Hidden,
        },
        ..Default::default()
    })
    .paint(Paint {
        fill: Some(Color::hex("#2a0505").into()),
        border: [if compact { 2. } else { 3. }, 0., 0., 0.],
        border_color: tokens::SILVER,
        shadows: vec![
            BoxShadow::inset(0., 0., 0., Color::hex("#ffffff1a")),
            BoxShadow::outer(0., 14., 40., Color::hex("#0008")),
        ],
        ..Default::default()
    });
    let fill_abs = || taffy::Rect {
        left: LengthPercentageAuto::length(0.),
        right: LengthPercentageAuto::length(0.),
        top: LengthPercentageAuto::length(0.),
        bottom: LengthPercentageAuto::length(0.),
    };
    match art.and_then(|a| a.backdrop) {
        Some((image, size)) => {
            art_el = art_el.child(
                El::new(Style {
                    position: Position::Absolute,
                    inset: fill_abs(),
                    ..Default::default()
                })
                .content(Content::Image {
                    image,
                    size,
                    fit: Fit::Cover,
                    shadow: None,
                }),
            );
        }
        None => {
            art_el = art_el.child(
                El::new(Style {
                    position: Position::Absolute,
                    inset: fill_abs(),
                    ..Default::default()
                })
                .paint(Paint::fill(Fill::ellipse_at(
                    [0.3, 0.3],
                    &[(0., Color::hex("#7a1212")), (1., Color::hex("#210504"))],
                ))),
            );
        }
    }
    // ::after: radial vignette and a bottom fade.
    art_el = art_el
        .child(
            El::new(Style {
                position: Position::Absolute,
                inset: fill_abs(),
                ..Default::default()
            })
            .paint(Paint::fill(Fill::ellipse_at(
                [0.5, 0.42],
                &[
                    (0., Color::hex("#0000")),
                    (0.3, Color::hex("#0000")),
                    (1., Color::hex("#12020299")),
                ],
            ))),
        )
        .child(
            El::new(Style {
                position: Position::Absolute,
                inset: fill_abs(),
                ..Default::default()
            })
            .paint(Paint::fill(Gradient::new(
                180.,
                &[
                    (0., Color::hex("#0000")),
                    (0.55, Color::hex("#0000")),
                    (1., Color::hex("#0b0101d9")),
                ],
            ))),
        );
    if let Some(a) = art.filter(|a| !a.disc.is_empty()) {
        // Stopped, the disc rests face-on on its first frame.
        let shown = if playing { frame % a.disc.len() } else { 0 };
        let (top, height) = if compact { (0.15, 0.7) } else { (0.12, 0.6) };
        art_el = art_el.child(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(0.),
                    right: LengthPercentageAuto::length(0.),
                    top: LengthPercentageAuto::percent(top),
                    bottom: LengthPercentageAuto::percent(1. - top - height),
                },
                display: Display::Flex,
                justify_content: JustifyContent::CENTER,
                ..Default::default()
            })
            .child(
                El::new(Style {
                    size: Size {
                        width: auto(),
                        height: percent(1.),
                    },
                    aspect_ratio: Some(1.),
                    ..Default::default()
                })
                .content(Content::Image {
                    image: a.disc[shown],
                    size: a.disc_size,
                    fit: Fit::Fill,
                    shadow: Some(crate::text::Shadow {
                        dx: 0.,
                        dy: 8.,
                        blur: 14.,
                        color: Color::hex("#000a"),
                    }),
                }),
            ),
        );
    }

    // Meta: genre, title, artist.
    let title_px = if short {
        20.
    } else if compact {
        (w * 0.06).clamp(20., 26.)
    } else {
        (w * 0.03).clamp(22., 32.)
    };
    let meta = El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: px(2.),
            height: px(2.),
        },
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        ..Default::default()
    })
    .child(text(
        &song.map_or("ORIGINAL SOUNDTRACK".to_string(), |s| s.2.to_uppercase()),
        TextStyle::new(display(12., 800, true), tokens::ORANGE).spacing(0.96),
        15.6,
        false,
    ))
    .child(text(
        &song.map_or("CHOOSE A SONG".to_string(), |s| s.0.to_uppercase()),
        TextStyle::new(display(title_px, 800, true), tokens::GOLD_LIGHT).shadow(
            0.,
            2.,
            4.,
            Color::BLACK,
        ),
        title_px * 1.1,
        true,
    ))
    .child(text(
        &song.map_or_else(
            || {
                if music.ready {
                    format!("{} original songs", music.songs.len())
                } else {
                    "Loading the soundtrack…".into()
                }
            },
            |s| s.1.clone(),
        ),
        TextStyle::new(
            display(if short || compact { 15. } else { 16. }, 600, false),
            tokens::SILVER_LIGHT,
        ),
        20.,
        false,
    ));

    // Seek: elapsed, range, duration.
    let time_style = |on: bool| {
        TextStyle::new(
            display(13., 600, false),
            if on {
                tokens::GOLD_LIGHT
            } else {
                Color::hex("#bbb7b2")
            },
        )
    };
    let seek_fill = if music.duration > 0. {
        (music.position / music.duration).clamp(0., 1.)
    } else {
        0.
    };
    let centered = |child: El| {
        El::new(Style {
            display: Display::Flex,
            justify_content: JustifyContent::CENTER,
            ..Default::default()
        })
        .child(child)
    };
    let mut seek_range = super::range(seek_fill);
    if let Some(i) = id("jukeseek") {
        seek_range = seek_range.id(i);
    }
    let seek = El::new(Style {
        display: Display::Grid,
        grid_template_columns: vec![length(39.), minmax(length(0.), fr(1.)), length(39.)],
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(10.),
            height: px(10.),
        },
        ..Default::default()
    })
    .child(centered(text(
        &clock(if song.is_some() { music.position } else { 0. }),
        time_style(selected("jukeseek")),
        13.,
        false,
    )))
    .child(seek_range)
    .child(centered(text(
        &clock(if song.is_some() { music.duration } else { 0. }),
        time_style(selected("jukeseek")),
        13.,
        false,
    )));

    // Transport and volume.
    let (skip, play) = if compact {
        (44., 56.)
    } else if short {
        (44., 48.)
    } else {
        (48., 64.)
    };
    let round = |action: &str, path: &str, size: f32, glyph: f32, primary: bool| {
        let on = selected(action);
        let mut paint = if primary {
            Paint::fill(gold_pill())
                .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#fff8")))
                .shadow(BoxShadow::outer(0., 4., 14., Color::hex("#0009")))
        } else {
            Paint::default()
        };
        paint.radii = [size / 2.; 4];
        if on {
            // outline: 3px gold-light, offset 3px.
            paint = paint.shadow(BoxShadow {
                spread: 6.,
                ..BoxShadow::outer(0., 0., 0., tokens::GOLD_LIGHT)
            });
            paint = paint.shadow(BoxShadow {
                spread: 3.,
                ..BoxShadow::outer(0., 0., 0., Color::hex("#0c0c0c"))
            });
        }
        let color = if primary {
            Color::hex("#1b1500")
        } else if on {
            Color::hex("#0d0b00")
        } else {
            tokens::SILVER_LIGHT
        };
        let mut el = El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            size: Size {
                width: length(size),
                height: length(size),
            },
            flex_shrink: 0.,
            ..Default::default()
        })
        .paint(paint)
        .child(icon(path, glyph, 24., color));
        if let Some(i) = id(action) {
            el = el.id(i);
        }
        el
    };
    let transport = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(if short {
                4.
            } else if compact {
                10.
            } else {
                14.
            }),
            height: px(0.),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .child(round("jukeprev", PREV, skip, 26., false))
    .child(round(
        "jukeplay",
        if music.enabled && !music.paused {
            PAUSE
        } else {
            PLAY
        },
        play,
        30.,
        true,
    ))
    .child(round("jukenext", NEXT, skip, 26., false));
    let percent_v = (music.volume * 100.).round();
    let mut volume_range = super::range(music.volume.clamp(0., 1.));
    if let Some(i) = id("jukevolume") {
        volume_range = volume_range.id(i);
    }
    let value_shown = !(w <= 420. || short);
    let mut volume = El::new(Style {
        display: Display::Grid,
        grid_template_columns: if value_shown {
            vec![auto(), minmax(length(80.), fr(1.)), length(31.2)]
        } else {
            vec![auto(), minmax(length(70.), fr(1.))]
        },
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(8.),
            height: px(8.),
        },
        flex_grow: 1.,
        flex_basis: length(if short || w <= 420. { 110. } else { 200. }),
        min_size: Size {
            width: length(if short || w <= 420. { 110. } else { 150. }),
            height: auto(),
        },
        ..Default::default()
    })
    .child(icon(VOLUME, 20., 24., tokens::SILVER))
    .child(volume_range);
    if value_shown {
        volume = volume.child(
            El::new(Style {
                display: Display::Flex,
                justify_content: JustifyContent::END,
                ..Default::default()
            })
            .child(text(
                &format!("{percent_v}%"),
                time_style(true).spacing(0.),
                12.,
                false,
            )),
        );
    }
    let controls = El::new(Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(if compact {
                14.
            } else if short {
                10.
            } else {
                20.
            }),
            height: px(8.),
        },
        ..Default::default()
    })
    .child(transport)
    .child(volume);

    let now = if compact {
        // Compact card: art beside the meta, seek and controls below.
        El::new(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            gap: Size {
                width: px(8.),
                height: px(8.),
            },
            flex_shrink: 0.,
            ..Default::default()
        })
        .child(
            El::new(Style {
                display: Display::Flex,
                align_items: AlignItems::CENTER,
                gap: Size {
                    width: px(14.),
                    height: px(14.),
                },
                ..Default::default()
            })
            .child(art_el)
            .child(meta),
        )
        .child(seek)
        .child(controls)
    } else {
        El::new(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            gap: Size {
                width: px(if short { 6. } else { 12. }),
                height: px(if short { 6. } else { 12. }),
            },
            min_size: Size {
                width: length(0.),
                height: length(0.),
            },
            overflow: taffy::Point {
                x: taffy::Overflow::Hidden,
                y: taffy::Overflow::Hidden,
            },
            ..Default::default()
        })
        .child(art_el)
        .child(meta)
        .child(seek)
        .child(controls)
    };

    // Track list: the only scrolling region.
    let mut list = El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        ..Default::default()
    })
    .id(CONTENT);
    for (i, action) in state.rows.choices.iter().enumerate() {
        let Some(n) = action
            .strip_prefix("song:")
            .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        let Some((title, artist, genre)) = music.songs.get(n) else {
            continue;
        };
        let on = state.selection == i;
        let current = music.playing_index == Some(n) && song.is_some();
        let dark = Color::hex("#0d0b00");
        let pick = |normal: Color, gold: Color| {
            if on {
                dark
            } else if current {
                gold
            } else {
                normal
            }
        };
        let mut paint = if on {
            Paint::fill(gold_pill()).radius(5.)
        } else if current {
            Paint::fill(Gradient::new(
                90.,
                &[(0., Color::hex("#e5d36014")), (0.7, Color::hex("#0000"))],
            ))
        } else {
            Paint::default()
        };
        paint.border = [0., 0., 1., 0.];
        paint.border_color = Color::hex("#ffffff10");
        let genre_shown = w > 420.;
        let mut row = El::new(Style {
            display: Display::Grid,
            grid_template_columns: if genre_shown {
                vec![
                    length(if compact { 27. } else { 30. }),
                    minmax(length(0.), fr(1.)),
                    auto(),
                    length(if compact { 16. } else { 18. }),
                ]
            } else {
                vec![length(27.), minmax(length(0.), fr(1.)), length(16.)]
            },
            align_items: AlignItems::CENTER,
            gap: Size {
                width: px(if compact { 10. } else { 12. }),
                height: px(0.),
            },
            min_size: Size {
                width: auto(),
                // min-height plus the list item's 1px rule.
                height: length(if short {
                    47.
                } else if compact {
                    53.
                } else {
                    55.
                }),
            },
            padding: sides(6., 12., 6., 14.),
            border: sides(0., 0., 1., 0.),
            flex_shrink: 0.,
            ..Default::default()
        })
        .id(ROW + i as u32)
        .paint(paint)
        .child(text(
            &format!("{:02}", n + 1),
            TextStyle::new(
                display(13., 600, false),
                pick(Color::hex("#8f8a85"), tokens::GOLD_LIGHT),
            ),
            13.,
            false,
        ))
        .child(
            El::new(Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                min_size: Size {
                    width: length(0.),
                    height: auto(),
                },
                ..Default::default()
            })
            .child(text(
                title,
                TextStyle::new(
                    display(15., 700, false),
                    pick(Color::hex("#e8e8e8"), tokens::GOLD_LIGHT),
                ),
                19.,
                false,
            ))
            .child(text(
                artist,
                TextStyle::new(
                    display(13., 500, false),
                    pick(Color::hex("#a9a49f"), Color::hex("#a9a49f")),
                ),
                17.,
                false,
            )),
        );
        if genre_shown {
            row = row.child(text(
                &genre.to_uppercase(),
                TextStyle::new(
                    display(11., 700, true),
                    pick(Color::hex("#8f8a85"), Color::hex("#8f8a85")),
                )
                .spacing(0.66),
                11.,
                false,
            ));
        }
        // .jb-eq: three bars on the playing row (animated while playing).
        let mut eq = El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::END,
            gap: Size {
                width: px(2.),
                height: px(0.),
            },
            size: Size {
                width: auto(),
                height: length(14.),
            },
            ..Default::default()
        });
        if current {
            for k in 0..3 {
                let level = if playing {
                    let t = (frame as f32 * 0.08 + k as f32 * 0.33) % 2.;
                    0.2 + 0.8 * if t > 1. { 2. - t } else { t }
                } else {
                    0.3
                };
                eq = eq.child(
                    El::new(Style {
                        size: Size {
                            width: length(4.),
                            height: length(14. * level),
                        },
                        ..Default::default()
                    })
                    .paint(Paint::fill(if on {
                        Color::hex("#3a2e00")
                    } else {
                        tokens::GOLD
                    })),
                );
            }
        }
        row = row.child(eq);
        list = list.child(row);
    }
    let tracks = El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        min_size: Size {
            width: length(0.),
            height: length(0.),
        },
        flex_grow: 1.,
        flex_shrink: 1.,
        border: sides(1., 0., 0., 0.),
        margin: if compact {
            margins(0., -16., 0., -16.)
        } else {
            margins(0., 0., 0., 0.)
        },
        padding: if compact {
            sides(0., 4., 0., 4.)
        } else {
            sides(0., 0., 0., 0.)
        },
        overflow: taffy::Point {
            x: taffy::Overflow::Visible,
            y: taffy::Overflow::Scroll,
        },
        scrollbar_width: 0.,
        ..Default::default()
    })
    .paint(Paint {
        border: [1., 0., 0., 0.],
        border_color: Color::hex("#ffffff1c"),
        ..Default::default()
    })
    .id(PANEL)
    .scroll(state.scroll)
    .child(list);

    let body = if compact {
        El::new(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            gap: Size {
                width: px(10.),
                height: px(10.),
            },
            flex_grow: 1.,
            min_size: Size {
                width: length(0.),
                height: length(0.),
            },
            ..Default::default()
        })
        .child(now)
        .child(tracks)
    } else {
        El::new(Style {
            display: Display::Grid,
            grid_template_columns: vec![minmax(length(0.), fr(1.)), minmax(length(0.), fr(1.15))],
            grid_template_rows: vec![minmax(length(0.), fr(1.))],
            gap: Size {
                width: px(if short { 18. } else { 28. }),
                height: px(0.),
            },
            flex_grow: 1.,
            min_size: Size {
                width: length(0.),
                height: length(0.),
            },
            ..Default::default()
        })
        .child(now)
        .child(tracks)
    };

    // `.pause-panel[data-screen]:not([data-screen=pause])` caps every page at
    // 700px and outranks jukebox.css's 1080px width; phones span the screen
    // less 12px each side, as measured in play.html.
    let panel_w = if compact {
        w - 24.
    } else {
        700f32.min(w - 48.)
    };
    let (panel_h, pad) = if compact {
        (h - 24., [16., 16., 8., 16.])
    } else if short {
        (h - 20., [10., 18., 6., 18.])
    } else {
        (h - 48., [30., 30., 20., 30.])
    };
    let panel = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length((w - panel_w) / 2.),
            top: LengthPercentageAuto::length((h - panel_h) / 2.),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(panel_w),
            height: length(panel_h),
        },
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        padding: sides(pad[0], pad[1], pad[2], pad[3]),
        overflow: taffy::Point {
            x: taffy::Overflow::Hidden,
            y: taffy::Overflow::Hidden,
        },
        ..Default::default()
    })
    .paint(
        Paint::fill(Color::hex("#0c0c0cdb"))
            .radius(5.)
            .shadow(BoxShadow::outer(0., 6., 0., Color::hex("#0009")))
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff1a"))),
    )
    .child(header)
    .child(body);
    El::new(Style {
        size: Size {
            width: length(w),
            height: length(h),
        },
        ..Default::default()
    })
    .paint(Paint::fill(Fill::ellipse_at(
        [0.5, 0.45],
        &[(0., Color::hex("#0000004d")), (1., Color::hex("#000000b3"))],
    )))
    .child(panel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_matches_jukebox_js() {
        assert_eq!(clock(0.), "0:00");
        assert_eq!(clock(65.9), "1:05");
        assert_eq!(clock(f32::NAN), "0:00");
    }
}
