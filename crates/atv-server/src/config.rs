//! Settings from the environment (research/GAME_SERVER.md "Server agnostic").

use std::{net::SocketAddr, path::PathBuf};

/// Page origins that open rooms in production: the site, its old workers.dev
/// name and the server's own name (the desktop app presents the server's
/// origin as its page origin).
pub const PRODUCTION_ORIGINS: &str = "https://furyremastered.com,https://www.furyremastered.com,https://atv-offroad-fury-2.champtoday.workers.dev,https://mp.furyremastered.com";
/// Added to the defaults when the server listens on loopback: the local
/// browser-test pages on any port.
pub const LOOPBACK_ORIGIN: &str = "http://127.0.0.1:*";

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
            commit: get("FURY_COMMIT").unwrap_or_else(|| "host-only".into()),
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
        assert_eq!(c.commit, "host-only");
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
