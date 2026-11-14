//! Instant replay (#542): a ring buffer of the drawn moments. Presentation
//! only: every other fixed step (30 Hz) the poses the scene just drew
//! (the player, race CPUs, split-screen players and game CPUs, the Lap
//! Attack ghost) and the ride's camera are copied into storage allocated
//! once, and playback blends neighbouring samples. Nothing feeds back into
//! the simulation, so a replay cannot change the ride it shows.
//!
//! A race keeps the whole of it instead (full-race replay, a port
//! enhancement): the samples are kept from the first drawn frame, and when
//! the bounded storage fills every other one is dropped and sampling
//! halves, so any race length fits in the same memory. Recording stops a
//! few seconds after race over.
use atv_core::math::Det;
use atv_game::ride::Pose;
use atv_render::Camera;

/// Steps between samples (30 Hz at the 60 Hz step).
pub const SAMPLE_STEPS: u64 = 2;
/// Samples kept: the last 30 seconds.
pub const CAPACITY: usize = 30 * 60 / SAMPLE_STEPS as usize;
/// Riders a sample keeps (the player and up to seven others).
pub const MAX_RIDERS: usize = 8;
/// Samples a whole race may hold before thinning: ten minutes at 15 Hz.
pub const WHOLE_CAPACITY: usize = 10 * 60 * 15;
/// A whole race's sampling before any thinning (15 Hz).
pub const WHOLE_STEPS: u64 = 4;
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

/// One recorded rider: its kind and its node counts (its matrices follow
/// in the sample's slab: quad world, rider world, quad nodes, rider nodes).
#[derive(Clone, Copy, Debug, Default)]
struct Slot {
    kind: Kind,
    quad: u16,
    rider: u16,
    /// First matrix in the sample's slab.
    at: u32,
}

impl Slot {
    fn width(&self) -> usize {
        2 + usize::from(self.quad) + usize::from(self.rider)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    step: u64,
    camera: Camera,
    focal: f32,
    riders: [Slot; MAX_RIDERS],
    count: u8,
    /// Matrices used of the slab.
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

/// Fills `pose` from a slot's matrices, blended toward `next` (the same
/// rider in the following sample) by `t`.
fn fill(pose: &mut Pose, slab: &[Affine], slot: &Slot, next: Option<(&[Affine], &Slot)>, t: f32) {
    let own = &slab[slot.at as usize..][..slot.width()];
    let other = next
        .filter(|(_, n)| n.quad == slot.quad && n.rider == slot.rider)
        .map(|(s, n)| &s[n.at as usize..][..n.width()]);
    let matrix = |i: usize| match other {
        Some(other) => blend(&own[i], &other[i], t),
        None => unpack(&own[i]),
    };
    pose.quad_world = matrix(0);
    pose.rider_world = matrix(1);
    let quad = usize::from(slot.quad);
    pose.quad_nodes.resize(quad, [0.; 16]);
    for (i, node) in pose.quad_nodes.iter_mut().enumerate() {
        *node = matrix(2 + i);
    }
    pose.rider_nodes.resize(usize::from(slot.rider), [0.; 16]);
    for (i, node) in pose.rider_nodes.iter_mut().enumerate() {
        *node = matrix(2 + quad + i);
    }
}

/// The recorded stretch: a ring of samples over one matrix slab.
#[derive(Default)]
pub struct Recorder {
    samples: Vec<Sample>,
    slab: Vec<Affine>,
    /// Matrices each sample may hold.
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
    /// new recording.
    pub fn set_whole(&mut self, whole: bool) {
        if whole != self.whole {
            self.whole = whole;
            self.clear();
            self.samples = Vec::new();
            self.slab = Vec::new();
            self.stride = 0;
        }
    }

    pub fn is_whole(&self) -> bool {
        self.whole
    }

    /// Race over at `step` (whole mode records `WHOLE_TAIL` more).
    pub fn race_over(&mut self, step: u64) {
        self.over.get_or_insert(step);
    }

    fn capacity(&self) -> usize {
        if self.whole { WHOLE_CAPACITY } else { CAPACITY }
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
            + self.slab.capacity() * std::mem::size_of::<Affine>()
    }

    /// Matrices each sample may hold.
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
        (self.head + i) % self.samples.len()
    }

    fn get(&self, i: usize) -> &Sample {
        &self.samples[self.index(i)]
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
        let capacity = self.capacity();
        if self.samples.is_empty() {
            self.samples = vec![Sample::default(); capacity];
        }
        if self.whole && self.len == capacity {
            self.thin();
        }
        let at = if self.len < capacity {
            self.len += 1;
            self.index(self.len - 1)
        } else {
            let at = self.head;
            self.head = (self.head + 1) % capacity;
            at
        };
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
        let at = self.index(self.len - 1);
        let sample = self.samples[at];
        let width = 2 + pose.quad_nodes.len() + pose.rider_nodes.len();
        if usize::from(sample.count) >= MAX_RIDERS
            || pose.quad_nodes.len() > usize::from(u16::MAX)
            || pose.rider_nodes.len() > usize::from(u16::MAX)
        {
            return;
        }
        if sample.used as usize + width > self.stride {
            // The first riders, or more of them than before (a race's CPUs,
            // a ghost's first lap): grow, with room for one more like it.
            self.grow(sample.used as usize + 2 * width);
        }
        let at = self.index(self.len - 1);
        let base = at * self.stride + sample.used as usize;
        let slab = &mut self.slab[base..base + width];
        slab[0] = pack(&pose.quad_world);
        slab[1] = pack(&pose.rider_world);
        for (dst, src) in slab[2..]
            .iter_mut()
            .zip(pose.quad_nodes.iter().chain(&pose.rider_nodes))
        {
            *dst = pack(src);
        }
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

    /// Whole mode, full: keeps every other sample (the first among them)
    /// and halves the sampling, in place.
    fn thin(&mut self) {
        let stride = self.stride;
        let kept = self.len.div_ceil(2);
        for i in 1..kept {
            self.samples[i] = self.samples[2 * i];
            self.slab
                .copy_within(2 * i * stride..(2 * i + 1) * stride, i * stride);
        }
        self.len = kept;
        self.interval = self.interval() * 2;
    }

    /// Widens every sample to `stride` matrices, keeping what they hold.
    fn grow(&mut self, stride: usize) {
        let mut slab = vec![[0.; 12]; self.capacity() * stride];
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
        let index = self.index(i);
        let sample = &self.samples[index];
        let slot = sample.riders[..usize::from(sample.count)]
            .iter()
            .find(|s| s.kind == Kind::Player)?;
        let m = &self.slab[index * self.stride + slot.at as usize];
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
        // Binary search over the ring's increasing steps.
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
        let index = self.index(i);
        let sample = &self.samples[index];
        let slab = &self.slab[index * self.stride..][..sample.used as usize];
        let next = (i + 1 < self.len).then(|| {
            let n = self.index(i + 1);
            (
                &self.slab[n * self.stride..][..self.samples[n].used as usize],
                &self.samples[n],
            )
        });
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
                Kind::Player => fill(&mut out.player, slab, slot, other, t),
                Kind::Cpu(_) => {
                    if out.cpus.len() <= cpus {
                        out.cpus.push(Pose::default());
                    }
                    fill(&mut out.cpus[cpus], slab, slot, other, t);
                    cpus += 1;
                }
                Kind::Peer(id) => {
                    if out.peers.len() <= peers {
                        out.peers.push((id, Pose::default()));
                    }
                    out.peers[peers].0 = id;
                    fill(&mut out.peers[peers].1, slab, slot, other, t);
                    peers += 1;
                }
                Kind::Ghost => {
                    fill(
                        out.ghost.get_or_insert_with(Pose::default),
                        slab,
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
}

impl Shot {
    pub fn next(self) -> Self {
        match self {
            Self::Chase => Self::Trackside,
            Self::Trackside => Self::Orbit,
            Self::Orbit => Self::Chase,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chase => "Chase",
            Self::Trackside => "Trackside",
            Self::Orbit => "Orbit",
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
    let (dx, dz) = (ahead[0] - now[0], ahead[2] - now[2]);
    let length = dx.hypot_d(dz);
    let (ux, uz) = if length > 1. {
        (dx / length, dz / length)
    } else {
        (0., 1.)
    };
    let side = 28.;
    let (x, z) = (ahead[0] - uz * side, ahead[2] + ux * side);
    let floor = ground(x, z);
    let y = if floor.is_finite() {
        floor.max(ahead[1]) + 7.
    } else {
        ahead[1] + 7.
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
        while r.len() < WHOLE_CAPACITY {
            record(&mut r, step, step as f32);
            step += 1;
        }
        assert_eq!(
            r.span(),
            Some((0, (WHOLE_CAPACITY as u64 - 1) * WHOLE_STEPS))
        );
        let (slab, bytes) = (r.slab.as_ptr(), r.bytes());
        // Past capacity: every other sample goes, sampling halves, the
        // start stays and nothing is reallocated.
        for _ in 0..WHOLE_CAPACITY as u64 * WHOLE_STEPS * 3 {
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
