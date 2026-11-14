use axum::{
    body::{to_bytes, Body},
    http::Request,
    response::Response,
    Router,
};
use http_body_util::BodyExt;
use katalog_core::db;
use katalog_server::{
    config::Config,
    password::{self, AuthFile},
    AppConfig, Clock,
};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};
use tower::ServiceExt;

// Password hashing admits few parallel jobs; tests take turns.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static NEXT: AtomicUsize = AtomicUsize::new(0);
const PASSWORD: &str = "export-test-password";

struct Time(AtomicU64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Fixture {
    app: Router,
    dir: PathBuf,
    time: Arc<Time>,
    digest: Option<Vec<u8>>,
    _serial: tokio::sync::MutexGuard<'static, ()>,
}
impl Fixture {
    async fn new() -> Self {
        Self::with(true, false, false, |_| {}).await
    }
    async fn with(auth: bool, forced: bool, write: bool, prepare: impl FnOnce(&PathBuf)) -> Self {
        let serial = SERIAL.lock().await;
        let dir = std::env::temp_dir().join(format!(
            "export-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::connect(&dir.join("catalog.db")).unwrap();
        for name in ["/models/a.stl", "/models/b.stl", "/models/c.stl"] {
            db::test_insert_minimal_file(&conn, name, None).unwrap();
        }
        drop(conn);
        prepare(&dir);
        let config = Config::from_lookup(|k| match k {
            "KATALOG_DB" => Some(dir.join("catalog.db").to_str().unwrap().into()),
            "KATALOG_ROOTS" => Some(dir.to_str().unwrap().into()),
            "KATALOG_WRITE" if write => Some("1".into()),
            "KATALOG_ALLOWED_HOSTS" if write => Some("localhost:8080".into()),
            "KATALOG_ALLOW_NO_AUTH" if !auth => Some("1".into()),
            _ => None,
        })
        .unwrap();
        let auth = if auth {
            Some(AuthFile {
                version: 1,
                password_hash: password::hash_password(PASSWORD.into()).await.unwrap(),
                must_change: forced,
                updated_at: 0,
            })
        } else {
            None
        };
        let time = Arc::new(Time(AtomicU64::new(1_793_000_000)));
        let app = katalog_server::try_build_app(AppConfig { config, auth, clock: time.clone() }).await.unwrap();
        // A write session legitimately changes the catalog file.
        let digest = (!write).then(|| Sha256::digest(std::fs::read(dir.join("catalog.db")).unwrap()).to_vec());
        Self { app, dir, time, digest, _serial: serial }
    }
    async fn get(&self, cookie: Option<&str>) -> Response {
        let mut req = Request::builder().uri("/api/backup/export").header("host", "localhost:8080");
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        self.app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
    }
    async fn login(&self) -> String {
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
        response.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().into()
    }
    fn leftovers(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.dir.join("backups")) else { return Vec::new() };
        entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let (false, Some(digest)) = (std::thread::panicking(), &self.digest) {
            assert_eq!(
                digest,
                &Sha256::digest(std::fs::read(self.dir.join("catalog.db")).unwrap()).to_vec(),
                "an export must not change the catalog file"
            );
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn all_bytes(r: Response) -> Vec<u8> {
    to_bytes(r.into_body(), 512 * 1024 * 1024).await.unwrap().to_vec()
}

#[tokio::test]
async fn export_requires_a_session_and_creates_nothing_without_one() {
    let f = Fixture::new().await;
    let r = f.get(None).await;
    assert_eq!(r.status(), 401);
    assert_eq!(f.leftovers(), Vec::<String>::new());
    assert!(!f.dir.join("backups").exists());
    let r = f.get(Some("katalog_session=forged")).await;
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn export_is_blocked_until_the_forced_password_change_is_done() {
    let f = Fixture::with(true, true, false, |_| {}).await;
    let cookie = f.login().await;
    assert_eq!(f.get(Some(&cookie)).await.status(), 403);
}

#[tokio::test]
async fn export_in_read_only_mode_returns_a_zip_with_desktop_layout() {
    let f = Fixture::new().await;
    let cookie = f.login().await;
    let r = f.get(Some(&cookie)).await;
    assert_eq!(r.status(), 200);
    let h = r.headers().clone();
    assert_eq!(h["content-type"], "application/zip");
    assert_eq!(h["content-disposition"], "attachment; filename=\"3mf-katalog-sicherung-20261026.zip\"");
    assert_eq!(h["cache-control"], "no-store");
    assert_eq!(h["x-content-type-options"], "nosniff");
    let body = all_bytes(r).await;
    assert_eq!(h["content-length"].to_str().unwrap(), body.len().to_string());

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(body)).unwrap();
    assert_eq!(archive.len(), 2);
    let mut database = Vec::new();
    archive.by_name("catalog.db").unwrap().read_to_end(&mut database).unwrap();
    let mut settings = String::new();
    archive.by_name("settings.json").unwrap().read_to_string(&mut settings).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&settings).unwrap().is_object());

    let copy = f.dir.join("restored.db");
    std::fs::write(&copy, &database).unwrap();
    let conn = rusqlite::Connection::open(&copy).unwrap();
    assert_eq!(conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).unwrap(), "ok");
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(),
        db::migrations_current_version()
    );
    drop(conn);
    // Nothing of the export stays in the work directory once the body was read.
    assert_eq!(f.leftovers(), Vec::<String>::new());
}

#[tokio::test]
async fn export_works_without_login_only_in_the_loopback_no_auth_mode() {
    let f = Fixture::with(false, false, false, |_| {}).await;
    let r = f.get(None).await;
    assert_eq!(r.status(), 200);
    assert!(all_bytes(r).await.starts_with(b"PK"));
}

#[tokio::test]
async fn second_export_is_refused_while_one_is_streaming_and_abort_frees_everything() {
    let f = Fixture::new().await;
    let first = f.login().await;
    let second = f.login().await;
    let held = f.get(Some(&first)).await;
    assert_eq!(held.status(), 200);
    // Another session: the single slot is taken.
    let busy = f.get(Some(&second)).await;
    assert_eq!(busy.status(), 503);
    assert!(busy.headers().contains_key("retry-after"));
    // Same session: the interval applies first.
    let again = f.get(Some(&first)).await;
    assert_eq!(again.status(), 429);
    let wait: u64 = again.headers()["retry-after"].to_str().unwrap().parse().unwrap();
    assert!((1..=30).contains(&wait));
    // The archive name is already gone while the client is still reading.
    assert_eq!(f.leftovers(), Vec::<String>::new());

    let mut body = held.into_body();
    let frame = body.frame().await.unwrap().unwrap();
    assert!(frame.into_data().unwrap().starts_with(b"PK"));
    drop(body);
    assert_eq!(f.get(Some(&second)).await.status(), 200);
    assert_eq!(f.leftovers(), Vec::<String>::new());
}

#[tokio::test]
async fn export_interval_is_per_session_and_expires() {
    let f = Fixture::new().await;
    let cookie = f.login().await;
    drop(f.get(Some(&cookie)).await);
    assert_eq!(f.get(Some(&cookie)).await.status(), 429);
    f.time.0.fetch_add(31, Ordering::SeqCst);
    // The session outlives the interval: it expires after two idle hours, at most 12 h (Max-Age 43200).
    assert_eq!(f.get(Some(&cookie)).await.status(), 200);
}

#[tokio::test]
async fn export_rejects_other_methods_hosts_and_unknown_siblings() {
    let f = Fixture::new().await;
    let cookie = f.login().await;
    let post = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backup/export")
                .header("host", "localhost:8080")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post.status(), 405);
    let sibling = f
        .app
        .clone()
        .oneshot(Request::builder().uri("/api/backup/other").header("host", "localhost:8080").header("cookie", &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(sibling.status(), 404);
}

#[tokio::test]
async fn export_failure_is_generic_and_leaves_no_files() {
    // A regular file where the work directory belongs: preparation must fail.
    let f = Fixture::with(true, false, false, |dir| std::fs::write(dir.join("backups"), b"x").unwrap()).await;
    let cookie = f.login().await;
    let r = f.get(Some(&cookie)).await;
    assert_eq!(r.status(), 500);
    let text = String::from_utf8(all_bytes(r).await).unwrap();
    assert!(!text.contains(f.dir.to_str().unwrap()) && !text.contains("backups"), "{text}");
    assert!(text.contains("Die Sicherung konnte nicht erstellt werden"));
}

#[tokio::test]
async fn startup_sweeps_only_old_export_temp_files() {
    let f = Fixture::with(true, false, false, |dir| {
        std::fs::create_dir(dir.join("backups")).unwrap();
        std::fs::set_permissions(dir.join("backups"), std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        std::fs::write(dir.join("backups/.web-export-db.1-2-3.tmp"), b"stale").unwrap();
        std::fs::write(dir.join("backups/.web-export-zip.1-2-3.tmp"), b"fresh").unwrap();
        let old = std::fs::OpenOptions::new().write(true).open(dir.join("backups/.web-export-db.1-2-3.tmp")).unwrap();
        old.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600)).unwrap();
        std::fs::write(dir.join("backups/web-fileops-day-1.db"), b"keep").unwrap();
    })
    .await;
    let mut left = f.leftovers();
    left.sort();
    assert_eq!(left, vec![".web-export-zip.1-2-3.tmp".to_string(), "web-fileops-day-1.db".to_string()]);
}

#[tokio::test]
async fn export_also_works_in_write_mode_and_does_not_need_it() {
    let f = Fixture::with(true, false, true, |_| {}).await;
    let cookie = f.login().await;
    let r = f.get(Some(&cookie)).await;
    assert_eq!(r.status(), 200);
    assert!(all_bytes(r).await.starts_with(b"PK"));
    assert_eq!(f.leftovers(), Vec::<String>::new());
}

#[tokio::test]
async fn cancelled_request_still_releases_slot_and_temp_files() {
    let f = Fixture::new().await;
    let first = f.login().await;
    let second = f.login().await;
    // The client gives up right away; preparation may still be running.
    let abandoned = tokio::time::timeout(std::time::Duration::from_millis(1), f.get(Some(&first))).await;
    drop(abandoned);
    let mut status = 0;
    for _ in 0..200 {
        status = f.get(Some(&second)).await.status().as_u16();
        if status == 200 {
            break;
        }
        assert_eq!(status, 503, "only the slot may block");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        f.time.0.fetch_add(1, Ordering::SeqCst);
    }
    assert_eq!(status, 200);
    assert_eq!(f.leftovers(), Vec::<String>::new());
}

fn post_favorite(cookie: &str, csrf: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/invoke/set_favorite")
        .header("host", "localhost:8080")
        .header("origin", "http://localhost:8080")
        .header("content-type", "application/json")
        .header("cookie", cookie)
        .header("x-csrf-token", csrf)
        .body(Body::from(r#"{"fileId":"1","favorite":true}"#))
        .unwrap()
}

#[tokio::test]
async fn export_of_a_large_catalog_does_not_make_concurrent_writes_fail() {
    // The catalog runs in rollback-journal mode (no WAL): a copy under a plain read lock
    // would make committers fail with BUSY, which surfaces as a 500.
    let f = Fixture::with(true, false, true, |dir| {
        let conn = rusqlite::Connection::open(dir.join("catalog.db")).unwrap();
        assert_eq!(conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0)).unwrap().to_lowercase(), "delete");
        for index in 0..14 {
            db::test_insert_minimal_file(&conn, &format!("/models/big{index}.stl"), None).unwrap();
        }
        conn.execute("UPDATE files SET thumbnail_png = zeroblob(7 * 1024 * 1024)", []).unwrap();
    })
    .await;
    let cookie = f.login().await;
    let session = f
        .app
        .clone()
        .oneshot(Request::builder().uri("/api/session").header("host", "localhost:8080").header("cookie", &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let csrf = serde_json::from_slice::<serde_json::Value>(&all_bytes(session).await).unwrap()["csrfToken"].as_str().unwrap().to_owned();
    assert!(std::fs::metadata(f.dir.join("catalog.db")).unwrap().len() > 100 * 1024 * 1024);

    let app = f.app.clone();
    let export_cookie = cookie.clone();
    let export = tokio::spawn(async move {
        let response = app
            .oneshot(Request::builder().uri("/api/backup/export").header("host", "localhost:8080").header("cookie", export_cookie).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        (status, all_bytes(response).await)
    });
    let mut statuses = Vec::new();
    while !export.is_finished() {
        let response = f.app.clone().oneshot(post_favorite(&cookie, &csrf)).await.unwrap();
        statuses.push(response.status().as_u16());
        // The write bucket refills with the (fake) clock.
        f.time.0.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let (status, body) = export.await.unwrap();
    assert_eq!(status, 200);
    assert!(!statuses.is_empty());
    assert!(statuses.iter().all(|s| matches!(s, 200 | 503)), "{statuses:?}");
    assert!(statuses.contains(&200), "{statuses:?}");
    // Writes work again right after the export.
    assert_eq!(f.app.clone().oneshot(post_favorite(&cookie, &csrf)).await.unwrap().status(), 200);

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(body)).unwrap();
    let copy = f.dir.join("restored.db");
    std::io::copy(&mut archive.by_name("catalog.db").unwrap(), &mut std::fs::File::create(&copy).unwrap()).unwrap();
    let conn = rusqlite::Connection::open(&copy).unwrap();
    assert_eq!(conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).unwrap(), "ok");
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0)).unwrap(), 17);
    assert_eq!(f.leftovers(), Vec::<String>::new());
}
