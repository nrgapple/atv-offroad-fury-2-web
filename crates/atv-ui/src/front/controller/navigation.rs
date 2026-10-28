//! The frontend's page history (research/MENU_FLOW.md).
//!
//! The original keeps a page stack in the game state (`0021eb50` pushes
//! up to ten entries at +0x1938: the page, its page set and the focused
//! row and value; the page base's GoToPage `0025d760` pushes the page it
//! leaves, Back `0025d8b0` pops it and the page's activation `0025d430`
//! restores the remembered row). Every page's Triangle calls that Back, so
//! Back always returns to the page the player came from, on the row they
//! left. Returning from an event pops to the page that started it
//! (`0021ecb0`), and results pages that name a page unwind to it
//! (`0021ed60`).
//!
//! The port follows the same model: [`Frontend::show`] pushes the page it
//! leaves, or unwinds to a page already in the history; Back pops. A page
//! opened without a history (a link, `?menu=`, `?front=`) gets the
//! canonical parents of [`Frontend::canonical_parent`] instead.
use super::Frontend;
use crate::hud::Viewport;

/// The original's stack holds ten pages; the port's deepest route
/// (Title → Main → Profile Editor → View Profile → ATVs) is far shorter.
const DEPTH: usize = 16;

/// A page left for another: Back returns to it on the same row.
#[derive(Clone, Debug, PartialEq)]
pub struct Visit {
    pub page: String,
    /// The selected row's action (rows can come and go while away, e.g.
    /// Championships once a profile loads), and its index as a fallback.
    pub action: Option<String>,
    pub selection: usize,
}

/// Pages that are never returned to: the course loader stands in for the
/// ride and is replaced when it ends.
fn transient(page: &str) -> bool {
    page == "loading"
}

impl Frontend {
    /// Opens `page`. A page already in the history is returned to (the
    /// pages above it are dropped, as the original's results pages unwind
    /// to a named page), restoring its row; any other page is pushed over
    /// the current one. Showing the current page again starts it afresh.
    pub fn show(&mut self, page: &str, view: &Viewport) {
        if page == self.page {
            self.present(page, view);
            return;
        }
        if let Some(at) = self.history.iter().rposition(|v| v.page == page) {
            let visit = self.history[at].clone();
            self.history.truncate(at);
            self.present(page, view);
            self.restore(&visit, view);
            return;
        }
        if transient(&self.page) {
            // Back from a ride (or a failed load) to a page that was never
            // visited: its canonical parents stand in.
            self.history = self.ancestors(page);
        } else {
            self.remember();
        }
        self.present(page, view);
    }

    /// Opens `page` from outside the menus (a link, `?menu=`, `?front=`):
    /// the history becomes its canonical parents, so Back climbs to the
    /// title.
    pub fn open(&mut self, page: &str, view: &Viewport) {
        self.history = self.ancestors(page);
        self.present(page, view);
    }

    /// Back: the previous page on its row, else the canonical parent (a
    /// page opened by the app with no history). None on the title.
    pub(super) fn go_back(&mut self, view: &Viewport) {
        let from = self.page.clone();
        if let Some(visit) = self.history.pop() {
            self.present(&visit.page, view);
            self.restore(&visit, view);
            return;
        }
        let Some(to) = self.canonical_parent(&from) else {
            return;
        };
        self.history = self.ancestors(&to);
        self.present(&to, view);
        // The row that leads to the page just left.
        if let Some(at) = self.choices.iter().position(|(a, _)| *a == from) {
            self.selection = at;
        }
    }

    /// The page that started the ride the loader stands in for.
    pub fn launcher(&self) -> Option<&str> {
        transient(&self.page)
            .then(|| self.history.last().map(|v| v.page.as_str()))
            .flatten()
    }

    /// The pages Back returns through, oldest first.
    pub fn history(&self) -> impl Iterator<Item = &str> {
        self.history.iter().map(|v| v.page.as_str())
    }

    fn remember(&mut self) {
        self.history.push(Visit {
            page: self.page.clone(),
            action: self.current().map(String::from),
            selection: self.selection,
        });
        if self.history.len() > DEPTH {
            self.history.remove(0);
        }
    }

    /// The returned-to page on its remembered row (`0025d430`).
    fn restore(&mut self, visit: &Visit, view: &Viewport) {
        let at = visit
            .action
            .as_ref()
            .and_then(|action| self.choices.iter().position(|(a, _)| a == action));
        self.selection = at.unwrap_or(visit.selection);
        // Scrolled back into view once laid out (a long Options or store
        // list); the course and vehicle lists show their current card.
        self.reveal_row = true;
        self.refresh(view);
    }

    /// The canonical parents of `page`, title first.
    fn ancestors(&self, page: &str) -> Vec<Visit> {
        let mut chain = Vec::new();
        let mut child = page.to_string();
        while let Some(parent) = self.canonical_parent(&child) {
            if chain.iter().any(|v: &Visit| v.page == parent) || chain.len() >= DEPTH {
                break;
            }
            chain.push(Visit {
                page: parent.clone(),
                // The parent's row that opens the child, where it has one.
                action: Some(child.clone()),
                selection: 0,
            });
            child = parent;
        }
        chain.reverse();
        chain
    }

    /// Where a page sits in the menus when it was not reached through them
    /// (the original's synthesized stacks, `0021ed60`).
    pub fn canonical_parent(&self, page: &str) -> Option<String> {
        // The current page's own rules know its state (a store page opened
        // from View Profile, a name entry for a copy, the page that opened
        // Pass Key).
        if page == self.page {
            if let Some(to) = self.store_back().or_else(|| self.profile_back()) {
                return Some(to.into());
            }
            if let Some(to) = self.passkey_back() {
                return Some(to);
            }
        }
        if super::store_page(page) {
            return Some(if page == "store" { "profile" } else { "store" }.into());
        }
        if page == "passkey" {
            return Some(if self.passkey.back.is_empty() {
                "main".into()
            } else {
                self.passkey.back.clone()
            });
        }
        Some(
            match page {
                "title" | "loading" => return None,
                "main" => "title",
                "single" | "multiplayer" | "profile" | "hall-of-fame" => "main",
                "tracks" if self.hosting => "create-lobby",
                "tracks" | "race" if self.split => "split",
                "tracks" | "race" | "championship" | "games" | "objective-courses" | "garage" => {
                    "single"
                }
                "objectives" if self.freestyle.is_none() => "objective-courses",
                "objectives" => "championship",
                "gear" => "garage",
                "create-lobby" | "join-lobby" | "split" | "lobby" => "multiplayer",
                "profile-overview" | "profile-trophies" | "profile-records" => "profile-view",
                p if p.starts_with("profile-") => "profile",
                p if p.starts_with("passkey-") => "passkey",
                _ => "main",
            }
            .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{tracks, view};
    use super::*;
    use crate::settings::Settings;

    fn front() -> (Frontend, Viewport, Settings) {
        let settings = Settings::memory();
        let view = view(1280., 720.);
        let mut front = Frontend::new(tracks(), &settings, true);
        front.set_race_courses(vec!["nat01".into()], &view);
        (front, view, settings)
    }

    fn select(front: &mut Frontend, action: &str) {
        front.selection = front
            .choices()
            .iter()
            .position(|(a, _)| a == action)
            .unwrap_or_else(|| panic!("{action} in {:?}", front.choices()));
    }

    fn open(front: &mut Frontend, action: &str, view: &Viewport, settings: &mut Settings) {
        select(front, action);
        front.key("Enter", view, settings);
    }

    fn current(front: &Frontend) -> &str {
        front.choices()[front.selection].0.as_str()
    }

    #[test]
    fn back_returns_to_the_page_and_row_it_came_from() {
        let (mut front, view, mut settings) = front();
        open(&mut front, "main", &view, &mut settings);
        open(&mut front, "single", &view, &mut settings);
        open(&mut front, "race", &view, &mut settings);
        assert_eq!(front.page, "race");
        assert_eq!(
            front.history().collect::<Vec<_>>(),
            ["title", "main", "single"]
        );
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), current(&front)), ("single", "race"));
        // Down to the Garage, in and back out: the Garage row again.
        open(&mut front, "garage", &view, &mut settings);
        assert_eq!(front.page, "garage");
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), current(&front)), ("single", "garage"));
        front.key("Backspace", &view, &mut settings);
        assert_eq!((front.page.as_str(), current(&front)), ("main", "single"));
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "title");
        // Back on the title stays there.
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "title");
        assert_eq!(front.history().count(), 0);
    }

    #[test]
    fn a_page_reached_two_ways_goes_back_the_way_it_came() {
        let (mut front, view, mut settings) = front();
        // Multiplayer → Host Game → Garage, then Single Player → Garage.
        front.show("main", &view);
        open(&mut front, "multiplayer", &view, &mut settings);
        open(&mut front, "create-lobby", &view, &mut settings);
        open(&mut front, "garage", &view, &mut settings);
        front.key("Escape", &view, &mut settings);
        assert_eq!(
            (front.page.as_str(), current(&front)),
            ("create-lobby", "garage")
        );
        // Host Game → Select Track goes back to Host Game, not Single Player.
        front.player_name = "Kim".into();
        open(&mut front, "hosttrack", &view, &mut settings);
        assert_eq!(front.page, "tracks");
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "create-lobby");
        assert!(!front.hosting);
        front.key("Escape", &view, &mut settings);
        front.key("Escape", &view, &mut settings);
        assert_eq!(
            (front.page.as_str(), current(&front)),
            ("main", "multiplayer")
        );
    }

    #[test]
    fn returning_from_a_ride_unwinds_to_the_page_that_started_it() {
        let (mut front, view, mut settings) = front();
        front.show("main", &view);
        open(&mut front, "single", &view, &mut settings);
        open(&mut front, "race", &view, &mut settings);
        select(&mut front, "racestart");
        // The loader replaces the page while the ride runs.
        front.begin_loading("nat01", &view);
        assert_eq!(front.page, "loading");
        // The results' Continue returns to the race setup on Start Race,
        // and Back climbs on from there (`0021ecb0`).
        front.show("race", &view);
        assert_eq!(
            (front.page.as_str(), current(&front)),
            ("race", "racestart")
        );
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), current(&front)), ("single", "race"));
        // Quit to Main Menu from a ride unwinds to the main menu's row.
        open(&mut front, "tracks", &view, &mut settings);
        front.begin_loading("env01", &view);
        front.show("main", &view);
        assert_eq!((front.page.as_str(), current(&front)), ("main", "single"));
        assert_eq!(front.history().collect::<Vec<_>>(), ["title"]);
    }

    #[test]
    fn pages_opened_from_outside_get_their_canonical_parents() {
        let (mut front, view, mut settings) = front();
        front.open("gear", &view);
        assert_eq!(
            front.history().collect::<Vec<_>>(),
            ["title", "main", "single", "garage"]
        );
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "garage");
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), current(&front)), ("single", "garage"));
        // A ride that started from a link returns to its page's parents.
        let (mut front, view, mut settings) = self::front();
        front.begin_loading("nat01", &view);
        front.show("race", &view);
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), current(&front)), ("single", "race"));
    }

    #[test]
    fn confirmations_and_forms_close_back_to_their_owner() {
        let (mut front, view, mut settings) = front();
        front.show("main", &view);
        open(&mut front, "profile", &view, &mut settings);
        open(&mut front, "profile-create", &view, &mut settings);
        assert_eq!(front.page, "profile-name");
        front.key("Escape", &view, &mut settings);
        assert_eq!(
            (front.page.as_str(), current(&front)),
            ("profile", "profile-create")
        );
        // Saving the name returns to the Profile Editor (not a new page
        // over the name entry), so Back then leaves it.
        open(&mut front, "profile-create", &view, &mut settings);
        front.set_profile_name("KIM");
        select(&mut front, "profilesave");
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "profile");
        assert_eq!(front.history().collect::<Vec<_>>(), ["title", "main"]);
        // Delete → slot → No returns to the slot list, then the editor.
        open(&mut front, "profile-delete", &view, &mut settings);
        assert_eq!(front.page, "profile-slots");
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "profile-confirm");
        open(&mut front, "confirmno", &view, &mut settings);
        assert_eq!(front.page, "profile-slots");
        front.key("Escape", &view, &mut settings);
        assert_eq!(
            (front.page.as_str(), current(&front)),
            ("profile", "profile-delete")
        );
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), current(&front)), ("main", "profile"));
    }

    #[test]
    fn the_history_is_bounded() {
        let (mut front, view, _) = front();
        for i in 0..40 {
            front.show(if i % 2 == 0 { "single" } else { "garage" }, &view);
            front.page = format!("page{i}");
        }
        assert!(front.history().count() <= DEPTH);
    }
}
