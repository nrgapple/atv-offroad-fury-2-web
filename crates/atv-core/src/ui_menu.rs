//! Responsive browser menu reconstruction. Original typography, red/silver
//! panels and yellow selection bars are retained; geometry and hit regions are
//! authored here instead of scaling a 640x448 background image. This is an
//! adaptation, not a recovered native layout/callback implementation.
use crate::Result;
use serde::{Deserialize, Serialize};

const FONT: &str = "tradegothic_boldcondensed_22s";
const SELECTED_FONT: &str = "tradegothic_boldcondensed_22soff";
const HEADER_FONT: &str = "tradegothic_boldcondenseditalic_-1fs_32ds";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRow {
    pub id: String,
    pub label: String,
    pub value: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub page: String,
    #[serde(default)]
    pub loading_status: String,
    #[serde(default = "default_quad")]
    pub quad_id: u16,
    #[serde(default)]
    pub rider_id: u16,
    #[serde(default)]
    pub room_code: String,
    #[serde(default)]
    pub player_name: String,
    #[serde(default)]
    pub lobby_status: String,
    #[serde(default)]
    pub lobby_players: Vec<ControlRow>,
    #[serde(default)]
    pub lobby_host: bool,
    #[serde(default)]
    pub lobby_started: bool,
    pub width: f32,
    pub height: f32,
    pub selection: usize,
    #[serde(default)]
    pub track_name: String,
    #[serde(default)]
    pub track_image: String,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub control_rows: Vec<ControlRow>,
    #[serde(default)]
    pub binding_device: String,
    #[serde(default)]
    pub binding_page: usize,
    #[serde(default)]
    pub capture_label: String,
    #[serde(default)]
    pub music_ready: bool,
    #[serde(default)]
    pub music_rows: Vec<ControlRow>,
    #[serde(default)]
    pub music_page: usize,
    #[serde(default)]
    pub music_status: String,
    #[serde(default)]
    pub music_enabled: bool,
    #[serde(default)]
    pub music_volume: usize,
    #[serde(default)]
    pub menu_logo: bool,
    #[serde(default)]
    pub menu_atv: bool,
    #[serde(default)]
    pub diagnostics_mode: String,
    #[serde(default)]
    pub one_handed_mobile: bool,
    #[serde(default)]
    pub race_laps: u32,
    #[serde(default)]
    pub race_cpu: u32,
    #[serde(default)]
    pub race_difficulty: u32,
    #[serde(default)]
    pub race_available: bool,
    #[serde(default)]
    pub race_active: bool,
    #[serde(default)]
    pub funny_crashes: bool,
    #[serde(default)]
    pub minimap_enabled: bool,
    #[serde(default = "default_minimap_size")]
    pub minimap_size: f32,
    #[serde(default = "default_minimap_shape")]
    pub minimap_shape: String,
    #[serde(default = "default_minimap_corner")]
    pub minimap_corner: String,
    #[serde(default = "default_minimap_orientation")]
    pub minimap_orientation: String,
    #[serde(default = "default_steering_sensitivity")]
    pub mobile_steering_sensitivity: f32,
}
fn default_quad() -> u16 {
    1
}
fn default_minimap_size() -> f32 {
    160.
}
fn default_minimap_shape() -> String {
    "circle".into()
}
fn default_minimap_orientation() -> String {
    "north-up".into()
}
fn default_minimap_corner() -> String {
    "top-right".into()
}
fn default_steering_sensitivity() -> f32 {
    85.
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Draw {
    Rect {
        rect: [f32; 4],
        radius: f32,
        color: &'static str,
        bottom: &'static str,
        stroke: &'static str,
    },
    Path {
        d: String,
        color: &'static str,
        bottom: &'static str,
        stroke: &'static str,
    },
    Text {
        text: String,
        at: [f32; 2],
        scale: f32,
        font: &'static str,
        color: &'static str,
        max_width: f32,
    },
    Artwork {
        image: &'static str,
        rect: [f32; 4],
    },
    ModelPreview {
        model: &'static str,
        rect: [f32; 4],
    },
    Preview {
        image: String,
        rect: [f32; 4],
    },
}
#[derive(Serialize)]
pub struct Hit {
    pub label: String,
    pub action: String,
    pub rect: [f32; 4],
    pub selected: bool,
}
#[derive(Serialize)]
pub struct Slider {
    pub action: &'static str,
    pub label: &'static str,
    pub rect: [f32; 4],
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub selected: bool,
}
#[derive(Serialize)]
pub struct TextInput {
    pub action: &'static str,
    pub label: &'static str,
    pub rect: [f32; 4],
    pub value: String,
    pub max_length: usize,
}
#[derive(Serialize)]
pub struct Frame {
    pub width: f32,
    pub height: f32,
    pub draw: Vec<Draw>,
    pub hits: Vec<Hit>,
    pub choices: Vec<String>,
    pub sliders: Vec<Slider>,
    pub inputs: Vec<TextInput>,
}
impl Frame {
    fn rect(
        &mut self,
        rect: [f32; 4],
        radius: f32,
        color: &'static str,
        bottom: &'static str,
        stroke: &'static str,
    ) {
        self.draw.push(Draw::Rect {
            rect,
            radius,
            color,
            bottom,
            stroke,
        });
    }
    fn text(
        &mut self,
        text: impl Into<String>,
        at: [f32; 2],
        scale: f32,
        font: &'static str,
        color: &'static str,
        max_width: f32,
    ) {
        self.draw.push(Draw::Text {
            text: text.into(),
            at,
            scale,
            font,
            color,
            max_width,
        });
    }
    fn button(&mut self, label: &str, action: &str, rect: [f32; 4], selected: bool) {
        let [x, y, w, h] = rect;
        if selected {
            self.rect(
                [x, y + 7., w, h - 14.],
                (h - 14.) / 2.,
                "#514200",
                "#171400",
                "#0b0905",
            );
            self.rect(
                [x + 2., y + 9., w - 4., h - 18.],
                (h - 18.) / 2.,
                "#fffaa1",
                "#a99900",
                "#d0bc39",
            );
        } else {
            self.rect(
                [x + 8., y + h / 2. - 4., 8., 8.],
                4.,
                "#e5d360",
                "#797024",
                "#252008",
            );
        }
        self.text(
            label,
            [x + 30., y + (h - 24.) / 2.],
            1.,
            if selected { SELECTED_FONT } else { FONT },
            if selected { "0,0,0" } else { "215,215,215" },
            w - 46.,
        );
        self.hits.push(Hit {
            label: label.into(),
            action: action.into(),
            rect,
            selected,
        });
    }
    fn footer(&mut self, back: bool) {
        let y = self.height - 68.;
        if back {
            self.button("Back", "back", [24., y, 132., 56.], false);
        }
    }
}

pub fn layout(request: Request) -> Result<Frame> {
    if !request.width.is_finite()
        || !request.height.is_finite()
        || !(240.0..=16384.0).contains(&request.width)
        || !(240.0..=16384.0).contains(&request.height)
        || !request.mobile_steering_sensitivity.is_finite()
        || !(50.0..=150.0).contains(&request.mobile_steering_sensitivity)
        || !matches!(request.minimap_shape.as_str(), "circle" | "square")
        || !matches!(request.minimap_corner.as_str(), "top-right" | "bottom-left")
        || !matches!(
            request.minimap_orientation.as_str(),
            "north-up" | "heading-up"
        )
        || !request.minimap_size.is_finite()
        || !(96.0..=224.0).contains(&request.minimap_size)
        || request.room_code.len() > 64
        || request.player_name.len() > 32
        || request.lobby_status.len() > 256
        || request.lobby_players.len() > 4
        || request.lobby_players.iter().any(|p| {
            p.id.parse::<u8>().map_or(true, |id| id >= 4)
                || p.label.len() > 64
                || !["self", "ready", "waiting"].contains(&p.value.as_str())
        })
        || !(1..=crate::motorcycle::SELECTION_COUNT).contains(&request.quad_id)
        || request.rider_id > 1
        || request.loading_status.len() > 128
        || request.track_name.len() > 128
        || request.track_image.len() > 128
        || request.capture_label.len() > 128
        || request.binding_device.len() > 32
        || request.music_status.len() > 256
        || request.control_rows.len() > 32
        || request.music_rows.len() > 64
        || request
            .control_rows
            .iter()
            .chain(&request.music_rows)
            .any(|row| {
                row.id.len() > 40
                    || !row.id.bytes().all(|c| c.is_ascii_alphanumeric())
                    || row.label.len() > 128
                    || row.value.len() > 256
            })
    {
        return Err("invalid menu viewport or label".into());
    }
    let scale = (request.width / 390.).min(request.height / 448.);
    let w = request.width / scale;
    let h = request.height / scale;
    let portrait = h > w;
    let pause = matches!(
        request.page.as_str(),
        "pause" | "players" | "hud-settings" | "minimap-config" | "controls" | "bindings" | "music"
    );
    let mut f = Frame {
        width: w,
        height: h,
        draw: vec![],
        hits: vec![],
        choices: vec![],
        sliders: vec![],
        inputs: vec![],
    };
    if pause {
        f.rect([0., 0., w, h], 0., "#00000070", "#00000070", "");
    } else {
        f.rect([0., 0., w, h], 0., "#b9b9b9", "#e9e9e9", "");
    }
    let edge = if portrait {
        w - 20.
    } else {
        (w * 0.62).min(w - 120.)
    };
    if pause {
        // The original pause overlay is a translucent charcoal panel with a
        // gray title strip, distinct from the red frontend pages.
        f.rect(
            [12., 16., w - 24., h - 32.],
            8.,
            "#222222b8",
            "#080808df",
            "#050505",
        );
        f.rect([13., 24., w - 26., 48.], 0., "#88888868", "#45454540", "");
    } else {
        f.draw.push(Draw::Path {
            d: format!(
                "M 0 0 H {} Q {} 0 {} 48 L {} {} Q {} {} {} {} H 0 Z",
                edge - 90.,
                edge - 20.,
                edge - 14.,
                edge + 12.,
                h - 60.,
                edge + 28.,
                h,
                edge - 20.,
                h
            ),
            color: "#9c1414",
            bottom: "#210504",
            stroke: "#170504",
        });
        f.rect([0., 24., edge - 8., 48.], 0., "#c8554b80", "#9c262c50", "");
    }
    let title = match request.page.as_str() {
        "title" => "Fury Remastered",
        "main" => "Main Menu",
        "single" => "Single Player",
        "multiplayer" => "Multiplayer",
        "lobby" => "Free Ride Lobby",
        "garage" => "Quad and Rider",
        "loading" => "Loading",
        "tracks" => "Freestyle",
        "race" => "Single Race",
        "pause" => "Pause",
        "players" => "Lobby Players",
        "hud-settings" => "HUD Settings",
        "minimap-config" => "Minimap Configuration",
        "controls" => "Controls",
        "bindings" => {
            if request.binding_device == "controller" {
                "Controller"
            } else {
                "Keyboard"
            }
        }
        "music" => "Music",
        _ => return Err("unknown menu page".into()),
    };
    if request.page != "title" {
        f.text(title, [24., 29.], 1., HEADER_FONT, "235,235,235", w - 56.);
    }
    let list_w = if portrait {
        w - 60.
    } else {
        (w * 0.48).min(380.)
    };
    match request.page.as_str() {
        "title" => {
            f.choices.extend(["main", "devlog"].map(String::from));
            let y = (h * 0.30).max(95.);
            if request.menu_logo {
                let width = (w - 60.).min(540.);
                f.draw.push(Draw::Artwork {
                    image: "brand-logo",
                    rect: [30., (y + 15.).min(h - 157. - width / 3.), width, width / 3.],
                });
            } else {
                f.text("FURY", [34., y], 3.2, HEADER_FONT, "232,232,232", w - 68.);
                f.text(
                    "REMASTERED",
                    [36., y + 115.],
                    1.25,
                    HEADER_FONT,
                    "247,206,72",
                    w - 72.,
                );
            }
            f.button(
                "Press START",
                "main",
                [24., h - 135., list_w.max(290.), 60.],
                request.selection.is_multiple_of(2),
            );
            f.button(
                "Dev log",
                "devlog",
                [24., h - 74., list_w.max(290.), 56.],
                request.selection % 2 == 1,
            );
        }
        "main" | "single" => {
            let (label, action, detail) = if request.page == "main" {
                ("Single Player", "single", "Ride the original environments.")
            } else {
                (
                    "Freestyle",
                    "tracks",
                    "Choose an environment and free ride.",
                )
            };
            f.choices.push(action.to_string());
            let main = request.page == "main";
            f.button(
                label,
                action,
                [20., 102., list_w, 58.],
                request.selection == 0,
            );
            let items: &[(&str, &str)] = if main {
                &[("Multiplayer", "multiplayer"), ("Dev log", "devlog")]
            } else {
                &[("Quad and Rider", "garage")]
            };
            for (i, (label, action)) in items.iter().enumerate() {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [20., 160. + i as f32 * 58., list_w, 58.],
                    request.selection == i + 1,
                );
            }
            if !main && request.race_available {
                f.choices.push("race".into());
                f.button(
                    "Race",
                    "race",
                    [20., 218., list_w, 58.],
                    request.selection == 2,
                );
            }
            let detail_y = if main || request.race_available {
                292.
            } else {
                236.
            };
            f.rect(
                [24., detail_y, list_w - 4., 72.],
                6.,
                "#292020",
                "#1c1717",
                "#100b09",
            );
            let detail = if !main && request.selection == 2 {
                "Race CPU riders on Caribou Peak."
            } else if main && request.selection == 2 {
                "Progress, screenshots and what is next."
            } else {
                detail
            };
            f.text(
                detail,
                [38., detail_y + 22.],
                0.85,
                FONT,
                "205,205,205",
                list_w - 32.,
            );
            if request.menu_atv {
                let width = if portrait {
                    (w - 60.).min((h - detail_y - 186.).max(0.) * 2.)
                } else {
                    (w * 0.43).min(380.)
                };
                if width >= 100. {
                    f.draw.push(Draw::Artwork {
                        image: "multi_atv01",
                        rect: [
                            if portrait {
                                (w - width) / 2.
                            } else {
                                w - width - 22.
                            },
                            if portrait { detail_y + 92. } else { 170. },
                            width,
                            width / 2.,
                        ],
                    });
                }
            }
            f.footer(true);
        }
        "race" => {
            f.text(
                "Caribou Peak - National",
                [30., 96.],
                1.,
                FONT,
                "255,255,255",
                w - 60.,
            );
            f.text(
                "Race preview",
                [30., 132.],
                0.8,
                FONT,
                "255,226,107",
                w - 60.,
            );
            let rows = [
                (format!("Laps: {}", request.race_laps.max(1)), "racelaps"),
                (format!("CPU riders: {}", request.race_cpu), "racecpu"),
                ("Start Race".to_string(), "racestart"),
            ];
            for (i, (label, action)) in rows.iter().enumerate() {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [24., 154. + i as f32 * 56., list_w.max(290.), 54.],
                    request.selection % rows.len() == i,
                );
            }
            if !portrait {
                f.draw.push(Draw::Preview {
                    image: request.track_image.clone(),
                    rect: [w * 0.55, 100., w * 0.4, (h - 210.).min(w * 0.4 * 0.64)],
                });
            }
            f.footer(true);
        }
        "garage" => {
            let model = crate::player_models::selection(request.quad_id, request.rider_id)?;
            // Portrait stacks two equal model cards above the controls; landscape
            // gives the previews the right-hand stage. Bounds are in Rust UI units.
            let panel = if portrait {
                [24., 98., w - 48., (h - 380.).clamp(100., 420.)]
            } else {
                [list_w + 44., 92., w - list_w - 68., h - 124.]
            };
            let card_w = (panel[2] - 12.) / 2.;
            for (i, (kind, label)) in [("quad", model.label.as_str()), ("rider", model.rider_label)]
                .iter()
                .enumerate()
            {
                let x = panel[0] + i as f32 * (card_w + 12.);
                f.rect(
                    [x, panel[1], card_w, panel[3]],
                    6.,
                    "#292929",
                    "#080808",
                    "#b9b9b98c",
                );
                f.text(
                    *label,
                    [x + 10., panel[1] + 12.],
                    0.85,
                    FONT,
                    "255,255,255",
                    card_w - 20.,
                );
                f.draw.push(Draw::ModelPreview {
                    model: kind,
                    rect: [x + 6., panel[1] + 46., card_w - 12., panel[3] - 74.],
                });
                f.text(
                    "Drag to rotate",
                    [x + 10., panel[1] + panel[3] - 22.],
                    0.65,
                    FONT,
                    "185,185,185",
                    card_w - 20.,
                );
            }
            let y = if portrait {
                panel[1] + panel[3] + 12.
            } else {
                114.
            };
            let controls_w = if portrait { w - 48. } else { list_w };
            let row_h = 54_f32.max(44. / scale);
            f.text(
                format!(
                    "Vehicle {} / {}",
                    request.quad_id,
                    crate::motorcycle::SELECTION_COUNT
                ),
                [30., y],
                0.85,
                FONT,
                "229,211,96",
                controls_w - 12.,
            );
            for (i, (label, action)) in [
                ("Previous quad", "quadprev"),
                ("Next quad", "quadnext"),
                ("Change rider", "riderchange"),
                ("Done", "back"),
            ]
            .iter()
            .enumerate()
            {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [24., y + 30. + i as f32 * row_h, controls_w, row_h],
                    request.selection == i,
                );
            }
        }
        "loading" => {
            let photo_w = if portrait {
                w - 48.
            } else {
                (w * 0.62).min((h - 205.) * 2.)
            };
            let photo_h = photo_w / 2.;
            let x = (w - photo_w) / 2.;
            let y = if portrait {
                ((h - photo_h) * 0.35).max(96.)
            } else {
                88.
            };
            f.rect(
                [x - 3., y - 3., photo_w + 6., photo_h + 6.],
                4.,
                "#b9b9b9",
                "#343434",
                "#e9e9e9",
            );
            if !request.track_image.is_empty() {
                f.draw.push(Draw::Preview {
                    image: request.track_image.clone(),
                    rect: [x, y, photo_w, photo_h],
                });
            }
            f.text(
                &request.track_name,
                [x, y + photo_h + 18.],
                1.15,
                HEADER_FONT,
                "229,211,96",
                photo_w,
            );
            f.text(
                &request.loading_status,
                [x, y + photo_h + 60.],
                0.8,
                FONT,
                "222,222,222",
                photo_w,
            );
        }
        "multiplayer" => {
            for (action, label, value, y, max_length) in [
                (
                    "player-name",
                    "Rider name",
                    request.player_name.clone(),
                    104.,
                    24,
                ),
                (
                    "room-code",
                    "Lobby code",
                    request.room_code.clone(),
                    166.,
                    8,
                ),
            ] {
                f.text(label, [30., y - 23.], 0.8, FONT, "215,215,215", list_w);
                f.inputs.push(TextInput {
                    action,
                    label,
                    value,
                    rect: [30., y, list_w - 12., 40.],
                    max_length,
                });
            }
            let rows = [
                ("Create lobby", "host"),
                ("Join code", "join"),
                ("Quad and Rider", "garage"),
            ];
            for (i, (label, action)) in rows.iter().enumerate() {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [24., 226. + i as f32 * 54., list_w, 54.],
                    request.selection == i,
                );
            }
            f.footer(true);
        }
        "lobby" => {
            f.text(
                format!("Code: {}", request.room_code),
                [30., 100.],
                1.15,
                FONT,
                "255,230,100",
                list_w,
            );
            f.text(
                &request.track_name,
                [30., 140.],
                1.,
                FONT,
                "235,235,235",
                list_w,
            );
            f.text(
                &request.lobby_status,
                [30., 179.],
                0.8,
                FONT,
                "210,210,210",
                list_w,
            );
            let mut rows = vec![("Copy invite link", "share")];
            if request.lobby_started {
                rows.push(("Resume ride", "resume"));
            } else if request.lobby_host {
                rows.extend([
                    ("Next environment", "lobbymap"),
                    ("Ride together", "lobbystart"),
                ]);
            }
            rows.push(("Leave lobby", "leave"));
            for (i, (label, action)) in rows.iter().enumerate() {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [24., 214. + i as f32 * 54., list_w, 54.],
                    request.selection == i,
                );
            }
        }
        "tracks" => {
            let preview = if portrait {
                [24., 112., w - 48., (w - 48.) * 0.64]
            } else {
                [
                    w * 0.48,
                    98.,
                    w * 0.48 - 24.,
                    (h - 180.).min((w * 0.48 - 24.) * 0.64),
                ]
            };
            f.rect(
                [
                    preview[0] - 2.,
                    preview[1] - 2.,
                    preview[2] + 4.,
                    preview[3] + 4.,
                ],
                0.,
                "#1b1111",
                "#1b1111",
                "#c2b6b0",
            );
            f.draw.push(Draw::Preview {
                image: request.track_image.clone(),
                rect: preview,
            });
            let y = if portrait {
                preview[1] + preview[3] + 24.
            } else {
                118.
            };
            let text_width = if portrait { w - 60. } else { w * 0.44 - 30. };
            f.text(
                &request.track_name,
                [30., y],
                1.2,
                FONT,
                "255,255,255",
                text_width,
            );
            f.text(
                "Free Ride",
                [30., y + 40.],
                0.85,
                FONT,
                "200,200,200",
                text_width,
            );
            let row_y = if portrait { y + 92. } else { 235. };
            f.button("Previous", "left", [24., row_y, 175., 54.], false);
            f.hits.last_mut().unwrap().label = "Previous environment".into();
            f.button(
                "Next",
                "right",
                [
                    if portrait { w - 184. } else { 24. },
                    if portrait { row_y } else { row_y + 58. },
                    160.,
                    54.,
                ],
                false,
            );
            f.hits.last_mut().unwrap().label = "Next environment".into();
            f.button("Ride", "accept", [w - 186., h - 82., 162., 62.], true);
            f.hits.last_mut().unwrap().label = format!("Ride {}", request.track_name);
            f.footer(true);
        }
        "players" => {
            let row = ((h - 170.) / 5.).clamp(44., 64.);
            for (i, player) in request.lobby_players.iter().enumerate() {
                let y = 96. + i as f32 * row;
                if player.value == "self" || player.value == "waiting" {
                    let label = if player.value == "waiting" {
                        format!("{} / waiting", player.label)
                    } else {
                        player.label.clone()
                    };
                    f.text(label, [30., y + 20.], 0.9, FONT, "210,210,210", w - 60.);
                } else {
                    let action = format!("spawn:{}", player.id);
                    f.choices.push(action.clone());
                    f.button(
                        &format!("Spawn beside {}", player.label),
                        &action,
                        [24., y, w - 48., row],
                        request.selection == f.choices.len() - 1,
                    );
                }
            }
            f.choices.push("pause".into());
            f.button(
                "Back",
                "pause",
                [24., h - 70., w - 48., 48.],
                request.selection == f.choices.len() - 1,
            );
        }
        "pause" => {
            let diagnostics_label = match request.diagnostics_mode.as_str() {
                "performance" => "Stats: Performance (F3)",
                "debug" => "Stats: Debug (F3)",
                _ => "Stats: Off (F3)",
            };
            let mobile_label = if request.one_handed_mobile {
                "One-handed mobile: On"
            } else {
                "One-handed mobile: Off"
            };
            let mut choices = vec![
                ("Resume", "resume"),
                (
                    if request.race_active {
                        "Restart Race"
                    } else {
                        "Reset ATV"
                    },
                    "reset",
                ),
                ("Controls", "controls"),
                ("HUD settings", "hud-settings"),
                (diagnostics_label, "diagnostics"),
                (mobile_label, "onehanded"),
                (
                    if request.funny_crashes {
                        "Funny crashes: On"
                    } else {
                        "Funny crashes: Off"
                    },
                    "funnycrashes",
                ),
            ];
            if !request.lobby_status.is_empty() {
                choices.insert(2, ("Players", "players"));
            }
            if request.music_ready {
                choices.push(("Music", "music"));
            }
            if request.fullscreen {
                choices.push(("Fullscreen", "fullscreen"));
            }
            choices.push(("Dev log", "devlog"));
            choices.push(if request.lobby_status.is_empty() {
                ("Quit to Main Menu", "main")
            } else {
                ("Lobby", "lobby")
            });
            let columns =
                if (h - if portrait { 110. } else { 90. }) / choices.len() as f32 * scale < 44. {
                    2
                } else {
                    1
                };
            let rows = choices.len().div_ceil(columns);
            let row = ((h - if portrait { 110. } else { 90. }) / rows as f32).min(if portrait {
                64.
            } else {
                48_f32.max(44. / scale)
            });
            let column_width = (w - if portrait { 60. } else { 108. }) / columns as f32;
            let selected = request.selection % choices.len();
            for (i, (label, action)) in choices.iter().enumerate() {
                f.choices.push(action.to_string());
                f.button(
                    label,
                    action,
                    [
                        24. + (i % columns) as f32 * column_width,
                        (if portrait { 98. } else { 78. }) + (i / columns) as f32 * row,
                        column_width,
                        row,
                    ],
                    i == selected,
                );
            }
        }
        "hud-settings" => {
            f.button(
                "Minimap configuration",
                "minimap-config",
                [24., 98., w - 48., 56.],
                request.selection.is_multiple_of(2),
            );
            f.choices.push("minimap-config".into());
            f.button(
                "Back",
                "back",
                [24., h - 68., 132., 56.],
                request.selection % 2 == 1,
            );
            f.choices.push("back".into());
        }
        "minimap-config" => {
            let selected = request.selection % 6;
            let columns = if h < 520. { 2 } else { 1 };
            let column_width = (w - 48.) / columns as f32;
            let row = 52_f32.max(44. / scale);
            for (i, (label, action)) in [
                (
                    if request.minimap_enabled {
                        "Freestyle minimap: On"
                    } else {
                        "Freestyle minimap: Off"
                    },
                    "minimap-toggle",
                ),
                (
                    if request.minimap_shape == "circle" {
                        "Shape: Circular"
                    } else {
                        "Shape: Square"
                    },
                    "minimap-shape",
                ),
                (
                    if request.minimap_corner == "top-right" {
                        "Position: Top right"
                    } else {
                        "Position: Bottom left"
                    },
                    "minimap-corner",
                ),
                (
                    if request.minimap_orientation == "north-up" {
                        "Orientation: North up"
                    } else {
                        "Orientation: Heading up"
                    },
                    "minimap-orientation",
                ),
            ]
            .iter()
            .enumerate()
            {
                f.button(
                    label,
                    action,
                    [
                        24. + (i % columns) as f32 * column_width,
                        98. + (i / columns) as f32 * (row + 4.),
                        column_width,
                        row,
                    ],
                    selected == i,
                );
                f.choices.push((*action).into());
            }
            let y = 98. + 4_usize.div_ceil(columns) as f32 * (row + 4.) + 10.;
            f.text(
                format!("Minimap size: {} px", request.minimap_size as u32),
                [30., y],
                0.85,
                FONT,
                "255,226,107",
                w - 60.,
            );
            f.sliders.push(Slider {
                action: "minimap-size",
                label: "Minimap size",
                rect: [30., y + 26., w - 60., row],
                value: request.minimap_size,
                min: 96.,
                max: 224.,
                step: 8.,
                selected: selected == 4,
            });
            f.choices.push("minimap-size".into());
            f.button("Back", "back", [24., h - 68., 132., 56.], selected == 5);
            f.choices.push("back".into());
        }
        "controls" => {
            let compact = portrait && h < 540.;
            let touch_lines = if compact {
                if request.one_handed_mobile {
                    vec![
                        "Hold: drive / brace. Drag: steer / lean.",
                        "2nd: reverse; air swipe / circle tricks.",
                    ]
                } else {
                    vec![
                        "Left: steer / lean. Right: drive / brake.",
                        "Hold: brace. Air: swipe / circle tricks.",
                    ]
                }
            } else if request.one_handed_mobile {
                vec![
                    "One thumb anywhere: hold to drive and brace.",
                    "Drag left / right to steer; up / down to lean.",
                    "Lift: no throttle; releases preload, no brake.",
                    "2nd finger: ground reverse; air swipe / circle tricks.",
                ]
            } else {
                vec![
                    "Left: steer and lean, on ground or in air.",
                    "Right: press to drive; drag back to brake / reverse.",
                    "Hold right to brace. Lift to release preload.",
                    "In air: right thumb swipes / circles tricks.",
                ]
            };
            let groups = [("TOUCH", touch_lines)];
            let mut y = 96.;
            for (index, (heading, lines)) in groups.into_iter().enumerate() {
                let x = if portrait {
                    30.
                } else {
                    30. + index as f32 * (w - 60.) / 3.
                };
                let width = if portrait { w - 60. } else { w * 0.55 - 60. };
                if !portrait {
                    y = 102.;
                }
                f.text(heading, [x, y], 1., FONT, "255,226,107", width);
                y += 34.;
                for line in lines {
                    f.text(
                        line,
                        [x, y],
                        if portrait { 0.80 } else { 0.72 },
                        FONT,
                        "222,222,222",
                        width,
                    );
                    y += if portrait { 22. } else { 26. };
                }
                y += 18.;
            }
            let slider_x = if portrait { 30. } else { w * 0.57 };
            let slider_y = if portrait { y - 4. } else { 110. };
            let slider_width = if portrait { w - 60. } else { w * 0.43 - 30. };
            f.text(
                format!(
                    "Mobile steering: {}%",
                    request.mobile_steering_sensitivity as u32
                ),
                [slider_x, slider_y],
                0.85,
                FONT,
                "255,226,107",
                slider_width,
            );
            f.sliders.push(Slider {
                action: "steering-sensitivity",
                label: "Mobile steering sensitivity",
                rect: [slider_x, slider_y + 26., slider_width, 44.],
                value: request.mobile_steering_sensitivity,
                min: 50.,
                max: 150.,
                step: 5.,
                selected: request.selection == 0,
            });
            f.choices.push("steering-sensitivity".into());
            for (i, (label, action)) in [
                ("Keyboard bindings", "keyboard"),
                ("Controller bindings", "controller"),
            ]
            .iter()
            .enumerate()
            {
                f.button(
                    label,
                    action,
                    [24., h - 182. + i as f32 * 54., w - 48., 54.],
                    request.selection == i + 1,
                );
                f.choices.push((*action).into());
            }
            f.button(
                "Back",
                "back",
                [24., h - 68., 132., 56.],
                request.selection == 3,
            );
            f.choices.push("back".into());
        }
        "bindings" | "music" => {
            let music = request.page == "music";
            if !request.capture_label.is_empty() {
                f.text(
                    &request.capture_label,
                    [30., 112.],
                    1.1,
                    FONT,
                    "255,226,107",
                    w - 60.,
                );
                for (i, line) in [
                    "Press a key, button or move an axis.",
                    "Hold several together for a combination.",
                    "Release to save. Escape cancels.",
                ]
                .iter()
                .enumerate()
                {
                    f.text(
                        *line,
                        [30., 172. + i as f32 * 34.],
                        0.85,
                        FONT,
                        "222,222,222",
                        w - 60.,
                    );
                }
                f.button("Cancel", "bindcancel", [24., h - 74., w - 48., 56.], true);
                f.choices.push("bindcancel".into());
            } else {
                let rows = if music {
                    &request.music_rows
                } else {
                    &request.control_rows
                };
                let pages = rows.len().div_ceil(4).max(1);
                let page = (if music {
                    request.music_page
                } else {
                    request.binding_page
                })
                .min(pages - 1);
                let note = if music {
                    request.music_status.clone()
                } else {
                    if request.binding_device == "controller" {
                        "Native preload: lean back, then forward at ramp lip.".into()
                    } else {
                        "Select an action to change its binding.".into()
                    }
                };
                f.text(note, [30., 80.], 0.75, FONT, "222,222,222", w - 60.);
                for (i, row) in rows.iter().skip(page * 4).take(4).enumerate() {
                    let y = 110. + i as f32 * 54.;
                    let action = format!("{}:{}", if music { "song" } else { "bind" }, row.id);
                    let selected = request.selection == i;
                    f.button("", &action, [24., y, w - 48., 54.], selected);
                    f.hits.last_mut().unwrap().label = format!("{}: {}", row.label, row.value);
                    let color = if selected { "0,0,0" } else { "222,222,222" };
                    f.text(
                        &row.label,
                        [50., y + 11.],
                        0.72,
                        if selected { SELECTED_FONT } else { FONT },
                        color,
                        w - 90.,
                    );
                    f.text(
                        &row.value,
                        [50., y + 29.],
                        0.58,
                        if selected { SELECTED_FONT } else { FONT },
                        color,
                        w - 90.,
                    );
                    f.choices.push(action);
                }
                let prefix = if music { "music" } else { "bind" };
                for (i, label) in ["Previous", "Next"].iter().enumerate() {
                    let action = format!("{prefix}{}", if i == 0 { "prev" } else { "next" });
                    f.button(
                        label,
                        &action,
                        [
                            24. + i as f32 * (w - 48.) / 2.,
                            h - 122.,
                            (w - 48.) / 2.,
                            54.,
                        ],
                        request.selection == f.choices.len(),
                    );
                    f.choices.push(action);
                }
                f.text(
                    format!("{} / {}", page + 1, pages),
                    [w - 90., 30.],
                    0.8,
                    FONT,
                    "230,230,230",
                    66.,
                );
                let actions = if music {
                    vec![
                        ("Back", "back"),
                        (
                            if request.music_enabled {
                                "Music off"
                            } else {
                                "Music on"
                            },
                            "musictoggle",
                        ),
                        ("Volume", "musicvolume"),
                    ]
                } else {
                    vec![
                        ("Back", "back"),
                        ("Clear", "bindclear"),
                        ("Defaults", "bindreset"),
                    ]
                };
                for (i, (label, action)) in actions.iter().enumerate() {
                    f.button(
                        label,
                        action,
                        [
                            20. + i as f32 * (w - 40.) / 3.,
                            h - 66.,
                            (w - 40.) / 3.,
                            54.,
                        ],
                        request.selection == f.choices.len(),
                    );
                    f.choices.push((*action).into());
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(page: &str, w: f32, h: f32) -> Request {
        Request {
            page: page.into(),
            loading_status: "Reading original ATV...".into(),
            quad_id: 1,
            rider_id: 0,
            room_code: String::new(),
            player_name: String::new(),
            lobby_status: String::new(),
            lobby_players: vec![],
            lobby_host: false,
            lobby_started: false,
            width: w,
            height: h,
            selection: 0,
            track_name: "Arlington Stream".into(),
            track_image: "arlington".into(),
            fullscreen: true,
            control_rows: vec![],
            binding_device: String::new(),
            binding_page: 0,
            capture_label: String::new(),
            music_ready: true,
            music_rows: vec![],
            music_page: 0,
            music_status: String::new(),
            music_enabled: true,
            music_volume: 70,
            menu_logo: true,
            menu_atv: true,
            diagnostics_mode: String::new(),
            one_handed_mobile: false,
            funny_crashes: false,
            minimap_enabled: true,
            minimap_size: 160.,
            minimap_shape: "circle".into(),
            minimap_corner: "top-right".into(),
            minimap_orientation: "north-up".into(),
            mobile_steering_sensitivity: 85.,
            race_laps: 3,
            race_cpu: 3,
            race_difficulty: 1,
            race_available: false,
            race_active: false,
        }
    }
    #[test]
    fn supported_routes_have_no_unfinished_choices() {
        assert_eq!(
            layout(request("main", 1280., 720.)).unwrap().choices,
            ["single", "multiplayer", "devlog"]
        );
        assert_eq!(
            layout(request("single", 1280., 720.)).unwrap().choices,
            ["tracks", "garage"]
        );
        let mut r = request("pause", 390., 844.);
        r.fullscreen = false;
        assert!(
            !layout(r)
                .unwrap()
                .choices
                .iter()
                .any(|choice| choice == "fullscreen")
        );
    }
    #[test]
    fn race_availability_setup_and_restart_match_the_live_mode() {
        let mut r = request("single", 390., 844.);
        r.race_available = true;
        assert_eq!(layout(r).unwrap().choices, ["tracks", "garage", "race"]);
        let f = layout(request("race", 844., 390.)).unwrap();
        assert_eq!(f.choices, ["racelaps", "racecpu", "racestart"]);
        for (i, a) in f.hits.iter().enumerate() {
            for b in &f.hits[i + 1..] {
                let [ax, ay, aw, ah] = a.rect;
                let [bx, by, bw, bh] = b.rect;
                assert!(
                    ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay,
                    "{} overlaps {}",
                    a.label,
                    b.label
                );
            }
        }
        let mut r = request("pause", 390., 844.);
        r.race_active = true;
        assert!(
            layout(r)
                .unwrap()
                .hits
                .iter()
                .any(|h| h.label == "Restart Race")
        );
    }
    #[test]
    fn touch_regions_fit_viewport_at_all_aspects() {
        for (w, h) in [
            (390., 844.),
            (844., 390.),
            (667., 375.),
            (1280., 720.),
            (320., 568.),
            (768., 1024.),
        ] {
            for page in [
                "title",
                "main",
                "single",
                "race",
                "tracks",
                "garage",
                "loading",
                "pause",
                "controls",
                "hud-settings",
                "minimap-config",
                "bindings",
                "music",
            ] {
                let f = layout(request(page, w, h)).unwrap();
                let scale = w / f.width;
                for hit in &f.hits {
                    let [x, y, width, height] = hit.rect;
                    assert!(
                        x >= 0. && y >= 0. && x + width <= f.width && y + height <= f.height,
                        "{page}: {}",
                        hit.label
                    );
                    assert!(height * scale >= 34., "landscape compact minimum");
                    if page == "pause" {
                        assert!(height * scale >= 44. - 0.001, "pause touch minimum");
                    }
                    if h > w {
                        assert!(height * scale >= 44., "portrait minimum");
                    }
                }
            }
        }
    }
    #[test]
    fn steering_slider_is_bounded_and_clear_of_bindings() {
        for (w, h) in [
            (390., 844.),
            (320., 568.),
            (390., 448.),
            (844., 390.),
            (1280., 720.),
        ] {
            let f = layout(request("controls", w, h)).unwrap();
            assert_eq!(f.sliders.len(), 1);
            let slider = &f.sliders[0];
            assert_eq!(
                (slider.min, slider.max, slider.step, slider.value),
                (50., 150., 5., 85.)
            );
            let [x, y, width, height] = slider.rect;
            assert!(x >= 0. && y >= 0. && x + width <= f.width && y + height <= f.height);
            assert!(height * w / f.width >= 34.);
            assert!(
                f.hits.iter().all(|hit| y + height <= hit.rect[1]),
                "slider must not cover binding buttons"
            );
        }
        let mut invalid = request("controls", 390., 844.);
        invalid.mobile_steering_sensitivity = f32::NAN;
        assert!(layout(invalid).is_err());
    }
    #[test]
    fn minimap_settings_fit_and_validate_sizes() {
        assert_eq!(
            layout(request("hud-settings", 390., 844.)).unwrap().choices,
            ["minimap-config", "back"]
        );
        for (w, h) in [
            (320., 568.),
            (390., 844.),
            (844., 390.),
            (667., 375.),
            (1280., 720.),
        ] {
            let frame = layout(request("minimap-config", w, h)).unwrap();
            assert_eq!(
                frame.choices,
                [
                    "minimap-toggle",
                    "minimap-shape",
                    "minimap-corner",
                    "minimap-orientation",
                    "minimap-size",
                    "back"
                ]
            );
            let slider = &frame.sliders[0];
            let [x, y, width, height] = slider.rect;
            assert!(x >= 0. && y >= 0. && x + width <= frame.width && y + height <= frame.height);
            assert!(height * w / frame.width >= 44. - 0.001);
            assert!(
                frame
                    .hits
                    .iter()
                    .all(|hit| y >= hit.rect[1] + hit.rect[3] || y + height <= hit.rect[1])
            );
            assert_eq!(
                (slider.min, slider.max, slider.step, slider.value),
                (96., 224., 8., 160.)
            );
        }
        for size in [f32::NAN, 0., 225.] {
            let mut r = request("hud-settings", 390., 844.);
            r.minimap_size = size;
            assert!(layout(r).is_err());
        }
        let mut shape = request("minimap-config", 390., 844.);
        shape.minimap_shape = "triangle".into();
        assert!(layout(shape).is_err());
        let mut corner = request("minimap-config", 390., 844.);
        corner.minimap_corner = "center".into();
        assert!(layout(corner).is_err());
    }
    #[test]
    fn invalid_menu_requests_are_rejected() {
        assert!(layout(request("unknown", 390., 844.)).is_err());
        assert!(layout(request("main", f32::NAN, 844.)).is_err());
    }
}
