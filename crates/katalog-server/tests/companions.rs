//! Companion files in the web: list, preview and download, read only.
use axum::{
    body::{to_bytes, Body},
    http::Request,
    response::Response,
    Router,
};
use katalog_core::db;
use katalog_server::{build_app, config::Config, password::{self, AuthFile}, AppConfig, SystemClock};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tower::ServiceExt;

static NEXT: AtomicUsize = AtomicUsize::new(0);
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nbody";
const PDF: &[u8] = b"%PDF-1.4 body";

struct Fx {
    app: Router,
    dir: PathBuf,
    model: i64,
    png: i64,
    pdf: i64,
}
impl Fx {
    async fn new(auth: bool) -> Self {
        Self::with(auth, false).await
    }
    async fn with(auth: bool, forced: bool) -> Self {
        let dir = std::env::temp_dir().join(format!("companions-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(dir.join("models/Foo_extras")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let model_path = dir.join("models/Foo.3mf");
        std::fs::write(&model_path, b"model").unwrap();
        std::fs::write(dir.join("models/Foo_extras/pic.png"), PNG).unwrap();
        std::fs::write(dir.join("models/Foo_extras/doc.pdf"), PDF).unwrap();
        let conn = db::connect(&dir.join("catalog.db")).unwrap();
        let model = db::test_insert_minimal_file(&conn, model_path.to_str().unwrap(), None).unwrap();
        let add = |name: &str, body: &[u8], kind: &str| {
            db::companions::insert_companion(
                &conn, model, name, dir.join("models/Foo_extras").join(name).to_str().unwrap(), kind, Some(body.len() as i64), "now",
            )
            .unwrap()
        };
        let png = add("pic.png", PNG, "image");
        let pdf = add("doc.pdf", PDF, "pdf");
        drop(conn);
        let config = Config::from_lookup(|k| match k {
            "KATALOG_DB" => Some(dir.join("catalog.db").to_str().unwrap().into()),
            "KATALOG_ROOTS" => Some(dir.join("models").to_str().unwrap().into()),
            "KATALOG_ALLOW_NO_AUTH" if !auth => Some("1".into()),
            _ => None,
        })
        .unwrap();
        let auth = auth.then(|| AuthFile {
            version: 1,
            password_hash: tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(password::hash_password("companion-test-password".into()))).unwrap(),
            must_change: forced,
            updated_at: 0,
        });
        let app = build_app(AppConfig { config, auth, clock: Arc::new(SystemClock) });
        Self { app, dir, model, png, pdf }
    }
    async fn login(&self) -> String {
        let r = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/login")
                    .header("host", "localhost:8080")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"password":"companion-test-password"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        r.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().into()
    }
    async fn download_with(&self, id: &str, cookie: Option<&str>, range: Option<&str>) -> Response {
        let mut req = Request::builder().uri(format!("/api/companions/{id}/download")).header("host", "localhost:8080");
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        if let Some(r) = range {
            req = req.header("range", r);
        }
        self.app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
    }
    fn conn(&self) -> rusqlite::Connection {
        db::connect(&self.dir.join("catalog.db")).unwrap()
    }
    async fn download(&self, id: &str) -> Response {
        self.app
            .clone()
            .oneshot(Request::builder().uri(format!("/api/companions/{id}/download")).header("host", "localhost:8080").body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
    async fn invoke(&self, command: &str, args: Value) -> (u16, Value) {
        let r = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/invoke/{command}"))
                    .header("host", "localhost:8080")
                    .header("content-type", "application/json")
                    .body(Body::from(args.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = r.status().as_u16();
        let body = to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
async fn bytes(r: Response) -> Vec<u8> {
    to_bytes(r.into_body(), 1 << 20).await.unwrap().to_vec()
}

#[tokio::test(flavor = "multi_thread")]
async fn download_needs_a_session() {
    let f = Fx::new(true).await;
    assert_eq!(f.download(&f.png.to_string()).await.status(), 401);
    let (status, _) = f.invoke("list_companions", json!({"fileId": f.model.to_string()})).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn images_and_documents_download_as_attachments_with_safe_types() {
    let f = Fx::new(false).await;
    let r = f.download(&f.png.to_string()).await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "image/png");
    assert_eq!(r.headers()["x-content-type-options"], "nosniff");
    assert_eq!(r.headers()["content-disposition"], "attachment; filename=\"pic.png\"; filename*=UTF-8''pic.png");
    assert_eq!(bytes(r).await, PNG);
    let r = f.download(&f.pdf.to_string()).await;
    assert_eq!(r.headers()["content-type"], "application/octet-stream");
    assert!(r.headers()["content-disposition"].to_str().unwrap().starts_with("attachment;"));
    assert_eq!(bytes(r).await, PDF);
}

#[tokio::test]
async fn unknown_trashed_and_malformed_ids_are_refused() {
    let f = Fx::new(false).await;
    assert_eq!(f.download("999999").await.status(), 404);
    assert_eq!(f.download("oops").await.status(), 400);
    f.conn().execute("UPDATE companion_files SET deleted_at='x' WHERE id=?1", [f.png]).unwrap();
    assert_eq!(f.download(&f.png.to_string()).await.status(), 404);
    let (status, _) = f.invoke("companion_image_data", json!({"companionId": f.png.to_string()})).await;
    assert_eq!(status, 404);
    let r = f.app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/companions/{}/download", f.pdf)).header("host", "localhost:8080").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), 405);
}

#[tokio::test]
async fn a_row_outside_the_model_folder_or_with_traversal_is_not_served() {
    let f = Fx::new(false).await;
    std::fs::create_dir_all(f.dir.join("models/Bar_extras")).unwrap();
    std::fs::write(f.dir.join("models/Bar_extras/s.png"), PNG).unwrap();
    std::fs::write(f.dir.join("models/s.png"), PNG).unwrap();
    let conn = f.conn();
    let other = db::companions::insert_companion(&conn, f.model, "s.png", f.dir.join("models/Bar_extras/s.png").to_str().unwrap(), "image", None, "now").unwrap();
    let dotdot = db::companions::insert_companion(&conn, f.model, "s.png", &format!("{}/models/Foo_extras/../s.png", f.dir.display()), "image", None, "now").unwrap();
    drop(conn);
    assert!(f.download(&other.to_string()).await.status().as_u16() >= 400);
    assert!(f.download(&dotdot.to_string()).await.status().as_u16() >= 400);
    let (status, _) = f.invoke("companion_image_data", json!({"companionId": other.to_string()})).await;
    assert!(status >= 400);
}

#[cfg(unix)]
#[tokio::test]
async fn symlinks_and_hard_links_are_refused() {
    let f = Fx::new(false).await;
    let pic = f.dir.join("models/Foo_extras/pic.png");
    std::fs::write(f.dir.join("secret.png"), PNG).unwrap();
    std::fs::remove_file(&pic).unwrap();
    std::os::unix::fs::symlink(f.dir.join("secret.png"), &pic).unwrap();
    assert_eq!(f.download(&f.png.to_string()).await.status(), 404);
    let (status, _) = f.invoke("companion_image_data", json!({"companionId": f.png.to_string()})).await;
    assert_eq!(status, 404);
    std::fs::remove_file(&pic).unwrap();
    std::fs::write(&pic, PNG).unwrap();
    std::fs::hard_link(&pic, f.dir.join("models/alias.png")).unwrap();
    assert_eq!(f.download(&f.png.to_string()).await.status(), 404);
    std::fs::remove_file(f.dir.join("models/alias.png")).unwrap();
    assert_eq!(f.download(&f.png.to_string()).await.status(), 200);
    // The folder itself replaced by a link.
    std::fs::rename(f.dir.join("models/Foo_extras"), f.dir.join("moved")).unwrap();
    std::os::unix::fs::symlink(f.dir.join("moved"), f.dir.join("models/Foo_extras")).unwrap();
    assert_eq!(f.download(&f.pdf.to_string()).await.status(), 404);
}

#[tokio::test]
async fn the_listing_has_no_paths_and_hides_trashed_rows() {
    let f = Fx::new(false).await;
    f.conn().execute("UPDATE companion_files SET deleted_at='x' WHERE id=?1", [f.pdf]).unwrap();
    let (status, list) = f.invoke("list_companions", json!({"fileId": f.model.to_string()})).await;
    assert_eq!(status, 200);
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "pic.png");
    assert_eq!(list[0]["previewable"], true);
    assert!(list[0].get("path").is_none() && list[0].get("deletedAt").is_none());
    let (status, _) = f.invoke("list_companions", json!({"fileId": "999999"})).await;
    assert_eq!(status, 404);
    let (status, _) = f.invoke("list_companions", json!({"fileId": "x"})).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn the_preview_is_a_data_uri_for_images_only() {
    let f = Fx::new(false).await;
    let (status, uri) = f.invoke("companion_image_data", json!({"companionId": f.png.to_string()})).await;
    assert_eq!(status, 200);
    assert!(uri.as_str().unwrap().starts_with("data:image/png;base64,"));
    let (status, _) = f.invoke("companion_image_data", json!({"companionId": f.pdf.to_string()})).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_model_in_the_trash_serves_nothing() {
    let f = Fx::new(false).await;
    f.conn().execute("UPDATE files SET deleted_at='x'", []).unwrap();
    assert_eq!(f.download(&f.png.to_string()).await.status(), 404);
    let (_, list) = f.invoke("list_companions", json!({"fileId": f.model.to_string()})).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forced_password_change_blocks_the_route_and_a_session_opens_it() {
    let f = Fx::with(true, true).await;
    let cookie = f.login().await;
    let r = f.download_with(&f.png.to_string(), Some(&cookie), None).await;
    assert_eq!(r.status(), 403);
    let v: Value = serde_json::from_slice(&bytes(r).await).unwrap();
    assert_eq!(v["code"], "password_change_required");
    let g = Fx::new(true).await;
    let cookie = g.login().await;
    assert_eq!(g.download_with(&g.png.to_string(), Some(&cookie), None).await.status(), 200);
}

#[tokio::test]
async fn ranges_work_and_unsatisfiable_ones_give_416() {
    let f = Fx::new(false).await;
    let id = f.pdf.to_string();
    let r = f.download_with(&id, None, Some("bytes=2-5")).await;
    assert_eq!(r.status(), 206);
    assert_eq!(r.headers()["content-range"], format!("bytes 2-5/{}", PDF.len()));
    assert_eq!(bytes(r).await, &PDF[2..6]);
    let r = f.download_with(&id, None, Some(&format!("bytes={}-", PDF.len()))).await;
    assert_eq!(r.status(), 416);
    assert_eq!(r.headers()["content-range"], format!("bytes */{}", PDF.len()));
}

#[tokio::test]
async fn a_companion_outside_the_configured_roots_is_not_served_or_listed_as_present() {
    let f = Fx::new(false).await;
    std::fs::create_dir_all(f.dir.join("outside/Baz_extras")).unwrap();
    std::fs::write(f.dir.join("outside/Baz.3mf"), b"m").unwrap();
    std::fs::write(f.dir.join("outside/Baz_extras/p.png"), PNG).unwrap();
    let conn = f.conn();
    let model = db::test_insert_minimal_file(&conn, f.dir.join("outside/Baz.3mf").to_str().unwrap(), None).unwrap();
    let id = db::companions::insert_companion(&conn, model, "p.png", f.dir.join("outside/Baz_extras/p.png").to_str().unwrap(), "image", Some(8), "now").unwrap();
    drop(conn);
    assert_eq!(f.download(&id.to_string()).await.status(), 404);
    let (status, _) = f.invoke("companion_image_data", json!({"companionId": id.to_string()})).await;
    assert_eq!(status, 404);
    let (_, list) = f.invoke("list_companions", json!({"fileId": model.to_string()})).await;
    assert_eq!(list[0]["missing"], true);
}

#[cfg(unix)]
#[tokio::test]
async fn crlf_and_bidi_in_a_name_cannot_reach_the_header() {
    let f = Fx::new(false).await;
    let name = "a\r\nX: y\u{202e}b.png";
    let path = f.dir.join("models/Foo_extras").join(name);
    std::fs::write(&path, PNG).unwrap();
    let id = db::companions::insert_companion(&f.conn(), f.model, name, path.to_str().unwrap(), "image", Some(8), "now").unwrap();
    let r = f.download(&id.to_string()).await;
    assert_eq!(r.status(), 200);
    let value = r.headers()["content-disposition"].to_str().unwrap();
    assert_eq!(value, "attachment; filename=\"aX: yb.png\"; filename*=UTF-8''aX%3A%20yb.png");
}

#[tokio::test]
async fn download_slots_are_held_until_the_body_ends_or_is_dropped() {
    let f = Fx::new(false).await;
    let id = f.png.to_string();
    let mut held = Vec::new();
    for _ in 0..4 {
        let r = f.download(&id).await;
        assert_eq!(r.status(), 200);
        held.push(r);
    }
    assert_eq!(f.download(&id).await.status(), 503);
    drop(held.pop());
    assert_eq!(f.download(&id).await.status(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_companion_bigger_than_the_old_50_mib_limit_still_downloads_in_ranges() {
    use std::io::{Seek, Write};
    let f = Fx::new(false).await;
    let size: u64 = 60 * 1024 * 1024;
    let path = f.dir.join("models/Foo_extras/big.pdf");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(PDF).unwrap();
    file.seek(std::io::SeekFrom::Start(size - 3)).unwrap();
    file.write_all(b"END").unwrap();
    drop(file);
    let model: i64 = f.conn().query_row("SELECT id FROM files LIMIT 1", [], |r| r.get(0)).unwrap();
    let id = katalog_core::db::companions::insert_companion(&f.conn(), model, "big.pdf", path.to_str().unwrap(), "pdf", Some(size as i64), "now").unwrap();
    let r = f.download_with(&id.to_string(), None, Some("bytes=-3")).await;
    assert_eq!(r.status(), 206);
    assert_eq!(bytes(r).await, b"END");
    let whole = f.download(&id.to_string()).await;
    assert_eq!(whole.status(), 200);
    assert_eq!(whole.headers()["content-length"], size.to_string());
}
