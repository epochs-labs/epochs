//! `epochs://host:port` URL parsing.

use std::net::{SocketAddr, ToSocketAddrs};

use crate::error::{Error, Result};

const DEFAULT_PORT: u16 = 7420;

/// Parsed connection target for EPX.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochsUrl {
    /// Host name or IP (as written in the URL).
    pub host: String,
    /// TCP port (default **7420**).
    pub port: u16,
}

impl EpochsUrl {
    /// Parse an `epochs://` URL (host required; port optional).
    ///
    /// Accepts `epochs://127.0.0.1:7420`, `epochs://localhost`, and bare
    /// `host:port` / `host` as a convenience.
    pub fn parse(input: &str) -> Result<Self> {
        let s = input.trim();
        let rest = if let Some(r) = s.strip_prefix("epochs://") {
            r
        } else if s.contains("://") {
            return Err(Error::Url(format!(
                "expected epochs:// scheme, got {input:?}"
            )));
        } else {
            s
        };

        let rest = rest.trim_start_matches('/').trim_end_matches('/');
        if rest.is_empty() {
            return Err(Error::Url("missing host".into()));
        }

        // Strip optional path/query (ignored in v1).
        let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest).trim();
        if authority.is_empty() {
            return Err(Error::Url("missing host".into()));
        }

        let (host, port) = if let Some(host) = authority.strip_prefix('[') {
            // IPv6: [addr]:port
            let (addr, after) = host
                .split_once(']')
                .ok_or_else(|| Error::Url("unclosed IPv6 bracket".into()))?;
            let port = if after.is_empty() {
                DEFAULT_PORT
            } else if let Some(p) = after.strip_prefix(':') {
                parse_port(p)?
            } else {
                return Err(Error::Url(format!("junk after IPv6 address: {after:?}")));
            };
            (addr.to_string(), port)
        } else if let Some((h, p)) = authority.rsplit_once(':') {
            // Only treat as port if host has no other colons (not bare IPv6).
            if h.contains(':') {
                (authority.to_string(), DEFAULT_PORT)
            } else if p.chars().all(|c| c.is_ascii_digit()) {
                (h.to_string(), parse_port(p)?)
            } else {
                (authority.to_string(), DEFAULT_PORT)
            }
        } else {
            (authority.to_string(), DEFAULT_PORT)
        };

        if host.is_empty() {
            return Err(Error::Url("empty host".into()));
        }

        Ok(Self { host, port })
    }

    /// Resolve to a [`SocketAddr`] (DNS if needed).
    pub fn to_socket_addr(&self) -> Result<SocketAddr> {
        let mut iter = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| Error::Url(format!("resolve {}:{}: {e}", self.host, self.port)))?;
        iter.next()
            .ok_or_else(|| Error::Url(format!("no addresses for {}:{}", self.host, self.port)))
    }

    /// `host:port` display form.
    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn parse_port(p: &str) -> Result<u16> {
    p.parse::<u16>()
        .map_err(|_| Error::Url(format!("invalid port: {p:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_url() {
        let u = EpochsUrl::parse("epochs://127.0.0.1:7420").unwrap();
        assert_eq!(u.host, "127.0.0.1");
        assert_eq!(u.port, 7420);
    }

    #[test]
    fn default_port() {
        let u = EpochsUrl::parse("epochs://localhost").unwrap();
        assert_eq!(u.host, "localhost");
        assert_eq!(u.port, DEFAULT_PORT);
    }

    #[test]
    fn bare_host_port() {
        let u = EpochsUrl::parse("db.example:9000").unwrap();
        assert_eq!(u.host, "db.example");
        assert_eq!(u.port, 9000);
    }

    #[test]
    fn rejects_http() {
        assert!(EpochsUrl::parse("http://127.0.0.1:7420").is_err());
    }
}
