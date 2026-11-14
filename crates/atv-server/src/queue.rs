//! The Quick Play queue (research/QUICK_PLAY.md): `POST
//! /multiplayer/quickplay` seats a rider in a server-run room of the chosen
//! playlist, and finished matches go to `FURY_STATE_DIR/matches.jsonl`.

use std::io::Write as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use atv_net::RoomCode;
use atv_net::quickplay::playlist;
use axum::body::Bytes as Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::engine::quickplay::Record;
use crate::engine::{self, Entropy, Room};
use crate::http::{client_ip, error, features, json};
use crate::rooms::OsEntropy;
use crate::{Shared, now_ms};

pub const ROUTE: &str = "/multiplayer/quickplay";
/// Rooms per playlist (`QPR00001`…, `QPS00001`…).
pub const MAX_ROOMS: u32 = 500;
const RECORDS_FILE: &str = "matches.jsonl";

/// Room `number` of a playlist.
pub fn room_code(playlist: u8, number: u32) -> RoomCode {
    let letter = if playlist == playlist::STUNTS {
        b'S'
    } else {
        b'R'
    };
    let mut code = *b"QP000000";
    code[2] = letter;
    let digits = format!("{:05}", number % 100_000);
    code[3..].copy_from_slice(digits.as_bytes());
    code
}

/// `{"playlist": "race" | "stunts", "avoid": "QPR00001"}`: a seat in the
/// lowest-numbered room of that playlist that is between rounds (never
/// `avoid`, the room a rider just left), else a new room. Answers like a
/// Free Ride claim, plus `"quickplay": true` and the playlist.
pub async fn claim(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if body.len() > 512 {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "Request too large");
    }
    let value: serde_json::Value = if body.is_empty() {
        serde_json::Value::Null
    } else {
        match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid Quick Play request"),
        }
    };
    let Some(list) = playlist::parse(value["playlist"].as_str().unwrap_or("race")) else {
        return error(StatusCode::BAD_REQUEST, "Unknown playlist");
    };
    let avoid = value["avoid"]
        .as_str()
        .and_then(|c| atv_net::parse_room_code(c).ok());
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
    let answer = |code: RoomCode, key: String| {
        let mut body = features();
        body.insert("code".into(), String::from_utf8_lossy(&code).into());
        body.insert("key".into(), key.into());
        body.insert("quickplay".into(), true.into());
        body.insert(
            "playlist".into(),
            playlist::name(list).unwrap_or("race").into(),
        );
        json(StatusCode::OK, body.into())
    };
    // A room between rounds with a free seat.
    let mut free = None;
    for number in 1..=MAX_ROOMS {
        let code = room_code(list, number);
        match shared.registry.get(&code) {
            Some(_) if Some(code) == avoid => {}
            Some(room) => {
                if let Some(Some(key)) = room.claim_quickplay(false).await {
                    return answer(code, key);
                }
            }
            None => {
                free.get_or_insert(number);
            }
        }
    }
    // Else a new room: its lobby fills with CPUs, so it starts at once.
    let mut rng = OsEntropy;
    for number in free.into_iter().chain(1..=MAX_ROOMS) {
        let code = room_code(list, number);
        if shared.registry.get(&code).is_some() {
            continue;
        }
        let key = engine::token(&mut rng);
        let mut seed = [0; 8];
        rng.fill(&mut seed);
        let room = Room::quickplay(code, key.clone(), list, now_ms(), u64::from_le_bytes(seed));
        if shared.registry.open(&shared, room).is_some() {
            tracing::info!(event = "quickplay_room", room = %String::from_utf8_lossy(&code));
            return answer(code, key);
        }
    }
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Quick Play is full. Try again shortly.",
    )
}

/// Appends finished matches to the state directory's record file, off the
/// room task.
pub fn save_records(shared: &Arc<Shared>, records: Vec<Record>) {
    if records.is_empty() {
        return;
    }
    let path = shared.config.state_dir.join(RECORDS_FILE);
    tokio::task::spawn_blocking(move || {
        let mut lines = String::new();
        for record in &records {
            if let Ok(line) = serde_json::to_string(record) {
                lines.push_str(&line);
                lines.push('\n');
            }
        }
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(lines.as_bytes()));
        match written {
            Ok(()) => tracing::info!(event = "match_recorded", matches = records.len()),
            Err(e) => tracing::warn!(event = "match_record_failed", error = %e),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_codes_are_reserved_and_valid() {
        let code = room_code(playlist::RACE, 1);
        assert_eq!(&code, b"QPR00001");
        assert_eq!(&room_code(playlist::STUNTS, 42), b"QPS00042");
        assert!(atv_net::valid_room_code(&code));
        assert!(atv_net::quickplay::is_quickplay_room_code(&code));
    }
}
