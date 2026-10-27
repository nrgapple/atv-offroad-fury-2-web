//! The original single-player race overlays (web/hud.js `drawRace`) and the
//! 321 LIGHTS start lamps: LAP TIME, LAP, SCORE, the PLACE name tree or
//! number, BEST/TOTAL/LEAD, the lap history, info text and signs, and the
//! north-up radar, laid out on the original 640x448 frame. Values and
//! animation clocks come from atv_core::race / race_hud.
use super::assets::{RACER, RACER_PLAYER, Section};
use super::meter::{Logical, Meter};
use crate::{
    color::Color,
    paint::{DisplayList, Item, Rect},
    settings::Settings,
    text::Fonts,
};
use atv_core::race::Frame;

/// 002e0770: overlay text without a colour draws 190,190,190.
const RACE_TEXT: Color = Color::rgb(190, 190, 190);
const RACE_GOLD: Color = Color::rgb(218, 192, 31);

/// Original MM:SS.hh (rounded hundredths); 0 shows dashes (`overlayTime`).
pub fn overlay_time(t: f32) -> String {
    if t == 0. || !t.is_finite() {
        return "--:--.--".into();
    }
    if t > 5999.99 {
        return "99:99.99".into();
    }
    if t < -5999.99 {
        return "-99:99.99".into();
    }
    let h = (f64::from(t.abs()) * 100. + 0.5).floor() as u64;
    format!(
        "{}{:02}:{:02}.{:02}",
        if t < 0. { "-" } else { "" },
        h / 6000,
        (h / 100) % 60,
        h % 100
    )
}

/// Pause → Race Overlays (002df120): hidden groups and the position style,
/// saved under the browser's keys (`RaceOverlaySettings` in hud.js).
#[derive(Clone, Debug, PartialEq)]
pub struct OverlaySettings {
    pub off: Vec<String>,
    /// `name` (the PLACE tree), `place` (the number) or `off`.
    pub position: String,
}

pub const GROUPS: [&str; 6] = ["timing", "lap", "score", "info", "speed", "radar"];

impl OverlaySettings {
    pub fn load(settings: &Settings) -> Self {
        let off = settings
            .get("atv-race-overlays-off")
            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|g| GROUPS.contains(&g.as_str()))
            .collect();
        let position = settings
            .get("atv-race-position")
            .filter(|p| ["name", "place", "off"].contains(&p.as_str()))
            .unwrap_or_else(|| "name".into());
        Self { off, position }
    }

    pub fn shown(&self, group: &str) -> bool {
        !self.off.iter().any(|g| g == group)
    }
}

/// What one frame's race overlays show.
pub struct RaceView<'a> {
    pub frame: &'a Frame,
    /// The player's committed stunt points (SCORE).
    pub score: f32,
    /// The drawn player origin (the radar's red dot).
    pub origin: [f32; 3],
    /// The toolbar's bottom edge in CSS pixels (top items sit below it).
    pub top_inset: f32,
    pub settings: &'a OverlaySettings,
}

impl Meter {
    /// Enduro compass (`QuadzillaCompass`, `002e6e80`): a heading-up face
    /// (turned by −yaw), the player's arrow fixed at the centre, the other
    /// riders clamped to the 33px rim (10.09 ft per pixel, single player) and
    /// the next gate's icon, turned to the gate's direction, only within it.
    fn compass(
        &self,
        list: &mut DisplayList,
        frame: &Frame,
        origin: [f32; 3],
        center: [f32; 2],
        unit: f32,
    ) {
        const FEET_PER_PIXEL: f32 = 333. / 33.;
        const RADIUS: f32 = 33.;
        let [face, arrow, gate] = super::assets::COMPASS;
        let (h, z) = (frame.yaw.sin(), frame.yaw.cos());
        // angle(h, t): positive toward the rider's right, clockwise on screen.
        let angle = |t: [f32; 2]| (z * t[0] - h * t[1]).atan2(h * t[0] + z * t[1]);
        let sprite = |list: &mut DisplayList, name: &str, at: [f32; 2], rotation: f32| {
            let (Some(&id), Some(image)) = (self.images.get(name), self.assets.images.get(name))
            else {
                return;
            };
            let (w, h) = (image.width as f32, image.height as f32);
            list.push(Item::Image {
                image: id,
                src: Rect::new(0., 0., w, h),
                dst: Rect::new(
                    (at[0] - w / 2.) * unit,
                    (at[1] - h / 2.) * unit,
                    w * unit,
                    h * unit,
                ),
                rotation,
                pivot: [at[0] * unit, at[1] * unit],
            });
        };
        sprite(list, face, center, -frame.yaw);
        let place = |p: [f32; 3], clamp: bool| {
            let d = [p[0] - origin[0], p[2] - origin[2]];
            let r = (d[0].hypot(p[1] - origin[1]).hypot(d[1])) / FEET_PER_PIXEL;
            let r = if clamp { r.min(RADIUS) } else { r };
            let a = angle(d);
            (r, [center[0] + r * a.sin(), center[1] - r * a.cos()])
        };
        for cpu in &frame.opponents {
            let (_, at) = place(cpu.origin, true);
            sprite(list, super::assets::RACER, at, 0.);
        }
        if let Some((position, direction)) = frame.enduro_next {
            let (r, at) = place(position, false);
            if r < RADIUS {
                let gate_yaw = direction[0].atan2(direction[1]);
                sprite(list, gate, at, gate_yaw - frame.yaw);
            }
        }
        sprite(list, arrow, center, 0.);
    }

    /// Race overlays for a `width` x `height` CSS-pixel viewport.
    pub fn paint_race(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        race: &RaceView,
        width: f32,
        height: f32,
    ) {
        let Some(assets) = &self.assets.race else {
            return;
        };
        let shown = |group: &str| race.settings.shown(group);
        let position = race.settings.position.as_str();
        // Phones keep a 480-unit minimum width and drop the right column
        // below the compact speed meter.
        let compact = width < 560.;
        let unit = (height / 448.).min(width / 480.);
        let (w, h, top) = (width / unit, height / unit, race.top_inset / unit);
        let meter = if compact { 118. / unit } else { 0. };
        let ax = |x: f32| {
            if x <= 213. {
                x
            } else if x >= 427. {
                w - (640. - x)
            } else {
                x + (w - 640.) / 2. - if compact { 30. } else { 0. }
            }
        };
        let ay = |y: f32, x: f32| {
            if y < 224. {
                y + top + if x >= 427. { meter } else { 0. }
            } else {
                h - (448. - y)
            }
        };
        let at = |x: f32, y: f32| [ax(x), ay(y, x)];
        let f = Logical {
            unit,
            logical_width: w,
        };
        let frame = race.frame;
        let fallback = atv_core::race_hud::Frame {
            sign: None,
            infotext: None,
            history: None,
            listing_alpha: 1.,
            tree: Vec::new(),
            place_scale: 1.,
        };
        let hud = frame.hud.as_ref().unwrap_or(&fallback);
        let layout = |n: u32| assets.layout.get(&n);
        let mut text = |list: &mut DisplayList,
                        value: &str,
                        where_: [f32; 2],
                        layout: &Section,
                        i: usize,
                        color: Option<Color>,
                        scale: f32,
                        alpha: Option<f32>,
                        offset: [f32; 2]| {
            let color = color.or_else(|| {
                Some(
                    layout
                        .get(&format!("text{i}color"))
                        .and_then(Color::from_triplet)
                        .unwrap_or(RACE_TEXT),
                )
            });
            self.text(
                fonts, list, &f, value, where_, layout, i, scale, color, alpha, offset,
            );
        };
        // LAP TIME, LAP, SCORE.
        if let Some(l) = layout(1).filter(|_| shown("timing")) {
            text(
                list,
                "LAP TIME",
                at(609., 23.),
                l,
                1,
                None,
                1.,
                None,
                [0.; 2],
            );
            let value = overlay_time(frame.lap_time);
            text(list, &value, at(609., 23.), l, 2, None, 1., None, [0.; 2]);
        }
        if let Some(l) = layout(2).filter(|_| shown("lap")) {
            text(list, "LAP", at(354., 23.), l, 1, None, 1., None, [0.; 2]);
            let value = format!("{} / {}", frame.lap, frame.laps);
            text(list, &value, at(354., 23.), l, 2, None, 1., None, [0.; 2]);
        }
        if let Some(l) = layout(3).filter(|_| shown("score")) {
            text(list, "SCORE", at(178., 23.), l, 1, None, 1., None, [0.; 2]);
            let value = format!("{}", (race.score + 0.5).floor());
            text(list, &value, at(178., 23.), l, 2, None, 1., None, [0.; 2]);
        }
        // Enduro GATE indicator (Overlay Set 11 object 40, value 22
        // `002df0b0` "%d of %d").
        if let (Some(l), Some([gate, gates])) = (layout(40), frame.enduro_gate)
            && shown("lap")
        {
            text(list, "GATE", at(25., 270.), l, 1, None, 1., None, [0.; 2]);
            let value = format!("{gate} of {gates}");
            text(list, &value, at(25., 270.), l, 2, None, 1., None, [0.; 2]);
        }
        // Position: the name tree (object 4) or the place number (object 41).
        if let Some(l) = layout(41).filter(|_| position == "place") {
            text(list, "PLACE", at(20., 23.), l, 1, None, 1., None, [0.; 2]);
            let value = if frame.position > 0 {
                format!("{}/{}", frame.position, frame.field)
            } else {
                "---".into()
            };
            text(list, &value, at(20., 23.), l, 2, None, 1., None, [0.; 2]);
        }
        if let Some(tree) = layout(4).filter(|_| position == "name") {
            // PLACE position tree: each rider slot's name slides to its rank.
            let names: Vec<String> = std::iter::once("YOU".to_string())
                .chain(frame.opponents.iter().map(|o| o.name.clone()))
                .map(|n| n.chars().take(9).collect())
                .collect();
            text(
                list,
                "PLACE",
                at(20., 23.),
                tree,
                1,
                None,
                1.,
                None,
                [0.; 2],
            );
            for rank in 1..=names.len().min(5) {
                let player = frame.position == rank;
                text(
                    list,
                    &rank.to_string(),
                    at(20., 23.),
                    tree,
                    rank + 1,
                    Some(if player { RACE_GOLD } else { RACE_TEXT }),
                    if player { hud.place_scale } else { 1. },
                    None,
                    [0.; 2],
                );
            }
            for (slot, name) in names.iter().take(5).enumerate() {
                let y = hud.tree.get(slot).copied().unwrap_or_else(|| {
                    (frame.ranks.get(slot).copied().unwrap_or(1) as f32 - 1.) * 23.
                });
                text(
                    list,
                    name,
                    at(20., 23.),
                    tree,
                    7,
                    Some(if slot == 0 { RACE_GOLD } else { RACE_TEXT }),
                    1.,
                    None,
                    [0., y],
                );
            }
        }
        // BEST/TOTAL/LEAD, faded by the lap history; LEAD green ahead, red behind.
        let alpha = 255. * hud.listing_alpha;
        if let Some(listing) = layout(7).filter(|_| alpha > 0. && shown("timing")) {
            for (i, label) in ["BEST:", "TOTAL:", "LEAD:"].iter().enumerate() {
                text(
                    list,
                    label,
                    at(609., 23.),
                    listing,
                    i + 1,
                    None,
                    1.,
                    Some(alpha),
                    [0.; 2],
                );
            }
            let best = overlay_time(frame.best.unwrap_or(0.));
            text(
                list,
                &best,
                at(609., 23.),
                listing,
                4,
                None,
                1.,
                Some(alpha),
                [0.; 2],
            );
            let total = overlay_time(frame.elapsed);
            text(
                list,
                &total,
                at(609., 23.),
                listing,
                5,
                None,
                1.,
                Some(alpha),
                [0.; 2],
            );
            let lead = overlay_time(frame.lead);
            let color = if frame.lead >= 0. {
                Color::rgb(30, 215, 30)
            } else {
                Color::rgb(200, 60, 60)
            };
            text(
                list,
                &lead,
                at(609., 23.),
                listing,
                6,
                Some(color),
                1.,
                Some(alpha),
                [0.; 2],
            );
        }
        if let (Some(history), Some(rows)) = (layout(14), hud.history.as_ref())
            && shown("lap")
        {
            let where_ = [ax(609.) + rows.offset, ay(23., 609.)];
            let best_color = history
                .get("bestlapcolor")
                .map(|c| c.replace(char::is_whitespace, ""))
                .and_then(|c| Color::from_triplet(&c))
                .unwrap_or(Color::rgb(76, 181, 255));
            for (i, row) in rows.rows.iter().enumerate() {
                let color = Some(if row.best { best_color } else { RACE_TEXT });
                let lap = format!("Lap {}", row.lap);
                text(
                    list,
                    &lap,
                    where_,
                    history,
                    i * 2 + 1,
                    color,
                    1.,
                    None,
                    [0.; 2],
                );
                let time = overlay_time(row.time);
                text(
                    list,
                    &time,
                    where_,
                    history,
                    i * 2 + 2,
                    color,
                    1.,
                    None,
                    [0.; 2],
                );
            }
        }
        // Infotext and signage.
        if let (Some(info), Some(l)) = (hud.infotext.as_deref(), layout(11))
            && shown("info")
        {
            text(list, info, at(320., 304.), l, 1, None, 1., None, [0.; 2]);
        }
        if let Some(sign_frame) = hud.sign.as_ref().filter(|_| shown("info"))
            && let Some(sign) = assets.signs.get(&sign_frame.id)
        {
            // Phones show signs below the stacked top items.
            let mut where_ = at(320., 80.);
            if compact {
                where_[1] += meter + 120.;
            }
            self.icon(list, &f, where_, sign, 1, None, None);
            let counted = sign_frame.digit.is_some();
            if let Some(digit) = sign_frame.digit {
                text(
                    list,
                    &digit.to_string(),
                    where_,
                    sign,
                    1,
                    None,
                    1.,
                    None,
                    [0.; 2],
                );
            }
            let index = if counted { 2 } else { 1 };
            if let Some(label) = sign.text(&format!("text{index}text")) {
                text(list, &label, where_, sign, index, None, 1., None, [0.; 2]);
            }
        }
        // Radar: north-up course map at native size, racer dots drawn last-first.
        if !shown("radar") {
            return;
        }
        if frame.enduro_gate.is_some() {
            self.compass(list, frame, race.origin, at(70., 375.), unit);
            return;
        }
        let Some((map, [left, north, right, _])) = &assets.radar else {
            return;
        };
        let (Some(&map_id), Some(map_image)) = (self.images.get(map), self.assets.images.get(map))
        else {
            return;
        };
        let (mw, mh) = (map_image.width as f32, map_image.height as f32);
        let scale = (right - left) / mw;
        let origin = at(20., 305.);
        let image = |list: &mut DisplayList, id, src: Rect, x: f32, y: f32, w: f32, h: f32| {
            list.push(Item::Image {
                image: id,
                src,
                dst: Rect::new(x * unit, y * unit, w * unit, h * unit),
                rotation: 0.,
                pivot: [0., 0.],
            });
        };
        image(
            list,
            map_id,
            Rect::new(0., 0., mw, mh),
            origin[0],
            origin[1],
            mw,
            mh,
        );
        let dot = |list: &mut DisplayList, p: [f32; 3], name: &str| {
            let (Some(&id), Some(icon)) = (self.images.get(name), self.assets.images.get(name))
            else {
                return;
            };
            let x = (((p[0] - left) / scale).trunc()).clamp(0., (mw - 8.).max(0.));
            let y = (((north - p[2]) / scale).trunc()).clamp(0., (mh - 8.).max(0.));
            let src = Rect::new(0., 0., icon.width as f32, icon.height as f32);
            image(list, id, src, origin[0] + x, origin[1] + y, 8., 8.);
        };
        for cpu in frame.opponents.iter().rev() {
            dot(list, cpu.origin, RACER);
        }
        dot(list, race.origin, RACER_PLAYER);
    }

    /// The course archive carries the 321 lamp art.
    pub fn has_lamps(&self) -> bool {
        self.assets.lamps
    }

    /// 321 LIGHTS: four 32x32 lamps from (256,208) in 640x448, centred on
    /// wide screens; three red lamps light one per second, then green at GO.
    pub fn paint_lamps(&self, list: &mut DisplayList, lights: u8, width: f32, height: f32) {
        if !self.assets.lamps {
            return;
        }
        let compact = width < 560.;
        let unit = if compact {
            (width / 400.).min(height / 448.)
        } else {
            height / 448.
        };
        let (w, h) = (width / unit, height / unit);
        let (left, top) = (w / 2. - 64., h * 208. / 448.);
        for lamp in 0..4u8 {
            let name = if lamp < 3 {
                if lights > lamp { "321ron" } else { "321roff" }
            } else if lights >= 4 {
                "321gon"
            } else {
                "321goff"
            };
            let (Some(&id), Some(image)) = (self.images.get(name), self.assets.images.get(name))
            else {
                continue;
            };
            list.push(Item::Image {
                image: id,
                src: Rect::new(0., 0., image.width as f32, image.height as f32),
                dst: Rect::new(
                    (left + f32::from(lamp) * 32.) * unit,
                    top * unit,
                    32. * unit,
                    32. * unit,
                ),
                rotation: 0.,
                pivot: [0., 0.],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_time_matches_hud_js() {
        assert_eq!(overlay_time(0.), "--:--.--");
        assert_eq!(overlay_time(65.432), "01:05.43");
        assert_eq!(overlay_time(59.996), "01:00.00");
        assert_eq!(overlay_time(-3.5), "-00:03.50");
        assert_eq!(overlay_time(6000.), "99:99.99");
    }

    #[test]
    fn overlay_settings_read_the_browser_keys() {
        let mut settings = Settings::memory();
        settings.set("atv-race-overlays-off", r#"["radar","bogus"]"#);
        settings.set("atv-race-position", "place");
        let s = OverlaySettings::load(&settings);
        assert!(!s.shown("radar") && s.shown("timing"));
        assert_eq!(s.position, "place");
    }
}
