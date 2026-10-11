//! Bounded ISO9660 directory records. File contents are read on demand by the host.
use crate::{Result, slice, u32le};
use serde::Serialize;

pub const SECTOR: u64 = 2048;

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    pub lba: u32,
    pub bytes: u32,
    pub directory: bool,
}

fn record(data: &[u8]) -> Result<Entry> {
    if data.len() < 34 || usize::from(data[0]) != data.len() {
        return Err("invalid directory record".into());
    }
    if data[1] != 0 || data[25] & 0x80 != 0 || data[26] != 0 || data[27] != 0 {
        return Err("extended/interleaved/multi-extent ISO records are unsupported".into());
    }
    let lba = u32le(data, 2)?;
    let bytes = u32le(data, 10)?;
    if lba != u32::from_be_bytes(slice(data, 6, 4)?.try_into().unwrap())
        || bytes != u32::from_be_bytes(slice(data, 14, 4)?.try_into().unwrap())
    {
        return Err("ISO endian copies disagree".into());
    }
    let raw = slice(data, 33, data[32] as usize)?;
    let name = if raw == [0] {
        ".".into()
    } else if raw == [1] {
        "..".into()
    } else {
        let s = std::str::from_utf8(raw)
            .map_err(|_| "invalid ISO name")?
            .split(';')
            .next()
            .unwrap();
        if s.is_empty() || s.contains(['/', '\\', ':', '\0']) || s == "." || s == ".." {
            return Err("unsafe ISO name".into());
        }
        s.into()
    };
    Ok(Entry {
        name,
        lba,
        bytes,
        directory: data[25] & 2 != 0,
    })
}

pub fn root(pvd: &[u8]) -> Result<Entry> {
    if pvd.len() != 2048 || &pvd[..7] != b"\x01CD001\x01" {
        return Err("expected ISO9660 primary volume descriptor".into());
    }
    if &pvd[128..132] != b"\x00\x08\x08\x00" {
        return Err("expected 2048-byte ISO sectors".into());
    }
    let root = record(slice(pvd, 156, pvd[156] as usize)?)?;
    if !root.directory {
        return Err("ISO root is not a directory".into());
    }
    Ok(root)
}

pub fn directory(data: &[u8]) -> Result<Vec<Entry>> {
    if data.len() > 16 * 1024 * 1024 {
        return Err("directory exceeds 16 MiB limit".into());
    }
    let mut result = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let size = data[pos] as usize;
        if size == 0 {
            pos = (pos / 2048 + 1) * 2048;
            continue;
        }
        if pos % 2048 + size > 2048 {
            return Err("directory record crosses sector".into());
        }
        let entry = record(slice(data, pos, size)?)?;
        if entry.name != "." && entry.name != ".." {
            result.push(entry);
        }
        pos += size;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_malformed_records() {
        assert!(directory(&[1]).is_err());
        let mut b = vec![0; 34];
        b[0] = 34;
        b[32] = 20;
        assert!(directory(&b).is_err());
        assert!(root(&b).is_err());
    }
}
