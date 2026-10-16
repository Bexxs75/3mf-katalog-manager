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
            writer: write.then(|| WriteGate::new(db::open_read_write(&path).unwrap())),
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
    assert_eq!(WRITE_COMMANDS.len(), 18);
    assert_eq!(crate::READ_COMMANDS.len(), 14);
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
