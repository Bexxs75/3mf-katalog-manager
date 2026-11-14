//! Signed "open in slicer" links: minting through the session command and the
//! unauthenticated download route behind the signature.
use axum::{
    body::{to_bytes, Body},
    http::Request,
    response::Response,
    Router,
};
use katalog_core::db;
use katalog_server::{
    config::Config,
    password::{self, AuthFile},
    AppConfig, Clock,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};
use tower::ServiceExt;

// Password hashing admits few parallel jobs; tests take turns.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static NEXT: AtomicUsize = AtomicUsize::new(0);
const PASSWORD: &str = "slicer-test-password";
const DATA: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const START: u64 = 1_793_000_000;

struct Time(AtomicU64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Session {
    cookie: String,
    csrf: String,
}

struct Fixture {
    app: Router,
    dir: PathBuf,
    id: i64,
    time: Arc<Time>,
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

fn config(dir: &Path, extra: &'static [(&'static str, &'static str)], auth: bool) -> Config {
    Config::from_lookup(|k| match k {
        "KATALOG_DB" => Some(dir.join("catalog.db").to_str().unwrap().into()),
        "KATALOG_ROOTS" => Some(dir.join("models").to_str().unwrap().into()),
        "KATALOG_ALLOW_NO_AUTH" if !auth => Some("1".into()),
        "KATALOG_STATIC_DIR" if extra.iter().any(|(key, _)| *key == "STATIC") => Some(dir.join("static").to_str().unwrap().into()),
        other => extra.iter().find(|(key, _)| *key == other).map(|(_, v)| (*v).to_string()),
    })
    .unwrap()
}

impl Fixture {
    async fn new() -> Self {
        Self::with(&[("KATALOG_SLICER_LINKS", "1")], true).await
    }
    async fn with_forced() -> Self {
        let mut f = Self::with(&[("KATALOG_SLICER_LINKS", "1")], true).await;
        f.app = Self::build(&f.dir, &[("KATALOG_SLICER_LINKS", "1")], true, f.time.clone(), true).await;
        f
    }
    async fn with(extra: &'static [(&'static str, &'static str)], auth: bool) -> Self {
        let serial = SERIAL.lock().await;
        let dir = std::env::temp_dir().join(format!("slicer-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        std::fs::create_dir_all(dir.join("static")).unwrap();
        std::fs::write(dir.join("static/index.html"), "<html>spa</html>").unwrap();
        let path = dir.join("models/Würfel groß.stl");
        std::fs::write(&path, DATA).unwrap();
        let conn = db::connect(&dir.join("catalog.db")).unwrap();
        let id = db::test_insert_minimal_file(&conn, path.to_str().unwrap(), None).unwrap();
        conn.execute("UPDATE files SET file_type='stl' WHERE id=?1", [id]).unwrap();
        drop(conn);
        let time = Arc::new(Time(AtomicU64::new(START)));
        let app = Self::build(&dir, extra, auth, time.clone(), false).await;
        Self { app, dir, id, time, _serial: serial }
    }
    async fn build(dir: &Path, extra: &'static [(&'static str, &'static str)], auth: bool, time: Arc<Time>, forced: bool) -> Router {
        let file = if auth {
            Some(AuthFile {
                version: 1,
                password_hash: password::hash_password(PASSWORD.into()).await.unwrap(),
                must_change: forced,
                updated_at: 0,
            })
        } else {
            None
        };
        katalog_server::try_build_app(AppConfig { config: config(dir, extra, auth), auth: file, clock: time })
            .await
            .unwrap()
    }
    fn conn(&self) -> katalog_core::write::Connection {
        db::connect(&self.dir.join("catalog.db")).unwrap()
    }
    async fn login(&self) -> Session {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/login")
                    .header("host", "localhost:8080")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(r#"{{"password":"{PASSWORD}"}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let cookie = response.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_string();
        let body = json(response).await;
        Session { cookie, csrf: body["csrfToken"].as_str().unwrap().into() }
    }
    async fn command(&self, session: Option<&Session>, args: Value) -> Response {
        let mut req = Request::builder()
            .method("POST")
            .uri("/api/invoke/slicer_link")
            .header("host", "localhost:8080")
            .header("origin", "http://localhost:8080")
            .header("content-type", "application/json");
        if let Some(s) = session {
            req = req.header("cookie", &s.cookie).header("x-csrf-token", &s.csrf);
        }
        self.app.clone().oneshot(req.body(Body::from(args.to_string())).unwrap()).await.unwrap()
    }
    /// Link path for the fixture file.
    async fn link(&self, session: Option<&Session>) -> String {
        self.link_for(session, self.id).await
    }
    async fn link_for(&self, session: Option<&Session>, id: i64) -> String {
        let response = self.command(session, json!({"fileId": id.to_string()})).await;
        assert_eq!(response.status(), 200);
        json(response).await["path"].as_str().unwrap().to_string()
    }
    async fn get(&self, path: &str, method: &str, headers: &[(&str, &str)]) -> Response {
        let mut req = Request::builder().method(method).uri(path).header("host", "localhost:8080");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        self.app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn json(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap()
}
async fn bytes(response: Response) -> Vec<u8> {
    to_bytes(response.into_body(), 1 << 20).await.unwrap().to_vec()
}
fn replace_file(f: &Fixture, content: &[u8]) {
    let path = f.dir.join("models/Würfel groß.stl");
    let other = f.dir.join("models/replacement.tmp");
    std::fs::write(&other, content).unwrap();
    std::fs::rename(other, path).unwrap();
}

#[tokio::test]
async fn valid_link_serves_content_ranges_head_and_safe_headers() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let response = f.command(Some(&s), json!({"fileId": f.id.to_string()})).await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = json(response).await;
    let path = body["path"].as_str().unwrap();
    assert_eq!(body["expiresAt"], START + 3600);
    assert_eq!(body["name"], "Würfel groß.stl");
    assert!(path.starts_with(&format!("/dl/{}/", f.id)));
    assert!(path.ends_with("/Wurfel_gross.stl"), "{path}");

    // No cookie and no CSRF token: the signature is the only credential.
    let response = f.get(path, "GET", &[]).await;
    assert_eq!(response.status(), 200);
    let h = response.headers().clone();
    assert!(h.get("set-cookie").is_none());
    assert_eq!(h["cache-control"], "no-store");
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h["accept-ranges"], "bytes");
    assert_eq!(h["content-length"], DATA.len().to_string());
    assert_eq!(h["content-type"], "application/octet-stream");
    assert!(h["content-disposition"].to_str().unwrap().starts_with("attachment; filename=\""));
    assert!(h.get("last-modified").is_some());
    assert_eq!(bytes(response).await, DATA);

    let partial = f.get(path, "GET", &[("range", "bytes=2-5")]).await;
    assert_eq!(partial.status(), 206);
    assert_eq!(partial.headers()["content-range"], format!("bytes 2-5/{}", DATA.len()));
    assert_eq!(bytes(partial).await, b"2345");
    let suffix = f.get(path, "GET", &[("range", "bytes=-4")]).await;
    assert_eq!(bytes(suffix).await, b"wxyz");
    let unsatisfiable = f.get(path, "GET", &[("range", "bytes=900-")]).await;
    assert_eq!(unsatisfiable.status(), 416);
    assert_eq!(unsatisfiable.headers()["content-range"], format!("bytes */{}", DATA.len()));
    assert_eq!(f.get(path, "GET", &[("range", "bytes=5-2")]).await.status(), 200, "malformed range is ignored");

    let head = f.get(path, "HEAD", &[]).await;
    assert_eq!(head.status(), 200);
    assert_eq!(head.headers()["content-length"], DATA.len().to_string());
    assert!(bytes(head).await.is_empty());

    // The cookie is neither needed nor read: a session cookie changes nothing.
    let with_cookie = f.get(path, "GET", &[("cookie", &s.cookie)]).await;
    assert_eq!(with_cookie.status(), 200);
    assert!(with_cookie.headers().get("set-cookie").is_none());
    // The last segment only labels the download for the slicer.
    let renamed = path.rsplit_once('/').unwrap().0.to_string() + "/anything.stl";
    assert_eq!(f.get(&renamed, "GET", &[]).await.status(), 200);
}

#[tokio::test]
async fn only_get_and_head_are_accepted() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    for method in ["POST", "PUT", "DELETE"] {
        assert_eq!(f.get(&path, method, &[]).await.status(), 405, "{method}");
    }
}

#[tokio::test]
async fn expired_link_is_gone_and_does_not_extend() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    f.time.0.store(START + 3599, Ordering::SeqCst);
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 200);
    f.time.0.store(START + 3600, Ordering::SeqCst);
    let response = f.get(&path, "GET", &[]).await;
    assert_eq!(response.status(), 410);
    let text = json(response).await.to_string();
    assert!(!text.contains(&f.dir.to_string_lossy().to_string()));
    // Moving the expiry forward breaks the signature instead of extending the link.
    let moved = path.replacen(&format!("{}.", START + 3600), &format!("{}.", START + 9999), 1);
    f.time.0.store(START + 4000, Ordering::SeqCst);
    assert_eq!(f.get(&moved, "GET", &[]).await.status(), 404);
}

#[tokio::test]
async fn manipulated_or_foreign_signatures_look_like_unknown_files() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    let last = path.len() - path.rsplit('/').next().unwrap().len() - 2;
    let mut flipped = path.clone().into_bytes();
    flipped[last] = if flipped[last] == b'A' { b'B' } else { b'A' };
    let flipped = String::from_utf8(flipped).unwrap();

    // A second file with its own valid link: its signature must not open the first.
    let second_path = f.dir.join("models/second.stl");
    std::fs::write(&second_path, b"second").unwrap();
    let conn = f.conn();
    let second = db::test_insert_minimal_file(&conn, second_path.to_str().unwrap(), None).unwrap();
    drop(conn);
    let other = f.link_for(Some(&s), second).await;
    let token = other.split('/').nth(3).unwrap();
    let cross = format!("/dl/{}/{token}/x.stl", f.id);

    let unknown = f.get("/dl/99999/aaaa/x.stl", "GET", &[]).await;
    let unknown = (unknown.status(), json(unknown).await);
    for bad in [flipped, cross, "/dl/0%2f/x/y".into(), format!("/dl/{}/x/y.stl", f.id), format!("/dl/{}//y.stl", f.id), format!("/dl/007/{}/y.stl", path.split('/').nth(3).unwrap())] {
        let response = f.get(&bad, "GET", &[]).await;
        assert_eq!((response.status(), json(response).await), unknown, "{bad}");
    }
    assert_eq!(unknown.0, 404);
    // Extra segments are not part of the route.
    let deeper = format!("{path}/more");
    assert_eq!(f.get(&deeper, "GET", &[]).await.status(), 404);
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 200);
}

#[tokio::test]
async fn link_dies_when_the_file_changes_or_is_replaced() {
    let f = Fixture::new().await;
    let s = f.login().await;

    let path = f.link(Some(&s)).await;
    let file = std::fs::OpenOptions::new().write(true).open(f.dir.join("models/Würfel groß.stl")).unwrap();
    file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)).unwrap();
    drop(file);
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 404, "same bytes, other modification time");

    let path = f.link(Some(&s)).await;
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 200);
    replace_file(&f, DATA);
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 404, "same size and content, replaced file");

    let path = f.link(Some(&s)).await;
    std::fs::write(f.dir.join("models/Würfel groß.stl"), b"short").unwrap();
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 404, "changed content");

    // A fresh link for the new state works.
    let fresh = f.link(Some(&s)).await;
    assert_eq!(bytes(f.get(&fresh, "GET", &[]).await).await, b"short");
}

#[tokio::test]
async fn trash_missing_and_unknown_files_get_no_link_and_old_links_die() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    let trash = f.dir.join("models/trashed.stl");
    std::fs::write(&trash, b"x").unwrap();
    let conn = f.conn();
    let trashed = db::test_insert_minimal_file(&conn, trash.to_str().unwrap(), None).unwrap();
    let gone = db::test_insert_minimal_file(&conn, f.dir.join("models/missing.stl").to_str().unwrap(), None).unwrap();
    drop(conn);
    let linked = f.link_for(Some(&s), trashed).await;

    let conn = f.conn();
    db::soft_delete_file(&conn, trashed, Some(trash.to_str().unwrap()), "today").unwrap();
    drop(conn);
    for id in [trashed, gone, 424242] {
        assert_eq!(f.command(Some(&s), json!({"fileId": id.to_string()})).await.status(), 404, "{id}");
    }
    assert_eq!(f.get(&linked, "GET", &[]).await.status(), 404, "link of a file moved to the trash");

    std::fs::remove_file(f.dir.join("models/Würfel groß.stl")).unwrap();
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 404, "file deleted after minting");
}

#[cfg(unix)]
#[tokio::test]
async fn symlinks_and_paths_outside_the_roots_are_refused() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let outside = f.dir.join("outside.stl");
    std::fs::write(&outside, b"secret").unwrap();
    let link = f.dir.join("models/link.stl");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let conn = f.conn();
    let via_symlink = db::test_insert_minimal_file(&conn, link.to_str().unwrap(), None).unwrap();
    let direct = db::test_insert_minimal_file(&conn, outside.to_str().unwrap(), None).unwrap();
    drop(conn);
    for id in [via_symlink, direct] {
        assert_eq!(f.command(Some(&s), json!({"fileId": id.to_string()})).await.status(), 404);
    }
    // A regular file swapped for a symlink to an outside target after minting.
    let path = f.link(Some(&s)).await;
    let model = f.dir.join("models/Würfel groß.stl");
    std::fs::remove_file(&model).unwrap();
    std::os::unix::fs::symlink(&outside, &model).unwrap();
    let response = f.get(&path, "GET", &[]).await;
    assert_eq!(response.status(), 404);
    assert!(!bytes(response).await.windows(6).any(|w| w == b"secret"));
}

#[tokio::test]
async fn disabled_feature_has_no_route_no_command_and_no_capability() {
    let f = Fixture::with(&[], true).await;
    let s = f.login().await;
    assert_eq!(f.command(Some(&s), json!({"fileId": "1"})).await.status(), 404);
    assert_eq!(f.command(None, json!({"fileId": "1"})).await.status(), 404);
    for method in ["GET", "HEAD"] {
        assert_eq!(f.get("/dl/1/2.0123456789abcdef.x/a.stl", method, &[]).await.status(), 404, "{method}");
    }
    let session = f.get("/api/session", "GET", &[("cookie", &s.cookie)]).await;
    assert_eq!(json(session).await["slicerLinks"], false);
}

#[tokio::test]
async fn capability_is_reported_only_to_signed_in_sessions() {
    let f = Fixture::new().await;
    assert_eq!(json(f.get("/api/session", "GET", &[]).await).await["slicerLinks"], false);
    let s = f.login().await;
    assert_eq!(json(f.get("/api/session", "GET", &[("cookie", &s.cookie)]).await).await["slicerLinks"], true);
}

#[tokio::test]
async fn command_needs_a_session_csrf_and_valid_arguments() {
    let f = Fixture::new().await;
    assert_eq!(f.command(None, json!({"fileId": f.id.to_string()})).await.status(), 401);
    let s = f.login().await;
    let wrong_csrf = Session { cookie: s.cookie.clone(), csrf: "nope".into() };
    assert_eq!(f.command(Some(&wrong_csrf), json!({"fileId": f.id.to_string()})).await.status(), 403);
    for args in [json!({}), json!({"fileId": 1}), json!({"fileId": "x"}), json!({"fileId": "1", "extra": 1}), json!({"fileId": "-1"})] {
        assert_eq!(f.command(Some(&s), args.clone()).await.status(), 400, "{args}");
    }
    // The command works without write mode: this fixture is read-only.
    assert_eq!(f.command(Some(&s), json!({"fileId": f.id.to_string()})).await.status(), 200);
}

#[tokio::test]
async fn minting_is_limited_per_session() {
    let f = Fixture::new().await;
    let s = f.login().await;
    for _ in 0..60 {
        assert_eq!(f.command(Some(&s), json!({"fileId": f.id.to_string()})).await.status(), 200);
    }
    let response = f.command(Some(&s), json!({"fileId": f.id.to_string()})).await;
    assert_eq!(response.status(), 429);
    assert!(response.headers().contains_key("retry-after"));
    f.time.0.store(START + 61, Ordering::SeqCst);
    assert_eq!(f.command(Some(&s), json!({"fileId": f.id.to_string()})).await.status(), 200);
}

#[tokio::test]
async fn guessing_is_throttled_but_valid_links_of_the_same_address_keep_working() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    let guess = format!("/dl/{}/{}.0123456789abcdef.{}/a.stl", f.id, START + 100, "A".repeat(43));
    for n in 0..20 {
        assert_eq!(f.get(&guess, "GET", &[]).await.status(), 404, "{n}");
    }
    let throttled = f.get(&guess, "GET", &[]).await;
    assert_eq!(throttled.status(), 429);
    assert!(throttled.headers().contains_key("retry-after"));
    // Guessers must not be able to lock a user out from a shared address.
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 200);
    f.time.0.store(START + 61, Ordering::SeqCst);
    assert_eq!(f.get(&guess, "GET", &[]).await.status(), 404, "the window has passed");
}

#[tokio::test]
async fn a_password_change_invalidates_issued_links_but_logout_does_not() {
    let f = Fixture::new().await;
    let first = f.login().await;
    let second = f.login().await;
    let before = f.link(Some(&first)).await;
    let other = f.link(Some(&second)).await;

    let logout = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("host", "localhost:8080")
                .header("origin", "http://localhost:8080")
                .header("content-type", "application/json")
                .header("cookie", &second.cookie)
                .header("x-csrf-token", &second.csrf)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), 200);
    assert_eq!(f.get(&before, "GET", &[]).await.status(), 200, "logging out elsewhere leaves links alone");
    assert_eq!(f.get(&other, "GET", &[]).await.status(), 200, "even the one of the session that logged out");

    let change = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/password")
                .header("host", "localhost:8080")
                .header("origin", "http://localhost:8080")
                .header("content-type", "application/json")
                .header("cookie", &first.cookie)
                .header("x-csrf-token", &first.csrf)
                .body(Body::from(json!({"current": PASSWORD, "new": "Correct-horse-battery-9"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(change.status(), 200);
    let renewed = Session {
        cookie: change.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().into(),
        csrf: json(change).await["csrfToken"].as_str().unwrap().into(),
    };
    assert_eq!(f.get(&before, "GET", &[]).await.status(), 404);
    assert_eq!(f.get(&other, "GET", &[]).await.status(), 404);
    let fresh = f.link(Some(&renewed)).await;
    assert_eq!(bytes(f.get(&fresh, "GET", &[]).await).await, DATA);
}

#[tokio::test]
async fn at_most_four_downloads_run_at_once() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    let mut held = Vec::new();
    for _ in 0..4 {
        let response = f.get(&path, "GET", &[]).await;
        assert_eq!(response.status(), 200);
        held.push(response);
    }
    let fifth = f.get(&path, "GET", &[]).await;
    assert_eq!(fifth.status(), 503);
    assert!(fifth.headers().contains_key("retry-after"));
    held.pop();
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 200, "a finished download frees its slot");
}

#[tokio::test]
async fn pending_password_change_blocks_minting_but_not_the_public_route() {
    let f = Fixture::with_forced().await;
    let s = f.login().await;
    let response = f.command(Some(&s), json!({"fileId": f.id.to_string()})).await;
    assert_eq!(response.status(), 403);
    assert_eq!(json(response).await["code"], "password_change_required");
    assert_eq!(json(f.get("/api/session", "GET", &[("cookie", &s.cookie)]).await).await["slicerLinks"], false);
    // The route does not look at sessions at all; a bad link is a plain 404, not a 401/403.
    assert_eq!(f.get("/dl/1/2.0123456789abcdef.x/a.stl", "GET", &[]).await.status(), 404);
}

#[tokio::test]
async fn restoring_the_same_file_from_the_trash_revives_an_earlier_link() {
    // Documented limit: inode, size, modification time and hash are unchanged.
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    let model = f.dir.join("models/Würfel groß.stl");
    let trash = f.dir.join("models/trashed-copy.stl");
    std::fs::rename(&model, &trash).unwrap();
    let conn = f.conn();
    db::soft_delete_file(&conn, f.id, Some(trash.to_str().unwrap()), "today").unwrap();
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 404, "in the trash");
    std::fs::rename(&trash, &model).unwrap();
    conn.execute("UPDATE files SET deleted_at=NULL, trash_path=NULL WHERE id=?1", [f.id]).unwrap();
    drop(conn);
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 200);
}

#[tokio::test]
async fn only_listed_hosts_may_use_the_route() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    let response = f
        .app
        .clone()
        .oneshot(Request::builder().uri(&path).header("host", "evil.example").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 421);
}

#[tokio::test]
async fn restart_invalidates_links_because_the_key_is_new() {
    let f = Fixture::new().await;
    let s = f.login().await;
    let path = f.link(Some(&s)).await;
    let restarted = Fixture::build(&f.dir, &[("KATALOG_SLICER_LINKS", "1")], true, f.time.clone(), false).await;
    let response = restarted
        .oneshot(Request::builder().uri(&path).header("host", "localhost:8080").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    assert_eq!(f.get(&path, "GET", &[]).await.status(), 200);
}

#[tokio::test]
async fn no_login_mode_still_signs_and_serves() {
    let f = Fixture::with(&[("KATALOG_SLICER_LINKS", "1")], false).await;
    let path = f.link(None).await;
    assert_eq!(bytes(f.get(&path, "GET", &[]).await).await, DATA);
}

#[tokio::test]
async fn route_is_not_shadowed_by_the_single_page_fallback() {
    let on = Fixture::with(&[("KATALOG_SLICER_LINKS", "1"), ("STATIC", "")], true).await;
    let s = on.login().await;
    let path = on.link(Some(&s)).await;
    assert_eq!(bytes(on.get(&path, "GET", &[]).await).await, DATA);
    let unknown = on.get("/dl/1/bad/x.stl", "GET", &[]).await;
    assert_eq!(unknown.status(), 404);
    drop(on);
    let off = Fixture::with(&[("STATIC", "")], true).await;
    let response = off.get("/dl/1/bad/x.stl", "GET", &[]).await;
    assert_eq!(response.status(), 404, "an unconfigured route must not fall back to the app shell");
    assert!(!String::from_utf8_lossy(&bytes(response).await).contains("spa"));
}

#[test]
fn configuration_accepts_only_known_values() {
    let base = |extra: &'static [(&'static str, &'static str)]| {
        let dir = std::env::temp_dir();
        Config::from_lookup(move |k| match k {
            "KATALOG_DB" => Some(dir.join("x.db").to_str().unwrap().into()),
            "KATALOG_ROOTS" => Some(dir.to_str().unwrap().into()),
            other => extra.iter().find(|(key, _)| *key == other).map(|(_, v)| (*v).to_string()),
        })
    };
    assert_eq!(base(&[]).unwrap().slicer_links, None);
    assert_eq!(base(&[("KATALOG_SLICER_LINKS", "0")]).unwrap().slicer_links, None);
    assert_eq!(base(&[("KATALOG_SLICER_LINKS", "1")]).unwrap().slicer_links, Some(3600));
    assert_eq!(base(&[("KATALOG_SLICER_LINKS", "1"), ("KATALOG_SLICER_LINK_TTL_SECONDS", "60")]).unwrap().slicer_links, Some(60));
    assert_eq!(base(&[("KATALOG_SLICER_LINKS", "1"), ("KATALOG_SLICER_LINK_TTL_SECONDS", "86400")]).unwrap().slicer_links, Some(86400));
    for bad in ["yes", "true", "2", ""] {
        let extra: &'static [(&str, &str)] = Box::leak(Box::new([("KATALOG_SLICER_LINKS", Box::leak(bad.to_string().into_boxed_str()) as &str)]));
        assert!(base(extra).is_err(), "{bad:?}");
    }
    for ttl in ["59", "86401", "0", "-5", "1h", ""] {
        let extra: &'static [(&str, &str)] = Box::leak(Box::new([("KATALOG_SLICER_LINKS", "1"), ("KATALOG_SLICER_LINK_TTL_SECONDS", Box::leak(ttl.to_string().into_boxed_str()) as &str)]));
        let error = base(extra).err().unwrap();
        assert!(error.contains("KATALOG_SLICER_LINK_TTL_SECONDS"), "{ttl:?}: {error}");
    }
}
