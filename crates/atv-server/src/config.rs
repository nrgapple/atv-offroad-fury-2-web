//! Settings from the environment (research/GAME_SERVER.md "Server agnostic").

use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

/// Page origins that open rooms in production: the site, its old workers.dev
/// name and the server's own name (the desktop app presents the server's
/// origin as its page origin).
pub const PRODUCTION_ORIGINS: &str = "https://furyremastered.com,https://www.furyremastered.com,https://atv-offroad-fury-2.champtoday.workers.dev,https://mp.furyremastered.com";
/// Added to the defaults when the server listens on loopback: the local
/// browser-test pages on any port.
pub const LOOPBACK_ORIGIN: &str = "http://127.0.0.1:*";

/// The commit this binary was built from, when the build set `FURY_COMMIT`.
pub const BUILD_COMMIT: Option<&str> = option_env!("FURY_COMMIT");

/// The commit to report: the running environment's `FURY_COMMIT`, else the
/// build's, else `host-only`.
pub fn commit(runtime: Option<String>) -> String {
    runtime
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .or(BUILD_COMMIT
            .map(str::to_string)
            .filter(|c| !c.trim().is_empty()))
        .unwrap_or_else(|| "host-only".into())
}

/// `fury-server --version`: version, commit and protocol on one line.
pub fn version_line(runtime_commit: Option<String>) -> String {
    format!(
        "fury-server {} {} protocol {}",
        env!("CARGO_PKG_VERSION"),
        commit(runtime_commit),
        atv_net::VERSION
    )
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tls {
    Off,
    Acme {
        domains: Vec<String>,
        email: Option<String>,
        cache: PathBuf,
    },
    Files {
        cert: PathBuf,
        key: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub listen: SocketAddr,
    pub origins: Origins,
    pub tls: Tls,
    pub state_dir: PathBuf,
    pub commit: String,
    /// Clients left out of the public rider and room counts.
    pub uncounted: Networks,
    /// Free Ride hub timers (`FURY_HUB_EVENT_SECS`, `FURY_HUB_JOIN_SECS`,
    /// `FURY_HUB_ROTATION_SECS`; research/FREERIDE_HUB.md "Settings").
    pub hub: crate::engine::hub::HubConfig,
}

/// `MIN-MAX` or `N` seconds as a millisecond range; `0` or `off` is None.
fn seconds_range(text: &str) -> Option<Option<(u64, u64)>> {
    if text == "0" || text == "off" {
        return Some(None);
    }
    let (low, high) = text.split_once('-').unwrap_or((text, text));
    let low: u64 = low.trim().parse().ok()?;
    let high: u64 = high.trim().parse().ok()?;
    (low > 0 && low <= high && high <= 24 * 3600).then_some(Some((low * 1000, high * 1000)))
}

impl Config {
    /// Reads `FURY_*` settings through `var`; an invalid value is an error
    /// naming it.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let get = |name: &str| {
            var(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let listen: SocketAddr = get("FURY_LISTEN")
            .unwrap_or_else(|| "127.0.0.1:8789".into())
            .parse()
            .map_err(|_| "FURY_LISTEN must be address:port".to_string())?;
        let origins = match get("FURY_ALLOWED_ORIGINS") {
            Some(list) => Origins::parse(&list),
            None if listen.ip().is_loopback() => {
                Origins::parse(&format!("{PRODUCTION_ORIGINS},{LOOPBACK_ORIGIN}"))
            }
            None => Origins::parse(PRODUCTION_ORIGINS),
        };
        let tls = match get("FURY_TLS").as_deref().unwrap_or("off") {
            "off" => Tls::Off,
            "acme" => {
                let domains: Vec<String> = get("FURY_ACME_DOMAINS")
                    .unwrap_or_default()
                    .split(',')
                    .map(str::trim)
                    .filter(|d| !d.is_empty())
                    .map(String::from)
                    .collect();
                if domains.is_empty() {
                    return Err("FURY_TLS=acme needs FURY_ACME_DOMAINS".into());
                }
                Tls::Acme {
                    domains,
                    email: get("FURY_ACME_EMAIL"),
                    cache: get("FURY_ACME_CACHE")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from("./acme")),
                }
            }
            "files" => match (get("FURY_TLS_CERT"), get("FURY_TLS_KEY")) {
                (Some(cert), Some(key)) => Tls::Files {
                    cert: cert.into(),
                    key: key.into(),
                },
                _ => return Err("FURY_TLS=files needs FURY_TLS_CERT and FURY_TLS_KEY".into()),
            },
            _ => return Err("FURY_TLS must be off, acme or files".into()),
        };
        Ok(Config {
            listen,
            origins,
            tls,
            state_dir: get("FURY_STATE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./state")),
            commit: commit(get("FURY_COMMIT")),
            uncounted: match get("FURY_UNCOUNTED_IPS") {
                Some(list) => Networks::parse(&list).ok_or(
                    "FURY_UNCOUNTED_IPS must be comma-separated addresses or address/prefix blocks",
                )?,
                None => Networks::default(),
            },
            hub: {
                let mut hub = crate::engine::hub::HubConfig::default();
                if let Some(text) = get("FURY_HUB_EVENT_SECS") {
                    hub.events = seconds_range(&text)
                        .ok_or("FURY_HUB_EVENT_SECS must be seconds, MIN-MAX seconds or off")?;
                }
                if let Some(text) = get("FURY_HUB_JOIN_SECS") {
                    hub.join_ms = match seconds_range(&text) {
                        Some(Some((low, high))) if low == high && low <= 600_000 => low,
                        _ => return Err("FURY_HUB_JOIN_SECS must be 1-600 seconds".into()),
                    };
                }
                if let Some(text) = get("FURY_HUB_ROTATION_SECS") {
                    hub.rotation = seconds_range(&text)
                        .ok_or("FURY_HUB_ROTATION_SECS must be seconds, MIN-MAX seconds or off")?;
                }
                hub
            },
        })
    }
}

/// Allowed page origins: exact `scheme://host[:port]` entries, or
/// `scheme://host:*` for any port of that host.
#[derive(Clone, Debug, PartialEq)]
pub struct Origins(Vec<String>);

impl Origins {
    pub fn parse(list: &str) -> Self {
        Origins(
            list.split(',')
                .map(|o| o.trim().trim_end_matches('/'))
                .filter(|o| !o.is_empty())
                .map(String::from)
                .collect(),
        )
    }

    pub fn allows(&self, origin: Option<&str>) -> bool {
        let Some(origin) = origin.filter(|o| !o.is_empty()) else {
            return false;
        };
        self.0
            .iter()
            .any(|allowed| match allowed.strip_suffix(":*") {
                Some(host) => origin
                    .strip_prefix(host)
                    .and_then(|rest| rest.strip_prefix(':'))
                    .is_some_and(|port| {
                        !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
                    }),
                None => allowed == origin,
            })
    }
}

/// Addresses and CIDR blocks (IPv4 or IPv6), e.g. the owner's test
/// machines, whose riders play normally but are left out of the public
/// counts. IPv4-mapped IPv6 addresses match their IPv4 entries. Debug shows
/// only how many entries there are, so settings never log an address.
#[derive(Clone, Default, PartialEq)]
pub struct Networks(Vec<(IpAddr, u8)>);

impl std::fmt::Debug for Networks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Networks({} entries)", self.0.len())
    }
}

impl Networks {
    /// Comma-separated `address` or `address/prefix` entries; None if any
    /// entry is invalid.
    pub fn parse(list: &str) -> Option<Self> {
        list.split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(|entry| {
                let (host, prefix) = entry.split_once('/').unwrap_or((entry, ""));
                let ip = host.parse::<IpAddr>().ok()?.to_canonical();
                let bits = if ip.is_ipv4() { 32 } else { 128 };
                let prefix = match prefix {
                    "" => bits,
                    p if p.bytes().all(|b| b.is_ascii_digit()) => p.parse().ok()?,
                    _ => return None,
                };
                (prefix <= bits).then_some((ip, prefix))
            })
            .collect::<Option<Vec<_>>>()
            .map(Networks)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let value = |ip: IpAddr| match ip {
            IpAddr::V4(v4) => u128::from(u32::from(v4)),
            IpAddr::V6(v6) => u128::from(v6),
        };
        let ip = ip.to_canonical();
        self.0.iter().any(|&(net, prefix)| {
            let bits = if net.is_ipv4() { 32 } else { 128 };
            let shift = u32::from(bits - prefix);
            net.is_ipv4() == ip.is_ipv4()
                && value(net).checked_shr(shift).unwrap_or(0)
                    == value(ip).checked_shr(shift).unwrap_or(0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(vars: &[(&str, &str)]) -> Result<Config, String> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_vars(|name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
    }

    #[test]
    fn defaults_listen_on_the_local_relay_port_with_test_origins() {
        let c = config(&[]).unwrap();
        assert_eq!(c.listen, "127.0.0.1:8789".parse().unwrap());
        assert_eq!(c.tls, Tls::Off);
        assert_eq!(c.commit, commit(None));
        assert_eq!(
            config(&[("FURY_COMMIT", "abc1234")]).unwrap().commit,
            "abc1234"
        );
        let line = version_line(Some("abc1234".into()));
        assert!(
            line.starts_with("fury-server ") && line.ends_with(" protocol 6"),
            "{line}"
        );
        assert_eq!(c.state_dir, PathBuf::from("./state"));
        assert!(c.origins.allows(Some("https://furyremastered.com")));
        assert!(c.origins.allows(Some("http://127.0.0.1:4187")));
        // Listening publicly, only the production origins.
        let c = config(&[("FURY_LISTEN", "0.0.0.0:443")]).unwrap();
        assert!(!c.origins.allows(Some("http://127.0.0.1:4187")));
        assert!(c.origins.allows(Some("https://mp.furyremastered.com")));
    }

    #[test]
    fn origins_match_exactly_or_by_any_port() {
        let o = Origins::parse("https://furyremastered.com, http://127.0.0.1:*,");
        for good in [
            "https://furyremastered.com",
            "http://127.0.0.1:4187",
            "http://127.0.0.1:1",
        ] {
            assert!(o.allows(Some(good)), "{good}");
        }
        for bad in [
            None,
            Some(""),
            Some("null"),
            Some("http://furyremastered.com"),
            Some("https://furyremastered.com.evil.example"),
            Some("http://127.0.0.1"),
            Some("http://127.0.0.1:"),
            Some("http://127.0.0.1:80@evil.example"),
            Some("http://127.0.0.10:80"),
        ] {
            assert!(!o.allows(bad), "{bad:?}");
        }
    }

    #[test]
    fn uncounted_networks_match_addresses_and_blocks() {
        assert!(config(&[]).unwrap().uncounted.is_empty());
        let c = config(&[(
            "FURY_UNCOUNTED_IPS",
            "192.0.2.44, 198.51.100.0/24, 2001:db8:aa:bb::/64,",
        )])
        .unwrap();
        let n = &c.uncounted;
        for ip in [
            "192.0.2.44",
            "198.51.100.7",
            "2001:db8:aa:bb:1:2:3:4",
            "::ffff:192.0.2.44",
        ] {
            assert!(n.contains(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["192.0.2.45", "198.51.101.7", "2001:db8:aa:bc::1", "::1"] {
            assert!(!n.contains(ip.parse().unwrap()), "{ip}");
        }
        assert!(
            !format!("{c:?}").contains("192.0.2"),
            "settings never log addresses"
        );
        let all = Networks::parse("0.0.0.0/0, ::/0").unwrap();
        assert!(
            all.contains("203.0.113.1".parse().unwrap())
                && all.contains("2001:db8::1".parse().unwrap())
        );
        for bad in [
            "192.0.2.1/33",
            "2001:db8::/129",
            "example.com",
            "192.0.2.1/x",
            "192.0.2.1/-1",
        ] {
            assert!(config(&[("FURY_UNCOUNTED_IPS", bad)]).is_err(), "{bad}");
        }
    }

    #[test]
    fn tls_modes_need_their_settings() {
        assert!(config(&[("FURY_TLS", "acme")]).is_err());
        assert!(config(&[("FURY_TLS", "files"), ("FURY_TLS_CERT", "c.pem")]).is_err());
        assert!(config(&[("FURY_TLS", "on")]).is_err());
        assert!(config(&[("FURY_LISTEN", "nowhere")]).is_err());
        let c = config(&[
            ("FURY_TLS", "acme"),
            ("FURY_ACME_DOMAINS", "mp.furyremastered.com, other.example"),
            ("FURY_ACME_EMAIL", "ops@example.com"),
            ("FURY_ACME_CACHE", "/var/lib/fury/acme"),
        ])
        .unwrap();
        assert_eq!(
            c.tls,
            Tls::Acme {
                domains: vec!["mp.furyremastered.com".into(), "other.example".into()],
                email: Some("ops@example.com".into()),
                cache: "/var/lib/fury/acme".into(),
            }
        );
    }
}
