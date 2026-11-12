//! Single Player → Waypoint Editor (PAGE09, research/RACING.md "User
//! Created Enduro and the Waypoint Editor"): an Environment and its three
//! saved races ("Race 1 (New)".."Race 3 (New)"). A new slot opens the editor
//! ride; a saved one opens its course page: Race, Edit Gates, Laps, the
//! name, Copy course link and Delete.
//!
//! Courses are kept on this device under `fury-waypoints` as course codes
//! (`atv_core::user_course`), three per venue. The original keeps them in a
//! per-profile memory card file and needs a loaded profile; the port keeps
//! them beside the profiles and needs none, so a shared link always plays.
use super::{Event, Frontend, Viewport};
use crate::settings::Settings;
use atv_core::user_course::{self, SLOTS_PER_VENUE, UserCourse};
use serde_json::Value;

pub const PAGES: [&str; 2] = ["waypoint", "waypoint-course"];
pub const WAYPOINTS_KEY: &str = "fury-waypoints";

/// The pages' working state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WaypointUi {
    /// The Environment (`env01`..`env08`).
    pub venue: String,
    pub slot: usize,
    /// The course page's name entry.
    pub name: String,
    /// Delete was pressed once.
    pub confirm_delete: bool,
    /// The venue's saved courses, read when the page opens or changes.
    pub slots: [Option<UserCourse>; SLOTS_PER_VENUE],
}

/// The saved codes of a venue's slots (empty for a new slot).
pub fn load_slots(settings: &Settings, venue: &str) -> [Option<UserCourse>; SLOTS_PER_VENUE] {
    let saved: Value = settings
        .get(WAYPOINTS_KEY)
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    std::array::from_fn(|i| {
        saved[venue][i]
            .as_str()
            .and_then(|code| UserCourse::decode(code).ok())
            .filter(|c| c.environment == venue)
    })
}

/// Stores (or with `None` clears) a slot; false when it lasts only for this
/// session.
pub fn save_slot(
    settings: &mut Settings,
    slot: usize,
    course: Option<&UserCourse>,
    venue: &str,
) -> bool {
    if slot >= SLOTS_PER_VENUE {
        return false;
    }
    let mut saved: Value = settings
        .get(WAYPOINTS_KEY)
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let mut slots: Vec<Value> = saved[venue].as_array().cloned().unwrap_or_default();
    slots.resize(SLOTS_PER_VENUE, Value::Null);
    slots[slot] = course.map_or(Value::Null, |c| Value::String(c.encode()));
    saved[venue] = Value::Array(slots);
    settings.set(WAYPOINTS_KEY, &saved.to_string())
}

/// The page's stepper rows.
pub fn waypoint_row(action: &str) -> bool {
    matches!(action, "wpvenue" | "wplaps")
}

impl Frontend {
    /// The Environments that ship a long Enduro (the editor's archive).
    fn waypoint_venues(&self) -> Vec<String> {
        (1..=user_course::VENUES)
            .filter_map(user_course::environment)
            .filter(|v| {
                self.race_courses
                    .contains(&atv_game::launch::race_course_id(
                        v,
                        atv_game::launch::RaceVariant::EnduroLong,
                    ))
            })
            .collect()
    }

    fn waypoint_venue(&self) -> String {
        let venues = self.waypoint_venues();
        if venues.contains(&self.waypoint.venue) {
            self.waypoint.venue.clone()
        } else {
            venues.first().cloned().unwrap_or_default()
        }
    }

    fn venue_name(&self, venue: &str) -> String {
        self.tracks
            .iter()
            .find(|t| t.id == venue)
            .map_or_else(|| venue.to_uppercase(), |t| t.name.clone())
    }

    /// The course the course page shows.
    fn waypoint_course(&self) -> Option<UserCourse> {
        self.waypoint
            .slots
            .get(self.waypoint.slot)
            .cloned()
            .flatten()
    }

    /// Re-reads the venue's saved courses.
    fn reload_waypoints(&mut self, settings: &Settings) {
        self.waypoint.venue = self.waypoint_venue();
        self.waypoint.slots = load_slots(settings, &self.waypoint.venue);
    }

    /// Saves the shown course (or clears its slot) and re-reads the slots.
    fn store_waypoint(&mut self, course: Option<&UserCourse>, settings: &mut Settings) {
        let venue = self.waypoint_venue();
        if !save_slot(settings, self.waypoint.slot, course, &venue) {
            self.notice = Some("The course is kept only until this page closes.".into());
        }
        self.reload_waypoints(settings);
    }

    pub(super) fn waypoint_choices(&self) -> Vec<(String, String)> {
        let venue = self.waypoint_venue();
        if self.page == "waypoint-course" {
            let course = self.waypoint_course();
            let laps = course
                .as_ref()
                .map_or(user_course::DEFAULT_LAPS, |c| c.laps);
            return vec![
                ("wprace".into(), "Race This Course".into()),
                ("wpedit".into(), "Edit Gates".into()),
                ("wplaps".into(), format!("Number of Laps: {laps}")),
                ("wprename".into(), "Save Name".into()),
                ("wpcopy".into(), "Copy course link".into()),
                (
                    "wpdelete".into(),
                    if self.waypoint.confirm_delete {
                        "Delete: press again to confirm".into()
                    } else {
                        "Delete Course".into()
                    },
                ),
                ("back".into(), "Back".into()),
            ];
        }
        let mut rows = vec![(
            "wpvenue".into(),
            format!("Environment: {}", self.venue_name(&venue)),
        )];
        for (i, course) in self.waypoint.slots.iter().enumerate() {
            rows.push((
                format!("wpslot{i}"),
                match course {
                    Some(c) => format!("{} · {} gates", c.label(i), c.gates.len()),
                    None => format!("Race {} (New)", i + 1),
                },
            ));
        }
        rows.push(("back".into(), "Back".into()));
        rows
    }

    /// The line under the rows.
    pub(super) fn waypoint_line(&self) -> Option<String> {
        Some(if self.page == "waypoint-course" {
            "Type a name and Save Name to rename. A course races alone; its records stay off the Hall of Fame."
                .into()
        } else {
            "Ride the course and drop gates where you stand. Three races per Environment.".into()
        })
    }

    pub(super) fn step_waypoint(&mut self, action: &str, direction: i32, settings: &mut Settings) {
        match action {
            "wpvenue" => {
                let venues = self.waypoint_venues();
                if venues.is_empty() {
                    return;
                }
                let at = venues
                    .iter()
                    .position(|v| *v == self.waypoint_venue())
                    .unwrap_or(0) as i32;
                self.waypoint.venue =
                    venues[(at + direction).rem_euclid(venues.len() as i32) as usize].clone();
                self.reload_waypoints(settings);
            }
            "wplaps" => {
                if let Some(mut course) = self.waypoint_course() {
                    let (low, high) = (*user_course::LAPS.start(), *user_course::LAPS.end());
                    course.laps = ((course.laps as i32 - low as i32 + direction)
                        .rem_euclid((high - low + 1) as i32)
                        + low as i32) as u32;
                    self.store_waypoint(Some(&course), settings);
                }
            }
            _ => {}
        }
    }

    /// The editor saved `course` into `slot`: its venue's slots re-read and
    /// its course page shown next.
    pub fn saved_waypoint(&mut self, course: &UserCourse, slot: usize, settings: &mut Settings) {
        self.waypoint.venue = course.environment.clone();
        self.waypoint.slot = slot.min(SLOTS_PER_VENUE - 1);
        self.waypoint.name = course.name.clone();
        self.store_waypoint(Some(course), settings);
    }

    /// The course page's name field (Save Name stores it).
    pub(super) fn set_waypoint_name(&mut self, text: &str) {
        self.waypoint.name = text
            .chars()
            .filter(|c| !c.is_control())
            .take(user_course::MAX_NAME)
            .collect();
    }

    /// A Waypoint Editor row; `None` when `action` is not one.
    pub(super) fn waypoint_action(
        &mut self,
        action: &str,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Option<Event>> {
        if action != "wpdelete" {
            self.waypoint.confirm_delete = false;
        }
        let venue = self.waypoint_venue();
        let quad = self.quad_id;
        let rider = self.rider_id;
        let event = match action {
            "waypoint" => {
                self.reload_waypoints(settings);
                self.show("waypoint", view);
                None
            }
            "wpvenue" | "wplaps" => {
                self.step_waypoint(action, 1, settings);
                self.refresh(view);
                None
            }
            _ if action.starts_with("wpslot") => {
                let slot = action["wpslot".len()..].parse::<usize>().ok()?;
                self.waypoint.slot = slot.min(SLOTS_PER_VENUE - 1);
                self.reload_waypoints(settings);
                match self.waypoint_course() {
                    Some(course) => {
                        self.waypoint.name = course.name.clone();
                        self.show("waypoint-course", view);
                        None
                    }
                    None => Some(Event::WaypointEdit {
                        code: UserCourse::new(&venue).encode(),
                        slot: self.waypoint.slot,
                        quad,
                        rider,
                    }),
                }
            }
            "wprace" => self.waypoint_course().map(|c| Event::UserCourse {
                code: c.encode(),
                quad,
                rider,
            }),
            "wpedit" => self.waypoint_course().map(|c| Event::WaypointEdit {
                code: c.encode(),
                slot: self.waypoint.slot,
                quad,
                rider,
            }),
            "wpcopy" => self
                .waypoint_course()
                .map(|c| Event::CopyCourseLink { code: c.encode() }),
            "wprename" => {
                if let Some(mut course) = self.waypoint_course() {
                    course.name = user_course::clean_name(&self.waypoint.name);
                    self.store_waypoint(Some(&course), settings);
                    self.notice = Some("Name saved.".into());
                    self.refresh(view);
                }
                None
            }
            "wpdelete" if self.waypoint.confirm_delete => {
                self.waypoint.confirm_delete = false;
                self.store_waypoint(None, settings);
                return Some(self.back(view));
            }
            "wpdelete" => {
                self.waypoint.confirm_delete = true;
                self.refresh(view);
                None
            }
            _ => return None,
        };
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_save_rename_and_delete() {
        let mut settings = Settings::memory();
        assert!(load_slots(&settings, "env05").iter().all(Option::is_none));
        let mut c = UserCourse::new("env05");
        for x in [100., 200., 300.] {
            assert!(c.place(x, 100., [0., 1.]));
        }
        assert!(save_slot(&mut settings, 2, Some(&c), "env05"));
        let slots = load_slots(&settings, "env05");
        assert_eq!(slots[2].as_ref(), Some(&c));
        assert!(slots[0].is_none() && load_slots(&settings, "env04")[2].is_none());
        // A code saved under another venue is not that venue's course.
        assert!(save_slot(&mut settings, 0, Some(&c), "env06"));
        assert!(load_slots(&settings, "env06")[0].is_none());
        assert!(save_slot(&mut settings, 2, None, "env05"));
        assert!(load_slots(&settings, "env05")[2].is_none());
        assert!(!save_slot(&mut settings, 3, Some(&c), "env05"));
        // Garbage in storage reads as empty slots.
        settings.set(WAYPOINTS_KEY, "{\"env05\":[\"!!\",7]}");
        assert!(load_slots(&settings, "env05").iter().all(Option::is_none));
    }
}
