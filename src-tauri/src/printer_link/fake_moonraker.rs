//! Tests only: tiny HTTP/1.1 server (loopback by default).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub type Handler = Arc<dyn Fn(&str) -> Option<(u16, Vec<u8>)> + Send + Sync>;

pub struct FakeServer {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeServer {
    /// Handler returns `None` -> the connection is kept open without a response (simulates a timeout).
    pub fn start(handler: impl Fn(&str) -> Option<(u16, Vec<u8>)> + Send + Sync + 'static) -> Self {
        Self::start_with_clock(None, handler)
    }

    /// Like `start`, but every answer carries a `Date` header from a clock that is
    /// `offset_s` seconds off the local one (a printer without NTP).
    pub fn start_with_clock(
        offset_s: Option<f64>,
        handler: impl Fn(&str) -> Option<(u16, Vec<u8>)> + Send + Sync + 'static,
    ) -> Self {
        Self::start_on("127.0.0.1:0", offset_s, handler)
    }

    fn start_on(
        address: &str,
        offset_s: Option<f64>,
        handler: impl Fn(&str) -> Option<(u16, Vec<u8>)> + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind(address).unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let log = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let handler = handler.clone();
                let log = log.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        return;
                    }
                    let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                            break;
                        }
                    }
                    log.lock().unwrap().push(target.clone());
                    match handler(&target) {
                        Some((status, body)) => {
                            let date = offset_s
                                .and_then(|o| {
                                    let secs = (super::sync::unix_now() + o) as i64;
                                    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
                                })
                                .map(|d| format!("Date: {}\r\n", d.format("%a, %d %b %Y %H:%M:%S GMT")))
                                .unwrap_or_default();
                            let head = format!(
                                "HTTP/1.1 {status} X\r\n{date}Content-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(&body);
                        }
                        None => std::thread::sleep(std::time::Duration::from_secs(8)),
                    }
                });
            }
        });
        FakeServer { port, requests }
    }

    /// Address in the app's format ("127.0.0.1:PORT").
    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// Reads a test file from `tests/fixtures/moonraker/`.
pub fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/moonraker/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Behaves like the tester's SV08.
pub fn sv08() -> FakeServer {
    FakeServer::start(|target| {
        let path = target.split('?').next().unwrap_or("");
        match path {
            "/server/info" => Some((200, fixture_bytes("server_info_v0_8_0_209.json"))),
            "/server/history/list" => Some((200, fixture_bytes("history_completed.json"))),
            p if p.starts_with("/server/files/gcodes/") => Some((200, b"\x89PNG\r\n\x1a\nfake".to_vec())),
            _ => Some((404, b"{}".to_vec())),
        }
    })
}

/// Behaves like the Qidi Smart 3 from the test report of 2026-09-26: Moonraker
/// v0.7.1 and a clock that is `offset_s` seconds off.
pub fn qidi_smart3(offset_s: f64) -> FakeServer {
    FakeServer::start_with_clock(Some(offset_s), |target| {
        let path = target.split('?').next().unwrap_or("");
        match path {
            "/server/info" => Some((200, fixture_bytes("server_info_qidi_smart3.json"))),
            "/server/history/list" => Some((200, fixture_bytes("history_qidi_smart3.json"))),
            _ => Some((404, b"{}".to_vec())),
        }
    })
}

/// Explicit opt-in: exposes only fixture data inside the Compose test network.
#[test]
#[ignore = "long-running server for docker/tests/printer/compose.yaml"]
fn serve_container_fixture() {
    assert_eq!(std::env::var("PRINTER_FIXTURE_SERVER").as_deref(), Ok("1"));
    let _server = FakeServer::start_on("0.0.0.0:7125", Some(3600.0), |target| {
        match target.split('?').next().unwrap_or("") {
            "/server/info" => Some((200, fixture_bytes("server_info_qidi_smart3.json"))),
            "/server/history/list" => Some((200, fixture_bytes("history_qidi_smart3.json"))),
            _ => Some((404, b"{}".to_vec())),
        }
    });
    println!("Fake Moonraker listening on port 7125; clock offset +3600 seconds");
    loop {
        std::thread::park();
    }
}

#[test]
#[ignore = "requires the Compose fixture server; never contacts a real printer"]
fn container_network_smoke() {
    use super::{address::AddressPolicy, moonraker::MoonrakerLink, PrinterLink};
    assert_eq!(std::env::var("PRINTER_FIXTURE_CLIENT").as_deref(), Ok("1"));
    // Exercise Docker DNS and the production policy, never the loopback test policy.
    let link = MoonrakerLink::new("moonraker:7125", AddressPolicy::HOME_NETWORK);
    let mut result = link.test();
    for _ in 0..30 {
        if result.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        result = link.test();
    }
    let info = result.expect("fixture must be reachable through the Compose network");
    assert!(info.base_url.starts_with("http://"));
    assert!((info.clock_offset_s - 3600.0).abs() < 3.0);
    let jobs = link.jobs_ended_since(&info, 0.0).unwrap();
    assert!(!jobs.is_empty());
    assert!(jobs.iter().any(|job| (job.ended_at - (1_702_311_947.72 - info.clock_offset_s)).abs() < 0.01));
    assert_eq!(super::address::resolve("127.0.0.1", AddressPolicy::HOME_NETWORK), Err(super::LinkError::AddressNotAllowed));
    println!("Docker DNS, private IP policy, Moonraker history and clock correction passed");
}
