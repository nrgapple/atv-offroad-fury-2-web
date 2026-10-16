//! Very simple name filter for shared Free Ride rooms, where strangers see
//! each other's names. A flagged name is replaced entirely, never partially
//! masked, so nothing offensive survives as asterisks around it.
use crate::PeerId;

/// Flagged anywhere in a name, after folding look-alike characters.
const ANYWHERE: &[&str] = &[
    "fuck", "fuk", "shit", "cunt", "bitch", "nigg", "niga", "fag", "slut", "whore", "pussy",
    "nazi", "hitler", "kike", "chink", "retard", "twat", "wank", "porn", "penis", "vagina",
    "dildo", "jizz", "bastard", "molest",
];
/// Flagged only as a whole word, since they occur inside ordinary names.
const WHOLE: &[&str] = &[
    "ass", "arse", "dick", "cock", "tit", "tits", "spic", "homo", "kkk", "sex", "hoe", "cum",
    "rape", "pedo",
];

fn fold(c: char) -> Option<char> {
    Some(match c.to_ascii_lowercase() {
        '0' => 'o',
        '1' | '!' | '|' => 'i',
        '3' => 'e',
        '4' | '@' => 'a',
        '5' | '$' => 's',
        '7' => 't',
        '8' => 'b',
        c if c.is_ascii_alphabetic() => c,
        _ => return None,
    })
}

pub fn offensive_name(name: &str) -> bool {
    // Separators are dropped so "f.u.c.k" and "f u c k" still match.
    let joined = name.chars().filter_map(fold).collect::<String>();
    if ANYWHERE.iter().any(|word| joined.contains(word)) {
        return true;
    }
    name.split(|c: char| c.is_whitespace() || c == '_' || c == '-' || c == '.')
        .map(|word| word.chars().filter_map(fold).collect::<String>())
        .any(|word| WHOLE.contains(&word.as_str()))
}

/// The name other riders see in a shared room.
pub fn public_name(name: &str, peer: PeerId) -> String {
    if offensive_name(name) || name.trim().is_empty() {
        format!("Rider {}", u16::from(peer) + 1)
    } else {
        name.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_obvious_words_and_spellings_but_keeps_ordinary_names() {
        for bad in [
            "fuck",
            "FuCk3r",
            "sh1t rider",
            "f.u.c.k",
            "b!tch",
            "Big Ass",
            "dick_42",
        ] {
            assert!(offensive_name(bad), "{bad}");
            assert_eq!(public_name(bad, 4), "Rider 5");
        }
        for fine in [
            "Cassie",
            "Dickson",
            "Hancock",
            "Grapefruit",
            "Torpedo",
            "Cumberland",
            "Titan",
            "Assassin",
            "Sam",
        ] {
            assert!(!offensive_name(fine), "{fine}");
            assert_eq!(public_name(fine, 0), fine);
        }
        assert_eq!(public_name("   ", 19), "Rider 20");
    }
}
