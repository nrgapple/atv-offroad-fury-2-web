//! Prepares the course the menus have settled on, so Start is faster
//! (research/PERFORMANCE.md, "Menu course preload").
//!
//! While Select Track, the race setup or a similar page shows the course the
//! next Start will load, this does the part of that load which needs no
//! window or GPU and whose results outlive it: it downloads the archives at
//! low priority and fills the persistent decoded-content cache
//! ([`atv_content::decoded`]) through the decode workers. The ride load then
//! finds every download and decode already done.
//!
//! The rules that keep it from costing the menus anything:
//!
//! - it starts [`DEBOUNCE_MS`] after the selection last changed, and a new
//!   selection, leaving the page or Start abandons it at its next step;
//! - one course at a time: a new run waits for the abandoned run's last job;
//! - decoding, inflating and scene parsing run on the workers, and without
//!   workers it does not run at all;
//! - it refuses data-saver, slow connections and low-memory devices
//!   ([`refusal`]), waits while the page is hidden, and leaves this visit's
//!   cache entries alone ([`atv_content::decoded::room_for`]);
//! - a course it has finished is remembered in the cache, so looking at it
//!   again reads and downloads nothing.
//!
//! Desktop builds read local files and have no decoded cache: every call is
//! a no-op there, and their loads are unchanged.
use crate::{Result, launch, materials, offload, ride::load};
use atv_content::{Content, decoded};
use atv_core::resource::Resource;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How long a selection must rest before anything starts.
pub const DEBOUNCE_MS: i32 = 800;
/// Version of the finished-course markers.
const FORMAT: u32 = 1;
/// What one course stores at most, about (ENV01: 63 MB,
/// research/PERFORMANCE.md).
const COURSE_BYTES: u64 = 64 * 1024 * 1024;
/// Courses a page may prepare from scratch: each costs a few megabytes of
/// download. A metered connection gets one, the selected course's.
const COLD_RUNS: u32 = 6;
const COLD_RUNS_METERED: u32 = 1;

/// What the next Start would load.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// A menu course id: a venue, or a race course such as `env05ends`.
    pub course: String,
    pub racing: bool,
    pub quad: u16,
    pub rider: u16,
    /// The graphics setting the sky tiles decode for.
    pub sky_smooth: bool,
}

/// What the device and connection report (all optional in browsers).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Device {
    /// `navigator.connection.saveData`.
    pub save_data: bool,
    /// `navigator.connection.effectiveType` is 3g or slower.
    pub slow: bool,
    /// `navigator.connection.type` is cellular.
    pub metered: bool,
    /// `navigator.deviceMemory`, in gigabytes.
    pub memory: Option<f64>,
}

/// Why a device or session does not preload at all, if so.
pub fn refusal(device: Device, cold_runs: u32) -> Option<&'static str> {
    if device.save_data {
        Some("save-data")
    } else if device.slow {
        Some("slow-connection")
    } else if device.memory.is_some_and(|gb| gb > 0. && gb <= 2.) {
        Some("low-memory")
    } else if cold_runs
        >= if device.metered {
            COLD_RUNS_METERED
        } else {
            COLD_RUNS
        }
    {
        Some("session-limit")
    } else {
        None
    }
}

/// The diagnostics a browser test reads (`window.atvPreload`).
#[derive(Clone, Debug, Default)]
struct Report {
    state: &'static str,
    reason: &'static str,
    course: String,
    /// Runs started, and how they ended.
    started: u32,
    completed: u32,
    /// Runs that found the course already prepared.
    cached: u32,
    superseded: u32,
    stopped: u32,
    /// The last selection change to the start of its run, milliseconds.
    settle_ms: f64,
    /// The last finished run's duration, milliseconds.
    run_ms: f64,
    prepared: Vec<String>,
}

impl Report {
    fn json(&self) -> String {
        serde_json::json!({
            "state": self.state, "reason": self.reason, "course": self.course,
            "started": self.started, "completed": self.completed, "cached": self.cached,
            "superseded": self.superseded, "stopped": self.stopped,
            "settleMs": self.settle_ms.round(), "runMs": self.run_ms.round(),
            "prepared": self.prepared,
        })
        .to_string()
    }
}

#[derive(Default)]
struct Shared {
    /// Bumped whenever the wanted course changes; a run of an older value
    /// abandons itself.
    generation: Cell<u64>,
    /// A run is between its start and its last job.
    running: Cell<bool>,
    cold_runs: Cell<u32>,
    diagnostics: Cell<bool>,
    report: RefCell<Report>,
}

impl Shared {
    fn update(&self, change: impl FnOnce(&mut Report)) {
        let mut report = self.report.borrow_mut();
        change(&mut report);
        if self.diagnostics.get() {
            decoded::publish("atvPreload", &report.json());
        }
    }
}

/// Follows the menus' selection (one per app).
#[derive(Default)]
pub struct Preloader {
    off: bool,
    target: Option<Target>,
    shared: Rc<Shared>,
}

enum Outcome {
    Prepared,
    AlreadyPrepared,
    Refused(&'static str),
    Abandoned,
}

impl Preloader {
    /// `off` (`?preload=0`) keeps it idle; `diagnostics` publishes its state.
    pub fn new(off: bool, diagnostics: bool) -> Self {
        let preloader = Self {
            off,
            ..Self::default()
        };
        preloader.shared.diagnostics.set(diagnostics);
        preloader.shared.update(|r| r.state = "idle");
        preloader
    }

    /// Whether this selection is the one already being followed: the check
    /// a menu frame makes, without allocating. `None` is no known course.
    pub fn follows(&self, wanted: Option<(&str, bool)>, quad: u16, rider: u16) -> bool {
        match (&self.target, wanted) {
            (None, None) => true,
            (Some(t), Some((course, racing))) => {
                t.course == course && t.racing == racing && t.quad == quad && t.rider == rider
            }
            _ => false,
        }
    }

    /// The menus' selection changed to `target`: abandons what was running
    /// and starts preparing `target` once it has rested.
    pub fn follow(&mut self, target: Option<Target>, device: Device, content: &Rc<Content>) {
        if self.off || !decoded::enabled() {
            self.target = target;
            return;
        }
        let was_busy = self.target.is_some() && matches!(self.state(), "waiting" | "running");
        self.target = target.clone();
        let shared = self.shared.clone();
        shared.generation.set(shared.generation.get() + 1);
        let Some(target) = target else {
            shared.update(|r| {
                r.state = "idle";
                r.superseded += u32::from(was_busy);
            });
            return;
        };
        shared.update(|r| {
            r.state = "waiting";
            r.reason = "";
            r.course = target.course.clone();
            r.superseded += u32::from(was_busy);
        });
        let generation = shared.generation.get();
        let content = content.clone();
        decoded::spawn(async move {
            let settling = web_time::Instant::now();
            let live_shared = shared.clone();
            let live = move || live_shared.generation.get() == generation;
            decoded::sleep(DEBOUNCE_MS).await;
            // One course at a time; a hidden page does nothing.
            while live() && (shared.running.get() || decoded::hidden()) {
                decoded::sleep(100).await;
            }
            if !live() {
                return;
            }
            shared.running.set(true);
            shared.update(|r| {
                r.state = "running";
                r.started += 1;
                r.settle_ms = settling.elapsed().as_secs_f64() * 1000.;
            });
            let started = web_time::Instant::now();
            let outcome = run(&content, &target, device, &shared, &live).await;
            shared.running.set(false);
            if !live() {
                return;
            }
            shared.update(|r| match outcome {
                Ok(Outcome::Prepared) => {
                    r.state = "done";
                    r.completed += 1;
                    r.run_ms = started.elapsed().as_secs_f64() * 1000.;
                    r.prepared.push(target.course.clone());
                }
                Ok(Outcome::AlreadyPrepared) => {
                    r.state = "done";
                    r.cached += 1;
                }
                Ok(Outcome::Refused(reason)) => {
                    r.state = "skipped";
                    r.reason = reason;
                }
                Ok(Outcome::Abandoned) => r.state = "idle",
                Err(_) => {
                    // The real load reports any error the player must see.
                    r.state = "failed";
                }
            });
        });
    }

    /// A ride is starting: nothing more runs until the menus are back.
    pub fn stop(&mut self) {
        let was_busy = self.target.is_some() && matches!(self.state(), "waiting" | "running");
        self.target = None;
        let shared = &self.shared;
        shared.generation.set(shared.generation.get() + 1);
        if !self.off {
            shared.update(|r| {
                r.state = "idle";
                r.stopped += u32::from(was_busy);
            });
        }
    }

    fn state(&self) -> &'static str {
        self.shared.report.borrow().state
    }
}

/// Prepares `target`'s course, then what its rider needs.
async fn run(
    content: &Content,
    target: &Target,
    device: Device,
    shared: &Shared,
    live: &dyn Fn() -> bool,
) -> Result<Outcome> {
    if let Some(reason) = refusal(device, 0) {
        return Ok(Outcome::Refused(reason));
    }
    if !offload::off_thread() {
        return Ok(Outcome::Refused("no-workers"));
    }
    let (environment, course) = resource(target)?;
    let key = load::course_key(content, &course).ok_or("no decoded cache")?;
    let marker = format!(
        "preload/{FORMAT}/{}/{key}/{}/{}/{}",
        offload::FORMAT,
        course.scene_stem,
        course.mode_stem,
        target.sky_smooth
    );
    let mut outcome = Outcome::AlreadyPrepared;
    if !finished(&marker).await {
        if let Some(reason) = refusal(device, shared.cold_runs.get()) {
            return Ok(Outcome::Refused(reason));
        }
        if !decoded::room_for(COURSE_BYTES) {
            return Ok(Outcome::Refused("cache-budget"));
        }
        shared.cold_runs.set(shared.cold_runs.get() + 1);
        let Some(keys) = prepare_course(
            content,
            &course,
            &environment,
            &key,
            target.sky_smooth,
            live,
        )
        .await?
        else {
            return Ok(Outcome::Abandoned);
        };
        decoded::put_now(&marker, keys.join("\n").as_bytes());
        outcome = Outcome::Prepared;
    }
    if !prepare_player(content, target, live).await? {
        return Ok(Outcome::Abandoned);
    }
    Ok(outcome)
}

/// The venue and archive a [`Target`] loads, as `venue::load_with` opens them.
fn resource(target: &Target) -> Result<(String, launch::CourseResource)> {
    if target.racing {
        let (venue, variant) = launch::split_race_course(&target.course)
            .ok_or_else(|| format!("not a race course: {}", target.course))?;
        Ok((venue.to_string(), launch::race_resource(venue, variant)?))
    } else {
        Ok((
            target.course.clone(),
            launch::course_resource(&target.course, false)?,
        ))
    }
}

/// Whether a run finished this course and everything it stored is still there.
async fn finished(marker: &str) -> bool {
    if !decoded::contains(marker) {
        return false;
    }
    decoded::get(marker).await.is_some_and(|list| {
        String::from_utf8_lossy(&list)
            .lines()
            .all(decoded::contains)
    })
}

/// Downloads the course, stores its expanded archive and its terrain and sky
/// decodes; returns the keys of what it stored, or None when abandoned.
async fn prepare_course(
    content: &Content,
    course: &launch::CourseResource,
    environment: &str,
    key: &str,
    sky_smooth: bool,
    live: &dyn Fn() -> bool,
) -> Result<Option<Vec<String>>> {
    let expanded = match load::cached_expanded(Some(key)).await {
        Some(bytes) => bytes,
        None => {
            if !content.warm(&course.archive, false, live).await? {
                return Ok(None);
            }
            if let Some(path) = &course.overlay
                && !content.warm(path, false, live).await?
            {
                return Ok(None);
            }
            let base = content.file(&course.archive).await?;
            let expanded = match &course.overlay {
                None => offload::uncached(offload::EXPAND, 0, &base).await?,
                Some(path) => {
                    let overlay = content.file(path).await?;
                    let mut both = base;
                    let split = u32::try_from(both.len()).map_err(|_| "archive too large")?;
                    both.extend_from_slice(&overlay);
                    offload::uncached(offload::EXPAND_OVERLAY, split, &both).await?
                }
            };
            decoded::put_now(key, &expanded);
            expanded
        }
    };
    if let Some(path) = &course.placements
        && !content.warm(path, false, live).await?
    {
        return Ok(None);
    }
    if !live() {
        return Ok(None);
    }
    let archive = Resource::parse(&expanded)?;
    let extract = |name: &str| -> Result<Vec<u8>> {
        archive.extract(
            materials::member_named(&archive, name)
                .ok_or_else(|| format!("course member is missing: {name}"))?,
        )
    };
    let mut keys = vec![key.to_string()];

    // The venue's terrain layers, each with its mips.
    let tr2 = extract(&format!("{environment}.tr2"))?;
    let layout = atv_core::terrain::texture_layout(&tr2)?;
    let sources = crate::venue::terrain_sources(&tr2, &layout)?;
    keys.extend(offload::ensure(offload::IPUM, offload::MIPS, &sources, live).await?);
    if !live() {
        return Ok(None);
    }

    // Its sky tiles: the scene names the cube (parsed on a worker).
    let mut scene = extract(&format!("{}.scn", course.scene_stem))?;
    let split = u32::try_from(scene.len()).map_err(|_| "scene too large")?;
    if let Some(mode) = materials::member_named(&archive, &format!("{}.toy", course.mode_stem)) {
        scene.extend_from_slice(&archive.extract(mode)?);
    }
    let cube = offload::uncached(offload::SCENE_SKY, split, &scene).await?;
    let cube = String::from_utf8_lossy(&cube).into_owned();
    if !live() {
        return Ok(None);
    }
    let index = archive
        .members
        .iter()
        .position(|m| materials::last_component(&m.name).eq_ignore_ascii_case(&cube))
        .ok_or("course sky is missing")?;
    let cube = archive.extract(index)?;
    let layout = atv_core::sky::layout(&cube)?;
    let sources = crate::sky::tile_sources(&cube, &layout)?;
    let arg = if sky_smooth { offload::SMOOTH } else { 0 };
    keys.extend(offload::ensure(offload::IPUM, arg, &sources, live).await?);
    Ok(live().then_some(keys))
}

/// The selected vehicle and rider: downloads, and the rider archives the ride
/// load reads expanded. False when abandoned.
async fn prepare_player(
    content: &Content,
    target: &Target,
    live: &dyn Fn() -> bool,
) -> Result<bool> {
    let models = atv_core::player_models::selection(target.quad, target.rider)?;
    let mut files = vec![models.quad.as_str(), models.quad_materials.as_str()];
    files.extend(["/OVERLAYS.RES", "/STARTEVT.RES", "/AUDIOTBL.RES"]);
    for path in files {
        if content.has(path) && !content.warm(path, false, live).await? {
            return Ok(false);
        }
    }
    // The ride's sounds read this archive by its directory.
    if content.has("/AUDIO/AUDIO.RES") && !content.warm("/AUDIO/AUDIO.RES", true, live).await? {
        return Ok(false);
    }
    if !models.motorcycle {
        let rider = [
            models.rider.as_str(),
            "/ANIMSRDR.RES",
            "/RIDER/RIDER_MT.RES",
        ];
        for path in rider {
            let Some(key) = load::archive_key(content, path) else {
                continue;
            };
            if decoded::contains(&key) || !content.has(path) {
                continue;
            }
            if !content.warm(path, false, live).await? {
                return Ok(false);
            }
            let bytes = content.file(path).await?;
            if Resource::parse(&bytes)?.compressed {
                let expanded = offload::uncached(offload::EXPAND, 0, &bytes).await?;
                decoded::put_now(&key, &expanded);
            }
        }
    }
    Ok(live())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(course: &str, racing: bool) -> Target {
        Target {
            course: course.into(),
            racing,
            quad: 1,
            rider: 0,
            sky_smooth: true,
        }
    }

    #[test]
    fn refuses_what_a_phone_cannot_spare() {
        let good = Device::default();
        assert_eq!(refusal(good, 0), None);
        assert_eq!(
            refusal(
                Device {
                    save_data: true,
                    ..good
                },
                0
            ),
            Some("save-data")
        );
        assert_eq!(
            refusal(Device { slow: true, ..good }, 0),
            Some("slow-connection")
        );
        assert_eq!(
            refusal(
                Device {
                    memory: Some(2.),
                    ..good
                },
                0
            ),
            Some("low-memory")
        );
        // An unknown or ample memory figure does not refuse.
        assert_eq!(
            refusal(
                Device {
                    memory: Some(8.),
                    ..good
                },
                0
            ),
            None
        );
    }

    #[test]
    fn limits_cold_runs_per_page() {
        let metered = Device {
            metered: true,
            ..Device::default()
        };
        assert_eq!(refusal(metered, 0), None);
        assert_eq!(refusal(metered, 1), Some("session-limit"));
        assert_eq!(refusal(Device::default(), COLD_RUNS - 1), None);
        assert_eq!(refusal(Device::default(), COLD_RUNS), Some("session-limit"));
    }

    #[test]
    fn targets_name_the_archives_a_ride_opens() {
        let (venue, course) = resource(&target("env05ends", true)).unwrap();
        assert_eq!(venue, "env05");
        assert!(course.overlay.is_some());
        let (venue, national) = resource(&target("env05", true)).unwrap();
        assert_eq!(venue, "env05");
        assert!(national.overlay.is_none());
        let (venue, freeride) = resource(&target("env05", false)).unwrap();
        assert_eq!(venue, "env05");
        assert_ne!(freeride.archive, national.archive);
        assert!(resource(&target("nonsense", true)).is_err());
    }

    #[test]
    fn following_compares_without_a_target() {
        let mut preloader = Preloader::new(false, false);
        assert!(preloader.follows(None, 1, 0));
        assert!(!preloader.follows(Some(("env01", false)), 1, 0));
        preloader.target = Some(target("env01", false));
        assert!(preloader.follows(Some(("env01", false)), 1, 0));
        // Another course, mode, vehicle or rider is a new selection.
        assert!(!preloader.follows(Some(("env02", false)), 1, 0));
        assert!(!preloader.follows(Some(("env01", true)), 1, 0));
        assert!(!preloader.follows(Some(("env01", false)), 2, 0));
        assert!(!preloader.follows(Some(("env01", false)), 1, 1));
        assert!(!preloader.follows(None, 1, 0));
    }
}
