//! The live Free Ride main menu (research/FREERIDE_MENU.md, issue #710):
//! once past the title the game joins the shared Free Ride world and lays
//! the main menu over it, the in-ride overlay's way. Hop in rides; any other
//! row leaves the world for the frontend page or ride it names, and coming
//! back to the main menu joins again. Offline, with the game server
//! unreachable, or after a refused claim, a failed load or a dropped room,
//! the static main menu shows as before, as it does with Game Settings →
//! Start in: Menu.
use web_time::{Duration, Instant};

/// A game server that answered the Free Ride count this recently is taken
/// to be up (the menus ask again every 15 s while they show).
const FRESH: Duration = Duration::from_secs(300);
/// After a failure the main menu stays static this long.
const RETRY_AFTER: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct Home {
    /// The ride loading or running is the live main menu.
    pub riding: bool,
    /// A row picked over the world runs on the frontend's main menu, which
    /// stays static until another page shows.
    pub hold: bool,
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

    /// The claim, the load or the room failed: the static main menu for a
    /// while.
    pub fn failed(&mut self) {
        self.riding = false;
        self.failed = Some(Instant::now());
    }

    /// The world may open as the main menu now.
    pub fn available(&self) -> bool {
        self.probe
            .is_some_and(|(at, answered)| answered && at.elapsed() < FRESH)
            && self.failed.is_none_or(|at| at.elapsed() >= RETRY_AFTER)
    }

    /// Test diagnostics (`?uiDiagnostics=1`).
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "riding": self.riding,
            "hold": self.hold,
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
    fn opens_only_after_the_server_answers_and_not_after_a_failure() {
        let mut home = Home::default();
        assert!(!home.available(), "nothing asked yet");
        home.probed(false);
        assert!(!home.available(), "unreachable");
        home.probed(true);
        assert!(home.available());
        home.riding = true;
        home.failed();
        assert!(!home.riding);
        assert!(!home.available(), "a failure holds the static menu");
    }
}
