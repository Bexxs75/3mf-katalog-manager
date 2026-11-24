//! Read-only comparison of the catalog with the disk in the web.
use axum::{body::{to_bytes, Body}, http::Request, Router};
use katalog_core::db;
use katalog_server::{build_app, try_build_app, config::Config, password::{self, AuthFile}, AppConfig, SystemClock, READ_COMMANDS};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::{atomic::{AtomicUsize, Ordering}, Arc}};
use tower::ServiceExt;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fx {
    app: Router,
    dir: PathBuf,
}
impl Fx {
    async fn new(auth: bool) -> Self {
        Self::build(auth, false).await
    }
    /// Signed-in fixture with the write mode on (needs auth and a host list).
    async fn writable() -> Self {
        Self::build(true, true).await
    }
    async fn build(auth: bool, write: bool) -> Self {
        let dir = std::env::temp_dir().join(format!("diskreport-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(dir.join("models/Foo_extras")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let models = dir.join("models");
        std::fs::write(models.join("Foo.3mf"), b"model").unwrap();
        std::fs::write(models.join("Foo_extras/pic.png"), b"png").unwrap();
        std::fs::write(models.join("Fresh.stl"), b"stl").unwrap();
        let conn = db::connect(&dir.join("catalog.db")).unwrap();
        let present = db::test_insert_minimal_file(&conn, models.join("Foo.3mf").to_str().unwrap(), None).unwrap();
        db::test_insert_minimal_file(&conn, models.join("Weg.3mf").to_str().unwrap(), None).unwrap();
        db::companions::insert_companion(&conn, present, "pic.png", models.join("Foo_extras/pic.png").to_str().unwrap(), "image", Some(3), "now").unwrap();
        db::companions::insert_companion(&conn, present, "lost.pdf", models.join("Foo_extras/lost.pdf").to_str().unwrap(), "pdf", Some(3), "now").unwrap();
        drop(conn);
        let config = Config::from_lookup(|k| match k {
            "KATALOG_DB" => Some(dir.join("catalog.db").to_str().unwrap().into()),
            "KATALOG_ROOTS" => Some(models.to_str().unwrap().into()),
            "KATALOG_ALLOW_NO_AUTH" if !auth => Some("1".into()),
            "KATALOG_WRITE" if write => Some("1".into()),
            "KATALOG_ALLOWED_HOSTS" if write => Some("localhost:8080".into()),
            _ => None,
        })
        .unwrap();
        // One hash for all fixtures: the password check admits only a few at a time.
        static HASH: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
        let hash = HASH.get_or_init(|| async { password::hash_password("report-test-password".into()).await.unwrap() }).await.clone();
        let auth = auth.then_some(AuthFile { version: 1, password_hash: hash, must_change: false, updated_at: 0 });
        let app_config = AppConfig { config, auth, clock: Arc::new(SystemClock) };
        let app = if write { try_build_app(app_config).await.unwrap() } else { build_app(app_config) };
        Self { app, dir }
    }
    async fn send(&self, req: Request<Body>) -> (u16, Value) {
        let r = self.app.clone().oneshot(req).await.unwrap();
        let status = r.status().as_u16();
        let headers = r.headers().clone();
        let body = to_bytes(r.into_body(), 1 << 20).await.unwrap();
        let mut value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        if let Some(c) = headers.get("set-cookie") {
            value["_cookie"] = c.to_str().unwrap().split(';').next().unwrap().into();
        }
        (status, value)
    }
    async fn invoke(&self, args: Value, session: Option<(&str, &str)>) -> (u16, Value) {
        self.call("disk_sync_report", args, session, None).await
    }
    async fn login(&self) -> (String, String) {
        let (status, login) = self
            .send(Request::builder().method("POST").uri("/api/login").header("host", "localhost:8080").header("origin", "http://localhost:8080").header("content-type", "application/json").body(Body::from(r#"{"password":"report-test-password"}"#)).unwrap())
            .await;
        assert_eq!(status, 200, "{login}");
        (login["_cookie"].as_str().unwrap().to_owned(), login["csrfToken"].as_str().unwrap().to_owned())
    }
    async fn call(&self, command: &str, args: Value, session: Option<(&str, &str)>, request_id: Option<&str>) -> (u16, Value) {
        let mut req = Request::builder().method("POST").uri(format!("/api/invoke/{command}")).header("origin", "http://localhost:8080").header("sec-fetch-site", "same-origin").header("host", "localhost:8080").header("content-type", "application/json");
        if let Some((cookie, csrf)) = session {
            req = req.header("cookie", cookie).header("x-csrf-token", csrf);
        }
        if let Some(id) = request_id {
            req = req.header("x-request-id", id);
        }
        self.send(req.body(Body::from(args.to_string())).unwrap()).await
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn the_command_is_on_the_read_allowlist() {
    assert!(READ_COMMANDS.contains(&"disk_sync_report"));
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_a_session() {
    let f = Fx::new(true).await;
    assert_eq!(f.invoke(json!({}), None).await.0, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_a_logged_in_session() {
    let f = Fx::new(true).await;
    let (status, login) = f
        .send(Request::builder().method("POST").uri("/api/login").header("host", "localhost:8080").header("content-type", "application/json").body(Body::from(r#"{"password":"report-test-password"}"#)).unwrap())
        .await;
    assert_eq!(status, 200);
    let cookie = login["_cookie"].as_str().unwrap().to_owned();
    let csrf = login["csrfToken"].as_str().unwrap().to_owned();
    let (status, report) = f.invoke(json!({}), Some((&cookie, &csrf))).await;
    assert_eq!(status, 200);
    assert_eq!(report["missingModels"][0]["relativePath"], "Weg.3mf");
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_relative_paths_only_and_changes_nothing() {
    let f = Fx::new(false).await;
    let db_before = Sha256::digest(std::fs::read(f.dir.join("catalog.db")).unwrap());
    let names = |d: &PathBuf| {
        let mut n: Vec<_> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name()).collect();
        n.sort();
        n
    };
    let (dir_before, models_before) = (names(&f.dir), names(&f.dir.join("models")));
    let (status, report) = f.invoke(json!({}), None).await;
    assert_eq!(status, 200);
    assert_eq!(report["missingModels"].as_array().unwrap().len(), 1);
    assert_eq!(report["missingModels"][0]["relativePath"], "Weg.3mf");
    assert_eq!(report["newFiles"].as_array().unwrap().len(), 1);
    assert_eq!(report["newFiles"][0]["relativePath"], "Fresh.stl");
    assert_eq!(report["danglingCompanions"][0]["relativePath"], "Foo_extras/lost.pdf");
    assert_eq!(report["danglingCompanions"][0]["reason"], "missing");
    assert_eq!(report["truncated"], false);
    assert_eq!(report["roots"][0]["name"], "models");
    assert!(!report.to_string().contains(f.dir.to_str().unwrap()), "{report}");
    assert_eq!(Sha256::digest(std::fs::read(f.dir.join("catalog.db")).unwrap()), db_before);
    assert_eq!((names(&f.dir), names(&f.dir.join("models"))), (dir_before, models_before));
}

#[tokio::test(flavor = "multi_thread")]
async fn takes_no_arguments() {
    let f = Fx::new(false).await;
    assert_eq!(f.invoke(json!({"path": "/etc"}), None).await.0, 422);
}

const NEW_COMMANDS_READ: [&str; 4] = ["scan_catalog_issues", "get_sync_on_start", "get_sync_auto_add", "run_start_sync"];

#[test]
fn the_new_commands_are_listed_and_classified_as_web() {
    let classes: Value = serde_json::from_str(include_str!("../../../docs/web/command-classes.json")).unwrap();
    for command in NEW_COMMANDS_READ {
        assert!(READ_COMMANDS.contains(&command), "{command}");
        assert!(!katalog_server::WRITE_COMMANDS.contains(&command), "{command}");
        assert_eq!(classes["commands"][command], "web", "{command}");
    }
    for command in ["set_sync_on_start", "set_sync_auto_add"] {
        assert!(katalog_server::WRITE_COMMANDS.contains(&command), "{command}");
        assert!(!READ_COMMANDS.contains(&command), "{command}");
        assert_eq!(classes["commands"][command], "web", "{command}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_new_reads_need_a_session() {
    let f = Fx::new(true).await;
    for command in NEW_COMMANDS_READ {
        assert_eq!(f.call(command, json!({}), None, None).await.0, 401, "{command}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_catalog_issues_lists_the_missing_model_by_the_file_roots_only() {
    let f = Fx::new(false).await;
    let (status, issues) = f.call("scan_catalog_issues", json!({}), None, None).await;
    assert_eq!(status, 200, "{issues}");
    assert_eq!(issues["truncated"], false);
    assert_eq!(issues["duplicateGroups"], json!([]));
    assert_eq!(issues["orphaned"].as_array().unwrap().len(), 1);
    assert_eq!(issues["orphaned"][0]["name"], "Weg.3mf");
    assert_eq!(f.call("scan_catalog_issues", json!({"x": 1}), None, None).await.0, 422);
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_catalog_issues_ignores_a_model_outside_the_file_roots() {
    let f = Fx::new(false).await;
    let conn = db::connect(&f.dir.join("catalog.db")).unwrap();
    db::test_insert_minimal_file(&conn, f.dir.join("elsewhere/gone.3mf").to_str().unwrap(), None).unwrap();
    drop(conn);
    let (_, issues) = f.call("scan_catalog_issues", json!({}), None, None).await;
    let names: Vec<_> = issues["orphaned"].as_array().unwrap().iter().map(|o| o["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(names, ["Weg.3mf"], "unclear rows are not listed as missing");
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_settings_roundtrip_independently_and_replay_a_request_id() {
    let f = Fx::writable().await;
    let (cookie, csrf) = f.login().await;
    let s = Some((cookie.as_str(), csrf.as_str()));
    for command in ["get_sync_on_start", "get_sync_auto_add"] {
        assert_eq!(f.call(command, json!({}), s, None).await, (200, json!(false)), "{command}");
    }
    let r1 = "11111111-1111-4111-8111-111111111111";
    let r2 = "22222222-2222-4222-8222-222222222222";
    assert_eq!(f.call("set_sync_auto_add", json!({"enabled": true}), s, Some(r1)).await.0, 200);
    assert_eq!(f.call("get_sync_auto_add", json!({}), s, None).await.1, json!(true));
    // Stored without the start switch: the server keeps the keys apart.
    assert_eq!(f.call("get_sync_on_start", json!({}), s, None).await.1, json!(false));
    assert_eq!(f.call("set_sync_on_start", json!({"enabled": true}), s, None).await.0, 200);
    assert_eq!(f.call("get_sync_on_start", json!({}), s, None).await.1, json!(true));
    assert_eq!(f.call("set_sync_auto_add", json!({"enabled": false}), s, Some(r2)).await.0, 200);
    assert_eq!(f.call("get_sync_auto_add", json!({}), s, None).await.1, json!(false));
    // The first request again: the stored answer comes back, nothing is written a second time.
    assert_eq!(f.call("set_sync_auto_add", json!({"enabled": true}), s, Some(r1)).await.0, 200);
    assert_eq!(f.call("get_sync_auto_add", json!({}), s, None).await.1, json!(false));
    // The same keys the desktop reads, so an exported catalog keeps the choice.
    let conn = db::connect(&f.dir.join("catalog.db")).unwrap();
    assert_eq!(katalog_core::disk_sync_settings::flag(&conn, "sync_on_start"), Ok(true));
    assert_eq!(katalog_core::disk_sync_settings::flag(&conn, "sync_auto_add"), Ok(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_settings_reject_bad_arguments_and_need_session_csrf_and_write_mode() {
    let f = Fx::writable().await;
    let (cookie, csrf) = f.login().await;
    let s = Some((cookie.as_str(), csrf.as_str()));
    for args in [json!({}), json!({"enabled": "1"}), json!({"enabled": 1}), json!({"enabled": true, "x": 1}), json!([true])] {
        for command in ["set_sync_on_start", "set_sync_auto_add"] {
            assert_eq!(f.call(command, args.clone(), s, None).await.0, 400, "{command} {args}");
        }
    }
    assert_eq!(f.call("set_sync_on_start", json!({"enabled": true}), None, None).await.0, 401);
    assert_eq!(f.call("set_sync_on_start", json!({"enabled": true}), Some((cookie.as_str(), "wrong")), None).await.0, 403);
    assert_eq!(f.call("get_sync_on_start", json!({}), s, None).await.1, json!(false), "nothing was stored");
    // Without the write mode the setting commands do not exist.
    let ro = Fx::new(false).await;
    assert_eq!(ro.call("set_sync_on_start", json!({"enabled": true}), None, None).await.0, 404);
    assert_eq!(ro.call("get_sync_on_start", json!({}), None, None).await, (200, json!(false)));
}

#[tokio::test(flavor = "multi_thread")]
async fn run_start_sync_without_file_operations_says_it_did_not_run() {
    let f = Fx::new(false).await;
    let (status, state) = f.call("run_start_sync", json!({}), None, None).await;
    assert_eq!(status, 200);
    assert_eq!((state["ran"].clone(), state["busy"].clone(), state["state"].clone()), (json!(false), json!(false), json!("off")));
    assert_eq!(state["applied"], Value::Null);
    assert_eq!(f.call("run_start_sync", json!({"x": 1}), None, None).await.0, 422);
}
