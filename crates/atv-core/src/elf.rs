//! Conservative ELF reconnaissance, NOT decompilation or verified function recovery.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct Segment {
    pub address: u32,
    pub offset: u32,
    pub file_bytes: u32,
    pub memory_bytes: u32,
    pub flags: u32,
}
#[derive(Debug, Serialize)]
pub struct CallCandidate {
    pub target: u32,
    pub sites: Vec<u32>,
}
#[derive(Debug, Serialize)]
pub struct ElfReport {
    pub entry: u32,
    pub segments: Vec<Segment>,
    pub direct_call_candidates: Vec<CallCandidate>,
    pub caveat: &'static str,
}

pub fn inspect(data: &[u8]) -> Result<ElfReport> {
    if slice(data, 0, 7)? != b"\x7fELF\x01\x01\x01"
        || u16le(data, 16)? != 2
        || u16le(data, 18)? != 8
    {
        return Err("expected executable ELF32 little-endian MIPS".into());
    }
    let entry = u32le(data, 24)?;
    let phoff = u32le(data, 28)? as usize;
    let phsize = usize::from(u16le(data, 42)?);
    let phnum = usize::from(u16le(data, 44)?);
    if phsize != 32 || phnum > 128 {
        return Err("unsupported program header layout".into());
    }
    slice(data, phoff, phsize * phnum)?;
    let mut segments = Vec::new();
    for i in 0..phnum {
        let p = phoff + i * phsize;
        if u32le(data, p)? != 1 {
            continue;
        }
        let s = Segment {
            offset: u32le(data, p + 4)?,
            address: u32le(data, p + 8)?,
            file_bytes: u32le(data, p + 16)?,
            memory_bytes: u32le(data, p + 20)?,
            flags: u32le(data, p + 24)?,
        };
        if s.file_bytes > s.memory_bytes || s.address.checked_add(s.memory_bytes).is_none() {
            return Err("invalid load segment".into());
        }
        slice(data, s.offset as usize, s.file_bytes as usize)?;
        segments.push(s);
    }
    let executable = |addr: u32| {
        segments
            .iter()
            .any(|s| s.flags & 1 != 0 && addr >= s.address && addr < s.address + s.file_bytes)
    };
    if !executable(entry) {
        return Err("entry is outside executable load segments".into());
    }
    let mut calls = BTreeMap::<u32, Vec<u32>>::new();
    for s in &segments {
        if s.flags & 1 == 0 {
            continue;
        }
        for offset in (0..s.file_bytes.saturating_sub(3)).step_by(4) {
            let pc = s.address + offset;
            let word = u32le(data, (s.offset + offset) as usize)?;
            if word >> 26 == 3 {
                let target = (pc.wrapping_add(4) & 0xf0000000) | ((word & 0x03ffffff) << 2);
                if executable(target) {
                    calls.entry(target).or_default().push(pc);
                }
            }
        }
    }
    Ok(ElfReport {
        entry,
        segments,
        direct_call_candidates: calls
            .into_iter()
            .map(|(target, sites)| CallCandidate { target, sites })
            .collect(),
        caveat: "Aligned JAL-pattern candidates only. Executable segments mix code and data; false positives are expected. No verified function boundaries, names, indirect calls, or behavior are inferred.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_jal_target() {
        let mut b = vec![0u8; 96];
        b[..7].copy_from_slice(b"\x7fELF\x01\x01\x01");
        for (at, v) in [(16, 2u16), (18, 8), (42, 32), (44, 1)] {
            b[at..at + 2].copy_from_slice(&v.to_le_bytes());
        }
        for (at, v) in [
            (24, 0x100000u32),
            (28, 52),
            (52, 1),
            (56, 84),
            (60, 0x100000),
            (68, 12),
            (72, 16),
            (76, 5),
            (84, 0x0c040002),
        ] {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        let r = inspect(&b).unwrap();
        assert_eq!(r.direct_call_candidates[0].target, 0x100008);
        assert!(inspect(&b[..95]).is_err());
    }
}
