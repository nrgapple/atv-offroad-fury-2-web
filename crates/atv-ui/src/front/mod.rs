//! Frontend screens (web/menu-screens.js with menu-screens.css): the red
//! scenic backdrop, the curved silver stage panel, gold bullet rows with the
//! glossy gold selection pill, and the Back control, for the title, main,
//! single player, course select, garage and course loader pages. Rust's
//! ui_menu owns the choices and labels; this module owns the responsive
//! layout, in the same CSS units as the browser.
pub mod assets;
pub mod controller;
pub mod logo;
pub mod pixels;

use crate::{
    color::{Color, Fill, Gradient},
    hud::Viewport,
    layout::{Content, El, Fit, Paint, margins, px, sides},
    menu::gold_pill,
    paint::{BoxShadow, ImageId},
    text::{Font, Shape, TextStyle},
};
use taffy::prelude::*;

/// Hit ids. Choices are `ROW + index` in the page's choices.
pub const ROW: u32 = 2000;
pub const TAB: u32 = 2100;
pub const COURSE: u32 = 2200;
pub const VEHICLE: u32 = 2300;
pub const LIST: u32 = 2400;
pub const BACK: u32 = 2401;
pub const PREV: u32 = 2402;
pub const NEXT: u32 = 2403;
pub const RIDE: u32 = 2404;
pub const QUAD_PREV: u32 = 2405;
pub const QUAD_NEXT: u32 = 2406;
pub const RIDER: u32 = 2407;
pub const DONE: u32 = 2408;
/// Host/Join form fields, by index.
pub const FIELD: u32 = 2420;
/// 3D preview slots (their rects size the render targets).
pub const MODEL_QUAD: u32 = 2409;
pub const MODEL_RIDER: u32 = 2410;
/// Single Race: Start Race under the preview, and Copy ride link by Back.
pub const START: u32 = 2411;
pub const SHARE: u32 = 2412;
/// Value rows' Previous/Next arrows, by choice index.
pub const STEP_PREV: u32 = 2430;
pub const STEP_NEXT: u32 = 2440;

/// A registered image and its texel size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Img {
    pub id: ImageId,
    pub width: f32,
    pub height: f32,
}

impl Img {
    fn content(self, fit: Fit) -> Content {
        Content::Image {
            image: self.id,
            size: [self.width, self.height],
            fit,
            shadow: None,
        }
    }

    /// `.ms-model .model-view canvas { filter: drop-shadow(0 10px 6px #0004) }`.
    fn model(self) -> Content {
        Content::Image {
            image: self.id,
            size: [self.width, self.height],
            fit: Fit::Fill,
            shadow: Some(crate::text::Shadow {
                dx: 0.,
                dy: 10.,
                blur: 6.,
                color: Color::hex("#0004"),
            }),
        }
    }
}

/// Original event families from tracks.dat ids; labels follow the manual.
pub const GROUPS: [(&str, &str); 6] = [
    ("all", "All"),
    ("env", "Enduro"),
    ("free", "Stunt"),
    ("nat", "Nationals"),
    ("sx", "Supercross"),
    ("extra", "Arena"),
];

pub fn course_group(id: &str) -> &'static str {
    match id.trim_end_matches(|c: char| c.is_ascii_digit()) {
        "env" => "env",
        "free" => "free",
        "nat" => "nat",
        "sx" => "sx",
        _ => "extra",
    }
}

pub fn group_label(id: &str) -> &'static str {
    let group = course_group(id);
    GROUPS
        .iter()
        .find(|(k, _)| *k == group)
        .map_or("", |(_, l)| l)
}

/// One garage vehicle (ui_menu's catalog): ATVs 1-26, motorcycles 27-46.
#[derive(Clone, Debug, PartialEq)]
pub struct Vehicle {
    pub id: u16,
    pub label: String,
    pub motorcycle: bool,
    /// Motorcycle class, e.g. "250cc Pro".
    pub class: String,
}

/// The garage's ATV stats panel (uiQuad.txt meters and the top speed).
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub engine: f32,
    pub traction: f32,
    pub suspension: f32,
    pub logo: Option<Img>,
    pub top_speed: Option<f32>,
}

/// Everything a page draws.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Screen {
    pub page: String,
    /// (action, label) in keyboard order.
    pub choices: Vec<(String, String)>,
    pub selection: usize,
    pub hovered: Option<u32>,
    /// Riders in Arlington Free Ride (`N riding` on its main-menu row).
    pub freeride_riders: Option<u32>,
    pub backdrop: Option<Img>,
    /// The backdrop is a course photo (greyed and tinted already).
    pub backdrop_track: bool,
    pub logo: Option<Img>,
    pub logo_shadow: Option<Img>,
    /// Course select: (id, name, thumbnail) in list order, the event tab,
    /// per-tab counts and the current course.
    pub courses: Vec<(String, String, Option<Img>)>,
    pub filter: String,
    pub tabs: Vec<(String, String, usize)>,
    pub current: Option<String>,
    pub hero: Option<Img>,
    pub hero_name: String,
    /// Single Race: `.ms-track-mode` ("National race").
    pub hero_mode: String,
    pub list_scroll: f32,
    /// Garage.
    pub vehicle_label: String,
    pub rider: u16,
    pub catalog: Vec<Vehicle>,
    pub quad_id: u16,
    pub stats: Option<Stats>,
    /// Loader.
    pub loading_track: String,
    pub tip: Vec<String>,
    pub progress: f32,
    pub shot: Option<Img>,
    pub previous_shot: Option<(Img, f32)>,
    /// Opacity of `shot` while it fades in over `previous_shot`.
    pub shot_fade: f32,
    /// Press Start pulse brightness (1-1.18).
    pub pulse: f32,
    /// Rendered 3D previews of the vehicle and the garage rider.
    pub quad_model: Option<Img>,
    pub rider_model: Option<Img>,
    /// The vehicle and rider preview slots' status while they show no
    /// model: "Loading model...", "Model unavailable" or "3D preview
    /// unavailable" (frontend.js ModelView messages).
    pub model_status: [Option<&'static str>; 2],
    /// Host/Join form fields: (label, value, focused).
    pub fields: Vec<(String, String, bool)>,
    /// A form error under the rows (`#menu-notice`).
    pub notice: Option<String>,
    /// Single Player: the saved profile's points and race record.
    pub profile: Option<String>,
    /// The Free Ride Lobby page's room code and status.
    pub lobby: Option<controller::Lobby>,
    /// The page's slide-in while it opens (crate::transition).
    pub motion: Option<crate::transition::Motion>,
}

/// Pages laid out like the main menu: rows on the left, the ride stage
/// (logo, vehicle and its plate) on the panel.
fn stage_page(page: &str) -> bool {
    matches!(
        page,
        "main" | "single" | "multiplayer" | "create-lobby" | "join-lobby"
    )
}

fn display(size: f32, weight: u16, italic: bool) -> Font {
    Font::display(size, weight, italic)
}

/// `.model-view span` over an empty preview: centred, and while loading
/// the `.is-loading` italic capitals with the tread bar under them.
fn model_status(slot: El, status: &str) -> El {
    let loading = status.starts_with("Loading");
    let style = if loading {
        TextStyle::new(display(14., 700, true), Color::hex("#f2f2f2")).spacing(14. * 0.04)
    } else {
        TextStyle::new(display(12., 700, false), Color::hex("#f2f2f2"))
    };
    let label = if loading {
        status.to_uppercase()
    } else {
        status.to_string()
    };
    let mut column = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(0.),
            right: LengthPercentageAuto::length(0.),
            top: LengthPercentageAuto::length(0.),
            bottom: LengthPercentageAuto::length(0.),
        },
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: length(0.),
            height: length(14.),
        },
        ..Default::default()
    })
    .child(text(label, style, 16.));
    if loading {
        column = column.child(
            El::new(Style {
                size: Size {
                    width: percent(0.6),
                    height: length(5.),
                },
                max_size: Size {
                    width: length(180.),
                    height: auto(),
                },
                ..Default::default()
            })
            .paint(Paint::fill(Color::hex("#e1c844"))),
        );
    }
    slot.child(column)
}

fn clamp(min: f32, value: f32, max: f32) -> f32 {
    value.clamp(min, max)
}

fn text(text: impl Into<String>, style: TextStyle, line_height: f32) -> El {
    El::new(Style {
        flex_shrink: 0.,
        ..Default::default()
    })
    .content(Content::Text {
        text: text.into(),
        style,
        line_height,
        wrap: false,
    })
}

fn inset(
    t: Option<f32>,
    r: Option<f32>,
    b: Option<f32>,
    l: Option<f32>,
) -> taffy::Rect<LengthPercentageAuto> {
    let v = |x: Option<f32>| x.map_or(LengthPercentageAuto::auto(), LengthPercentageAuto::length);
    taffy::Rect {
        top: v(t),
        right: v(r),
        bottom: v(b),
        left: v(l),
    }
}

fn absolute(t: Option<f32>, r: Option<f32>, b: Option<f32>, l: Option<f32>) -> Style {
    Style {
        position: Position::Absolute,
        inset: inset(t, r, b, l),
        ..Default::default()
    }
}

fn column(gap: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: px(0.),
            height: px(gap),
        },
        min_size: Size {
            width: length(0.),
            height: length(0.),
        },
        ..Default::default()
    }
}

fn row(gap: f32) -> Style {
    Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(gap),
            height: px(0.),
        },
        ..Default::default()
    }
}

fn pill_shadows(paint: Paint) -> Paint {
    paint
        .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#fffbe0")))
        .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")))
}

fn arrow(left: bool, size: f32, color: Color, shadow: bool) -> El {
    let path = if left {
        "M13 3 5 10l8 7z"
    } else {
        "M7 3l8 7-8 7z"
    };
    El::new(Style {
        size: Size {
            width: length(size),
            height: length(size),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .content(Content::Shape(Shape {
        path: path.into(),
        bounds: [0., 0., 20., 20.],
        unit: size / 20.,
        fill: color,
        stroke: None,
        drop_shadow: shadow.then_some(crate::text::Shadow {
            dx: 0.,
            dy: 1.,
            blur: 2.,
            color: Color::hex("#000a"),
        }),
    }))
}

/// Media-query derived sizes (menu-screens.css).
struct Metrics {
    w: f32,
    h: f32,
    portrait: bool,
    /// `(orientation: landscape) and (max-height: 500px)`.
    short: bool,
    gutter: f32,
    row: f32,
    row_font: f32,
}

impl Metrics {
    fn new(view: &Viewport) -> Self {
        let (w, h) = (view.width, view.height);
        let portrait = h > w;
        let short = !portrait && h <= 500.;
        Self {
            w,
            h,
            portrait,
            short,
            gutter: clamp(16., w * 0.032, 44.),
            row: if portrait {
                if h <= 640. { 46. } else { 50. }
            } else if short {
                44.
            } else {
                clamp(48., h * 0.064, 60.)
            },
            row_font: if short {
                18.
            } else {
                clamp(19., w * 0.023, 27.)
            },
        }
    }
}

/// The full-viewport frontend page. While it slides in (frontend.js
/// `animatePage`), the whole HD screen moves and fades in `#menu-stage`
/// over `#frontend`'s own background.
pub fn element(view: &Viewport, s: &Screen) -> El {
    let page = page_element(view, s);
    match s.motion {
        Some(m) => El::new(Style {
            size: Size {
                width: length(view.width),
                height: length(view.height),
            },
            ..Default::default()
        })
        .paint(Paint::fill(Color::hex("#080606")))
        .child(page.opacity(m.opacity).translate(m.dx, 0.)),
        None => page,
    }
}

fn page_element(view: &Viewport, s: &Screen) -> El {
    let m = Metrics::new(view);
    let (w, h) = (m.w, m.h);
    let mut root = El::new(Style {
        position: Position::Relative,
        size: Size {
            width: length(w),
            height: length(h),
        },
        display: Display::Grid,
        ..Default::default()
    })
    .paint(Paint::fill(Color::hex("#2a0505")));
    root = root.children(backdrop(&m, s));
    let page = s.page.as_str();
    // Settings opened from the main menu draw over the bare backdrop.
    if page == "backdrop" {
        return root;
    }
    if page == "title" {
        return title(&m, s, root);
    }
    let loading = page == "loading";
    // Grid tracks and areas.
    let left_column = if m.short { (280., 0.46) } else { (300., 0.44) };
    let style = &mut root.style;
    if m.portrait {
        style.grid_template_columns = vec![minmax(length(0.), fr(1.))];
        let stage = match page {
            page if stage_page(page) => {
                if h <= 640. {
                    0.4
                } else {
                    0.5
                }
            }
            "garage" => {
                if h <= 640. {
                    0.3
                } else {
                    0.4
                }
            }
            "gear" => {
                if h <= 640. {
                    0.2
                } else {
                    0.26
                }
            }
            _ => -1.,
        };
        style.grid_template_rows = if loading {
            vec![auto(), auto(), minmax(length(0.), fr(1.)), auto()]
        } else if stage > 0. {
            vec![
                auto(),
                minmax(length(0.), percent(stage)),
                minmax(length(0.), fr(1.)),
                auto(),
            ]
        } else {
            vec![
                auto(),
                minmax(length(0.), auto()),
                minmax(length(0.), fr(1.)),
                auto(),
            ]
        };
    } else {
        style.grid_template_columns = vec![
            minmax(length(left_column.0), percent(left_column.1)),
            minmax(length(0.), fr(1.)),
        ];
        style.grid_template_rows = vec![auto(), minmax(length(0.), fr(1.)), auto()];
    }
    // grid-area placements: (row start, row end, column start, column end).
    let area = |r: (i16, i16), c: (i16, i16)| {
        taffy::Line {
            start: line(r.0),
            end: line(r.1),
        }
        .pipe(|rows| {
            (
                rows,
                taffy::Line {
                    start: line(c.0),
                    end: line(c.1),
                },
            )
        })
    };
    let (header_area, panel_area, left_area, hints_area) = if m.portrait {
        if loading {
            (
                area((1, 2), (1, 2)),
                area((3, 4), (1, 2)),
                area((2, 3), (1, 2)),
                area((4, 5), (1, 2)),
            )
        } else {
            (
                area((1, 2), (1, 2)),
                area((2, 3), (1, 2)),
                area((3, 4), (1, 2)),
                area((4, 5), (1, 2)),
            )
        }
    } else {
        (
            area((1, 2), (1, 2)),
            area((1, 3), (2, 3)),
            area((2, 3), (1, 2)),
            area((3, 4), (1, 3)),
        )
    };
    if !m.portrait {
        root = root.child(panel_shape(&m));
    }
    let mut panel = panel(&m, s, loading);
    (panel.style.grid_row, panel.style.grid_column) = panel_area;
    let mut header = heading(&m, s);
    (header.style.grid_row, header.style.grid_column) = header_area;
    let mut left = left(&m, s);
    (left.style.grid_row, left.style.grid_column) = left_area;
    // Portrait: the panel paints its silver behind the heading.
    root = root.child(panel).child(header).child(left);
    if loading {
        let mut bar = loading_bar(&m, s);
        (bar.style.grid_row, bar.style.grid_column) = hints_area;
        root = root.child(bar);
    } else if let Some(mut hints) = hints(&m, s) {
        (hints.style.grid_row, hints.style.grid_column) = hints_area;
        root = root.child(hints);
    }
    root
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

/// `.ms-backdrop`: PAGE01's photo (or the course photo), blurred, under the
/// vignette.
fn backdrop(m: &Metrics, s: &Screen) -> Vec<El> {
    let mut out = Vec::new();
    if let Some(image) = s.backdrop {
        out.push(
            El::new(absolute(Some(-40.), Some(-40.), Some(-40.), Some(-40.)))
                .content(image.content(Fit::Cover)),
        );
    }
    let vignette = |fill: Fill| {
        El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.))).paint(Paint::fill(fill))
    };
    out.push(vignette(Fill::Linear(Gradient::new(
        180.,
        &[
            (0., Color::hex("#0000")),
            (0.55, Color::hex("#0000")),
            (1., Color::hex("#12020299")),
        ],
    ))));
    out.push(vignette(Fill::ellipse_at(
        [0.18, 0.3],
        &[
            (0., Color::hex("#0000")),
            (0.35, Color::hex("#0000")),
            (1., Color::hex("#1a0202b0")),
        ],
    )));
    let _ = m;
    out
}

/// `.ms-panel-shape`: the silver stage sweeping in from the right.
fn panel_shape(m: &Metrics) -> El {
    let radius = clamp(90., m.h * 0.18, 200.);
    let mut paint = Paint::fill(silver())
        .shadow(BoxShadow::outer(-14., 0., 34., Color::hex("#000a")))
        .shadow(BoxShadow::inset(3., 0., 0., Color::hex("#f2f2f2")))
        .shadow(BoxShadow::inset(7., 0., 10., Color::hex("#ffffff55")));
    paint.radii = [radius, 0., 0., 0.];
    paint.skew_x = 7f32.to_radians().tan();
    El::new(absolute(
        Some(-2.),
        Some(-0.2 * m.w),
        Some(-2.),
        Some(0.44 * m.w + 0.04 * m.w),
    ))
    .paint(paint)
}

fn silver() -> Fill {
    Fill::Linear(Gradient::new(
        180.,
        &[
            (0., Color::hex("#bcbcbc")),
            (0.48, Color::hex("#a9a9a9")),
            (1., Color::hex("#8c8c8c")),
        ],
    ))
}

fn page_title(page: &str) -> &'static str {
    match page {
        "main" => "Main Menu",
        "single" => "Single Player",
        "multiplayer" => "Multiplayer",
        "create-lobby" => "Create Lobby",
        "join-lobby" => "Join Lobby",
        "lobby" => "Free Ride Lobby",
        "garage" => "Select ATV & Rider",
        "tracks" => "Select Track",
        "race" => "Single Race",
        "gear" => "Rider Gear",
        "loading" => "Now Loading...",
        "settings" => "Settings",
        _ => "",
    }
}

fn heading(m: &Metrics, s: &Screen) -> El {
    let dark = m.portrait && s.page != "loading";
    let size = if m.portrait {
        clamp(24., m.w * 0.07, 32.)
    } else if m.short {
        24.
    } else {
        clamp(26., m.w * 0.036, 44.)
    };
    let mut style = TextStyle::new(
        display(size, 800, true),
        Color::hex(if dark { "#1a1a1a" } else { "#f4f4f4" }),
    )
    .spacing(size * 0.015);
    style = if dark {
        style.shadow(0., 1., 0., Color::hex("#fff8"))
    } else {
        style
            .shadow(2., 3., 0., Color::hex("#000a"))
            .shadow(0., 0., 18., Color::hex("#0006"))
    };
    let fill = if dark {
        Gradient::new(
            90.,
            &[
                (0., Color::hex("#00000038")),
                (0.6, Color::hex("#00000014")),
                (1., Color::hex("#0000")),
            ],
        )
    } else {
        Gradient::new(
            90.,
            &[
                (0., Color::hex("#ffffff52")),
                (0.55, Color::hex("#ffffff26")),
                (1., Color::hex("#fff0")),
            ],
        )
    };
    let (pad_top, pad_bottom) = if m.short { (3., 4.) } else { (6., 8.) };
    let strip = El::new(Style {
        padding: sides(pad_top, m.gutter, pad_bottom, m.gutter),
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        overflow: taffy::Point {
            x: taffy::Overflow::Hidden,
            y: taffy::Overflow::Visible,
        },
        ..Default::default()
    })
    .paint(Paint::fill(fill))
    .content(Content::Text {
        text: page_title(&s.page).into(),
        style,
        line_height: size * 1.05,
        wrap: false,
    });
    let padding = if m.portrait {
        (10., 6.)
    } else if m.short {
        (8., 4.)
    } else {
        (clamp(12., m.h * 0.03, 28.), clamp(8., m.h * 0.016, 16.))
    };
    El::new(Style {
        padding: sides(padding.0, 0., padding.1, 0.),
        ..column(0.)
    })
    .child(strip)
}

/// `.ms-panel`: the stage contents (logo and ride plate, course hero,
/// garage plate and stats, loader info).
fn panel(m: &Metrics, s: &Screen, loading: bool) -> El {
    let mut style = column(if m.portrait {
        8.
    } else if m.short {
        6.
    } else {
        12.
    });
    style.position = Position::Relative;
    style.padding = if m.portrait {
        sides(
            if loading { 34. } else { 4. },
            m.gutter,
            if matches!(s.page.as_str(), "tracks" | "race") {
                if m.h <= 640. { 16. } else { 22. }
            } else {
                10.
            },
            m.gutter,
        )
    } else {
        sides(
            if m.short {
                10.
            } else {
                clamp(16., m.h * 0.04, 40.)
            },
            m.gutter,
            if m.short { 6. } else { 12. },
            clamp(64., m.w * 0.04 + m.h * 0.12, 160.),
        )
    };
    let mut panel = El::new(style);
    if m.portrait {
        // `.ms-panel::before`: silver reaching up behind the heading with a
        // sweeping elliptical lower-left corner.
        let mut paint = Paint::fill(silver());
        if loading {
            paint.radii = [0.75 * 1.2 * m.w, 0., 0., 0.];
            paint.radii_y = [56., 0., 0., 0.];
            paint = paint
                .shadow(BoxShadow::outer(0., -10., 30., Color::hex("#000a")))
                .shadow(BoxShadow::inset(0., 3., 0., Color::hex("#f2f2f2")));
        } else {
            paint.radii = [0., 0., 0., 0.75 * 1.2 * m.w];
            paint.radii_y = [0., 0., 0., 56.];
            paint = paint
                .shadow(BoxShadow::outer(0., 14., 30., Color::hex("#000a")))
                .shadow(BoxShadow::inset(0., -3., 0., Color::hex("#f2f2f2")));
        }
        let (top, bottom) = if loading {
            (0., -0.4 * m.h)
        } else {
            (-240., 0.)
        };
        panel = panel.child(
            El::new(absolute(
                Some(top),
                Some(-0.1 * m.w),
                Some(bottom),
                Some(-0.1 * m.w),
            ))
            .paint(paint),
        );
    }
    match s.page.as_str() {
        page if stage_page(page) => {
            if let Some(logo) = panel_logo(m, s) {
                panel = panel.child(logo);
            }
            // `.ms-model`: flex 1, min-height 72px.
            let mut slot = El::new(Style {
                flex_grow: 1.,
                flex_basis: length(0.),
                min_size: Size {
                    width: auto(),
                    height: length(72.),
                },
                ..Default::default()
            })
            .id(MODEL_QUAD);
            if let Some(image) = s.quad_model {
                slot = slot.content(image.model());
            } else if let Some(status) = s.model_status[0] {
                slot = model_status(slot, status);
            }
            panel = panel.child(slot).child(plate(m, s));
        }
        "tracks" | "lobby" | "race" => panel = panel.child(hero(m, s)),
        "garage" | "gear" => {
            // `.ms-models`: the rider just behind the ATV's rear wheel.
            let model = |image: Option<Img>, id: u32, style: Style| {
                let el = El::new(style).id(id);
                let status = s.model_status[usize::from(id == MODEL_RIDER)];
                match (image, status) {
                    (Some(image), _) => el.content(image.model()),
                    (None, Some(status)) => model_status(el, status),
                    (None, None) => el,
                }
            };
            let mut quad = absolute(Some(0.), Some(0.), Some(0.), Some(0.));
            quad.inset.top = LengthPercentageAuto::percent(0.14);
            quad.inset.right = LengthPercentageAuto::percent(0.3);
            let mut rider = absolute(Some(0.), Some(0.), None, None);
            rider.inset.bottom = LengthPercentageAuto::percent(0.02);
            rider.size.width = percent(0.36);
            let models = El::new(Style {
                position: Position::Relative,
                flex_grow: 1.,
                flex_basis: length(0.),
                min_size: Size {
                    width: auto(),
                    height: length(0.),
                },
                ..Default::default()
            })
            .child(model(s.quad_model, MODEL_QUAD, quad))
            .child(model(s.rider_model, MODEL_RIDER, rider));
            if s.page == "gear" {
                return panel.child(models);
            }
            panel = panel.child(plate(m, s)).child(models);
            if let Some(stats) = &s.stats
                && !(m.portrait && m.h <= 640.)
            {
                panel = panel.child(stats_panel(m, stats));
            }
        }
        "loading" => panel = panel.child(loading_info(m, s)),
        _ => {}
    }
    panel
}

fn panel_logo(m: &Metrics, s: &Screen) -> Option<El> {
    let logo = s.logo?;
    // width: min(100%, 460px) (portrait: max-width min(78%, 340px), max-height 24%).
    let (width, height) = if m.portrait {
        let width = (m.w - 2. * m.gutter) * 0.78;
        let width = width.min(340.);
        (length(width), length(width / 3.))
    } else {
        let w = if m.short { percent(0.7) } else { percent(1.) };
        (w, auto())
    };
    let mut style = Style {
        size: Size { width, height },
        max_size: Size {
            width: length(if m.short { 300. } else { 460. }),
            height: auto(),
        },
        aspect_ratio: Some(3.),
        flex_shrink: if m.portrait { 1. } else { 0. },
        align_self: if m.portrait {
            Some(AlignSelf::CENTER)
        } else {
            Some(AlignSelf::FLEX_START)
        },
        position: Position::Relative,
        ..Default::default()
    };
    style.min_size.height = length(0.);
    let mut el = El::new(style);
    // filter: drop-shadow(0 3px 0 #0006).
    if let Some(shadow) = s.logo_shadow {
        el = el.child(
            El::new(absolute(Some(3.), Some(0.), Some(-3.), Some(0.)))
                .content(shadow.content(Fit::Contain))
                .opacity(0.4 / 0.6),
        );
    }
    Some(
        el.child(
            El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.)))
                .content(logo.content(Fit::Contain)),
        ),
    )
}

/// `.ms-plate .ms-ride-name`.
fn plate(m: &Metrics, s: &Screen) -> El {
    let size = if m.short {
        22.
    } else {
        clamp(22., m.w * 0.028, 34.)
    };
    let mut el = El::new(Style {
        display: Display::Flex,
        justify_content: if m.portrait {
            JustifyContent::CENTER
        } else {
            JustifyContent::FLEX_START
        },
        flex_shrink: 0.,
        ..Default::default()
    });
    el = el.child(text(
        s.vehicle_label.clone(),
        TextStyle::new(display(size, 800, true), Color::hex("#111")).spacing(size * 0.01),
        size * 1.05,
    ));
    el
}

/// `.ms-track-hero`: the framed course photo with arrows, name and Ride.
fn hero(m: &Metrics, s: &Screen) -> El {
    let mut hero = column(if m.portrait { 8. } else { 16. });
    hero.justify_content = JustifyContent::CENTER;
    // padding-bottom: 6vh applies in both orientations.
    hero.padding = sides(0., 0., m.h * 0.06, 0.);
    if !m.portrait {
        hero.flex_grow = 1.;
    }
    let aspect = if m.portrait {
        if m.h <= 640. { 2.4 } else { 16. / 9. }
    } else {
        1.5
    };
    let mut photo = El::new(Style {
        position: Position::Relative,
        size: Size {
            width: percent(1.),
            height: auto(),
        },
        max_size: Size {
            width: if m.portrait { auto() } else { length(580.) },
            height: if m.short { percent(0.62) } else { auto() },
        },
        aspect_ratio: Some(aspect),
        flex_shrink: 1.,
        min_size: Size {
            width: auto(),
            height: length(0.),
        },
        ..Default::default()
    })
    .paint(
        Paint::fill(Color::hex("#222"))
            .radius(5.)
            .shadow(BoxShadow::outer(0., 4., 0., Color::hex("#000a"))),
    );
    if let Some(image) = s.hero {
        photo = photo.child(
            El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.)))
                .content(image.content(Fit::Cover)),
        );
    }
    photo = photo.child(
        El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.))).paint(
            Paint::fill(Gradient::new(
                180.,
                &[
                    (0., Color::hex("#fff1")),
                    (0.3, Color::hex("#0000")),
                    (0.7, Color::hex("#0000")),
                    (1., Color::hex("#0004")),
                ],
            ))
            .radius(5.),
        ),
    );
    // The lobby's `.ms-track-hero` is the photo and name only.
    let lobby = s.page == "lobby";
    let race = s.page == "race";
    for (left, id) in [(true, PREV), (false, NEXT)] {
        if lobby {
            break;
        }
        let hovered = s.hovered == Some(id);
        let mut style = absolute(None, (!left).then_some(10.), None, left.then_some(10.));
        style.inset.top = LengthPercentageAuto::percent(0.5);
        style.margin = margins(-28., 0., 0., 0.);
        style.size = Size {
            width: length(46.),
            height: length(56.),
        };
        style.display = Display::Flex;
        style.align_items = AlignItems::CENTER;
        style.justify_content = JustifyContent::CENTER;
        photo = photo.child(
            El::new(style)
                .paint(
                    Paint::fill(Color::hex(if hovered { "#000000b3" } else { "#0000008c" }))
                        .radius(5.),
                )
                .id(id)
                .child(arrow(left, 20., Color::WHITE, false)),
        );
    }
    let name_size = if m.portrait {
        24.
    } else if m.short {
        22.
    } else {
        clamp(24., m.w * 0.03, 38.)
    };
    let ride_size = if m.portrait {
        19.
    } else if m.short {
        18.
    } else {
        22.
    };
    let ride_height = if m.portrait {
        48.
    } else if m.short {
        44.
    } else {
        52.
    };
    // The caption grid ("badge ride" "name ride" "mode ride"; portrait drops
    // the badge row): Chrome gives each empty row an equal share of Ride's
    // height, so the name sits one share down and Ride centres on all rows.
    let empty_rows = if m.portrait { 1. } else { 2. };
    let share = ride_height / (empty_rows + 1.);
    let name_height = (name_size * 1.05).max(share);
    let badge = if m.portrait { 0. } else { share };
    let caption = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::FLEX_START,
        justify_content: JustifyContent::SPACE_BETWEEN,
        gap: Size {
            width: px(16.),
            height: px(0.),
        },
        min_size: Size {
            width: auto(),
            height: length(name_height + empty_rows * share),
        },
        max_size: Size {
            width: if m.portrait { auto() } else { length(580.) },
            height: auto(),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .child({
        let name = El::new(Style {
            min_size: Size {
                width: length(0.),
                height: auto(),
            },
            ..Default::default()
        })
        .content(Content::Text {
            text: s.hero_name.clone(),
            style: TextStyle::new(display(name_size, 800, true), Color::hex("#111")),
            line_height: name_size * 1.05,
            wrap: false,
        });
        let mut copy = El::new(Style {
            flex_grow: 1.,
            flex_shrink: 1.,
            margin: margins(badge, 0., 0., 0.),
            ..column(2.)
        })
        .child(name);
        if race && !s.hero_mode.is_empty() {
            // `.ms-track-mode`: 700 14px/1.3, .1em, upper case.
            copy = copy.child(text(
                s.hero_mode.to_uppercase(),
                TextStyle::new(display(14., 700, false), Color::hex("#3a3a3a")).spacing(1.4),
                18.2,
            ));
        }
        copy
    })
    .children((!lobby).then(|| {
        let (label, id) = if race {
            ("Start Race", START)
        } else {
            ("Ride", RIDE)
        };
        let mut ride = gold_button(
            label,
            id,
            ride_size,
            ride_height,
            if m.portrait { 24. } else { 34. },
            s.hovered == Some(id),
            race && current_action(s) == Some("racestart"),
        );
        ride.style.align_self = Some(AlignSelf::CENTER);
        ride
    }));
    hero.flex_shrink = 1.;
    El::new(hero).child(photo).child(caption)
}

/// `.ms-pill`: Ride, Done and Press Start.
fn gold_button(
    label: &str,
    id: u32,
    size: f32,
    min_height: f32,
    pad: f32,
    hovered: bool,
    ring: bool,
) -> El {
    let mut paint = pill_shadows(Paint::fill(gold_pill()).radius(5.));
    if ring {
        paint.shadows.insert(
            0,
            BoxShadow {
                spread: 3.,
                ..BoxShadow::outer(0., 0., 0., Color::WHITE)
            },
        );
    }
    let mut el = El::new(Style {
        position: Position::Relative,
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        min_size: Size {
            width: auto(),
            height: length(min_height),
        },
        padding: sides(0., pad, 0., pad),
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(paint)
    .id(id)
    .child(text(
        label.to_uppercase(),
        TextStyle::new(display(size, 800, true), Color::hex("#0d0b00")).spacing(size * 0.06),
        size,
    ));
    if hovered {
        // `filter: brightness(1.08)`: a faint white wash.
        el = el.child(
            El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.)))
                .paint(Paint::fill(Color::hex("#ffffff12")).radius(5.)),
        );
    }
    el
}

fn left(m: &Metrics, s: &Screen) -> El {
    let mut style = column(if m.portrait {
        if s.page == "garage" && m.h <= 640. {
            6.
        } else {
            10.
        }
    } else if s.page == "garage" && m.short {
        4.
    } else {
        clamp(8., m.h * 0.016, 16.)
    });
    style.position = Position::Relative;
    style.padding = if m.portrait {
        sides(14., m.gutter, 10., m.gutter)
    } else {
        sides(0., 0., 12., m.gutter)
    };
    if s.page == "loading" {
        style.justify_content = JustifyContent::CENTER;
        style.padding = if m.portrait {
            sides(10., m.gutter, 10., m.gutter)
        } else {
            sides(0., 12., 12., m.gutter)
        };
    }
    let mut left = El::new(style);
    match s.page.as_str() {
        "create-lobby" | "join-lobby" => {
            left = left.child(form(m, s));
            let mut rows = El::new(column(2.));
            rows.style.flex_shrink = 0.;
            for (index, (action, label)) in s.choices.iter().enumerate() {
                if action != "back" {
                    rows = rows.child(menu_row(m, s, index, action, label));
                }
            }
            left = left.child(rows);
            if let Some(notice) = &s.notice {
                left = left.child(text(
                    notice.clone(),
                    TextStyle::new(display(15., 700, false), Color::hex("#ffd166")),
                    20.,
                ));
            }
        }
        "main" | "single" | "multiplayer" => {
            let mut rows = El::new(column(2.));
            rows.style.flex_shrink = 0.;
            rows.style.padding = sides(0., if m.portrait { 0. } else { 8. }, 0., 0.);
            for (index, (action, label)) in s.choices.iter().enumerate() {
                if action == "back" {
                    continue;
                }
                rows = rows.child(menu_row(m, s, index, action, label));
            }
            left = left.child(rows);
            if let Some(profile) = &s.profile {
                left = left.child(
                    El::new(Style {
                        flex_shrink: 0.,
                        margin: margins(12., 0., 0., if m.portrait { 0. } else { 8. }),
                        ..Default::default()
                    })
                    .child(text(
                        profile.clone(),
                        TextStyle::new(display(16., 700, false), Color::hex("#f1d84e")).shadow(
                            1.,
                            2.,
                            0.,
                            Color::hex("#000a"),
                        ),
                        20.,
                    )),
                );
            }
            // `#menu-notice`, e.g. why a ride link could not open.
            if let Some(notice) = &s.notice {
                left = left.child(
                    El::new(Style {
                        flex_shrink: 0.,
                        margin: margins(10., 0., 0., 0.),
                        max_size: Size {
                            width: length(m.w.min(460.)),
                            height: auto(),
                        },
                        ..Default::default()
                    })
                    .content(Content::Text {
                        text: notice.clone(),
                        style: TextStyle::new(display(15., 700, false), Color::hex("#ffd166")),
                        line_height: 20.,
                        wrap: true,
                    }),
                );
            }
        }
        "tracks" => {
            left = left.child(tabs(m, s)).child(course_list(m, s));
        }
        "race" => {
            // `.ms-race-settings`: the value rows under the course list.
            let mut settings = El::new(column(2.));
            settings.style.flex_shrink = 0.;
            settings.style.padding = sides(4., if m.portrait { 0. } else { 8. }, 0., 0.);
            settings.paint.border = [1., 0., 0., 0.];
            settings.paint.border_color = Color::hex("#ffffff26");
            settings.style.border = sides(1., 0., 0., 0.);
            for (index, (action, label)) in s.choices.iter().enumerate() {
                if matches!(action.as_str(), "racelaps" | "racecpu" | "racedifficulty") {
                    settings = settings.child(stepper_row(m, s, index, label));
                }
            }
            left = left
                .child(tabs(m, s))
                .child(course_list(m, s))
                .child(settings);
            if let Some(notice) = &s.notice {
                left = left.child(text(
                    notice.clone(),
                    TextStyle::new(display(15., 700, false), Color::hex("#ffd166")),
                    20.,
                ));
            }
        }
        "gear" => {
            let mut rows = El::new(column(2.));
            rows.style.flex_shrink = 0.;
            rows.style.padding = sides(0., if m.portrait { 0. } else { 8. }, 0., 0.);
            for (index, (action, label)) in s.choices.iter().enumerate() {
                if action != "back" {
                    rows = rows.child(stepper_row(m, s, index, label));
                }
            }
            left = left.child(rows);
        }
        "lobby" => {
            // `.ms-lobby-info`: the code large, the status under it, then
            // the lobby actions.
            let lobby = s.lobby.as_ref();
            let code = lobby
                .map(|l| l.code.clone())
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| "--------".into());
            let status = lobby
                .map(|l| l.status.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "Connecting…".into());
            let size = clamp(30., m.w * 0.04, 44.);
            let mut info = El::new(column(4.));
            info.style.flex_shrink = 0.;
            info = info
                .child(text(
                    code,
                    TextStyle::new(display(size, 800, true), Color::hex("#ffffff"))
                        .spacing(size * 0.14)
                        .shadow(0., 1., 3., Color::hex("#000")),
                    size,
                ))
                .child(text(
                    status,
                    TextStyle::new(display(15., 600, false), Color::hex("#e4e4e4")).shadow(
                        0.,
                        1.,
                        3.,
                        Color::hex("#000"),
                    ),
                    20.,
                ));
            left = left.child(info);
            let mut rows = El::new(column(2.));
            rows.style.flex_shrink = 0.;
            for (index, (action, label)) in s.choices.iter().enumerate() {
                if action != "back" {
                    rows = rows.child(menu_row(m, s, index, action, label));
                }
            }
            left = left.child(rows);
            if let Some(notice) = &s.notice {
                left = left.child(text(
                    notice.clone(),
                    TextStyle::new(display(15., 700, false), Color::hex("#ffd166")),
                    20.,
                ));
            }
        }
        "garage" => left = garage_left(m, s, left),
        "loading" => left = left.child(shot(s)),
        _ => {}
    }
    left
}

fn bullet() -> El {
    El::new(Style {
        size: Size {
            width: length(14.),
            height: length(14.),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(
        Paint::fill(Fill::radial_at(
            [0.34, 0.3],
            &[
                (0., Color::hex("#fff8bf")),
                (0.32, Color::hex("#ead046")),
                (0.62, Color::hex("#b49704")),
                (1., Color::hex("#5e4d02")),
            ],
        ))
        .radius(7.)
        .shadow(BoxShadow::outer(0., 1., 2., Color::BLACK))
        .shadow(BoxShadow::inset(0., -1., 1., Color::hex("#0006"))),
    )
}

/// `.ms-row::before`: the glossy gold pill running off the left edge.
fn row_pill(m: &Metrics, right: f32) -> El {
    El::new(absolute(
        Some(5.),
        Some(right),
        Some(5.),
        Some(-m.gutter - 30.),
    ))
    .paint(pill_shadows(Paint::fill(gold_pill()).radius(5.)))
}

fn menu_row(m: &Metrics, s: &Screen, index: usize, action: &str, label: &str) -> El {
    let selected = s.selection == index;
    let mut style = row(14.);
    style.position = Position::Relative;
    style.size.width = percent(1.);
    style.min_size.height = length(m.row);
    style.padding = sides(0., 18., 0., 6.);
    let mut el = El::new(style).id(ROW + index as u32);
    if selected {
        el = el.child(row_pill(m, if m.portrait { -12. } else { 0. }));
    }
    let mut text_style = TextStyle::new(
        display(m.row_font, 700, false),
        Color::hex(if selected { "#0d0b00" } else { "#d2d2d2" }),
    )
    .spacing(m.row_font * 0.035);
    text_style = if selected {
        text_style.shadow(0., 1., 0., Color::hex("#fff6b066"))
    } else {
        text_style.shadow(1., 2., 0., Color::hex("#000c"))
    };
    let bullet = if selected {
        // transform: scale(0) keeps its 14px slot.
        El::new(Style {
            size: Size {
                width: length(14.),
                height: length(14.),
            },
            flex_shrink: 0.,
            ..Default::default()
        })
    } else {
        bullet()
    };
    let mut el = el
        .child(bullet)
        .child(text(label, text_style, m.row_font * 1.1));
    if action == "freeride" {
        el = freeride_badges(m, el, selected, s.freeride_riders);
    }
    el
}

/// `.ms-new` and `.ms-live`: the NEW tag and the live rider count on the
/// Arlington Free Ride row (static; the browser's glow/pulse animations are
/// not drawn).
fn freeride_badges(m: &Metrics, el: El, selected: bool, riders: Option<u32>) -> El {
    let tag_font = m.row_font * 0.5;
    let mut tag = El::new(Style {
        flex_shrink: 0.,
        padding: sides(
            tag_font * 0.22,
            tag_font * 0.5,
            tag_font * 0.18,
            tag_font * 0.5,
        ),
        ..Default::default()
    })
    .paint(
        Paint::fill(Fill::Linear(Gradient::new(
            180.,
            &[(0., Color::hex("#ff4a3a")), (1., Color::hex("#c4140c"))],
        )))
        .radius(999.)
        .shadow(BoxShadow::inset(0., 0., 0., Color::hex("#ffd0c0aa")))
        .shadow(BoxShadow::outer(0., 0., 12., Color::hex("#ff3a2a88"))),
    );
    tag = tag.child(text(
        "NEW",
        TextStyle::new(display(tag_font, 700, false), Color::WHITE)
            .spacing(tag_font * 0.12)
            .shadow(0., 1., 0., Color::hex("#6008")),
        tag_font * 1.1,
    ));
    let mut el = el.child(tag);
    if let Some(riders) = riders {
        let live_font = m.row_font * 0.62;
        let empty = riders == 0;
        let (dot, color) = match (selected, empty) {
            (true, _) => ("#1f8a0a", "#123d05"),
            (false, true) => ("#9a9a9a", "#d8d8d8"),
            (false, false) => ("#5df03a", "#9ff07a"),
        };
        let mut style = row(live_font * 0.4);
        style.flex_shrink = 0.;
        style.margin.left = taffy::LengthPercentageAuto::auto();
        style.padding = sides(0., 0., 0., 10.);
        let mut dot_el = El::new(Style {
            size: Size {
                width: length(live_font * 0.62),
                height: length(live_font * 0.62),
            },
            flex_shrink: 0.,
            ..Default::default()
        })
        .paint(Paint::fill(Fill::Solid(Color::hex(dot))).radius(live_font));
        if !selected && !empty {
            dot_el = dot_el.paint(
                Paint::fill(Fill::Solid(Color::hex(dot)))
                    .radius(live_font)
                    .shadow(BoxShadow::outer(0., 0., 8., Color::hex(dot))),
            );
        }
        el = el.child(
            El::new(style).child(dot_el).child(text(
                format!("{riders} riding"),
                TextStyle::new(display(live_font, 700, false), Color::hex(color))
                    .spacing(live_font * 0.04),
                live_font * 1.1,
            )),
        );
    }
    el
}

/// `.ms-fields`: the Host/Join text fields (label over a dark input box).
/// The web build lays a real input over each box for mobile keyboards; the
/// canvas draws the value and, on desktop, the caret of the focused field.
fn form(m: &Metrics, s: &Screen) -> El {
    let mut fields = El::new(column(12.));
    fields.style.flex_shrink = 0.;
    fields.style.padding = sides(0., if m.portrait { 0. } else { 12. }, 0., 0.);
    for (index, (label, value, focused)) in s.fields.iter().enumerate() {
        let mut field = El::new(column(6.));
        field = field.child(text(
            label.clone(),
            TextStyle::new(display(15., 700, false), Color::hex("#eeeeee")).spacing(0.6),
            18.,
        ));
        let mut input = El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            min_size: Size {
                width: auto(),
                height: length(48.),
            },
            padding: sides(4., 20., 4., 20.),
            ..Default::default()
        })
        .id(FIELD + index as u32)
        .paint(
            Paint::fill(Color::hex("#0000008c"))
                .radius(8.)
                .shadow(BoxShadow {
                    spread: 1.,
                    ..BoxShadow::inset(
                        0.,
                        0.,
                        0.,
                        Color::hex(if *focused { "#e8c832" } else { "#ffffff26" }),
                    )
                }),
        );
        // The browser's own input draws the text over this box.
        let shown = if cfg!(target_arch = "wasm32") {
            String::new()
        } else if *focused {
            format!("{value}|")
        } else {
            value.clone()
        };
        input = input.child(text(
            shown,
            TextStyle::new(display(19., 700, false), Color::hex("#f4f4f4")).spacing(1.14),
            23.,
        ));
        fields = fields.child(field.child(input));
    }
    fields
}

fn tabs(m: &Metrics, s: &Screen) -> El {
    let mut style = row(6.);
    style.flex_wrap = if m.portrait {
        FlexWrap::NoWrap
    } else {
        FlexWrap::Wrap
    };
    style.gap.height = px(6.);
    style.flex_shrink = 0.;
    style.padding = if m.portrait {
        sides(0., m.gutter, 2., m.gutter)
    } else {
        sides(0., 12., 0., 0.)
    };
    if m.portrait {
        style.margin = margins(0., -m.gutter, 0., -m.gutter);
    }
    let mut el = El::new(style);
    for (index, (key, label, count)) in s.tabs.iter().enumerate() {
        let selected = *key == s.filter;
        let hovered = s.hovered == Some(TAB + index as u32) && !selected;
        let mut paint = Paint::default().radius(5.);
        if selected {
            paint.fill = Some(gold_pill());
        } else if hovered {
            paint.fill = Some(Color::hex("#ffffff14").into());
        }
        let mut label_style = TextStyle::new(
            display(15., 700, false),
            Color::hex(if selected { "#0d0b00" } else { "#eee" }),
        )
        .spacing(0.6);
        if !selected {
            label_style = label_style.shadow(0., 1., 3., Color::BLACK);
        }
        let mut count_style = TextStyle::new(
            display(12., 700, false),
            Color::hex(if selected { "#3a2e00" } else { "#f1d84e" }),
        )
        .spacing(0.48);
        if !selected {
            count_style = count_style.shadow(0., 1., 3., Color::BLACK);
        }
        let mut tab = row(6.);
        tab.min_size.height = length(44.);
        tab.padding = sides(0., 14., 0., 14.);
        tab.flex_shrink = 0.;
        tab.align_items = AlignItems::BASELINE;
        tab.align_content = AlignContent::CENTER;
        el = el.child(
            El::new(Style {
                align_items: AlignItems::CENTER,
                ..tab
            })
            .paint(paint)
            .id(TAB + index as u32)
            .child(text(label.as_str(), label_style, 15.))
            .child(text(count.to_string(), count_style, 15.)),
        );
    }
    el
}

fn course_list(m: &Metrics, s: &Screen) -> El {
    let mut style = column(4.);
    style.flex_grow = 1.;
    style.flex_shrink = 1.;
    style.flex_basis = auto();
    style.padding = sides(2., if m.portrait { 0. } else { 12. }, 8., 0.);
    style.overflow = taffy::Point {
        x: taffy::Overflow::Visible,
        y: taffy::Overflow::Scroll,
    };
    style.scrollbar_width = 0.;
    let mut list = El::new(style).id(LIST).scroll(s.list_scroll);
    let thumb_size = if m.short { (60., 40.) } else { (72., 48.) };
    let name_size = clamp(16., m.w * 0.017, 20.);
    for (index, (id, name, thumb)) in s.courses.iter().enumerate() {
        let selected = s.current.as_deref() == Some(id.as_str());
        let hovered = s.hovered == Some(COURSE + index as u32) && !selected;
        let mut paint = Paint::default().radius(5.);
        if selected && s.page == "race" && current_action(s) == Some("racetrack") {
            // The course list stands for the Course row: a white ring.
            paint = Paint::fill(gold_pill())
                .radius(5.)
                .shadow(BoxShadow {
                    spread: 3.,
                    ..BoxShadow::inset(0., 0., 0., Color::WHITE)
                })
                .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")));
        } else if selected {
            paint = pill_shadows(Paint::fill(gold_pill()).radius(5.));
        } else if hovered {
            paint.fill = Some(Color::hex("#ffffff14").into());
        }
        let mut card = row(12.);
        card.min_size.height = length(if m.short { 48. } else { 56. });
        card.padding = sides(4., 12., 4., 4.);
        card.flex_shrink = 0.;
        let mut thumb_el = El::new(Style {
            size: Size {
                width: length(thumb_size.0),
                height: length(thumb_size.1),
            },
            flex_shrink: 0.,
            ..Default::default()
        })
        .paint(
            Paint::fill(Color::hex("#0006"))
                .radius(2.)
                .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009"))),
        );
        if let Some(image) = thumb {
            thumb_el = thumb_el.content(image.content(Fit::Cover));
        }
        let mut name_style = TextStyle::new(
            display(name_size, 700, false),
            Color::hex(if selected { "#0d0b00" } else { "#ececec" }),
        )
        .spacing(name_size * 0.02);
        if !selected {
            name_style = name_style.shadow(1., 2., 0., Color::hex("#000b"));
        }
        let copy = El::new(column(0.))
            .child(text(name.as_str(), name_style, name_size * 1.15))
            .child(text(
                group_label(id).to_uppercase(),
                TextStyle::new(
                    display(12., 700, false),
                    Color::hex(if selected { "#4a3a00" } else { "#e3c74a" }),
                )
                .spacing(1.44),
                15.6,
            ));
        list = list.child(
            El::new(card)
                .paint(paint)
                .id(COURSE + index as u32)
                .child(thumb_el)
                .child(copy),
        );
    }
    list
}

fn garage_left(m: &Metrics, s: &Screen, mut left: El) -> El {
    let selected_action = s.choices.get(s.selection).map(|(a, _)| a.as_str());
    let vehicle_selected = matches!(selected_action, Some("quadprev" | "quadnext"));
    let rider_selected = selected_action == Some("riderchange");
    let font = m.row_font;
    let cycle = |left_arrow: bool, id: u32, selected: bool| -> El {
        let hovered = s.hovered == Some(id);
        El::new(Style {
            size: Size {
                width: length(46.),
                height: length(44.),
            },
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            flex_shrink: 0.,
            ..Default::default()
        })
        .paint(if hovered {
            Paint::fill(Color::hex("#ffffff1f")).radius(5.)
        } else {
            Paint::default()
        })
        .id(id)
        .child(arrow(
            left_arrow,
            16.,
            Color::hex(if selected { "#1b1500" } else { "#f1d84e" }),
            !selected,
        ))
    };
    let mut rows = El::new(column(2.));
    rows.style.flex_shrink = 0.;
    rows.style.padding = sides(0., if m.portrait { 0. } else { 8. }, 0., 0.);
    // Vehicle row: arrows either side of the kind and name.
    let mut vehicle = row(14.);
    vehicle.position = Position::Relative;
    vehicle.min_size.height = length(m.row);
    vehicle.padding = sides(0., 18., 0., 6.);
    let mut vehicle_row = El::new(vehicle).id(ROW + index_of(s, "quadprev"));
    if vehicle_selected {
        vehicle_row = vehicle_row.child(row_pill(m, if m.portrait { -m.gutter - 30. } else { 0. }));
    }
    let kind_style = TextStyle::new(
        display(font * 0.62, 700, false),
        Color::hex(if vehicle_selected {
            "#4a3a00"
        } else {
            "#f1d84e"
        }),
    )
    .spacing(font * 0.62 * 0.14);
    let mut name_style = TextStyle::new(
        display(font * 0.92, 700, false),
        Color::hex(if vehicle_selected {
            "#0d0b00"
        } else {
            "#d2d2d2"
        }),
    )
    .spacing(font * 0.92 * 0.035);
    name_style = if vehicle_selected {
        name_style.shadow(0., 1., 0., Color::hex("#fff6b066"))
    } else {
        name_style.shadow(1., 2., 0., Color::hex("#000c"))
    };
    let mut kind_style = kind_style;
    if !vehicle_selected {
        kind_style = kind_style.shadow(1., 2., 0., Color::hex("#000c"));
    } else {
        kind_style = kind_style.shadow(0., 1., 0., Color::hex("#fff6b066"));
    }
    let stack = El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::CENTER,
        flex_grow: 1.,
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        ..Default::default()
    })
    .child(text(
        if s.quad_id >= 27 { "BIKE" } else { "ATV" },
        kind_style,
        font * 0.62 * 1.1,
    ))
    .child(text(s.vehicle_label.clone(), name_style, font * 0.92 * 1.1));
    vehicle_row = vehicle_row
        .child(cycle(true, QUAD_PREV, vehicle_selected))
        .child(stack)
        .child(cycle(false, QUAD_NEXT, vehicle_selected));
    rows = rows.child(vehicle_row);
    // Rider row: bullet, label and the Male/Female switch.
    let mut rider = row(14.);
    rider.position = Position::Relative;
    rider.min_size.height = length(m.row);
    rider.padding = sides(0., 18., 0., 6.);
    let mut rider_row = El::new(rider).id(ROW + index_of(s, "riderchange"));
    if rider_selected {
        rider_row = rider_row.child(row_pill(m, if m.portrait { -m.gutter - 30. } else { 0. }));
    }
    let mut label_style = TextStyle::new(
        display(font, 700, false),
        Color::hex(if rider_selected { "#0d0b00" } else { "#d2d2d2" }),
    )
    .spacing(font * 0.035);
    label_style = if rider_selected {
        label_style.shadow(0., 1., 0., Color::hex("#fff6b066"))
    } else {
        label_style.shadow(1., 2., 0., Color::hex("#000c"))
    };
    let switch_size = font * 0.74;
    let mut switch = El::new(Style {
        display: Display::Flex,
        padding: sides(3., 3., 3., 3.),
        margin: taffy::Rect {
            left: LengthPercentageAuto::auto(),
            right: LengthPercentageAuto::length(0.),
            top: LengthPercentageAuto::length(0.),
            bottom: LengthPercentageAuto::length(0.),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(
        Paint::fill(Color::hex(if rider_selected {
            "#1b150026"
        } else {
            "#0000005c"
        }))
        .radius(5.),
    );
    // Motorcycles carry the MX rider: the switch dims and shows no choice.
    let mx = s.quad_id >= 27;
    for (value, name) in [(0u16, "Male"), (1, "Female")] {
        let on = s.rider == value && !mx;
        let (fill, color) = match (on, rider_selected) {
            (true, true) => (Some(Fill::Solid(Color::hex("#1b1500"))), "#f6e061"),
            (true, false) => (Some(gold_pill()), "#0d0b00"),
            (false, true) => (None, "#3a2e00"),
            (false, false) => (None, "#e6e6e6"),
        };
        let mut style = TextStyle::new(display(switch_size, 700, false), Color::hex(color))
            .spacing(switch_size * 0.03);
        if !on && !rider_selected {
            style = style.shadow(1., 2., 0., Color::hex("#000c"));
        }
        let mut paint = Paint::default().radius(5.);
        paint.fill = fill;
        switch = switch.child(
            El::new(Style {
                padding: sides(7., 14., 7., 14.),
                ..Default::default()
            })
            .paint(paint)
            .content(Content::Text {
                text: name.into(),
                style,
                line_height: switch_size * 1.1,
                wrap: false,
            }),
        );
    }
    if mx {
        switch = switch.opacity(0.4);
    }
    rider_row = rider_row
        .child(bullet_slot(rider_selected))
        .child(text(
            if mx { "MX rider" } else { "Rider" },
            label_style,
            font * 1.1,
        ))
        .child(switch.id(RIDER));
    rows = rows.child(rider_row);
    left = left.child(rows);
    // The vehicle list: every ATV under one heading.
    let mut list_style = column(10.);
    list_style.flex_grow = 1.;
    list_style.flex_shrink = 1.;
    list_style.padding = sides(8., if m.portrait { 0. } else { 12. }, 8., 0.);
    list_style.border = sides(1., 0., 0., 0.);
    list_style.overflow = taffy::Point {
        x: taffy::Overflow::Visible,
        y: taffy::Overflow::Scroll,
    };
    list_style.scrollbar_width = 0.;
    let mut list = El::new(list_style).id(LIST).scroll(s.list_scroll);
    list.paint.border = [1., 0., 0., 0.];
    list.paint.border_color = Color::hex("#ffffff26");
    let item_size = clamp(16., m.w * 0.016, 19.);
    for (motorcycles, heading) in [(false, "ATVS"), (true, "MOTORCYCLES")] {
        let vehicles: Vec<&Vehicle> = s
            .catalog
            .iter()
            .filter(|v| v.motorcycle == motorcycles)
            .collect();
        if vehicles.is_empty() {
            continue;
        }
        let mut group = El::new(column(2.)).child(
            El::new(Style {
                padding: sides(4., 14., 4., 14.),
                flex_shrink: 0.,
                ..Default::default()
            })
            .content(Content::Text {
                text: heading.into(),
                style: TextStyle::new(display(13., 700, false), Color::hex("#f1d84e"))
                    .spacing(13. * 0.16)
                    .shadow(0., 1., 2., Color::BLACK),
                line_height: 15.6,
                wrap: false,
            }),
        );
        group.style.flex_shrink = 0.;
        for vehicle in vehicles {
            let id = vehicle.id;
            let selected = id == s.quad_id;
            let hovered = s.hovered == Some(VEHICLE + u32::from(id)) && !selected;
            let mut paint = Paint::default().radius(5.);
            if selected {
                paint.fill = Some(Color::hex("#0000006b").into());
                paint = paint.shadow(BoxShadow::inset(4., 0., 0., Color::hex("#f1d84e")));
            } else if hovered {
                paint.fill = Some(Color::hex("#ffffff14").into());
            }
            let mut item = row(12.);
            item.min_size.height = length(if m.short { 44. } else { 46. });
            item.padding = sides(0., 14., 0., 14.);
            item.flex_shrink = 0.;
            let mut el = El::new(item)
                .paint(paint)
                .id(VEHICLE + u32::from(id))
                .child(text(
                    vehicle.label.as_str(),
                    TextStyle::new(
                        display(item_size, 700, false),
                        Color::hex(if selected { "#f6e061" } else { "#e2e2e2" }),
                    )
                    .spacing(item_size * 0.02)
                    .shadow(1., 2., 0., Color::hex("#000b")),
                    item_size * 1.1,
                ));
            if !vehicle.class.is_empty() {
                // `.ms-vehicle small`: the class at the right.
                el = el.child(
                    El::new(Style {
                        margin: taffy::Rect {
                            left: LengthPercentageAuto::auto(),
                            right: LengthPercentageAuto::length(0.),
                            top: LengthPercentageAuto::length(0.),
                            bottom: LengthPercentageAuto::length(0.),
                        },
                        flex_shrink: 0.,
                        ..Default::default()
                    })
                    .content(Content::Text {
                        text: vehicle.class.clone(),
                        style: TextStyle::new(display(12., 700, false), Color::hex("#e3c74a"))
                            .spacing(0.96),
                        line_height: 12.,
                        wrap: false,
                    }),
                );
            }
            group = group.child(el);
        }
        list = list.child(group);
    }
    left = left.child(list);
    // Done (the garage's Back), under the list.
    let done_size = if m.short { 18. } else { 22. };
    let mut actions = row(10.);
    actions.margin = margins(0., if m.portrait { 0. } else { 12. }, 0., 0.);
    actions.flex_shrink = 0.;
    let mut done = gold_button(
        "Done",
        DONE,
        done_size,
        if m.short || (m.portrait && m.h <= 640.) {
            if m.short { 44. } else { 46. }
        } else {
            52.
        },
        34.,
        s.hovered == Some(DONE),
        selected_action == Some("back"),
    );
    done.style.min_size.width = length(180.);
    if m.portrait {
        done.style.flex_grow = 1.;
        done.style.flex_basis = length(0.);
    }
    let mut actions = El::new(actions);
    // `.ms-pill-secondary.ms-gear-link`: Rider gear opens its own page.
    if let Some(index) = s.choices.iter().position(|(a, _)| a == "gear") {
        let id = ROW + index as u32;
        let selected = s.selection == index;
        let hovered = s.hovered == Some(id) && !selected;
        let color = Color::hex(if selected { "#0d0b00" } else { "#f6e061" });
        let paint = if selected {
            pill_shadows(Paint::fill(gold_pill()).radius(5.))
        } else {
            Paint::fill(Color::hex(if hovered { "#000000b3" } else { "#0000008c" }))
                .radius(5.)
                .shadow(BoxShadow {
                    spread: 2.,
                    ..BoxShadow::inset(0., 0., 0., Color::hex("#f1d84e"))
                })
        };
        let mut style = row(10.);
        style.justify_content = JustifyContent::CENTER;
        style.min_size.height = done.style.min_size.height;
        style.padding = sides(0., 18., 0., 24.);
        style.flex_shrink = 1.;
        if m.portrait {
            style.flex_grow = 1.;
            style.flex_basis = length(0.);
        }
        actions = actions.child(
            El::new(style)
                .paint(paint)
                .id(id)
                .child(text(
                    "Rider gear",
                    TextStyle::new(display(19., 800, false), color).spacing(0.76),
                    19.,
                ))
                .child(arrow(false, 14., color, false)),
        );
    }
    left.child(actions.child(done))
}

fn bullet_slot(selected: bool) -> El {
    if selected {
        El::new(Style {
            size: Size {
                width: length(14.),
                height: length(14.),
            },
            flex_shrink: 0.,
            ..Default::default()
        })
    } else {
        bullet()
    }
}

fn index_of(s: &Screen, action: &str) -> u32 {
    s.choices
        .iter()
        .position(|(a, _)| a == action)
        .unwrap_or(usize::MAX - ROW as usize) as u32
}

/// `.ms-stats`: Engine, Traction and Suspension meters, top speed and logo.
fn stats_panel(m: &Metrics, stats: &Stats) -> El {
    let label_size = if m.portrait {
        13.
    } else if m.short {
        12.
    } else {
        clamp(14., m.w * 0.015, 18.)
    };
    let meter_height = if m.portrait {
        8.
    } else if m.short {
        7.
    } else {
        clamp(9., m.h * 0.012, 12.)
    };
    let label = |t: &str| {
        El::new(Style {
            size: Size {
                width: length(6.6 * label_size),
                height: auto(),
            },
            flex_shrink: 0.,
            display: Display::Flex,
            justify_content: JustifyContent::FLEX_END,
            ..Default::default()
        })
        .child(text(
            t,
            TextStyle::new(display(label_size, 800, true), Color::hex("#161616"))
                .spacing(label_size * 0.02),
            label_size,
        ))
    };
    let meter = |value: f32| {
        let steps = (value * 10.).round() as usize;
        let mut track = El::new(Style {
            display: Display::Flex,
            flex_grow: 1.,
            size: Size {
                width: auto(),
                height: length(meter_height),
            },
            gap: Size {
                width: px(2.),
                height: px(0.),
            },
            ..Default::default()
        });
        for i in 0..10 {
            let on = i < steps;
            track = track.child(
                El::new(Style {
                    flex_grow: 1.,
                    flex_basis: length(0.),
                    ..Default::default()
                })
                .paint(if on {
                    Paint::fill(Color::hex("#dac01f")).shadow(BoxShadow::outer(
                        0.,
                        1.,
                        0.,
                        Color::hex("#6a5600"),
                    ))
                } else {
                    Paint::fill(Color::hex("#00000038"))
                }),
            );
        }
        track
    };
    let mut rows = El::new(column(if m.portrait || m.short { 2. } else { 4. }));
    rows.style.flex_grow = 1.;
    for (name, value) in [
        ("Engine", stats.engine),
        ("Traction", stats.traction),
        ("Suspension", stats.suspension),
    ] {
        let mut line = row(10.);
        line.flex_shrink = 0.;
        rows = rows.child(El::new(line).child(label(name)).child(meter(value)));
    }
    if let Some(speed) = stats.top_speed {
        rows = rows.child(text(
            format!("Top speed {} mph", speed.round() as i32),
            TextStyle::new(display(label_size, 800, true), Color::hex("#161616")),
            label_size,
        ));
    }
    let mut panel = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(if m.portrait { 10. } else { 14. }),
            height: px(0.),
        },
        flex_shrink: 0.,
        padding: if m.portrait {
            sides(6., 10., 6., 10.)
        } else {
            sides(0., 0., 0., 0.)
        },
        ..Default::default()
    });
    if m.portrait {
        panel = panel.paint(
            Paint::fill(Color::hex("#eeeeeee6"))
                .radius(8.)
                .shadow(BoxShadow::outer(0., 2., 6., Color::hex("#0006"))),
        );
    }
    panel = panel.child(rows);
    if let Some(logo) = stats.logo {
        let max_w = if m.portrait {
            96.
        } else {
            clamp(80., m.w * 0.12, 150.)
        };
        let max_h = if m.portrait {
            44.
        } else if m.short {
            36.
        } else {
            64.
        };
        let s = (max_w / logo.width).min(max_h / logo.height).min(1.);
        panel = panel.child(
            El::new(Style {
                size: Size {
                    width: length(logo.width * s),
                    height: length(logo.height * s),
                },
                flex_shrink: 0.,
                ..Default::default()
            })
            .content(logo.content(Fit::Contain)),
        );
    }
    panel
}

/// The Back control bottom right.
fn hints(m: &Metrics, s: &Screen) -> Option<El> {
    if s.page == "garage" {
        return None;
    }
    let size = if m.short { 15. } else { 17. };
    let hovered = s.hovered == Some(BACK);
    let color = Color::hex(if hovered { "#fffaa1" } else { "#f2f2f2" });
    // A <button> resets letter-spacing (UA stylesheet); .ms-hint-back sets none.
    let style = TextStyle::new(display(size, 700, false), color).shadow(0., 1., 3., Color::BLACK);
    let back = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(7.),
            height: px(0.),
        },
        min_size: Size {
            width: length(64.),
            height: length(46.),
        },
        padding: sides(0., 12., 0., 12.),
        ..Default::default()
    })
    .paint(Paint::fill(Color::hex("#0000008f")).radius(5.))
    .id(BACK)
    .child(arrow(true, 16., color, false))
    .child(text("Back", style, size));
    // Race setup keeps its share link beside Back (`.ms-hint-share`).
    let share = (s.page == "race")
        .then(|| s.choices.iter().position(|(a, _)| a == "copyride"))
        .flatten()
        .map(|index| {
            let selected = s.selection == index;
            let hovered = s.hovered == Some(SHARE);
            let color = Color::hex(if selected {
                "#0d0b00"
            } else if hovered {
                "#fffaa1"
            } else {
                "#f2f2f2"
            });
            let mut style = TextStyle::new(display(size, 700, false), color);
            if !selected {
                style = style.shadow(0., 1., 3., Color::BLACK);
            }
            El::new(Style {
                display: Display::Flex,
                align_items: AlignItems::CENTER,
                min_size: Size {
                    width: length(64.),
                    height: length(46.),
                },
                padding: sides(0., 12., 0., 12.),
                ..Default::default()
            })
            .paint(if selected {
                pill_shadows(Paint::fill(gold_pill()).radius(5.))
            } else {
                Paint::fill(Color::hex("#0000008f")).radius(5.)
            })
            .id(SHARE)
            .child(text(s.choices[index].1.clone(), style, size))
        });
    let mut bar = row(clamp(14., m.w * 0.024, 30.));
    bar.min_size.height = length(if m.short { 44. } else { 50. });
    if m.portrait {
        bar.justify_content = JustifyContent::FLEX_END;
        bar.margin = margins(0., m.gutter, 6., m.gutter);
        bar.padding = sides(0., 0., 0., 4.);
    } else {
        bar.justify_self = Some(JustifySelf::END);
        bar.margin = margins(
            0.,
            0.,
            if m.short {
                6.
            } else {
                clamp(6., m.h * 0.014, 14.)
            },
            0.,
        );
        bar.padding = sides(0., m.gutter, 0., 26.);
    }
    Some(El::new(bar).children(share).child(back))
}

/// The selected choice's action.
fn current_action(s: &Screen) -> Option<&str> {
    s.choices.get(s.selection).map(|(a, _)| a.as_str())
}

/// `.ms-stepper`: a value row. Tapping the label steps forward; arrows
/// either side of the value step back or forward.
fn stepper_row(m: &Metrics, s: &Screen, index: usize, label: &str) -> El {
    let selected = s.selection == index;
    let (name, value) = label.split_once(": ").unwrap_or((label, ""));
    let mut style = row(0.);
    style.position = Position::Relative;
    style.size.width = percent(1.);
    style.min_size.height = length(m.row);
    style.padding = sides(0., 0., 0., 6.);
    let mut el = El::new(style);
    if selected {
        el = el.child(row_pill(m, if m.portrait { -12. } else { 0. }));
    }
    let mut text_style = TextStyle::new(
        display(m.row_font, 700, false),
        Color::hex(if selected { "#0d0b00" } else { "#d2d2d2" }),
    )
    .spacing(m.row_font * 0.035);
    text_style = if selected {
        text_style.shadow(0., 1., 0., Color::hex("#fff6b066"))
    } else {
        text_style.shadow(1., 2., 0., Color::hex("#000c"))
    };
    let mut label_style = row(14.);
    label_style.flex_grow = 1.;
    label_style.flex_shrink = 1.;
    label_style.min_size = Size {
        width: length(0.),
        height: length(m.row),
    };
    let label_el = El::new(label_style)
        .id(ROW + index as u32)
        .child(bullet_slot(selected))
        .child(text(name, text_style, m.row_font * 1.1));
    let cycle = |left: bool, id: u32| {
        let hovered = s.hovered == Some(id);
        El::new(Style {
            size: Size {
                width: length(46.),
                height: length(44.),
            },
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            flex_shrink: 0.,
            ..Default::default()
        })
        .paint(if hovered {
            Paint::fill(Color::hex("#ffffff1f")).radius(5.)
        } else {
            Paint::default()
        })
        .id(id)
        .child(arrow(
            left,
            16.,
            Color::hex(if selected { "#1b1500" } else { "#f1d84ebf" }),
            !selected,
        ))
    };
    let value_size = m.row_font * 0.86;
    let mut value_style = TextStyle::new(
        display(value_size, 700, false),
        Color::hex(if selected { "#3a2e00" } else { "#f1d84e" }),
    );
    if !selected {
        value_style = value_style.shadow(1., 2., 0., Color::hex("#000c"));
    }
    let value_el = El::new(Style {
        display: Display::Flex,
        justify_content: JustifyContent::CENTER,
        flex_grow: 0.,
        flex_shrink: 1.,
        // flex: 0 1 9em keeps the arrows in one column across rows.
        size: Size {
            width: length(9. * value_size),
            height: auto(),
        },
        min_size: Size {
            width: length(2.5 * value_size),
            height: auto(),
        },
        ..Default::default()
    })
    .child(
        El::new(Style {
            min_size: Size {
                width: length(0.),
                height: auto(),
            },
            ..Default::default()
        })
        .content(Content::Ellipsis {
            text: value.to_string(),
            style: value_style,
            line_height: value_size * 1.1,
        }),
    );
    let mut control = row(0.);
    control.flex_shrink = 1.;
    control.min_size.width = length(0.);
    el.child(label_el).child(
        El::new(control)
            .child(cycle(true, STEP_PREV + index as u32))
            .child(value_el)
            .child(cycle(false, STEP_NEXT + index as u32)),
    )
}

/// The title: full-bleed scenery, the centred wordmark and Press Start.
fn title(m: &Metrics, s: &Screen, mut root: El) -> El {
    root.style.grid_template_columns = vec![fr(1.)];
    root.style.grid_template_rows = vec![fr(1.), auto()];
    let width = (0.88 * m.w).min(760.);
    let mut lockup = El::new(Style {
        grid_row: taffy::Line {
            start: line(1),
            end: line(2),
        },
        grid_column: taffy::Line {
            start: line(1),
            end: line(2),
        },
        align_self: Some(AlignSelf::CENTER),
        justify_self: Some(JustifySelf::CENTER),
        position: Position::Relative,
        size: Size {
            width: length(width),
            height: length(width / 3.),
        },
        ..Default::default()
    });
    if let Some(shadow) = s.logo_shadow {
        // drop-shadow(0 4px 0 #0009).
        lockup = lockup.child(
            El::new(absolute(Some(4.), Some(0.), Some(-4.), Some(0.)))
                .content(shadow.content(Fit::Contain)),
        );
    }
    if let Some(logo) = s.logo {
        lockup = lockup.child(
            El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.)))
                .content(logo.content(Fit::Contain)),
        );
    }
    let start_index = s.choices.iter().position(|(a, _)| a == "main");
    let devlog_index = s.choices.iter().position(|(a, _)| a == "devlog");
    let downloads_index = s.choices.iter().position(|(a, _)| a == "downloads");
    let mut actions = El::new(Style {
        grid_row: taffy::Line {
            start: line(2),
            end: line(3),
        },
        grid_column: taffy::Line {
            start: line(1),
            end: line(2),
        },
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(0.),
            height: px(10.),
        },
        padding: sides(0., 16., clamp(24., m.h * 0.07, 64.), 16.),
        ..Default::default()
    });
    if let Some(index) = start_index {
        let label = &s.choices[index].1;
        let mut start = gold_button(
            label,
            ROW + index as u32,
            26.,
            60.,
            34.,
            s.hovered == Some(ROW + index as u32),
            false,
        );
        start.style.min_size.width = length(320f32.min(0.8 * m.w));
        if s.pulse > 1.001 {
            // ms-pulse: brightness(1 → 1.18).
            start = start.child(
                El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.))).paint(
                    Paint::fill(Color::rgba(255, 255, 255, ((s.pulse - 1.) * 0.6).min(0.2)))
                        .radius(5.),
                ),
            );
        }
        actions = actions.child(start);
    }
    // DEV LOG and DOWNLOADS (the desktop and Android releases) share a row under Start.
    let links: Vec<El> = [(devlog_index, "DEV LOG"), (downloads_index, "DOWNLOADS")]
        .into_iter()
        .filter_map(|(index, label)| index.map(|index| title_link(s, index, label)))
        .collect();
    if !links.is_empty() {
        let mut row = El::new(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::CENTER,
            gap: Size {
                width: px(6.),
                height: px(0.),
            },
            ..Default::default()
        });
        for link in links {
            row = row.child(link);
        }
        actions = actions.child(row);
    }
    root.child(lockup).child(actions)
}

/// A quiet text button under the title's Start (DEV LOG, DOWNLOADS).
fn title_link(s: &Screen, index: usize, label: &str) -> El {
    let selected = s.selection == index;
    let hovered = s.hovered == Some(ROW + index as u32);
    let mut paint = Paint::default().radius(5.);
    if selected {
        paint = paint.shadow(BoxShadow {
            spread: 3.,
            ..BoxShadow::outer(0., 0., 0., Color::hex("#f6e061b3"))
        });
    }
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        min_size: Size {
            width: auto(),
            height: length(46.),
        },
        padding: sides(0., 22., 0., 22.),
        ..Default::default()
    })
    .paint(paint)
    .id(ROW + index as u32)
    .child(text(
        label,
        TextStyle::new(
            display(16., 700, false),
            Color::hex(if hovered { "#fffaa1" } else { "#f2f2f2" }),
        )
        .spacing(1.28)
        .shadow(0., 1., 3., Color::BLACK),
        16.,
    ))
}

/// `.ms-shot`: the course action shot, crossfading between frames.
fn shot(s: &Screen) -> El {
    let mut frame = El::new(Style {
        position: Position::Relative,
        size: Size {
            width: percent(1.),
            height: auto(),
        },
        max_size: Size {
            width: length(460.),
            height: auto(),
        },
        aspect_ratio: Some(64. / 45.),
        border: sides(3., 3., 3., 3.),
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint({
        let mut p = Paint::fill(Color::hex("#111"))
            .radius(14.)
            .shadow(BoxShadow::outer(0., 4., 0., Color::hex("#0009")));
        p.border = [3.; 4];
        p.border_color = Color::hex("#0b0b0b");
        p
    });
    if let Some((image, alpha)) = s.previous_shot {
        frame = frame.child(
            El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.)))
                .content(image.content(Fit::Cover))
                .opacity(alpha),
        );
    }
    if let Some(image) = s.shot {
        let fade = if s.previous_shot.is_some() {
            s.shot_fade
        } else {
            1.
        };
        frame = frame.child(
            El::new(absolute(Some(0.), Some(0.), Some(0.), Some(0.)))
                .content(image.content(Fit::Cover))
                .opacity(fade),
        );
    }
    frame
}

fn loading_info(m: &Metrics, s: &Screen) -> El {
    let track_size = clamp(18., m.w * 0.02, 24.);
    let mut info = El::new(column(16.));
    info.style.max_size.width = length(520.);
    if !m.portrait {
        info.style.margin = taffy::Rect {
            top: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
            left: LengthPercentageAuto::length(0.),
            right: LengthPercentageAuto::length(0.),
        };
    }
    let style = TextStyle::new(display(track_size, 700, false), Color::hex("#f2f2f2"))
        .spacing(track_size * 0.03);
    let mut pill = row(8.);
    pill.align_items = AlignItems::BASELINE;
    pill.padding = sides(8., 18., 8., 18.);
    pill.flex_shrink = 0.;
    info = info.child(
        El::new(pill)
            .paint(
                Paint::fill(Color::hex("#0b0b0b"))
                    .radius(999.)
                    .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0008"))),
            )
            .child(text("Track:", style.clone(), track_size * 1.2))
            .child(text(s.loading_track.clone(), style, track_size * 1.2)),
    );
    if !s.tip.is_empty() {
        let label_size = clamp(17., m.w * 0.018, 21.);
        let tip_size = clamp(16., m.w * 0.017, 20.);
        let (pad_v, pad_h) = if m.short { (10., 14.) } else { (16., 20.) };
        let mut tips = El::new(Style {
            padding: sides(pad_v, pad_h, pad_v, pad_h),
            flex_shrink: 0.,
            ..column(0.)
        })
        .paint(
            Paint::fill(Color::hex("#2b2b2bd9"))
                .radius(5.)
                .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff1c")))
                .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0008"))),
        )
        .child(
            El::new(Style::default()).content(Content::Text {
                text: "Tips:".into(),
                style: TextStyle::new(display(label_size, 700, false), Color::hex("#e8c832"))
                    .spacing(label_size * 0.03),
                line_height: label_size * 1.3,
                wrap: false,
            }),
        );
        for line in &s.tip {
            tips = tips.child(
                El::new(Style::default()).content(Content::Text {
                    text: line.clone(),
                    style: TextStyle::new(display(tip_size, 600, false), Color::hex("#f0f0f0"))
                        .spacing(tip_size * 0.02),
                    line_height: tip_size * 1.35,
                    wrap: true,
                }),
            );
        }
        info = info.child(tips);
    }
    info
}

/// `.ms-ld-bar`: the gold progress bar in a black pill.
fn loading_bar(m: &Metrics, s: &Screen) -> El {
    let width = 420f32.min(0.7 * m.w);
    let height = if m.short { 24. } else { 28. };
    let inner = width - 12.;
    let fill = (inner * s.progress.clamp(0., 1.)).max(14.);
    let meter = El::new(Style {
        size: Size {
            width: length(width),
            height: length(height),
        },
        padding: sides(4., 4., 4., 4.),
        border: sides(2., 2., 2., 2.),
        ..Default::default()
    })
    .paint({
        let mut p = Paint::fill(Color::hex("#0b0b0b"))
            .radius(999.)
            .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")));
        p.border = [2.; 4];
        p.border_color = Color::hex("#cfcfcf");
        p
    })
    .child(
        El::new(Style {
            size: Size {
                width: length(fill),
                height: percent(1.),
            },
            ..Default::default()
        })
        .paint(
            Paint::fill(Gradient::new(
                180.,
                &[
                    (0., Color::hex("#e9d36a")),
                    (0.45, Color::hex("#c3a227")),
                    (0.55, Color::hex("#8f7410")),
                    (1., Color::hex("#6d5708")),
                ],
            ))
            .radius(999.),
        ),
    );
    El::new(Style {
        display: Display::Flex,
        justify_content: JustifyContent::CENTER,
        align_items: AlignItems::CENTER,
        padding: sides(8., m.gutter, clamp(14., m.h * 0.04, 36.), m.gutter),
        ..Default::default()
    })
    .child(meter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayList, Fonts, layout};

    fn screen(page: &str) -> Screen {
        Screen {
            page: page.into(),
            choices: vec![
                ("single".into(), "Single Player".into()),
                ("settings".into(), "Settings".into()),
            ],
            vehicle_label: "Ravage Talon Black".into(),
            ..Default::default()
        }
    }

    #[test]
    fn main_menu_rows_follow_the_browser_grid() {
        let view = Viewport {
            width: 1280.,
            height: 720.,
            scale: 1.,
            ..Default::default()
        };
        let mut fonts = Fonts::new();
        let mut list = DisplayList::default();
        let mut hits = Vec::new();
        layout::render(
            &element(&view, &screen("main")),
            &mut fonts,
            1280.,
            720.,
            &mut list,
            &mut hits,
        );
        let first = hits.iter().find(|(_, id)| *id == ROW).unwrap().0;
        // Header: 21.6 + 46.2 + 14 + 11.52 = 93.32; rows are 48 tall.
        assert!((first.y - 93.32).abs() < 0.5, "{first:?}");
        assert!((first.x - 40.96).abs() < 0.5, "{first:?}");
        assert!((first.h - 48.).abs() < 0.5, "{first:?}");
        let back = hits.iter().find(|(_, id)| *id == BACK).unwrap().0;
        assert!(back.right() <= 1280. - 40.);
    }

    #[test]
    fn a_sliding_page_moves_and_fades_with_its_buttons() {
        let view = Viewport {
            width: 1280.,
            height: 720.,
            scale: 1.,
            ..Default::default()
        };
        let mut fonts = Fonts::new();
        let render = |fonts: &mut Fonts, s: &Screen| {
            let (mut list, mut hits) = (DisplayList::default(), Vec::new());
            layout::render(&element(&view, s), fonts, 1280., 720., &mut list, &mut hits);
            (list, hits)
        };
        let (rest, rest_hits) = render(&mut fonts, &screen("main"));
        let mut moving = screen("main");
        moving.motion = crate::transition::enter(40.);
        let m = moving.motion.unwrap();
        let (list, hits) = render(&mut fonts, &moving);
        let row = |hits: &[(crate::Rect, u32)]| hits.iter().find(|(_, id)| *id == ROW).unwrap().0;
        // Buttons follow the page (a transformed DOM button is hit there).
        assert!((row(&hits).x - row(&rest_hits).x - m.dx).abs() < 1e-3);
        assert_eq!(row(&hits).y, row(&rest_hits).y);
        // `#frontend` stays behind at full strength; the stage fades.
        let crate::paint::Item::Box(under) = &list.items[0] else {
            panic!("{:?}", list.items[0]);
        };
        assert_eq!(under.rect, crate::Rect::new(0., 0., 1280., 720.));
        assert_eq!(list.items[1], crate::paint::Item::PushOpacity(m.opacity));
        assert_eq!(list.items.last(), Some(&crate::paint::Item::PopOpacity));
        assert_eq!(list.items.len(), rest.items.len() + 3);
    }
}
