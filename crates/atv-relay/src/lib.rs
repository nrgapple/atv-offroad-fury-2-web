//! Authored Rust room service. Original PS2 networking is not executed here.
use std::{cell::RefCell, rc::Rc};

use atv_net::{Lobby, MAX_PLAYERS, Packet, Participant, ROOM_CODE_BYTES, RoomCode};
use serde::{Deserialize, Serialize};
use worker::{wasm_bindgen::JsCast, *};

const ROOT: &str = "/multiplayer/rooms";
const ROOM_KEY: &str = "room";
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const MAX_AGE_MS: f64 = 12. * 60. * 60. * 1000.;
const IDLE_MS: f64 = 60_000.;

#[derive(Clone, Serialize, Deserialize)]
struct Room {
    code: RoomCode,
    host_token: String,
    created_ms: f64,
    closed: bool,
    epoch: u32,
    map: u16,
    started: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Connection {
    active: bool,
    peer: u8,
    player: Option<Participant>,
    sequence: Option<u32>,
    last_seen_ms: f64,
    window_ms: f64,
    messages: u16,
    bytes: usize,
}

impl Connection {
    fn account(&mut self, now: f64, bytes: usize) -> bool {
        if now - self.window_ms >= 1000. {
            self.window_ms = now;
            self.messages = 0;
            self.bytes = 0;
        }
        self.messages = self.messages.saturating_add(1);
        self.bytes = self.bytes.saturating_add(bytes);
        self.last_seen_ms = now;
        self.messages <= 120 && self.bytes <= 1024 * 1024
    }
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

fn same_origin(req: &Request, env: &Env) -> Result<bool> {
    Ok(req.headers().get("Origin")?.as_deref()
        == Some(env.var("GAME_ORIGIN")?.to_string().as_str()))
}

fn send(ws: &WebSocket, packet: &Packet) -> Result<()> {
    let bytes = atv_net::encode(packet).map_err(|e| Error::RustError(e.to_string()))?;
    ws.send_with_bytes(bytes)
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
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
        // Name lookup is stable, but guessed codes must never create joinable rooms.
        // Initialization goes only through this creator route, with collision retry.
        for _ in 0..4 {
            let code = random_code()?;
            let text = std::str::from_utf8(&code).expect("ASCII room code");
            let stub = namespace.id_from_name(text)?.get_stub()?;
            let token = random_token()?;
            let mut init = RequestInit::new();
            init.method = Method::Post;
            init.body = Some(
                serde_json::to_string(&Room {
                    code,
                    host_token: token.clone(),
                    created_ms: now(),
                    closed: false,
                    epoch: 1,
                    map: 1,
                    started: false,
                })?
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
            if response.status_code() != 409 {
                return Response::error("Room creation failed", 503);
            }
        }
        return Response::error("Room creation unavailable", 503);
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
}

impl LobbyRoom {
    fn connections(&self) -> Result<Vec<(WebSocket, Connection)>> {
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
            .map(|all| all.into_iter().filter(|(_, c)| c.active).collect())
    }

    fn lobby(&self, except: Option<&WebSocket>) -> Result<Lobby> {
        let room = self.room.borrow();
        let room = room.as_ref().ok_or("Room does not exist")?;
        let mut players = self
            .connections()?
            .into_iter()
            .filter(|(ws, _)| except != Some(ws))
            .filter_map(|(_, connection)| connection.player)
            .collect::<Vec<_>>();
        players.sort_by_key(|player| player.id);
        Ok(Lobby {
            host: 0,
            epoch: room.epoch,
            map: room.map,
            started: room.started,
            players,
        })
    }

    fn broadcast(&self, packet: &Packet, except: Option<&WebSocket>) -> Result<()> {
        let bytes = atv_net::encode(packet).map_err(|e| Error::RustError(e.to_string()))?;
        for (ws, connection) in self.connections()? {
            if except != Some(&ws) && connection.player.is_some() {
                // A failed peer send must not turn another rider's valid message into an error.
                if ws.send_with_bytes(&bytes).is_err() {
                    let _ = ws.close(Some(1011), Some("Send failed"));
                }
            }
        }
        Ok(())
    }

    async fn finish(&self, reason: &str) -> Result<()> {
        if self.room.borrow().as_ref().is_none_or(|room| room.closed) {
            return Ok(());
        }
        let saved = {
            let mut room = self.room.borrow_mut();
            if let Some(room) = room.as_mut() {
                room.closed = true;
            }
            room.clone()
        };
        if let Some(room) = saved {
            self.state.storage().put(ROOM_KEY, room).await?;
        }
        for ws in self.state.get_websockets() {
            let _ = ws.close(Some(1000), Some(reason));
        }
        self.state.storage().delete_alarm().await?;
        Ok(())
    }

    async fn departed(&self, ws: &WebSocket) -> Result<()> {
        if let Some(mut connection) = ws.deserialize_attachment::<Connection>()? {
            if !connection.active {
                return Ok(());
            }
            connection.active = false;
            ws.serialize_attachment(&connection)?;
            if self.room.borrow().as_ref().is_none_or(|room| room.closed) {
                return Ok(());
            }
            if connection.peer == 0 {
                return self.finish("Host left the room").await;
            }
            self.bump_epoch().await?;
            self.broadcast(
                &Packet::Leave {
                    peer: connection.peer,
                },
                Some(ws),
            )?;
            let lobby = self.lobby(Some(ws))?;
            if !lobby.players.is_empty() {
                self.broadcast(&Packet::Lobby(lobby), Some(ws))?;
            }
        }
        Ok(())
    }

    async fn bump_epoch(&self) -> Result<()> {
        let saved = {
            let mut room = self.room.borrow_mut();
            let room = room.as_mut().ok_or("Room does not exist")?;
            room.epoch = room.epoch.wrapping_add(1);
            room.clone()
        };
        self.state.storage().put(ROOM_KEY, saved).await
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
        Self { state, env, room }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        if req.path() == "/create" && req.method() == Method::Post {
            if self.room.borrow().is_some() {
                return Response::error("Room exists", 409);
            }
            let room: Room = req.json().await?;
            self.state.storage().put(ROOM_KEY, &room).await?;
            *self.room.borrow_mut() = Some(room);
            self.state.storage().set_alarm(30_000_i64).await?;
            return Ok(Response::empty()?.with_status(201));
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
        if room.code != code || room.closed || now() - room.created_ms > MAX_AGE_MS {
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
        let host_token = req
            .url()?
            .query_pairs()
            .find(|(k, _)| k == "host")
            .map(|(_, v)| v.into_owned());
        let connections = self.connections()?;
        if connections.len() >= MAX_PLAYERS {
            return Response::error("Room is full", 409);
        }
        let peer = if host_token.as_deref() == Some(room.host_token.as_str()) {
            if connections.iter().any(|(_, c)| c.peer == 0) {
                return Response::error("Host already connected", 409);
            }
            0
        } else {
            if host_token.is_some() {
                return Response::error("Invalid host token", 403);
            }
            if !connections
                .iter()
                .any(|(_, c)| c.peer == 0 && c.player.is_some())
            {
                return Response::error("Host is not connected", 409);
            }
            (1..MAX_PLAYERS as u8)
                .find(|id| connections.iter().all(|(_, c)| c.peer != *id))
                .ok_or("Room is full")?
        };
        let pair = WebSocketPair::new()?;
        let instant = now();
        pair.server.serialize_attachment(Connection {
            active: true,
            peer,
            player: None,
            sequence: None,
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
            return ws.close(Some(1000), Some("Room ended"));
        };
        let WebSocketIncomingMessage::Binary(bytes) = message else {
            return ws.close(Some(1003), Some("Binary protocol required"));
        };
        let Some(mut connection) = ws.deserialize_attachment::<Connection>()? else {
            return ws.close(Some(1008), Some("Connection missing"));
        };
        if !connection.active {
            return ws.close(Some(1000), Some("Disconnected"));
        }
        if !connection.account(now(), bytes.len()) {
            return ws.close(Some(1008), Some("Message rate exceeded"));
        }
        let Ok(packet) = atv_net::decode(&bytes) else {
            return ws.close(Some(1008), Some("Invalid multiplayer packet"));
        };
        match packet {
            Packet::Join { code, mut player }
                if connection.player.is_none() && code == room.code =>
            {
                if !(1..=26).contains(&player.atv_model) || player.rider_model > 1 {
                    return ws.close(Some(1008), Some("Unknown actor model"));
                }
                player.id = connection.peer;
                connection.player = Some(player);
                ws.serialize_attachment(&connection)?;
                self.bump_epoch().await?;
                let lobby = self.lobby(None)?;
                send(
                    &ws,
                    &Packet::Welcome {
                        peer: connection.peer,
                        code,
                        lobby: lobby.clone(),
                    },
                )?;
                self.broadcast(&Packet::Lobby(lobby), Some(&ws))?;
            }
            Packet::Snapshot(mut snapshot) if connection.player.is_some() => {
                if !room.started {
                    ws.serialize_attachment(connection)?;
                    return Ok(());
                }
                let player = connection
                    .player
                    .as_ref()
                    .expect("matched connected player");
                if snapshot.atv_model != player.atv_model
                    || snapshot.rider_model != player.rider_model
                {
                    return ws.close(Some(1008), Some("Actor model changed"));
                }
                if connection
                    .sequence
                    .is_none_or(|old| atv_net::sequence_newer(snapshot.sequence, old))
                {
                    connection.sequence = Some(snapshot.sequence);
                    snapshot.peer = connection.peer;
                    self.broadcast(&Packet::Snapshot(snapshot), Some(&ws))?;
                }
            }
            Packet::Ping { nonce } if connection.player.is_some() => {
                send(&ws, &Packet::Pong { nonce })?
            }
            Packet::SetLobby {
                epoch,
                map,
                started,
            } if connection.peer == 0 && connection.player.is_some() => {
                if !(1..=8).contains(&map) {
                    return ws.close(Some(1008), Some("Unknown environment"));
                }
                if epoch != room.epoch || (room.started && (map != room.map || !started)) {
                    send(&ws, &Packet::Lobby(self.lobby(None)?))?;
                    ws.serialize_attachment(connection)?;
                    return Ok(());
                }
                let mut next = room;
                next.map = map;
                next.started = started;
                next.epoch = next.epoch.wrapping_add(1);
                self.state.storage().put(ROOM_KEY, &next).await?;
                *self.room.borrow_mut() = Some(next);
                self.broadcast(&Packet::Lobby(self.lobby(None)?), None)?;
            }
            Packet::Leave { .. } => return ws.close(Some(1000), Some("Left room")),
            _ => return ws.close(Some(1008), Some("Message not permitted")),
        }
        ws.serialize_attachment(connection)?;
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        _code: usize,
        _reason: String,
        _clean: bool,
    ) -> Result<()> {
        let _ = ws.close(Some(1000), Some("Disconnected"));
        self.departed(&ws).await
    }

    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        let _ = ws.close(Some(1011), Some("Socket error"));
        self.departed(&ws).await
    }

    async fn alarm(&self) -> Result<Response> {
        let Some(room) = self.room.borrow().clone() else {
            return Response::empty();
        };
        if room.closed {
            return Response::empty();
        }
        let instant = now();
        if instant - room.created_ms > MAX_AGE_MS {
            self.finish("Room expired").await?;
        } else {
            let connections = self.connections()?;
            if connections.is_empty() && instant - room.created_ms > IDLE_MS {
                self.finish("Empty room expired").await?;
            } else {
                for (ws, connection) in connections {
                    if instant - connection.last_seen_ms > IDLE_MS {
                        let _ = ws.close(Some(1000), Some("Connection timed out"));
                        self.departed(&ws).await?;
                    }
                }
                if self.room.borrow().as_ref().is_some_and(|r| !r.closed) {
                    self.state.storage().set_alarm(30_000_i64).await?;
                }
            }
        }
        Response::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_rejects_traversal_extra_path_and_ambiguous_codes() {
        assert_eq!(
            code_from_path("/multiplayer/rooms/ABCD2345"),
            Some(*b"ABCD2345")
        );
        for path in [
            "/multiplayer/rooms/ABCD2345/extra",
            "/multiplayer/rooms/ABCD234",
            "/multiplayer/rooms/ABCD23IO",
            "/multiplayer/rooms/../thing",
            "/another/rooms/ABCD2345",
        ] {
            assert!(code_from_path(path).is_none());
        }
    }

    #[test]
    fn per_connection_rate_budget_recovers_and_bytes_are_bounded() {
        let mut connection = Connection {
            active: true,
            peer: 0,
            player: None,
            sequence: None,
            last_seen_ms: 0.,
            window_ms: 0.,
            messages: 0,
            bytes: 0,
        };
        for _ in 0..120 {
            assert!(connection.account(10., 100));
        }
        assert!(!connection.account(20., 100));
        assert!(connection.account(1001., 8192));
        assert!(!connection.account(1002., 1024 * 1024));
    }

    #[test]
    fn hibernation_metadata_roundtrip_preserves_authority_sequence_and_rate() {
        let room = Room {
            code: *b"ABCD2345",
            host_token: "secret".into(),
            created_ms: 123.,
            closed: false,
            epoch: 5,
            map: 8,
            started: true,
        };
        let connection = Connection {
            active: true,
            peer: 2,
            player: Some(Participant {
                id: 2,
                name: "Female".into(),
                atv_model: 26,
                rider_model: 1,
            }),
            sequence: Some(u32::MAX),
            last_seen_ms: 555.,
            window_ms: 500.,
            messages: 119,
            bytes: 999,
        };
        let loaded: Room = serde_json::from_str(&serde_json::to_string(&room).unwrap()).unwrap();
        assert_eq!(loaded.code, room.code);
        assert_eq!(loaded.host_token, room.host_token);
        assert_eq!((loaded.epoch, loaded.map, loaded.started), (5, 8, true));
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
