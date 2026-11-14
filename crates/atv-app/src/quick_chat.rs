//! Quick chat beside a room ride (research/QUICK_CHAT.md): the pick wheel,
//! what other riders said (bubbles, feed, horn), mute, block and report.
//! Preset ids only; nothing typed ever leaves the game.
//!
//! Controls: the toolbar's CHAT button, T (wheel), 1-8 (a line, or with
//! Shift an emote, while the wheel is open), B (horn); on a controller,
//! Select (or R3, then L3, when the preset binds Select) opens the wheel,
//! the D-pad left/right moves round it, A picks and B closes.
use crate::overlay::Action;
use atv_game::multiplayer::chat;
use atv_ui::hud::chat::{self as hud, Bubble, Feed, Said, View};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Settings: hide every rider's lines and emotes.
pub const MUTE_ALL_KEY: &str = "atv-chat-mute-all";
/// Settings: blocked riders (JSON list of `{"name", "pid"}`).
pub const BLOCKED_KEY: &str = "atv-chat-blocked";
/// Most riders a player can block (the oldest is forgotten past this).
pub const MAX_BLOCKED: usize = 100;
const FEED_LINES: usize = 3;

/// Gamepad buttons that may open the wheel, in order of preference.
const PAD_OPEN: [usize; 3] = [8, 11, 10];
const PAD_A: usize = 0;
const PAD_B: usize = 1;
const PAD_LEFT: usize = 14;
const PAD_RIGHT: usize = 15;

/// The wheel's state, kept by the overlay.
#[derive(Default)]
pub struct Wheel {
    pub open: bool,
    /// Showing emotes instead of lines.
    pub emotes: bool,
    /// After a race in a room: the post-race lines.
    pub post: bool,
    /// The controller's highlighted target (hud::chat::wheel_targets).
    pub focus: Option<usize>,
    pad_held: [bool; 4],
}

impl Wheel {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.emotes = false;
        self.focus = None;
    }

    /// Line ids round the wheel.
    pub fn line_ids(&self) -> [u8; 8] {
        if self.post {
            chat::POST_LINES
        } else {
            std::array::from_fn(|i| i as u8)
        }
    }

    pub fn lines(&self) -> [&'static str; 8] {
        self.line_ids().map(|id| chat::line(id).unwrap_or(""))
    }

    fn pick(&mut self, kind: u8, id: u8) -> Option<Action> {
        self.open = false;
        Some(Action::Chat(kind, id))
    }

    /// A released wheel target: `Some(Some(pick))`, `Some(None)` for the
    /// page switch or backdrop, None when `id` is not the wheel's.
    pub fn hit(&mut self, id: u32) -> Option<Option<Action>> {
        Some(match id {
            hud::LINE_HIT..hud::EMOTE_HIT => {
                let line = self.line_ids()[(id - hud::LINE_HIT) as usize % 8];
                self.pick(chat::LINE, line)
            }
            hud::EMOTE_HIT..hud::HORN_HIT => {
                self.pick(chat::EMOTE, (id - hud::EMOTE_HIT) as u8 % 8)
            }
            hud::HORN_HIT => self.pick(chat::HORN, 0),
            hud::PAGE_HIT => {
                self.emotes = !self.emotes;
                None
            }
            hud::CLOSE_HIT => {
                self.open = false;
                None
            }
            _ => return None,
        })
    }

    /// A key while riding in a chat room. `free` says whether the player's
    /// bindings leave the key alone.
    pub fn key(&mut self, code: &str, shift: bool, free: impl Fn(&str) -> bool) -> Option<Action> {
        match code {
            "KeyT" if free(code) => {
                self.toggle();
                None
            }
            "KeyB" if free(code) => Some(Action::Chat(chat::HORN, 0)),
            _ if self.open => {
                let digit = code.strip_prefix("Digit")?.parse::<usize>().ok()?;
                let slot = digit.checked_sub(1).filter(|s| *s < 8)?;
                if shift || self.emotes {
                    self.pick(chat::EMOTE, slot as u8)
                } else {
                    self.pick(chat::LINE, self.line_ids()[slot])
                }
            }
            _ => None,
        }
    }

    /// A controller sample while riding in a chat room; `bound` says
    /// whether the player's bindings use a button.
    pub fn pad(&mut self, buttons: &[f32], bound: impl Fn(u8) -> bool) -> Option<Action> {
        let down = |i: usize| buttons.get(i).is_some_and(|v| *v > 0.5);
        let open_button = PAD_OPEN.into_iter().find(|b| !bound(*b as u8));
        let now = [
            open_button.is_some_and(down),
            down(PAD_A),
            down(PAD_B),
            down(PAD_LEFT) || down(PAD_RIGHT),
        ];
        let pressed = |i: usize| now[i] && !self.pad_held[i];
        let (left, right) = (down(PAD_LEFT), down(PAD_RIGHT));
        let mut action = None;
        if pressed(0) {
            self.toggle();
            if self.open {
                self.focus = Some(0);
            }
        } else if self.open {
            if pressed(3) {
                let at = self.focus.unwrap_or(0);
                self.focus = Some(if right && !left {
                    (at + 1) % 10
                } else {
                    (at + 9) % 10
                });
            } else if pressed(2) {
                self.open = false;
            } else if pressed(1) {
                let id = match self.focus.unwrap_or(0) {
                    i @ 0..8 if self.emotes => hud::EMOTE_HIT + i as u32,
                    i @ 0..8 => hud::LINE_HIT + i as u32,
                    8 => hud::PAGE_HIT,
                    _ => hud::HORN_HIT,
                };
                action = self.hit(id).flatten();
            }
        }
        self.pad_held = now;
        action
    }
}

/// A blocked rider: by its verified id when the server gives one (the
/// identity slice's per-viewer reference, the same `id` the Friends lists
/// and `/multiplayer/friends/block` use; research/ONLINE_IDENTITY.md), else
/// by its shown name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocked {
    pub name: String,
    pub pid: Option<String>,
}

fn fold(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The blocked list as saved in the settings.
pub fn load_blocked(settings: &atv_ui::settings::Settings) -> Vec<Blocked> {
    let Some(list) = settings
        .get(BLOCKED_KEY)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return Vec::new();
    };
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| {
            Some(Blocked {
                name: b["name"].as_str()?.to_string(),
                pid: b["pid"].as_str().map(str::to_string),
            })
        })
        .take(MAX_BLOCKED)
        .collect()
}

/// Whether `blocked` holds a rider (by `pid` when both have one, else by
/// name).
pub fn holds(blocked: &[Blocked], name: &str, pid: Option<&str>) -> bool {
    let name = fold(name);
    blocked.iter().any(|b| match (&b.pid, pid) {
        (Some(a), Some(b)) => a == b,
        _ => !name.is_empty() && fold(&b.name) == name,
    })
}

fn save_blocked(settings: &mut atv_ui::settings::Settings, blocked: &[Blocked]) {
    let list: Vec<serde_json::Value> = blocked
        .iter()
        .map(|b| match &b.pid {
            Some(pid) => serde_json::json!({"name": b.name, "pid": pid}),
            None => serde_json::json!({"name": b.name}),
        })
        .collect();
    settings.set(BLOCKED_KEY, &serde_json::Value::from(list).to_string());
}

/// Keeps the saved list in step with the identity block list (the Friends
/// lists' `blocked`, as `(id, name)`): for riders with an id the server's
/// list wins, so a block or unblock on the Friends page (or another device
/// on the Pass Key) reaches quick chat; name-only blocks of unverified
/// riders stay on this device. True when the saved list changed.
pub fn sync_blocked(
    settings: &mut atv_ui::settings::Settings,
    server: &[(String, String)],
) -> bool {
    let before = load_blocked(settings);
    let mut list: Vec<Blocked> = before.iter().filter(|b| b.pid.is_none()).cloned().collect();
    for (id, name) in server {
        // A name-only block of the same rider becomes the id's.
        list.retain(|b| fold(&b.name) != fold(name));
        list.push(Blocked {
            name: name.trim().to_string(),
            pid: Some(id.clone()),
        });
    }
    if list.len() > MAX_BLOCKED {
        list.drain(..list.len() - MAX_BLOCKED);
    }
    if list == before {
        return false;
    }
    save_blocked(settings, &list);
    true
}

/// Whether the player blocked this rider: their chat and emotes stay hidden
/// and Free Ride hub challenges from them are declined (research/
/// QUICK_CHAT.md "Block"). Reads the saved settings.
pub fn is_blocked(name: &str, pid: Option<&str>) -> bool {
    holds(
        &load_blocked(&atv_ui::settings::Settings::load()),
        name,
        pid,
    )
}

/// A feed line as received: the sender's slot (None for the player), the
/// shown name, kind, id and when (ms).
type FeedEvent = (Option<u8>, std::rc::Rc<str>, u8, u8, f64);

/// What the room's riders said, and the player's mute and block choices.
#[derive(Default)]
pub struct QuickChat {
    /// The HUD's view, rebuilt in place each frame.
    pub view: View,
    /// Latest event per rider (slot, kind, id, when), and the feed.
    latest: BTreeMap<u8, (u8, u8, f64)>,
    feed: VecDeque<FeedEvent>,
    /// Riders muted this session, by slot, and by verified id so a rider
    /// who rejoins in another slot stays muted.
    pub muted: BTreeSet<u8>,
    pub muted_ids: BTreeSet<String>,
    /// The room's verified riders: slot to id (`/multiplayer/room/riders`).
    pub ids: BTreeMap<u8, String>,
    pub blocked: Vec<Blocked>,
    pub mute_all: bool,
    /// Counts for tests: shown and hidden events, horns heard.
    pub shown: u32,
    pub hidden: u32,
    pub horns: u32,
}

impl QuickChat {
    pub fn load(settings: &atv_ui::settings::Settings) -> Self {
        QuickChat {
            blocked: load_blocked(settings),
            mute_all: settings.get(MUTE_ALL_KEY).as_deref() == Some("on"),
            ..Default::default()
        }
    }

    /// See [`is_blocked`]; this room's copy of the list.
    pub fn blocked(&self, name: &str, pid: Option<&str>) -> bool {
        holds(&self.blocked, name, pid)
    }

    /// A slot's verified id, when the server gave one.
    pub fn id(&self, slot: u8) -> Option<&str> {
        self.ids.get(&slot).map(String::as_str)
    }

    /// Whether the rider in `slot` is blocked (by id when verified).
    pub fn blocked_slot(&self, slot: u8, name: &str) -> bool {
        self.blocked(name, self.id(slot))
    }

    /// Whether the rider in `slot` is muted.
    pub fn muted_slot(&self, slot: u8) -> bool {
        self.muted.contains(&slot) || self.id(slot).is_some_and(|id| self.muted_ids.contains(id))
    }

    fn hides(&self, slot: u8, name: &str, kind: u8) -> bool {
        self.muted_slot(slot)
            || self.blocked_slot(slot, name)
            || (self.mute_all && kind != chat::HORN)
    }

    /// The room's verified riders as `(slot, id, blocked on the server)`
    /// (research/QUICK_CHAT.md "Block"). A rider the server says this
    /// player blocked is blocked here too; a name-only block that names a
    /// verified rider takes on its id, so a rename keeps it. Returns the
    /// ids to block on the server as well (the identity block list).
    pub fn set_riders(
        &mut self,
        settings: &mut atv_ui::settings::Settings,
        riders: &[(u8, String, bool)],
        name: impl Fn(u8) -> Option<String>,
    ) -> Vec<String> {
        self.ids = riders.iter().map(|(s, id, _)| (*s, id.clone())).collect();
        let mut push = Vec::new();
        let mut changed = false;
        for (slot, id, server) in riders {
            let Some(name) = name(*slot) else {
                continue;
            };
            let folded = fold(&name);
            if let Some(entry) = self
                .blocked
                .iter_mut()
                .find(|b| b.pid.is_none() && fold(&b.name) == folded)
            {
                entry.pid = Some(id.clone());
                changed = true;
                if !server {
                    push.push(id.clone());
                }
            } else if *server && !holds(&self.blocked, &name, Some(id)) {
                if self.blocked.len() >= MAX_BLOCKED {
                    self.blocked.remove(0);
                }
                self.blocked.push(Blocked {
                    name: name.trim().to_string(),
                    pid: Some(id.clone()),
                });
                changed = true;
            }
        }
        if changed {
            save_blocked(settings, &self.blocked);
        }
        push
    }

    /// One event from the server; `me` is the player's slot. True when it
    /// is a horn to sound.
    pub fn receive(&mut self, message: chat::Chat, name: &str, me: Option<u8>, now: f64) -> bool {
        let own = me == Some(message.peer);
        if !own && self.hides(message.peer, name, message.kind) {
            self.hidden += 1;
            return false;
        }
        self.shown += 1;
        if !own {
            self.latest
                .insert(message.peer, (message.kind, message.id, now));
        }
        if message.kind != chat::HORN {
            if self.feed.len() == FEED_LINES {
                self.feed.pop_front();
            }
            let who = if own { None } else { Some(message.peer) };
            let name = std::rc::Rc::from(if own { "You" } else { name });
            self.feed
                .push_back((who, name, message.kind, message.id, now));
            return false;
        }
        self.horns += 1;
        true
    }

    /// Mutes or unmutes a rider for this room; true when now muted.
    pub fn toggle_mute(&mut self, slot: u8) -> bool {
        let id = self.id(slot).map(str::to_string);
        if !self.muted_slot(slot) {
            self.muted.insert(slot);
            self.muted_ids.extend(id);
            self.latest.remove(&slot);
            return true;
        }
        self.muted.remove(&slot);
        if let Some(id) = id {
            self.muted_ids.remove(&id);
        }
        false
    }

    /// Blocks or unblocks a rider and saves the list; true when now blocked.
    pub fn toggle_block(
        &mut self,
        settings: &mut atv_ui::settings::Settings,
        name: &str,
        pid: Option<&str>,
    ) -> bool {
        let was = self.blocked(name, pid);
        if was {
            let folded = fold(name);
            self.blocked.retain(|b| match (&b.pid, pid) {
                (Some(a), Some(p)) => a != p,
                _ => fold(&b.name) != folded,
            });
        } else {
            if self.blocked.len() >= MAX_BLOCKED {
                self.blocked.remove(0);
            }
            self.blocked.push(Blocked {
                name: name.trim().to_string(),
                pid: pid.map(str::to_string),
            });
        }
        save_blocked(settings, &self.blocked);
        !was
    }

    pub fn set_mute_all(&mut self, settings: &mut atv_ui::settings::Settings, on: bool) {
        self.mute_all = on;
        settings.set(MUTE_ALL_KEY, if on { "on" } else { "off" });
    }

    /// Rebuilds [`Self::view`] for this frame from the riders' presented
    /// positions; `ready` is the player's budget (lines, emotes, horn).
    pub fn update(
        &mut self,
        available: bool,
        position: impl Fn(u8) -> Option<[f32; 3]>,
        ready: [bool; 3],
        now: f64,
    ) {
        let v = &mut self.view;
        v.available = available;
        v.ready = ready;
        v.bubbles.clear();
        v.feed.clear();
        let said = |kind: u8, id: u8| match kind {
            chat::LINE => Said::Line(chat::line(id).unwrap_or("")),
            chat::EMOTE => Said::Emote(id),
            _ => Said::Horn,
        };
        self.latest.retain(|_, (kind, _, at)| {
            let life = if *kind == chat::HORN {
                hud::HORN_S
            } else {
                hud::BUBBLE_S
            };
            ((now - *at) / 1000.) < f64::from(life)
        });
        for (slot, (kind, id, at)) in &self.latest {
            if let Some(position) = position(*slot) {
                v.bubbles.push(Bubble {
                    slot: *slot,
                    position,
                    said: said(*kind, *id),
                    age: ((now - at) / 1000.) as f32,
                });
            }
        }
        self.feed
            .retain(|(.., at)| ((now - at) / 1000.) < f64::from(hud::FEED_S));
        for (slot, name, kind, id, at) in &self.feed {
            v.feed.push(Feed {
                slot: *slot,
                name: name.clone(),
                said: said(*kind, *id),
                age: ((now - at) / 1000.) as f32,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(peer: u8, kind: u8, id: u8) -> chat::Chat {
        chat::Chat { peer, kind, id }
    }

    #[test]
    fn muted_and_blocked_riders_stay_hidden_but_horns_survive_mute_all() {
        let mut c = QuickChat::default();
        assert!(!c.receive(event(1, chat::LINE, 0), "Bo", Some(0), 0.));
        c.update(true, |_| Some([0.; 3]), [true; 3], 100.);
        assert_eq!(c.view.bubbles.len(), 1);
        assert_eq!(c.view.feed[0].said, Said::Line("Nice!"));
        c.toggle_mute(1);
        c.update(true, |_| Some([0.; 3]), [true; 3], 200.);
        assert!(c.view.bubbles.is_empty());
        c.receive(event(1, chat::EMOTE, 2), "Bo", Some(0), 300.);
        assert_eq!((c.shown, c.hidden), (1, 1));
        c.blocked.push(Blocked {
            name: " CY ".into(),
            pid: None,
        });
        c.receive(event(2, chat::LINE, 1), "cy", Some(0), 300.);
        assert_eq!(c.hidden, 2);
        assert!(c.blocked("Cy", None) && !c.blocked("Cyd", None));
        c.mute_all = true;
        c.receive(event(3, chat::LINE, 1), "Di", Some(0), 300.);
        assert!(c.receive(event(3, chat::HORN, 0), "Di", Some(0), 300.));
        // The player's own lines always reach the feed.
        c.receive(event(0, chat::LINE, 7), "Me", Some(0), 300.);
        c.update(true, |_| None, [true; 3], 400.);
        assert_eq!(&*c.view.feed.last().unwrap().name, "You");
        // Bubbles and the feed fade out.
        c.update(true, |_| Some([0.; 3]), [true; 3], 10_000.);
        assert!(c.view.bubbles.is_empty() && c.view.feed.is_empty());
    }

    #[test]
    fn blocks_persist_in_the_settings() {
        let mut settings = atv_ui::settings::Settings::memory();
        let mut c = QuickChat::default();
        assert!(c.toggle_block(&mut settings, " Bo ", None));
        assert!(c.toggle_block(&mut settings, "Cy", Some("p9")));
        let loaded = QuickChat::load(&settings);
        assert!(loaded.blocked("bo", None) && loaded.blocked("x", Some("p9")));
        assert!(!c.toggle_block(&mut settings, "BO", None));
        assert!(!QuickChat::load(&settings).blocked("Bo", None));
    }

    #[test]
    fn verified_ids_key_mute_and_block_and_follow_the_identity_list() {
        let mut settings = atv_ui::settings::Settings::memory();
        let mut c = QuickChat::default();
        // Blocked by name before the room gave ids.
        assert!(c.toggle_block(&mut settings, "Bo", None));
        let names = |slot: u8| match slot {
            1 => Some("Bo".to_string()),
            2 => Some("Cy".to_string()),
            3 => Some("Di".to_string()),
            _ => None,
        };
        let riders = [
            (1, "r1".to_string(), false),
            (2, "r2".to_string(), true),
            (3, "r3".to_string(), false),
        ];
        // Bo's name block takes on Bo's id and goes to the server; Cy,
        // blocked on the Friends page, is blocked here too.
        assert_eq!(c.set_riders(&mut settings, &riders, names), ["r1"]);
        assert!(c.blocked_slot(2, "Cy"));
        let saved = load_blocked(&settings);
        assert!(saved.contains(&Blocked {
            name: "Bo".into(),
            pid: Some("r1".into())
        }));
        // Renamed, Bo stays blocked; another rider named Bo is not.
        c.receive(event(1, chat::LINE, 0), "Bo Renamed", Some(0), 0.);
        assert_eq!(c.hidden, 1);
        assert!(!c.blocked("Bo", Some("r9")));
        // Mute follows the id into another slot.
        assert!(c.toggle_mute(3));
        c.ids.insert(5, "r3".into());
        c.ids.remove(&3);
        assert!(c.muted_slot(5));
        assert!(!c.toggle_mute(5));
        assert!(!c.muted_slot(5) && c.muted_ids.is_empty());
        // The Friends lists' blocked riders win for ids: Bo was unblocked
        // there; a name-only block stays; Di is newly blocked.
        c.toggle_block(&mut settings, "Eve", None);
        let server = [("r3".to_string(), "Di".to_string())];
        assert!(sync_blocked(&mut settings, &server));
        let saved = load_blocked(&settings);
        assert_eq!(
            saved,
            [
                Blocked {
                    name: "Eve".into(),
                    pid: None
                },
                Blocked {
                    name: "Di".into(),
                    pid: Some("r3".into())
                },
            ]
        );
        assert!(!sync_blocked(&mut settings, &server), "unchanged");
        assert!(is_blocked_in(&saved, "Di Renamed", Some("r3")));
    }

    fn is_blocked_in(list: &[Blocked], name: &str, pid: Option<&str>) -> bool {
        holds(list, name, pid)
    }

    #[test]
    fn a_verified_id_outranks_the_name() {
        let list = [Blocked {
            name: "Dusty".into(),
            pid: Some("p1".into()),
        }];
        assert!(holds(&list, "Someone", Some("p1")));
        assert!(!holds(&list, "Dusty", Some("p2")));
        assert!(holds(&list, "dusty", None));
    }

    #[test]
    fn wheel_keys_pick_lines_emotes_and_the_post_race_set() {
        let mut w = Wheel::default();
        let free = |_: &str| true;
        assert_eq!(w.key("Digit1", false, free), None);
        assert_eq!(
            w.key("KeyB", false, free),
            Some(Action::Chat(chat::HORN, 0))
        );
        w.key("KeyT", false, free);
        assert!(w.open);
        assert_eq!(
            w.key("Digit2", false, free),
            Some(Action::Chat(chat::LINE, 1))
        );
        assert!(!w.open);
        w.toggle();
        assert_eq!(
            w.key("Digit8", true, free),
            Some(Action::Chat(chat::EMOTE, 7))
        );
        w.post = true;
        w.toggle();
        assert_eq!(
            w.key("Digit2", false, free),
            Some(Action::Chat(chat::LINE, chat::POST_LINES[1]))
        );
        // A bound key stays the player's.
        assert_eq!(w.key("KeyT", false, |k| k != "KeyT"), None);
        assert!(!w.open);
    }

    #[test]
    fn the_pad_opens_moves_and_picks() {
        let mut w = Wheel::default();
        let mut buttons = [0f32; 17];
        let mut press = |w: &mut Wheel, i: usize, bound: u8| {
            buttons[i] = 1.;
            let a = w.pad(&buttons, |b| b == bound);
            buttons[i] = 0.;
            w.pad(&buttons, |b| b == bound);
            a
        };
        // Select is bound: R3 opens.
        assert_eq!(press(&mut w, 8, 8), None);
        assert!(!w.open);
        press(&mut w, 11, 8);
        assert!(w.open);
        press(&mut w, PAD_RIGHT, 8);
        assert_eq!(press(&mut w, PAD_A, 8), Some(Action::Chat(chat::LINE, 1)));
        press(&mut w, 11, 8);
        press(&mut w, PAD_LEFT, 8);
        assert_eq!(w.focus, Some(9));
        assert_eq!(press(&mut w, PAD_A, 8), Some(Action::Chat(chat::HORN, 0)));
    }
}
