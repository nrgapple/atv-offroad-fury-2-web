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
    #[serde(default = "default_steering_sensitivity")]
    pub mobile_steering_sensitivity: f32,
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
pub struct Frame {
    pub width: f32,
    pub height: f32,
    pub draw: Vec<Draw>,
    pub hits: Vec<Hit>,
    pub choices: Vec<String>,
    pub sliders: Vec<Slider>,
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
        "pause" | "controls" | "bindings" | "music"
    );
    let mut f = Frame {
        width: w,
        height: h,
        draw: vec![],
        hits: vec![],
        choices: vec![],
        sliders: vec![],
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
        "title" => "ATV Offroad Fury 2",
        "main" => "Main Menu",
        "single" => "Single Player",
        "tracks" => "Freestyle",
        "pause" => "Pause",
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
                    image: "original-logo",
                    rect: [30., y + 30., width, width / 3.],
                });
            } else {
                f.text("ATV", [34., y], 3.2, HEADER_FONT, "232,232,232", w - 68.);
                f.text(
                    "OFFROAD FURY 2",
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
                !main || request.selection.is_multiple_of(2),
            );
            if main {
                f.choices.push("devlog".into());
                f.button(
                    "Dev log",
                    "devlog",
                    [20., 160., list_w, 58.],
                    request.selection % 2 == 1,
                );
            }
            let detail_y = if main { 254. } else { 190. };
            f.rect(
                [24., detail_y, list_w - 4., 72.],
                6.,
                "#292020",
                "#1c1717",
                "#100b09",
            );
            let detail = if main && request.selection % 2 == 1 {
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
                ("Reset ATV", "reset"),
                ("Controls", "controls"),
                (diagnostics_label, "diagnostics"),
                (mobile_label, "onehanded"),
            ];
            if request.music_ready {
                choices.push(("Music", "music"));
            }
            if request.fullscreen {
                choices.push(("Fullscreen", "fullscreen"));
            }
            choices.extend([("Dev log", "devlog"), ("Quit to Main Menu", "main")]);
            let row = ((h - if portrait { 110. } else { 90. }) / choices.len() as f32)
                .min(if portrait { 64. } else { 48. });
            let selected = request.selection % choices.len();
            for (i, (label, action)) in choices.iter().enumerate() {
                f.choices.push(action.to_string());
                f.button(
                    label,
                    action,
                    [
                        24.,
                        (if portrait { 98. } else { 78. }) + i as f32 * row,
                        if portrait { w - 60. } else { w - 108. },
                        row,
                    ],
                    i == selected,
                );
            }
        }
        "controls" => {
            let compact = portrait && h < 540.;
            let touch_lines = if compact {
                if request.one_handed_mobile {
                    vec![
                        "Hold: drive / brace. Drag: steer / lean.",
                        "Lift: coast / release preload.",
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
                    "Same controls in air. No reverse or tricks.",
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
            mobile_steering_sensitivity: 85.,
        }
    }
    #[test]
    fn supported_routes_have_no_unfinished_choices() {
        assert_eq!(
            layout(request("main", 1280., 720.)).unwrap().choices,
            ["single", "devlog"]
        );
        assert_eq!(
            layout(request("single", 1280., 720.)).unwrap().choices,
            ["tracks"]
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
    fn touch_regions_fit_viewport_at_all_aspects() {
        for (w, h) in [
            (390., 844.),
            (844., 390.),
            (1280., 720.),
            (320., 568.),
            (768., 1024.),
        ] {
            for page in [
                "title", "main", "single", "tracks", "pause", "controls", "bindings", "music",
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
    fn invalid_menu_requests_are_rejected() {
        assert!(layout(request("race", 390., 844.)).is_err());
        assert!(layout(request("main", f32::NAN, 844.)).is_err());
    }
}
