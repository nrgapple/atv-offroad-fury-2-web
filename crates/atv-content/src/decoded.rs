//! Persistent cache of decoded and converted content (web), so a course the
//! player has loaded before opens without repeating its decode work.
//!
//! The browser's HTTP cache and the offline service worker already keep the
//! downloaded chunks (research/OFFLINE.md); a second load still inflated the
//! course archive and decoded its terrain layers, sky tiles and sounds. This
//! keeps those results in Cache Storage (research/PERFORMANCE.md, "Decoded
//! course cache"):
//!
//! - one cache, [`CACHE`], whose entries are named by the content package
//!   (a digest of `manifest.json`), this container's [`FORMAT`] and a key the
//!   caller builds from its own format version and the exact input; a new
//!   package or format never reads an older entry;
//! - each entry carries a header (magic, format, its key, length, checksum)
//!   that is checked on every read; a mismatch deletes the entry and the
//!   caller decodes normally;
//! - a least-recently-used index in `localStorage` bounds the total size
//!   ([`MAX_BYTES`], less when the origin's quota is small);
//! - no Cache Storage (an insecure page, a private window), a full disk or
//!   any storage error is a miss: loading works exactly as without it.
//!
//! Desktop builds read local files and decode on threads; there the cache
//! is off and every call is a cheap miss.
// The container, index and checks are tested on every target and used on
// the web.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Entry container version: bump when the header or naming changes.
pub const FORMAT: u32 = 1;
/// The Cache Storage cache holding every entry. Not a `fury-` name: the
/// offline worker deletes unknown `fury-` caches when it activates.
pub const CACHE: &str = "atv-decoded-v1";
/// Upper bound of the stored entries' bytes.
pub const MAX_BYTES: usize = 192 * 1024 * 1024;
/// A single entry larger than this is not stored.
const MAX_ENTRY: usize = 48 * 1024 * 1024;
const MAGIC: &[u8; 4] = b"ATVD";

/// What the cache did since the page loaded (`?uiDiagnostics=1`
/// `decodedCache`, tools/test_engine_decoded_cache.mjs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    /// Cache Storage opened for this package.
    pub enabled: bool,
    pub hits: u32,
    pub misses: u32,
    /// Entries written (completed).
    pub stores: u32,
    /// Entries found damaged or mismatched, deleted and decoded again.
    pub rejected: u32,
    /// Writes or reads that failed (quota, storage errors).
    pub failed: u32,
    pub hit_bytes: u64,
    /// Milliseconds spent reading hits.
    pub read_ms: f64,
    /// Writes still in flight.
    pub pending: u32,
    /// Background productions and writes not finished (claimed keys).
    pub background: u32,
    /// Entries and bytes in the index.
    pub entries: u32,
    pub bytes: u64,
    /// The bound in force ([`MAX_BYTES`] or less).
    pub limit: u64,
}

impl Stats {
    /// The diagnostics form (`decodedCache`, `window.atvDecodedCache`).
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled, "hits": self.hits, "misses": self.misses,
            "stores": self.stores, "rejected": self.rejected, "failed": self.failed,
            "hitBytes": self.hit_bytes, "readMs": (self.read_ms * 10.).round() / 10.,
            "pending": self.pending, "background": self.background,
            "entries": self.entries, "bytes": self.bytes, "limit": self.limit,
        })
    }
}

/// A fast 64-bit hash of `bytes` (not cryptographic): entry checksums and
/// input keys. Inputs here come from integrity-checked content. Four
/// independent lanes keep a phone's multiplier busy on large entries.
pub fn hash64(bytes: &[u8], seed: u64) -> u64 {
    const K: [u64; 4] = [
        0x9e37_79b9_7f4a_7c15,
        0xc2b2_ae3d_27d4_eb4f,
        0x1656_67b1_9e37_79f9,
        0x85eb_ca77_c2b2_ae63,
    ];
    let word = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap());
    let mut lanes = K.map(|k| seed ^ k);
    let (blocks, rest) = bytes.as_chunks::<32>();
    for block in blocks {
        for (i, lane) in lanes.iter_mut().enumerate() {
            *lane = (*lane ^ word(&block[i * 8..i * 8 + 8]))
                .wrapping_mul(K[i])
                .rotate_left(29);
        }
    }
    let mut h = seed ^ (bytes.len() as u64).wrapping_mul(K[0]);
    for lane in lanes {
        h = (h ^ lane).wrapping_mul(K[1]).rotate_left(27);
    }
    let (words, tail) = rest.as_chunks::<8>();
    for w in words {
        h = (h ^ u64::from_le_bytes(*w))
            .wrapping_mul(K[2])
            .rotate_left(31);
    }
    let mut last = [0u8; 8];
    last[..tail.len()].copy_from_slice(tail);
    h = (h ^ u64::from_le_bytes(last)).wrapping_mul(K[3]);
    // splitmix64 finalizer.
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

/// A key part naming `bytes` exactly enough for a cache key: its length and
/// two independently seeded hashes (128 bits).
pub fn fingerprint(bytes: &[u8]) -> String {
    format!(
        "{:x}-{:016x}{:016x}",
        bytes.len(),
        hash64(bytes, 0x5eed_0001),
        hash64(bytes, 0x5eed_0002)
    )
}

/// The stored form of `payload` under `key`.
fn header(key: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(28 + key.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&hash64(payload, 0xc4ec_c5a4).to_le_bytes());
    out.extend_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out
}

/// The header length of an entry stored under `key`.
fn header_len(key: &str) -> usize {
    28 + key.len()
}

/// Checks a stored entry's header (`head`, [`header_len`] bytes) against
/// `key` and its payload.
fn check(key: &str, head: &[u8], payload: &[u8]) -> bool {
    if head.len() != header_len(key) {
        return false;
    }
    let word = |at: usize| u64::from_le_bytes(head[at..at + 8].try_into().unwrap());
    &head[..4] == MAGIC
        && head[4..8] == FORMAT.to_le_bytes()
        && word(8) == payload.len() as u64
        && head[24..28] == (key.len() as u32).to_le_bytes()
        && &head[28..] == key.as_bytes()
        && word(16) == hash64(payload, 0xc4ec_c5a4)
}

/// The least-recently-used index: entry key -> (bytes, last use).
#[derive(Clone, Debug, Default, PartialEq)]
struct Index {
    namespace: String,
    entries: Vec<(String, u64, f64)>,
}

impl Index {
    fn total(&self) -> u64 {
        self.entries.iter().map(|e| e.1).sum()
    }

    fn touch(&mut self, key: &str, now: f64) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.0 == key) {
            entry.2 = now;
        }
    }

    fn remove(&mut self, key: &str) {
        self.entries.retain(|e| e.0 != key);
    }

    /// Keys to delete so `incoming` more bytes fit under `limit`, oldest
    /// first; they are removed from the index.
    fn make_room(&mut self, incoming: u64, limit: u64) -> Vec<String> {
        self.entries.sort_by(|a, b| a.2.total_cmp(&b.2));
        let mut total = self.total();
        let mut evicted = Vec::new();
        while total + incoming > limit && !self.entries.is_empty() {
            let (key, bytes, _) = self.entries.remove(0);
            total -= bytes;
            evicted.push(key);
        }
        evicted
    }

    /// Whether `incoming` more bytes fit under `limit` counting only entries
    /// last used before `since` as replaceable.
    fn room_for(&self, incoming: u64, limit: u64, since: f64) -> bool {
        let recent: u64 = self
            .entries
            .iter()
            .filter(|e| e.2 >= since)
            .map(|e| e.1)
            .sum();
        recent + incoming <= limit
    }

    fn to_json(&self) -> String {
        serde_json::json!({
            "namespace": self.namespace,
            "entries": self.entries.iter().map(|(k, b, t)| serde_json::json!([k, b, t])).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn parse(text: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let entries = value["entries"]
            .as_array()?
            .iter()
            .filter_map(|e| Some((e[0].as_str()?.to_string(), e[1].as_u64()?, e[2].as_f64()?)))
            .collect();
        Some(Self {
            namespace: value["namespace"].as_str()?.to_string(),
            entries,
        })
    }
}

/// Runs `futures` concurrently on the caller's task; outputs in order.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
async fn join_all<F: std::future::Future>(futures: Vec<F>) -> Vec<F::Output>
where
    F::Output: Unpin,
{
    struct JoinAll<F: std::future::Future> {
        futures: Vec<Option<std::pin::Pin<Box<F>>>>,
        outputs: Vec<Option<F::Output>>,
    }
    impl<F: std::future::Future> std::future::Future for JoinAll<F>
    where
        F::Output: Unpin,
    {
        type Output = Vec<F::Output>;
        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            // Boxed futures and plain vectors: the struct is Unpin.
            let this = self.get_mut();
            let mut done = true;
            for (slot, out) in this.futures.iter_mut().zip(this.outputs.iter_mut()) {
                if let Some(future) = slot {
                    match future.as_mut().poll(cx) {
                        std::task::Poll::Ready(value) => {
                            *out = Some(value);
                            *slot = None;
                        }
                        std::task::Poll::Pending => done = false,
                    }
                }
            }
            if done {
                std::task::Poll::Ready(this.outputs.iter_mut().filter_map(Option::take).collect())
            } else {
                std::task::Poll::Pending
            }
        }
    }
    let count = futures.len();
    JoinAll {
        futures: futures.into_iter().map(|f| Some(Box::pin(f))).collect(),
        outputs: (0..count).map(|_| None).collect(),
    }
    .await
}

pub use imp::*;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::Stats;

    /// Desktop: no cache (local files, threaded decodes).
    pub fn configure(_manifest: &[u8]) {}
    pub fn enabled() -> bool {
        false
    }
    pub async fn get(_key: &str) -> Option<Vec<u8>> {
        None
    }
    pub async fn get_many(keys: &[String]) -> Vec<Option<Vec<u8>>> {
        vec![None; keys.len()]
    }
    pub fn put(_key: &str, _payload: &[u8]) {}
    pub fn put_now(_key: &str, _payload: &[u8]) {}
    pub fn contains(_key: &str) -> bool {
        false
    }
    pub fn room_for(_bytes: u64) -> bool {
        false
    }
    pub async fn sleep(_ms: i32) {}
    pub fn hidden() -> bool {
        false
    }
    pub fn publish(_name: &str, _json: &str) {}
    pub fn diagnostics(_on: bool) {}
    /// Whether a background job for `key` should start (never on desktop).
    pub fn claim(_key: &str) -> bool {
        false
    }
    pub fn release(_key: &str) {}
    pub fn spawn(_task: impl std::future::Future<Output = ()> + 'static) {}
    pub async fn quiet() {}
    pub struct LoadGuard;
    pub fn loading() -> LoadGuard {
        LoadGuard
    }
    pub fn stats() -> Stats {
        Stats::default()
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::{CACHE, FORMAT, Index, MAX_BYTES, MAX_ENTRY, Stats, check, header, header_len};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    const INDEX: &str = "atv-decoded-index-v1";

    thread_local! {
        /// Loads in progress ([`loading`]).
        static LOADS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        /// [`diagnostics`] is on.
        static DIAGNOSTICS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Publishes [`stats`] as `window.atvDecodedCache` whenever they change
    /// (`?uiDiagnostics=1`). Tests read it there: an idle menu draws no
    /// frames, so the frame-published `#ui-state` copy goes stale while
    /// background writes finish.
    pub fn diagnostics(on: bool) {
        DIAGNOSTICS.with(|d| d.set(on));
        changed();
    }

    /// Republishes the stats after a change (diagnostics only).
    fn changed() {
        if DIAGNOSTICS.with(|d| d.get()) {
            publish("atvDecodedCache", &stats().json().to_string());
        }
    }

    /// Marks a load running until the guard drops: background cache work
    /// ([`quiet`]) waits for it, leaving the load the workers and thread.
    pub fn loading() -> LoadGuard {
        LOADS.with(|l| l.set(l.get() + 1));
        LoadGuard
    }

    pub struct LoadGuard;

    impl Drop for LoadGuard {
        fn drop(&mut self) {
            LOADS.with(|l| l.set(l.get().saturating_sub(1)));
        }
    }

    /// Resolves a second after no load is running.
    pub async fn quiet() {
        loop {
            sleep(1000).await;
            if LOADS.with(|l| l.get()) == 0 {
                return;
            }
        }
    }

    #[derive(Default)]
    struct State {
        index: Index,
        /// `None` until configured, or when Cache Storage is unusable.
        on: bool,
        limit: u64,
        stats: Stats,
        /// Keys being written or produced in the background.
        claimed: HashSet<String>,
        /// When this page opened the cache ([`room_for`]).
        opened: f64,
        /// The last large hit, so a second read in the same load (the
        /// venue and the ride both open the course archive) is a copy.
        recent: Option<(String, std::rc::Rc<Vec<u8>>)>,
        save_scheduled: bool,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }

    fn now() -> f64 {
        js_sys::Date::now()
    }

    fn perf() -> f64 {
        web_sys::window()
            .and_then(|w| w.performance())
            .map_or_else(js_sys::Date::now, |p| p.now())
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    fn caches() -> Option<web_sys::CacheStorage> {
        let window = web_sys::window()?;
        // `caches` is missing on insecure origins; reading it can throw.
        let present = js_sys::Reflect::has(&window, &"caches".into()).unwrap_or(false);
        if !present {
            return None;
        }
        window.caches().ok()
    }

    async fn open() -> Option<web_sys::Cache> {
        let cache = JsFuture::from(caches()?.open(CACHE)).await.ok()?;
        cache.dyn_into().ok()
    }

    fn url(namespace: &str, key: &str) -> String {
        format!("/__atv-decoded/{namespace}/{key}")
    }

    /// Writes the index a moment later, once for a burst of changes (a
    /// load reads and writes many entries). A page closed before then
    /// leaves entries the next start reconciles.
    fn schedule_save(s: &mut State) {
        if s.save_scheduled {
            return;
        }
        s.save_scheduled = true;
        wasm_bindgen_futures::spawn_local(async {
            sleep(500).await;
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.save_scheduled = false;
                save(&s.index);
            });
        });
    }

    fn save(index: &Index) {
        if let Some(storage) = storage() {
            let _ = storage.set_item(INDEX, &index.to_json());
        }
    }

    fn disabled_by_page() -> bool {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|q| {
                q.trim_start_matches('?')
                    .split('&')
                    .any(|p| p == "decodedCache=0")
            })
    }

    /// Opens the cache for the package whose manifest is `manifest`: entries
    /// of other packages or formats are dropped, the index is reconciled
    /// with the stored entries and the size bound is set from the quota.
    pub fn configure(manifest: &[u8]) {
        if disabled_by_page() || caches().is_none() {
            return;
        }
        let namespace = format!("{FORMAT}-{:016x}", super::hash64(manifest, 0x6d61_6e69));
        if STATE.with(|s| s.borrow().on && s.borrow().index.namespace == namespace) {
            return;
        }
        let stored = storage()
            .and_then(|s| s.get_item(INDEX).ok().flatten())
            .and_then(|text| Index::parse(&text));
        let fresh = stored.as_ref().is_none_or(|i| i.namespace != namespace);
        let index = match stored {
            Some(index) if !fresh => index,
            _ => Index {
                namespace: namespace.clone(),
                entries: Vec::new(),
            },
        };
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.index = index;
            s.on = true;
            s.opened = now();
            s.limit = MAX_BYTES as u64;
            s.stats.enabled = true;
        });
        wasm_bindgen_futures::spawn_local(async move {
            if fresh {
                // Another package or format: nothing stored is usable.
                if let Some(caches) = caches() {
                    let _ = JsFuture::from(caches.delete(CACHE)).await;
                }
                STATE.with(|s| schedule_save(&mut s.borrow_mut()));
            } else {
                reconcile(&namespace).await;
            }
            limit_from_quota().await;
        });
    }

    /// Deletes stored entries the index does not list (a write the page
    /// left unfinished) and forgets index entries with nothing stored.
    async fn reconcile(namespace: &str) {
        let Some(cache) = open().await else {
            return;
        };
        let Ok(keys) = JsFuture::from(cache.keys()).await else {
            return;
        };
        let prefix = url(namespace, "");
        let mut present = HashSet::new();
        for request in js_sys::Array::from(&keys).iter() {
            let Ok(request) = request.dyn_into::<web_sys::Request>() else {
                continue;
            };
            let path = web_sys::Url::new(&request.url())
                .map(|u| u.pathname())
                .unwrap_or_default();
            let key = path.strip_prefix(&prefix).map(str::to_string);
            let listed = key.as_ref().is_some_and(|k| {
                STATE.with(|s| s.borrow().index.entries.iter().any(|e| &e.0 == k))
            });
            if listed {
                present.extend(key);
            } else {
                let _ = JsFuture::from(cache.delete_with_request(&request)).await;
            }
        }
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let pending = s.claimed.clone();
            s.index
                .entries
                .retain(|e| present.contains(&e.0) || pending.contains(&e.0));
            schedule_save(&mut s);
        });
    }

    /// A quarter of the origin's free quota, at most [`MAX_BYTES`].
    async fn limit_from_quota() {
        let Some(manager) = web_sys::window().map(|w| w.navigator().storage()) else {
            return;
        };
        let Ok(promise) = manager.estimate() else {
            return;
        };
        let Ok(estimate) = JsFuture::from(promise).await else {
            return;
        };
        let get = |key: &str| {
            js_sys::Reflect::get(&estimate, &key.into())
                .ok()
                .and_then(|v| v.as_f64())
        };
        if let (Some(quota), Some(usage)) = (get("quota"), get("usage")) {
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                let ours = s.index.total() as f64;
                let free = (quota - usage).max(0.) + ours;
                s.limit = (MAX_BYTES as f64).min(free / 4.) as u64;
            });
        }
    }

    pub fn enabled() -> bool {
        STATE.with(|s| s.borrow().on)
    }

    fn namespace() -> String {
        STATE.with(|s| s.borrow().index.namespace.clone())
    }

    fn fail(key: &str) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.stats.failed += 1;
            s.index.remove(key);
            schedule_save(&mut s);
        });
    }

    /// The payload stored under `key`, if present and intact.
    pub async fn get(key: &str) -> Option<Vec<u8>> {
        if !enabled() {
            return None;
        }
        let recent = STATE.with(|s| {
            let s = s.borrow();
            s.recent
                .as_ref()
                .filter(|(k, _)| k == key)
                .map(|(_, bytes)| bytes.clone())
        });
        if let Some(bytes) = recent {
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.stats.hits += 1;
                s.stats.hit_bytes += bytes.len() as u64;
            });
            changed();
            return Some(bytes.to_vec());
        }
        let listed = STATE.with(|s| s.borrow().index.entries.iter().any(|e| e.0 == key));
        let started = perf();
        let found = if listed { read(key).await } else { None };
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            match &found {
                Some(bytes) => {
                    s.stats.hits += 1;
                    s.stats.hit_bytes += bytes.len() as u64;
                    s.stats.read_ms += perf() - started;
                    s.index.touch(key, now());
                    schedule_save(&mut s);
                    if bytes.len() >= 1 << 20 {
                        s.recent = Some((key.to_string(), std::rc::Rc::new(bytes.clone())));
                    }
                }
                None => s.stats.misses += 1,
            }
        });
        changed();
        found
    }

    /// [`get`] of every key, read concurrently, in order.
    pub async fn get_many(keys: &[String]) -> Vec<Option<Vec<u8>>> {
        super::join_all(keys.iter().map(|k| get(k)).collect()).await
    }

    async fn read(key: &str) -> Option<Vec<u8>> {
        let cache = open().await?;
        let address = url(&namespace(), key);
        let found = JsFuture::from(cache.match_with_str(&address)).await.ok()?;
        let Ok(response) = found.dyn_into::<web_sys::Response>() else {
            // Listed but not stored (cleared by the browser or another tab).
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.index.remove(key);
                schedule_save(&mut s);
            });
            return None;
        };
        let buffer = match response.array_buffer() {
            Ok(promise) => JsFuture::from(promise).await.ok(),
            Err(_) => None,
        };
        let Some(buffer) = buffer else {
            fail(key);
            return None;
        };
        let entry = js_sys::Uint8Array::new(&buffer);
        let size = header_len(key);
        if (entry.length() as usize) < size {
            reject(&cache, &address, key).await;
            return None;
        }
        let head = entry.subarray(0, size as u32).to_vec();
        let payload = entry.subarray(size as u32, entry.length()).to_vec();
        if !check(key, &head, &payload) {
            reject(&cache, &address, key).await;
            return None;
        }
        Some(payload)
    }

    async fn reject(cache: &web_sys::Cache, address: &str, key: &str) {
        let _ = JsFuture::from(cache.delete_with_str(address)).await;
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.stats.rejected += 1;
            s.index.remove(key);
            schedule_save(&mut s);
        });
    }

    /// Stores `payload` under `key` in the background (the bytes are copied
    /// at once); the oldest entries make room. Errors only count.
    pub fn put(key: &str, payload: &[u8]) {
        store(key, payload, false);
    }

    /// [`put`] for work done outside a load (a menu preload): the write
    /// starts at once, so a Start moments later can already read it. It
    /// still waits for a load that is running.
    pub fn put_now(key: &str, payload: &[u8]) {
        store(key, payload, true);
    }

    fn store(key: &str, payload: &[u8], immediate: bool) {
        if !enabled() || payload.len() > MAX_ENTRY {
            return;
        }
        let head = header(key, payload);
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(&head[..]));
        // SAFETY: a view of the module's memory, copied by the Blob
        // constructor right below before anything can allocate.
        parts.push(&unsafe { js_sys::Uint8Array::view(payload) }.into());
        let Ok(entry) = web_sys::Blob::new_with_u8_array_sequence(&parts) else {
            return;
        };
        drop(parts);
        let bytes = (head.len() + payload.len()) as u64;
        let (evicted, namespace) = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.index.remove(key);
            let limit = s.limit;
            let evicted = s.index.make_room(bytes, limit);
            // Reserved now so concurrent writes count it.
            s.index.entries.push((key.to_string(), bytes, now()));
            s.claimed.insert(key.to_string());
            s.stats.pending += 1;
            (evicted, s.index.namespace.clone())
        });
        changed();
        let key = key.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            // Written once the load that produced it has finished, so the
            // transfer to storage does not compete with it.
            if !immediate || LOADS.with(|l| l.get()) > 0 {
                quiet().await;
            }
            let stored = async {
                let cache = open().await?;
                for old in &evicted {
                    let _ = JsFuture::from(cache.delete_with_str(&url(&namespace, old))).await;
                }
                let response = web_sys::Response::new_with_opt_blob(Some(&entry)).ok()?;
                JsFuture::from(cache.put_with_str(&url(&namespace, &key), &response))
                    .await
                    .ok()
            }
            .await;
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.stats.pending -= 1;
                s.claimed.remove(&key);
                match stored {
                    Some(_) => s.stats.stores += 1,
                    None => {
                        // Quota or storage error: forget it and free the
                        // oldest half so the next write can fit.
                        s.stats.failed += 1;
                        s.index.remove(&key);
                        let total = s.index.total();
                        let more = s.index.make_room(total / 2, total);
                        let namespace = s.index.namespace.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Some(cache) = open().await {
                                for old in more {
                                    let _ = JsFuture::from(
                                        cache.delete_with_str(&url(&namespace, &old)),
                                    )
                                    .await;
                                }
                            }
                        });
                    }
                }
                schedule_save(&mut s);
            });
            changed();
        });
    }

    /// Whether `key` is in the index (stored or being written).
    pub fn contains(key: &str) -> bool {
        STATE.with(|s| s.borrow().index.entries.iter().any(|e| e.0 == key))
    }

    /// Whether `bytes` more fit without evicting anything this page used:
    /// a preload may replace an earlier visit's entries, never the course
    /// the player just rode or looked at.
    pub fn room_for(bytes: u64) -> bool {
        STATE.with(|s| {
            let s = s.borrow();
            s.on && s.index.room_for(bytes, s.limit, s.opened)
        })
    }

    /// Publishes a JSON diagnostic as `window.<name>` (browser tests).
    pub fn publish(name: &str, json: &str) {
        if let Some(window) = web_sys::window() {
            let _ = js_sys::Reflect::set(&window, &name.into(), &json.into());
        }
    }

    /// Whether the page is in the background.
    pub fn hidden() -> bool {
        web_sys::window()
            .and_then(|w| js_sys::Reflect::get(&w, &"document".into()).ok())
            .and_then(|d| js_sys::Reflect::get(&d, &"hidden".into()).ok())
            .is_some_and(|v| v.as_bool() == Some(true))
    }

    /// Claims a background production of `key` (an entry not stored and
    /// not already being produced); release by [`put`] or [`release`].
    pub fn claim(key: &str) -> bool {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let stored = s.index.entries.iter().any(|e| e.0 == key);
            s.on && !stored && s.claimed.insert(key.to_string())
        })
    }

    /// Ends a [`claim`] that stored nothing.
    pub fn release(key: &str) {
        STATE.with(|s| s.borrow_mut().claimed.remove(key));
        changed();
    }

    /// Runs `task` in the background on the page's executor.
    pub fn spawn(task: impl std::future::Future<Output = ()> + 'static) {
        wasm_bindgen_futures::spawn_local(task);
    }

    /// Waits about `ms` milliseconds (background work after a load).
    pub async fn sleep(ms: i32) {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
            }
        });
        let _ = JsFuture::from(promise).await;
    }

    pub fn stats() -> Stats {
        STATE.with(|s| {
            let s = s.borrow();
            Stats {
                entries: s.index.entries.len() as u32,
                background: s.claimed.len() as u32,
                bytes: s.index.total(),
                limit: s.limit,
                ..s.stats.clone()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preloads_replace_only_older_visits() {
        let index = Index {
            namespace: "n".into(),
            entries: vec![("old".into(), 60, 1.), ("today".into(), 30, 10.)],
        };
        // The old visit's bytes may go, this page's may not.
        assert!(index.room_for(70, 100, 5.));
        assert!(!index.room_for(71, 100, 5.));
        // With no older visit nothing is replaceable.
        assert!(index.room_for(10, 100, 0.));
        assert!(!index.room_for(11, 100, 0.));
    }

    #[test]
    fn entries_round_trip_and_reject_damage() {
        let payload = b"decoded pixels".repeat(5);
        let head = header("job/1/abc", &payload);
        assert_eq!(head.len(), header_len("job/1/abc"));
        assert!(check("job/1/abc", &head, &payload));
        // Another key, a flipped payload byte, a truncation: all rejected.
        assert!(!check("job/1/abd", &head, &payload));
        let mut flipped = payload.clone();
        *flipped.last_mut().unwrap() ^= 1;
        assert!(!check("job/1/abc", &head, &flipped));
        assert!(!check("job/1/abc", &head, &payload[..payload.len() - 1]));
        assert!(!check("job/1/abc", &head[..10], &payload));
        let mut format = head.clone();
        format[4] ^= 1;
        assert!(!check("job/1/abc", &format, &payload));
    }

    #[test]
    fn fingerprints_follow_every_byte() {
        let a = vec![7u8; 1001];
        // A change in any lane, the word tail or the byte tail.
        for at in [0, 9, 17, 31, 990, 1000] {
            let mut b = a.clone();
            b[at] = 8;
            assert_ne!(fingerprint(&a), fingerprint(&b), "{at}");
        }
        assert_ne!(fingerprint(&a), fingerprint(&a[..1000]));
        assert_eq!(fingerprint(&a), fingerprint(&a.clone()));
        assert_ne!(hash64(&[], 1), hash64(&[0], 1));
    }

    #[test]
    fn index_evicts_least_recently_used_first() {
        let mut index = Index {
            namespace: "1-x".into(),
            entries: vec![
                ("a".into(), 40, 3.),
                ("b".into(), 40, 1.),
                ("c".into(), 40, 2.),
            ],
        };
        index.touch("b", 9.);
        assert_eq!(index.make_room(50, 100), ["c", "a"]);
        assert_eq!(index.entries, [("b".to_string(), 40, 9.)]);
        assert!(index.make_room(10, 100).is_empty());
        let back = Index::parse(&index.to_json()).unwrap();
        assert_eq!(back, index);
        assert!(Index::parse("not json").is_none());
    }
}
