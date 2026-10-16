use super::*;
use katalog_core::db;
use std::sync::atomic::{AtomicU64, Ordering};

struct Time(AtomicU64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
struct Fixture {
    app: Arc<App>,
    time: Arc<Time>,
    dir: std::path::PathBuf,
    session: Session,
}
impl Fixture {
    async fn new(enabled: bool) -> Self {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "web-fileops-http-{:032x}",
            u128::from_ne_bytes(random)
        ));
        for sub in ["config", "models/uploads", "static"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let path = dir.join("config/catalog.db");
        drop(db::connect(&path).unwrap());
        let config = Config::from_lookup(|key| match key {
            "KATALOG_DB" => Some(path.to_string_lossy().into_owned()),
            "KATALOG_ROOTS" => Some(dir.join("models").to_string_lossy().into_owned()),
            "KATALOG_STATIC_DIR" => Some(dir.join("static").to_string_lossy().into_owned()),
            "KATALOG_FILE_OPS" if enabled => Some("1".into()),
            "KATALOG_UPLOAD_DIR" => Some(dir.join("models/uploads").to_string_lossy().into_owned()),
            "KATALOG_UPLOAD_MAX_MB" => Some("1".into()),
            "KATALOG_WRITE" => Some("1".into()),
            "KATALOG_ALLOWED_HOSTS" => Some("localhost:8080".into()),
            _ => None,
        })
        .unwrap();
        let time = Arc::new(Time(AtomicU64::new(100)));
        let mut data = Data {
            auth: Some(AuthFile {
                version: 1,
                password_hash: test_hash(),
                must_change: false,
                updated_at: 100,
            }),
            sessions: HashMap::new(),
            limits: Limits::default(),
            revision: 0,
        };
        let session = new_session(&mut data, "test", 100).unwrap();
        let uploads = uploads::initialize(&config).unwrap();
        let writer = WriteGate::new(db::open_read_write(&path).unwrap());
        let fileops = crate::fileops_startup::prepare(
            &config,
            writer.conn.clone(),
            uploads.as_ref().map(|u| u.store.clone()),
        )
        .unwrap()
        .map(|o| Arc::new(fileops::Runtime::new(o)));
        let app = Arc::new(App {
            fileops,
            fileops_safety: Mutex::new(crate::fileops_safety::FileOpsSafety::new(2000)),
            uploads,
            writer: Some(writer),
            config,
            clock: time.clone(),
            data: Mutex::new(data),
            changes: Arc::new(tokio::sync::Mutex::new(())),
            geometry: Arc::new(GeometryGate::default()),
            downloads: Arc::new(tokio::sync::Semaphore::new(4)),
        });
        Self {
            app,
            time,
            dir,
            session,
        }
    }
    fn request(&self, method: &str, path: &str, body: Body) -> Request {
        Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost:8080")
            .header("origin", "http://localhost:8080")
            .header("cookie", format!("katalog_session={}", self.session.token))
            .header("x-csrf-token", &self.session.csrf)
            .header("sec-fetch-site", "same-origin")
            .body(body)
            .unwrap()
    }
    async fn invoke(&self, cmd: &str, args: Value) -> Response {
        let mut req = self.request(
            "POST",
            &format!("/api/invoke/{cmd}"),
            Body::from(args.to_string()),
        );
        req.headers_mut()
            .insert("content-type", "application/json".parse().unwrap());
        router(self.app.clone()).oneshot(req).await.unwrap()
    }
    fn file(&self, name: &str) -> String {
        let path = self.dir.join("models").join(name);
        std::fs::write(&path, b"model").unwrap();
        db::test_insert_minimal_file(
            &self.app.writer.as_ref().unwrap().conn.lock().unwrap(),
            path.to_str().unwrap(),
            None,
        )
        .unwrap()
        .to_string()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
async fn value(r: Response) -> Value {
    serde_json::from_slice(&to_bytes(r.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
#[tokio::test]
async fn all_fileops_disabled_before_security_or_json() {
    let f = Fixture::new(false).await;
    for cmd in crate::write::FILE_OPS_COMMANDS {
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/invoke/{cmd}"))
            .body(Body::from("invalid"))
            .unwrap();
        assert_eq!(
            router(f.app.clone()).oneshot(req).await.unwrap().status(),
            404,
            "{cmd}"
        );
    }
}
#[tokio::test]
async fn fileops_security_gate_and_session_capability() {
    let f = Fixture::new(true).await;
    let id = f.file("a.3mf");
    for (remove, status) in [("cookie", 401), ("x-csrf-token", 403), ("origin", 403)] {
        let mut req = f.request(
            "POST",
            "/api/invoke/rename_file",
            Body::from(json!({"fileId":id,"name":"b"}).to_string()),
        );
        req.headers_mut()
            .insert("content-type", "application/json".parse().unwrap());
        req.headers_mut().remove(remove);
        assert_eq!(
            router(f.app.clone()).oneshot(req).await.unwrap().status(),
            status
        );
    }
    let permit = f
        .app
        .uploads
        .as_ref()
        .unwrap()
        .active
        .clone()
        .try_acquire_owned()
        .unwrap();
    let response = f
        .invoke("rename_file", json!({"fileId":id,"name":"b"}))
        .await;
    assert_eq!(response.status(), 503);
    assert!(response.headers().contains_key("retry-after"));
    drop(permit);
    let change = f.app.changes.clone().lock_owned().await;
    assert_eq!(
        f.invoke("rename_file", json!({"fileId":id,"name":"b"}))
            .await
            .status(),
        200
    );
    drop(change);
    let req = f.request("GET", "/api/session", Body::empty());
    let v = value(router(f.app.clone()).oneshot(req).await.unwrap()).await;
    assert_eq!(v["fileOps"], true);
}
#[tokio::test]
async fn fileops_file_cycle_password_snapshot_and_strict_contract() {
    let f = Fixture::new(true).await;
    let id = f.file("a.gcode.3mf");
    assert_eq!(
        f.invoke("rename_file", json!({"fileId":id,"name":"b","other":true}))
            .await
            .status(),
        422
    );
    assert_eq!(
        f.invoke("rename_file", json!({"fileId":1,"name":"b"}))
            .await
            .status(),
        400
    );
    assert_eq!(
        f.invoke("rename_file", json!({"fileId":id,"name":".hidden"}))
            .await
            .status(),
        400
    );
    assert_eq!(
        f.invoke("rename_file", json!({"fileId":id,"name":"b"}))
            .await
            .status(),
        200
    );
    assert!(f.dir.join("models/b.gcode.3mf").exists());
    let r = value(f.invoke("delete_files", json!({"fileIds":[id,id]})).await).await;
    assert_eq!(r.as_array().unwrap().len(), 1);
    assert_eq!(r[0]["ok"], true);
    assert_eq!(
        f.invoke(
            "delete_file_permanently",
            json!({"fileId":id,"password":"test-password-long"})
        )
        .await
        .status(),
        422
    );
    assert_eq!(
        f.invoke(
            "delete_file_permanently",
            json!({"fileId":id,"confirm":true,"password":"wrong"})
        )
        .await
        .status(),
        403
    );
    assert_eq!(
        f.invoke("restore_file", json!({"fileId":id}))
            .await
            .status(),
        200
    );
    assert_eq!(
        f.invoke(
            "move_files_to_folder",
            json!({"fileIds":[id],"folderId":null})
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        f.invoke("delete_files", json!({"fileIds":[id]}))
            .await
            .status(),
        200
    );
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    let start=f.invoke("empty_trash_start",json!({"expectedCount":1,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"})).await;
    assert_eq!(start.status(), 200);
    let token = value(start).await["token"].clone();
    let block = value(f.invoke("empty_trash_block", json!({"token":token})).await).await;
    assert_eq!(block["remaining"], 0);
    assert_eq!(block["results"][0]["ok"], true);
    let other = f.file("permanent.3mf");
    assert_eq!(
        f.invoke("delete_files", json!({"fileIds":[other]}))
            .await
            .status(),
        200
    );
    let permanent = f
        .invoke(
            "delete_file_permanently",
            json!({"fileId":other,"confirm":true,"password":"test-password-long"}),
        )
        .await;
    assert_eq!(permanent.status(), 200);
    assert_eq!(value(permanent).await["kept_on_disk"], false);
}
#[tokio::test]
async fn folder_commands_and_backups_before_removal() {
    let f = Fixture::new(true).await;
    let parent = {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute(
            "INSERT INTO folders(name,path) VALUES('root',?1)",
            [f.dir.join("models").to_str().unwrap()],
        )
        .unwrap();
        c.last_insert_rowid().to_string()
    };
    let a = value(
        f.invoke("create_folder", json!({"parentId":parent,"name":"A"}))
            .await,
    )
    .await;
    assert_eq!(
        f.invoke("rename_folder", json!({"folderId":a,"name":"B"}))
            .await
            .status(),
        200
    );
    let other = value(
        f.invoke("create_folder", json!({"parentId":parent,"name":"Other"}))
            .await,
    )
    .await;
    assert_eq!(
        f.invoke("move_folder", json!({"folderId":a,"newParentId":other}))
            .await
            .status(),
        200
    );
    assert_eq!(
        f.invoke("remove_folder_from_catalog", json!({"folderId":a}))
            .await
            .status(),
        200
    );
    assert!(f.dir.join("models/Other/B").exists());
    assert!(f.dir.join("config/backups").exists());
}

fn test_hash() -> String {
    use argon2::{
        password_hash::{PasswordHasher, SaltString},
        Algorithm, Argon2, Params, Version,
    };
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(8, 1, 1, None).unwrap(),
    )
    .hash_password(
        b"test-password-long",
        &SaltString::encode_b64(b"0123456789abcdef").unwrap(),
    )
    .unwrap()
    .to_string()
}
#[tokio::test]
async fn snapshot_is_session_bound_and_blocks_are_throttled() {
    let f = Fixture::new(true).await;
    let id = f.file("a.3mf");
    f.invoke("delete_files", json!({"fileIds":[id]})).await;
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    let token=value(f.invoke("empty_trash_start",json!({"expectedCount":1,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"})).await).await["token"].clone();
    let other = {
        let mut d = f.app.data.lock().unwrap();
        new_session(&mut d, "test", 100).unwrap()
    };
    let mut req = f.request(
        "POST",
        "/api/invoke/empty_trash_block",
        Body::from(json!({"token":token}).to_string()),
    );
    req.headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    req.headers_mut().insert(
        "cookie",
        format!("katalog_session={}", other.token).parse().unwrap(),
    );
    req.headers_mut()
        .insert("x-csrf-token", other.csrf.parse().unwrap());
    assert_eq!(
        router(f.app.clone()).oneshot(req).await.unwrap().status(),
        404
    );
    assert_eq!(
        f.invoke("empty_trash_block", json!({"token":token}))
            .await
            .status(),
        200
    );
    f.time.0.store(101, Ordering::SeqCst);
    let denied = f
        .invoke(
            "empty_trash_start",
            json!({"expectedCount":0,"maxDeletedAt":null,"password":"test-password-long"}),
        )
        .await;
    assert_eq!(denied.status(), 429);
    assert!(denied.headers().contains_key("retry-after"));
}
#[tokio::test]
async fn backup_failure_rejects_before_filesystem_change_and_noop_does_not_backup() {
    let f = Fixture::new(true).await;
    let id = f.file("a.3mf");
    assert_eq!(
        f.invoke("delete_files", json!({"fileIds":["999999"]}))
            .await
            .status(),
        200
    );
    assert!(!f.dir.join("config/backups").exists());
    std::fs::write(f.dir.join("config/backups"), b"blocked").unwrap();
    let r = f.invoke("delete_files", json!({"fileIds":[id]})).await;
    assert_eq!(r.status(), 503);
    assert!(!r.headers().contains_key("retry-after"));
    assert_eq!(value(r).await["kind"], "backup_failed");
    assert!(f.dir.join("models/a.3mf").exists());
}
#[tokio::test]
#[allow(clippy::await_holding_lock)] // Deliberately hold DB to test revocation while a worker waits.
async fn revoked_session_after_wait_stops_permanent_delete() {
    let f = Fixture::new(true).await;
    let id = f.file("a.3mf");
    f.invoke("delete_files", json!({"fileIds":[id]})).await;
    let request = f.request(
        "POST",
        "/api/invoke/delete_file_permanently",
        Body::from(json!({"fileId":id,"confirm":true,"password":"test-password-long"}).to_string()),
    );
    let mut request = request;
    request
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    let app = f.app.clone();
    let task = tokio::spawn(async move { router(app).oneshot(request).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    f.app.data.lock().unwrap().sessions.remove(&f.session.token);
    drop(conn);
    let response = task.await.unwrap();
    assert!(response.status() == 401 || value(response).await["kind"] == "not_started");
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    assert!(db::get_file(&conn, id.parse().unwrap()).unwrap().is_some());
}
#[tokio::test]
async fn blocked_runtime_disables_capability_without_retry_and_keeps_reads() {
    let f = Fixture::new(true).await;
    let id = f.file("a.3mf");
    f.app.fileops.as_ref().unwrap().block_for_test();
    let r = f
        .invoke("rename_file", json!({"fileId":id,"name":"b"}))
        .await;
    assert_eq!(r.status(), 503);
    assert!(!r.headers().contains_key("retry-after"));
    assert_eq!(value(r).await["kind"], "restart_required");
    let req = f.request("GET", "/api/session", Body::empty());
    let v = value(router(f.app.clone()).oneshot(req).await.unwrap()).await;
    assert_eq!(v["fileOps"], false);
    assert_eq!(v["writable"], true);
    assert_eq!(
        f.invoke("empty_trash_summary", json!({})).await.status(),
        200
    );
}
#[tokio::test]
#[allow(clippy::await_holding_lock)] // Deliberate barrier proving a detached worker owns admission.
async fn detached_worker_keeps_import_gate_until_completion() {
    let f = Fixture::new(true).await;
    let id = f.file("a.3mf");
    let mut request = f.request(
        "POST",
        "/api/invoke/rename_file",
        Body::from(json!({"fileId":id,"name":"b"}).to_string()),
    );
    request
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    let app = f.app.clone();
    let task = tokio::spawn(async move { router(app).oneshot(request).await.unwrap() });
    for _ in 0..100 {
        if f.app.uploads.as_ref().unwrap().active.available_permits() == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert_eq!(
        f.app.uploads.as_ref().unwrap().active.available_permits(),
        0
    );
    task.abort();
    assert!(f
        .app
        .uploads
        .as_ref()
        .unwrap()
        .active
        .clone()
        .try_acquire_owned()
        .is_err());
    drop(conn);
    for _ in 0..100 {
        if f.app.uploads.as_ref().unwrap().active.available_permits() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert!(f.dir.join("models/b.3mf").exists());
    assert_eq!(
        f.app.uploads.as_ref().unwrap().active.available_permits(),
        1
    );
}
#[tokio::test]
async fn db_only_removal_revoked_after_first_delete_rolls_back_all_rows() {
    for folder_mode in [false, true] {
        let f = Fixture::new(true).await;
        std::fs::create_dir(f.dir.join("models/sub")).unwrap();
        let a = f.file("sub/a.3mf");
        let b = f.file("sub/b.3mf");
        let folder = {
            let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
            conn.execute(
                "INSERT INTO folders(name,path) VALUES('sub',?1)",
                [f.dir.join("models/sub").to_str().unwrap()],
            )
            .unwrap();
            let folder = conn.last_insert_rowid();
            conn.execute("UPDATE files SET folder_id=?1", [folder])
                .unwrap();
            folder.to_string()
        };
        let weak = Arc::downgrade(&f.app);
        let token = f.session.token.clone();
        f.app
            .writer
            .as_ref()
            .unwrap()
            .conn
            .lock()
            .unwrap()
            .update_hook(Some(
                move |action: rusqlite::hooks::Action, _db: &str, table: &str, _id: i64| {
                    if action == rusqlite::hooks::Action::SQLITE_DELETE && table == "files" {
                        weak.upgrade()
                            .unwrap()
                            .data
                            .lock()
                            .unwrap()
                            .sessions
                            .remove(&token);
                    }
                },
            ))
            .unwrap();
        let response = if folder_mode {
            f.invoke("remove_folder_from_catalog", json!({"folderId":folder}))
                .await
        } else {
            f.invoke("remove_files_from_catalog", json!({"fileIds":[a,b]}))
                .await
        };
        assert_eq!(response.status(), 409);
        assert_eq!(value(response).await["kind"], "not_started");
        let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        assert!(conn.is_autocommit());
        assert_eq!(
            conn.query_row("SELECT count(*) FROM files", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM folders", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(f.app.fileops.as_ref().unwrap().available());
    }
}
