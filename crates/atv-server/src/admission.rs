//! Per-address admission: how many rooms an address may open or claim, and
//! how many room sockets it may open and hold. One school's Chromebooks
//! share one public address, so the limits are generous; they stop floods,
//! not classrooms. Loopback peers (local tests, the load tester, a proxy on
//! the same host that does not say who it forwards) are not limited.

use std::collections::HashMap;
use std::net::IpAddr;

/// Private room creations: a burst of 30, then one every 2 s.
const ROOMS_BURST: f64 = 30.;
const ROOMS_PER_MS: f64 = 1. / 2000.;
/// Free Ride claims (a slot in a shared room, not a new room): a whole
/// class arriving together, then two a second, like the sockets they open.
const CLAIMS_BURST: f64 = 120.;
const CLAIMS_PER_MS: f64 = 2. / 1000.;
/// Room sockets opened: a burst of 120, then two a second.
const SOCKETS_BURST: f64 = 120.;
const SOCKETS_PER_MS: f64 = 2. / 1000.;
/// Room sockets one address may hold open.
const SOCKETS_OPEN: u32 = 200;
/// Addresses remembered before idle ones are forgotten.
const REMEMBERED: usize = 10_000;

#[derive(Clone, Copy, Debug)]
struct Address {
    rooms: f64,
    claims: f64,
    sockets: f64,
    open: u32,
    at_ms: u64,
}

impl Address {
    fn refill(&mut self, now: u64) {
        let ms = now.saturating_sub(self.at_ms) as f64;
        self.rooms = (self.rooms + ms * ROOMS_PER_MS).min(ROOMS_BURST);
        self.claims = (self.claims + ms * CLAIMS_PER_MS).min(CLAIMS_BURST);
        self.sockets = (self.sockets + ms * SOCKETS_PER_MS).min(SOCKETS_BURST);
        self.at_ms = now;
    }
    fn idle(&self) -> bool {
        self.open == 0
            && self.rooms >= ROOMS_BURST
            && self.claims >= CLAIMS_BURST
            && self.sockets >= SOCKETS_BURST
    }
}

#[derive(Default)]
pub struct Admission {
    addresses: HashMap<IpAddr, Address>,
}

impl Admission {
    fn address(&mut self, ip: IpAddr, now: u64) -> &mut Address {
        if self.addresses.len() >= REMEMBERED && !self.addresses.contains_key(&ip) {
            self.addresses.retain(|_, a| {
                a.refill(now);
                !a.idle()
            });
        }
        let a = self.addresses.entry(ip).or_insert(Address {
            rooms: ROOMS_BURST,
            claims: CLAIMS_BURST,
            sockets: SOCKETS_BURST,
            open: 0,
            at_ms: now,
        });
        a.refill(now);
        a
    }

    /// May `ip` open a private room now?
    pub fn room(&mut self, ip: IpAddr, now: u64) -> bool {
        if ip.is_loopback() {
            return true;
        }
        let a = self.address(ip, now);
        if a.rooms < 1. {
            return false;
        }
        a.rooms -= 1.;
        true
    }

    /// May `ip` claim a Free Ride slot now?
    pub fn claim(&mut self, ip: IpAddr, now: u64) -> bool {
        if ip.is_loopback() {
            return true;
        }
        let a = self.address(ip, now);
        if a.claims < 1. {
            return false;
        }
        a.claims -= 1.;
        true
    }

    /// May `ip` open a room socket now? Each admitted socket must be
    /// [`Self::closed`] when it ends.
    pub fn socket(&mut self, ip: IpAddr, now: u64) -> bool {
        if ip.is_loopback() {
            return true;
        }
        let a = self.address(ip, now);
        if a.sockets < 1. || a.open >= SOCKETS_OPEN {
            return false;
        }
        a.sockets -= 1.;
        a.open += 1;
        true
    }

    pub fn closed(&mut self, ip: IpAddr) {
        if let Some(a) = self.addresses.get_mut(&ip) {
            a.open = a.open.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_get_a_burst_then_a_steady_rate() {
        let mut admission = Admission::default();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        for _ in 0..30 {
            assert!(admission.room(ip, 0));
        }
        assert!(!admission.room(ip, 0));
        assert!(!admission.room(ip, 1999));
        assert!(admission.room(ip, 2000));
        // Another address is unaffected; loopback is never limited.
        assert!(admission.room("203.0.113.8".parse().unwrap(), 0));
        for _ in 0..1000 {
            assert!(admission.room("127.0.0.1".parse().unwrap(), 0));
            assert!(admission.socket("::1".parse().unwrap(), 0));
        }
        // A class joining Free Ride together is not held to the room rate.
        let school: IpAddr = "192.0.2.10".parse().unwrap();
        for _ in 0..120 {
            assert!(admission.claim(school, 0));
        }
        assert!(!admission.claim(school, 0));
        assert!(admission.claim(school, 500));
        assert!(
            admission.room(school, 500),
            "claims leave private rooms alone"
        );
        // Sockets: a burst, a rate, and a cap on open ones.
        for _ in 0..120 {
            assert!(admission.socket(ip, 0));
        }
        assert!(!admission.socket(ip, 0));
        assert!(admission.socket(ip, 500));
        let held: IpAddr = "198.51.100.1".parse().unwrap();
        for k in 0..u64::from(SOCKETS_OPEN) {
            assert!(admission.socket(held, k * 1000));
        }
        assert!(!admission.socket(held, 10_000_000));
        admission.closed(held);
        assert!(admission.socket(held, 10_000_000));
    }
}
