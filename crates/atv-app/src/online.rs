//! Online identity in the app (research/ONLINE_IDENTITY.md): the rider
//! secret in the settings (carried by the Pass Key save, atv_ui::cloud),
//! registration with the game server for a token and friend code, the
//! token on every room socket (`?rider=`), and the Online pages' requests.
//! Nothing is sent before the rider first uses something online.
use crate::{App, Message, net};
use atv_ui::cloud::{RIDER_KEY, rider_secret};
use atv_ui::front::controller::online_page::{Board, Lists, Me, OnlineCommand, Reach};
use atv_ui::settings::Settings;
use serde_json::{Value, json};
use std::sync::Mutex;
use web_time::Instant;

/// The token room sockets present, once registered.
static TOKEN: Mutex<String> = Mutex::new(String::new());

/// Lists refresh this often while an Online page shows (presence).
const LISTS_S: f32 = 20.;
/// The stored secret is compared with the registered one this often (a
/// Pass Key on another device may hand this one its rider).
const SECRET_S: f32 = 10.;

/// `url` with the rider token, when this rider has one.
pub fn with_rider(url: String) -> String {
    let token = TOKEN.lock().map(|t| t.clone()).unwrap_or_default();
    if token.is_empty() {
        return url;
    }
    let join = if url.contains('?') { '&' } else { '?' };
    format!("{url}{join}rider={token}")
}

/// What a request was for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Purpose {
    Register,
    Me,
    Lists,
    Board,
    /// Add, accept, decline… (the answer's `state`).
    Act,
    Settings,
    NewCode,
}

#[derive(Default)]
pub struct Online {
    /// The secret the token was issued for.
    registered: Option<String>,
    token: Option<String>,
    registering: bool,
    /// The rider has used something online on this device.
    wanted: Option<bool>,
    lists_at: Option<Instant>,
    secret_at: Option<Instant>,
    /// Consecutive failed registrations (backs off).
    failures: u32,
    retry_at: Option<Instant>,
}

/// 16 bytes from the platform's cryptographic random source, as hex.
fn new_secret() -> String {
    let mut bytes = [0u8; 16];
    #[cfg(target_arch = "wasm32")]
    {
        let filled = web_sys::window()
            .and_then(|w| w.crypto().ok())
            .is_some_and(|c| c.get_random_values_with_u8_array(&mut bytes).is_ok());
        assert!(filled, "no cryptographic random source");
    }
    #[cfg(not(target_arch = "wasm32"))]
    getrandom::fill(&mut bytes).expect("no cryptographic random source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Wall clock in ms since the epoch (the league's "ends in").
fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    return js_sys::Date::now() as u64;
    #[cfg(not(target_arch = "wasm32"))]
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// A short line for a request's outcome.
fn state_line(state: &str) -> &'static str {
    match state {
        "sent" => "Sent ✓",
        "friends" => "Friends ✓",
        "declined" => "Declined",
        "cancelled" => "Cancelled",
        "removed" => "Removed",
        "blocked" => "Blocked",
        "unblocked" => "Unblocked",
        _ => "Done",
    }
}

impl App {
    fn online_send(&mut self, purpose: Purpose, path: &str, mut body: Value) {
        let Some((site, server)) = self
            .defaults
            .as_ref()
            .map(|d| (d.site.clone(), d.server.clone()))
        else {
            return;
        };
        if purpose != Purpose::Register {
            let Some(token) = self.online.token.clone() else {
                return;
            };
            body["token"] = token.into();
        }
        let url = format!("{}{path}", server.trim_end_matches('/'));
        let origin = net::origin(&site);
        let body = body.to_string();
        self.spawn(
            async move { net::post(&url, &origin, &body).await },
            move |result| Message::Online(purpose, result),
        );
    }

    /// The rider's secret, made on first use.
    fn online_secret(&mut self) -> String {
        let mut settings = Settings::load();
        if let Some(secret) = rider_secret(&settings) {
            return secret;
        }
        let secret = new_secret();
        settings.set(RIDER_KEY, &secret);
        secret
    }

    fn online_register(&mut self) {
        if self.defaults.is_none() {
            return;
        }
        let secret = self.online_secret();
        let name = self
            .front
            .as_ref()
            .map(|f| f.borrow().state.player_name.clone())
            .unwrap_or_default();
        self.online.registering = true;
        self.online.registered = Some(secret.clone());
        self.online_send(
            Purpose::Register,
            "/multiplayer/players/register",
            json!({"secret": secret, "name": name}),
        );
    }

    /// The rider used something online (a room, an Online page): register.
    pub(crate) fn online_wanted(&mut self) {
        self.online.wanted = Some(true);
        if self.online.token.is_none() && !self.online.registering {
            self.online_register();
        }
    }

    fn online_refresh(&mut self, page: &str) {
        self.online.lists_at = Some(Instant::now());
        if page == "online" {
            self.online_send(Purpose::Me, "/multiplayer/players/me", json!({}));
        }
        self.online_send(Purpose::Lists, "/multiplayer/friends", json!({}));
    }

    /// An Online page's request.
    pub(crate) fn online_command(&mut self, command: OnlineCommand) {
        self.online_wanted();
        if self.online.token.is_none() {
            // The registration's answer refreshes the open page.
            return;
        }
        match command {
            OnlineCommand::Refresh => self.online_refresh("online"),
            OnlineCommand::Board => {
                self.online_send(Purpose::Board, "/multiplayer/league", json!({}))
            }
            OnlineCommand::Add(code) => self.online_send(
                Purpose::Act,
                "/multiplayer/friends/add",
                json!({"code": code}),
            ),
            OnlineCommand::Act(action, id) => self.online_send(
                Purpose::Act,
                &format!("/multiplayer/friends/{action}"),
                json!({"id": id}),
            ),
            OnlineCommand::Hidden(hidden) => self.online_send(
                Purpose::Settings,
                "/multiplayer/players/settings",
                json!({"hidden": hidden}),
            ),
            OnlineCommand::NewCode => {
                self.online_send(Purpose::NewCode, "/multiplayer/players/code", json!({}))
            }
            OnlineCommand::Copy(code) => self.copy_text(code, "Code copied ✓"),
        }
    }

    /// Each event-loop pass: register a rider who uses online, follow a
    /// secret a Pass Key brought, and keep an open Online page's presence
    /// fresh.
    pub(crate) fn online_poll(&mut self) {
        let Some(front) = self.front.clone() else {
            return;
        };
        if self.defaults.is_none() {
            return;
        }
        let now = Instant::now();
        let due = |at: Option<Instant>, s: f32| at.is_none_or(|t| t.elapsed().as_secs_f32() >= s);
        if due(self.online.secret_at, SECRET_S) {
            self.online.secret_at = Some(now);
            let stored = rider_secret(&Settings::load());
            if self.online.wanted.is_none() {
                self.online.wanted = Some(stored.is_some());
            }
            if stored.is_some() && stored != self.online.registered && !self.online.registering {
                // A new rider (another device's, through the Pass Key).
                self.online.token = None;
                self.online.wanted = Some(true);
            }
        }
        if self.online.wanted == Some(true)
            && self.online.token.is_none()
            && !self.online.registering
            && self.online.retry_at.is_none_or(|t| now >= t)
        {
            self.online_register();
            return;
        }
        let page = {
            let mut f = front.borrow_mut();
            f.state.online.now_ms = now_ms();
            f.state.page.clone()
        };
        if atv_ui::front::controller::online_page::PAGES.contains(&page.as_str())
            && self.online.token.is_some()
            && due(self.online.lists_at, LISTS_S)
        {
            self.online_refresh(&page);
        }
    }

    pub(crate) fn online_response(
        &mut self,
        purpose: Purpose,
        result: Result<(u16, String), String>,
    ) {
        let (status, body) = result.unwrap_or((0, String::new()));
        let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        if purpose == Purpose::Register {
            self.online.registering = false;
        }
        if status == 401 && purpose != Purpose::Register {
            // The token lapsed or the server lost its key: register again.
            self.online.token = None;
            return;
        }
        let Some(front) = self.front.clone() else {
            return;
        };
        let mut f = front.borrow_mut();
        let view = f.view;
        let state = &mut f.state;
        if status != 200 {
            let message = value["error"].as_str().map(str::to_string);
            if purpose == Purpose::Register {
                // 10 s, doubling to about 10 minutes.
                self.online.failures += 1;
                let wait = 5u64 << self.online.failures.min(7);
                self.online.retry_at = Some(Instant::now() + std::time::Duration::from_secs(wait));
            }
            match purpose {
                Purpose::Register | Purpose::Me | Purpose::Lists
                    if status == 0 || status >= 404 =>
                {
                    state.online.reach = Reach::Offline;
                    state.online_changed(&view);
                }
                Purpose::Act | Purpose::NewCode | Purpose::Settings | Purpose::Board => {
                    state.online_message(
                        message.unwrap_or_else(|| {
                            if status == 0 {
                                "Offline".into()
                            } else {
                                format!("Error {status}")
                            }
                        }),
                        &view,
                    );
                }
                _ => {}
            }
            drop(f);
            self.wake();
            return;
        }
        state.online.reach = Reach::Ready;
        let mut refresh = false;
        match purpose {
            Purpose::Register => {
                self.online.failures = 0;
                self.online.retry_at = None;
                let token = value["token"].as_str().unwrap_or_default().to_string();
                if let Ok(mut shared) = TOKEN.lock() {
                    shared.clone_from(&token);
                }
                self.online.token = (!token.is_empty()).then_some(token);
                refresh =
                    atv_ui::front::controller::online_page::PAGES.contains(&state.page.as_str());
            }
            Purpose::Me | Purpose::Settings => {
                if let Ok(me) = serde_json::from_value::<Me>(value) {
                    state.online.me = Some(me);
                }
                state.online_changed(&view);
            }
            Purpose::Lists => {
                if let Ok(lists) = serde_json::from_value::<Lists>(value) {
                    state.online.lists = Some(lists);
                }
                state.online_changed(&view);
            }
            Purpose::Board => {
                if let Ok(board) = serde_json::from_value::<Board>(value) {
                    state.online.board = Some(board);
                }
                state.online_changed(&view);
            }
            Purpose::Act => {
                state.notice = Some(state_line(value["state"].as_str().unwrap_or("")).into());
                state.online_done(&view);
                refresh = true;
            }
            Purpose::NewCode => {
                if let (Some(me), Some(code)) = (&mut state.online.me, value["code"].as_str()) {
                    me.code = code.to_string();
                }
                state.notice = Some("New code ✓".into());
                state.online_changed(&view);
            }
        }
        let page = state.page.clone();
        drop(f);
        if refresh && self.online.token.is_some() {
            self.online_send(Purpose::Me, "/multiplayer/players/me", json!({}));
            self.online_refresh(&page);
        }
        self.wake();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sockets_carry_the_token_once_registered() {
        let url = "wss://mp.example/multiplayer/rooms/ABCD1234?key=00".to_string();
        assert_eq!(super::with_rider(url.clone()), url);
        *super::TOKEN.lock().unwrap() = "r1.ab.1.cd".into();
        assert_eq!(
            super::with_rider(url.clone()),
            format!("{url}&rider=r1.ab.1.cd")
        );
        assert_eq!(
            super::with_rider("ws://h/multiplayer/rooms/X".into()),
            "ws://h/multiplayer/rooms/X?rider=r1.ab.1.cd"
        );
        super::TOKEN.lock().unwrap().clear();
    }
}
