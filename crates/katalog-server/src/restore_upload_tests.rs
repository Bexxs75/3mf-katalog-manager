use super::*;
use katalog_core::db;
use std::sync::OnceLock;

struct Fixture {
    app: Arc<App>,
    session: Session,
    dir: PathBuf,
}
impl Fixture {
    fn new(write: bool) -> Self {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.target/restore-http")
            .join(format!("{:032x}", u128::from_ne_bytes(random)));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        std::fs::create_dir_all(dir.join("sources")).unwrap();
        let dir = dir.canonicalize().unwrap();
        std::fs::create_dir_all(dir.join("config")).unwrap();
        let path = dir.join("config/catalog.db");
        drop(db::connect(&path).unwrap());
        let config = Config::from_lookup(|key| match key {
            "KATALOG_ALLOWED_HOSTS" => Some("localhost:8080".into()),
            "KATALOG_DB" => Some(path.to_string_lossy().into_owned()),
            "KATALOG_ROOTS" => Some(dir.join("models").to_string_lossy().into_owned()),
            "KATALOG_SOURCE_ROOTS" => Some(dir.join("sources").to_string_lossy().into_owned()),
            "KATALOG_WRITE" if write => Some("1".into()),
            _ => None,
        })
        .unwrap();
        let now = SystemClock.now();
        let mut data = Data {
            auth: Some(AuthFile {
                version: 1,
                password_hash: hash(),
                must_change: false,
                updated_at: now,
            }),
            sessions: HashMap::new(),
            limits: Limits::default(),
            revision: 0,
        };
        let session = new_session(&mut data, "test", now).unwrap();
        let app = Arc::new(App {
            restore: Runtime::default(),
            events: Arc::default(),
            printers: crate::web::printer_service::Runtime::default(),
            fileops: None,
            fileops_safety: Mutex::new(crate::fileops_safety::FileOpsSafety::new(2000)),
            uploads: None,
            writer: None,
            config,
            clock: Arc::new(SystemClock),
            data: Mutex::new(data),
            changes: Arc::new(tokio::sync::Mutex::new(())),
            geometry: Arc::new(GeometryGate::default()),
            downloads: Arc::new(tokio::sync::Semaphore::new(4)),
            export: crate::export::Runtime::new(),
            slicer: None,
            _instance_lock: None,
        });
        Self { app, session, dir }
    }
    fn archive(&self, sql: &str) -> Vec<u8> {
        let path = self.dir.join("source.db");
        let _ = std::fs::remove_file(&path);
        let conn = db::connect(&path).unwrap();
        db::test_insert_minimal_file(&conn, self.dir.join("models/x.3mf").to_str().unwrap(), None)
            .unwrap();
        conn.execute_batch(sql).unwrap();
        drop(conn);
        zip(&[
            ("catalog.db", &std::fs::read(path).unwrap()),
            ("settings.json", b"{}"),
        ])
    }
    fn request(&self, method: &str, path: &str, bytes: Vec<u8>, password: &str) -> Request {
        Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost:8080")
            .header("origin", "http://localhost:8080")
            .header("cookie", format!("katalog_session={}", self.session.token))
            .header("x-csrf-token", &self.session.csrf)
            .header(
                "x-restore-password",
                base64::engine::general_purpose::STANDARD.encode(password),
            )
            .body(Body::from(bytes))
            .unwrap()
    }
    async fn upload(&self, bytes: Vec<u8>) -> Response {
        router(self.app.clone())
            .oneshot(self.request("POST", UPLOAD, bytes, "restore-test-password"))
            .await
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.app.restore.entries.lock().unwrap().clear();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn hash() -> String {
    use argon2::{
        password_hash::{PasswordHasher, SaltString},
        Argon2,
    };
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        Argon2::default()
            .hash_password(
                b"restore-test-password",
                &SaltString::encode_b64(&[9_u8; 16]).unwrap(),
            )
            .unwrap()
            .to_string()
    })
    .clone()
}
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
async fn value(response: Response) -> Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn restore_http_password_write_session_csrf_and_origin_required() {
    let _guard = crate::password::PASSWORD_TEST_LOCK.lock().await;
    let f = Fixture::new(true);
    let response = router(f.app.clone())
        .oneshot(f.request("POST", UPLOAD, vec![], "wrong"))
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(value(response).await["message"], "password_invalid");
    for header in ["cookie", "x-csrf-token", "origin"] {
        let mut request = f.request("POST", UPLOAD, vec![], "restore-test-password");
        request.headers_mut().remove(header);
        let response = router(f.app.clone()).oneshot(request).await.unwrap();
        assert!([401, 403].contains(&response.status().as_u16()), "{header}");
    }
    let f = Fixture::new(false);
    assert_eq!(f.upload(vec![]).await.status(), 404);
    assert!(!f.dir.join("config").join(ROOT).exists());
}

#[tokio::test]
async fn restore_http_preview_permissions_tokens_delete_and_lease() {
    let _guard = crate::password::PASSWORD_TEST_LOCK.lock().await;
    let f = Fixture::new(true);
    let active = std::fs::read(&f.app.config.db_path).unwrap();
    let response = f.upload(f.archive("")).await;
    assert_eq!(response.status(), 200, "{}", value(response).await);
    let preview = value(f.upload(f.archive("")).await).await;
    assert_eq!(preview["settings_ignored"], true);
    assert_eq!(preview["report"]["schema_version"], 66);
    assert_eq!(preview["report"]["model_count"], 1);
    let token = preview["token"].as_str().unwrap();
    assert!(valid_token(token));
    let lease = f
        .app
        .restore
        .checked_catalog(token, &f.session.token)
        .unwrap();
    assert!(f
        .app
        .restore
        .checked_catalog(token, "another-session")
        .is_none());
    assert!(f
        .app
        .restore
        .checked_catalog("../catalog.db", &f.session.token)
        .is_none());
    let path = lease.path().unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        std::fs::metadata(path.with_file_name("backup.zip"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    {
        let entries = f.app.restore.entries.lock().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries
                .keys()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            2
        );
    }

    let mut foreign = f.request("DELETE", &format!("{DELETE_PREFIX}{token}"), vec![], "");
    let other = new_session(&mut f.app.data.lock().unwrap(), "test", SystemClock.now()).unwrap();
    foreign.headers_mut().insert(
        "cookie",
        format!("katalog_session={}", other.token).parse().unwrap(),
    );
    foreign
        .headers_mut()
        .insert("x-csrf-token", other.csrf.parse().unwrap());
    assert_eq!(
        router(f.app.clone())
            .oneshot(foreign)
            .await
            .unwrap()
            .status(),
        404
    );
    let delete = f.request("DELETE", &format!("{DELETE_PREFIX}{token}"), vec![], "");
    assert_eq!(
        router(f.app.clone())
            .oneshot(delete)
            .await
            .unwrap()
            .status(),
        204
    );
    assert!(path.exists(), "held lease keeps catalog alive");
    drop(lease);
    assert!(!path.exists());
    assert_eq!(std::fs::read(&f.app.config.db_path).unwrap(), active);
}

#[tokio::test]
async fn restore_http_rejects_prepared_databases_and_preserves_active_catalog() {
    let _guard = crate::password::PASSWORD_TEST_LOCK.lock().await;
    for sql in [
        "CREATE TRIGGER injected AFTER INSERT ON files BEGIN DELETE FROM files; END; PRAGMA user_version = 40",
        "CREATE VIEW injected AS SELECT * FROM files",
        "CREATE VIRTUAL TABLE injected USING fts5(content)",
        "UPDATE files SET file_size_bytes = 'wrong'",
        "PRAGMA foreign_keys = OFF; UPDATE files SET folder_id = 999",
        "PRAGMA user_version = 67",
    ] {
        let f = Fixture::new(true);
        let active = std::fs::read(&f.app.config.db_path).unwrap();
        let response = f.upload(f.archive(sql)).await;
        assert_eq!(response.status(), 400, "{sql}: {}", value(response).await);
        assert!(f.app.restore.entries.lock().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(f.dir.join("config").join(ROOT)).unwrap().count(), 0);
        assert_eq!(std::fs::read(&f.app.config.db_path).unwrap(), active);
    }
}

#[tokio::test]
async fn restore_http_reports_foreign_paths_unreachable_sources_and_old_schema() {
    let _guard = crate::password::PASSWORD_TEST_LOCK.lock().await;
    let f = Fixture::new(true);
    let sql = format!("UPDATE files SET path = '{}', name = 'foreign.3mf'; INSERT INTO source_folders(path, added_at) VALUES ('{}', 'now'); PRAGMA user_version = 40", f.dir.join("outside/foreign.3mf").display(), f.dir.join("outside").display());
    let response = f.upload(f.archive(&sql)).await;
    assert_eq!(response.status(), 200, "{}", value(response).await);
    let report = value(f.upload(f.archive(&sql)).await).await;
    assert_eq!(report["report"]["schema_version"], 40);
    assert_eq!(report["report"]["migration_needed"], true);
    assert_eq!(
        report["report"]["path_warnings"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        report["report"]["unreachable_source_folders"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn restore_http_rejects_zip_paths_duplicates_oversize_and_bomb() {
    let _guard = crate::password::PASSWORD_TEST_LOCK.lock().await;
    let f = Fixture::new(true);
    let traversal = zip(&[("../catalog.db", b"x"), ("settings.json", b"{}")]);
    let mut duplicate = zip(&[
        ("catalog.db", b"x"),
        ("another.db", b"y"),
        ("settings.json", b"{}"),
    ]);
    for i in 0..duplicate.len() - 10 {
        if &duplicate[i..i + 10] == b"another.db" {
            duplicate[i..i + 10].copy_from_slice(b"catalog.db");
        }
    }
    let mut huge = zip(&[("catalog.db", b"x"), ("settings.json", b"{}")]);
    let central = huge.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    huge[central + 24..central + 28].copy_from_slice(&(257_u32 * 1024 * 1024).to_le_bytes());
    huge[22..26].copy_from_slice(&(257_u32 * 1024 * 1024).to_le_bytes());
    // Highly compressible settings exceed their independent decompression budget.
    let bomb = zip(&[
        ("catalog.db", b"x"),
        ("settings.json", &vec![b' '; 1024 * 1024 + 1]),
    ]);
    for bytes in [traversal, duplicate, huge, bomb] {
        assert_eq!(f.upload(bytes).await.status(), 400);
        assert_eq!(
            std::fs::read_dir(f.dir.join("config").join(ROOT))
                .unwrap()
                .count(),
            0
        );
    }
}

#[tokio::test]
async fn restore_expiry_startup_cleanup_and_staging_symlink_rejected() {
    let _guard = crate::password::PASSWORD_TEST_LOCK.lock().await;
    let f = Fixture::new(true);
    let preview = value(f.upload(f.archive("")).await).await;
    let token = preview["token"].as_str().unwrap();
    let path = f
        .app
        .restore
        .checked_catalog(token, &f.session.token)
        .unwrap()
        .path()
        .unwrap();
    f.app
        .restore
        .entries
        .lock()
        .unwrap()
        .get_mut(token)
        .unwrap()
        .created -= TTL;
    assert!(f
        .app
        .restore
        .checked_catalog(token, &f.session.token)
        .is_none());
    assert!(!path.exists());
    let (root, _) = staging_root(&f.app.config.db_path).unwrap();
    let stale = "a".repeat(64);
    root.mkdir(&stale, 0o700).unwrap();
    root.open_dir(&stale).unwrap().create("backup.zip").unwrap();
    startup(&f.app.config.db_path).unwrap();
    assert!(root.open_dir(&stale).is_err());
    std::fs::remove_dir(f.dir.join("config").join(ROOT)).unwrap();
    std::os::unix::fs::symlink(f.dir.join("models"), f.dir.join("config").join(ROOT)).unwrap();
    assert!(staging_root(&f.app.config.db_path).is_err());
}

#[tokio::test]
async fn restore_http_accepts_both_trash_locations_and_refuses_maintenance() {
    let _guard = crate::password::PASSWORD_TEST_LOCK.lock().await;
    let f = Fixture::new(true);
    for trash in [
        f.dir.join("config/trash/1-x.3mf"),
        f.dir.join("models/.katalog-papierkorb/1-deadbeef-x.3mf"),
    ] {
        let sql = format!(
            "UPDATE files SET deleted_at = 'now', trash_path = '{}'",
            trash.display()
        );
        let response = f.upload(f.archive(&sql)).await;
        assert_eq!(response.status(), 200, "{}", value(response).await);
    }
    let window = f
        .app
        .geometry
        .barrier
        .enter_draining(Duration::from_secs(1))
        .await
        .unwrap()
        .enter_exclusive(Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(f.upload(f.archive("")).await.status(), 503);
    window.release();
}
