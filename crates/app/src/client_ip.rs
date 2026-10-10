//! The client's real address, also behind a reverse proxy.
//!
//! Proxy headers are trusted only when the TCP peer is one of the configured
//! proxies (`AKASHA_TRUSTED_PROXIES`, CIDRs or bare addresses; empty by default,
//! so nobody is trusted). Then the client is the **rightmost** address in
//! `X-Forwarded-For` that is not itself a trusted proxy: every proxy appends the
//! address it received the request from, so entries to the left of the first
//! untrusted hop may have been written by the client and prove nothing.
//! `Forwarded` (RFC 7239) is read only when there is no `X-Forwarded-For`, so a
//! proxy that sets only `X-Forwarded-For` cannot be bypassed with a forged
//! `Forwarded` header.
//!
//! [`middleware`] resolves the address once per request and stores a
//! [`ClientAddr`]; the credential rate limiter ([`ClientIpKeyExtractor`]), the
//! activity/security log and the session list all read it from there.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, header::FORWARDED},
    middleware::Next,
    response::Response,
};
use ipnet::IpNet;
use tower_governor::{GovernorError, key_extractor::KeyExtractor};

/// The configured reverse proxies.
#[derive(Debug, Clone, Default)]
pub struct TrustedProxies(Vec<IpNet>);

impl TrustedProxies {
    /// Parse CIDRs (`10.0.0.0/8`, `fd00::/8`) and bare addresses (`127.0.0.1`).
    pub fn parse<S: AsRef<str>>(entries: &[S]) -> Result<Self, String> {
        entries
            .iter()
            .map(|entry| {
                let entry = entry.as_ref().trim();
                entry
                    .parse::<IpNet>()
                    .or_else(|_| entry.parse::<IpAddr>().map(IpNet::from))
                    .map(|net| net.trunc())
                    .map_err(|_| format!("not an IP address or CIDR: {entry:?}"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = canonical(ip);
        self.0.iter().any(|net| net.contains(&ip))
    }
}

/// Where a request really came from. Stored in the request extensions by
/// [`middleware`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientAddr {
    pub ip: IpAddr,
    /// The client used HTTPS: a trusted proxy said so (`X-Forwarded-Proto` or
    /// `Forwarded: proto=`). Akasha itself only speaks plain HTTP.
    pub https: bool,
}

/// The client behind `peer`, the TCP address that connected to us.
pub fn resolve(peer: IpAddr, headers: &HeaderMap, trusted: &TrustedProxies) -> ClientAddr {
    let peer = canonical(peer);
    if !trusted.contains(peer) {
        return ClientAddr {
            ip: peer,
            https: false,
        };
    }
    let (hops, proto) = match header_values(headers, "x-forwarded-for") {
        Some(xff) => (
            xff.split(',').map(str::trim).map(str::to_owned).collect(),
            header_values(headers, "x-forwarded-proto")
                .and_then(|p| p.split(',').next().map(|p| p.trim().to_ascii_lowercase())),
        ),
        None => forwarded(headers),
    };
    ClientAddr {
        ip: rightmost_untrusted(peer, &hops, trusted),
        https: proto.as_deref() == Some("https"),
    }
}

/// Walk the hops from the nearest one back; the first address that is not a
/// trusted proxy is the client. A hop we cannot read (`unknown`, obfuscated
/// identifiers, garbage) ends the walk at the last trusted address: anything
/// further left is unverifiable.
fn rightmost_untrusted(peer: IpAddr, hops: &[String], trusted: &TrustedProxies) -> IpAddr {
    let mut client = peer;
    for hop in hops.iter().rev() {
        let Some(ip) = parse_hop(hop) else {
            break;
        };
        client = ip;
        if !trusted.contains(ip) {
            break;
        }
    }
    client
}

/// Every value of a (possibly repeated) header, joined with commas in order.
fn header_values(headers: &HeaderMap, name: &str) -> Option<String> {
    let values: Vec<&str> = headers
        .get_all(name)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    (!values.is_empty()).then(|| values.join(","))
}

/// `for=` hops and the first `proto=` of the `Forwarded` header.
fn forwarded(headers: &HeaderMap) -> (Vec<String>, Option<String>) {
    let mut hops = Vec::new();
    let mut proto = None;
    let joined = header_values(headers, FORWARDED.as_str()).unwrap_or_default();
    for element in joined.split(',') {
        for pair in element.split(';') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            match key.trim().to_ascii_lowercase().as_str() {
                "for" => hops.push(value.to_owned()),
                "proto" if proto.is_none() => proto = Some(value.to_ascii_lowercase()),
                _ => {}
            }
        }
    }
    (hops, proto)
}

/// `1.2.3.4`, `1.2.3.4:5678`, `[2001:db8::1]:443`, `2001:db8::1`.
fn parse_hop(hop: &str) -> Option<IpAddr> {
    let hop = hop.trim().trim_matches('"');
    if let Ok(ip) = hop.parse::<IpAddr>() {
        return Some(canonical(ip));
    }
    if let Ok(addr) = hop.parse::<SocketAddr>() {
        return Some(canonical(addr.ip()));
    }
    // `[2001:db8::1]` without a port.
    hop.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .and_then(|h| h.parse::<IpAddr>().ok())
        .map(canonical)
}

/// `::ffff:1.2.3.4` → `1.2.3.4`.
pub fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    }
}

/// Resolve the client once per request (see the module docs). Requests without
/// connect info (never in `serve`) get no [`ClientAddr`].
pub async fn middleware(
    State(trusted): State<Arc<TrustedProxies>>,
    mut req: Request,
    next: Next,
) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());
    if let Some(peer) = peer {
        let client = resolve(peer, req.headers(), &trusted);
        req.extensions_mut().insert(client);
    }
    next.run(req).await
}

/// Rate-limit key: the resolved client address (the TCP peer if the middleware
/// did not run).
#[derive(Debug, Clone, Copy)]
pub struct ClientIpKeyExtractor;

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        req.extensions()
            .get::<ClientAddr>()
            .map(|c| c.ip)
            .or_else(|| {
                req.extensions()
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|ConnectInfo(addr)| canonical(addr.ip()))
            })
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("ip")
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_static(value));
        }
        map
    }

    fn proxies(list: &[&str]) -> TrustedProxies {
        TrustedProxies::parse(list).expect("valid")
    }

    #[test]
    fn parses_cidrs_and_bare_addresses() {
        let t = proxies(&["10.0.0.0/8", "192.168.1.5", "fd00::/8", " ::1 "]);
        assert!(t.contains(ip("10.1.2.3")));
        assert!(t.contains(ip("192.168.1.5")));
        assert!(!t.contains(ip("192.168.1.6")));
        assert!(t.contains(ip("fd12::1")));
        assert!(t.contains(ip("::1")));
        assert!(t.contains(ip("::ffff:10.0.0.1")), "IPv4-mapped peers match");
        assert!(TrustedProxies::parse(&["10.0.0.0/33"]).is_err());
        assert!(TrustedProxies::parse(&["proxy.local"]).is_err());
        assert!(TrustedProxies::default().is_empty());
    }

    #[test]
    fn untrusted_peers_cannot_spoof() {
        let h = headers(&[
            ("x-forwarded-for", "6.6.6.6"),
            ("x-forwarded-proto", "https"),
            ("forwarded", "for=7.7.7.7"),
        ]);
        let none = resolve(ip("203.0.113.9"), &h, &TrustedProxies::default());
        assert_eq!(none.ip, ip("203.0.113.9"));
        assert!(!none.https);
        let other = resolve(ip("203.0.113.9"), &h, &proxies(&["10.0.0.0/8"]));
        assert_eq!(other.ip, ip("203.0.113.9"));
    }

    #[test]
    fn takes_the_rightmost_untrusted_hop() {
        let t = proxies(&["10.0.0.0/8"]);
        // The client forged the first entry; the proxy appended the real address.
        let h = headers(&[("x-forwarded-for", "1.1.1.1, 198.51.100.7")]);
        assert_eq!(resolve(ip("10.0.0.2"), &h, &t).ip, ip("198.51.100.7"));
        // A chain of trusted proxies (CDN edge 10.9.9.9 → nginx 10.0.0.2).
        let h = headers(&[("x-forwarded-for", "1.1.1.1, 198.51.100.7, 10.9.9.9")]);
        assert_eq!(resolve(ip("10.0.0.2"), &h, &t).ip, ip("198.51.100.7"));
        // Repeated headers count as one list, in order.
        let h = headers(&[
            ("x-forwarded-for", "1.1.1.1"),
            ("x-forwarded-for", "198.51.100.8"),
        ]);
        assert_eq!(resolve(ip("10.0.0.2"), &h, &t).ip, ip("198.51.100.8"));
        // Every hop trusted: the leftmost one.
        let h = headers(&[("x-forwarded-for", "10.1.1.1, 10.2.2.2")]);
        assert_eq!(resolve(ip("10.0.0.2"), &h, &t).ip, ip("10.1.1.1"));
        // No header: the proxy itself.
        assert_eq!(
            resolve(ip("10.0.0.2"), &HeaderMap::new(), &t).ip,
            ip("10.0.0.2")
        );
        // Garbage stops the walk at the last trusted address.
        let h = headers(&[("x-forwarded-for", "1.1.1.1, unknown, 10.9.9.9")]);
        assert_eq!(resolve(ip("10.0.0.2"), &h, &t).ip, ip("10.9.9.9"));
        // Ports and IPv6 forms.
        let h = headers(&[("x-forwarded-for", "[2001:db8::7]:4711")]);
        assert_eq!(resolve(ip("10.0.0.2"), &h, &t).ip, ip("2001:db8::7"));
        let h = headers(&[("x-forwarded-for", "198.51.100.9:5000")]);
        assert_eq!(resolve(ip("10.0.0.2"), &h, &t).ip, ip("198.51.100.9"));
    }

    #[test]
    fn reads_forwarded_only_without_x_forwarded_for() {
        let t = proxies(&["127.0.0.1"]);
        let h = headers(&[(
            "forwarded",
            "for=1.1.1.1, for=\"[2001:db8::cafe]:443\";proto=https;by=127.0.0.1",
        )]);
        let c = resolve(ip("127.0.0.1"), &h, &t);
        assert_eq!(c.ip, ip("2001:db8::cafe"));
        assert!(c.https);
        let both = headers(&[
            ("forwarded", "for=6.6.6.6"),
            ("x-forwarded-for", "198.51.100.7"),
        ]);
        assert_eq!(resolve(ip("127.0.0.1"), &both, &t).ip, ip("198.51.100.7"));
    }

    #[test]
    fn https_comes_from_trusted_proxies_only() {
        let t = proxies(&["127.0.0.1"]);
        let h = headers(&[
            ("x-forwarded-for", "198.51.100.7"),
            ("x-forwarded-proto", "HTTPS"),
        ]);
        assert!(resolve(ip("127.0.0.1"), &h, &t).https);
        assert!(!resolve(ip("127.0.0.2"), &h, &t).https);
        let http = headers(&[
            ("x-forwarded-for", "198.51.100.7"),
            ("x-forwarded-proto", "http"),
        ]);
        assert!(!resolve(ip("127.0.0.1"), &http, &t).https);
    }
}
