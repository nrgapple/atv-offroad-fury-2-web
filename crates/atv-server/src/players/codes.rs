//! Rider secrets, player ids, signed tokens and public friend codes
//! (research/ONLINE_IDENTITY.md "Identity").

use ring::{digest, hmac};

use crate::engine::Entropy;

/// Friend-code symbols: Crockford base32 without 0, 1 and the vowels A and
/// E (Crockford already leaves out I, L, O and U), so a code never spells a
/// word and never mixes up 0/O or 1/I.
pub const FRIEND_ALPHABET: &[u8; 28] = b"23456789BCDFGHJKMNPQRSTVWXYZ";
pub const FRIEND_CODE_LEN: usize = 8;
/// A token is good for this long; clients register again at every start.
pub const TOKEN_TTL_S: u64 = 30 * 86_400;
const TOKEN_PREFIX: &str = "r1";

/// A rider secret is 32 to 64 lowercase hex digits (the client makes 16
/// random bytes).
pub fn valid_secret(secret: &str) -> bool {
    (32..=64).contains(&secret.len())
        && secret
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The stable player id of a secret: 32 hex digits of its SHA-256. The
/// server never stores the secret, and the id does not depend on the
/// signing key, so a lost key only means fresh tokens.
pub fn pid(secret: &str) -> String {
    let hash = digest::digest(&digest::SHA256, format!("fury-rider:{secret}").as_bytes());
    hex(&hash.as_ref()[..16])
}

pub fn valid_pid(pid: &str) -> bool {
    pid.len() == 32
        && pid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// `r1.<pid>.<issued seconds>.<HMAC-SHA256 hex>`.
pub fn token(key: &hmac::Key, pid: &str, issued_s: u64) -> String {
    let body = format!("{TOKEN_PREFIX}.{pid}.{issued_s}");
    let tag = hmac::sign(key, body.as_bytes());
    format!("{body}.{}", hex(tag.as_ref()))
}

/// The player id a token was issued for, while it is unexpired and signed
/// with `key`.
pub fn verify(key: &hmac::Key, token: &str, now_s: u64) -> Option<String> {
    if token.len() > 160 {
        return None;
    }
    let (body, tag) = token.rsplit_once('.')?;
    let mut parts = body.split('.');
    let (Some(TOKEN_PREFIX), Some(pid), Some(issued), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let issued: u64 = issued.parse().ok()?;
    if !valid_pid(pid) || issued > now_s + 300 || now_s.saturating_sub(issued) > TOKEN_TTL_S {
        return None;
    }
    hmac::verify(key, body.as_bytes(), &unhex(tag)?).ok()?;
    Some(pid.to_string())
}

/// A short reference to `other` that only `viewer` sees: lists hand these
/// out instead of player ids, so a list never leaks a stable id.
pub fn reference(key: &hmac::Key, viewer: &str, other: &str) -> String {
    let tag = hmac::sign(key, format!("ref:{viewer}:{other}").as_bytes());
    hex(&tag.as_ref()[..6])
}

/// A new random friend code (8 symbols, stored without the dash).
pub fn friend_code(rng: &mut dyn Entropy) -> String {
    let mut bytes = [0u8; FRIEND_CODE_LEN];
    rng.fill(&mut bytes);
    bytes
        .iter()
        // 252 = 9 × 28: values from 252 up would bias the first symbols,
        // so fold them back with a second draw.
        .map(|&b| {
            let mut b = b;
            while b >= 252 {
                let mut again = [0u8; 1];
                rng.fill(&mut again);
                b = again[0];
            }
            char::from(FRIEND_ALPHABET[usize::from(b) % FRIEND_ALPHABET.len()])
        })
        .collect()
}

/// `XXXX-XXXX` for display.
pub fn format_code(code: &str) -> String {
    if code.len() == FRIEND_CODE_LEN {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.to_string()
    }
}

/// Reads a typed friend code: case, spaces and dashes do not matter; any
/// symbol outside the alphabet refuses it.
pub fn parse_code(text: &str) -> Option<String> {
    let code: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    (code.len() == FRIEND_CODE_LEN && code.bytes().all(|b| FRIEND_ALPHABET.contains(&b)))
        .then_some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(u8);
    impl Entropy for Counter {
        fn fill(&mut self, out: &mut [u8]) {
            for b in out {
                self.0 = self.0.wrapping_add(37);
                *b = self.0;
            }
        }
    }

    fn key() -> hmac::Key {
        hmac::Key::new(hmac::HMAC_SHA256, &[7; 32])
    }

    #[test]
    fn secrets_and_ids() {
        assert!(valid_secret(&"ab".repeat(16)));
        assert!(!valid_secret(&"AB".repeat(16)));
        assert!(!valid_secret("abc"));
        assert!(!valid_secret(&"g".repeat(32)));
        let a = pid(&"ab".repeat(16));
        assert!(valid_pid(&a));
        assert_eq!(a, pid(&"ab".repeat(16)));
        assert_ne!(a, pid(&"cd".repeat(16)));
    }

    #[test]
    fn tokens_verify_expire_and_refuse_forgery() {
        let id = pid(&"ab".repeat(16));
        let t = token(&key(), &id, 1_000);
        assert_eq!(verify(&key(), &t, 1_000).as_deref(), Some(id.as_str()));
        assert_eq!(
            verify(&key(), &t, 1_000 + TOKEN_TTL_S).as_deref(),
            Some(id.as_str())
        );
        assert_eq!(verify(&key(), &t, 1_001 + TOKEN_TTL_S), None);
        // Issued in the future.
        assert_eq!(verify(&key(), &t, 500), None);
        // Another server key.
        let other = hmac::Key::new(hmac::HMAC_SHA256, &[8; 32]);
        assert_eq!(verify(&other, &t, 1_000), None);
        // Another id under the same tag.
        let other_id = pid(&"cd".repeat(16));
        let forged = t.replace(&id, &other_id);
        assert_eq!(verify(&key(), &forged, 1_000), None);
        assert_eq!(verify(&key(), "r1.x.1.00", 1), None);
        assert_eq!(verify(&key(), "", 1), None);
    }

    #[test]
    fn friend_codes_use_the_safe_alphabet() {
        let mut rng = Counter(0);
        for _ in 0..200 {
            let code = friend_code(&mut rng);
            assert_eq!(code.len(), 8);
            assert!(code.bytes().all(|b| FRIEND_ALPHABET.contains(&b)));
            for bad in b"01OILUAE" {
                assert!(!code.as_bytes().contains(bad));
            }
            assert_eq!(parse_code(&format_code(&code)).as_deref(), Some(&*code));
        }
        assert_eq!(parse_code(" kq7m-3xpz ").as_deref(), Some("KQ7M3XPZ"));
        assert_eq!(parse_code("KQ7M-3XP0"), None);
        assert_eq!(parse_code("KQ7M-3XPA"), None);
        assert_eq!(parse_code("KQ7M3XP"), None);
        assert_eq!(format_code("KQ7M3XPZ"), "KQ7M-3XPZ");
    }

    #[test]
    fn references_are_per_viewer() {
        let a = pid(&"ab".repeat(16));
        let b = pid(&"cd".repeat(16));
        let r = reference(&key(), &a, &b);
        assert_eq!(r.len(), 12);
        assert_eq!(r, reference(&key(), &a, &b));
        assert_ne!(r, reference(&key(), &b, &a));
    }
}
