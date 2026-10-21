//! Touch gesture logic, ported from web/touch.js `TouchInput` and
//! `TouchLeanIntent`. Gestures adapt input axes only; the recovered Rust gates
//! own jump force. Positions are CSS pixels, times milliseconds.
use atv_game::ride::bindings::TouchIntent;
use std::f32::consts::PI;

fn clamp(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1., 1.)
    } else {
        0.
    }
}

pub const DEFAULT_STEERING_SENSITIVITY: f32 = 85.;
pub fn normalize_sensitivity(value: f32) -> f32 {
    let v = if value.is_finite() {
        value
    } else {
        DEFAULT_STEERING_SENSITIVITY
    };
    ((v / 5.).round() * 5.).clamp(50., 150.)
}
/// `loadSteeringSensitivity`: a saved 50-150 value, else the default.
pub fn load_sensitivity(saved: Option<&str>) -> f32 {
    match saved.and_then(|s| s.trim().parse::<f32>().ok()) {
        Some(v) if v.is_finite() && (50. ..=150.).contains(&v) => normalize_sensitivity(v),
        _ => DEFAULT_STEERING_SENSITIVITY,
    }
}
/// Scale displacement before clipping so a larger drag still reaches full lock.
pub fn steering_axis(dx: f32, sensitivity: f32) -> f32 {
    clamp(dx / 70. * sensitivity / 100.)
}

pub struct Trick {
    pub name: &'static str,
    pub label: &'static str,
    pub modifiers: u8,
    pub axes: [f32; 2],
    /// Screen angle (y down), radians.
    pub angle: f32,
    pub span: f32,
    pub outer: bool,
}

const fn deg(d: f32) -> f32 {
    d * PI / 180.
}
const fn trick(
    name: &'static str,
    label: &'static str,
    modifiers: u8,
    axes: [f32; 2],
    angle: f32,
    outer: bool,
) -> Trick {
    Trick {
        name,
        label,
        modifiers,
        axes,
        angle: deg(angle),
        span: deg(22.5),
        outer,
    }
}

pub const TRICK_WHEEL: [Trick; 10] = [
    trick("Nac Nac", "Nac Nac", 1, [0., -1.], -90., false),
    trick("Point Back", "Point Back", 2, [1., 0.], -45., false),
    trick("Bar Hop", "Bar Hop", 1, [1., 0.], 0., false),
    trick("Cliff Hanger", "Cliff Hanger", 2, [0., 1.], 45., false),
    trick("No Footed Can Can", "Can Can", 1, [0., 1.], 90., false),
    trick("Nothing", "Nothing", 2, [-1., 0.], 135., false),
    trick("Suicide No Hander", "No Hander", 1, [-1., 0.], 180., false),
    trick("Heel Clicker", "Heel Clicker", 2, [0., -1.], -135., false),
    trick("Cat Nac", "Cat Nac", 3, [0., -1.], -60., true),
    trick("Cordova", "Cordova", 3, [0., 1.], -120., true),
];
pub const OUTER_DISTANCE: f32 = 132.;

fn sector(outer: bool, angle: f32) -> Option<usize> {
    TRICK_WHEEL.iter().position(|c| {
        c.outer == outer && (angle - c.angle).sin().atan2((angle - c.angle).cos()).abs() <= c.span
    })
}
pub fn trick_wheel_selection(dx: f32, dy: f32) -> Option<usize> {
    let distance = dx.hypot(dy);
    if !dx.is_finite() || !dy.is_finite() || distance < 28. {
        return None;
    }
    let angle = dy.atan2(dx);
    (if distance >= OUTER_DISTANCE {
        sector(true, angle)
    } else {
        None
    })
    .or_else(|| sector(false, angle))
}

pub struct Flip {
    pub name: &'static str,
    pub flip: &'static str,
    pub cue: &'static str,
}
pub const FLIPS: [Flip; 3] = [
    Flip {
        name: "Backflip",
        flip: "backflip",
        cue: "LIFT → BACKFLIP",
    },
    Flip {
        name: "360 Left",
        flip: "left",
        cue: "LIFT → 360 LEFT",
    },
    Flip {
        name: "360 Right",
        flip: "right",
        cue: "LIFT → 360 RIGHT",
    },
];
pub const FLIP_REACH: f32 = 105.;
pub const FLIP_DWELL: f64 = 220.;
pub const FLIP_GRACE: f64 = 100.;
const FLIP_CONE: f32 = PI / 6.;
fn side_reach(sensitivity: f32) -> f32 {
    FLIP_REACH.max(70. * 100. / sensitivity.max(1.) * 1.3)
}
pub fn flip_zone(dx: f32, dy: f32, sensitivity: f32) -> Option<usize> {
    if !dx.is_finite() || !dy.is_finite() {
        return None;
    }
    let distance = dx.hypot(dy);
    let angle = dy.atan2(dx);
    let toward =
        |target: f32| (angle - target).sin().atan2((angle - target).cos()).abs() <= FLIP_CONE;
    if distance >= FLIP_REACH && toward(PI / 2.) {
        return Some(0);
    }
    if distance < side_reach(sensitivity) {
        return None;
    }
    if toward(PI) {
        Some(1)
    } else if toward(0.) {
        Some(2)
    } else {
        None
    }
}
pub const SPIN_RAMP: f32 = 50.;
pub fn arcade_spin_axis(dx: f32, sensitivity: f32) -> f32 {
    let dx = if dx.is_finite() { dx } else { 0. };
    let over = dx.abs() - side_reach(sensitivity);
    if over > 0. {
        dx.signum() * clamp(over / SPIN_RAMP)
    } else {
        0.
    }
}
/// Held pull time in source stick-hold frames (60 = one second).
fn hold_frames(since: f64, now: f64) -> f32 {
    ((now - since) * 0.06).clamp(0., 60.) as f32
}
pub const FLIP_SWIPE: f32 = 120.;

/// A chosen air trick: a wheel sector, a swipe/circle stunt, or a flip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    Wheel(usize),
    Stunt { modifiers: u8, axes: [f32; 2] },
    Flip(usize),
}

impl Selection {
    fn modifiers(&self) -> u8 {
        match self {
            Self::Wheel(i) => TRICK_WHEEL[*i].modifiers,
            Self::Stunt { modifiers, .. } => *modifiers,
            Self::Flip(_) => 0,
        }
    }
    fn axes(&self) -> [f32; 2] {
        match self {
            Self::Wheel(i) => TRICK_WHEEL[*i].axes,
            Self::Stunt { axes, .. } => *axes,
            Self::Flip(_) => [0., 0.],
        }
    }
    fn flip(&self) -> Option<&'static str> {
        match self {
            Self::Flip(i) => Some(FLIPS[*i].flip),
            _ => None,
        }
    }
    pub fn name(&self) -> Option<&'static str> {
        match self {
            Self::Wheel(i) => Some(TRICK_WHEEL[*i].name),
            Self::Flip(i) => Some(FLIPS[*i].name),
            Self::Stunt { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Drive,
    Trick,
    Radial,
}

#[derive(Clone, Debug)]
pub struct Pointer {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub current_x: f32,
    pub current_y: f32,
    pub axis: f32,
    pub mode: Mode,
    points: Vec<[f32; 2]>,
    last_move: f64,
    pub selection: Option<Selection>,
    selection_sent: bool,
    pub flip: Option<usize>,
    pub flip_since: f64,
    last_armed: Option<(usize, f64, f64)>,
}

impl Pointer {
    fn new(id: u64, x: f32, y: f32, mode: Mode) -> Self {
        Self {
            id,
            x,
            y,
            current_x: x,
            current_y: y,
            axis: 1.,
            mode,
            points: vec![[x, y]],
            last_move: f64::INFINITY,
            selection: None,
            selection_sent: false,
            flip: None,
            flip_since: 0.,
            last_armed: None,
        }
    }
    fn track_flip(&mut self, flip: Option<usize>, now: f64) {
        if flip == self.flip {
            return;
        }
        if let Some(f) = self.flip
            && now - self.flip_since >= FLIP_DWELL
        {
            self.last_armed = Some((f, self.flip_since, now));
        }
        self.flip = flip;
        self.flip_since = now;
    }
    pub fn flip_armed(&self, now: f64) -> bool {
        self.flip.is_some() && now - self.flip_since >= FLIP_DWELL
    }
    fn flip_launch(&self, now: f64) -> Option<(usize, f32)> {
        if self.flip_armed(now) {
            return Some((self.flip?, hold_frames(self.flip_since, now)));
        }
        let (flip, since, at) = self.last_armed?;
        (now - at <= FLIP_GRACE).then(|| (flip, hold_frames(since, at)))
    }
}

/// One sample's driving controls.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub drive: TouchIntent,
    pub preload_axis: Option<f32>,
    pub steer: f32,
    pub lean: f32,
    pub spin: f32,
    pub stunt_modifiers: u8,
    pub stunt_x: f32,
    pub stunt_y: f32,
    pub flip_request: Option<&'static str>,
    pub flip_launch: bool,
    pub flip_hold: f32,
}

#[derive(Default)]
pub struct TouchInput {
    radial: bool,
    pub generation: u64,
    pub airborne: bool,
    pub one_handed: bool,
    pub flips: bool,
    pub arcade: bool,
    pub steering_sensitivity: f32,
    pub pointer: Option<Pointer>,
    pub secondary: Option<Box<TouchInput>>,
    pending_selection: Option<Selection>,
    pending_flip: Option<(usize, f32)>,
    sampled: bool,
    release_pending: bool,
}

impl TouchInput {
    pub fn new(radial: bool) -> Self {
        let mut input = Self {
            radial,
            steering_sensitivity: 100.,
            ..Default::default()
        };
        input.clear();
        input
    }

    pub fn clear(&mut self) {
        self.generation += 1;
        self.pointer = None;
        self.secondary = None;
        self.pending_selection = None;
        self.pending_flip = None;
        self.sampled = false;
        self.release_pending = false;
    }

    pub fn owns(&self, id: u64) -> bool {
        self.pointer.as_ref().is_some_and(|p| p.id == id)
            || self.secondary.as_ref().is_some_and(|s| s.owns(id))
    }

    pub fn begin(&mut self, id: u64, x: f32, y: f32, airborne: bool) -> bool {
        if self.pointer.is_some() {
            if !self.one_handed
                || self.owns(id)
                || self.secondary.as_ref().is_some_and(|s| s.pointer.is_some())
            {
                return false;
            }
            let mut secondary = TouchInput::new(true);
            secondary.flips = self.flips;
            let begun = secondary.begin(id, x, y, airborne);
            self.secondary = Some(Box::new(secondary));
            return begun;
        }
        self.secondary = None;
        self.airborne = airborne;
        let mode = if airborne && !self.one_handed {
            if self.radial {
                Mode::Radial
            } else {
                Mode::Trick
            }
        } else {
            Mode::Drive
        };
        self.pointer = Some(Pointer::new(id, x, y, mode));
        self.sampled = false;
        self.release_pending = false;
        true
    }

    pub fn set_one_handed(&mut self, enabled: bool) {
        if self.one_handed == enabled {
            return;
        }
        self.one_handed = enabled;
        self.clear();
    }

    pub fn set_airborne(&mut self, airborne: bool) {
        if airborne == self.airborne {
            return;
        }
        self.airborne = airborne;
        if self.one_handed {
            if let Some(s) = &mut self.secondary {
                s.set_airborne(airborne);
            }
            return;
        }
        let radial = self.radial;
        if let Some(p) = &mut self.pointer {
            p.mode = if airborne {
                if radial { Mode::Radial } else { Mode::Trick }
            } else {
                Mode::Drive
            };
            p.x = p.current_x;
            p.y = p.current_y;
            if !airborne {
                p.axis = 1.;
            }
            p.points = vec![[p.x, p.y]];
            p.selection = None;
            p.last_move = f64::INFINITY;
        }
        self.sampled = false;
        self.release_pending = false;
        self.pending_selection = None;
        self.generation += 1;
    }

    pub fn move_to(&mut self, id: u64, x: f32, y: f32, now: f64) {
        if let Some(s) = &mut self.secondary
            && s.owns(id)
        {
            s.move_to(id, x, y, now);
            return;
        }
        let one_handed = self.one_handed;
        let sensitivity = self.steering_sensitivity;
        let Some(p) = self.pointer.as_mut().filter(|p| p.id == id) else {
            return;
        };
        p.current_x = x;
        p.current_y = y;
        match p.mode {
            Mode::Radial => {
                let [sx, sy] = p.points[0];
                p.selection = if (x - sx).hypot(y - sy) < 8. {
                    None
                } else {
                    trick_wheel_selection(x - p.x, y - p.y).map(Selection::Wheel)
                };
            }
            Mode::Drive => {
                if one_handed {
                    p.track_flip(flip_zone(x - p.x, y - p.y, sensitivity), now);
                }
                let back = y - p.y;
                p.axis = if one_handed {
                    1.
                } else if back > 8. {
                    -clamp((back - 8.) / 62.)
                } else {
                    1.
                };
            }
            Mode::Trick => {
                if p.selection.is_some() {
                    return;
                }
                let previous = *p.points.last().unwrap();
                if (x - previous[0]).hypot(y - previous[1]) < 3. {
                    return;
                }
                p.points.push([x, y]);
                p.last_move = now;
                if p.points.len() > 256 {
                    p.points.remove(1);
                }
                let points = &p.points;
                if points.len() < 8 {
                    return;
                }
                let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for q in points {
                    x0 = x0.min(q[0]);
                    x1 = x1.max(q[0]);
                    y0 = y0.min(q[1]);
                    y1 = y1.max(q[1]);
                }
                let (width, height) = (x1 - x0, y1 - y0);
                if width.min(height) < 44. || width.max(height) / width.min(height) > 2.5 {
                    return;
                }
                let mut turn = 0f32;
                for i in 2..points.len() {
                    let a = (points[i - 1][1] - points[i - 2][1])
                        .atan2(points[i - 1][0] - points[i - 2][0]);
                    let b =
                        (points[i][1] - points[i - 1][1]).atan2(points[i][0] - points[i - 1][0]);
                    turn += (b - a).sin().atan2((b - a).cos());
                }
                if turn.abs() >= 5.2 && (x - p.x).hypot(y - p.y) <= width.min(height) * 0.45 {
                    p.selection = Some(Selection::Stunt {
                        modifiers: 3,
                        axes: [0., if turn > 0. { -1. } else { 1. }],
                    });
                }
            }
        }
    }

    pub fn end(&mut self, id: u64, cancelled: bool, now: f64) {
        if let Some(s) = &mut self.secondary
            && s.owns(id)
        {
            s.end(id, cancelled, now);
            return;
        }
        if !self.pointer.as_ref().is_some_and(|p| p.id == id) {
            return;
        }
        if cancelled {
            self.clear();
            return;
        }
        if self.flips && self.one_handed {
            self.pending_flip = self.pointer.as_ref().and_then(|p| p.flip_launch(now));
        }
        let mode = self.pointer.as_ref().unwrap().mode;
        if mode == Mode::Radial {
            self.pending_selection = self.pointer.as_ref().unwrap().selection;
        }
        if mode == Mode::Trick {
            let sent = self.pointer.as_ref().unwrap().selection_sent;
            self.sample(now + 120.);
            if !sent {
                self.pending_selection = self.pointer.as_ref().and_then(|p| p.selection);
            }
        }
        self.release_pending = self.sampled && mode == Mode::Drive;
        self.pointer = None;
        self.secondary = None;
        self.sampled = false;
    }

    pub fn sample(&mut self, now: f64) -> Sample {
        let flips = self.flips;
        if let Some(p) = &mut self.pointer
            && p.mode == Mode::Trick
            && p.selection.is_none()
            && now - p.last_move >= 120.
        {
            let (dx, dy) = (p.current_x - p.x, p.current_y - p.y);
            let distance = dx.hypot(dy);
            let length: f32 = p
                .points
                .windows(2)
                .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                .sum();
            if distance >= 36. && distance / length >= 0.985 {
                let diagonal = dx.abs().min(dy.abs()) > dx.abs().max(dy.abs()) * 0.5;
                let flip = (flips
                    && !diagonal
                    && distance >= FLIP_SWIPE
                    && !(dy.abs() >= dx.abs() && dy < 0.))
                    .then(|| {
                        if dy.abs() >= dx.abs() {
                            0
                        } else if dx < 0. {
                            1
                        } else {
                            2
                        }
                    });
                let axes = if diagonal {
                    if dy < 0. {
                        if dx < 0. { [0., -1.] } else { [1., 0.] }
                    } else if dx < 0. {
                        [-1., 0.]
                    } else {
                        [0., 1.]
                    }
                } else if dy.abs() >= dx.abs() {
                    [0., dy.signum()]
                } else {
                    [dx.signum(), 0.]
                };
                p.selection = Some(match flip {
                    Some(f) => Selection::Flip(f),
                    None => Selection::Stunt {
                        modifiers: if diagonal { 2 } else { 1 },
                        axes,
                    },
                });
            }
        }
        let pointer = self.pointer.as_ref();
        let driving = pointer.as_ref().is_some_and(|p| p.mode == Mode::Drive);
        let leaning = self.one_handed && pointer.is_some();
        let (steer, spin, lean) = match (pointer, leaning) {
            (Some(p), true) => (
                steering_axis(p.current_x - p.x, self.steering_sensitivity),
                arcade_spin_axis(p.current_x - p.x, self.steering_sensitivity),
                clamp((p.current_y - p.y) / 70.),
            ),
            _ => (0., 0., 0.),
        };
        let mut drive = TouchIntent {
            generation: self.generation,
            held: driving,
            axis: if driving {
                pointer.as_ref().unwrap().axis
            } else {
                0.
            },
            grounded_only: false,
            forward_only: false,
            reverse_held: false,
        };
        if self.one_handed {
            drive.forward_only = true;
            drive.reverse_held =
                pointer.is_some() && self.secondary.as_ref().is_some_and(|s| s.pointer.is_some());
        }
        let mut preload_axis = None;
        if driving {
            self.sampled = true;
            preload_axis = Some(1.);
        } else if self.release_pending {
            self.release_pending = false;
            preload_axis = Some(-1.);
        }
        let secondary = if self.one_handed && pointer.is_some() && self.airborne {
            self.secondary.as_mut().map(|s| s.sample(now))
        } else {
            None
        };
        let selection = match &secondary {
            Some(s) if s.flip_request.is_some() => FLIPS
                .iter()
                .position(|f| Some(f.flip) == s.flip_request)
                .map(Selection::Flip),
            Some(s) if s.stunt_modifiers != 0 => Some(Selection::Stunt {
                modifiers: s.stunt_modifiers,
                axes: [s.stunt_x, s.stunt_y],
            }),
            _ => pointer
                .as_ref()
                .filter(|p| p.mode == Mode::Trick)
                .and_then(|p| p.selection)
                .or(self.pending_selection),
        };
        if let Some(p) = &mut self.pointer
            && p.mode == Mode::Trick
            && p.selection.is_some()
        {
            p.selection_sent = true;
        }
        self.pending_selection = None;
        let launch = self.pending_flip.take();
        let axes = selection.map_or([0., 0.], |s| s.axes());
        Sample {
            drive,
            preload_axis,
            steer,
            lean,
            spin,
            stunt_modifiers: selection.map_or(0, |s| s.modifiers()),
            stunt_x: axes[0],
            stunt_y: axes[1],
            flip_request: launch
                .map(|(f, _)| FLIPS[f].flip)
                .or_else(|| selection.and_then(|s| s.flip())),
            flip_launch: launch.is_some(),
            flip_hold: launch.map_or(0., |(_, h)| h),
        }
    }
}

/// Two-finger mode's left thumb: steers and leans, grounded or airborne.
#[derive(Default)]
pub struct TouchLeanIntent {
    pointer: Option<Pointer>,
    dx: f32,
    dy: f32,
}

impl TouchLeanIntent {
    pub fn clear(&mut self) {
        self.pointer = None;
        self.dx = 0.;
        self.dy = 0.;
    }
    pub fn begin(&mut self, id: u64, x: f32, y: f32) -> bool {
        if self.pointer.is_some() {
            return false;
        }
        self.pointer = Some(Pointer::new(id, x, y, Mode::Drive));
        self.dx = 0.;
        self.dy = 0.;
        true
    }
    pub fn owns(&self, id: u64) -> bool {
        self.pointer.as_ref().is_some_and(|p| p.id == id)
    }
    pub fn move_to(&mut self, id: u64, x: f32, y: f32, now: f64, sensitivity: f32) {
        let Some(p) = self.pointer.as_mut().filter(|p| p.id == id) else {
            return;
        };
        self.dx = x - p.x;
        self.dy = clamp((y - p.y) / 70.);
        p.track_flip(flip_zone(self.dx, y - p.y, sensitivity), now);
    }
    pub fn launch_flip(&self, now: f64) -> Option<(&'static str, f32)> {
        self.pointer
            .as_ref()
            .and_then(|p| p.flip_launch(now))
            .map(|(f, h)| (FLIPS[f].flip, h))
    }
    pub fn end(&mut self, id: u64) {
        if self.owns(id) {
            self.clear();
        }
    }
    pub fn steer(&self, sensitivity: f32) -> f32 {
        if self.pointer.is_some() {
            steering_axis(self.dx, sensitivity)
        } else {
            0.
        }
    }
    pub fn spin(&self, sensitivity: f32) -> f32 {
        if self.pointer.is_some() {
            arcade_spin_axis(self.dx, sensitivity)
        } else {
            0.
        }
    }
    pub fn lean(&self) -> f32 {
        if self.pointer.is_some() { self.dy } else { 0. }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_finger_hold_drives_steers_leans_and_releases_preload() {
        let mut input = TouchInput::new(false);
        input.set_one_handed(true);
        input.steering_sensitivity = 85.;
        assert!(input.begin(1, 200., 400., false));
        input.move_to(1, 235., 435., 10.);
        let s = input.sample(16.);
        assert!(s.drive.held && s.drive.forward_only && !s.drive.reverse_held);
        assert_eq!(s.drive.axis, 1.);
        assert_eq!(s.preload_axis, Some(1.));
        assert!((s.steer - 35. / 70. * 0.85).abs() < 1e-6);
        assert!((s.lean - 0.5).abs() < 1e-6);
        // A second finger on the ground requests reverse.
        assert!(input.begin(2, 300., 300., false));
        assert!(input.sample(20.).drive.reverse_held);
        input.end(1, false, 30.);
        let released = input.sample(33.);
        assert_eq!(released.preload_axis, Some(-1.));
        assert!(!released.drive.held);
        assert_eq!(input.sample(50.).preload_axis, None);
    }

    #[test]
    fn wheel_sectors_and_outer_band_follow_touch_js() {
        assert_eq!(trick_wheel_selection(10., 0.), None);
        assert_eq!(trick_wheel_selection(60., 0.), Some(2));
        assert_eq!(trick_wheel_selection(0., -60.), Some(0));
        // Outer band above-right: Cat Nac at -60 degrees.
        let (x, y) = (140. * deg(-60.).cos(), 140. * deg(-60.).sin());
        assert_eq!(trick_wheel_selection(x, y), Some(8));
        // Straight up in the outer band falls through to Nac Nac.
        assert_eq!(trick_wheel_selection(0., -140.), Some(0));
    }

    #[test]
    fn airborne_second_finger_wheel_submits_on_release() {
        let mut input = TouchInput::new(false);
        input.set_one_handed(true);
        input.begin(1, 200., 400., false);
        input.set_airborne(true);
        assert!(input.begin(2, 200., 300., true));
        input.move_to(2, 260., 300., 5.);
        assert_eq!(input.sample(10.).stunt_modifiers, 0);
        input.end(2, false, 20.);
        let s = input.sample(25.);
        assert_eq!((s.stunt_modifiers, s.stunt_x, s.stunt_y), (1, 1., 0.));
    }

    #[test]
    fn two_finger_swipe_selects_stunt_and_pull_back_brakes() {
        let mut input = TouchInput::new(false);
        input.begin(1, 300., 300., false);
        input.move_to(1, 300., 339., 1.);
        let s = input.sample(2.);
        assert!((s.drive.axis + (31. / 62.)).abs() < 1e-6);
        input.set_airborne(true);
        for i in 1..=10 {
            input.move_to(1, 300. + i as f32 * 6., 339., i as f64 * 10.);
        }
        let s = input.sample(400.);
        assert_eq!((s.stunt_modifiers, s.stunt_x, s.stunt_y), (1, 1., 0.));
        assert!(!s.drive.held);
    }

    #[test]
    fn sensitivity_loads_like_touch_js() {
        assert_eq!(load_sensitivity(None), 85.);
        assert_eq!(load_sensitivity(Some("120")), 120.);
        assert_eq!(load_sensitivity(Some("200")), 85.);
        assert_eq!(load_sensitivity(Some("72")), 70.);
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[test]
    fn cancel_clear_and_unseen_tap_never_release_preload() {
        for sampled in [false, true] {
            let mut input = TouchInput::new(false);
            input.begin(7, 200., 300., false);
            if sampled {
                input.sample(1.);
            }
            input.end(7, true, 2.);
            let s = input.sample(3.);
            assert!(!s.drive.held);
            assert_eq!(s.preload_axis, None);
            input.begin(7, 200., 300., false);
            input.end(7, false, 4.);
            assert_eq!(input.sample(5.).preload_axis, None);
            input.begin(7, 200., 300., false);
            input.sample(6.);
            input.clear();
            assert_eq!(input.sample(7.).preload_axis, None);
        }
    }

    #[test]
    fn ownership_and_secondary_cancel_preserve_primary_axes() {
        let mut input = TouchInput::new(false);
        input.set_one_handed(true);
        input.begin(1, 200., 300., false);
        input.move_to(1, 235., 335., 1.);
        input.begin(2, 200., 300., false);
        assert!(!input.begin(3, 200., 300., false));
        input.end(2, true, 2.);
        let s = input.sample(3.);
        assert!(s.drive.held && !s.drive.reverse_held);
        assert_eq!((s.steer, s.lean), (0.5, 0.5));
        input.set_airborne(true);
        input.begin(2, 200., 300., true);
        input.move_to(2, 260., 300., 4.);
        input.end(1, false, 5.);
        assert_eq!(input.sample(6.).stunt_modifiers, 0);
        assert!(!input.owns(2));
    }

    #[test]
    fn every_wheel_sector_submits_once_and_center_cancels() {
        for (index, trick) in TRICK_WHEEL.iter().enumerate() {
            let mut input = TouchInput::new(true);
            let distance = if trick.outer { 145. } else { 80. };
            input.begin(1, 200., 300., true);
            input.move_to(
                1,
                200. + distance * trick.angle.cos(),
                300. + distance * trick.angle.sin(),
                1.,
            );
            assert_eq!(
                input.pointer.as_ref().unwrap().selection,
                Some(Selection::Wheel(index))
            );
            assert_eq!(input.sample(2.).stunt_modifiers, 0);
            input.end(1, false, 3.);
            let s = input.sample(4.);
            assert_eq!(
                (s.stunt_modifiers, [s.stunt_x, s.stunt_y]),
                (trick.modifiers, trick.axes)
            );
            assert_eq!(input.sample(5.).stunt_modifiers, 0);
        }
        let mut input = TouchInput::new(true);
        input.begin(1, 200., 300., true);
        input.move_to(1, 280., 300., 1.);
        input.move_to(1, 200., 300., 2.);
        input.end(1, false, 3.);
        assert_eq!(input.sample(4.).stunt_modifiers, 0);
    }

    #[test]
    fn standard_mode_takeoff_rebases_gesture_and_landing_restores_drive() {
        let mut input = TouchInput::new(false);
        input.begin(1, 200., 300., false);
        input.move_to(1, 270., 370., 1.);
        input.sample(2.);
        input.set_airborne(true);
        let s = input.sample(1000.);
        assert!(!s.drive.held);
        assert_eq!(s.stunt_modifiers, 0);
        assert_eq!(s.preload_axis, None);
        input.move_to(1, 330., 370., 1010.);
        assert_eq!(input.sample(1131.).stunt_modifiers, 1);
        input.set_airborne(false);
        let s = input.sample(1150.);
        assert!(s.drive.held);
        assert_eq!(
            (s.drive.axis, s.preload_axis, s.stunt_modifiers),
            (1., Some(1.), 0)
        );
    }

    #[test]
    fn swipe_extension_releases_without_repeating_the_trigger() {
        let mut input = TouchInput::new(false);
        input.begin(1, 200., 300., true);
        input.move_to(1, 200., 240., 1.);
        assert_eq!(input.sample(100.).stunt_modifiers, 0);
        assert_eq!(input.sample(122.).stunt_modifiers, 1);
        assert_eq!(input.sample(150.).stunt_modifiers, 1);
        input.end(1, false, 151.);
        assert_eq!(input.sample(160.).stunt_modifiers, 0);
        // Quick releases still deliver a one-batch gesture.
        input.begin(2, 200., 300., true);
        input.move_to(2, 200., 240., 200.);
        input.end(2, false, 201.);
        assert_eq!(input.sample(202.).stunt_modifiers, 1);
        assert_eq!(input.sample(203.).stunt_modifiers, 0);
    }

    #[test]
    fn lean_owner_survives_airborne_input_and_ignores_other_fingers() {
        let mut lean = TouchLeanIntent::default();
        assert!(lean.begin(1, 100., 300.));
        assert!(!lean.begin(2, 200., 300.));
        lean.move_to(2, 400., 400., 0., 85.);
        assert_eq!(lean.steer(85.), 0.);
        lean.move_to(1, 170., 230., 1., 85.);
        assert_eq!(lean.steer(85.), 0.85);
        assert_eq!(lean.lean(), -1.);
        lean.end(2);
        assert_eq!(lean.lean(), -1.);
        lean.clear();
        assert_eq!((lean.steer(85.), lean.lean()), (0., 0.));
    }
}
