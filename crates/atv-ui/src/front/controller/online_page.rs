//! Online identity pages (port addition; research/ONLINE_IDENTITY.md): the
//! Online card (friend code, level, weekly league, stats), Friends with
//! presence and Join, a rider's actions, Add Code, Recent riders and the
//! weekly board. Numbers and icons, few words. The host talks to the game
//! server ([`OnlineCommand`]) and fills [`OnlineView`].
use super::{Event, Frontend, Viewport};
use serde::Deserialize;

pub const PAGES: [&str; 6] = [
    "online",
    "friends",
    "friend",
    "friend-add",
    "friend-recent",
    "league",
];

/// Friend-code symbols (the server's alphabet: no 0, 1, A, E, I, L, O, U).
pub const CODE_ALPHABET: &[u8; 28] = b"23456789BCDFGHJKMNPQRSTVWXYZ";
pub const CODE_LEN: usize = 8;

/// Whether the game server answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reach {
    #[default]
    Loading,
    Ready,
    /// No network, or a server without online identity.
    Offline,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct League {
    pub tier: String,
    pub points: u32,
    pub next: Option<u32>,
    pub ends_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct OnlineStats {
    pub races: u32,
    pub wins: u32,
    pub podiums: u32,
}

/// `/multiplayer/players/me`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Me {
    pub code: String,
    pub name: String,
    pub hidden: bool,
    pub level: u32,
    pub xp: u64,
    pub level_xp: u64,
    pub next_xp: u64,
    pub league: League,
    pub stats: OnlineStats,
    pub friends: u32,
    pub requests: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Room {
    pub kind: String,
    /// Present when the room can be joined (Free Ride, private).
    pub code: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Rider {
    pub id: String,
    pub name: String,
    pub level: u32,
    pub tier: String,
    pub online: bool,
    pub room: Option<Room>,
    /// Recent riders: seconds since met, and friend / sent / asked / add.
    pub ago: u64,
    pub state: String,
}

/// `/multiplayer/friends`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Lists {
    pub friends: Vec<Rider>,
    pub requests: Vec<Rider>,
    pub sent: Vec<Rider>,
    pub recent: Vec<Rider>,
    pub blocked: Vec<Rider>,
    pub max: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct BoardRow {
    pub rank: u32,
    pub name: String,
    pub points: u32,
    pub level: u32,
    pub tier: String,
    pub me: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct BoardMe {
    pub rank: u32,
    pub top: u32,
}

/// `/multiplayer/league`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Board {
    pub riders: u32,
    pub ends_ms: u64,
    pub top: Vec<BoardRow>,
    pub me: Option<BoardMe>,
}

/// The menus' copy of the rider's online state, filled by the host.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OnlineView {
    pub reach: Reach,
    pub me: Option<Me>,
    pub lists: Option<Lists>,
    pub board: Option<Board>,
    /// The host clock (ms since the epoch) for "ends in".
    pub now_ms: u64,
}

/// What the host must do for an Online page.
#[derive(Clone, Debug, PartialEq)]
pub enum OnlineCommand {
    /// The card and lists are wanted (an Online page opened).
    Refresh,
    Board,
    /// Send a request to a friend code.
    Add(String),
    /// accept, decline, cancel, remove, block, unblock or add (a recent
    /// rider) for a rider reference.
    Act(&'static str, String),
    Hidden(bool),
    NewCode,
    /// Copy the friend code.
    Copy(String),
}

/// Which list the open rider page came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum From {
    #[default]
    Friend,
    Request,
    Sent,
    Recent,
    Blocked,
}

/// The pages' working state.
#[derive(Clone, Debug, Default)]
pub struct OnlineUi {
    /// The typed friend code (no dash).
    pub entry: String,
    /// The controller row's symbol (index into CODE_ALPHABET).
    pub letter: usize,
    /// The open rider page's reference and list.
    pub rider: Option<(String, From)>,
}

const TIER_ICON: &str = "◆";

fn tier_name(tier: &str) -> String {
    let mut chars = tier.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_else(|| "Bronze".into())
}

/// A ten-step bar for `part` of `whole`.
pub fn bar(part: u64, whole: u64) -> String {
    let filled = if whole == 0 {
        0
    } else {
        (part.min(whole) * 10 / whole) as usize
    };
    format!("{}{}", "▰".repeat(filled), "▱".repeat(10 - filled))
}

/// `2d 4h`, `3h 10m`, `5m`.
fn span(ms: u64) -> String {
    let minutes = ms / 60_000;
    let (d, h, m) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{}m", m.max(1))
    }
}

/// `XXXX-XXXX` with blanks for the rest.
fn grouped_code(symbols: &str) -> String {
    let padded: String = symbols
        .chars()
        .chain(std::iter::repeat('_'))
        .take(CODE_LEN)
        .collect();
    format!("{}-{}", &padded[..4], &padded[4..])
}

/// A typed code's symbols: case, spaces and dashes do not matter; other
/// symbols are dropped.
pub fn clean_code(text: &str) -> String {
    text.chars()
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| c.is_ascii() && CODE_ALPHABET.contains(&(*c as u8)))
        .take(CODE_LEN)
        .collect()
}

impl Frontend {
    pub(super) fn is_online_page(page: &str) -> bool {
        PAGES.contains(&page)
    }

    /// The host's answers arrived: redraw an Online page.
    pub fn online_changed(&mut self, view: &Viewport) {
        if Self::is_online_page(&self.page) {
            self.refresh(view);
        }
    }

    /// A server answer to show (a request's outcome or why it failed).
    pub fn online_message(&mut self, message: String, view: &Viewport) {
        self.notice = Some(message);
        self.online_changed(view);
    }

    fn rider_of(&self, id: &str, from: From) -> Option<&Rider> {
        let lists = self.online.lists.as_ref()?;
        let list = match from {
            From::Friend => &lists.friends,
            From::Request => &lists.requests,
            From::Sent => &lists.sent,
            From::Recent => &lists.recent,
            From::Blocked => &lists.blocked,
        };
        list.iter().find(|r| r.id == id)
    }

    fn open_rider(&self) -> Option<(&Rider, From)> {
        let (id, from) = self.online_ui.rider.as_ref()?;
        Some((self.rider_of(id, *from)?, *from))
    }

    pub(super) fn online_choices(&self) -> Vec<(String, String)> {
        let row = |a: &str, l: &str| (a.to_string(), l.to_string());
        let ready = self.online.reach == Reach::Ready;
        let lists = self.online.lists.as_ref();
        let mut rows = match self.page.as_str() {
            "online" if ready => {
                let me = self.online.me.as_ref();
                let requests =
                    lists.map_or(me.map_or(0, |m| m.requests) as usize, |l| l.requests.len());
                let online = lists.map_or(0, |l| l.friends.iter().filter(|f| f.online).count());
                let mut friends = "Friends".to_string();
                if online > 0 {
                    friends.push_str(&format!(" · ● {online}"));
                }
                if requests > 0 {
                    friends.push_str(&format!(" · +{requests}"));
                }
                vec![
                    ("friends".into(), friends),
                    row("league", "League"),
                    row("copycode", "Copy Code"),
                    row("newcode", "New Code"),
                    (
                        "appearoffline".into(),
                        format!(
                            "Appear Offline: {}",
                            if me.is_some_and(|m| m.hidden) {
                                "On"
                            } else {
                                "Off"
                            }
                        ),
                    ),
                ]
            }
            "online" => vec![row("onlineretry", "Retry")],
            "friends" if ready => {
                let mut rows = vec![row("friend-add", "Add Code")];
                if let Some(lists) = lists {
                    rows.push((
                        "friend-recent".into(),
                        format!("Recent · {}", lists.recent.len()),
                    ));
                    for r in &lists.requests {
                        rows.push((format!("request:{}", r.id), format!("+ {}", r.name)));
                    }
                    for f in &lists.friends {
                        let dot = if f.online { "●" } else { "○" };
                        let join = if f.room.as_ref().is_some_and(|r| r.code.is_some()) {
                            " · Join"
                        } else {
                            ""
                        };
                        rows.push((
                            format!("friend:{}", f.id),
                            format!("{dot} {}{join}", f.name),
                        ));
                    }
                    for s in &lists.sent {
                        rows.push((format!("sent:{}", s.id), format!("… {}", s.name)));
                    }
                    for b in &lists.blocked {
                        rows.push((format!("blocked:{}", b.id), format!("⊘ {}", b.name)));
                    }
                }
                rows
            }
            "friend" => match self.open_rider() {
                Some((r, From::Friend)) => {
                    let mut rows = Vec::new();
                    if r.room.as_ref().is_some_and(|r| r.code.is_some()) {
                        rows.push(row("riderjoin", "Join"));
                    }
                    rows.push(row("riderremove", "Remove"));
                    rows.push(row("riderblock", "Block"));
                    rows
                }
                Some((_, From::Request)) => vec![
                    row("rideraccept", "Accept"),
                    row("riderdecline", "Decline"),
                    row("riderblock", "Block"),
                ],
                Some((_, From::Sent)) => vec![row("ridercancel", "Cancel")],
                Some((r, From::Recent)) => {
                    let mut rows = Vec::new();
                    match r.state.as_str() {
                        "add" => rows.push(row("rideradd", "Add Friend")),
                        "asked" => rows.push(row("rideraccept", "Accept")),
                        _ => {}
                    }
                    rows.push(row("riderblock", "Block"));
                    rows
                }
                Some((_, From::Blocked)) => vec![row("riderunblock", "Unblock")],
                None => Vec::new(),
            },
            "friend-add" => {
                let symbol = CODE_ALPHABET[self.online_ui.letter % CODE_ALPHABET.len()] as char;
                vec![
                    ("friendletter".into(), format!("Add Symbol: {symbol}")),
                    row("frienderase", "Delete Symbol"),
                    row("friendsend", "Send"),
                ]
            }
            "friend-recent" => lists.map_or_else(Vec::new, |l| {
                l.recent
                    .iter()
                    .map(|r| {
                        let mark = match r.state.as_str() {
                            "friend" => "✓ ",
                            "sent" => "… ",
                            "asked" => "+ ",
                            _ => "",
                        };
                        (
                            format!("recent:{}", r.id),
                            format!("{mark}{} · {}", r.name, span(r.ago * 1000)),
                        )
                    })
                    .collect()
            }),
            _ => Vec::new(),
        };
        rows.push(row("back", "Back"));
        rows
    }

    pub(super) fn online_heading(&self) -> Option<String> {
        Some(match self.page.as_str() {
            "online" => "Online".into(),
            "friends" => "Friends".into(),
            "friend" => self
                .open_rider()
                .map_or_else(|| "Rider".into(), |(r, _)| r.name.clone()),
            "friend-add" => "Add Friend".into(),
            "friend-recent" => "Recent Riders".into(),
            "league" => "Weekly League".into(),
            _ => return None,
        })
    }

    pub(super) fn online_status(&self) -> Option<String> {
        match self.page.as_str() {
            "friend-add" => Some("8 letters and numbers".into()),
            _ => None,
        }
    }

    pub(super) fn online_details(&self) -> Vec<(String, String)> {
        let line = |l: &str, v: String| (l.to_string(), v);
        match self.online.reach {
            Reach::Loading if self.online.me.is_none() => {
                return vec![line("Loading…", String::new())];
            }
            Reach::Offline => return vec![line("● Offline", String::new())],
            _ => {}
        }
        let now = self.online.now_ms;
        match self.page.as_str() {
            "online" => {
                let Some(me) = &self.online.me else {
                    return vec![line("Loading…", String::new())];
                };
                let into = me.xp.saturating_sub(me.level_xp);
                let need = me.next_xp.saturating_sub(me.level_xp);
                let mut lines = vec![
                    line("Friend Code", me.code.clone()),
                    line(
                        &format!("Level {}", me.level),
                        format!("{} {into}/{need}", bar(into, need)),
                    ),
                    line(
                        "League",
                        format!(
                            "{TIER_ICON} {} · {}",
                            tier_name(&me.league.tier),
                            me.league.points
                        ),
                    ),
                ];
                if let Some(next) = me.league.next {
                    lines.push(line(
                        "Next Tier",
                        format!(
                            "{} {}/{next}",
                            bar(u64::from(me.league.points), u64::from(next)),
                            me.league.points
                        ),
                    ));
                }
                if me.league.ends_ms > now && now > 0 {
                    lines.push(line("Week Ends", span(me.league.ends_ms - now)));
                }
                lines.push(line("Races", me.stats.races.to_string()));
                lines.push(line("Wins", me.stats.wins.to_string()));
                lines.push(line("Podiums", me.stats.podiums.to_string()));
                lines
            }
            "friends" => {
                let Some(lists) = &self.online.lists else {
                    return vec![line("Loading…", String::new())];
                };
                let online = lists.friends.iter().filter(|f| f.online).count();
                let mut lines = vec![
                    line(
                        "Friends",
                        format!("{}/{}", lists.friends.len(), lists.max.max(1)),
                    ),
                    line("● Online", online.to_string()),
                ];
                if !lists.requests.is_empty() {
                    lines.push(line("+ Requests", lists.requests.len().to_string()));
                }
                if let Some(me) = &self.online.me {
                    lines.push(line("Your Code", me.code.clone()));
                }
                lines
            }
            "friend" => match self.open_rider() {
                Some((r, from)) => {
                    let mut lines = Vec::new();
                    if r.level > 0 {
                        lines.push(line("Level", r.level.to_string()));
                        lines.push(line(
                            "League",
                            format!("{TIER_ICON} {}", tier_name(&r.tier)),
                        ));
                    }
                    match from {
                        From::Friend => lines.push(line(
                            if r.online {
                                "● Online"
                            } else {
                                "○ Offline"
                            },
                            r.room.as_ref().map_or(String::new(), |room| {
                                match room.kind.as_str() {
                                    "freeride" => "Free Ride",
                                    "quickplay" => "Quick Play",
                                    _ => "Private Room",
                                }
                                .into()
                            }),
                        )),
                        From::Recent => lines.push(line("Met", span(r.ago * 1000))),
                        From::Request => lines.push(line("+ Wants to be friends", String::new())),
                        From::Sent => lines.push(line("… Sent", String::new())),
                        From::Blocked => lines.push(line("Blocked", String::new())),
                    }
                    lines
                }
                None => Vec::new(),
            },
            "friend-add" => vec![line("Code", grouped_code(&self.online_ui.entry))],
            "friend-recent" => match &self.online.lists {
                Some(l) if l.recent.is_empty() => vec![line("No riders yet", String::new())],
                Some(l) => vec![line("Last 7 days", l.recent.len().to_string())],
                None => vec![line("Loading…", String::new())],
            },
            "league" => {
                let Some(board) = &self.online.board else {
                    return vec![line("Loading…", String::new())];
                };
                let mut lines: Vec<(String, String)> = board
                    .top
                    .iter()
                    .take(10)
                    .map(|r| {
                        let mark = if r.me { "▶ " } else { "" };
                        (
                            format!("{mark}{}. {} · Lv {}", r.rank, r.name, r.level),
                            r.points.to_string(),
                        )
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push(line("No points yet", String::new()));
                }
                if let Some(me) = &board.me {
                    lines.push(line(
                        &format!("▶ You #{}", me.rank),
                        format!("Top {}%", me.top.max(1)),
                    ));
                }
                if board.ends_ms > now && now > 0 {
                    lines.push(line("Week Ends", span(board.ends_ms - now)));
                }
                lines
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn online_back(&self) -> Option<&'static str> {
        Some(match self.page.as_str() {
            "online" => "profile",
            "friends" | "league" => "online",
            "friend" => match self.online_ui.rider.as_ref().map(|r| r.1) {
                Some(From::Recent) => "friend-recent",
                _ => "friends",
            },
            "friend-add" | "friend-recent" => "friends",
            _ => return None,
        })
    }

    /// Left/Right on the symbol row.
    pub(super) fn step_friend_code(&mut self, direction: i32, view: &Viewport) {
        let n = CODE_ALPHABET.len() as i32;
        self.online_ui.letter = (self.online_ui.letter as i32 + direction).rem_euclid(n) as usize;
        self.refresh(view);
    }

    pub(super) fn set_friend_entry(&mut self, text: &str) {
        self.online_ui.entry = clean_code(text);
        self.notice = None;
    }

    /// Opens the Online card (Profile Editor → Online).
    pub fn open_online(&mut self, view: &Viewport) -> Option<Event> {
        self.show("online", view);
        Some(Event::Online(OnlineCommand::Refresh))
    }

    /// An Online page's row; None when the action is not one of them.
    pub(super) fn online_action(&mut self, action: &str, view: &Viewport) -> Option<Option<Event>> {
        let rider = |ui: &OnlineUi| ui.rider.as_ref().map(|r| r.0.clone());
        let act = |name: &'static str, id: Option<String>| {
            id.map(|id| Event::Online(OnlineCommand::Act(name, id)))
        };
        let event = match action {
            "online" | "onlineretry" => return Some(self.open_online(view)),
            "friends" => {
                self.online_ui.rider = None;
                self.show("friends", view);
                Some(Event::Online(OnlineCommand::Refresh))
            }
            "league" => {
                self.show("league", view);
                Some(Event::Online(OnlineCommand::Board))
            }
            "copycode" => self
                .online
                .me
                .as_ref()
                .map(|m| Event::Online(OnlineCommand::Copy(m.code.clone()))),
            "newcode" => {
                self.notice = None;
                Some(Event::Online(OnlineCommand::NewCode))
            }
            "appearoffline" => {
                let hidden = !self.online.me.as_ref().is_some_and(|m| m.hidden);
                if let Some(me) = &mut self.online.me {
                    me.hidden = hidden;
                }
                self.refresh(view);
                Some(Event::Online(OnlineCommand::Hidden(hidden)))
            }
            "friend-add" => {
                self.online_ui.entry.clear();
                self.show("friend-add", view);
                None
            }
            "friend-recent" => {
                self.show("friend-recent", view);
                Some(Event::Online(OnlineCommand::Refresh))
            }
            "friendletter" => {
                if self.online_ui.entry.len() < CODE_LEN {
                    let symbol = CODE_ALPHABET[self.online_ui.letter % CODE_ALPHABET.len()];
                    self.online_ui.entry.push(char::from(symbol));
                }
                self.refresh(view);
                None
            }
            "frienderase" => {
                self.online_ui.entry.pop();
                self.refresh(view);
                None
            }
            "friendsend" => {
                if self.online_ui.entry.len() == CODE_LEN {
                    self.notice = None;
                    Some(Event::Online(OnlineCommand::Add(
                        self.online_ui.entry.clone(),
                    )))
                } else {
                    self.notice = Some("8 symbols".into());
                    self.refresh(view);
                    None
                }
            }
            "riderjoin" => {
                let code = self
                    .open_rider()
                    .and_then(|(r, _)| r.room.as_ref()?.code.clone());
                code.map(|code| Event::Join {
                    environment: if atv_game::multiplayer::is_shared_room(&code) {
                        atv_game::multiplayer::FREERIDE_ENVIRONMENT.into()
                    } else {
                        self.tracks
                            .get(self.track)
                            .map_or("env01".into(), |t| t.id.clone())
                    },
                    code,
                    quad: if self.quad_id >= 27 { 1 } else { self.quad_id },
                    rider: self.rider_id,
                    name: self.room_name(),
                })
            }
            "rideraccept" => act("accept", rider(&self.online_ui)),
            "riderdecline" => act("decline", rider(&self.online_ui)),
            "ridercancel" => act("cancel", rider(&self.online_ui)),
            "riderremove" => act("remove", rider(&self.online_ui)),
            "riderblock" => act("block", rider(&self.online_ui)),
            "riderunblock" => act("unblock", rider(&self.online_ui)),
            "rideradd" => act("add", rider(&self.online_ui)),
            _ => {
                let (from, id) = if let Some(id) = action.strip_prefix("friend:") {
                    (From::Friend, id)
                } else if let Some(id) = action.strip_prefix("request:") {
                    (From::Request, id)
                } else if let Some(id) = action.strip_prefix("sent:") {
                    (From::Sent, id)
                } else if let Some(id) = action.strip_prefix("recent:") {
                    (From::Recent, id)
                } else if let Some(id) = action.strip_prefix("blocked:") {
                    (From::Blocked, id)
                } else {
                    return None;
                };
                self.online_ui.rider = Some((id.to_string(), from));
                self.notice = None;
                self.show("friend", view);
                None
            }
        };
        Some(event)
    }

    /// A rider action finished: back to the list it came from.
    pub fn online_done(&mut self, view: &Viewport) {
        if self.page == "friend" {
            let back = self.online_back().unwrap_or("friends");
            self.show(back, view);
        } else if self.page == "friend-add" {
            self.online_ui.entry.clear();
            self.show("friends", view);
        } else {
            self.online_changed(view);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_spans_and_codes() {
        assert_eq!(bar(0, 100), "▱▱▱▱▱▱▱▱▱▱");
        assert_eq!(bar(50, 100), "▰▰▰▰▰▱▱▱▱▱");
        assert_eq!(bar(500, 100), "▰▰▰▰▰▰▰▰▰▰");
        assert_eq!(bar(1, 0), "▱▱▱▱▱▱▱▱▱▱");
        assert_eq!(span(30_000), "1m");
        assert_eq!(span(3 * 3_600_000 + 600_000), "3h 10m");
        assert_eq!(span(2 * 86_400_000 + 4 * 3_600_000), "2d 4h");
        assert_eq!(clean_code(" kq7m-3xpz "), "KQ7M3XPZ");
        assert_eq!(clean_code("A0O1-IE"), "");
        assert_eq!(grouped_code("KQ7"), "KQ7_-____");
        assert_eq!(tier_name("silver"), "Silver");
    }
}
