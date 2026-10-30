//! The original frontend music (GameAudio.loadMenuMusic in web/audio.js).
//!
//! Recovered: the routine at 00255b50 builds the frontend's UI sound list at object
//! +0x160 from `audio\UI\uimusic.aud`: section `UIMusic`, `NumTracks`, and
//! `Track%d` names, each added as a looping sample (flags 0xc1) from
//! `audio\audio`; the routine at 00254ae0 frees it when the frontend closes. The four
//! tunes are `uitune1`-`uitune4`, stored as left/right mono VAGs in
//! AUDIO.RES. Which tune the frontend starts, and how it changes them, is
//! not recovered; the browser game loops the first with its VAG loop
//! markers, and so does this.
use super::mixer::Pcm;
use crate::{Result, archive::Archive};
use atv_content::Content;

/// The UI tune names from UIMUSIC.AUD (`Track1`..`TrackN`).
pub fn tracks(definition: &[u8]) -> Result<Vec<String>> {
    let doc = atv_core::ui::Document::parse(definition)?;
    let section = doc
        .sections
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("uimusic"))
        .ok_or("UIMusic section missing")?;
    let value = |key: &str| {
        section
            .fields
            .iter()
            .find(|f| f.key.eq_ignore_ascii_case(key))
            .map(|f| f.value.replace('\\', "/"))
    };
    let count: usize = value("numtracks")
        .and_then(|v| v.trim().parse().ok())
        .ok_or("UIMusic NumTracks missing")?;
    (1..=count)
        .map(|i| value(&format!("track{i}")).ok_or_else(|| format!("UIMusic Track{i} missing")))
        .collect()
}

/// The first UI tune as stereo PCM at `rate`, looping on its VAG markers.
pub async fn load_tune(content: &Content, rate: u32) -> Result<Pcm> {
    let page = Archive::open(content, "/UI/RESOURCE/PAGEA/PAGE07.RES").await?;
    let index = page
        .members
        .iter()
        .position(|m| {
            m.name
                .replace('\\', "/")
                .to_ascii_lowercase()
                .rsplit('/')
                .next()
                == Some("uimusic.aud")
        })
        .ok_or("original menu music definition missing")?;
    let names = tracks(&page.member(index).await?)?;
    if names.len() != 4 {
        return Err("original UI expected four tunes".into());
    }
    let sounds = Archive::open(content, "/AUDIO/AUDIO.RES").await?;
    let mut vags = Vec::with_capacity(2);
    for channel in ["l", "r"] {
        let suffix = format!("{}{channel}.vag", names[0]).to_ascii_lowercase();
        let index = sounds
            .members
            .iter()
            .position(|m| {
                let name = m.name.replace('\\', "/").to_ascii_lowercase();
                name == suffix || name.ends_with(&format!("/{suffix}"))
            })
            .ok_or_else(|| format!("original UI tune missing: {suffix}"))?;
        vags.push(sounds.member(index).await?);
    }
    // Both channels decode on the decode workers (web): the tune starts on
    // the first tap, when the menus are animating.
    let inputs: Vec<&[u8]> = vags.iter().map(Vec::as_slice).collect();
    let channels: Vec<Pcm> = crate::offload::pcms(&inputs, rate)
        .await?
        .into_iter()
        .map(|(_, pcm)| pcm)
        .collect();
    let (left, right) = (&channels[0], &channels[1]);
    if left.channels != 1 || right.channels != 1 || left.frames != right.frames {
        return Err("menu stereo channels differ".into());
    }
    let mut data = Vec::with_capacity(left.frames * 2);
    for (l, r) in left.data.iter().zip(&right.data) {
        data.push(*l);
        data.push(*r);
    }
    Ok(Pcm {
        channels: 2,
        frames: left.frames,
        data,
        loop_markers: left.loop_markers,
        rate,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_ui_music_definition() {
        let text =
            b"[UIMusic]\r\nNumTracks = 2\r\nTrack1 = UI\\uitune1\r\nTrack2 = UI\\uitune2\r\n";
        assert_eq!(super::tracks(text).unwrap(), ["UI/uitune1", "UI/uitune2"]);
    }
}
