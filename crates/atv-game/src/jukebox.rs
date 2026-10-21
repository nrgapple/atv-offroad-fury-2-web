//! The original Jukebox page art (PAGE14.RES, jukebox.js `loadArt`): the red
//! snow backdrop `bk_jukebox.pst` and the spinning CD `cd64.atm`.
use crate::{Result, archive::Archive};
use atv_content::Content;
use atv_core::texture;

pub const JUKEBOX: &str = "/UI/RESOURCE/PAGEA/PAGE14.RES";

/// Width, height and RGBA of the backdrop and each CD frame.
pub struct JukeboxArt {
    pub backdrop: Option<(u32, u32, Vec<u8>)>,
    pub disc: Vec<(u32, u32, Vec<u8>)>,
}

/// None for content packages without PAGE14 (the panel keeps its drawn disc).
pub async fn load(content: &Content) -> Result<Option<JukeboxArt>> {
    if content.manifest.record(JUKEBOX).is_none() {
        return Ok(None);
    }
    let archive = Archive::open(content, JUKEBOX).await?;
    let member = async |name: &str| -> Result<Vec<u8>> {
        let index = archive
            .named(name)
            .ok_or_else(|| format!("Original Jukebox member missing: {name}"))?;
        archive.member(index).await
    };
    let picture = texture::decode_pst(&member("bk_jukebox.pst").await?)?;
    let atm = member("cd64.atm").await?;
    let (frames, _) = texture::decode_atm(&atm, 0)?;
    let mut disc = Vec::with_capacity(frames);
    for frame in 0..frames {
        let (_, t) = texture::decode_atm(&atm, frame)?;
        disc.push((t.info.width, t.info.height, t.rgba));
    }
    Ok(Some(JukeboxArt {
        backdrop: Some((picture.info.width, picture.info.height, picture.rgba)),
        disc,
    }))
}
