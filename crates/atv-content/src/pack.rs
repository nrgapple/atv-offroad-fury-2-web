//! Single-file content pack: the hosted package (`manifest.json` plus
//! SHA-256 addressed chunks) flattened into one file so desktop builds can
//! carry their runtime resources inside the executable or app bundle.
//!
//! Layout, little endian (built by `tools/pack_content.py`):
//!
//! ```text
//! 0   8   magic "FURYPAK1"
//! 8   4   format version (1)
//! 12  4   blob count N
//! 16  8   manifest offset
//! 24  8   manifest length
//! 32  32  SHA-256 of the manifest
//! 64  48*N index sorted by digest: sha256[32], offset u64, length u64
//!         manifest.json bytes, then the blobs
//! ```
//!
//! A release executable can also carry the pack as an appended overlay: the
//! pack bytes followed by a 16 byte trailer, `"FURYEXE1"` and the pack length
//! (u64). `tools/build_desktop.py` appends it after the content-free binary is
//! built, so cross builds never hold the content in the compiler.
//!
//! Blobs are stored uncompressed: the runtime files barely compress (about
//! 9% overall, none for `.RES`) and decompression would only slow startup.
//! Integrity is still end to end. The manifest digest is checked when the
//! pack opens and `Content::file` re-hashes every chunk it reads against the
//! manifest, exactly as for a package directory.
use super::{Result, hex};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"FURYPAK1";
const HEADER: usize = 64;
const ENTRY: usize = 48;
const MAX_BLOBS: usize = 4096;
const MAX_BLOB: u64 = 64 * 1024 * 1024;
const MAX_MANIFEST: u64 = 4 * 1024 * 1024;
/// File name looked for beside the executable (or in a macOS bundle).
pub const SIDECAR: &str = "content.pack";
/// Trailer magic of a pack appended to an executable.
pub const TRAILER_MAGIC: &[u8; 8] = b"FURYEXE1";
const TRAILER: u64 = 16;

enum Source {
    Static(&'static [u8]),
    /// A file region: the pack starts `base` bytes in and spans `len` bytes.
    File {
        file: std::sync::Mutex<std::fs::File>,
        base: u64,
        len: u64,
    },
}

pub struct Pack {
    source: Source,
    manifest: Vec<u8>,
    /// `(digest, offset, length)` sorted by digest.
    index: Vec<([u8; 32], u64, u64)>,
}

impl Source {
    fn len(&self) -> Result<u64> {
        match self {
            Source::Static(bytes) => Ok(bytes.len() as u64),
            Source::File { len, .. } => Ok(*len),
        }
    }

    fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        match self {
            Source::Static(bytes) => {
                let end = offset.checked_add(len).filter(|&e| e <= bytes.len() as u64);
                let end = end.ok_or("content pack is truncated")?;
                Ok(bytes[offset as usize..end as usize].to_vec())
            }
            Source::File {
                file,
                base,
                len: total,
            } => {
                use std::io::{Read, Seek, SeekFrom};
                if offset.checked_add(len).is_none_or(|end| end > *total) {
                    return Err("content pack is truncated".into());
                }
                let mut file = file.lock().map_err(|_| "content pack lock".to_string())?;
                let mut out = vec![0; len as usize];
                file.seek(SeekFrom::Start(base + offset))
                    .and_then(|_| file.read_exact(&mut out))
                    .map_err(|e| format!("content pack: {e}"))?;
                Ok(out)
            }
        }
    }
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

impl Pack {
    /// A pack compiled into the executable.
    pub fn from_static(bytes: &'static [u8]) -> Result<Self> {
        Self::parse(Source::Static(bytes))
    }

    /// A pack file read on demand, e.g. `Contents/Resources/content.pack`.
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let len = file
            .metadata()
            .map_err(|e| format!("content pack: {e}"))?
            .len();
        Self::parse(Source::File {
            file: std::sync::Mutex::new(file),
            base: 0,
            len,
        })
    }

    /// The pack appended to `exe`, or `None` when the executable carries no trailer.
    pub fn open_attached(exe: &Path) -> Result<Option<Self>> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(exe).map_err(|e| format!("{}: {e}", exe.display()))?;
        let total = file
            .metadata()
            .map_err(|e| format!("{}: {e}", exe.display()))?
            .len();
        if total < TRAILER {
            return Ok(None);
        }
        let mut trailer = [0u8; TRAILER as usize];
        file.seek(SeekFrom::Start(total - TRAILER))
            .and_then(|_| file.read_exact(&mut trailer))
            .map_err(|e| format!("{}: {e}", exe.display()))?;
        if &trailer[..8] != TRAILER_MAGIC {
            return Ok(None);
        }
        let len = u64_at(&trailer, 8);
        if len > total - TRAILER {
            return Err("attached content pack is truncated".into());
        }
        Self::parse(Source::File {
            file: std::sync::Mutex::new(file),
            base: total - TRAILER - len,
            len,
        })
        .map(Some)
    }

    fn parse(source: Source) -> Result<Self> {
        let total = source.len()?;
        if total < HEADER as u64 {
            return Err("content pack is truncated".into());
        }
        let header = source.read(0, HEADER as u64)?;
        if &header[..8] != MAGIC || u32_at(&header, 8) != 1 {
            return Err("not a content pack".into());
        }
        let count = u32_at(&header, 12) as usize;
        let (manifest_at, manifest_len) = (u64_at(&header, 16), u64_at(&header, 24));
        if count > MAX_BLOBS || manifest_len == 0 || manifest_len > MAX_MANIFEST {
            return Err("unsupported content pack".into());
        }
        let table = source.read(HEADER as u64, (count * ENTRY) as u64)?;
        let mut index = Vec::with_capacity(count);
        for entry in table.as_chunks::<ENTRY>().0 {
            let digest: [u8; 32] = entry[..32].try_into().unwrap();
            let (offset, len) = (u64_at(entry, 32), u64_at(entry, 40));
            let in_bounds = offset.checked_add(len).is_some_and(|end| end <= total);
            if len == 0 || len > MAX_BLOB || !in_bounds {
                return Err("invalid content pack entry".into());
            }
            if index.last().is_some_and(|(last, _, _)| *last >= digest) {
                return Err("content pack index is not sorted".into());
            }
            index.push((digest, offset, len));
        }
        if manifest_at
            .checked_add(manifest_len)
            .is_none_or(|e| e > total)
        {
            return Err("content pack is truncated".into());
        }
        let manifest = source.read(manifest_at, manifest_len)?;
        if Sha256::digest(&manifest)[..] != header[32..64] {
            return Err("content pack manifest integrity check failed".into());
        }
        Ok(Self {
            source,
            manifest,
            index,
        })
    }

    pub fn blob_count(&self) -> usize {
        self.index.len()
    }

    /// `manifest.json`, or the blob named `<sha256>.<extension>`.
    pub(crate) fn read(&self, name: &str) -> Result<Vec<u8>> {
        if name == "manifest.json" {
            return Ok(self.manifest.clone());
        }
        let digest = name.split('.').next().unwrap_or_default();
        let key = unhex(digest).ok_or_else(|| format!("content pack has no {name}"))?;
        let at = self
            .index
            .binary_search_by(|(d, _, _)| d.cmp(&key))
            .map_err(|_| format!("content pack has no {name}"))?;
        let (_, offset, len) = self.index[at];
        self.source.read(offset, len)
    }
}

fn unhex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in text.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    debug_assert_eq!(hex(&out), text);
    Some(out)
}

/// The pack compiled in by the `embedded` feature (`FURY_PACK` at build time).
#[cfg(feature = "embedded")]
pub fn embedded() -> Option<Result<Pack>> {
    static PACK: &[u8] = include_bytes!(env!("FURY_PACK"));
    Some(Pack::from_static(PACK))
}

#[cfg(not(feature = "embedded"))]
pub fn embedded() -> Option<Result<Pack>> {
    None
}

/// Where a bundled pack lives relative to the executable: next to it, or in
/// the macOS bundle's `Contents/Resources` (executable in `Contents/MacOS`).
pub fn sidecar_candidates(exe: &Path) -> Vec<PathBuf> {
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    vec![dir.join(SIDECAR), dir.join("../Resources").join(SIDECAR)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Content, Store};

    /// Synthetic pack written by `tools/pack_content.py fixture --hex`; no game
    /// data. Stored as hex text because the repository tracks no binary files.
    fn fixture() -> &'static [u8] {
        static BYTES: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
            let text = include_str!("../tests/fixtures/mini.pack.hex");
            let digits: Vec<u8> = text.bytes().filter(u8::is_ascii_hexdigit).collect();
            digits
                .chunks(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect()
        });
        &BYTES
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(v) = future.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    #[test]
    fn reads_whole_files_through_the_manifest() {
        let pack = Pack::from_static(fixture()).unwrap();
        assert_eq!(pack.blob_count(), 3);
        let content = block_on(Content::open(Store::Pack(pack))).unwrap();
        assert_eq!(
            block_on(content.file("/MINI/ONE.RES")).unwrap(),
            b"first file"
        );
        let chunked = block_on(content.file("/MINI/TWO.VAG")).unwrap();
        assert_eq!(chunked, b"chunk-a|chunk-b");
        assert!(block_on(content.file("/MINI/NONE.RES")).is_err());
    }

    #[test]
    fn file_backed_pack_matches_static() {
        let dir = std::env::temp_dir().join(format!("fury-pack-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SIDECAR);
        std::fs::write(&path, fixture()).unwrap();
        let content = block_on(Content::open(Store::Pack(Pack::open(&path).unwrap()))).unwrap();
        assert_eq!(
            block_on(content.file("/MINI/ONE.RES")).unwrap(),
            b"first file"
        );
        drop(content); // Windows cannot remove a folder holding an open file
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_a_pack_appended_to_an_executable() {
        let dir = std::env::temp_dir().join(format!("fury-attach-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fury");
        std::fs::write(&path, b"not really an executable").unwrap();
        assert!(Pack::open_attached(&path).unwrap().is_none());
        let mut bytes = b"not really an executable".to_vec();
        bytes.extend_from_slice(fixture());
        bytes.extend_from_slice(TRAILER_MAGIC);
        bytes.extend_from_slice(&(fixture().len() as u64).to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let pack = Pack::open_attached(&path).unwrap().unwrap();
        let content = block_on(Content::open(Store::Pack(pack))).unwrap();
        assert_eq!(
            block_on(content.file("/MINI/ONE.RES")).unwrap(),
            b"first file"
        );
        drop(content);
        // A length longer than the file is damage, not "no pack".
        let mut bad = bytes.clone();
        let at = bad.len() - 8;
        bad[at..].copy_from_slice(&u64::MAX.to_le_bytes());
        std::fs::write(&path, &bad).unwrap();
        assert!(Pack::open_attached(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_tampering_and_damage() {
        let mut bad = fixture().to_vec();
        *bad.last_mut().unwrap() ^= 1;
        let pack = Pack::from_static(Box::leak(bad.into_boxed_slice())).unwrap();
        let content = block_on(Content::open(Store::Pack(pack))).unwrap();
        let tampered = ["/MINI/ONE.RES", "/MINI/TWO.VAG"]
            .iter()
            .filter(|path| block_on(content.file(path)).is_err())
            .count();
        assert_eq!(tampered, 1, "flipping a blob byte fails exactly that file");

        let mut manifest = fixture().to_vec();
        let at = u64_at(&manifest, 16) as usize + 20;
        manifest[at] ^= 1;
        assert!(Pack::from_static(Box::leak(manifest.into_boxed_slice())).is_err());

        assert!(Pack::from_static(&fixture()[..fixture().len() - 4]).is_err());
        assert!(Pack::from_static(&fixture()[..40]).is_err());
        assert!(
            Pack::from_static(
                b"not a content pack at all, not even close to one!!!!!!!!!!!!!!!!!!"
            )
            .is_err()
        );
    }

    /// Runs only with `--features embedded` and FURY_PACK set (CI uses the fixture).
    #[cfg(feature = "embedded")]
    #[test]
    fn embedded_pack_opens() {
        let pack = embedded().expect("feature builds embed a pack").unwrap();
        assert!(pack.blob_count() > 0);
        block_on(Content::open(Store::Pack(pack))).unwrap();
    }

    #[test]
    fn sidecar_search_covers_app_bundles() {
        let found = sidecar_candidates(Path::new("/A/Fury.app/Contents/MacOS/fury"));
        assert_eq!(
            found[0],
            Path::new("/A/Fury.app/Contents/MacOS/content.pack")
        );
        assert!(found[1].ends_with("MacOS/../Resources/content.pack"));
    }
}
