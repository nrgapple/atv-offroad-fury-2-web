//! The original Credits page text (PAGE18.RES `ui/frontend/credits.txt`,
//! research/MENU_FLOW.md "Credits"), followed by the port's own section.
use crate::{Result, archive::Archive};
use atv_content::Content;
use atv_core::credits::{self, Credits};

pub const CREDITS: &str = "/UI/RESOURCE/PAGEA/PAGE18.RES";

/// The original credits then the port's section; a package without PAGE18
/// gives the port's section alone (`original` false).
pub async fn load(content: &Content) -> Result<(Credits, bool)> {
    let mut out = Credits::default();
    let mut original = false;
    if content.manifest.record(CREDITS).is_some() {
        let archive = Archive::open(content, CREDITS).await?;
        if let Some(index) = archive.named("credits.txt") {
            out = credits::parse(&credits::decode(&archive.member(index).await?));
            original = !out.entries.is_empty();
        }
    }
    out.entries.extend(credits::port_section());
    Ok((out, original))
}
