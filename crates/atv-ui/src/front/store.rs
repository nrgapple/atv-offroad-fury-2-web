//! The original Profile Store pages (Profile Editor → Unlock Items:
//! `QuadzillaUIPageProfileStore_*`) and the unlock gating of the garage,
//! courses and championships (research/PROFILE_STORE.md). A child of the
//! controller so it shares the Frontend state.
use super::Frontend;
use crate::{hud::Viewport, settings::Settings};
use atv_core::store::{self, Atv, CourseList, GAMES, GEAR_PAGES, Purchase, TRACK_PAGES};

/// The store pages ("store" is the Unlock Items menu).
pub fn store_page(page: &str) -> bool {
    matches!(
        page,
        "store" | "store-atv" | "store-gear" | "store-games" | "store-tracks" | "store-cheats"
    )
}

/// Catalogs and the item being browsed.
#[derive(Clone, Debug, Default)]
pub struct StoreView {
    /// uiQuad.txt's ATVs (empty until the frontend art loads).
    pub atvs: Vec<Atv>,
    /// Tracks.dat's course lists.
    pub courses: Vec<CourseList>,
    /// Rider Equipment or Tracks page ("Switch Equipment"/"Switch Event").
    pub section: usize,
    /// The item on the page.
    pub item: usize,
    /// Unlock Items → Cheats: the code being typed.
    pub cheat: String,
}

/// One item on a store page.
pub struct Item {
    pub name: String,
    pub cost: i64,
    pub owned: bool,
    pub for_sale: bool,
    /// "To unlock this …" hint for an item not sold.
    pub how: Option<&'static str>,
}

impl Frontend {
    /// The current store page's items.
    pub fn store_items(&self) -> Vec<Item> {
        let p = &self.profile;
        match self.page.as_str() {
            "store-atv" => {
                let atvs = &self.store.atvs;
                atvs.iter()
                    .enumerate()
                    // Access 3 marks a hidden ATV (the Honda 300EX).
                    .filter(|(_, a)| a.access != 3)
                    .map(|(i, a)| Item {
                        name: a.name.clone(),
                        cost: a.cost,
                        owned: store::atv_owned(atvs, a.id, p),
                        for_sale: store::atv_for_sale(i),
                        how: (!store::atv_for_sale(i)).then_some("Complete the Training Course."),
                    })
                    .collect()
            }
            "store-gear" => {
                let (_, slots) = GEAR_PAGES[self.store.section.min(GEAR_PAGES.len() - 1)];
                let Some(category) = self.gear_catalog.iter().find(|c| c.slot == slots[0]) else {
                    return Vec::new();
                };
                category
                    .items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let sale = store::gear_for_sale(category.slot, i);
                        Item {
                            name: item.name.clone(),
                            cost: i64::from(item.points),
                            owned: store::gear_owned(category, i, p),
                            for_sale: sale,
                            how: (!sale).then_some("Complete the Training Course."),
                        }
                    })
                    .collect()
            }
            "store-tracks" => {
                let (kind, _) = TRACK_PAGES[self.store.section.min(TRACK_PAGES.len() - 1)];
                let Some(list) = self.store.courses.iter().find(|l| l.kind == kind) else {
                    return Vec::new();
                };
                list.courses
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Item {
                        name: c.name.clone(),
                        cost: c.cost,
                        owned: store::course_owned(&self.store.courses, kind, i, p),
                        for_sale: true,
                        how: None,
                    })
                    .collect()
            }
            "store-games" => GAMES
                .iter()
                .map(|(kind, name)| Item {
                    name: name.to_string(),
                    cost: store::EVENT_COST,
                    owned: store::event_open(*kind, p),
                    for_sale: true,
                    how: None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The store page's rows.
    pub(super) fn store_choices(&self) -> Vec<(String, String)> {
        let row = |a: &str, l: &str| (a.to_string(), l.to_string());
        let mut rows = match self.page.as_str() {
            "store" => vec![
                row("store-atv", "ATVs"),
                row("store-gear", "Rider Equipment"),
                row("store-games", "Games"),
                row("store-tracks", "Tracks"),
                row("store-cheats", "Cheats"),
            ],
            "store-cheats" => vec![row("cheatenter", "Enter")],
            _ => {
                let items = self.store_items();
                let mut rows = Vec::new();
                if let Some(item) = items.get(self.store.item) {
                    let state = if item.owned { "Unlocked" } else { "Locked" };
                    rows.push((
                        "storeitem".into(),
                        format!(
                            "{} {}/{} · {state}: {}",
                            match self.page.as_str() {
                                "store-atv" => "ATV",
                                "store-games" => "Game",
                                "store-gear" => GEAR_PAGES[self.store.section].0,
                                _ => "Track",
                            },
                            self.store.item + 1,
                            items.len(),
                            item.name
                        ),
                    ));
                    if !item.owned {
                        rows.push(row("storebuy", "Purchase Item"));
                    }
                } else {
                    rows.push(row("storeitem", "Loading the catalog…"));
                }
                match self.page.as_str() {
                    "store-gear" => rows.push((
                        "storeswitch".into(),
                        format!("Switch Equipment: {}", GEAR_PAGES[self.store.section].0),
                    )),
                    "store-tracks" => rows.push((
                        "storeswitch".into(),
                        format!("Switch Event: {}", TRACK_PAGES[self.store.section].1),
                    )),
                    _ => {}
                }
                rows
            }
        };
        rows.push(row("back", "Back"));
        rows
    }

    /// The readout under the rows: the item's cost and the remaining
    /// points (the store pages' "Point Cost:" and "Current Profile
    /// Points:").
    pub(super) fn store_text(&self) -> Option<String> {
        let points = format!(
            "Current Profile Points: {}",
            store::grouped(store::remaining(&self.profile))
        );
        if matches!(self.page.as_str(), "store" | "store-cheats") {
            return Some(points);
        }
        let items = self.store_items();
        let Some(item) = items.get(self.store.item) else {
            return Some(points);
        };
        let detail = if item.owned {
            "Unlocked".to_string()
        } else if let Some(how) = item.how {
            format!("To unlock this item... {how}")
        } else {
            format!("Point Cost: {}", store::grouped(item.cost))
        };
        Some(format!("{detail} · {points}"))
    }

    /// Opens a store page on its first item.
    pub(super) fn open_store(&mut self, page: &str, view: &Viewport) {
        if page != self.page {
            self.store.item = 0;
            self.store.section = 0;
        }
        self.store.cheat.clear();
        self.show(page, view);
    }

    pub(super) fn store_step(&mut self, direction: i32, view: &Viewport) {
        let n = self.store_items().len() as i32;
        if n > 0 {
            self.store.item = (self.store.item as i32 + direction).rem_euclid(n) as usize;
            self.notice = None;
        }
        self.refresh(view);
    }

    pub(super) fn store_switch(&mut self, view: &Viewport) {
        let n = if self.page == "store-gear" {
            GEAR_PAGES.len()
        } else {
            TRACK_PAGES.len()
        };
        self.store.section = (self.store.section + 1) % n;
        self.store.item = 0;
        self.notice = None;
        self.refresh(view);
    }

    /// Purchase Item (`0029c4d0`, `0029daa0`, `00296ca0`, `0029b3b0`).
    pub(super) fn store_buy(&mut self, view: &Viewport, settings: &mut Settings) {
        let items = self.store_items();
        let Some(item) = items.get(self.store.item) else {
            return;
        };
        let name = item.name.clone();
        let index = self.store.item;
        let result = match self.page.as_str() {
            "store-atv" => {
                // The visible list hides access-3 ATVs; map back.
                let at = self
                    .store
                    .atvs
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.access != 3)
                    .nth(index)
                    .map(|(i, _)| i);
                match at {
                    Some(at) => store::buy_atv(&mut self.profile, &self.store.atvs, at),
                    None => Purchase::NotForSale,
                }
            }
            "store-gear" => store::buy_gear(
                &mut self.profile,
                &self.gear_catalog,
                self.store.section,
                index,
            ),
            "store-tracks" => {
                let (kind, _) = TRACK_PAGES[self.store.section];
                let courses = self.store.courses.clone();
                store::buy_course(&mut self.profile, &courses, kind, index)
            }
            "store-games" => store::buy_event(&mut self.profile, GAMES[index].0),
            _ => return,
        };
        self.notice = Some(match result {
            Purchase::Bought { .. } => {
                crate::settings::save_profile(settings, &self.profile);
                format!("Unlocked {name}!")
            }
            Purchase::AlreadyOwned => format!("{name} is already unlocked."),
            Purchase::Insufficient { cost } => format!(
                "Not enough Profile Points: {name} costs {}.",
                store::grouped(cost)
            ),
            Purchase::NotForSale => "To unlock this item... Complete the Training Course.".into(),
        });
        self.refresh(view);
        // Keep the item row selected (Purchase Item disappears once owned).
        self.selection = 0;
    }

    /// Unlock Items → Cheats → Enter (`0033ede0`).
    pub(super) fn enter_cheat(&mut self, view: &Viewport, settings: &mut Settings) {
        let code = std::mem::take(&mut self.store.cheat);
        self.notice = Some(match store::enter_cheat(&mut self.profile, &code) {
            Some((code, message)) => {
                match code {
                    // The race cheats apply to the next Single Race.
                    "EATDIRT" => self.race.cheats.0 = true,
                    "BRINGIT" => self.race.cheats.1 = true,
                    _ => {}
                }
                crate::settings::save_profile(settings, &self.profile);
                if matches!(
                    code,
                    "WIDESCRN" | "FLYPAPER" | "GETONUP" | "STOPCAM" | "SYNC30HZ"
                ) {
                    format!("{message} (not available in this port)")
                } else {
                    message.to_string()
                }
            }
            None => "Invalid cheat!".into(),
        });
        self.refresh(view);
    }

    /// A store action; true when handled.
    pub(super) fn store_run(
        &mut self,
        action: &str,
        view: &Viewport,
        settings: &mut Settings,
    ) -> bool {
        match action {
            _ if store_page(action) => self.open_store(action, view),
            "storeitem" => self.store_step(1, view),
            "storebuy" => self.store_buy(view, settings),
            "storeswitch" => self.store_switch(view),
            "cheatenter" => self.enter_cheat(view, settings),
            _ => return false,
        }
        true
    }

    /// The page Back returns to from a store page.
    pub(super) fn store_back(&self) -> Option<&'static str> {
        match self.page.as_str() {
            "store" => Some("single"),
            p if store_page(p) => Some("store"),
            _ => None,
        }
    }

    /// The ATV the stage shows: the browsed store ATV on its page.
    pub fn stage_quad(&self) -> u16 {
        if self.page == "store-atv" {
            let visible: Vec<&Atv> = self.store.atvs.iter().filter(|a| a.access != 3).collect();
            if let Some(atv) = visible.get(self.store.item) {
                return atv.id;
            }
        }
        self.quad_id
    }

    // ---- Gating (the profile's unlocks in Single Player) ----

    /// Whether the vehicle may be chosen for a Single Player ride.
    pub fn quad_owned(&self, quad: u16) -> bool {
        store::atv_owned(&self.store.atvs, quad, &self.profile)
    }

    /// The first owned ATV in store order (uiQuad.txt `DefaultQuad_P1` is
    /// its first entry), or ATV01 without a catalog.
    pub(super) fn default_quad(&self) -> u16 {
        self.store
            .atvs
            .iter()
            .find(|a| store::atv_owned(&self.store.atvs, a.id, &self.profile))
            .map_or(1, |a| a.id)
    }

    /// Whether the gear slot's 1-based choice (0 = Original) may be worn.
    pub fn gear_choice_owned(&self, slot: usize, choice: u8) -> bool {
        let Some(index) = usize::from(choice).checked_sub(1) else {
            return true;
        };
        self.gear_catalog
            .get(slot)
            .is_none_or(|c| store::gear_owned(c, index, &self.profile))
    }

    /// A Single Player ride keeps the original unlocks: a locked ATV gives
    /// way to the first owned one and locked gear to the rider's own.
    /// Returns the notice when something changed.
    pub(super) fn enforce_unlocks(&mut self, settings: &mut Settings) -> Option<String> {
        let mut changed = None;
        if !self.quad_owned(self.quad_id) {
            let from = self.vehicle_name(self.quad_id);
            self.quad_id = self.default_quad();
            changed = Some(format!(
                "{from} is locked: riding the {}. Unlock ATVs in Unlock Items.",
                self.vehicle_name(self.quad_id)
            ));
        }
        for slot in 0..self.appearance.gear.len() {
            if !self.gear_choice_owned(slot, self.appearance.gear[slot]) {
                self.appearance.gear[slot] = 0;
                changed.get_or_insert_with(|| "Locked gear was replaced by the original.".into());
            }
        }
        if changed.is_some() {
            self.save(settings);
        }
        changed
    }

    pub(super) fn vehicle_name(&self, quad: u16) -> String {
        self.catalog
            .iter()
            .find(|v| v.id == quad)
            .map_or(format!("Vehicle {quad}"), |v| v.label.clone())
    }

    /// A race course's lock: None when open, else its cost.
    pub fn race_course_cost(&self, id: &str) -> Option<i64> {
        let (kind, index) = store::race_course_entry(&self.store.courses, id)?;
        self.course_cost(kind, index)
    }

    /// A Freestyle course's lock: None when open, else its cost.
    pub fn freestyle_cost(&self, id: &str) -> Option<i64> {
        let (kind, index) = store::freestyle_entry(&self.store.courses, id)?;
        self.course_cost(kind, index)
    }

    pub(super) fn course_cost(&self, kind: u8, index: usize) -> Option<i64> {
        if store::course_owned(&self.store.courses, kind, index, &self.profile) {
            return None;
        }
        self.store
            .courses
            .iter()
            .find(|l| l.kind == kind)
            .and_then(|l| l.courses.get(index))
            .map(|c| c.cost)
    }

    /// The Track store's "To unlock this course..." line.
    pub(super) fn locked_course(cost: i64) -> String {
        format!(
            "To unlock this course... Spend {} Profile Points in Unlock Items.",
            store::grouped(cost)
        )
    }
}
