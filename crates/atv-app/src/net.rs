//! Relay transport: a binary WebSocket and the room-creation POST, over
//! web-sys in the browser and a native client (tungstenite/ureq, rustls) on
//! desktop. Bytes only; [`atv_game::multiplayer::Session`] owns the protocol.

/// What the socket reported since the last poll.
#[derive(Debug)]
pub enum Event {
    Open,
    Message(Vec<u8>),
    Closed { code: u16, reason: String },
}

/// Outgoing bytes held beyond this are not queued (the browser checks
/// `bufferedAmount < 32768` before every send).
const BACKLOG: usize = 32768;
/// Relay messages are at most 8 KiB (web/multiplayer.js closes otherwise).
const MESSAGE_LIMIT: usize = 8192;

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
    use super::{BACKLOG, Event, MESSAGE_LIMIT};
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
    }

    impl Socket {
        pub fn connect(url: &str, origin: &str) -> Socket {
            let (out_tx, out_rx) = mpsc::channel::<Out>();
            let (event_tx, events) = mpsc::channel();
            let queued = Arc::new(AtomicUsize::new(0));
            let backlog = queued.clone();
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
                                if event_tx.send(Event::Message(bytes.to_vec())).is_err() {
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
            self.events.try_iter().collect()
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
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{BACKLOG, Event, MESSAGE_LIMIT};
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};
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
        _callbacks: Callbacks,
    }

    impl Socket {
        pub fn connect(url: &str, _origin: &str) -> Socket {
            let events: Queue = Rc::default();
            let socket = web_sys::WebSocket::new(url);
            let open = {
                let events = events.clone();
                Closure::<dyn FnMut()>::new(move || events.borrow_mut().push_back(Event::Open))
            };
            let message = {
                let events = events.clone();
                Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
                    if let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                        let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                        if bytes.len() <= MESSAGE_LIMIT {
                            events.borrow_mut().push_back(Event::Message(bytes));
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
pub use native::{Socket, post, sleep};
#[cfg(target_arch = "wasm32")]
pub use web::{Socket, post, sleep};

#[cfg(test)]
mod tests {
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
