//! Mirrors the hosted content package (the R2 `manifest.json` and its
//! SHA-256 named blobs, exactly what the browser autoloads) into a local
//! directory that [`atv_content::Store::Directory`] reads. Android downloads
//! the game's runtime resources this way on first launch instead of carrying
//! them in the APK (research/ANDROID.md).
//!
//! - Every blob streams to `<name>.part`, is checked against its size and
//!   SHA-256, and only then renamed into place, so a present blob of the right
//!   size is a verified one: an interrupted sync resumes where it stopped.
//! - `manifest.json` is written last; until then the previous package (if
//!   any) stays complete and readable.
//! - Blobs the new manifest no longer references are removed afterwards.
//! - Without a network the last complete package is used as is.
// Built for Android, and on every host for its unit tests.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

/// The public package the browser loads (web/engine.js `/content/`).
pub const HOSTED: &str = "https://furyremastered.com/content/";

/// Parallel downloads: enough to hide per-request latency on a phone link.
const WORKERS: usize = 4;

/// Where blobs come from: `open(name)` yields the bytes of `name`
/// (`manifest.json`, `<sha256>.bin`, `<sha256>.m4a`).
pub trait Source: Send + Sync {
    fn open(&self, name: &str) -> Result<Box<dyn Read + Send>, String>;
}

/// The hosted package over HTTPS (ureq, rustls, webpki roots).
pub struct Https {
    base: String,
    agent: ureq::Agent,
}

impl Https {
    pub fn new(base: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_recv_body(Some(std::time::Duration::from_secs(60)))
            .build()
            .into();
        Self {
            base: base.to_string(),
            agent,
        }
    }
}

impl Source for Https {
    fn open(&self, name: &str) -> Result<Box<dyn Read + Send>, String> {
        let url = format!("{}{name}", self.base);
        let response = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| format!("{name}: {e}"))?;
        Ok(Box::new(response.into_body().into_reader()))
    }
}

/// What the sync is doing, for the progress screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    /// Bytes of blobs present and verified, and of the whole package.
    pub done: u64,
    pub total: u64,
    /// Blobs still to download in this run.
    pub missing: usize,
    /// The package was already complete (no download this run).
    pub current: bool,
    /// The hosted manifest could not be read; the local package is used.
    pub offline: bool,
    pub finished: bool,
    pub error: Option<String>,
}

/// One blob a manifest references: file name, SHA-256 and size.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Blob {
    pub name: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Every distinct blob of a manifest (chunks and soundtrack streams), after
/// the same validation the game applies when it opens the package.
pub fn blobs(manifest: &[u8]) -> Result<Vec<Blob>, String> {
    let parsed = atv_content::Manifest::parse(manifest)?;
    let mut out: Vec<Blob> = Vec::new();
    for record in &parsed.files {
        for chunk in record.chunks() {
            out.push(Blob {
                name: format!("{}.bin", chunk.sha256),
                sha256: chunk.sha256,
                bytes: chunk.bytes as u64,
            });
        }
        if let Some(stream) = &record.stream
            && let Some(sha256) = &stream.sha256
        {
            out.push(Blob {
                name: format!("{sha256}.m4a"),
                sha256: sha256.clone(),
                bytes: stream.bytes as u64,
            });
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// The blobs not yet present at their size in `dir`.
pub fn missing(dir: &Path, blobs: &[Blob]) -> Vec<Blob> {
    blobs
        .iter()
        .filter(|blob| {
            std::fs::metadata(dir.join(&blob.name)).map_or(true, |m| m.len() != blob.bytes)
        })
        .cloned()
        .collect()
}

/// The complete local package's manifest, if every blob it lists is present.
pub fn local_complete(dir: &Path) -> bool {
    std::fs::read(dir.join("manifest.json"))
        .ok()
        .and_then(|bytes| blobs(&bytes).ok())
        .is_some_and(|all| missing(dir, &all).is_empty())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads one blob to `<name>.part`, verifies it and renames it into
/// place; `progress` counts its bytes as they arrive.
fn fetch_blob(
    source: &dyn Source,
    dir: &Path,
    blob: &Blob,
    progress: &AtomicU64,
    stop: &AtomicBool,
) -> Result<(), String> {
    let part = dir.join(format!("{}.part", blob.name));
    let result = (|| {
        let mut reader = source.open(&blob.name)?;
        let mut file =
            std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut digest = Sha256::new();
        let mut buffer = vec![0; 256 * 1024];
        let mut written = 0u64;
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("stopped".to_string());
            }
            let read = reader
                .read(&mut buffer)
                .map_err(|e| format!("{}: {e}", blob.name))?;
            if read == 0 {
                break;
            }
            written += read as u64;
            if written > blob.bytes {
                return Err(format!("{}: larger than the manifest says", blob.name));
            }
            digest.update(&buffer[..read]);
            file.write_all(&buffer[..read])
                .map_err(|e| format!("{}: {e}", part.display()))?;
            progress.fetch_add(read as u64, Ordering::Relaxed);
        }
        if written != blob.bytes || hex(&digest.finalize()) != blob.sha256 {
            return Err(format!(
                "{}: download failed its integrity check",
                blob.name
            ));
        }
        file.sync_all()
            .map_err(|e| format!("{}: {e}", part.display()))?;
        std::fs::rename(&part, dir.join(&blob.name)).map_err(|e| format!("{}: {e}", blob.name))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// Brings `dir` up to the hosted package. `report` sees each progress
/// change (called from worker threads, so it must be cheap). Returns once
/// the directory holds a complete package, or the reason it cannot.
pub fn sync(
    source: &dyn Source,
    dir: &Path,
    report: &(dyn Fn(Progress) + Sync),
) -> Result<Progress, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let manifest = source.open("manifest.json").and_then(|mut reader| {
        let mut bytes = Vec::new();
        reader
            .by_ref()
            .take(16 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("manifest.json: {e}"))?;
        Ok(bytes)
    });
    let manifest = match manifest {
        Ok(bytes) => bytes,
        // Offline: ride on the last complete package.
        Err(error) if local_complete(dir) => {
            let progress = Progress {
                offline: true,
                current: true,
                finished: true,
                error: Some(error),
                ..Default::default()
            };
            report(progress.clone());
            return Ok(progress);
        }
        Err(error) => return Err(error),
    };
    let all = blobs(&manifest)?;
    let total: u64 = all.iter().map(|b| b.bytes).sum();
    let need = missing(dir, &all);
    let present: u64 = total - need.iter().map(|b| b.bytes).sum::<u64>();
    let mut progress = Progress {
        done: present,
        total,
        missing: need.len(),
        current: need.is_empty(),
        ..Default::default()
    };
    report(progress.clone());
    if !need.is_empty() {
        let next = AtomicUsize::new(0);
        let fetched = AtomicU64::new(0);
        let stop = AtomicBool::new(false);
        let failure: Mutex<Option<String>> = Mutex::new(None);
        let base = progress.clone();
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..WORKERS.min(need.len()))
                .map(|_| {
                    scope.spawn(|| {
                        while let Some(blob) = need.get(next.fetch_add(1, Ordering::Relaxed)) {
                            if stop.load(Ordering::Relaxed) {
                                break;
                            }
                            if let Err(error) = fetch_blob(source, dir, blob, &fetched, &stop) {
                                if !stop.swap(true, Ordering::Relaxed) {
                                    *failure.lock().unwrap() = Some(error);
                                }
                                break;
                            }
                        }
                    })
                })
                .collect();
            // Byte-level progress while the blobs stream in.
            while !workers.iter().all(|w| w.is_finished()) {
                report(Progress {
                    done: present + fetched.load(Ordering::Relaxed),
                    ..base.clone()
                });
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        });
        if let Some(error) = failure.into_inner().unwrap() {
            return Err(error);
        }
        progress.done = total;
    }
    // The package is complete: publish its manifest, then drop stale blobs.
    let manifest_path = dir.join("manifest.json");
    if std::fs::read(&manifest_path).ok().as_deref() != Some(&manifest[..]) {
        let temporary = dir.join("manifest.json.part");
        std::fs::write(&temporary, &manifest).map_err(|e| format!("manifest.json: {e}"))?;
        std::fs::rename(&temporary, &manifest_path).map_err(|e| format!("manifest.json: {e}"))?;
    }
    remove_unreferenced(dir, &all);
    progress.finished = true;
    report(progress.clone());
    Ok(progress)
}

/// Removes blobs and partial downloads `dir` holds that `all` does not list.
fn remove_unreferenced(dir: &Path, all: &[Blob]) {
    let keep: std::collections::HashSet<&str> = all.iter().map(|b| b.name.as_str()).collect();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let blob = name.ends_with(".bin") || name.ends_with(".m4a") || name.ends_with(".part");
        if blob && !keep.contains(name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The latest progress and, once the sync ended, its outcome.
type Shared = Arc<Mutex<(Progress, Option<Result<(), String>>)>>;

/// A sync on its own thread, polled by the event loop.
pub struct Background {
    pub dir: PathBuf,
    state: Shared,
}

impl Background {
    pub fn start(source: Arc<dyn Source>, dir: PathBuf) -> Self {
        let state: Shared = Default::default();
        let shared = state.clone();
        let target = dir.clone();
        std::thread::spawn(move || {
            let report = |progress: Progress| shared.lock().unwrap().0 = progress;
            let result = sync(source.as_ref(), &target, &report).map(|_| ());
            shared.lock().unwrap().1 = Some(result);
        });
        Self { dir, state }
    }

    /// The latest progress, and the outcome once the sync ended.
    pub fn poll(&self) -> (Progress, Option<Result<(), String>>) {
        let state = self.state.lock().unwrap();
        (state.0.clone(), state.1.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const BUILD: &str = "11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49";

    struct Memory {
        files: HashMap<String, Vec<u8>>,
        opened: Mutex<Vec<String>>,
    }

    impl Source for Memory {
        fn open(&self, name: &str) -> Result<Box<dyn Read + Send>, String> {
            self.opened.lock().unwrap().push(name.to_string());
            self.files
                .get(name)
                .cloned()
                .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read + Send>)
                .ok_or_else(|| format!("{name}: 404"))
        }
    }

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    /// A two-file package: one single-chunk file and one two-chunk file.
    fn package(a: &[u8], b: &[u8], c: &[u8]) -> Memory {
        let manifest = format!(
            r#"{{"version":1,"source_build_sha256":"{BUILD}","files":[
            {{"path":"/A.RES","bytes":{},"sha256":"{}"}},
            {{"path":"/B.VAG","bytes":{},"chunks":[
                {{"offset":0,"bytes":{},"sha256":"{}"}},
                {{"offset":{},"bytes":{},"sha256":"{}"}}]}}]}}"#,
            a.len(),
            sha(a),
            b.len() + c.len(),
            b.len(),
            sha(b),
            b.len(),
            c.len(),
            sha(c)
        );
        let mut files = HashMap::new();
        files.insert("manifest.json".to_string(), manifest.into_bytes());
        for blob in [a, b, c] {
            files.insert(format!("{}.bin", sha(blob)), blob.to_vec());
        }
        Memory {
            files,
            opened: Default::default(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fury-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn syncs_resumes_and_removes_stale_blobs() {
        let dir = scratch("resume");
        let source = package(b"alpha", b"bravo", b"charlie");
        let done = sync(&source, &dir, &|_| {}).unwrap();
        assert!(done.finished && !done.current);
        assert_eq!(done.total, 17);
        assert!(local_complete(&dir));
        // A second run downloads nothing but the manifest.
        source.opened.lock().unwrap().clear();
        let again = sync(&source, &dir, &|_| {}).unwrap();
        assert!(again.current);
        assert_eq!(*source.opened.lock().unwrap(), vec!["manifest.json"]);
        // A removed blob is fetched again, alone.
        std::fs::remove_file(dir.join(format!("{}.bin", sha(b"bravo")))).unwrap();
        source.opened.lock().unwrap().clear();
        sync(&source, &dir, &|_| {}).unwrap();
        assert_eq!(source.opened.lock().unwrap().len(), 2);
        // A new package replaces the old one's blobs.
        let newer = package(b"alpha", b"delta", b"echo");
        sync(&newer, &dir, &|_| {}).unwrap();
        assert!(!dir.join(format!("{}.bin", sha(b"bravo"))).exists());
        assert!(dir.join(format!("{}.bin", sha(b"delta"))).exists());
        assert!(dir.join(format!("{}.bin", sha(b"alpha"))).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_download_is_rejected_and_old_package_kept() {
        let dir = scratch("corrupt");
        let good = package(b"alpha", b"bravo", b"charlie");
        sync(&good, &dir, &|_| {}).unwrap();
        let mut bad = package(b"alpha", b"delta", b"echo");
        // Same length, wrong bytes.
        bad.files
            .insert(format!("{}.bin", sha(b"delta")), b"DELTA".to_vec());
        let error = sync(&bad, &dir, &|_| {}).unwrap_err();
        assert!(error.contains("integrity"), "{error}");
        // No partial file is left, and the previous package still opens.
        assert!(local_complete(&dir));
        assert!(
            std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".part"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn offline_uses_a_complete_local_package_only() {
        let dir = scratch("offline");
        let offline = Memory {
            files: HashMap::new(),
            opened: Default::default(),
        };
        assert!(sync(&offline, &dir, &|_| {}).is_err());
        sync(&package(b"alpha", b"bravo", b"charlie"), &dir, &|_| {}).unwrap();
        let used = sync(&offline, &dir, &|_| {}).unwrap();
        assert!(used.offline && used.finished);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_manifests_the_game_would_reject() {
        assert!(blobs(br#"{"version":1,"source_build_sha256":"00","files":[]}"#).is_err());
    }
}
