//! Original frontend layout data and CEL font tables. Decoding these resources
//! does not implement page callbacks, widget animation or race/menu behavior.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Field {
    pub key: String,
    pub value: String,
}
#[derive(Debug, Serialize)]
pub struct Section {
    pub name: String,
    pub fields: Vec<Field>,
}
#[derive(Debug, Serialize)]
pub struct Document {
    pub sections: Vec<Section>,
    /// Original authoring typos are retained, not silently repaired.
    pub unbalanced_quote_lines: Vec<usize>,
}
impl Document {
    /// UI layouts contain empty values and an optional final NUL, unlike PHY.
    /// Preserve field order and empty values rather than inventing defaults.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        if bytes.len() > 2 * 1024 * 1024
            || bytes.iter().any(|b| *b < 32 && !matches!(b, 9 | 10 | 13))
        {
            return Err("invalid UI text bytes".into());
        }
        let text: String = bytes.iter().map(|&byte| char::from(byte)).collect();
        let mut sections: Vec<Section> = Vec::new();
        let mut total = 0;
        let mut unbalanced_quote_lines = Vec::new();
        for (line_number, raw) in text.lines().enumerate() {
            if raw.len() > 8192 {
                return Err("UI line exceeds limit".into());
            }
            let mut quoted = false;
            let mut end = raw.len();
            for (i, c) in raw.char_indices() {
                if c == '"' {
                    quoted = !quoted;
                }
                if c == ';' && !quoted {
                    end = i;
                    break;
                }
            }
            if quoted {
                unbalanced_quote_lines.push(line_number + 1);
            }
            let line = raw[..end].trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                let name = line
                    .strip_prefix('[')
                    .and_then(|v| v.strip_suffix(']'))
                    .ok_or("invalid UI section")?
                    .trim();
                if name.is_empty() || name.contains(['[', ']']) || sections.len() >= 4096 {
                    return Err("invalid UI section name/count".into());
                }
                sections.push(Section {
                    name: name.into(),
                    fields: vec![],
                });
            } else {
                let (key, value) = line.split_once('=').ok_or("invalid UI assignment")?;
                let key = key.trim();
                let value = value.trim();
                if key.is_empty() || key.contains(['[', ']', '"']) || total >= 32768 {
                    return Err("invalid UI key/count".into());
                }
                let section = sections.last_mut().ok_or("UI field before section")?;
                section.fields.push(Field {
                    key: key.into(),
                    value: value.into(),
                });
                total += 1;
            }
        }
        if sections.is_empty() {
            return Err("empty UI document".into());
        }
        Ok(Self {
            sections,
            unbalanced_quote_lines,
        })
    }
    pub fn value(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .filter(|s| s.name == section)
            .flat_map(|s| &s.fields)
            .find(|f| f.key.eq_ignore_ascii_case(key))
            .map(|f| f.value.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Glyph {
    pub code: u32,
    /// File order is left, top, right, bottom. Native runtime rearranges to
    /// left/right/top/bottom at 0x001ccb50; preserve file order in this reader.
    pub uv: [f32; 4],
    /// The four signed 16-bit metrics from the optional second table, in file
    /// order. Rendering interpretation is separate from this verified layout.
    pub metrics: Option<[i16; 4]>,
}
#[derive(Debug, Serialize)]
pub struct Font {
    pub glyphs: Vec<Glyph>,
}
impl Font {
    /// CEL loader 0x001ccb50: count + (code, four floats), then optional count
    /// + (code, four shorts). The second table looks up the low byte of code.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let count = u32le(bytes, 0)? as usize;
        if count == 0 || count > 256 {
            return Err("invalid CEL glyph count".into());
        }
        slice(bytes, 4, count * 20)?;
        let mut glyphs: Vec<Glyph> = Vec::new();
        for i in 0..count {
            let at = 4 + i * 20;
            let code = u32le(bytes, at)?;
            if code > 255 {
                return Err("CEL glyph code exceeds byte range".into());
            }
            let mut uv = [0.; 4];
            for (j, value) in uv.iter_mut().enumerate() {
                *value = f32::from_bits(u32le(bytes, at + 4 + j * 4)?);
            }
            if uv
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || uv[0] > uv[2]
                || uv[1] > uv[3]
            {
                return Err("invalid CEL UV rectangle".into());
            }
            let glyph = Glyph {
                code,
                uv,
                metrics: None,
            };
            if let Some(existing) = glyphs.iter_mut().find(|g| g.code == code) {
                *existing = glyph;
            } else {
                glyphs.push(glyph);
            }
        }
        let at = 4 + count * 20;
        if at != bytes.len() {
            let metric_count = u32le(bytes, at)? as usize;
            if metric_count > 256 || bytes.len() != at + 4 + metric_count * 12 {
                return Err("invalid CEL metric table extent".into());
            }
            for i in 0..metric_count {
                let at = at + 4 + i * 12;
                let code = u32le(bytes, at)? & 255;
                let mut metrics = [0; 4];
                for (j, value) in metrics.iter_mut().enumerate() {
                    *value = u16le(bytes, at + 4 + j * 2)? as i16;
                }
                if let Some(glyph) = glyphs.iter_mut().find(|g| g.code == code) {
                    glyph.metrics = Some(metrics);
                }
            }
        }
        Ok(Self { glyphs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_preserves_empty_fields_quotes_and_first_match() {
        let doc = Document::parse(
            b"[Widget 1]\nType=Text\nFontName=\nText=\"One; two\" ;comment\nType=Icon\0",
        )
        .unwrap();
        assert_eq!(doc.value("Widget 1", "fontname"), Some(""));
        assert_eq!(doc.value("Widget 1", "Text"), Some("\"One; two\""));
        assert_eq!(doc.value("Widget 1", "Type"), Some("Text"));
        let typo = Document::parse(b"[Widget]\nText=\"broken\n").unwrap();
        assert_eq!(typo.unbalanced_quote_lines, [2]);
        assert_eq!(typo.value("Widget", "Text"), Some("\"broken"));
        assert!(Document::parse(b"[Widget]\0\nText=ok").is_err());
    }
    #[test]
    fn font_uv_metrics_and_extent_are_checked() {
        let mut data = Vec::new();
        for value in [
            1u32,
            65,
            0f32.to_bits(),
            0f32.to_bits(),
            0.5f32.to_bits(),
            1f32.to_bits(),
            1,
            65,
        ] {
            data.extend(value.to_le_bytes());
        }
        for value in [-2i16, 12, 10, 24] {
            data.extend(value.to_le_bytes());
        }
        let font = Font::decode(&data).unwrap();
        assert_eq!(font.glyphs[0].metrics, Some([-2, 12, 10, 24]));
        assert_eq!(font.glyphs[0].uv, [0., 0., 0.5, 1.]);
        assert!(Font::decode(&data[..data.len() - 1]).is_err());
        data.extend([0]);
        assert!(Font::decode(&data).is_err());
    }
}
