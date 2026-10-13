//! MX vs. ATV Unleashed PSR v2 directory. Payload sectors are relative to the
//! PSR, not the ISO. The trailing lookup tables remain opaque; no executable
//! or payload is loaded by this reader.
use crate::{Result, slice, u32le};
use serde::Serialize;
use std::collections::BTreeSet;

const HEADER: usize = 24;
const MAX_DIRECTORY: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, Serialize)]
pub struct Entry {
    pub name: String,
    pub bytes: u32,
    pub sector: u32,
}
impl Entry {
    pub fn offset(&self) -> u64 {
        u64::from(self.sector) * 2048
    }
}

#[derive(Debug, Serialize)]
pub struct Directory {
    pub entries: Vec<Entry>,
    pub directory_bytes: usize,
    /// Retained without assigning unverified semantics.
    pub header_words: [u32; 6],
}
impl Directory {
    /// Accepts a header/directory prefix and the full PSR extent, so native
    /// tools need not allocate the multi-gigabyte disc or container.
    pub fn parse(prefix: &[u8], total_bytes: u64) -> Result<Self> {
        let mut header_words = [0; 6];
        for (i, value) in header_words.iter_mut().enumerate() {
            *value = u32le(prefix, i * 4)?;
        }
        let count = header_words[1] as usize;
        let size = header_words[4] as usize;
        if header_words[0] != 2
            || count == 0
            || count > MAX_ENTRIES
            || size > MAX_DIRECTORY
            || count > size / 13
        {
            return Err("unsupported PSR directory header".into());
        }
        let end = HEADER.checked_add(size).ok_or("PSR directory overflow")?;
        let directory = slice(prefix, HEADER, size)?;
        if end as u64 > total_bytes {
            return Err("PSR directory exceeds container".into());
        }
        let mut at = 0;
        let mut entries = Vec::with_capacity(count);
        let mut names = BTreeSet::new();
        for _ in 0..count {
            let len = u32le(directory, at)? as usize;
            at += 4;
            if len == 0 || len > 1024 {
                return Err("invalid PSR name length".into());
            }
            let raw = slice(directory, at, len)?;
            let name = std::str::from_utf8(raw)
                .map_err(|_| "non-UTF8 PSR name")?
                .replace('\\', "/");
            if name.contains(['\0', ':'])
                || name.starts_with('/')
                || name
                    .split('/')
                    .any(|s| s.is_empty() || s == "." || s == "..")
                || !names.insert(name.to_ascii_lowercase())
            {
                return Err("unsafe or duplicate PSR member name".into());
            }
            at += len;
            let entry = Entry {
                name,
                bytes: u32le(directory, at)?,
                sector: u32le(directory, at + 4)?,
            };
            at += 8;
            let offset = entry.offset();
            if offset < end as u64 || offset + u64::from(entry.bytes) > total_bytes {
                return Err(format!("PSR member outside payload: {}", entry.name));
            }
            entries.push(entry);
        }
        if at != directory.len() {
            return Err("PSR directory has unparsed bytes".into());
        }
        Ok(Self {
            entries,
            directory_bytes: end,
            header_words,
        })
    }
    pub fn named(&self, name: &str) -> Result<&Entry> {
        let name = name.replace('\\', "/");
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&name))
            .ok_or_else(|| format!("missing PSR member: {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str, sector: u32, bytes: u32) -> Vec<u8> {
        let mut data = Vec::new();
        for word in [2, 1, 0, 0, 12 + name.len() as u32, 0] {
            data.extend(word.to_le_bytes());
        }
        data.extend((name.len() as u32).to_le_bytes());
        data.extend(name.as_bytes());
        data.extend(bytes.to_le_bytes());
        data.extend(sector.to_le_bytes());
        data
    }
    #[test]
    fn payload_offsets_are_relative_and_lookup_is_unambiguous() {
        let d = Directory::parse(&fixture("Veh\\Bike.psg", 8, 40), 20000).unwrap();
        let e = d.named("veh/bike.psg").unwrap();
        assert_eq!(e.offset(), 16384);
        assert_eq!(e.bytes, 40);
        assert!(d.named("bike.psg").is_err());
    }
    #[test]
    fn rejects_every_truncation_and_unsafe_or_outside_extents() {
        let data = fixture("bike", 8, 40);
        for n in 0..data.len() {
            assert!(Directory::parse(&data[..n], 20000).is_err());
        }
        for name in ["../bike", "/bike", "a//b", "a:b", "a\0b"] {
            assert!(Directory::parse(&fixture(name, 8, 40), 20000).is_err());
        }
        assert!(Directory::parse(&fixture("bike", 0, 40), 20000).is_err());
        assert!(Directory::parse(&fixture("bike", 8, 4000), 20000).is_err());
        let mut bad = data.clone();
        bad[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Directory::parse(&bad, 20000).is_err());
    }
}
