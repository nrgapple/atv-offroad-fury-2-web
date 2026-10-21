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
    pub name: String,
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
    presented: Vec<(u8, [f32; 3], bool)>,
}

fn now_ms(clock: Instant) -> u64 {
    clock.elapsed().as_millis() as u64
}

/// Creates (`host`) or opens (`join`, `open`) the room on the relay and
/// returns its code and, when this rider becomes its creator, the slot key.
async fn claim(start: &RoomStart) -> Result<(String, Option<String>), String> {
    let origin = net::origin(&start.relay);
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
        return Ok((code, None));
    }
    Ok((code, room["hostToken"].as_str().map(String::from)))
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

    /// Socket traffic, keep-alive pings and the player's pose at the 50 ms
    /// send tick (web/multiplayer.js timers). Returns models to load.
    pub fn tick(&mut self, ride: &Ride, paused: bool) -> Vec<Key> {
        if self.closed.is_some() {
            return Vec::new();
        }
        let now = now_ms(self.clock);
        let now_f = now as f64;
        for event in self.socket.poll() {
            match event {
                Event::Open => {}
                Event::Message(bytes) => {
                    if let Err(error) = self.session.receive(&bytes, now) {
                        self.socket.close();
                        self.closed = Some(format!("Invalid session message: {error}"));
                        return Vec::new();
                    }
                }
                Event::Closed { code, reason } => {
                    // Reconnecting with the slot key is a later slice.
                    self.closed = Some(if reason.is_empty() {
                        format!("Connection lost ({code}).")
                    } else {
                        reason
                    });
                    return Vec::new();
                }
            }
        }
        // The room moved to another course: the reload is a later slice.
        if !self.ready_sent
            && let Ok(bytes) = self.session.ready()
        {
            self.ready_sent = self.socket.send(bytes);
            self.last_traffic = now_f;
        }
        if now_f - self.last_send >= 50. && self.ready_sent {
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
            && let Ok(bytes) = self.session.ping(now)
        {
            self.socket.send(bytes);
            self.last_ping = now_f;
            self.last_traffic = now_f;
        }
        let mut wanted = Vec::new();
        for player in self.session.others() {
            let key = (player.atv_model, player.rider_model, player.appearance);
            if !self.models.contains_key(&key) && !wanted.contains(&key) {
                wanted.push(key);
            }
        }
        for key in &wanted {
            self.models.insert(*key, Loaded::Loading);
        }
        wanted
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
        own: [f32; 3],
        eye: [f32; 3],
    ) {
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
        for (peer, sampled) in self.session.present(now, Some(own)) {
            self.presented
                .push((peer, sampled.snapshot.atv.position, sampled.stale));
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
            "remote": self.presented.iter().map(|(peer, position, stale)| {
                serde_json::json!({"peer": peer, "position": position, "stale": stale})
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
