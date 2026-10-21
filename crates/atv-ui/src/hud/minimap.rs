//! Freestyle GPS minimap (web/minimap.js `FreestyleMinimap`, game.css
//! `#freestyle-minimap`): an orthographic 1024x1024 course map built once from
//! the original terrain textures and heights, a gold heading marker, north
//! lettering and the speed/preload footer.
use super::{Viewport, tokens};
use crate::settings::Settings;
use crate::{
    color::Color,
    paint::{DisplayList, ImageId, Item, Rect},
    text::{Font, Fonts, Join, Shape, TextStyle},
};
use atv_game::venue::Terrain;

pub const MAP_RESOLUTION: usize = 1024;
pub const MAP_RANGE: f32 = 768.;
/// Seconds between instrument updates (MAP_INTERVAL 50 ms).
pub const MAP_INTERVAL: f64 = 0.05;

pub fn clamp_size(value: f32) -> f32 {
    if value.is_finite() {
        (value.clamp(96., 224.) / 8.).round() * 8.
    } else {
        160.
    }
}
pub fn clamp_zoom(value: f32) -> f32 {
    if value.is_finite() {
        (value.clamp(25., 200.) / 25.).round() * 25.
    } else {
        50.
    }
}
pub fn map_range(zoom: f32) -> f32 {
    MAP_RANGE * 100. / zoom
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapShape {
    Circle,
    Square,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MinimapSettings {
    pub enabled: bool,
    pub size: f32,
    pub zoom: f32,
    pub shape: MapShape,
    pub bottom_left: bool,
    pub heading_up: bool,
}

impl MinimapSettings {
    pub fn load(settings: &Settings) -> Self {
        let get = |k| settings.get(k);
        Self {
            enabled: get("atv-minimap-enabled").as_deref() != Some("false"),
            size: get("atv-minimap-size")
                .and_then(|v| v.parse().ok())
                .map_or(160., clamp_size),
            zoom: get("atv-minimap-zoom")
                .and_then(|v| v.parse().ok())
                .map_or(50., clamp_zoom),
            shape: if get("atv-minimap-shape").as_deref() == Some("square") {
                MapShape::Square
            } else {
                MapShape::Circle
            },
            bottom_left: get("atv-minimap-corner").as_deref() == Some("bottom-left"),
            heading_up: get("atv-minimap-orientation").as_deref() == Some("heading-up"),
        }
    }

    pub fn save(&self, settings: &mut Settings) {
        settings.set("atv-minimap-enabled", &self.enabled.to_string());
        settings.set("atv-minimap-size", &self.size.to_string());
        settings.set(
            "atv-minimap-shape",
            match self.shape {
                MapShape::Circle => "circle",
                MapShape::Square => "square",
            },
        );
        settings.set(
            "atv-minimap-corner",
            if self.bottom_left {
                "bottom-left"
            } else {
                "top-right"
            },
        );
        settings.set(
            "atv-minimap-orientation",
            if self.heading_up {
                "heading-up"
            } else {
                "north-up"
            },
        );
        settings.set("atv-minimap-zoom", &self.zoom.to_string());
    }
}

/// minimap.js `rasterRow`: one row of the shaded terrain map.
pub fn raster_row(terrain: &Terrain, pixels: &mut [u8], row: usize, width: usize, height: usize) {
    let (tw, td) = (terrain.width as i64, terrain.depth as i64);
    let h =
        |x: i64, z: i64| terrain.heights[(z.clamp(0, td - 1) * tw + x.clamp(0, tw - 1)) as usize];
    let gz = (1. - (row as f64 + 0.5) / height as f64) * (td - 1) as f64;
    let iz = gz.floor() as i64;
    let spacing = terrain.spacing as f64;
    for col in 0..width {
        let gx = ((col as f64 + 0.5) / width as f64) * (tw - 1) as f64;
        let ix = gx.floor() as i64;
        let dx = (h(ix + 1, iz) - h(ix - 1, iz)) as f64 / (2. * spacing);
        let dz = (h(ix, iz + 1) - h(ix, iz - 1)) as f64 / (2. * spacing);
        let light = (0.8 + (-dx * 0.4 + dz * 0.3 + 0.6) / (dx * dx + dz * dz + 1.).sqrt() * 0.35)
            .clamp(0.42, 1.15);
        let tile_x = (gx / 64.).floor() as usize;
        let tile_z = (gz / 64.).floor() as usize;
        let layer = terrain
            .mapping
            .get(tile_z * terrain.mapping_width as usize + tile_x)
            .and_then(|&l| terrain.layers.get(l as usize));
        let dest = (row * width + col) * 4;
        for ch in 0..3 {
            let value = match layer {
                Some(image) => {
                    let (iw, ih) = (image.width as f64, image.height as f64);
                    let tx = (((gx / 64.) % 1. * 0.9375 + 0.03125) * iw)
                        .floor()
                        .min(iw - 1.);
                    let tz = (((gz / 64.) % 1. * 0.9375 + 0.03125) * ih)
                        .floor()
                        .min(ih - 1.);
                    image.rgba[((tz * iw + tx) as usize) * 4 + ch] as f64
                }
                None => [104., 110., 91.][ch],
            };
            // Math.round, then Uint8ClampedArray clamping.
            pixels[dest + ch] = (value * light * 0.82 + 0.5).floor().clamp(0., 255.) as u8;
        }
        pixels[dest + 3] = 255;
    }
}

/// The map projection: right = +X, up = +Z.
pub fn map_point(x: f32, z: f32, terrain: &Terrain, size: f32) -> (f32, f32) {
    (
        x / terrain.size_x() * size,
        (1. - z / terrain.size_z()) * size,
    )
}

/// minimap.js `instrumentFooter`: speed baseline inside the circular safe area.
pub fn instrument_footer(size: f32, shape: MapShape, width: f32, descent: f32) -> f32 {
    let radius = size / 2.;
    let half = width / 2. + 2.;
    let edge = match shape {
        MapShape::Circle => radius + (radius * radius - half * half).max(0.).sqrt(),
        MapShape::Square => size,
    };
    edge - descent - 4.
}

/// The player marker pose (`mapPose`): rider origin and heading.
#[derive(Clone, Copy, Debug)]
pub struct MapPose {
    pub origin: [f32; 3],
    pub heading: f32,
}

impl MapPose {
    /// From the rider (or ragdoll root) world matrix.
    pub fn from_matrix(m: &[f32; 16]) -> Self {
        Self {
            origin: [m[12], m[13], m[14]],
            heading: m[8].atan2(m[10]),
        }
    }
}

/// Speed footer values from the original HUD controller.
#[derive(Clone, Debug, PartialEq)]
pub struct Instrument {
    pub speed: String,
    pub units: String,
    pub preload_visible: bool,
    pub preload_fill: f32,
    pub preload_blue_alpha: f32,
}

struct Build {
    pixels: Vec<u8>,
    row: usize,
    triangles: Vec<Triangle>,
    next: usize,
}

/// One scenery triangle in map pixels with its flat fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub points: [[f32; 2]; 3],
    pub fill: [u8; 3],
}

/// minimap.js `sceneryTriangles`: LOD0 groups of every placed static model,
/// lowest models first, filled with 72% of a sampled average of their base
/// texture (skipping reflection and blended passes).
pub fn scenery_triangles(
    scenery: &atv_game::scenery::Scenery,
    terrain: &Terrain,
    size: f32,
) -> Vec<Triangle> {
    let mut draws: Vec<_> = scenery
        .draws
        .iter()
        .filter(|d| scenery.chunks[d.chunk].pass_index == 0)
        .collect();
    draws.sort_by(|a, b| a.matrix[13].total_cmp(&b.matrix[13]));
    let mut fills: std::collections::HashMap<usize, [u8; 3]> = Default::default();
    let mut out = Vec::new();
    for draw in draws {
        let chunk = &scenery.chunks[draw.chunk];
        let pass = chunk.pass();
        if pass
            .pass
            .as_ref()
            .is_some_and(|p| p.reflection_plane != 0 || p.blend)
        {
            continue;
        }
        let fill = *fills
            .entry(draw.chunk)
            .or_insert_with(|| match &pass.texture {
                Some(texture) => {
                    let rgba = &texture.image.rgba;
                    let (mut r, mut g, mut b, mut n) = (0f64, 0f64, 0f64, 0f64);
                    for p in (0..rgba.len()).step_by(1024) {
                        r += rgba[p] as f64;
                        g += rgba[p + 1] as f64;
                        b += rgba[p + 2] as f64;
                        n += 1.;
                    }
                    if n == 0. {
                        [0x78, 0x76, 0x6c]
                    } else {
                        [r, g, b].map(|c| (c / n * 0.72).round().clamp(0., 255.) as u8)
                    }
                }
                None => [0x78, 0x76, 0x6c],
            });
        let m = &draw.matrix;
        let point = |i: u32| {
            let v = &chunk.vertices[i as usize * 8..];
            let (x, y, z) = (v[0], v[1], v[2]);
            let wx = m[0] * x + m[4] * y + m[8] * z + m[12];
            let wz = m[2] * x + m[6] * y + m[10] * z + m[14];
            let (px, py) = map_point(wx, wz, terrain, size);
            [px, py]
        };
        for tri in chunk.indices.as_chunks::<3>().0 {
            out.push(Triangle {
                points: [point(tri[0]), point(tri[1]), point(tri[2])],
                fill,
            });
        }
    }
    out
}

/// Antialiased flat triangle fill (4x4 coverage samples), source-over.
pub fn fill_triangle(pixels: &mut [u8], width: usize, height: usize, t: &Triangle) {
    let [a, b, c] = t.points;
    let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.) as usize;
    let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.) as usize;
    let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(width);
    let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(height);
    let edge = |p: [f32; 2], q: [f32; 2], x: f32, y: f32| {
        (q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0])
    };
    let area = edge(a, b, c[0], c[1]);
    if area == 0. || !area.is_finite() {
        return;
    }
    let sign = area.signum();
    for y in y0..y1 {
        for x in x0..x1 {
            let mut covered = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let (px, py) = (
                        x as f32 + (sx as f32 + 0.5) / 4.,
                        y as f32 + (sy as f32 + 0.5) / 4.,
                    );
                    if edge(a, b, px, py) * sign >= 0.
                        && edge(b, c, px, py) * sign >= 0.
                        && edge(c, a, px, py) * sign >= 0.
                    {
                        covered += 1;
                    }
                }
            }
            if covered == 0 {
                continue;
            }
            let k = covered as f32 / 16.;
            let at = (y * width + x) * 4;
            for ch in 0..3 {
                let d = pixels[at + ch] as f32;
                pixels[at + ch] = (d + (t.fill[ch] as f32 - d) * k).round() as u8;
            }
        }
    }
}

pub struct Minimap {
    pub settings: MinimapSettings,
    build: Option<Build>,
    /// A finished base map waiting for upload.
    pub pending: Option<Vec<u8>>,
    pub image: Option<ImageId>,
    pub terrain_size: [f32; 2],
    /// Instrument state, refreshed every MAP_INTERVAL.
    shown: Option<(MapPose, Option<Instrument>)>,
    updated: f64,
    pub build_ms: f64,
    pub updates: u64,
    /// Other riders (slot, world position), sampled with the instrument.
    pub peers: Vec<(u8, [f32; 3])>,
    peers_shown: Vec<(u8, [f32; 3])>,
}

impl Minimap {
    pub fn new(settings: MinimapSettings) -> Self {
        Self {
            settings,
            build: None,
            pending: None,
            image: None,
            terrain_size: [1., 1.],
            shown: None,
            updated: f64::NEG_INFINITY,
            build_ms: 0.,
            updates: 0,
            peers: Vec::new(),
            peers_shown: Vec::new(),
        }
    }

    /// Starts building the map for a new course (setTerrain).
    pub fn set_terrain(&mut self, terrain: &Terrain, scenery: &atv_game::scenery::Scenery) {
        self.build = Some(Build {
            pixels: vec![0; MAP_RESOLUTION * MAP_RESOLUTION * 4],
            row: 0,
            triangles: scenery_triangles(scenery, terrain, MAP_RESOLUTION as f32),
            next: 0,
        });
        self.terrain_size = [terrain.size_x(), terrain.size_z()];
        self.shown = None;
        self.updated = f64::NEG_INFINITY;
    }

    /// One time slice of the base build (about 2 ms, like `buildSlice`).
    pub fn build_slice(&mut self, terrain: &Terrain, budget_ms: f64) {
        let Some(build) = &mut self.build else {
            return;
        };
        let start = web_time::Instant::now();
        let spent = || start.elapsed().as_secs_f64() * 1000.;
        while build.row < MAP_RESOLUTION {
            raster_row(
                terrain,
                &mut build.pixels,
                build.row,
                MAP_RESOLUTION,
                MAP_RESOLUTION,
            );
            build.row += 1;
            if spent() >= budget_ms {
                break;
            }
        }
        if build.row == MAP_RESOLUTION {
            while build.next < build.triangles.len() {
                let t = build.triangles[build.next];
                fill_triangle(&mut build.pixels, MAP_RESOLUTION, MAP_RESOLUTION, &t);
                build.next += 1;
                if build.next % 64 == 0 && spent() >= budget_ms {
                    break;
                }
            }
        }
        self.build_ms += spent();
        if build.row == MAP_RESOLUTION && build.next == build.triangles.len() {
            self.pending = self.build.take().map(|b| b.pixels);
        }
    }

    pub fn building(&self) -> bool {
        self.build.is_some()
    }

    /// The on-screen footprint (CSS `--atv-minimap-footprint`).
    pub fn rect(&self, view: &Viewport) -> Rect {
        let size = self
            .settings
            .size
            .min(view.width * 0.42)
            .min(view.height * 0.34);
        if self.settings.bottom_left {
            Rect::new(
                8. + view.safe[3],
                view.height - 8. - view.safe[2] - size,
                size,
                size,
            )
        } else {
            Rect::new(
                view.width - 8. - view.safe[1] - size,
                8. + view.safe[0],
                size,
                size,
            )
        }
    }

    pub fn visible(&self) -> bool {
        self.settings.enabled && self.image.is_some() && self.shown.is_some()
    }

    /// Samples the pose and footer at the instrument rate.
    pub fn update(&mut self, now: f64, pose: Option<MapPose>, instrument: Option<Instrument>) {
        let Some(pose) = pose else {
            self.shown = None;
            return;
        };
        if self.shown.is_none() || now - self.updated >= MAP_INTERVAL {
            self.updated = now;
            self.shown = Some((pose, instrument));
            self.peers_shown.clone_from(&self.peers);
            self.updates += 1;
        }
    }

    pub fn paint(&self, view: &Viewport, fonts: &mut Fonts, list: &mut DisplayList) {
        let (Some(image), Some((pose, instrument))) = (self.image, &self.shown) else {
            return;
        };
        if !self.settings.enabled {
            return;
        }
        let rect = self.rect(view);
        let size = rect.w;
        let (cx, cy) = (rect.x + size / 2., rect.y + size / 2.);
        let radius = match self.settings.shape {
            MapShape::Circle => size / 2.,
            MapShape::Square => 4.,
        };
        list.opacity(0.94);
        list.clip(rect, radius);
        list.rect(rect, tokens::INK);
        let resolution = MAP_RESOLUTION as f32;
        let range = map_range(self.settings.zoom);
        let [size_x, size_z] = self.terrain_size;
        let px = pose.origin[0] / size_x * resolution;
        let py = (1. - pose.origin[2] / size_z) * resolution;
        let (sx, sy) = (range / size_x * resolution, range / size_z * resolution);
        let heading_up = self.settings.heading_up;
        let over = if heading_up {
            std::f32::consts::SQRT_2
        } else {
            1.
        };
        let src = Rect::new(
            px - sx * over / 2.,
            py - sy * over / 2.,
            sx * over,
            sy * over,
        );
        let dst = Rect::new(
            cx - size * over / 2.,
            cy - size * over / 2.,
            size * over,
            size * over,
        );
        if let Some((src, dst)) = clip_source(src, dst, resolution, resolution) {
            list.push(Item::Image {
                image,
                src,
                dst,
                rotation: if heading_up { -pose.heading } else { 0. },
                pivot: [cx, cy],
            });
        }
        // Other riders: slot-coloured dots with an ink ring, clamped to 40%
        // of the map (minimap.js `mapPeerMarker`).
        for (slot, position) in &self.peers_shown {
            let (dx, dy, _) = super::peers::map_marker(
                *position,
                pose.origin,
                heading_up.then_some(pose.heading),
                range,
                size,
            );
            list.push(Item::Box(crate::paint::BoxPaint {
                rect: Rect::new(cx + dx - 4.25, cy + dy - 4.25, 8.5, 8.5),
                radii: [4.25; 4],
                fill: Some(super::peers::color(*slot).into()),
                border: [1.5; 4],
                border_color: tokens::INK,
                ..Default::default()
            }));
        }
        // Heading marker: pale gold arrow with an ink outline.
        list.push(Item::Shape {
            shape: Shape {
                path: "M0 -8L6 6L0 3L-6 6Z".into(),
                bounds: [-6., -8., 6., 6.],
                unit: 1.,
                fill: tokens::GOLD_LIGHT,
                stroke: Some(crate::text::Stroke {
                    width: 3.,
                    color: tokens::INK,
                    join: Join::Miter,
                }),
                drop_shadow: None,
            },
            x: cx,
            y: cy,
            rotation: if heading_up { 0. } else { pose.heading },
        });
        let outlined = |size: f32, weight: u16, italic: bool| {
            TextStyle::new(Font::display(size, weight, italic), tokens::SILVER_LIGHT).stroke(
                3.,
                tokens::INK,
                Join::Miter,
            )
        };
        list.text("N", &outlined(10., 800, true), cx - 4., rect.y + 13.);
        if heading_up {
            list.push(Item::Shape {
                shape: Shape {
                    path: "M0 -4L3 3L0 1L-3 3Z".into(),
                    bounds: [-3., -4., 3., 3.],
                    unit: 1.,
                    fill: tokens::SILVER_LIGHT,
                    stroke: Some(crate::text::Stroke {
                        width: 2.,
                        color: tokens::INK,
                        join: Join::Miter,
                    }),
                    drop_shadow: None,
                },
                x: cx,
                y: rect.y + 22.,
                rotation: -pose.heading,
            });
        }
        if let Some(hud) = instrument {
            let speed_size = (size * 0.17).max(16.);
            let units_size = (size * 0.07).max(9.);
            let speed_style = outlined(speed_size, 800, true);
            let units_style = outlined(units_size, 800, false);
            let speed_width = fonts.measure(&hud.speed, &speed_style.font, 0.).width;
            let units_width = fonts.measure(&hud.units, &units_style.font, 0.).width;
            let descent = fonts.ink(&hud.units, &units_style.font)[3];
            let descent = if descent > 0. {
                descent
            } else {
                units_size * 0.25
            };
            let width = speed_width + units_width + 4.;
            let x = rect.x + (size - width) / 2.;
            let y = rect.y + instrument_footer(size, self.settings.shape, width, descent);
            list.text(&hud.speed, &speed_style, x, y);
            list.text(&hud.units, &units_style, x + speed_width + 4., y);
            if hud.preload_visible {
                let w = size * 0.4;
                let bar = Rect::new(rect.x + (size - w) / 2., rect.y + size * 0.6, w, 3.);
                list.rect(bar, tokens::INK);
                list.rect(
                    bar,
                    Color::rgba(100, 160, 255, hud.preload_blue_alpha / 255.),
                );
                list.rect(
                    Rect::new(bar.x, bar.y, w * hud.preload_fill.clamp(0., 1.), 3.),
                    tokens::ORANGE,
                );
            }
        }
        list.unclip();
        list.unopacity();
    }
}

/// Canvas drawImage source clipping: the visible part of `src` inside the
/// image and the matching part of `dst`.
pub fn clip_source(src: Rect, dst: Rect, width: f32, height: f32) -> Option<(Rect, Rect)> {
    let (kx, ky) = (dst.w / src.w, dst.h / src.h);
    let x0 = src.x.max(0.);
    let y0 = src.y.max(0.);
    let x1 = src.right().min(width);
    let y1 = src.bottom().min(height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((
        Rect::new(x0, y0, x1 - x0, y1 - y0),
        Rect::new(
            dst.x + (x0 - src.x) * kx,
            dst.y + (y0 - src.y) * ky,
            (x1 - x0) * kx,
            (y1 - y0) * ky,
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_and_footer_follow_minimap_js() {
        assert_eq!(clamp_size(161.), 160.);
        assert_eq!(clamp_size(500.), 224.);
        assert_eq!(clamp_zoom(60.), 50.);
        assert_eq!(map_range(50.), 1536.);
        let y = instrument_footer(160., MapShape::Circle, 60., 3.);
        assert!((y - (80. + (6400f32 - 1024.).sqrt() - 7.)).abs() < 1e-4);
        assert_eq!(instrument_footer(160., MapShape::Square, 60., 3.), 153.);
    }
    #[test]
    fn source_clipping_scales_destination() {
        let (s, d) = clip_source(
            Rect::new(-10., 0., 20., 20.),
            Rect::new(0., 0., 40., 40.),
            1024.,
            1024.,
        )
        .unwrap();
        assert_eq!(s, Rect::new(0., 0., 10., 20.));
        assert_eq!(d, Rect::new(20., 0., 20., 40.));
    }
}
