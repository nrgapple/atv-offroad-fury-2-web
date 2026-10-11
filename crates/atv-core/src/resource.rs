//! Version-3 Quadzilla containers, independently recovered from USA v2.00.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;

pub const BLOCK_SIZE: usize = 0x6000;
const MAX_MEMBER: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Member {
    pub name: String,
    pub offset: usize,
    pub bytes: usize,
}

pub struct Resource<'a> {
    data: &'a [u8],
    pub members: Vec<Member>,
    pub compressed: bool,
    payload: usize,
    blocks: Vec<(usize, usize)>,
}

impl<'a> Resource<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if u32le(data, 0)? != 3 || u32le(data, 12)? != 0 {
            return Err("unsupported resource header/version".into());
        }
        let compressed = match u32le(data, 8)? {
            0x340 => true,
            0x100 => false,
            flags => return Err(format!("unsupported resource flags {flags:#x}")),
        };
        let directory = (u32le(data, 4)? as usize)
            .checked_add(8)
            .ok_or("directory overflow")?;
        if directory < 20 {
            return Err("directory overlaps header".into());
        }
        let count = u32le(data, directory)? as usize;
        let dir_size = u32le(data, directory + 4)? as usize;
        let dir = slice(data, directory + 8, dir_size)?;
        if count > dir_size / 13 {
            return Err("impossible member count".into());
        }
        let payload = directory + 8 + dir_size;
        let mut pos = 0;
        let mut members = Vec::with_capacity(count);
        for _ in 0..count {
            let length = u32le(dir, pos)? as usize;
            pos += 4;
            let name = slice(dir, pos, length)?;
            if name.is_empty() || name.contains(&0) {
                return Err("invalid member name".into());
            }
            let name = std::str::from_utf8(name)
                .map_err(|_| "non-UTF8 member name")?
                .replace('\\', "/");
            // Names can be displayed or used as paths by tools. Reject traversal here.
            if name.is_empty()
                || name.starts_with('/')
                || name.contains(':')
                || name
                    .split('/')
                    .any(|p| p.is_empty() || p == ".." || p == ".")
            {
                return Err("unsafe resource member path".into());
            }
            pos += length;
            members.push(Member {
                name,
                offset: u32le(dir, pos)? as usize,
                bytes: u32le(dir, pos + 4)? as usize,
            });
            pos += 8;
        }
        if pos != dir.len() {
            return Err("resource directory has unparsed bytes".into());
        }
        let mut blocks = Vec::new();
        let logical_size = if compressed {
            let table_size = u32le(data, 16)? as usize;
            if table_size < 2 || !table_size.is_multiple_of(2) || table_size != directory - 20 {
                return Err("invalid compressed block table size".into());
            }
            if u16le(data, directory - 2)? != 0 {
                return Err("missing block table sentinel".into());
            }
            let mut at = payload;
            for i in (20..directory - 2).step_by(2) {
                let bytes = usize::from(u16le(data, i)?) + 1;
                slice(data, at, bytes)?;
                blocks.push((at, bytes));
                at += bytes;
            }
            if at != data.len() {
                return Err("compressed payload length mismatch".into());
            }
            blocks
                .len()
                .checked_mul(BLOCK_SIZE)
                .ok_or("logical size overflow")?
        } else {
            data.len() - payload
        };
        for member in &members {
            if member
                .offset
                .checked_add(member.bytes)
                .ok_or("member range overflow")?
                > logical_size
            {
                return Err(format!("member outside resource: {}", member.name));
            }
        }
        Ok(Self {
            data,
            members,
            compressed,
            payload,
            blocks,
        })
    }

    /// Inflate only blocks intersecting this member, with a strict per-block limit.
    pub fn extract(&self, index: usize) -> Result<Vec<u8>> {
        let member = self.members.get(index).ok_or("member index out of range")?;
        if member.bytes > MAX_MEMBER {
            return Err("member exceeds 128 MiB limit".into());
        }
        if !self.compressed {
            return Ok(slice(self.data, self.payload + member.offset, member.bytes)?.to_vec());
        }
        let end = member.offset + member.bytes;
        let mut result = Vec::with_capacity(member.bytes);
        if member.bytes == 0 {
            return Ok(result);
        }
        for block in member.offset / BLOCK_SIZE..=(end - 1) / BLOCK_SIZE {
            let bytes = self.decode_block(block)?;
            let start_in_block = member.offset.saturating_sub(block * BLOCK_SIZE);
            let end_in_block = (end - block * BLOCK_SIZE).min(BLOCK_SIZE);
            result.extend_from_slice(&bytes[start_in_block..end_in_block]);
        }
        Ok(result)
    }

    fn decode_block(&self, index: usize) -> Result<Vec<u8>> {
        let &(at, len) = self.blocks.get(index).ok_or("block index out of range")?;
        let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
            slice(self.data, at, len)?,
            BLOCK_SIZE,
        )
        .map_err(|e| format!("invalid zlib block {index}: {e}"))?;
        if out.len() != BLOCK_SIZE {
            return Err(format!("block {index} is not 24576 bytes"));
        }
        Ok(out)
    }

    pub fn validate_blocks(&self) -> Result<usize> {
        for i in 0..self.blocks.len() {
            self.decode_block(i)?;
        }
        Ok(self.blocks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(compressed: bool, name: &str) -> Vec<u8> {
        let mut plain = vec![0; BLOCK_SIZE * 2];
        plain[BLOCK_SIZE - 2..BLOCK_SIZE + 3].copy_from_slice(b"hello");
        let chunks: Vec<Vec<u8>> = plain
            .chunks(BLOCK_SIZE)
            .map(|c| miniz_oxide::deflate::compress_to_vec_zlib(c, 6))
            .collect();
        let mut out = Vec::new();
        let table_size = if compressed { 6 } else { 0 };
        for word in [
            3u32,
            12 + table_size,
            if compressed { 0x340 } else { 0x100 },
            0,
            table_size,
        ] {
            out.extend(word.to_le_bytes());
        }
        if compressed {
            for c in &chunks {
                out.extend(((c.len() - 1) as u16).to_le_bytes());
            }
            out.extend(0u16.to_le_bytes());
        }
        out.extend(1u32.to_le_bytes());
        out.extend((name.len() as u32 + 12).to_le_bytes());
        out.extend((name.len() as u32).to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(((BLOCK_SIZE - 2) as u32).to_le_bytes());
        out.extend(5u32.to_le_bytes());
        if compressed {
            for c in chunks {
                out.extend(c);
            }
        } else {
            out.extend(plain);
        }
        out
    }
    #[test]
    fn reads_member_across_blocks_and_raw_equivalently() {
        for compressed in [true, false] {
            let bytes = fixture(compressed, "folder\\test.txt");
            let r = Resource::parse(&bytes).unwrap();
            assert_eq!(r.members[0].name, "folder/test.txt");
            assert_eq!(r.extract(0).unwrap(), b"hello");
        }
    }
    #[test]
    fn rejects_truncation_corruption_and_traversal() {
        let good = fixture(true, "test");
        for end in 0..good.len() {
            assert!(Resource::parse(&good[..end]).is_err());
        }
        let mut bad = good.clone();
        let end = bad.len();
        bad[end - 3] ^= 0xff;
        assert!(Resource::parse(&bad).unwrap().validate_blocks().is_err());
        assert!(Resource::parse(&fixture(true, "../escape")).is_err());
    }
}
