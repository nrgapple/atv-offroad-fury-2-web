//! fury-server, the multiplayer game server (research/GAME_SERVER.md).
//! Authored Rust; original PS2 networking is not executed here.
//!
//! [`engine`] is one room as a pure state machine and [`interest`] decides
//! which poses each rider is due; [`rooms`] runs each room in its own task;
//! [`http`] holds the routes; [`run`] serves them with optional TLS and
//! saves rooms across restarts.

pub mod admission;
pub mod config;
pub mod engine;
pub mod http;
pub mod interest;
pub mod outbox;
pub mod rooms;
pub mod stats;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::{Config, Tls};
use crate::engine::{Room, SavedRoom};

/// Wall clock in ms since the Unix epoch: room times and the server time
/// riders share their world clocks from.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub struct Shared {
    pub config: Config,
    pub started: Instant,
    pub metrics: stats::Metrics,
    pub usage: Mutex<stats::Usage>,
    pub registry: rooms::Registry,
    pub admission: Mutex<admission::Admission>,
    pub next_conn: AtomicU64,
    pub shutting_down: AtomicBool,
}

impl Shared {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Shared {
            config,
            started: Instant::now(),
            metrics: stats::Metrics::default(),
            usage: Mutex::default(),
            registry: rooms::Registry::default(),
            admission: Mutex::default(),
            next_conn: AtomicU64::new(1),
            shutting_down: AtomicBool::new(false),
        })
    }
}

const ROOMS_FILE: &str = "rooms.json";
const USAGE_FILE: &str = "usage.json";
/// Daily usage is saved this often as well as at shutdown.
const USAGE_SAVE: Duration = Duration::from_secs(300);
/// Shutdown waits at most this long for rooms to save and sockets to close
/// (systemd's default stop timeout is far longer; the design asks for 10 s).
const SHUTDOWN_WAIT: Duration = Duration::from_secs(6);

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SavedRooms {
    saved_ms: u64,
    commit: String,
    rooms: Vec<SavedRoom>,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

/// Opens the rooms saved at the last shutdown, with their riders' slots
/// held from now, and loads the daily usage. The rooms file is removed once
/// read, so a crash later never brings back stale rooms.
pub fn restore(shared: &Arc<Shared>) -> usize {
    let dir = &shared.config.state_dir;
    if let Ok(bytes) = std::fs::read(dir.join(USAGE_FILE)) {
        match serde_json::from_slice(&bytes) {
            Ok(usage) => *shared.usage.lock().expect("usage lock") = usage,
            Err(e) => tracing::warn!(event = "usage_unreadable", error = %e),
        }
    }
    let path = dir.join(ROOMS_FILE);
    let Ok(bytes) = std::fs::read(&path) else {
        return 0;
    };
    let _ = std::fs::remove_file(&path);
    let saved: SavedRooms = match serde_json::from_slice(&bytes) {
        Ok(saved) => saved,
        Err(e) => {
            tracing::warn!(event = "rooms_unreadable", error = %e);
            return 0;
        }
    };
    let now = now_ms();
    // Rooms saved longer ago than the hold have nobody left to come back.
    if now.saturating_sub(saved.saved_ms) > engine::GRACE_MS * 4 {
        tracing::warn!(event = "rooms_stale", saved_ms = saved.saved_ms);
        return 0;
    }
    let mut restored = 0;
    for room in saved.rooms {
        if let Some(room) = Room::restore(room, now)
            && shared.registry.open(shared, room).is_some()
        {
            restored += 1;
        }
    }
    tracing::info!(event = "rooms_restored", rooms = restored);
    restored
}

fn save_usage(shared: &Shared) {
    let usage = serde_json::to_vec(&*shared.usage.lock().expect("usage lock"));
    if let Ok(bytes) = usage
        && let Err(e) = write_atomic(&shared.config.state_dir.join(USAGE_FILE), &bytes)
    {
        tracing::warn!(event = "usage_save_failed", error = %e);
    }
}

/// Stops every room, saves them, and waits briefly for sockets to close
/// (each gets 1012 so its client reconnects). Returns the rooms saved.
pub async fn shut_down(shared: &Arc<Shared>) -> usize {
    shared.shutting_down.store(true, Ordering::Relaxed);
    let deadline = Instant::now() + SHUTDOWN_WAIT;
    let rooms = shared.registry.all();
    let saves = rooms.iter().map(|room| room.shut_down());
    let saved: Vec<SavedRoom> = tokio::time::timeout(
        Duration::from_secs(3),
        futures_util::future::join_all(saves),
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .flatten()
    .collect();
    let count = saved.len();
    let file = SavedRooms {
        saved_ms: now_ms(),
        commit: shared.config.commit.clone(),
        rooms: saved,
    };
    match serde_json::to_vec(&file) {
        Ok(bytes) => {
            if let Err(e) = write_atomic(&shared.config.state_dir.join(ROOMS_FILE), &bytes) {
                tracing::error!(event = "rooms_save_failed", error = %e);
            }
        }
        Err(e) => tracing::error!(event = "rooms_save_failed", error = %e),
    }
    save_usage(shared);
    while shared.metrics.connections.load(Ordering::Relaxed) > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tracing::info!(event = "shutdown", rooms = count);
    count
}

/// Serves `listener` until `stop` turns true, then shuts down.
pub async fn serve(
    shared: Arc<Shared>,
    listener: TcpListener,
    mut stop: watch::Receiver<bool>,
) -> std::io::Result<()> {
    std::fs::create_dir_all(&shared.config.state_dir)?;
    let tls = tls(&shared.config.tls)?;
    let router = http::router(shared.clone());
    let saver = {
        let shared = shared.clone();
        tokio::spawn(async move {
            let mut every = tokio::time::interval(USAGE_SAVE);
            every.tick().await;
            loop {
                every.tick().await;
                save_usage(&shared);
            }
        })
    };
    tracing::info!(event = "listening", address = %listener.local_addr()?, commit = %shared.config.commit);
    loop {
        let (tcp, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(accepted) => accepted,
                Err(e) => {
                    tracing::warn!(event = "accept_failed", error = %e);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
            // (The watch guard is not Send: drop it inside.)
            () = async { let _ = stop.wait_for(|stop| *stop).await; } => break,
        };
        let _ = tcp.set_nodelay(true);
        let router = router.clone();
        let tls = tls.clone();
        tokio::spawn(async move {
            match tls {
                TlsAcceptor::Off => connection(tcp, peer, router).await,
                TlsAcceptor::Files(config) => {
                    let acceptor = tokio_rustls::TlsAcceptor::from(config);
                    if let Ok(Ok(stream)) =
                        tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await
                    {
                        connection(stream, peer, router).await;
                    }
                }
                TlsAcceptor::Acme { challenge, default } => {
                    let start = tokio_rustls::LazyConfigAcceptor::new(Default::default(), tcp);
                    let Ok(Ok(start)) = tokio::time::timeout(Duration::from_secs(10), start).await
                    else {
                        return;
                    };
                    if rustls_acme::is_tls_alpn_challenge(&start.client_hello()) {
                        if let Ok(mut stream) = start.into_stream(challenge).await {
                            let _ = tokio::io::AsyncWriteExt::shutdown(&mut stream).await;
                        }
                    } else if let Ok(stream) = start.into_stream(default).await {
                        connection(stream, peer, router).await;
                    }
                }
            }
        });
    }
    drop(listener);
    shut_down(&shared).await;
    saver.abort();
    Ok(())
}

/// One HTTP/1.1 connection (WebSocket upgrades included).
async fn connection<IO>(io: IO, peer: SocketAddr, router: axum::Router)
where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use tower::ServiceExt;
    let service = router.map_request(
        move |mut request: axum::extract::Request<hyper::body::Incoming>| {
            request
                .extensions_mut()
                .insert(axum::extract::ConnectInfo(peer));
            request
        },
    );
    let service = hyper_util::service::TowerToHyperService::new(service);
    let _ = hyper::server::conn::http1::Builder::new()
        .timer(hyper_util::rt::TokioTimer::new())
        .header_read_timeout(Duration::from_secs(10))
        .serve_connection(hyper_util::rt::TokioIo::new(io), service)
        .with_upgrades()
        .await;
}

#[derive(Clone)]
enum TlsAcceptor {
    Off,
    Files(Arc<rustls::ServerConfig>),
    Acme {
        challenge: Arc<rustls::ServerConfig>,
        default: Arc<rustls::ServerConfig>,
    },
}

fn tls(tls: &Tls) -> std::io::Result<TlsAcceptor> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    let invalid = |e: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, e);
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = || {
        rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| invalid(e.to_string()))
    };
    Ok(match tls {
        Tls::Off => TlsAcceptor::Off,
        Tls::Files { cert, key } => {
            let certs = CertificateDer::pem_file_iter(cert)
                .and_then(Iterator::collect::<Result<Vec<_>, _>>)
                .map_err(|e| invalid(format!("FURY_TLS_CERT: {e}")))?;
            let key = PrivateKeyDer::from_pem_file(key)
                .map_err(|e| invalid(format!("FURY_TLS_KEY: {e}")))?;
            let mut config = builder()?
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .map_err(|e| invalid(e.to_string()))?;
            config.alpn_protocols = vec![b"http/1.1".to_vec()];
            TlsAcceptor::Files(Arc::new(config))
        }
        Tls::Acme {
            domains,
            email,
            cache,
        } => {
            use futures_util::StreamExt;
            std::fs::create_dir_all(cache)?;
            let mut state = rustls_acme::AcmeConfig::new(domains)
                .contact(email.iter().map(|e| format!("mailto:{e}")))
                .cache(rustls_acme::caches::DirCache::new(cache.clone()))
                .directory_lets_encrypt(true)
                .state();
            let challenge = state.challenge_rustls_config();
            let mut default = builder()?
                .with_no_client_auth()
                .with_cert_resolver(state.resolver());
            default.alpn_protocols = vec![b"http/1.1".to_vec()];
            tokio::spawn(async move {
                while let Some(event) = state.next().await {
                    match event {
                        Ok(ok) => tracing::info!(event = "acme", detail = ?ok),
                        Err(err) => tracing::error!(event = "acme_failed", error = ?err),
                    }
                }
            });
            TlsAcceptor::Acme {
                challenge,
                default: Arc::new(default),
            }
        }
    })
}

/// Waits for SIGTERM or SIGINT.
pub async fn terminated() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
