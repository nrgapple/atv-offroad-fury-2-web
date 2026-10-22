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
            return Err("unsupported content package".into());
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
                return Err(format!("invalid content record {}", record.path));
            }
            let mut end = 0;
            for chunk in record.chunks() {
                let digest = chunk.sha256.len() == 64
                    && chunk
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
                if !digest || chunk.offset != end || chunk.bytes == 0 || chunk.bytes > MAX_CHUNK {
                    return Err(format!("invalid content chunk in {}", record.path));
                }
                end += chunk.bytes;
            }
            if end != record.bytes {
                return Err(format!("truncated content record {}", record.path));
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
            Store::Http(base) => web::fetch(&format!("{base}{name}")).await,
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
}

const CACHE_BYTES: usize = 40 * 1024 * 1024;

impl Content {
    pub async fn open(store: Store) -> Result<Self> {
        let manifest = Manifest::parse(&store.read("manifest.json").await?)?;
        Ok(Self {
            manifest,
            store,
            cache: Default::default(),
        })
    }

    fn record_for(&self, path: &str) -> Result<&FileRecord> {
        self.manifest
            .record(path)
            .ok_or_else(|| format!("content package lacks {path}"))
    }

    async fn chunk(&self, path: &str, chunk: &Chunk) -> Result<std::rc::Rc<Vec<u8>>> {
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
        let bytes = self.store.read(&format!("{}.bin", chunk.sha256)).await?;
        // Hash in 512 KiB pieces, yielding between them, so verifying a large
        // file never holds one frame (other riders' models load mid-ride).
        let mut digest = Sha256::new();
        for piece in bytes.chunks(512 * 1024) {
            digest.update(piece);
            yield_once().await;
        }
        if bytes.len() != chunk.bytes || hex(&digest.finalize()) != chunk.sha256 {
            return Err(format!("content integrity check failed for {path}"));
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

    /// One whole original file, integrity checked chunk by chunk.
    pub async fn file(&self, path: &str) -> Result<Vec<u8>> {
        let record = self.record_for(path)?;
        let mut out = Vec::with_capacity(record.bytes);
        for chunk in record.chunks() {
            out.extend_from_slice(&self.chunk(path, &chunk).await?);
        }
        Ok(out)
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
            .ok_or_else(|| format!("read outside {path}"))?;
        let mut out = Vec::with_capacity(count);
        for chunk in record.chunks() {
            let (low, high) = (
                offset.max(chunk.offset),
                end.min(chunk.offset + chunk.bytes),
            );
            if low >= high {
                continue;
            }
            let bytes = self.chunk(path, &chunk).await?;
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
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    fn js(error: wasm_bindgen::JsValue) -> String {
        format!("{error:?}")
    }

    pub async fn fetch(url: &str) -> super::Result<Vec<u8>> {
        let window = web_sys::window().ok_or("no window")?;
        let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
            .await
            .map_err(js)?
            .dyn_into()
            .map_err(js)?;
        if !response.ok() {
            return Err(format!("{url}: HTTP {}", response.status()));
        }
        let buffer = JsFuture::from(response.array_buffer().map_err(js)?)
            .await
            .map_err(js)?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
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
}
