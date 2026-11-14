//! One simulated rider: claims a room over HTTP, joins over the room
//! WebSocket, rides a loop at the game's adaptive cadence and counts what
//! the server sends back.
use crate::stats::Stats;
use crate::{Args, Frames};
use atv_net::{AnimationState, Lobby, Packet, Participant, Snapshot, signal};
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

/// A private room's map (a host starts it there); Free Ride's is the
/// server's.
const PRIVATE_MAP: u16 = 1;
/// A full pose at least this often (ms), deltas between, as the game sends.
const FULL_MS: u64 = 2000;
/// Riders ride near each other in groups of this many.
const CLUSTER: usize = 6;

/// splitmix64: reproducible paths without a dependency.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `low..high`.
    fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (self.next() >> 11) as f64 / (1u64 << 53) as f64 * (high - low)
    }
}

/// Where and how one rider rides.
#[derive(Clone, Debug)]
pub struct Plan {
    pub index: usize,
    /// Private room index; None rides Free Ride.
    pub room: Option<usize>,
    /// Creates its private room and starts it.
    pub host: bool,
    /// Rides near others (20 Hz while moving), or far apart (10 Hz).
    pub clustered: bool,
    center: [f64; 2],
    radius: f64,
    /// Feet per second while moving.
    speed: f64,
    phase: f64,
    /// Seconds moving, then seconds at rest, repeating.
    moving_s: f64,
    resting_s: f64,
    quad: u16,
}

impl Plan {
    /// Every rider's plan. Riders `0..rooms` host the private rooms; rider
    /// `i` rides in room `i % rooms`. Clustered riders ride loops in groups
    /// of six within about 60 ft of each other; the rest ride far apart.
    pub fn all(args: &Args) -> Vec<Plan> {
        let mut random = Random(args.seed);
        let clustered = (args.riders as f64 * args.clustered).round() as usize;
        let groups: Vec<[f64; 2]> = (0..clustered.div_ceil(CLUSTER))
            .map(|_| [random.range(-1500., 1500.), random.range(-1500., 1500.)])
            .collect();
        (0..args.riders)
            .map(|index| {
                // Spread the clustered riders over the riders (and so over
                // the rooms they fill).
                let near =
                    (index * clustered) / args.riders != ((index + 1) * clustered) / args.riders;
                let rank = ((index + 1) * clustered) / args.riders;
                let (center, radius) = if near {
                    let group = groups[(rank.max(1) - 1) / CLUSTER];
                    (
                        [
                            group[0] + random.range(-10., 10.),
                            group[1] + random.range(-10., 10.),
                        ],
                        random.range(15., 30.),
                    )
                } else {
                    (
                        [random.range(-3000., 3000.), random.range(-3000., 3000.)],
                        random.range(50., 150.),
                    )
                };
                Plan {
                    index,
                    room: (args.rooms > 0).then(|| index % args.rooms),
                    host: args.rooms > 0 && index < args.rooms,
                    clustered: near,
                    center,
                    radius,
                    speed: random.range(20., 45.),
                    phase: random.range(0., std::f64::consts::TAU),
                    moving_s: random.range(15., 40.),
                    resting_s: random.range(3., 10.),
                    quad: 1 + (index % 26) as u16,
                }
            })
            .collect()
    }

    /// Position, velocity and whether the rider moves, `t` seconds in.
    fn at(&self, t: f64) -> ([f32; 3], [f32; 3], bool) {
        let cycle = self.moving_s + self.resting_s;
        let into = t % cycle;
        let moving = into < self.moving_s;
        let ridden = (t / cycle).floor() * self.moving_s + into.min(self.moving_s);
        let angle = self.phase + ridden * self.speed / self.radius;
        let (sin, cos) = angle.sin_cos();
        let position = [
            (self.center[0] + self.radius * cos) as f32,
            0.,
            (self.center[1] + self.radius * sin) as f32,
        ];
        let velocity = if moving {
            [(-self.speed * sin) as f32, 0., (self.speed * cos) as f32]
        } else {
            [0.; 3]
        };
        (position, velocity, moving)
    }

    /// The game's adaptive cadence (research/MULTIPLAYER.md): 20 Hz near
    /// other riders, 10 Hz farther, 1 Hz at rest.
    fn interval_ms(&self, moving: bool) -> u64 {
        match (moving, self.clustered) {
            (false, _) => 1000,
            (true, true) => 50,
            (true, false) => 100,
        }
    }

    fn participant(&self) -> Participant {
        Participant {
            id: 0,
            name: format!("Load {}", self.index),
            atv_model: self.quad,
            rider_model: (self.index % 2) as u16,
            appearance: [0; atv_net::APPEARANCE_BYTES],
        }
    }
}

/// Private room codes, published by each room's host for its guests.
pub struct Rooms(Vec<watch::Sender<Option<Option<String>>>>);

impl Rooms {
    pub fn new(rooms: usize) -> Rooms {
        Rooms((0..rooms).map(|_| watch::channel(None).0).collect())
    }

    /// The host's code, or None when the host could not create the room.
    fn publish(&self, room: usize, code: Option<String>) {
        self.0[room].send_replace(Some(code));
    }

    async fn code(&self, room: usize) -> Option<String> {
        let mut receiver = self.0[room].subscribe();
        let code = receiver.wait_for(Option::is_some).await.ok()?.clone();
        code.flatten()
    }
}

/// A room claim: the code, the slot key, and the claim's JSON.
struct Claim {
    code: String,
    key: Option<String>,
    json: serde_json::Value,
}

fn post(url: &str, origin: &str, body: &str) -> Result<(u16, String), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .post(url)
        .header("Origin", origin)
        .header("Content-Type", "application/json")
        .send(body)
        .map_err(|e| format!("claim request: {e}"))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok((status, text))
}

/// Claims a Free Ride slot, creates the rider's private room (host) or opens
/// it (guest), as the game does.
async fn claim(plan: &Plan, args: &Args, rooms: &Rooms) -> Result<Claim, String> {
    let (url, body) = match plan.room {
        None => (format!("{}/multiplayer/freeride", args.url), String::new()),
        Some(_) if plan.host => (format!("{}/multiplayer/rooms", args.url), "{}".into()),
        Some(room) => {
            let code = rooms.code(room).await.ok_or("host could not create room")?;
            (
                format!("{}/multiplayer/rooms", args.url),
                serde_json::json!({ "code": code, "open": true }).to_string(),
            )
        }
    };
    let origin = args.origin.clone();
    let (status, text) = tokio::task::spawn_blocking(move || post(&url, &origin, &body))
        .await
        .map_err(|e| e.to_string())??;
    if !matches!(status, 200 | 201) {
        return Err(format!("claim status {status}"));
    }
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "claim is not JSON".to_string())?;
    let code = json["code"]
        .as_str()
        .ok_or("claim has no code")?
        .to_string();
    let key = json["key"]
        .as_str()
        .or(json["hostToken"].as_str())
        .map(String::from);
    Ok(Claim { code, key, json })
}

fn encode(packet: &Packet) -> Vec<u8> {
    atv_net::encode(packet).expect("valid load-test packet")
}

/// What a rider sends: poses with their sequence and delta baseline.
struct Poser {
    sequence: u32,
    baseline: Option<Snapshot>,
    last_full_ms: u64,
    last_ms: Option<u64>,
    moving: bool,
}

impl Poser {
    fn new() -> Poser {
        Poser {
            sequence: 0,
            baseline: None,
            last_full_ms: 0,
            last_ms: None,
            moving: true,
        }
    }

    /// The pose due `now_ms` into the ride, if any: a compact snapshot, or
    /// a delta on the last one sent.
    fn due(&mut self, plan: &Plan, peer: u8, now_ms: u64) -> Option<Vec<u8>> {
        let (position, velocity, moving) = plan.at(now_ms as f64 / 1000.);
        let changed = moving != self.moving;
        if !changed
            && self
                .last_ms
                .is_some_and(|last| now_ms - last < plan.interval_ms(moving))
        {
            return None;
        }
        self.moving = moving;
        self.last_ms = Some(now_ms);
        self.sequence = self.sequence.wrapping_add(1);
        let participant = plan.participant();
        let speed = velocity.iter().map(|v| v * v).sum::<f32>().sqrt();
        let mut s = Snapshot {
            animation: Some(Box::new(AnimationState {
                paused: false,
                ..Default::default()
            })),
            peer,
            sequence: self.sequence,
            time_ms: now_ms as u32,
            atv_model: participant.atv_model,
            rider_model: participant.rider_model,
            velocity,
            rider_velocity: velocity,
            speed,
            rpm: if moving { 6000. } else { 1200. },
            throttle: if moving { 0.6 } else { 0. },
            ..Default::default()
        };
        s.atv.position = position;
        s.rider.position = [position[0], position[1] + 2., position[2]];
        let full = changed || now_ms - self.last_full_ms >= FULL_MS;
        let patch = self
            .baseline
            .as_ref()
            .filter(|_| !full)
            .and_then(|base| atv_net::delta(&s, base).ok())
            .filter(|patch| patch.mask != 15);
        let bytes = match patch {
            Some(patch) => encode(&Packet::Delta(Box::new(patch))),
            None => {
                self.last_full_ms = now_ms;
                encode(&Packet::Snapshot(Box::new(s.clone())))
            }
        };
        self.baseline = Some(s);
        Some(bytes)
    }
}

/// Rides until `stop`, counting into `stats`.
pub async fn ride(
    plan: Plan,
    args: &Args,
    stats: &Stats,
    rooms: &Rooms,
    stop: watch::Receiver<bool>,
) {
    let claim = match claim(&plan, args, rooms).await {
        Ok(claim) => claim,
        Err(reason) => {
            if plan.host
                && let Some(room) = plan.room
            {
                rooms.publish(room, None);
            }
            Stats::add(&stats.connect_failed, 1);
            stats.reason(reason);
            return;
        }
    };
    if plan.host
        && let Some(room) = plan.room
    {
        rooms.publish(room, Some(claim.code.clone()));
    }
    if let Ok(mut first) = stats.claim.lock()
        && first.is_none()
    {
        let flag = |name: &str| claim.json[name].as_bool() == Some(true);
        *first = Some(serde_json::json!({
            "links": flag("links"), "batches": flag("batches"), "frames": flag("frames"),
        }));
    }
    match session(&plan, args, stats, &claim, stop).await {
        Ok(()) => {}
        Err(Ended::Refused(reason)) => {
            Stats::add(&stats.connect_failed, 1);
            stats.reason(reason);
        }
        Err(Ended::Dropped(reason)) => {
            Stats::add(&stats.disconnects, 1);
            stats.reason(reason);
        }
    }
}

enum Ended {
    /// Never admitted.
    Refused(String),
    /// Admitted, then closed before the end.
    Dropped(String),
}

async fn session(
    plan: &Plan,
    args: &Args,
    stats: &Stats,
    claim: &Claim,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Ended> {
    let base = args
        .url
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let url = match &claim.key {
        Some(key) => format!("{base}/multiplayer/rooms/{}?key={key}", claim.code),
        None => format!("{base}/multiplayer/rooms/{}", claim.code),
    };
    let refused = |e: &dyn std::fmt::Display| Ended::Refused(format!("connect: {e}"));
    let mut request = url.into_client_request().map_err(|e| refused(&e))?;
    request
        .headers_mut()
        .insert("Origin", args.origin.parse().map_err(|e| refused(&e))?);
    let socket = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| Ended::Refused("connect: timed out".into()))?
    .map_err(|e| refused(&e))?
    .0;
    let (mut sink, mut stream) = socket.split();
    let clock = Instant::now();
    let now_ms = || clock.elapsed().as_millis() as u64;
    let code = atv_net::parse_room_code(&claim.code)
        .map_err(|_| Ended::Refused("claim code is not a room code".into()))?;
    let flag = |name: &str| claim.json[name].as_bool() == Some(true);
    let frames = match args.frames {
        Frames::Auto => flag("frames"),
        Frames::On => true,
        Frames::Off => false,
    };
    let mut out = vec![
        encode(&Packet::Join {
            code,
            player: plan.participant(),
        }),
        encode(&Packet::Ping {
            nonce: now_ms() as u32,
        }),
    ];
    let mut peer: Option<u8> = None;
    let mut lobby: Option<Lobby> = None;
    let mut set_up = false;
    let mut ready = false;
    let mut last_ping = 0;
    let mut poser = Poser::new();
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let admitted = |peer: Option<u8>, reason: String| {
        if peer.is_some() {
            Ended::Dropped(reason)
        } else {
            Ended::Refused(reason)
        }
    };
    loop {
        for bytes in out.drain(..) {
            Stats::add(&stats.messages_out, 1);
            Stats::add(&stats.bytes_out, bytes.len() as u64);
            if let Err(e) = sink.send(Message::binary(bytes)).await {
                if peer.is_some() {
                    stats
                        .live
                        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                }
                return Err(admitted(peer, format!("send: {e}")));
            }
        }
        tokio::select! {
            _ = stop.changed() => {
                let _ = sink.send(Message::Close(None)).await;
                if peer.is_some() {
                    stats.live.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                }
                return Ok(());
            }
            message = stream.next() => {
                let reason = match message {
                    Some(Ok(Message::Binary(bytes))) => {
                        Stats::add(&stats.messages_in, 1);
                        Stats::add(&stats.bytes_in, bytes.len() as u64);
                        match atv_net::decode(&bytes) {
                            Ok(Packet::Welcome { peer: id, lobby: l, .. }) => {
                                if peer.is_none() {
                                    Stats::add(&stats.connect_ok, 1);
                                    Stats::add(&stats.live, 1);
                                }
                                peer = Some(id);
                                lobby = Some(l);
                            }
                            Ok(Packet::Lobby(l)) => lobby = Some(l),
                            Ok(Packet::Snapshot(_) | Packet::Delta(_)) => {
                                Stats::add(&stats.poses_in, 1);
                            }
                            Ok(Packet::Batch { poses, .. }) => {
                                Stats::add(&stats.batches_in, 1);
                                Stats::add(&stats.poses_in, poses.len() as u64);
                            }
                            Ok(Packet::Bundle { poses }) => {
                                Stats::add(&stats.poses_in, poses.len() as u64);
                            }
                            Ok(Packet::Frame { poses }) => {
                                Stats::add(&stats.frames_in, 1);
                                Stats::add(&stats.poses_in, poses.len() as u64);
                            }
                            Ok(Packet::Pong { nonce, .. }) => {
                                stats.rtt((now_ms() as u32).wrapping_sub(nonce));
                            }
                            Ok(_) => {}
                            Err(e) => stats.reason(format!("undecodable message: {e:?}")),
                        }
                        None
                    }
                    Some(Ok(Message::Close(frame))) => Some(match frame {
                        Some(f) => format!("closed {} {}", u16::from(f.code), f.reason),
                        None => "closed".into(),
                    }),
                    Some(Ok(_)) => None,
                    Some(Err(e)) => Some(format!("socket: {e}")),
                    None => Some("socket ended".into()),
                };
                if let Some(reason) = reason {
                    if peer.is_some() {
                        stats.live.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    return Err(admitted(peer, reason));
                }
            }
            _ = tick.tick() => {
                let now = now_ms();
                if peer.is_none() && now > 10_000 {
                    return Err(Ended::Refused("not admitted in 10 s".into()));
                }
                if now - last_ping >= args.ping_ms {
                    last_ping = now;
                    out.push(encode(&Packet::Ping { nonce: now as u32 }));
                }
                let (Some(id), Some(l)) = (peer, &lobby) else { continue };
                // A new private room waits for its host to start it.
                if !l.started {
                    if plan.host && !set_up && l.host == id {
                        set_up = true;
                        out.push(encode(&Packet::SetLobby {
                            epoch: l.epoch,
                            map: PRIVATE_MAP,
                            started: true,
                        }));
                    }
                    continue;
                }
                if !ready {
                    ready = true;
                    out.push(encode(&Packet::Ready { map: l.map }));
                    // The hello the game sends a server that takes one
                    // (desktop flags: no direct links).
                    if flag("links") || flag("frames") || frames {
                        let mut flags = signal::CPU_SOUNDS;
                        if flag("batches") {
                            flags |= signal::BATCHES;
                        }
                        if frames {
                            flags |= signal::FRAMES;
                            Stats::add(&stats.said_frames, 1);
                        }
                        out.push(encode(&Packet::Signal(atv_net::Signal {
                            peer: id,
                            kind: signal::HELLO,
                            data: vec![flags],
                        })));
                    }
                }
                if let Some(bytes) = poser.due(plan, id, now) {
                    out.push(bytes);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(riders: usize, rooms: usize, clustered: f64) -> Args {
        Args {
            url: "http://127.0.0.1:8789".into(),
            origin: "http://127.0.0.1:8789".into(),
            riders,
            rooms,
            duration: 1.,
            ramp: 0.,
            frames: Frames::Auto,
            clustered,
            ping_ms: 5000,
            seed: 7,
            out: None,
        }
    }

    #[test]
    fn plans_cluster_some_riders_and_host_each_room() {
        let plans = Plan::all(&args(12, 3, 0.5));
        assert_eq!(plans.iter().filter(|p| p.clustered).count(), 6);
        assert_eq!(plans.iter().filter(|p| p.host).count(), 3);
        assert!(plans.iter().all(|p| p.room == Some(p.index % 3)));
        // Clustered riders ride within about 60 ft of each other.
        let near: Vec<_> = plans.iter().filter(|p| p.clustered).collect();
        for a in &near {
            for b in &near {
                let (pa, pb) = (a.at(3.).0, b.at(3.).0);
                let d = ((pa[0] - pb[0]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt();
                assert!(d < 90., "{d}");
            }
        }
        assert!(
            Plan::all(&args(4, 0, 0.))
                .iter()
                .all(|p| p.room.is_none() && !p.host)
        );
    }

    #[test]
    fn poses_follow_the_cadence_and_decode() {
        let plan = Plan::all(&args(2, 0, 1.)).remove(0);
        let mut poser = Poser::new();
        let mut sent = Vec::new();
        for now in (0..2000).step_by(50) {
            if let Some(bytes) = poser.due(&plan, 3, now) {
                sent.push(atv_net::decode(&bytes).unwrap());
            }
        }
        // Clustered and moving: 20 Hz, a full pose then deltas or full ones.
        assert_eq!(sent.len(), 40);
        assert!(matches!(&sent[0], Packet::Snapshot(s) if s.peer == 3 && s.animation.is_some()));
        // At rest: once a second.
        let rest = plan.moving_s + 0.1;
        let mut poser = Poser::new();
        let at_rest = (0..2000)
            .step_by(50)
            .filter(|t| poser.due(&plan, 3, (rest * 1000.) as u64 + t).is_some())
            .count();
        assert_eq!(at_rest, 2);
    }
}
