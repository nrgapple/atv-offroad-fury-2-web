//! The live Free Ride world behind the menus (research/FREERIDE_MENU.md,
//! issue #710). Past the title the frontend's menus show at once, as ever;
//! Free Ride loads in the background (no course loader) and, once ready,
//! crossfades in where the stage showed the wordmark and vehicle, a roaming
//! CPU riding the player's rider. Hop in hands that ride to the player and
//! the menus give way to the ride's HUD; the ride's Pause → Quit to Main
//! Menu brings the menus back over it. Starting any other ride leaves the
//! world; back on the menus it loads again. Offline, with the game server
//! unreachable, or after a refused claim, a failed load or a dropped room,
//! the stage keeps its own art, as it does with Game Settings → Start in:
//! Menu.
use web_time::{Duration, Instant};

/// A game server that answered the Free Ride count this recently is taken
/// to be up (the menus ask again every 15 s while they show).
const FRESH: Duration = Duration::from_secs(300);
/// After a failure the stage keeps its art this long.
const RETRY_AFTER: Duration = Duration::from_secs(120);
/// The stage's art crossfades to the world over this long.
const FADE: Duration = Duration::from_millis(700);

#[derive(Default)]
pub struct Home {
    /// The world is loading in the background.
    pub loading: bool,
    /// `App::ready` is the world (loaded; under the menus or ridden).
    pub world: bool,
    /// The menus show over the world (`world` without Hop in).
    pub live: bool,
    /// Hop in was pressed before the world was ready.
    pub waiting: bool,
    /// When the world first showed under the menus (the crossfade).
    shown: Option<Instant>,
    /// When the game server last answered the Free Ride count, and whether
    /// it did.
    probe: Option<(Instant, bool)>,
    /// When joining or riding the world last failed.
    failed: Option<Instant>,
}

impl Home {
    /// The Free Ride count's answer (None: offline or unreachable).
    pub fn probed(&mut self, answered: bool) {
        self.probe = Some((Instant::now(), answered));
    }

    /// The claim, the load or the room failed: the stage keeps its art for
    /// a while.
    pub fn failed(&mut self) {
        self.loading = false;
        self.world = false;
        self.live = false;
        self.waiting = false;
        self.shown = None;
        self.failed = Some(Instant::now());
    }

    /// The world left without a failure (another ride, or the room closed
    /// by the player).
    pub fn left(&mut self) {
        self.loading = false;
        self.world = false;
        self.live = false;
        self.waiting = false;
        self.shown = None;
    }

    /// The world may load now.
    pub fn available(&self) -> bool {
        self.probe
            .is_some_and(|(at, answered)| answered && at.elapsed() < FRESH)
            && self.failed.is_none_or(|at| at.elapsed() >= RETRY_AFTER)
    }

    /// The world is ready under the menus: its crossfade starts (once).
    pub fn show(&mut self) {
        self.world = true;
        self.live = true;
        self.shown.get_or_insert_with(Instant::now);
    }

    /// The stage's art (0) crossfading to the world (1).
    pub fn fade(&self) -> f32 {
        self.shown.map_or(0., |at| {
            (at.elapsed().as_secs_f32() / FADE.as_secs_f32()).min(1.)
        })
    }

    /// Test diagnostics (`?uiDiagnostics=1`).
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "loading": self.loading,
            "world": self.world,
            "live": self.live,
            "waiting": self.waiting,
            "fade": self.fade(),
            "probed": self.probe.map(|(_, answered)| answered),
            "failed": self.failed.is_some(),
            "available": self.available(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_only_after_the_server_answers_and_not_after_a_failure() {
        let mut home = Home::default();
        assert!(!home.available(), "nothing asked yet");
        home.probed(false);
        assert!(!home.available(), "unreachable");
        home.probed(true);
        assert!(home.available());
        assert_eq!(home.fade(), 0.);
        home.show();
        assert!(home.live && home.world);
        home.failed();
        assert!(!home.world && !home.live);
        assert!(!home.available(), "a failure keeps the stage's art");
    }
}
