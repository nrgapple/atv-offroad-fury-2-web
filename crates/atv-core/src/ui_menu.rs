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
    /// Shared Arlington Free Ride room: no invites and no track changes.
    #[serde(default)]
    pub lobby_public: bool,
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
    /// Rider gear page values (helmet, jersey, pants, gloves, boots, goggles,
    /// quad colour), named from the original Profile Store catalog.
    #[serde(default)]
    pub gear_rows: Vec<String>,
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
    /// Selected original race course id (env/nat/sx); its name and preview
    /// arrive through track_name / track_image.
    #[serde(default)]
    pub race_course: String,
    #[serde(default)]
    pub race_active: bool,
    #[serde(default)]
    pub funny_crashes: bool,
    #[serde(default)]
    pub crash_mode: String,
    #[serde(default)]
    pub original_camera: bool,
    /// Authored chase-camera adaptation; off presents the recovered camera.
    #[serde(default)]
    pub chase_assist: bool,
    /// Authored MX vs. ATV Unleashed backflip/three-sixty; off is original.
    #[serde(default)]
    pub flips: bool,
    #[serde(default)]
    pub speed_rush: String,
    #[serde(default)]
    pub minimap_enabled: bool,
    #[serde(default = "default_minimap_size")]
    pub minimap_size: f32,
    #[serde(default = "default_minimap_zoom")]
    pub minimap_zoom: f32,
    #[serde(default = "default_minimap_shape")]
    pub minimap_shape: String,
    #[serde(default = "default_minimap_corner")]
    pub minimap_corner: String,
    #[serde(default = "default_minimap_orientation")]
    pub minimap_orientation: String,
    #[serde(default = "default_steering_sensitivity")]
    pub mobile_steering_sensitivity: f32,
    /// Original Pause → Overlays groups switched off (timing, lap, score,
    /// info, speed, radar) and the Position row (name, place or off).
    #[serde(default)]
    pub race_overlays_off: Vec<String>,
    #[serde(default = "default_race_position")]
    pub race_position: String,
}
fn default_race_position() -> String {
    "name".into()
}
fn default_quad() -> u16 {
    1
}
fn default_minimap_size() -> f32 {
    160.
}
fn default_minimap_zoom() -> f32 {
    50.
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
/// One garage selection. The browser lists these together so vehicle and
/// rider choices share a single page; ids are the gameplay selection ids.
#[derive(Serialize)]
pub struct CatalogItem {
    pub id: u16,
    pub label: String,
    pub group: &'static str,
    pub class: String,
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
    pub catalog: Vec<CatalogItem>,
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
                [x + 8., y + h / 2. - 8., 4., 16.],
                0.,
                "#fffaa1",
                "#e5d360",
                "",
            );
        }
        self.text(
            label,
            [x + 30., y + (h - 24.) / 2.],
            1.,
            if selected { SELECTED_FONT } else { FONT },
            if selected {
                "255,240,140"
            } else {
                "235,235,235"
            },
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
        || !request.minimap_zoom.is_finite()
        || !(25.0..=200.0).contains(&request.minimap_zoom)
        || request.room_code.len() > 64
        || request.player_name.len() > 32
        || request.lobby_status.len() > 256
        // Shared Free Ride rooms hold up to 20 riders; private rooms four.
        || request.lobby_players.len() > 20
        || request.lobby_players.iter().any(|p| {
            p.id.parse::<u8>().map_or(true, |id| id >= 20)
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
        || request.gear_rows.len() > 8
        || request.gear_rows.iter().any(|row| row.len() > 64)
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
        "settings"
            | "extras"
            | "pause"
            | "players"
            | "hud-settings"
            | "minimap-config"
            | "race-overlays"
            | "controls"
            | "bindings"
            | "music"
    );
    let mut f = Frame {
        width: w,
        height: h,
        draw: vec![],
        hits: vec![],
        choices: vec![],
        sliders: vec![],
        inputs: vec![],
        catalog: vec![],
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
        "create-lobby" => "Host Game",
        "join-lobby" => "Join Game",
        "lobby" => "Free Ride Lobby",
        "garage" => "Garage",
        "gear" => "Rider Gear",
        "loading" => "Loading",
        "tracks" => "Freestyle",
        "race" => "Single Race",
        "pause" => "Pause",
        "settings" => "Settings",
        "extras" => "Extras",
        "players" => "Lobby Players",
        "hud-settings" => "Display",
        "minimap-config" => "Minimap Configuration",
        "race-overlays" => "Race Overlays",
        "controls" => "Controls",
        "bindings" => {
            if request.binding_device == "controller" {
                "Controller"
            } else {
                "Keyboard"
            }
        }
        "music" => "Audio",
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
    // Pages that show the current ride carry the selectable vehicle catalog.
    if matches!(
        request.page.as_str(),
        "main" | "single" | "garage" | "create-lobby" | "join-lobby"
    ) {
        for id in 1..=crate::motorcycle::SELECTION_COUNT {
            let label = crate::player_models::selection(id, 0)?.label;
            f.catalog.push(if id >= 27 {
                let (size, pro) = crate::motorcycle::class(id)?;
                CatalogItem {
                    id,
                    label: label.split(" / ").next().unwrap_or(&label).into(),
                    group: "Motorcycle",
                    class: format!("{size}cc {}", if pro { "Pro" } else { "Race" }),
                }
            } else {
                CatalogItem {
                    id,
                    label,
                    group: "ATV",
                    class: String::new(),
                }
            });
        }
    }
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
            // Ride modes come first; the Garage (ride setup) follows them.
            let items: &[(&str, &str)] = if main {
                &[
                    ("Multiplayer", "multiplayer"),
                    ("Arlington Free Ride", "freeride"),
                    ("Settings", "settings"),
                    ("Dev log", "devlog"),
                ]
            } else if request.race_available {
                &[("Race", "race"), ("Garage", "garage")]
            } else {
                &[("Garage", "garage")]
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
            let detail_y = if main {
                394.
            } else if request.race_available {
                292.
            } else {
                236.
            };
            let detail = if !main && request.race_available && request.selection == 1 {
                "Race CPU riders on 32 original National and Supercross courses."
            } else if main && request.selection == 2 {
                "Drop straight into Arlington Stream with up to 20 riders online."
            } else if main && request.selection == 3 {
                "Controls, display, audio and extras."
            } else if main && request.selection == 4 {
                "Progress, screenshots and what is next."
            } else {
                detail
            };
            f.text(
                detail,
                [30., detail_y + 12.],
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
                    f.draw.push(Draw::ModelPreview {
                        model: "quad",
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
            let series = if request.race_course.starts_with("sx") {
                "Supercross"
            } else {
                "National"
            };
            let name = if request.track_name.is_empty() {
                "Original course"
            } else {
                request.track_name.as_str()
            };
            f.text(name, [30., 82.], 1., FONT, "255,255,255", w - 60.);
            f.text(
                format!("{series} race preview"),
                [30., 112.],
                0.8,
                FONT,
                "255,226,107",
                w - 60.,
            );
            // Five full-height rows fit portrait. Landscape moves the share
            // link beside Back so every target keeps its 44px minimum.
            let rows = [
                (format!("Course: {name}"), "racetrack"),
                (format!("Laps: {}", request.race_laps.max(1)), "racelaps"),
                (format!("CPU riders: {}", request.race_cpu), "racecpu"),
                ("Start Race".to_string(), "racestart"),
                ("Copy ride link".to_string(), "copyride"),
            ];
            for (i, (label, action)) in rows.iter().enumerate() {
                let rect = if !portrait && *action == "copyride" {
                    [168., h - 68., 200., 56.]
                } else {
                    [24., 140. + i as f32 * 56., list_w.max(290.), 54.]
                };
                f.choices.push((*action).into());
                f.button(label, action, rect, request.selection % rows.len() == i);
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
                [24., 98., w - 48., (h - 440.).clamp(100., 420.)]
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
                ("Rider gear", "gear"),
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
        "gear" => {
            // Original Profile Store categories; the jersey row sets the shirt
            // and its matching sleeves together. Quad colour is authored.
            let actions = [
                ("Helmet", "gearhelmet"),
                ("Jersey", "gearjersey"),
                ("Pants", "gearpant"),
                ("Gloves", "gearglove"),
                ("Boots", "gearboot"),
                ("Goggles", "geargoggle"),
                ("Quad colour", "gearpaint"),
            ];
            // Seven rows between the title and the footer Back button.
            let row_h =
                ((h - 76. - 70.) / actions.len() as f32).min(if portrait { 56. } else { 50. });
            let list_w = if portrait {
                w - 48.
            } else {
                (w * 0.42).max(290.)
            };
            for (i, (label, action)) in actions.iter().enumerate() {
                let value = request.gear_rows.get(i).map_or("Original", String::as_str);
                f.choices.push((*action).into());
                f.button(
                    &format!("{label}: {value}"),
                    action,
                    [24., 76. + i as f32 * row_h, list_w, row_h - 2.],
                    request.selection == i,
                );
            }
            f.footer(true);
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
            for (i, (label, action)) in [("Host Game", "create-lobby"), ("Join Game", "join-lobby")]
                .iter()
                .enumerate()
            {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [24., 104. + i as f32 * 58., list_w, 58.],
                    request.selection == i,
                );
            }
            f.footer(true);
        }
        "create-lobby" | "join-lobby" => {
            let creating = request.page == "create-lobby";
            for (action, label, value, y, max_length) in [
                (
                    "player-name",
                    "Rider name",
                    request.player_name.clone(),
                    104.,
                    24,
                ),
                (
                    if creating { "create-code" } else { "room-code" },
                    if creating {
                        "Lobby code (8 letters/numbers, optional)"
                    } else {
                        "Lobby code"
                    },
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
            // Hosting continues to the original track picker; its Ride starts
            // the game at once so friends can drop in from the invite.
            let rows = [
                if creating {
                    ("Select Track", "hosttrack")
                } else {
                    ("Join Game", "join")
                },
                ("Garage", "garage"),
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
            let list_w = if portrait {
                list_w
            } else {
                list_w.min(w * 0.48 - 36.)
            };
            // Shown only while joining: the ride starts as soon as it loads.
            let rows: &[(&str, &str)] = if request.lobby_public {
                &[("Leave Game", "leave")]
            } else {
                &[("Copy invite link", "share"), ("Leave Game", "leave")]
            };
            let mut details_y = 100.;
            if !request.track_image.is_empty() {
                let preview = if portrait {
                    // Keep the source aspect ratio and room for all lobby actions
                    // on short portrait screens as well as full-height phones.
                    let photo_w = (w - 48.).min((h - 414.).max(34.) / 0.64);
                    [0.5 * (w - photo_w), 90., photo_w, photo_w * 0.64]
                } else {
                    [
                        w * 0.48,
                        100.,
                        w * 0.48 - 24.,
                        (h - 156.).min((w * 0.48 - 24.) * 0.64),
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
                if portrait {
                    details_y = preview[1] + preview[3] + 16.;
                }
            }
            f.text(
                format!("Code: {}", request.room_code),
                [30., details_y],
                1.15,
                FONT,
                "255,230,100",
                list_w,
            );
            f.text(
                &request.track_name,
                [30., details_y + if portrait { 32. } else { 40. }],
                1.,
                FONT,
                "235,235,235",
                list_w,
            );
            f.text(
                &request.lobby_status,
                [30., details_y + if portrait { 64. } else { 79. }],
                0.8,
                FONT,
                "210,210,210",
                list_w,
            );
            let row_y = details_y + 114.;
            let row_h = ((h - 20. - row_y) / rows.len() as f32).clamp(44., 54.);
            for (i, (label, action)) in rows.iter().enumerate() {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [24., row_y + i as f32 * row_h, list_w, row_h],
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
            // A full shared room lists 20 riders: switch to columns of
            // compact rows once they no longer fit above Back.
            let count = request.lobby_players.len().max(1);
            let max_rows = (((h - 170.) / 44.).floor() as usize).max(1);
            let columns = count.div_ceil(max_rows);
            let rows = count.div_ceil(columns).max(5);
            let row = ((h - 170.) / rows as f32).clamp(36., 64.);
            let column_w = (w - 48. - (columns - 1) as f32 * 8.) / columns as f32;
            for (i, player) in request.lobby_players.iter().enumerate() {
                let x = 24. + (i / rows) as f32 * (column_w + 8.);
                let y = 96. + (i % rows) as f32 * row;
                if player.value == "self" || player.value == "waiting" {
                    let label = if player.value == "waiting" {
                        format!("{} / waiting", player.label)
                    } else {
                        player.label.clone()
                    };
                    f.text(
                        label,
                        [x + 6., y + (row - 24.) / 2. + 4.],
                        0.9,
                        FONT,
                        "210,210,210",
                        column_w - 12.,
                    );
                } else {
                    let action = format!("spawn:{}", player.id);
                    f.choices.push(action.clone());
                    let label = format!("Spawn beside {}", player.label);
                    f.button(
                        if columns > 1 { &player.label } else { &label },
                        &action,
                        [x, y, column_w, row],
                        request.selection == f.choices.len() - 1,
                    );
                    f.hits.last_mut().unwrap().label = label;
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
                ("Settings", "settings"),
            ];
            let online = !request.lobby_status.is_empty();
            if online {
                // Original pause rows: change ride in place; the host picks
                // the next track for everyone.
                choices.insert(2, ("Players", "players"));
                choices.insert(3, ("Change ATV and Rider", "changeride"));
                if request.lobby_host && !request.lobby_public {
                    choices.insert(4, ("Select Track", "hosttrack"));
                }
            }
            // Shared Free Ride rooms have no invite: everyone just drops in.
            if !request.lobby_public {
                choices.push((
                    if online {
                        "Copy invite link"
                    } else {
                        "Copy ride link"
                    },
                    "copyride",
                ));
            }
            choices.push(("Dev log", "devlog"));
            choices.push(if online {
                ("Leave Game", "leave")
            } else {
                ("Quit to Main Menu", "main")
            });
            let available = h - if portrait { 110. } else { 90. };
            let max_rows = ((available * scale / 44.).floor() as usize).max(1);
            let columns = choices.len().div_ceil(max_rows);
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
        "settings" | "extras" | "hud-settings" => {
            let crash_label = match request.crash_mode.as_str() {
                "gorn" => "Crashes: Gorn (blood & limbs)",
                "crazy" => "Crashes: Crazy",
                _ => "Crashes: Original",
            };
            let diagnostics = match request.diagnostics_mode.as_str() {
                "performance" => "Stats: Performance (F3)",
                "debug" => "Stats: Debug (F3)",
                _ => "Stats: Off (F3)",
            };
            let speed_label = match request.speed_rush.as_str() {
                "low" => "Speed rush: Low",
                "normal" => "Speed rush: Normal",
                "high" => "Speed rush: High",
                _ => "Speed rush: Off",
            };
            let mut choices = match request.page.as_str() {
                "settings" => vec![
                    ("Controls", "controls"),
                    ("Display", "hud-settings"),
                    ("Audio", "music"),
                    ("Extras", "extras"),
                ],
                "extras" => vec![
                    (crash_label, "funnycrashes"),
                    (
                        if request.original_camera {
                            "Crash camera: Original"
                        } else {
                            "Crash camera: Tight"
                        },
                        "crash-camera",
                    ),
                    (
                        if request.chase_assist {
                            "Chase camera: Assisted"
                        } else {
                            "Chase camera: Original"
                        },
                        "chase-camera",
                    ),
                    (
                        if request.flips {
                            "Flips: Unleashed backflip & 360"
                        } else {
                            "Flips: Off (original)"
                        },
                        "flips",
                    ),
                    (diagnostics, "diagnostics"),
                ],
                _ => vec![
                    ("Minimap configuration", "minimap-config"),
                    ("Race overlays", "race-overlays"),
                    (speed_label, "speed-rush"),
                ],
            };
            if request.page == "hud-settings" && request.fullscreen {
                choices.push(("Fullscreen", "fullscreen"));
            }
            choices.push(("Back", "back"));
            for (i, (label, action)) in choices.iter().enumerate() {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [24., 92. + i as f32 * 54., w - 48., 54.],
                    request.selection % choices.len() == i,
                );
            }
        }
        "race-overlays" => {
            // The original Pause → Overlays rows (002df120): each group On/Off;
            // Position chooses the name list, the place number or nothing.
            let on = |group: &str| {
                if request.race_overlays_off.iter().any(|g| g == group) {
                    "Off"
                } else {
                    "On"
                }
            };
            let position = match request.race_position.as_str() {
                "place" => "Place",
                "off" => "Off",
                _ => "Name",
            };
            let rows = [
                (format!("Timing: {}", on("timing")), "overlay-timing"),
                (format!("Lap: {}", on("lap")), "overlay-lap"),
                (format!("Score: {}", on("score")), "overlay-score"),
                (format!("Position: {position}"), "overlay-position"),
                (format!("Informational: {}", on("info")), "overlay-info"),
                (
                    format!("Speedometer/Preload: {}", on("speed")),
                    "overlay-speed",
                ),
                (format!("Radar: {}", on("radar")), "overlay-radar"),
                ("Back".to_string(), "back"),
            ];
            let row = 52_f32.max(44. / scale);
            let columns = if 92. + rows.len() as f32 * (row + 2.) > h - 12. {
                2
            } else {
                1
            };
            let column_width = (w - 48.) / columns as f32;
            for (i, (label, action)) in rows.iter().enumerate() {
                f.choices.push((*action).into());
                f.button(
                    label,
                    action,
                    [
                        24. + (i % columns) as f32 * column_width,
                        92. + (i / columns) as f32 * (row + 2.),
                        column_width,
                        row,
                    ],
                    request.selection % rows.len() == i,
                );
            }
        }
        "minimap-config" => {
            let selected = request.selection % 7;
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
            for (i, (action, label, value, min, max, step, units)) in [
                (
                    "minimap-size",
                    "Minimap size",
                    request.minimap_size,
                    96.,
                    224.,
                    8.,
                    "px",
                ),
                (
                    "minimap-zoom",
                    "Minimap zoom",
                    request.minimap_zoom,
                    25.,
                    200.,
                    25.,
                    "%",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let x = 30. + (i % columns) as f32 * column_width;
                let y = y + (i / columns) as f32 * (row + 36.);
                let width = column_width - 12.;
                f.text(
                    format!("{label}: {} {units}", value as u32),
                    [x, y],
                    0.85,
                    FONT,
                    "255,226,107",
                    width,
                );
                f.sliders.push(Slider {
                    action,
                    label,
                    rect: [x, y + 26., width, row],
                    value,
                    min,
                    max,
                    step,
                    selected: selected == 4 + i,
                });
                f.choices.push(action.into());
            }
            f.button("Back", "back", [24., h - 68., 132., 56.], selected == 6);
            f.choices.push("back".into());
        }
        "controls" => {
            f.sliders.push(Slider {
                action: "steering-sensitivity",
                label: "Mobile steering sensitivity",
                rect: [30., 100., w - 60., 44.],
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
                (
                    if request.one_handed_mobile {
                        "One-handed mobile: On"
                    } else {
                        "One-handed mobile: Off"
                    },
                    "onehanded",
                ),
                ("Back", "back"),
            ]
            .iter()
            .enumerate()
            {
                f.button(
                    label,
                    action,
                    [24., 166. + i as f32 * 54., w - 48., 54.],
                    request.selection == i + 1,
                );
                f.choices.push((*action).into());
            }
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
                        ("Back".to_string(), "back"),
                        (
                            if request.music_enabled {
                                "Music: On".to_string()
                            } else {
                                "Music: Off".to_string()
                            },
                            "musictoggle",
                        ),
                        (format!("Volume: {}%", request.music_volume), "musicvolume"),
                    ]
                } else {
                    vec![
                        ("Back".to_string(), "back"),
                        ("Clear".to_string(), "bindclear"),
                        ("Defaults".to_string(), "bindreset"),
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
            lobby_public: false,
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
            gear_rows: vec![],
            music_page: 0,
            music_status: String::new(),
            music_enabled: true,
            music_volume: 70,
            menu_logo: true,
            menu_atv: true,
            diagnostics_mode: String::new(),
            one_handed_mobile: false,
            funny_crashes: false,
            crash_mode: String::new(),
            original_camera: false,
            chase_assist: false,
            flips: false,
            speed_rush: "normal".into(),
            minimap_enabled: true,
            minimap_size: 160.,
            minimap_zoom: 50.,
            minimap_shape: "circle".into(),
            minimap_corner: "top-right".into(),
            minimap_orientation: "north-up".into(),
            mobile_steering_sensitivity: 85.,
            race_overlays_off: vec![],
            race_position: "name".into(),
            race_laps: 3,
            race_cpu: 3,
            race_difficulty: 1,
            race_available: false,
            race_course: "env01".into(),
            race_active: false,
        }
    }
    #[test]
    fn multiplayer_intent_and_forms_are_separate() {
        let menu = layout(request("multiplayer", 390., 844.)).unwrap();
        assert!(menu.inputs.is_empty());
        assert_eq!(menu.choices, ["create-lobby", "join-lobby"]);
        assert_eq!(menu.hits[0].label, "Host Game");
        let create = layout(request("create-lobby", 390., 844.)).unwrap();
        assert_eq!(create.inputs.len(), 2);
        assert_eq!(create.inputs[1].action, "create-code");
        assert_eq!(create.choices, ["hosttrack", "garage"]);
        let join = layout(request("join-lobby", 390., 844.)).unwrap();
        assert_eq!(join.inputs[1].action, "room-code");
        assert_eq!(join.choices, ["join", "garage"]);
    }
    #[test]
    fn online_pause_offers_ride_change_host_track_and_leave() {
        let online = |host| {
            let mut input = request("pause", 1280., 720.);
            input.lobby_status = "2/4".into();
            input.lobby_host = host;
            layout(input).unwrap()
        };
        let guest = online(false);
        assert!(guest.choices.contains(&"changeride".to_string()));
        assert!(!guest.choices.contains(&"hosttrack".to_string()));
        assert_eq!(guest.choices.last().unwrap(), "leave");
        let host = online(true);
        assert!(host.choices.contains(&"hosttrack".to_string()));
        let solo = layout(request("pause", 1280., 720.)).unwrap();
        assert!(!solo.choices.contains(&"changeride".to_string()));
        assert_eq!(solo.choices.last().unwrap(), "main");
        // Shared Free Ride: change ride, find riders and leave; no host or invite rows.
        let mut input = request("pause", 390., 844.);
        input.lobby_status = "12/20 riders".into();
        input.lobby_host = true;
        input.lobby_public = true;
        let public = layout(input).unwrap();
        for row in ["changeride", "players", "leave"] {
            assert!(public.choices.contains(&row.to_string()), "{row}");
        }
        for row in ["hosttrack", "copyride"] {
            assert!(!public.choices.contains(&row.to_string()), "{row}");
        }
        let mut input = request("lobby", 390., 844.);
        input.lobby_public = true;
        assert_eq!(layout(input).unwrap().choices, ["leave"]);
    }
    #[test]
    fn full_shared_room_players_fit_above_back_on_phones_and_desktop() {
        for (width, height) in [(390., 844.), (844., 390.), (1280., 720.), (390., 600.)] {
            let mut input = request("players", width, height);
            input.lobby_players = (0..20)
                .map(|id| ControlRow {
                    id: id.to_string(),
                    label: format!("Rider {id:02} with a long name"),
                    value: if id == 0 { "self" } else { "ready" }.into(),
                })
                .collect();
            let frame = layout(input).unwrap();
            let spawns = frame
                .hits
                .iter()
                .filter(|hit| hit.action.starts_with("spawn:"))
                .collect::<Vec<_>>();
            assert_eq!(spawns.len(), 19, "{width}x{height}");
            let back = frame.hits.iter().find(|hit| hit.action == "pause").unwrap();
            for hit in &spawns {
                let [x, y, w, h] = hit.rect;
                assert!(hit.label.starts_with("Spawn beside "));
                assert!(
                    x >= 0. && x + w <= frame.width + 0.5,
                    "{width}x{height} {:?}",
                    hit.rect
                );
                assert!(
                    y + h <= back.rect[1] + 0.5,
                    "{width}x{height} {:?}",
                    hit.rect
                );
                assert!(h >= 36.);
            }
            // Rows never overlap one another.
            for (i, a) in spawns.iter().enumerate() {
                for b in &spawns[i + 1..] {
                    let overlap = a.rect[0] < b.rect[0] + b.rect[2]
                        && b.rect[0] < a.rect[0] + a.rect[2]
                        && a.rect[1] < b.rect[1] + b.rect[3] - 0.5
                        && b.rect[1] < a.rect[1] + a.rect[3] - 0.5;
                    assert!(!overlap, "{width}x{height} {:?} {:?}", a.rect, b.rect);
                }
            }
        }
    }
    #[test]
    fn lobby_preview_leaves_visible_actions_on_short_and_full_screens() {
        for (width, height) in [
            (1280., 720.),
            (844., 390.),
            (390., 844.),
            (390., 600.),
            (390., 448.),
        ] {
            for (host, started) in [(true, false), (false, false), (true, true)] {
                let mut input = request("lobby", width, height);
                input.lobby_host = host;
                input.lobby_started = started;
                let frame = layout(input).unwrap();
                let preview = frame
                    .draw
                    .iter()
                    .find_map(|draw| match draw {
                        Draw::Preview { image, rect } => {
                            assert_eq!(image, "arlington");
                            Some(*rect)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!((preview[3] / preview[2] - 0.64).abs() < 0.001);
                for hit in &frame.hits {
                    assert!(hit.rect[0] >= 0. && hit.rect[1] >= 0.);
                    assert!(hit.rect[0] + hit.rect[2] <= frame.width);
                    assert!(hit.rect[1] + hit.rect[3] <= frame.height);
                    assert!(
                        hit.rect[1] >= preview[1] + preview[3]
                            || hit.rect[0] + hit.rect[2] <= preview[0]
                    );
                }
            }
        }
        let mut training = request("lobby", 390., 844.);
        training.track_image.clear();
        assert!(
            layout(training)
                .unwrap()
                .draw
                .iter()
                .all(|draw| !matches!(draw, Draw::Preview { .. }))
        );
    }
    #[test]
    fn supported_routes_have_no_unfinished_choices() {
        assert_eq!(
            layout(request("main", 1280., 720.)).unwrap().choices,
            ["single", "multiplayer", "freeride", "settings", "devlog"]
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
    fn garage_catalog_lists_every_vehicle_once() {
        let menu = layout(request("garage", 1280., 720.)).unwrap();
        assert_eq!(menu.catalog.len(), 46);
        assert!(
            menu.catalog
                .iter()
                .enumerate()
                .all(|(i, item)| item.id as usize == i + 1)
        );
        assert_eq!(
            menu.catalog
                .iter()
                .filter(|item| item.group == "ATV")
                .count(),
            26
        );
        assert_eq!(menu.catalog[13].label, "Honda 250EX");
        assert_eq!(menu.catalog[30].class, "250cc Race");
        assert!(!menu.catalog[30].label.contains(" / "));
        assert_eq!(
            layout(request("main", 1280., 720.)).unwrap().catalog.len(),
            46
        );
        assert!(
            layout(request("tracks", 1280., 720.))
                .unwrap()
                .catalog
                .is_empty()
        );
    }
    #[test]
    fn race_availability_setup_and_restart_match_the_live_mode() {
        let mut r = request("single", 390., 844.);
        r.race_available = true;
        assert_eq!(layout(r).unwrap().choices, ["tracks", "race", "garage"]);
        let f = layout(request("race", 844., 390.)).unwrap();
        assert_eq!(
            f.choices,
            ["racetrack", "racelaps", "racecpu", "racestart", "copyride"]
        );
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
            (390., 568.),
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
                "gear",
                "loading",
                "settings",
                "extras",
                "pause",
                "controls",
                "hud-settings",
                "minimap-config",
                "race-overlays",
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
    fn extras_default_to_original_crashes_and_chase_camera() {
        let labels = |r: Request| {
            let menu = layout(r).unwrap();
            assert_eq!(
                menu.choices,
                [
                    "funnycrashes",
                    "crash-camera",
                    "chase-camera",
                    "flips",
                    "diagnostics",
                    "back"
                ]
            );
            menu.hits
                .into_iter()
                .map(|hit| hit.label)
                .collect::<Vec<_>>()
        };
        let original = labels(request("extras", 390., 844.));
        assert_eq!(original[0], "Crashes: Original");
        assert_eq!(original[2], "Chase camera: Original");
        assert_eq!(original[3], "Flips: Off (original)");
        let mut enhanced = request("extras", 390., 844.);
        enhanced.crash_mode = "crazy".into();
        enhanced.chase_assist = true;
        enhanced.flips = true;
        let enhanced = labels(enhanced);
        assert_eq!(enhanced[0], "Crashes: Crazy");
        assert_eq!(enhanced[2], "Chase camera: Assisted");
        assert_eq!(enhanced[3], "Flips: Unleashed backflip & 360");
    }
    #[test]
    fn minimap_settings_fit_and_validate_sizes() {
        assert_eq!(
            layout(request("hud-settings", 390., 844.)).unwrap().choices,
            [
                "minimap-config",
                "race-overlays",
                "speed-rush",
                "fullscreen",
                "back"
            ]
        );
        for (w, h) in [
            (320., 568.),
            (390., 844.),
            (390., 568.),
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
                    "minimap-zoom",
                    "back"
                ]
            );
            assert_eq!(frame.sliders.len(), 2);
            for slider in &frame.sliders {
                let [x, y, width, height] = slider.rect;
                assert!(
                    x >= 0. && y >= 0. && x + width <= frame.width && y + height <= frame.height
                );
                assert!(height * w / frame.width >= 44. - 0.001);
                assert!(frame.hits.iter().all(|hit| x >= hit.rect[0] + hit.rect[2]
                    || x + width <= hit.rect[0]
                    || y >= hit.rect[1] + hit.rect[3]
                    || y + height <= hit.rect[1]));
            }
            assert_eq!(
                (
                    frame.sliders[0].min,
                    frame.sliders[0].max,
                    frame.sliders[0].step,
                    frame.sliders[0].value
                ),
                (96., 224., 8., 160.)
            );
            assert_eq!(
                (
                    frame.sliders[1].min,
                    frame.sliders[1].max,
                    frame.sliders[1].step,
                    frame.sliders[1].value
                ),
                (25., 200., 25., 50.)
            );
            let [x, y, width, height] = frame.sliders[0].rect;
            let [other_x, other_y, _, _] = frame.sliders[1].rect;
            assert!(
                x + width <= other_x || y + height <= other_y,
                "sliders do not overlap"
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
        for zoom in [f32::NAN, 0., 201.] {
            let mut r = request("minimap-config", 390., 844.);
            r.minimap_zoom = zoom;
            assert!(layout(r).is_err());
        }
    }
    #[test]
    fn invalid_menu_requests_are_rejected() {
        assert!(layout(request("unknown", 390., 844.)).is_err());
        assert!(layout(request("main", f32::NAN, 844.)).is_err());
    }
}
