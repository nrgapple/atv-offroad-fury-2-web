//! Pass Key cloud saves in the app (research/CLOUD_SAVE.md, client section):
//! the HTTP transport for `atv_ui::cloud::Sync` (`fetch` on the web; ureq on
//! a worker thread on desktop and Android, against furyremastered.com), its
//! clock and triggers, the menus' view of it, and the Pass Key pages'
//! requests (copy, share, look up, save files, `#key=`/`#share=` links).
use crate::App;
#[cfg(target_arch = "wasm32")]
use crate::Message;
use atv_ui::cloud::{self, Notice, Request, Response, Status, Sync};
use atv_ui::front::controller::{CloudCommand, CloudView};
use atv_ui::settings::Settings;
#[cfg(target_arch = "wasm32")]
use winit::event_loop::EventLoopProxy;

/// A request's id and answer.
#[cfg(not(target_arch = "wasm32"))]
type Answer = (u64, Response);

pub struct Cloud {
    pub sync: Sync,
    start: web_time::Instant,
    /// The frontend page generation last seen (a new page pokes the sync:
    /// menu actions that change progress show a page or refresh it).
    shown: u64,
    /// A `#key=` / `#share=` link waiting for the menus.
    link: Option<String>,
    /// Persistent storage was requested this session.
    persisted: bool,
    /// Desktop and Android: answers from the request threads, and when the
    /// machine last ticked.
    #[cfg(not(target_arch = "wasm32"))]
    answers: (
        std::sync::mpsc::Sender<Answer>,
        std::sync::mpsc::Receiver<Answer>,
    ),
    #[cfg(not(target_arch = "wasm32"))]
    last_tick: f64,
}

impl Default for Cloud {
    fn default() -> Self {
        Self {
            sync: Sync::new(),
            start: web_time::Instant::now(),
            shown: 0,
            link: page_link(),
            persisted: false,
            #[cfg(not(target_arch = "wasm32"))]
            answers: std::sync::mpsc::channel(),
            #[cfg(not(target_arch = "wasm32"))]
            last_tick: f64::NEG_INFINITY,
        }
    }
}

impl Cloud {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.
    }
}

/// The page's `#key=` / `#share=` code, removed from the address so a reload
/// does not offer it again.
fn page_link() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window()?;
        let location = window.location();
        let hash = location.hash().ok()?;
        let (_, code) = cloud::link_code(&hash)?;
        let clean = format!(
            "{}{}",
            location.pathname().unwrap_or_default(),
            location.search().unwrap_or_default()
        );
        if let Ok(history) = js_sys::Reflect::get(&window, &"history".into())
            && let Ok(replace) = js_sys::Reflect::get(&history, &"replaceState".into())
            && let Ok(replace) = wasm_bindgen::JsCast::dyn_into::<js_sys::Function>(replace)
        {
            let _ = replace.call3(
                &history,
                &wasm_bindgen::JsValue::NULL,
                &"".into(),
                &clean.into(),
            );
        }
        Some(code)
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

/// 10 bytes from the platform's cryptographic random source.
fn random_bytes() -> [u8; 10] {
    let mut bytes = [0u8; 10];
    #[cfg(target_arch = "wasm32")]
    {
        let filled = web_sys::window()
            .and_then(|w| w.crypto().ok())
            .is_some_and(|c| c.get_random_values_with_u8_array(&mut bytes).is_ok());
        assert!(filled, "no cryptographic random source");
    }
    #[cfg(not(target_arch = "wasm32"))]
    getrandom::fill(&mut bytes).expect("no cryptographic random source");
    bytes
}

/// Where links point: the page's own origin on the web.
fn origin() -> String {
    #[cfg(target_arch = "wasm32")]
    if let Some(origin) = web_sys::window().and_then(|w| w.location().origin().ok()) {
        return origin;
    }
    std::env::var("FURY_SAVE_ORIGIN").unwrap_or_else(|_| cloud::ORIGIN.into())
}

/// Sends a request; its answer comes back as `Message::Cloud` (web) or
/// through the answer channel (`App::cloud_poll`).
#[cfg(target_arch = "wasm32")]
fn send(request: Request, proxy: EventLoopProxy<Message>) {
    wasm_bindgen_futures::spawn_local(async move {
        let id = request.id;
        let response = fetch(request).await.unwrap_or_default();
        let _ = proxy.send_event(Message::Cloud(id, response));
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn send(request: Request, answers: std::sync::mpsc::Sender<Answer>) {
    let origin = origin();
    std::thread::spawn(move || {
        let id = request.id;
        let _ = answers.send((id, native_request(&origin, request)));
    });
}

#[cfg(target_arch = "wasm32")]
async fn fetch(request: Request) -> Option<Response> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let init = web_sys::RequestInit::new();
    init.set_method(request.method);
    let headers = web_sys::Headers::new().ok()?;
    for (name, value) in &request.headers {
        headers.set(name, value).ok()?;
    }
    if let Some(body) = &request.body {
        headers.set("Content-Type", "application/json").ok()?;
        init.set_body(&wasm_bindgen::JsValue::from_str(body));
    }
    init.set_headers(&headers);
    let window = web_sys::window()?;
    let response: web_sys::Response =
        JsFuture::from(window.fetch_with_str_and_init(request.path, &init))
            .await
            .ok()?
            .dyn_into()
            .ok()?;
    let status = response.status();
    let retry_after = response
        .headers()
        .get("Retry-After")
        .ok()
        .flatten()
        .and_then(|v| v.trim().parse().ok());
    let body = JsFuture::from(response.text().ok()?)
        .await
        .ok()?
        .as_string()
        .unwrap_or_default();
    Some(Response {
        status,
        body,
        retry_after,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn native_request(origin: &str, request: Request) -> Response {
    use std::time::Duration;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let url = format!("{}{}", origin.trim_end_matches('/'), request.path);
    let result = match request.method {
        "GET" => {
            let mut call = agent.get(&url);
            for (name, value) in &request.headers {
                call = call.header(*name, value.as_str());
            }
            call.call()
        }
        method => {
            let mut call = match method {
                "PUT" => agent.put(&url),
                _ => agent.post(&url),
            };
            for (name, value) in &request.headers {
                call = call.header(*name, value.as_str());
            }
            call = call.header("Content-Type", "application/json");
            call.send(request.body.unwrap_or_default())
        }
    };
    match result {
        Ok(mut response) => {
            let status = response.status().as_u16();
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok());
            let body = response.body_mut().read_to_string().unwrap_or_default();
            Response {
                status,
                body,
                retry_after,
            }
        }
        Err(_) => Response::default(),
    }
}

/// Asks the browser to keep this site's storage (the saves) under storage
/// pressure; desktop files need nothing.
fn persist_storage() {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(navigator) = web_sys::window().map(|w| w.navigator()) else {
            return;
        };
        let call = || -> Option<()> {
            let storage = js_sys::Reflect::get(&navigator, &"storage".into()).ok()?;
            let persist = js_sys::Reflect::get(&storage, &"persist".into()).ok()?;
            let persist: js_sys::Function = wasm_bindgen::JsCast::dyn_into(persist).ok()?;
            let promise = persist.call0(&storage).ok()?;
            // A refusal changes nothing.
            let promise = js_sys::Promise::from(promise);
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            });
            Some(())
        };
        let _ = call();
    }
}

/// Save files: the browser's downloads and file picker, desktop's
/// Documents folder. Android keeps none (its Pass Key moves the save).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn saves_dir() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("FURY_SAVES_DIR") {
        return Some(dir.into());
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    Some(
        std::path::PathBuf::from(home)
            .join("Documents")
            .join("Fury Remastered"),
    )
}

fn files_hint() -> (bool, String) {
    #[cfg(target_arch = "wasm32")]
    return (
        true,
        "Export Save downloads a save file. Import Save loads one into this device \
         (merge or replace)."
            .into(),
    );
    #[cfg(target_os = "android")]
    return (false, String::new());
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    (
        saves_dir().is_some(),
        saves_dir().map_or(String::new(), |d| {
            format!(
                "Export Save writes a save file to {}. Import Save loads the newest one there \
                 (merge or replace).",
                d.display()
            )
        }),
    )
}

/// `fury-save-YYYY-MM-DD-HHMMSS.json` (UTC).
fn file_name() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    #[cfg(target_arch = "wasm32")]
    let seconds = (js_sys::Date::now() / 1000.) as u64;
    let days = (seconds / 86_400) as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let t = seconds % 86_400;
    format!(
        "fury-save-{year:04}-{month:02}-{day:02}-{:02}{:02}{:02}.json",
        t / 3600,
        t / 60 % 60,
        t % 60
    )
}

impl App {
    /// Runs the sync: the next request, and the menus' view of it.
    pub(crate) fn cloud_tick(&mut self) {
        if let Some(front) = &self.front {
            let shown = front.borrow().state.shown;
            if shown != self.cloud.shown {
                self.cloud.shown = shown;
                self.cloud.sync.poke();
            }
        }
        let now = self.cloud.now();
        let mut settings = Settings::load();
        if let Some(request) = self.cloud.sync.tick(now, &mut settings, &mut random_bytes) {
            #[cfg(target_arch = "wasm32")]
            send(request, self.proxy.clone());
            #[cfg(not(target_arch = "wasm32"))]
            send(request, self.cloud.answers.0.clone());
        }
        if !self.cloud.persisted && cloud::pass_key(&settings).is_some() {
            self.cloud.persisted = true;
            persist_storage();
        }
        self.cloud_view(&settings);
    }

    /// Desktop and Android, from the event loop: takes finished answers and
    /// ticks once a second; when to come back.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn cloud_poll(&mut self) -> web_time::Instant {
        while let Ok((id, response)) = self.cloud.answers.1.try_recv() {
            self.cloud_response(id, response);
        }
        let now = self.cloud.now();
        if now - self.cloud.last_tick >= 1000. {
            self.cloud.last_tick = now;
            self.cloud_tick();
        }
        // Answers are picked up promptly while a request is out.
        let wait = if self.cloud.sync.waiting() { 100 } else { 1000 };
        web_time::Instant::now() + std::time::Duration::from_millis(wait)
    }

    /// The menus are up: a waiting `#key=`/`#share=` link is looked up.
    pub(crate) fn cloud_ready(&mut self) {
        self.cloud_view(&Settings::load());
        let Some(code) = self.cloud.link.take() else {
            return;
        };
        let Some(front) = self.front_input() else {
            return;
        };
        let event = {
            let mut f = front.borrow_mut();
            let view = f.view;
            f.state.open_link(&code, &view)
        };
        if let Some(atv_ui::front::controller::Event::Cloud(command)) = event {
            self.cloud_command(command);
        }
    }

    /// Focus and visibility: back in the foreground pulls (at most every
    /// 30 s); going to the background writes pending progress at once.
    pub(crate) fn cloud_activity(&mut self, active: bool) {
        let now = self.cloud.now();
        if active {
            self.cloud.sync.wake(now);
        } else {
            self.cloud.sync.flush(now);
        }
        self.cloud_tick();
    }

    pub(crate) fn cloud_response(&mut self, id: u64, response: Response) {
        let now = self.cloud.now();
        let mut settings = Settings::load();
        let notices = self.cloud.sync.response(id, response, now, &mut settings);
        for notice in notices {
            match notice {
                Notice::LocalChanged => {
                    if let Some(front) = &self.front {
                        let mut f = front.borrow_mut();
                        let view = f.view;
                        f.state.load_profiles(&settings);
                        f.state.refresh(&view);
                    }
                }
                Notice::Fetched(fetched) => {
                    if let Some(front) = self.front_input() {
                        let mut f = front.borrow_mut();
                        let view = f.view;
                        f.state.show_fetched(*fetched, &view);
                    }
                }
                Notice::Shared(code) => self.offer_share(&code),
                Notice::Message(message) => {
                    if let Some(front) = &self.front {
                        front.borrow_mut().state.passkey_message(message);
                    }
                }
            }
        }
        self.cloud_tick();
        self.wake();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// The menus' copy of the sync state, refreshed when it changes.
    fn cloud_view(&mut self, settings: &Settings) {
        let Some(front) = self.front.clone() else {
            return;
        };
        let record = cloud::record(settings);
        let status = &self.cloud.sync.status;
        let (files, files_hint) = files_hint();
        let view = CloudView {
            key: cloud::pass_key(settings),
            status: status.label(),
            short: status.short().into(),
            synced: *status == Status::Saved,
            trouble: matches!(status, Status::Offline | Status::Failed(_)),
            share_code: record.share_code,
            previous_key: record.previous_key,
            origin: origin(),
            files,
            files_hint,
        };
        if front.borrow().state.cloud == view {
            return;
        }
        let mut f = front.borrow_mut();
        f.state.cloud = view;
        if f.state.page.starts_with("passkey") {
            let page_view = f.view;
            f.state.refresh(&page_view);
        }
        drop(f);
        self.wake();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// A Pass Key page's request.
    pub(crate) fn cloud_command(&mut self, command: CloudCommand) {
        match command {
            CloudCommand::Copy(text) => self.copy_text(text, "Pass Key copied."),
            CloudCommand::Share => {
                self.cloud.sync.share();
                self.cloud.sync.flush(self.cloud.now());
            }
            CloudCommand::Fetch(code) => self.cloud.sync.fetch(&code),
            CloudCommand::Applied => {
                self.cloud.sync.flush(self.cloud.now());
                self.cloud.sync.wake(self.cloud.now());
            }
            CloudCommand::Export => self.export_save(),
            CloudCommand::Import => self.import_save(),
        }
        self.cloud_tick();
        self.wake();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn front_notice(&self, text: String) {
        if let Some(front) = &self.front {
            front.borrow_mut().state.notice = Some(text);
        }
    }

    /// Copies `text`; the notice says so, or shows the text to copy by hand.
    fn copy_text(&mut self, text: String, done: &'static str) {
        #[cfg(target_arch = "wasm32")]
        if let Some(clipboard) = web_sys::window().map(|w| w.navigator().clipboard()) {
            let promise = clipboard.write_text(&text);
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let copied = wasm_bindgen_futures::JsFuture::from(promise).await.is_ok();
                let _ = proxy.send_event(Message::LobbyNotice(if copied {
                    done.into()
                } else {
                    text
                }));
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.front_notice(if crate::clipboard::copy(&text) {
            done.into()
        } else {
            text
        });
    }

    /// A new share code: the system share sheet where there is one (phones),
    /// else the link on the clipboard; the code shows on the page either way.
    fn offer_share(&mut self, code: &str) {
        let link = cloud::link(&origin(), code);
        self.front_notice(format!("Share code {code}: {link}"));
        #[cfg(target_arch = "wasm32")]
        {
            let navigator = web_sys::window().map(|w| w.navigator());
            let share = navigator.as_ref().and_then(|n| {
                let share = js_sys::Reflect::get(n, &"share".into()).ok()?;
                wasm_bindgen::JsCast::dyn_into::<js_sys::Function>(share).ok()
            });
            let proxy = self.proxy.clone();
            let clipboard = navigator.as_ref().map(|n| n.clipboard());
            let data = js_sys::Object::new();
            let set = |k: &str, v: &str| {
                let _ = js_sys::Reflect::set(&data, &k.into(), &v.into());
            };
            set("title", "Fury Remastered progress");
            set(
                "text",
                &format!("Load my Fury Remastered progress with share code {code}."),
            );
            set("url", &link);
            let code = code.to_string();
            wasm_bindgen_futures::spawn_local(async move {
                let shared = match (&share, &navigator) {
                    (Some(share), Some(navigator)) => match share.call1(navigator, &data) {
                        Ok(promise) => {
                            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
                                .await
                                .is_ok()
                        }
                        Err(_) => false,
                    },
                    _ => false,
                };
                let copied = !shared
                    && match &clipboard {
                        Some(c) => wasm_bindgen_futures::JsFuture::from(c.write_text(&link))
                            .await
                            .is_ok(),
                        None => false,
                    };
                let line = if shared {
                    format!("Share code {code} sent.")
                } else if copied {
                    format!("Share code {code}: link copied.")
                } else {
                    format!("Share code {code}: {link}")
                };
                let _ = proxy.send_event(Message::LobbyNotice(line));
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        if crate::clipboard::copy(&link) {
            self.front_notice(format!("Share code {code}: link copied."));
        }
    }

    fn export_save(&mut self) {
        let text = cloud::export_text(&Settings::load());
        let name = file_name();
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let done = (|| -> Option<()> {
                let window = web_sys::window()?;
                let document = window.document()?;
                let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(&text));
                let options = web_sys::BlobPropertyBag::new();
                options.set_type("application/json");
                let blob =
                    web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).ok()?;
                let url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
                let link = document.create_element("a").ok()?;
                link.set_attribute("href", &url).ok()?;
                link.set_attribute("download", &name).ok()?;
                link.dyn_ref::<web_sys::HtmlElement>()?.click();
                let _ = web_sys::Url::revoke_object_url(&url);
                Some(())
            })()
            .is_some();
            self.front_notice(if done {
                format!("Save exported as {name}.")
            } else {
                "The browser refused the download.".into()
            });
        }
        #[cfg(target_os = "android")]
        {
            let _ = (text, name);
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let message = match saves_dir() {
                Some(dir) => {
                    let path = dir.join(&name);
                    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, text)) {
                        Ok(()) => format!("Save exported to {}.", path.display()),
                        Err(e) => format!("Export failed: {e}"),
                    }
                }
                None => "No Documents folder to export to.".into(),
            };
            self.front_notice(message);
        }
    }

    fn import_save(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let proxy = self.proxy.clone();
            let opened = (|| -> Option<()> {
                let document = web_sys::window()?.document()?;
                let input: web_sys::HtmlInputElement =
                    document.create_element("input").ok()?.dyn_into().ok()?;
                input.set_type("file");
                input.set_accept(".json,application/json");
                let picker = input.clone();
                let on_change = wasm_bindgen::closure::Closure::once_into_js(move || {
                    let file = js_sys::Reflect::get(&picker, &"files".into())
                        .ok()
                        .and_then(|files| js_sys::Reflect::get(&files, &0.into()).ok())
                        .filter(|f| !f.is_undefined());
                    let Some(file) = file else {
                        return;
                    };
                    let name = js_sys::Reflect::get(&file, &"name".into())
                        .ok()
                        .and_then(|n| n.as_string())
                        .unwrap_or_else(|| "save file".into());
                    let text = js_sys::Reflect::get(&file, &"text".into())
                        .ok()
                        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                        .and_then(|f| f.call0(&file).ok());
                    let Some(text) = text else {
                        return;
                    };
                    wasm_bindgen_futures::spawn_local(async move {
                        let text =
                            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(text))
                                .await
                                .ok()
                                .and_then(|t| t.as_string())
                                .unwrap_or_default();
                        let _ = proxy.send_event(Message::CloudImport(name, text));
                    });
                });
                input.set_onchange(Some(on_change.unchecked_ref()));
                input.click();
                Some(())
            })();
            if opened.is_none() {
                self.front_notice("The browser refused the file picker.".into());
            }
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let newest = saves_dir().and_then(|dir| {
                std::fs::read_dir(dir)
                    .ok()?
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                    .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
            });
            match newest {
                Some(entry) => {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
                    self.cloud_import(name, text);
                }
                None => self.front_notice(format!(
                    "No save file found in {}.",
                    saves_dir().map_or(String::new(), |d| d.display().to_string())
                )),
            }
        }
    }

    /// A save file's text: confirm it on the page.
    pub(crate) fn cloud_import(&mut self, name: String, text: String) {
        let Some(front) = self.front.clone() else {
            return;
        };
        let mut f = front.borrow_mut();
        match atv_core::cloud_save::Save::parse(&text) {
            Ok(save) => {
                let view = f.view;
                f.state.show_fetched(
                    cloud::Fetched {
                        source: cloud::Source::File(name),
                        revision: 0,
                        updated_at: String::new(),
                        save,
                    },
                    &view,
                );
            }
            Err(message) => f.state.passkey_message(message),
        }
        drop(f);
        self.wake();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
