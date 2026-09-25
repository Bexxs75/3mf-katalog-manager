//! Nur Adressen im Heimnetz. Die Prüfung läuft bei jedem Verbindungsaufbau
//! erneut, und verbunden wird zur geprüften IP (kein zweiter DNS-Lookup).

use std::net::{IpAddr, ToSocketAddrs};

use super::LinkError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressPolicy {
    pub allow_loopback: bool,
}

impl AddressPolicy {
    pub const HOME_NETWORK: Self = Self { allow_loopback: false };
    #[cfg(test)]
    pub const TEST: Self = Self { allow_loopback: true };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub ip: IpAddr,
    pub port: Option<u16>,
}

fn valid_host_chars(host: &str) -> bool {
    !host.is_empty()
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
}

fn parse_port(s: &str) -> Result<u16, LinkError> {
    match s.parse::<u16>() {
        Ok(p) if p > 0 => Ok(p),
        _ => Err(LinkError::AddressNotAllowed),
    }
}

/// Zerlegt "host", "host:port", "[ipv6]:port" oder eine nackte IPv6-Adresse.
/// Kein Schema, kein Pfad, keine Anmeldedaten.
pub fn split_host_port(input: &str) -> Result<(String, Option<u16>), LinkError> {
    let s = input.trim();
    if s.is_empty() || s.contains("://") || s.contains('/') || s.contains('@') || s.contains(char::is_whitespace) {
        return Err(LinkError::AddressNotAllowed);
    }
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (host, after) = rest.split_once(']').ok_or(LinkError::AddressNotAllowed)?;
        match after {
            "" => (host.to_string(), None),
            _ => {
                let p = after.strip_prefix(':').ok_or(LinkError::AddressNotAllowed)?;
                (host.to_string(), Some(parse_port(p)?))
            }
        }
    } else if s.matches(':').count() >= 2 {
        (s.to_string(), None)
    } else if let Some((h, p)) = s.split_once(':') {
        (h.to_string(), Some(parse_port(p)?))
    } else {
        (s.to_string(), None)
    };
    if !valid_host_chars(&host) {
        return Err(LinkError::AddressNotAllowed);
    }
    Ok((host, port))
}

pub fn is_allowed_ip(ip: IpAddr, policy: AddressPolicy) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                return policy.allow_loopback;
            }
            v4.is_private() || v4.is_link_local()
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_allowed_ip(IpAddr::V4(v4), policy);
            }
            if v6.is_loopback() {
                return policy.allow_loopback;
            }
            let first = v6.segments()[0];
            (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

/// Löst die Adresse auf und prüft JEDE aufgelöste IP. Bevorzugt IPv4.
pub fn resolve(input: &str, policy: AddressPolicy) -> Result<Target, LinkError> {
    let (host, port) = split_host_port(input)?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if is_allowed_ip(ip, policy) { Ok(Target { ip, port }) } else { Err(LinkError::AddressNotAllowed) };
    }
    let ips: Vec<IpAddr> = (host.as_str(), 0)
        .to_socket_addrs()
        .map_err(|_| LinkError::Unreachable)?
        .map(|sa| sa.ip())
        .collect();
    if ips.is_empty() {
        return Err(LinkError::Unreachable);
    }
    if !ips.iter().all(|ip| is_allowed_ip(*ip, policy)) {
        return Err(LinkError::AddressNotAllowed);
    }
    let ip = ips.iter().copied().find(IpAddr::is_ipv4).unwrap_or(ips[0]);
    Ok(Target { ip, port })
}

/// Prüfung ohne DNS (für das Wiederherstellen einer Sicherung): Syntax
/// stimmt, und eine wörtliche IP muss im Heimnetz liegen. Namen werden
/// erst beim nächsten Verbindungstest aufgelöst.
pub fn looks_valid(input: &str) -> bool {
    match split_host_port(input) {
        Ok((host, _)) => match host.parse::<IpAddr>() {
            Ok(ip) => is_allowed_ip(ip, AddressPolicy::HOME_NETWORK),
            Err(_) => true,
        },
        Err(_) => false,
    }
}

pub fn base_url(ip: IpAddr, port: u16) -> String {
    let host = match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
    };
    if port == 80 { format!("http://{host}") } else { format!("http://{host}:{port}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn home_network_addresses_are_allowed() {
        for a in ["192.168.1.60", "10.0.0.5", "172.20.1.1", "169.254.1.1", "fd00::1", "fe80::1"] {
            assert!(is_allowed_ip(ip(a), AddressPolicy::HOME_NETWORK), "{a}");
        }
    }

    #[test]
    fn everything_else_is_rejected() {
        for a in ["8.8.8.8", "127.0.0.1", "::1", "0.0.0.0", "224.0.0.1", "255.255.255.255", "2001:db8::1", "172.32.0.1"] {
            assert!(!is_allowed_ip(ip(a), AddressPolicy::HOME_NETWORK), "{a}");
        }
    }

    #[test]
    fn ipv4_mapped_ipv6_uses_the_ipv4_rules() {
        assert!(is_allowed_ip(IpAddr::V6(Ipv4Addr::new(192, 168, 1, 2).to_ipv6_mapped()), AddressPolicy::HOME_NETWORK));
        assert!(!is_allowed_ip(IpAddr::V6(Ipv4Addr::new(8, 8, 8, 8).to_ipv6_mapped()), AddressPolicy::HOME_NETWORK));
    }

    #[test]
    fn loopback_only_with_test_policy() {
        assert!(is_allowed_ip(IpAddr::V4(Ipv4Addr::LOCALHOST), AddressPolicy::TEST));
        assert!(is_allowed_ip(IpAddr::V6(Ipv6Addr::LOCALHOST), AddressPolicy::TEST));
    }

    #[test]
    fn host_and_port_are_split() {
        assert_eq!(split_host_port("192.168.1.60").unwrap(), ("192.168.1.60".into(), None));
        assert_eq!(split_host_port(" 192.168.1.60:7125 ").unwrap(), ("192.168.1.60".into(), Some(7125)));
        assert_eq!(split_host_port("sv08.local").unwrap(), ("sv08.local".into(), None));
        assert_eq!(split_host_port("[fd00::1]:7125").unwrap(), ("fd00::1".into(), Some(7125)));
        assert_eq!(split_host_port("fd00::1").unwrap(), ("fd00::1".into(), None));
    }

    #[test]
    fn malformed_input_is_rejected() {
        for a in ["", "http://192.168.1.60", "192.168.1.60/x", "a b", "user@host", "host:0", "host:70000", "host:abc", "[fd00::1", "ho%st"] {
            assert!(split_host_port(a).is_err(), "{a}");
        }
    }

    #[test]
    fn resolve_checks_literal_ips_without_dns() {
        assert_eq!(
            resolve("192.168.1.60:7125", AddressPolicy::HOME_NETWORK).unwrap(),
            Target { ip: ip("192.168.1.60"), port: Some(7125) }
        );
        assert_eq!(resolve("8.8.8.8", AddressPolicy::HOME_NETWORK), Err(LinkError::AddressNotAllowed));
    }

    #[test]
    fn names_resolving_to_loopback_are_rejected() {
        assert_eq!(resolve("localhost", AddressPolicy::HOME_NETWORK), Err(LinkError::AddressNotAllowed));
    }

    #[test]
    fn base_url_omits_port_80_and_brackets_ipv6() {
        assert_eq!(base_url(ip("192.168.1.60"), 80), "http://192.168.1.60");
        assert_eq!(base_url(ip("192.168.1.60"), 7125), "http://192.168.1.60:7125");
        assert_eq!(base_url(ip("fd00::1"), 7125), "http://[fd00::1]:7125");
    }

    #[test]
    fn looks_valid_is_a_syntax_and_literal_ip_check_only() {
        assert!(looks_valid("192.168.1.60"));
        assert!(looks_valid("sv08.local:7125"));
        assert!(!looks_valid("8.8.8.8"));
        assert!(!looks_valid("http://x"));
    }
}
