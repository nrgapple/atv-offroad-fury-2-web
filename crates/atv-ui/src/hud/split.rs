//! The split-screen HUD (research/SPLIT_SCREEN.md): each player's
//! instruments at the original `OverlaySet<n>-ScreenLayout-<layout>`
//! positions (Overlays.pb, read by `002edf00`) instead of the single-screen
//! HUD scaled into the view. Positions are on the 640x448 frame; each
//! player's part of that frame maps onto its view, scaled by height with
//! wide views keeping left-third items on the left, right-third items on
//! the right and the middle centred (the single-screen rule of
//! `paint_race`).
use super::assets::{Placed, SplitLayout};
use super::meter::{Logical, Meter};
use super::race::{CompassView, RACE_TEXT, RaceView, overlay_time};
use crate::{color::Color, paint::DisplayList, text::Fonts};
use atv_core::hud::Frame;

/// The layout name of `players` local players (`2Player-Horiz` and
/// `-Vertical` for two, else `3Player` / `4Player`).
pub fn layout_name(players: usize, vertical: bool) -> &'static str {
    match players {
        0..=2 if vertical => "2Player-Vertical",
        0..=2 => "2Player-Horiz",
        3 => "3Player",
        _ => "4Player",
    }
}

/// Player `player`'s (0-based) part of the 640x448 frame: top/bottom
/// halves, left/right halves, or quarters left to right, top to bottom
/// (the order of each overlay's `Overlay1`..`Overlay4`).
pub fn part(layout: &str, player: usize) -> [f32; 4] {
    match layout {
        "2Player-Horiz" => [0., 224. * player as f32, 640., 224.],
        "2Player-Vertical" => [320. * player as f32, 0., 320., 448.],
        _ => [
            320. * (player % 2) as f32,
            224. * (player / 2) as f32,
            320.,
            224.,
        ],
    }
}

/// One player's seat in a split layout.
pub struct Seat<'a> {
    pub layout: &'a SplitLayout,
    /// The layout name ([`layout_name`]).
    pub name: &'a str,
    /// 0-based player.
    pub player: usize,
    /// CSS pixels the toolbar takes from the top of this view's upper
    /// right items (the top-right view only).
    pub top_inset: f32,
}

/// What a seat's instruments show this frame.
pub struct SeatFrame<'a> {
    pub hud: &'a Frame,
    pub race: Option<&'a RaceView<'a>>,
    pub compass: Option<&'a CompassView>,
    pub speed_visible: bool,
}

/// The view mapping of one seat: CSS px per frame unit and the view's
/// logical size.
struct Mapping {
    unit: f32,
    part: [f32; 4],
    width: f32,
    height: f32,
    inset: f32,
}

impl Mapping {
    fn new(part: [f32; 4], width: f32, height: f32, top_inset: f32) -> Self {
        let unit = (height / part[3]).min(width / part[2]).max(1e-3);
        Self {
            unit,
            part,
            width: width / unit,
            height: height / unit,
            inset: top_inset / unit,
        }
    }

    /// A frame position in this view's logical units.
    fn at(&self, p: [f32; 2]) -> [f32; 2] {
        let [px, py, pw, ph] = self.part;
        let (x, y) = (p[0] - px, p[1] - py);
        let x = if x <= pw / 3. {
            x
        } else if x >= pw * 2. / 3. {
            self.width - (pw - x)
        } else {
            x + (self.width - pw) / 2.
        };
        let y = if y < ph / 2. {
            y + if x >= self.width - pw / 3. {
                self.inset
            } else {
                0.
            }
        } else {
            self.height - (ph - y)
        };
        [x, y]
    }
}

/// One racer on the shared split radar: its world origin and its colour
/// slot (0-3 the human players' red, blue, green and yellow; none for a
/// CPU rider, drawn white).
pub struct RadarDot {
    pub origin: [f32; 3],
    pub player: Option<u8>,
}

impl Meter {
    /// The race radar of a split layout (object 13, one overlay the whole
    /// frame shares: `2Player-Horiz` 20,165, `-Vertical` 256,305, three
    /// and four players 251,159 over the views' crossing). The north-up
    /// course map draws at its native size like the single-screen radar;
    /// every racer is a dot, CPUs white and each human in its player
    /// colour (`002e6160`), drawn last to first so player 1 stays on top.
    /// Returns the map's top-left corner (CSS px) when drawn.
    pub fn paint_split_radar(
        &self,
        list: &mut DisplayList,
        layout: &SplitLayout,
        dots: &[RadarDot],
        width: f32,
        height: f32,
    ) -> Option<[f32; 2]> {
        use super::assets::{RACER, RACER_PLAYER, tinted_name};
        use crate::paint::{Item, Rect};
        let placed = layout.shared.iter().find(|p| p.object == 13)?;
        let (map, [left, north, right, _]) = self.assets.race.as_ref()?.radar.as_ref()?;
        let (&map_id, map_image) = (self.images.get(map)?, self.assets.images.get(map)?);
        let m = Mapping::new([0., 0., 640., 448.], width, height, 0.);
        let origin = m.at(placed.at);
        let (mw, mh) = (map_image.width as f32, map_image.height as f32);
        let scale = (right - left) / mw;
        let unit = m.unit;
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
        for dot in dots.iter().rev() {
            let name = match dot.player {
                None => RACER.to_string(),
                Some(0) => RACER_PLAYER.to_string(),
                Some(slot) => tinted_name(RACER, slot),
            };
            let (Some(&id), Some(icon)) = (self.images.get(&name), self.assets.images.get(&name))
            else {
                continue;
            };
            let p = dot.origin;
            let x = ((p[0] - left) / scale).trunc().clamp(0., (mw - 8.).max(0.));
            let y = ((north - p[2]) / scale)
                .trunc()
                .clamp(0., (mh - 8.).max(0.));
            let src = Rect::new(0., 0., icon.width as f32, icon.height as f32);
            image(list, id, src, origin[0] + x, origin[1] + y, 8., 8.);
        }
        Some([origin[0] * unit, origin[1] * unit])
    }

    /// The split layout of overlay set `set`, when Overlays.pb listed it.
    pub fn split_layout(&self, set: u32, name: &str) -> Option<&SplitLayout> {
        self.assets
            .split
            .iter()
            .find(|((s, n), _)| *s == set && *n == name)
            .map(|(_, l)| l)
    }

    /// Paints one seat's instruments for its `width` x `height` CSS-pixel
    /// view and returns each drawn object with its anchor (CSS px in the
    /// view, `#ui-state` for tests).
    pub fn paint_seat(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        seat: &Seat,
        frame: &SeatFrame,
        width: f32,
        height: f32,
    ) -> Vec<(u32, [f32; 2])> {
        let mut drawn = Vec::new();
        let Some(overlays) = seat.layout.players.get(seat.player) else {
            return drawn;
        };
        let m = Mapping::new(part(seat.name, seat.player), width, height, seat.top_inset);
        let f = Logical {
            unit: m.unit,
            logical_width: m.width,
        };
        let shown = |group: &str| frame.race.is_none_or(|r| r.settings.shown(group));
        for placed in overlays {
            let at = m.at(placed.at);
            let painted = match placed.object {
                5 | 33 if frame.speed_visible && shown("speed") => {
                    self.seat_speed(fonts, list, &f, placed, at, frame.hud);
                    true
                }
                12 | 24 if shown("info") => {
                    self.seat_stunt(fonts, list, &f, placed, at, frame.hud);
                    frame.hud.stunt_visible
                }
                31 => match frame.compass.filter(|_| self.has_compass()) {
                    Some(compass) if frame.race.is_none() => {
                        // The 128-unit face at its layout position would
                        // cross a half or quarter's edge (60,177 in a
                        // 224-unit quarter): it draws at the part's share
                        // of the frame height (an assumption; the
                        // compass's split scale was not traced).
                        let k = m.part[3] / 448.;
                        self.draw_compass(list, compass, None, [at[0] / k, at[1] / k], m.unit * k);
                        true
                    }
                    _ => false,
                },
                19..=23 | 35 | 41 => match frame.race {
                    Some(race) => self.seat_race(fonts, list, &f, placed, at, race),
                    None => false,
                },
                _ => false,
            };
            if painted {
                drawn.push((placed.object, [at[0] * m.unit, at[1] * m.unit]));
            }
        }
        drawn
    }

    fn seat_speed(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        f: &Logical,
        placed: &Placed,
        at: [f32; 2],
        hud: &Frame,
    ) {
        let section = &placed.section;
        // Native draw depth: background (5), blue (4), orange (2), highlight (1).
        self.icon(list, f, at, section, 2, None, None);
        if hud.preload_visible {
            let blue = Some(hud.preload_blue_alpha as f32);
            self.icon(list, f, at, section, 1, blue, None);
            self.icon(list, f, at, section, 4, None, Some(hud.preload_fill));
        }
        self.icon(list, f, at, section, 3, None, None);
        // Object 33 (three and four players) has NumText=0: no digits.
        let texts = section.number("numtext").unwrap_or(0.) as usize;
        for (index, value) in [(1, hud.speed_text.as_str()), (2, hud.units)] {
            if index <= texts {
                self.text(
                    fonts,
                    list,
                    f,
                    value,
                    at,
                    section,
                    index,
                    1.,
                    None,
                    None,
                    [0., 0.],
                );
            }
        }
    }

    fn seat_stunt(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        f: &Logical,
        placed: &Placed,
        at: [f32; 2],
        hud: &Frame,
    ) {
        if !hud.stunt_visible {
            return;
        }
        let [r, g, b] = hud.stunt_color.map(|c| c.clamp(0, 255) as u8);
        let color = Some(Color::rgb(r, g, b));
        let alpha = Some(hud.stunt_alpha as f32);
        let section = &placed.section;
        self.text(
            fonts,
            list,
            f,
            &hud.stunt_name,
            at,
            section,
            1,
            1.,
            color,
            alpha,
            hud.stunt_offsets[0],
        );
        self.text(
            fonts,
            list,
            f,
            &hud.stunt_points,
            at,
            section,
            2,
            hud.stunt_scale,
            color,
            alpha,
            hud.stunt_offsets[1],
        );
    }

    /// The multiplayer race objects: 19 LAP TIME, 20 LAP, 21 SCORE, 23
    /// infotext, 35 signage (`Multi-Sign-N`) and 41 PLACE.
    fn seat_race(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        f: &Logical,
        placed: &Placed,
        at: [f32; 2],
        race: &RaceView,
    ) -> bool {
        let section = &placed.section;
        let shown = |group: &str| race.settings.shown(group);
        let frame = race.frame;
        let mut text = |list: &mut DisplayList, value: &str, i: usize| {
            let color = section
                .get(&format!("text{i}color"))
                .and_then(Color::from_triplet)
                .unwrap_or(RACE_TEXT);
            self.text(
                fonts,
                list,
                f,
                value,
                at,
                section,
                i,
                1.,
                Some(color),
                None,
                [0.; 2],
            );
        };
        let label = |fallback: &str| section.text("text1text").unwrap_or(fallback.into());
        match placed.object {
            19 if shown("timing") => {
                text(list, &label("LAP TIME"), 1);
                text(list, &overlay_time(frame.lap_time), 2);
            }
            20 if shown("lap") => {
                text(list, &label("LAP"), 1);
                text(list, &format!("{} / {}", frame.lap, frame.laps), 2);
            }
            21 if shown("score") => {
                text(list, &label("SCORE"), 1);
                text(list, &format!("{}", (race.score + 0.5).floor()), 2);
            }
            41 => {
                text(list, &label("PLACE"), 1);
                let value = if frame.position > 0 {
                    format!("{}/{}", frame.position, frame.field)
                } else {
                    "---".into()
                };
                text(list, &value, 2);
            }
            23 if shown("info") => {
                let Some(info) = frame.hud.as_ref().and_then(|h| h.infotext.as_deref()) else {
                    return false;
                };
                text(list, info, 1);
            }
            35 if shown("info") => {
                let Some(sign_frame) = frame.hud.as_ref().and_then(|h| h.sign.as_ref()) else {
                    return false;
                };
                let Some(sign) = self
                    .assets
                    .race
                    .as_ref()
                    .and_then(|r| r.multi_signs.get(&sign_frame.id))
                else {
                    return false;
                };
                self.icon(list, f, at, sign, 1, None, None);
                let counted = sign_frame.digit.is_some();
                let mut sign_text = |list: &mut DisplayList, value: &str, i: usize| {
                    let color = sign
                        .get(&format!("text{i}color"))
                        .and_then(Color::from_triplet)
                        .unwrap_or(RACE_TEXT);
                    self.text(
                        fonts,
                        list,
                        f,
                        value,
                        at,
                        sign,
                        i,
                        1.,
                        Some(color),
                        None,
                        [0.; 2],
                    );
                };
                if let Some(digit) = sign_frame.digit {
                    sign_text(list, &digit.to_string(), 1);
                }
                let index = if counted { 2 } else { 1 };
                if let Some(label) = sign.text(&format!("text{index}text")) {
                    sign_text(list, &label, index);
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_follow_the_layout_player_order() {
        assert_eq!(part("2Player-Horiz", 1), [0., 224., 640., 224.]);
        assert_eq!(part("2Player-Vertical", 1), [320., 0., 320., 448.]);
        assert_eq!(part("4Player", 3), [320., 224., 320., 224.]);
        assert_eq!(part("3Player", 2), [0., 224., 320., 224.]);
        assert_eq!(layout_name(2, true), "2Player-Vertical");
        assert_eq!(layout_name(3, false), "3Player");
    }

    #[test]
    fn quarter_positions_map_onto_wide_views() {
        // A 1280x720 four-player frame: 640x360 quarters.
        let m = Mapping::new(part("4Player", 1), 640., 360., 0.);
        let unit = 360. / 224.;
        assert!((m.unit - unit).abs() < 1e-4);
        // Player 2's lap time at 615,23 keeps its 25-unit right margin.
        let at = m.at([615., 23.]);
        assert!((at[0] - (640. / unit - 25.)).abs() < 1e-3);
        assert!((at[1] - 23.).abs() < 1e-3);
        // Player 4's speed meter at 597,372 keeps its bottom-right margins.
        let m = Mapping::new(part("4Player", 3), 640., 360., 0.);
        let at = m.at([597., 372.]);
        assert!((at[0] - (640. / unit - 43.)).abs() < 1e-3);
        assert!((at[1] - (224. - 76.)).abs() < 1e-3);
        // The stunt text keeps its place 5 units left of the centre.
        let at = m.at([475., 385.]);
        assert!((at[0] - (640. / unit / 2. - 5.)).abs() < 1e-3);
    }

    #[test]
    fn split_layouts_read_each_players_positions() {
        let doc = b"[OverlaySets]\nNumOverlaySets=1\n[OverlaySet1]\nType=Race\n\
NumOverlays-ScreenLayout-4Player=2\n\
[OverlaySet1-ScreenLayout-4Player-Overlay1]\nOverlayObject=19\nText1Text=\"LAP TIME\"\n\
NumOverlay = 4\nOverlay1 = 20,23\nOverlay2 = 615,23\nOverlay3 = 20,231\nOverlay4 = 615,231\n\
AlterJustification=2\nOverlay1Text1Justification=L\n\
[OverlaySet1-ScreenLayout-4Player-Overlay2]\nOverlayObject=10\nNumOverlay = 1\nOverlay1 = 256,208\n\
[OverlayObject-19]\nType=Display\nNumText=2\nText1Justification = R\n\
[OverlayObject-10]\nType=Lights\n";
        let all = super::super::assets::sections(doc);
        let layouts = super::super::assets::split_layouts(&all);
        let layout = &layouts[&(1, "4Player")];
        assert_eq!(layout.players.len(), 4);
        assert_eq!(layout.players[1][0].at, [615., 23.]);
        assert_eq!(
            layout.players[0][0].section.get("text1justification"),
            Some("L")
        );
        assert_eq!(
            layout.players[1][0].section.get("text1justification"),
            Some("R")
        );
        assert_eq!(
            layout.players[2][0].section.text("text1text").as_deref(),
            Some("LAP TIME")
        );
        assert_eq!(layout.shared.len(), 1);
        assert_eq!(layout.shared[0].at, [256., 208.]);
    }

    #[test]
    fn the_split_radar_is_one_overlay_over_the_views_crossing() {
        let doc = b"[OverlaySet1]\nType=Race\nNumOverlays-ScreenLayout-4Player=1\n\
NumOverlays-ScreenLayout-2Player-Vertical=1\n\
[OverlaySet1-ScreenLayout-4Player-Overlay1]\nOverlayObject=13\nNumOverlay = 1\nOverlay1 = 251,159\n\
[OverlaySet1-ScreenLayout-2Player-Vertical-Overlay1]\nOverlayObject=13\nNumOverlay = 1\nOverlay1 = 256,305\n\
[OverlayObject-13]\nType=Radar\n";
        let all = super::super::assets::sections(doc);
        let layouts = super::super::assets::split_layouts(&all);
        let quad = &layouts[&(1, "4Player")];
        assert!(quad.players.iter().all(Vec::is_empty));
        assert_eq!(quad.shared[0].object, 13);
        // A 1280x720 frame: the radar keeps the middle column centred
        // (251 is 69 units left of the centre) and its 159-unit top.
        let m = Mapping::new([0., 0., 640., 448.], 1280., 720., 0.);
        let at = m.at(quad.shared[0].at);
        assert!((at[0] - (1280. / m.unit / 2. - 69.)).abs() < 1e-3);
        assert!((at[1] - 159.).abs() < 1e-3);
        // Side by side: bottom-anchored, 143 units above the bottom.
        let vertical = &layouts[&(1, "2Player-Vertical")];
        let at = m.at(vertical.shared[0].at);
        assert!((at[1] - (720. / m.unit - 143.)).abs() < 1e-3);
    }
}
