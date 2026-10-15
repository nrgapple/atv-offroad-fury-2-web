//! Authored INI-style text read with the semantics of the browser's former
//! JavaScript expressions (ECMAScript whitespace, trim, Number and multiline
//! anchors), so moved readers keep identical results.

pub(crate) const LINE_BREAKS: [char; 4] = ['\n', '\r', '\u{2028}', '\u{2029}'];
/// ECMAScript WhiteSpace and LineTerminator (\s and String.prototype.trim).
pub(crate) fn space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\u{b}' | '\u{c}' | ' ' | '\u{a0}' | '\u{feff}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    ) || LINE_BREAKS.contains(&c)
}
pub(crate) fn trim(text: &str) -> &str {
    text.trim_matches(space)
}
pub(crate) fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}
pub(crate) fn uncommented(line: &str) -> &str {
    trim(line.split(';').next().unwrap_or(""))
}
pub(crate) fn skip_space(text: &str, mut at: usize) -> usize {
    while let Some(c) = text[at..].chars().next().filter(|c| space(*c)) {
        at += c.len_utf8();
    }
    at
}
pub(crate) fn starts_with_key(text: &str, at: usize, key: &str) -> bool {
    text.as_bytes()
        .get(at..at + key.len())
        .is_some_and(|bytes| bytes.eq_ignore_ascii_case(key.as_bytes()))
}
pub(crate) fn occurrences<'a>(text: &'a str, key: &'a str) -> impl Iterator<Item = usize> + 'a {
    (0..text.len()).filter(move |at| starts_with_key(text, *at, key))
}
/// `key\s*=\s*` then the position of the value, case-insensitive.
pub(crate) fn assignment(text: &str, at: usize, key: &str) -> Option<usize> {
    let at = skip_space(text, at + key.len());
    text[at..]
        .starts_with('=')
        .then(|| skip_space(text, at + 1))
}
/// First /key\s*=\s*(\d+)/i anywhere in the text.
pub(crate) fn first_digits<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    occurrences(text, key).find_map(|at| {
        let start = assignment(text, at, key)?;
        let end = start
            + text[start..]
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(text.len() - start);
        (end > start).then(|| &text[start..end])
    })
}
/// /^\s*key\s*=\s*[values]\s*$/mi: ^ and $ are line boundaries, \s spans lines.
pub(crate) fn line_assignment(text: &str, key: &str, values: &[char]) -> bool {
    occurrences(text, key).any(|at| {
        let before = text[..at].trim_end_matches(space);
        if !before.is_empty() && !text[before.len()..at].contains(LINE_BREAKS) {
            return false;
        }
        let Some(value) = assignment(text, at, key) else {
            return false;
        };
        if !text[value..].starts_with(values) {
            return false;
        }
        let after = &text[value + 1..];
        let rest = after.trim_start_matches(space);
        rest.is_empty() || after[..after.len() - rest.len()].contains(LINE_BREAKS)
    })
}
/// JavaScript Number(string).
pub(crate) fn js_number(text: &str) -> f64 {
    let text = trim(text);
    let integer = |digits: &str, radix: u32| {
        if digits.is_empty() {
            return f64::NAN;
        }
        digits
            .chars()
            .try_fold(0f64, |n, c| {
                c.to_digit(radix)
                    .map(|d| n * f64::from(radix) + f64::from(d))
            })
            .unwrap_or(f64::NAN)
    };
    match text.get(..2).map(str::to_ascii_lowercase).as_deref() {
        _ if text.is_empty() => return 0.,
        Some("0x") => return integer(&text[2..], 16),
        Some("0o") => return integer(&text[2..], 8),
        Some("0b") => return integer(&text[2..], 2),
        _ => {}
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned == "Infinity" {
        return if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let mantissa = unsigned.split(['e', 'E']).next().unwrap_or("");
    let digits = mantissa.chars().filter(char::is_ascii_digit).count();
    let valid = digits > 0
        && mantissa.chars().all(|c| c.is_ascii_digit() || c == '.')
        && mantissa.matches('.').count() <= 1
        && unsigned[mantissa.len()..].get(1..).is_none_or(|exponent| {
            let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
            !exponent.is_empty() && exponent.chars().all(|c| c.is_ascii_digit())
        });
    if valid {
        text.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

/// Line-start offsets for multiline `^`.
pub(crate) fn line_starts(text: &str) -> impl Iterator<Item = usize> + '_ {
    std::iter::once(0).chain(
        text.char_indices()
            .filter(|(_, c)| LINE_BREAKS.contains(c))
            .map(|(at, c)| at + c.len_utf8()),
    )
}
