//! `POST /multiplayer/report` (research/QUICK_CHAT.md "Reports"): one tap
//! with a fixed reason. The reporter proves its slot with its rejoin key;
//! the room adds the names and its recent chat, and the line is appended to
//! `FURY_STATE_DIR/reports.jsonl`. No address, email or free text is kept.

use std::collections::HashMap;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes as Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::engine::chat::ReportContext;
use crate::{Shared, now_ms};

pub const ROUTE: &str = "/multiplayer/report";
/// The fixed reasons a report may give.
pub const REASONS: [&str; 3] = ["name", "griefing", "cheating"];
const FILE: &str = "reports.jsonl";
const OLD_FILE: &str = "reports.1.jsonl";
/// The file is rotated past this size, so at most twice it is kept.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Per client address: this many reports, then one per [`REFILL_MS`].
const BURST: u32 = 5;
const REFILL_MS: u64 = 60_000;
/// Addresses remembered before the oldest are forgotten.
const MAX_ADDRESSES: usize = 4096;

/// Per-address report budgets.
#[derive(Default)]
pub struct Reports {
    budgets: HashMap<IpAddr, (u32, u64)>,
}

impl Reports {
    /// Takes one report from `ip` at `now` if its budget allows.
    pub fn allow(&mut self, ip: IpAddr, now: u64) -> bool {
        if self.budgets.len() >= MAX_ADDRESSES && !self.budgets.contains_key(&ip) {
            self.budgets
                .retain(|_, (used, at)| *used > 0 && now.saturating_sub(*at) < REFILL_MS * 5);
            if self.budgets.len() >= MAX_ADDRESSES {
                return false;
            }
        }
        let (used, at) = self.budgets.entry(ip).or_insert((0, now));
        let refilled = (now.saturating_sub(*at) / REFILL_MS) as u32;
        if refilled > 0 {
            *used = used.saturating_sub(refilled);
            *at = now;
        }
        if *used >= BURST {
            return false;
        }
        *used += 1;
        true
    }
}

#[derive(Serialize)]
struct Line<'a> {
    t: u64,
    reason: &'a str,
    #[serde(flatten)]
    context: &'a ReportContext,
}

/// Appends one report, rotating the file once it passes [`MAX_FILE_BYTES`].
pub fn append(dir: &Path, t: u64, reason: &str, context: &ReportContext) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FILE);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() >= MAX_FILE_BYTES) {
        std::fs::rename(&path, dir.join(OLD_FILE))?;
    }
    let mut line = serde_json::to_vec(&Line { t, reason, context })?;
    line.push(b'\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&line)
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}

/// `{"room": "FREERD01", "key": "<rejoin key>", "target": 3, "reason": "name"}`.
pub async fn report(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if body.len() > 512 {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "Report too large");
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return error(StatusCode::BAD_REQUEST, "Invalid report");
    };
    let reason = value["reason"]
        .as_str()
        .and_then(|r| REASONS.iter().find(|&&x| x == r));
    let room = value["room"]
        .as_str()
        .and_then(|c| atv_net::parse_room_code(c).ok());
    let key = value["key"].as_str().filter(|k| k.len() == 32);
    let target = value["target"].as_u64().and_then(|t| u8::try_from(t).ok());
    let (Some(reason), Some(room), Some(key), Some(target)) = (reason, room, key, target) else {
        return error(StatusCode::BAD_REQUEST, "Invalid report");
    };
    let now = now_ms();
    let ip = crate::http::client_ip(peer, &headers);
    if !shared.reports.lock().expect("reports lock").allow(ip, now) {
        return error(StatusCode::TOO_MANY_REQUESTS, "Too many reports");
    }
    let Some(handle) = shared.registry.get(&room) else {
        return error(StatusCode::NOT_FOUND, "Room ended");
    };
    let Some(context) = handle.report(key.to_string(), target).await else {
        return error(StatusCode::FORBIDDEN, "Not in that room");
    };
    let dir = shared.config.state_dir.clone();
    let reason = *reason;
    let written = tokio::task::spawn_blocking(move || append(&dir, now, reason, &context)).await;
    match written {
        Ok(Ok(())) => {
            tracing::info!(event = "report", reason);
            (StatusCode::OK, axum::Json(serde_json::json!({"ok": true}))).into_response()
        }
        _ => error(StatusCode::SERVICE_UNAVAILABLE, "Report not saved"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_reports_then_one_a_minute_per_address() {
        let mut r = Reports::default();
        let ip: IpAddr = "203.0.113.4".parse().unwrap();
        let other: IpAddr = "203.0.113.5".parse().unwrap();
        for _ in 0..BURST {
            assert!(r.allow(ip, 0));
        }
        assert!(!r.allow(ip, 59_999));
        assert!(r.allow(other, 0));
        assert!(r.allow(ip, 60_000));
        assert!(!r.allow(ip, 60_001));
    }

    #[test]
    fn the_file_rotates_at_its_cap() {
        let dir = std::env::temp_dir().join(format!("fury-reports-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let context = ReportContext {
            room: "FREERD01".into(),
            public: true,
            reporter: 0,
            reporter_name: "A".into(),
            target: 1,
            target_name: "B".into(),
            events: Vec::new(),
        };
        append(&dir, 1, "name", &context).unwrap();
        let line = std::fs::read_to_string(dir.join(FILE)).unwrap();
        let value: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(value["reason"], "name");
        assert_eq!(value["target_name"], "B");
        std::fs::write(dir.join(FILE), vec![b'x'; MAX_FILE_BYTES as usize]).unwrap();
        append(&dir, 2, "cheating", &context).unwrap();
        assert_eq!(
            std::fs::metadata(dir.join(OLD_FILE)).unwrap().len(),
            MAX_FILE_BYTES
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE))
                .unwrap()
                .lines()
                .count(),
            1
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
