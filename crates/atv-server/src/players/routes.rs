//! `/multiplayer/players/…`, `/multiplayer/friends/…` and
//! `/multiplayer/league`: JSON POSTs whose body carries the token, so no
//! token reaches a URL or a log line.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::Router;
use axum::body::Bytes as Body;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::Deserialize;
use serde_json::{Value, json};

use super::Refusal;
use crate::rooms::OsEntropy;
use crate::{Shared, now_ms};

const MAX_BODY: usize = 1024;

pub fn routes() -> Router<Arc<Shared>> {
    Router::new()
        .route("/multiplayer/players/register", post(register))
        .route("/multiplayer/players/me", post(me))
        .route("/multiplayer/players/settings", post(settings))
        .route("/multiplayer/players/code", post(new_code))
        .route("/multiplayer/friends", post(friends))
        .route("/multiplayer/friends/{action}", post(friend))
        .route("/multiplayer/league", post(league))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Ask {
    token: Option<String>,
    secret: Option<String>,
    name: Option<String>,
    code: Option<String>,
    id: Option<String>,
    hidden: Option<bool>,
}

fn answer(result: Result<Value, Refusal>) -> Response {
    let mut response = match result {
        Ok(value) => (StatusCode::OK, axum::Json(value)).into_response(),
        Err((status, message)) => (
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
            axum::Json(json!({"error": message})),
        )
            .into_response(),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn parse(body: &[u8]) -> Result<Ask, Refusal> {
    if body.len() > MAX_BODY {
        return Err((413, "Too large"));
    }
    serde_json::from_slice(body).map_err(|_| (400, "Bad request"))
}

/// The body and the verified rider it speaks for.
fn rider(shared: &Shared, body: &[u8]) -> Result<(Ask, String), Refusal> {
    let ask = parse(body)?;
    let pid = ask
        .token
        .as_deref()
        .and_then(|t| shared.players.verify(t, now_ms()))
        .ok_or((401, "Register again"))?;
    Ok((ask, pid))
}

/// Writes share the room-claim allowance per address.
fn admitted(shared: &Shared, peer: SocketAddr, headers: &HeaderMap) -> Result<(), Refusal> {
    let ip = crate::http::client_ip(peer, headers);
    if shared
        .admission
        .lock()
        .expect("admission lock")
        .claim(ip, now_ms())
    {
        Ok(())
    } else {
        shared
            .metrics
            .refused_admission
            .fetch_add(1, Ordering::Relaxed);
        Err((429, "Too many requests"))
    }
}

async fn register(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    answer((|| {
        let ask = parse(&body)?;
        admitted(&shared, peer, &headers)?;
        shared.players.register(
            ask.secret.as_deref().unwrap_or(""),
            ask.name.as_deref().unwrap_or(""),
            now_ms(),
            &mut OsEntropy,
        )
    })())
}

async fn me(State(shared): State<Arc<Shared>>, body: Body) -> Response {
    answer(rider(&shared, &body).and_then(|(_, pid)| shared.players.me(&pid, now_ms())))
}

async fn settings(State(shared): State<Arc<Shared>>, body: Body) -> Response {
    answer(rider(&shared, &body).and_then(|(ask, pid)| {
        shared
            .players
            .settings(&pid, ask.hidden, ask.name.as_deref())?;
        shared.players.me(&pid, now_ms())
    }))
}

async fn new_code(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    answer(rider(&shared, &body).and_then(|(_, pid)| {
        admitted(&shared, peer, &headers)?;
        let code = shared.players.new_code(&pid, &mut OsEntropy)?;
        Ok(json!({"code": code}))
    }))
}

async fn friends(State(shared): State<Arc<Shared>>, body: Body) -> Response {
    answer(rider(&shared, &body).and_then(|(_, pid)| shared.players.friends(&pid, now_ms())))
}

async fn friend(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(action): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    answer(rider(&shared, &body).and_then(|(ask, pid)| {
        admitted(&shared, peer, &headers)?;
        let state = if action == "add" {
            shared
                .players
                .add(&pid, ask.code.as_deref(), ask.id.as_deref())?
        } else {
            let id = ask.id.as_deref().ok_or((400, "Rider needed"))?;
            shared.players.act(&pid, &action, id)?
        };
        Ok(json!({"state": state}))
    }))
}

async fn league(State(shared): State<Arc<Shared>>, body: Body) -> Response {
    let pid = parse(&body)
        .ok()
        .and_then(|ask| ask.token)
        .and_then(|t| shared.players.verify(&t, now_ms()));
    answer(Ok(shared.players.league(pid.as_deref(), now_ms())))
}
