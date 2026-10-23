//! Original runtime content addressed by disc path.
//!
//! Every build reads the same hosted package format: `manifest.json` lists the
//! authorized `.RES`/`.VAG`/`.LSI` files as SHA-256 addressed chunks. The web
//! build fetches chunks over HTTP; desktop builds read a local package
//! directory or a single content pack (see [`pack`]). Release builds may
//! embed a pack through the opt-in `embedded` feature; no content is
//! compiled into this crate by default or stored in Git.
#[cfg(not(target_arch = "wasm32"))]
pub mod pack;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, String>;

/// A chunk whose bytes do not match its manifest digest (content.js).
const INTEGRITY: &str = "Hosted content integrity check failed.";

/// A file the package does not list, worded as game.js `archiveAt` reports
/// it. Loaders with their own wording check [`Content::has`] first.
pub fn missing_archive(path: &str) -> String {
    format!("Original archive is missing: {path}")
}

/// What an HTTP read fetches, for content.js's status wording.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fetch {
    /// `manifest.json`: "Original content package is unavailable (404)."
    Manifest,
    /// A chunk or stream: "Original game content could not load (503)."
    Content,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl Fetch {
    fn of(name: &str) -> Self {
        if name == "manifest.json" {
            Fetch::Manifest
        } else {
            Fetch::Content
        }
    }

    /// The message for a response with HTTP `status` (content.js
    /// `openHostedContent`).
    fn status(self, status: u16) -> String {
        match self {
            Fetch::Manifest => format!("Original content package is unavailable ({status})."),
            Fetch::Content => format!("Original game content could not load ({status})."),
        }
    }
}

/// SHA-256 of the USA v2.00 boot executable the package was extracted from.
pub const SOURCE_BUILD: &str = "11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49";
const MAX_FILES: usize = 512;
const MAX_FILE: usize = 128 * 1024 * 1024;
const MAX_CHUNK: usize = 25 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub source_build_sha256: String,
    pub files: Vec<FileRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub bytes: usize,
    #[serde(default)]
    pub lazy: bool,
    sha256: Option<String>,
    chunks: Option<Vec<Chunk>>,
    /// Soundtrack songs: a hosted AAC re-encode a media element can stream.
    #[serde(default)]
    pub stream: Option<MediaStream>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaStream {
    pub url: String,
    pub bytes: usize,
    pub seconds: f64,
    /// The encoded file's SHA-256 (its blob name, `<sha256>.m4a`).
    #[serde(default)]
    pub sha256: Option<String>,
    /// The media type (`audio/mp4`).
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

impl MediaStream {
    /// content.js `contentSource`: a soundtrack song's stream is a
    /// same-origin, hash-addressed AAC file of bounded size and positive length.
    fn valid_on(&self, path: &str) -> bool {
        let song = path
            .strip_prefix("/AUDIO/MUSIC/")
            .and_then(|name| name.strip_suffix(".VAG"))
            .is_some_and(|name| {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            });
        let digest = self.sha256.as_deref().filter(|d| is_digest(d));
        song && self.kind.as_deref() == Some("audio/mp4")
            && self.bytes > 0
            && self.bytes <= MAX_CHUNK
            && digest.is_some_and(|d| self.url == format!("/content/{d}.m4a"))
            && self.seconds.is_finite()
            && self.seconds > 0.
    }
}

/// A lowercase 64-digit hex SHA-256.
fn is_digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Chunk {
    pub offset: usize,
    pub bytes: usize,
    pub sha256: String,
}

impl FileRecord {
    /// Chunks in file order; a single-chunk file lists its own digest.
    pub fn chunks(&self) -> Vec<Chunk> {
        match (&self.chunks, &self.sha256) {
            (Some(chunks), _) => chunks.clone(),
            (None, Some(sha256)) => vec![Chunk {
                offset: 0,
                bytes: self.bytes,
                sha256: sha256.clone(),
            }],
            (None, None) => Vec::new(),
        }
    }
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let manifest: Manifest =
            serde_json::from_slice(bytes).map_err(|e| format!("content manifest: {e}"))?;
        if manifest.version != 1
            || manifest.source_build_sha256 != SOURCE_BUILD
            || manifest.files.len() > MAX_FILES
        {
            return Err("Unsupported hosted content package.".into());
        }
        for record in &manifest.files {
            let valid_path = record.path.starts_with('/')
                && [".RES", ".VAG", ".LSI"]
                    .iter()
                    .any(|ext| record.path.ends_with(ext))
                && record
                    .path
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/_.-".contains(&b));
            if !valid_path || record.bytes == 0 || record.bytes > MAX_FILE {
                return Err("Invalid hosted content record.".into());
            }
            let chunks = record.chunks();
            if chunks.is_empty() || chunks.len() > 128 {
                return Err("Invalid hosted content chunks.".into());
            }
            let mut end = 0;
            for chunk in chunks {
                if !is_digest(&chunk.sha256)
                    || chunk.offset != end
                    || chunk.bytes == 0
                    || chunk.bytes > MAX_CHUNK
                {
                    return Err("Invalid hosted content extent.".into());
                }
                end += chunk.bytes;
            }
            if end != record.bytes {
                return Err("Truncated hosted content record.".into());
            }
            if record
                .stream
                .as_ref()
                .is_some_and(|s| !s.valid_on(&record.path))
            {
                return Err("Invalid hosted music stream.".into());
            }
        }
        Ok(manifest)
    }

    pub fn record(&self, path: &str) -> Option<&FileRecord> {
        self.files
            .iter()
            .find(|record| record.path.eq_ignore_ascii_case(path))
    }
}

/// Where chunk bytes come from.
pub enum Store {
    /// A hosted package laid out as `<dir>/manifest.json` and `<dir>/<sha256>.bin`.
    #[cfg(not(target_arch = "wasm32"))]
    Directory(std::path::PathBuf),
    /// The same package flattened into one pack file or embedded blob.
    #[cfg(not(target_arch = "wasm32"))]
    Pack(pack::Pack),
    /// The same layout served over HTTP, e.g. `/content/`.
    #[cfg(target_arch = "wasm32")]
    Http(String),
}

impl Store {
    async fn read(&self, name: &str) -> Result<Vec<u8>> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Store::Directory(dir) => std::fs::read(dir.join(name))
                .map_err(|e| format!("{}: {e}", dir.join(name).display())),
            #[cfg(not(target_arch = "wasm32"))]
            Store::Pack(pack) => pack.read(name),
            #[cfg(target_arch = "wasm32")]
            Store::Http(base) => web::fetch(&format!("{base}{name}"), Fetch::of(name)).await,
        }
    }
}

pub struct Content {
    pub manifest: Manifest,
    store: Store,
    /// Verified chunks recently read, newest last, bounded by `CACHE_BYTES`.
    /// Venue, ride and audio loading read the same course archive; the
    /// second and third reads are served from here.
    cache: std::cell::RefCell<Vec<(String, std::rc::Rc<Vec<u8>>)>>,
    /// Prefetched chunk downloads in flight by blob name (web): the read that
    /// needs one awaits the same request (content.js `prefetch`).
    #[cfg(target_arch = "wasm32")]
    inflight: std::cell::RefCell<std::collections::HashMap<String, js_sys::Promise>>,
}

const CACHE_BYTES: usize = 40 * 1024 * 1024;

impl Content {
    pub async fn open(store: Store) -> Result<Self> {
        let manifest = Manifest::parse(&store.read("manifest.json").await?)?;
        Ok(Self {
            manifest,
            store,
            cache: Default::default(),
            #[cfg(target_arch = "wasm32")]
            inflight: Default::default(),
        })
    }

    /// Whether the package lists `path`.
    pub fn has(&self, path: &str) -> bool {
        self.manifest.record(path).is_some()
    }

    fn record_for(&self, path: &str) -> Result<&FileRecord> {
        self.manifest
            .record(path)
            .ok_or_else(|| missing_archive(path))
    }

    async fn chunk(&self, chunk: &Chunk) -> Result<std::rc::Rc<Vec<u8>>> {
        let cached = self
            .cache
            .borrow()
            .iter()
            .position(|(sha, _)| *sha == chunk.sha256);
        if let Some(index) = cached {
            let mut cache = self.cache.borrow_mut();
            let entry = cache.remove(index);
            let bytes = entry.1.clone();
            cache.push(entry);
            return Ok(bytes);
        }
        let name = format!("{}.bin", chunk.sha256);
        #[cfg(target_arch = "wasm32")]
        let pending = self.inflight.borrow_mut().remove(&name);
        #[cfg(target_arch = "wasm32")]
        let bytes = match (pending, &self.store) {
            (Some(response), Store::Http(_)) => {
                match web::finish(response, Fetch::Content).await {
                    Ok(bytes) => bytes,
                    // A failed prefetch is retried by the real read.
                    Err(_) => self.store.read(&name).await?,
                }
            }
            _ => self.store.read(&name).await?,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let bytes = self.store.read(&name).await?;
        // Hash in 512 KiB pieces, yielding between them, so verifying a large
        // file never holds one frame (other riders' models load mid-ride).
        let mut digest = Sha256::new();
        for piece in bytes.chunks(512 * 1024) {
            digest.update(piece);
            yield_once().await;
        }
        if bytes.len() != chunk.bytes || hex(&digest.finalize()) != chunk.sha256 {
            return Err(INTEGRITY.into());
        }
        let bytes = std::rc::Rc::new(bytes);
        let mut cache = self.cache.borrow_mut();
        cache.push((chunk.sha256.clone(), bytes.clone()));
        let mut total: usize = cache.iter().map(|(_, b)| b.len()).sum();
        while total > CACHE_BYTES && cache.len() > 1 {
            total -= cache.remove(0).1.len();
        }
        Ok(bytes)
    }

    /// Starts downloading the chunks of `paths` a load is about to read, so
    /// they arrive in parallel instead of one request after another (web;
    /// content.js `prefetch`). `low` asks the browser for low fetch priority;
    /// `head` fetches only a file's first chunk (an archive read by
    /// directory). Reads reuse the in-flight request. Local stores need none.
    pub fn prefetch(&self, paths: &[String], low: bool, head: bool) {
        #[cfg(target_arch = "wasm32")]
        {
            let Store::Http(base) = &self.store;
            for path in paths {
                let Some(record) = self.manifest.record(path) else {
                    continue;
                };
                let chunks = record.chunks();
                let take = if head || record.lazy { 1 } else { chunks.len() };
                for chunk in chunks.iter().take(take) {
                    let name = format!("{}.bin", chunk.sha256);
                    let cached = self
                        .cache
                        .borrow()
                        .iter()
                        .any(|(sha, _)| *sha == chunk.sha256);
                    if cached || self.inflight.borrow().contains_key(&name) {
                        continue;
                    }
                    if let Some(request) = web::start(&format!("{base}{name}"), low) {
                        self.inflight.borrow_mut().insert(name, request);
                    }
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (paths, low, head);
    }

    /// Prefetched downloads not yet read (tests and diagnostics).
    pub fn prefetching(&self) -> usize {
        #[cfg(target_arch = "wasm32")]
        return self.inflight.borrow().len();
        #[cfg(not(target_arch = "wasm32"))]
        0
    }

    /// One whole original file, integrity checked chunk by chunk.
    pub async fn file(&self, path: &str) -> Result<Vec<u8>> {
        let record = self.record_for(path)?;
        let mut out = Vec::with_capacity(record.bytes);
        for chunk in record.chunks() {
            out.extend_from_slice(&self.chunk(&chunk).await?);
        }
        Ok(out)
    }

    /// The bundled soundtrack stream of a song (the `.m4a` the web streams
    /// from `stream.url`), integrity checked; `None` when the record has no
    /// stream. A package or pack built without streams reports the blob as
    /// missing, and the caller plays the original file instead.
    pub async fn stream(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let Some(stream) = self.record_for(path)?.stream.as_ref() else {
            return Ok(None);
        };
        // Manifest::parse checked the record (`MediaStream::valid_on`).
        let digest = stream
            .sha256
            .as_deref()
            .ok_or_else(|| format!("invalid stream record for {path}"))?;
        // Read past the chunk cache: a song is several megabytes, read once.
        let bytes = self.store.read(&format!("{digest}.m4a")).await?;
        let mut hash = Sha256::new();
        for piece in bytes.chunks(512 * 1024) {
            hash.update(piece);
            yield_once().await;
        }
        if bytes.len() != stream.bytes || hex(&hash.finalize()) != digest {
            return Err(INTEGRITY.into());
        }
        Ok(Some(bytes))
    }

    /// Size of an original file in the package.
    pub fn size(&self, path: &str) -> Result<usize> {
        Ok(self.record_for(path)?.bytes)
    }

    /// `count` bytes at `offset` of an original file, fetching only the
    /// chunks that cover them (archives read member by member).
    pub async fn read(&self, path: &str, offset: usize, count: usize) -> Result<Vec<u8>> {
        let record = self.record_for(path)?;
        let end = offset
            .checked_add(count)
            .filter(|end| *end <= record.bytes)
            .ok_or("Invalid content read.")?;
        let mut out = Vec::with_capacity(count);
        for chunk in record.chunks() {
            let (low, high) = (
                offset.max(chunk.offset),
                end.min(chunk.offset + chunk.bytes),
            );
            if low >= high {
                continue;
            }
            let bytes = self.chunk(&chunk).await?;
            out.extend_from_slice(&bytes[low - chunk.offset..high - chunk.offset]);
        }
        Ok(out)
    }
}

/// Returns to the caller's executor once (the app polls mid-ride loads a
/// slice per frame; other executors re-poll at once).
fn yield_once() -> impl std::future::Future<Output = ()> {
    struct Yield(bool);
    impl std::future::Future for Yield {
        type Output = ();
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<()> {
            if self.0 {
                std::task::Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        }
    }
    Yield(false)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::Fetch;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    /// A rejection as frontend.js shows it: the Error's `message` (a refused
    /// request is the browser's own "Failed to fetch" TypeError).
    fn js(error: wasm_bindgen::JsValue) -> String {
        if let Some(error) = error.dyn_ref::<js_sys::Error>() {
            return String::from(error.message());
        }
        error.as_string().unwrap_or_else(|| format!("{error:?}"))
    }

    /// Starts a fetch (optionally at low priority) and returns its promise.
    pub fn start(url: &str, low: bool) -> Option<js_sys::Promise> {
        let window = web_sys::window()?;
        let init = web_sys::RequestInit::new();
        if low {
            js_sys::Reflect::set(&init, &"priority".into(), &"low".into()).ok()?;
        }
        Some(window.fetch_with_str_and_init(url, &init))
    }

    /// The bytes of a fetch started by [`start`].
    pub async fn finish(request: js_sys::Promise, kind: Fetch) -> super::Result<Vec<u8>> {
        let response: web_sys::Response = JsFuture::from(request)
            .await
            .map_err(js)?
            .dyn_into()
            .map_err(js)?;
        if !response.ok() {
            return Err(kind.status(response.status()));
        }
        let buffer = JsFuture::from(response.array_buffer().map_err(js)?)
            .await
            .map_err(js)?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }

    pub async fn fetch(url: &str, kind: Fetch) -> super::Result<Vec<u8>> {
        let window = web_sys::window().ok_or("no window")?;
        finish(window.fetch_with_str(url), kind).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(files: &str) -> String {
        format!(r#"{{"version":1,"source_build_sha256":"{SOURCE_BUILD}","files":{files}}}"#)
    }

    #[test]
    fn accepts_single_and_chunked_records() {
        let digest = "a".repeat(64);
        let text = manifest(&format!(
            r#"[{{"path":"/A.RES","bytes":4,"sha256":"{digest}"}},{{"path":"/B/C.VAG","bytes":6,"chunks":[{{"offset":0,"bytes":2,"sha256":"{digest}"}},{{"offset":2,"bytes":4,"sha256":"{digest}"}}]}}]"#
        ));
        let parsed = Manifest::parse(text.as_bytes()).unwrap();
        assert_eq!(parsed.record("/a.res").unwrap().chunks().len(), 1);
        assert_eq!(parsed.record("/B/C.VAG").unwrap().chunks().len(), 2);
    }

    #[test]
    fn rejects_gaps_foreign_builds_and_other_files() {
        let digest = "a".repeat(64);
        let gap = manifest(&format!(
            r#"[{{"path":"/A.RES","bytes":6,"chunks":[{{"offset":0,"bytes":2,"sha256":"{digest}"}},{{"offset":3,"bytes":3,"sha256":"{digest}"}}]}}]"#
        ));
        assert!(Manifest::parse(gap.as_bytes()).is_err());
        let executable = manifest(&format!(
            r#"[{{"path":"/SCUS_972.11","bytes":4,"sha256":"{digest}"}}]"#
        ));
        assert!(Manifest::parse(executable.as_bytes()).is_err());
        let foreign = r#"{"version":1,"source_build_sha256":"00","files":[]}"#;
        assert!(Manifest::parse(foreign.as_bytes()).is_err());
    }

    #[test]
    fn failures_read_as_content_js_words_them() {
        // The failed-load notice shows these (frontend.js `fail`).
        let digest = "a".repeat(64);
        let error = |files: String| Manifest::parse(manifest(&files).as_bytes()).unwrap_err();
        assert_eq!(
            error(format!(
                r#"[{{"path":"/SCUS_972.11","bytes":4,"sha256":"{digest}"}}]"#
            )),
            "Invalid hosted content record."
        );
        assert_eq!(
            error(r#"[{"path":"/A.RES","bytes":4}]"#.into()),
            "Invalid hosted content chunks."
        );
        assert_eq!(
            error(format!(
                r#"[{{"path":"/A.RES","bytes":4,"chunks":[{{"offset":1,"bytes":4,"sha256":"{digest}"}}]}}]"#
            )),
            "Invalid hosted content extent."
        );
        assert_eq!(
            error(format!(
                r#"[{{"path":"/A.RES","bytes":6,"chunks":[{{"offset":0,"bytes":4,"sha256":"{digest}"}}]}}]"#
            )),
            "Truncated hosted content record."
        );
        let foreign = r#"{"version":1,"source_build_sha256":"00","files":[]}"#;
        assert_eq!(
            Manifest::parse(foreign.as_bytes()).unwrap_err(),
            "Unsupported hosted content package."
        );
        assert_eq!(
            Fetch::of("manifest.json").status(404),
            "Original content package is unavailable (404)."
        );
        assert_eq!(
            Fetch::of(&format!("{digest}.bin")).status(503),
            "Original game content could not load (503)."
        );
        assert_eq!(
            missing_archive("/ANIMSRDR.RES"),
            "Original archive is missing: /ANIMSRDR.RES"
        );
    }

    /// web/content.test.mjs: music streams must be same-origin hash-addressed
    /// AAC on soundtrack songs.
    #[test]
    fn music_streams_are_same_origin_hash_addressed_aac_on_songs() {
        let digest = "a".repeat(64);
        let with = |path: &str, stream: serde_json::Value| {
            manifest(&format!(
                r#"[{{"path":"{path}","bytes":4,"sha256":"{digest}","stream":{stream}}}]"#
            ))
        };
        let good = serde_json::json!({
            "url": format!("/content/{digest}.m4a"), "bytes": 3, "sha256": digest,
            "type": "audio/mp4", "seconds": 187.8,
        });
        let parsed = Manifest::parse(with("/AUDIO/MUSIC/AMPFIED.VAG", good.clone()).as_bytes());
        assert_eq!(
            parsed
                .unwrap()
                .record("/AUDIO/MUSIC/AMPFIED.VAG")
                .unwrap()
                .stream
                .as_ref()
                .unwrap()
                .url,
            format!("/content/{digest}.m4a")
        );
        let changes = [
            ("url", serde_json::json!("https://example.com/a.m4a")),
            ("url", serde_json::json!(format!("/content/{digest}.bin"))),
            ("type", serde_json::json!("audio/ogg")),
            ("type", serde_json::Value::Null),
            ("bytes", serde_json::json!(0)),
            ("bytes", serde_json::json!(MAX_CHUNK + 1)),
            ("seconds", serde_json::json!(0)),
            ("seconds", serde_json::json!(-1)),
            ("sha256", serde_json::json!("b".repeat(64))),
            ("sha256", serde_json::json!("A".repeat(64))),
        ];
        for (key, value) in changes {
            let mut altered = good.clone();
            altered[key] = value.clone();
            let error =
                Manifest::parse(with("/AUDIO/MUSIC/AMPFIED.VAG", altered).as_bytes()).unwrap_err();
            assert!(error.contains("music stream"), "{key}={value}: {error}");
        }
        for path in [
            "/UI/RESOURCE/PAGEA/PAGE14.RES",
            "/AUDIO/MUSIC/lower.VAG",
            "/AUDIO/SFX/A.VAG",
        ] {
            let error = Manifest::parse(with(path, good.clone()).as_bytes()).unwrap_err();
            assert!(error.contains("music stream"), "{path}: {error}");
        }
    }
}
