//! Relay transport: a binary WebSocket and the room-creation POST, over
//! web-sys in the browser and a native client (tungstenite/ureq, rustls) on
//! desktop, plus the browser's direct links between riders ([`Mesh`]).
//! Bytes only; [`atv_game::multiplayer::Session`] owns the protocol.

/// What the socket reported since the last poll.
#[derive(Debug)]
pub enum Event {
    Open,
    /// A relay message and when it arrived (the browser stamps
    /// `performance.now()` in `onmessage`; poses and round trips use it).
    Message(Vec<u8>, web_time::Instant),
    Closed {
        code: u16,
        reason: String,
    },
}

/// What the direct links reported since the last poll (research/
/// MULTIPLAYER.md "Direct links"). Desktop builds have no links: every
/// rider's poses come through the relay.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Debug)]
pub enum MeshEvent {
    /// This end's session description for `peer`, to send through the relay.
    Local {
        peer: u8,
        answer: bool,
        description: Vec<u8>,
    },
    Open(u8),
    /// The link failed, closed or could not be made.
    Closed(u8),
    /// A message from `peer`'s link and when it arrived.
    Message(u8, Vec<u8>, web_time::Instant),
}

/// Outgoing bytes held beyond this are not queued (the browser checks
/// `bufferedAmount < 32768` before every send).
const BACKLOG: usize = 32768;
/// Relay messages are at most 8 KiB (web/multiplayer.js closes otherwise).
const MESSAGE_LIMIT: usize = 8192;
/// Received messages not yet taken by the session (web/multiplayer.js): pose
/// snapshots beyond `INBOX_POSES` are dropped (newer ones follow); anything
/// beyond `INBOX_LIMIT` means the session is stuck, and the socket closes.
const INBOX_POSES: usize = 192;
const INBOX_LIMIT: usize = 256;
/// The close reason for a stuck inbox (web/multiplayer.js, code 1008).
const BACKLOG_REASON: &str = "Control message backlog";

/// What happens to a received message with `queued` messages waiting.
#[derive(Debug, PartialEq)]
enum Admit {
    Queue,
    Drop,
    Close,
}

fn admit(queued: usize, bytes: &[u8]) -> Admit {
    if queued >= INBOX_POSES && is_pose(bytes) {
        Admit::Drop
    } else if queued >= INBOX_LIMIT {
        Admit::Close
    } else {
        Admit::Queue
    }
}

/// A pose snapshot by its header alone (web/multiplayer.js sniffs it; the
/// session still validates every admitted packet): `ATVM`, then protocol 2
/// kind 4 or protocol 3+ kind 9.
fn is_pose(bytes: &[u8]) -> bool {
    bytes.len() >= 8
        && bytes.starts_with(b"ATVM")
        && ((bytes[4] == 2 && bytes[5] == 4) || (bytes[4] >= 3 && bytes[5] == 9))
}

/// The multiplayer game server (research/GAME_SERVER.md).
pub const GAME_SERVER: &str = "https://mp.furyremastered.com";

/// The multiplayer origin (`http(s)://host[:port]`) for the game served on
/// `site`: `chosen` (the `?server=` page parameter, desktop `FURY_SERVER`)
/// when it is an http(s) origin, else [`GAME_SERVER`]. Only a page served
/// from this machine keeps its own origin: the local test servers, which
/// pass `/multiplayer/` on to a local fury-server
/// (tools/serve_multiplayer.mjs).
pub fn server_for(site: &str, chosen: Option<&str>) -> String {
    if let Some(chosen) = chosen
        .map(|c| c.trim().trim_end_matches('/'))
        .filter(|c| is_origin(c))
    {
        return chosen.to_string();
    }
    let site = origin(site);
    if is_loopback(&site) {
        site
    } else {
        GAME_SERVER.into()
    }
}

/// An http(s) origin on this machine (`127.x`, `localhost`, `[::1]`).
fn is_loopback(origin: &str) -> bool {
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let host = if rest.starts_with('[') {
        rest.split_inclusive(']').next().unwrap_or(rest)
    } else {
        rest.split(':').next().unwrap_or(rest)
    };
    host == "localhost"
        || host == "[::1]"
        || (host.starts_with("127.") && host.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
}

/// `http(s)://host[:port]` and nothing more.
fn is_origin(text: &str) -> bool {
    let Some(host) = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
    else {
        return false;
    };
    !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
}

/// `http(s)://host[:port]` to the matching `ws(s)://` room URL.
pub fn room_url(relay: &str, code: &str, key: Option<&str>) -> String {
    let base = relay.trim_end_matches('/');
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    match key {
        Some(key) => format!("{base}/multiplayer/rooms/{code}?key={key}"),
        None => format!("{base}/multiplayer/rooms/{code}"),
    }
}

/// A URL's origin: its scheme and host (the page origin a server admits).
pub fn origin(relay: &str) -> String {
    let base = relay.trim_end_matches('/');
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    match base[scheme_end..].find('/') {
        Some(i) => base[..scheme_end + i].to_string(),
        None => base.to_string(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Admit, BACKLOG, BACKLOG_REASON, Event, MESSAGE_LIMIT, admit};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;
    use tungstenite::{Message, client::IntoClientRequest, stream::MaybeTlsStream};

    enum Out {
        Send(Vec<u8>),
        Close,
        Drop,
    }

    /// A socket served by its own thread (reads with a short timeout so
    /// queued sends go out promptly).
    pub struct Socket {
        out: mpsc::Sender<Out>,
        events: mpsc::Receiver<Event>,
        queued: Arc<AtomicUsize>,
        /// Received messages the session has not polled yet.
        inbox: Arc<AtomicUsize>,
    }

    impl Socket {
        pub fn connect(url: &str, origin: &str) -> Socket {
            let (out_tx, out_rx) = mpsc::channel::<Out>();
            let (event_tx, events) = mpsc::channel();
            let queued = Arc::new(AtomicUsize::new(0));
            let backlog = queued.clone();
            let inbox = Arc::new(AtomicUsize::new(0));
            let waiting = inbox.clone();
            let (url, origin) = (url.to_string(), origin.to_string());
            std::thread::Builder::new()
                .name("relay socket".into())
                .spawn(move || {
                    let closed = |code, reason: String| Event::Closed { code, reason };
                    let mut request = match url.as_str().into_client_request() {
                        Ok(request) => request,
                        Err(e) => {
                            let _ = event_tx.send(closed(1006, e.to_string()));
                            return;
                        }
                    };
                    if let Ok(value) = origin.parse() {
                        request.headers_mut().insert("Origin", value);
                    }
                    let mut socket = match tungstenite::connect(request) {
                        Ok((socket, _)) => socket,
                        Err(e) => {
                            let _ = event_tx.send(closed(1006, e.to_string()));
                            return;
                        }
                    };
                    let timeout = Some(Duration::from_millis(4));
                    let _ = match socket.get_mut() {
                        MaybeTlsStream::Plain(s) => s.set_read_timeout(timeout),
                        MaybeTlsStream::Rustls(s) => s.sock.set_read_timeout(timeout),
                        _ => Ok(()),
                    };
                    let _ = event_tx.send(Event::Open);
                    loop {
                        let mut wrote = false;
                        loop {
                            let out = match out_rx.try_recv() {
                                Ok(out) => out,
                                Err(mpsc::TryRecvError::Empty) => break,
                                // The session dropped its socket: leave.
                                Err(mpsc::TryRecvError::Disconnected) => Out::Close,
                            };
                            match out {
                                Out::Send(bytes) => {
                                    backlog.fetch_sub(bytes.len(), Ordering::Relaxed);
                                    let _ = socket.write(Message::Binary(bytes.into()));
                                    wrote = true;
                                }
                                Out::Drop => return,
                                Out::Close => {
                                    let _ = socket.close(None);
                                    let _ = socket.flush();
                                    let _ = event_tx.send(closed(1000, "Left lobby".into()));
                                    return;
                                }
                            }
                        }
                        if wrote {
                            let _ = socket.flush();
                        }
                        match socket.read() {
                            Ok(Message::Binary(bytes)) => {
                                if bytes.len() > MESSAGE_LIMIT {
                                    let _ = socket.close(None);
                                    let _ = event_tx.send(closed(1008, "Message limit".into()));
                                    return;
                                }
                                match admit(waiting.load(Ordering::Relaxed), &bytes) {
                                    Admit::Queue => {}
                                    Admit::Drop => continue,
                                    Admit::Close => {
                                        let _ = socket.close(None);
                                        let _ = socket.flush();
                                        let _ = event_tx.send(closed(1008, BACKLOG_REASON.into()));
                                        return;
                                    }
                                }
                                waiting.fetch_add(1, Ordering::Relaxed);
                                if event_tx
                                    .send(Event::Message(bytes.to_vec(), web_time::Instant::now()))
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            Ok(Message::Close(frame)) => {
                                let (code, reason) = frame.map_or((1005, String::new()), |f| {
                                    (u16::from(f.code), f.reason.to_string())
                                });
                                let _ = event_tx.send(closed(code, reason));
                                return;
                            }
                            Ok(_) => {}
                            Err(tungstenite::Error::Io(e))
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) => {}
                            Err(e) => {
                                let _ = event_tx.send(closed(1006, e.to_string()));
                                return;
                            }
                        }
                    }
                })
                .expect("relay socket thread");
            Socket {
                out: out_tx,
                events,
                queued,
                inbox,
            }
        }

        /// Queues `bytes`; false when the socket is backlogged or gone.
        pub fn send(&self, bytes: Vec<u8>) -> bool {
            let queued = self.queued.load(Ordering::Relaxed);
            if queued >= BACKLOG {
                return false;
            }
            let length = bytes.len();
            self.queued.fetch_add(length, Ordering::Relaxed);
            if self.out.send(Out::Send(bytes)).is_err() {
                self.queued.fetch_sub(length, Ordering::Relaxed);
                return false;
            }
            true
        }

        pub fn poll(&self) -> Vec<Event> {
            let events: Vec<Event> = self.events.try_iter().collect();
            let messages = events
                .iter()
                .filter(|e| matches!(e, Event::Message(..)))
                .count();
            self.inbox.fetch_sub(messages, Ordering::Relaxed);
            events
        }

        pub fn close(&self) {
            let _ = self.out.send(Out::Close);
        }

        /// Test hook: the socket thread ends without a close handshake.
        pub fn drop_connection(&self) {
            let _ = self.out.send(Out::Drop);
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            self.close();
        }
    }

    /// POST a JSON body to the relay; the status and body text. The request
    /// runs on its own thread: the event loop keeps going while it waits.
    pub async fn post(url: &str, origin: &str, body: &str) -> Result<(u16, String), String> {
        let (url, origin, body) = (url.to_string(), origin.to_string(), body.to_string());
        off_thread(move || post_blocking(&url, &origin, &body)).await
    }

    fn post_blocking(url: &str, origin: &str, body: &str) -> Result<(u16, String), String> {
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
            .map_err(|e| format!("Unable to reach the multiplayer relay: {e}"))?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        Ok((status, text))
    }

    /// Resolves after `ms` without holding the caller's thread.
    pub async fn sleep(ms: u32) {
        off_thread(move || std::thread::sleep(Duration::from_millis(u64::from(ms)))).await;
    }

    /// Runs blocking `work` on its own thread; the future resolves with its
    /// result and wakes its task (the app's loop, or `pollster` in tools).
    pub(super) fn off_thread<T: Send + 'static>(
        work: impl FnOnce() -> T + Send + 'static,
    ) -> impl std::future::Future<Output = T> {
        use std::sync::Mutex;
        use std::task::{Poll, Waker};
        type Slot<T> = Arc<Mutex<(Option<T>, Option<Waker>)>>;
        let slot: Slot<T> = Arc::new(Mutex::new((None, None)));
        let job = Arc::new(Mutex::new(Some(work)));
        let run = {
            let (slot, job) = (slot.clone(), job.clone());
            move || {
                let work = job.lock().unwrap_or_else(|e| e.into_inner()).take();
                let value = work.map(|work| work());
                let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
                slot.0 = value;
                if let Some(waker) = slot.1.take() {
                    waker.wake();
                }
            }
        };
        let thread = std::thread::Builder::new().name("relay request".into());
        if thread.spawn(run).is_err() {
            // No thread to be had: do the work here rather than never.
            let work = job.lock().unwrap_or_else(|e| e.into_inner()).take();
            slot.lock().unwrap_or_else(|e| e.into_inner()).0 = work.map(|work| work());
        }
        std::future::poll_fn(move |cx| {
            let mut state = slot.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(value) = state.0.take() {
                return Poll::Ready(value);
            }
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        })
    }

    /// Desktop builds open no direct links.
    #[derive(Default)]
    pub struct Mesh;

    impl Mesh {
        pub fn offer(&mut self, _peer: u8) {}
        pub fn accept(&mut self, _peer: u8, _description: &[u8]) {}
        pub fn answer(&mut self, _peer: u8, _description: &[u8]) {}
        pub fn close(&mut self, _peer: u8) {}
        pub fn clear(&mut self) {}
        pub fn send(&self, _peer: u8, _bytes: &[u8]) -> bool {
            false
        }
        pub fn poll(&mut self) -> Vec<super::MeshEvent> {
            Vec::new()
        }
    }

    /// The Free Ride rider count (blocking; call off the UI thread).
    pub fn riders_blocking(server: &str) -> Option<super::FreeRide> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        let url = format!(
            "{}/multiplayer/freeride/riders",
            server.trim_end_matches('/')
        );
        let text = agent
            .get(&url)
            .call()
            .ok()?
            .body_mut()
            .read_to_string()
            .ok()?;
        super::riders_from(&text)
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{Admit, BACKLOG, BACKLOG_REASON, Event, MESSAGE_LIMIT, admit};
    use std::{
        cell::{Cell, RefCell},
        collections::{HashMap, VecDeque},
        rc::Rc,
    };
    use wasm_bindgen::{JsCast, closure::Closure};

    type Queue = Rc<RefCell<VecDeque<Event>>>;
    type Callbacks = (
        Closure<dyn FnMut()>,
        Closure<dyn FnMut(web_sys::MessageEvent)>,
        Closure<dyn FnMut(web_sys::CloseEvent)>,
    );

    /// A browser WebSocket whose callbacks queue events for the frame loop.
    pub struct Socket {
        socket: web_sys::WebSocket,
        events: Queue,
        /// The inbox overflowed: the socket closes at the next poll.
        stuck: Rc<Cell<bool>>,
        _callbacks: Callbacks,
    }

    impl Socket {
        pub fn connect(url: &str, _origin: &str) -> Socket {
            let events: Queue = Rc::default();
            let stuck: Rc<Cell<bool>> = Rc::default();
            let socket = web_sys::WebSocket::new(url);
            let open = {
                let events = events.clone();
                Closure::<dyn FnMut()>::new(move || events.borrow_mut().push_back(Event::Open))
            };
            let message = {
                let (events, stuck) = (events.clone(), stuck.clone());
                Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
                    if stuck.get() {
                        return;
                    }
                    if let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                        let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                        if bytes.len() > MESSAGE_LIMIT {
                            return;
                        }
                        let mut events = events.borrow_mut();
                        let queued = events
                            .iter()
                            .filter(|e| matches!(e, Event::Message(..)))
                            .count();
                        match admit(queued, &bytes) {
                            Admit::Queue => {
                                events.push_back(Event::Message(bytes, web_time::Instant::now()))
                            }
                            Admit::Drop => {}
                            Admit::Close => {
                                stuck.set(true);
                                events.push_back(Event::Closed {
                                    code: 1008,
                                    reason: BACKLOG_REASON.into(),
                                });
                            }
                        }
                    }
                })
            };
            let close = {
                let events = events.clone();
                Closure::<dyn FnMut(_)>::new(move |event: web_sys::CloseEvent| {
                    events.borrow_mut().push_back(Event::Closed {
                        code: event.code(),
                        reason: event.reason(),
                    })
                })
            };
            let socket = match socket {
                Ok(socket) => socket,
                Err(_) => {
                    events.borrow_mut().push_back(Event::Closed {
                        code: 1006,
                        reason: "Unable to connect to this lobby.".into(),
                    });
                    // An inert socket; poll reports the failure.
                    return Socket {
                        socket: web_sys::WebSocket::new("ws://0.0.0.0:9/")
                            .expect("placeholder socket"),
                        events,
                        stuck,
                        _callbacks: (open, message, close),
                    };
                }
            };
            socket.set_binary_type(web_sys::BinaryType::Arraybuffer);
            socket.set_onopen(Some(open.as_ref().unchecked_ref()));
            socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
            socket.set_onclose(Some(close.as_ref().unchecked_ref()));
            Socket {
                socket,
                events,
                stuck,
                _callbacks: (open, message, close),
            }
        }

        pub fn send(&self, bytes: Vec<u8>) -> bool {
            if self.socket.ready_state() != web_sys::WebSocket::OPEN
                || self.socket.buffered_amount() as usize >= BACKLOG
            {
                return false;
            }
            self.socket.send_with_u8_array(&bytes).is_ok()
        }

        pub fn poll(&self) -> Vec<Event> {
            if self.stuck.get() && self.socket.ready_state() <= web_sys::WebSocket::OPEN {
                // Browsers only let pages close with 1000 or 3000-4999; the
                // session already reported the 1008 backlog close.
                let _ = self
                    .socket
                    .close_with_code_and_reason(1000, "Control message backlog");
            }
            self.events.borrow_mut().drain(..).collect()
        }

        pub fn close(&self) {
            let _ = self.socket.close_with_code_and_reason(1000, "Left lobby");
        }

        /// An unclean-looking close (test hook): the relay holds the slot.
        pub fn drop_connection(&self) {
            let _ = self.socket.close_with_code_and_reason(4000, "Test drop");
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            // Leaving the ride (or replacing the socket) ends this connection.
            let _ = self.socket.close_with_code_and_reason(1000, "Left lobby");
            self.socket.set_onopen(None);
            self.socket.set_onmessage(None);
            self.socket.set_onclose(None);
        }
    }

    /// Public STUN servers: each browser learns its own public address from
    /// them; no rider traffic passes through them.
    const STUN: [&str; 2] = [
        "stun:stun.cloudflare.com:3478",
        "stun:stun.l.google.com:19302",
    ];
    /// The longest a description waits for its network candidates (ms): one
    /// description each way carries all of them.
    const GATHER_MS: f64 = 2000.;
    /// A link holding this much unsent data skips poses (newer ones follow).
    const LINK_BACKLOG: u32 = 16_384;

    enum Happened {
        Local(bool, Vec<u8>),
        Open,
        Closed,
        Message(Vec<u8>, web_time::Instant),
    }

    /// Link events by rider slot and link generation: events of a link that
    /// has since been replaced or closed are discarded at the next poll.
    type MeshQueue = Rc<RefCell<VecDeque<(u8, u32, Happened)>>>;

    struct Handlers {
        open: Closure<dyn FnMut()>,
        message: Closure<dyn FnMut(web_sys::MessageEvent)>,
        close: Closure<dyn FnMut()>,
    }

    impl Handlers {
        fn attach(&self, channel: &web_sys::RtcDataChannel) {
            channel.set_binary_type(web_sys::RtcDataChannelType::Arraybuffer);
            channel.set_onopen(Some(self.open.as_ref().unchecked_ref()));
            channel.set_onmessage(Some(self.message.as_ref().unchecked_ref()));
            channel.set_onclose(Some(self.close.as_ref().unchecked_ref()));
        }
    }

    struct Link {
        generation: u32,
        connection: web_sys::RtcPeerConnection,
        channel: Rc<RefCell<Option<web_sys::RtcDataChannel>>>,
        _handlers: Rc<Handlers>,
        _incoming: Closure<dyn FnMut(web_sys::RtcDataChannelEvent)>,
        _state: Closure<dyn FnMut()>,
    }

    impl Drop for Link {
        fn drop(&mut self) {
            if let Some(channel) = self.channel.borrow_mut().take() {
                channel.set_onopen(None);
                channel.set_onmessage(None);
                channel.set_onclose(None);
                channel.close();
            }
            self.connection.set_ondatachannel(None);
            self.connection.set_oniceconnectionstatechange(None);
            self.connection.close();
        }
    }

    /// One WebRTC connection per other rider, each with a single unordered,
    /// unreliable data channel for poses (a late pose is worthless; the next
    /// one follows). Callbacks queue events for the frame loop.
    #[derive(Default)]
    pub struct Mesh {
        links: HashMap<u8, Link>,
        events: MeshQueue,
        generation: u32,
    }

    impl Mesh {
        /// A fresh connection to `peer`, replacing any earlier one.
        fn open(&mut self, peer: u8) -> Option<&Link> {
            self.links.remove(&peer);
            self.generation += 1;
            let generation = self.generation;
            let servers = js_sys::Array::new();
            for url in STUN {
                let server = web_sys::RtcIceServer::new();
                server.set_urls(&wasm_bindgen::JsValue::from_str(url));
                servers.push(&server);
            }
            let configuration = web_sys::RtcConfiguration::new();
            configuration.set_ice_servers(&servers);
            let Ok(connection) = web_sys::RtcPeerConnection::new_with_configuration(&configuration)
            else {
                self.events
                    .borrow_mut()
                    .push_back((peer, generation, Happened::Closed));
                return None;
            };
            let push = {
                let events = self.events.clone();
                move |happened| events.borrow_mut().push_back((peer, generation, happened))
            };
            let handlers = Rc::new(Handlers {
                open: {
                    let push = push.clone();
                    Closure::<dyn FnMut()>::new(move || push(Happened::Open))
                },
                message: {
                    let push = push.clone();
                    Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
                        if let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>()
                            && buffer.byte_length() as usize <= MESSAGE_LIMIT
                        {
                            let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                            push(Happened::Message(bytes, web_time::Instant::now()));
                        }
                    })
                },
                close: {
                    let push = push.clone();
                    Closure::<dyn FnMut()>::new(move || push(Happened::Closed))
                },
            });
            let channel: Rc<RefCell<Option<web_sys::RtcDataChannel>>> = Rc::default();
            // The answering end receives the offerer's channel.
            let incoming = {
                let (handlers, channel) = (handlers.clone(), channel.clone());
                Closure::<dyn FnMut(_)>::new(move |event: web_sys::RtcDataChannelEvent| {
                    let arrived = event.channel();
                    handlers.attach(&arrived);
                    *channel.borrow_mut() = Some(arrived);
                })
            };
            connection.set_ondatachannel(Some(incoming.as_ref().unchecked_ref()));
            // "disconnected" may recover; the session's silence timeout
            // decides about a link that stays quiet.
            let state = {
                let connection = connection.clone();
                Closure::<dyn FnMut()>::new(move || {
                    use web_sys::RtcIceConnectionState as State;
                    if matches!(
                        connection.ice_connection_state(),
                        State::Failed | State::Closed
                    ) {
                        push(Happened::Closed);
                    }
                })
            };
            connection.set_oniceconnectionstatechange(Some(state.as_ref().unchecked_ref()));
            self.links.insert(
                peer,
                Link {
                    generation,
                    connection,
                    channel,
                    _handlers: handlers,
                    _incoming: incoming,
                    _state: state,
                },
            );
            self.links.get(&peer)
        }

        /// Creates the description for `link` (an answer to `remote`, or an
        /// offer) and queues it once its candidates are gathered.
        fn describe(&self, peer: u8, link: &Link, remote: Option<String>) {
            let (connection, events, generation) = (
                link.connection.clone(),
                self.events.clone(),
                link.generation,
            );
            wasm_bindgen_futures::spawn_local(async move {
                let answer = remote.is_some();
                let happened = match description(&connection, remote).await {
                    Some(text) => Happened::Local(answer, text.into_bytes()),
                    None => Happened::Closed,
                };
                events.borrow_mut().push_back((peer, generation, happened));
            });
        }

        /// Opens a connection to `peer` and offers it.
        pub fn offer(&mut self, peer: u8) {
            let Some(link) = self.open(peer) else {
                return;
            };
            let init = web_sys::RtcDataChannelInit::new();
            init.set_ordered(false);
            init.set_max_retransmits(0);
            let channel = link
                .connection
                .create_data_channel_with_data_channel_dict("pose", &init);
            link._handlers.attach(&channel);
            *link.channel.borrow_mut() = Some(channel);
            let link = &self.links[&peer];
            self.describe(peer, link, None);
        }

        /// Opens a connection from `peer`'s offer and answers it.
        pub fn accept(&mut self, peer: u8, description: &[u8]) {
            let offer = String::from_utf8_lossy(description).into_owned();
            if self.open(peer).is_some() {
                let link = &self.links[&peer];
                self.describe(peer, link, Some(offer));
            }
        }

        /// Completes the connection offered to `peer` with its answer.
        pub fn answer(&mut self, peer: u8, description: &[u8]) {
            let Some(link) = self.links.get(&peer) else {
                return;
            };
            let remote = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
            remote.set_sdp(&String::from_utf8_lossy(description));
            let (connection, events, generation) = (
                link.connection.clone(),
                self.events.clone(),
                link.generation,
            );
            wasm_bindgen_futures::spawn_local(async move {
                let applied = wasm_bindgen_futures::JsFuture::from(
                    connection.set_remote_description(&remote),
                )
                .await;
                if applied.is_err() {
                    events
                        .borrow_mut()
                        .push_back((peer, generation, Happened::Closed));
                }
            });
        }

        pub fn close(&mut self, peer: u8) {
            self.links.remove(&peer);
        }

        /// Closes every link (the room socket closed or the ride ended).
        pub fn clear(&mut self) {
            self.links.clear();
            self.events.borrow_mut().clear();
        }

        /// Sends `bytes` to `peer`; false when its link is not open or is
        /// backlogged.
        pub fn send(&self, peer: u8, bytes: &[u8]) -> bool {
            let Some(link) = self.links.get(&peer) else {
                return false;
            };
            let channel = link.channel.borrow();
            channel.as_ref().is_some_and(|channel| {
                channel.ready_state() == web_sys::RtcDataChannelState::Open
                    && channel.buffered_amount() < LINK_BACKLOG
                    && channel.send_with_u8_array(bytes).is_ok()
            })
        }

        pub fn poll(&mut self) -> Vec<super::MeshEvent> {
            use super::MeshEvent;
            let events: Vec<_> = self.events.borrow_mut().drain(..).collect();
            events
                .into_iter()
                .filter(|(peer, generation, _)| {
                    self.links
                        .get(peer)
                        .is_some_and(|link| link.generation == *generation)
                })
                .map(|(peer, _, happened)| match happened {
                    Happened::Local(answer, description) => MeshEvent::Local {
                        peer,
                        answer,
                        description,
                    },
                    Happened::Open => MeshEvent::Open(peer),
                    Happened::Closed => MeshEvent::Closed(peer),
                    Happened::Message(bytes, at) => MeshEvent::Message(peer, bytes, at),
                })
                .collect()
        }
    }

    /// Applies `remote` (an offer) if given, sets the local answer or offer
    /// and returns it once candidate gathering finishes or `GATHER_MS` pass.
    async fn description(
        connection: &web_sys::RtcPeerConnection,
        remote: Option<String>,
    ) -> Option<String> {
        use wasm_bindgen_futures::JsFuture;
        let local = match remote {
            Some(sdp) => {
                let offer = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Offer);
                offer.set_sdp(&sdp);
                JsFuture::from(connection.set_remote_description(&offer))
                    .await
                    .ok()?;
                JsFuture::from(connection.create_answer()).await.ok()?
            }
            None => JsFuture::from(connection.create_offer()).await.ok()?,
        };
        JsFuture::from(connection.set_local_description(local.unchecked_ref()))
            .await
            .ok()?;
        let started = web_time::Instant::now();
        while connection.ice_gathering_state() != web_sys::RtcIceGatheringState::Complete
            && started.elapsed().as_secs_f64() * 1000. < GATHER_MS
        {
            sleep(50).await;
        }
        Some(connection.local_description()?.sdp())
    }

    pub async fn post(url: &str, _origin: &str, body: &str) -> Result<(u16, String), String> {
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        // The game server is another origin than the page: no cookies.
        init.set_mode(web_sys::RequestMode::Cors);
        init.set_credentials(web_sys::RequestCredentials::Omit);
        init.set_body(&wasm_bindgen::JsValue::from_str(body));
        let headers = web_sys::Headers::new().map_err(|_| "headers")?;
        headers
            .set("Content-Type", "application/json")
            .map_err(|_| "headers")?;
        init.set_headers(&headers);
        let window = web_sys::window().ok_or("no window")?;
        let response =
            wasm_bindgen_futures::JsFuture::from(window.fetch_with_str_and_init(url, &init))
                .await
                // A refused request shows the browser's own message, as
                // multiplayer.js's awaited fetch does ("Failed to fetch").
                .map_err(|error| {
                    error
                        .dyn_ref::<js_sys::Error>()
                        .map_or_else(|| "Failed to fetch".to_string(), |e| e.message().into())
                })?;
        let response: web_sys::Response = response.dyn_into().map_err(|_| "fetch response")?;
        let text = wasm_bindgen_futures::JsFuture::from(response.text().map_err(|_| "body")?)
            .await
            .map_err(|_| "body")?
            .as_string()
            .unwrap_or_default();
        Ok((response.status(), text))
    }

    /// The Free Ride rider count (a GET without credentials, as the menu
    /// polls it).
    pub async fn riders(server: &str) -> Option<super::FreeRide> {
        let url = format!(
            "{}/multiplayer/freeride/riders",
            server.trim_end_matches('/')
        );
        let window = web_sys::window()?;
        let init = web_sys::RequestInit::new();
        init.set_mode(web_sys::RequestMode::Cors);
        init.set_credentials(web_sys::RequestCredentials::Omit);
        let response =
            wasm_bindgen_futures::JsFuture::from(window.fetch_with_str_and_init(&url, &init))
                .await
                .ok()?;
        let response: web_sys::Response = response.dyn_into().ok()?;
        if !response.ok() {
            return None;
        }
        let text = wasm_bindgen_futures::JsFuture::from(response.text().ok()?)
            .await
            .ok()?
            .as_string()?;
        super::riders_from(&text)
    }

    pub async fn sleep(ms: u32) {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            if let Some(window) = web_sys::window() {
                let _ = window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
            }
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{Mesh, Socket, post, riders_blocking, sleep};
#[cfg(target_arch = "wasm32")]
pub use web::{Mesh, Socket, post, riders, sleep};

/// The riders in Free Ride and the map a claim lands on.
pub type FreeRide = (u32, Option<u16>);

/// `{"riders": N, "map": M}` from `/multiplayer/freeride/riders` (`map`
/// since the Free Ride hub, research/FREERIDE_HUB.md).
fn riders_from(text: &str) -> Option<FreeRide> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let riders = value["riders"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())?;
    let map = value["map"].as_u64().and_then(|m| u16::try_from(m).ok());
    Some((riders, map))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rider_counts_parse_like_the_menu() {
        assert_eq!(super::riders_from(r#"{"riders":7}"#), Some((7, None)));
        assert_eq!(
            super::riders_from(r#"{"riders":7,"map":3}"#),
            Some((7, Some(3)))
        );
        assert_eq!(super::riders_from(r#"{"riders":-1}"#), None);
        assert_eq!(super::riders_from("nope"), None);
    }

    #[test]
    fn a_stuck_inbox_drops_poses_then_closes() {
        use super::{Admit, admit, is_pose};
        let pose = *b"ATVM\x03\x09\0\0";
        let old_pose = *b"ATVM\x02\x04\0\0";
        let control = *b"ATVM\x03\x01\0\0";
        assert!(is_pose(&pose) && is_pose(&old_pose) && !is_pose(&control));
        assert!(!is_pose(b"ATVM\x03\x09"));
        assert_eq!(admit(0, &pose), Admit::Queue);
        assert_eq!(admit(191, &pose), Admit::Queue);
        // Beyond 192 waiting: poses are dropped, control traffic still queues.
        assert_eq!(admit(192, &pose), Admit::Drop);
        assert_eq!(admit(192, &control), Admit::Queue);
        // Beyond 256: the session is stuck.
        assert_eq!(admit(256, &control), Admit::Close);
        assert_eq!(admit(300, &pose), Admit::Drop);
    }

    #[test]
    fn multiplayer_is_the_game_server() {
        use super::{GAME_SERVER, server_for};
        for site in [
            "https://furyremastered.com",
            "https://atv-offroad-fury-2.champtoday.workers.dev/play.html",
            "http://192.168.1.20:4173",
            "https://127.example.com",
        ] {
            assert_eq!(server_for(site, None), GAME_SERVER, "{site}");
        }
        // Local test pages keep their own origin (a local fury-server).
        for site in [
            "http://127.0.0.1:4187",
            "http://localhost:4187/play.html",
            "http://[::1]:4187",
        ] {
            assert_eq!(server_for(site, None), super::origin(site), "{site}");
        }
        // An override wins when it is an origin and only then.
        let public = "https://furyremastered.com";
        for chosen in ["https://mp.furyremastered.com/", " http://127.0.0.1:8789"] {
            assert_eq!(
                server_for(public, Some(chosen)),
                chosen.trim().trim_end_matches('/')
            );
        }
        for bad in [
            "",
            "mp.furyremastered.com",
            "javascript:alert(1)",
            "https://a/b",
            "https://u@h",
            "wss://h",
        ] {
            assert_eq!(server_for(public, Some(bad)), GAME_SERVER);
            assert_eq!(
                server_for("http://127.0.0.1:4187", Some(bad)),
                "http://127.0.0.1:4187"
            );
        }
    }

    #[test]
    fn room_urls_follow_the_relay_scheme() {
        assert_eq!(
            super::room_url("https://furyremastered.com/", "ABCD1234", Some("00ff")),
            "wss://furyremastered.com/multiplayer/rooms/ABCD1234?key=00ff"
        );
        assert_eq!(
            super::room_url("http://127.0.0.1:4187", "ABCD1234", None),
            "ws://127.0.0.1:4187/multiplayer/rooms/ABCD1234"
        );
        assert_eq!(
            super::origin("http://127.0.0.1:4187/play.html"),
            "http://127.0.0.1:4187"
        );
    }
}
