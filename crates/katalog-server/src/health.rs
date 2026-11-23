//! `katalog-server health`: asks the server's own `/api/health` and reports the result as an
//! exit code. It exists for container health checks, where the distroless image has neither a
//! shell nor curl. Plain std networking keeps it free of extra dependencies.
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(4);
const DEFAULT_BIND: &str = "127.0.0.1:8080";

/// Where to connect and which `Host` header to send. The server only answers hosts on its
/// allow list, so the first allowed host is used; without a list the connect address itself.
pub fn target(lookup: impl Fn(&str) -> Option<String>) -> Result<(SocketAddr, String), String> {
    let bind: SocketAddr = lookup("KATALOG_BIND")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_BIND.into())
        .parse()
        .map_err(|_| "KATALOG_BIND is not a valid IP:port".to_string())?;
    // A wildcard bind address cannot be connected to; the loopback of the same family reaches it.
    let ip = match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    let addr = SocketAddr::new(ip, bind.port());
    let host = lookup("KATALOG_ALLOWED_HOSTS")
        .and_then(|v| v.split(',').map(str::trim).find(|h| !h.is_empty()).map(str::to_owned))
        .unwrap_or_else(|| addr.to_string());
    Ok((addr, host))
}

/// One GET against `/api/health`; `Ok` only for status 200.
pub fn probe(addr: SocketAddr, host: &str) -> Result<(), String> {
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|e| format!("no connection to {addr}: {e}"))?;
    stream.set_read_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
    // The forwarded-for header is only read behind a trusted proxy, where it is mandatory.
    let request = format!(
        "GET /api/health HTTP/1.1\r\nHost: {host}\r\nX-Forwarded-For: 127.0.0.1\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut head = Vec::new();
    // Only the status line matters; stop after the first chunk that contains it.
    let mut buf = [0u8; 256];
    while !head.contains(&b'\n') && head.len() < 1024 {
        let n = stream.read(&mut buf).map_err(|e| format!("no answer from {addr}: {e}"))?;
        if n == 0 {
            break;
        }
        head.extend_from_slice(&buf[..n]);
    }
    let line = String::from_utf8_lossy(&head);
    let line = line.lines().next().unwrap_or("");
    let status = line.split_whitespace().nth(1).unwrap_or("");
    if status == "200" {
        Ok(())
    } else {
        Err(format!("unexpected answer: {line:?}"))
    }
}

pub fn run(lookup: impl Fn(&str) -> Option<String>) -> Result<(), String> {
    let (addr, host) = target(lookup)?;
    probe(addr, &host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(answer: &'static str) -> (SocketAddr, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let n = s.read(&mut buf).unwrap();
            s.write_all(answer.as_bytes()).unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (addr, handle)
    }

    #[test]
    fn running_server_with_200_is_healthy_and_gets_host_header() {
        let (addr, handle) = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 11\r\n\r\n{\"ok\":true}");
        probe(addr, "katalog.example:8080").unwrap();
        let request = handle.join().unwrap();
        assert!(request.starts_with("GET /api/health HTTP/1.1\r\n"));
        assert!(request.contains("Host: katalog.example:8080\r\n"));
    }

    #[test]
    fn server_answering_an_error_status_is_unhealthy() {
        let (addr, handle) = serve_once("HTTP/1.1 503 Service Unavailable\r\n\r\n");
        assert!(probe(addr, "x").is_err());
        handle.join().unwrap();
    }

    #[test]
    fn stopped_server_is_unhealthy() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        assert!(probe(addr, "x").is_err());
    }

    #[test]
    fn target_uses_wildcard_as_loopback_and_first_allowed_host() {
        let env = |k: &str| match k {
            "KATALOG_BIND" => Some("0.0.0.0:8080".to_string()),
            "KATALOG_ALLOWED_HOSTS" => Some(" katalog.home:8080 ,127.0.0.1:8080".to_string()),
            _ => None,
        };
        let (addr, host) = target(env).unwrap();
        assert_eq!(addr, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(host, "katalog.home:8080");
        let (addr, host) = target(|_| None).unwrap();
        assert_eq!((addr, host.as_str()), ("127.0.0.1:8080".parse().unwrap(), "127.0.0.1:8080"));
        assert!(target(|k| (k == "KATALOG_BIND").then(|| "nonsense".into())).is_err());
    }
}
