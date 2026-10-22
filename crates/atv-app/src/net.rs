//! Relay transport: a binary WebSocket and the room-creation POST, over
//! web-sys in the browser and a native client (tungstenite/ureq, rustls) on
//! desktop. Bytes only; [`atv_game::multiplayer::Session`] owns the protocol.

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

/// The page origin the relay admits: the relay's own scheme and host.
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

    /// POST a JSON body to the relay; the status and body text.
    pub async fn post(url: &str, origin: &str, body: &str) -> Result<(u16, String), String> {
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

    pub async fn sleep(ms: u32) {
        std::thread::sleep(Duration::from_millis(u64::from(ms)));
    }

    /// The Free Ride rider count (blocking; call off the UI thread).
    pub fn riders_blocking(relay: &str) -> Option<u32> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        let url = format!(
            "{}/multiplayer/freeride/riders",
            relay.trim_end_matches('/')
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
        collections::VecDeque,
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

    pub async fn post(url: &str, _origin: &str, body: &str) -> Result<(u16, String), String> {
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
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
                .map_err(|_| "Unable to reach the multiplayer relay.".to_string())?;
        let response: web_sys::Response = response.dyn_into().map_err(|_| "fetch response")?;
        let text = wasm_bindgen_futures::JsFuture::from(response.text().map_err(|_| "body")?)
            .await
            .map_err(|_| "body")?
            .as_string()
            .unwrap_or_default();
        Ok((response.status(), text))
    }

    /// The Free Ride rider count (a same-origin GET, as the menu polls it).
    pub async fn riders(relay: &str) -> Option<u32> {
        let url = format!(
            "{}/multiplayer/freeride/riders",
            relay.trim_end_matches('/')
        );
        let window = web_sys::window()?;
        let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(&url))
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
pub use native::{Socket, post, riders_blocking, sleep};
#[cfg(target_arch = "wasm32")]
pub use web::{Socket, post, riders, sleep};

/// `{"riders": N}` from `/multiplayer/freeride/riders`.
fn riders_from(text: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    value["riders"].as_u64().and_then(|n| u32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rider_counts_parse_like_the_menu() {
        assert_eq!(super::riders_from(r#"{"riders":7}"#), Some(7));
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
