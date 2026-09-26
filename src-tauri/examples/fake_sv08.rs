//! Simulated Sovol SV08 for manually testing the printer connection.
//!   cargo run --no-default-features --example fake_sv08 -- 8125
//! Listens on all interfaces. In the app, enter this machine's LAN IP with the
//! port, e.g. 192.168.1.20:8125 (loopback is blocked in the app on purpose).
//! Every call of
//!   curl http://127.0.0.1:8125/fake/new?status=completed
//! adds another finished print (status=cancelled for an aborted one).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

fn main() {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(8125);
    let info = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/moonraker/server_info_v0_8_0_209.json")).unwrap();
    let jobs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind(("0.0.0.0", port)).unwrap();
    println!("Fake-SV08 auf Port {port}. Neuer Druck: curl http://127.0.0.1:{port}/fake/new?status=completed");
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                break;
            }
        }
        let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
        let path = target.split('?').next().unwrap_or("");
        let (status, ctype, body): (u16, &str, Vec<u8>) = match path {
            "/server/info" => (200, "application/json", info.clone().into_bytes()),
            "/fake/new" => {
                let st = if target.contains("status=cancelled") { "cancelled" } else { "completed" };
                let mut j = jobs.lock().unwrap();
                let n = j.len() + 1;
                let used = if st == "completed" { 18464.75 } else { 7200.0 };
                j.push(format!(
                    r#"{{"end_time": {end}, "filament_used": {used}, "filename": "Testdruck_{n}_PLA_0.2_1h42m.gcode", "metadata": {{"filament_type": "PLA", "filament_total": 18440.65, "filament_weight_total": 55.0, "thumbnails": []}}, "print_duration": 6995.0, "status": "{st}", "start_time": {start}, "job_id": "F{n:05}"}}"#,
                    end = now(), start = now() - 7000.0
                ));
                (200, "text/plain", format!("Druck {n} ({st}) angelegt\n").into_bytes())
            }
            "/server/history/list" => {
                let j = jobs.lock().unwrap();
                (200, "application/json", format!(r#"{{"result": {{"count": {}, "jobs": [{}]}}}}"#, j.len(), j.join(",")).into_bytes())
            }
            _ => (404, "application/json", b"{}".to_vec()),
        };
        let head = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: {ctype}\r\nConnection: close\r\n\r\n", body.len());
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&body);
    }
}
