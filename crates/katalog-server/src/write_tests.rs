use super::*;
use katalog_core::db;
use sha2::{Digest, Sha256};
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
    id: String,
}
impl Fixture {
    fn new(write: bool) -> Self {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!("web-write-{:x}", u128::from_ne_bytes(random)));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("catalog.db");
        let c = db::connect(&path).unwrap();
        let id = db::test_insert_minimal_file(&c, "/example.3mf", None)
            .unwrap()
            .to_string();
        drop(c);
        let config = Config::from_lookup(|key| match key {
            "KATALOG_DB" => Some(path.to_string_lossy().into()),
            "KATALOG_ROOTS" => Some(dir.to_string_lossy().into()),
            "KATALOG_WRITE" if write => Some("1".into()),
            "KATALOG_ALLOWED_HOSTS" => Some("localhost:8080".into()),
            _ => None,
        })
        .unwrap();
        let time = Arc::new(Time(AtomicU64::new(100)));
        let mut data = Data {
            auth: Some(AuthFile {
                version: 1,
                password_hash: "unused".into(),
                must_change: false,
                updated_at: 100,
            }),
            sessions: HashMap::new(),
            limits: Limits::default(),
            revision: 0,
        };
        let session = new_session(&mut data, "test", 100).unwrap();
        let app = Arc::new(App {
            fileops: None,
            fileops_safety: Mutex::new(crate::fileops_safety::FileOpsSafety::new(2000)),
            uploads:None,
            writer: write.then(|| WriteGate::new(db::open_read_write(&path).unwrap())),
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
            id,
        }
    }
    fn request(
        &self,
        command: &str,
        body: Body,
        session: Option<&Session>,
        origin: Option<&str>,
        csrf: bool,
    ) -> Request {
        let mut r = Request::builder()
            .method("POST")
            .uri(format!("/api/invoke/{command}"))
            .header("host", "localhost:8080")
            .header("content-type", "application/json");
        if let Some(s) = session {
            r = r.header("cookie", format!("katalog_session={}", s.token));
            if csrf {
                r = r.header("x-csrf-token", &s.csrf);
            }
        }
        if let Some(origin) = origin {
            r = r.header("origin", origin);
        }
        r.body(body).unwrap()
    }
    async fn send(&self, cmd: &str, args: Value) -> Response {
        router(self.app.clone())
            .oneshot(self.request(
                cmd,
                Body::from(args.to_string()),
                Some(&self.session),
                Some("http://localhost:8080"),
                true,
            ))
            .await
            .unwrap()
    }
    fn favorite(&self) -> bool {
        db::get_file(
            &db::open_read_only(&self.app.config.db_path).unwrap(),
            self.id.parse().unwrap(),
        )
        .unwrap()
        .unwrap()
        .favorite
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
async fn value(r: Response) -> Value {
    serde_json::from_slice(&to_bytes(r.into_body(), 10 * 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn all_disabled_commands_leave_catalog_hash_and_sidecar_names_unchanged() {
    let f = Fixture::new(false);
    let hash = Sha256::digest(std::fs::read(&f.app.config.db_path).unwrap());
    let names = || {
        let mut n: Vec<_> = std::fs::read_dir(&f.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        n.sort();
        n
    };
    let before = names();
    assert_eq!(WRITE_COMMANDS.len(), 55);
    assert_eq!(crate::READ_COMMANDS.len(), 27);
    for cmd in WRITE_COMMANDS {
        assert_eq!(f.send(cmd, json!({})).await.status(), 404, "{cmd}");
    }
    assert_eq!(
        Sha256::digest(std::fs::read(&f.app.config.db_path).unwrap()),
        hash
    );
    assert_eq!(names(), before);
}
#[tokio::test]
async fn write_security_errors_and_actual_effects() {
    let f = Fixture::new(true);
    let args = json!({"fileId":f.id,"favorite":true});
    for (session, origin, csrf, status) in [
        (None, Some("http://localhost:8080"), true, 401),
        (Some(&f.session), Some("http://localhost:8080"), false, 403),
        (Some(&f.session), None, true, 403),
        (Some(&f.session), Some("http://other.test"), true, 403),
    ] {
        let r = router(f.app.clone())
            .oneshot(f.request(
                "set_favorite",
                Body::from(args.to_string()),
                session,
                origin,
                csrf,
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), status);
        assert_eq!(r.headers()["cache-control"], "no-store");
    }
    let mut request = f.request(
        "set_favorite",
        Body::from(args.to_string()),
        Some(&f.session),
        Some("http://localhost:8080"),
        true,
    );
    request
        .headers_mut()
        .insert("host", "evil.test".parse().unwrap());
    assert_eq!(
        router(f.app.clone())
            .oneshot(request)
            .await
            .unwrap()
            .status(),
        403
    );
    f.app
        .data
        .lock()
        .unwrap()
        .auth
        .as_mut()
        .unwrap()
        .must_change = true;
    assert_eq!(f.send("set_favorite", args.clone()).await.status(), 403);
    f.app
        .data
        .lock()
        .unwrap()
        .auth
        .as_mut()
        .unwrap()
        .must_change = false;
    assert!(!f.favorite());
    assert_eq!(f.send("set_favorite", args).await.status(), 200);
    assert!(f.favorite());
    let r = f
        .send("set_favorite", json!({"fileId":"999","favorite":true}))
        .await;
    assert_eq!(r.status(), 404);
    assert_eq!(value(r).await["expected"], true);
    assert_eq!(
        f.send("set_favorite", json!({"fileId":"bad","favorite":true}))
            .await
            .status(),
        400
    );
    assert_eq!(
        f.send(
            "set_print_status",
            json!({"fileId":f.id,"status":"invented"})
        )
        .await
        .status(),
        422
    );
    assert_eq!(
        f.send("delete_file", json!({"fileId":f.id})).await.status(),
        404
    );
    let r = router(f.app.clone())
        .oneshot(f.request(
            "set_favorite",
            Body::from("{"),
            Some(&f.session),
            Some("http://localhost:8080"),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
}
#[tokio::test]
async fn every_enabled_command_reaches_core() {
    let f = Fixture::new(true);
    for (cmd, args) in [
        ("set_favorite", json!({"fileId":f.id,"favorite":true})),
        (
            "set_print_status",
            json!({"fileId":f.id,"status":"not_printed"}),
        ),
        ("add_tag", json!({"fileId":f.id,"tag":"test"})),
        ("remove_tag", json!({"fileId":f.id,"tag":"test"})),
        ("set_license", json!({"fileIds":[f.id],"license":"cc0"})),
        (
            "set_source_url",
            json!({"fileId":f.id,"url":"https://example.test"}),
        ),
        ("mark_file_viewed", json!({"fileId":f.id})),
        ("add_to_queue", json!({"fileId":f.id})),
        (
            "reorder_queue",
            json!({"updates":[{"fileId":f.id,"position":4}]}),
        ),
        ("remove_from_queue", json!({"fileId":f.id})),
    ] {
        assert_eq!(f.send(cmd, args).await.status(), 200, "{cmd}");
    }
    let r = f
        .send(
            "add_print_log_entry",
            json!({"fileId":f.id,"printedAt":"2026-10-16T00:00:00.000Z","note":"ok","photoBase64":"iVBORw0KGgo="}),
        )
        .await;
    assert_eq!(r.status(), 200);
    let entry = value(r).await;
    assert_eq!(
        f.send("delete_print_log_entry", json!({"entryId":entry["id"]}))
            .await
            .status(),
        200
    );
    let r = f.send("create_collection", json!({"name":"one"})).await;
    assert_eq!(r.status(), 200);
    let cid = value(r).await["id"].clone();
    for (cmd, args) in [
        (
            "rename_collection",
            json!({"collectionId":cid,"name":"two"}),
        ),
        (
            "add_files_to_collection",
            json!({"collectionId":cid,"fileIds":[f.id]}),
        ),
        (
            "reorder_collection",
            json!({"collectionId":cid,"updates":[{"fileId":f.id,"position":5}]}),
        ),
        (
            "remove_file_from_collection",
            json!({"collectionId":cid,"fileId":f.id}),
        ),
        ("delete_collection", json!({"collectionId":cid})),
    ] {
        assert_eq!(f.send(cmd, args).await.status(), 200, "{cmd}");
    }
    let c = db::open_read_only(&f.app.config.db_path).unwrap();
    let file = db::get_file(&c, f.id.parse().unwrap()).unwrap().unwrap();
    assert!(file.favorite);
    assert_eq!(file.license.as_deref(), Some("cc0"));
    assert!(file.tags.is_empty());
}
#[tokio::test]
async fn body_limits_apply_with_and_without_content_length_and_reads_keep_64k() {
    let f = Fixture::new(true);
    for (cmd, limit) in [
        ("set_favorite", 16 * 1024),
        ("get_app_version", 64 * 1024),
        ("add_print_log_entry", body_limit("add_print_log_entry")),
    ] {
        for explicit in [false, true] {
            let body = format!("{{\"padding\":\"{}\"}}", "x".repeat(limit));
            let size = body.len();
            let mut r = f.request(
                cmd,
                Body::from(body),
                Some(&f.session),
                Some("http://localhost:8080"),
                true,
            );
            if explicit {
                r.headers_mut()
                    .insert("content-length", size.to_string().parse().unwrap());
            }
            assert_eq!(
                router(f.app.clone()).oneshot(r).await.unwrap().status(),
                413,
                "{cmd} CL={explicit}"
            );
        }
    }
    assert_eq!(
        f.send("get_app_version", json!({"padding":"x".repeat(40*1024)}))
            .await
            .status(),
        200
    );
    use base64::Engine;
    let photo = base64::engine::general_purpose::STANDARD.encode({
        let mut bytes = vec![1; katalog_core::write::MAX_CUSTOM_IMAGE_BYTES];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes
    });
    for explicit in [false, true] {
        let body =
            json!({"fileId":f.id,"printedAt":"2026-10-16T00:00:00.000Z","photoBase64":photo})
                .to_string();
        let size = body.len();
        let mut r = f.request(
            "add_print_log_entry",
            Body::from(body),
            Some(&f.session),
            Some("http://localhost:8080"),
            true,
        );
        if explicit {
            r.headers_mut()
                .insert("content-length", size.to_string().parse().unwrap());
        }
        assert_eq!(
            router(f.app.clone()).oneshot(r).await.unwrap().status(),
            200
        );
    }
}
#[tokio::test]
async fn session_bucket_refills_and_is_removed_with_session() {
    let f = Fixture::new(true);
    for _ in 0..120 {
        assert_eq!(
            f.send("set_favorite", json!({"fileId":f.id,"favorite":true}))
                .await
                .status(),
            200
        );
    }
    let r = f
        .send("set_favorite", json!({"fileId":f.id,"favorite":true}))
        .await;
    assert_eq!(r.status(), 429);
    assert_eq!(r.headers()["retry-after"], "1");
    f.time.0.store(101, Ordering::SeqCst);
    assert_eq!(
        f.send("set_favorite", json!({"fileId":f.id,"favorite":false}))
            .await
            .status(),
        200
    );
    f.app.data.lock().unwrap().sessions.remove(&f.session.token);
    assert_eq!(
        f.send("set_favorite", json!({"fileId":f.id,"favorite":true}))
            .await
            .status(),
        401
    );
    let fresh = new_session(&mut f.app.data.lock().unwrap(), "test", 101).unwrap();
    let mut bucket = fresh.bucket;
    for _ in 0..120 {
        assert!(bucket.take(101));
    }
    assert!(!bucket.take(101));
}
#[tokio::test]
async fn real_sqlite_busy_is_503_and_does_not_change_data() {
    let f = Fixture::new(true);
    let c = db::open_read_write(&f.app.config.db_path).unwrap();
    c.execute_batch("BEGIN IMMEDIATE").unwrap();
    let r = f
        .send("set_favorite", json!({"fileId":f.id,"favorite":true}))
        .await;
    assert_eq!(r.status(), 503);
    assert_eq!(r.headers()["retry-after"], "1");
    c.execute_batch("ROLLBACK").unwrap();
    assert!(!f.favorite());
}
#[tokio::test]
async fn queued_write_rechecks_session_after_mutex_and_admission_is_bounded() {
    let f = Fixture::new(true);
    let gate = f.app.writer.as_ref().unwrap();
    let permits = gate.admission.clone().acquire_many_owned(33).await.unwrap();
    let r = f
        .send("set_favorite", json!({"fileId":f.id,"favorite":true}))
        .await;
    assert_eq!(r.status(), 503);
    assert_eq!(r.headers()["retry-after"], "1");
    drop(permits);
    let app = f.app.clone();
    let (locked, rx) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let blocker = tokio::task::spawn_blocking(move || {
        let _lock = app.writer.as_ref().unwrap().conn.lock().unwrap();
        locked.send(()).unwrap();
        wait.recv().unwrap();
    });
    rx.await.unwrap();
    let request = f.request(
        "set_favorite",
        Body::from(json!({"fileId":f.id,"favorite":true}).to_string()),
        Some(&f.session),
        Some("http://localhost:8080"),
        true,
    );
    let app_router = router(f.app.clone());
    let pending = tokio::spawn(async move { app_router.oneshot(request).await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while gate.worker.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let request = Request::builder()
        .method("POST")
        .uri("/api/logout")
        .header("host", "localhost:8080")
        .header("cookie", format!("katalog_session={}", f.session.token))
        .header("x-csrf-token", &f.session.csrf)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router(f.app.clone())
            .oneshot(request)
            .await
            .unwrap()
            .status(),
        200
    );
    release.send(()).unwrap();
    blocker.await.unwrap();
    assert_eq!(pending.await.unwrap().status(), 401);
    assert!(!f.favorite());
}
#[tokio::test]
async fn parallel_sessions_keep_both_fields() {
    let f = Fixture::new(true);
    let second = new_session(&mut f.app.data.lock().unwrap(), "second", 100).unwrap();
    let a = f.request(
        "set_favorite",
        Body::from(json!({"fileId":f.id,"favorite":true}).to_string()),
        Some(&f.session),
        Some("http://localhost:8080"),
        true,
    );
    let b = f.request(
        "set_source_url",
        Body::from(json!({"fileId":f.id,"url":"https://example.test"}).to_string()),
        Some(&second),
        Some("http://localhost:8080"),
        true,
    );
    let (a, b) = tokio::join!(
        router(f.app.clone()).oneshot(a),
        router(f.app.clone()).oneshot(b)
    );
    assert_eq!(a.unwrap().status(), 200);
    assert_eq!(b.unwrap().status(), 200);
    let file = db::get_file(
        &db::open_read_only(&f.app.config.db_path).unwrap(),
        f.id.parse().unwrap(),
    )
    .unwrap()
    .unwrap();
    assert!(file.favorite);
    assert_eq!(file.source_url.as_deref(), Some("https://example.test"));
}
#[tokio::test]
async fn write_startup_requires_auth_and_explicit_hosts() {
    for (no_auth, hosts) in [
        (true, Some("localhost:8080")),
        (false, None),
        (false, Some("")),
    ] {
        assert!(Config::from_lookup(|key| match key {
            "KATALOG_DB" => Some("/unused/catalog.db".into()),
            "KATALOG_ROOTS" => Some("/unused".into()),
            "KATALOG_WRITE" => Some("1".into()),
            "KATALOG_ALLOW_NO_AUTH" if no_auth => Some("1".into()),
            "KATALOG_ALLOWED_HOSTS" => hosts.map(str::to_owned),
            _ => None,
        })
        .is_err());
    }
    let f = Fixture::new(true);
    let mut config = f.app.config.clone();
    config.allow_no_auth = true;
    assert!(initialize(config).await.is_err());
    let mut config = f.app.config.clone();
    config.allowed_hosts = None;
    assert!(initialize(config).await.is_err());
}

#[tokio::test]
async fn writable_is_reported_by_session_login_and_password_change() {
    let _serial = password::PASSWORD_TEST_LOCK.lock().await;
    for write in [false, true] {
        let f = Fixture::new(write);
        let old = "Write-test-password-42";
        f.app
            .data
            .lock()
            .unwrap()
            .auth
            .as_mut()
            .unwrap()
            .password_hash = password::hash_password(old.into()).await.unwrap();
        let request = |path: &str, method: &str, body: Value, s: Option<&Session>| {
            let mut r = Request::builder()
                .method(method)
                .uri(path)
                .header("host", "localhost:8080")
                .header("content-type", "application/json");
            if let Some(s) = s {
                r = r
                    .header("cookie", format!("katalog_session={}", s.token))
                    .header("x-csrf-token", &s.csrf);
            }
            r.body(Body::from(body.to_string())).unwrap()
        };
        for forced in [false, true] {
            f.app
                .data
                .lock()
                .unwrap()
                .auth
                .as_mut()
                .unwrap()
                .must_change = forced;
            let r = router(f.app.clone())
                .oneshot(request("/api/session", "GET", json!({}), Some(&f.session)))
                .await
                .unwrap();
            assert_eq!(value(r).await["writable"], write && !forced);
            let r = router(f.app.clone())
                .oneshot(request("/api/login", "POST", json!({"password":old}), None))
                .await
                .unwrap();
            assert_eq!(r.status(), 200);
            assert_eq!(value(r).await["writable"], write && !forced);
        }
        let r = router(f.app.clone())
            .oneshot(request(
                "/api/password",
                "POST",
                json!({"current":old,"new":"Changed-write-password-84"}),
                Some(&f.session),
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(value(r).await["writable"], write);
        let r = router(f.app.clone())
            .oneshot(request("/api/session", "GET", json!({}), Some(&f.session)))
            .await
            .unwrap();
        assert_eq!(value(r).await["writable"], false);
    }
}

#[tokio::test(start_paused = true)]
async fn started_blocking_write_finishes_after_http_timeout() {
    let f = Fixture::new(true);
    let app = f.app.clone();
    let (locked, rx) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let blocker = tokio::task::spawn_blocking(move || {
        let _lock = app.writer.as_ref().unwrap().conn.lock().unwrap();
        locked.send(()).unwrap();
        wait.recv().unwrap();
    });
    rx.await.unwrap();
    let request = f.request(
        "set_favorite",
        Body::from(json!({"fileId":f.id,"favorite":true}).to_string()),
        Some(&f.session),
        Some("http://localhost:8080"),
        true,
    );
    let app_router = router(f.app.clone());
    let pending = tokio::spawn(async move { app_router.oneshot(request).await.unwrap() });
    while f.app.writer.as_ref().unwrap().worker.available_permits() != 0 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    assert_eq!(pending.await.unwrap().status(), 408);
    release.send(()).unwrap();
    blocker.await.unwrap();
    while f.app.writer.as_ref().unwrap().worker.available_permits() != 1 {
        tokio::task::yield_now().await;
    }
    assert!(f.favorite());
}

#[tokio::test]
async fn signed_positive_id_is_bad_request() {
    let f = Fixture::new(true);
    assert_eq!(
        f.send(
            "set_favorite",
            json!({"fileId":format!("+{}", f.id), "favorite":true})
        )
        .await
        .status(),
        400
    );
    assert!(!f.favorite());
}

struct ControlledBody {
    ready: tokio::sync::oneshot::Receiver<axum::body::Bytes>,
    polled: Arc<std::sync::atomic::AtomicBool>,
    done: bool,
}
impl hyper::body::Body for ControlledBody {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;
    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        use std::future::Future;
        use std::task::Poll;
        self.polled.store(true, Ordering::SeqCst);
        if self.done {
            return Poll::Ready(None);
        }
        match std::pin::Pin::new(&mut self.ready).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                self.done = true;
                Poll::Ready(Some(
                    result
                        .map(hyper::body::Frame::data)
                        .map_err(std::io::Error::other),
                ))
            }
        }
    }
}
fn controlled_body() -> (
    Body,
    tokio::sync::oneshot::Sender<axum::body::Bytes>,
    Arc<std::sync::atomic::AtomicBool>,
) {
    let (send, ready) = tokio::sync::oneshot::channel();
    let polled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    (
        Body::new(ControlledBody {
            ready,
            polled: polled.clone(),
            done: false,
        }),
        send,
        polled,
    )
}

#[tokio::test(start_paused = true)]
async fn photo_reserves_admission_before_reading_and_releases_on_abort_and_error() {
    let f = Fixture::new(true);
    let gate = f.app.writer.as_ref().unwrap();
    for abort in [true, false] {
        let (body, send, polled) = controlled_body();
        let req = f.request(
            "add_print_log_entry",
            body,
            Some(&f.session),
            Some("http://localhost:8080"),
            true,
        );
        let pending = tokio::spawn(router(f.app.clone()).oneshot(req));
        while !polled.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
        assert_eq!(gate.admission.available_permits(), 32);
        if abort {
            pending.abort();
            let _ = pending.await;
        } else {
            send.send(axum::body::Bytes::from_static(b"{")).unwrap();
            assert_eq!(pending.await.unwrap().unwrap().status(), 400);
        }
        assert_eq!(gate.admission.available_permits(), 33);
    }
    let permits = gate.admission.clone().acquire_many_owned(33).await.unwrap();
    let (body, _send, polled) = controlled_body();
    let req = f.request(
        "add_print_log_entry",
        body,
        Some(&f.session),
        Some("http://localhost:8080"),
        true,
    );
    assert_eq!(
        router(f.app.clone()).oneshot(req).await.unwrap().status(),
        503
    );
    assert!(!polled.load(Ordering::SeqCst));
    drop(permits);
    for _ in 0..120 {
        f.app
            .data
            .lock()
            .unwrap()
            .sessions
            .get_mut(&f.session.token)
            .unwrap()
            .bucket
            .take(100);
    }
    let (body, _send, polled) = controlled_body();
    let req = f.request(
        "add_print_log_entry",
        body,
        Some(&f.session),
        Some("http://localhost:8080"),
        true,
    );
    assert_eq!(
        router(f.app.clone()).oneshot(req).await.unwrap().status(),
        429
    );
    assert!(!polled.load(Ordering::SeqCst));
}

#[tokio::test(start_paused = true)]
async fn photo_body_has_30_seconds_other_bodies_keep_15_seconds() {
    let f = Fixture::new(true);
    for (command, seconds) in [
        ("add_print_log_entry", 30),
        ("add_filament_spool", 30),
        ("update_filament_spool", 30),
        ("set_render_snapshot", 30),
        ("set_custom_image", 30),
        ("set_favorite", 15),
        ("get_app_version", 15),
    ] {
        let (body, _send, polled) = controlled_body();
        let req = f.request(
            command,
            body,
            Some(&f.session),
            Some("http://localhost:8080"),
            true,
        );
        let pending = tokio::spawn(router(f.app.clone()).oneshot(req));
        while !polled.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(std::time::Duration::from_secs(seconds - 1)).await;
        tokio::task::yield_now().await;
        assert!(!pending.is_finished(), "{command}");
        tokio::time::advance(std::time::Duration::from_secs(2)).await;
        assert_eq!(pending.await.unwrap().unwrap().status(), 408);
        assert_eq!(
            f.app.writer.as_ref().unwrap().admission.available_permits(),
            33
        );
    }
}

#[tokio::test]
async fn poisoned_writer_rolls_back_open_transaction_and_recovers() {
    let f = Fixture::new(true);
    let app = f.app.clone();
    let crash = tokio::task::spawn_blocking(move || {
        let conn = app.writer.as_ref().unwrap().conn.lock().unwrap();
        conn.execute_batch("BEGIN IMMEDIATE; UPDATE files SET favorite=1")
            .unwrap();
        panic!("controlled worker panic");
    })
    .await;
    assert!(crash.unwrap_err().is_panic());
    assert_eq!(
        f.send(
            "set_source_url",
            json!({"fileId":f.id,"url":"https://example.test"})
        )
        .await
        .status(),
        200
    );
    assert!(!f.favorite());
    let conn = f
        .app
        .writer
        .as_ref()
        .unwrap()
        .conn
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    assert!(conn.is_autocommit());
    assert_eq!(
        db::get_file(&conn, f.id.parse().unwrap())
            .unwrap()
            .unwrap()
            .source_url
            .as_deref(),
        Some("https://example.test")
    );
}

#[test]
fn audit_logs_only_write_mode_with_http_status_and_redacts_internal_errors() {
    const CHILD: &str = "KATALOG_TEST_AUDIT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let f = Fixture::new(false);
            for cmd in WRITE_COMMANDS { assert_eq!(f.send(cmd, json!({})).await.status(), 404); }
            eprintln!("READ_ONLY_END");
            let f = Fixture::new(true);
            let (body, _send, polled) = controlled_body();
            let req = f.request("add_print_log_entry", body, Some(&f.session), Some("http://localhost:8080"), true);
            let pending = tokio::spawn(router(f.app.clone()).oneshot(req));
            while !polled.load(Ordering::SeqCst) { tokio::task::yield_now().await; }
            pending.abort();
            let _ = pending.await;
            let req = f.request("set_favorite", Body::empty(), None, Some("http://localhost:8080"), true);
            assert_eq!(router(f.app.clone()).oneshot(req).await.unwrap().status(), 401);
            let req = f.request("set_favorite", Body::empty(), Some(&f.session), None, true);
            assert_eq!(router(f.app.clone()).oneshot(req).await.unwrap().status(), 403);
            assert_eq!(f.send("set_favorite", json!({"padding":"x".repeat(17000)})).await.status(), 413);
            let permits = f.app.writer.as_ref().unwrap().admission.clone().acquire_many_owned(33).await.unwrap();
            assert_eq!(f.send("set_favorite", json!({"fileId":f.id,"favorite":true})).await.status(), 503);
            drop(permits);
            assert_eq!(f.send("set_favorite", json!({"fileId":f.id,"favorite":true})).await.status(), 200);
            f.app.writer.as_ref().unwrap().conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_write BEFORE UPDATE ON files BEGIN SELECT RAISE(ABORT,'PRIVATE_CONTENT'); END;").unwrap();
            let r = f.send("set_favorite", json!({"fileId":f.id,"favorite":false})).await;
            assert_eq!(r.status(), 500);
            assert!(!value(r).await.to_string().contains("PRIVATE_CONTENT"));
            for _ in 0..120 { f.app.data.lock().unwrap().sessions.get_mut(&f.session.token).unwrap().bucket.take(100); }
            assert_eq!(f.send("set_favorite", json!({"fileId":f.id,"favorite":false})).await.status(), 429);
        });
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "web::write_tests::audit_logs_only_write_mode_with_http_status_and_redacts_internal_errors", "--nocapture"])
        .env(CHILD, "1").output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    let (read_only, writes) = stderr.split_once("READ_ONLY_END").unwrap();
    assert!(!read_only.contains("write time="), "{read_only}");
    let lines: Vec<_> = writes
        .lines()
        .filter(|l| l.starts_with("write time="))
        .collect();
    assert_eq!(lines.len(), 8, "{writes}");
    for status in [401, 403, 413, 503, 200, 500, 429, 499] {
        assert!(
            lines
                .iter()
                .any(|l| l.contains(&format!("status={status}"))),
            "{writes}"
        );
    }
    assert!(writes.contains("error_kind=sqlite_constraint"), "{writes}");
    assert!(!writes.contains("PRIVATE_CONTENT"));
}

#[tokio::test]
async fn manager_reads_are_available_without_write_mode() {
    let f = Fixture::new(false);
    for (cmd, args, expected) in [
        ("list_filament_spools", json!({}), json!([])),
        ("list_printers", json!({}), json!([])),
        (
            "list_printer_history",
            json!({"printerId":"999"}),
            json!([]),
        ),
        (
            "get_last_printer_for_file",
            json!({"fileId":"999"}),
            Value::Null,
        ),
        (
            "check_filament",
            json!({"fileIds":["999", "bad"]}),
            json!([]),
        ),
    ] {
        let r = f.send(cmd, args).await;
        assert_eq!(r.status(), 200, "{cmd}");
        assert_eq!(value(r).await, expected);
    }
}

#[tokio::test]
async fn manager_printer_writes_and_strict_errors() {
    let f = Fixture::new(true);
    let r = f
        .send("add_printer", json!({"name":"A", "holderName":"Holder"}))
        .await;
    assert_eq!(r.status(), 200);
    let p = value(r).await;
    assert_eq!(p["units"].as_array().unwrap().len(), 1);
    assert_eq!(
        f.send("rename_printer", json!({"printerId":p["id"], "name":"B"}))
            .await
            .status(),
        200
    );
    assert_eq!(
        f.send("rename_printer", json!({"printerId":"999", "name":"B"}))
            .await
            .status(),
        404
    );
    assert_eq!(
        f.send("rename_printer", json!({"printerId":p["id"], "name":" "}))
            .await
            .status(),
        422
    );
    assert_eq!(
        f.send("reorder_printers", json!({"ids":[p["id"], "999"]}))
            .await
            .status(),
        422
    );
    let all = value(f.send("list_printers", json!({})).await).await;
    assert_eq!(all[0]["name"], "B");
}

fn manager_spool() -> Value {
    json!({"id":"", "material":"PLA", "location":"Shelf", "diameterMm":1.75, "originalWeightG":1000,"remainingWeightG":1000,"kind":"filament"})
}
#[tokio::test]
async fn manager_http_all_writes_round_trip_and_rejected_commands_stay_closed() {
    let f = Fixture::new(true);
    let p = value(
        f.send("add_printer", json!({"name":"A","holderName":"Holder"}))
            .await,
    )
    .await;
    let unit = value(
        f.send(
            "add_unit",
            json!({"printerId":p["id"],"kind":"custom","name":"Custom","slotCount":2}),
        )
        .await,
    )
    .await;
    let s = value(
        f.send("add_filament_spool", json!({"spool":manager_spool()}))
            .await,
    )
    .await;
    let mut changed = s.clone();
    changed["material"] = json!("PETG");
    for (cmd, args) in [
        ("update_filament_spool", json!({"spool":changed})),
        (
            "update_printer_details",
            json!({"id":p["id"],"details":{"manufacturer":"Maker","nozzleMm":0.4}}),
        ),
        ("reorder_printers", json!({"ids":[p["id"]]})),
        (
            "update_unit",
            json!({"unitId":unit["id"],"name":"Changed","slotCount":1}),
        ),
        (
            "reorder_units",
            json!({"printerId":p["id"],"unitIds":[unit["id"],p["units"][0]["id"]]}),
        ),
        (
            "load_spool",
            json!({"spoolId":s["id"],"unitId":unit["id"],"slotIndex":0}),
        ),
        (
            "unload_spool",
            json!({"spoolId":s["id"],"location":"New shelf"}),
        ),
        ("delete_unit", json!({"unitId":unit["id"]})),
        ("delete_filament_spool", json!({"spoolId":s["id"]})),
        ("delete_printer", json!({"printerId":p["id"]})),
    ] {
        assert_eq!(f.send(cmd, args).await.status(), 200, "{cmd}");
    }
    assert_eq!(
        value(f.send("list_printers", json!({})).await).await,
        json!([])
    );
    for cmd in [
        "set_printer_link_enabled",
        "test_printer_connection",
        "remove_printer_connection",
        "sync_printers_now",
        "pick_and_read_image",
    ] {
        assert_eq!(f.send(cmd, json!({})).await.status(), 404, "{cmd}");
    }
}
#[tokio::test]
async fn manager_http_auth_csrf_and_nonimage_body_limits_cover_every_write() {
    let f = Fixture::new(true);
    for cmd in &WRITE_COMMANDS[18..] {
        for (session, csrf, status) in [(None, false, 401), (Some(&f.session), false, 403)] {
            let request = f.request(
                cmd,
                Body::from("{}"),
                session,
                Some("http://localhost:8080"),
                csrf,
            );
            assert_eq!(
                router(f.app.clone())
                    .oneshot(request)
                    .await
                    .unwrap()
                    .status(),
                status,
                "{cmd}"
            );
        }
        if !crate::write::image_command(cmd) {
            let request = f.request(
                cmd,
                Body::from(" ".repeat(16 * 1024 + 1)),
                Some(&f.session),
                Some("http://localhost:8080"),
                true,
            );
            assert_eq!(
                router(f.app.clone())
                    .oneshot(request)
                    .await
                    .unwrap()
                    .status(),
                413,
                "{cmd}"
            );
        }
    }
    for cmd in [
        "list_filament_spools",
        "get_filament_spool_image",
        "list_printers",
        "list_printer_history",
        "get_last_printer_for_file",
        "check_filament",
    ] {
        let request = f.request(
            cmd,
            Body::from("{}"),
            None,
            Some("http://localhost:8080"),
            false,
        );
        assert_eq!(
            router(f.app.clone())
                .oneshot(request)
                .await
                .unwrap()
                .status(),
            401,
            "{cmd}"
        );
    }
}
#[tokio::test]
async fn manager_http_images_large_bodies_chunked_overflow_and_strict_nested_dto() {
    use base64::Engine;
    let f = Fixture::new(true);
    let mut s = manager_spool();
    let mut image = vec![0u8; katalog_core::write::MAX_CUSTOM_IMAGE_BYTES];
    image[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    s["imagePng"] = json!(base64::engine::general_purpose::STANDARD.encode(image));
    let created = f.send("add_filament_spool", json!({"spool":s})).await;
    assert_eq!(created.status(), 200);
    s = value(created).await;
    assert_eq!(
        f.send("update_filament_spool", json!({"spool":s}))
            .await
            .status(),
        200
    );
    let listed = value(f.send("list_filament_spools", json!({})).await).await;
    assert_eq!(listed[0]["hasImage"], true);
    assert!(listed[0].get("imagePng").is_none());
    let img = value(
        f.send("get_filament_spool_image", json!({"id":s["id"]}))
            .await,
    )
    .await;
    assert_eq!(img["imagePng"], s["imagePng"]);
    assert_eq!(
        f.send("get_filament_spool_image", json!({"id":"999"}))
            .await
            .status(),
        404
    );
    for cmd in ["add_filament_spool", "update_filament_spool"] {
        let mut bad = s.clone();
        bad["extra"] = json!(true);
        assert_eq!(f.send(cmd, json!({"spool":bad})).await.status(), 400);
        for content_length in [false, true] {
            let bytes = " ".repeat(body_limit(cmd) + 1);
            let (body, send, _) = controlled_body();
            send.send(bytes.clone().into()).unwrap();
            let mut request = f.request(
                cmd,
                body,
                Some(&f.session),
                Some("http://localhost:8080"),
                true,
            );
            if content_length {
                request
                    .headers_mut()
                    .insert("content-length", bytes.len().to_string().parse().unwrap());
            }
            assert_eq!(
                router(f.app.clone())
                    .oneshot(request)
                    .await
                    .unwrap()
                    .status(),
                413,
                "{cmd}"
            );
        }
    }
    assert_eq!(
        f.send("check_filament", json!({"fileIds":vec!["1";1001]}))
            .await
            .status(),
        422
    );
}
#[tokio::test(start_paused = true)]
async fn manager_image_admission_rate_and_logout_during_body_wait() {
    for cmd in ["add_filament_spool", "update_filament_spool", "set_render_snapshot", "set_custom_image"] {
        let f = Fixture::new(true);
        let gate = f.app.writer.as_ref().unwrap();
        let permits = gate.admission.clone().acquire_many_owned(33).await.unwrap();
        let (body, _send, polled) = controlled_body();
        let request = f.request(
            cmd,
            body,
            Some(&f.session),
            Some("http://localhost:8080"),
            true,
        );
        let r = router(f.app.clone()).oneshot(request).await.unwrap();
        assert_eq!(r.status(), 503);
        assert_eq!(r.headers()["retry-after"], "1");
        assert!(!polled.load(Ordering::SeqCst));
        drop(permits);
        let (body, send, polled) = controlled_body();
        let request = f.request(
            cmd,
            body,
            Some(&f.session),
            Some("http://localhost:8080"),
            true,
        );
        let pending = tokio::spawn(router(f.app.clone()).oneshot(request));
        while !polled.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
        assert_eq!(gate.admission.available_permits(), 32);
        f.app.data.lock().unwrap().sessions.remove(&f.session.token);
        send.send(json!({"spool":manager_spool()}).to_string().into())
            .unwrap();
        assert_eq!(pending.await.unwrap().unwrap().status(), 401);
        let c = db::open_read_only(&f.app.config.db_path).unwrap();
        assert!(db::list_filament_spools(&c).unwrap().is_empty());
        assert_eq!(gate.admission.available_permits(), 33);
    }
}

#[tokio::test]
async fn manager_busy_remains_503_and_business_rejections_are_422() {
    let f = Fixture::new(true);
    let p = value(
        f.send("add_printer", json!({"name":"P","holderName":"H"}))
            .await,
    )
    .await;
    assert_eq!(
        f.send(
            "add_unit",
            json!({"printerId":p["id"],"kind":"resin_vat","name":"Vat"})
        )
        .await
        .status(),
        422
    );
    f.app
        .writer
        .as_ref()
        .unwrap()
        .conn
        .lock()
        .unwrap()
        .busy_timeout(std::time::Duration::from_millis(10))
        .unwrap();
    let c = db::open_read_write(&f.app.config.db_path).unwrap();
    c.execute_batch("BEGIN IMMEDIATE").unwrap();
    for (cmd, args) in [
        ("add_printer", json!({"name":"B","holderName":"H"})),
        ("add_filament_spool", json!({"spool":manager_spool()})),
    ] {
        let r = f.send(cmd, args).await;
        assert_eq!(r.status(), 503);
        assert_eq!(r.headers()["retry-after"], "1");
    }
    c.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        value(f.send("list_printers", json!({})).await)
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[tokio::test]
async fn manager_image_writes_recheck_session_after_database_mutex_wait() {
    for cmd in ["add_filament_spool", "update_filament_spool"] {
        let f = Fixture::new(true);
        let s = value(
            f.send("add_filament_spool", json!({"spool":manager_spool()}))
                .await,
        )
        .await;
        let mut changed = s.clone();
        changed["material"] = json!("PETG");
        let app = f.app.clone();
        let (locked, rx) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            let _lock = app.writer.as_ref().unwrap().conn.lock().unwrap();
            locked.send(()).unwrap();
            wait.recv().unwrap();
        });
        rx.await.unwrap();
        let req = f.request(
            cmd,
            Body::from(json!({"spool":changed}).to_string()),
            Some(&f.session),
            Some("http://localhost:8080"),
            true,
        );
        let pending = tokio::spawn(router(f.app.clone()).oneshot(req));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while f.app.writer.as_ref().unwrap().worker.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        f.app.data.lock().unwrap().sessions.remove(&f.session.token);
        release.send(()).unwrap();
        blocker.await.unwrap();
        assert_eq!(pending.await.unwrap().unwrap().status(), 401);
        let c = db::open_read_only(&f.app.config.db_path).unwrap();
        let spools = db::list_filament_spools(&c).unwrap();
        assert_eq!(spools.len(), 1);
        assert_eq!(spools[0].material, "PLA");
    }
}

#[tokio::test]
async fn manager_http_image_patch_preserves_replaces_clears_and_rejects_invalid() {
    let f = Fixture::new(true);
    let mut input = manager_spool();
    input["imagePng"] = json!("iVBORw0KGgo=");
    let created = value(f.send("add_filament_spool", json!({"spool":input})).await).await;
    input["id"] = created["id"].clone();
    input.as_object_mut().unwrap().remove("imagePng");
    input["material"] = json!("PETG");
    let response = f.send("update_filament_spool", json!({"spool":input})).await;
    assert_eq!(response.status(), 200);
    assert_eq!(value(response).await["material"], "PETG");
    let c = db::connect(&f.dir.join("catalog.db")).unwrap();
    let id = created["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(db::get_filament_spool(&c, id).unwrap().image_png, Some(vec![137,80,78,71,13,10,26,10]));
    for (image, status, expected) in [
        (json!("/9j/"), 200, Some(vec![255,216,255])),
        (json!("R0lGODlh"), 422, Some(vec![255,216,255])),
        (Value::Null, 200, None),
    ] {
        input["imagePng"] = image;
        assert_eq!(f.send("update_filament_spool", json!({"spool":input})).await.status(), status);
        assert_eq!(db::get_filament_spool(&c, id).unwrap().image_png, expected);
    }
}

#[tokio::test]
async fn manager_http_printer_details_reject_unknown_keys_without_changes() {
    let f = Fixture::new(true);
    let p = value(f.send("add_printer", json!({"name":"P","holderName":"H"})).await).await;
    assert_eq!(f.send("update_printer_details", json!({"id":p["id"],"details":{"manufacturer":"Changed","nozzleMM":0.4}})).await.status(), 422);
    assert_eq!(value(f.send("list_printers", json!({})).await).await[0], p);
}

#[tokio::test]
async fn session_explicitly_reports_upload_disabled() {
    let fixture = Fixture::new(true);
    let request = Request::builder().uri("/api/session").header("host", "localhost:8080")
        .header("cookie", format!("katalog_session={}", fixture.session.token))
        .body(Body::empty()).unwrap();
    let response = router(fixture.app.clone()).oneshot(request).await.unwrap();
    let value: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(value["uploadable"], false);
}

#[tokio::test]
async fn snapshot_http_storage_validation_and_access() {
    let f = Fixture::new(true);
    let args = json!({"fileId":f.id,"imageBase64":"iVBORw0KGgo="});
    assert_eq!(
        f.send("set_render_snapshot", args.clone()).await.status(),
        200
    );
    let c = db::open_read_only(&f.app.config.db_path).unwrap();
    assert_eq!(
        db::get_file(&c, f.id.parse().unwrap())
            .unwrap()
            .unwrap()
            .render_snapshot_png
            .as_deref(),
        Some(&b"\x89PNG\r\n\x1a\n"[..])
    );
    let summaries = value(f.send("list_file_summaries", json!({})).await).await;
    assert_eq!(summaries[0]["hasRenderSnapshot"], true);
    for (args, status) in [
        (json!({"fileId":"999","imageBase64":"iVBORw0KGgo="}), 404),
        (json!({"fileId":f.id,"imageBase64":"bad"}), 422),
        (
            json!({"fileId":f.id,"imageBase64":"iVBORw0KGgo=","extra":true}),
            422,
        ),
    ] {
        assert_eq!(f.send("set_render_snapshot", args).await.status(), status);
    }
    for (session, csrf, status) in [(None, true, 401), (Some(&f.session), false, 403)] {
        let req = f.request(
            "set_render_snapshot",
            Body::from(args.to_string()),
            session,
            Some("http://localhost:8080"),
            csrf,
        );
        assert_eq!(
            router(f.app.clone()).oneshot(req).await.unwrap().status(),
            status
        );
    }
    for content_length in [false, true] {
        let bytes = " ".repeat(crate::write::body_limit("add_print_log_entry") + 1);
        let mut req = f.request(
            "set_render_snapshot",
            Body::from(bytes.clone()),
            Some(&f.session),
            Some("http://localhost:8080"),
            true,
        );
        if content_length {
            req.headers_mut()
                .insert("content-length", bytes.len().to_string().parse().unwrap());
        }
        assert_eq!(
            router(f.app.clone()).oneshot(req).await.unwrap().status(),
            413
        );
    }
    f.app
        .writer
        .as_ref()
        .unwrap()
        .conn
        .lock()
        .unwrap()
        .execute("UPDATE files SET deleted_at='today' WHERE id=?1", [&f.id])
        .unwrap();
    assert_eq!(
        f.send("set_render_snapshot", args.clone()).await.status(),
        404
    );
    assert_eq!(
        Fixture::new(false)
            .send("set_render_snapshot", args)
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn catalog_removal_is_atomic_strict_and_preserves_disk() {
    let f = Fixture::new(true);
    let disk = f.dir.join("keep.stl");
    std::fs::write(&disk, b"unchanged").unwrap();
    {
        let c = f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        c.execute("UPDATE files SET path=?1 WHERE id=?2", [disk.to_str().unwrap(), f.id.as_str()]).unwrap();
        c.execute_batch("INSERT INTO tags VALUES(1,'keep',1); INSERT INTO file_tags VALUES(1,1); INSERT INTO collections VALUES(1,'keep','today'); INSERT INTO collection_files VALUES(1,1,0);").unwrap();
    }
    for args in [json!({"fileIds":[f.id,"9999"]}), json!({"fileIds":["9999",f.id]})] {
        assert_eq!(f.send("remove_files_from_catalog",args).await.status(),404);
        assert!(!f.favorite());
    }
    for args in [json!({"fileIds":vec![f.id.clone();201]}),json!({"fileIds":[f.id],"extra":true}),json!({"fileIds":["0"]}),json!({"fileIds":["-1"]}),json!({"fileIds":["1.5"]})] {
        assert_eq!(f.send("remove_files_from_catalog",args).await.status(),422);
        assert!(!f.favorite());
    }
    let r=f.send("remove_files_from_catalog",json!({"fileIds":[f.id,f.id]})).await;
    assert_eq!(r.status(),200);
    assert_eq!(value(r).await,json!(1));
    let c=db::open_read_only(&f.app.config.db_path).unwrap();
    for table in ["files","file_tags","collection_files"] {
        assert_eq!(c.query_row(&format!("SELECT COUNT(*) FROM {table}"),[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    for table in ["tags","collections"] {
        assert_eq!(c.query_row(&format!("SELECT COUNT(*) FROM {table}"),[],|r|r.get::<_,i64>(0)).unwrap(),1);
    }
    assert_eq!(std::fs::read(disk).unwrap(),b"unchanged");
}

#[tokio::test]
async fn catalog_removal_requires_write_session_csrf_and_active_model() {
    let disabled=Fixture::new(false);
    assert_eq!(disabled.send("remove_files_from_catalog",json!({"fileIds":[disabled.id]})).await.status(),404);
    let f=Fixture::new(true);
    for (session,csrf,status) in [(None,false,401),(Some(&f.session),false,403)] {
        let r=router(f.app.clone()).oneshot(f.request("remove_files_from_catalog",Body::from(json!({"fileIds":[f.id]}).to_string()),session,Some("http://localhost:8080"),csrf)).await.unwrap();
        assert_eq!(r.status(),status);
    }
    {
        let c=f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        db::soft_delete_file(&c,f.id.parse().unwrap(),Some("/trash/keep.stl"),"today").unwrap();
    }
    assert_eq!(f.send("remove_files_from_catalog",json!({"fileIds":[f.id]})).await.status(),404);
    assert!(db::get_file(&db::open_read_only(&f.app.config.db_path).unwrap(),f.id.parse().unwrap()).unwrap().is_some());
}

#[tokio::test]
async fn catalog_removal_rolls_back_delete_failure_and_counts_only_main_rows() {
    let f=Fixture::new(true);
    {
        let c=f.app.writer.as_ref().unwrap().conn.lock().unwrap();
        db::test_insert_minimal_file(&c,"/second.stl",None).unwrap();
        c.execute_batch("CREATE TRIGGER reject_remove BEFORE DELETE ON files WHEN OLD.id=2 BEGIN SELECT RAISE(ABORT,'failure'); END;").unwrap();
    }
    assert_eq!(f.send("remove_files_from_catalog",json!({"fileIds":["1","2"]})).await.status(),500);
    let mut c=f.app.writer.as_ref().unwrap().conn.lock().unwrap();
    assert!(db::get_file(&c,1).unwrap().is_some());
    assert!(db::get_file(&c,2).unwrap().is_some());
    c.execute_batch("DROP TRIGGER reject_remove; INSERT INTO tags VALUES(1,'keep',1); INSERT INTO file_tags VALUES(1,1);").unwrap();
    let result=katalog_core::write::web::remove_files_from_catalog(&mut c,&[1,1,2]).unwrap();
    assert_eq!(result.value,2);
    assert_eq!(result.changes,2);
}

#[tokio::test]
async fn restock_and_consume_resin_http_round_trip_and_strict_errors() {
    let f = Fixture::new(true);
    let filament = value(f.send("add_filament_spool", json!({"spool":manager_spool()})).await).await;
    let mut resin = manager_spool();
    resin["kind"] = json!("resin");
    resin["diameterMm"] = json!(0);
    resin["remainingWeightG"] = json!(100);
    let bottle = value(f.send("add_filament_spool", json!({"spool":resin})).await).await;

    let r = f.send("restock_filament_spool", json!({"templateId":filament["id"],"count":2,"weight":750.04,"price":null,"location":" Shelf B "})).await;
    assert_eq!(r.status(), 200);
    let created = value(r).await;
    assert_eq!(created.as_array().unwrap().len(), 2);
    assert_eq!(created[0]["originalWeightG"], json!(750.0));
    assert_eq!(created[0]["location"], json!("Shelf B"));
    assert_eq!(value(f.send("list_filament_spools", json!({})).await).await.as_array().unwrap().len(), 4);

    let r = f.send("consume_resin", json!({"spoolId":bottle["id"],"amountMl":25.44})).await;
    assert_eq!(r.status(), 200);
    assert_eq!(value(r).await["remainingWeightG"], json!(74.6));

    for (cmd, args, status) in [
        ("restock_filament_spool", json!({"templateId":filament["id"],"count":21,"weight":1}), 422),
        ("restock_filament_spool", json!({"templateId":filament["id"],"count":0,"weight":1}), 422),
        ("restock_filament_spool", json!({"templateId":filament["id"],"count":1,"weight":0}), 422),
        ("restock_filament_spool", json!({"templateId":filament["id"],"count":1,"weight":1,"price":-1}), 422),
        ("restock_filament_spool", json!({"templateId":"999999","count":1,"weight":1}), 404),
        ("restock_filament_spool", json!({"templateId":filament["id"],"count":1.5,"weight":1}), 400),
        ("restock_filament_spool", json!({"templateId":filament["id"],"count":"1","weight":1}), 400),
        ("restock_filament_spool", json!({"templateId":filament["id"],"count":1,"weight":"1"}), 400),
        ("restock_filament_spool", json!({"templateId":1,"count":1,"weight":1}), 400),
        ("restock_filament_spool", json!({"templateId":filament["id"],"count":1}), 400),
        ("consume_resin", json!({"spoolId":bottle["id"],"amountMl":0}), 422),
        ("consume_resin", json!({"spoolId":bottle["id"],"amountMl":-3}), 422),
        ("consume_resin", json!({"spoolId":bottle["id"],"amountMl":"3"}), 400),
        ("consume_resin", json!({"spoolId":filament["id"],"amountMl":3}), 422),
        ("consume_resin", json!({"spoolId":"999999","amountMl":3}), 404),
        ("consume_resin", json!({"spoolId":"abc","amountMl":3}), 400),
        ("consume_resin", json!({"amountMl":3}), 400),
    ] {
        assert_eq!(f.send(cmd, args.clone()).await.status(), status, "{cmd} {args}");
    }
    assert_eq!(value(f.send("list_filament_spools", json!({})).await).await.as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn restock_and_consume_resin_need_write_mode_and_a_session() {
    let f = Fixture::new(false);
    for cmd in ["restock_filament_spool", "consume_resin"] {
        assert_eq!(f.send(cmd, json!({})).await.status(), 404, "{cmd}");
    }
    let f = Fixture::new(true);
    for cmd in ["restock_filament_spool", "consume_resin"] {
        let r = router(f.app.clone())
            .oneshot(f.request(cmd, Body::from("{}"), None, Some("http://localhost:8080"), true))
            .await
            .unwrap();
        assert_eq!(r.status(), 401, "{cmd}");
    }
}

#[tokio::test]
async fn auto_tag_block_list_http_round_trip_and_strict_errors() {
    let f = Fixture::new(true);
    let list = || async { value(f.send("list_blocked_auto_tags", json!({})).await).await };
    assert_eq!(list().await, json!([]));

    // Blocking twice, in another language and case, ends in one entry (idempotent).
    for tag in ["Multicolor", "MEHRFARBIG", "  multicolore "] {
        assert_eq!(f.send("block_auto_tag", json!({"tag":tag})).await.status(), 200, "{tag}");
    }
    assert_eq!(f.send("block_auto_tag", json!({"tag":"Zahnrad"})).await.status(), 200);
    assert_eq!(list().await, json!(["mehrfarbig", "zahnrad"]));

    // Lifting a block that is not there is fine too.
    assert_eq!(f.send("unblock_auto_tag", json!({"tag":"zahnrad"})).await.status(), 200);
    assert_eq!(f.send("unblock_auto_tag", json!({"tag":"zahnrad"})).await.status(), 200);
    assert_eq!(list().await, json!(["mehrfarbig"]));

    let long = "x".repeat(101);
    for (cmd, args, status) in [
        ("block_auto_tag", json!({"tag":""}), 422),
        ("block_auto_tag", json!({"tag":"   "}), 422),
        ("block_auto_tag", json!({"tag":"a\nb"}), 422),
        ("block_auto_tag", json!({"tag":long}), 422),
        ("unblock_auto_tag", json!({"tag":""}), 422),
        ("unblock_auto_tag", json!({"tag":long}), 422),
        ("block_auto_tag", json!({"tag":5}), 400),
        ("block_auto_tag", json!({"tag":null}), 400),
        ("block_auto_tag", json!({}), 400),
        ("unblock_auto_tag", json!({"tag":["a"]}), 400),
        ("unblock_auto_tag", json!({}), 400),
    ] {
        assert_eq!(f.send(cmd, args.clone()).await.status(), status, "{cmd} {args}");
    }
    assert_eq!(list().await, json!(["mehrfarbig"]));
}

#[tokio::test]
async fn suggested_auto_tags_http_reads_one_model_and_rejects_bad_ids() {
    let f = Fixture::new(true);
    let c = katalog_core::db::connect(&f.app.config.db_path).unwrap();
    let id = katalog_core::db::test_insert_minimal_file(&c, "/Kabelhalter_v3.3mf", None).unwrap();
    drop(c);
    let r = f.send("list_suggested_auto_tags", json!({"fileId":id.to_string()})).await;
    assert_eq!(r.status(), 200);
    assert_eq!(value(r).await, json!(["kabelhalter"]));
    for (args, status) in [
        (json!({"fileId":"999999"}), 422),
        (json!({"fileId":"abc"}), 400),
        (json!({"fileId":1}), 400),
        (json!({}), 400),
    ] {
        assert_eq!(f.send("list_suggested_auto_tags", args.clone()).await.status(), status, "{args}");
    }
}

#[tokio::test]
async fn auto_tag_block_commands_need_write_mode_and_a_session() {
    let f = Fixture::new(false);
    for cmd in ["block_auto_tag", "unblock_auto_tag"] {
        assert_eq!(f.send(cmd, json!({"tag":"x"})).await.status(), 404, "{cmd}");
    }
    let f = Fixture::new(true);
    for cmd in ["block_auto_tag", "unblock_auto_tag"] {
        let r = router(f.app.clone())
            .oneshot(f.request(cmd, Body::from(r#"{"tag":"x"}"#), None, Some("http://localhost:8080"), true))
            .await
            .unwrap();
        assert_eq!(r.status(), 401, "{cmd}");
    }
    assert_eq!(value(f.send("list_blocked_auto_tags", json!({})).await).await, json!([]));
}

mod request_id_tests {
    use super::*;
    const KEY: &str = "0123456789abcdef-key-one";
    impl Fixture {
        fn keyed(&self, cmd: &str, args: &Value, key: Option<&str>, session: &Session) -> Request {
            let mut r = self.request(cmd, Body::from(args.to_string()), Some(session), Some("http://localhost:8080"), true);
            if let Some(key) = key {
                r.headers_mut().insert("x-request-id", key.parse().unwrap());
            }
            r
        }
        async fn send_keyed(&self, cmd: &str, args: Value, key: Option<&str>) -> Response {
            router(self.app.clone()).oneshot(self.keyed(cmd, &args, key, &self.session)).await.unwrap()
        }
        async fn collections(&self) -> usize {
            value(self.send("list_collections", json!({})).await).await.as_array().unwrap().len()
        }
    }
    fn replayed(r: &Response) -> bool {
        r.headers().get("x-idempotent-replay").is_some_and(|v| v == "1")
    }

    #[tokio::test]
    async fn same_key_twice_creates_once_and_replays_identical_answer() {
        let f = Fixture::new(true);
        let first = f.send_keyed("create_collection", json!({"name":"one"}), Some(KEY)).await;
        assert_eq!(first.status(), 200);
        assert!(!replayed(&first));
        let first = value(first).await;
        let second = f.send_keyed("create_collection", json!({"name":"one"}), Some(KEY)).await;
        assert_eq!(second.status(), 200);
        assert!(replayed(&second));
        assert_eq!(value(second).await, first);
        assert_eq!(f.collections().await, 1);
    }
    #[tokio::test]
    async fn counting_command_is_not_applied_twice() {
        let f = Fixture::new(true);
        let filament = value(f.send("add_filament_spool", json!({"spool":manager_spool()})).await).await;
        let args = json!({"templateId":filament["id"],"count":2,"weight":750.0});
        for _ in 0..2 {
            assert_eq!(f.send_keyed("restock_filament_spool", args.clone(), Some(KEY)).await.status(), 200);
        }
        assert_eq!(value(f.send("list_filament_spools", json!({})).await).await.as_array().unwrap().len(), 3);
    }
    #[tokio::test]
    async fn same_key_with_other_arguments_or_command_is_rejected() {
        let f = Fixture::new(true);
        f.send_keyed("create_collection", json!({"name":"one"}), Some(KEY)).await;
        let r = f.send_keyed("create_collection", json!({"name":"two"}), Some(KEY)).await;
        assert_eq!(r.status(), 422);
        assert_eq!(value(r).await["kind"], "request_id_reused");
        let r = f.send_keyed("add_printer", json!({"name":"one","holderName":"H"}), Some(KEY)).await;
        assert_eq!(r.status(), 422);
        assert_eq!(f.collections().await, 1);
    }
    #[tokio::test]
    async fn different_keys_or_no_key_execute_each_time() {
        let f = Fixture::new(true);
        f.send_keyed("create_collection", json!({"name":"one"}), Some(KEY)).await;
        f.send_keyed("create_collection", json!({"name":"one"}), Some("0123456789abcdef-key-two")).await;
        assert_eq!(f.collections().await, 2);
        for _ in 0..2 {
            let r = f.send_keyed("create_collection", json!({"name":"one"}), None).await;
            assert_eq!(r.status(), 200);
            assert!(!replayed(&r));
        }
        assert_eq!(f.collections().await, 4);
    }
    #[tokio::test]
    async fn malformed_or_repeated_header_is_rejected_before_any_work() {
        let f = Fixture::new(true);
        for key in ["short", "0123456789abcdef!!!!", &"a".repeat(65)] {
            let r = f.send_keyed("create_collection", json!({"name":"one"}), Some(key)).await;
            assert_eq!(r.status(), 400, "{key}");
            assert_eq!(value(r).await["kind"], "request_id_invalid");
        }
        let mut req = f.keyed("create_collection", &json!({"name":"one"}), Some(KEY), &f.session);
        req.headers_mut().append("x-request-id", "0123456789abcdef-key-two".parse().unwrap());
        assert_eq!(router(f.app.clone()).oneshot(req).await.unwrap().status(), 400);
        assert_eq!(f.collections().await, 0);
    }
    #[tokio::test]
    async fn access_checks_run_before_the_key_is_looked_at() {
        let f = Fixture::new(true);
        f.send_keyed("create_collection", json!({"name":"one"}), Some(KEY)).await;
        let mut req = f.keyed("create_collection", &json!({"name":"one"}), Some(KEY), &f.session);
        req.headers_mut().insert("x-csrf-token", "wrong".parse().unwrap());
        let r = router(f.app.clone()).oneshot(req).await.unwrap();
        assert_eq!(r.status(), 403);
        assert!(!replayed(&r));
    }
    #[tokio::test]
    async fn other_session_cannot_read_or_collide_with_a_foreign_key() {
        let f = Fixture::new(true);
        let first = value(f.send_keyed("create_collection", json!({"name":"one"}), Some(KEY)).await).await;
        let second = new_session(&mut f.app.data.lock().unwrap(), "second", 100).unwrap();
        let r = router(f.app.clone())
            .oneshot(f.keyed("create_collection", &json!({"name":"one"}), Some(KEY), &second))
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert!(!replayed(&r));
        assert_ne!(value(r).await["id"], first["id"]);
        assert_eq!(f.collections().await, 2);
    }
    #[tokio::test]
    async fn cancelled_request_finishes_in_background_and_retry_gets_that_result() {
        let f = Fixture::new(true);
        let conn = f.app.writer.as_ref().unwrap().conn.clone();
        let holder = std::thread::spawn(move || {
            let _g = conn.lock().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(600));
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let abandoned = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            router(f.app.clone()).oneshot(f.keyed("create_collection", &json!({"name":"slow"}), Some(KEY), &f.session)),
        )
        .await;
        assert!(abandoned.is_err(), "request must be cancelled while the database is busy");
        let retry = f.send_keyed("create_collection", json!({"name":"slow"}), Some(KEY)).await;
        assert_eq!(retry.status(), 200);
        assert!(replayed(&retry));
        holder.join().unwrap();
        assert_eq!(f.collections().await, 1);
    }
    #[tokio::test]
    async fn busy_database_answer_is_not_stored_so_the_retry_really_runs() {
        let f = Fixture::new(true);
        let conn = f.app.writer.as_ref().unwrap().conn.clone();
        let holder = std::thread::spawn(move || {
            let _g = conn.lock().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5600));
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let busy = f.send_keyed("create_collection", json!({"name":"later"}), Some(KEY)).await;
        assert_eq!(busy.status(), 503);
        holder.join().unwrap();
        let retry = f.send_keyed("create_collection", json!({"name":"later"}), Some(KEY)).await;
        assert_eq!(retry.status(), 200);
        assert!(!replayed(&retry));
        assert_eq!(f.collections().await, 1);
    }
    fn holder(f: &Fixture, millis: u64) -> std::thread::JoinHandle<()> {
        let conn = f.app.writer.as_ref().unwrap().conn.clone();
        let (locked, ready) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _g = conn.lock().unwrap();
            locked.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(millis));
        });
        ready.recv().unwrap();
        handle
    }
    #[tokio::test]
    async fn concurrent_repeats_during_execution_run_the_work_once_and_share_the_answer() {
        let f = Fixture::new(true);
        let hold = holder(&f, 600);
        let args = json!({"name":"shared"});
        let (a, b, c) = tokio::join!(
            f.send_keyed("create_collection", args.clone(), Some(KEY)),
            f.send_keyed("create_collection", args.clone(), Some(KEY)),
            f.send_keyed("create_collection", args.clone(), Some(KEY)),
        );
        hold.join().unwrap();
        let replays = [&a, &b, &c].iter().filter(|r| replayed(r)).count();
        assert_eq!([a.status(), b.status(), c.status()], [200, 200, 200]);
        assert_eq!(replays, 2);
        let (a, b, c) = (value(a).await, value(b).await, value(c).await);
        assert!(a == b && b == c);
        assert_eq!(f.collections().await, 1);
    }
    #[tokio::test]
    async fn server_error_while_others_wait_lets_exactly_one_waiter_run_again() {
        let f = Fixture::new(true);
        let hold = holder(&f, 5600);
        let args = json!({"name":"again"});
        let (a, b, c) = tokio::join!(
            f.send_keyed("create_collection", args.clone(), Some(KEY)),
            f.send_keyed("create_collection", args.clone(), Some(KEY)),
            f.send_keyed("create_collection", args.clone(), Some(KEY)),
        );
        hold.join().unwrap();
        let mut outcomes: Vec<_> = [&a, &b, &c].iter().map(|r| (r.status().as_u16(), replayed(r))).collect();
        outcomes.sort();
        assert_eq!(outcomes, [(200, false), (200, true), (503, false)]);
        assert_eq!(f.collections().await, 1);
    }
    #[tokio::test]
    async fn answer_over_the_storage_limit_is_not_replayable_but_the_change_happened_once() {
        use base64::Engine;
        let f = Fixture::new(true);
        let photo = base64::engine::general_purpose::STANDARD.encode({
            let mut bytes = vec![1; 100_000];
            bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
            bytes
        });
        let args = json!({"fileId":f.id,"printedAt":"2026-10-16T00:00:00.000Z","photoBase64":photo});
        let first = f.send_keyed("add_print_log_entry", args.clone(), Some(KEY)).await;
        assert_eq!(first.status(), 200);
        assert!(to_bytes(first.into_body(), 10 * 1024 * 1024).await.unwrap().len() > crate::idempotency::MAX_BODY_BYTES);
        let second = f.send_keyed("add_print_log_entry", args, Some(KEY)).await;
        assert_eq!(second.status(), 409);
        assert_eq!(value(second).await["kind"], "request_id_result_unavailable");
        let entries = value(f.send("list_print_log_entries", json!({"fileId":f.id})).await).await;
        assert_eq!(entries.as_array().unwrap().len(), 1);
    }
}

mod custom_image_and_rescan_tests {
    use super::*;
    use base64::Engine;
    const OBJ: &[u8] = b"v 0 0 0\nv 10 0 0\nv 0 20 0\nf 1 2 3\n";
    const KEY: &str = "0123456789abcdef-image-key";

    fn png(extra: usize) -> String {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.resize(8 + extra, 0);
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }
    impl Fixture {
        fn catalog_path(&self, path: &std::path::Path) {
            self.app.writer.as_ref().unwrap().conn.lock().unwrap()
                .execute("UPDATE files SET path=?1 WHERE id=?2", [path.to_str().unwrap(), self.id.as_str()]).unwrap();
        }
        fn stored_image(&self) -> Option<Vec<u8>> {
            db::get_file(&db::open_read_only(&self.app.config.db_path).unwrap(), self.id.parse().unwrap())
                .unwrap().unwrap().custom_image_png
        }
        fn stored_hash(&self) -> Option<String> {
            db::get_file(&db::open_read_only(&self.app.config.db_path).unwrap(), self.id.parse().unwrap())
                .unwrap().unwrap().content_hash
        }
        async fn send_raw(&self, cmd: &str, body: String, key: Option<&str>) -> Response {
            let mut r = self.request(cmd, Body::from(body), Some(&self.session), Some("http://localhost:8080"), true);
            if let Some(key) = key { r.headers_mut().insert("x-request-id", key.parse().unwrap()); }
            router(self.app.clone()).oneshot(r).await.unwrap()
        }
    }
    fn replayed(r: &Response) -> bool {
        r.headers().get("x-idempotent-replay").is_some_and(|v| v == "1")
    }

    #[tokio::test]
    async fn both_commands_need_write_mode_and_a_session() {
        let off = Fixture::new(false);
        for cmd in ["set_custom_image", "rescan_file_metadata"] {
            assert_eq!(off.send(cmd, json!({"fileId":off.id})).await.status(), 404, "{cmd}");
        }
        let f = Fixture::new(true);
        for cmd in ["set_custom_image", "rescan_file_metadata"] {
            for (session, csrf, status) in [(None, true, 401), (Some(&f.session), false, 403)] {
                let req = f.request(cmd, Body::from(json!({"fileId":f.id,"imageBase64":png(0)}).to_string()), session, Some("http://localhost:8080"), csrf);
                assert_eq!(router(f.app.clone()).oneshot(req).await.unwrap().status(), status, "{cmd}");
            }
        }
        assert_eq!(f.stored_image(), None);
    }

    #[tokio::test]
    async fn custom_image_is_stored_served_by_the_image_listing_and_validated() {
        let f = Fixture::new(true);
        let ok = f.send("set_custom_image", json!({"fileId":f.id,"imageBase64":png(0)})).await;
        assert_eq!(ok.status(), 200);
        // The answer stays small; the UI reloads the picture through the image listing.
        assert_eq!(value(ok).await, Value::Null);
        assert_eq!(f.stored_image().as_deref(), Some(&b"\x89PNG\r\n\x1a\n"[..]));
        let images = value(f.send("list_file_images", json!({"ids":[f.id]})).await).await;
        assert!(images[0]["customImage"].as_str().unwrap().starts_with("data:image/"));
        for (args, status) in [
            (json!({"fileId":"999","imageBase64":png(0)}), 404),
            (json!({"fileId":f.id,"imageBase64":"!!"}), 422),
            (json!({"fileId":f.id,"imageBase64":""}), 422),
            (json!({"fileId":f.id,"imageBase64":base64::engine::general_purpose::STANDARD.encode(b"GIF89a")}), 422),
            (json!({"fileId":f.id,"imageBase64":png(0),"extra":1}), 422),
            (json!({"fileId":f.id}), 422),
            (json!({"fileId":7,"imageBase64":png(0)}), 422),
            (json!({"fileId":"abc","imageBase64":png(0)}), 400),
        ] {
            assert_eq!(f.send("set_custom_image", args.clone()).await.status(), status, "{args}");
        }
        let r = f.send("set_custom_image", json!({"fileId":f.id,"imageBase64":""})).await;
        assert_eq!(value(r).await["message"], "imageUploadUnsupported");
        assert_eq!(f.stored_image().as_deref(), Some(&b"\x89PNG\r\n\x1a\n"[..]));
    }

    #[tokio::test]
    async fn only_the_image_command_gets_the_larger_body_limit() {
        let f = Fixture::new(true);
        let base = crate::write::body_limit("set_favorite");
        assert_eq!(base, 16 * 1024);
        assert_eq!(crate::write::body_limit("set_custom_image"), crate::write::body_limit("set_render_snapshot"));
        assert!(crate::write::body_limit("set_custom_image") > 5 * 1024 * 1024);
        assert_eq!(crate::write::body_limit("rescan_file_metadata"), base);
        // 200 KB of picture is far beyond the normal write limit but fine for a cover image.
        assert_eq!(f.send("set_custom_image", json!({"fileId":f.id,"imageBase64":png(150_000)})).await.status(), 200);
        for cmd in ["set_favorite", "rescan_file_metadata", "add_tag"] {
            let padded = json!({"fileId":f.id,"padding":"x".repeat(base)}).to_string();
            assert_eq!(f.send_raw(cmd, padded, None).await.status(), 413, "{cmd}");
        }
        // The full desktop limit works end to end, one byte more does not.
        let mut max = vec![0; katalog_core::write::MAX_CUSTOM_IMAGE_BYTES];
        max[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        let encoded = base64::engine::general_purpose::STANDARD.encode(&max);
        assert_eq!(f.send("set_custom_image", json!({"fileId":f.id,"imageBase64":encoded})).await.status(), 200);
        assert_eq!(f.stored_image().unwrap().len(), max.len());
        let too_big = base64::engine::general_purpose::STANDARD.encode(vec![0; max.len() + 1]);
        let r = f.send("set_custom_image", json!({"fileId":f.id,"imageBase64":too_big})).await;
        assert_eq!(r.status(), 422);
        assert_eq!(value(r).await["message"], "imageUploadTooLarge");
        let body = " ".repeat(crate::write::body_limit("set_custom_image") + 1);
        assert_eq!(f.send_raw("set_custom_image", body, None).await.status(), 413);
        assert_eq!(f.stored_image().unwrap().len(), max.len());
    }

    #[tokio::test]
    async fn repeating_the_image_request_with_the_same_key_applies_it_once() {
        let f = Fixture::new(true);
        let args = json!({"fileId":f.id,"imageBase64":png(0)}).to_string();
        let first = f.send_raw("set_custom_image", args.clone(), Some(KEY)).await;
        assert_eq!(first.status(), 200);
        assert!(!replayed(&first));
        // Out-of-band change: a second execution would overwrite it again.
        f.app.writer.as_ref().unwrap().conn.lock().unwrap()
            .execute("UPDATE files SET custom_image_png=x'ffd8ff'", []).unwrap();
        let second = f.send_raw("set_custom_image", args, Some(KEY)).await;
        assert_eq!(second.status(), 200);
        assert!(replayed(&second));
        assert_eq!(f.stored_image().as_deref(), Some(&b"\xff\xd8\xff"[..]));
        let other = json!({"fileId":f.id,"imageBase64":png(5)}).to_string();
        assert_eq!(f.send_raw("set_custom_image", other, Some(KEY)).await.status(), 422);
    }

    #[tokio::test]
    async fn rescan_reads_the_cataloged_file_and_returns_the_model() {
        let f = Fixture::new(true);
        let path = f.dir.join("model.obj");
        std::fs::write(&path, OBJ).unwrap();
        f.catalog_path(&path);
        let r = f.send("rescan_file_metadata", json!({"fileId":f.id})).await;
        assert_eq!(r.status(), 200);
        let model = value(r).await;
        assert_eq!(model["id"], f.id.as_str());
        assert_eq!(model["fileSizeBytes"], OBJ.len());
        assert!(model["dimensionsMm"].is_array());
        assert!(f.stored_hash().is_some());
        // The same result a plain read returns for the model.
        let listed = value(f.send("list_files_by_ids", json!({"ids":[f.id]})).await).await;
        assert_eq!(listed[0]["contentHash"], model["contentHash"]);
    }

    #[tokio::test]
    async fn rescan_refuses_bad_arguments_and_unreadable_files_without_leaking_paths() {
        let f = Fixture::new(true);
        let secret = f.dir.join("secret-name.obj");
        // Not written: the file is gone.
        f.catalog_path(&secret);
        let r = f.send("rescan_file_metadata", json!({"fileId":f.id})).await;
        assert_eq!(r.status(), 404);
        let text = value(r).await.to_string();
        assert!(text.contains("rescan_file_unavailable"), "{text}");
        assert!(!text.contains("secret-name") && !text.contains(f.dir.to_str().unwrap()), "{text}");
        let outside = std::env::temp_dir().join(format!("outside-{}.obj", f.id));
        std::fs::write(&outside, OBJ).unwrap();
        f.catalog_path(&outside);
        let r = f.send("rescan_file_metadata", json!({"fileId":f.id})).await;
        assert_eq!(r.status(), 404);
        let text = value(r).await.to_string();
        assert!(text.contains("rescan_file_unavailable") && !text.contains("outside-"), "{text}");
        assert_eq!(f.stored_hash(), None, "a refused rescan must not touch the record");
        let _ = std::fs::remove_file(&outside);
        let step = f.dir.join("part.step");
        std::fs::write(&step, b"ISO-10303-21;").unwrap();
        f.catalog_path(&step);
        let r = f.send("rescan_file_metadata", json!({"fileId":f.id})).await;
        assert_eq!(r.status(), 422);
        assert_eq!(value(r).await["message"], "rescan_step_unavailable");
        // Only the id is accepted from the client: a path argument is refused outright.
        for args in [json!({"fileId":f.id,"path":"/etc/passwd"}), json!({}), json!({"fileId":"0"}), json!({"fileId":"x"})] {
            assert_eq!(f.send("rescan_file_metadata", args.clone()).await.status(), 400, "{args}");
        }
        assert_eq!(f.send("rescan_file_metadata", json!({"fileId":"9999"})).await.status(), 404);
    }

    #[tokio::test]
    async fn repeating_the_rescan_with_the_same_key_reads_the_file_once() {
        let f = Fixture::new(true);
        let path = f.dir.join("again.obj");
        std::fs::write(&path, OBJ).unwrap();
        f.catalog_path(&path);
        let args = json!({"fileId":f.id}).to_string();
        let first = f.send_raw("rescan_file_metadata", args.clone(), Some(KEY)).await;
        assert_eq!(first.status(), 200);
        let first_hash = f.stored_hash();
        std::fs::write(&path, [OBJ, b"v 5 5 5\n"].concat()).unwrap();
        let replay = f.send_raw("rescan_file_metadata", args.clone(), Some(KEY)).await;
        assert!(replayed(&replay));
        assert_eq!(f.stored_hash(), first_hash);
        let fresh = f.send_raw("rescan_file_metadata", args, Some("0123456789abcdef-image-other")).await;
        assert_eq!(fresh.status(), 200);
        assert_ne!(f.stored_hash(), first_hash);
    }

    #[tokio::test]
    async fn slow_scan_does_not_block_writers_and_a_changed_file_is_refused() {
        use crate::write::rescan_hook::{HOLD, REACHED};
        use std::sync::atomic::Ordering::SeqCst;
        let f = Fixture::new(true);
        let path = f.dir.join("hold-model.obj");
        std::fs::write(&path, OBJ).unwrap();
        f.catalog_path(&path);
        REACHED.store(false, SeqCst);
        HOLD.store(true, SeqCst);
        let request = f.request("rescan_file_metadata", Body::from(json!({"fileId":f.id}).to_string()), Some(&f.session), Some("http://localhost:8080"), true);
        let scan = tokio::spawn(router(f.app.clone()).oneshot(request));
        for _ in 0..1000 {
            if REACHED.load(SeqCst) { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(REACHED.load(SeqCst), "scan never reached the parsed state");
        // The file is parsed but the write connection is free: another write goes straight through.
        assert_eq!(f.send("set_favorite", json!({"fileId":f.id,"favorite":true})).await.status(), 200);
        assert!(f.favorite());
        // Only one scan at a time; the waiting one is told to retry instead of queueing for ever.
        let second = f.send("rescan_file_metadata", json!({"fileId":f.id})).await;
        assert_eq!(second.status(), 503);
        assert!(second.headers().contains_key("retry-after"));
        std::fs::write(&path, [OBJ, b"v 2 2 2\n"].concat()).unwrap();
        HOLD.store(false, SeqCst);
        let r = scan.await.unwrap().unwrap();
        assert_eq!(r.status(), 409);
        assert_eq!(value(r).await["message"], "rescan_file_changed");
        assert_eq!(f.stored_hash(), None);
        assert!(f.favorite(), "the interleaved write must survive");
        // A fresh scan of the settled file succeeds and frees the place again.
        let ok = f.send("rescan_file_metadata", json!({"fileId":f.id})).await;
        assert_eq!(ok.status(), 200);
        assert!(f.stored_hash().is_some());
    }

    #[tokio::test]
    async fn jpeg_and_webp_covers_are_accepted_over_http() {
        let f = Fixture::new(true);
        for bytes in [&b"\xff\xd8\xff\xe0\x00\x10JFIF"[..], &b"RIFF\x04\x00\x00\x00WEBPVP8 "[..]] {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            assert_eq!(f.send("set_custom_image", json!({"fileId":f.id,"imageBase64":encoded})).await.status(), 200);
            assert_eq!(f.stored_image().as_deref(), Some(bytes));
        }
    }

    #[test]
    fn audit_records_both_commands_with_status_and_without_contents() {
        const CHILD: &str = "KATALOG_TEST_IMAGE_AUDIT_CHILD";
        if std::env::var_os(CHILD).is_some() {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let f = Fixture::new(true);
                let path = f.dir.join("audit-secret.obj");
                std::fs::write(&path, OBJ).unwrap();
                f.catalog_path(&path);
                assert_eq!(f.send("set_custom_image", json!({"fileId":f.id,"imageBase64":png(0)})).await.status(), 200);
                assert_eq!(f.send("rescan_file_metadata", json!({"fileId":f.id})).await.status(), 200);
                assert_eq!(f.send("set_custom_image", json!({"fileId":f.id,"imageBase64":"!!"})).await.status(), 422);
            });
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "web::write_tests::custom_image_and_rescan_tests::audit_records_both_commands_with_status_and_without_contents", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{stderr}");
        let lines: Vec<_> = stderr.lines().filter(|l| l.starts_with("write time=")).collect();
        assert_eq!(lines.len(), 3, "{stderr}");
        assert!(lines.iter().any(|l| l.contains("command=set_custom_image status=200")), "{stderr}");
        assert!(lines.iter().any(|l| l.contains("command=rescan_file_metadata status=200")), "{stderr}");
        assert!(lines.iter().any(|l| l.contains("command=set_custom_image status=422")), "{stderr}");
        assert!(!stderr.contains("audit-secret") && !stderr.contains("iVBOR"), "{stderr}");
    }
}
