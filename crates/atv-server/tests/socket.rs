//! fury-server over real sockets: two WebSocket riders in a Free Ride room,
//! a frame between them, the HTTP routes, and a restart that keeps the room.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use atv_net::{Packet, Participant, Snapshot, decode, encode, signal};
use atv_server::config::Config;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest, protocol::CloseFrame};

const ORIGIN: &str = "http://127.0.0.1:4187";
type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

struct Server {
    address: SocketAddr,
    stop: watch::Sender<bool>,
    done: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn start(state: &std::path::Path) -> Server {
    start_with(state, &[]).await
}

async fn start_with(state: &std::path::Path, extra: &[(&str, &str)]) -> Server {
    let state = state.to_string_lossy().into_owned();
    let config = Config::from_vars(|name| match name {
        "FURY_LISTEN" => Some("127.0.0.1:0".into()),
        "FURY_STATE_DIR" => Some(state.clone()),
        "FURY_COMMIT" => Some("test-commit".into()),
        _ => extra
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.to_string()),
    })
    .unwrap();
    let listener = tokio::net::TcpListener::bind(config.listen).await.unwrap();
    let address = listener.local_addr().unwrap();
    let shared = atv_server::Shared::new(config);
    std::fs::create_dir_all(&shared.config.state_dir).unwrap();
    atv_server::restore(&shared);
    let (stop, stopped) = watch::channel(false);
    let done = tokio::spawn(atv_server::serve(shared, listener, stopped));
    Server {
        address,
        stop,
        done,
    }
}

/// One HTTP/1.1 request; the status and body.
async fn http(
    address: SocketAddr,
    method: &str,
    path: &str,
    origin: Option<&str>,
    body: &str,
) -> (u16, String) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let origin = origin
        .map(|o| format!("Origin: {o}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (status, body)
}

/// A CORS preflight for a POST from `origin`; the status and lowercased
/// response headers.
async fn preflight(address: SocketAddr, path: &str, origin: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "OPTIONS {path} HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: content-type\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let head = response
        .split_once("\r\n\r\n")
        .map_or(response.as_str(), |(h, _)| h);
    (status, head.to_ascii_lowercase())
}

async fn connect(address: SocketAddr, code: &str, key: &str) -> Socket {
    connect_from(address, code, key, None).await
}

/// Connects as a local proxy forwarding for `forwarded`, when given.
async fn connect_from(
    address: SocketAddr,
    code: &str,
    key: &str,
    forwarded: Option<&str>,
) -> Socket {
    let mut request = format!("ws://{address}/multiplayer/rooms/{code}?key={key}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    if let Some(forwarded) = forwarded {
        request
            .headers_mut()
            .insert("X-Forwarded-For", forwarded.parse().unwrap());
    }
    tokio_tungstenite::connect_async(request).await.unwrap().0
}

async fn send(socket: &mut Socket, packet: &Packet) {
    socket
        .send(Message::Binary(encode(packet).unwrap().into()))
        .await
        .unwrap();
}

/// The next packet matching `want`, skipping others; panics after 3 s.
async fn expect(socket: &mut Socket, want: impl Fn(&Packet) -> bool) -> Packet {
    let wait = async {
        while let Some(message) = socket.next().await {
            if let Message::Binary(bytes) = message.unwrap() {
                let packet = decode(&bytes).unwrap();
                if want(&packet) {
                    return packet;
                }
            }
        }
        panic!("socket ended");
    };
    tokio::time::timeout(Duration::from_secs(3), wait)
        .await
        .expect("packet in time")
}

async fn close_frame(socket: &mut Socket) -> Option<CloseFrame> {
    let wait = async {
        while let Some(message) = socket.next().await {
            if let Ok(Message::Close(frame)) = message {
                return frame;
            }
        }
        None
    };
    tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .expect("close in time")
}

async fn join(socket: &mut Socket, code: &str, name: &str) -> u8 {
    send(
        socket,
        &Packet::Join {
            code: code.as_bytes().try_into().unwrap(),
            player: Participant {
                id: 0,
                name: name.into(),
                atv_model: 5,
                rider_model: 0,
                appearance: [0; 8],
            },
        },
    )
    .await;
    match expect(socket, |p| matches!(p, Packet::Welcome { .. })).await {
        Packet::Welcome { peer, .. } => peer,
        _ => unreachable!(),
    }
}

fn pose(peer: u8, sequence: u32) -> Packet {
    Packet::Snapshot(Box::new(Snapshot {
        peer,
        sequence,
        time_ms: sequence * 50,
        atv_model: 5,
        rider_model: 0,
        velocity: [20., 0., 0.],
        animation: Some(Box::default()),
        ..Default::default()
    }))
}

fn claim_of(body: &str) -> (String, String) {
    let value: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(value["links"], false);
    assert_eq!(value["batches"], false);
    assert_eq!(value["frames"], true);
    assert_eq!(value["motorcycles"], true);
    (
        value["code"].as_str().unwrap().to_string(),
        value["key"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn riders_meet_in_free_ride_get_frames_and_survive_a_restart() {
    let state = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("fury-socket-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let server = start(&state).await;
    let address = server.address;

    // Routes: health has its exact shape; rooms need an allowed Origin.
    let (status, body) = http(address, "GET", "/healthz", None, "").await;
    assert_eq!(status, 200);
    let health: serde_json::Value = serde_json::from_str(&body).unwrap();
    let keys: Vec<&str> = health
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = [
        "ok",
        "commit",
        "protocol",
        "uptimeSeconds",
        "rooms",
        "riders",
        "connections",
    ];
    expected.sort();
    assert_eq!(keys, expected);
    assert!(body.starts_with(r#"{"ok":true,"commit":"test-commit","protocol":6,"uptimeSeconds":"#));
    assert_eq!(
        http(address, "POST", "/multiplayer/freeride", None, "")
            .await
            .0,
        403
    );
    assert_eq!(
        http(
            address,
            "POST",
            "/multiplayer/freeride",
            Some("https://evil.example"),
            ""
        )
        .await
        .0,
        403
    );
    // Cross-origin pages preflight their JSON POSTs: allowed origins are
    // echoed, without credentials; others are refused.
    for path in ["/multiplayer/rooms", "/multiplayer/freeride"] {
        let (status, head) = preflight(address, path, ORIGIN).await;
        assert_eq!(status, 204, "{path}: {head}");
        for header in [
            "access-control-allow-origin: http://127.0.0.1:4187",
            "access-control-allow-methods: get, post, options",
            "access-control-allow-headers: content-type",
            "vary: origin",
        ] {
            assert!(head.contains(header), "{path}: {header} in {head}");
        }
        assert!(!head.contains("access-control-allow-credentials"), "{head}");
        assert_eq!(
            preflight(address, path, "https://evil.example").await.0,
            403
        );
    }
    let (status, body) = http(address, "GET", "/multiplayer/version", Some(ORIGIN), "").await;
    assert_eq!(status, 200);
    assert!(body.contains(r#""commit":"test-commit""#), "{body}");
    let (status, body) = http(address, "GET", "/metrics", None, "").await;
    assert_eq!(status, 200);
    assert!(body.contains("fury_connections 0"));

    // A private room: created, then taken, then opened by an invite link.
    let (status, body) = http(
        address,
        "POST",
        "/multiplayer/rooms",
        Some(ORIGIN),
        r#"{"code":"party234"}"#,
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let created: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        (created["code"].as_str(), created["frames"].as_bool()),
        (Some("PARTY234"), Some(true))
    );
    assert_eq!(created["hostToken"].as_str().map(str::len), Some(32));
    let taken = http(
        address,
        "POST",
        "/multiplayer/rooms",
        Some(ORIGIN),
        r#"{"code":"PARTY234"}"#,
    )
    .await;
    assert_eq!(taken.0, 409);
    let (status, body) = http(
        address,
        "POST",
        "/multiplayer/rooms",
        Some(ORIGIN),
        r#"{"code":"PARTY234","open":true}"#,
    )
    .await;
    assert_eq!(status, 200);
    assert!(
        body.contains(r#""open":true"#) && body.contains(r#""max":4"#),
        "{body}"
    );
    assert_eq!(
        http(
            address,
            "POST",
            "/multiplayer/rooms",
            Some(ORIGIN),
            r#"{"code":"FREERD01"}"#
        )
        .await
        .0,
        400
    );

    // Two Free Ride claims land in the same shared room.
    let (code, key_a) = claim_of(
        &http(address, "POST", "/multiplayer/freeride", Some(ORIGIN), "")
            .await
            .1,
    );
    let (code_b, key_b) = claim_of(
        &http(address, "POST", "/multiplayer/freeride", Some(ORIGIN), "")
            .await
            .1,
    );
    assert_eq!((code.as_str(), code_b.as_str()), ("FREERD01", "FREERD01"));
    let mut a = connect(address, &code, &key_a).await;
    let mut b = connect(address, &code, &key_b).await;
    let peer_a = join(&mut a, &code, "Alpha").await;
    let peer_b = join(&mut b, &code, "Bravo").await;
    assert_ne!(peer_a, peer_b);
    // B takes frames; A is an older build.
    send(
        &mut b,
        &Packet::Signal(atv_net::Signal {
            peer: peer_b,
            kind: signal::HELLO,
            data: vec![signal::FRAMES | signal::CPU_SOUNDS],
        }),
    )
    .await;
    for socket in [&mut a, &mut b] {
        send(
            socket,
            &Packet::Ready {
                map: atv_net::PUBLIC_MAP,
            },
        )
        .await;
    }
    // Ping answers with the server's clock.
    send(&mut b, &Packet::Ping { nonce: 42 }).await;
    let Packet::Pong { server_ms, .. } =
        expect(&mut b, |p| matches!(p, Packet::Pong { nonce: 42, .. })).await
    else {
        unreachable!()
    };
    assert!(server_ms > 1_700_000_000_000);
    // A's pose reaches B in a frame; B's reaches A on its own.
    send(&mut a, &pose(peer_a, 1)).await;
    let Packet::Frame { poses } = expect(&mut b, |p| matches!(p, Packet::Frame { .. })).await
    else {
        unreachable!()
    };
    assert!(
        matches!(decode(&poses[0]).unwrap(), Packet::Snapshot(s) if s.peer == peer_a && s.sequence == 1)
    );
    send(&mut b, &pose(peer_b, 1)).await;
    expect(
        &mut a,
        |p| matches!(p, Packet::Snapshot(s) if s.peer == peer_b),
    )
    .await;
    let (_, body) = http(address, "GET", "/healthz", None, "").await;
    assert!(body.contains(r#""connections":2"#), "{body}");

    // Restart: sockets close with 1012, the room is saved and comes back.
    server.stop.send(true).unwrap();
    for socket in [&mut a, &mut b] {
        let frame = close_frame(socket).await.expect("close frame");
        assert_eq!(u16::from(frame.code), 1012);
    }
    tokio::time::timeout(Duration::from_secs(10), server.done)
        .await
        .expect("shutdown within 10 s")
        .unwrap()
        .unwrap();
    let saved = std::fs::read_to_string(state.join("rooms.json")).unwrap();
    assert!(saved.contains("FREERD01") && saved.contains("PARTY234"));
    let server = start(&state).await;
    let address = server.address;
    let mut a = connect(address, &code, &key_a).await;
    assert_eq!(join(&mut a, &code, "Alpha").await, peer_a);
    // Bravo was held in the roster and takes its slot back.
    let mut b = connect(address, &code, &key_b).await;
    assert_eq!(join(&mut b, &code, "Bravo").await, peer_b);
    let Packet::Lobby(lobby) = expect(&mut a, |p| matches!(p, Packet::Lobby(_))).await else {
        unreachable!()
    };
    assert_eq!(lobby.players.len(), 2);
    server.stop.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(10), server.done).await;
    let _ = std::fs::remove_dir_all(&state);
}

/// Waits until `/metrics` shows `line` (rooms report every few seconds).
async fn metric(address: SocketAddr, line: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let (_, body) = http(address, "GET", "/metrics", None, "").await;
        if body.lines().any(|l| l == line) {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "{line} in {body}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn uncounted_addresses_ride_but_stay_out_of_the_public_counts() {
    let state = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("fury-uncounted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let server = start_with(&state, &[("FURY_UNCOUNTED_IPS", "198.51.100.0/24")]).await;
    let address = server.address;
    let public = |path: &'static str| async move {
        let (_, body) = http(address, "GET", path, None, "").await;
        serde_json::from_str::<serde_json::Value>(&body).unwrap()
    };

    // A tester behind the local proxy, at an uncounted address, rides alone.
    let (_, body) = http(address, "POST", "/multiplayer/freeride", Some(ORIGIN), "").await;
    let (code, key) = claim_of(&body);
    let mut tester = connect_from(address, &code, &key, Some("198.51.100.5")).await;
    let tester_peer = join(&mut tester, &code, "Tester").await;
    metric(address, "fury_riders_all 1").await;
    let health = public("/healthz").await;
    assert_eq!(
        (&health["riders"], &health["rooms"], &health["connections"]),
        (&0.into(), &0.into(), &0.into()),
        "{health}"
    );
    assert_eq!(public("/multiplayer/freeride/riders").await["riders"], 0);
    assert!(
        http(address, "GET", "/metrics", None, "")
            .await
            .1
            .contains("fury_uncounted_connections 1\n")
    );

    // A counted rider joins: the room and that rider count; the tester
    // still plays with them.
    let (_, body) = http(address, "POST", "/multiplayer/freeride", Some(ORIGIN), "").await;
    let (same, key) = claim_of(&body);
    assert_eq!(same, code);
    let mut rider = connect(address, &code, &key).await;
    join(&mut rider, &code, "Rider").await;
    metric(address, "fury_riders_all 2").await;
    metric(address, "fury_riders 1").await;
    let health = public("/healthz").await;
    assert_eq!(
        (&health["riders"], &health["rooms"], &health["connections"]),
        (&1.into(), &1.into(), &1.into()),
        "{health}"
    );
    assert_eq!(public("/multiplayer/freeride/riders").await["riders"], 1);
    let days = public("/multiplayer/freeride/days").await;
    assert_eq!(days["days"][0]["busiest"]["riders"], 1, "{days}");
    for socket in [&mut tester, &mut rider] {
        send(
            socket,
            &Packet::Ready {
                map: atv_net::PUBLIC_MAP,
            },
        )
        .await;
    }
    send(&mut tester, &pose(tester_peer, 1)).await;
    expect(
        &mut rider,
        |p| matches!(p, Packet::Snapshot(s) if s.peer == tester_peer),
    )
    .await;

    server.stop.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(10), server.done).await;
    let _ = std::fs::remove_dir_all(&state);
}
