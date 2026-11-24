use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request},
    Router,
};
use katalog_server::{
    build_app,
    config::Config,
    password::{self, AuthFile},
    AppConfig, ClientAddress, Clock,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tower::ServiceExt;
const PASSWORD: &str = "Only-test-password-42";
const NEW_PASSWORD: &str = "Changed-test-password-84";
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Time(AtomicU64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
impl Time {
    fn set(&self, n: u64) {
        self.0.store(n, Ordering::SeqCst)
    }
}
struct Fixture {
    app: Router,
    time: Arc<Time>,
    dir: PathBuf,
    catalog_digest: Vec<u8>,
    directory_entries: Vec<std::ffi::OsString>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert_eq!(
                Sha256::digest(std::fs::read(self.dir.join("catalog.db")).unwrap()).to_vec(),
                self.catalog_digest,
                "catalog end-state changed"
            );
            assert_eq!(
                entries(&self.dir),
                self.directory_entries,
                "catalog directory end-state changed"
            );
        }
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}
fn entries(dir: &std::path::Path) -> Vec<std::ffi::OsString> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n != "web-auth.json" && n != "catalog.db.lock")
        .collect();
    names.sort();
    names
}
async fn fixture(forced: bool, proxy: bool) -> Fixture {
    fixture_with(forced, proxy, "10.0.0.0/24,fd00::/16").await
}
async fn fixture_with(forced: bool, proxy: bool, trusted: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "katalog-http-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&dir).unwrap();
    drop(katalog_core::db::connect(&dir.join("catalog.db")).unwrap());
    let catalog_digest = Sha256::digest(std::fs::read(dir.join("catalog.db")).unwrap()).to_vec();
    let directory_entries = entries(&dir);
    let auth = AuthFile {
        version: 1,
        password_hash: password::hash_password(PASSWORD.into()).await.unwrap(),
        must_change: forced,
        updated_at: 100,
    };
    let auth_path = dir.join("web-auth.json");
    password::write_auth(&auth_path, &auth).unwrap();
    let config = Config {
        source_roots: vec![], printer_allow: Default::default(),
        slicer_links: None, file_ops: false, file_ops_daily_limit: 2000, companion_max_bytes: 1024 * 1024 * 1024, file_ops_ack: vec![], file_roots: vec![],
        upload: None,
        write: false,
        static_dir: None,
        db_path: dir.join("catalog.db"),
        roots: vec![dir.clone()],
        auth_file: auth_path,
        bind: "127.0.0.1:8080".parse().unwrap(),
        allowed_hosts: Some(vec!["localhost:8080".into(), "[::1]:8080".into()]),
        trust_proxy: proxy,
        trusted_proxies: if proxy { katalog_server::config::TrustedProxies::parse(trusted).unwrap() } else { Default::default() },
        allow_no_auth: false,
        password: None,
        password_file: None,
    };
    let time = Arc::new(Time(AtomicU64::new(100)));
    let app = build_app(AppConfig {
        config,
        auth: Some(auth),
        clock: time.clone(),
    });
    Fixture {
        app,
        time,
        dir,
        catalog_digest,
        directory_entries,
    }
}
async fn raw(
    app: &Router,
    method: &str,
    path: &str,
    body: Body,
    headers: &[(&str, &str)],
    client: &str,
) -> (u16, HeaderMap, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if !headers.iter().any(|(name, _)| *name == "host") {
        req = req.header("host", "localhost:8080");
    }
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let mut req = req.body(body).unwrap();
    // Short names stand for fixed direct peers; the proxy fixtures trust 10.0.0.0/24.
    let peer = match client {
        "a" => "10.0.0.1",
        "b" => "10.0.0.2",
        "proxy" => "10.0.0.3",
        "other" => "10.0.0.4",
        other => other,
    };
    req.extensions_mut().insert(ClientAddress(peer.into()));
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["referrer-policy"], "no-referrer");
    assert_eq!(headers["content-security-policy"], "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'; img-src 'self' data: blob:; font-src 'self'");
    if path.starts_with("/api/") {
        assert_eq!(headers["cache-control"], "no-store");
    }
    let bytes = to_bytes(response.into_body(), 100000).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}
async fn call(
    app: &Router,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
    client: &str,
) -> (u16, HeaderMap, Value) {
    let mut h = headers.to_vec();
    // Model the trusted proxy's mandatory appended client address.
    if !h.iter().any(|(name, _)| *name == "x-forwarded-for") {
        h.push((
            "x-forwarded-for",
            if client == "a" {
                "192.0.2.1"
            } else {
                "192.0.2.2"
            },
        ));
    }
    h.push(("content-type", "application/json"));
    raw(app, "POST", path, Body::from(body.to_string()), &h, client).await
}
async fn login(app: &Router, client: &str, proxy: bool) -> (String, String) {
    let h = if proxy {
        vec![("x-forwarded-proto", "https")]
    } else {
        vec![]
    };
    let (status, headers, value) =
        call(app, "/api/login", json!({"password":PASSWORD}), &h, client).await;
    assert_eq!(status, 200);
    (
        headers["set-cookie"].to_str().unwrap().into(),
        value["csrfToken"].as_str().unwrap().into(),
    )
}
async fn session(app: &Router, cookie: &str) -> Value {
    raw(
        app,
        "GET",
        "/api/session",
        Body::empty(),
        &[("cookie", cookie), ("x-forwarded-for", "192.0.2.1")],
        "a",
    )
    .await
    .2
}

#[tokio::test]
async fn login_logout_proxy_cookie_and_auth_contract() {
    let _lock = SERIAL.lock().await;
    let f = fixture(false, true).await;
    // The answers carry registered codes; the interface owns the wording.
    let (status, _, v) = call(&f.app, "/api/invoke/get_app_version", json!({}), &[], "a").await;
    assert_eq!(status, 401);
    assert_eq!(v["message"], "webLoginRequired");
    assert_eq!(v["expected"], true);
    let (status, _, v) = call(&f.app, "/api/login", json!({"password":""}), &[], "a").await;
    assert_eq!(status, 401);
    assert_eq!(v["message"], "webPasswordIncorrect");
    let (cookie, csrf) = login(&f.app, "a", true).await;
    for flag in [
        "__Host-katalog_session=",
        "HttpOnly",
        "SameSite=Strict",
        "Path=/",
        "Max-Age=43200",
        "Secure",
    ] {
        assert!(cookie.contains(flag));
    }
    let (status, h, _) = call(
        &f.app,
        "/api/logout",
        json!({}),
        &[
            ("cookie", &cookie),
            ("x-csrf-token", &csrf),
            ("x-forwarded-proto", "https"),
        ],
        "a",
    )
    .await;
    assert_eq!(status, 200);
    let deleted = h["set-cookie"].to_str().unwrap();
    for flag in [
        "__Host-katalog_session=",
        "HttpOnly",
        "SameSite=Strict",
        "Path=/",
        "Max-Age=0",
        "Secure",
    ] {
        assert!(deleted.contains(flag));
    }
    let f = fixture(false, false).await;
    let (cookie, _) = login(&f.app, "a", false).await;
    assert!(cookie.starts_with("katalog_session="));
    assert!(!cookie.contains("Secure"));
    assert_eq!(session(&f.app, &cookie).await["authenticated"], true);
}

#[tokio::test]
async fn csrf_origin_hosts_methods_json_and_body_limits() {
    let _lock = SERIAL.lock().await;
    let f = fixture(false, false).await;
    let (cookie, csrf) = login(&f.app, "a", false).await;
    let route = "/api/invoke/get_app_version";
    for token in [None, Some("wrong")] {
        let mut h = vec![("cookie", cookie.as_str())];
        if let Some(token) = token {
            h.push(("x-csrf-token", token));
        }
        assert_eq!(call(&f.app, route, json!({}), &h, "a").await.0, 403);
    }
    for origin in [
        "https://localhost:8080",
        "http://evil:8080",
        "http://localhost:8081",
        "null",
        "bogus",
    ] {
        assert_eq!(
            call(
                &f.app,
                route,
                json!({}),
                &[
                    ("cookie", &cookie),
                    ("x-csrf-token", &csrf),
                    ("origin", origin)
                ],
                "a"
            )
            .await
            .0,
            403
        );
    }
    for origin in [None, Some("http://localhost:8080")] {
        let mut h = vec![("cookie", cookie.as_str()), ("x-csrf-token", csrf.as_str())];
        if let Some(origin) = origin {
            h.push(("origin", origin));
        }
        assert_eq!(call(&f.app, route, json!({}), &h, "a").await.0, 200);
    }
    for method in ["HEAD", "PUT", "DELETE", "OPTIONS"] {
        assert_eq!(
            raw(&f.app, method, "/api/health", Body::empty(), &[], "a")
                .await
                .0,
            405
        );
    }
    for (host, status) in [("evil:8080", 421), ("bad/host", 400), ("[::1]:8080", 200)] {
        assert_eq!(
            raw(
                &f.app,
                "GET",
                "/api/health",
                Body::empty(),
                &[("host", host)],
                "a"
            )
            .await
            .0,
            status
        );
    }
    assert_eq!(
        raw(
            &f.app,
            "GET",
            "/api/health",
            Body::empty(),
            &[("host", "localhost:8080"), ("host", "localhost:8080")],
            "a"
        )
        .await
        .0,
        400
    );
    for path in ["/api/health", "/api/session"] {
        assert_eq!(
            raw(&f.app, "GET", path, Body::empty(), &[], "a").await.0,
            200
        );
    }
    let (status, _, value) = raw(
        &f.app,
        "POST",
        "/api/login",
        Body::from("{"),
        &[("content-type", "application/json")],
        "a",
    )
    .await;
    assert_eq!(status, 400);
    assert!(value["message"].is_string());
    assert_eq!(value["expected"], true);
    for explicit in [false, true] {
        let h = if explicit {
            vec![
                ("content-length", "65537"),
                ("content-type", "application/json"),
            ]
        } else {
            vec![("content-type", "application/json")]
        };
        assert_eq!(
            raw(
                &f.app,
                "POST",
                "/api/login",
                Body::from(vec![b'x'; 65537]),
                &h,
                "a"
            )
            .await
            .0,
            413
        );
    }
    assert_eq!(
        raw(&f.app, "GET", "/unknown", Body::empty(), &[], "a")
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn mandatory_password_change_rotates_sessions_and_write_failure_is_atomic() {
    let _lock = SERIAL.lock().await;
    let f = fixture(true, false).await;
    let (cookie, csrf) = login(&f.app, "a", false).await;
    let (other, _) = login(&f.app, "b", false).await;
    assert_eq!(session(&f.app, &cookie).await["mustChangePassword"], true);
    let h = [("cookie", cookie.as_str()), ("x-csrf-token", csrf.as_str())];
    let (status, _, v) = call(&f.app, "/api/invoke/get_app_version", json!({}), &h, "a").await;
    assert_eq!(status, 403);
    assert_eq!(v["code"], "password_change_required");
    for new in [
        "short".into(),
        "x".repeat(129),
        "x".repeat(12),
        PASSWORD.into(),
        "123456789012".into(),
    ] {
        assert_eq!(
            call(
                &f.app,
                "/api/password",
                json!({"current":PASSWORD,"new":new}),
                &h,
                "a"
            )
            .await
            .0,
            422
        );
    }
    let auth_path = f.dir.join("web-auth.json");
    let before = std::fs::read(&auth_path).unwrap();
    std::fs::remove_file(&auth_path).unwrap();
    std::fs::create_dir(&auth_path).unwrap();
    let (status, _, v) = call(
        &f.app,
        "/api/password",
        json!({"current":PASSWORD,"new":NEW_PASSWORD}),
        &h,
        "a",
    )
    .await;
    assert_eq!(status, 500);
    assert_eq!(v["message"], "webServerInternal");
    assert_eq!(session(&f.app, &other).await["authenticated"], true);
    std::fs::remove_dir(&auth_path).unwrap();
    std::fs::write(&auth_path, before).unwrap();
    let (status, headers, v) = call(
        &f.app,
        "/api/password",
        json!({"current":PASSWORD,"new":NEW_PASSWORD}),
        &h,
        "a",
    )
    .await;
    assert_eq!(status, 200);
    let new_cookie = headers["set-cookie"].to_str().unwrap();
    assert_ne!(new_cookie, cookie);
    assert_ne!(v["csrfToken"], csrf);
    assert_eq!(v["mustChangePassword"], false);
    assert_eq!(session(&f.app, &cookie).await["authenticated"], false);
    assert_eq!(session(&f.app, &other).await["authenticated"], false);
    assert_eq!(session(&f.app, new_cookie).await["authenticated"], true);
    assert_eq!(
        call(
            &f.app,
            "/api/invoke/get_app_version",
            json!({}),
            &[
                ("cookie", new_cookie),
                ("x-csrf-token", v["csrfToken"].as_str().unwrap())
            ],
            "a"
        )
        .await
        .0,
        200
    );
    assert!(password::verify_password(
        password::read_auth(&auth_path).unwrap().password_hash,
        NEW_PASSWORD.into()
    )
    .await
    .unwrap());
}

#[tokio::test]
async fn session_idle_absolute_expiry_and_per_client_cap() {
    let _lock = SERIAL.lock().await;
    let f = fixture(false, false).await;
    let (first, _) = login(&f.app, "a", false).await;
    f.time.set(7300);
    assert_eq!(session(&f.app, &first).await["authenticated"], false);
    f.time.set(8000);
    let (cookie, _) = login(&f.app, "a", false).await;
    for now in (11000..51200).step_by(3000) {
        f.time.set(now);
        assert_eq!(session(&f.app, &cookie).await["authenticated"], true);
    }
    f.time.set(51200);
    assert_eq!(session(&f.app, &cookie).await["authenticated"], false);
    let (first, _) = login(&f.app, "a", false).await;
    for n in 1..=10 {
        f.time.set(51200 + n);
        login(&f.app, "a", false).await;
    }
    assert_eq!(session(&f.app, &first).await["authenticated"], false);
}

#[tokio::test]
async fn client_lock_proxy_selection_and_wrong_current_share_limits() {
    let _lock = SERIAL.lock().await;
    let f = fixture(false, true).await;
    let (cookie, csrf) = login(&f.app, "a", true).await;
    let h = [
        ("cookie", cookie.as_str()),
        ("x-csrf-token", csrf.as_str()),
        ("x-forwarded-proto", "https"),
    ];
    // Wrong current-password checks and login failures share the peer's counter.
    for _ in 0..4 {
        assert_eq!(
            call(
                &f.app,
                "/api/password",
                json!({"current":"wrong","new":NEW_PASSWORD}),
                &h,
                "a"
            )
            .await
            .0,
            401
        );
    }
    assert_eq!(
        call(&f.app, "/api/login", json!({"password":"wrong"}), &[], "a")
            .await
            .0,
        401
    );
    let (status, headers, v) =
        call(&f.app, "/api/login", json!({"password":PASSWORD}), &[], "a").await;
    assert_eq!(status, 429);
    assert_eq!(headers["retry-after"], "60");
    assert_eq!(v["message"], "webTooManyAttempts");
    f.time.set(120);
    let (status, headers, _) =
        call(&f.app, "/api/login", json!({"password":PASSWORD}), &[], "a").await;
    assert_eq!(status, 429);
    assert_eq!(headers["retry-after"], "40");
    assert_eq!(
        call(&f.app, "/api/invoke/get_app_version", json!({}), &h, "a")
            .await
            .0,
        200
    );
    assert_eq!(
        call(
            &f.app,
            "/api/login",
            json!({"password":PASSWORD}),
            &[],
            "other"
        )
        .await
        .0,
        200
    );
    f.time.set(160);
    assert_eq!(
        call(&f.app, "/api/login", json!({"password":PASSWORD}), &[], "a")
            .await
            .0,
        200
    );
    // The first XFF element changes, but the final proxy-provided address remains locked.
    for n in 0..5 {
        let xff = format!("192.0.2.{n}, 198.51.100.1");
        assert_eq!(
            call(
                &f.app,
                "/api/login",
                json!({"password":"wrong"}),
                &[("x-forwarded-for", &xff)],
                "proxy"
            )
            .await
            .0,
            401
        );
    }
    assert_eq!(
        call(
            &f.app,
            "/api/login",
            json!({"password":PASSWORD}),
            &[("x-forwarded-for", "192.0.2.99, 198.51.100.1")],
            "proxy"
        )
        .await
        .0,
        429
    );
    assert_eq!(
        call(
            &f.app,
            "/api/login",
            json!({"password":PASSWORD}),
            &[("x-forwarded-for", "198.51.100.1, 192.0.2.99")],
            "proxy"
        )
        .await
        .0,
        200
    );
    let f = fixture(false, false).await;
    let (status, headers, _) = call(
        &f.app,
        "/api/login",
        json!({"password":PASSWORD}),
        &[("x-forwarded-proto", "https")],
        "a",
    )
    .await;
    assert_eq!(status, 200);
    assert!(!headers["set-cookie"].to_str().unwrap().contains("Secure"));
    for n in 0..5 {
        let xff = format!("192.0.2.{n}");
        assert_eq!(
            call(
                &f.app,
                "/api/login",
                json!({"password":"wrong"}),
                &[("x-forwarded-for", &xff)],
                "a"
            )
            .await
            .0,
            401
        );
    }
    assert_eq!(
        call(
            &f.app,
            "/api/login",
            json!({"password":PASSWORD}),
            &[("x-forwarded-for", "198.51.100.1")],
            "a"
        )
        .await
        .0,
        429
    );
    assert_eq!(
        call(
            &f.app,
            "/api/login",
            json!({"password":PASSWORD}),
            &[("x-forwarded-for", "198.51.100.1")],
            "b"
        )
        .await
        .0,
        200
    );
}

#[tokio::test]
async fn trusted_proxy_requires_valid_last_xff_and_accepts_socket_addresses() {
    let _serial = SERIAL.lock().await;
    let f = fixture(false, true).await;
    for headers in [
        vec![],
        vec![("x-forwarded-for", "invalid")],
        vec![("x-forwarded-for", "192.0.2.1, ")],
    ] {
        let (status, _, body) = raw(
            &f.app,
            "GET",
            "/api/health",
            Body::empty(),
            &headers,
            "proxy",
        )
        .await;
        assert_eq!(status, 400);
        assert_eq!(
            body,
            json!({"message":"webProxyHeaderInvalid","expected":true})
        );
    }
    for address in [
        "192.0.2.1:1234",
        "[2001:db8::1]:1234",
        "192.0.2.2, 2001:db8::1",
    ] {
        assert_eq!(
            raw(
                &f.app,
                "GET",
                "/api/health",
                Body::empty(),
                &[("x-forwarded-for", address)],
                "proxy"
            )
            .await
            .0,
            200
        );
    }
}

fn xff(value: &str) -> [(&str, &str); 1] {
    [("x-forwarded-for", value)]
}
async fn wrong_logins(f: &Fixture, peer: &str, forged: impl Fn(usize) -> String) {
    for n in 0..5 {
        let value = forged(n);
        assert_eq!(call(&f.app, "/api/login", json!({"password":"wrong"}), &xff(&value), peer).await.0, 401);
    }
}

#[tokio::test]
async fn forwarded_headers_from_a_foreign_peer_are_ignored() {
    let _lock = SERIAL.lock().await;
    let f = fixture(false, true).await;
    // No 400 for a missing header and no trust in a forged one: the peer's own address counts.
    assert_eq!(raw(&f.app, "GET", "/api/health", Body::empty(), &[], "203.0.113.5").await.0, 200);
    assert_eq!(raw(&f.app, "GET", "/api/health", Body::empty(), &xff("nonsense"), "203.0.113.5").await.0, 200);
    // Rotating a forged X-Forwarded-For does not dodge the lockout of the real peer.
    wrong_logins(&f, "203.0.113.5", |n| format!("192.0.2.{n}")).await;
    let forged = "198.51.100.77";
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff(forged), "203.0.113.5").await.0, 429);
    // Counter-check: another real peer is unaffected, and a trusted proxy still maps to the forwarded client.
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff(forged), "203.0.113.6").await.0, 200);
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff(forged), "proxy").await.0, 200);
    // X-Forwarded-Proto: https from a foreign peer must not switch on Secure cookies; from the proxy it does.
    let h = [("x-forwarded-proto", "https")];
    let (status, headers, _) = call(&f.app, "/api/login", json!({"password":PASSWORD}), &h, "203.0.113.7").await;
    assert_eq!(status, 200);
    let cookie = headers["set-cookie"].to_str().unwrap();
    assert!(cookie.starts_with("katalog_session=") && !cookie.contains("Secure"));
    let (_, headers, _) = call(&f.app, "/api/login", json!({"password":PASSWORD}), &h, "proxy").await;
    assert!(headers["set-cookie"].to_str().unwrap().contains("__Host-katalog_session=") && headers["set-cookie"].to_str().unwrap().contains("Secure"));
}

#[tokio::test]
async fn forwarded_chain_is_unwound_from_the_right_and_a_forged_left_entry_is_useless() {
    let _lock = SERIAL.lock().await;
    let f = fixture(false, true).await;
    // Hops: forged entry, the real client, a second trusted proxy (10.0.0.9). The client is the
    // first non-trusted address from the right, whatever stands further left.
    wrong_logins(&f, "proxy", |n| format!("192.0.2.{n}, 198.51.100.1, 10.0.0.9")).await;
    let chain = "203.0.113.200, 198.51.100.1, 10.0.0.9";
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff(chain), "proxy").await.0, 429);
    // Counter-check: a different real client behind the same proxies is not locked.
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff("198.51.100.2, 10.0.0.9"), "proxy").await.0, 200);
    // A trusted proxy as the last hop is not the client; the entry in front of it is.
    let f = fixture(false, true).await;
    wrong_logins(&f, "proxy", |_| "198.51.100.1, 10.0.0.8".into()).await;
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff("198.51.100.1"), "proxy").await.0, 429);
    // Garbage next to the proxy is rejected, even when something valid stands to its left.
    let (status, _, body) = raw(&f.app, "GET", "/api/health", Body::empty(), &xff("198.51.100.1, garbage"), "proxy").await;
    assert_eq!((status, body["message"].as_str()), (400, Some("webProxyHeaderInvalid")));
    // Several header fields are read as one chain, in order.
    let (status, _, _) = raw(&f.app, "GET", "/api/health", Body::empty(), &[("x-forwarded-for", "192.0.2.1"), ("x-forwarded-for", "198.51.100.1")], "proxy").await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn ipv6_proxies_and_clients_use_the_slash_64_lockout() {
    let _lock = SERIAL.lock().await;
    let f = fixture(false, true).await;
    // fd00::/16 is trusted; the client behind it is an IPv6 address, grouped by /64.
    wrong_logins(&f, "fd00::5", |n| format!("2001:db8:1:2::{}", n + 1)).await;
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff("2001:db8:1:2:ffff::9"), "fd00::5").await.0, 429);
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff("2001:db8:1:3::1"), "fd00::5").await.0, 200);
    // A foreign IPv6 peer forging the proxy's address in the header gains nothing.
    wrong_logins(&f, "2001:db8:ffff::bad", |_| "fd00::1".into()).await;
    assert_eq!(call(&f.app, "/api/login", json!({"password":PASSWORD}), &xff("198.51.100.50"), "2001:db8:ffff::bad").await.0, 429);
    // An IPv4-mapped peer address matches an IPv4 proxy entry.
    assert_eq!(raw(&f.app, "GET", "/api/health", Body::empty(), &[], "::ffff:10.0.0.3").await.0, 400);
}
