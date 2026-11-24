//! Web printer link against a virtual Klipper/Moonraker printer (Forgejo #42, C6).
//!
//! Needs a running virtual printer (`docker/tests/virtual-klipper`) on a private test network,
//! so the tests are `#[ignore]`d. `docker/tests/virtual-klipper/run.sh` starts the printers and runs them:
//!   VK_PRINTER=172.29.252.10:7125 VK_REDIRECT=172.29.252.11:7125 VK_KEY=<key of VK_PRINTER> VK_ALLOW=172.29.252.0/24
//!   cargo test -p katalog-server --test virtual_klipper -- --ignored --test-threads=1
//! No home-network device is touched: the only reachable printer is the throw-away container.
use axum::{
    body::{to_bytes, Body},
    http::Request,
    response::Response,
    Router,
};
use katalog_server::{
    config::Config,
    password::{self, AuthFile},
    AppConfig, Clock,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tower::ServiceExt;

const PASSWORD: &str = "virtual-klipper-test-password";

struct Now;
impl Clock for Now {
    fn now(&self) -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set (see the header of this file)"))
}

struct Web {
    app: Router,
    dir: PathBuf,
    cookie: String,
    csrf: String,
}

impl Web {
    async fn new(allow: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("vk-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        let (db, models, allow) = (
            dir.join("catalog.db").to_str().unwrap().to_string(),
            dir.join("models").to_str().unwrap().to_string(),
            allow.to_string(),
        );
        drop(katalog_core::db::connect(&dir.join("catalog.db")).unwrap());
        let config = Config::from_lookup(|k| match k {
            "KATALOG_DB" => Some(db.clone()),
            "KATALOG_ROOTS" => Some(models.clone()),
            "KATALOG_WRITE" => Some("1".into()),
            "KATALOG_ALLOWED_HOSTS" => Some("localhost:8080".into()),
            "KATALOG_PRINTER_ALLOW" if !allow.is_empty() => Some(allow.clone()),
            _ => None,
        })
        .unwrap();
        let auth = AuthFile {
            version: 1,
            password_hash: password::hash_password(PASSWORD.into()).await.unwrap(),
            must_change: false,
            updated_at: 0,
        };
        let app = katalog_server::try_build_app(AppConfig { config, auth: Some(auth), clock: Arc::new(Now) })
            .await
            .unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/login")
                    .header("host", "localhost:8080")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"password": PASSWORD}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let cookie = response.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_string();
        let csrf = json(response).await["csrfToken"].as_str().unwrap().to_string();
        Self { app, dir, cookie, csrf }
    }
    async fn call(&self, command: &str, args: Value) -> (u16, Value, String) {
        let response: Response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/invoke/{command}"))
                    .header("host", "localhost:8080")
                    .header("origin", "http://localhost:8080")
                    .header("content-type", "application/json")
                    .header("cookie", &self.cookie)
                    .header("x-csrf-token", &self.csrf)
                    .body(Body::from(args.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status().as_u16();
        let bytes = to_bytes(response.into_body(), 4 << 20).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null), text)
    }
    async fn ok(&self, command: &str, args: Value) -> Value {
        let (status, value, text) = self.call(command, args).await;
        assert_eq!(status, 200, "{command}: {text}");
        value
    }
}
impl Drop for Web {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn json(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap()
}

/// Test hook of the virtual printer (plain HTTP, no key: the hook is not a Moonraker call).
fn sim(printer: &str, path: &str, body: Value) {
    let body = body.to_string();
    let mut s = TcpStream::connect(printer).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        s,
        "POST {path} HTTP/1.1\r\nHost: {printer}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    assert!(out.starts_with("HTTP/1.1 200"), "{out}");
}

fn spool() -> Value {
    json!({"spool": {"id": "0", "material": "PLA", "manufacturer": "Virtuell", "color": "Weiss", "diameterMm": 1.75,
        "originalWeightG": 1000.0, "remainingWeightG": 1000.0}})
}
async fn remaining(web: &Web) -> f64 {
    let all = web.ok("list_filament_spools", json!({})).await;
    all.as_array().unwrap()[0]["remainingWeightG"].as_f64().unwrap()
}
async fn sync(web: &Web) {
    web.ok("sync_printers_now", json!({})).await;
}
async fn open_jobs(web: &Web) -> Vec<Value> {
    web.ok("list_open_printer_jobs", json!({})).await.as_array().unwrap().clone()
}

/// connect -> status of a running print -> finished prints appear once -> booked off the spool.
#[tokio::test]
#[ignore = "needs the virtual printer container, see the file header"]
async fn connect_status_and_booking_against_the_virtual_printer() {
    let (printer, key, allow) = (env("VK_PRINTER"), env("VK_KEY"), env("VK_ALLOW"));
    let web = Web::new(&allow).await;
    let printer_id = web.ok("add_printer", json!({"name": "Virtuell", "holderName": "Spule"})).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    web.ok("add_filament_spool", spool()).await;
    web.ok("set_printer_link_enabled", json!({"enabled": true})).await;
    let args = |key: &str| json!({"printerId": printer_id, "kind": "moonraker", "address": printer, "apiKey": key});

    // A wrong key is refused by the printer and nothing is stored.
    let rejected = web.ok("test_printer_connection", args("0000000000000000wrongwrongwrong00")).await;
    assert_eq!((rejected["ok"].as_bool(), rejected["error"].as_str()), (Some(false), Some("auth_rejected")), "{rejected}");
    assert!(web.ok("list_printer_connections", json!({})).await.as_array().unwrap().is_empty());

    // The right key connects. The key is never part of an answer.
    let (status, connected, text) = web.call("test_printer_connection", args(&key)).await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(connected["ok"], true, "{connected}");
    assert_eq!(connected["connection"]["hasApiKey"], true);
    assert!(!text.contains(&key), "the API key leaked into the connection answer");
    let (_, _, listed) = web.call("list_printer_connections", json!({})).await;
    assert!(!listed.contains(&key), "the API key leaked into the connection list");

    // Printing status: a job that is still running is not offered for booking.
    sim(&printer, "/_sim/print", json!({"filename": "Virtuell_A_PLA.gcode", "filament_mm": 3000.0, "seconds": 2.0}));
    sync(&web).await;
    assert!(open_jobs(&web).await.is_empty(), "a running print must not be offered");
    tokio::time::sleep(Duration::from_millis(2600)).await;
    sim(&printer, "/_sim/print", json!({"filename": "Virtuell_B_PLA.gcode", "filament_mm": 6000.0, "seconds": 1.0}));
    tokio::time::sleep(Duration::from_millis(1600)).await;

    // Both finished prints arrive once, also after a second sync (no duplicates).
    sync(&web).await;
    sync(&web).await;
    let jobs = open_jobs(&web).await;
    assert_eq!(jobs.len(), 2, "{jobs:?}");
    let mut names: Vec<&str> = jobs.iter().map(|j| j["fileName"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["Virtuell_A_PLA.gcode", "Virtuell_B_PLA.gcode"]);

    // Booking off the spool: grams from the filament length, once per print.
    let spool_id = web.ok("list_filament_spools", json!({})).await[0]["id"].as_str().unwrap().to_string();
    let mut expected = 0.0;
    for job in &jobs {
        let preview = web.ok("preview_printer_job", json!({"jobId": job["id"], "spoolId": spool_id})).await;
        expected += preview["grams"].as_f64().unwrap();
    }
    assert!(expected > 5.0, "unexpected total weight {expected}");
    let decisions: Vec<Value> = jobs.iter().map(|j| json!({"jobId": j["id"], "spoolId": spool_id})).collect();
    let done = web.ok("confirm_printer_jobs", json!({"decisions": decisions})).await;
    assert_eq!((done["confirmed"].as_u64(), done["failed"].as_u64()), (Some(2), Some(0)), "{done}");
    let left = remaining(&web).await;
    assert!((1000.0 - left - expected).abs() < 0.05, "spool {left} g, expected {} g", 1000.0 - expected);
    assert!(open_jobs(&web).await.is_empty());
    let again = web.ok("confirm_printer_jobs", json!({"decisions": decisions})).await;
    assert_eq!(again["confirmed"], 0, "a print must not be booked twice: {again}");
    assert_eq!(remaining(&web).await, left);

    // Later syncs bring no old print back.
    sync(&web).await;
    assert!(open_jobs(&web).await.is_empty());
}

/// Nothing leaves the server for an address outside the allowlist, and the refusals do not depend on the list.
#[tokio::test]
#[ignore = "needs the virtual printer container, see the file header"]
async fn forbidden_addresses_and_redirects_are_refused() {
    let (printer, redirect, allow) = (env("VK_PRINTER"), env("VK_REDIRECT"), env("VK_ALLOW"));
    // Wide list on purpose: metadata and loopback stay refused even when the list covers them.
    let web = Web::new(&format!("{allow},127.0.0.0/8,169.254.0.0/16,0.0.0.0/0")).await;
    let printer_id = web.ok("add_printer", json!({"name": "Virtuell", "holderName": "Spule"})).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    web.ok("set_printer_link_enabled", json!({"enabled": true})).await;
    let test = |address: &str| json!({"printerId": printer_id, "kind": "moonraker", "address": address});
    for address in ["127.0.0.1:7125", "localhost:7125", "169.254.169.254", "169.254.169.254:80", "8.8.8.8", "[::1]:7125", "[fd00:ec2::254]"] {
        let result = web.ok("test_printer_connection", test(address)).await;
        assert_eq!((result["ok"].as_bool(), result["error"].as_str()), (Some(false), Some("address_not_allowed")), "{address}: {result}");
    }
    // A printer that answers with a redirect to the metadata address is not followed.
    let result = web.ok("test_printer_connection", test(&redirect)).await;
    assert_eq!(result["ok"], false, "{result}");
    assert!(web.ok("list_printer_connections", json!({})).await.as_array().unwrap().is_empty());
    // The reachable printer needs its key; without one it is refused, not connected.
    let result = web.ok("test_printer_connection", test(&printer)).await;
    assert_eq!((result["ok"].as_bool(), result["error"].as_str()), (Some(false), Some("auth_required")), "{result}");
}

/// A narrow list keeps a private address out that is not on it, and an empty list switches the link off.
#[tokio::test]
#[ignore = "needs the virtual printer container, see the file header"]
async fn address_outside_the_allowlist_and_empty_allowlist() {
    let (printer, _allow) = (env("VK_PRINTER"), env("VK_ALLOW"));
    let narrow = Web::new("192.0.2.0/24").await;
    let printer_id = narrow.ok("add_printer", json!({"name": "V", "holderName": "S"})).await["id"].as_str().unwrap().to_string();
    narrow.ok("set_printer_link_enabled", json!({"enabled": true})).await;
    let result = narrow.ok("test_printer_connection", json!({"printerId": printer_id, "kind": "moonraker", "address": printer})).await;
    assert_eq!(result["error"], "address_not_allowed", "{result}");
    drop(narrow);
    let off = Web::new("").await;
    let (status, body, _) = off.call("list_printer_connections", json!({})).await;
    assert_eq!(status, 403);
    assert_eq!(body["message"], "printerAddressNotAllowed", "{body}");
}
