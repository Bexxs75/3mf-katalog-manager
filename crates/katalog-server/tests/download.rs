use axum::{
    body::{to_bytes, Body},
    http::Request,
    response::Response,
    Router,
};
use katalog_core::db;
use katalog_server::{
    build_app,
    config::Config,
    password::{self, AuthFile},
    AppConfig, SystemClock,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tower::ServiceExt;
static NEXT: AtomicUsize = AtomicUsize::new(0);
const DATA: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
struct Fixture {
    app: Router,
    dir: PathBuf,
    id: i64,
    before: Vec<u8>,
}
impl Fixture {
    async fn new(auth: bool, forced: bool) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "download-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        let path = dir.join("models/model.step");
        std::fs::write(&path, DATA).unwrap();
        let conn = db::connect(&dir.join("catalog.db")).unwrap();
        let id = db::test_insert_minimal_file(&conn, path.to_str().unwrap(), None).unwrap();
        conn.execute("UPDATE files SET name=?1", ["..Mödel\"/\\\r\n.step"])
            .unwrap();
        let trash = db::test_insert_minimal_file(&conn, "/missing/original.step", None).unwrap();
        db::soft_delete_file(&conn, trash, Some(path.to_str().unwrap()), "today").unwrap();
        let outside = dir.join("outside.step");
        std::fs::write(&outside, DATA).unwrap();
        db::test_insert_minimal_file(&conn, outside.to_str().unwrap(), None).unwrap();
        drop(conn);
        let config = Config::from_lookup(|k| match k {
            "KATALOG_DB" => Some(dir.join("catalog.db").to_str().unwrap().into()),
            "KATALOG_ROOTS" => Some(dir.join("models").to_str().unwrap().into()),
            "KATALOG_ALLOW_NO_AUTH" if !auth => Some("1".into()),
            _ => None,
        })
        .unwrap();
        let auth = if auth {
            Some(AuthFile {
                version: 1,
                password_hash: password::hash_password("download-test-password".into())
                    .await
                    .unwrap(),
                must_change: forced,
                updated_at: 0,
            })
        } else {
            None
        };
        let app = build_app(AppConfig {
            config,
            auth,
            clock: Arc::new(SystemClock),
        });
        let before = std::fs::read(dir.join("catalog.db")).unwrap();
        Self {
            app,
            dir,
            id,
            before,
        }
    }
    async fn request(
        &self,
        id: &str,
        range: Option<&str>,
        cookie: Option<&str>,
        method: &str,
    ) -> Response {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/files/{id}/download"))
            .header("host", "localhost:8080");
        if let Some(r) = range {
            req = req.header("range", r);
        }
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        self.app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
    async fn get(&self, range: Option<&str>) -> Response {
        self.request(&self.id.to_string(), range, None, "GET").await
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
                    .body(Body::from(r#"{"password":"download-test-password"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .into()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert_eq!(
                self.before,
                std::fs::read(self.dir.join("catalog.db")).unwrap(),
                "catalog end state changed"
            );
        }
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}
async fn bytes(r: Response) -> Vec<u8> {
    to_bytes(r.into_body(), 100_000).await.unwrap().to_vec()
}
#[tokio::test]
async fn download_requires_session_and_password_change() {
    let f = Fixture::new(true, false).await;
    assert_eq!(f.get(None).await.status(), 401);
    let cookie = f.login().await;
    let r = f
        .request(&f.id.to_string(), None, Some(&cookie), "GET")
        .await;
    assert_eq!(r.status(), 200);
    assert_eq!(bytes(r).await, DATA);
    let f = Fixture::new(true, true).await;
    let cookie = f.login().await;
    let r = f
        .request(&f.id.to_string(), None, Some(&cookie), "GET")
        .await;
    assert_eq!(r.status(), 403);
    let v: serde_json::Value = serde_json::from_slice(&bytes(r).await).unwrap();
    assert_eq!(v["code"], "password_change_required");
}
#[tokio::test]
async fn download_headers_and_original_bytes() {
    let f = Fixture::new(false, false).await;
    let r = f.get(None).await;
    assert_eq!(r.status(), 200);
    for (name, value) in [
        ("content-type", "application/octet-stream"),
        ("content-length", "36"),
        ("x-content-type-options", "nosniff"),
        ("cache-control", "no-store"),
        ("accept-ranges", "bytes"),
    ] {
        assert_eq!(r.headers()[name], value);
    }
    assert_eq!(
        r.headers()["content-disposition"],
        "attachment; filename=\"M_del.step\"; filename*=UTF-8''M%C3%B6del.step"
    );
    assert_eq!(bytes(r).await, DATA);
}
#[tokio::test]
async fn download_ranges() {
    let f = Fixture::new(false, false).await;
    for (range, expected, content_range) in [
        ("bytes=0-9", &DATA[..10], "bytes 0-9/36"),
        ("bytes=5-", &DATA[5..], "bytes 5-35/36"),
        ("bytes=-5", &DATA[31..], "bytes 31-35/36"),
        ("bytes=30-999", &DATA[30..], "bytes 30-35/36"),
        ("bytes=0-18446744073709551616", DATA, "bytes 0-35/36"),
        ("bytes=-18446744073709551616", DATA, "bytes 0-35/36"),
    ] {
        let r = f.get(Some(range)).await;
        assert_eq!(r.status(), 206, "{range}");
        assert_eq!(r.headers()["content-range"], content_range);
        assert_eq!(bytes(r).await, expected);
    }
    for range in ["bytes=36-", "bytes=-0", "bytes=18446744073709551616-"] {
        let r = f.get(Some(range)).await;
        assert_eq!(r.status(), 416, "{range}");
        assert_eq!(r.headers()["content-range"], "bytes */36");
    }
    let r = f.get(Some("bytes=0-2,5-9")).await;
    assert_eq!(r.status(), 200);
    assert_eq!(bytes(r).await, DATA);
}
#[tokio::test]
async fn download_missing_invalid_id_trash_and_methods() {
    let f = Fixture::new(false, false).await;
    assert_eq!(f.request("oops", None, None, "GET").await.status(), 400);
    assert_eq!(
        f.request("9223372036854775808", None, None, "GET")
            .await
            .status(),
        400
    );
    assert_eq!(f.request("999999", None, None, "GET").await.status(), 404);
    assert_eq!(f.request("1", None, None, "HEAD").await.status(), 405);
    assert_eq!(f.request("1", None, None, "POST").await.status(), 405);
    assert_eq!(bytes(f.request("2", None, None, "GET").await).await, DATA);
    std::fs::remove_file(f.dir.join("models/model.step")).unwrap();
    let r = f.get(None).await;
    assert_eq!(r.status(), 404);
    let v: serde_json::Value = serde_json::from_slice(&bytes(r).await).unwrap();
    assert_eq!(v["expected"], true);
}
#[tokio::test]
async fn download_rejects_directory() {
    let f = Fixture::new(false, false).await;
    let path = f.dir.join("models/model.step");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(f.get(None).await.status(), 500);
}
#[cfg(unix)]
#[tokio::test]
async fn download_rejects_symlink_escape_and_fifo() {
    let f = Fixture::new(false, false).await;
    let path = f.dir.join("models/model.step");
    std::fs::remove_file(&path).unwrap();
    std::fs::write(f.dir.join("outside"), DATA).unwrap();
    std::os::unix::fs::symlink(f.dir.join("outside"), &path).unwrap();
    let r = f.get(None).await;
    assert_eq!(r.status(), 422);
    let v: serde_json::Value = serde_json::from_slice(&bytes(r).await).unwrap();
    assert_eq!(v["expected"], true);
    std::fs::remove_file(&path).unwrap();
    assert!(std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    let r = tokio::time::timeout(std::time::Duration::from_secs(2), f.get(None))
        .await
        .unwrap();
    assert_eq!(r.status(), 500);
}
#[tokio::test]
async fn download_limit_releases_on_abort_and_completion() {
    let f = Fixture::new(false, false).await;
    let mut held = Vec::new();
    for _ in 0..4 {
        let r = f.get(None).await;
        assert_eq!(r.status(), 200);
        held.push(r);
    }
    let r = f.get(None).await;
    assert_eq!(r.status(), 503);
    assert_eq!(r.headers()["retry-after"], "1");
    drop(held.pop());
    let r = f.get(None).await;
    assert_eq!(r.status(), 200);
    assert_eq!(bytes(r).await, DATA);
    assert_eq!(f.get(None).await.status(), 200);
}
#[tokio::test]
async fn download_does_not_send_appended_bytes() {
    use std::io::Write;
    let f = Fixture::new(false, false).await;
    let r = f.get(None).await;
    assert_eq!(r.status(), 200);
    std::fs::OpenOptions::new()
        .append(true)
        .open(f.dir.join("models/model.step"))
        .unwrap()
        .write_all(b"APPENDED")
        .unwrap();
    assert_eq!(bytes(r).await, DATA);
}

#[tokio::test]
async fn download_empty_file_and_suffix_larger_than_file() {
    let f = Fixture::new(false, false).await;
    let r = f.get(Some("bytes=-999")).await;
    assert_eq!(r.status(), 206);
    assert_eq!(bytes(r).await, DATA);
    std::fs::write(f.dir.join("models/model.step"), []).unwrap();
    let r = f.get(None).await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-length"], "0");
    assert!(bytes(r).await.is_empty());
    let r = f.get(Some("bytes=0-")).await;
    assert_eq!(r.status(), 416);
    assert_eq!(r.headers()["content-range"], "bytes */0");
}

#[tokio::test]
async fn download_partial_stream_abort_and_read_error_release_slots() {
    use http_body_util::BodyExt;
    let f = Fixture::new(false, false).await;
    let path = f.dir.join("models/model.step");
    std::fs::write(&path, vec![42; 256 * 1024]).unwrap();
    let mut held = Vec::new();
    for _ in 0..3 {
        held.push(f.get(None).await);
    }
    let mut body = f.get(None).await.into_body();
    let first = body.frame().await.unwrap().unwrap().into_data().unwrap();
    assert_eq!(first.len(), 64 * 1024);
    assert!(first.iter().all(|b| *b == 42));
    assert_eq!(f.get(None).await.status(), 503);
    drop(body);
    let r = f.get(None).await;
    assert_eq!(r.status(), 200);
    let mut body = r.into_body();
    assert_eq!(
        body.frame()
            .await
            .unwrap()
            .unwrap()
            .into_data()
            .unwrap()
            .len(),
        64 * 1024
    );
    std::fs::write(&path, []).unwrap();
    assert!(body.frame().await.unwrap().is_err());
    let replacement = f.get(None).await;
    assert_eq!(replacement.status(), 200);
    drop(body);
}

#[tokio::test]
async fn download_protect_rejects_host_and_does_not_fall_back_to_static_files() {
    let f = Fixture::new(false, false).await;
    let r = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/files/1/download")
                .header("host", "evil.invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), 421);
    assert_eq!(r.headers()["x-content-type-options"], "nosniff");
    let r = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/files/1/extra/download")
                .header("host", "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    assert!(r.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("application/json"));
}

#[tokio::test]
async fn download_rejects_database_path_outside_roots() {
    let f = Fixture::new(false, false).await;
    let r = f.request("3", None, None, "GET").await;
    assert_eq!(r.status(), 422);
    let v: serde_json::Value = serde_json::from_slice(&bytes(r).await).unwrap();
    assert_eq!(v["expected"], true);
    assert_eq!(
        v["message"],
        "Dateipfad liegt außerhalb der freigegebenen Verzeichnisse"
    );
}

#[tokio::test]
async fn download_ignores_invalid_ranges_and_accepts_case_insensitive_unit() {
    let f = Fixture::new(false, false).await;
    for range in [
        "items=0-1",
        "bytes= 0-1",
        "bytes=0 -1",
        "bytes=x-y",
        "bytes=",
        "garbage",
        "bytes=+1-2",
        "bytes=7-2",
        "bytes=-",
    ] {
        let r = f.get(Some(range)).await;
        assert_eq!(r.status(), 200, "{range}");
        assert_eq!(bytes(r).await, DATA);
    }
    let r = f.get(Some("BYTES=0-1")).await;
    assert_eq!(r.status(), 206);
    assert_eq!(bytes(r).await, &DATA[..2]);
    let r = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/files/{}/download", f.id))
                .header("host", "localhost:8080")
                .header(
                    "range",
                    axum::http::HeaderValue::from_bytes(b"bytes=\xff-").unwrap(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(bytes(r).await, DATA);
}

#[tokio::test]
async fn download_if_range_uses_handle_last_modified() {
    let f = Fixture::new(false, false).await;
    let path = f.dir.join("models/model.step");
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(784111777))
        .unwrap();
    let r = f.get(None).await;
    let modified = r.headers()["last-modified"].to_str().unwrap().to_owned();
    assert_eq!(modified, "Sun, 06 Nov 1994 08:49:37 GMT");
    drop(r);
    for (validator, status) in [
        (modified.as_str(), 206),
        ("Sun, 06 Nov 1994 08:49:38 GMT", 200),
        ("invalid", 200),
        ("\"tag\"", 200),
        ("W/\"tag\"", 200),
    ] {
        let r = f
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{}/download", f.id))
                    .header("host", "localhost:8080")
                    .header("range", "bytes=0-1")
                    .header("if-range", validator)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), status, "{validator}");
        assert_eq!(r.headers()["last-modified"], modified);
        assert_eq!(
            bytes(r).await,
            if status == 206 { &DATA[..2] } else { DATA }
        );
    }
}

#[tokio::test]
async fn download_skips_unresolvable_roots() {
    let f = Fixture::new(false, false).await;
    for (roots, status) in [
        (
            format!(
                "{}:{}",
                f.dir.join("missing").display(),
                f.dir.join("models").display()
            ),
            200,
        ),
        (f.dir.join("missing").display().to_string(), 422),
    ] {
        let config = Config::from_lookup(|k| match k {
            "KATALOG_DB" => Some(f.dir.join("catalog.db").display().to_string()),
            "KATALOG_ROOTS" => Some(roots.clone()),
            "KATALOG_ALLOW_NO_AUTH" => Some("1".into()),
            _ => None,
        })
        .unwrap();
        let app = build_app(AppConfig {
            config,
            auth: None,
            clock: Arc::new(SystemClock),
        });
        let r = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/files/{}/download", f.id))
                    .header("host", "localhost:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), status);
        if status == 200 {
            assert_eq!(bytes(r).await, DATA);
        } else {
            let v: serde_json::Value = serde_json::from_slice(&bytes(r).await).unwrap();
            assert_eq!(
                v["message"],
                "Dateipfad liegt außerhalb der freigegebenen Verzeichnisse"
            );
        }
    }
}
