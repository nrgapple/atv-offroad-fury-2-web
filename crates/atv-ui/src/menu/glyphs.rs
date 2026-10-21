//! Controller button glyphs (web/pad-glyphs.js): the original Controller
//! Settings page's buttons redrawn as vectors in the 32x32 viewBox, the
//! binding rows' glyph lists and the original-style summary panel. Glyph
//! shapes rasterize once per device scale (`front::logo` rasterizer); their
//! labels (L1, R2, L/R, raw B4) are UI text over the image.
use crate::{
    color::Color,
    front::logo::{Op, Paint, raster_ops},
    front::pixels::Rgba,
};
use atv_game::ride::bindings::{Atom, Binding};

/// What one input atom shows (pad-glyphs.js `describe`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Glyph {
    /// cross, circle, square, triangle, l1, r1, l2, r2, select, start, l3,
    /// r3, dpad-up/down/left/right, home, stick, stick-pair, dpad, raw.
    pub kind: String,
    pub name: String,
    /// Raw inputs: `B4`, `A2-`.
    pub short: String,
    /// stick: left/right; direction or plane.
    pub stick: String,
    pub direction: String,
}

const BUTTONS: [(&str, &str); 17] = [
    ("cross", "Cross"),
    ("circle", "Circle"),
    ("square", "Square"),
    ("triangle", "Triangle"),
    ("l1", "L1"),
    ("r1", "R1"),
    ("l2", "L2"),
    ("r2", "R2"),
    ("select", "SELECT"),
    ("start", "START"),
    ("l3", "L3"),
    ("r3", "R3"),
    ("dpad-up", "D-pad up"),
    ("dpad-down", "D-pad down"),
    ("dpad-left", "D-pad left"),
    ("dpad-right", "D-pad right"),
    ("home", "Home"),
];

fn glyph(kind: &str, name: String) -> Glyph {
    Glyph {
        kind: kind.into(),
        name,
        short: String::new(),
        stick: String::new(),
        direction: String::new(),
    }
}

pub fn describe(atom: &Atom, standard: bool) -> Glyph {
    match atom {
        Atom::Button(b) => match BUTTONS.get(*b as usize).filter(|_| standard) {
            Some((kind, name)) => glyph(kind, (*name).into()),
            None => Glyph {
                short: format!("B{b}"),
                ..glyph("raw", format!("Button {b}"))
            },
        },
        Atom::Axis(a, s) if standard && *a < 4 => {
            let stick = if *a < 2 { "left" } else { "right" };
            let direction = match (a % 2, *s < 0) {
                (0, true) => "left",
                (0, false) => "right",
                (_, true) => "up",
                (_, false) => "down",
            };
            Glyph {
                stick: stick.into(),
                direction: direction.into(),
                ..glyph(
                    "stick",
                    format!(
                        "{} stick {direction}",
                        if stick == "left" { "Left" } else { "Right" }
                    ),
                )
            }
        }
        Atom::Axis(a, s) => {
            let sign = if *s < 0 { '-' } else { '+' };
            Glyph {
                short: format!("A{a}{sign}"),
                ..glyph("raw", format!("Axis {a} {sign}"))
            }
        }
        Atom::Key(k) => glyph("raw", k.clone()),
    }
}

/// One action's alternatives as glyph chords (`bindingGlyphs`).
pub fn binding_glyphs(bindings: &[Binding], standard: bool) -> Vec<Vec<Glyph>> {
    bindings
        .iter()
        .map(|b| b.atoms().iter().map(|a| describe(a, standard)).collect())
        .collect()
}

/// A label drawn over the glyph image: text, baseline y, size, fill, and
/// the stroke under it (pad-glyphs.js `<text>`), in viewBox units.
pub struct Label {
    pub text: String,
    pub baseline: f32,
    pub size: f32,
    pub fill: Color,
    pub stroke: Option<(Color, f32)>,
}

pub fn label(g: &Glyph) -> Option<Label> {
    let white = |text: &str, baseline| Label {
        text: text.into(),
        baseline,
        size: 12.,
        fill: Color::WHITE,
        stroke: Some((Color::hex("#111"), 0.6)),
    };
    let light = |text: &str, baseline, size| Label {
        text: text.into(),
        baseline,
        size,
        fill: Color::hex("#e6eef8"),
        stroke: None,
    };
    match g.kind.as_str() {
        "l1" | "r1" => Some(white(&g.name, 20.6)),
        "l2" | "r2" => Some(white(&g.name, 21.5)),
        "l3" | "r3" => Some(light(&g.name, 20.2, 11.)),
        "stick" | "stick-pair" => Some(light(if g.stick == "left" { "L" } else { "R" }, 20.2, 11.)),
        "raw" => Some(light(
            &g.short,
            20.,
            if g.short.chars().count() > 3 {
                8.5
            } else {
                10.
            },
        )),
        _ => None,
    }
}

const SLATE: &str = "#4f5a68";
const RIM: &str = "#07090b";
const SHOULDER: &str = "#647c9e";

fn solid(hex: &str) -> Paint {
    Paint::Solid(Color::hex(hex))
}
fn op(path: String, fill: Option<&str>, stroke: Option<(&str, f32)>, round: bool) -> Op {
    Op {
        path,
        transform: [1., 0., 0., 1., 0., 0.],
        fill: fill.map(solid),
        stroke: stroke.map(|(c, w)| (solid(c), w)),
        round,
    }
}
fn translucent(path: String, alpha: f32) -> Op {
    Op {
        path,
        transform: [1., 0., 0., 1., 0., 0.],
        fill: Some(Paint::Solid(Color::WHITE.with_alpha(alpha))),
        stroke: None,
        round: false,
    }
}
fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> String {
    format!(
        "M{} {cy}A{rx} {ry} 0 1 0 {} {cy}A{rx} {ry} 0 1 0 {} {cy}Z",
        cx - rx,
        cx + rx,
        cx - rx
    )
}
fn circle(cx: f32, cy: f32, r: f32) -> String {
    ellipse(cx, cy, r, r)
}
fn rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> String {
    if r <= 0. {
        return format!("M{x} {y}H{}V{}H{x}Z", x + w, y + h);
    }
    format!(
        "M{} {y}H{}A{r} {r} 0 0 1 {} {}V{}A{r} {r} 0 0 1 {} {}H{}A{r} {r} 0 0 1 {x} {}V{}A{r} {r} 0 0 1 {} {y}Z",
        x + r,
        x + w - r,
        x + w,
        y + r,
        y + h - r,
        x + w - r,
        y + h,
        x + r,
        y + h - r,
        y + r,
        x + r
    )
}

const DPAD: &str = "M12 3h8v9h9v8h-9v9h-8v-9H3v-8h9z";
fn dpad_arm(direction: &str) -> Option<&'static str> {
    match direction {
        "up" => Some("M12.5 3.5h7v9h-7z"),
        "down" => Some("M12.5 19.5h7v9h-7z"),
        "left" => Some("M3.5 12.5h9v7h-9z"),
        "right" => Some("M19.5 12.5h9v7h-9z"),
        _ => None,
    }
}
fn stick_arrow(direction: &str) -> Option<&'static str> {
    match direction {
        "up" => Some("M16 1.5l4 4.5h-8z"),
        "down" => Some("M16 30.5l4-4.5h-8z"),
        "left" => Some("M1.5 16l4.5-4v8z"),
        "right" => Some("M30.5 16l-4.5-4v8z"),
        _ => None,
    }
}

/// The glyph's vector elements in its 32x32 viewBox (pad-glyphs.js `svg`).
pub fn ops(g: &Glyph) -> Vec<Op> {
    let face = |symbol: Op| {
        vec![
            op(circle(16., 16., 13.), Some(SLATE), Some((RIM, 2.)), false),
            translucent(ellipse(13., 11., 7., 4.5), 0.12),
            symbol,
        ]
    };
    let stick = |direction: &[&str]| {
        let mut v = vec![
            op(
                circle(16., 16., 12.),
                Some("#1a1e24"),
                Some((RIM, 1.5)),
                false,
            ),
            op(
                circle(16., 16., 8.6),
                Some(SLATE),
                Some(("#7d8da3", 1.)),
                false,
            ),
        ];
        for d in direction {
            if let Some(path) = stick_arrow(d) {
                v.push(op(path.into(), Some("#c4d6ef"), Some((RIM, 0.8)), false));
            }
        }
        v
    };
    let dpad = |lit: Option<&str>| {
        let mut v = vec![op(DPAD.into(), Some("#68728a"), Some((RIM, 1.6)), true)];
        match lit.and_then(dpad_arm) {
            Some(arm) => v.push(op(arm.into(), Some("#c4d6ef"), None, false)),
            None => v.push(op(circle(16., 16., 2.4), Some("#4b5466"), None, false)),
        }
        v
    };
    match g.kind.as_str() {
        "cross" => face(op(
            "M10.5 10.5l11 11M21.5 10.5l-11 11".into(),
            None,
            Some(("#9cbde4", 2.4)),
            true,
        )),
        "circle" => face(op(
            circle(16., 16., 6.4),
            None,
            Some(("#d888b8", 2.3)),
            false,
        )),
        "square" => face(op(
            rect(10.4, 10.4, 11.2, 11.2, 0.),
            None,
            Some(("#b890c8", 2.2)),
            false,
        )),
        "triangle" => face(op(
            "M16 9.6l6.6 11.6H9.4z".into(),
            None,
            Some(("#58c0c0", 2.2)),
            true,
        )),
        "l1" | "r1" => vec![
            op(
                rect(3., 7., 26., 19., 3.),
                Some("#14171b"),
                Some((RIM, 1.5)),
                false,
            ),
            op(rect(5.5, 9., 21., 14.5, 2.), Some(SHOULDER), None, false),
            translucent(rect(6.5, 9.8, 19., 3., 1.5), 0.18),
        ],
        "l2" | "r2" => vec![
            op(
                "M4 6h24v14c0 5-5 8-12 8S4 25 4 20z".into(),
                Some("#14171b"),
                Some((RIM, 1.5)),
                false,
            ),
            op(
                "M6.5 8h19v11.5c0 3.6-4 6-9.5 6s-9.5-2.4-9.5-6z".into(),
                Some(SHOULDER),
                None,
                false,
            ),
        ],
        "start" => vec![op(
            "M7 7.5l19 8.5-19 8.5z".into(),
            Some(SLATE),
            Some((RIM, 1.6)),
            true,
        )],
        "select" => vec![op(
            rect(5., 11., 22., 10., 5.),
            Some(SLATE),
            Some((RIM, 1.6)),
            false,
        )],
        "l3" | "r3" => stick(&[]),
        "stick" => stick(&[g.direction.as_str()]),
        "stick-pair" => stick(if g.direction == "x" {
            &["left", "right"]
        } else {
            &["up", "down"]
        }),
        "dpad" => dpad(None),
        "home" => vec![
            op(circle(16., 16., 11.), Some(SLATE), Some((RIM, 1.6)), false),
            op(
                "M10 17l6-5.5 6 5.5v5h-12z".into(),
                Some("#c4d6ef"),
                None,
                false,
            ),
        ],
        kind if kind.starts_with("dpad-") => dpad(Some(&kind[5..])),
        _ => vec![op(
            rect(1.5, 7., 29., 18., 4.),
            Some("#2b3139"),
            Some((RIM, 1.5)),
            false,
        )],
    }
}

/// The image key: glyphs sharing shapes share one raster.
pub fn image_key(g: &Glyph) -> String {
    match g.kind.as_str() {
        "stick" => format!("stick-{}", g.direction),
        "stick-pair" => format!("stick-pair-{}", g.direction),
        "l3" | "r3" => "stick-".into(),
        "raw" => "raw".into(),
        k => k.into(),
    }
}

/// The glyph shape at `px` square pixels.
pub fn raster(g: &Glyph, px: u32) -> Rgba {
    let px = px.clamp(8, 256);
    raster_ops(&ops(g), px, px, px as f32 / 32.)
}

/// pad-glyphs.js `GROUPS`: the summary panel's sections (only the port's
/// actions; ids pair for Steering, Lean and Orbit Camera).
/// A summary line: the action ids (two for a pair) and its label.
pub type Item = (&'static [&'static str], &'static str);
/// One summary section's chords per label.
pub type Lines = Vec<(String, Vec<Vec<Glyph>>)>;

pub const GROUPS: [(&str, &[Item]); 3] = [
    (
        "ATV Controls",
        &[
            (&["pause"], "Pause Menu"),
            (&["steerLeft", "steerRight"], "Steering"),
            (&["leanForward", "leanBack"], "Lean"),
            (&["throttle"], "Accelerate"),
            (&["brake"], "Brake"),
            (&["reverse"], "Reverse"),
            (&["reset"], "Reset ATV"),
            (&["preloadHold"], "Preload"),
            (&["clutch"], "Motorcycle clutch"),
        ],
    ),
    (
        "Stunt Controls",
        &[
            (&["stuntCircle"], "Stunt"),
            (&["stuntTriangle"], "Stunt"),
            (&["stuntL1"], "Stunt combo"),
        ],
    ),
    (
        "Camera Controls",
        &[
            (&["camera"], "Change Camera"),
            (&["cameraLeft", "cameraRight"], "Orbit Camera"),
        ],
    ),
];

/// `pairEntries`: a matched pair (one stick axis both ways, or opposite
/// D-pad arms) collapses to one glyph; the rest keep their own labels
/// (`None` uses the pair's label, else the first or second row's label).
pub fn pair_entries(
    first: &[Binding],
    second: &[Binding],
    standard: bool,
) -> Vec<(Option<usize>, Vec<Glyph>)> {
    let mut entries = Vec::new();
    let mut used = vec![false; second.len()];
    for a in first {
        let matched = match a {
            Binding::One(x) => second.iter().enumerate().position(|(j, b)| {
                !used[j]
                    && matches!(b, Binding::One(y) if match (x, y) {
                        (Atom::Axis(ax, sa), Atom::Axis(bx, sb)) => ax == bx && *sa == -*sb,
                        (Atom::Button(p), Atom::Button(q)) => standard
                            && [(14, 15), (15, 14), (12, 13), (13, 12)].contains(&(*p, *q)),
                        _ => false,
                    })
            }),
            Binding::Chord(_) => None,
        };
        match (matched, a) {
            (Some(j), Binding::One(atom)) => {
                used[j] = true;
                match atom {
                    Atom::Axis(axis, _) => {
                        let info = describe(atom, standard);
                        if info.kind == "stick" {
                            entries.push((
                                None,
                                vec![
                                    Glyph {
                                        direction: if axis % 2 == 0 { "x" } else { "y" }.into(),
                                        name: format!(
                                            "{} stick",
                                            if info.stick == "left" {
                                                "Left"
                                            } else {
                                                "Right"
                                            }
                                        ),
                                        ..glyph("stick-pair", String::new())
                                    }
                                    .with_stick(&info.stick),
                                ],
                            ));
                        } else {
                            let other = second[j].atoms()[0].clone();
                            entries.push((None, vec![info, describe(&other, standard)]));
                        }
                    }
                    _ => entries.push((None, vec![glyph("dpad", "D-pad".into())])),
                }
            }
            _ => entries.push((
                Some(0),
                a.atoms().iter().map(|x| describe(x, standard)).collect(),
            )),
        }
    }
    for (j, b) in second.iter().enumerate() {
        if !used[j] {
            entries.push((
                Some(1),
                b.atoms().iter().map(|x| describe(x, standard)).collect(),
            ));
        }
    }
    entries
}

impl Glyph {
    fn with_stick(mut self, stick: &str) -> Self {
        self.stick = stick.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_like_pad_glyphs_js() {
        assert_eq!(describe(&Atom::Button(0), true).kind, "cross");
        assert_eq!(describe(&Atom::Button(7), true).name, "R2");
        let raw = describe(&Atom::Button(20), true);
        assert_eq!((raw.kind.as_str(), raw.short.as_str()), ("raw", "B20"));
        let s = describe(&Atom::Axis(0, -1), true);
        assert_eq!((s.stick.as_str(), s.direction.as_str()), ("left", "left"));
        assert_eq!(describe(&Atom::Axis(3, 1), false).short, "A3+");
    }

    #[test]
    fn pairs_collapse_to_one_glyph() {
        let left = [
            Binding::One(Atom::Axis(0, -1)),
            Binding::One(Atom::Button(14)),
        ];
        let right = [
            Binding::One(Atom::Axis(0, 1)),
            Binding::One(Atom::Button(15)),
        ];
        let entries = pair_entries(&left, &right, true);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1[0].kind, "stick-pair");
        assert_eq!(entries[1].1[0].kind, "dpad");
    }

    #[test]
    fn rasterizes_a_face_button() {
        let image = raster(&describe(&Atom::Button(0), true), 64);
        let at = |x: u32, y: u32| image.data[((y * 64 + x) * 4 + 3) as usize];
        assert_eq!(at(0, 0), 0);
        assert_eq!(at(32, 8), 255);
    }
}

/// The controller bindings page's glyphs: each `bind:` row's alternatives
/// and the summary panel's sections (title, lines of label and chords).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadGlyphs {
    pub rows: Vec<(String, Vec<Vec<Glyph>>)>,
    pub summary: Vec<(String, Lines)>,
}

impl PadGlyphs {
    /// From the live controller profile (`gameplayBindings.rows`).
    pub fn new(bindings: &atv_game::ride::bindings::Bindings, standard: bool) -> Self {
        use atv_game::ride::bindings::ACTIONS;
        let row = |id: &str| {
            ACTIONS
                .iter()
                .find(|(_, i, _)| *i == id)
                .map(|(action, _, label)| (bindings.row(false, *action), *label))
        };
        let rows = ACTIONS
            .iter()
            .map(|(action, id, _)| {
                (
                    format!("bind:{id}"),
                    binding_glyphs(bindings.row(false, *action), standard),
                )
            })
            .collect();
        let mut summary = Vec::new();
        for (title, items) in GROUPS {
            let mut lines: Lines = Vec::new();
            let mut add = |glyphs: Vec<Glyph>, label: &str| match lines
                .iter_mut()
                .find(|(l, _)| l == label)
            {
                Some((_, list)) => list.push(glyphs),
                None => lines.push((label.into(), vec![glyphs])),
            };
            for (ids, label) in items.iter() {
                if let [a, b] = ids {
                    let (Some((first, first_label)), Some((second, second_label))) =
                        (row(a), row(b))
                    else {
                        continue;
                    };
                    for (suffix, glyphs) in pair_entries(first, second, standard) {
                        let label = match suffix {
                            None => *label,
                            Some(0) => first_label,
                            Some(_) => second_label,
                        };
                        add(glyphs, label);
                    }
                } else if let Some((bound, _)) = row(ids[0]) {
                    for b in bound {
                        add(
                            b.atoms().iter().map(|x| describe(x, standard)).collect(),
                            label,
                        );
                    }
                }
            }
            if !lines.is_empty() {
                summary.push(((*title).to_string(), lines));
            }
        }
        Self { rows, summary }
    }

    pub fn row(&self, action: &str) -> Option<&Vec<Vec<Glyph>>> {
        self.rows.iter().find(|(a, _)| a == action).map(|(_, g)| g)
    }

    fn all(&self) -> impl Iterator<Item = &Glyph> {
        self.rows
            .iter()
            .flat_map(|(_, r)| r.iter().flatten())
            .chain(self.summary.iter().flat_map(|(_, lines)| {
                lines.iter().flat_map(|(_, chords)| chords.iter().flatten())
            }))
    }
}

/// Glyph rasters registered with a UI renderer, one per shape and scale.
#[derive(Default)]
pub struct GlyphImages {
    images: std::collections::HashMap<String, crate::paint::ImageId>,
    px: u32,
}

impl GlyphImages {
    pub fn prepare(
        &mut self,
        renderer: &mut crate::render::Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pad: Option<&PadGlyphs>,
        scale: f32,
    ) {
        let Some(pad) = pad else {
            return;
        };
        let px = (32. * scale.max(1.)).ceil() as u32;
        if px != self.px {
            for (_, id) in self.images.drain() {
                renderer.remove_image(id);
            }
            self.px = px;
        }
        for g in pad.all() {
            let key = image_key(g);
            if let std::collections::hash_map::Entry::Vacant(slot) = self.images.entry(key) {
                let image = raster(g, px);
                slot.insert(renderer.add_image(
                    device,
                    queue,
                    image.width,
                    image.height,
                    &image.data,
                ));
            }
        }
    }

    pub fn get(&self, g: &Glyph) -> Option<crate::paint::ImageId> {
        self.images.get(&image_key(g)).copied()
    }

    /// The renderer was replaced: its images are gone.
    pub fn clear(&mut self) {
        self.images.clear();
        self.px = 0;
    }
}
