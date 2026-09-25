//! Nur für Tests: winziger HTTP/1.1-Server auf 127.0.0.1, der pro Anfrage
//! eine Handler-Funktion (Pfad inkl. Query → Status, Body) aufruft.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub type Handler = Arc<dyn Fn(&str) -> Option<(u16, Vec<u8>)> + Send + Sync>;

pub struct FakeServer {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeServer {
    /// Handler liefert `None` → Verbindung wird ohne Antwort offen gehalten
    /// (simuliert Zeitüberschreitung).
    pub fn start(handler: impl Fn(&str) -> Option<(u16, Vec<u8>)> + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
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
                            let head = format!(
                                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
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

    /// Adresse im Format der App ("127.0.0.1:PORT").
    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// Liest eine Testdatei aus `tests/fixtures/moonraker/`.
pub fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/moonraker/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Verhält sich wie der SV08 des Testers.
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
