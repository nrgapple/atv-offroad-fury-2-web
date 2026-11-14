//! The original Credits page (Options → Credits, page 18; research/
//! MENU_FLOW.md "Credits"): `PAGE18.RES` carries `ui/frontend/credits.txt`,
//! a scroll script read by the page. The text is game content: it comes
//! from the content package at run time, never from this repository.
//!
//! Format (the file's own delimiter notes): option `Key=Value` lines up to
//! `--credits start--`, then one entry per line: `%name` an icon texture,
//! `@text` a heading, `;` a comment, an empty or blank line a gap of
//! `BlankLinePixels`, anything else a name. The options also give the
//! scroll speed (pixels per second of the 640 × 448 page) and the heading
//! and name colours.

/// One line of the scroll.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// An icon texture (`rainbowlogo`, `sonylogo`).
    Icon(String),
    Heading(String),
    Name(String),
    Blank,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Credits {
    /// `ScrollSpeed`: page pixels per second.
    pub scroll_speed: f32,
    /// `BlankLinePixels`: a gap's height in page pixels.
    pub blank_pixels: f32,
    pub heading_color: [u8; 3],
    pub name_color: [u8; 3],
    /// Script rectangle [left, top, right, bottom] on the 640 × 448 page.
    pub bounds: [f32; 4],
    /// Names are indented from headings, in page pixels.
    pub name_indent: f32,
    pub entries: Vec<Entry>,
}

impl Default for Credits {
    fn default() -> Self {
        Self {
            scroll_speed: 40.,
            blank_pixels: 30.,
            heading_color: [218, 192, 31],
            name_color: [200, 200, 200],
            bounds: [359., 0., 640., 448.],
            name_indent: 10.,
            entries: Vec::new(),
        }
    }
}

/// The file is Windows-1252 (its trademark sign is 0x99); the other
/// non-ASCII bytes it uses are Latin-1 letters and signs.
pub fn decode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x99 => '\u{2122}',
            0x80..=0x9f => '?',
            _ => char::from(b),
        })
        .collect()
}

fn color(value: &str) -> Option<[u8; 3]> {
    let mut parts = value.split(',').map(|p| p.trim().parse::<u8>().ok());
    let rgb = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(rgb)
}

/// Reads `credits.txt`. Trailing blank lines are dropped; at most 4096
/// entries are kept.
pub fn parse(text: &str) -> Credits {
    let mut credits = Credits::default();
    let mut started = false;
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if !started {
            if line.trim().eq_ignore_ascii_case("--credits start--") {
                started = true;
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match key.to_ascii_lowercase().as_str() {
                "leftx" | "topy" | "rightx" | "bottomy" => {
                    let index = match key.to_ascii_lowercase().as_str() {
                        "leftx" => 0,
                        "topy" => 1,
                        "rightx" => 2,
                        _ => 3,
                    };
                    let limit = if matches!(index, 0 | 2) { 640. } else { 448. };
                    if let Ok(v) = value.parse::<f32>()
                        && v.is_finite()
                        && (0. ..=limit).contains(&v)
                    {
                        credits.bounds[index] = v;
                    }
                }
                "nameoffsetfromheadingx" => {
                    if let Ok(v) = value.parse::<f32>()
                        && v.is_finite()
                        && (0. ..=100.).contains(&v)
                    {
                        credits.name_indent = v;
                    }
                }
                "scrollspeed" => {
                    if let Ok(v) = value.parse::<f32>()
                        && v.is_finite()
                        && v > 0.
                    {
                        credits.scroll_speed = v;
                    }
                }
                "blanklinepixels" => {
                    if let Ok(v) = value.parse::<f32>()
                        && v.is_finite()
                        && v >= 0.
                    {
                        credits.blank_pixels = v;
                    }
                }
                "headingcolor" => {
                    credits.heading_color = color(value).unwrap_or(credits.heading_color)
                }
                "namecolor" => credits.name_color = color(value).unwrap_or(credits.name_color),
                _ => {}
            }
            continue;
        }
        if credits.entries.len() >= 4096 {
            break;
        }
        let entry = if line.trim().is_empty() {
            Entry::Blank
        } else if line.starts_with(';') {
            continue;
        } else if let Some(icon) = line.strip_prefix('%') {
            Entry::Icon(icon.trim().to_string())
        } else if let Some(heading) = line.strip_prefix('@') {
            Entry::Heading(heading.trim().to_string())
        } else {
            Entry::Name(line.trim().to_string())
        };
        credits.entries.push(entry);
    }
    while credits.entries.last() == Some(&Entry::Blank) {
        credits.entries.pop();
    }
    if credits.bounds[2] <= credits.bounds[0] || credits.bounds[3] <= credits.bounds[1] {
        credits.bounds = Credits::default().bounds;
    }
    credits
}

/// The port's own section, shown after the original credits and kept
/// apart from them.
pub fn port_section() -> Vec<Entry> {
    let heading = |s: &str| Entry::Heading(s.into());
    let name = |s: &str| Entry::Name(s.into());
    vec![
        Entry::Blank,
        Entry::Blank,
        heading("Fury Remastered"),
        name("An unofficial fan port of ATV Offroad Fury 2"),
        name("(USA v2.00) to Rust and WebAssembly."),
        name("Not affiliated with or endorsed by"),
        name("Sony Interactive Entertainment"),
        name("or Rainbow Studios."),
        Entry::Blank,
        heading("Port"),
        name("furyremastered.com"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_options_headings_names_icons_and_gaps() {
        // An authored script in the original's format.
        let text = "TopY=0.0\r\nScrollSpeed=25.5\nHeadingColor=1,2,3\nNameColor=bad\n\
                    BlankLinePixels=12\n;comment\n--credits start--\n;\n%logo\n@Lead\n\
                    Ann Example \n \n@Team\n@Second Heading\nBo\n\n\n";
        let c = parse(text);
        assert_eq!(c.scroll_speed, 25.5);
        assert_eq!(c.blank_pixels, 12.);
        assert_eq!(c.heading_color, [1, 2, 3]);
        assert_eq!(c.name_color, [200, 200, 200]);
        assert_eq!(
            c.entries,
            [
                Entry::Icon("logo".into()),
                Entry::Heading("Lead".into()),
                Entry::Name("Ann Example".into()),
                Entry::Blank,
                Entry::Heading("Team".into()),
                Entry::Heading("Second Heading".into()),
                Entry::Name("Bo".into()),
            ]
        );
        // Nothing before the start marker is a credit.
        assert!(parse("ScrollSpeed=3\nName").entries.is_empty());
    }

    #[test]
    fn decodes_the_trademark_sign() {
        assert_eq!(decode(b"Fury\x99 \xa9 \xe9"), "Fury\u{2122} \u{a9} \u{e9}");
    }

    #[test]
    fn the_port_section_says_it_is_unofficial() {
        let section = port_section();
        assert_eq!(section[2], Entry::Heading("Fury Remastered".into()));
        assert!(
            section
                .iter()
                .any(|e| matches!(e, Entry::Name(n) if n.contains("unofficial fan port")))
        );
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;
    #[test]
    fn reads_disc_rectangle_and_name_indent() {
        let c = parse(
            "LeftX=350\nTopY=2\nRightX=630\nBottomY=440\nNameOffsetFromHeadingX=12\n--credits start--\n@Team\nName",
        );
        assert_eq!(c.bounds, [350., 2., 630., 440.]);
        assert_eq!(c.name_indent, 12.);
        let c = parse("LeftX=640\nRightX=20\nNameOffsetFromHeadingX=NaN\n--credits start--");
        assert_eq!(c.bounds, Credits::default().bounds);
        assert_eq!(c.name_indent, 10.);
    }
}
