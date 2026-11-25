//! Home network addresses only. The check runs again on every connection, and
//! the connection goes to the checked IP (no second DNS lookup).

use std::net::{IpAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use super::LinkError;

/// An optional allowlist narrows the home-network policy; it never replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressPolicy {
    pub allow_loopback: bool,
    /// Blocks the whole link-local range (169.254.0.0/16, fe80::/10), also for allowlist entries.
    /// Off by default (the desktop app keeps accepting directly cabled printers); the server turns it on.
    block_link_local: bool,
    /// A port of the service itself; printer addresses must not point at it.
    denied_port: Option<u16>,
    allowlist: Option<Arc<AddressAllowlist>>,
}

impl AddressPolicy {
    pub const HOME_NETWORK: Self = Self {
        allow_loopback: false,
        block_link_local: false,
        denied_port: None,
        allowlist: None,
    };
    #[cfg(test)]
    pub const TEST: Self = Self {
        allow_loopback: true,
        block_link_local: false,
        denied_port: None,
        allowlist: None,
    };

    pub fn with_allowlist(mut self, allowlist: AddressAllowlist) -> Self {
        self.allowlist = Some(Arc::new(allowlist));
        self
    }

    /// Server hardening: the whole link-local range is refused, even with a covering allowlist entry.
    pub fn blocking_link_local(mut self) -> Self {
        self.block_link_local = true;
        self
    }

    /// Server hardening: refuses an explicit or default printer port that is the service's own port.
    pub fn denying_port(mut self, port: u16) -> Self {
        self.denied_port = Some(port);
        self
    }

    pub fn denies_port(&self, port: u16) -> bool {
        self.denied_port == Some(port)
    }
}

/// IP networks and pinned host addresses. An empty list permits no addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressAllowlist {
    networks: Vec<IpNetwork>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IpNetwork {
    address: IpAddr,
    prefix: u8,
}

fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        _ => ip,
    }
}

impl IpNetwork {
    fn new(address: IpAddr, prefix: u8) -> Result<Self, LinkError> {
        let max = if address.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(LinkError::AddressNotAllowed);
        }
        Ok(Self { address, prefix })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match self.address {
            IpAddr::V4(network) => {
                let IpAddr::V4(ip) = canonical_ip(ip) else { return false };
                let mask = u32::MAX.checked_shl(u32::from(32 - self.prefix)).unwrap_or(0);
                u32::from(network) & mask == u32::from(ip) & mask
            }
            IpAddr::V6(network) => {
                // IPv4 addresses use their mapped representation in IPv6 networks.
                let ip = match ip {
                    IpAddr::V4(ip) => ip.to_ipv6_mapped(),
                    IpAddr::V6(ip) => ip,
                };
                let mask = u128::MAX.checked_shl(u32::from(128 - self.prefix)).unwrap_or(0);
                u128::from(network) & mask == u128::from(ip) & mask
            }
        }
    }
}

impl AddressAllowlist {
    /// Parses individual host, IP or CIDR strings (no URLs, ports or wildcards).
    ///
    /// Hosts are resolved once here and pinned to their current IPs. Rebuild the
    /// list explicitly to accept changed host addresses. Resolution is blocking.
    /// Invalid entries reject the entire list with AddressNotAllowed; unresolved
    /// or empty host answers return NameResolutionFailed. Public and metadata
    /// addresses can be parsed but are never enabled by the resulting policy.
    /// IPv4 addresses match IPv6 networks via their IPv4-mapped representation.
    pub fn from_strings<I, S>(entries: I) -> Result<Self, LinkError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        // All host lookups together get one budget, so a hanging system resolver cannot hold the caller for long.
        let end = Instant::now() + ALLOWLIST_DNS_BUDGET;
        Self::from_strings_with_resolver(entries, |host| lookup_ips(host, end.saturating_duration_since(Instant::now()).min(DNS_TIMEOUT)))
    }

    fn from_strings_with_resolver<I, S>(entries: I, mut resolver: impl FnMut(&str) -> Result<Vec<IpAddr>, LinkError>) -> Result<Self, LinkError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut networks = Vec::new();
        for entry in entries {
            let entry = entry.as_ref().trim();
            if let Some((address, prefix)) = entry.split_once('/') {
                let address = address.parse::<IpAddr>().map_err(|_| LinkError::AddressNotAllowed)?;
                if prefix.is_empty() || !prefix.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(LinkError::AddressNotAllowed);
                }
                let prefix = prefix.parse::<u8>().map_err(|_| LinkError::AddressNotAllowed)?;
                networks.push(IpNetwork::new(address, prefix)?);
            } else {
                let (host, port) = split_host_port(entry)?;
                if port.is_some() {
                    return Err(LinkError::AddressNotAllowed);
                }
                let ips = if let Ok(ip) = host.parse::<IpAddr>() {
                    vec![ip]
                } else {
                    let host = host.strip_suffix('.').unwrap_or(&host).to_ascii_lowercase();
                    if !valid_allowlist_host(&host) || entry.starts_with('[') {
                        return Err(LinkError::AddressNotAllowed);
                    }
                    resolver(&host)?
                };
                if ips.is_empty() {
                    return Err(LinkError::NameResolutionFailed);
                }
                for ip in ips {
                    networks.push(IpNetwork::new(ip, if ip.is_ipv4() { 32 } else { 128 })?);
                }
            }
        }
        Ok(Self { networks })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        self.networks.iter().any(|network| network.contains(ip))
    }
}

fn valid_allowlist_host(host: &str) -> bool {
    host.len() <= 253
        && !host.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Longest wait for one name lookup, and for all host lookups of one allowlist together.
const DNS_TIMEOUT: Duration = Duration::from_secs(5);
const ALLOWLIST_DNS_BUDGET: Duration = Duration::from_secs(15);
/// The system resolver cannot be interrupted, so a hanging lookup keeps its thread until it returns.
/// At most this many such threads exist at once; further lookups fail fast instead of adding more.
const MAX_LOOKUP_THREADS: usize = 8;

static LOOKUPS: LookupGate = LookupGate { running: AtomicUsize::new(0), max: MAX_LOOKUP_THREADS };

struct LookupGate {
    running: AtomicUsize,
    max: usize,
}

struct LookupSlot<'a>(&'a LookupGate);

impl Drop for LookupSlot<'_> {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::SeqCst);
    }
}

impl LookupGate {
    fn enter(&self) -> Option<LookupSlot<'_>> {
        self.running
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| (n < self.max).then_some(n + 1))
            .ok()
            .map(|_| LookupSlot(self))
    }
}

/// Runs `lookup` on its own thread and waits at most `limit`. The slot is held by the thread until the
/// lookup really returns, so hanging lookups count against the cap. No slot or no time: `Timeout`.
fn bounded_lookup(
    gate: &'static LookupGate,
    host: &str,
    limit: Duration,
    lookup: impl FnOnce(&str) -> Result<Vec<IpAddr>, LinkError> + Send + 'static,
) -> Result<Vec<IpAddr>, LinkError> {
    if limit.is_zero() {
        return Err(LinkError::Timeout);
    }
    let slot = gate.enter().ok_or(LinkError::Timeout)?;
    let (tx, rx) = mpsc::channel();
    let host = host.to_string();
    // The slot moves into the thread and is freed when the lookup returns (or if the thread cannot start).
    std::thread::Builder::new()
        .name("printer-dns".into())
        .spawn(move || {
            let _slot = slot;
            let _ = tx.send(lookup(&host));
        })
        .map_err(|_| LinkError::Timeout)?;
    rx.recv_timeout(limit).unwrap_or(Err(LinkError::Timeout))
}

fn system_lookup(host: &str) -> Result<Vec<IpAddr>, LinkError> {
    (host, 0)
        .to_socket_addrs()
        .map(|addresses| addresses.map(|sa| sa.ip()).collect())
        .map_err(|_| LinkError::NameResolutionFailed)
}

fn lookup_ips(host: &str, limit: Duration) -> Result<Vec<IpAddr>, LinkError> {
    bounded_lookup(&LOOKUPS, host, limit, system_lookup)
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
    // Digits only ("+80" would parse as a number) and 1..=65535.
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LinkError::AddressNotAllowed);
    }
    match s.parse::<u16>() {
        Ok(p) if p > 0 => Ok(p),
        _ => Err(LinkError::AddressNotAllowed),
    }
}

/// Splits "host", "host:port", "[ipv6]:port" or a bare IPv6 address.
/// No scheme, no path, no credentials.
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

/// Container-specific guidance without a DNS lookup or a change to IP policy.
pub fn check_container_address(input: &str, container: bool) -> Result<(), LinkError> {
    let (host, _) = split_host_port(input)?;
    if container && host.trim_end_matches('.').to_ascii_lowercase().ends_with(".local") {
        return Err(LinkError::ContainerLocalName);
    }
    Ok(())
}

pub fn explain_address_error(input: &str, container: bool, error: LinkError) -> LinkError {
    if container && error == LinkError::AddressNotAllowed {
        if let Ok((host, _)) = split_host_port(input) {
            let loopback = host.trim_end_matches('.').eq_ignore_ascii_case("localhost")
                || host.parse::<IpAddr>().is_ok_and(|ip| match ip {
                    IpAddr::V4(ip) => ip.is_loopback(),
                    IpAddr::V6(ip) => ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback()),
                });
            if loopback {
                return LinkError::ContainerLoopback;
            }
        }
    }
    error
}

pub fn is_allowed_ip(ip: IpAddr, policy: AddressPolicy) -> bool {
    policy_allows_ip(ip, &policy)
}

fn policy_allows_ip(ip: IpAddr, policy: &AddressPolicy) -> bool {
    let ip = canonical_ip(ip);
    let home_network = match ip {
        IpAddr::V4(v4) => {
            if v4.octets() == [169, 254, 169, 254] {
                return false;
            }
            if policy.block_link_local && v4.is_link_local() {
                return false;
            }
            if v4.is_loopback() {
                policy.allow_loopback
            } else {
                v4.is_private() || v4.is_link_local()
            }
        }
        IpAddr::V6(v6) => {
            if v6.segments() == [0xfd00, 0xec2, 0, 0, 0, 0, 0, 0x254] {
                return false;
            }
            if policy.block_link_local && (v6.segments()[0] & 0xffc0) == 0xfe80 {
                return false;
            }
            if v6.is_loopback() {
                policy.allow_loopback
            } else {
                let first = v6.segments()[0];
                (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
            }
        }
    };
    home_network && policy.allowlist.as_ref().is_none_or(|list| list.contains(ip))
}

/// Resolves the address and checks EVERY resolved IP. Prefers IPv4.
pub fn resolve(input: &str, policy: AddressPolicy) -> Result<Target, LinkError> {
    resolve_within(input, policy, DNS_TIMEOUT)
}

/// Like `resolve`, but a name lookup waits at most `limit` (callers pass the time left in their budget).
pub fn resolve_within(input: &str, policy: AddressPolicy, limit: Duration) -> Result<Target, LinkError> {
    resolve_with_resolver(input, policy, |host| lookup_ips(host, limit.min(DNS_TIMEOUT)))
}

fn resolve_with_resolver(input: &str, policy: AddressPolicy, resolver: impl FnOnce(&str) -> Result<Vec<IpAddr>, LinkError>) -> Result<Target, LinkError> {
    let (host, port) = split_host_port(input)?;
    if port.is_some_and(|p| policy.denies_port(p)) {
        return Err(LinkError::AddressNotAllowed);
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if policy_allows_ip(ip, &policy) {
            Ok(Target { ip, port })
        } else {
            Err(LinkError::AddressNotAllowed)
        };
    }
    let ips = resolver(&host)?;
    if ips.is_empty() {
        return Err(LinkError::NameResolutionFailed);
    }
    if !ips.iter().all(|ip| policy_allows_ip(*ip, &policy)) {
        return Err(LinkError::AddressNotAllowed);
    }
    let ip = ips.iter().copied().find(IpAddr::is_ipv4).unwrap_or(ips[0]);
    Ok(Target { ip, port })
}

/// Check without DNS (for restoring a backup): the syntax is valid and a literal
/// IP must be on the home network. Names are only resolved at the next
/// connection test.
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

    fn policy(entries: &[&str]) -> AddressPolicy {
        AddressPolicy::HOME_NETWORK.with_allowlist(AddressAllowlist::from_strings(entries).unwrap())
    }

    #[test]
    fn allowlist_requires_both_membership_and_home_network() {
        let p = policy(&["192.168.1.0/24", "8.8.8.8"]);
        assert!(is_allowed_ip(ip("192.168.1.60"), p.clone()));
        for address in ["192.168.2.60", "8.8.8.8", "127.0.0.1"] {
            assert!(!is_allowed_ip(ip(address), p.clone()), "{address}");
        }
        assert!(!is_allowed_ip(ip("192.168.1.60"), policy(&[])));
    }

    #[test]
    fn allowlist_loopback_requires_test_policy_and_membership() {
        let list = AddressAllowlist::from_strings(["127.0.0.1", "::1"]).unwrap();
        for address in ["127.0.0.1", "::1", "::ffff:127.0.0.1"] {
            assert!(!is_allowed_ip(ip(address), AddressPolicy::HOME_NETWORK.with_allowlist(list.clone())));
            assert!(is_allowed_ip(ip(address), AddressPolicy::TEST.with_allowlist(list.clone())));
        }
        assert!(!is_allowed_ip(ip("127.0.0.2"), AddressPolicy::TEST.with_allowlist(list)));
    }

    #[test]
    fn metadata_is_denied_even_with_covering_cidr_or_test_policy() {
        for address in ["169.254.169.254", "::ffff:169.254.169.254", "fd00:ec2::254"] {
            for p in [
                AddressPolicy::HOME_NETWORK,
                AddressPolicy::TEST,
                policy(&["169.254.0.0/16", "fd00::/8"]),
                AddressPolicy::TEST.with_allowlist(AddressAllowlist::from_strings(["0.0.0.0/0", "::/0"]).unwrap()),
            ] {
                assert!(!is_allowed_ip(ip(address), p), "{address}");
            }
        }
    }

    #[test]
    fn allowlist_ipv6_and_mapped_ipv4_match_network_boundaries() {
        let p = policy(&["fd12:3456::1234/64", "192.168.1.60/32"]);
        for address in ["fd12:3456::1", "fd12:3456::ffff", "::ffff:192.168.1.60"] {
            assert!(is_allowed_ip(ip(address), p.clone()), "{address}");
        }
        for address in ["fd12:3456:0:1::1", "fd13::1", "2001:db8::1", "192.168.1.61"] {
            assert!(!is_allowed_ip(ip(address), p.clone()), "{address}");
        }
        let mapped = policy(&["::ffff:192.168.1.0/120"]);
        assert!(is_allowed_ip(ip("192.168.1.60"), mapped.clone()));
        assert!(!is_allowed_ip(ip("192.168.2.60"), mapped));
        let exact = policy(&["fd12::1/128"]);
        assert!(is_allowed_ip(ip("fd12::1"), exact.clone()));
        assert!(!is_allowed_ip(ip("fd12::2"), exact));
    }

    #[test]
    fn allowlist_cidr_zero_and_non_octet_prefixes() {
        let all = policy(&["0.0.0.0/0", "::/0"]);
        for address in ["10.0.0.1", "192.168.1.1", "fd12::1", "fe80::1"] {
            assert!(is_allowed_ip(ip(address), all.clone()), "{address}");
        }
        for address in ["8.8.8.8", "2001:db8::1"] {
            assert!(!is_allowed_ip(ip(address), all.clone()), "{address}");
        }
        let mapped = policy(&["::ffff:192.168.1.1/80"]);
        assert!(is_allowed_ip(ip("10.0.0.1"), mapped.clone()));
        assert!(!is_allowed_ip(ip("fd12::1"), mapped));
        let p = policy(&["192.168.1.129/25", "fd12:3456:789a:8000::1/49"]);
        for address in ["192.168.1.128", "192.168.1.255", "fd12:3456:789a:8000::", "fd12:3456:789a:ffff::1"] {
            assert!(is_allowed_ip(ip(address), p.clone()), "{address}");
        }
        for address in ["192.168.1.127", "192.168.2.0", "fd12:3456:789a:7fff::1", "fd12:3456:789b:8000::1"] {
            assert!(!is_allowed_ip(ip(address), p.clone()), "{address}");
        }
    }

    #[test]
    fn allowlist_parser_rejects_invalid_entries() {
        for entry in [
            "",
            " ",
            "host:7125",
            "http://host",
            "user@host",
            "*.local",
            "host/24",
            "192.168.1.1/33",
            "::1/129",
            "10.0.0.1/-1",
            "10.0.0.1/1/2",
            "bad..host",
            "-host",
            "host-",
            "999.999.1.1",
            "[fd00::1]:80",
        ] {
            assert_eq!(AddressAllowlist::from_strings([entry]), Err(LinkError::AddressNotAllowed), "{entry}");
        }
    }

    #[test]
    fn allowlist_hosts_are_pinned_at_construction() {
        let list = AddressAllowlist::from_strings_with_resolver(["Printer.LOCAL."], |host| {
            assert_eq!(host, "printer.local");
            Ok(vec![ip("192.168.1.60"), ip("fd12::1")])
        })
        .unwrap();
        let p = AddressPolicy::HOME_NETWORK.with_allowlist(list);
        assert!(is_allowed_ip(ip("192.168.1.60"), p.clone()));
        assert!(is_allowed_ip(ip("fd12::1"), p.clone()));
        assert!(!is_allowed_ip(ip("192.168.1.61"), p));
    }

    #[test]
    fn blocked_lookup_times_out_and_frees_its_slot() {
        use std::sync::{mpsc, Mutex};
        static GATE: LookupGate = LookupGate { running: AtomicUsize::new(0), max: 2 };
        let (release, wait) = mpsc::channel::<()>();
        let wait = Arc::new(Mutex::new(wait));
        let started = Instant::now();
        // Two lookups hang: both time out near the limit and keep their slots.
        for _ in 0..2 {
            let wait = wait.clone();
            let r = bounded_lookup(&GATE, "blocked.invalid", Duration::from_millis(100), move |_| {
                let _ = wait.lock().unwrap().recv();
                Ok(vec![])
            });
            assert_eq!(r, Err(LinkError::Timeout));
        }
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(GATE.running.load(Ordering::SeqCst), 2);
        // Cap reached: the next lookup fails at once and starts no thread.
        let t = Instant::now();
        assert_eq!(bounded_lookup(&GATE, "x.invalid", Duration::from_secs(5), |_| Ok(vec![])), Err(LinkError::Timeout));
        assert!(t.elapsed() < Duration::from_millis(500));
        // Released lookups end their threads; the slots return and lookups work again.
        drop(release);
        let deadline = Instant::now() + Duration::from_secs(5);
        while GATE.running.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(GATE.running.load(Ordering::SeqCst), 0);
        assert_eq!(bounded_lookup(&GATE, "ok.invalid", Duration::from_secs(5), |_| Ok(vec![ip("192.168.1.2")])), Ok(vec![ip("192.168.1.2")]));
        assert_eq!(bounded_lookup(&GATE, "ok.invalid", Duration::ZERO, |_| Ok(vec![])), Err(LinkError::Timeout));
    }

    #[test]
    fn allowlist_host_resolution_fails_closed() {
        assert_eq!(
            AddressAllowlist::from_strings_with_resolver(["printer.local"], |_| Ok(vec![])),
            Err(LinkError::NameResolutionFailed)
        );
        assert_eq!(
            AddressAllowlist::from_strings_with_resolver(["printer.local"], |_| Err(LinkError::NameResolutionFailed)),
            Err(LinkError::NameResolutionFailed)
        );
    }

    #[test]
    fn resolver_rechecks_rebinding_and_returns_checked_ip() {
        let mut calls = 0;
        let mut resolver = |_: &str| {
            calls += 1;
            Ok(vec![if calls == 1 { ip("192.168.1.60") } else { ip("127.0.0.1") }])
        };
        let p = policy(&["192.168.1.0/24", "127.0.0.0/8"]);
        let target = resolve_with_resolver("printer.local:7125", p.clone(), &mut resolver).unwrap();
        assert_eq!(
            target,
            Target {
                ip: ip("192.168.1.60"),
                port: Some(7125)
            }
        );
        assert_eq!(base_url(target.ip, target.port.unwrap()), "http://192.168.1.60:7125");
        assert_eq!(resolve_with_resolver("printer.local:7125", p, &mut resolver), Err(LinkError::AddressNotAllowed));
        assert_eq!(calls, 2);
    }

    #[test]
    fn resolver_rejects_any_forbidden_answer_and_prefers_ipv4() {
        let p = policy(&["192.168.1.60", "fd12::1"]);
        for forbidden in ["192.168.1.61", "8.8.8.8", "127.0.0.1", "169.254.169.254", "fd00:ec2::254"] {
            for answers in [vec![ip("192.168.1.60"), ip(forbidden)], vec![ip(forbidden), ip("192.168.1.60")]] {
                assert_eq!(
                    resolve_with_resolver("printer.local", p.clone(), |_| Ok(answers)),
                    Err(LinkError::AddressNotAllowed)
                );
            }
        }
        assert_eq!(
            resolve_with_resolver("printer.local", p, |_| Ok(vec![ip("fd12::1"), ip("192.168.1.60")]))
                .unwrap()
                .ip,
            ip("192.168.1.60")
        );
    }

    #[test]
    fn allowlist_literal_resolution_does_not_use_dns() {
        assert_eq!(
            resolve_with_resolver("192.168.1.60", policy(&["192.168.1.60"]), |_| panic!("literal must not resolve"))
                .unwrap()
                .ip,
            ip("192.168.1.60")
        );
        assert_eq!(resolve("192.168.1.61", policy(&["192.168.1.60"])), Err(LinkError::AddressNotAllowed));
    }

    #[test]
    fn dns_failure_has_its_own_code_without_weakening_address_errors() {
        // A resolver that hijacks unknown names answers with a public address, which the policy refuses.
        assert!(matches!(
            resolve("printer.invalid", AddressPolicy::HOME_NETWORK),
            Err(LinkError::NameResolutionFailed | LinkError::AddressNotAllowed)
        ));
        assert_eq!(LinkError::NameResolutionFailed.code(), "name_resolution_failed");
        assert_eq!(resolve("8.8.8.8", AddressPolicy::HOME_NETWORK), Err(LinkError::AddressNotAllowed));
        assert_eq!(resolve("http://printer.invalid", AddressPolicy::HOME_NETWORK), Err(LinkError::AddressNotAllowed));
        assert_eq!(LinkError::Unreachable.code(), "unreachable");
        assert_eq!(LinkError::AuthRequired.code(), "auth_required");
        assert_eq!(LinkError::AuthRejected.code(), "auth_rejected");
        assert_eq!(LinkError::BadResponse("x".into()).code(), "bad_response");
        assert_eq!(LinkError::HistoryMissing.code(), "history_missing");
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

    #[test]
    fn whole_link_local_range_is_blocked_only_when_asked() {
        let addrs = ["169.254.0.1", "169.254.1.1", "169.254.255.254", "::ffff:169.254.1.1", "fe80::1", "febf::1"];
        let strict = AddressPolicy::HOME_NETWORK.blocking_link_local();
        let covering = strict.clone().with_allowlist(AddressAllowlist::from_strings(["169.254.0.0/16", "fe80::/10", "::/0", "0.0.0.0/0"]).unwrap());
        for a in addrs {
            assert!(!is_allowed_ip(ip(a), strict.clone()), "{a}");
            assert!(!is_allowed_ip(ip(a), covering.clone()), "{a} with allowlist");
            // Default (desktop) behavior is unchanged.
            assert!(is_allowed_ip(ip(a), AddressPolicy::HOME_NETWORK), "{a} default");
        }
        // Neighbours of the range and normal LAN addresses stay allowed.
        for a in ["169.253.255.255", "169.255.0.1"] {
            assert!(!is_allowed_ip(ip(a), strict.clone()), "{a} is public");
        }
        for a in ["192.168.1.60", "10.0.0.5", "fd00::1", "::ffff:192.168.1.60", "fec0::1"] {
            let expected = a != "fec0::1";
            assert_eq!(is_allowed_ip(ip(a), strict.clone()), expected, "{a}");
        }
        assert_eq!(resolve("169.254.10.10", strict), Err(LinkError::AddressNotAllowed));
    }

    #[test]
    fn own_service_port_is_refused() {
        let p = AddressPolicy::HOME_NETWORK.denying_port(8080);
        assert_eq!(resolve("192.168.1.60:8080", p.clone()), Err(LinkError::AddressNotAllowed));
        assert_eq!(resolve("[fd00::1]:8080", p.clone()), Err(LinkError::AddressNotAllowed));
        assert_eq!(resolve("192.168.1.60:7125", p.clone()).unwrap().port, Some(7125));
        assert_eq!(resolve("192.168.1.60", p.clone()).unwrap().port, None);
        assert!(p.denies_port(8080) && !p.denies_port(7125));
        // Without the setting every valid port stays possible.
        assert_eq!(resolve("192.168.1.60:8080", AddressPolicy::HOME_NETWORK).unwrap().port, Some(8080));
    }

    #[test]
    fn port_range_is_one_to_65535() {
        for bad in ["host:0", "host:00", "host:65536", "host:-1", "host:+80", "host:", "[fd00::1]:0", "[fd00::1]:65536", "host:8 0"] {
            assert!(split_host_port(bad).is_err(), "{bad}");
        }
        assert_eq!(split_host_port("host:1").unwrap().1, Some(1));
        assert_eq!(split_host_port("host:65535").unwrap().1, Some(65535));
    }
}
