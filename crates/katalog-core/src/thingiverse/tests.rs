//! Tests against a local stub server. The fixtures follow the ASSUMED answer shape
//! described in `parse.rs`; they are not recordings of the real service.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use super::*;

const KEY: &str = "SECRETKEY0123456789abcdef";
const TERM: &str = "secretterm";

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

struct Resp {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
    delay_ms: u64,
}

fn json(status: u16, body: &str) -> Resp {
    Resp { status, headers: vec![("Content-Type", "application/json".into())], body: body.as_bytes().to_vec(), delay_ms: 0 }
}

fn image(bytes: Vec<u8>) -> Resp {
    Resp { status: 200, headers: vec![("Content-Type", "image/png".into())], body: bytes, delay_ms: 0 }
}

struct Stub {
    port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Stub {
    fn start(handler: impl Fn(&Seen) -> Resp + Send + Sync + 'static) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(handler);
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let (handler, log) = (handler.clone(), log.clone());
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut first = String::new();
                    if reader.read_line(&mut first).is_err() {
                        return;
                    }
                    let mut parts = first.split_whitespace();
                    let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                    let mut headers = Vec::new();
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                            break;
                        }
                        if let Some((k, v)) = h.trim_end().split_once(':') {
                            headers.push((k.trim().to_string(), v.trim().to_string()));
                        }
                    }
                    let seen = Seen { method, target, headers };
                    log.lock().unwrap().push(seen.clone());
                    let resp = handler(&seen);
                    std::thread::sleep(Duration::from_millis(resp.delay_ms));
                    let mut head = format!("HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n", resp.status, resp.body.len());
                    for (k, v) in &resp.headers {
                        head.push_str(&format!("{k}: {v}\r\n"));
                    }
                    head.push_str("\r\n");
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&resp.body);
                });
            }
        });
        Stub { port, seen }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }

    fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn config(stub: &Stub, limits: Limits) -> Config {
    Config { base_url: stub.base(), timeout: Duration::from_secs(5), limits, image_limits: Limits::NONE, local_test: true }
}

fn service(stub: &Stub) -> Service {
    Service::with_config(config(stub, Limits::NONE), Arc::new(|| 0.0))
}

fn token() -> Token {
    Token::parse(KEY).unwrap()
}

fn open() -> impl Fn() -> Result<(), TvError> {
    || Ok(())
}

fn search_body(n: usize, image_base: &str) -> String {
    let hits: Vec<String> = (1..=n)
        .map(|i| format!(r#"{{"id":{i},"name":"Thing {i}","thumbnail":"{image_base}/img/{i}.png","creator":{{"name":"Author {i}"}},"license":"CC BY","like_count":{i}}}"#))
        .collect();
    format!(r#"{{"total":{n},"hits":[{}]}}"#, hits.join(","))
}

fn png(len: usize) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.resize(len, 7);
    v
}

fn err(r: Result<impl std::fmt::Debug, TvError>) -> TvError {
    r.expect_err("an error was expected")
}

#[test]
fn a_search_is_parsed_cached_and_only_asked_once() {
    let stub = Stub::start(|_| json(200, &search_body(3, "https://cdn.thingiverse.com")));
    let svc = service(&stub);
    let gate_calls = AtomicUsize::new(0);
    let gate = || {
        gate_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    };
    let first = svc.search(&token(), "cable", 1, &gate).unwrap();
    assert_eq!(first.items.len(), 3);
    assert_eq!(first.items[0].author_name, "Author 1");
    assert_eq!(first.page, 1);
    assert!(!first.has_more);
    let second = svc.search(&token(), "cable", 1, &gate).unwrap();
    assert_eq!(first, second);
    assert_eq!(stub.count(), 1, "the second call came from memory");
    assert_eq!(gate_calls.load(Ordering::SeqCst), 1, "a cache hit is not counted against the day");
    svc.search(&token(), "cable", 2, &gate).unwrap();
    assert_eq!(stub.count(), 2, "another page is another request");
}

#[test]
fn only_get_is_used_with_a_bearer_header_and_no_key_in_the_address() {
    let stub = Stub::start(|s| {
        if s.target.starts_with("/users/me") {
            json(200, r#"{"name":"Ann"}"#)
        } else if s.target.starts_with("/things/") {
            json(200, r#"{"id":4,"name":"T","description":"d"}"#)
        } else {
            json(200, &search_body(1, "https://cdn.thingiverse.com"))
        }
    });
    let svc = service(&stub);
    assert_eq!(svc.test_token(&token(), &open()).unwrap(), "Ann");
    svc.search(&token(), TERM, 1, &open()).unwrap();
    svc.thing(&token(), 4, &open()).unwrap();
    let seen = stub.requests();
    assert_eq!(seen.len(), 3);
    for s in &seen {
        assert_eq!(s.method, "GET");
        assert_eq!(s.header("authorization"), Some(format!("Bearer {KEY}").as_str()));
        assert!(!s.target.contains(KEY), "the key is never part of the address");
        assert!(s.header("user-agent").is_some_and(|u| u.starts_with("3MF-Katalog-Manager/")));
    }
}

#[test]
fn errors_are_classified_from_the_status_and_the_kind_of_answer() {
    type Case = (&'static str, u16, Vec<(&'static str, String)>, Vec<u8>, ErrorCode);
    let cases: Vec<Case> = vec![
        ("broken json", 200, vec![("Content-Type", "application/json".into())], b"{not json".to_vec(), ErrorCode::ThingiverseBadResponse),
        ("unauthorized", 401, vec![], format!("echo {KEY}").into_bytes(), ErrorCode::ThingiverseTokenInvalid),
        ("forbidden json", 403, vec![("Content-Type", "application/json".into())], b"{}".to_vec(), ErrorCode::ThingiverseTokenInvalid),
        ("forbidden html", 403, vec![("Content-Type", "text/html".into()), ("cf-ray", "abc-FRA".into())], b"<html>blocked</html>".to_vec(), ErrorCode::ThingiverseBlocked),
        ("not found", 404, vec![], vec![], ErrorCode::ThingiverseNotFound),
        ("server error", 500, vec![], vec![], ErrorCode::ThingiverseBadResponse),
        ("html with 200", 200, vec![("Content-Type", "text/html".into())], b"<html></html>".to_vec(), ErrorCode::ThingiverseBadResponse),
    ];
    for (name, status, headers, body, expected) in cases {
        let stub = Stub::start(move |_| Resp { status, headers: headers.clone(), body: body.clone(), delay_ms: 0 });
        let e = err(service(&stub).search(&token(), "x", 1, &open()));
        assert_eq!(e.code, expected, "{name}");
    }
}

#[test]
fn a_429_carries_the_retry_after_seconds() {
    let stub = Stub::start(|_| Resp { status: 429, headers: vec![("Retry-After", "30".into())], body: vec![], delay_ms: 0 });
    let e = err(service(&stub).search(&token(), "x", 1, &open()));
    assert_eq!((e.code, e.retry_after_s), (ErrorCode::ThingiverseRateLimited, Some(30)));
    assert_eq!(e.params(), vec!["30".to_string()]);
}

#[test]
fn a_slow_answer_runs_into_the_time_limit() {
    let stub = Stub::start(|_| Resp { delay_ms: 800, ..json(200, "{}") });
    let cfg = Config { timeout: Duration::from_millis(150), ..config(&stub, Limits::NONE) };
    let svc = Service::with_config(cfg, Arc::new(|| 0.0));
    let started = Instant::now();
    assert_eq!(err(svc.search(&token(), "x", 1, &open())).code, ErrorCode::ThingiverseTimeout);
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
}

#[test]
fn an_answer_above_one_mib_is_refused() {
    let stub = Stub::start(|_| Resp { body: vec![b' '; MAX_JSON_BYTES as usize + 10], ..json(200, "") });
    assert_eq!(err(service(&stub).search(&token(), "x", 1, &open())).code, ErrorCode::ThingiverseBadResponse);
}

#[test]
fn a_refused_connection_is_offline() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let cfg = Config { base_url: format!("http://127.0.0.1:{port}"), timeout: Duration::from_secs(2), limits: Limits::NONE, image_limits: Limits::NONE, local_test: true };
    let svc = Service::with_config(cfg, Arc::new(|| 0.0));
    assert_eq!(err(svc.search(&token(), "x", 1, &open())).code, ErrorCode::ThingiverseOffline);
}

#[test]
fn an_invalid_query_never_reaches_the_network() {
    let stub = Stub::start(|_| json(200, &search_body(1, "")));
    let svc = service(&stub);
    for q in ["", "  ", "a\nb", &"x".repeat(101)] {
        assert_eq!(err(svc.search(&token(), q, 1, &open())).code, ErrorCode::ThingiverseQueryInvalid);
    }
    assert_eq!(stub.count(), 0);
}

struct Capture(Mutex<Vec<String>>);

impl log::Log for Capture {
    // Only this area's records: the logger is global to the whole test run, and
    // storing every line of every other test would slow the run to a crawl.
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.target().starts_with("thingiverse")
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            self.0.lock().unwrap().push(format!("{} {}", r.target(), r.args()));
        }
    }
    fn flush(&self) {}
}

fn capture() -> &'static Capture {
    static ONCE: Once = Once::new();
    static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));
    ONCE.call_once(|| {
        let _ = log::set_logger(&CAPTURE);
        log::set_max_level(log::LevelFilter::Debug);
    });
    &CAPTURE
}

#[test]
fn neither_the_key_nor_the_term_shows_up_in_errors_dtos_or_the_log() {
    let log = capture();
    // The stub echoes the key and the term in every answer, error or not.
    let stub = Stub::start(|s| {
        let echo = format!(r#"{{"echo":"{KEY}","target":"{}"}}"#, s.target);
        if s.target.contains("fail") {
            json(401, &echo)
        } else {
            json(200, &search_body(2, "https://cdn.thingiverse.com"))
        }
    });
    let svc = service(&stub);
    let failed = err(svc.search(&token(), &format!("{TERM}fail"), 1, &open()));
    let ok = svc.search(&token(), TERM, 1, &open()).unwrap();
    let mut seen_text = vec![format!("{failed:?}"), format!("{failed:?}").to_lowercase(), serde_json::to_string(&ok).unwrap(), format!("{ok:?}")];
    seen_text.push(failed.params().join("|"));
    for text in &seen_text {
        assert!(!text.contains(KEY) && !text.contains(TERM), "leak in {text}");
    }
    let lines = log.0.lock().unwrap().join("\n");
    assert!(lines.contains("thingiverse"), "the request is logged at all");
    assert!(!lines.contains(KEY), "the key is in the log");
    assert!(!lines.to_lowercase().contains(TERM), "the search term is in the log");
}

#[test]
fn the_gate_stops_a_request_and_the_limiter_refuses_without_network_use() {
    let stub = Stub::start(|_| json(200, &search_body(1, "")));
    let svc = service(&stub);
    let closed = || Err(TvError::new(ErrorCode::ThingiverseDailyLimit));
    assert_eq!(err(svc.search(&token(), "a", 1, &closed)).code, ErrorCode::ThingiverseDailyLimit);
    assert_eq!(stub.count(), 0);

    // 60 requests fit into five minutes (time stands still), the 61st is refused.
    let cfg = config(&stub, Limits { min_gap_s: 0.0, ..Limits::PRODUCTION });
    let svc = Service::with_config(cfg, Arc::new(|| 0.0));
    for i in 0..60 {
        svc.search(&token(), &format!("q{i}"), 1, &open()).unwrap();
    }
    assert_eq!(stub.count(), 60);
    let e = err(svc.search(&token(), "q61", 1, &open()));
    assert_eq!(e.code, ErrorCode::ThingiverseRateLimited);
    assert_eq!(e.retry_after_s, Some(300));
    assert_eq!(stub.count(), 60, "the refused request made no network call");
}

#[test]
fn requests_are_spaced_by_the_minimum_gap() {
    let stub = Stub::start(|_| json(200, &search_body(1, "")));
    let cfg = config(&stub, Limits { min_gap_s: 0.25, window_s: 300.0, window_max: 60 });
    let start = Instant::now();
    let svc = Service::with_config(cfg, Arc::new(move || start.elapsed().as_secs_f64()));
    for q in ["a", "b", "c"] {
        svc.search(&token(), q, 1, &open()).unwrap();
    }
    assert!(start.elapsed() >= Duration::from_millis(500), "{:?}", start.elapsed());
}

fn seen_thing_service(stub: &Stub) -> Service {
    let svc = service(stub);
    svc.search(&token(), "x", 1, &open()).unwrap();
    svc
}

/// A stub that serves `n` hits whose images come from the stub itself.
fn stub_with_images(img: Vec<u8>, n: usize) -> Stub {
    let holder: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let h = holder.clone();
    let stub = Stub::start(move |s| {
        if s.target.starts_with("/img/") { image(img.clone()) } else { json(200, &search_body(n, &h.lock().unwrap())) }
    });
    *holder.lock().unwrap() = stub.base();
    stub
}


#[test]
fn a_valid_image_becomes_a_data_address_without_the_key() {
    for (bytes, mime) in [
        (png(300), "image/png"),
        ({ let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0]; v.resize(300, 1); v }, "image/jpeg"),
        ({ let mut v = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec(); v.resize(300, 1); v }, "image/webp"),
    ] {
        let stub = stub_with_images(bytes, 1);
        let svc = seen_thing_service(&stub);
        let data = svc.image(1, ImageSize::Thumb).unwrap().expect("an image");
        assert!(data.starts_with(&format!("data:{mime};base64,")), "{data}");
        let img_request = stub.requests().into_iter().find(|s| s.target.starts_with("/img/")).unwrap();
        assert!(img_request.header("authorization").is_none(), "the key is not sent to the image host");
        let again = svc.image(1, ImageSize::Thumb).unwrap();
        assert_eq!(again.as_deref(), Some(data.as_str()));
        assert_eq!(stub.count(), 2, "the second image call came from memory");
    }
}

#[test]
fn images_are_not_counted_for_the_day_and_have_their_own_limit() {
    let stub = stub_with_images(png(300), 3);
    let cfg = Config { image_limits: Limits { min_gap_s: 0.0, window_s: 300.0, window_max: 2 }, ..config(&stub, Limits { min_gap_s: 0.0, ..Limits::PRODUCTION }) };
    let svc = Service::with_config(cfg, Arc::new(|| 0.0));
    // Three hits cost one API slot; the images use the image limiter, never the gate.
    svc.search(&token(), "x", 1, &open()).unwrap();
    let closed = || Err(TvError::new(ErrorCode::ThingiverseDailyLimit));
    assert!(svc.search(&token(), "y", 1, &closed).is_err(), "precondition: the gate would refuse an API request");
    svc.image(1, ImageSize::Thumb).unwrap();
    svc.image(2, ImageSize::Thumb).unwrap();
    let e = err(svc.image(3, ImageSize::Thumb));
    assert_eq!(e.code, ErrorCode::ThingiverseRateLimited, "the image window is full");
    // The API limiter was not used up by the images: with 60 slots and one used, more fit.
    svc.search(&token(), "z", 1, &open()).unwrap();
}

#[test]
fn unusable_images_are_refused() {
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>".to_vec();
    let too_big = png(MAX_IMAGE_BYTES as usize + 10);
    let wrong_magic = b"GIF89a....".to_vec();
    for (name, bytes) in [("svg", svg), ("too big", too_big), ("magic does not match", wrong_magic)] {
        let stub = stub_with_images(bytes, 1);
        let svc = seen_thing_service(&stub);
        assert_eq!(err(svc.image(1, ImageSize::Thumb)).code, ErrorCode::ThingiverseBadResponse, "{name}");
        assert!(svc.cached_bytes() < 4096, "{name}: nothing from the refused image is kept");
    }
}

#[test]
fn production_rules_refuse_a_foreign_host_and_plain_http() {
    let stub = stub_with_images(png(100), 1);
    // `local_test` off: the loopback http address is neither https nor a Thingiverse host.
    let cfg = Config { local_test: false, ..config(&stub, Limits::NONE) };
    let svc = Service::with_config(cfg, Arc::new(|| 0.0));
    svc.search(&token(), "x", 1, &open()).unwrap();
    let before = stub.count();
    assert_eq!(err(svc.image(1, ImageSize::Thumb)).code, ErrorCode::ThingiverseBadResponse);
    assert_eq!(stub.count(), before, "no request went to the foreign host");
}

#[test]
fn an_image_of_an_unknown_thing_is_not_fetched() {
    let stub = Stub::start(|_| json(200, "{}"));
    let svc = service(&stub);
    assert_eq!(svc.image(999, ImageSize::Thumb).unwrap(), None);
    assert_eq!(stub.count(), 0);
}

#[test]
fn the_cache_expires_and_clear_empties_it() {
    let stub = Stub::start(|_| json(200, &search_body(1, "")));
    let millis = Arc::new(AtomicU64::new(0));
    let m = millis.clone();
    let svc = Service::with_config(config(&stub, Limits::NONE), Arc::new(move || m.load(Ordering::SeqCst) as f64 / 1000.0));
    svc.search(&token(), "x", 1, &open()).unwrap();
    millis.store(599_000, Ordering::SeqCst);
    svc.search(&token(), "x", 1, &open()).unwrap();
    assert_eq!(stub.count(), 1, "still fresh after 599 s");
    millis.store(601_000, Ordering::SeqCst);
    svc.search(&token(), "x", 1, &open()).unwrap();
    assert_eq!(stub.count(), 2, "asked again after ten minutes");
    assert!(svc.cached_bytes() > 0);
    svc.clear();
    assert_eq!(svc.cached_bytes(), 0);
    svc.search(&token(), "x", 1, &open()).unwrap();
    assert_eq!(stub.count(), 3, "clear dropped the result");
}

#[test]
fn a_hundred_hits_with_images_stay_below_the_memory_budget() {
    let stub = stub_with_images(png(200 * 1024), 100);
    let svc = service(&stub);
    svc.search(&token(), "x", 1, &open()).unwrap();
    for id in 1..=100 {
        svc.image(id, ImageSize::Thumb).unwrap();
        assert!(svc.cached_bytes() <= cache::MAX_BYTES, "after image {id}");
    }
    assert!(svc.cached_bytes() > cache::MAX_BYTES / 2, "the images really are held: {}", svc.cached_bytes());
}

#[test]
fn nothing_is_written_to_the_working_directory() {
    let listing = || {
        let mut v: Vec<String> = std::fs::read_dir(".").unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    };
    let before = listing();
    let stub = Stub::start(|_| json(200, &search_body(3, "")));
    let svc = service(&stub);
    svc.search(&token(), "x", 1, &open()).unwrap();
    assert_eq!(before, listing());
}

#[test]
fn the_page_address_is_checked_or_built() {
    let stub = Stub::start(|_| {
        json(200, r#"{"hits":[{"id":5,"name":"A","public_url":"https://evil.example/thing:5"},{"id":6,"name":"B","public_url":"https://www.thingiverse.com/thing:6"}]}"#)
    });
    let svc = service(&stub);
    let r = svc.search(&token(), "x", 1, &open()).unwrap();
    assert_eq!(r.items[0].url, "https://www.thingiverse.com/thing:5");
    assert_eq!(svc.page_url(5), "https://www.thingiverse.com/thing:5");
    assert_eq!(svc.page_url(6), "https://www.thingiverse.com/thing:6");
    assert_eq!(svc.page_url(77), "https://www.thingiverse.com/thing:77");
}

#[test]
fn a_detail_of_another_thing_than_asked_for_is_a_bad_response() {
    let stub = Stub::start(|_| json(200, r#"{"id":2,"name":"T"}"#));
    assert_eq!(err(service(&stub).thing(&token(), 1, &open())).code, ErrorCode::ThingiverseBadResponse);
}

/// Optional check against the real service with the production client; skipped unless
/// `TV_TEST_TOKEN_FILE` names a file holding an app token. Prints only the outcome,
/// never the key. Run: `TV_TEST_TOKEN_FILE=/path cargo test -p katalog-core --lib
/// real_service -- --nocapture`.
#[test]
fn real_service_accepts_the_production_client_when_a_token_file_is_given() {
    let Ok(path) = std::env::var("TV_TEST_TOKEN_FILE") else { return };
    let raw = std::fs::read_to_string(path).expect("token file readable");
    let token = Token::parse(raw.trim()).expect("token file holds a well-formed key");
    let svc = Service::new();
    let report = |what: &str, result: Result<(), TvError>| {
        eprintln!("real service, {what}: {}", result.as_ref().map_or_else(|e| format!("{:?}", e.code), |_| "OK".to_string()));
        result
    };
    report("users/me", svc.test_token(&token, &open()).map(|_| ())).expect("users/me");
    let found = svc.search(&token, "benchy", 1, &open());
    report("search", found.as_ref().map(|_| ()).map_err(Clone::clone)).expect("search");
    let found = found.unwrap();
    assert!(!found.items.is_empty());
    let first = found.items.iter().find(|i| i.preview_available).expect("a hit with a preview");
    report("image", svc.image(first.id, ImageSize::Thumb).map(|_| ())).expect("image");
}
