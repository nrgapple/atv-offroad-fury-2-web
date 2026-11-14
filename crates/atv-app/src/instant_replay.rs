//! Instant replay (#542): a ring buffer of the drawn moments. Presentation
//! only: every other fixed step (30 Hz) the poses the scene just drew
//! (the player, race CPUs, split-screen players and game CPUs, the Lap
//! Attack ghost) and the ride's camera are copied into reused storage, and
//! playback blends neighbouring samples. Nothing feeds back into the
//! simulation, so a replay cannot change the ride it shows.
//!
//! A race keeps the whole of it instead (full-race replay, a port
//! enhancement; no exact-build replay is known): samples from the first
//! drawn frame at 15 Hz in a fixed byte budget. When the budget fills,
//! every other sample is dropped and sampling halves, so any race length
//! fits the same memory. Recording stops a few seconds after race over.
//!
//! Storage: each rider's two world matrices exactly (f32 bits), its node
//! matrices quantised to 16 bits (axes in 1/8192, translations in 1/128 of
//! a unit within 256 of the first node of their part), 24 bytes a node.
use atv_core::math::Det;
use atv_game::ride::Pose;
use atv_render::Camera;

/// Steps between samples (30 Hz at the 60 Hz step).
pub const SAMPLE_STEPS: u64 = 2;
/// Samples kept: the last 30 seconds.
pub const CAPACITY: usize = 30 * 60 / SAMPLE_STEPS as usize;
/// Riders a sample keeps (the player and up to seven others).
pub const MAX_RIDERS: usize = 8;
/// A whole race's sampling before any thinning (15 Hz).
pub const WHOLE_STEPS: u64 = 4;
/// A whole race's matrix storage (bytes): its samples are thinned to fit.
pub const WHOLE_BUDGET: usize = 24 << 20;
/// A whole race's most samples (ten minutes at 15 Hz).
pub const WHOLE_SAMPLES: usize = 10 * 60 * 15;
/// A whole race keeps recording this long after race over (steps).
pub const WHOLE_TAIL: u64 = 3 * 60;

/// Which drawn rider a recorded pose is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Player,
    /// A race CPU, in `pose_race_cpus` order.
    Cpu(u8),
    /// A `pose_peers` rider (split-screen player or game CPU) by its id.
    Peer(u8),
    Ghost,
}

/// An affine matrix's first three columns (column-major; the fourth row
/// is 0, 0, 0, 1).
type Affine = [f32; 12];

fn pack(m: &[f32; 16]) -> Affine {
    [
        m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10], m[12], m[13], m[14],
    ]
}

fn unpack(a: &Affine) -> [f32; 16] {
    [
        a[0], a[1], a[2], 0., a[3], a[4], a[5], 0., a[6], a[7], a[8], 0., a[9], a[10], a[11], 1.,
    ]
}

/// `a` to `b` at `t`: each axis column blended in direction and length
/// (a wheel turning a quarter turn between samples keeps its size), the
/// translation linearly.
fn blend(a: &Affine, b: &Affine, t: f32) -> [f32; 16] {
    let mut out = unpack(a);
    if t <= 0. {
        return out;
    }
    for column in 0..3 {
        let (ca, cb) = (
            &a[column * 3..column * 3 + 3],
            &b[column * 3..column * 3 + 3],
        );
        let length = |c: &[f32]| (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        let mixed: [f32; 3] = std::array::from_fn(|i| ca[i] + (cb[i] - ca[i]) * t);
        let (la, lb, lm) = (length(ca), length(cb), length(&mixed));
        let scale = if lm > 1e-6 {
            (la + (lb - la) * t) / lm
        } else {
            1.
        };
        for i in 0..3 {
            out[column * 4 + i] = if lm > 1e-6 { mixed[i] * scale } else { cb[i] };
        }
    }
    for i in 0..3 {
        out[12 + i] = a[9 + i] + (b[9 + i] - a[9 + i]) * t;
    }
    out
}

/// One storage cell: a quantised node matrix, half a world matrix, or a
/// rider's two part offsets.
type Cell = [i16; 12];
/// Cells before a rider's nodes: two world matrices and the offsets.
const HEADER: usize = 5;
const AXIS: f32 = 8192.;
const PLACE: f32 = 128.;

fn quantise(v: f32, scale: f32) -> i16 {
    (v * scale)
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

/// Six f32s exactly, as twelve 16-bit halves.
fn put_exact(cell: &mut Cell, values: &[f32]) {
    for (i, v) in values.iter().enumerate() {
        let bits = v.to_bits();
        cell[2 * i] = (bits >> 16) as u16 as i16;
        cell[2 * i + 1] = bits as u16 as i16;
    }
}

fn get_exact(cell: &Cell, out: &mut [f32]) {
    for (i, v) in out.iter_mut().enumerate() {
        let bits = (u32::from(cell[2 * i] as u16) << 16) | u32::from(cell[2 * i + 1] as u16);
        *v = f32::from_bits(bits);
    }
}

/// Writes one rider's matrices into its cells.
fn encode(cells: &mut [Cell], pose: &Pose) {
    let mut world = [0.; 12];
    for (k, m) in [&pose.quad_world, &pose.rider_world]
        .into_iter()
        .enumerate()
    {
        world = pack(m);
        put_exact(&mut cells[2 * k], &world[..6]);
        put_exact(&mut cells[2 * k + 1], &world[6..]);
    }
    let origin = |nodes: &[[f32; 16]]| nodes.first().map_or([0.; 3], |m| [m[12], m[13], m[14]]);
    let (quad, rider) = (origin(&pose.quad_nodes), origin(&pose.rider_nodes));
    world[..3].copy_from_slice(&quad);
    world[3..6].copy_from_slice(&rider);
    put_exact(&mut cells[4], &world[..6]);
    let parts = pose
        .quad_nodes
        .iter()
        .map(|m| (m, quad))
        .chain(pose.rider_nodes.iter().map(|m| (m, rider)));
    for (cell, (m, offset)) in cells[HEADER..].iter_mut().zip(parts) {
        let a = pack(m);
        for i in 0..9 {
            cell[i] = quantise(a[i], AXIS);
        }
        for i in 0..3 {
            cell[9 + i] = quantise(a[9 + i] - offset[i], PLACE);
        }
    }
}

/// Matrix `i` of a rider's cells (0 quad world, 1 rider world, then the
/// quad's nodes and the rider's).
fn decode(cells: &[Cell], quad: usize, i: usize) -> Affine {
    let mut a = [0.; 12];
    if i < 2 {
        get_exact(&cells[2 * i], &mut a[..6]);
        get_exact(&cells[2 * i + 1], &mut a[6..]);
        return a;
    }
    let mut offsets = [0.; 6];
    get_exact(&cells[4], &mut offsets);
    let offset = if i - 2 < quad {
        &offsets[..3]
    } else {
        &offsets[3..]
    };
    let cell = &cells[HEADER + i - 2];
    for k in 0..9 {
        a[k] = f32::from(cell[k]) / AXIS;
    }
    for k in 0..3 {
        a[9 + k] = f32::from(cell[9 + k]) / PLACE + offset[k];
    }
    a
}

/// One recorded rider: its kind and its node counts (its cells follow in
/// the sample's row: the header, quad nodes, rider nodes).
#[derive(Clone, Copy, Debug, Default)]
struct Slot {
    kind: Kind,
    quad: u16,
    rider: u16,
    /// First cell in the sample's row.
    at: u32,
}

impl Slot {
    fn width(&self) -> usize {
        HEADER + usize::from(self.quad) + usize::from(self.rider)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    step: u64,
    camera: Camera,
    focal: f32,
    riders: [Slot; MAX_RIDERS],
    count: u8,
    /// Cells used of the row.
    used: u32,
}

/// The poses one replayed moment draws, reused frame to frame.
#[derive(Debug, Default)]
pub struct Moment {
    pub player: Pose,
    pub cpus: Vec<Pose>,
    pub peers: Vec<(u8, Pose)>,
    pub ghost: Option<Pose>,
    /// The ride's own camera then.
    pub camera: Camera,
    pub focal: f32,
}

/// Fills `pose` from a slot's cells, blended toward `next` (the same rider
/// in the following sample) by `t`.
fn fill(pose: &mut Pose, row: &[Cell], slot: &Slot, next: Option<(&[Cell], &Slot)>, t: f32) {
    let own = &row[slot.at as usize..][..slot.width()];
    let other = next
        .filter(|(_, n)| n.quad == slot.quad && n.rider == slot.rider)
        .map(|(s, n)| &s[n.at as usize..][..n.width()]);
    let quad = usize::from(slot.quad);
    let matrix = |i: usize| match other {
        Some(other) => blend(&decode(own, quad, i), &decode(other, quad, i), t),
        None => unpack(&decode(own, quad, i)),
    };
    pose.quad_world = matrix(0);
    pose.rider_world = matrix(1);
    pose.quad_nodes.resize(quad, [0.; 16]);
    for (i, node) in pose.quad_nodes.iter_mut().enumerate() {
        *node = matrix(2 + i);
    }
    pose.rider_nodes.resize(usize::from(slot.rider), [0.; 16]);
    for (i, node) in pose.rider_nodes.iter_mut().enumerate() {
        *node = matrix(2 + quad + i);
    }
}

/// Grows `v` to at least `len` (doubling, at most `limit`), allocating
/// only when it must.
fn reserve<T: Clone>(v: &mut Vec<T>, len: usize, limit: usize, fill: T) {
    if v.len() >= len {
        return;
    }
    let want = (v.len() * 2).max(len).max(64).min(limit.max(len));
    v.reserve_exact(want - v.len());
    v.resize(want, fill);
}

/// The recorded stretch: a ring (or a whole race) of samples over rows of
/// one cell slab.
#[derive(Default)]
pub struct Recorder {
    samples: Vec<Sample>,
    slab: Vec<Cell>,
    /// Cells each sample's row holds.
    stride: usize,
    /// The oldest sample's index and the samples held.
    head: usize,
    len: usize,
    /// The latest sample is still taking riders (this frame's).
    open: bool,
    last_step: Option<u64>,
    /// Whole-ride mode (a race): kept from the start, thinned when full.
    whole: bool,
    /// Steps between samples now (whole mode doubles it per thinning).
    interval: u64,
    /// Race over's step (whole mode stops `WHOLE_TAIL` after it).
    over: Option<u64>,
}

impl Recorder {
    /// Forgets the recording (a reset, a new ride); keeps the storage.
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
        self.open = false;
        self.last_step = None;
        self.interval = 0;
        self.over = None;
    }

    /// Whole-ride mode (a race) or the 30-second ring; a change starts a
    /// new recording in new storage.
    pub fn set_whole(&mut self, whole: bool) {
        if whole != self.whole {
            *self = Self {
                whole,
                ..Self::default()
            };
        }
    }

    pub fn is_whole(&self) -> bool {
        self.whole
    }

    /// Race over at `step` (whole mode records `WHOLE_TAIL` more).
    pub fn race_over(&mut self, step: u64) {
        self.over.get_or_insert(step);
    }

    /// Samples the storage may hold at `stride` cells a sample.
    fn capacity_at(&self, stride: usize) -> usize {
        if self.whole {
            let fit = WHOLE_BUDGET / (stride.max(1) * std::mem::size_of::<Cell>());
            fit.clamp(2, WHOLE_SAMPLES)
        } else {
            CAPACITY
        }
    }

    fn capacity(&self) -> usize {
        self.capacity_at(self.stride)
    }

    /// Steps between samples now.
    pub fn interval(&self) -> u64 {
        match self.interval {
            0 if self.whole => WHOLE_STEPS,
            0 => SAMPLE_STEPS,
            n => n,
        }
    }

    /// Bytes the recording's storage holds (samples and matrices).
    pub fn bytes(&self) -> usize {
        self.samples.capacity() * std::mem::size_of::<Sample>()
            + self.slab.capacity() * std::mem::size_of::<Cell>()
    }

    /// Cells each sample's row holds.
    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The recorded span in steps (first, last).
    pub fn span(&self) -> Option<(u64, u64)> {
        (self.len > 0).then(|| (self.get(0).step, self.get(self.len - 1).step))
    }

    fn index(&self, i: usize) -> usize {
        (self.head + i) % self.capacity()
    }

    fn get(&self, i: usize) -> &Sample {
        &self.samples[self.index(i)]
    }

    fn row(&self, i: usize) -> &[Cell] {
        let index = self.index(i);
        &self.slab[index * self.stride..][..self.samples[index].used as usize]
    }

    /// Starts a sample at `step` with the ride's camera when a sample is
    /// due; `add` then records this frame's riders. A step that went back
    /// (a restart) starts a new recording.
    pub fn begin(&mut self, step: u64, camera: Camera, focal: f32) -> bool {
        self.open = false;
        if let Some(last) = self.last_step {
            if step < last {
                self.clear();
            } else if step < last + self.interval() {
                return false;
            }
        }
        if self.over.is_some_and(|over| step > over + WHOLE_TAIL) {
            return false;
        }
        while self.whole && self.len >= self.capacity() {
            self.thin(false);
        }
        let capacity = self.capacity();
        let at = if self.len < capacity {
            self.len += 1;
            self.index(self.len - 1)
        } else {
            let at = self.head;
            self.head = (self.head + 1) % capacity;
            at
        };
        reserve(&mut self.samples, at + 1, capacity, Sample::default());
        let stride = self.stride;
        reserve(
            &mut self.slab,
            (at + 1) * stride,
            capacity * stride,
            [0; 12],
        );
        self.samples[at] = Sample {
            step,
            camera,
            focal,
            ..Sample::default()
        };
        self.last_step = Some(step);
        self.open = true;
        true
    }

    /// Records one drawn rider into the sample `begin` started.
    pub fn add(&mut self, kind: Kind, pose: &Pose) {
        if !self.open || self.len == 0 {
            return;
        }
        let sample = self.samples[self.index(self.len - 1)];
        let width = HEADER + pose.quad_nodes.len() + pose.rider_nodes.len();
        if usize::from(sample.count) >= MAX_RIDERS
            || pose.quad_nodes.len() > usize::from(u16::MAX)
            || pose.rider_nodes.len() > usize::from(u16::MAX)
        {
            return;
        }
        if sample.used as usize + width > self.stride {
            // The first riders, or more of them than before (a race's CPUs,
            // a ghost's first lap): widen the rows. A ring keeps room for
            // one more like it; a whole race widens exactly.
            let extra = if self.whole { width } else { 2 * width };
            self.grow(sample.used as usize + extra);
        }
        let at = self.index(self.len - 1);
        let base = at * self.stride + sample.used as usize;
        encode(&mut self.slab[base..base + width], pose);
        let s = &mut self.samples[at];
        s.riders[usize::from(s.count)] = Slot {
            kind,
            quad: pose.quad_nodes.len() as u16,
            rider: pose.rider_nodes.len() as u16,
            at: s.used,
        };
        s.count += 1;
        s.used += width as u32;
    }

    /// Whole mode, full: keeps every other sample (the first among them,
    /// and with `keep_last` the open one) and halves the sampling, in place.
    fn thin(&mut self, keep_last: bool) {
        let stride = self.stride;
        let last = if keep_last { self.len - 1 } else { usize::MAX };
        let mut kept = 0;
        for i in (0..self.len).filter(|i| i % 2 == 0 || *i == last) {
            if i != kept {
                self.samples[kept] = self.samples[i];
                self.slab
                    .copy_within(i * stride..(i + 1) * stride, kept * stride);
            }
            kept += 1;
        }
        self.len = kept;
        self.interval = self.interval() * 2;
    }

    /// Widens every row to `stride` cells, keeping what they hold (a whole
    /// race thins first if the wider rows would pass its budget).
    fn grow(&mut self, stride: usize) {
        while self.whole && self.len > self.capacity_at(stride) {
            self.thin(true);
        }
        let capacity = self.capacity_at(stride);
        // A full ring uses every row; otherwise rows run 0..len.
        let rows = if self.len >= capacity {
            capacity
        } else {
            self.len
        };
        let mut slab = Vec::new();
        reserve(&mut slab, rows * stride, capacity * stride, [0; 12]);
        for i in 0..self.len {
            let at = self.index(i);
            let used = self.samples[at].used as usize;
            slab[at * stride..][..used].copy_from_slice(&self.slab[at * self.stride..][..used]);
        }
        self.slab = slab;
        self.stride = stride;
    }

    /// Closes this frame's sample.
    pub fn end(&mut self) {
        self.open = false;
    }

    /// The player's origin at sample `i` (camera placement).
    fn origin(&self, i: usize) -> Option<[f32; 3]> {
        let sample = self.get(i);
        let slot = sample.riders[..usize::from(sample.count)]
            .iter()
            .find(|s| s.kind == Kind::Player)?;
        let m = decode(&self.row(i)[slot.at as usize..], 0, 0);
        Some([m[9], m[10], m[11]])
    }

    /// The player's origin at `step` (blended), for the replay cameras.
    pub fn origin_at(&self, step: f64) -> Option<[f32; 3]> {
        let (i, t) = self.locate(step)?;
        let a = self.origin(i)?;
        let b = if i + 1 < self.len {
            self.origin(i + 1).unwrap_or(a)
        } else {
            a
        };
        Some(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t))
    }

    /// The sample at or before `step` and the blend toward the next.
    fn locate(&self, step: f64) -> Option<(usize, f32)> {
        if self.len == 0 {
            return None;
        }
        // Binary search over the samples' increasing steps.
        let (mut lo, mut hi) = (0, self.len - 1);
        if step <= self.get(0).step as f64 {
            return Some((0, 0.));
        }
        if step >= self.get(hi).step as f64 {
            return Some((hi, 0.));
        }
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.get(mid).step as f64 <= step {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (a, b) = (self.get(lo).step as f64, self.get(hi).step as f64);
        Some((lo, ((step - a) / (b - a).max(1.)) as f32))
    }

    /// The moment at `step` (fractional), written into `out`.
    pub fn sample(&self, step: f64, out: &mut Moment) -> bool {
        let Some((i, t)) = self.locate(step) else {
            return false;
        };
        let sample = self.get(i);
        let row = self.row(i);
        let next = (i + 1 < self.len).then(|| (self.row(i + 1), self.get(i + 1)));
        let mut cpus = 0;
        let mut peers = 0;
        let mut ghost = false;
        for slot in &sample.riders[..usize::from(sample.count)] {
            let other = next.and_then(|(s, n)| {
                n.riders[..usize::from(n.count)]
                    .iter()
                    .find(|o| o.kind == slot.kind)
                    .map(|o| (s, o))
            });
            match slot.kind {
                Kind::Player => fill(&mut out.player, row, slot, other, t),
                Kind::Cpu(_) => {
                    if out.cpus.len() <= cpus {
                        out.cpus.push(Pose::default());
                    }
                    fill(&mut out.cpus[cpus], row, slot, other, t);
                    cpus += 1;
                }
                Kind::Peer(id) => {
                    if out.peers.len() <= peers {
                        out.peers.push((id, Pose::default()));
                    }
                    out.peers[peers].0 = id;
                    fill(&mut out.peers[peers].1, row, slot, other, t);
                    peers += 1;
                }
                Kind::Ghost => {
                    fill(
                        out.ghost.get_or_insert_with(Pose::default),
                        row,
                        slot,
                        other,
                        t,
                    );
                    ghost = true;
                }
            }
        }
        out.cpus.truncate(cpus);
        out.peers.truncate(peers);
        if !ghost {
            out.ghost = None;
        }
        out.camera = sample.camera;
        out.focal = sample.focal;
        if let Some((_, n)) = next {
            let c = &n.camera;
            let mix = |a: f32, b: f32| a + (b - a) * t;
            // Yaw blends the short way round.
            let mut dy = c.yaw - out.camera.yaw;
            dy = (dy + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            out.camera = Camera {
                x: mix(out.camera.x, c.x),
                y: mix(out.camera.y, c.y),
                z: mix(out.camera.z, c.z),
                yaw: out.camera.yaw + dy * t,
                pitch: mix(out.camera.pitch, c.pitch),
            };
            out.focal = mix(out.focal, n.focal);
        }
        true
    }
}

/// Replay cameras the player cycles through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shot {
    /// The ride's own camera as it was.
    Chase,
    /// A vantage beside the line ahead, following the rider (TV style).
    Trackside,
    /// A slow circle around the rider.
    Orbit,
    /// A broadcast director: cuts between trackside, crane, low, head-on,
    /// aerial and onboard vantages as the rider passes each.
    Broadcast,
}

impl Shot {
    pub fn next(self) -> Self {
        match self {
            Self::Chase => Self::Trackside,
            Self::Trackside => Self::Orbit,
            Self::Orbit => Self::Broadcast,
            Self::Broadcast => Self::Chase,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chase => "Chase",
            Self::Trackside => "Trackside",
            Self::Orbit => "Orbit",
            Self::Broadcast => "Broadcast",
        }
    }
}

/// Points a camera at `eye` toward `target`.
pub fn look(eye: [f32; 3], target: [f32; 3]) -> Camera {
    let d = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
    Camera {
        x: eye[0],
        y: eye[1],
        z: eye[2],
        yaw: d[0].atan2_d(d[2]),
        pitch: d[1].atan2_d(d[0].hypot_d(d[2])),
    }
}

/// The trackside vantage for a rider at `now` who will be at `ahead`: off
/// to the side of the line between them, `height` above `ground`.
pub fn vantage(now: [f32; 3], ahead: [f32; 3], ground: &dyn Fn(f32, f32) -> f32) -> [f32; 3] {
    vantage_beside(now, ahead, 28., 7., ground)
}

/// A vantage `side` units to the left of the line from `now` to `ahead`
/// (negative: the right), beside `ahead` and `height` above the ground or
/// the rider there, whichever is higher.
pub fn vantage_beside(
    now: [f32; 3],
    ahead: [f32; 3],
    side: f32,
    height: f32,
    ground: &dyn Fn(f32, f32) -> f32,
) -> [f32; 3] {
    let (dx, dz) = (ahead[0] - now[0], ahead[2] - now[2]);
    let length = dx.hypot_d(dz);
    let (ux, uz) = if length > 1. {
        (dx / length, dz / length)
    } else {
        (0., 1.)
    };
    let (x, z) = (ahead[0] - uz * side, ahead[2] + ux * side);
    let floor = ground(x, z);
    let y = if floor.is_finite() {
        floor.max(ahead[1]) + height
    } else {
        ahead[1] + height
    };
    [x, y, z]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(x: f32, nodes: usize) -> Pose {
        let mut world = [0.; 16];
        world[0] = 1.;
        world[5] = 1.;
        world[10] = 1.;
        world[15] = 1.;
        world[12] = x;
        Pose {
            quad_world: world,
            quad_nodes: vec![world; nodes],
            rider_world: world,
            rider_nodes: vec![world; 2],
        }
    }

    fn record(r: &mut Recorder, step: u64, x: f32) -> bool {
        let began = r.begin(step, Camera::default(), 2.);
        if began {
            r.add(Kind::Player, &pose(x, 3));
            r.add(Kind::Cpu(0), &pose(-x, 3));
            r.end();
        }
        began
    }

    #[test]
    fn samples_every_other_step_and_blends_between_them() {
        let mut r = Recorder::default();
        assert!(record(&mut r, 10, 0.));
        assert!(!record(&mut r, 11, 5.));
        assert!(record(&mut r, 12, 10.));
        assert_eq!(r.span(), Some((10, 12)));
        let mut m = Moment::default();
        assert!(r.sample(11., &mut m));
        assert!((m.player.quad_world[12] - 5.).abs() < 1e-5);
        assert!((m.cpus[0].quad_nodes[2][12] + 5.).abs() < 1e-5);
        assert_eq!(m.player.rider_nodes.len(), 2);
        // Past either end: the end samples.
        r.sample(100., &mut m);
        assert_eq!(m.player.quad_world[12], 10.);
        r.sample(0., &mut m);
        assert_eq!(m.player.quad_world[12], 0.);
        assert_eq!(r.origin_at(11.), Some([5., 0., 0.]));
    }

    #[test]
    fn the_ring_keeps_the_last_thirty_seconds_without_growing() {
        let mut r = Recorder::default();
        for i in 0..(CAPACITY as u64 * 3) {
            record(&mut r, i * SAMPLE_STEPS, i as f32);
        }
        assert_eq!(r.len(), CAPACITY);
        let slab = r.slab.as_ptr();
        record(&mut r, CAPACITY as u64 * 3 * SAMPLE_STEPS, 0.);
        assert_eq!(r.slab.as_ptr(), slab, "no reallocation once sized");
        let (first, last) = r.span().unwrap();
        assert_eq!(last - first, (CAPACITY as u64 - 1) * SAMPLE_STEPS);
    }

    #[test]
    fn a_whole_race_keeps_its_start_and_thins_into_the_same_storage() {
        let mut r = Recorder::default();
        r.set_whole(true);
        let mut step = 0;
        while r.len() < WHOLE_SAMPLES {
            record(&mut r, step, step as f32);
            step += 1;
        }
        assert_eq!(r.capacity(), WHOLE_SAMPLES);
        assert_eq!(
            r.span(),
            Some((0, (WHOLE_SAMPLES as u64 - 1) * WHOLE_STEPS))
        );
        let (slab, bytes) = (r.slab.as_ptr(), r.bytes());
        // Past capacity: every other sample goes, sampling halves, the
        // start stays and nothing is reallocated.
        for _ in 0..WHOLE_SAMPLES as u64 * WHOLE_STEPS * 3 {
            record(&mut r, step, step as f32);
            step += 1;
        }
        assert_eq!(r.slab.as_ptr(), slab);
        assert_eq!(r.bytes(), bytes);
        assert!(r.interval() >= WHOLE_STEPS * 4);
        let (first, last) = r.span().unwrap();
        assert_eq!(first, 0);
        assert!(step - 1 - last < r.interval());
        let mut m = Moment::default();
        let probe = (last / 2) as f64 + 1.;
        r.sample(probe, &mut m);
        assert!((m.player.quad_world[12] - probe as f32).abs() < 1e-2);
        // Evenly spaced after thinning.
        let steps: Vec<u64> = (0..r.len()).map(|i| r.get(i).step).collect();
        assert!(steps.windows(2).all(|w| w[1] - w[0] <= r.interval()));
    }

    #[test]
    fn a_whole_race_of_wide_riders_stays_in_its_budget() {
        let mut r = Recorder::default();
        r.set_whole(true);
        let wide = |x: f32| pose(x, 40);
        for i in 0..20_000u64 {
            if r.begin(i * WHOLE_STEPS, Camera::default(), 2.) {
                for k in 0..6 {
                    r.add(Kind::Cpu(k), &wide(i as f32));
                }
                // A ghost joins halfway: the rows widen, thinning to fit.
                if i >= 10_000 {
                    r.add(Kind::Ghost, &wide(0.));
                }
                r.end();
            }
        }
        let samples = WHOLE_SAMPLES * std::mem::size_of::<Sample>();
        assert!(r.bytes() <= WHOLE_BUDGET + samples, "{} bytes", r.bytes());
        assert_eq!(r.span().unwrap().0, 0);
        let mut m = Moment::default();
        r.sample(r.span().unwrap().1 as f64, &mut m);
        assert_eq!(m.cpus.len(), 6);
        assert!(m.ghost.is_some());
    }

    #[test]
    fn a_five_lap_race_of_six_riders_fits_at_seven_and_a_half_hertz() {
        // sx01 laps take about 70 s: five laps, the intro and the tail make
        // about six minutes. Riders draw about 45 nodes (50 cells each).
        let mut r = Recorder::default();
        r.set_whole(true);
        let steps = (5 * 70 + 13) * 60;
        for step in 0..steps {
            if r.begin(step, Camera::default(), 2.) {
                r.add(Kind::Player, &pose(step as f32, 43));
                for k in 0..5 {
                    r.add(Kind::Cpu(k), &pose(step as f32, 43));
                }
                r.end();
            }
        }
        assert_eq!(r.stride(), 300);
        let samples = WHOLE_SAMPLES * std::mem::size_of::<Sample>();
        assert!(r.bytes() <= WHOLE_BUDGET + samples, "{} bytes", r.bytes());
        // One thinning: 7.5 Hz from the grid to the finish.
        assert_eq!(r.interval(), WHOLE_STEPS * 2);
        assert_eq!(r.span().unwrap().0, 0);
        eprintln!(
            "five laps, six riders: {} samples, {:.1} MiB",
            r.len(),
            r.bytes() as f64 / 1048576.
        );
    }

    #[test]
    fn storage_keeps_worlds_exactly_and_nodes_within_a_hundredth() {
        let mut p = pose(1234.567, 2);
        p.quad_world[13] = -87.654_32;
        p.quad_nodes[1] = [
            0.6, 0.8, 0., 0., -0.8, 0.6, 0., 0., 0., 0., 1., 0., 1434.25, 20.5, -9.75, 1.,
        ];
        p.rider_nodes[0][12] = 0.3;
        let mut cells = vec![[0; 12]; HEADER + 4];
        encode(&mut cells, &p);
        assert_eq!(unpack(&decode(&cells, 2, 0)), p.quad_world);
        let node = unpack(&decode(&cells, 2, 3));
        for (a, b) in node.iter().zip(&p.quad_nodes[1]) {
            assert!((a - b).abs() < 5e-3, "{node:?}");
        }
        assert!((decode(&cells, 2, 4)[9] - 0.3).abs() < 5e-3);
    }

    #[test]
    fn a_whole_race_stops_a_little_after_race_over_and_a_restart_records_again() {
        let mut r = Recorder::default();
        r.set_whole(true);
        for step in (0..400).step_by(4) {
            record(&mut r, step, 0.);
        }
        r.race_over(400);
        for step in (400..2000).step_by(4) {
            r.race_over(step);
            record(&mut r, step, 0.);
        }
        assert_eq!(r.span(), Some((0, 400 + WHOLE_TAIL)));
        assert!(record(&mut r, 0, 0.), "a restart records again");
        assert!(record(&mut r, 2000, 0.));
        // Leaving the race for a free ride: the 30-second ring.
        r.set_whole(false);
        assert!(r.is_empty() && r.interval() == SAMPLE_STEPS);
    }

    #[test]
    fn a_restart_starts_over_and_more_riders_widen_the_storage() {
        let mut r = Recorder::default();
        record(&mut r, 100, 1.);
        record(&mut r, 102, 1.);
        record(&mut r, 4, 2.);
        assert_eq!(r.span(), Some((4, 4)));
        // A ghost and a wide rider join: earlier samples keep their poses.
        r.begin(6, Camera::default(), 2.);
        r.add(Kind::Player, &pose(3., 3));
        r.add(Kind::Ghost, &pose(4., 3));
        r.add(Kind::Peer(2), &pose(5., 200));
        r.end();
        let mut m = Moment::default();
        r.sample(4., &mut m);
        assert_eq!(m.player.quad_world[12], 2.);
        assert_eq!(m.cpus[0].quad_world[12], -2.);
        assert!(m.ghost.is_none());
        r.sample(6., &mut m);
        assert!(m.cpus.is_empty());
        assert_eq!(m.ghost.as_ref().map(|g| g.quad_world[12]), Some(4.));
        assert_eq!(m.peers[0].0, 2);
        assert_eq!(m.peers[0].1.quad_nodes.len(), 200);
    }

    #[test]
    fn blending_keeps_a_turning_axis_its_length() {
        // A quarter turn about y between samples.
        let a = pack(&[
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ]);
        let b = pack(&[
            0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 0., 0., 0., 1.,
        ]);
        let m = blend(&a, &b, 0.5);
        let length = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
        assert!((length - 1.).abs() < 1e-5);
    }

    #[test]
    fn trackside_vantage_sits_beside_the_line_above_ground() {
        let v = vantage([0., 0., 0.], [0., 0., 100.], &|_, _| 3.);
        assert!((v[0].abs() - 28.).abs() < 1e-4 && (v[2] - 100.).abs() < 1e-4);
        assert_eq!(v[1], 10.);
        let c = look([0., 0., 0.], [0., 0., 10.]);
        assert!(c.yaw.abs() < 1e-6 && c.pitch.abs() < 1e-6);
    }
}
