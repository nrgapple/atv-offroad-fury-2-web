//! The original Profile Editor (PAGE19 `Profile_Main`) and its pages: name
//! entry (PAGE21), Load/Copy/Delete (PAGE22, 23, 53) and View Profile
//! (PAGE24 `ProfileView_Menu`, PAGE25 Overview, PAGE26 Trophies and the
//! per-event Records pages PAGE27-35). Rows, labels and alert text follow
//! the exact build's page labels and executable strings (research/PROFILE.md);
//! the memory card is the port's local storage (one card of four saves).
use super::{Event, Frontend, Viewport};
use crate::settings::{Settings, load_book, save_book};
use atv_core::profile::{BookError, NAME_CHARS, NAME_MAX, Profile, ProfileBook, clean_name};

/// Profile pages laid out as the editor's: rows on the left and the
/// details table on the panel.
pub const DETAIL_PAGES: [&str; 6] = [
    "profile-name",
    "profile-slots",
    "profile-confirm",
    "profile-overview",
    "profile-trophies",
    "profile-records",
];

/// What the slot list picks a profile for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SlotMode {
    #[default]
    Load,
    Copy,
    Delete,
}

/// What the name entry saves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NameFor {
    #[default]
    Create,
    Copy(usize),
}

/// A Yes/No alert and what Yes does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    Load(usize),
    Delete(usize),
}

/// The Profile Editor's working state.
#[derive(Clone, Debug, Default)]
pub struct ProfileUi {
    pub slots: SlotMode,
    pub name_for: NameFor,
    /// The name being entered.
    pub name: String,
    /// The controller letter row's character (index into NAME_CHARS).
    pub letter: usize,
    pub confirm: Option<Confirm>,
    /// Records: the event group and the course within it.
    pub group: usize,
    pub course: usize,
}

/// Records' event groups (the ProfileView_* event pages the port has
/// courses for), each with its course ids in page order.
pub fn record_groups(race_courses: &[String]) -> Vec<(&'static str, Vec<String>)> {
    use atv_game::launch::{RaceVariant, split_race_course};
    let pick = |f: &dyn Fn(&str, RaceVariant) -> bool| -> Vec<String> {
        race_courses
            .iter()
            .filter(|id| split_race_course(id).is_some_and(|(venue, v)| f(venue, v)))
            .cloned()
            .collect()
    };
    let in_series = |key: &str| {
        let series = atv_core::championship::series(key).map_or(&[][..], |s| s.courses);
        let mut ids: Vec<String> = series
            .iter()
            .filter(|c| race_courses.iter().any(|r| r == *c))
            .map(|c| c.to_string())
            .collect();
        // Courses of the event the series does not race (`Tracks.dat` lists
        // more than a championship plays).
        ids.extend(pick(&|venue, v| {
            v == RaceVariant::National
                && venue.starts_with("sx")
                && key == "amsx"
                && !atv_core::championship::SERIES
                    .iter()
                    .any(|s| s.courses.contains(&venue))
        }));
        ids
    };
    let national = |prefix: &'static str| {
        move |venue: &str, v: RaceVariant| v == RaceVariant::National && venue.starts_with(prefix)
    };
    let groups = [
        ("Amateur Nationals", pick(&national("env"))),
        ("Pro Nationals", pick(&national("nat"))),
        (
            "Amateur Enduro",
            pick(&|_, v| v == RaceVariant::EnduroShort),
        ),
        ("Pro Enduro", pick(&|_, v| v == RaceVariant::EnduroLong)),
        ("Amateur Supercross", in_series("amsx")),
        ("Pro Supercross", in_series("prosx")),
        ("Short Tracks", pick(&|_, v| v == RaceVariant::ShortTrack)),
    ];
    groups.into_iter().filter(|(_, c)| !c.is_empty()).collect()
}

/// `##:##.##`, or `--:--.--` with no time.
fn race_time(ms: Option<u32>) -> String {
    match ms {
        Some(ms) => {
            let centis = ms / 10;
            format!(
                "{:02}:{:02}.{:02}",
                centis / 6000,
                centis / 100 % 60,
                centis % 100
            )
        }
        None => "--:--.--".into(),
    }
}

/// `###,###,###`.
pub fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// Not measured by the port (jumps, combos and Freestyle objectives).
const UNMEASURED: &str = "-";

impl Frontend {
    /// Reloads the profile book and the loaded profile (on start, and back
    /// from a ride, which may have credited a race).
    pub fn load_profiles(&mut self, settings: &Settings) {
        self.book = load_book(settings);
        self.profile = self.book.current_profile().cloned().unwrap_or_default();
    }

    fn store_book(&mut self, settings: &mut Settings, book: ProfileBook) {
        save_book(settings, &book);
        self.book = book;
        self.profile = self.book.current_profile().cloned().unwrap_or_default();
    }

    /// The loaded profile's name, if any.
    pub fn profile_name(&self) -> Option<&str> {
        self.book.current_profile().map(|p| p.name.as_str())
    }

    pub(super) fn is_profile_page(page: &str) -> bool {
        page == "profile" || page == "profile-view" || DETAIL_PAGES.contains(&page)
    }

    /// The rows of a Profile Editor page (keyboard order).
    pub(super) fn profile_choices(&self) -> Vec<(String, String)> {
        let row = |a: &str, l: &str| (a.to_string(), l.to_string());
        let mut rows = match self.page.as_str() {
            // PAGE19 `Profile_Main`. "Save Current Profile" is left out: the
            // port saves the loaded profile after every change (AutoSave).
            "profile" => vec![
                row("profile-create", "Create New Profile"),
                row("profile-load", "Load Profile"),
                row("profile-view", "View Current Profile"),
                row("profile-store", "Unlock Items"),
                row("profile-copy", "Copy Profile"),
                row("profile-delete", "Delete Profile"),
            ],
            // PAGE21: the name, then the controller's letter row.
            "profile-name" => {
                let letter = NAME_CHARS
                    .chars()
                    .nth(self.profile_ui.letter)
                    .unwrap_or('A');
                let shown = if letter == ' ' {
                    "Space".to_string()
                } else {
                    letter.to_string()
                };
                vec![
                    ("profileletter".into(), format!("Add Letter: {shown}")),
                    row("profileerase", "Delete Letter"),
                    row(
                        "profilesave",
                        match self.profile_ui.name_for {
                            NameFor::Create => "Save Profile",
                            NameFor::Copy(_) => "Save Copy",
                        },
                    ),
                ]
            }
            "profile-slots" => self
                .book
                .profiles
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let loaded = if self.book.current == Some(i) {
                        " (loaded)"
                    } else {
                        ""
                    };
                    (
                        format!("slot:{i}"),
                        format!("{}{loaded} · {} pts", p.name, grouped(p.points)),
                    )
                })
                .collect(),
            "profile-confirm" => vec![row("confirmyes", "Yes"), row("confirmno", "No")],
            // PAGE24 `ProfileView_Menu`. ATVs, Rider Equipment and Games list
            // Profile Store ownership (PROFILE_STORE.md).
            "profile-view" => vec![
                row("view-overview", "Overview"),
                row("view-trophies", "Trophies"),
                row("view-records", "Records"),
                row("view-atvs", "ATVs"),
                row("view-gear", "Rider Equipment"),
                row("view-games", "Games"),
            ],
            "profile-records" => {
                let groups = record_groups(&self.race_courses);
                let group = groups.get(self.profile_ui.group);
                let course = group.and_then(|(_, c)| c.get(self.profile_ui.course));
                vec![
                    (
                        "recordevent".into(),
                        format!("Event: {}", group.map_or("None", |g| g.0)),
                    ),
                    (
                        "recordcourse".into(),
                        format!(
                            "Course: {}",
                            course.map_or("None".into(), |c| self.course_name(c))
                        ),
                    ),
                ]
            }
            _ => Vec::new(),
        };
        rows.push(row("back", "Back"));
        rows
    }

    /// Load/Copy/Delete Profile titles (PAGE22, 23, 53 share one layout).
    pub fn profile_heading(&self) -> Option<String> {
        if !matches!(self.page.as_str(), "profile-slots" | "profile-confirm") {
            return None;
        }
        Some(
            match (self.profile_ui.slots, self.profile_ui.confirm) {
                (_, Some(Confirm::Delete(_))) | (SlotMode::Delete, _) => "Delete Profile",
                (_, Some(Confirm::Load(_))) | (SlotMode::Load, _) => "Load Profile",
                (SlotMode::Copy, _) => "Copy Profile",
            }
            .into(),
        )
    }

    /// The line under the Profile Editor's rows: the loaded profile.
    pub(super) fn profile_status(&self) -> Option<String> {
        match self.page.as_str() {
            "profile" | "profile-view" => Some(match self.book.current_profile() {
                Some(p) => format!(
                    "Current Profile: {} · Current Profile Points: {}",
                    p.name,
                    grouped(p.points)
                ),
                None => "No profile loaded".into(),
            }),
            "profile-name" => Some(format!("Up to {NAME_MAX} letters, numbers or - _ . !",)),
            _ => None,
        }
    }

    /// The panel's details table (label, value) for the page.
    pub fn profile_details(&self) -> Vec<(String, String)> {
        let line = |l: &str, v: String| (l.to_string(), v);
        let current = self.book.current_profile();
        match self.page.as_str() {
            "profile-name" => vec![
                line(
                    match self.profile_ui.name_for {
                        NameFor::Create => "New profile",
                        NameFor::Copy(_) => "Copy of",
                    },
                    match self.profile_ui.name_for {
                        NameFor::Create => String::new(),
                        NameFor::Copy(i) => self
                            .book
                            .profiles
                            .get(i)
                            .map_or(String::new(), |p| p.name.clone()),
                    },
                ),
                line("Name", self.profile_ui.name.clone()),
                line(
                    "Player Profile save file",
                    format!(
                        "{} of {}",
                        (self.book.profiles.len() + 1).min(atv_core::profile::SLOTS),
                        atv_core::profile::SLOTS
                    ),
                ),
            ],
            "profile-slots" => {
                let mut lines = vec![line(
                    match self.profile_ui.slots {
                        SlotMode::Load => "Choose a profile to load",
                        SlotMode::Copy => "Choose a profile to copy",
                        SlotMode::Delete => "Choose a profile to delete",
                    },
                    String::new(),
                )];
                if let Some((action, _)) = self.choices.get(self.selection)
                    && let Some(p) = action
                        .strip_prefix("slot:")
                        .and_then(|i| i.parse::<usize>().ok())
                        .and_then(|i| self.book.profiles.get(i))
                {
                    lines.extend(self.summary(p));
                }
                lines
            }
            "profile-confirm" => match self.profile_ui.confirm {
                Some(Confirm::Delete(i)) => vec![
                    line("Delete this save file?", String::new()),
                    line(
                        "Player Profile",
                        self.book
                            .profiles
                            .get(i)
                            .map_or(String::new(), |p| p.name.clone()),
                    ),
                ],
                Some(Confirm::Load(i)) => vec![
                    line("Are you sure you wish to load", String::new()),
                    line(
                        "profile",
                        self.book
                            .profiles
                            .get(i)
                            .map_or(String::new(), |p| format!("{}?", p.name)),
                    ),
                ],
                None => Vec::new(),
            },
            "profile-overview" => current.map_or_else(Vec::new, |p| self.overview(p)),
            "profile-trophies" => current.map_or_else(Vec::new, trophies),
            "profile-records" => current.map_or_else(Vec::new, |p| self.records(p)),
            _ => Vec::new(),
        }
    }

    fn summary(&self, p: &Profile) -> Vec<(String, String)> {
        vec![
            ("Profile".into(), p.name.clone()),
            ("Profile Points Remaining:".into(), grouped(p.points)),
            ("Total Races:".into(), grouped(i64::from(p.races))),
            ("Total Victories:".into(), grouped(i64::from(p.wins))),
        ]
    }

    fn vehicle_name(&self, id: Option<u16>) -> String {
        match id {
            Some(id) => self
                .catalog
                .iter()
                .find(|v| v.id == id)
                .map_or(format!("Vehicle {id}"), |v| v.label.clone()),
            None => "None".into(),
        }
    }

    /// PAGE25 `ProfileView_Main` in its row order.
    fn overview(&self, p: &Profile) -> Vec<(String, String)> {
        let line = |l: &str, v: String| (l.to_string(), v);
        vec![
            line("Profile", p.name.clone()),
            line(
                "Percent Complete:",
                format!("{:3.0}%", p.percent_complete()),
            ),
            line("Profile Points Earned:", grouped(p.earned_points())),
            line("Profile Points Remaining:", grouped(p.points)),
            line("Total Races:", grouped(i64::from(p.races))),
            line("Total Victories:", grouped(i64::from(p.wins))),
            line("Win Percentage:", format!("{:3.0}%", p.win_percentage())),
            line("Most Winning ATV:", self.vehicle_name(p.most_winning_atv())),
            line("Longest Jump:", UNMEASURED.into()),
            line("Most Stunt Points In An Objective:", UNMEASURED.into()),
            line("Most Combos:", UNMEASURED.into()),
            line("Most Points In One Jump:", UNMEASURED.into()),
            line("Highest Race Stunt Score:", grouped(p.best_stunt)),
        ]
    }

    /// PAGE27-35: the course's record.
    fn records(&self, p: &Profile) -> Vec<(String, String)> {
        let groups = record_groups(&self.race_courses);
        let Some((group, courses)) = groups.get(self.profile_ui.group) else {
            return vec![("No race courses are loaded.".into(), String::new())];
        };
        let Some(id) = courses.get(self.profile_ui.course) else {
            return Vec::new();
        };
        let record = p.courses.get(id).cloned().unwrap_or_default();
        let laps = if group.contains("Enduro") { 3 } else { 5 };
        vec![
            (self.course_name(id), group.to_string()),
            ("Total Races:".into(), grouped(i64::from(record.races))),
            ("Total Victories:".into(), grouped(i64::from(record.wins))),
            ("Best Lap Time:".into(), race_time(record.best_lap_ms)),
            (
                format!("Best {laps}-Lap Time:"),
                race_time(record.best_total_ms),
            ),
            ("Longest Jump:".into(), UNMEASURED.into()),
            (
                "Most Winning ATV:".into(),
                self.vehicle_name(record.most_winning_atv()),
            ),
        ]
    }

    /// Opens a Profile Editor page; true when it handled the action.
    pub(super) fn profile_action(
        &mut self,
        action: &str,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Option<Event>> {
        let missing = "No ATV Offroad Fury 2 profiles present.";
        match action {
            "profile" => self.show("profile", view),
            "profile-create" => {
                if self.book.is_full() {
                    self.notice = Some(BookError::Full.message().into());
                } else {
                    self.profile_ui.name_for = NameFor::Create;
                    self.profile_ui.name = self.book.suggested_name();
                    self.show("profile-name", view);
                }
            }
            "profile-load" | "profile-copy" | "profile-delete" => {
                if self.book.profiles.is_empty() {
                    self.notice = Some(missing.into());
                } else {
                    self.profile_ui.slots = match action {
                        "profile-load" => SlotMode::Load,
                        "profile-copy" => SlotMode::Copy,
                        _ => SlotMode::Delete,
                    };
                    self.show("profile-slots", view);
                }
            }
            "profile-view" => {
                if self.book.current.is_none() {
                    self.notice = Some(NO_PROFILE.into());
                } else {
                    self.show("profile-view", view);
                }
            }
            "profile-store" => return Some(self.open_store(view)),
            "view-overview" => self.show("profile-overview", view),
            "view-atvs" => self.view_collection("store-atv", view),
            "view-gear" => self.view_collection("store-gear", view),
            "view-games" => self.view_collection("store-games", view),
            "view-trophies" => self.show("profile-trophies", view),
            "view-records" => {
                self.profile_ui.group = 0;
                self.profile_ui.course = 0;
                self.show("profile-records", view);
            }
            "profileletter" => self.step_profile("profileletter", 0, view),
            "profileerase" => {
                self.profile_ui.name.pop();
                self.notice = None;
                self.refresh(view);
            }
            "profilesave" => self.save_name(view, settings),
            _ if action.starts_with("slot:") => {
                let index: usize = action["slot:".len()..].parse().ok()?;
                match self.profile_ui.slots {
                    SlotMode::Load => {
                        self.profile_ui.confirm = Some(Confirm::Load(index));
                        self.show("profile-confirm", view);
                    }
                    SlotMode::Delete => {
                        self.profile_ui.confirm = Some(Confirm::Delete(index));
                        self.show("profile-confirm", view);
                        // Delete defaults to No.
                        self.selection = 1;
                    }
                    SlotMode::Copy => {
                        if self.book.is_full() {
                            self.notice = Some(BookError::Full.message().into());
                        } else {
                            self.profile_ui.name_for = NameFor::Copy(index);
                            self.profile_ui.name = self.book.suggested_name();
                            self.show("profile-name", view);
                        }
                    }
                }
            }
            "confirmyes" => {
                let mut book = self.book.clone();
                let done = match self.profile_ui.confirm.take() {
                    Some(Confirm::Load(i)) => book.load(i).map(|()| "Load Successful."),
                    Some(Confirm::Delete(i)) => book.delete(i).map(|_| "Delete Successful."),
                    None => return Some(None),
                };
                match done {
                    Ok(message) => {
                        self.store_book(settings, book);
                        self.show("profile", view);
                        self.notice = Some(message.into());
                    }
                    Err(e) => {
                        self.show("profile", view);
                        self.notice = Some(e.message().into());
                    }
                }
            }
            "confirmno" => {
                self.profile_ui.confirm = None;
                self.show("profile-slots", view);
            }
            _ => return None,
        }
        Some(None)
    }

    /// Profile Editor → Unlock Items: the Profile Store (PAGE36
    /// `ProfileStore_Menu`, PROFILE_STORE.md); it sells to the loaded
    /// profile.
    fn open_store(&mut self, view: &Viewport) -> Option<Event> {
        if self.book.current.is_none() {
            self.notice = Some(NO_PROFILE.into());
        } else {
            self.show_store("store", view);
        }
        None
    }

    fn save_name(&mut self, view: &Viewport, settings: &mut Settings) {
        let mut book = self.book.clone();
        let name = self.profile_ui.name.clone();
        let done = match self.profile_ui.name_for {
            NameFor::Create => book
                .create(&name)
                .map(|i| format!("Profile {} created and loaded.", book.profiles[i].name)),
            NameFor::Copy(from) => book.copy(from, &name).map(|_| "Copy Successful.".into()),
        };
        match done {
            Ok(message) => {
                self.store_book(settings, book);
                self.focus = None;
                self.show("profile", view);
                self.notice = Some(message);
            }
            Err(e) => self.notice = Some(e.message().into()),
        }
    }

    /// Left/Right on the letter row (appends nothing), Records' event and
    /// course rows. A tap or Enter on the letter row (direction 0) adds
    /// the letter.
    pub(super) fn step_profile(&mut self, action: &str, direction: i32, view: &Viewport) {
        let wrap = |at: usize, n: usize| -> usize {
            if n == 0 {
                0
            } else {
                (at as i32 + direction).rem_euclid(n as i32) as usize
            }
        };
        match action {
            "profileletter" => {
                let n = NAME_CHARS.chars().count();
                if direction == 0 {
                    let c = NAME_CHARS
                        .chars()
                        .nth(self.profile_ui.letter)
                        .unwrap_or('A');
                    let mut name = self.profile_ui.name.clone();
                    name.push(c);
                    self.set_profile_name(&name);
                } else {
                    self.profile_ui.letter = wrap(self.profile_ui.letter, n);
                }
            }
            "recordevent" => {
                let n = record_groups(&self.race_courses).len();
                self.profile_ui.group = wrap(self.profile_ui.group, n);
                self.profile_ui.course = 0;
            }
            "recordcourse" => {
                let groups = record_groups(&self.race_courses);
                let n = groups.get(self.profile_ui.group).map_or(0, |g| g.1.len());
                self.profile_ui.course = wrap(self.profile_ui.course, n);
            }
            _ => return,
        }
        self.refresh(view);
    }

    /// The name field's text (typed, pasted or from the letter row): upper
    /// case, offered characters, eight at most. A trailing space stays
    /// while typing; saving trims it.
    pub(super) fn set_profile_name(&mut self, text: &str) {
        self.profile_ui.name = clean_name(text);
        self.notice = None;
    }

    pub(super) fn profile_back(&self) -> Option<&'static str> {
        Some(match self.page.as_str() {
            "profile" => "main",
            "profile-name" => match self.profile_ui.name_for {
                NameFor::Create => "profile",
                NameFor::Copy(_) => "profile-slots",
            },
            "profile-slots" => "profile",
            "profile-confirm" => "profile-slots",
            "profile-view" => "profile",
            "profile-overview" | "profile-trophies" | "profile-records" => "profile-view",
            _ => return None,
        })
    }
}

/// The original No Profile Alert (`0x432490`, `0x432550`), shown where the
/// port needs a loaded profile.
pub const NO_PROFILE: &str = "You must have an ATV Offroad Fury 2 Profile loaded. \
     To create or load a Profile, use the Profile Editor.";

/// PAGE26 `ProfileView_Champ`: each championship's highest trophy on Normal
/// and Expert. The Freestyle and Short Tracks championships are not
/// offered by the port.
fn trophies(p: &Profile) -> Vec<(String, String)> {
    let mut lines = vec![(
        "Highest Trophy Awarded".to_string(),
        "Normal / Expert".to_string(),
    )];
    for series in &atv_core::championship::SERIES {
        let name = |expert| {
            p.championships
                .progress(series, expert)
                .trophy
                .map_or("None", |t| t.name())
        };
        lines.push((
            series.name.to_string(),
            format!("{} / {}", name(false), name(true)),
        ));
    }
    for name in ["Amateur Freestyle", "Pro Freestyle", "Short Tracks"] {
        lines.push((name.to_string(), "Not offered".into()));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_times_use_the_page_formats() {
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(-1000), "-1,000");
        assert_eq!(race_time(Some(83_456)), "01:23.45");
        assert_eq!(race_time(None), "--:--.--");
    }

    #[test]
    fn record_groups_follow_the_event_pages() {
        let courses: Vec<String> = [
            "env05",
            "nat01",
            "sx01",
            "sx03",
            "sx09",
            "env05ends",
            "env05endl",
        ]
        .map(String::from)
        .to_vec();
        let groups = record_groups(&courses);
        let names: Vec<&str> = groups.iter().map(|g| g.0).collect();
        assert_eq!(
            names,
            [
                "Amateur Nationals",
                "Pro Nationals",
                "Amateur Enduro",
                "Pro Enduro",
                "Amateur Supercross",
                "Pro Supercross"
            ]
        );
        // sx09 is not raced by a championship; it lists under Amateur.
        assert_eq!(groups[4].1, ["sx01", "sx09"]);
    }

    fn view() -> Viewport {
        Viewport {
            width: 1280.,
            height: 720.,
            scale: 1.,
            ..Default::default()
        }
    }

    #[test]
    fn the_editor_creates_copies_deletes_and_loads_profiles() {
        let view = view();
        let mut settings = Settings::memory();
        settings.set(
            crate::settings::PROFILE_KEY,
            "{\"points\":40,\"wins\":1,\"races\":2}",
        );
        let mut front = Frontend::new(Vec::new(), &settings, true);
        assert_eq!(front.profile_name(), Some("PLAYER1"));
        front.show("main", &view);
        let main: Vec<&str> = front.choices().iter().map(|(a, _)| a.as_str()).collect();
        let at = main.iter().position(|a| *a == "profile").unwrap();
        assert_eq!(main[at - 1], "freeride");
        assert_eq!(main[at + 1], "settings");
        front.run("profile", &view, &mut settings);
        assert_eq!(front.page, "profile");
        assert_eq!(
            front.profile_text().as_deref(),
            Some("Current Profile: PLAYER1 · Current Profile Points: 40")
        );
        // Create: the name entry starts on the next default name.
        front.run("profile-create", &view, &mut settings);
        assert_eq!(
            (front.page.as_str(), front.profile_ui.name.as_str()),
            ("profile-name", "PLAYER2")
        );
        assert_eq!(front.fields()[0].max_length, 8);
        front.set_field("profile-name", "ace rider");
        assert_eq!(front.profile_ui.name, "ACE RIDE");
        // The controller row: Right steps the letter, accept adds it.
        front.run("profileerase", &view, &mut settings);
        front.step("profileletter", 1, &view, &mut settings);
        front.run("profileletter", &view, &mut settings);
        assert_eq!(front.profile_ui.name, "ACE RIDB");
        front.run("profilesave", &view, &mut settings);
        assert_eq!(front.page, "profile");
        assert_eq!(front.profile_name(), Some("ACE RIDB"));
        assert_eq!(front.profile.points, 0);
        let saved = load_book(&settings);
        assert_eq!(saved.profiles.len(), 2);
        assert_eq!(saved.current, Some(1));
        // A duplicate name is refused.
        front.run("profile-create", &view, &mut settings);
        front.set_field("profile-name", "player1");
        front.run("profilesave", &view, &mut settings);
        assert_eq!(front.page, "profile-name");
        assert_eq!(
            front.notice.as_deref(),
            Some(BookError::Duplicate.message())
        );
        front.run("back", &view, &mut settings);
        // Copy PLAYER1 as COPY.
        front.run("profile-copy", &view, &mut settings);
        assert_eq!(front.profile_heading().as_deref(), Some("Copy Profile"));
        front.run("slot:0", &view, &mut settings);
        front.set_field("profile-name", "copy");
        front.run("profilesave", &view, &mut settings);
        assert_eq!(front.notice.as_deref(), Some("Copy Successful."));
        let saved = load_book(&settings);
        assert_eq!(saved.profiles[2].name, "COPY");
        assert_eq!(saved.profiles[2].points, 40);
        assert_eq!(front.profile_name(), Some("ACE RIDB"));
        // Delete COPY: the alert defaults to No.
        front.run("profile-delete", &view, &mut settings);
        front.run("slot:2", &view, &mut settings);
        assert_eq!(front.page, "profile-confirm");
        assert_eq!(front.choices()[front.selection].0, "confirmno");
        assert_eq!(front.profile_details()[0].0, "Delete this save file?");
        front.run("confirmyes", &view, &mut settings);
        assert_eq!(load_book(&settings).profiles.len(), 2);
        // Load PLAYER1 back.
        front.run("profile-load", &view, &mut settings);
        front.run("slot:0", &view, &mut settings);
        front.run("confirmyes", &view, &mut settings);
        assert_eq!(front.notice.as_deref(), Some("Load Successful."));
        assert_eq!(
            (front.profile_name(), front.profile.points),
            (Some("PLAYER1"), 40)
        );
        // View Profile → Overview.
        front.run("profile-view", &view, &mut settings);
        front.run("view-overview", &view, &mut settings);
        let details = front.profile_details();
        assert!(details.contains(&("Profile Points Remaining:".into(), "40".into())));
        assert!(details.contains(&("Win Percentage:".into(), " 50%".into())));
        front.run("back", &view, &mut settings);
        assert_eq!(front.page, "profile-view");
        front.run("view-trophies", &view, &mut settings);
        assert_eq!(front.profile_details()[1].1, "None / None");
        // Deleting the loaded profile leaves none: Single Player says so and
        // the Championship needs one.
        front.show("profile", &view);
        front.run("profile-delete", &view, &mut settings);
        front.run("slot:0", &view, &mut settings);
        front.run("confirmyes", &view, &mut settings);
        assert_eq!(front.profile_name(), None);
        front.run("profile-view", &view, &mut settings);
        assert_eq!(front.notice.as_deref(), Some(NO_PROFILE));
        front.show("single", &view);
        assert!(
            front
                .profile_text()
                .unwrap()
                .starts_with("No profile loaded")
        );
        front.run("championship", &view, &mut settings);
        assert_eq!(front.page, "single");
        assert_eq!(front.notice.as_deref(), Some(NO_PROFILE));
        assert!(!crate::settings::profile_loaded(&settings));
    }

    #[test]
    fn records_step_events_and_courses() {
        let view = view();
        let mut settings = Settings::memory();
        let mut book = load_book(&settings);
        book.current_mut()
            .unwrap()
            .record_course(&atv_core::profile::RaceResult {
                course: "sx01".into(),
                quad: 3,
                place: 1,
                best_lap: Some(41.5),
                total: Some(212.25),
                full_length: true,
            });
        save_book(&mut settings, &book);
        let mut front = Frontend::new(Vec::new(), &settings, true);
        front.race_courses = ["env05", "sx08", "sx01"].map(String::from).to_vec();
        front.show("profile-records", &view);
        assert_eq!(front.choices()[0].1, "Event: Amateur Nationals");
        front.step("recordevent", 1, &view, &mut settings);
        assert_eq!(front.choices()[0].1, "Event: Amateur Supercross");
        assert_eq!(front.profile_details()[1].1, "0");
        front.step("recordcourse", -1, &view, &mut settings);
        let details = front.profile_details();
        assert_eq!(details[0].0, "SX01");
        assert_eq!(details[1].1, "1");
        assert_eq!(details[3].1, "00:41.50");
        assert_eq!(details[4], ("Best 5-Lap Time:".into(), "03:32.25".into()));
        assert_eq!(details[6].1, "Vehicle 3");
    }
}
