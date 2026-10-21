//! A multiplayer ride in the Rust engine: joins or hosts a relay room, sends
//! the player's pose at the adaptive cadence and draws the other riders with
//! their own quads and riders (web/multiplayer.js `connect`/`send`/`sample`).
use crate::net::{self, Event};
use atv_content::Content;
use atv_game::multiplayer::{
    Profile, Session,
    remote::{self, Key, Local, RemoteModel},
};
use atv_game::ride::{Pose, Ride};
use atv_render::{Gpu, SceneRenderer};
use std::{
    collections::{BTreeMap, HashMap},
    rc::Rc,
};
use web_time::Instant;

/// How a ride starts in a room (`--host`/`--join`, `?host=1`/`?room=`).
#[derive(Clone, Debug)]
pub struct RoomStart {
    /// The relay origin (`http(s)://host[:port]`), the page origin on the web.
    pub relay: String,
    /// The room code: required to join; optional custom code to host.
    pub code: Option<String>,
    pub host: bool,
    /// Arlington Free Ride: the relay reserves a slot in a shared room.
    pub freeride: bool,
    pub name: String,
    /// Test hook (`?dropAfter=S`): close the socket once after S seconds of
    /// riding, exercising the slot-key reconnect.
    pub drop_after: Option<f64>,
    /// Test hook (`?trainPlace=S`): after S seconds of riding, stand on the
    /// train's flatbed.
    pub train_place: Option<f64>,
}

enum Loaded {
    Loading,
    Ready(Rc<RemoteModel>),
    Failed,
}

/// The live room beside a ride.
pub struct Net {
    pub session: Session,
    socket: net::Socket,
    content: Rc<Content>,
    clock: Instant,
    relay: String,
    last_send: f64,
    last_ping: f64,
    last_traffic: f64,
    last_frame: Option<f64>,
    ready_sent: bool,
    models: HashMap<Key, Loaded>,
    locals: BTreeMap<u8, Local>,
    /// Why the room ended (closed socket, leave, protocol error).
    pub closed: Option<String>,
    /// Remote riders drawn on the latest frame.
    pub drawn: usize,
    /// Each presented rider's slot, ATV position and staleness (tests).
    presented: Vec<(u8, [f32; 3], bool, Option<u8>)>,
    /// The course this client has loaded (the room may move on).
    pub environment: String,
    /// Ride choices to reload with when the room changes course.
    pub ride: RoomRide,
    /// Arriving while others ride: next meet attempt (ms) and attempts left.
    meet: Option<(f64, u32)>,
    /// A dropped socket retries its slot key: next attempt (ms), deadline,
    /// attempts so far.
    reconnect: Option<(f64, f64, u32)>,
    pub reconnects: u32,
    drop_at: Option<f64>,
    train_place_at: Option<f64>,
    /// The slot key last kept for this tab.
    saved_key: String,
    /// The latest arrival beside another rider (tests): slot and distance.
    pub met: Option<(u8, f32)>,
}

/// What the app does after a room tick.
#[derive(Default)]
pub struct Tick {
    /// Remote models to load.
    pub loads: Vec<Key>,
    /// Place the player here (arrival beside a rider).
    pub place: Option<([f32; 3], f32)>,
    /// The room moved to this course: reload the ride there.
    pub course: Option<String>,
    /// Test hook: stand on the train's flatbed now.
    pub train_place: bool,
}

/// The player's ride choices, kept to reload on a course change.
#[derive(Clone)]
pub struct RoomRide {
    pub graphics: atv_render::Graphics,
    pub weather: Option<bool>,
    pub crash: atv_game::crash::CrashMode,
    pub sound: Option<(atv_game::audio::ride::SoundMode, bool)>,
    pub appearance: atv_game::ride::gear::Appearance,
}

/// Close codes a rider with a slot key retries (web/multiplayer.js).
fn retryable(code: u16, reason: &str) -> bool {
    matches!(code, 1001 | 1005 | 1006 | 1011 | 1012 | 1013)
        || (code == 1000 && reason == "Connection timed out")
}

fn now_ms(clock: Instant) -> u64 {
    clock.elapsed().as_millis() as u64
}

/// Creates (`host`) or opens (`join`, `open`) the room on the relay and
/// returns its code and, when this rider becomes its creator, the slot key.
async fn claim(start: &RoomStart) -> Result<(String, Option<String>), String> {
    let origin = net::origin(&start.relay);
    if start.freeride {
        // POST /multiplayer/freeride reserves a slot in the first shared
        // room with space (web/multiplayer.js `freeride`).
        let url = format!("{}/multiplayer/freeride", start.relay.trim_end_matches('/'));
        let (status, text) = net::post(&url, &origin, "").await?;
        match status {
            200 => {}
            429 => {
                return Err(
                    "Too many attempts or multiplayer is at capacity. Try again shortly.".into(),
                );
            }
            503 => {
                return Err(
                    "Arlington Free Ride is full or unavailable. Try again shortly.".into(),
                );
            }
            status => return Err(format!("Unable to join Free Ride ({status}).")),
        }
        let room: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let code = room["code"].as_str().unwrap_or_default().to_string();
        let key = room["key"].as_str().unwrap_or_default().to_string();
        let shared = atv_game::multiplayer::is_shared_room(&code);
        if !shared || key.len() != 32 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Unable to join Free Ride.".into());
        }
        return Ok((code, Some(key)));
    }
    let url = format!("{}/multiplayer/rooms", start.relay.trim_end_matches('/'));
    let code = start
        .code
        .as_deref()
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| !c.is_empty());
    if let Some(code) = &code {
        atv_game::multiplayer::room_code(code)?;
    }
    if !start.host && code.is_none() {
        return Err("Enter the 8-letter or number lobby code.".into());
    }
    let body = match (&code, start.host) {
        (Some(code), true) => serde_json::json!({ "code": code }),
        (Some(code), false) => serde_json::json!({ "code": code, "open": true }),
        (None, _) => serde_json::json!({}),
    };
    let (status, text) = net::post(&url, &origin, &body.to_string()).await?;
    match status {
        200 | 201 => {}
        409 => return Err("Unable to create Game, game name already exists".into()),
        429 => {
            return Err(
                "Too many lobby attempts or multiplayer is at capacity. Try again shortly.".into(),
            );
        }
        status => return Err(format!("Game creation failed! ({status})")),
    }
    let room: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let code = room["code"]
        .as_str()
        .ok_or("relay returned no room code")?
        .to_string();
    if room["open"].as_bool() == Some(true) {
        let players = room["players"].as_u64().unwrap_or(0);
        if players >= room["max"].as_u64().unwrap_or(4) {
            return Err("Game is full!".into());
        }
        // This tab's kept slot key reclaims its rider after a reload.
        return Ok((code.clone(), saved_key(&code)));
    }
    Ok((code, room["hostToken"].as_str().map(String::from)))
}

/// The browser tab's slot key for `code` (sessionStorage `atv-mp-session`,
/// shared with web/multiplayer.js); desktop keeps keys in memory only.
fn saved_key(code: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let text = web_sys::window()?
            .session_storage()
            .ok()??
            .get_item("atv-mp-session")
            .ok()??;
        let saved: serde_json::Value = serde_json::from_str(&text).ok()?;
        let key = saved["key"].as_str()?;
        (saved["code"].as_str() == Some(code)
            && key.len() == 32
            && key.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| key.to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = code;
        None
    }
}

/// Keeps this tab's slot key (never in links).
fn save_key(code: &str, key: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = web_sys::window().and_then(|w| w.session_storage().ok().flatten()) {
        let _ = storage.set_item(
            "atv-mp-session",
            &serde_json::json!({ "code": code, "key": key }).to_string(),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (code, key);
}

impl Net {
    /// Claims the room, connects, sends Join and waits for admission. The
    /// host starts the room on `venue`; a guest rides the room's course,
    /// which is returned.
    pub async fn join(
        start: &RoomStart,
        profile: Profile,
        venue: &str,
        content: Rc<Content>,
        ride: RoomRide,
    ) -> Result<(Net, String), String> {
        let (code, key) = claim(start).await?;
        let socket = net::Socket::connect(
            &net::room_url(&start.relay, &code, key.as_deref()),
            &net::origin(&start.relay),
        );
        let mut net = Net {
            session: Session::new(profile),
            socket,
            content,
            clock: Instant::now(),
            relay: start.relay.clone(),
            last_send: 0.,
            last_ping: 0.,
            last_traffic: 0.,
            last_frame: None,
            ready_sent: false,
            models: HashMap::new(),
            locals: BTreeMap::new(),
            closed: None,
            drawn: 0,
            presented: Vec::new(),
            environment: String::new(),
            ride,
            meet: None,
            reconnect: None,
            reconnects: 0,
            drop_at: start.drop_after.map(|s| s * 1000.),
            train_place_at: start.train_place.map(|s| s * 1000.),
            saved_key: String::new(),
            met: None,
        };
        let deadline = Instant::now();
        let mut set_up = false;
        loop {
            if deadline.elapsed().as_secs_f32() > 10. {
                net.socket.close();
                return Err("Lobby connection timed out.".into());
            }
            for event in net.socket.poll() {
                let now = now_ms(net.clock);
                match event {
                    Event::Open => {
                        net.socket.send(net.session.join(&code)?);
                        net.socket.send(net.session.ping(now)?);
                        net.last_ping = now as f64;
                    }
                    Event::Message(bytes) => net.session.receive(&bytes, now)?,
                    Event::Closed { code, reason } => {
                        return Err(if reason.is_empty() {
                            format!("Unable to connect to this lobby ({code}).")
                        } else {
                            reason
                        });
                    }
                }
            }
            if let Some(environment) = net.session.environment() {
                save_key(&net.session.code, &net.session.key);
                net.saved_key.clone_from(&net.session.key);
                net.environment = environment.to_string();
                // Arriving while others already ride: meet them instead of
                // the start line (up to 12 tries, 250 ms apart).
                if net.session.others().count() > 0 {
                    net.meet = Some((0., 12));
                }
                return Ok((net, environment.to_string()));
            }
            // A new room waits for its host to choose the course.
            if !set_up && net.session.peer.is_some() && net.session.is_host() {
                net.socket.send(net.session.start_room(venue)?);
                set_up = true;
            }
            net::sleep(20).await;
        }
    }

    pub fn code(&self) -> &str {
        &self.session.code
    }

    /// An invite link for the JavaScript game (buildRideUrl in web/launch.js,
    /// without the player's own name or ride).
    pub fn invite(&self) -> String {
        format!(
            "{}/play.html?mode=multiplayer&activity=freestyle&map={}&quad=1&rider=0&action=join&room={}&play=1",
            self.relay.trim_end_matches('/'),
            self.session.environment().unwrap_or("env01"),
            self.session.code
        )
    }

    /// Socket traffic, keep-alive pings, the player's pose at the 50 ms
    /// send tick (web/multiplayer.js timers), reconnects, arrivals and
    /// course changes.
    pub fn tick(&mut self, ride: &Ride, paused: bool) -> Tick {
        let mut tick = Tick::default();
        if self.closed.is_some() {
            return tick;
        }
        let now = now_ms(self.clock);
        let now_f = now as f64;
        if self.ready_sent && self.train_place_at.is_some_and(|at| now_f >= at) {
            self.train_place_at = None;
            tick.train_place = true;
        }
        if self.drop_at.is_some_and(|at| now_f >= at) {
            self.drop_at = None;
            self.drop_socket();
        }
        // A dropped socket retries its slot key; `at` is infinite while the
        // new socket connects.
        if let Some((at, until, attempts)) = self.reconnect {
            if now_f > until {
                self.reconnect = None;
                self.closed = Some("Connection lost.".into());
                return tick;
            }
            if at.is_finite() {
                if now_f < at {
                    return tick;
                }
                // A fresh socket and session with the slot key; the ride stays.
                let key = self.session.key.clone();
                let code = self.session.code.clone();
                self.session.restart();
                self.session.code = code.clone();
                self.session.key = key.clone();
                self.socket = net::Socket::connect(
                    &net::room_url(&self.relay, &code, Some(&key)),
                    &net::origin(&self.relay),
                );
                self.ready_sent = false;
                self.reconnect = Some((f64::INFINITY, until, attempts));
                self.reconnects += 1;
            }
        }
        for event in self.socket.poll() {
            match event {
                Event::Open => {
                    // Only a reconnect opens here; the first join is in `join`.
                    let code = self.session.code.clone();
                    if let Ok(bytes) = self.session.join(&code) {
                        self.socket.send(bytes);
                    }
                    if let Ok(bytes) = self.session.ping(now) {
                        self.socket.send(bytes);
                        self.last_ping = now_f;
                    }
                }
                Event::Message(bytes) => {
                    if let Err(error) = self.session.receive(&bytes, now) {
                        self.socket.close();
                        self.closed = Some(format!("Invalid session message: {error}"));
                        return tick;
                    }
                    if self.session.peer.is_some() {
                        self.reconnect = None;
                    }
                    // A new Welcome may carry a new slot key (a released slot
                    // rejoined as a new rider): keep the latest.
                    if !self.session.key.is_empty() && self.session.key != self.saved_key {
                        self.saved_key.clone_from(&self.session.key);
                        save_key(&self.session.code, &self.session.key);
                    }
                }
                Event::Closed { code, reason } => {
                    let key = !self.session.key.is_empty() && !self.session.code.is_empty();
                    let until = self.reconnect.map_or(now_f + 25_000., |r| r.1);
                    if key && retryable(code, &reason) && now_f < until {
                        let attempts = self.reconnect.map_or(0, |r| r.2);
                        let delay = (500. * 2f64.powi(attempts as i32)).min(3000.);
                        self.reconnect = Some((now_f + delay, until, attempts + 1));
                        crate::status("Reconnecting to the multiplayer room…");
                    } else {
                        self.closed = Some(if reason.is_empty() {
                            format!("Connection lost ({code}).")
                        } else {
                            reason
                        });
                    }
                    return tick;
                }
            }
        }
        // The host moved the room to another course: reload there.
        if let Some(environment) = self.session.environment()
            && environment != self.environment
        {
            tick.course = Some(environment.to_string());
            return tick;
        }
        if !self.ready_sent
            && self.session.peer.is_some()
            && let Ok(bytes) = self.session.ready()
        {
            self.ready_sent = self.socket.send(bytes);
            self.last_traffic = now_f;
            // A fresh arrival starts apart from nobody: contacts wait.
            self.session.ghost(now);
        }
        if let Some((at, tries)) = self.meet
            && self.ready_sent
            && now_f >= at
        {
            self.meet = (tries > 1).then_some((now_f + 250., tries - 1));
            let cars = |car| ride.train_car(car);
            let others = self.session.positions(now, &cars);
            for target in self.session.meet_candidates(now, now, &cars) {
                if let Ok(spot) = atv_game::multiplayer::spawn::beside(ride, &target, &others) {
                    let d = spot
                        .0
                        .iter()
                        .zip(target.atv.position)
                        .map(|(a, b)| (a - b) * (a - b))
                        .sum::<f32>()
                        .sqrt();
                    self.met = Some((target.peer, d));
                    tick.place = Some(spot);
                    self.meet = None;
                    self.session.ghost(now);
                    break;
                }
            }
        }
        if now_f - self.last_send >= 50. && self.ready_sent && tick.place.is_none() {
            self.last_send = now_f;
            match self.session.snapshot(ride, now, paused) {
                Ok(Some(bytes)) => {
                    if self.socket.send(bytes.clone()) {
                        let _ = self.session.sent(&bytes);
                        self.last_traffic = now_f;
                    }
                }
                Ok(None) => {}
                Err(error) => crate::status(&format!("Multiplayer pose: {error}")),
            }
        }
        // Pings keep the socket warm and refine the shared world clock.
        let synced = self.session.server_now(now).is_some() && self.session.rtt_ms.is_some();
        if (now_f - self.last_traffic >= 10_000.
            || now_f - self.last_ping >= 15_000.
            || (!synced && now_f - self.last_ping >= 2000.))
            && self.session.peer.is_some()
            && let Ok(bytes) = self.session.ping(now)
        {
            self.socket.send(bytes);
            self.last_ping = now_f;
            self.last_traffic = now_f;
        }
        for player in self.session.others() {
            let key = (player.atv_model, player.rider_model, player.appearance);
            if !self.models.contains_key(&key) && !tick.loads.contains(&key) {
                tick.loads.push(key);
            }
        }
        for key in &tick.loads {
            self.models.insert(*key, Loaded::Loading);
        }
        tick
    }

    /// Other riders' bodies for this frame's contact steps.
    pub fn contact_bodies(&self, ride: &Ride) -> Vec<atv_game::multiplayer::PeerBody> {
        self.session
            .contact_bodies(now_ms(self.clock), &|car| ride.train_car(car))
    }

    /// The relay clock as (local clock origin, offset ms) once synced, so the
    /// ride can derive each step's shared train phase.
    pub fn server_clock(&self) -> Option<(Instant, f64)> {
        let started = self.session.lobby.as_ref().is_some_and(|l| l.started);
        let offset = self.session.server_now(0)?;
        started.then_some((self.clock, offset))
    }

    /// After a course reload: report Ready again.
    pub fn loaded(&mut self, environment: &str) {
        self.environment = environment.to_string();
        self.ready_sent = false;
        self.locals.clear();
        // A course change starts everyone at the start line; only a fresh
        // arrival meets a rider (web/multiplayer.js `ride`).
        self.met = None;
        self.meet = None;
    }

    /// Simulates a dropped connection (tests: `?dropSocket` / reconnect gate).
    pub fn drop_socket(&mut self) {
        self.socket.drop_connection();
        let now = now_ms(self.clock) as f64;
        self.reconnect = Some((now + 500., now + 25_000., 1));
    }

    pub fn content(&self) -> Rc<Content> {
        self.content.clone()
    }

    pub fn model_loaded(&mut self, model: Result<RemoteModel, (Key, String)>) {
        match model {
            Ok(model) => {
                self.models.insert(model.key, Loaded::Ready(Rc::new(model)));
            }
            Err((key, error)) => {
                crate::status(&format!("Remote rider model unavailable: {error}"));
                self.models.insert(key, Loaded::Failed);
            }
        }
    }

    /// Uploads arrived models, then poses every presented remote rider.
    pub fn present(
        &mut self,
        gpu: &Gpu,
        scene: &mut SceneRenderer,
        venue: &atv_game::venue::Venue,
        ride: &Ride,
        eye: [f32; 3],
    ) {
        let own = ride.frame().origin;
        let now = now_ms(self.clock);
        let dt = self
            .last_frame
            .map_or(0.016, |last| ((now as f64 - last) * 0.001) as f32)
            .clamp(0., 0.1);
        self.last_frame = Some(now as f64);
        // Riders the roster lists, with their current model.
        let roster: Vec<(u8, Key)> = self
            .session
            .others()
            .map(|p| (p.id, (p.atv_model, p.rider_model, p.appearance)))
            .collect();
        scene.retain_peers(|peer, key| roster.contains(&(peer, key)));
        self.locals
            .retain(|peer, _| roster.iter().any(|(id, _)| id == peer));
        for &(peer, key) in &roster {
            if scene.peer_model(peer) != Some(key)
                && let Some(Loaded::Ready(model)) = self.models.get(&key)
                && let Err(error) = scene.set_peer(
                    gpu,
                    venue,
                    peer,
                    key,
                    (&model.quad, &model.rider),
                    model.paint,
                )
            {
                crate::status(&format!("Remote rider upload: {error}"));
            }
        }
        let mut poses: Vec<(u8, Pose)> = Vec::new();
        self.presented.clear();
        let cars = |car| ride.train_car(car);
        for (peer, sampled) in self.session.present(now, Some(own), &cars) {
            self.presented.push((
                peer,
                sampled.snapshot.atv.position,
                sampled.stale,
                sampled.snapshot.platform.as_ref().map(|p| p.car),
            ));
            // The roster lists the rider's look; the session only admits
            // snapshots whose models match it.
            let Some(&(_, key)) = roster.iter().find(|(id, _)| *id == peer) else {
                continue;
            };
            let Some(Loaded::Ready(model)) = self.models.get(&key) else {
                continue;
            };
            if self
                .locals
                .get(&peer)
                .is_none_or(|l| !l.key_matches(&model.rig))
            {
                match Local::new(model.rig.clone()) {
                    Ok(local) => {
                        self.locals.insert(peer, local);
                    }
                    Err(error) => {
                        crate::status(&format!("Remote rider rig: {error}"));
                        continue;
                    }
                }
            }
            let local = self.locals.get_mut(&peer).expect("remote rig");
            match local.pose(&sampled.snapshot, dt, sampled.stale) {
                Ok(pose) => poses.push((peer, pose)),
                Err(error) => crate::status(&format!("Remote rider pose: {error}")),
            }
        }
        self.drawn = scene.pose_peers(gpu, &poses, eye);
    }

    /// Status for tests and the diagnostics line (window.furyMultiplayer).
    pub fn report(&self) -> serde_json::Value {
        let c = self.session.counters;
        serde_json::json!({
            "code": self.session.code,
            "peer": self.session.peer,
            "host": self.session.is_host(),
            "environment": self.session.environment(),
            "players": self.session.lobby.as_ref().map(|l| l.players.len()),
            "drawn": self.drawn,
            "rttMs": self.session.rtt_ms,
            "clockSynced": self.session.server_now(0).is_some(),
            "closed": self.closed,
            "reconnects": self.reconnects,
            "reconnecting": self.reconnect.is_some(),
            "met": self.met.map(|(peer, d)| serde_json::json!({"peer": peer, "distanceFt": d})),
            "remote": self.presented.iter().map(|(peer, position, stale, platform)| {
                serde_json::json!({"peer": peer, "position": position, "stale": stale, "platform": platform})
            }).collect::<Vec<_>>(),
            "network": {
                "messages": c.messages, "full": c.full, "delta": c.delta,
                "suppressed": c.suppressed, "sentBytes": c.sent_bytes,
                "receivedBytes": c.received_bytes, "lastPacketBytes": c.last_packet_bytes,
                "protocol": atv_net_version(),
            },
        })
    }
}

fn atv_net_version() -> u8 {
    atv_game::multiplayer::PROTOCOL
}

/// Loads a remote rider's model (web: on the event loop; desktop: now).
pub async fn load_model(content: Rc<Content>, key: Key) -> Result<RemoteModel, (Key, String)> {
    remote::load_model(&content, key)
        .await
        .map_err(|e| (key, e))
}
