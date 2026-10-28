//! Pass Key cloud saves on this device: the stored key and sync record, the
//! save read from and written to local storage, and the background sync
//! state machine. The host performs the HTTP requests ([`Sync::tick`]
//! hands one out, [`Sync::response`] takes its answer), so the same machine
//! runs over `fetch` on the web and a worker thread on desktop and Android.
//!
//! Local storage stays the source of truth: the game saves locally as it
//! always did, and the machine pushes the changes when it can, merging a
//! conflicting server save in (`atv_core::cloud_save::merge`) instead of
//! dropping either side (research/CLOUD_SAVE.md, client section).
use crate::settings::{Settings, load_book, load_hall_of_fame, save_book, save_hall_of_fame};
use atv_core::cloud_save::{self, Save};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// This device's Pass Key (`FURY-XXXX-XXXX-XXXX-XXXX`).
pub const PASS_KEY_KEY: &str = "fury-pass-key";
/// The sync record ([`Record`]).
pub const CLOUD_KEY: &str = "fury-cloud";
pub const KEY_HEADER: &str = "X-Fury-Pass-Key";
pub const SHARE_HEADER: &str = "X-Fury-Share-Code";
/// Where desktop and Android builds reach the save API (the web page uses
/// its own origin).
pub const ORIGIN: &str = "https://furyremastered.com";

/// How often local storage is compared with the last synced save while
/// nothing pokes the machine.
const CHECK_MS: f64 = 10_000.;
/// Focus and visibility pull the server save at most this often.
const PULL_MS: f64 = 30_000.;
/// A request with no answer by then counts as a network failure.
const TIMEOUT_MS: f64 = 30_000.;
const BACKOFF_MIN_MS: f64 = 2_000.;
/// At most one save write this often (bursts coalesce; hiding the app
/// flushes at once).
const PUSH_GAP_MS: f64 = 5_000.;
const BACKOFF_MAX_MS: f64 = 300_000.;
const TOO_LARGE: &str = "Save is too large to sync (64 KiB limit).";

/// What the device remembers about the server save.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Record {
    /// The server revision `base` is (0: never synced).
    pub revision: u64,
    pub updated_at: String,
    /// The save as the server last held it: the merge base for conflicts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<Value>,
    /// The last share code this device made.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub share_code: String,
    /// The key this device used before entering another one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub previous_key: String,
}

pub fn pass_key(settings: &Settings) -> Option<String> {
    settings
        .get(PASS_KEY_KEY)
        .and_then(|k| cloud_save::normalize_key(&k))
}

pub fn record(settings: &Settings) -> Record {
    settings
        .get(CLOUD_KEY)
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn store_record(settings: &mut Settings, record: &Record) -> bool {
    serde_json::to_string(record).is_ok_and(|t| settings.set(CLOUD_KEY, &t))
}

/// The progress saved on this device.
pub fn local_save(settings: &Settings) -> Save {
    Save::new(load_book(settings), load_hall_of_fame(settings))
}

/// Replaces the progress saved on this device; false when it lasts only for
/// this session.
pub fn write_save(settings: &mut Settings, save: &Save) -> bool {
    let book = save_book(settings, &save.profiles);
    save_hall_of_fame(settings, &save.hall_of_fame) && book
}

/// A save as JSON for a file (Export Save).
pub fn export_text(settings: &Settings) -> String {
    serde_json::to_string_pretty(&local_save(settings)).unwrap_or_default()
}

/// The link that opens the game and offers to load a key or share code.
pub fn link(origin: &str, code: &str) -> String {
    let kind = if code.starts_with(cloud_save::SHARE_PREFIX) {
        "share"
    } else {
        "key"
    };
    format!("{}/play.html#{kind}={code}", origin.trim_end_matches('/'))
}

/// A link's `#key=` or `#share=` code, normalized.
pub fn link_code(fragment: &str) -> Option<(bool, String)> {
    let fragment = fragment.trim_start_matches('#');
    for part in fragment.split('&') {
        let (name, value) = part.split_once('=')?;
        match name {
            "key" => return cloud_save::normalize_key(value).map(|k| (false, k)),
            "share" => return cloud_save::normalize_share(value).map(|c| (true, c)),
            _ => {}
        }
    }
    None
}

/// Where a save shown for confirmation came from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Enter Pass Key (or a `#key=` link).
    Key(String),
    /// Enter Share Code (or a `#share=` link).
    Share(String),
    /// Import Save: the file's name.
    File(String),
}

/// A save fetched (or read from a file) and waiting for the player's choice.
#[derive(Clone, Debug, PartialEq)]
pub struct Fetched {
    pub source: Source,
    pub revision: u64,
    pub updated_at: String,
    pub save: Save,
}

/// What the player chose for a [`Fetched`] save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Keep this device's progress and the fetched save's (idempotent
    /// merge, `cloud_save::merge` without a base).
    Merge,
    /// The fetched save replaces this device's.
    Replace,
    /// A share code's progress joins the loaded profile.
    ShareIntoCurrent,
    /// A share code's profile becomes a new profile.
    ShareAsNew,
}

/// Applies a fetched save; the message to show, or why not.
pub fn apply(settings: &mut Settings, fetched: &Fetched, choice: Choice) -> Result<String, String> {
    let local = local_save(settings);
    let incoming = &fetched.save;
    let (save, message) = match (&fetched.source, choice) {
        (Source::Share(_), Choice::ShareIntoCurrent | Choice::ShareAsNew) => {
            let shared = incoming
                .main_profile()
                .cloned()
                .ok_or("That share code holds no profile.")?;
            let hall = cloud_save::merge(
                &Save::new(Default::default(), local.hall_of_fame.clone()),
                &Save::new(Default::default(), incoming.hall_of_fame.clone()),
                None,
            )
            .hall_of_fame;
            let mut book = local.profiles.clone();
            let current = book.current_profile().map(|p| p.name.clone());
            let message = match (choice, current) {
                (Choice::ShareIntoCurrent, Some(name)) => {
                    let one = atv_core::profile::ProfileBook {
                        current: Some(0),
                        profiles: vec![atv_core::profile::Profile {
                            name: name.clone(),
                            ..shared
                        }],
                    };
                    let mine = Save::new(book.clone(), Default::default());
                    let theirs = Save::new(one, Default::default());
                    book = cloud_save::merge(&mine, &theirs, None).profiles;
                    format!("Shared progress copied into {name}.")
                }
                _ => {
                    let index = cloud_save::add_profile(&mut book, &shared)
                        .map_err(|e| e.message().to_string())?;
                    format!(
                        "Shared progress added as profile {}.",
                        book.profiles[index].name
                    )
                }
            };
            (Save::new(book, hall), message)
        }
        (_, Choice::Merge) => (
            cloud_save::merge(&local, incoming, None),
            "Saves merged.".to_string(),
        ),
        (_, _) => (incoming.clone(), "Save loaded.".to_string()),
    };
    if serde_json::to_string(&save).map_or(0, |t| t.len()) > cloud_save::MAX_BYTES {
        return Err("The combined save is too large to keep. Choose Replace instead.".into());
    }
    if !write_save(settings, &save) {
        return Err(crate::settings::STORAGE_ERROR.into());
    }
    if let Source::Key(key) = &fetched.source {
        let old = record(settings);
        let previous = pass_key(settings)
            .filter(|k| k != key)
            .unwrap_or(old.previous_key);
        settings.set(PASS_KEY_KEY, key);
        store_record(
            settings,
            &Record {
                revision: fetched.revision,
                updated_at: fetched.updated_at.clone(),
                base: Some(fetched.save.to_value()),
                share_code: String::new(),
                previous_key: previous,
            },
        );
        return Ok(match choice {
            Choice::Merge => "Pass Key loaded. This device's progress was merged in.".into(),
            _ => "Pass Key loaded. This device now continues that save.".into(),
        });
    }
    Ok(message)
}

/// A request for the host to send.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub id: u64,
    pub method: &'static str,
    /// Path on the save API's origin (`/api/save`…).
    pub path: &'static str,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<String>,
}

/// The host's answer: the HTTP status (0 when the request failed to reach
/// the server), the body and a `Retry-After` in seconds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
enum Op {
    Pull,
    /// The save (as compared with the base), the revision it replaces and
    /// the save as sent.
    Push(Value, u64, Value),
    Share,
    FetchKey(String),
    FetchShare(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// No progress yet, so no key.
    NoKey,
    Saving,
    Saved,
    /// The server could not be reached; progress is safe on the device.
    Offline,
    Failed(String),
}

impl Status {
    pub fn label(&self) -> String {
        match self {
            Self::NoKey => "No Pass Key yet".into(),
            Self::Saving => "Saving…".into(),
            Self::Saved => "Saved online".into(),
            Self::Offline => "Offline · saved on this device".into(),
            Self::Failed(message) => message.clone(),
        }
    }

    /// The badge's word.
    pub fn short(&self) -> &'static str {
        match self {
            Self::NoKey => "",
            Self::Saving => "Saving…",
            Self::Saved => "Saved",
            Self::Offline => "Offline",
            Self::Failed(_) => "Not synced",
        }
    }
}

/// What a response changed, for the host and the menus.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// A merge rewrote local progress: reload the menus' profiles.
    LocalChanged,
    /// A Pass Key's or share code's save, for the player to confirm.
    Fetched(Box<Fetched>),
    /// A share code was made.
    Shared(String),
    /// A line for the menu notice.
    Message(String),
}

struct InFlight {
    id: u64,
    op: Op,
    /// The key the request was sent for (a key entered meanwhile makes its
    /// answer stale).
    key: Option<String>,
    sent: f64,
}

/// The background sync state machine (one per app).
pub struct Sync {
    next_id: u64,
    in_flight: Option<InFlight>,
    retry_at: f64,
    backoff: f64,
    pull_due: bool,
    check_due: bool,
    last_check: f64,
    last_pull: f64,
    /// Writes are coalesced: none before this time (D1 bills rows written).
    push_after: f64,
    share_wanted: bool,
    user: Option<Op>,
    /// A save the server refused (too large, invalid): not sent again until
    /// it changes.
    refused: Option<Value>,
    pub status: Status,
}

impl Default for Sync {
    fn default() -> Self {
        Self::new()
    }
}

impl Sync {
    /// Starting up pulls the server save and checks local progress.
    pub fn new() -> Self {
        Self {
            next_id: 1,
            in_flight: None,
            retry_at: 0.,
            backoff: 0.,
            pull_due: true,
            check_due: true,
            last_check: f64::NEG_INFINITY,
            last_pull: f64::NEG_INFINITY,
            push_after: f64::NEG_INFINITY,
            share_wanted: false,
            user: None,
            refused: None,
            status: Status::NoKey,
        }
    }

    /// Local progress may have changed (a menu action, a finished race).
    pub fn poke(&mut self) {
        self.check_due = true;
    }

    /// The app came back to the foreground: pull (at most every 30 s) and
    /// retry at once if offline.
    /// The app is being hidden or closed: write pending progress now.
    pub fn flush(&mut self, now: f64) {
        self.push_after = now;
        self.check_due = true;
        if self.status == Status::Offline {
            self.retry_at = now;
        }
    }

    pub fn wake(&mut self, now: f64) {
        if now - self.last_pull >= PULL_MS {
            self.pull_due = true;
        }
        self.check_due = true;
        if self.status == Status::Offline {
            self.retry_at = now;
        }
    }

    /// Share Progress: sync, then make a share code.
    pub fn share(&mut self) {
        self.share_wanted = true;
        self.check_due = true;
    }

    /// Enter Pass Key / Enter Share Code: fetch that save for confirmation.
    pub fn fetch(&mut self, code: &str) {
        self.user = Some(if code.starts_with(cloud_save::SHARE_PREFIX) {
            Op::FetchShare(code.to_string())
        } else {
            Op::FetchKey(code.to_string())
        });
    }

    /// A fetch is queued or waiting for its answer.
    pub fn fetching(&self) -> bool {
        self.user.is_some()
            || self
                .in_flight
                .as_ref()
                .is_some_and(|f| matches!(f.op, Op::FetchKey(_) | Op::FetchShare(_)))
    }

    /// A request is on its way.
    pub fn waiting(&self) -> bool {
        self.in_flight.is_some()
    }

    pub fn sharing(&self) -> bool {
        self.share_wanted
    }

    /// The next request to send, if any. `random` gives 10 bytes from a
    /// cryptographic source for a new key.
    pub fn tick(
        &mut self,
        now: f64,
        settings: &mut Settings,
        random: &mut dyn FnMut() -> [u8; 10],
    ) -> Option<Request> {
        if let Some(f) = &self.in_flight {
            if now - f.sent < TIMEOUT_MS {
                return None;
            }
            // No answer: a network failure (a late answer is ignored).
            let id = f.id;
            let _ = self.response(id, Response::default(), now, settings);
        }
        let key = pass_key(settings);
        if let Some(op) = self.user.take() {
            return Some(self.issue(op, key, now));
        }
        if now < self.retry_at {
            return None;
        }
        let check = self.check_due || now - self.last_check >= CHECK_MS;
        let key = match key {
            Some(key) => key,
            None => {
                if !check {
                    return None;
                }
                self.check_due = false;
                self.last_check = now;
                if !local_save(settings).has_progress() {
                    self.status = Status::NoKey;
                    if std::mem::take(&mut self.share_wanted) {
                        // Nothing to share yet (the page hides Share then).
                        self.status = Status::NoKey;
                    }
                    return None;
                }
                // First saved progress: this device's Pass Key.
                let key = cloud_save::pass_key(random());
                let previous = record(settings).previous_key;
                settings.set(PASS_KEY_KEY, &key);
                store_record(
                    settings,
                    &Record {
                        previous_key: previous,
                        ..Default::default()
                    },
                );
                self.pull_due = false;
                self.check_due = true;
                key
            }
        };
        if self.pull_due {
            self.pull_due = false;
            self.last_pull = now;
            return Some(self.issue(Op::Pull, Some(key), now));
        }
        if self.check_due || now - self.last_check >= CHECK_MS {
            self.check_due = false;
            self.last_check = now;
            let local = local_save(settings);
            let save = local.to_value();
            let record = record(settings);
            if record.revision == 0 || record.base.as_ref() != Some(&save) {
                if self.refused.as_ref() == Some(&save) {
                    return None;
                }
                if now < self.push_after {
                    // A burst of changes goes as one write.
                    self.check_due = true;
                    self.saving();
                    return None;
                }
                let wire = local.to_wire();
                if wire.to_string().len() > cloud_save::MAX_BYTES {
                    self.refused = Some(save);
                    self.status = Status::Failed(TOO_LARGE.into());
                    return None;
                }
                self.refused = None;
                self.saving();
                return Some(self.issue(Op::Push(save, record.revision, wire), Some(key), now));
            }
            self.status = Status::Saved;
        }
        if self.share_wanted && self.status == Status::Saved {
            return Some(self.issue(Op::Share, Some(key), now));
        }
        None
    }

    /// Changes wait to be written. Offline stays Offline until a write
    /// gets through (retries do not flicker the badge).
    fn saving(&mut self) {
        if self.status != Status::Offline {
            self.status = Status::Saving;
        }
    }

    fn issue(&mut self, op: Op, key: Option<String>, now: f64) -> Request {
        let id = self.next_id;
        self.next_id += 1;
        let header = |name: &'static str, value: &str| vec![(name, value.to_string())];
        let key_header = header(KEY_HEADER, key.as_deref().unwrap_or(""));
        let (method, path, headers, body) = match &op {
            Op::Pull => ("GET", "/api/save", key_header, None),
            Op::Push(_, base, wire) => {
                let body = json!({"baseRevision": base, "save": wire}).to_string();
                ("PUT", "/api/save", key_header, Some(body))
            }
            Op::Share => ("POST", "/api/save/share", key_header, None),
            Op::FetchKey(k) => ("GET", "/api/save", header(KEY_HEADER, k), None),
            Op::FetchShare(c) => ("GET", "/api/share", header(SHARE_HEADER, c), None),
        };
        self.in_flight = Some(InFlight {
            id,
            op,
            key,
            sent: now,
        });
        Request {
            id,
            method,
            path,
            headers,
            body,
        }
    }

    /// Takes the answer to request `id`; what changed.
    pub fn response(
        &mut self,
        id: u64,
        response: Response,
        now: f64,
        settings: &mut Settings,
    ) -> Vec<Notice> {
        let Some(sent) = self.in_flight.take_if(|f| f.id == id) else {
            return Vec::new();
        };
        let mut notices = Vec::new();
        let status = response.status;
        let body: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
        let error = || {
            body["error"]
                .as_str()
                .map_or_else(|| format!("Save server error ({status})."), str::to_string)
        };
        let transient = status == 0 || status == 429 || status >= 500;
        match &sent.op {
            Op::FetchKey(_) | Op::FetchShare(_) => {
                let share = matches!(sent.op, Op::FetchShare(_));
                let message = match status {
                    200 => match Save::from_value(body["save"].clone()) {
                        Ok(save) => {
                            let source = match &sent.op {
                                Op::FetchShare(code) => Source::Share(code.clone()),
                                Op::FetchKey(key) => Source::Key(key.clone()),
                                _ => unreachable!(),
                            };
                            notices.push(Notice::Fetched(Box::new(Fetched {
                                source,
                                revision: body["revision"].as_u64().unwrap_or(0),
                                updated_at: text(
                                    &body[if share { "createdAt" } else { "updatedAt" }],
                                ),
                                save,
                            })));
                            None
                        }
                        Err(message) => Some(message),
                    },
                    404 if share => Some("No shared progress was found for that code.".into()),
                    404 => Some("No save was found for that Pass Key.".into()),
                    400 if share => Some("That share code is not valid.".into()),
                    400 => Some("That Pass Key is not valid.".into()),
                    _ if transient => Some(
                        "The save server could not be reached. Check the connection and try again."
                            .into(),
                    ),
                    _ => Some(error()),
                };
                notices.extend(message.map(Notice::Message));
                return notices;
            }
            _ if pass_key(settings) != sent.key => {
                // The player entered another key meanwhile.
                self.check_due = true;
                return notices;
            }
            _ if transient => {
                self.backoff = (self.backoff * 2.).clamp(BACKOFF_MIN_MS, BACKOFF_MAX_MS);
                let wait = response
                    .retry_after
                    .map_or(self.backoff, |s| (s * 1000.).max(BACKOFF_MIN_MS));
                self.retry_at = now + wait;
                match sent.op {
                    Op::Pull => self.pull_due = true,
                    _ => self.check_due = true,
                }
                self.status = Status::Offline;
                return notices;
            }
            _ => {}
        }
        self.backoff = 0.;
        match (sent.op, status) {
            (Op::Pull, 200) => {
                self.apply_remote(&body, settings, &mut notices);
            }
            (Op::Pull, 404) => {
                // Not on the server (never pushed, or lost): create it.
                let mut record = record(settings);
                if record.revision != 0 {
                    record.revision = 0;
                    record.base = None;
                    store_record(settings, &record);
                }
                self.check_due = true;
            }
            (Op::Push(save, ..), 200 | 201) => {
                let mut record = record(settings);
                record.revision = body["revision"].as_u64().unwrap_or(record.revision + 1);
                record.updated_at = text(&body["updatedAt"]);
                record.base = Some(save);
                store_record(settings, &record);
                self.status = Status::Saved;
                self.push_after = now + PUSH_GAP_MS;
                // Changes made while it was on its way go next.
                self.check_due = true;
            }
            (Op::Push(..), 409) => {
                if body["save"].is_object() {
                    self.apply_remote(&body, settings, &mut notices);
                } else {
                    self.pull_due = true;
                }
                self.check_due = true;
            }
            (Op::Push(save, ..), 413) => {
                self.refused = Some(save);
                self.status = Status::Failed(TOO_LARGE.into());
            }
            (Op::Push(save, ..), _) => {
                self.refused = Some(save);
                self.status = Status::Failed(error());
            }
            (Op::Share, 200 | 201) => {
                let code = text(&body["shareCode"]);
                self.share_wanted = false;
                if let Some(code) = cloud_save::normalize_share(&code) {
                    let mut record = record(settings);
                    record.share_code = code.clone();
                    store_record(settings, &record);
                    notices.push(Notice::Shared(code));
                } else {
                    notices.push(Notice::Message(
                        "The save server sent no share code.".into(),
                    ));
                }
            }
            (Op::Share, 404) => {
                // Not on the server yet: push first, then share.
                let mut record = record(settings);
                record.revision = 0;
                store_record(settings, &record);
                self.check_due = true;
            }
            (Op::Share, _) => {
                self.share_wanted = false;
                notices.push(Notice::Message(error()));
            }
            (Op::Pull, _) => self.status = Status::Failed(error()),
            (Op::FetchKey(_) | Op::FetchShare(_), _) => {}
        }
        notices
    }

    /// A server save (a pull or a conflict): merged into local progress
    /// over the last agreed save, and recorded as the new base.
    fn apply_remote(&mut self, body: &Value, settings: &mut Settings, notices: &mut Vec<Notice>) {
        let remote = match Save::from_value(body["save"].clone()) {
            Ok(save) => save,
            Err(message) => {
                self.status = Status::Failed(message);
                self.refused = Some(local_save(settings).to_value());
                return;
            }
        };
        let revision = body["revision"].as_u64().unwrap_or(0);
        let mut record = record(settings);
        let base = record.base.clone().and_then(|b| Save::from_value(b).ok());
        if revision == record.revision && base.is_some() {
            return;
        }
        let local = local_save(settings);
        let merged = cloud_save::merge(&local, &remote, base.as_ref());
        if merged != local {
            write_save(settings, &merged);
            notices.push(Notice::LocalChanged);
        }
        record.revision = revision;
        record.updated_at = text(&body["updatedAt"]);
        record.base = Some(remote.to_value());
        store_record(settings, &record);
        self.check_due = true;
    }
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// The contract's server, in memory.
    #[derive(Default)]
    struct Server {
        saves: BTreeMap<String, (u64, Value)>,
        shares: BTreeMap<String, Value>,
        /// Fail every request with this status (0: network down).
        down: Option<u16>,
        requests: Vec<(String, String)>,
    }

    impl Server {
        fn handle(&mut self, r: &Request) -> Response {
            self.requests.push((r.method.into(), r.path.into()));
            if let Some(status) = self.down {
                return Response {
                    status,
                    body: String::new(),
                    retry_after: None,
                };
            }
            let header = |name| {
                r.headers
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            let reply = |status, body: Value| Response {
                status,
                body: body.to_string(),
                retry_after: None,
            };
            match (r.method, r.path) {
                ("GET", "/api/save") => match self.saves.get(&header(KEY_HEADER)) {
                    Some((rev, save)) => reply(
                        200,
                        json!({"revision": rev, "updatedAt": "t", "save": save}),
                    ),
                    None => reply(404, json!({"error": "not_found"})),
                },
                ("PUT", "/api/save") => {
                    let body: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
                    let key = header(KEY_HEADER);
                    let current = self.saves.get(&key).map_or(0, |s| s.0);
                    if body["baseRevision"].as_u64() != Some(current) {
                        let (rev, save) = self.saves[&key].clone();
                        return reply(
                            409,
                            json!({"error": "conflict", "revision": rev, "updatedAt": "t", "save": save}),
                        );
                    }
                    self.saves.insert(key, (current + 1, body["save"].clone()));
                    reply(200, json!({"revision": current + 1, "updatedAt": "t"}))
                }
                ("POST", "/api/save/share") => match self.saves.get(&header(KEY_HEADER)) {
                    Some((_, save)) => {
                        let code = format!("SHARE-AAAA-BBBB-CCCC-{:04}", self.shares.len());
                        self.shares.insert(code.clone(), save.clone());
                        reply(201, json!({"shareCode": code, "createdAt": "t"}))
                    }
                    None => reply(404, json!({"error": "not_found"})),
                },
                ("GET", "/api/share") => match self.shares.get(&header(SHARE_HEADER)) {
                    Some(save) => reply(200, json!({"createdAt": "t", "save": save})),
                    None => reply(404, json!({"error": "not_found"})),
                },
                _ => reply(400, json!({"error": "bad_request"})),
            }
        }
    }

    /// One device: its storage and machine.
    struct Device {
        settings: Settings,
        sync: Sync,
        now: f64,
        seed: u8,
        notices: Vec<Notice>,
    }

    impl Device {
        fn new(seed: u8) -> Self {
            Self {
                settings: Settings::memory(),
                sync: Sync::new(),
                now: 0.,
                seed,
                notices: Vec::new(),
            }
        }

        /// Runs the machine against the server until it has nothing to send.
        fn settle(&mut self, server: &mut Server) {
            for _ in 0..20 {
                let seed = self.seed;
                let mut random = || [seed; 10];
                let Some(request) = self.sync.tick(self.now, &mut self.settings, &mut random)
                else {
                    return;
                };
                let response = server.handle(&request);
                let notices =
                    self.sync
                        .response(request.id, response, self.now, &mut self.settings);
                self.notices.extend(notices);
                self.now += 100.;
            }
            panic!("sync did not settle");
        }

        fn profile(&self) -> atv_core::profile::Profile {
            crate::settings::load_profile(&self.settings)
        }

        fn earn(&mut self, points: i64) {
            let mut p = self.profile();
            p.record_race(1, points as i32, 0.);
            crate::settings::save_profile(&mut self.settings, &p);
            self.sync.poke();
            self.now += PUSH_GAP_MS;
        }
    }

    #[test]
    fn no_progress_no_key_then_one_on_first_progress() {
        let mut server = Server::default();
        let mut a = Device::new(1);
        a.settle(&mut server);
        assert_eq!(a.sync.status, Status::NoKey);
        assert!(pass_key(&a.settings).is_none());
        assert!(server.requests.is_empty());
        a.earn(10);
        a.settle(&mut server);
        let key = pass_key(&a.settings).unwrap();
        assert_eq!(key, cloud_save::pass_key([1; 10]));
        assert_eq!(a.sync.status, Status::Saved);
        assert_eq!(server.saves[&key].0, 1);
        assert_eq!(record(&a.settings).revision, 1);
        // Nothing changed: nothing is sent.
        let sent = server.requests.len();
        a.now += 20_000.;
        a.settle(&mut server);
        assert_eq!(server.requests.len(), sent);
    }

    #[test]
    fn a_key_entered_on_another_device_loads_the_progress() {
        let mut server = Server::default();
        let mut a = Device::new(1);
        a.earn(10);
        a.earn(5);
        a.settle(&mut server);
        let key = pass_key(&a.settings).unwrap();
        let mut b = Device::new(2);
        b.sync.fetch(&key);
        b.settle(&mut server);
        let Some(Notice::Fetched(fetched)) = b.notices.pop() else {
            panic!("no fetched save");
        };
        assert_eq!(fetched.source, Source::Key(key.clone()));
        apply(&mut b.settings, &fetched, Choice::Replace).unwrap();
        assert_eq!(pass_key(&b.settings).as_deref(), Some(key.as_str()));
        assert_eq!(b.profile().points, 15);
        b.sync.flush(b.now);
        b.settle(&mut server);
        // Loading changed nothing to push.
        assert_eq!(server.saves[&key].0, 1);
        assert_eq!(b.sync.status, Status::Saved);
        // B races; A pulls it on focus.
        b.earn(10);
        b.settle(&mut server);
        assert_eq!(server.saves[&key].0, 2);
        a.now += PULL_MS;
        a.sync.wake(a.now);
        a.settle(&mut server);
        assert_eq!(a.profile().points, 25);
        assert!(a.notices.contains(&Notice::LocalChanged));
        // An unknown key.
        b.sync.fetch("FURY-0000-0000-0000-0000");
        b.settle(&mut server);
        assert_eq!(
            b.notices.pop(),
            Some(Notice::Message(
                "No save was found for that Pass Key.".into()
            ))
        );
    }

    #[test]
    fn a_conflict_merges_both_devices_progress() {
        let mut server = Server::default();
        let mut a = Device::new(1);
        a.earn(10);
        a.settle(&mut server);
        let key = pass_key(&a.settings).unwrap();
        let mut b = Device::new(2);
        b.sync.fetch(&key);
        b.settle(&mut server);
        let Some(Notice::Fetched(fetched)) = b.notices.pop() else {
            panic!()
        };
        apply(&mut b.settings, &fetched, Choice::Replace).unwrap();
        // Both race offline-ish: A pushes first, B conflicts and merges.
        a.earn(5);
        b.earn(7);
        a.settle(&mut server);
        b.settle(&mut server);
        assert_eq!(b.profile().points, 22);
        assert_eq!(b.profile().races, 3);
        let (rev, save) = server.saves[&key].clone();
        assert_eq!(rev, 3);
        let saved = Save::from_value(save).unwrap();
        assert_eq!(saved.main_profile().unwrap().points, 22);
        a.sync.wake(a.now + PULL_MS);
        a.now += PULL_MS;
        a.settle(&mut server);
        assert_eq!(a.profile().points, 22);
        assert_eq!(server.saves[&key].0, 3);
    }

    #[test]
    fn offline_backs_off_and_keeps_local_progress() {
        let mut server = Server {
            down: Some(0),
            ..Default::default()
        };
        let mut a = Device::new(1);
        a.earn(10);
        a.settle(&mut server);
        assert_eq!(a.sync.status, Status::Offline);
        assert_eq!(a.profile().points, 10);
        let sent = server.requests.len();
        // Within the backoff nothing is sent.
        a.now += 500.;
        a.settle(&mut server);
        assert_eq!(server.requests.len(), sent);
        // Rate limited with Retry-After, then back.
        server.down = None;
        a.now += 60_000.;
        a.settle(&mut server);
        assert_eq!(a.sync.status, Status::Saved);
        let key = pass_key(&a.settings).unwrap();
        assert_eq!(server.saves[&key].0, 1);
    }

    #[test]
    fn share_codes_copy_progress_into_another_save() {
        let mut server = Server::default();
        let mut a = Device::new(1);
        a.earn(40);
        a.sync.share();
        a.settle(&mut server);
        let Some(Notice::Shared(code)) = a.notices.pop() else {
            panic!("no share code")
        };
        assert_eq!(record(&a.settings).share_code, code);
        let mut b = Device::new(2);
        b.earn(3);
        b.settle(&mut server);
        let b_key = pass_key(&b.settings).unwrap();
        b.sync.fetch(&code);
        b.settle(&mut server);
        let Some(Notice::Fetched(fetched)) = b.notices.pop() else {
            panic!()
        };
        let message = apply(&mut b.settings, &fetched, Choice::ShareIntoCurrent).unwrap();
        assert_eq!(message, "Shared progress copied into PLAYER1.");
        // The loader keeps its own key; the copy is idempotent.
        assert_eq!(pass_key(&b.settings), Some(b_key.clone()));
        assert_eq!(b.profile().points, 40);
        apply(&mut b.settings, &fetched, Choice::ShareIntoCurrent).unwrap();
        assert_eq!(b.profile().points, 40);
        apply(&mut b.settings, &fetched, Choice::ShareAsNew).unwrap();
        let book = load_book(&b.settings);
        assert_eq!(book.profiles.len(), 2);
        assert_eq!(book.current_profile().unwrap().name, "PLAYER12");
        b.sync.flush(b.now);
        b.settle(&mut server);
        assert_eq!(server.saves[&b_key].0, 2);
    }

    #[test]
    fn merge_on_entering_a_key_keeps_this_devices_progress() {
        let mut server = Server::default();
        let mut a = Device::new(1);
        a.earn(10);
        a.settle(&mut server);
        let key = pass_key(&a.settings).unwrap();
        let mut b = Device::new(2);
        b.earn(30);
        {
            let mut p = b.profile();
            p.store.atvs.push(12);
            crate::settings::save_profile(&mut b.settings, &p);
        }
        b.settle(&mut server);
        let old = pass_key(&b.settings).unwrap();
        b.sync.fetch(&key);
        b.settle(&mut server);
        let Some(Notice::Fetched(fetched)) = b.notices.pop() else {
            panic!()
        };
        apply(&mut b.settings, &fetched, Choice::Merge).unwrap();
        assert_eq!(record(&b.settings).previous_key, old);
        assert_eq!(b.profile().points, 30);
        assert_eq!(b.profile().store.atvs, [12]);
        b.sync.flush(b.now);
        b.settle(&mut server);
        let saved = Save::from_value(server.saves[&key].1.clone()).unwrap();
        assert_eq!(saved.main_profile().unwrap().store.atvs, [12]);
        assert_eq!(server.saves[&key].0, 2);
    }

    #[test]
    fn too_large_saves_are_not_resent_until_they_change() {
        let mut server = Server {
            down: Some(413),
            ..Default::default()
        };
        let mut a = Device::new(1);
        a.earn(10);
        a.settle(&mut server);
        assert!(matches!(a.sync.status, Status::Failed(_)));
        let sent = server.requests.len();
        a.now += 60_000.;
        a.settle(&mut server);
        assert_eq!(server.requests.len(), sent);
        a.earn(1);
        a.settle(&mut server);
        assert_eq!(server.requests.len(), sent + 1);
    }

    #[test]
    fn bursts_of_changes_go_as_one_write_and_hiding_flushes() {
        let mut server = Server::default();
        let mut a = Device::new(1);
        a.earn(10);
        a.settle(&mut server);
        let key = pass_key(&a.settings).unwrap();
        assert_eq!(server.saves[&key].0, 1);
        // Two quick changes inside the gap: nothing yet.
        a.now -= PUSH_GAP_MS - 100.;
        let mut p = a.profile();
        p.credit(1);
        crate::settings::save_profile(&mut a.settings, &p);
        a.sync.poke();
        a.settle(&mut server);
        assert_eq!(server.saves[&key].0, 1);
        assert_eq!(a.sync.status, Status::Saving);
        // The page is hidden: written at once.
        a.sync.flush(a.now);
        a.settle(&mut server);
        assert_eq!(server.saves[&key].0, 2);
        assert_eq!(a.sync.status, Status::Saved);
    }

    #[test]
    fn links_carry_keys_and_share_codes() {
        let key = "FURY-28T5-CY4T-QKFF-03ZD";
        let url = link("https://furyremastered.com/", key);
        assert_eq!(
            url,
            "https://furyremastered.com/play.html#key=FURY-28T5-CY4T-QKFF-03ZD"
        );
        assert_eq!(
            link_code("#key=fury-28t5-cy4t-qkff-o3zd"),
            Some((false, key.into()))
        );
        assert_eq!(
            link_code("#share=SHARE-AAAA-BBBB-CCCC-0000"),
            Some((true, "SHARE-AAAA-BBBB-CCCC-0000".into()))
        );
        assert_eq!(link_code("#key=nope"), None);
        assert_eq!(link_code(""), None);
    }

    #[test]
    fn export_round_trips_through_import() {
        let mut a = Device::new(1);
        a.earn(10);
        let text = export_text(&a.settings);
        let save = Save::parse(&text).unwrap();
        let mut b = Device::new(2);
        let fetched = Fetched {
            source: Source::File("fury-save.json".into()),
            revision: 0,
            updated_at: String::new(),
            save,
        };
        apply(&mut b.settings, &fetched, Choice::Replace).unwrap();
        assert_eq!(b.profile().points, 10);
        assert!(pass_key(&b.settings).is_none());
    }
}
