//! The printer API key must not show up in log output or in error texts, neither on the
//! successful connection test nor on the failure paths. Own test binary: the logger is global.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;

use katalog_core::printer_link::address::AddressPolicy;
use katalog_core::printer_link::api_key::ApiKey;
use katalog_core::printer_link::{make_link, LinkError, KIND_MOONRAKER};

const KEY: &str = "dummyKEY0123456789abcdefDUMMYkey";

struct Capture(Mutex<Vec<String>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        self.0.lock().unwrap().push(format!("{} {} {}", r.level(), r.target(), r.args()));
    }
    fn flush(&self) {}
}
static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

/// Answers every request; the body always echoes the key it received (a hostile or sloppy printer).
fn server(status: u16, fixture: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut seen = Vec::new();
            let mut buf = [0u8; 2048];
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => seen.extend_from_slice(&buf[..n]),
                }
            }
            let body = if fixture {
                std::fs::read(format!("{}/tests/fixtures/moonraker/server_info_v0_8_0_209.json", env!("CARGO_MANIFEST_DIR"))).unwrap()
            } else {
                format!(r#"{{"error":"bad key {KEY}"}}"#).into_bytes()
            };
            let head = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    address
}

fn policy() -> AddressPolicy {
    let mut p = AddressPolicy::HOME_NETWORK;
    p.allow_loopback = true; // fake printer on 127.0.0.1
    p
}

fn test_with_key(address: &str) -> Result<String, LinkError> {
    let key = ApiKey::parse(KEY).unwrap();
    let link = make_link(KIND_MOONRAKER, address, Some(&key), policy())?;
    link.test().map(|info| format!("{info:?}"))
}

#[test]
fn api_key_is_in_no_log_line_and_no_error_text() {
    log::set_logger(&CAPTURE).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    let mut texts = Vec::new();

    // Success: the request carries the key (header), the answer echoes nothing of it.
    let ok = test_with_key(&server(200, true)).expect("connection test works");
    texts.push(ok);
    // Failure paths: wrong key (401, 403), server error echoing the key, closed port, URL with the key as credentials.
    for status in [401u16, 403, 500] {
        let err = test_with_key(&server(status, false)).unwrap_err();
        texts.push(format!("{err:?} {}", err.code()));
    }
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().to_string()
    };
    let refused = test_with_key(&closed).unwrap_err();
    texts.push(format!("{refused:?}"));
    let hostile = test_with_key(&format!("http://{KEY}@127.0.0.1")).unwrap_err();
    texts.push(format!("{hostile:?}"));

    let logs = CAPTURE.0.lock().unwrap().join("\n");
    assert!(logs.contains("drucker"), "the connection test must log something, or this test proves nothing");
    assert!(!logs.contains(KEY), "key in log output:\n{logs}");
    for t in &texts {
        assert!(!t.contains(KEY), "key in text: {t}");
    }
}
