//! Authored Rust room service. Original PS2 networking is not executed here.
use std::{cell::RefCell, rc::Rc};

use atv_net::{
    Lobby, PRIVATE_CAPACITY, PUBLIC_CAPACITY, PUBLIC_MAP, PUBLIC_ROOMS, Packet, Participant,
    ROOM_CODE_BYTES, RoomCode,
};
use serde::{Deserialize, Serialize};
use worker::{wasm_bindgen::JsCast, *};

const ROOT: &str = "/multiplayer/rooms";
const FREERIDE: &str = "/multiplayer/freeride";
/// Public, read-only total of riders across shared Free Ride rooms.
const FREERIDE_RIDERS: &str = "/multiplayer/freeride/riders";
/// Durable Object name of the rider counter; never a valid room code.
const STATS: &str = "freeride-stats";
const STATS_KEY: &str = "riders";
/// A room's report counts until this old. Live rooms report at least every
/// 30 seconds from their alarm, so a vanished room drops out on its own.
const STATS_FRESH_MS: f64 = 90_000.;
const ROOM_KEY: &str = "room";
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const MAX_AGE_MS: f64 = 12. * 60. * 60. * 1000.;
const IDLE_MS: f64 = 60_000.;
/// A dropped rider keeps their slot (and color) this long for reconnection.
const GRACE_MS: f64 = 30_000.;
const CLEANUP_MS: i64 = 30_000;

/// A participant slot held for reconnection. The creator's slot 0 is held
/// (without a player yet) until its first connection arrives.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct Reserved {
    peer: u8,
    #[serde(default)]
    player: Option<Participant>,
    key: String,
    since_ms: f64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Room {
    #[serde(default)]
    instance: String,
    #[serde(default)]
    admission_lease: Option<String>,
    #[serde(default)]
    generation: u32,
    /// Current map authority. Passes to another rider when the host leaves.
    #[serde(default)]
    host: u8,
    #[serde(default)]
    reserved: Vec<Reserved>,
    #[serde(default)]
    empty_since_ms: Option<f64>,
    code: RoomCode,
    host_token: String,
    created_ms: f64,
    closed: bool,
    epoch: u32,
    map: u16,
    started: bool,
    /// Shared Arlington Free Ride room: fixed map, no host controls, more
    /// riders, and no age limit while anyone is riding.
    #[serde(default)]
    public: bool,
}

impl Room {
    fn capacity(&self) -> usize {
        if self.public {
            PUBLIC_CAPACITY
        } else {
            PRIVATE_CAPACITY
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Connection {
    #[serde(default)]
    instance: String,
    #[serde(default)]
    generation: u32,
    #[serde(default)]
    close_recoverable: Option<bool>,
    /// Rejoin secret for this slot; only its owner receives it.
    #[serde(default)]
    key: String,
    /// Map this rider has loaded. Poses are relayed only on the room's map.
    #[serde(default)]
    ready_map: u16,
    active: bool,
    peer: u8,
    player: Option<Participant>,
    sequence: Option<u32>,
    #[serde(default)]
    snapshot: Option<atv_net::Snapshot>,
    /// The client announced room races (a serial-0 `Progress`); only such
    /// clients receive race state, so older cached clients in the room
    /// never see a packet kind they cannot decode.
    #[serde(default)]
    races: bool,
    last_seen_ms: f64,
    window_ms: f64,
    messages: u16,
    bytes: usize,
}

/// Per-connection budget: 120 messages and 1 MiB a second on average, over
/// a 3 s window, so a client whose packets queued during a stall (a starved
/// tab, a congested link) and arrive together is not closed for the burst.
const RATE_WINDOW_MS: f64 = 3000.;
const RATE_MESSAGES: u16 = 360;
const RATE_BYTES: usize = 3 * 1024 * 1024;

impl Connection {
    fn account(&mut self, now: f64, bytes: usize) -> bool {
        if now - self.window_ms >= RATE_WINDOW_MS {
            self.window_ms = now;
            self.messages = 0;
            self.bytes = 0;
        }
        self.messages = self.messages.saturating_add(1);
        self.bytes = self.bytes.saturating_add(bytes);
        self.last_seen_ms = now;
        self.messages <= RATE_MESSAGES && self.bytes <= RATE_BYTES
    }
}

fn recoverable_close(code: usize, reason: &str) -> bool {
    matches!(code, 1001 | 1005 | 1006 | 1011 | 1012 | 1013)
        || (code == 1000 && reason == "Connection timed out")
}
fn room_reusable(room: &Room, instant: f64) -> bool {
    room.closed || (!room.public && instant - room.created_ms > MAX_AGE_MS)
}
/// Lowest slot that is neither connected nor held for a reconnecting rider.
fn free_slot(taken: impl Iterator<Item = u8> + Clone, capacity: usize) -> Option<u8> {
    (0..capacity as u8).find(|id| taken.clone().all(|t| t != *id))
}
/// Reservations older than the reconnect grace, oldest first.
fn expired(reserved: &[Reserved], instant: f64) -> Vec<Reserved> {
    reserved
        .iter()
        .filter(|r| instant - r.since_ms >= GRACE_MS)
        .cloned()
        .collect()
}
fn key_bytes(key: &str) -> [u8; 16] {
    let mut out = [0; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = key
            .get(i * 2..i * 2 + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .unwrap_or(0);
    }
    out
}

fn same_room(room: &Room, connection: &Connection) -> bool {
    room.generation == connection.generation && room.instance == connection.instance
}

fn code_from_path(path: &str) -> Option<RoomCode> {
    let code = path.strip_prefix(ROOT)?.strip_prefix('/')?;
    let code: RoomCode = code.as_bytes().try_into().ok()?;
    atv_net::valid_room_code(&code).then_some(code)
}

fn now() -> f64 {
    js_sys::Date::now()
}

fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let crypto: web_sys::Crypto =
        js_sys::Reflect::get(&js_sys::global(), &"crypto".into())?.dyn_into()?;
    let mut bytes = [0; N];
    crypto.get_random_values_with_u8_array(&mut bytes)?;
    Ok(bytes)
}

fn random_code() -> Result<RoomCode> {
    Ok(random_bytes::<ROOM_CODE_BYTES>()?.map(|byte| ALPHABET[usize::from(byte & 31)]))
}

fn random_token() -> Result<String> {
    Ok(random_bytes::<16>()?
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn origin_allowed(origin: Option<&str>, allowed: &str) -> bool {
    origin.is_some_and(|origin| {
        !origin.is_empty()
            && allowed
                .split(',')
                .map(str::trim)
                .any(|candidate| !candidate.is_empty() && candidate == origin)
    })
}

fn same_origin(req: &Request, env: &Env) -> Result<bool> {
    let allowed = env
        .var("GAME_ORIGINS")
        .or_else(|_| env.var("GAME_ORIGIN"))?
        .to_string();
    Ok(origin_allowed(
        req.headers().get("Origin")?.as_deref(),
        &allowed,
    ))
}

/// A new room whose first slot is held for its creator (or first claimant).
fn new_room(
    code: RoomCode,
    key: String,
    admission_lease: Option<String>,
    public: bool,
) -> Result<Room> {
    Ok(Room {
        instance: random_token()?,
        admission_lease,
        generation: 0,
        host: 0,
        reserved: vec![Reserved {
            peer: 0,
            player: None,
            key: key.clone(),
            since_ms: now(),
        }],
        empty_since_ms: None,
        code,
        host_token: key,
        created_ms: now(),
        closed: false,
        epoch: 1,
        // Shared rooms are riding Arlington from the start.
        map: if public { PUBLIC_MAP } else { 1 },
        started: public,
        public,
    })
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct RoomRiders {
    riders: usize,
    at_ms: f64,
}
/// Riders in rooms that reported recently.
fn fresh_riders(rooms: &std::collections::BTreeMap<String, RoomRiders>, instant: f64) -> usize {
    rooms
        .values()
        .filter(|r| instant - r.at_ms < STATS_FRESH_MS)
        .map(|r| r.riders)
        .sum()
}

fn send(ws: &WebSocket, packet: &Packet) -> Result<()> {
    let bytes = atv_net::encode(packet).map_err(|e| Error::RustError(e.to_string()))?;
    ws.send_with_bytes(bytes)
}

#[event(fetch)]
pub async fn main(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // The main menu shows how many people are in Free Ride. Same-origin GETs
    // carry no Origin header, and this number is public, so it is answered
    // before the origin check; the Worker caches it briefly at the edge.
    if req.path() == FREERIDE_RIDERS && req.method() == Method::Get {
        let mut total = env
            .durable_object("LOBBIES")?
            .id_from_name(STATS)?
            .get_stub()?
            .fetch_with_request(Request::new("https://counter/stats/total", Method::Get)?)
            .await?;
        let value: serde_json::Value = total.json().await?;
        let mut response = Response::from_json(&value)?;
        response
            .headers_mut()
            .set("Cache-Control", "public, max-age=10")?;
        return Ok(response);
    }
    if !same_origin(&req, &env)? {
        return Response::error("Origin denied", 403);
    }
    if req.path() == "/multiplayer/version" && req.method() == Method::Get {
        return Response::from_json(&serde_json::json!({
            "commit": env.var("BUILD_COMMIT").map(|value| value.to_string()).unwrap_or_else(|_| "host-only".into()),
            "wasmSha256": env.var("BUILD_WASM_SHA").map(|value| value.to_string()).unwrap_or_else(|_| "host-only".into()),
            "protocol": atv_net::VERSION,
        }));
    }
    let namespace = env.durable_object("LOBBIES")?;
    if req.path() == ROOT && req.method() == Method::Post {
        let body = req.text().await?;
        if body.len() > 512 {
            return Response::error("Lobby request too large", 413);
        }
        let admission_lease = req.headers().get("X-ATV-Admission-Lease")?;
        if env
            .var("MULTIPLAYER_GUARDS")
            .is_ok_and(|v| v.to_string() == "required")
            && admission_lease.is_none()
        {
            return Response::error("Admission required", 503);
        }
        let mut open = false;
        let requested = if body.is_empty() {
            None
        } else {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
                return Response::error("Invalid lobby request", 400);
            };
            // Invite links "open" their code: join a live room, or start it again.
            open = value.get("open").and_then(|v| v.as_bool()) == Some(true);
            match value.get("code").and_then(|v| v.as_str()) {
                None | Some("") => None,
                Some(code) => match atv_net::parse_room_code(code) {
                    Ok(code) if atv_net::is_public_room_code(&code) => {
                        return Response::error("That code is reserved", 400);
                    }
                    Ok(code) => Some(code),
                    Err(_) => return Response::error("Use 8 letters or numbers", 400),
                },
            }
        };
        // Name lookup is stable, but guessed codes must never create joinable rooms.
        // Initialization goes only through this creator route, with collision retry.
        for _ in 0..4 {
            let code = match requested {
                Some(code) => code,
                None => random_code()?,
            };
            let text = std::str::from_utf8(&code).expect("ASCII room code");
            let stub = namespace.id_from_name(text)?.get_stub()?;
            let token = random_token()?;
            let mut init = RequestInit::new();
            init.method = Method::Post;
            init.body = Some(
                serde_json::to_string(&new_room(
                    code,
                    token.clone(),
                    admission_lease.clone(),
                    false,
                )?)?
                .into(),
            );
            let response = stub
                .fetch_with_request(Request::new_with_init("https://room/create", &init)?)
                .await?;
            if response.status_code() == 201 {
                return Ok(Response::from_json(&serde_json::json!({
                    "code": text, "hostToken": token
                }))?
                .with_status(201));
            }
            if response.status_code() == 409 && requested.is_some() {
                if open {
                    let mut response = response;
                    let live: serde_json::Value = response.json().await.unwrap_or_default();
                    return Response::from_json(&serde_json::json!({
                        "code": text, "open": true,
                        "players": live.get("players"), "max": PRIVATE_CAPACITY,
                    }));
                }
                return Response::error("Lobby code is taken", 409);
            }
            if response.status_code() != 409 {
                return Response::error("Room creation failed", 503);
            }
        }
        return Response::error("Room creation unavailable", 503);
    }
    // Arlington Free Ride: reserve a slot in the lowest-numbered shared room
    // with space, so riders gather together; a full room opens the next one.
    // Each room reserves atomically, so racing arrivals cannot overfill it.
    if req.path() == FREERIDE && req.method() == Method::Post {
        for number in 1..=PUBLIC_ROOMS {
            let code = atv_net::public_room_code(number).expect("public room number");
            let text = std::str::from_utf8(&code).expect("ASCII room code");
            let mut init = RequestInit::new();
            init.method = Method::Post;
            init.body =
                Some(serde_json::to_string(&new_room(code, random_token()?, None, true)?)?.into());
            let mut response = namespace
                .id_from_name(text)?
                .get_stub()?
                .fetch_with_request(Request::new_with_init("https://room/claim", &init)?)
                .await?;
            match response.status_code() {
                200 | 201 => {
                    let claim: serde_json::Value = response.json().await?;
                    return Response::from_json(&serde_json::json!({
                        "code": text, "key": claim.get("key"),
                    }));
                }
                409 => continue,
                _ => return Response::error("Free Ride unavailable", 503),
            }
        }
        return Response::error("Free Ride is full. Try again shortly.", 503);
    }
    if req.method() == Method::Get
        && let Some(code) = code_from_path(&req.path())
    {
        let text = std::str::from_utf8(&code).expect("ASCII room code");
        return namespace
            .id_from_name(text)?
            .get_stub()?
            .fetch_with_request(req)
            .await;
    }
    Response::error("Unknown multiplayer route", 404)
}

#[durable_object]
pub struct LobbyRoom {
    state: State,
    env: Env,
    room: Rc<RefCell<Option<Room>>>,
    /// Joined, active sockets, kept in memory so a full room's ~400 poses a
    /// second do not each deserialize every rider's attachment. Cleared
    /// whenever a rider joins or leaves; a woken (hibernated) object rebuilds it.
    recipients: RefCell<Option<Vec<WebSocket>>>,
}

impl LobbyRoom {
    async fn release_lease(&self, room: &Room) -> Result<()> {
        let Some(lease) = &room.admission_lease else {
            return Ok(());
        };
        let mut init = RequestInit::new();
        init.method = Method::Post;
        init.body = Some(serde_json::json!({"lease": lease}).to_string().into());
        let released = self
            .env
            .durable_object("ADMISSION")?
            .id_from_name("global-budget-v1")?
            .get_stub()?
            .fetch_with_request(Request::new_with_init("https://admission/release", &init)?)
            .await?;
        if released.status_code() != 200 {
            return Err(Error::RustError("Room retirement unavailable".into()));
        }
        Ok(())
    }
    fn connections(&self) -> Result<Vec<(WebSocket, Connection)>> {
        let room = self.room.borrow().clone();
        self.state
            .get_websockets()
            .into_iter()
            .map(|ws| {
                let connection = ws
                    .deserialize_attachment::<Connection>()?
                    .ok_or_else(|| Error::RustError("Missing connection attachment".into()))?;
                Ok((ws, connection))
            })
            .collect::<Result<Vec<_>>>()
            .map(|all| {
                all.into_iter()
                    .filter(|(_, c)| c.active && room.as_ref().is_some_and(|r| same_room(r, c)))
                    .collect()
            })
    }

    /// Connected riders plus riders whose slots are held for reconnection.
    fn lobby(&self, except: Option<&WebSocket>) -> Result<Lobby> {
        let room = self.room.borrow();
        let room = room.as_ref().ok_or("Room does not exist")?;
        let mut players = self
            .connections()?
            .into_iter()
            .filter(|(ws, _)| except != Some(ws))
            .filter_map(|(_, connection)| connection.player)
            .collect::<Vec<_>>();
        for held in &room.reserved {
            if let Some(player) = &held.player
                && !players.iter().any(|p| p.id == held.peer)
            {
                players.push(player.clone());
            }
        }
        players.sort_by_key(|player| player.id);
        let host = if players.iter().any(|p| p.id == room.host) {
            room.host
        } else {
            players.first().map_or(room.host, |p| p.id)
        };
        Ok(Lobby {
            host,
            epoch: room.epoch,
            map: room.map,
            started: room.started,
            public: room.public,
            players,
        })
    }

    async fn save(&self, room: Room) -> Result<()> {
        *self.room.borrow_mut() = Some(room.clone());
        self.state.storage().put(ROOM_KEY, room).await
    }

    /// Pass map authority to the lowest connected rider, else a held slot.
    fn promote(&self, room: &mut Room, except: Option<&WebSocket>) -> Result<()> {
        let connected = self
            .connections()?
            .into_iter()
            .filter(|(ws, c)| except != Some(ws) && c.player.is_some())
            .map(|(_, c)| c.peer)
            .min();
        if let Some(peer) = connected.or_else(|| {
            room.reserved
                .iter()
                .filter(|r| r.player.is_some())
                .map(|r| r.peer)
                .min()
        }) {
            room.host = peer;
        }
        Ok(())
    }

    /// Roster without (and not sent to) `except`, when anyone remains.
    fn broadcast_lobby(&self, except: Option<&WebSocket>) -> Result<()> {
        let lobby = self.lobby(except)?;
        if !lobby.players.is_empty() {
            self.broadcast(&Packet::Lobby(lobby), except)?;
        }
        Ok(())
    }

    /// Lifecycle-only diagnostics: no names, credentials, packet bodies or URLs.
    fn record(
        &self,
        event: &str,
        ws: Option<&WebSocket>,
        reason: &str,
        code: Option<usize>,
        clean: Option<bool>,
    ) {
        let room = self.room.borrow();
        let connection =
            ws.and_then(|socket| socket.deserialize_attachment::<Connection>().ok().flatten());
        console_log!(
            "{}",
            serde_json::json!({
                "service": "atv-multiplayer", "event": event,
                "commit": self.env.var("BUILD_COMMIT").map(|v|v.to_string()).unwrap_or_else(|_|"host-only".into()),
                "room": room.as_ref().map(|r| String::from_utf8_lossy(&r.code).into_owned()),
                "started": room.as_ref().map(|r| r.started), "time_ms": now(),
                "peer": connection.as_ref().map(|c| c.peer), "code": code, "clean": clean,
                "reason": reason.chars().take(120).collect::<String>(),
                "idle_ms": connection.as_ref().map(|c| (now()-c.last_seen_ms).max(0.)),
                "window_messages": connection.as_ref().map(|c| c.messages),
                "window_bytes": connection.as_ref().map(|c| c.bytes),
                "last_sequence": connection.as_ref().and_then(|c| c.sequence)
            })
        );
    }
    fn close_socket(&self, ws: &WebSocket, code: u16, reason: &str) -> Result<()> {
        // Cloudflare can report 1006 after our own policy close. Persist the
        // original decision so that callback cannot turn it into recovery.
        if let Some(mut connection) = ws.deserialize_attachment::<Connection>()? {
            connection
                .close_recoverable
                .get_or_insert(recoverable_close(code as usize, reason));
            ws.serialize_attachment(connection)?;
        }
        self.record(
            "socket_close_requested",
            Some(ws),
            reason,
            Some(code as usize),
            None,
        );
        ws.close(Some(code), Some(reason))
    }

    fn invalidate_recipients(&self) {
        *self.recipients.borrow_mut() = None;
    }

    fn broadcast(&self, packet: &Packet, except: Option<&WebSocket>) -> Result<()> {
        let bytes = atv_net::encode(packet).map_err(|e| Error::RustError(e.to_string()))?;
        let cached = self.recipients.borrow().clone();
        let recipients = match cached {
            Some(recipients) => recipients,
            None => {
                let recipients = self
                    .connections()?
                    .into_iter()
                    .filter(|(_, connection)| connection.player.is_some())
                    .map(|(ws, _)| ws)
                    .collect::<Vec<_>>();
                *self.recipients.borrow_mut() = Some(recipients.clone());
                recipients
            }
        };
        for ws in recipients {
            if except != Some(&ws) {
                // A failed peer send must not turn another rider's valid message into an error.
                if ws.send_with_bytes(&bytes).is_err() {
                    let _ = self.close_socket(&ws, 1011, "Send failed");
                }
            }
        }
        Ok(())
    }

    async fn finish(&self, reason: &str) -> Result<()> {
        if self.room.borrow().as_ref().is_none_or(|room| room.closed) {
            return Ok(());
        }
        self.record("room_closed", None, reason, None, None);
        self.report_riders(Some(0)).await;
        let saved = {
            let mut room = self.room.borrow_mut();
            if let Some(room) = room.as_mut() {
                room.closed = true;
            }
            room.clone()
        };
        if let Some(room) = saved {
            self.state.storage().put(ROOM_KEY, &room).await?;
        }

        for ws in self.state.get_websockets() {
            if let Some(mut connection) = ws.deserialize_attachment::<Connection>()? {
                connection.active = false;
                ws.serialize_attachment(connection)?;
            }
            let _ = ws.close(Some(1000), Some(reason));
        }
        self.invalidate_recipients();
        self.state.storage().set_alarm(CLEANUP_MS).await?;
        let closed = self.room.borrow().clone();
        if let Some(room) = closed {
            // Retirement failure must not leave peers running; the alarm retries.
            let _ = self.release_lease(&room).await;
        }
        Ok(())
    }

    /// Install a new (or reused) room record; its creator holds slot 0.
    async fn install(&self, mut room: Room, claim: bool) -> Result<Response> {
        let previous = self.room.borrow().clone();
        room.generation = previous
            .as_ref()
            .map_or(1, |r| r.generation.wrapping_add(1));
        // Old close events must never terminate a newly reused code.
        for socket in self.state.get_websockets() {
            let _ = socket.close(Some(1000), Some("Room replaced"));
        }
        let key = room.host_token.clone();
        *self.room.borrow_mut() = Some(room.clone());
        self.invalidate_recipients();
        self.state.storage().put(ROOM_KEY, &room).await?;
        self.record("room_created", None, "", None, None);
        self.state.storage().set_alarm(30_000_i64).await?;
        if let Some(old) = previous.as_ref().filter(|r| r.closed) {
            // Install the replacement before any external admission await.
            // A failed retirement remains bounded by the lease expiry.
            let _ = self.release_lease(old).await;
        }
        if claim {
            return Ok(Response::from_json(&serde_json::json!({ "key": key }))?.with_status(201));
        }
        Ok(Response::empty()?.with_status(201))
    }

    /// Tell the rider counter how many riders a shared room holds. Counting
    /// is best effort and never affects the room itself. Rooms report from
    /// their alarm (within 5 seconds of a claim, else every 30 seconds), never
    /// from the rider message path.
    async fn report_riders(&self, riders: Option<usize>) {
        let Some(room) = self.room.borrow().clone().filter(|room| room.public) else {
            return;
        };
        let riders = riders.unwrap_or_else(|| self.lobby(None).map_or(0, |l| l.players.len()));
        let report = async {
            let mut init = RequestInit::new();
            init.method = Method::Post;
            init.body = Some(
                serde_json::json!({"room": String::from_utf8_lossy(&room.code), "riders": riders})
                    .to_string()
                    .into(),
            );
            self.env
                .durable_object("LOBBIES")?
                .id_from_name(STATS)?
                .get_stub()?
                .fetch_with_request(Request::new_with_init(
                    "https://counter/stats/report",
                    &init,
                )?)
                .await
        };
        let _ = report.await;
    }

    async fn departed(&self, ws: &WebSocket, recoverable: bool) -> Result<()> {
        let Some(mut connection) = ws.deserialize_attachment::<Connection>()? else {
            return Ok(());
        };
        if !connection.active {
            return Ok(());
        }
        connection.active = false;
        ws.serialize_attachment(&connection)?;
        self.invalidate_recipients();
        let Some(mut room) = self.room.borrow().clone() else {
            return Ok(());
        };
        if room.closed || !same_room(&room, &connection) {
            return Ok(());
        }
        let held = recoverable && connection.player.is_some();
        room.reserved.retain(|r| r.peer != connection.peer);
        if held {
            // Keep the rider's slot, color and roster entry while it reconnects.
            room.reserved.push(Reserved {
                peer: connection.peer,
                player: connection.player.clone(),
                key: connection.key.clone(),
                since_ms: now(),
            });
            self.record(
                "peer_reconnect_grace",
                Some(ws),
                "Holding slot for 30 seconds",
                None,
                None,
            );
            self.state.storage().set_alarm(5_000_i64).await?;
        } else {
            self.broadcast(
                &Packet::Leave {
                    peer: connection.peer,
                },
                Some(ws),
            )?;
            if room.host == connection.peer {
                self.promote(&mut room, Some(ws))?;
            }
        }
        if self.connections()?.iter().all(|(other, _)| other == ws) {
            room.empty_since_ms.get_or_insert(now());
        }
        room.epoch = room.epoch.wrapping_add(1);
        self.save(room).await?;
        self.broadcast_lobby(Some(ws))
    }
}

impl DurableObject for LobbyRoom {
    fn new(state: State, env: Env) -> Self {
        let room = Rc::new(RefCell::new(None));
        let cached = room.clone();
        let storage = state.storage();
        let _initialization = state.block_concurrency_while(async move {
            *cached.borrow_mut() = storage.get::<Room>(ROOM_KEY).await?;
            Ok(())
        });
        Self {
            state,
            env,
            room,
            recipients: RefCell::new(None),
        }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        // Rider counter instance (named STATS): shared rooms report here.
        if req.path() == "/stats/report" && req.method() == Method::Post {
            let value: serde_json::Value = req.json().await?;
            let (Some(room), Some(riders)) = (
                value["room"]
                    .as_str()
                    .filter(|room| room.len() <= ROOM_CODE_BYTES),
                value["riders"]
                    .as_u64()
                    .filter(|n| *n as usize <= PUBLIC_CAPACITY),
            ) else {
                return Response::error("Invalid rider report", 400);
            };
            let storage = self.state.storage();
            let instant = now();
            let mut rooms = storage
                .get::<std::collections::BTreeMap<String, RoomRiders>>(STATS_KEY)
                .await?
                .unwrap_or_default();
            rooms.retain(|_, r| instant - r.at_ms < STATS_FRESH_MS);
            rooms.insert(
                room.to_string(),
                RoomRiders {
                    riders: riders as usize,
                    at_ms: instant,
                },
            );
            storage.put(STATS_KEY, &rooms).await?;
            return Ok(Response::empty()?.with_status(204));
        }
        if req.path() == "/stats/total" && req.method() == Method::Get {
            let rooms = self
                .state
                .storage()
                .get::<std::collections::BTreeMap<String, RoomRiders>>(STATS_KEY)
                .await?
                .unwrap_or_default();
            return Response::from_json(
                &serde_json::json!({"riders": fresh_riders(&rooms, now())}),
            );
        }
        if req.path() == "/claim" && req.method() == Method::Post {
            let fresh: Room = req.json().await?;
            let previous = self.room.borrow().clone();
            let Some(mut room) = previous.filter(|room| !room_reusable(room, now())) else {
                // Nobody is here: open the shared room with this claimant in slot 0.
                return self.install(fresh, true).await;
            };
            if !room.public {
                return Response::error("Not a shared room", 409);
            }
            let taken = self
                .connections()?
                .iter()
                .map(|(_, c)| c.peer)
                .chain(room.reserved.iter().map(|r| r.peer))
                .collect::<Vec<_>>();
            let Some(peer) = free_slot(taken.iter().copied(), room.capacity()) else {
                return Response::error("Game is full!", 409);
            };
            let key = random_token()?;
            room.reserved.push(Reserved {
                peer,
                player: None,
                key: key.clone(),
                since_ms: now(),
            });
            room.empty_since_ms = None;
            self.save(room).await?;
            // Unused claims expire with the reconnect grace.
            self.state.storage().set_alarm(5_000_i64).await?;
            return Response::from_json(&serde_json::json!({ "key": key }));
        }
        if req.path() == "/create" && req.method() == Method::Post {
            let room: Room = req.json().await?;
            let previous = self.room.borrow().clone();
            if previous
                .as_ref()
                .is_some_and(|room| !room_reusable(room, now()))
            {
                self.record("connection_rejected", None, "Room exists", Some(409), None);
                let players = self.lobby(None).map_or(0, |l| l.players.len());
                return Ok(
                    Response::from_json(&serde_json::json!({"players": players}))?.with_status(409),
                );
            }
            return self.install(room, false).await;
        }
        if !same_origin(&req, &self.env)? {
            return Response::error("Origin denied", 403);
        }
        let Some(code) = code_from_path(&req.path()) else {
            return Response::error("Unknown room", 404);
        };
        let Some(room) = self.room.borrow().clone() else {
            return Response::error("Room does not exist", 404);
        };
        if room.code != code || room_reusable(&room, now()) {
            if room.closed {
                // Also retire legacy closed records, which had no cleanup alarm.
                self.state.storage().set_alarm(CLEANUP_MS).await?;
            }
            self.record("connection_rejected", None, "Room ended", Some(410), None);
            return Response::error("Room ended", 410);
        }
        if req
            .headers()
            .get("Upgrade")?
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case("websocket"))
            != Some(true)
        {
            return Response::error("WebSocket required", 426);
        }
        let key = req
            .url()?
            .query_pairs()
            .find(|(k, _)| k == "key" || k == "host")
            .map(|(_, v)| v.into_owned())
            .filter(|key| key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit()));
        let mut room = room;
        let mut connections = self.connections()?;
        let mut reclaimed = None;
        if let Some(key) = &key {
            // A rider's key replaces its own broken connection, even if the
            // old transport has not delivered a close callback yet.
            for (socket, connection) in &mut connections {
                if &connection.key == key {
                    reclaimed = Some(connection.peer);
                    connection.active = false;
                    socket.serialize_attachment(&connection)?;
                    self.invalidate_recipients();
                    let _ = self.close_socket(socket, 1000, "Connection replaced");
                }
            }
            connections.retain(|(_, c)| c.active);
            if let Some(index) = room.reserved.iter().position(|r| &r.key == key) {
                reclaimed.get_or_insert(room.reserved.remove(index).peer);
            }
        }
        let peer = match reclaimed {
            Some(peer) => peer,
            None => {
                let taken = connections
                    .iter()
                    .map(|(_, c)| c.peer)
                    .chain(room.reserved.iter().map(|r| r.peer))
                    .collect::<Vec<_>>();
                let Some(peer) = free_slot(taken.iter().copied(), room.capacity()) else {
                    self.record("connection_rejected", None, "Room is full", Some(409), None);
                    return Response::error("Game is full!", 409);
                };
                peer
            }
        };
        let key = match (reclaimed, key) {
            (Some(_), Some(key)) => key,
            _ => random_token()?,
        };
        room.empty_since_ms = None;
        self.save(room.clone()).await?;
        let pair = WebSocketPair::new()?;
        let instant = now();
        pair.server.serialize_attachment(Connection {
            instance: room.instance.clone(),
            generation: room.generation,
            close_recoverable: None,
            key,
            ready_map: 0,
            active: true,
            peer,
            player: None,
            sequence: None,
            snapshot: None,
            races: false,
            last_seen_ms: instant,
            window_ms: instant,
            messages: 0,
            bytes: 0,
        })?;
        self.state.accept_web_socket(&pair.server);
        Response::from_websocket(pair.client)
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let Some(room) = self.room.borrow().clone().filter(|r| !r.closed) else {
            return self.close_socket(&ws, 1000, "Room ended");
        };
        let WebSocketIncomingMessage::Binary(bytes) = message else {
            return self.close_socket(&ws, 1003, "Binary protocol required");
        };
        let Some(mut connection) = ws.deserialize_attachment::<Connection>()? else {
            return self.close_socket(&ws, 1008, "Connection missing");
        };
        if !same_room(&room, &connection) {
            return self.close_socket(&ws, 1000, "Room replaced");
        }
        if !connection.active {
            return self.close_socket(&ws, 1000, "Disconnected");
        }
        if !connection.account(now(), bytes.len()) {
            return self.close_socket(&ws, 1008, "Message rate exceeded");
        }
        let packet = match atv_net::decode(&bytes) {
            Ok(packet) => packet,
            Err(atv_net::Error::UnsupportedVersion) => {
                return self.close_socket(
                    &ws,
                    1008,
                    "Game updated. Reload the page to join multiplayer.",
                );
            }
            Err(_) => return self.close_socket(&ws, 1008, "Invalid multiplayer packet"),
        };
        let packet = match packet {
            Packet::Delta(mut d) => {
                d.snapshot.peer = connection.peer;
                // A delta can race a map/vehicle change. Wait for the sender's
                // next full snapshot rather than ending a healthy session.
                let Some(base) = connection.snapshot.as_ref() else {
                    ws.serialize_attachment(connection)?;
                    return Ok(());
                };
                match atv_net::apply_delta(&d, base) {
                    Ok(s) => Packet::Snapshot((s).into()),
                    Err(_) => return self.close_socket(&ws, 1008, "Invalid snapshot baseline"),
                }
            }
            p => p,
        };
        match packet {
            Packet::Join { code, mut player }
                if connection.player.is_none() && code == room.code =>
            {
                if !(1..=26).contains(&player.atv_model) || player.rider_model > 1 {
                    return self.close_socket(&ws, 1008, "Unknown actor model");
                }
                player.id = connection.peer;
                if room.public {
                    // Strangers see this name: apply the shared-room filter.
                    player.name = atv_net::public_name(&player.name, player.id);
                }
                connection.player = Some(player);
                ws.serialize_attachment(&connection)?;
                self.invalidate_recipients();
                let mut room = room;
                let present =
                    self.connections()?.iter().any(|(other, c)| {
                        other != &ws && c.player.is_some() && c.peer == room.host
                    }) || room
                        .reserved
                        .iter()
                        .any(|r| r.player.is_some() && r.peer == room.host);
                if !present {
                    room.host = connection.peer;
                }
                room.epoch = room.epoch.wrapping_add(1);
                self.save(room.clone()).await?;
                // A reclaimed or reused slot starts a new pose sequence; clear
                // observers' old buffers before its new snapshots arrive.
                self.broadcast(
                    &Packet::Leave {
                        peer: connection.peer,
                    },
                    Some(&ws),
                )?;
                self.record("peer_joined", Some(&ws), "", None, None);
                send(
                    &ws,
                    &Packet::Welcome {
                        peer: connection.peer,
                        code,
                        lobby: self.lobby(None)?,
                        key: key_bytes(&connection.key),
                    },
                )?;
                // Everyone else learns the full roster, including the arrival.
                self.broadcast(&Packet::Lobby(self.lobby(None)?), Some(&ws))?;
                return Ok(());
            }
            Packet::Snapshot(mut snapshot) if connection.player.is_some() => {
                let player = connection
                    .player
                    .as_ref()
                    .expect("matched connected player");
                let current = snapshot.atv_model == player.atv_model
                    && snapshot.rider_model == player.rider_model;
                if connection
                    .sequence
                    .is_none_or(|old| atv_net::sequence_newer(snapshot.sequence, old))
                {
                    connection.sequence = Some(snapshot.sequence);
                    snapshot.peer = connection.peer;
                    // The sender's delta baseline advances on every send, so the
                    // relay keeps it even when this pose is not forwarded.
                    // Persist only bounded attachment state; no SQL writes per pose.
                    connection.snapshot = Some(snapshot.as_ref().clone());
                    // Poses from a previous map or vehicle are never relayed.
                    if room.started && connection.ready_map == room.map && current {
                        self.broadcast(&Packet::Snapshot(snapshot), Some(&ws))?;
                    }
                }
            }
            Packet::Ready { map } if connection.player.is_some() => {
                if map == room.map {
                    connection.ready_map = map;
                    // Give an arriving rider everyone's current state, even if
                    // all peers are momentarily idle or suspended.
                    for (peer, other) in self.connections()? {
                        if peer != ws
                            && other.ready_map == room.map
                            && let (Some(s), Some(p)) = (other.snapshot, other.player)
                            && s.atv_model == p.atv_model
                            && s.rider_model == p.rider_model
                        {
                            send(&ws, &Packet::Snapshot(s.into()))?;
                        }
                    }
                }
            }
            Packet::Profile(mut player) if connection.player.is_some() => {
                if !(1..=26).contains(&player.atv_model) || player.rider_model > 1 {
                    return self.close_socket(&ws, 1008, "Unknown actor model");
                }
                player.id = connection.peer;
                if room.public {
                    // Strangers see this name: apply the shared-room filter.
                    player.name = atv_net::public_name(&player.name, player.id);
                }
                connection.player = Some(player);
                // The rider reloads with its new vehicle and reports Ready again.
                connection.ready_map = 0;
                connection.snapshot = None;
                ws.serialize_attachment(&connection)?;
                let mut room = room;
                room.epoch = room.epoch.wrapping_add(1);
                self.save(room).await?;
                self.broadcast_lobby(None)?;
                return Ok(());
            }
            Packet::Ping { nonce } if connection.player.is_some() => send(
                &ws,
                &Packet::Pong {
                    nonce,
                    server_ms: now() as u64,
                },
            )?,
            Packet::SetLobby {
                epoch,
                map,
                started,
            } if connection.player.is_some()
                && !room.public
                && self.lobby(None)?.host == connection.peer =>
            {
                if !atv_net::valid_map(map) {
                    return self.close_socket(&ws, 1008, "Unknown environment");
                }
                if epoch != room.epoch || (room.started && !started) {
                    send(&ws, &Packet::Lobby(self.lobby(None)?))?;
                    ws.serialize_attachment(connection)?;
                    return Ok(());
                }
                let mut next = room;
                let moved = next.map != map;
                next.map = map;
                next.started = started;
                next.epoch = next.epoch.wrapping_add(1);
                self.save(next).await?;
                self.record(
                    if moved {
                        "map_changed"
                    } else if started {
                        "lobby_started"
                    } else {
                        "lobby_updated"
                    },
                    Some(&ws),
                    "",
                    None,
                    None,
                );
                self.broadcast(&Packet::Lobby(self.lobby(None)?), None)?;
            }
            // A room mini-game's state (research/GAMES.md "Online"): only
            // the host of a started private room runs it; riders on the map
            // receive it.
            Packet::Game(state)
                if connection.player.is_some()
                    && !room.public
                    && room.started
                    && connection.ready_map == room.map
                    && self.lobby(None)?.host == connection.peer =>
            {
                let race = state.game == 3;
                for (peer, other) in self.connections()? {
                    if peer != ws && other.ready_map == room.map && (!race || other.races) {
                        send(&peer, &Packet::Game(state.clone()))?;
                    }
                }
            }
            // A rider's room-race progress (research/MULTIPLAYER.md "Room
            // races"): serial 0 only announces support; the relay stamps the
            // sender and forwards progress to race-aware riders on the map.
            Packet::Progress(mut progress) if connection.player.is_some() => {
                connection.races = true;
                if progress.serial != 0
                    && !room.public
                    && room.started
                    && connection.ready_map == room.map
                {
                    progress.peer = connection.peer;
                    for (peer, other) in self.connections()? {
                        if peer != ws && other.ready_map == room.map && other.races {
                            send(&peer, &Packet::Progress(progress))?;
                        }
                    }
                }
            }
            Packet::Leave { .. } => return self.close_socket(&ws, 1000, "Left room"),
            // A former host's map command can race a handoff; resynchronize it.
            Packet::SetLobby { .. } if connection.player.is_some() => {
                send(&ws, &Packet::Lobby(self.lobby(None)?))?
            }
            _ => return self.close_socket(&ws, 1008, "Message not permitted"),
        }
        ws.serialize_attachment(connection)?;
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        code: usize,
        reason: String,
        clean: bool,
    ) -> Result<()> {
        self.record("socket_closed", Some(&ws), &reason, Some(code), Some(clean));
        let recoverable = ws
            .deserialize_attachment::<Connection>()?
            .and_then(|c| c.close_recoverable)
            .unwrap_or_else(|| recoverable_close(code, &reason));
        let _ = if recoverable {
            ws.close(Some(1012), Some("Connection interrupted"))
        } else {
            ws.close(Some(1000), Some("Disconnected"))
        };
        self.departed(&ws, recoverable).await
    }

    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        let _ = self.close_socket(&ws, 1011, "Socket error");
        let recoverable = ws
            .deserialize_attachment::<Connection>()?
            .and_then(|c| c.close_recoverable)
            .unwrap_or(true);
        self.departed(&ws, recoverable).await
    }

    async fn alarm(&self) -> Result<Response> {
        let Some(room) = self.room.borrow().clone() else {
            return Response::empty();
        };
        if room.closed {
            // Keep retrying retirement even if admission has a prolonged outage.
            self.state.storage().set_alarm(CLEANUP_MS).await?;
            self.release_lease(&room).await?;
            let storage = self.state.storage();
            let cached = self.room.clone();
            // Block code reuse until storage AND cached identity are cleared.
            self.state
                .block_concurrency_while(async move {
                    // Admission awaits allow requests to reuse this code. Never
                    // delete a replacement installed while retirement ran.
                    if cached.borrow().as_ref().is_none_or(|current| {
                        current.instance != room.instance || current.generation != room.generation
                    }) {
                        return Ok(());
                    }
                    storage.delete_alarm().await?;
                    storage.delete_all().await?;
                    *cached.borrow_mut() = None;
                    Ok(())
                })
                .await?;
            return Response::empty();
        }
        let instant = now();
        if !room.public && instant - room.created_ms > MAX_AGE_MS {
            self.finish("Room expired").await?;
            return Response::empty();
        }
        for (ws, connection) in self.connections()? {
            if instant - connection.last_seen_ms > IDLE_MS {
                let _ = self.close_socket(&ws, 1000, "Connection timed out");
                self.departed(&ws, true).await?;
            }
        }
        let Some(mut room) = self.room.borrow().clone() else {
            return Response::empty();
        };
        let gone = expired(&room.reserved, instant);
        if !gone.is_empty() {
            room.reserved.retain(|r| !gone.contains(r));
            for held in &gone {
                if held.player.is_some() {
                    self.broadcast(&Packet::Leave { peer: held.peer }, None)?;
                }
                self.record("peer_slot_released", None, "", None, None);
            }
            if gone.iter().any(|r| r.peer == room.host) {
                self.promote(&mut room, None)?;
            }
            room.epoch = room.epoch.wrapping_add(1);
        }
        let empty = self.connections()?.is_empty() && room.reserved.is_empty();
        if empty {
            room.empty_since_ms.get_or_insert(instant);
        } else {
            room.empty_since_ms = None;
        }
        let idle = room
            .empty_since_ms
            .is_some_and(|since| instant - since > IDLE_MS);
        let holding = !room.reserved.is_empty();
        self.save(room).await?;
        if !gone.is_empty() {
            self.broadcast_lobby(None)?;
        }
        if idle {
            self.finish("Empty room expired").await?;
        } else {
            self.report_riders(None).await;
            self.state
                .storage()
                .set_alarm(if holding { 5_000_i64 } else { 30_000_i64 })
                .await?;
        }
        Response::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_identity_rejects_old_callbacks_after_storage_cleanup() {
        let mut room: Room = serde_json::from_value(serde_json::json!({
            "instance":"new-instance", "generation":1, "code":b"ABCD2345",
            "host_token":"secret", "created_ms":0., "closed":false,
            "epoch":1, "map":1, "started":false
        }))
        .unwrap();
        let mut connection: Connection = serde_json::from_value(serde_json::json!({
            "instance":"old-instance", "generation":1, "active":true, "peer":0,
            "player":null, "sequence":null,"last_seen_ms":0., "window_ms":0.,
            "messages":0,"bytes":0
        }))
        .unwrap();
        assert!(!same_room(&room, &connection));
        connection.instance = room.instance.clone();
        assert!(same_room(&room, &connection));
        room.generation += 1;
        assert!(!same_room(&room, &connection));
    }

    #[test]
    fn old_room_schema_loads_and_only_closed_or_expired_codes_are_reusable() {
        let mut room: Room = serde_json::from_value(serde_json::json!({
            "code": b"ABCD2345", "host_token":"secret", "created_ms":100.,
            "closed":false,"epoch":1,"map":8,"started":true,
            "host_disconnected_ms": 5., "host_player": null
        }))
        .unwrap();
        assert_eq!((room.generation, room.host), (0, 0));
        assert!(room.reserved.is_empty());
        assert!(!room_reusable(&room, 101.));
        assert!(!room_reusable(&room, 100. + MAX_AGE_MS));
        assert!(room_reusable(&room, 101. + MAX_AGE_MS));
        room.closed = true;
        assert!(room_reusable(&room, 101.));
        assert_eq!(room.capacity(), PRIVATE_CAPACITY);
        // A shared room never ages out while riders keep it alive.
        room.closed = false;
        room.public = true;
        assert!(!room_reusable(&room, 101. + MAX_AGE_MS * 4.));
        assert_eq!(room.capacity(), PUBLIC_CAPACITY);
    }

    #[test]
    fn rider_count_sums_only_rooms_that_reported_recently() {
        let mut rooms = std::collections::BTreeMap::new();
        let at = |riders, at_ms| RoomRiders { riders, at_ms };
        rooms.insert("FREERD01".to_string(), at(20, 1_000.));
        rooms.insert("FREERD02".to_string(), at(3, 50_000.));
        rooms.insert("FREERD03".to_string(), at(0, 60_000.));
        assert_eq!(fresh_riders(&rooms, 60_000.), 23);
        // FREERD01 stopped reporting; its riders no longer count.
        assert_eq!(fresh_riders(&rooms, 1_000. + STATS_FRESH_MS), 3);
        assert_eq!(fresh_riders(&std::collections::BTreeMap::new(), 0.), 0);
    }

    #[test]
    fn slots_skip_connected_and_held_riders_and_holds_expire_after_grace() {
        assert_eq!(free_slot([].into_iter(), 4), Some(0));
        assert_eq!(free_slot([0, 2].into_iter(), 4), Some(1));
        assert_eq!(free_slot([1, 0, 2].into_iter(), 4), Some(3));
        assert_eq!(free_slot([0, 1, 2, 3].into_iter(), 4), None);
        // Shared Free Ride rooms continue past the private four-rider limit.
        assert_eq!(
            free_slot([0, 1, 2, 3].into_iter(), PUBLIC_CAPACITY),
            Some(4)
        );
        assert_eq!(
            free_slot((0..20).collect::<Vec<_>>().into_iter(), PUBLIC_CAPACITY),
            None
        );
        let held = |peer, since_ms| Reserved {
            peer,
            player: None,
            key: "00".repeat(16),
            since_ms,
        };
        let reserved = [held(1, 0.), held(2, 10_000.)];
        assert!(expired(&reserved, GRACE_MS - 1.).is_empty());
        assert_eq!(expired(&reserved, GRACE_MS), vec![held(1, 0.)]);
        assert_eq!(expired(&reserved, GRACE_MS + 10_000.).len(), 2);
    }

    #[test]
    fn rejoin_keys_roundtrip_through_welcome_bytes() {
        let key = "00ff10a0b1c2d3e4f5061728394a5b6c";
        let bytes = key_bytes(key);
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes[1], 255);
        assert_eq!(bytes[15], 0x6c);
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, key);
    }

    #[test]
    fn only_transport_failures_allow_host_recovery() {
        for code in [1001, 1005, 1006, 1011, 1012, 1013] {
            assert!(recoverable_close(code, ""));
        }
        assert!(recoverable_close(1000, "Connection timed out"));
        for (code, reason) in [
            (1000, "Left lobby"),
            (1000, ""),
            (1008, "Rate limit"),
            (1003, "Binary protocol required"),
        ] {
            assert!(!recoverable_close(code, reason));
        }
    }

    #[test]
    fn game_origins_accept_only_explicit_hosts() {
        let allowed =
            "https://furyremastered.com, https://atv-offroad-fury-2.champtoday.workers.dev,";
        for origin in [
            "https://furyremastered.com",
            "https://atv-offroad-fury-2.champtoday.workers.dev",
        ] {
            assert!(origin_allowed(Some(origin), allowed));
        }
        for origin in [
            None,
            Some(""),
            Some("null"),
            Some("http://furyremastered.com"),
            Some("https://www.furyremastered.com"),
            Some("https://furyremastered.com.evil.example"),
            Some("https://foreign.example"),
        ] {
            assert!(!origin_allowed(origin, allowed));
        }
        assert!(origin_allowed(
            Some("https://legacy.example"),
            "https://legacy.example"
        ));
        assert!(!origin_allowed(Some(""), ""));
    }

    #[test]
    fn route_rejects_traversal_extra_path_and_invalid_codes() {
        assert_eq!(
            code_from_path("/multiplayer/rooms/ABCD2345"),
            Some(*b"ABCD2345")
        );
        for path in [
            "/multiplayer/rooms/ABCD2345/extra",
            "/multiplayer/rooms/ABCD234",
            "/multiplayer/rooms/ABCD23!O",
            "/multiplayer/rooms/../thing",
            "/another/rooms/ABCD2345",
        ] {
            assert!(code_from_path(path).is_none());
        }
    }

    #[test]
    fn per_connection_rate_budget_recovers_and_bytes_are_bounded() {
        let mut connection = Connection {
            instance: String::new(),
            generation: 1,
            close_recoverable: None,
            key: String::new(),
            ready_map: 0,
            active: true,
            peer: 0,
            player: None,
            sequence: None,
            snapshot: None,
            races: false,
            last_seen_ms: 0.,
            window_ms: 0.,
            messages: 0,
            bytes: 0,
        };
        // A burst of queued packets within the window's average budget.
        for _ in 0..360 {
            assert!(connection.account(10., 100));
        }
        assert!(!connection.account(20., 100));
        assert!(connection.account(3010., 8192));
        assert!(!connection.account(3011., 3 * 1024 * 1024));
    }

    #[test]
    fn hibernation_metadata_roundtrip_preserves_authority_sequence_and_rate() {
        let room = Room {
            instance: String::new(),
            admission_lease: None,
            generation: 1,
            host: 2,
            reserved: vec![],
            empty_since_ms: None,
            code: *b"ABCD2345",
            host_token: "secret".into(),
            created_ms: 123.,
            closed: false,
            epoch: 5,
            map: 9,
            started: true,
            public: false,
        };
        let connection = Connection {
            instance: String::new(),
            generation: 1,
            close_recoverable: None,
            key: "ab".repeat(16),
            ready_map: 9,
            active: true,
            peer: 2,
            player: Some(Participant {
                id: 2,
                name: "Female".into(),
                atv_model: 26,
                rider_model: 1,
                appearance: [0; 8],
            }),
            snapshot: None,
            races: false,
            sequence: Some(u32::MAX),
            last_seen_ms: 555.,
            window_ms: 500.,
            messages: 359,
            bytes: 999,
        };
        let loaded: Room = serde_json::from_str(&serde_json::to_string(&room).unwrap()).unwrap();
        assert_eq!(loaded.code, room.code);
        assert_eq!(loaded.host_token, room.host_token);
        assert_eq!((loaded.epoch, loaded.map, loaded.started), (5, 9, true));
        let mut resumed: Connection =
            serde_json::from_str(&serde_json::to_string(&connection).unwrap()).unwrap();
        assert_eq!(resumed.player, connection.player);
        assert_eq!(resumed.sequence, Some(u32::MAX));
        assert!(resumed.active);
        assert!(resumed.account(556., 500));
        assert!(!resumed.account(557., 500));
        assert!(atv_net::sequence_newer(0, resumed.sequence.unwrap()));
    }
}
