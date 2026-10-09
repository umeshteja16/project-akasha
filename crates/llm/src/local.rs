//! Which endpoints count as local for strict offline mode.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use reqwest::Url;

/// `true` if `url` points at this machine or the local network: loopback,
/// private and link-local addresses, `localhost`, `*.local`, `*.lan`,
/// `*.internal`, `*.home.arpa`, or a single-label host name (a Docker Compose
/// service such as `ollama`). Anything else is assumed to be the internet.
pub fn is_local_url(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    url.host_str()
        .is_some_and(|host| is_local_name(host.trim_start_matches('[').trim_end_matches(']')))
}

fn is_local_name(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if let Ok(ip) = name.parse::<IpAddr>() {
        return is_local_ip(ip);
    }
    !name.contains('.')
        || [".localhost", ".local", ".lan", ".internal", ".home.arpa"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_local_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_local_v4(v4);
            }
            v6.is_loopback() || is_unique_local(v6) || is_link_local(v6)
        }
    }
}

fn is_local_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_private() || ip.is_link_local()
}

fn is_unique_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn is_link_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_and_remote_urls() {
        for local in [
            "http://localhost:11434",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:11434",
            "http://192.168.1.20:11434",
            "http://10.0.0.5",
            "http://ollama:11434",
            "http://gpu-box.local:8000/v1",
            "http://llm.internal/v1",
        ] {
            assert!(is_local_url(local), "{local}");
        }
        for remote in [
            "https://api.openai.com/v1",
            "https://openrouter.ai/api/v1",
            "http://8.8.8.8",
            "http://[2001:db8::1]",
            "not a url",
        ] {
            assert!(!is_local_url(remote), "{remote}");
        }
    }
}
