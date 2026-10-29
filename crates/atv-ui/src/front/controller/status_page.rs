//! The status bar's state and taps (front/status_bar.rs draws it): which
//! pages show it, its numbers, and the popover each chip opens.
use super::Frontend;
use crate::front::status_bar::{
    self as bar, Chip, POPOVER_CLOSE, POPOVER_LINK, POPOVER_PANEL, Popover, StatusBar, Sync,
};
use crate::{hud::Viewport, paint::Rect};

impl Frontend {
    /// Menu pages show the bar; the title shows the save chip alone; the
    /// course loader (a ride starting) shows none.
    fn status_page(&self) -> bool {
        self.page != "loading"
    }

    /// The Pass Key sync state as the save chip shows it.
    pub fn sync_state(&self) -> Sync {
        let c = &self.cloud;
        if c.key.is_none() {
            Sync::NoKey
        } else if c.synced {
            Sync::Synced
        } else if c.trouble {
            if c.status.starts_with("Offline") {
                Sync::Offline
            } else {
                Sync::Error
            }
        } else {
            Sync::Syncing
        }
    }

    /// Championship trophies won (any trophy, Normal and Expert) of those
    /// the port offers.
    pub fn trophies_won(&self) -> (u32, u32) {
        let champ = &self.profile.championships;
        let mut won = 0;
        let mut total = 0;
        for series in &atv_core::championship::SERIES {
            for expert in [false, true] {
                total += 1;
                if self.profile_name().is_some() && champ.progress(series, expert).trophy.is_some()
                {
                    won += 1;
                }
            }
        }
        (won, total)
    }

    /// Profile Points to spend (0 without a profile).
    pub fn status_points(&self) -> i64 {
        if self.profile_name().is_some() {
            atv_core::store::remaining(&self.profile)
        } else {
            0
        }
    }

    /// The bar for the current page (None: the page shows none).
    /// `shown_points` is the counting value the host animates.
    pub fn status_bar(&self) -> Option<StatusBar> {
        if !self.status_page() {
            return None;
        }
        Some(StatusBar {
            profile: self.profile_name().map(String::from),
            points: self.status_points(),
            shown_points: self.status_points(),
            counting: false,
            trophies: self.trophies_won(),
            sync: self.sync_state(),
            minimal: self.page == "title",
            store: super::store_page(&self.page),
            open: self.status_open.and_then(|chip| self.popover(chip)),
            anchor: self.status_anchor,
        })
    }

    fn popover(&self, chip: Chip) -> Option<Popover> {
        let line = |l: &str, v: String| (l.to_string(), v);
        let loaded = self.profile_name().is_some();
        let p = &self.profile;
        Some(match chip {
            Chip::Profile => Popover {
                chip: Some(chip),
                title: self.profile_name().unwrap_or("No profile loaded").into(),
                lines: if loaded {
                    vec![
                        line("Races", p.races.to_string()),
                        line("Wins", p.wins.to_string()),
                        line("Points earned", bar::grouped(p.earned.max(p.points))),
                        line("Best stunt", bar::grouped(p.best_stunt)),
                    ]
                } else {
                    vec![line("Progress", "Not being saved".into())]
                },
                link: Some(
                    if loaded {
                        "View Profile"
                    } else {
                        "Profile Editor"
                    }
                    .into(),
                ),
            },
            Chip::Coins => Popover {
                chip: Some(chip),
                title: "Profile Points".into(),
                lines: if loaded {
                    vec![
                        line("Available", bar::grouped(self.status_points())),
                        line("Earned", bar::grouped(p.earned.max(p.points))),
                        line("Spent", bar::grouped(p.store.spent)),
                    ]
                } else {
                    vec![line("Load a profile", "to earn points".into())]
                },
                link: Some(
                    if loaded {
                        "Profile Store"
                    } else {
                        "Profile Editor"
                    }
                    .into(),
                ),
            },
            Chip::Trophies => {
                let (won, total) = self.trophies_won();
                let mut lines = vec![line("Won", format!("{won} of {total}"))];
                for series in &atv_core::championship::SERIES {
                    let name = |expert| {
                        if !loaded {
                            return "None";
                        }
                        p.championships
                            .progress(series, expert)
                            .trophy
                            .map_or("None", |t| t.name())
                    };
                    lines.push(line(
                        series.name,
                        format!("{} / {}", name(false), name(true)),
                    ));
                }
                Popover {
                    chip: Some(chip),
                    title: "Trophies · Normal / Expert".into(),
                    lines,
                    link: Some(
                        if loaded {
                            "View Trophies"
                        } else {
                            "Profile Editor"
                        }
                        .into(),
                    ),
                }
            }
            Chip::Save => Popover {
                chip: Some(chip),
                title: "Pass Key".into(),
                lines: vec![
                    line(
                        "Status",
                        if self.cloud.status.is_empty() {
                            self.sync_state().word().to_string()
                        } else {
                            self.cloud.status.clone()
                        },
                    ),
                    line(
                        "Key",
                        self.cloud
                            .key
                            .clone()
                            .unwrap_or_else(|| "Made with your first save".into()),
                    ),
                ],
                link: Some("Open Pass Key".into()),
            },
        })
    }

    /// A popover is open.
    pub fn status_open(&self) -> bool {
        self.status_open.is_some()
    }

    pub fn close_status(&mut self) {
        self.status_open = None;
    }

    /// A tap on the bar or its popover; true when it was one. `rect` is the
    /// tapped target's box (a chip's centre anchors its popover).
    pub(super) fn status_click(&mut self, id: u32, rect: Option<Rect>, view: &Viewport) -> bool {
        if let Some(chip) = Chip::from_id(id) {
            if self.status_open == Some(chip) {
                self.status_open = None;
            } else {
                self.status_open = Some(chip);
                if let Some(r) = rect {
                    self.status_anchor = r.x + r.w / 2.;
                }
            }
            return true;
        }
        match id {
            POPOVER_CLOSE => self.status_open = None,
            POPOVER_PANEL => {}
            POPOVER_LINK => {
                let Some(chip) = self.status_open.take() else {
                    return true;
                };
                let loaded = self.profile_name().is_some();
                match chip {
                    Chip::Save => self.open_passkey(view),
                    _ if !loaded => self.show("profile", view),
                    Chip::Profile => self.show("profile-view", view),
                    Chip::Coins => self.show_store("store", view),
                    Chip::Trophies => self.show("profile-trophies", view),
                }
            }
            _ => return false,
        }
        true
    }
}
