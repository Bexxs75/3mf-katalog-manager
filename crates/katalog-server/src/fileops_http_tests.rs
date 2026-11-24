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
            "KATALOG_COMPANION_MAX_MB" => Some("1".into()),
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
            restore: restore_upload::Runtime::default(),
            events: uploads.as_ref().map(|r| r.hub.clone()).unwrap_or_default(),
            printers: printer_service::Runtime::default(),
            _instance_lock: None,
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
            export: crate::export::Runtime::new(),
            slicer: None,
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
        422
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
    revoke_session(&mut f.app.data.lock().unwrap(), &f.session.token);
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
    // Generous waits (10 s each, left as soon as the state is reached): in a fully parallel
    // run of an unoptimised build the detached worker can take well over 200 ms.
    for _ in 0..5000 {
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
    for _ in 0..5000 {
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
                        revoke_session(&mut weak.upgrade().unwrap().data.lock().unwrap(), &token);
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

#[tokio::test]
async fn disabled_fileops_have_security_headers() {
    let f = Fixture::new(false).await;
    let r = f.invoke("delete_files", json!({"fileIds":["1"]})).await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.headers()["cache-control"], "no-store");
    assert_eq!(r.headers()["x-content-type-options"], "nosniff");
    assert!(r.headers().contains_key("content-security-policy"));
}
#[tokio::test]
async fn large_folder_removal_is_possible_and_charges_one_window() {
    let f = Fixture::new(true).await;
    std::fs::create_dir(f.dir.join("models/large")).unwrap();
    let folder = {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("INSERT INTO folders(name,path) VALUES('large',?1)", [f.dir.join("models/large").to_str().unwrap()]).unwrap();
        let folder = c.last_insert_rowid();
        for n in 0..500 {
            db::test_insert_minimal_file(&c, f.dir.join(format!("models/large/{n}.3mf")).to_str().unwrap(), Some(folder)).unwrap();
        }
        folder.to_string()
    };
    let r = f.invoke("remove_folder_from_catalog", json!({"folderId":folder})).await;
    assert_eq!(r.status(), 200);
    assert_eq!(value(r).await["modelCount"], 500);
    let id = f.file("after.3mf");
    let r = f.invoke("remove_files_from_catalog", json!({"fileIds":[id]})).await;
    assert_eq!(r.status(), 429);
    assert_eq!(r.headers()["retry-after"], "600");
    f.time.0.store(700, Ordering::SeqCst);
    assert_eq!(f.invoke("remove_files_from_catalog", json!({"fileIds":[id]})).await.status(), 200);
}
#[tokio::test]
async fn poisoned_fileops_mutex_requires_restart_also_for_6d() {
    let f = Fixture::new(true).await;
    let rt = f.app.fileops.as_ref().unwrap().clone();
    let _ = std::thread::spawn(move || { let _guard = rt.state.lock().unwrap(); panic!("poison fixture"); }).join();
    for (cmd, args) in [("remove_files_from_catalog", json!({"fileIds":["1"]})), ("delete_files", json!({"fileIds":["1"]}))] {
        let r = f.invoke(cmd, args).await;
        assert_eq!(r.status(), 503);
        assert!(!r.headers().contains_key("retry-after"));
        assert_eq!(value(r).await["kind"], "restart_required");
    }
}
#[tokio::test]
async fn password_failures_are_charged_to_request_peer() {
    let f = Fixture::new(true).await;
    for _ in 0..5 {
        let mut req = f.request("POST", "/api/invoke/delete_file_permanently", Body::from(json!({"fileId":"1","confirm":true,"password":"wrong"}).to_string()));
        req.headers_mut().insert("content-type", "application/json".parse().unwrap());
        req.extensions_mut().insert(axum::extract::ConnectInfo("192.0.2.42:1234".parse::<std::net::SocketAddr>().unwrap()));
        let _ = router(f.app.clone()).oneshot(req).await.unwrap();
    }
    let data = f.app.data.lock().unwrap();
    assert!(!data.limits.clients.contains_key("test"));
    assert!(data.limits.clients.contains_key("192.0.2.42"));
}
#[test]
fn fileops_commands_are_all_audited_writes() {
    for command in crate::write::FILE_OPS_COMMANDS {
        assert!(WRITE_COMMANDS.contains(command), "{command}");
    }
}

#[tokio::test]
async fn thousand_trash_entries_empty_in_blocks_without_ten_minute_limit() {
    let f = Fixture::new(true).await;
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        for n in 0..1000 {
            let id = db::test_insert_minimal_file(&c, f.dir.join(format!("models/t{n}.3mf")).to_str().unwrap(), None).unwrap();
            c.execute("UPDATE files SET deleted_at='2026-01-01T00:00:00+00:00' WHERE id=?1", [id]).unwrap();
        }
    }
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    let started = f.invoke("empty_trash_start", json!({"expectedCount":1000,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"})).await;
    assert_eq!(started.status(), 200);
    let token = value(started).await["token"].clone();
    // A block is bounded by a real 20 s deadline (`Ops::batch`, not by the mock clock). On a slow disk a block
    // may stop early; its untouched items are answered `not_started` and stay in the snapshot. A block that ran
    // to the end must remove exactly 200 (or all that is left); a stopped one must still make progress. The daily item budget is
    // raised so that extra blocks on a slow machine cannot hit it (the daily limit is not under test here).
    *f.app.fileops_safety.lock().unwrap() = crate::fileops_safety::FileOpsSafety::new(100_000);
    let mut remaining = 1000u64;
    for block in 0..60u64 {
        f.time.0.store(100 + block * 2, Ordering::SeqCst);
        let r = f.invoke("empty_trash_block", json!({"token":token})).await;
        assert_eq!(r.status(), 200, "block {block}");
        let body = value(r).await;
        let now = body["remaining"].as_u64().unwrap();
        let stopped = body["results"].as_array().unwrap().iter().any(|i| i["kind"] == "not_started");
        if stopped {
            assert!(now < remaining, "a stopped block {block} still makes progress");
        } else {
            assert_eq!(now, remaining - remaining.min(200), "block {block}");
        }
        remaining = now;
        if remaining == 0 {
            break;
        }
    }
    assert_eq!(remaining, 0, "the trash is empty after the blocks");
    // The 10-minute pot for single permanent deletions was not consumed by the blocks.
    let kept = f.file("kept.3mf");
    assert_eq!(f.invoke("delete_files", json!({"fileIds":[kept]})).await.status(), 200);
    let r = f.invoke("delete_file_permanently", json!({"fileId":kept,"confirm":true,"password":"test-password-long"})).await;
    assert_ne!(r.status(), 429);
}
#[tokio::test]
async fn empty_trash_blocks_still_hit_the_daily_limit_with_real_retry_after() {
    let f = Fixture::new(true).await;
    f.app.fileops_safety.lock().unwrap().reserve_empty_block_items(2000, 100).unwrap();
    let id = f.file("a.3mf");
    assert_eq!(f.invoke("delete_files", json!({"fileIds":[id]})).await.status(), 200);
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    let token = value(f.invoke("empty_trash_start", json!({"expectedCount":1,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"})).await).await["token"].clone();
    let r = f.invoke("empty_trash_block", json!({"token":token})).await;
    assert_eq!(r.status(), 429);
    assert_eq!(r.headers()["retry-after"], "86400");
}

fn add_real_companion(f: &Fixture, file_id: &str, dir: &str, name: &str) -> (i64, std::path::PathBuf) {
    let folder = f.dir.join("models").join(dir);
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(name);
    std::fs::write(&path, [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]).unwrap();
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    let id = db::companions::insert_companion(&conn, file_id.parse().unwrap(), name, path.to_str().unwrap(), "image", Some(8), "2026-01-01T00:00:00Z").unwrap();
    (id, path)
}

fn companion_path(f: &Fixture, id: i64) -> String {
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    conn.query_row("SELECT path FROM companion_files WHERE id=?1", [id], |r| r.get(0)).unwrap()
}

#[tokio::test]
async fn rename_of_model_with_companion_carries_the_file() {
    let f = Fixture::new(true).await;
    let id = f.file("c.3mf");
    let (cid, old) = add_real_companion(&f, &id, "c_extras", "pic.png");
    let r = f.invoke("rename_file", json!({"fileId":id,"name":"d"})).await;
    assert_eq!(r.status(), 200);
    let new = f.dir.join("models/d_extras/pic.png");
    assert!(new.is_file());
    assert!(!old.exists());
    assert!(!f.dir.join("models/c_extras").exists());
    assert!(f.dir.join("models/d.3mf").exists() && !f.dir.join("models/c.3mf").exists());
    assert_eq!(companion_path(&f, cid), new.to_str().unwrap());
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    let model_path: String = conn.query_row("SELECT path FROM files WHERE id=?1", [id.parse::<i64>().unwrap()], |r| r.get(0)).unwrap();
    assert_eq!(model_path, f.dir.join("models/d.3mf").to_str().unwrap());
}

#[tokio::test]
async fn rename_with_a_taken_companion_name_is_a_409_conflict_and_changes_nothing() {
    let f = Fixture::new(true).await;
    let id = f.file("c.3mf");
    let (cid, old) = add_real_companion(&f, &id, "c_extras", "pic.png");
    std::fs::create_dir_all(f.dir.join("models/d_extras")).unwrap();
    std::fs::write(f.dir.join("models/d_extras/pic.png"), b"someone else").unwrap();
    let r = f.invoke("rename_file", json!({"fileId":id,"name":"d"})).await;
    assert_eq!(r.status(), 409);
    let v = value(r).await;
    assert_eq!(v["kind"], "companions_conflict");
    assert_eq!(v["expected"], true);
    assert!(f.dir.join("models/c.3mf").exists() && old.exists());
    assert_eq!(companion_path(&f, cid), old.to_str().unwrap());
    assert_eq!(std::fs::read(f.dir.join("models/d_extras/pic.png")).unwrap(), b"someone else");
}

#[tokio::test]
async fn rename_with_a_symlinked_extras_folder_stays_a_409_has_companions() {
    let f = Fixture::new(true).await;
    let id = f.file("c.3mf");
    let (_, real) = add_real_companion(&f, &id, "real", "pic.png");
    std::os::unix::fs::symlink(real.parent().unwrap(), f.dir.join("models/c_extras")).unwrap();
    {
        let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        conn.execute(
            "UPDATE companion_files SET path=?1 WHERE file_id=?2",
            rusqlite::params![f.dir.join("models/c_extras/pic.png").to_str().unwrap(), id.parse::<i64>().unwrap()],
        )
        .unwrap();
    }
    let r = f.invoke("rename_file", json!({"fileId":id,"name":"d"})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(value(r).await["kind"], "has_companions");
    assert!(real.exists() && f.dir.join("models/c.3mf").exists());
}

#[tokio::test]
async fn batch_move_carries_companions_per_model() {
    let f = Fixture::new(true).await;
    let with = f.file("with.3mf");
    let free = f.file("free.3mf");
    let (cid, _) = add_real_companion(&f, &with, "with_extras", "pic.png");
    let folder = {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        std::fs::create_dir_all(f.dir.join("models/sub")).unwrap();
        c.execute("INSERT INTO folders(name,path) VALUES('sub',?1)", [f.dir.join("models/sub").to_str().unwrap()]).unwrap();
        c.last_insert_rowid().to_string()
    };
    let r = value(f.invoke("move_files_to_folder", json!({"fileIds":[with,free],"folderId":folder})).await).await;
    assert_eq!(r[0]["ok"], true, "{r}");
    assert_eq!(r[1]["ok"], true, "{r}");
    assert!(f.dir.join("models/sub/with_extras/pic.png").is_file());
    assert_eq!(companion_path(&f, cid), f.dir.join("models/sub/with_extras/pic.png").to_str().unwrap());
}

fn add_companion(f: &Fixture, file_id: i64) {
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    conn.execute(
        "INSERT INTO companion_files (file_id, name, path, kind, size_bytes, added_at) VALUES (?1,'pic.png','/x/pic.png','image',1,'2026-01-01T00:00:00Z')",
        [file_id],
    )
    .unwrap();
}

#[tokio::test]
async fn batch_trash_reports_has_companions_per_item_and_empty_trash_skips_them() {
    let f = Fixture::new(true).await;
    let blocked = f.file("blocked.3mf");
    let free = f.file("free.3mf");
    add_companion(&f, blocked.parse().unwrap());
    let r = value(f.invoke("delete_files", json!({"fileIds":[blocked,free]})).await).await;
    assert_eq!(r[0]["ok"], false);
    assert_eq!(r[0]["kind"], "has_companions");
    assert_eq!(r[1]["ok"], true);
    assert!(f.dir.join("models/blocked.3mf").exists());
    assert!(!f.dir.join("models/free.3mf").exists());
    // A trash entry with companions (e.g. trashed on the desktop) is neither counted nor offered.
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("UPDATE files SET deleted_at='2026-01-02T00:00:00Z' WHERE name='blocked.3mf'", []).unwrap();
    }
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    assert_eq!(summary["blockedByCompanions"], 1);
    assert_eq!(summary["count"], 1);
    let free_only = value(f.invoke("empty_trash_summary", json!({})).await).await;
    let start = f
        .invoke("empty_trash_start", json!({"expectedCount":free_only["count"],"maxDeletedAt":free_only["maxDeletedAt"],"password":"test-password-long"}))
        .await;
    assert_eq!(start.status(), 200);
}

#[tokio::test]
async fn empty_trash_with_only_blocked_entries_ends_without_token_limit_or_backup() {
    let f = Fixture::new(true).await;
    let id = f.file("only.3mf");
    add_companion(&f, id.parse().unwrap());
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("UPDATE files SET deleted_at='2026-01-02T00:00:00Z'", []).unwrap();
    }
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    assert_eq!(summary["count"], 0);
    assert_eq!(summary["blockedByCompanions"], 1);
    let start = f
        .invoke("empty_trash_start", json!({"expectedCount":0,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"}))
        .await;
    assert_eq!(start.status(), 200);
    assert_eq!(value(start).await["remaining"], 0);
    assert!(!f.dir.join("config/backups").exists());
    assert!(f.dir.join("models/only.3mf").exists());
}

#[tokio::test]
async fn folder_rename_and_move_with_a_companion_row_outside_the_folder_are_rejected() {
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
    let a = value(f.invoke("create_folder", json!({"parentId":parent,"name":"A"})).await).await;
    let other = value(f.invoke("create_folder", json!({"parentId":parent,"name":"Other"})).await).await;
    let folder_id: i64 = a.as_str().unwrap().parse().unwrap();
    let model = f.dir.join("models/A/m.3mf");
    std::fs::write(&model, b"model").unwrap();
    let file_id = {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        db::test_insert_minimal_file(&c, model.to_str().unwrap(), Some(folder_id)).unwrap()
    };
    add_companion(&f, file_id);
    for (cmd, args) in [
        ("rename_folder", json!({"folderId":a,"name":"B"})),
        ("move_folder", json!({"folderId":a,"newParentId":other})),
    ] {
        let r = f.invoke(cmd, args).await;
        assert_eq!(r.status(), 409, "{cmd}");
        assert_eq!(value(r).await["kind"], "has_companions", "{cmd}");
    }
    assert!(model.exists());
}

#[tokio::test]
async fn folder_rename_and_move_carry_companions_and_rewrite_their_paths() {
    let f = Fixture::new(true).await;
    let parent = {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("INSERT INTO folders(name,path) VALUES('root',?1)", [f.dir.join("models").to_str().unwrap()]).unwrap();
        c.last_insert_rowid().to_string()
    };
    let a = value(f.invoke("create_folder", json!({"parentId":parent,"name":"A"})).await).await;
    let other = value(f.invoke("create_folder", json!({"parentId":parent,"name":"Other"})).await).await;
    let folder_id: i64 = a.as_str().unwrap().parse().unwrap();
    let model = f.dir.join("models/A/m.3mf");
    std::fs::write(&model, b"model").unwrap();
    let file_id = {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        db::test_insert_minimal_file(&c, model.to_str().unwrap(), Some(folder_id)).unwrap()
    };
    let (cid, _) = add_real_companion(&f, &file_id.to_string(), "A/m_extras", "pic.png");
    let r = f.invoke("rename_folder", json!({"folderId":a,"name":"B"})).await;
    assert_eq!(r.status(), 200);
    assert_eq!(companion_path(&f, cid), f.dir.join("models/B/m_extras/pic.png").to_str().unwrap());
    let r = f.invoke("move_folder", json!({"folderId":a,"newParentId":other})).await;
    assert_eq!(r.status(), 200);
    let moved = f.dir.join("models/Other/B/m_extras/pic.png");
    assert!(moved.is_file());
    assert_eq!(companion_path(&f, cid), moved.to_str().unwrap());
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    let model_path: String = conn.query_row("SELECT path FROM files WHERE id=?1", [file_id], |r| r.get(0)).unwrap();
    assert_eq!(model_path, f.dir.join("models/Other/B/m.3mf").to_str().unwrap());
}

#[tokio::test]
async fn folder_rename_with_a_symlinked_extras_folder_stays_a_409_has_companions() {
    let f = Fixture::new(true).await;
    let parent = {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("INSERT INTO folders(name,path) VALUES('root',?1)", [f.dir.join("models").to_str().unwrap()]).unwrap();
        c.last_insert_rowid().to_string()
    };
    let a = value(f.invoke("create_folder", json!({"parentId":parent,"name":"A"})).await).await;
    let folder_id: i64 = a.as_str().unwrap().parse().unwrap();
    let model = f.dir.join("models/A/m.3mf");
    std::fs::write(&model, b"model").unwrap();
    let file_id = {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        db::test_insert_minimal_file(&c, model.to_str().unwrap(), Some(folder_id)).unwrap()
    };
    let (cid, real) = add_real_companion(&f, &file_id.to_string(), "real", "pic.png");
    std::os::unix::fs::symlink(real.parent().unwrap(), f.dir.join("models/A/m_extras")).unwrap();
    let linked = f.dir.join("models/A/m_extras/pic.png");
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("UPDATE companion_files SET path=?1 WHERE id=?2", rusqlite::params![linked.to_str().unwrap(), cid]).unwrap();
    }
    let r = f.invoke("rename_folder", json!({"folderId":a,"name":"B"})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(value(r).await["kind"], "has_companions");
    assert!(model.exists() && !f.dir.join("models/B").exists());
    assert_eq!(companion_path(&f, cid), linked.to_str().unwrap());
}

fn trash_names(f: &Fixture) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(f.dir.join("models/.katalog-papierkorb"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

fn count(f: &Fixture, table: &str) -> i64 {
    let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

#[tokio::test]
async fn trash_restore_and_permanent_delete_take_companions_along() {
    let f = Fixture::new(true).await;
    let id = f.file("c.3mf");
    let (cid, old) = add_real_companion(&f, &id, "c_extras", "pic.png");

    let r = f.invoke("delete_files", json!({"fileIds":[id]})).await;
    assert_eq!(r.status(), 200);
    let v = value(r).await;
    assert_eq!(v[0]["ok"], true, "{v}");
    assert_eq!(v[0]["companions_moved"], 1);
    assert!(!old.exists() && !f.dir.join("models/c_extras").exists());
    assert_eq!(trash_names(&f).len(), 2);
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    assert_eq!((summary["count"].clone(), summary["companionCount"].clone(), summary["blockedByCompanions"].clone()), (json!(1), json!(1), json!(0)));
    // The minimal model row records no size; the 8 bytes are the companion file.
    assert_eq!(summary["bytes"], 8);

    let r = f.invoke("restore_file", json!({"fileId":id})).await;
    assert_eq!(r.status(), 200);
    assert!(old.is_file() && f.dir.join("models/c.3mf").exists());
    assert_eq!(companion_path(&f, cid), old.to_str().unwrap());
    assert!(trash_names(&f).is_empty());

    assert_eq!(f.invoke("delete_files", json!({"fileIds":[id]})).await.status(), 200);
    let r = f.invoke("delete_file_permanently", json!({"fileId":id,"confirm":true,"password":"test-password-long"})).await;
    assert_eq!(r.status(), 200);
    assert!(trash_names(&f).is_empty());
    // The real server connection enforces the cascade: no row outlives its model.
    let fk: i64 = f.app.writer.as_ref().unwrap().conn.lock().unwrap().query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
    assert_eq!(fk, 1);
    assert_eq!((count(&f, "files"), count(&f, "companion_files")), (0, 0));
}

#[tokio::test]
async fn empty_trash_removes_companions_and_leaves_desktop_entries_blocked() {
    let f = Fixture::new(true).await;
    let web = f.file("web.3mf");
    add_real_companion(&f, &web, "web_extras", "pic.png");
    let desk = f.file("desk.3mf");
    add_real_companion(&f, &desk, "desk_extras", "d.png");
    assert_eq!(f.invoke("delete_files", json!({"fileIds":[web]})).await.status(), 200);
    {
        // The desktop moved this one to its own trash.
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("UPDATE files SET deleted_at='2026-01-02T00:00:00Z', trash_path='/desktop/trash/2-desk.3mf' WHERE name='desk.3mf'", []).unwrap();
        c.execute("UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z', trash_path='/desktop/trash/companion-2-d.png' WHERE name='d.png'", []).unwrap();
    }
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    assert_eq!((summary["count"].clone(), summary["companionCount"].clone(), summary["blockedByCompanions"].clone()), (json!(1), json!(1), json!(1)));
    let r = f.invoke("delete_file_permanently", json!({"fileId":desk,"confirm":true,"password":"test-password-long"})).await;
    assert_eq!(r.status(), 409);
    assert_eq!(value(r).await["kind"], "has_companions");
    let start = value(
        f.invoke("empty_trash_start", json!({"expectedCount":1,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"})).await,
    )
    .await;
    let block = f.invoke("empty_trash_block", json!({"token":start["token"]})).await;
    assert_eq!(block.status(), 200);
    let block = value(block).await;
    assert_eq!(block["remaining"], 0);
    assert_eq!(block["results"][0]["ok"], true);
    assert_eq!(block["results"][0]["companions_moved"], 1);
    assert!(trash_names(&f).is_empty());
    assert_eq!((count(&f, "files"), count(&f, "companion_files")), (1, 1));
    assert!(f.dir.join("models/desk_extras/d.png").is_file(), "the desktop entry's file is not touched");
}

#[tokio::test]
async fn restore_reports_companions_that_stayed_in_the_trash() {
    let f = Fixture::new(true).await;
    let id = f.file("c.3mf");
    add_real_companion(&f, &id, "c_extras", "pic.png");
    assert_eq!(f.invoke("delete_files", json!({"fileIds":[id]})).await.status(), 200);
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("UPDATE companion_files SET trash_path='/desktop/trash/companion-1-pic.png'", []).unwrap();
    }
    let r = f.invoke("restore_file", json!({"fileId":id})).await;
    assert_eq!(r.status(), 200);
    assert_eq!(value(r).await["companions_kept_in_trash"], 1);
    assert!(f.dir.join("models/c.3mf").exists());
    assert!(!f.dir.join("models/c_extras/pic.png").exists());
}

#[tokio::test]
async fn folder_list_marks_protected_move_targets_like_the_move_does() {
    let f = Fixture::new(true).await;
    let id = f.file("m.3mf");
    std::fs::create_dir_all(f.dir.join("models/sub")).unwrap();
    let paths = ["models/sub", "models/uploads", "models/.katalog-papierkorb"];
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        for (i, p) in paths.iter().enumerate() {
            c.execute("INSERT INTO folders(name,path) VALUES(?1,?2)", [&format!("f{i}"), f.dir.join(p).to_str().unwrap()]).unwrap();
        }
    }
    let list = value(f.invoke("list_folders", json!({})).await).await;
    let flag = |name: &str| list.as_array().unwrap().iter().find(|x| x["name"] == name).unwrap()["moveTarget"].clone();
    assert_eq!(flag("f0"), true, "{list}");
    assert_eq!(flag("f1"), false, "{list}");
    assert_eq!(flag("f2"), false, "{list}");
    let ids: Vec<String> = list.as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap().to_owned()).collect();
    for i in [1, 2] {
        let r = f.invoke("move_files_to_folder", json!({"fileIds":[id],"folderId":ids[i]})).await;
        let body = value(r).await;
        assert_ne!(body[0]["ok"], true, "{body}");
    }
}

#[tokio::test]
async fn folder_list_keeps_every_target_without_file_management() {
    let f = Fixture::new(false).await;
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("INSERT INTO folders(name,path) VALUES('u',?1)", [f.dir.join("models/uploads").to_str().unwrap()]).unwrap();
    }
    let list = value(f.invoke("list_folders", json!({})).await).await;
    assert_eq!(list[0]["moveTarget"], true, "{list}");
}

async fn folder_list_flags_a_folder_deleted_from_disk(file_ops: bool) {
    let f = Fixture::new(file_ops).await;
    f.file("m.3mf");
    std::fs::create_dir_all(f.dir.join("models/here")).unwrap();
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        for p in ["models/here", "models/gone"] {
            c.execute("INSERT INTO folders(name,path) VALUES(?1,?2)", [p, f.dir.join(p).to_str().unwrap()]).unwrap();
        }
    }
    let list = value(f.invoke("list_folders", json!({})).await).await;
    let missing = |name: &str| list.as_array().unwrap().iter().find(|x| x["name"] == name).unwrap()["missing"].clone();
    assert_eq!(missing("models/here"), false, "{list}");
    assert_eq!(missing("models/gone"), true, "{list}");
}

#[tokio::test]
async fn folder_list_flags_missing_folders_with_file_management() {
    folder_list_flags_a_folder_deleted_from_disk(true).await;
}

#[tokio::test]
async fn folder_list_flags_missing_folders_in_the_plain_read_path() {
    folder_list_flags_a_folder_deleted_from_disk(false).await;
}

// ---- "Als STL speichern" als Download ----

impl Fixture {
    fn typed_file(&self, name: &str, file_type: &str, content: &[u8]) -> String {
        let id = self.file(name);
        std::fs::write(self.dir.join("models").join(name), content).unwrap();
        self.app.writer.as_ref().unwrap().conn.lock().unwrap().execute("UPDATE files SET file_type=?1 WHERE id=?2", [file_type, &id]).unwrap();
        id
    }
    async fn stl(&self, id: &str, query: &str) -> Response {
        router(self.app.clone()).oneshot(self.request("GET", &format!("/api/files/{id}/stl{query}"), Body::empty())).await.unwrap()
    }
}

#[tokio::test]
async fn an_obj_is_downloaded_as_a_binary_stl_through_the_core_writer() {
    let f = Fixture::new(true).await;
    let id = f.typed_file("Wuerfel.obj", "obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
    let r = f.stl(&id, "").await;
    assert_eq!(r.status(), 200);
    let disposition = r.headers()["content-disposition"].to_str().unwrap().to_owned();
    assert!(disposition.starts_with("attachment;") && disposition.contains("Wuerfel.stl"), "{disposition}");
    let bytes = to_bytes(r.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(bytes.len(), 84 + 50);
    assert!(bytes.starts_with(b"Binary STL export"));
    assert_eq!(u32::from_le_bytes(bytes[80..84].try_into().unwrap()), 1);
}

#[tokio::test]
async fn an_obj_is_downloaded_as_ascii_stl_on_request_and_binary_stays_the_default() {
    let f = Fixture::new(true).await;
    let id = f.typed_file("Wuerfel.obj", "obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
    let r = f.stl(&id, "?format=ascii").await;
    assert_eq!(r.status(), 200);
    let disposition = r.headers()["content-disposition"].to_str().unwrap().to_owned();
    assert!(disposition.contains("Wuerfel.stl"), "{disposition}");
    let bytes = to_bytes(r.into_body(), 1024 * 1024).await.unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.starts_with("solid ") && text.contains("facet normal") && text.trim_end().lines().last().unwrap().starts_with("endsolid"));
    assert_eq!(text.matches("vertex ").count(), 3);
    let explicit = to_bytes(f.stl(&id, "?format=binary").await.into_body(), 1024 * 1024).await.unwrap();
    let default = to_bytes(f.stl(&id, "").await.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(explicit, default);
    assert_eq!(default.len(), 84 + 50);
    let reversed = f.stl(&id, "?format=ascii&plate=1").await;
    assert_eq!(reversed.status(), f.stl(&id, "?plate=1&format=ascii").await.status());
}

#[tokio::test]
async fn stl_download_refuses_what_it_cannot_convert_with_codes() {
    let f = Fixture::new(true).await;
    let stl = f.typed_file("a.stl", "stl", b"solid x\nendsolid x\n");
    let step = f.typed_file("a.stp", "stp", b"ISO-10303-21;");
    let obj = f.typed_file("b.obj", "obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
    for (id, query, status, code) in [
        (stl.as_str(), "", 422, "stlSourceIsStl"),
        (step.as_str(), "", 422, "stlStepUnavailable"),
        ("999999", "", 404, "fileNotFound"),
        ("x1", "", 400, "reqInvalidIds"),
        (obj.as_str(), "?plate=0", 400, "reqInvalidArguments"),
        (obj.as_str(), "?plate=x", 400, "reqInvalidArguments"),
        (obj.as_str(), "?other=1", 400, "reqInvalidArguments"),
        (obj.as_str(), "?plate=7", 422, "stlNothingToExport"),
        (obj.as_str(), "?format=text", 400, "reqInvalidArguments"),
        (obj.as_str(), "?format=", 400, "reqInvalidArguments"),
        (obj.as_str(), "?format=ASCII", 400, "reqInvalidArguments"),
        (obj.as_str(), "?format=ascii&format=binary", 400, "reqInvalidArguments"),
        (obj.as_str(), "?format=ascii&plate=0", 400, "reqInvalidArguments"),
        (obj.as_str(), "?format=ascii&x=1", 400, "reqInvalidArguments"),
        (obj.as_str(), "?format=ascii&", 400, "reqInvalidArguments"),
        (obj.as_str(), "?plate=7&format=ascii", 422, "stlNothingToExport"),
    ] {
        let r = f.stl(id, query).await;
        assert_eq!(r.status(), status, "{id}{query}");
        assert_eq!(value(r).await["message"], code, "{id}{query}");
    }
}

#[tokio::test]
async fn stl_download_needs_a_session_and_a_get() {
    let f = Fixture::new(true).await;
    let id = f.typed_file("c.obj", "obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
    let anonymous = Request::builder().method("GET").uri(format!("/api/files/{id}/stl")).header("host", "localhost:8080").body(Body::empty()).unwrap();
    assert_eq!(router(f.app.clone()).oneshot(anonymous).await.unwrap().status(), 401);
    let post = f.request("POST", &format!("/api/files/{id}/stl"), Body::empty());
    assert_eq!(router(f.app.clone()).oneshot(post).await.unwrap().status(), 405);
}

// ---- Einzelne Begleitdateien: entfernen, wiederherstellen, Papierkorb leeren ----

const PNG: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52];

impl Fixture {
    /// A model with one companion picture; returns (model id, companion id).
    fn with_companion(&self, model: &str, name: &str) -> (String, String) {
        let id = self.file(model);
        let stem = model.trim_end_matches(".3mf");
        let extras = self.dir.join("models").join(format!("{stem}_extras"));
        std::fs::create_dir_all(&extras).unwrap();
        let path = extras.join(name);
        std::fs::write(&path, PNG).unwrap();
        let conn = self.app.writer.as_ref().unwrap().conn.lock().unwrap();
        let cid = db::companions::insert_companion(&conn, id.parse().unwrap(), name, path.to_str().unwrap(), "image", Some(PNG.len() as i64), "2026-01-01T00:00:00Z").unwrap();
        (id, cid.to_string())
    }
    async fn invoke_id(&self, cmd: &str, args: Value, request_id: &str) -> Response {
        let mut req = self.request("POST", &format!("/api/invoke/{cmd}"), Body::from(args.to_string()));
        req.headers_mut().insert("content-type", "application/json".parse().unwrap());
        req.headers_mut().insert("x-request-id", request_id.parse().unwrap());
        router(self.app.clone()).oneshot(req).await.unwrap()
    }
    fn row(&self, cid: &str) -> (Option<String>, Option<String>, String) {
        let conn = self.app.writer.as_ref().unwrap().conn.lock().unwrap();
        conn.query_row("SELECT deleted_at,trash_path,path FROM companion_files WHERE id=?1", [cid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap()
    }
    fn trash_entries(&self) -> Vec<String> {
        std::fs::read_dir(self.dir.join("models").join(katalog_core::fsops::TRASH))
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default()
    }
}

#[tokio::test]
async fn a_companion_is_removed_and_restored_with_a_backup_and_a_counted_budget() {
    let f = Fixture::new(true).await;
    let (_, cid) = f.with_companion("a.3mf", "pic.png");
    let r = f.invoke("remove_companion", json!({"companionId": cid})).await;
    assert_eq!(r.status(), 200);
    assert!(!f.dir.join("models/a_extras/pic.png").exists());
    let (deleted, trash, _) = f.row(&cid);
    assert!(deleted.is_some() && trash.is_some());
    assert_eq!(f.trash_entries().len(), 1);
    assert!(f.dir.join("config/backups").exists(), "a backup comes before the removal");
    let r = f.invoke("restore_companion", json!({"companionId": cid})).await;
    assert_eq!(r.status(), 200);
    assert!(f.dir.join("models/a_extras/pic.png").is_file());
    assert_eq!(f.row(&cid).0, None);
    assert!(f.trash_entries().is_empty());
}

#[tokio::test]
async fn the_removal_counts_against_the_deletion_budget_and_a_refusal_costs_nothing() {
    let f = Fixture::new(true).await;
    *f.app.fileops_safety.lock().unwrap() = crate::fileops_safety::FileOpsSafety::new(1);
    let (_, a) = f.with_companion("a.3mf", "pic.png");
    let (_, b) = f.with_companion("b.3mf", "pic.png");
    // Refusals first: unknown and already restored rows must not spend the budget.
    assert_eq!(f.invoke("remove_companion", json!({"companionId": "999999"})).await.status(), 404);
    assert_eq!(f.invoke("remove_companion", json!({"companionId": a})).await.status(), 200);
    assert_eq!(f.invoke("remove_companion", json!({"companionId": a})).await.status(), 409, "already removed");
    let denied = f.invoke("remove_companion", json!({"companionId": b})).await;
    assert!(matches!(denied.status().as_u16(), 422 | 429), "{}", denied.status());
    assert!(f.dir.join("models/b_extras/pic.png").is_file(), "the second file is untouched");
    assert_eq!(f.row(&b).0, None);
}

#[tokio::test]
async fn companion_edit_arguments_and_conflicts_have_their_own_codes() {
    let f = Fixture::new(true).await;
    let (model, cid) = f.with_companion("a.3mf", "pic.png");
    for (cmd, args, status, kind) in [
        ("remove_companion", json!({"companionId": cid, "extra": 1}), 422, "unknown_argument"),
        ("remove_companion", json!({}), 400, "invalid_arguments"),
        ("remove_companion", json!({"companionId": 5}), 400, "invalid_arguments"),
        ("remove_companion", json!({"companionId": "0"}), 400, "invalid_arguments"),
        ("remove_companion", json!({"companionId": "x1"}), 400, "invalid_arguments"),
        ("restore_companion", json!({"companionId": cid}), 409, "conflict"),
        ("restore_companion", json!({"companionId": "424242"}), 404, "not_found"),
    ] {
        let r = f.invoke(cmd, args).await;
        assert_eq!(r.status(), status, "{cmd}");
        assert_eq!(value(r).await["kind"], kind);
    }
    // The model in the trash: neither remove nor restore of its companions.
    assert_eq!(f.invoke("delete_files", json!({"fileIds":[model]})).await.status(), 200);
    assert_eq!(value(f.invoke("remove_companion", json!({"companionId": cid})).await).await["kind"], "conflict");
    // A file the desktop trashed stays the desktop's.
    let (_, other) = f.with_companion("b.3mf", "pic.png");
    {
        let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        conn.execute("UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z', trash_path='/desktop/trash/companion-9-x.png' WHERE id=?1", [&other]).unwrap();
    }
    let r = f.invoke("restore_companion", json!({"companionId": other})).await;
    assert_eq!((r.status().as_u16(), value(r).await["kind"].clone()), (409, json!("trash_foreign")));
}

#[tokio::test]
async fn a_full_model_refuses_a_restore_with_companion_limit() {
    let f = Fixture::new(true).await;
    let (model, cid) = f.with_companion("a.3mf", "pic.png");
    assert_eq!(f.invoke("remove_companion", json!({"companionId": cid})).await.status(), 200);
    {
        let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        for n in 0..100 {
            let path = f.dir.join(format!("models/a_extras/n{n}.png"));
            db::companions::insert_companion(&conn, model.parse().unwrap(), &format!("n{n}.png"), path.to_str().unwrap(), "image", None, "t").unwrap();
        }
    }
    let r = f.invoke("restore_companion", json!({"companionId": cid})).await;
    assert_eq!((r.status().as_u16(), value(r).await["kind"].clone()), (422, json!("companion_limit")));
    assert_eq!(f.trash_entries().len(), 1);
}

#[tokio::test]
async fn companion_edits_need_a_session_csrf_and_origin() {
    let f = Fixture::new(true).await;
    let (_, cid) = f.with_companion("a.3mf", "pic.png");
    for cmd in ["remove_companion", "restore_companion"] {
        for (remove, status) in [("cookie", 401), ("x-csrf-token", 403), ("origin", 403)] {
            let mut req = f.request("POST", &format!("/api/invoke/{cmd}"), Body::from(json!({"companionId": cid}).to_string()));
            req.headers_mut().insert("content-type", "application/json".parse().unwrap());
            req.headers_mut().remove(remove);
            assert_eq!(router(f.app.clone()).oneshot(req).await.unwrap().status(), status, "{cmd} without {remove}");
        }
    }
    assert!(f.dir.join("models/a_extras/pic.png").is_file());
    assert_eq!(f.row(&cid).0, None);
}

#[tokio::test]
async fn companion_edits_are_not_available_with_writing_off() {
    let f = Fixture::new(false).await;
    for cmd in ["remove_companion", "restore_companion"] {
        assert_eq!(f.invoke(cmd, json!({"companionId": "1"})).await.status(), 404, "{cmd}");
    }
}

#[tokio::test]
async fn a_repeated_request_id_gets_the_stored_answer_and_does_nothing_twice() {
    let f = Fixture::new(true).await;
    let (_, cid) = f.with_companion("a.3mf", "pic.png");
    let first = f.invoke_id("remove_companion", json!({"companionId": cid}), "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(first.status(), 200);
    let again = f.invoke_id("remove_companion", json!({"companionId": cid}), "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(again.status(), 200, "a plain retry would have been 'already removed'");
    assert_eq!(f.trash_entries().len(), 1);
    // The same id for other arguments, or for the other command, is a mistake.
    let reused = f.invoke_id("remove_companion", json!({"companionId": "77"}), "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(reused.status(), 422);
    assert_eq!(value(reused).await["kind"], "request_id_reused");
    let other = f.invoke_id("restore_companion", json!({"companionId": cid}), "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(other.status(), 422);
    assert_eq!(f.trash_entries().len(), 1);
    // A new id restores; its retry does not make a second copy.
    let restore = f.invoke_id("restore_companion", json!({"companionId": cid}), "22222222-2222-4222-8222-222222222222").await;
    assert_eq!(restore.status(), 200);
    let restore_again = f.invoke_id("restore_companion", json!({"companionId": cid}), "22222222-2222-4222-8222-222222222222").await;
    assert_eq!(restore_again.status(), 200);
    assert!(f.dir.join("models/a_extras/pic.png").is_file());
    assert!(!f.dir.join("models/a_extras/pic (2).png").exists());
    // A malformed id is refused before anything runs.
    let bad = f.invoke_id("remove_companion", json!({"companionId": cid}), "no spaces allowed").await;
    assert_eq!(bad.status(), 400);
    assert_eq!(value(bad).await["kind"], "request_id_invalid");
    assert!(f.dir.join("models/a_extras/pic.png").is_file());
}

#[tokio::test]
async fn emptying_the_trash_takes_removed_companions_along_and_checks_their_number() {
    let f = Fixture::new(true).await;
    let (_, cid) = f.with_companion("a.3mf", "pic.png");
    let (_, keep) = f.with_companion("b.3mf", "keep.png");
    assert_eq!(f.invoke("remove_companion", json!({"companionId": cid})).await.status(), 200);
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    assert_eq!((summary["count"].clone(), summary["looseCompanionCount"].clone()), (json!(0), json!(1)));
    assert_eq!(summary["looseCompanionBytes"], PNG.len());
    let start = |expected: Option<u64>| {
        let mut args = json!({"expectedCount":0,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"});
        if let Some(n) = expected {
            args["expectedCompanions"] = json!(n);
        }
        f.invoke("empty_trash_start", args)
    };
    // A client that does not know the field, or the wrong number: nothing starts.
    let r = start(None).await;
    assert_eq!((r.status().as_u16(), value(r).await["kind"].clone()), (409, json!("conflict")));
    f.time.0.store(200, Ordering::SeqCst);
    let r = start(Some(2)).await;
    assert_eq!(r.status(), 409);
    f.time.0.store(300, Ordering::SeqCst);
    let started = start(Some(1)).await;
    assert_eq!(started.status(), 200);
    let started = value(started).await;
    assert_eq!(started["remaining"], 1);
    let block = value(f.invoke("empty_trash_block", json!({"token": started["token"]})).await).await;
    assert_eq!(block["remaining"], 0);
    assert_eq!(block["results"][0]["ok"], true);
    assert!(f.trash_entries().is_empty());
    let count: i64 = f.app.writer.as_ref().unwrap().conn.lock().unwrap().query_row("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "only the active companion of the other model is left");
    assert_eq!(f.row(&keep).0, None);
}

#[tokio::test]
async fn the_listing_shows_removed_companions_with_their_state_but_no_path() {
    let f = Fixture::new(true).await;
    let (model, cid) = f.with_companion("a.3mf", "pic.png");
    {
        let path = f.dir.join("models/a_extras/two.png");
        std::fs::write(&path, PNG).unwrap();
        let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        db::companions::insert_companion(&conn, model.parse().unwrap(), "two.png", path.to_str().unwrap(), "image", None, "t").unwrap();
    }
    assert_eq!(f.invoke("remove_companion", json!({"companionId": cid})).await.status(), 200);
    let plain = f.invoke("list_companions", json!({"fileId": model})).await;
    assert!(!serde_json::to_string(&value(plain).await).unwrap().contains("deletedAt"), "the plain listing hides removed files");
    let r = f.invoke("list_companions", json!({"fileId": model, "includeRemoved": true})).await;
    assert_eq!(r.status(), 200);
    let text = serde_json::to_string(&value(r).await).unwrap();
    assert!(text.contains("deletedAt") && text.contains("\"restorable\":true"), "{text}");
    assert!(!text.contains(f.dir.to_str().unwrap()) && !text.contains("trash_path"));
}

// ---- Nachprüfung: Audit, Berechtigung der Liste, Backup-Zählung ----

impl Fixture {
    /// Runs a file operation directly so the test can read what the audit would log.
    async fn invoke_audited(&self, cmd: &str, args: Value) -> (Response, Arc<crate::write::WriteProgress>) {
        let progress = Arc::new(crate::write::WriteProgress::default());
        let permit = self.app.writer.as_ref().unwrap().admission.clone().try_acquire_owned().unwrap();
        let response = fileops::invoke(self.app.clone(), self.session.clone(), "peer".into(), cmd.into(), args, permit, progress.clone()).await;
        (response, progress)
    }
}

#[tokio::test]
async fn the_audit_names_model_and_companion_ids_for_removals_and_for_emptying() {
    let f = Fixture::new(true).await;
    let (model, cid) = f.with_companion("a.3mf", "pic.png");
    let (r, p) = f.invoke_audited("remove_companion", json!({"companionId": cid})).await;
    assert_eq!(r.status(), 200);
    assert_eq!(*p.file_ids.lock().unwrap(), vec![model.parse::<i64>().unwrap()]);
    assert_eq!(*p.companion_ids.lock().unwrap(), vec![cid.parse::<i64>().unwrap()]);
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    let (r, p) = f
        .invoke_audited("empty_trash_start", json!({"expectedCount":0,"expectedCompanions":1,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"}))
        .await;
    assert_eq!(r.status(), 200);
    assert_eq!(*p.companion_ids.lock().unwrap(), vec![cid.parse::<i64>().unwrap()]);
    let token = value(r).await["token"].clone();
    f.time.0.store(500, Ordering::SeqCst);
    let (r, p) = f.invoke_audited("empty_trash_block", json!({"token": token})).await;
    assert_eq!(r.status(), 200);
    assert_eq!(*p.companion_ids.lock().unwrap(), vec![cid.parse::<i64>().unwrap()]);
    assert!(p.file_ids.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_refused_block_still_logs_which_companion_files_it_was_about() {
    let f = Fixture::new(true).await;
    let (_, cid) = f.with_companion("a.3mf", "pic.png");
    let model = f.file("b.3mf");
    assert_eq!(f.invoke("remove_companion", json!({"companionId": cid})).await.status(), 200);
    assert_eq!(f.invoke("delete_files", json!({"fileIds":[model]})).await.status(), 200);
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    let token = value(f.invoke("empty_trash_start", json!({"expectedCount":1,"expectedCompanions":1,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"})).await).await["token"].clone();
    f.time.0.store(500, Ordering::SeqCst);
    assert_eq!(f.invoke("empty_trash_block", json!({"token": token})).await.status(), 200, "the model block");
    // Only the companion file is left, and the next block comes too soon.
    let (r, p) = f.invoke_audited("empty_trash_block", json!({"token": token})).await;
    assert_eq!(r.status(), 429);
    assert_eq!(*p.companion_ids.lock().unwrap(), vec![cid.parse::<i64>().unwrap()]);
}

#[tokio::test]
async fn the_removed_part_of_the_listing_needs_file_management_on_the_server() {
    let f = Fixture::new(false).await;
    let (model, cid) = f.with_companion("a.3mf", "pic.png");
    f.app.writer.as_ref().unwrap().conn.lock().unwrap()
        .execute("UPDATE companion_files SET deleted_at='2026-01-02T00:00:00Z' WHERE id=?1", [&cid]).unwrap();
    let r = f.invoke("list_companions", json!({"fileId": model, "includeRemoved": true})).await;
    assert_eq!(r.status(), 200);
    assert!(!serde_json::to_string(&value(r).await).unwrap().contains("deletedAt"));
}

#[tokio::test]
async fn emptying_only_removed_companions_still_makes_a_backup_at_start_and_per_block() {
    let f = Fixture::new(true).await;
    let (_, cid) = f.with_companion("a.3mf", "pic.png");
    assert_eq!(f.invoke("remove_companion", json!({"companionId": cid})).await.status(), 200);
    let backups = f.dir.join("config/backups");
    std::fs::remove_dir_all(&backups).unwrap();
    let summary = value(f.invoke("empty_trash_summary", json!({})).await).await;
    let started = f.invoke("empty_trash_start", json!({"expectedCount":0,"expectedCompanions":1,"maxDeletedAt":summary["maxDeletedAt"],"password":"test-password-long"})).await;
    assert_eq!(started.status(), 200);
    assert!(backups.exists(), "start counts the companion files");
    std::fs::remove_dir_all(&backups).unwrap();
    f.time.0.store(500, Ordering::SeqCst);
    let token = value(started).await["token"].clone();
    assert_eq!(f.invoke("empty_trash_block", json!({"token": token})).await.status(), 200);
    assert!(backups.exists(), "the block counts them too");
}

// ---- Begleitdatei hochladen (POST /api/companion-upload) ----

/// A body that hands out its frames and then either ends or never delivers another byte.
struct Scripted {
    frames: std::collections::VecDeque<Result<axum::body::Bytes, std::io::Error>>,
    stall: bool,
}
impl hyper::body::Body for Scripted {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;
    fn poll_frame(mut self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        match self.frames.pop_front() {
            Some(Ok(b)) => std::task::Poll::Ready(Some(Ok(hyper::body::Frame::data(b)))),
            Some(Err(e)) => std::task::Poll::Ready(Some(Err(e))),
            None if self.stall => std::task::Poll::Pending,
            None => std::task::Poll::Ready(None),
        }
    }
}

fn png_of(len: usize) -> Vec<u8> {
    let mut v = PNG.to_vec();
    v.resize(len.max(PNG.len()), 9);
    v
}

impl Fixture {
    fn upload_request(&self, file_id: &str, name: &str, length: Option<usize>, body: Body) -> Request {
        let mut req = self.request("POST", "/api/companion-upload", body);
        let h = req.headers_mut();
        h.insert("content-type", "application/octet-stream".parse().unwrap());
        h.insert("x-file-id", file_id.parse().unwrap());
        h.insert("x-upload-name", name.parse().unwrap());
        if let Some(n) = length {
            h.insert("content-length", n.to_string().parse().unwrap());
        }
        req
    }
    async fn upload(&self, file_id: &str, name: &str, bytes: Vec<u8>) -> Response {
        let req = self.upload_request(file_id, name, Some(bytes.len()), Body::from(bytes));
        router(self.app.clone()).oneshot(req).await.unwrap()
    }
    fn extras(&self, model_stem: &str) -> std::path::PathBuf {
        self.dir.join("models").join(format!("{model_stem}_extras"))
    }
    fn companion_count(&self) -> i64 {
        self.app.writer.as_ref().unwrap().conn.lock().unwrap().query_row("SELECT COUNT(*) FROM companion_files", [], |r| r.get(0)).unwrap()
    }
}

#[tokio::test]
async fn a_companion_is_uploaded_into_the_extras_folder_and_listed() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let bytes = png_of(5000);
    let r = f.upload(&id, "Bild%20%C3%A4.PNG", bytes.clone()).await;
    assert_eq!(r.status(), 200);
    let v = value(r).await;
    assert_eq!((v["name"].as_str(), v["kind"].as_str(), v["sizeBytes"].as_u64(), v["fileId"].as_str()), (Some("Bild \u{e4}.png"), Some("image"), Some(5000), Some(id.as_str())));
    assert_eq!(std::fs::read(f.extras("Foo").join("Bild \u{e4}.png")).unwrap(), bytes);
    assert_eq!(f.companion_count(), 1);
    let listed = value(f.invoke("list_companions", json!({"fileId": id})).await).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    // The same name again gets a number; nothing is replaced.
    let again = value(f.upload(&id, "Bild%20%C3%A4.png", png_of(10)).await).await;
    assert_eq!(again["name"], "Bild \u{e4} (2).png");
    // A download of what was uploaded works and carries the whole body.
    let cid = v["id"].as_str().unwrap();
    let req = f.request("GET", &format!("/api/companions/{cid}/download"), Body::empty());
    let d = router(f.app.clone()).oneshot(req).await.unwrap();
    assert_eq!(d.status(), 200);
    assert_eq!(to_bytes(d.into_body(), 1 << 20).await.unwrap().len(), 5000);
}

#[tokio::test]
async fn the_upload_route_takes_the_formats_that_are_stored_but_never_opened() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let kind = |r: Response| async move { (r.status().as_u16(), value(r).await["kind"].as_str().unwrap_or_default().to_owned()) };
    let zip = || {
        let mut v = b"PK\x03\x04".to_vec();
        v.resize(4000, 0);
        v
    };
    let accepted = value(f.upload(&id, "Model.FCStd", zip()).await).await;
    assert_eq!((accepted["name"].as_str(), accepted["kind"].as_str()), (Some("Model.fcstd"), Some("other")));
    assert_eq!(value(f.upload(&id, "part.scad", b"cube(10);".to_vec()).await).await["kind"], "other");
    assert_eq!(f.companion_count(), 2);
    // The download is an opaque attachment, whatever the name says.
    let cid = accepted["id"].as_str().unwrap();
    let d = router(f.app.clone()).oneshot(f.request("GET", &format!("/api/companions/{cid}/download"), Body::empty())).await.unwrap();
    assert_eq!(d.status(), 200);
    assert_eq!(d.headers()["content-type"], "application/octet-stream");
    assert!(d.headers()["content-disposition"].to_str().unwrap().starts_with("attachment;"));
    // Content must fit, and nothing outside the list gets in.
    assert_eq!(kind(f.upload(&id, "x.docx", b"hello".to_vec()).await).await, (422, "companion_content".into()));
    assert_eq!(kind(f.upload(&id, "x.csv", b"a\0b".to_vec()).await).await, (422, "companion_content".into()));
    assert_eq!(kind(f.upload(&id, "x.exe", zip()).await).await, (415, "companion_type".into()));
    assert_eq!(kind(f.upload(&id, "x.docm", zip()).await).await, (415, "companion_type".into()));
    assert_eq!(kind(f.upload(&id, "x.step", b"ISO-10303".to_vec()).await).await, (415, "companion_type".into()));
    assert_eq!(f.companion_count(), 2);
}

#[tokio::test]
async fn the_upload_route_has_the_same_gates_as_the_other_writes() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    for (remove, status) in [("cookie", 401), ("x-csrf-token", 403), ("origin", 403)] {
        let mut req = f.upload_request(&id, "a.png", Some(20), Body::from(png_of(20)));
        req.headers_mut().remove(remove);
        assert_eq!(router(f.app.clone()).oneshot(req).await.unwrap().status(), status, "{remove}");
    }
    let mut req = f.upload_request(&id, "a.png", Some(20), Body::from(png_of(20)));
    req.headers_mut().insert("host", "evil.example".parse().unwrap());
    assert_eq!(router(f.app.clone()).oneshot(req).await.unwrap().status(), 403);
    let req = f.request("GET", "/api/companion-upload", Body::empty());
    assert_ne!(router(f.app.clone()).oneshot(req).await.unwrap().status(), 200);
    assert_eq!(f.companion_count(), 0);
    assert!(!f.extras("Foo").exists());
    // Without file management on the server the route does not exist.
    let g = Fixture::new(false).await;
    let gid = g.file("Foo.3mf");
    assert_eq!(g.upload(&gid, "a.png", png_of(20)).await.status(), 404);
    assert!(!g.extras("Foo").exists());
}

#[tokio::test]
async fn headers_and_content_are_refused_with_their_own_codes_before_anything_is_written() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let kind = |r: Response| async move { (r.status().as_u16(), value(r).await["kind"].as_str().unwrap().to_owned()) };
    // Wrong content type, missing or bad length.
    let mut req = f.upload_request(&id, "a.png", Some(20), Body::from(png_of(20)));
    req.headers_mut().insert("content-type", "application/json".parse().unwrap());
    assert_eq!(kind(router(f.app.clone()).oneshot(req).await.unwrap()).await, (400, "content_type".into()));
    let req = f.upload_request(&id, "a.png", None, Body::from(png_of(20)));
    assert_eq!(kind(router(f.app.clone()).oneshot(req).await.unwrap()).await, (411, "length_required".into()));
    let mut req = f.upload_request(&id, "a.png", Some(20), Body::from(png_of(20)));
    req.headers_mut().insert("content-length", "2x".parse().unwrap());
    assert_eq!(kind(router(f.app.clone()).oneshot(req).await.unwrap()).await, (400, "length_invalid".into()));
    for bad in ["0", "-1", "x", "", "1 2"] {
        let mut req = f.upload_request(&id, "a.png", Some(20), Body::from(png_of(20)));
        req.headers_mut().insert("x-file-id", bad.parse().unwrap());
        assert_eq!(kind(router(f.app.clone()).oneshot(req).await.unwrap()).await, (400, "invalid_arguments".into()), "{bad}");
    }
    for bad in ["%zz", "%C3", "%FF%FE"] {
        assert_eq!(kind(f.upload(&id, bad, png_of(20)).await).await, (400, "name_invalid".into()), "{bad}");
    }
    // Size limit (1 MiB in this fixture): exactly at it is fine, one byte over is refused unread.
    assert_eq!(kind(f.upload(&id, "big.png", png_of(1024 * 1024 + 1)).await).await, (413, "companion_too_large".into()));
    assert!(!f.extras("Foo").exists());
    assert_eq!(kind(f.upload(&id, "x.exe", b"MZ".to_vec()).await).await, (415, "companion_type".into()));
    assert_eq!(kind(f.upload(&id, "x.svg", b"<svg/>".to_vec()).await).await, (415, "companion_type".into()));
    assert_eq!(kind(f.upload(&id, "x.jpg", png_of(30)).await).await, (422, "companion_content".into()));
    assert_eq!(kind(f.upload(&id, "x.txt", b"a\0b".to_vec()).await).await, (422, "companion_content".into()));
    assert_eq!(kind(f.upload(&id, "x.txt", Vec::new()).await).await, (422, "companion_content".into()));
    assert_eq!(kind(f.upload("99999", "x.png", png_of(30)).await).await, (404, "not_found".into()));
    assert!(!f.extras("Foo").exists() && f.companion_count() == 0);
    assert_eq!(f.upload(&id, "big.png", png_of(1024 * 1024)).await.status(), 200, "exactly the limit");
}

#[tokio::test]
async fn a_body_that_does_not_match_its_length_or_breaks_off_leaves_nothing_behind() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let code = |r: Response| async move { (r.status().as_u16(), value(r).await["kind"].as_str().unwrap().to_owned()) };
    // Shorter than announced, longer than announced.
    let req = f.upload_request(&id, "a.png", Some(100), Body::from(png_of(60)));
    assert_eq!(code(router(f.app.clone()).oneshot(req).await.unwrap()).await, (400, "length_mismatch".into()));
    let req = f.upload_request(&id, "a.png", Some(60), Body::from(png_of(100)));
    assert_eq!(code(router(f.app.clone()).oneshot(req).await.unwrap()).await, (400, "length_mismatch".into()));
    // The connection breaks after the head and a part of the body.
    let frames = [Ok(axum::body::Bytes::from(png_of(300))), Err(std::io::Error::from(std::io::ErrorKind::ConnectionReset))].into_iter().collect();
    let req = f.upload_request(&id, "a.png", Some(100_000), Body::new(Scripted { frames, stall: false }));
    assert_eq!(code(router(f.app.clone()).oneshot(req).await.unwrap()).await, (400, "length_mismatch".into()));
    assert_eq!(f.companion_count(), 0);
    let left: Vec<_> = std::fs::read_dir(f.dir.join("models")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(!left.iter().any(|n| n.ends_with("_extras")), "{left:?}");
    // The runtime is free again afterwards.
    assert_eq!(f.upload(&id, "ok.png", png_of(50)).await.status(), 200);
}

#[tokio::test]
async fn a_body_that_stalls_is_given_up_and_cleaned_away() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let frames = [Ok(axum::body::Bytes::from(png_of(300)))].into_iter().collect();
    let progress = Arc::new(WriteProgress::default());
    let admission = f.app.writer.as_ref().unwrap().admission.clone().try_acquire_owned().unwrap();
    let response = fileops::add_companion(
        f.app.clone(),
        f.session.clone(),
        id.parse().unwrap(),
        "a.png".into(),
        100_000,
        Body::new(Scripted { frames, stall: true }),
        progress,
        admission,
        std::time::Duration::from_millis(150),
    )
    .await;
    assert_eq!(response.status(), 408);
    assert_eq!(value(response).await["kind"], "upload_timeout");
    assert_eq!(f.companion_count(), 0);
    assert!(!f.extras("Foo").exists(), "the folder made for it is gone");
    assert_eq!(f.upload(&id, "ok.png", png_of(50)).await.status(), 200, "the slot and the runtime are free again");
}

#[tokio::test]
async fn only_one_companion_upload_runs_at_a_time() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let slot = f.app.fileops.as_ref().unwrap().companion_slot.clone().try_acquire_owned().unwrap();
    let r = f.upload(&id, "a.png", png_of(50)).await;
    assert_eq!(r.status(), 503);
    assert!(r.headers().contains_key("retry-after"));
    assert_eq!(value(r).await["kind"], "upload_busy");
    drop(slot);
    assert_eq!(f.upload(&id, "a.png", png_of(50)).await.status(), 200);
}

#[tokio::test]
async fn a_repeated_request_id_gets_the_stored_answer_and_adds_nothing_twice() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let send = |name: &'static str, rid: &'static str| {
        let mut req = f.upload_request(&id, name, Some(50), Body::from(png_of(50)));
        req.headers_mut().insert("x-request-id", rid.parse().unwrap());
        let app = f.app.clone();
        async move { router(app).oneshot(req).await.unwrap() }
    };
    let first = value(send("a.png", "11111111-1111-4111-8111-111111111111").await).await;
    let replay = send("a.png", "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(replay.status(), 200);
    assert_eq!(value(replay).await, first);
    assert_eq!(f.companion_count(), 1);
    assert!(!f.extras("Foo").join("a (2).png").exists());
    let reused = send("b.png", "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(reused.status(), 422);
    assert_eq!(value(reused).await["kind"], "request_id_reused");
    let mut bad = f.upload_request(&id, "a.png", Some(50), Body::from(png_of(50)));
    bad.headers_mut().insert("x-request-id", "not a key".parse().unwrap());
    assert_eq!(router(f.app.clone()).oneshot(bad).await.unwrap().status(), 400);
    // Without a request id the upload still works (the browser always sends one).
    assert_eq!(f.upload(&id, "c.png", png_of(50)).await.status(), 200);
}

#[tokio::test]
async fn a_blocked_runtime_and_a_full_model_are_reported_not_hidden() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    for n in 0..100 {
        let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        db::companions::insert_companion(&conn, id.parse().unwrap(), &format!("n{n}.png"), &format!("{}/n{n}.png", f.extras("Foo").display()), "image", Some(1), "t").unwrap();
    }
    let r = f.upload(&id, "x.png", png_of(50)).await;
    assert_eq!((r.status().as_u16(), value(r).await["kind"].as_str().unwrap().to_owned()), (422, "companion_limit".into()));
    f.app.fileops.as_ref().unwrap().block_for_test();
    assert_eq!(f.upload(&id, "x.png", png_of(50)).await.status(), 503);
}

// ---- Lange Uploads halten weder Laufzeit noch Import-Gate ----

/// A body fed from the test: frames arrive when the test sends them, and it ends when the sender is dropped.
struct Fed(tokio::sync::mpsc::Receiver<axum::body::Bytes>);
impl hyper::body::Body for Fed {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;
    fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        self.0.poll_recv(cx).map(|b| b.map(|b| Ok(hyper::body::Frame::data(b))))
    }
}

impl Fixture {
    /// Starts an upload whose body the test feeds; returns the sender and the pending response.
    fn start_fed_upload(&self, id: &str, name: &str, length: usize) -> (tokio::sync::mpsc::Sender<axum::body::Bytes>, tokio::task::JoinHandle<Response>) {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let (app, session, id, name) = (self.app.clone(), self.session.clone(), id.parse().unwrap(), name.to_owned());
        let admission = self.app.writer.as_ref().unwrap().admission.clone().try_acquire_owned().unwrap();
        let task = tokio::spawn(async move {
            fileops::add_companion(app, session, id, name, length as u64, Body::new(Fed(rx)), Arc::new(WriteProgress::default()), admission, std::time::Duration::from_secs(30)).await
        });
        (tx, task)
    }
    /// Waits until the upload has made its temporary file.
    async fn wait_for_temporary(&self, stem: &str, task: &mut tokio::task::JoinHandle<Response>) {
        for _ in 0..200 {
            let found = std::fs::read_dir(self.extras(stem)).into_iter().flatten().flatten().any(|e| e.file_name().to_string_lossy().starts_with(".fsops-"));
            if found {
                return;
            }
            if task.is_finished() {
                let r = task.await.unwrap();
                panic!("the upload ended before it made its file: {} {:?}", r.status(), value(r).await);
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("the upload never started");
    }
}

#[tokio::test]
async fn other_file_operations_run_while_a_companion_body_is_still_arriving() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let other = f.file("Bar.3mf");
    let bytes = png_of(900_000);
    let (tx, mut task) = f.start_fed_upload(&id, "big.png", bytes.len());
    tx.send(bytes[..300_000].to_vec().into()).await.unwrap();
    f.wait_for_temporary("Foo", &mut task).await;
    let rt = f.app.fileops.as_ref().unwrap();
    // The temporary file is made inside `begin_companion_add`, i.e. while the runtime is still held to start the
    // job; it is released right after. So wait (bounded) for the release, then require it stays free while the
    // body is still incomplete (the task is not finished and the second half has not been sent).
    let mut released = false;
    for _ in 0..800 {
        if rt.state.try_lock().is_ok() {
            released = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(released, "the runtime is not held while the body streams");
    assert!(!task.is_finished(), "the upload is still waiting for the rest of its body");
    assert!(rt.state.try_lock().is_ok(), "the runtime stays free while the body streams");
    if let Some(u) = &f.app.uploads {
        assert_eq!(u.active.available_permits(), 1, "the import gate is free");
    }
    let r = f.invoke("rename_file", json!({"fileId": other, "name": "Baz"})).await;
    assert_eq!(r.status(), 200, "{:?}", value(r).await);
    tx.send(bytes[300_000..].to_vec().into()).await.unwrap();
    drop(tx);
    let response = task.await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(std::fs::read(f.extras("Foo").join("big.png")).unwrap(), bytes);
    assert_eq!(f.companion_count(), 1);
}

#[tokio::test]
async fn a_session_that_ends_during_the_upload_leaves_no_file_and_no_row() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let bytes = png_of(900_000);
    let (tx, mut task) = f.start_fed_upload(&id, "big.png", bytes.len());
    tx.send(bytes[..300_000].to_vec().into()).await.unwrap();
    f.wait_for_temporary("Foo", &mut task).await;
    f.app.data.lock().unwrap().sessions.remove(&f.session.token);
    tx.send(bytes[300_000..].to_vec().into()).await.unwrap();
    drop(tx);
    let response = task.await.unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(value(response).await["kind"], "session_expired");
    assert_eq!(f.companion_count(), 0);
    assert!(!f.extras("Foo").exists(), "neither the file nor the folder made for it");
}

#[test]
fn a_full_disk_is_reported_as_507() {
    let e = katalog_core::fsops::Error::from(std::io::Error::from_raw_os_error(28));
    assert_eq!(fileops::core_error(e).status(), 507);
}

#[tokio::test]
async fn the_session_tells_the_browser_how_big_a_companion_file_may_be() {
    let f = Fixture::new(true).await;
    let req = f.request("GET", "/api/session", Body::empty());
    let v = value(router(f.app.clone()).oneshot(req).await.unwrap()).await;
    assert_eq!(v["companionMaxBytes"].as_u64(), Some(f.app.config.companion_max_bytes));
}

#[tokio::test]
async fn a_blocked_journal_names_only_entries_that_wait_for_a_person_not_an_upload_under_way() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let bytes = png_of(900_000);
    let (tx, mut task) = f.start_fed_upload(&id, "big.png", bytes.len());
    tx.send(bytes[..300_000].to_vec().into()).await.unwrap();
    f.wait_for_temporary("Foo", &mut task).await;
    let rt = f.app.fileops.as_ref().unwrap();
    assert_eq!(rt.pending_entries_for_test(), 1, "the upload is journalled in Prepared");
    rt.block_journal_for_test();
    let r = f.invoke("rename_file", json!({"fileId": f.file("Bar.3mf"), "name": "Baz"})).await;
    assert_eq!(r.status(), 503);
    let v = value(r).await;
    assert_eq!(v["kind"], "restart_required");
    assert_eq!(v["opIds"], json!([]), "recovery rolls the upload back itself; naming it would make the ACK fail");
    tx.send(bytes[300_000..].to_vec().into()).await.unwrap();
    drop(tx);
    assert_eq!(task.await.unwrap().status(), 503, "the upload is left for recovery");
}

/// Every file below `dir`, relative to it, sorted.
fn tree(dir: &std::path::Path) -> Vec<String> {
    fn walk(base: &std::path::Path, at: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(at).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.push(path.strip_prefix(base).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[tokio::test]
async fn a_path_in_the_upload_name_never_leaves_the_extras_folder() {
    let f = Fixture::new(true).await;
    let id = f.file("Foo.3mf");
    let before = tree(&f.dir.join("models"));
    for (sent, stored) in [
        ("..%2F..%2Fevil.png", "evil.png"),
        ("%2Fetc%2Fpasswd.png", "passwd.png"),
        ("..%5C..%5Cwin.png", "win.png"),
        ("sub%2F..%2Fdeep.png", "deep.png"),
    ] {
        let r = f.upload(&id, sent, png_of(20)).await;
        assert_eq!(r.status(), 200, "{sent}");
        assert_eq!(value(r).await["name"], stored, "{sent}");
        assert!(f.extras("Foo").join(stored).is_file(), "{sent}");
    }
    // Names that are nothing but a path or dots have no file name left to keep.
    for sent in ["..", "..%2F", "%2E%2E%2F.png.exe", "%2F"] {
        assert!(f.upload(&id, sent, png_of(20)).await.status().is_client_error(), "{sent}");
    }
    let added: Vec<_> = tree(&f.dir.join("models")).into_iter().filter(|p| !before.contains(p)).collect();
    assert!(added.iter().all(|p| p.starts_with("Foo_extras/")), "{added:?}");
    assert_eq!(added.len(), 4, "{added:?}");
}

#[tokio::test]
async fn maintenance_fileops_and_empty_trash_block_are_rejected() {
    let f = Fixture::new(true).await;
    let barrier = &f.app.geometry.barrier;
    let window = barrier.enter_draining(std::time::Duration::from_secs(1)).await.unwrap();
    for (command, args) in [
        ("empty_trash_block", json!({"token":"unused"})),
        ("rename_file", json!({"fileId":"1","name":"new.stl"})),
    ] {
        let response = f.invoke(command, args).await;
        assert_eq!(response.status(), 503);
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap()["message"], "maintenance");
    }
    assert_eq!(f.invoke("list_folders", json!({})).await.status(), 200);
    let window = window.enter_exclusive(std::time::Duration::from_secs(1)).await.unwrap();
    assert_eq!(f.invoke("list_folders", json!({})).await.status(), 503);
    window.release();
}

mod source_dirs_http_tests {
    include!("source_dirs_http_tests.rs");
}

/// A folder `big` with a subfolder `sub`: `top` models in `big`, `deep` in `big/sub`, real files on disk.
fn folder_with_models(f: &Fixture, top: usize, deep: usize) -> String {
    std::fs::create_dir_all(f.dir.join("models/big/sub")).unwrap();
    let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    c.execute("INSERT INTO folders(name,path) VALUES('big',?1)", [f.dir.join("models/big").to_str().unwrap()]).unwrap();
    let big = c.last_insert_rowid();
    c.execute("INSERT INTO folders(name,path,parent_id) VALUES('sub',?1,?2)", rusqlite::params![f.dir.join("models/big/sub").to_str().unwrap(), big]).unwrap();
    let sub = c.last_insert_rowid();
    for (count, dir, folder) in [(top, "big", big), (deep, "big/sub", sub)] {
        for n in 0..count {
            let path = f.dir.join(format!("models/{dir}/{n}.3mf"));
            std::fs::write(&path, format!("model {dir} {n}")).unwrap();
            db::test_insert_minimal_file(&c, path.to_str().unwrap(), Some(folder)).unwrap();
        }
    }
    big.to_string()
}

#[tokio::test]
async fn a_folder_goes_to_the_trash_in_throttled_blocks_and_comes_back_model_by_model() {
    let f = Fixture::new(true).await;
    let folder = folder_with_models(&f, 300, 150);
    let summary = value(f.invoke("folder_trash_summary", json!({"folderId":folder})).await).await;
    assert_eq!((summary["modelCount"].clone(), summary["subfolderCount"].clone(), summary["isCatalogRootOrAncestor"].clone()), (json!(450), json!(1), json!(false)));
    let start = value(f.invoke("trash_folder_start", json!({"folderId":folder})).await).await;
    assert_eq!(start["remaining"], 450);
    let token = start["token"].clone();
    // The first block runs at once, the next one has to wait the short pause with a real Retry-After.
    let first = value(f.invoke("trash_folder_block", json!({"token":token})).await).await;
    assert_eq!((first["results"].as_array().unwrap().len(), first["remaining"].clone()), (200, json!(250)));
    assert!(first.get("removedFolderCount").is_none());
    let early = f.invoke("trash_folder_block", json!({"token":token})).await;
    assert_eq!(early.status(), 429);
    assert!(early.headers().contains_key("retry-after"));
    f.time.0.store(103, Ordering::SeqCst);
    let second = value(f.invoke("trash_folder_block", json!({"token":token})).await).await;
    assert_eq!(second["remaining"], 50);
    f.time.0.store(106, Ordering::SeqCst);
    let last = value(f.invoke("trash_folder_block", json!({"token":token})).await).await;
    assert_eq!((last["remaining"].clone(), last["removedFolderCount"].clone()), (json!(0), json!(2)));
    // A finished token is gone.
    f.time.0.store(109, Ordering::SeqCst);
    assert_eq!(f.invoke("trash_folder_block", json!({"token":token})).await.status(), 404);
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        let n = |sql: &str| c.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(n("SELECT COUNT(*) FROM files WHERE deleted_at IS NOT NULL"), 450);
        assert_eq!(n("SELECT COUNT(*) FROM folder_trash_origin"), 450);
        assert_eq!(n("SELECT COUNT(*) FROM folders"), 0);
    }
    assert!(f.dir.join("config/backups").exists());
    // The branch is gone from the disk; restoring one model rebuilds its folders and clears its marker.
    std::fs::remove_dir_all(f.dir.join("models/big")).unwrap();
    let id: i64 = f.app.writer.as_ref().unwrap().conn.lock().unwrap()
        .query_row("SELECT id FROM files WHERE name='7.3mf' AND path LIKE '%/big/sub/%'", [], |r| r.get(0)).unwrap();
    assert_eq!(f.invoke("restore_file", json!({"fileId":id.to_string()})).await.status(), 200);
    assert!(f.dir.join("models/big/sub/7.3mf").exists());
    let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    assert_eq!(c.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
    assert_eq!(c.query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get::<_, i64>(0)).unwrap(), 449);
}

#[tokio::test]
async fn folder_trash_tokens_are_session_bound_and_the_catalog_base_is_refused() {
    let f = Fixture::new(true).await;
    let folder = folder_with_models(&f, 2, 1);
    let token = value(f.invoke("trash_folder_start", json!({"folderId":folder})).await).await["token"].clone();
    let other = { let mut d = f.app.data.lock().unwrap(); new_session(&mut d, "test", 100).unwrap() };
    let mut req = f.request("POST", "/api/invoke/trash_folder_block", Body::from(json!({"token":token}).to_string()));
    req.headers_mut().insert("content-type", "application/json".parse().unwrap());
    req.headers_mut().insert("cookie", format!("katalog_session={}", other.token).parse().unwrap());
    req.headers_mut().insert("x-csrf-token", other.csrf.parse().unwrap());
    assert_eq!(router(f.app.clone()).oneshot(req).await.unwrap().status(), 404);
    assert_eq!(f.invoke("trash_folder_block", json!({"token":"nope"})).await.status(), 404);
    // Nothing moved so far; with the folder set as catalog base, the start is refused with a 409.
    f.app.writer.as_ref().unwrap().conn.lock().unwrap()
        .execute("INSERT INTO app_settings(key,value) VALUES('catalog_base_dir',?1)", [f.dir.join("models/big").to_str().unwrap()]).unwrap();
    let refused = f.invoke("trash_folder_start", json!({"folderId":folder})).await;
    assert_eq!(refused.status(), 409);
    let summary = value(f.invoke("folder_trash_summary", json!({"folderId":folder})).await).await;
    assert_eq!(summary["isCatalogRootOrAncestor"], true);
    assert_eq!(f.app.writer.as_ref().unwrap().conn.lock().unwrap()
        .query_row("SELECT COUNT(*) FROM files WHERE deleted_at IS NOT NULL", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
}

#[tokio::test]
async fn folder_trash_commands_have_strict_arguments_and_an_unknown_folder_is_a_404() {
    let f = Fixture::new(true).await;
    assert_eq!(f.invoke("trash_folder_start", json!({"folderId":"999"})).await.status(), 404);
    assert_eq!(f.invoke("trash_folder_start", json!({"folderId":"1","extra":1})).await.status(), 422);
    assert_eq!(f.invoke("trash_folder_start", json!({"folderId":"x"})).await.status(), 400);
    assert_eq!(f.invoke("folder_trash_summary", json!({"folderId":"999"})).await.status(), 404);
    assert_eq!(f.invoke("folder_trash_summary", json!({"folderId":"1","x":1})).await.status(), 422);
    let off = Fixture::new(false).await;
    assert_eq!(off.invoke("trash_folder_start", json!({"folderId":"1"})).await.status(), 404);
}

/// W2 (Forgejo #39): the disk comparison as plan, start and blocks, and the date prefix.
mod disksync_tests {
    use super::*;
    use std::io::Write as _;
    use std::time::Duration;
    use std::os::unix::fs::symlink;

    const OBJ: &[u8] = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
    const STL: &[u8] = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
    fn three_mf() -> Vec<u8> {
        let xml = r#"<model unit="millimeter"><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="5" y="0" z="0"/><vertex x="0" y="5" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }
    impl Fixture {
        /// A catalog row for a file that is not on the disk.
        fn gone(&self, name: &str) -> String {
            let path = self.dir.join("models").join(name);
            db::test_insert_minimal_file(&self.app.writer.as_ref().unwrap().conn.lock().unwrap(), path.to_str().unwrap(), None)
                .unwrap()
                .to_string()
        }
        fn put(&self, name: &str, bytes: &[u8]) {
            let path = self.dir.join("models").join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        fn session_two(&self) -> Session {
            let mut d = self.app.data.lock().unwrap();
            new_session(&mut d, "test", 100).unwrap()
        }
        async fn invoke_as(&self, session: &Session, cmd: &str, args: Value) -> Response {
            let mut req = self.request("POST", &format!("/api/invoke/{cmd}"), Body::from(args.to_string()));
            req.headers_mut().insert("content-type", "application/json".parse().unwrap());
            req.headers_mut().insert("cookie", format!("katalog_session={}", session.token).parse().unwrap());
            req.headers_mut().insert("x-csrf-token", session.csrf.parse().unwrap());
            router(self.app.clone()).oneshot(req).await.unwrap()
        }
        async fn invoke_raw(&self, cmd: &str, body: String) -> Response {
            let mut req = self.request("POST", &format!("/api/invoke/{cmd}"), Body::from(body));
            req.headers_mut().insert("content-type", "application/json".parse().unwrap());
            router(self.app.clone()).oneshot(req).await.unwrap()
        }
        /// Preview, start (confirmed) and the token.
        async fn plan(&self, selection: Value) -> String {
            let r = self.invoke("disk_sync_preview", selection).await;
            assert_eq!(r.status(), 200);
            let token = value(r).await["token"].as_str().unwrap().to_owned();
            assert!(!token.is_empty());
            token
        }
        async fn started(&self, selection: Value) -> String {
            let token = self.plan(selection).await;
            let r = self.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
            assert_eq!(r.status(), 200);
            token
        }
        async fn block(&self, token: &str, seq: u64) -> Response {
            self.invoke("disk_sync_apply_block", json!({"token":token,"seq":seq})).await
        }
        fn state(&self, id: &str) -> (Option<String>, Option<String>) {
            let conn = self.app.writer.as_ref().unwrap().conn.lock().unwrap();
            conn.query_row("SELECT deleted_at, trash_path FROM files WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        }
    }
    fn reasons(v: &Value) -> Vec<String> {
        v["results"].as_array().unwrap().iter().map(|r| r["reason"].as_str().unwrap_or("ok").to_owned()).collect()
    }
    fn backups(f: &Fixture) -> bool {
        f.dir.join("config/backups").exists()
    }
    async fn kind(r: Response, status: u16) -> Value {
        assert_eq!(r.status(), status);
        value(r).await
    }

    #[tokio::test]
    async fn a_removal_goes_to_the_trash_state_without_touching_a_file() {
        let f = Fixture::new(true).await;
        let keep = f.file("keep.3mf");
        let id = f.gone("gone.3mf");
        let token = f.started(json!({"missingModels":[id]})).await;
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(reasons(&answer), ["ok"]);
        assert_eq!(answer["results"][0], json!({"kind":"removeModel","key":id,"ok":true}));
        assert_eq!((answer["done"].clone(), answer["remaining"].clone(), answer["stopped"].clone()), (json!(1), json!(0), Value::Null));
        assert_eq!(answer["counts"], json!({"imported":0,"removedModels":1,"removedCompanions":0,"skipped":0}));
        let (deleted_at, trash_path) = f.state(&id);
        assert!(deleted_at.is_some() && trash_path.is_none());
        assert!(f.state(&keep).0.is_none());
        assert!(trash_names(&f).is_empty());
        assert_eq!(f.app.fileops.as_ref().unwrap().pending_entries_for_test(), 0);
        assert!(backups(&f), "a backup exists before the first removal");
    }

    #[tokio::test]
    async fn a_block_without_a_successful_start_touches_nothing() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let id = f.gone("gone.3mf");
        // Directly after the preview.
        let token = f.plan(json!({"missingModels":[id]})).await;
        let r = f.block(&token, 0).await;
        assert_eq!(kind(r, 409).await["kind"], "not_started");
        assert!(f.state(&id).0.is_none() && !backups(&f));
        // The refused block did not cost the plan its sequence number or anything the budget holds.
        assert_eq!(f.app.fileops_safety.lock().unwrap().reserve_disk_sync(2000, 100), Ok(()));
        *f.app.fileops_safety.lock().unwrap() = crate::fileops_safety::FileOpsSafety::new(2000);
        // Start without the confirmation: 422, then the block is still refused.
        let r = f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":false})).await;
        assert_eq!(kind(r, 422).await["kind"], "confirmation_required");
        let r = f.invoke("disk_sync_apply_start", json!({"token":token})).await;
        assert_eq!(kind(r, 422).await["kind"], "confirmation_required");
        assert_eq!(kind(f.block(&token, 0).await, 409).await["kind"], "not_started");
        assert!(f.state(&id).0.is_none() && !backups(&f));
        // The backup cannot be made: 503 before any change, and the block is still refused.
        std::fs::write(f.dir.join("config/backups"), b"blocked").unwrap();
        let r = f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
        assert_eq!(kind(r, 503).await["kind"], "backup_failed");
        assert_eq!(kind(f.block(&token, 0).await, 409).await["kind"], "not_started");
        assert!(f.state(&id).0.is_none());
        // Once it can be made, the same plan starts and the same block runs.
        std::fs::remove_file(f.dir.join("config/backups")).unwrap();
        let r = f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
        assert_eq!(kind(r, 200).await, json!({"total":1,"remaining":1}));
        assert_eq!(reasons(&kind(f.block(&token, 0).await, 200).await), ["ok"]);
        assert!(f.state(&id).0.is_some());
    }

    #[tokio::test]
    async fn a_plan_above_the_daily_limit_is_refused_whole_and_its_blocks_never_run() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        *f.app.fileops_safety.lock().unwrap() = crate::fileops_safety::FileOpsSafety::new(10);
        let ids: Vec<String> = (0..100).map(|n| f.gone(&format!("gone{n}.3mf"))).collect();
        let token = f.plan(json!({"missingModels":ids})).await;
        let r = f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
        assert_eq!(kind(r, 422).await["kind"], "limit_exceeded");
        assert!(!backups(&f), "a refusal makes no backup");
        assert_eq!(kind(f.block(&token, 0).await, 409).await["kind"], "not_started");
        assert!(ids.iter().all(|id| f.state(id).0.is_none()));
        // Nothing was booked: a plan of exactly ten is allowed, the next single removal waits.
        let ten = f.started(json!({"missingModels":&ids[..10]})).await;
        let done = kind(f.block(&ten, 0).await, 200).await;
        assert_eq!(done["counts"]["removedModels"], 10);
        let one = f.plan(json!({"missingModels":[ids[10]]})).await;
        let r = f.invoke("disk_sync_apply_start", json!({"token":one,"confirmRemoval":true})).await;
        assert_eq!(r.status(), 429);
        assert!(r.headers().contains_key("retry-after"));
        assert_eq!(kind(f.block(&one, 0).await, 409).await["kind"], "not_started");
        assert!(f.state(&ids[10]).0.is_none());
        assert_eq!(ids.iter().filter(|id| f.state(id).0.is_some()).count(), 10);
    }

    #[tokio::test]
    async fn imports_alone_book_no_budget() {
        let f = Fixture::new(true).await;
        *f.app.fileops_safety.lock().unwrap() = crate::fileops_safety::FileOpsSafety::new(1);
        f.put("a.obj", OBJ);
        let token = f.started(json!({"newFiles":[{"root":0,"relativePath":"a.obj"}]})).await;
        assert_eq!(kind(f.block(&token, 0).await, 200).await["counts"]["imported"], 1);
        assert_eq!(f.app.fileops_safety.lock().unwrap().reserve_disk_sync(1, 100), Ok(()));
    }

    #[tokio::test]
    async fn a_file_that_is_back_is_skipped_and_stays_where_it_is() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let id = f.gone("back.3mf");
        let token = f.started(json!({"missingModels":[id]})).await;
        f.put("back.3mf", b"here again");
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(reasons(&answer), ["exists_again"]);
        assert_eq!(answer["counts"]["skipped"], 1);
        assert!(f.state(&id).0.is_none());
        assert_eq!(std::fs::read(f.dir.join("models/back.3mf")).unwrap(), b"here again");
        assert!(trash_names(&f).is_empty());
    }

    #[tokio::test]
    async fn plans_expire_and_belong_to_their_session() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let id = f.gone("gone.3mf");
        let token = f.plan(json!({"missingModels":[id]})).await;
        let other = f.session_two();
        let r = f.invoke_as(&other, "disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
        assert_eq!(kind(r, 404).await["kind"], "snapshot_not_found");
        let r = f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
        assert_eq!(r.status(), 200);
        let r = f.invoke_as(&other, "disk_sync_apply_block", json!({"token":token,"seq":0})).await;
        assert_eq!(kind(r, 404).await["kind"], "snapshot_not_found");
        assert!(f.state(&id).0.is_none());
        f.time.0.store(100 + 3600, Ordering::SeqCst);
        assert_eq!(kind(f.block(&token, 0).await, 404).await["kind"], "snapshot_not_found");
        let r = f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
        assert_eq!(kind(r, 404).await["kind"], "snapshot_not_found");
        assert!(f.state(&id).0.is_none());
        // A second preview replaces the first one of the session.
        let first = f.plan(json!({"missingModels":[id]})).await;
        let second = f.plan(json!({"missingModels":[id]})).await;
        assert_ne!(first, second);
        let r = f.invoke("disk_sync_apply_start", json!({"token":first,"confirmRemoval":true})).await;
        assert_eq!(r.status(), 404);
    }

    #[tokio::test]
    async fn the_plan_count_is_limited() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let id = f.gone("gone.3mf");
        {
            let mut state = f.app.fileops.as_ref().unwrap().state.lock().unwrap();
            fileops::disksync::insert_other_plans(&mut state, 100, 100);
        }
        let r = f.invoke("disk_sync_preview", json!({"missingModels":[id]})).await;
        assert_eq!(kind(r, 503).await["kind"], "snapshot_capacity");
        // Expired plans make room.
        f.time.0.store(100 + 3600, Ordering::SeqCst);
        assert_eq!(f.invoke("disk_sync_preview", json!({"missingModels":[id]})).await.status(), 200);
    }

    #[tokio::test]
    async fn a_repeated_start_or_block_does_nothing_twice() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        *f.app.fileops_safety.lock().unwrap() = crate::fileops_safety::FileOpsSafety::new(20);
        let ids: Vec<String> = (0..12).map(|n| f.gone(&format!("gone{n}.3mf"))).collect();
        let token = f.started(json!({"missingModels":&ids[..10]})).await;
        let again = f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
        assert_eq!(kind(again, 409).await["kind"], "snapshot_mismatch");
        // Booked once: another plan of ten (another session, a session has one plan) still fits into the day of twenty.
        let other = f.session_two();
        let r = f.invoke_as(&other, "disk_sync_preview", json!({"missingModels":&ids[10..]})).await;
        let more = value(r).await["token"].as_str().unwrap().to_owned();
        let r = f.invoke_as(&other, "disk_sync_apply_start", json!({"token":more,"confirmRemoval":true})).await;
        assert_eq!(r.status(), 200);
        let first = kind(f.block(&token, 0).await, 200).await;
        let (before, entries) = (count(&f, "files"), f.app.fileops.as_ref().unwrap().pending_entries_for_test());
        let again = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(first, again);
        assert_eq!((count(&f, "files"), f.app.fileops.as_ref().unwrap().pending_entries_for_test()), (before, entries));
        // Wrong sequence numbers and a finished plan.
        assert_eq!(kind(f.block(&token, 1).await, 409).await["kind"], "snapshot_mismatch");
        assert_eq!(kind(f.block(&token, 7).await, 409).await["kind"], "snapshot_mismatch");
        let r = f.invoke_as(&other, "disk_sync_apply_block", json!({"token":more,"seq":0})).await;
        assert_eq!(kind(r, 200).await["counts"]["removedModels"], 2);
        let r = f.invoke_as(&other, "disk_sync_apply_block", json!({"token":more,"seq":1})).await;
        assert_eq!(kind(r, 409).await["kind"], "snapshot_mismatch");
        assert_eq!(first["counts"]["removedModels"], 10);
    }

    #[tokio::test]
    async fn blocks_are_ten_rows_and_the_audit_names_the_ids_never_paths() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let ids: Vec<String> = (0..12).map(|n| f.gone(&format!("gone{n}.3mf"))).collect();
        let token = f.started(json!({"missingModels":ids})).await;
        let progress = {
            let (r, p) = f.invoke_audited("disk_sync_apply_block", json!({"token":token,"seq":0})).await;
            let answer = kind(r, 200).await;
            assert_eq!((answer["done"].clone(), answer["remaining"].clone()), (json!(10), json!(2)));
            p
        };
        assert_eq!(progress.file_ids.lock().unwrap().len(), 10);
        assert_eq!(progress.completed_items.load(Ordering::Relaxed), 10);
        let answer = kind(f.block(&token, 1).await, 200).await;
        assert_eq!((answer["done"].clone(), answer["remaining"].clone()), (json!(12), json!(0)));
        assert!(!answer.to_string().contains(f.dir.to_str().unwrap()));
    }

    #[tokio::test]
    async fn a_session_that_ends_stops_the_block_after_the_current_row() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let ids: Vec<String> = (0..3).map(|n| f.gone(&format!("gone{n}.3mf"))).collect();
        let token = f.started(json!({"missingModels":ids})).await;
        let (weak, session) = (Arc::downgrade(&f.app), f.session.token.clone());
        fileops::disksync::row_hooks().lock().unwrap().insert(
            f.session.token.clone(),
            Arc::new(move |index| {
                if index == 0 {
                    revoke_session(&mut weak.upgrade().unwrap().data.lock().unwrap(), &session);
                }
            }),
        );
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(answer["stopped"], "session");
        assert_eq!((answer["done"].clone(), answer["remaining"].clone()), (json!(1), json!(2)));
        assert_eq!(ids.iter().filter(|id| f.state(id).0.is_some()).count(), 1);
        // The rest stays untouched and the session is gone.
        assert_eq!(f.block(&token, 1).await.status(), 401);
        assert_eq!(ids.iter().filter(|id| f.state(id).0.is_some()).count(), 1);
        fileops::disksync::row_hooks().lock().unwrap().remove(&f.session.token);
    }

    #[tokio::test]
    async fn maintenance_stops_a_running_block_and_the_rest_continues_afterwards() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let ids: Vec<String> = (0..3).map(|n| f.gone(&format!("gone{n}.3mf"))).collect();
        let token = f.started(json!({"missingModels":ids})).await;
        let window: Arc<Mutex<Option<crate::barrier::MaintenanceWindow>>> = Arc::default();
        let barrier = f.app.geometry.barrier.clone();
        let (handle, held) = (tokio::runtime::Handle::current(), window.clone());
        fileops::disksync::row_hooks().lock().unwrap().insert(
            f.session.token.clone(),
            Arc::new(move |index| {
                if index != 0 {
                    return;
                }
                let (b, held) = (barrier.clone(), held.clone());
                handle.spawn(async move {
                    let w = b.enter_draining(Duration::from_secs(20)).await.unwrap();
                    *held.lock().unwrap() = Some(w);
                });
                for _ in 0..400 {
                    if barrier.snapshot().state == crate::barrier::State::Draining {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }),
        );
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(answer["stopped"], "maintenance");
        assert_eq!(answer["results"].as_array().unwrap().len(), 1);
        fileops::disksync::row_hooks().lock().unwrap().remove(&f.session.token);
        // The new block is refused while the window is open, and the barrier counts no writer.
        for _ in 0..400 {
            if window.lock().unwrap().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(f.app.geometry.barrier.snapshot().writers, 0);
        let r = f.block(&token, 1).await;
        assert_eq!(kind(r, 503).await["message"], "maintenance");
        window.lock().unwrap().take().unwrap().release();
        let rest = kind(f.block(&token, 1).await, 200).await;
        assert_eq!(rest["stopped"], Value::Null);
        assert_eq!((rest["done"].clone(), rest["remaining"].clone()), (json!(3), json!(0)));
        assert!(ids.iter().all(|id| f.state(id).0.is_some()));
    }

    #[tokio::test]
    async fn every_command_is_refused_in_maintenance_and_with_a_blocked_journal() {
        let f = Fixture::new(true).await;
        let commands = [
            ("disk_sync_preview", json!({"missingModels":["1"]})),
            ("disk_sync_apply_start", json!({"token":"x","confirmRemoval":true})),
            ("disk_sync_apply_block", json!({"token":"x","seq":0})),
            ("preview_date_rename", json!({"fileIds":["1"],"dateSource":"modified"})),
            ("apply_date_rename", json!({"items":[{"id":"1","newName":"a"}]})),
        ];
        let window = f.app.geometry.barrier.enter_draining(Duration::from_secs(1)).await.unwrap();
        for (command, args) in &commands {
            let r = f.invoke(command, args.clone()).await;
            assert_eq!(kind(r, 503).await["message"], "maintenance", "{command}");
        }
        let window = window.enter_exclusive(Duration::from_secs(1)).await.unwrap();
        for (command, args) in &commands {
            let r = f.invoke(command, args.clone()).await;
            assert_eq!(kind(r, 503).await["message"], "maintenance", "{command}");
        }
        window.release();
        f.app.fileops.as_ref().unwrap().block_for_test();
        for (command, args) in &commands {
            let r = f.invoke(command, args.clone()).await;
            assert_eq!(kind(r, 503).await["kind"], "restart_required", "{command}");
        }
    }

    #[tokio::test]
    async fn without_file_management_the_commands_do_not_exist_and_checks_come_first_otherwise() {
        let off = Fixture::new(false).await;
        for command in ["disk_sync_preview", "disk_sync_apply_start", "disk_sync_apply_block", "preview_date_rename", "apply_date_rename"] {
            let r = off.invoke(command, json!({})).await;
            assert_eq!(kind(r, 404).await["kind"], "fileops_disabled", "{command}");
        }
        let f = Fixture::new(true).await;
        for (command, args, status) in [
            ("disk_sync_preview", json!({"missingModels":["1"],"extra":1}), 422),
            ("disk_sync_preview", json!({}), 422),
            ("disk_sync_preview", json!({"missingModels":["0"]}), 400),
            ("disk_sync_preview", json!({"newFiles":[{"root":0}]}), 400),
            ("disk_sync_apply_start", json!({"token":"x","confirmRemoval":"yes"}), 400),
            ("disk_sync_apply_block", json!({"token":"x","seq":-1}), 400),
            ("disk_sync_apply_block", json!({"token":"x","seq":0,"x":1}), 422),
            ("preview_date_rename", json!({"fileIds":["1"],"dateSource":"today"}), 400),
            ("apply_date_rename", json!({"items":[]}), 422),
        ] {
            assert_eq!(f.invoke(command, args).await.status(), status, "{command}");
        }
        let many: Vec<String> = (1..=1501).map(|n| n.to_string()).collect();
        let r = f.invoke("disk_sync_preview", json!({"missingModels":many})).await;
        assert_eq!(kind(r, 422).await["kind"], "limit_exceeded");
    }

    #[tokio::test]
    async fn an_import_running_makes_every_command_wait_and_keeps_the_plan() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let id = f.gone("gone.3mf");
        let token = f.plan(json!({"missingModels":[id]})).await;
        let permit = f.app.uploads.as_ref().unwrap().active.clone().try_acquire_owned().unwrap();
        let r = f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await;
        assert_eq!(r.status(), 503);
        assert!(r.headers().contains_key("retry-after"));
        drop(permit);
        assert_eq!(f.invoke("disk_sync_apply_start", json!({"token":token,"confirmRemoval":true})).await.status(), 200);
    }

    #[tokio::test]
    async fn new_files_are_imported_with_their_folders_and_checked_again_per_row() {
        let f = Fixture::new(true).await;
        f.put("new/a.obj", OBJ);
        f.put("b.stl", STL);
        f.put("deep/er/c.3mf", &three_mf());
        let selection = json!({"newFiles":[
            {"root":0,"relativePath":"new/a.obj"},{"root":0,"relativePath":"b.stl"},{"root":0,"relativePath":"deep/er/c.3mf"}]});
        let token = f.plan(selection.clone()).await;
        // Not started: nothing happens, with no confirmation needed once it is.
        assert_eq!(kind(f.block(&token, 0).await, 409).await["kind"], "not_started");
        assert_eq!(count(&f, "files"), 0);
        let r = f.invoke("disk_sync_apply_start", json!({"token":token})).await;
        assert_eq!(kind(r, 200).await["total"], 3);
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(reasons(&answer), ["ok", "ok", "ok"], "{answer}");
        assert_eq!(answer["counts"]["imported"], 3);
        assert_eq!(count(&f, "files"), 3);
        let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        let folders: Vec<String> = conn
            .prepare("SELECT name FROM folders ORDER BY name").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert!(["deep", "er", "new"].iter().all(|n| folders.iter().any(|f| f == n)), "{folders:?}");
        drop(conn);
        // The files themselves are untouched.
        assert_eq!(std::fs::read(f.dir.join("models/b.stl")).unwrap(), STL);
        // Offered again, they are in the catalog.
        let again = value(f.invoke("disk_sync_preview", selection).await).await;
        assert_eq!(again["token"], "");
        assert_eq!(again["skipped"].as_array().unwrap().len(), 3);
        assert!(again["skipped"].as_array().unwrap().iter().all(|s| s["reason"] == "already_in_catalog"));
    }

    #[tokio::test]
    async fn a_file_that_changed_or_vanished_after_the_preview_is_not_imported() {
        let f = Fixture::new(true).await;
        f.put("changed.obj", OBJ);
        f.put("gone.obj", OBJ);
        f.put("swapped.obj", OBJ);
        let token = f.started(json!({"newFiles":[
            {"root":0,"relativePath":"changed.obj"},{"root":0,"relativePath":"gone.obj"},{"root":0,"relativePath":"swapped.obj"}]})).await;
        f.put("changed.obj", b"v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n# longer than before\n");
        std::fs::remove_file(f.dir.join("models/gone.obj")).unwrap();
        // Another file under the same name (a different inode, same size).
        std::fs::write(f.dir.join("models/other.tmp"), OBJ).unwrap();
        std::fs::rename(f.dir.join("models/other.tmp"), f.dir.join("models/swapped.obj")).unwrap();
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(reasons(&answer), ["changed", "changed", "changed"], "{answer}");
        assert_eq!(count(&f, "files"), 0);
        assert_eq!(count(&f, "folders"), 0);
    }

    #[tokio::test]
    async fn links_the_upload_folder_and_bad_paths_are_never_read() {
        let f = Fixture::new(true).await;
        let outside = f.dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("x.obj"), OBJ).unwrap();
        symlink(outside.join("x.obj"), f.dir.join("models/link.obj")).unwrap();
        symlink(&outside, f.dir.join("models/linkdir")).unwrap();
        f.put("uploads/up.obj", OBJ);
        f.put("fine.obj", OBJ);
        std::fs::write(f.dir.join("models/.hidden.obj"), OBJ).unwrap();
        let args = json!({"newFiles":[
            {"root":0,"relativePath":"link.obj"},
            {"root":0,"relativePath":"linkdir/x.obj"},
            {"root":0,"relativePath":"uploads/up.obj"},
            {"root":0,"relativePath":"../outside/x.obj"},
            {"root":0,"relativePath":"/etc/passwd.obj"},
            {"root":0,"relativePath":"a//b.obj"},
            {"root":0,"relativePath":".hidden.obj"},
            {"root":0,"relativePath":"fine.txt"},
            {"root":5,"relativePath":"fine.obj"},
            {"root":0,"relativePath":"fine.obj"}]});
        let answer = value(f.invoke("disk_sync_preview", args).await).await;
        let skipped: Vec<(String, String)> = answer["skipped"].as_array().unwrap().iter()
            .map(|s| (s["key"].as_str().unwrap().to_owned(), s["reason"].as_str().unwrap().to_owned())).collect();
        assert_eq!(skipped.len(), 9, "{skipped:?}");
        for (key, reason) in &skipped {
            let expected = if key.ends_with("fine.txt") { "invalid" } else { "outside" };
            assert_eq!(reason, expected, "{key}");
        }
        assert_eq!(answer["counts"]["import"], 1);
        // A link put in place after the preview is not followed either.
        let token = f.started(json!({"newFiles":[{"root":0,"relativePath":"fine.obj"}]})).await;
        std::fs::remove_file(f.dir.join("models/fine.obj")).unwrap();
        symlink(outside.join("x.obj"), f.dir.join("models/fine.obj")).unwrap();
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(reasons(&answer), ["outside"]);
        assert_eq!(count(&f, "files"), 0);
    }

    #[tokio::test]
    async fn names_that_differ_only_in_case_are_already_in_the_catalog() {
        let f = Fixture::new(true).await;
        f.file("foo.3mf");
        f.put("Foo.3MF", &three_mf());
        let answer = value(f.invoke("disk_sync_preview", json!({"newFiles":[{"root":0,"relativePath":"Foo.3MF"}]})).await).await;
        assert_eq!(answer["skipped"][0]["reason"], "already_in_catalog");
        assert_eq!(answer["token"], "");
    }

    #[tokio::test]
    async fn rows_outside_the_roots_and_unusable_roots_are_skipped_not_removed() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let foreign = {
            let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
            db::test_insert_minimal_file(&conn, "/somewhere/else/x.3mf", None).unwrap().to_string()
        };
        let answer = value(f.invoke("disk_sync_preview", json!({"missingModels":[foreign]})).await).await;
        assert_eq!(answer["skipped"][0]["reason"], "outside");
        // The root is replaced by an empty folder after the preview.
        let id = f.gone("gone.3mf");
        let comp_owner = f.gone("owner.3mf");
        let token = f.started(json!({"missingModels":[id]})).await;
        std::fs::rename(f.dir.join("models"), f.dir.join("models-old")).unwrap();
        std::fs::create_dir(f.dir.join("models")).unwrap();
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(reasons(&answer), ["root_unavailable"], "{answer}");
        assert!(f.state(&id).0.is_none() && f.state(&comp_owner).0.is_none());
    }

    #[tokio::test]
    async fn a_model_with_companion_files_is_not_removed() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let id = f.gone("gone.3mf");
        add_real_companion(&f, &id, "gone_extras", "pic.png");
        let token = f.started(json!({"missingModels":[id]})).await;
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(reasons(&answer), ["has_companions"], "{answer}");
        assert!(f.state(&id).0.is_none());
        assert!(f.dir.join("models/gone_extras/pic.png").is_file());
    }

    #[tokio::test]
    async fn companion_rows_without_a_file_are_removed_and_a_folder_in_the_way_is_not_a_file() {
        let f = Fixture::new(true).await;
        let model = f.file("m.3mf");
        let (loose, folder) = {
            let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
            let m: i64 = model.parse().unwrap();
            let loose = db::companions::insert_companion(&conn, m, "gone.png", f.dir.join("models/m_extras/gone.png").to_str().unwrap(), "image", Some(8), "2026-01-01T00:00:00Z").unwrap();
            let folder = db::companions::insert_companion(&conn, m, "dir.png", f.dir.join("models/m_extras/dir.png").to_str().unwrap(), "image", Some(8), "2026-01-01T00:00:00Z").unwrap();
            (loose, folder)
        };
        std::fs::create_dir_all(f.dir.join("models/m_extras/dir.png")).unwrap();
        let token = f.started(json!({"danglingCompanions":[loose.to_string(), folder.to_string()]})).await;
        let answer = kind(f.block(&token, 0).await, 200).await;
        assert_eq!(reasons(&answer), ["ok", "not_a_file"], "{answer}");
        assert_eq!(answer["counts"]["removedCompanions"], 1);
        assert!(f.row(&loose.to_string()).0.is_some());
        assert!(f.row(&folder.to_string()).0.is_none());
        assert!(f.dir.join("models/m_extras/dir.png").is_dir());
    }

    #[tokio::test]
    async fn the_audit_of_a_removal_names_model_and_companion_ids() {
        let f = Fixture::new(true).await;
        let model = f.file("m.3mf");
        let cid = {
            let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
            db::companions::insert_companion(&conn, model.parse().unwrap(), "gone.png", f.dir.join("models/m_extras/gone.png").to_str().unwrap(), "image", Some(8), "2026-01-01T00:00:00Z").unwrap()
        };
        let token = f.started(json!({"danglingCompanions":[cid.to_string()]})).await;
        let (r, p) = f.invoke_audited("disk_sync_apply_block", json!({"token":token,"seq":0})).await;
        assert_eq!(r.status(), 200);
        assert_eq!(*p.companion_ids.lock().unwrap(), vec![cid]);
        assert_eq!(*p.file_ids.lock().unwrap(), vec![model.parse::<i64>().unwrap()]);
    }

    #[tokio::test]
    async fn the_date_prefix_previews_and_renames_with_companions_and_twice_changes_nothing() {
        let f = Fixture::new(true).await;
        let id = f.file("c.3mf");
        let (cid, old) = add_real_companion(&f, &id, "c_extras", "pic.png");
        let outside = {
            let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
            db::test_insert_minimal_file(&conn, "/somewhere/else/x.3mf", None).unwrap().to_string()
        };
        let missing = f.gone("none.3mf");
        let preview = value(f.invoke("preview_date_rename", json!({"fileIds":[id, outside, missing],"dateSource":"modified"})).await).await;
        let status: Vec<&str> = preview.as_array().unwrap().iter().map(|p| p["status"].as_str().unwrap()).collect();
        // Outside the roots nothing can be resolved: reported like a missing file, and never touched.
        assert_eq!(status, ["rename", "skip_missing", "skip_missing"], "{preview}");
        let new_name = preview[0]["newName"].as_str().unwrap().to_owned();
        assert!(new_name.ends_with("_c.3mf") || new_name.contains("c.3mf"), "{new_name}");
        let items = json!({"items":[{"id":id,"newName":new_name}]});
        let r = f.invoke("apply_date_rename", items.clone()).await;
        let done = kind(r, 200).await;
        assert_eq!(done, json!([{"id":id,"status":"renamed"}]));
        let renamed = f.dir.join("models").join(&new_name);
        assert!(renamed.is_file() && !f.dir.join("models/c.3mf").exists());
        assert!(!old.exists());
        let stem = new_name.trim_end_matches(".3mf");
        assert!(f.dir.join("models").join(format!("{stem}_extras/pic.png")).is_file());
        assert!(companion_path(&f, cid).contains(stem));
        // The same names again: the file already has the prefix, nothing changes.
        let again = kind(f.invoke("apply_date_rename", items).await, 200).await;
        assert_eq!(again[0]["status"], "failed");
        assert!(renamed.is_file());
        assert!(f.dir.join("models/c.3mf").symlink_metadata().is_err());
        // A name the preview never made, and one outside the roots.
        let bad = kind(f.invoke("apply_date_rename", json!({"items":[{"id":outside,"newName":"2026-01-01_x.3mf"},{"id":missing,"newName":"2026-01-01_none.3mf"}]})).await, 200).await;
        assert_eq!(bad[0]["reason"], "missing");
        assert_eq!(bad[1]["reason"], "missing");
        assert_eq!(f.app.fileops.as_ref().unwrap().pending_entries_for_test(), 0);
        // 201 entries are too many.
        let many: Vec<String> = (1..=201).map(|n| n.to_string()).collect();
        let r = f.invoke("preview_date_rename", json!({"fileIds":many,"dateSource":"imported"})).await;
        assert_eq!(kind(r, 422).await["kind"], "limit_exceeded");
    }

    #[tokio::test]
    async fn the_date_prefix_never_follows_a_link_out_of_the_roots() {
        let f = Fixture::new(true).await;
        let outside = f.dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("x.3mf"), b"x").unwrap();
        symlink(&outside, f.dir.join("models/linkdir")).unwrap();
        let id = {
            let conn = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
            db::test_insert_minimal_file(&conn, f.dir.join("models/linkdir/x.3mf").to_str().unwrap(), None).unwrap().to_string()
        };
        let preview = value(f.invoke("preview_date_rename", json!({"fileIds":[id],"dateSource":"modified"})).await).await;
        assert_eq!(preview[0]["status"], "skip_missing");
        let r = kind(f.invoke("apply_date_rename", json!({"items":[{"id":id,"newName":"2026-01-01_x.3mf"}]})).await, 200).await;
        assert_eq!(r[0]["status"], "failed");
        assert!(outside.join("x.3mf").is_file());
    }

    #[tokio::test]
    async fn the_date_prefix_body_limit_is_384_kib_over_the_real_http_path() {
        let f = Fixture::new(true).await;
        // 200 names of 255 bytes, every character escaped as a: about 316 000 bytes.
        let name: String = "\\u0061".repeat(255);
        let items: Vec<String> = (1..=200).map(|n| format!(r#"{{"id":"{n}","newName":"{name}"}}"#)).collect();
        let body = format!(r#"{{"items":[{}]}}"#, items.join(","));
        assert!(body.len() > 300_000 && body.len() < 384 * 1024);
        let r = f.invoke_raw("apply_date_rename", body).await;
        let answer = kind(r, 200).await;
        assert_eq!(answer.as_array().unwrap().len(), 200);
        // Above the limit: 413, nothing parsed.
        let too_big = format!(r#"{{"items":[{{"id":"1","newName":"{}"}}]}}"#, "a".repeat(400 * 1024));
        assert_eq!(f.invoke_raw("apply_date_rename", too_big).await.status(), 413);
        // The preview stays small: 200 ids fit the 16 KiB of the other writes.
        let ids: Vec<String> = (1..=200).map(|n| n.to_string()).collect();
        let r = f.invoke("preview_date_rename", json!({"fileIds":ids,"dateSource":"modified"})).await;
        assert_eq!(r.status(), 200);
    }

    #[tokio::test]
    async fn a_large_selection_fits_the_preview_body() {
        let f = Fixture::new(true).await;
        f.file("keep.3mf");
        let rows: Vec<Value> = (0..500).map(|n| json!({"root":0,"relativePath":format!("dir{n}/{}.3mf", "x".repeat(150))})).collect();
        let ids: Vec<String> = (1..=1000).map(|n| (n + 100).to_string()).collect();
        let r = f.invoke("disk_sync_preview", json!({"newFiles":rows,"missingModels":ids})).await;
        let answer = kind(r, 200).await;
        assert_eq!(answer["token"], "");
        assert_eq!(answer["skipped"].as_array().unwrap().len(), 1500);
    }
}
