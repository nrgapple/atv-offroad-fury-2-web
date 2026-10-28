//! The Pass Key pages (authored for the port; research/CLOUD_SAVE.md):
//! this device's key and sync state, Copy, Show QR Code, Share Progress,
//! Enter Pass Key, Enter Share Code, the save files (Export / Import), Save
//! History (restore one of the kept revisions) and the confirmation for a
//! fetched save. They follow the Profile Editor's layout:
//! rows on the left, the details table (or the QR code) on the panel.
use super::{Event, Frontend, Viewport};
use crate::cloud::{Choice, Fetched, Revision, Source};
use crate::settings::Settings;
use atv_core::cloud_save::{self, ALPHABET, KEY_PREFIX, SHARE_PREFIX, SYMBOLS};

/// The Pass Key pages laid out as detail pages.
pub const PAGES: [&str; 5] = [
    "passkey",
    "passkey-entry",
    "passkey-file",
    "passkey-history",
    "passkey-confirm",
];

/// What the menus show of the sync (the host refreshes it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CloudView {
    /// This device's Pass Key.
    pub key: Option<String>,
    /// The sync state's line ("Saved online", "Offline · saved on this
    /// device"…).
    pub status: String,
    /// The badge's word for it ("Saved", "Saving…", "Offline").
    pub short: String,
    /// Everything on the device is on the server.
    pub synced: bool,
    /// The sync failed or the server could not be reached.
    pub trouble: bool,
    pub share_code: String,
    pub previous_key: String,
    /// The origin links point at (the page's own on the web).
    pub origin: String,
    /// The host can read and write save files (web downloads; desktop's
    /// Documents folder).
    pub files: bool,
    /// Where Export writes and Import reads, for the page's line.
    pub files_hint: String,
}

/// What the host must do for a Pass Key page.
#[derive(Clone, Debug, PartialEq)]
pub enum CloudCommand {
    /// Copy this text (the key or a link) to the clipboard.
    Copy(String),
    /// Sync, then make a share code and offer its link.
    Share,
    /// Look up a Pass Key or share code (normalized).
    Fetch(String),
    Export,
    Import,
    /// A fetched save was applied: sync now.
    Applied,
    /// Save History: list the kept revisions.
    History,
    /// Save History: fetch a kept revision for confirmation.
    FetchRevision(u64),
    /// Save History: make a kept revision current and load it here.
    Restore(u64),
}

/// The pages' working state.
#[derive(Clone, Debug, Default)]
pub struct PassKeyUi {
    /// The typed symbols (no prefix or dashes).
    pub entry: String,
    /// The entry takes a share code (else a Pass Key).
    pub share: bool,
    /// The controller row's symbol (index into ALPHABET).
    pub letter: usize,
    /// A save waiting for the player's choice.
    pub pending: Option<Fetched>,
    pub qr: bool,
    /// A lookup is on its way.
    pub busy: bool,
    /// The page Back returns to from the Pass Key page.
    pub back: String,
    /// A link's code to look up once the menus are ready.
    pub link: Option<String>,
    /// Save History's kept revisions, newest first (None while loading).
    pub history: Option<Vec<Revision>>,
}

/// `XXXX-XXXX-…` of typed symbols, with blanks for the rest.
fn grouped_entry(symbols: &str) -> String {
    let padded: String = symbols
        .chars()
        .chain(std::iter::repeat('_'))
        .take(SYMBOLS)
        .collect();
    cloud_save::format("", &padded)
        .trim_start_matches('-')
        .to_string()
}

impl Frontend {
    pub(super) fn is_passkey_page(page: &str) -> bool {
        PAGES.contains(&page)
    }

    /// The badge's pages: the title, main menu and Profile Editor pages.
    pub fn badge_page(&self) -> bool {
        matches!(
            self.page.as_str(),
            "title" | "main" | "profile" | "profile-view"
        ) || Self::is_passkey_page(&self.page)
    }

    /// Opens the Pass Key page (the badge, or Profile Editor → Pass Key).
    pub fn open_passkey(&mut self, view: &Viewport) {
        if !Self::is_passkey_page(&self.page) {
            self.passkey.back = self.page.clone();
        }
        self.passkey.qr = false;
        self.show("passkey", view);
    }

    /// A `#key=` / `#share=` link: look the code up once the menus run.
    pub fn open_link(&mut self, code: &str, view: &Viewport) -> Option<Event> {
        self.passkey.back = "main".into();
        self.passkey.share = code.starts_with(SHARE_PREFIX);
        let prefix = if self.passkey.share {
            SHARE_PREFIX
        } else {
            KEY_PREFIX
        };
        self.passkey.entry = cloud_save::clean_symbols(code, prefix);
        // From outside the menus: Back climbs Pass Key → Main → Title.
        self.open("passkey-entry", view);
        self.lookup()
    }

    /// The host fetched a save (or read a file) for the player to confirm.
    pub fn show_fetched(&mut self, fetched: Fetched, view: &Viewport) {
        self.passkey.busy = false;
        if let (Source::Key(key), Some(mine)) = (&fetched.source, &self.cloud.key)
            && key == mine
        {
            self.show("passkey", view);
            self.notice = Some("That is already this device's Pass Key.".into());
            return;
        }
        if self.passkey.back.is_empty() {
            self.passkey.back = "main".into();
        }
        self.passkey.pending = Some(fetched);
        self.show("passkey-confirm", view);
    }

    /// Save History's list arrived.
    pub fn show_history(&mut self, revisions: Vec<Revision>, view: &Viewport) {
        self.passkey.busy = false;
        self.passkey.history = Some(revisions);
        if self.page == "passkey-history" {
            self.notice = None;
            self.refresh(view);
        }
    }

    /// A revision was restored over this device's save.
    pub fn passkey_restored(&mut self, message: String, view: &Viewport) {
        self.passkey.busy = false;
        self.passkey.pending = None;
        self.passkey.history = None;
        self.show("passkey", view);
        self.notice = Some(message);
    }

    /// A lookup or file read failed: the reason on the page.
    pub fn passkey_message(&mut self, message: String) {
        self.passkey.busy = false;
        self.notice = Some(message);
    }

    fn local_progress(&self) -> bool {
        cloud_save::Save::new(self.book.clone(), self.hall.clone()).has_progress()
    }

    pub(super) fn passkey_choices(&self) -> Vec<(String, String)> {
        let row = |a: &str, l: &str| (a.to_string(), l.to_string());
        let mut rows = match self.page.as_str() {
            "passkey" => {
                let mut rows = Vec::new();
                if self.cloud.key.is_some() {
                    rows.push(row("copykey", "Copy Pass Key"));
                    rows.push(row("sharesave", "Share Progress"));
                    rows.push(row(
                        "showqr",
                        if self.passkey.qr {
                            "Hide QR Code"
                        } else {
                            "Show QR Code"
                        },
                    ));
                }
                rows.push(row("enterkey", "Enter Pass Key"));
                rows.push(row("entershare", "Enter Share Code"));
                if self.cloud.files {
                    rows.push(row("savefiles", "Save Files"));
                }
                if self.cloud.key.is_some() {
                    rows.push(row("savehistory", "Save History"));
                }
                // Unlocks travel with the key (port option).
                rows.push(row(super::profile_pages::UNLOCK_PAGE, "Unlock Everything"));
                rows
            }
            // The current revision is the save itself; the older ones can
            // be restored.
            "passkey-history" => self
                .passkey
                .history
                .iter()
                .flatten()
                .skip(1)
                .map(|r| {
                    (
                        format!("revision-{}", r.revision),
                        format!("#{} {}", r.revision, crate::cloud::when(&r.updated_at)),
                    )
                })
                .collect(),
            "passkey-entry" => {
                let symbol = ALPHABET[self.passkey.letter % ALPHABET.len()] as char;
                vec![
                    ("codeletter".into(), format!("Add Symbol: {symbol}")),
                    row("codeerase", "Delete Symbol"),
                    row(
                        "codeload",
                        if self.passkey.share {
                            "Look Up Share Code"
                        } else {
                            "Look Up Pass Key"
                        },
                    ),
                ]
            }
            "passkey-file" => vec![
                row("exportsave", "Export Save"),
                row("importsave", "Import Save"),
            ],
            "passkey-confirm" => match &self.passkey.pending {
                Some(Fetched {
                    source: Source::Share(_),
                    ..
                }) => {
                    let mut rows = Vec::new();
                    if let Some(name) = self.profile_name() {
                        rows.push(("sharecurrent".into(), format!("Copy Into {name}")));
                    }
                    rows.push(row("sharenew", "Add As New Profile"));
                    rows.push(row("cancel", "Cancel"));
                    rows
                }
                Some(Fetched {
                    source: Source::Revision(_),
                    ..
                }) => vec![row("restore", "Restore This Save"), row("cancel", "Cancel")],
                Some(_) if self.local_progress() => vec![
                    row("merge", "Merge With This Device"),
                    row("replace", "Replace This Device's Save"),
                    row("cancel", "Cancel"),
                ],
                Some(_) => vec![row("replace", "Load This Save"), row("cancel", "Cancel")],
                None => Vec::new(),
            },
            _ => Vec::new(),
        };
        rows.push(row("back", "Back"));
        rows
    }

    pub(super) fn passkey_heading(&self) -> Option<String> {
        Some(
            match self.page.as_str() {
                "passkey-entry" if self.passkey.share => "Enter Share Code",
                "passkey-entry" => "Enter Pass Key",
                "passkey-file" => "Save Files",
                "passkey-history" => "Save History",
                "passkey-confirm" => match self.passkey.pending.as_ref().map(|p| &p.source) {
                    Some(Source::Share(_)) => "Load Shared Progress",
                    Some(Source::File(_)) => "Import Save",
                    Some(Source::Revision(_)) => "Restore Save",
                    _ => "Load Pass Key",
                },
                _ => return None,
            }
            .into(),
        )
    }

    pub(super) fn passkey_status(&self) -> Option<String> {
        match self.page.as_str() {
            "passkey-entry" => {
                Some("16 letters and numbers. O reads as 0; I and L read as 1.".into())
            }
            _ => None,
        }
    }

    pub(super) fn passkey_details(&self) -> Vec<(String, String)> {
        let line = |l: &str, v: String| (l.to_string(), v);
        match self.page.as_str() {
            "passkey" => {
                let mut lines = Vec::new();
                match &self.cloud.key {
                    Some(key) => lines.push(line("Your Pass Key", key.clone())),
                    None => {
                        lines.push(line("No Pass Key yet", String::new()));
                        lines.push(line(
                            "Your Pass Key appears once you save progress.",
                            String::new(),
                        ));
                    }
                }
                if self.cloud.key.is_some() {
                    lines.push(line("Status", self.cloud.status.clone()));
                }
                if self.passkey.qr {
                    return lines;
                }
                if !self.cloud.share_code.is_empty() {
                    lines.push(line("Last Share Code", self.cloud.share_code.clone()));
                }
                if !self.cloud.previous_key.is_empty() {
                    lines.push(line("Previous Pass Key", self.cloud.previous_key.clone()));
                }
                lines.push(line(
                    "Enter your Pass Key on another device or browser to continue this save. \
                     Anyone with it can continue it; Share Progress gives others a copy instead.",
                    String::new(),
                ));
                lines
            }
            "passkey-entry" => {
                let prefix = if self.passkey.share {
                    SHARE_PREFIX
                } else {
                    KEY_PREFIX
                };
                vec![line(
                    if self.passkey.share {
                        "Share Code"
                    } else {
                        "Pass Key"
                    },
                    format!("{prefix}-{}", grouped_entry(&self.passkey.entry)),
                )]
            }
            "passkey-file" => vec![line(&self.cloud.files_hint, String::new())],
            "passkey-history" => self.history_details(),
            "passkey-confirm" => self.confirm_details(),
            _ => Vec::new(),
        }
    }

    fn confirm_details(&self) -> Vec<(String, String)> {
        let Some(fetched) = &self.passkey.pending else {
            return Vec::new();
        };
        let line = |l: &str, v: String| (l.to_string(), v);
        let mut lines = vec![match &fetched.source {
            Source::Key(key) => line("Pass Key", key.clone()),
            Source::Share(code) => line("Share Code", code.clone()),
            Source::File(name) => line("File", name.clone()),
            Source::Revision(n) => line(
                "Saved",
                format!("{} (revision {n})", crate::cloud::when(&fetched.updated_at)),
            ),
        }];
        let save = &fetched.save;
        let names: Vec<&str> = save
            .profiles
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        lines.push(line(
            "Profiles",
            if names.is_empty() {
                "None".into()
            } else {
                names.join(", ")
            },
        ));
        if let Some(p) = save.main_profile() {
            lines.push(line(
                "Percent Complete",
                format!("{:.0}%", p.percent_complete()),
            ));
            lines.push(line(
                "Profile Points",
                super::profile_pages::grouped(p.points),
            ));
        }
        lines.push(line(
            "Hall of Fame Records",
            save.fame_records().to_string(),
        ));
        let replace = matches!(fetched.source, Source::Key(_) | Source::File(_));
        if replace && self.local_progress() {
            lines.push(line(
                if self.cloud.synced || self.cloud.key.is_none() {
                    "Merge keeps both saves' progress. Replace swaps this device's save for this one."
                } else {
                    "Merge keeps both saves' progress. Replace loses this device's progress that \
                     is not saved online yet."
                },
                String::new(),
            ));
        }
        if let Source::Revision(_) = fetched.source {
            lines.push(line(
                if self.cloud.synced {
                    "Restore makes this the online save and replaces this device's. The save it \
                     replaces stays in Save History."
                } else {
                    "Restore makes this the online save and replaces this device's. Progress on \
                     this device that is not saved online yet is lost."
                },
                String::new(),
            ));
        }
        if let Source::Share(_) = fetched.source {
            lines.push(line(
                "Shared progress is copied into your own save; your Pass Key stays the same.",
                String::new(),
            ));
        }
        lines
    }

    fn history_details(&self) -> Vec<(String, String)> {
        let line = |l: &str, v: String| (l.to_string(), v);
        let Some(history) = &self.passkey.history else {
            return vec![line("Loading Save History…", String::new())];
        };
        let mut lines = Vec::new();
        if let Some(current) = history.first() {
            lines.push(line(
                "Current Save",
                format!(
                    "{} (revision {})",
                    crate::cloud::when(&current.updated_at),
                    current.revision
                ),
            ));
        }
        lines.push(line(
            if history.len() > 1 {
                "The server keeps the last 10 saves of this Pass Key. Choose one to see it \
                 before restoring it, for example if someone with your key changed your progress."
            } else {
                "No earlier saves are kept yet. The server keeps the last 10 saves of this \
                 Pass Key."
            },
            String::new(),
        ));
        lines
    }

    /// The link a QR code shows (this device's key).
    pub fn passkey_qr(&self) -> Option<String> {
        (self.page == "passkey" && self.passkey.qr)
            .then(|| {
                self.cloud
                    .key
                    .as_ref()
                    .map(|k| crate::cloud::link(&self.cloud.origin, k))
            })
            .flatten()
    }

    fn lookup(&mut self) -> Option<Event> {
        let prefix = if self.passkey.share {
            SHARE_PREFIX
        } else {
            KEY_PREFIX
        };
        let Some(code) = cloud_save::normalize(&self.passkey.entry, prefix) else {
            self.notice = Some(format!(
                "Enter all 16 symbols of the {}.",
                if self.passkey.share {
                    "share code"
                } else {
                    "Pass Key"
                }
            ));
            return None;
        };
        if !self.passkey.share && self.cloud.key.as_deref() == Some(code.as_str()) {
            self.notice = Some("That is already this device's Pass Key.".into());
            return None;
        }
        self.passkey.busy = true;
        self.notice = Some("Looking up…".into());
        Some(Event::Cloud(CloudCommand::Fetch(code)))
    }

    /// The entry's text (typed or pasted into the field, or a link).
    pub(super) fn set_passkey_entry(&mut self, text: &str) {
        let prefix = if self.passkey.share {
            SHARE_PREFIX
        } else {
            KEY_PREFIX
        };
        self.passkey.entry = cloud_save::clean_symbols(text, prefix);
        if !self.passkey.busy {
            self.notice = None;
        }
    }

    /// Left/Right on the symbol row.
    pub(super) fn step_passkey(&mut self, direction: i32, view: &Viewport) {
        let n = ALPHABET.len() as i32;
        self.passkey.letter = (self.passkey.letter as i32 + direction).rem_euclid(n) as usize;
        self.refresh(view);
    }

    /// A Pass Key page's row; None when the action is not one of them.
    pub(super) fn passkey_action(
        &mut self,
        action: &str,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Option<Event>> {
        let choice = match action {
            "passkey" => {
                self.open_passkey(view);
                return Some(None);
            }
            "copykey" => {
                return Some(
                    self.cloud
                        .key
                        .clone()
                        .map(|k| Event::Cloud(CloudCommand::Copy(k))),
                );
            }
            "sharesave" => {
                self.notice = Some("Making a share code…".into());
                return Some(Some(Event::Cloud(CloudCommand::Share)));
            }
            "showqr" => {
                self.passkey.qr = !self.passkey.qr;
                self.refresh(view);
                return Some(None);
            }
            "enterkey" | "entershare" => {
                self.passkey.share = action == "entershare";
                self.passkey.entry.clear();
                self.passkey.busy = false;
                self.show("passkey-entry", view);
                return Some(None);
            }
            "savefiles" => {
                self.show("passkey-file", view);
                return Some(None);
            }
            "savehistory" => {
                self.passkey.history = None;
                self.passkey.busy = true;
                self.show("passkey-history", view);
                return Some(Some(Event::Cloud(CloudCommand::History)));
            }
            _ if action.starts_with("revision-") => {
                let n = action["revision-".len()..].parse().ok()?;
                if self.passkey.busy {
                    return Some(None);
                }
                self.passkey.busy = true;
                self.notice = Some("Looking up…".into());
                return Some(Some(Event::Cloud(CloudCommand::FetchRevision(n))));
            }
            "restore" => {
                let Some(Fetched {
                    source: Source::Revision(n),
                    ..
                }) = self.passkey.pending
                else {
                    return Some(None);
                };
                if self.passkey.busy {
                    return Some(None);
                }
                self.passkey.busy = true;
                self.notice = Some("Restoring…".into());
                return Some(Some(Event::Cloud(CloudCommand::Restore(n))));
            }
            "exportsave" => return Some(Some(Event::Cloud(CloudCommand::Export))),
            "importsave" => return Some(Some(Event::Cloud(CloudCommand::Import))),
            "codeletter" => {
                if self.passkey.entry.len() < SYMBOLS {
                    let symbol = ALPHABET[self.passkey.letter % ALPHABET.len()] as char;
                    self.passkey.entry.push(symbol);
                }
                self.notice = None;
                self.refresh(view);
                return Some(None);
            }
            "codeerase" => {
                self.passkey.entry.pop();
                self.notice = None;
                self.refresh(view);
                return Some(None);
            }
            "codeload" => {
                if self.passkey.busy {
                    return Some(None);
                }
                return Some(self.lookup());
            }
            "cancel" if self.page == "passkey-confirm" => {
                let back = match self.passkey.pending.take() {
                    Some(Fetched {
                        source: Source::Revision(_),
                        ..
                    }) => "passkey-history",
                    _ => "passkey",
                };
                self.show(back, view);
                return Some(None);
            }
            "merge" => Choice::Merge,
            "replace" => Choice::Replace,
            "sharecurrent" => Choice::ShareIntoCurrent,
            "sharenew" => Choice::ShareAsNew,
            _ => return None,
        };
        let pending = self.passkey.pending.clone()?;
        match crate::cloud::apply(settings, &pending, choice) {
            Ok(message) => {
                self.passkey.pending = None;
                self.load_profiles(settings);
                self.show("passkey", view);
                self.notice = Some(message);
                Some(Some(Event::Cloud(CloudCommand::Applied)))
            }
            Err(message) => {
                self.notice = Some(message);
                Some(None)
            }
        }
    }

    pub(super) fn passkey_back(&self) -> Option<String> {
        Some(match self.page.as_str() {
            "passkey" => {
                if self.passkey.back.is_empty() {
                    "main".into()
                } else {
                    self.passkey.back.clone()
                }
            }
            "passkey-confirm"
                if matches!(
                    self.passkey.pending.as_ref().map(|p| &p.source),
                    Some(Source::Revision(_))
                ) =>
            {
                "passkey-history".into()
            }
            "passkey-entry" | "passkey-file" | "passkey-history" | "passkey-confirm" => {
                "passkey".into()
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::Fetched;

    fn view() -> Viewport {
        Viewport {
            width: 1280.,
            height: 720.,
            scale: 1.,
            ..Default::default()
        }
    }

    fn actions(front: &Frontend) -> Vec<&str> {
        front.choices().iter().map(|(a, _)| a.as_str()).collect()
    }

    #[test]
    fn the_page_offers_entry_before_a_key_exists() {
        let view = view();
        let mut settings = Settings::memory();
        let mut front = Frontend::new(Vec::new(), &settings, true);
        front.show("profile", &view);
        assert!(actions(&front).contains(&"passkey"));
        front.run("passkey", &view, &mut settings);
        assert_eq!(front.page, "passkey");
        assert_eq!(
            actions(&front),
            ["enterkey", "entershare", "profile-unlock", "back"]
        );
        assert_eq!(front.profile_details()[0].0, "No Pass Key yet");
        front.cloud.key = Some("FURY-28T5-CY4T-QKFF-03ZD".into());
        front.cloud.files = true;
        front.refresh(&view);
        assert_eq!(
            actions(&front),
            [
                "copykey",
                "sharesave",
                "showqr",
                "enterkey",
                "entershare",
                "savefiles",
                "savehistory",
                "profile-unlock",
                "back"
            ]
        );
        assert_eq!(
            front.run("copykey", &view, &mut settings),
            Some(Event::Cloud(CloudCommand::Copy(
                "FURY-28T5-CY4T-QKFF-03ZD".into()
            )))
        );
        front.cloud.origin = "https://furyremastered.com".into();
        front.run("showqr", &view, &mut settings);
        assert_eq!(
            front.passkey_qr().as_deref(),
            Some("https://furyremastered.com/play.html#key=FURY-28T5-CY4T-QKFF-03ZD")
        );
        assert_eq!(front.back(&view), None);
        assert_eq!(front.page, "profile");
    }

    #[test]
    fn keys_are_entered_by_typing_or_with_the_symbol_row() {
        let view = view();
        let mut settings = Settings::memory();
        let mut front = Frontend::new(Vec::new(), &settings, true);
        front.open_passkey(&view);
        front.run("enterkey", &view, &mut settings);
        assert_eq!(front.page, "passkey-entry");
        assert_eq!(front.fields()[0].action, "passkey-code");
        front.set_field("passkey-code", "fury-28t5-cy4t-qkff-o3z");
        assert_eq!(front.passkey.entry, "28T5CY4TQKFF03Z");
        assert_eq!(front.profile_details()[0].1, "FURY-28T5-CY4T-QKFF-03Z_");
        assert_eq!(front.run("codeload", &view, &mut settings), None);
        assert!(front.notice.as_deref().unwrap().contains("16 symbols"));
        // The controller row: Right steps to "1", accept adds it.
        front.step("codeletter", 1, &view, &mut settings);
        assert_eq!(front.choices()[0].1, "Add Symbol: 1");
        front.run("codeletter", &view, &mut settings);
        assert_eq!(
            front.run("codeload", &view, &mut settings),
            Some(Event::Cloud(CloudCommand::Fetch(
                "FURY-28T5-CY4T-QKFF-03Z1".into()
            )))
        );
        assert!(front.passkey.busy);
        front.run("codeerase", &view, &mut settings);
        assert_eq!(front.passkey.entry.len(), 15);
    }

    #[test]
    fn a_fetched_key_is_confirmed_then_applied() {
        let view = view();
        let mut settings = Settings::memory();
        let mut front = Frontend::new(Vec::new(), &settings, true);
        // The other device's progress.
        let mut book = atv_core::profile::ProfileBook::migrate(None);
        book.current_mut().unwrap().credit(77);
        let save = cloud_save::Save::new(book, Default::default());
        let fetched = Fetched {
            source: Source::Key("FURY-28T5-CY4T-QKFF-03ZD".into()),
            revision: 4,
            updated_at: String::new(),
            save,
        };
        front.open_passkey(&view);
        front.show_fetched(fetched.clone(), &view);
        assert_eq!(front.page, "passkey-confirm");
        // No progress here: just Load.
        assert_eq!(actions(&front), ["replace", "cancel", "back"]);
        assert_eq!(front.profile_heading().as_deref(), Some("Load Pass Key"));
        assert_eq!(
            front.run("replace", &view, &mut settings),
            Some(Event::Cloud(CloudCommand::Applied))
        );
        assert_eq!(front.page, "passkey");
        assert_eq!(front.profile.points, 77);
        assert_eq!(
            crate::cloud::pass_key(&settings).as_deref(),
            Some("FURY-28T5-CY4T-QKFF-03ZD")
        );
        assert_eq!(crate::cloud::record(&settings).revision, 4);
        // With progress here, Merge and Replace are offered.
        let mut fetched = fetched;
        fetched.source = Source::File("save.json".into());
        front.show_fetched(fetched, &view);
        assert_eq!(actions(&front), ["merge", "replace", "cancel", "back"]);
        front.run("cancel", &view, &mut settings);
        assert_eq!(front.page, "passkey");
        assert!(front.passkey.pending.is_none());
    }

    #[test]
    fn share_codes_copy_into_the_loaded_profile_or_a_new_one() {
        let view = view();
        let mut settings = Settings::memory();
        let mut front = Frontend::new(Vec::new(), &settings, true);
        let mut book = atv_core::profile::ProfileBook::migrate(None);
        book.current_mut().unwrap().credit(500);
        let fetched = Fetched {
            source: Source::Share("SHARE-AAAA-BBBB-CCCC-DDDD".into()),
            revision: 0,
            updated_at: String::new(),
            save: cloud_save::Save::new(book, Default::default()),
        };
        front.show_fetched(fetched.clone(), &view);
        assert_eq!(
            actions(&front),
            ["sharecurrent", "sharenew", "cancel", "back"]
        );
        assert_eq!(front.choices()[0].1, "Copy Into PLAYER1");
        front.run("sharenew", &view, &mut settings);
        assert_eq!(front.profile_name(), Some("PLAYER12"));
        assert_eq!(front.profile.points, 500);
        assert!(crate::cloud::pass_key(&settings).is_none());
    }

    #[test]
    fn links_open_the_lookup() {
        let view = view();
        let settings = Settings::memory();
        let mut front = Frontend::new(Vec::new(), &settings, true);
        let event = front.open_link("SHARE-AAAA-BBBB-CCCC-DDDD", &view);
        assert_eq!(front.page, "passkey-entry");
        assert!(front.passkey.share);
        assert_eq!(
            event,
            Some(Event::Cloud(CloudCommand::Fetch(
                "SHARE-AAAA-BBBB-CCCC-DDDD".into()
            )))
        );
        assert_eq!(grouped_entry("AB"), "AB__-____-____-____");
    }
}
