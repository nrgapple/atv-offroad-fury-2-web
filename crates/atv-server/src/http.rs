//! Routes: the relay's `/multiplayer/` routes with the same JSON, plus
//! `/healthz` and `/metrics`. Origin and CORS checks replace the relay's
//! same-origin rule behind the site Worker.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes as Body;
use axum::extract::ws::{
    CloseFrame, Message, WebSocket, WebSocketUpgrade, rejection::WebSocketUpgradeRejection,
};
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next as Continue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;

use crate::engine::{self, Room};
use crate::outbox::{Next, Outbox};
use crate::rooms::{OsEntropy, RoomMsg};
use crate::{Shared, now_ms};

const ROOMS: &str = "/multiplayer/rooms";
const FREERIDE: &str = "/multiplayer/freeride";
const FREERIDE_RIDERS: &str = "/multiplayer/freeride/riders";
const FREERIDE_DAYS: &str = "/multiplayer/freeride/days";
/// Routes anyone may read, without an Origin.
const PUBLIC: [&str; 4] = [FREERIDE_RIDERS, FREERIDE_DAYS, "/healthz", "/metrics"];
/// A rider that stops reading gets this long to take a packet.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// After the server's close, the client gets this long to answer it.
const CLOSE_WAIT: Duration = Duration::from_secs(5);
/// What the server claims for every room: no direct links or batches, and
/// frames for builds that take them.
fn features() -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    map.insert("links".into(), false.into());
    map.insert("batches".into(), false.into());
    map.insert("frames".into(), true.into());
    map
}

pub fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route(ROOMS, post(create))
        .route("/multiplayer/rooms/{code}", get(socket))
        .route(FREERIDE, post(claim))
        .route(FREERIDE_RIDERS, get(riders))
        .route(FREERIDE_DAYS, get(days))
        .route("/multiplayer/version", get(version))
        .route("/healthz", get(health))
        .route("/metrics", get(metrics))
        .fallback(|| async { error(StatusCode::NOT_FOUND, "Unknown multiplayer route") })
        .layer(middleware::from_fn_with_state(shared.clone(), origins))
        .with_state(shared)
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}

fn json(status: StatusCode, value: serde_json::Value) -> Response {
    (status, axum::Json(value)).into_response()
}

/// The page origin, when the request has one.
fn origin(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
}

/// The client's address: the peer, or for a loopback peer (a proxy on the
/// same host) the last address it says it forwarded for.
pub fn client_ip(peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
    if peer.ip().is_loopback()
        && let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .and_then(|v| v.trim().parse().ok())
    {
        return forwarded;
    }
    peer.ip()
}

/// Public reads answer anyone; everything else needs an allowed Origin,
/// gets CORS headers for it, and has its preflight answered here.
async fn origins(State(shared): State<Arc<Shared>>, request: Request, next: Continue) -> Response {
    let path = request.uri().path();
    if PUBLIC.contains(&path) {
        let mut response = next.run(request).await;
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
        return response;
    }
    // A restarting server takes no new rooms or riders.
    if shared.shutting_down.load(Ordering::Relaxed) {
        return error(StatusCode::SERVICE_UNAVAILABLE, "Server restarting");
    }
    let origin = origin(request.headers()).map(str::to_string);
    if !shared.config.origins.allows(origin.as_deref()) {
        shared
            .metrics
            .refused_origin
            .fetch_add(1, Ordering::Relaxed);
        return error(StatusCode::FORBIDDEN, "Origin denied");
    }
    let mut response = if request.method() == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        let h = response.headers_mut();
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("content-type"),
        );
        h.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
        response
    } else {
        next.run(request).await
    };
    if let Some(value) = origin.and_then(|o| HeaderValue::from_str(&o).ok()) {
        let h = response.headers_mut();
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        h.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response
}

/// Opens a private room, or with `{"code": …, "open": true}` (an invite
/// link) reports a live room with that code instead of refusing it.
async fn create(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if body.len() > 512 {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "Lobby request too large");
    }
    let mut open = false;
    let requested = if body.is_empty() {
        None
    } else {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body) else {
            return error(StatusCode::BAD_REQUEST, "Invalid lobby request");
        };
        open = value.get("open").and_then(|v| v.as_bool()) == Some(true);
        match value.get("code").and_then(|v| v.as_str()) {
            None | Some("") => None,
            Some(code) => match atv_net::parse_room_code(code) {
                Ok(code) if atv_net::is_public_room_code(&code) => {
                    return error(StatusCode::BAD_REQUEST, "That code is reserved");
                }
                Ok(code) => Some(code),
                Err(_) => return error(StatusCode::BAD_REQUEST, "Use 8 letters or numbers"),
            },
        }
    };
    let ip = client_ip(peer, &headers);
    if !shared
        .admission
        .lock()
        .expect("admission lock")
        .room(ip, now_ms())
    {
        shared
            .metrics
            .refused_admission
            .fetch_add(1, Ordering::Relaxed);
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many rooms. Try again shortly.",
        );
    }
    let mut rng = OsEntropy;
    for _ in 0..4 {
        let code = requested.unwrap_or_else(|| engine::room_code(&mut rng));
        let text = String::from_utf8_lossy(&code).into_owned();
        let token = engine::token(&mut rng);
        let room = Room::new(code, token.clone(), false, now_ms());
        if shared.registry.open(&shared, room).is_some() {
            tracing::info!(event = "room_created", room = %text);
            let mut body = features();
            body.insert("code".into(), text.into());
            body.insert("hostToken".into(), token.into());
            return json(StatusCode::CREATED, body.into());
        }
        if requested.is_some() {
            if !open {
                return error(StatusCode::CONFLICT, "Lobby code is taken");
            }
            let Some(players) = shared.registry.get(&code) else {
                // It ended meanwhile: try again with the code free.
                continue;
            };
            let Some(players) = players.riders().await else {
                continue;
            };
            let mut body = features();
            body.insert("code".into(), text.into());
            body.insert("open".into(), true.into());
            body.insert("players".into(), players.into());
            body.insert("max".into(), atv_net::PRIVATE_CAPACITY.into());
            return json(StatusCode::OK, body.into());
        }
    }
    error(StatusCode::SERVICE_UNAVAILABLE, "Room creation unavailable")
}

/// Arlington Free Ride: a slot in the lowest-numbered shared room with
/// space, so riders gather together; a full room opens the next one.
async fn claim(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let ip = client_ip(peer, &headers);
    if !shared
        .admission
        .lock()
        .expect("admission lock")
        .claim(ip, now_ms())
    {
        shared
            .metrics
            .refused_admission
            .fetch_add(1, Ordering::Relaxed);
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many requests. Try again shortly.",
        );
    }
    let mut rng = OsEntropy;
    for number in 1..=atv_net::PUBLIC_ROOMS {
        let code = atv_net::public_room_code(number).expect("public room number");
        let text = String::from_utf8_lossy(&code).into_owned();
        // A live room reserves a slot; a room that ended is opened again.
        for _ in 0..2 {
            let key = match shared.registry.get(&code) {
                Some(room) => match room.claim().await {
                    Some(Some(key)) => Some(key),
                    Some(None) => break,
                    None => continue,
                },
                None => {
                    let key = engine::token(&mut rng);
                    let room = Room::new(code, key.clone(), true, now_ms());
                    shared.registry.open(&shared, room).map(|_| key)
                }
            };
            if let Some(key) = key {
                let mut body = features();
                body.insert("code".into(), text.into());
                body.insert("key".into(), key.into());
                return json(StatusCode::OK, body.into());
            }
        }
    }
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Free Ride is full. Try again shortly.",
    )
}

async fn riders(State(shared): State<Arc<Shared>>) -> Response {
    let value = shared
        .usage
        .lock()
        .expect("usage lock")
        .riders_json(now_ms());
    let mut response = json(StatusCode::OK, value);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=10"),
    );
    response
}

async fn days(State(shared): State<Arc<Shared>>) -> Response {
    let value = shared.usage.lock().expect("usage lock").days_json();
    let mut response = json(StatusCode::OK, value);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    response
}

async fn version(State(shared): State<Arc<Shared>>) -> Response {
    json(
        StatusCode::OK,
        serde_json::json!({
            "commit": shared.config.commit,
            "wasmSha256": "host-only",
            "protocol": atv_net::VERSION,
        }),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Health<'a> {
    ok: bool,
    commit: &'a str,
    protocol: u8,
    uptime_seconds: u64,
    rooms: u64,
    riders: u64,
    connections: u64,
}

async fn health(State(shared): State<Arc<Shared>>) -> Response {
    let m = &shared.metrics;
    let mut response = axum::Json(Health {
        ok: true,
        commit: &shared.config.commit,
        protocol: atv_net::VERSION,
        uptime_seconds: shared.started.elapsed().as_secs(),
        rooms: m.rooms.load(Ordering::Relaxed),
        riders: m.riders.load(Ordering::Relaxed),
        connections: m
            .connections
            .load(Ordering::Relaxed)
            .saturating_sub(m.uncounted_connections.load(Ordering::Relaxed)),
    })
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn metrics(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if !peer.ip().is_loopback() {
        return error(StatusCode::NOT_FOUND, "Unknown multiplayer route");
    }
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        shared
            .metrics
            .prometheus(shared.started.elapsed().as_secs()),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct Keys {
    key: Option<String>,
    host: Option<String>,
}

/// A room's WebSocket: `?key=` (or the creator's `?host=`) rejoins a slot.
async fn socket(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(code): Path<String>,
    Query(keys): Query<Keys>,
    headers: HeaderMap,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let Some(code) = <[u8; atv_net::ROOM_CODE_BYTES]>::try_from(code.as_bytes())
        .ok()
        .filter(atv_net::valid_room_code)
    else {
        return error(StatusCode::NOT_FOUND, "Unknown room");
    };
    let Some(room) = shared.registry.get(&code) else {
        return error(StatusCode::NOT_FOUND, "Room does not exist");
    };
    let Ok(upgrade) = upgrade else {
        return error(StatusCode::UPGRADE_REQUIRED, "WebSocket required");
    };
    if shared.shutting_down.load(Ordering::Relaxed) {
        return error(StatusCode::SERVICE_UNAVAILABLE, "Server restarting");
    }
    let ip = client_ip(peer, &headers);
    if !shared
        .admission
        .lock()
        .expect("admission lock")
        .socket(ip, now_ms())
    {
        shared
            .metrics
            .refused_admission
            .fetch_add(1, Ordering::Relaxed);
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many connections. Try again shortly.",
        );
    }
    // Releases the address's open socket on every path from here.
    let guard = Admitted(shared.clone(), ip);
    let conn = shared.next_conn.fetch_add(1, Ordering::Relaxed);
    let outbox = Arc::new(Outbox::default());
    let key = keys.key.or(keys.host);
    // Uncounted addresses play normally; only the public counts skip them.
    let counted = !shared.config.uncounted.contains(ip);
    let peer_id = match room.admit(conn, key, counted, outbox.clone()).await {
        Some(Ok(peer)) => peer,
        Some(Err(refused)) => {
            let status = StatusCode::from_u16(refused.status).unwrap_or(StatusCode::CONFLICT);
            return error(status, refused.message);
        }
        None => return error(StatusCode::GONE, "Room ended"),
    };
    let failed = room.tx.clone();
    upgrade
        .max_message_size(2 * atv_net::MAX_PACKET_BYTES)
        .max_frame_size(2 * atv_net::MAX_PACKET_BYTES)
        .on_failed_upgrade(move |_| {
            let _ = failed.try_send(RoomMsg::Closed {
                conn,
                code: 1006,
                reason: String::new(),
            });
        })
        .on_upgrade(move |ws| async move {
            let _guard = guard;
            tracing::debug!(event = "socket_opened", peer = peer_id);
            run_socket(shared, room.tx, conn, counted, outbox, ws).await;
        })
}

struct Admitted(Arc<Shared>, IpAddr);
impl Drop for Admitted {
    fn drop(&mut self) {
        if let Ok(mut admission) = self.0.admission.lock() {
            admission.closed(self.1);
        }
    }
}

/// Pumps one room socket: what arrives goes to the room, what the room
/// queued goes out. Ends when either side closes.
async fn run_socket(
    shared: Arc<Shared>,
    room: tokio::sync::mpsc::Sender<RoomMsg>,
    conn: engine::ConnId,
    counted: bool,
    outbox: Arc<Outbox>,
    ws: WebSocket,
) {
    shared.metrics.connections.fetch_add(1, Ordering::Relaxed);
    if !counted {
        shared
            .metrics
            .uncounted_connections
            .fetch_add(1, Ordering::Relaxed);
    }
    let (mut sink, mut stream) = ws.split();
    let writer_outbox = outbox.clone();
    let mut writer = tokio::spawn(async move {
        loop {
            match writer_outbox.next().await {
                Next::Packet(bytes) => {
                    match tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Binary(bytes)))
                        .await
                    {
                        Ok(Ok(())) => {}
                        // The client stopped reading or the socket broke.
                        _ => return Some((1006, "Send failed")),
                    }
                }
                Next::Close(code, reason) => {
                    let frame = CloseFrame {
                        code,
                        reason: reason.into(),
                    };
                    let _ =
                        tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Close(Some(frame))))
                            .await;
                    return None;
                }
            }
        }
    });
    let reader = async {
        while let Some(message) = stream.next().await {
            let msg = match message {
                Ok(Message::Binary(bytes)) => RoomMsg::Message { conn, bytes },
                Ok(Message::Text(_)) => {
                    outbox.close(1003, "Binary protocol required");
                    RoomMsg::Closed {
                        conn,
                        code: 1003,
                        reason: "Binary protocol required".into(),
                    }
                }
                Ok(Message::Close(frame)) => {
                    let (code, reason) =
                        frame.map_or((1005, String::new()), |f| (f.code, f.reason.to_string()));
                    RoomMsg::Closed { conn, code, reason }
                }
                Ok(_) => continue,
                Err(_) => RoomMsg::Closed {
                    conn,
                    code: 1006,
                    reason: String::new(),
                },
            };
            let closed = matches!(msg, RoomMsg::Closed { .. });
            if room.send(msg).await.is_err() || closed {
                return;
            }
        }
        let _ = room
            .send(RoomMsg::Closed {
                conn,
                code: 1006,
                reason: String::new(),
            })
            .await;
    };
    tokio::pin!(reader);
    tokio::select! {
        () = &mut reader => {
            // The client went: let the writer send what it can, briefly.
            outbox.close(1000, "Disconnected");
            let _ = tokio::time::timeout(CLOSE_WAIT, &mut writer).await;
        }
        result = &mut writer => {
            if let Ok(Some((code, reason))) = result {
                let _ = room.send(RoomMsg::Closed { conn, code, reason: reason.into() }).await;
            } else {
                // The server closed it: give the client a moment to answer.
                let _ = tokio::time::timeout(CLOSE_WAIT, reader).await;
            }
        }
    }
    writer.abort();
    if !counted {
        shared
            .metrics
            .uncounted_connections
            .fetch_sub(1, Ordering::Relaxed);
    }
    shared.metrics.connections.fetch_sub(1, Ordering::Relaxed);
}
