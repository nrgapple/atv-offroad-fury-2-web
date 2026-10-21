//! Other riders in a multiplayer room: overhead name tags (web/multiplayer.js
//! `PlayerIndicators`, game.css `.player-indicator`) and the slot colours the
//! minimap's peer markers share.
//!
//! Slots 0-3 use the original CompassArrow tints (0x002e68e0: red, blue,
//! green, yellow); slots 4-19 exist only in shared Free Ride rooms and their
//! colours are authored. The tag placement and arrow artwork are authored
//! browser presentation, not recovered PS2 behaviour.
use super::Viewport;
use crate::{
    color::Color,
    paint::{DisplayList, Item},
    text::{Font, Fonts, Join, Shape, Stroke, TextStyle},
};

/// PLAYER_COLORS in web/multiplayer.js, by slot.
pub const PLAYER_COLORS: [&str; 20] = [
    "#ff0000", "#0000ff", "#00ff00", "#ffff00", "#ff8c00", "#00e5ff", "#ff3fd2", "#9b5cff",
    "#7cff6b", "#ffffff", "#ff6f91", "#3d7dff", "#c8ff00", "#00b894", "#ffb3b3", "#b07a44",
    "#ff2e63", "#7fdbff", "#e0c3fc", "#ffd166",
];
/// Name tags beyond the nearest few riders clutter a busy shared room.
pub const MAX_NAME_TAGS: usize = 8;
pub const MAX_EDGE_TAGS: usize = 3;

pub fn color(slot: u8) -> Color {
    Color::hex(PLAYER_COLORS[usize::from(slot) % PLAYER_COLORS.len()])
}

/// Another rider as the HUD shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    pub slot: u8,
    pub name: String,
    /// The ATV's world position.
    pub position: [f32; 3],
    pub stale: bool,
}

/// Where a tag goes: screen position, whether it is pinned to the edge (the
/// rider is off-screen or behind) and the direction toward it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    pub x: f32,
    pub y: f32,
    pub edge: bool,
    pub angle: f32,
}

/// `screenMarker`: 7 ft above the ATV, projected with the column-major
/// view-projection; points outside the inner 84%/72% clamp to that edge.
pub fn screen_marker(
    position: [f32; 3],
    vp: &[f32; 16],
    width: f32,
    height: f32,
) -> Option<Marker> {
    if !position.iter().all(|v| v.is_finite()) {
        return None;
    }
    let point = [position[0], position[1] + 7., position[2], 1.];
    let clip: [f32; 4] = std::array::from_fn(|r| (0..4).map(|k| vp[k * 4 + r] * point[k]).sum());
    if !clip.iter().all(|v| v.is_finite()) {
        return None;
    }
    let divisor = clip[3].abs().max(0.001);
    let (mut x, mut y) = (clip[0] / divisor, -clip[1] / divisor);
    let edge = clip[3] <= 0. || x.abs() > 0.84 || y.abs() > 0.72;
    if edge {
        if x.hypot(y) < 0.001 {
            y = 1.;
        }
        let scale = (x.abs() / 0.84).max(y.abs() / 0.72).max(1.);
        x /= scale;
        y /= scale;
    }
    Some(Marker {
        x: (x + 1.) * width / 2.,
        y: (y + 1.) * height / 2.,
        edge,
        angle: y.atan2(x),
    })
}

const FONT_SIZE: f32 = 18.;
/// The tag arrow (`M12 2 L21 21 L12 17 L3 21 Z` in a 24 unit box, 18 px),
/// stored pointing down (the browser's `rotate(180deg)` on-screen case).
const ARROW: &str = "M0 10L9 -9L0 -5L-9 -9Z";

/// Draws the nearest riders' tags (`PlayerIndicators.draw`): at most eight,
/// of which at most three point to riders off-screen.
pub fn paint_tags(
    fonts: &mut Fonts,
    list: &mut DisplayList,
    view: &Viewport,
    peers: &[Peer],
    origin: [f32; 3],
    vp: &[f32; 16],
) -> usize {
    let distance = |p: &Peer| {
        p.position
            .iter()
            .zip(origin)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
    };
    let mut nearest: Vec<&Peer> = peers.iter().filter(|p| !p.stale).collect();
    nearest.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
    nearest.truncate(MAX_NAME_TAGS);
    let font = Font::display(FONT_SIZE, 800, true);
    let style = TextStyle::new(font, Color::WHITE)
        .stroke(2.5, Color::BLACK, Join::Round)
        .shadow(0., 1., 1., Color::BLACK);
    let line = FONT_SIZE * 1.2;
    let mut edges = 0;
    let mut drawn = 0;
    for peer in nearest {
        let Some(marker) = screen_marker(peer.position, vp, view.width, view.height) else {
            continue;
        };
        if marker.edge {
            edges += 1;
            if edges > MAX_EDGE_TAGS {
                continue;
            }
        }
        // Label (2px 4px padding, ellipsis past 300 px), then an 18 px arrow
        // 2 px below, the whole node's bottom centre on the marker.
        let mut name = peer.name.clone();
        let max = 300f32.min(view.width * 0.7);
        if fonts.measure(&name, &style.font, 0.).width + 8. > max {
            while !name.is_empty()
                && fonts.measure(&format!("{name}…"), &style.font, 0.).width + 8. > max
            {
                name.pop();
            }
            name.push('…');
        }
        let text_width = fonts.measure(&name, &style.font, 0.).width;
        let node_width = (text_width + 8.).max(18.);
        let node_height = line + 4. + 2. + 18.;
        let half = node_width / 2.;
        let x = marker
            .x
            .clamp(half + 6., (view.width - half - 6.).max(half + 6.));
        let y = marker
            .y
            .clamp(node_height + 6., (view.height - 6.).max(node_height + 6.));
        if marker.edge {
            list.opacity(0.85);
        }
        let baseline = y - 18. - 2. - 2. - line * 0.22;
        list.text(&name, &style, x - text_width / 2., baseline);
        list.push(Item::Shape {
            shape: Shape {
                path: ARROW.into(),
                bounds: [-9., -9., 9., 10.],
                unit: 0.75,
                fill: color(peer.slot),
                stroke: Some(Stroke {
                    width: 0.94,
                    color: Color::hex("#111"),
                    join: Join::Round,
                }),
                drop_shadow: None,
            },
            x,
            y: y - 9.,
            // Edge arrows point toward the rider (down, turned by angle - 90 deg).
            rotation: if marker.edge {
                marker.angle - std::f32::consts::FRAC_PI_2
            } else {
                0.
            },
        });
        if marker.edge {
            list.unopacity();
        }
        drawn += 1;
    }
    drawn
}

/// `mapPeerMarker`: a rider's offset on the GPS map, clamped to 40% of its
/// size; `heading` rotates with a heading-up map.
pub fn map_marker(
    position: [f32; 3],
    center: [f32; 3],
    heading: Option<f32>,
    range: f32,
    size: f32,
) -> (f32, f32, bool) {
    let mut dx = (position[0] - center[0]) / range * size;
    let mut dy = -(position[2] - center[2]) / range * size;
    if let Some(heading) = heading {
        let a = -heading;
        (dx, dy) = (dx * a.cos() - dy * a.sin(), dx * a.sin() + dy * a.cos());
    }
    let radius = size * 0.4;
    let distance = dx.hypot(dy);
    let edge = distance > radius;
    if edge {
        dx *= radius / distance;
        dy *= radius / distance;
    }
    (dx, dy, edge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_clamp_behind_and_off_screen_riders_to_the_edge() {
        // Identity view-projection: clip = world.
        let mut vp = [0.; 16];
        vp[0] = 1.;
        vp[5] = 1.;
        vp[10] = 1.;
        vp[15] = 1.;
        let m = screen_marker([0., -7., 0.], &vp, 200., 100.).unwrap();
        assert_eq!((m.x, m.y, m.edge), (100., 50., false));
        let m = screen_marker([5., -7., 0.], &vp, 200., 100.).unwrap();
        assert!(m.edge && (m.x - 184.).abs() < 1e-3);
        assert!(screen_marker([f32::NAN, 0., 0.], &vp, 200., 100.).is_none());
    }

    #[test]
    fn map_markers_follow_north_up_and_clamp_to_the_ring() {
        let (dx, dy, edge) = map_marker([10., 0., 20.], [0.; 3], None, 100., 160.);
        assert_eq!((dx, dy, edge), (16., -32., false));
        let (dx, _, edge) = map_marker([1000., 0., 0.], [0.; 3], None, 100., 160.);
        assert!(edge && (dx - 64.).abs() < 1e-3);
        assert_eq!(color(0), Color::hex("#ff0000"));
    }
}
