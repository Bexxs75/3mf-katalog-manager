//! Read-only comparison of the catalog with the disk in the web.
use axum::{body::{to_bytes, Body}, http::Request, Router};
use katalog_core::db;
use katalog_server::{build_app, config::Config, password::{self, AuthFile}, AppConfig, SystemClock, READ_COMMANDS};
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
            _ => None,
        })
        .unwrap();
        let auth = auth.then(|| AuthFile {
            version: 1,
            password_hash: tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(password::hash_password("report-test-password".into()))).unwrap(),
            must_change: false,
            updated_at: 0,
        });
        Self { app: build_app(AppConfig { config, auth, clock: Arc::new(SystemClock) }), dir }
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
        let mut req = Request::builder().method("POST").uri("/api/invoke/disk_sync_report").header("host", "localhost:8080").header("content-type", "application/json");
        if let Some((cookie, csrf)) = session {
            req = req.header("cookie", cookie).header("x-csrf-token", csrf);
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
