//! Version-3 resource containers read through their directory, so a caller
//! downloads only the members it uses (openArchive in web/content.js). A
//! compressed member inflates just the 24 KiB blocks it spans.
use crate::Result;
use atv_content::Content;
use atv_core::resource::BLOCK_SIZE;

pub struct Member {
    pub name: String,
    offset: usize,
    bytes: usize,
}

pub struct Archive<'c> {
    content: &'c Content,
    path: String,
    pub members: Vec<Member>,
    payload: usize,
    /// File offsets of each compressed block and one past the last; empty
    /// for an uncompressed container.
    blocks: Vec<usize>,
}

fn u32le(bytes: &[u8], at: usize) -> Result<usize> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        .ok_or_else(|| "truncated resource header".into())
}

impl<'c> Archive<'c> {
    pub async fn open(content: &'c Content, path: &str) -> Result<Self> {
        let total = content.size(path)?;
        let mut head = content.read(path, 0, total.min(65536)).await?;
        let compressed = match (u32le(&head, 0)?, u32le(&head, 12)?, u32le(&head, 8)?) {
            (3, 0, 0x340) => true,
            (3, 0, 0x100) => false,
            _ => return Err(format!("unsupported resource {path}")),
        };
        let directory = u32le(&head, 4)? + 8;
        if directory < 20 || directory + 8 > total {
            return Err(format!("invalid resource directory in {path}"));
        }
        if directory + 8 > head.len() {
            head = content.read(path, 0, directory + 8).await?;
        }
        let count = u32le(&head, directory)?;
        let payload = directory + 8 + u32le(&head, directory + 4)?;
        if payload > total {
            return Err(format!("invalid resource directory in {path}"));
        }
        if payload > head.len() {
            head = content.read(path, 0, payload).await?;
        }
        let mut members = Vec::with_capacity(count);
        let mut at = directory + 8;
        for _ in 0..count {
            let length = u32le(&head, at)?;
            let name = head
                .get(at + 4..at + 4 + length)
                .ok_or("truncated resource directory")?;
            let name = String::from_utf8_lossy(name).replace('\\', "/");
            members.push(Member {
                name,
                offset: u32le(&head, at + 4 + length)?,
                bytes: u32le(&head, at + 8 + length)?,
            });
            at += 12 + length;
        }
        if at != payload {
            return Err(format!("resource directory has unparsed bytes in {path}"));
        }
        let mut blocks = Vec::new();
        if compressed {
            if u32le(&head, 16)? != directory - 20 || head[directory - 2..directory] != [0, 0] {
                return Err(format!("invalid compressed block table in {path}"));
            }
            blocks.push(payload);
            for at in (20..directory - 2).step_by(2) {
                let length = u16::from_le_bytes([head[at], head[at + 1]]) as usize + 1;
                blocks.push(blocks.last().unwrap() + length);
            }
            if *blocks.last().unwrap() != total {
                return Err(format!("compressed payload length mismatch in {path}"));
            }
        }
        let logical = if compressed {
            (blocks.len() - 1) * BLOCK_SIZE
        } else {
            total - payload
        };
        if members.iter().any(|m| m.offset + m.bytes > logical) {
            return Err(format!("member outside resource {path}"));
        }
        Ok(Self {
            content,
            path: path.to_string(),
            members,
            payload,
            blocks,
        })
    }

    /// Index of the member whose file name (last path component) matches.
    pub fn named(&self, name: &str) -> Option<usize> {
        self.members.iter().position(|m| {
            m.name
                .rsplit('/')
                .next()
                .is_some_and(|last| last.eq_ignore_ascii_case(name))
        })
    }

    pub async fn member(&self, index: usize) -> Result<Vec<u8>> {
        let member = self.members.get(index).ok_or("member index out of range")?;
        if self.blocks.is_empty() {
            return self
                .content
                .read(&self.path, self.payload + member.offset, member.bytes)
                .await;
        }
        if member.bytes == 0 {
            return Ok(Vec::new());
        }
        let (first, last) = (
            member.offset / BLOCK_SIZE,
            (member.offset + member.bytes - 1) / BLOCK_SIZE,
        );
        let start = self.blocks[first];
        let bytes = self
            .content
            .read(&self.path, start, self.blocks[last + 1] - start)
            .await?;
        let mut out = Vec::with_capacity(member.bytes);
        for block in first..=last {
            let data = &bytes[self.blocks[block] - start..self.blocks[block + 1] - start];
            let plain = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, BLOCK_SIZE)
                .map_err(|e| format!("invalid zlib block {block}: {e}"))?;
            if plain.len() != BLOCK_SIZE {
                return Err(format!("block {block} is not 24576 bytes"));
            }
            let low = member.offset.saturating_sub(block * BLOCK_SIZE);
            let high = (member.offset + member.bytes - block * BLOCK_SIZE).min(BLOCK_SIZE);
            out.extend_from_slice(&plain[low..high]);
        }
        Ok(out)
    }
}
