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
    Preview {
        image: String,
        rect: [f32; 4],
    },
}
#[derive(Serialize)]
pub struct Hit {
    pub label: String,
    pub action: &'static str,
    pub rect: [f32; 4],
    pub selected: bool,
}
#[derive(Serialize)]
pub struct Frame {
    pub width: f32,
    pub height: f32,
    pub draw: Vec<Draw>,
    pub hits: Vec<Hit>,
    pub choices: Vec<&'static str>,
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
    fn button(&mut self, label: &str, action: &'static str, rect: [f32; 4], selected: bool) {
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
            action,
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
        || request.track_name.len() > 128
        || request.track_image.len() > 128
    {
        return Err("invalid menu viewport or label".into());
    }
    let scale = (request.width / 390.).min(request.height / 448.);
    let w = request.width / scale;
    let h = request.height / scale;
    let portrait = h > w;
    let pause = matches!(request.page.as_str(), "pause" | "controls");
    let mut f = Frame {
        width: w,
        height: h,
        draw: vec![],
        hits: vec![],
        choices: vec![],
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
            f.choices.extend(["main", "devlog"]);
            let y = (h * 0.30).max(95.);
            f.text("ATV", [34., y], 3.2, HEADER_FONT, "232,232,232", w - 68.);
            f.text(
                "OFFROAD FURY 2",
                [36., y + 115.],
                1.25,
                HEADER_FONT,
                "247,206,72",
                w - 72.,
            );
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
            f.choices.push(action);
            let main = request.page == "main";
            f.button(
                label,
                action,
                [20., 102., list_w, 58.],
                !main || request.selection.is_multiple_of(2),
            );
            if main {
                f.choices.push("devlog");
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
            let mut choices = vec![
                ("Resume", "resume"),
                ("Reset ATV", "reset"),
                ("Controls", "controls"),
            ];
            if request.fullscreen {
                choices.push(("Fullscreen", "fullscreen"));
            }
            choices.extend([("Dev log", "devlog"), ("Quit to Main Menu", "main")]);
            let row = if portrait { 64. } else { 48. };
            let selected = request.selection % choices.len();
            for (i, (label, action)) in choices.iter().enumerate() {
                f.choices.push(action);
                f.button(
                    label,
                    action,
                    [
                        24.,
                        98. + i as f32 * row,
                        if portrait { w - 60. } else { w - 108. },
                        row,
                    ],
                    i == selected,
                );
            }
        }
        "controls" => {
            let groups = [
                (
                    "TOUCH",
                    vec![
                        "Left: steer and lean, on ground or in air.",
                        "Right: up to drive; down to brake / reverse.",
                        "Hold right to brace. Lift to release preload.",
                        "In air: extra right finger swipes stunts.",
                        "Up / down: Nac Nac / No Footed Can Can.",
                        "Left / right: Suicide No Hander / Bar Hop.",
                    ],
                ),
                (
                    "CONTROLLER",
                    vec![
                        "Stick: steer / lean. Triggers: drive / brake.",
                        "Bottom + left face buttons: reverse.",
                        "Circle / Triangle / L1 + stick: stunts.",
                        "R3: camera. Start: pause.",
                    ],
                ),
                (
                    "KEYBOARD",
                    vec![
                        "WASD: drive. Q / E: lean. R: reverse.",
                        "J / K / L + arrows: stunts.",
                        "C: camera. Esc: pause.",
                    ],
                ),
            ];
            let mut y = 96.;
            for (index, (heading, lines)) in groups.into_iter().enumerate() {
                let x = if portrait {
                    30.
                } else {
                    30. + index as f32 * (w - 60.) / 3.
                };
                let width = if portrait { w - 60. } else { (w - 90.) / 3. };
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
                    y += if portrait { 27. } else { 34. };
                }
                y += 28.;
            }
            f.footer(true);
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
        assert!(!layout(r).unwrap().choices.contains(&"fullscreen"));
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
            for page in ["title", "main", "single", "tracks", "pause", "controls"] {
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
    fn invalid_menu_requests_are_rejected() {
        assert!(layout(request("race", 390., 844.)).is_err());
        assert!(layout(request("main", f32::NAN, 844.)).is_err());
    }
}
