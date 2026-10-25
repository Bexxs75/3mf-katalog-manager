use super::*;
use http_body_util::BodyExt;
use hyper::body::{Body as HttpBody, Frame};
use katalog_core::db;
use std::{
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};

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
    fn new(enabled: bool) -> Self {
        Self::with_env(enabled, &[])
    }
    fn with_env(enabled: bool, extra: &[(&str, &str)]) -> Self {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "web-upload-http-{:032x}",
            u128::from_ne_bytes(random)
        ));
        for sub in ["config", "models/uploads", "static"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let path = dir.join("config/catalog.db");
        drop(db::connect(&path).unwrap());
        let config = Config::from_lookup(|key| match key {
            _ if extra.iter().any(|(k, _)| *k == key) => {
                extra.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_owned())
            }
            "KATALOG_DB" => Some(path.to_string_lossy().into_owned()),
            "KATALOG_ROOTS" => Some(dir.join("models").to_string_lossy().into_owned()),
            "KATALOG_STATIC_DIR" => Some(dir.join("static").to_string_lossy().into_owned()),
            "KATALOG_UPLOAD_DIR" if enabled => {
                Some(dir.join("models/uploads").to_string_lossy().into_owned())
            }
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
            uploads: uploads::initialize(&config).unwrap(),
            writer: Some(WriteGate::new(db::open_read_write(&path).unwrap())),
            config,
            clock: time.clone(),
            data: Mutex::new(data),
            changes: Arc::new(tokio::sync::Mutex::new(())),
            geometry: Arc::new(GeometryGate::default()),
            downloads: Arc::new(tokio::sync::Semaphore::new(4)),
            export: crate::export::Runtime::new(),
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
    fn upload_request(&self, name: &str, bytes: &[u8]) -> Request {
        let mut r = self.request("POST", "/api/upload", Body::from(bytes.to_vec()));
        r.headers_mut()
            .insert("content-type", "application/octet-stream".parse().unwrap());
        r.headers_mut()
            .insert("x-upload-name", name.parse().unwrap());
        r.headers_mut()
            .insert("content-length", bytes.len().to_string().parse().unwrap());
        r
    }
    async fn send(&self, r: Request) -> Response {
        router(self.app.clone()).oneshot(r).await.unwrap()
    }
    async fn upload(&self, name: &str, bytes: &[u8]) -> Value {
        let r = self.send(self.upload_request(name, bytes)).await;
        assert_eq!(r.status(), 200);
        value(r).await
    }
    async fn invoke(&self, cmd: &str, args: Value) -> Response {
        let mut r = self.request(
            "POST",
            &format!("/api/invoke/{cmd}"),
            Body::from(args.to_string()),
        );
        r.headers_mut()
            .insert("content-type", "application/json".parse().unwrap());
        self.send(r).await
    }
    fn files(&self) -> usize {
        std::fs::read_dir(self.dir.join("models/uploads"))
            .unwrap()
            .count()
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
const MODEL: &[u8] = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";

#[tokio::test]
async fn upload_disabled_security_headers_and_authentication() {
    let disabled = Fixture::new(false);
    for path in ["/api/upload", "/api/uploads", "/api/events"] {
        assert_eq!(
            disabled
                .send(disabled.request("GET", path, Body::empty()))
                .await
                .status(),
            404
        );
    }
    let f = Fixture::new(true);
    for (remove, status) in [("cookie", 401), ("x-csrf-token", 403), ("origin", 403)] {
        let mut r = f.upload_request("a.obj", MODEL);
        r.headers_mut().remove(remove);
        assert_eq!(f.send(r).await.status(), status, "{remove}");
    }
    for (header, bad) in [("origin", "http://evil.test"), ("host", "evil.test")] {
        let mut r = f.upload_request("a.obj", MODEL);
        r.headers_mut().insert(header, bad.parse().unwrap());
        assert_eq!(f.send(r).await.status(), 403);
    }
    assert_eq!(f.files(), 0);
}
#[tokio::test]
async fn upload_length_signature_and_size_failures_leave_no_files() {
    let f = Fixture::new(true);
    let mut r = f.upload_request("a.obj", MODEL);
    r.headers_mut().remove("content-length");
    assert_eq!(f.send(r).await.status(), 411);
    for length in [MODEL.len() - 1, MODEL.len() + 1, 1024 * 1024 + 1] {
        let mut r = f.upload_request("a.obj", MODEL);
        r.headers_mut()
            .insert("content-length", length.to_string().parse().unwrap());
        assert_eq!(
            f.send(r).await.status(),
            if length > 1024 * 1024 { 413 } else { 400 }
        );
        assert_eq!(f.files(), 0);
    }
    assert_eq!(
        f.send(f.upload_request("bad.3mf", b"not zip"))
            .await
            .status(),
        400
    );
    assert_eq!(f.send(f.upload_request("x.3mf.exe", MODEL)).await.status(), 415);
    // Archives are accepted by name, but only with the matching signature.
    for name in ["x.zip", "x.7z"] {
        assert_eq!(f.send(f.upload_request(name, MODEL)).await.status(), 400);
    }
    assert_eq!(f.files(), 0);
}
#[tokio::test]
async fn parallel_same_name_uploads_never_overwrite_and_discard_removes_owned_file() {
    let f = Fixture::new(true);
    let (a, b) = tokio::join!(
        f.send(f.upload_request("same.obj", MODEL)),
        f.send(f.upload_request("same.obj", MODEL))
    );
    assert_eq!(a.status(), 200);
    assert_eq!(b.status(), 200);
    let a = value(a).await;
    let b = value(b).await;
    assert_ne!(a["uploadId"], b["uploadId"]);
    assert_ne!(a["name"], b["name"]);
    assert_eq!(f.files(), 2);
    assert!(!a.to_string().contains(&f.dir.to_string_lossy().to_string()));
    assert_eq!(
        f.invoke("discard_upload", json!({"uploadId":a["uploadId"]}))
            .await
            .status(),
        200
    );
    assert_eq!(f.files(), 1);
    assert_eq!(
        f.invoke("discard_upload", json!({"uploadId":a["uploadId"]}))
            .await
            .status(),
        404
    );
}

struct ControlledBody {
    rx: tokio::sync::mpsc::Receiver<Result<axum::body::Bytes, std::io::Error>>,
}
impl HttpBody for ControlledBody {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        self.rx.poll_recv(cx).map(|v| v.map(|r| r.map(Frame::data)))
    }
}
#[tokio::test]
async fn interrupted_stream_and_logout_during_upload_do_not_register_files() {
    for logout in [false, true] {
        let f = Fixture::new(true);
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let mut r = f.upload_request("a.obj", MODEL);
        *r.body_mut() = Body::new(ControlledBody { rx });
        let app = f.app.clone();
        let task = tokio::spawn(async move { router(app).oneshot(r).await.unwrap() });
        tx.send(Ok(axum::body::Bytes::copy_from_slice(MODEL)))
            .await
            .unwrap();
        if logout {
            assert_eq!(
                f.send(f.request("POST", "/api/logout", Body::empty()))
                    .await
                    .status(),
                200
            );
        } else {
            tx.send(Err(std::io::Error::other("interrupted")))
                .await
                .unwrap();
        }
        drop(tx);
        let r = task.await.unwrap();
        assert_eq!(r.status(), if logout { 401 } else { 400 });
        assert_eq!(f.files(), 0);
        assert!(f
            .app
            .uploads
            .as_ref()
            .unwrap()
            .store
            .list()
            .unwrap()
            .is_empty());
    }
}
#[tokio::test]
async fn sse_snapshot_headers_origin_and_per_session_limit() {
    let f = Fixture::new(true);
    let uploaded = f.upload("visible.obj", MODEL).await;
    let mut missing = f.request("GET", "/api/events", Body::empty());
    missing.headers_mut().remove("cookie");
    assert_eq!(f.send(missing).await.status(), 401);
    for (key, val) in [
        ("origin", "http://evil.test"),
        ("sec-fetch-site", "cross-site"),
    ] {
        let mut r = f.request("GET", "/api/events", Body::empty());
        r.headers_mut().insert(key, val.parse().unwrap());
        assert_eq!(f.send(r).await.status(), 403);
    }
    let mut responses = Vec::new();
    for _ in 0..4 {
        let r = f.send(f.request("GET", "/api/events", Body::empty())).await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["content-type"], "text/event-stream");
        assert_eq!(r.headers()["cache-control"], "no-store");
        assert_eq!(r.headers()["x-accel-buffering"], "no");
        responses.push(r);
    }
    assert_eq!(
        f.send(f.request("GET", "/api/events", Body::empty()))
            .await
            .status(),
        429
    );
    let mut body = responses.pop().unwrap().into_body();
    let bytes = body.frame().await.unwrap().unwrap().into_data().unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("\"snapshot\":true"));
    assert!(text.contains(uploaded["uploadId"].as_str().unwrap()));
    assert!(!text.contains(&f.dir.to_string_lossy().to_string()));
    drop(body);
    assert_eq!(
        f.send(f.request("GET", "/api/events", Body::empty()))
            .await
            .status(),
        200
    );
}
#[tokio::test]
async fn sse_over_plain_http_without_fetch_metadata_uses_origin_check() {
    let f = Fixture::new(true);
    // (drop Sec-Fetch-Site, Origin to send, Sec-Fetch-Site to send, status)
    let cases: [(bool, Option<&str>, Option<&str>, u16); 6] = [
        // plain HTTP via IP: no Sec-Fetch-*, no Origin on same-origin GET
        (true, None, None, 200),
        (true, Some("http://localhost:8080"), None, 200),
        (true, Some("http://evil.test"), None, 403),
        (true, Some("https://localhost:8080"), None, 403),
        (false, None, Some("same-site"), 403),
        (false, None, Some("cross-origin"), 403),
    ];
    for (drop_site, origin, site, expected) in cases {
        let mut r = f.request("GET", "/api/events", Body::empty());
        let h = r.headers_mut();
        h.remove("origin");
        if drop_site {
            h.remove("sec-fetch-site");
        }
        if let Some(o) = origin {
            h.insert("origin", o.parse().unwrap());
        }
        if let Some(s) = site {
            h.insert("sec-fetch-site", s.parse().unwrap());
        }
        let status = f.send(r).await.status();
        assert_eq!(status, expected, "origin={origin:?} site={site:?}");
    }
    let mut twice = f.request("GET", "/api/events", Body::empty());
    twice
        .headers_mut()
        .append("sec-fetch-site", "cross-site".parse().unwrap());
    assert_eq!(f.send(twice).await.status(), 403);
}
#[tokio::test(start_paused = true)]
async fn sse_does_not_refresh_session_and_closes_on_expiration() {
    let f = Fixture::new(true);
    f.time.0.store(200, Ordering::SeqCst);
    let r = f.send(f.request("GET", "/api/events", Body::empty())).await;
    assert_eq!(r.status(), 200);
    assert_eq!(
        f.app.data.lock().unwrap().sessions[&f.session.token].seen,
        100
    );
    let mut body = r.into_body();
    body.frame().await.unwrap().unwrap();
    f.time.0.store(100 + 2 * 60 * 60 + 1, Ordering::SeqCst);
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    assert!(body.frame().await.is_none());
}

async fn wait_result(f: &Fixture, job: &Value) -> Value {
    for _ in 0..100 {
        let r = f.invoke("get_import_result", json!({"jobId":job})).await;
        assert_eq!(r.status(), 200);
        let result = value(r).await;
        if matches!(
            result["state"].as_str(),
            Some("completed" | "failed" | "cancelled")
        ) {
            return result;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("import job did not complete");
}
#[tokio::test]
async fn import_duplicate_results_persist_files_stay_and_only_pending_can_be_discarded() {
    let f = Fixture::new(true);
    let first = f.upload("first.obj", MODEL).await;
    let start = f
        .invoke(
            "start_upload_import",
            json!({"uploadIds":[first["uploadId"]]}),
        )
        .await;
    assert_eq!(start.status(), 200);
    let job = value(start).await["jobId"].clone();
    let result = wait_result(&f, &job).await;
    assert_eq!(result["state"], "completed");
    assert_eq!(result["files"][0]["state"], "imported");
    assert_eq!(f.files(), 1);
    assert!(!result
        .to_string()
        .contains(&f.dir.to_string_lossy().to_string()));
    assert_eq!(
        f.invoke("discard_upload", json!({"uploadId":first["uploadId"]}))
            .await
            .status(),
        409
    );
    assert_eq!(
        f.invoke(
            "start_upload_import",
            json!({"uploadIds":[first["uploadId"]]})
        )
        .await
        .status(),
        409
    );
    let second = f.upload("second.obj", MODEL).await;
    let start = f
        .invoke(
            "start_upload_import",
            json!({"uploadIds":[second["uploadId"]]}),
        )
        .await;
    assert_eq!(start.status(), 200);
    let second_job = value(start).await["jobId"].clone();
    assert_ne!(job, second_job);
    for id in [&job, &second_job] {
        let id = id.as_str().unwrap();
        assert_eq!(id.len(), 32, "web job IDs encode 128 random bits");
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    let result = wait_result(&f, &second_job).await;
    assert_eq!(result["files"][0]["state"], "duplicate");
    assert_eq!(f.files(), 2);
    assert_eq!(
        f.invoke("discard_upload", json!({"uploadId":second["uploadId"]}))
            .await
            .status(),
        200
    );
    assert_eq!(f.files(), 1);
    // Results are readable from a newly opened journal after in-memory jobs retire.
    let restarted = uploads::initialize(&f.app.config).unwrap().unwrap();
    let persisted = restarted.store.job_results().unwrap();
    assert!(persisted.iter().any(|v| v["jobId"] == job));
}

#[tokio::test]
async fn sse_global_connection_cap_and_release_across_sessions() {
    let f = Fixture::new(true);
    let mut streams = Vec::new();
    for n in 0..8 {
        let session =
            new_session(&mut f.app.data.lock().unwrap(), &format!("client-{n}"), 100).unwrap();
        for _ in 0..4 {
            let mut request = f.request("GET", "/api/events", Body::empty());
            request.headers_mut().insert(
                "cookie",
                format!("katalog_session={}", session.token)
                    .parse()
                    .unwrap(),
            );
            let response = f.send(request).await;
            assert_eq!(response.status(), 200);
            streams.push(response);
        }
    }
    let rejected = f.send(f.request("GET", "/api/events", Body::empty())).await;
    assert_eq!(rejected.status(), 503);
    assert!(rejected.headers().contains_key("retry-after"));
    streams.pop();
    assert_eq!(
        f.send(f.request("GET", "/api/events", Body::empty()))
            .await
            .status(),
        200
    );
}

#[tokio::test(start_paused = true)]
async fn sse_slow_reader_disconnects_when_bounded_buffer_fills() {
    let f = Fixture::new(true);
    let response = f.send(f.request("GET", "/api/events", Body::empty())).await;
    assert_eq!(response.status(), 200);
    tokio::task::yield_now().await;
    for _ in 0..40 {
        tokio::time::advance(std::time::Duration::from_secs(15)).await;
        tokio::task::yield_now().await;
    }
    let mut body = response.into_body();
    let mut frames = 0;
    while let Some(frame) = body.frame().await {
        frame.unwrap();
        frames += 1;
        assert!(frames <= 32, "slow connection buffer grew without a bound");
    }
    assert!(frames > 0);
}

#[tokio::test]
async fn metadata_write_deadline_releases_admission_while_import_waits_on_database() {
    let f = Fixture::new(true);
    let upload = f.upload("waiting.obj", MODEL).await;
    let app = f.app.clone();
    let (acquired_tx, acquired_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let lock_task = tokio::task::spawn_blocking(move || {
        let _lock = app.writer.as_ref().unwrap().conn.lock().unwrap();
        acquired_tx.send(()).unwrap();
        let _ = release_rx.blocking_recv();
    });
    acquired_rx.await.unwrap();
    let start = f
        .invoke(
            "start_upload_import",
            json!({"uploadIds":[upload["uploadId"]]}),
        )
        .await;
    assert_eq!(start.status(), 200);
    let job = value(start).await["jobId"].clone();
    assert_eq!(
        f.invoke(
            "start_upload_import",
            json!({"uploadIds":[upload["uploadId"]]})
        )
        .await
        .status(),
        409
    );
    assert_eq!(
        f.invoke("discard_upload", json!({"uploadId":upload["uploadId"]}))
            .await
            .status(),
        409
    );
    let another = f.upload("another.obj", MODEL).await;
    assert_eq!(
        f.invoke(
            "start_upload_import",
            json!({"uploadIds":[another["uploadId"]]})
        )
        .await
        .status(),
        503
    );
    let state = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        f.invoke("get_import_state", json!({"jobId":job})),
    )
    .await
    .unwrap();
    assert_eq!(state.status(), 200);
    let began = std::time::Instant::now();
    let response = f
        .invoke("create_collection", json!({"name":"while importing"}))
        .await;
    let elapsed = began.elapsed();
    assert_eq!(response.status(), 503);
    assert!(response.headers().contains_key("retry-after"));
    // 250 ms scheduling tolerance around the specified five-second lock deadline.
    assert!(
        elapsed < std::time::Duration::from_millis(5250),
        "{elapsed:?}"
    );
    assert_eq!(
        f.app.writer.as_ref().unwrap().admission.available_permits(),
        33
    );
    assert_eq!(
        f.invoke("cancel_import", json!({"jobId":job}))
            .await
            .status(),
        200
    );
    release_tx.send(()).unwrap();
    lock_task.await.unwrap();
    let result = wait_result(&f, &job).await;
    assert_eq!(result["state"], "cancelled");
}

#[tokio::test]
async fn running_import_survives_session_invalidation_and_results_belong_to_instance() {
    let mut f = Fixture::new(true);
    let upload = f.upload("survives.obj", MODEL).await;
    let app = f.app.clone();
    let (acquired_tx, acquired_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let lock_task = tokio::task::spawn_blocking(move || {
        let _lock = app.writer.as_ref().unwrap().conn.lock().unwrap();
        acquired_tx.send(()).unwrap();
        let _ = release_rx.blocking_recv();
    });
    acquired_rx.await.unwrap();
    let start = f
        .invoke(
            "start_upload_import",
            json!({"uploadIds":[upload["uploadId"]]}),
        )
        .await;
    assert_eq!(start.status(), 200);
    let job = value(start).await["jobId"].clone();
    // Password changes invalidate sessions and increment the auth revision;
    // this directly exercises that state transition without hashing a password.
    {
        let mut data = f.app.data.lock().unwrap();
        data.sessions.clear();
        data.revision += 1;
    }
    assert_eq!(
        f.invoke("get_import_state", json!({"jobId":job}))
            .await
            .status(),
        401
    );
    f.session = new_session(&mut f.app.data.lock().unwrap(), "new-login", 100).unwrap();
    assert_eq!(
        f.invoke("get_import_state", json!({"jobId":job}))
            .await
            .status(),
        200
    );
    release_tx.send(()).unwrap();
    lock_task.await.unwrap();
    let result = wait_result(&f, &job).await;
    assert_eq!(result["state"], "completed");
    assert_eq!(result["files"][0]["state"], "imported");
    for command in ["get_import_state", "get_import_result", "cancel_import"] {
        assert_eq!(
            f.invoke(command, json!({"jobId":"00000000000000000000000000000000"}))
                .await
                .status(),
            404
        );
    }
    let ids = vec!["00000000000000000000000000000000"; 201];
    assert_eq!(
        f.invoke("start_upload_import", json!({"uploadIds":ids}))
            .await
            .status(),
        503
    );
}

struct ObservedBody {
    rx: tokio::sync::mpsc::Receiver<Result<axum::body::Bytes, std::io::Error>>,
    waiting: Arc<AtomicU64>,
}
impl HttpBody for ObservedBody {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.rx.poll_recv(cx) {
            Poll::Pending => {
                self.waiting.fetch_add(1, Ordering::SeqCst);
                Poll::Pending
            }
            Poll::Ready(frame) => Poll::Ready(frame.map(|r| r.map(Frame::data))),
        }
    }
}
async fn wait_for_body_poll(waiting: &AtomicU64, after: u64) {
    // Yield rather than sleeping: paused-time auto-advance would otherwise jump
    // to the upload deadline while its blocking file write is still scheduled.
    for _ in 0..100_000 {
        if waiting.load(Ordering::SeqCst) > after {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("upload did not request another body frame");
}

#[tokio::test]
async fn upload_small_drips_do_not_reset_sixty_second_idle_deadline() {
    let f = Fixture::new(true);
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    let waiting = Arc::new(AtomicU64::new(0));
    let mut request = f.upload_request("drip.obj", MODEL);
    request
        .headers_mut()
        .insert("content-length", "131072".parse().unwrap());
    *request.body_mut() = Body::new(ObservedBody {
        rx,
        waiting: waiting.clone(),
    });
    let app = f.app.clone();
    let task = tokio::spawn(async move { router(app).oneshot(request).await.unwrap() });
    wait_for_body_poll(&waiting, 0).await;
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(59)).await;
    assert!(
        !task.is_finished(),
        "generic fifteen-second deadline applied to upload"
    );
    let before = waiting.load(Ordering::SeqCst);
    tx.send(Ok(axum::body::Bytes::from(vec![b' '; 65_535])))
        .await
        .unwrap();
    wait_for_body_poll(&waiting, before).await;
    tokio::time::advance(std::time::Duration::from_secs(2)).await;
    let response = task.await.unwrap();
    assert_eq!(response.status(), 408);
    assert_eq!(f.files(), 0);
    assert!(f
        .app
        .uploads
        .as_ref()
        .unwrap()
        .store
        .list()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn upload_sixty_four_kib_progress_renews_idle_deadline_beyond_generic_timeout() {
    let f = Fixture::new(true);
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    let waiting = Arc::new(AtomicU64::new(0));
    let mut request = f.upload_request("progress.obj", MODEL);
    request
        .headers_mut()
        .insert("content-length", "131072".parse().unwrap());
    *request.body_mut() = Body::new(ObservedBody {
        rx,
        waiting: waiting.clone(),
    });
    let app = f.app.clone();
    let task = tokio::spawn(async move { router(app).oneshot(request).await.unwrap() });
    wait_for_body_poll(&waiting, 0).await;
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(59)).await;
    let mut first = vec![b' '; 65_536];
    first[..MODEL.len()].copy_from_slice(MODEL);
    let before = waiting.load(Ordering::SeqCst);
    tx.send(Ok(axum::body::Bytes::from(first))).await.unwrap();
    wait_for_body_poll(&waiting, before).await;
    tokio::time::advance(std::time::Duration::from_secs(59)).await;
    assert!(
        !task.is_finished(),
        "64 KiB progress did not renew upload idle deadline"
    );
    tx.send(Ok(axum::body::Bytes::from(vec![b' '; 65_536])))
        .await
        .unwrap();
    drop(tx);
    // Restore real time while final fsync/journal work finishes off-runtime.
    tokio::time::resume();
    let response = task.await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(value(response).await["size"], 131_072);
    assert_eq!(f.files(), 1);
}

#[tokio::test]
async fn committed_import_with_failed_journal_remains_failed_and_cannot_be_discarded() {
    let f = Fixture::new(true);
    let upload = f.upload("journal-failure.obj", MODEL).await;
    let app = f.app.clone();
    let (acquired_tx, acquired_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let lock_task = tokio::task::spawn_blocking(move || {
        let _lock = app.writer.as_ref().unwrap().conn.lock().unwrap();
        acquired_tx.send(()).unwrap();
        let _ = release_rx.blocking_recv();
    });
    acquired_rx.await.unwrap();
    let start = f
        .invoke(
            "start_upload_import",
            json!({"uploadIds":[upload["uploadId"]]}),
        )
        .await;
    assert_eq!(start.status(), 200);
    let job = value(start).await["jobId"].clone();
    // begin_import is durable before the response. Replacing its destination
    // with a directory forces atomic rename failure even when tests run as root.
    let journal = f.app.config.auth_file.with_file_name("web-uploads.json");
    let backup = journal.with_extension("backup");
    std::fs::rename(&journal, &backup).unwrap();
    std::fs::create_dir(&journal).unwrap();
    release_tx.send(()).unwrap();
    lock_task.await.unwrap();
    let result = wait_result(&f, &job).await;
    assert_eq!(result["state"], "failed");
    // Failure must remain in runtime: the old journal has no durable job result.
    for _ in 0..3 {
        let response = f.invoke("get_import_result", json!({"jobId":job})).await;
        assert_eq!(response.status(), 200);
        assert_eq!(value(response).await["state"], "failed");
        tokio::task::yield_now().await;
    }
    assert_eq!(
        f.invoke("discard_upload", json!({"uploadId":upload["uploadId"]}))
            .await
            .status(),
        409
    );
    let count: i64 = db::open_read_only(&f.app.config.db_path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1, "the catalog commit must survive journal failure");
    assert_eq!(f.files(), 1);
    let entries = f.app.uploads.as_ref().unwrap().store.list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(serde_json::to_value(entries[0].state).unwrap(), "importing");
    let retry = f
        .invoke("start_upload_import", json!({"uploadIds":["other"]}))
        .await;
    assert_eq!(retry.status(), 503);
    assert!(!retry.headers().contains_key("retry-after"));
    assert_eq!(value(retry).await["kind"], "restart_required");
    std::fs::remove_dir(&journal).unwrap();
    std::fs::rename(backup, journal).unwrap();
}

#[test]
fn upload_startup_resolves_relative_database_and_auth_locations() {
    let fixture=Fixture::new(true);
    let mut config=fixture.app.config.clone();
    let cwd=std::env::current_dir().unwrap();
    let mut root=std::path::PathBuf::new();
    for _ in cwd.components().skip(1){root.push("..");}
    config.db_path=root.join(config.db_path.strip_prefix("/").unwrap());
    config.auth_file=root.join(config.auth_file.strip_prefix("/").unwrap());
    assert!(uploads::initialize(&config).unwrap().is_some());
}

#[tokio::test]
async fn round3_status_and_cancel_do_not_consume_rate_tokens() {
    let f = Fixture::new(true);
    for _ in 0..100 {
        for command in ["get_import_state", "get_import_result", "cancel_import"] {
            assert_eq!(
                f.invoke(command, json!({"jobId":"unknown"})).await.status(),
                404
            );
        }
    }
    assert_eq!(
        f.send(f.upload_request("after.obj", MODEL)).await.status(),
        200
    );
}

#[tokio::test]
async fn round3_slow_file_sync_does_not_block_session_requests() {
    let f = Fixture::new(true);
    let store = &f.app.uploads.as_ref().unwrap().store;
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut entered_tx = Some(entered_tx);
    let hook = Mutex::new(move || {
        entered_tx.take().unwrap().send(()).unwrap();
        release_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
    });
    *store.sync_hook.lock().unwrap() = Some(Box::new(move || (hook.lock().unwrap())()));
    let app = f.app.clone();
    let req = f.upload_request("slow.obj", MODEL);
    let upload = tokio::spawn(async move { router(app).oneshot(req).await.unwrap() });
    entered_rx.await.unwrap();
    let start = std::time::Instant::now();
    let session = f
        .send(f.request("GET", "/api/session", Body::empty()))
        .await;
    let elapsed = start.elapsed();
    let _ = release_tx.send(());
    assert_eq!(session.status(), 200);
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "session waited {elapsed:?}"
    );
    assert_eq!(upload.await.unwrap().status(), 200);
}

#[tokio::test]
async fn round3_list_hides_imported_and_import_journal_is_batched() {
    let f = Fixture::new(true);
    let mut ids = Vec::new();
    for n in 0..200 {
        f.time.0.fetch_add(1, Ordering::SeqCst);
        let upload = f.upload(&format!("batch{n}.obj"), MODEL).await;
        ids.push(upload["uploadId"].clone());
    }
    let store = &f.app.uploads.as_ref().unwrap().store;
    let before = store.persist_count.load(Ordering::Relaxed);
    let r = f
        .invoke("start_upload_import", json!({"uploadIds":ids}))
        .await;
    assert_eq!(r.status(), 200);
    let result = wait_result(&f, &value(r).await["jobId"]).await;
    assert_eq!(result["files"].as_array().unwrap().len(), 200);
    assert!(store.persist_count.load(Ordering::Relaxed) - before <= 13);
    let list = value(
        f.send(f.request("GET", "/api/uploads", Body::empty()))
            .await,
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 199);
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["state"] != "imported"));
}

#[tokio::test]
async fn round3_store_waits_do_not_block_tokio_or_existing_sse() {
    let f = Fixture::new(true);
    let existing = f.send(f.request("GET", "/api/events", Body::empty())).await;
    assert_eq!(existing.status(), 200);
    let store = f.app.uploads.as_ref().unwrap().store.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let held = tokio::task::spawn_blocking(move || {
        store.with_test_lock(|| {
            ready_tx.send(()).unwrap();
            let _ = release_rx.recv_timeout(std::time::Duration::from_secs(3));
        })
    });
    ready_rx.await.unwrap();
    let app = f.app.clone();
    let req = f.request("GET", "/api/events", Body::empty());
    let event = tokio::spawn(async move { router(app).oneshot(req).await.unwrap() });
    let start = std::time::Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Dropping an existing stream needs the hub lock; a new snapshot must not
    // keep it locked while waiting for the store.
    drop(existing);
    let session = f
        .send(f.request("GET", "/api/session", Body::empty()))
        .await;
    let elapsed = start.elapsed();
    let _ = release_tx.send(());
    held.await.unwrap();
    assert_eq!(session.status(), 200);
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "runtime stalled {elapsed:?}"
    );
    assert_eq!(event.await.unwrap().status(), 200);
}

#[tokio::test]
async fn round3_live_catalog_path_conflicts_and_restart_hash_mismatch() {
    let f = Fixture::new(true);
    let upload = f.upload("occupied.obj", MODEL).await;
    let started = f
        .invoke(
            "start_upload_import",
            json!({"uploadIds":[upload["uploadId"]]}),
        )
        .await;
    let result = wait_result(&f, &value(started).await["jobId"]).await;
    assert_eq!(result["files"][0]["state"], "imported");
    // A row created since startup must reserve its path even after the file disappears.
    std::fs::remove_file(f.dir.join("models/uploads/occupied.obj")).unwrap();
    let next = f.upload("occupied.obj", MODEL).await;
    assert_eq!(next["name"], "occupied (2).obj");
    // Restore the original inode's contents in place for a hash-only mismatch.
    let path = f.dir.join("models/uploads/occupied (2).obj");
    let next_started = f
        .invoke(
            "start_upload_import",
            json!({"uploadIds":[next["uploadId"]]}),
        )
        .await;
    let _ = wait_result(&f, &value(next_started).await["jobId"]).await;
    let conn = db::open_read_write(&f.app.config.db_path).unwrap();
    conn.execute(
        "UPDATE files SET path=?1,content_hash='wrong'",
        [path.to_string_lossy().as_ref()],
    )
    .unwrap();
    let reopened = uploads::initialize(&f.app.config).unwrap().unwrap();
    let entry = reopened
        .store
        .list()
        .unwrap()
        .into_iter()
        .find(|e| e.id == next["uploadId"].as_str().unwrap())
        .unwrap();
    assert_ne!(entry.state, crate::upload_store::UploadState::Imported);
}

#[tokio::test]
async fn round3_cancelled_http_keeps_final_registration_serialized_with_logout() {
    let f = Fixture::new(true);
    let store = f.app.uploads.as_ref().unwrap().store.clone();
    let (sync_tx, sync_rx) = tokio::sync::oneshot::channel();
    let (continue_tx, continue_rx) = std::sync::mpsc::channel();
    let sync_tx = Mutex::new(Some(sync_tx));
    let continue_rx = Mutex::new(continue_rx);
    *store.sync_hook.lock().unwrap() = Some(Box::new(move || {
        sync_tx.lock().unwrap().take().unwrap().send(()).unwrap();
        let _ = continue_rx
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(3));
    }));
    let app = f.app.clone();
    let req = f.upload_request("cancel-final.obj", MODEL);
    let request = tokio::spawn(async move { router(app).oneshot(req).await.unwrap() });
    sync_rx.await.unwrap();
    let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let locked = tokio::task::spawn_blocking(move || {
        store.with_test_lock(|| {
            locked_tx.send(()).unwrap();
            let _ = release_rx.recv_timeout(std::time::Duration::from_secs(3));
        })
    });
    locked_rx.await.unwrap();
    continue_tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while f.app.changes.try_lock().is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    request.abort();
    let _ = request.await;
    let serialized = f.app.changes.try_lock().is_err();
    release_tx.send(()).unwrap();
    locked.await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while f.app.changes.try_lock().is_err() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        serialized,
        "cancelling the HTTP future released the logout gate during registration"
    );
}

#[tokio::test]
async fn catalog_removal_reopens_upload_then_reimports_and_discards_only_owned_file() {
    let f=Fixture::new(true);
    let foreign=f.dir.join("models/foreign.obj");
    std::fs::write(&foreign,MODEL).unwrap();
    let upload=f.upload("remove.obj",MODEL).await;
    for _ in 0..2 {
        let start=f.invoke("start_upload_import",json!({"uploadIds":[upload["uploadId"]]})).await;
        assert_eq!(start.status(),200);
        let job=value(start).await;
        let result=wait_result(&f,&job["jobId"]).await;
        assert_eq!(result["files"][0]["state"],"imported");
        let r=f.invoke("remove_files_from_catalog",json!({"fileIds":[result["files"][0]["fileId"]]})).await;
        assert_eq!(r.status(),200);
        let entries=f.app.uploads.as_ref().unwrap().store.pending_list().unwrap();
        assert_eq!(entries.len(),1);
        assert_eq!(entries[0].state,crate::upload_store::UploadState::Uploaded);
        assert!(entries[0].result.is_none());
        assert_eq!(std::fs::read(f.dir.join("models/uploads/remove.obj")).unwrap(),MODEL);
    }
    assert_eq!(f.invoke("discard_upload",json!({"uploadId":upload["uploadId"]})).await.status(),200);
    assert!(!f.dir.join("models/uploads/remove.obj").exists());
    assert_eq!(std::fs::read(foreign).unwrap(),MODEL);
}

#[tokio::test]
async fn catalog_removal_rejects_importing_journal_entry() {
    let f=Fixture::new(true);
    let upload=f.upload("busy.obj",MODEL).await;
    let store=&f.app.uploads.as_ref().unwrap().store;
    store.begin_import(&[upload["uploadId"].as_str().unwrap().to_owned()],"busy").unwrap();
    let r=f.invoke("remove_files_from_catalog",json!({"fileIds":[]})).await;
    assert_eq!(r.status(),503);
    assert_eq!(r.headers()["retry-after"],"1");
}

#[tokio::test]
async fn catalog_removal_rejects_active_import_permit_even_without_importing_entries() {
    let f=Fixture::new(true);
    let rt=f.app.uploads.as_ref().unwrap();
    let _permit=rt.active.clone().try_acquire_owned().unwrap();
    let r=f.invoke("remove_files_from_catalog",json!({"fileIds":[]})).await;
    assert_eq!(r.status(),503);
    assert_eq!(r.headers()["retry-after"],"1");
}

#[tokio::test]
async fn catalog_removal_committed_with_journal_failure_is_healed_on_restart() {
    let f=Fixture::new(true);
    let upload=f.upload("recovery.obj",MODEL).await;
    let job=value(f.invoke("start_upload_import",json!({"uploadIds":[upload["uploadId"]]})).await).await;
    let result=wait_result(&f,&job["jobId"]).await;
    let journal=f.app.config.auth_file.parent().unwrap().join("web-uploads.json");
    let backup=journal.with_extension("backup");
    std::fs::rename(&journal,&backup).unwrap();
    std::fs::create_dir(&journal).unwrap();
    let r=f.invoke("remove_files_from_catalog",json!({"fileIds":[result["files"][0]["fileId"]]})).await;
    assert_eq!(r.status(),200);
    assert_eq!(f.app.uploads.as_ref().unwrap().store.ensure_importable().unwrap_err().kind,"restart_required");
    std::fs::remove_dir(&journal).unwrap();
    std::fs::rename(backup,journal).unwrap();
    let recovered=uploads::initialize(&f.app.config).unwrap().unwrap();
    uploads::reconcile(&f.app.config,Some(&recovered)).unwrap();
    let entries=recovered.store.pending_list().unwrap();
    assert_eq!(entries.len(),1);
    assert_eq!(entries[0].state,crate::upload_store::UploadState::Uploaded);
    assert!(entries[0].result.is_none());
    assert_eq!(std::fs::read(f.dir.join("models/uploads/recovery.obj")).unwrap(),MODEL);
}

async fn import_upload(f: &Fixture, name: &str, bytes: &[u8]) -> Value {
    let upload = f.upload(name, bytes).await;
    let start = f
        .invoke("start_upload_import", json!({"uploadIds":[upload["uploadId"]]}))
        .await;
    assert_eq!(start.status(), 200);
    let job = value(start).await["jobId"].clone();
    let result = wait_result(f, &job).await;
    assert_eq!(result["files"][0]["state"], "imported", "{result}");
    result
}
async fn tag_names(f: &Fixture) -> Vec<String> {
    let r = f.invoke("list_all_file_tags", json!({})).await;
    assert_eq!(r.status(), 200);
    value(r).await.as_object().unwrap().values().flat_map(|tags| tags.as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_owned())).collect()
}

#[tokio::test]
async fn upload_import_assigns_automatic_tags_only_while_the_switch_is_on() {
    let f = Fixture::new(true);
    let on = |s: &str| s == "kabelhalter";
    // Default: the switch is on and the file name yields an automatic tag.
    let r = f.invoke("get_auto_tagging", json!({})).await;
    assert_eq!(value(r).await, json!(true));
    import_upload(&f, "kabelhalter.obj", MODEL).await;
    assert!(tag_names(&f).await.iter().any(|t| on(t)), "automatic tag expected while on");
    // Switched off through the web command: the next upload gets no automatic tags,
    // the tag that already exists stays.
    let r = f.invoke("set_auto_tagging", json!({"enabled":false})).await;
    assert_eq!(r.status(), 200);
    let r = f.invoke("get_auto_tagging", json!({})).await;
    assert_eq!(value(r).await, json!(false));
    import_upload(&f, "schraubenschluessel.obj", b"v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n").await;
    let tags = tag_names(&f).await;
    assert!(tags.iter().any(|t| on(t)), "existing tags are never removed");
    assert!(!tags.iter().any(|t| t == "schraubenschluessel"), "no automatic tag while off: {tags:?}");
    // Back on: automatic tags again.
    assert_eq!(f.invoke("set_auto_tagging", json!({"enabled":true})).await.status(), 200);
    import_upload(&f, "werkzeughalter.obj", b"v 0 0 0\nv 3 0 0\nv 0 3 0\nf 1 2 3\n").await;
    assert!(tag_names(&f).await.iter().any(|t| t == "werkzeughalter"));
    // Strict argument: anything but a boolean is refused and changes nothing.
    assert_eq!(f.invoke("set_auto_tagging", json!({"enabled":"0"})).await.status(), 400);
    assert_eq!(f.invoke("set_auto_tagging", json!({})).await.status(), 400);
    let r = f.invoke("get_auto_tagging", json!({})).await;
    assert_eq!(value(r).await, json!(true));
}

// ---------- Archive uploads (zip / 7z) ----------

fn obj(n: u32) -> Vec<u8> {
    format!("v 0 0 {n}\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").into_bytes()
}
fn zip_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, data) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
/// One solid block, like the 7-Zip default.
fn sevenz_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).unwrap();
    let headers = entries.iter().map(|(n, _)| sevenz_rust2::ArchiveEntry::new_file(n)).collect();
    let readers = entries
        .iter()
        .map(|(_, d)| sevenz_rust2::SourceReader::new(std::io::Cursor::new(d.clone())))
        .collect();
    writer.push_archive_entries(headers, readers).unwrap();
    writer.finish().unwrap().into_inner()
}
fn upload_names(f: &Fixture) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(f.dir.join("models/uploads"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
async fn run_archive(f: &Fixture, name: &str, bytes: &[u8]) -> (Value, Value) {
    let upload = f.upload(name, bytes).await;
    let start = f
        .invoke("start_upload_import", json!({"uploadIds":[upload["uploadId"]]}))
        .await;
    assert_eq!(start.status(), 200);
    let job = value(start).await["jobId"].clone();
    let result = wait_result(f, &job).await;
    (upload, result)
}
fn archive_row(result: &Value) -> &Value {
    result["files"].as_array().unwrap().iter().find(|file| file["kind"] == "archive").expect("archive row")
}
fn states(result: &Value, state: &str) -> usize {
    result["files"].as_array().unwrap().iter().filter(|file| file["state"] == state && file["kind"] != "archive").count()
}

#[tokio::test]
async fn zip_with_two_models_and_a_picture_imports_two_and_reports_one_skipped() {
    let f = Fixture::new(true);
    let bytes = zip_bytes(&[("Modell/a.obj", obj(1)), ("Modell/b.obj", obj(2)), ("Modell/vorschau.png", b"\x89PNG....".to_vec())]);
    let (_, result) = run_archive(&f, "paket.zip", &bytes).await;
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(archive_row(&result)["models"], 2);
    assert_eq!(archive_row(&result)["skipped"], 1);
    assert_eq!(states(&result, "imported"), 2);
    // The archive stays; the models are ordinary files next to it. No staging folder is left.
    assert_eq!(upload_names(&f), ["a.obj", "b.obj", "paket.zip"]);
    assert!(!result.to_string().contains(&f.dir.to_string_lossy().to_string()));
    let mode = std::fs::metadata(f.dir.join("models/uploads/a.obj")).unwrap();
    assert_eq!(std::os::unix::fs::PermissionsExt::mode(&mode.permissions()) & 0o777, 0o644);
    // The archive is no longer importable but stays listed (and discardable) as handled.
    let pending = f.app.uploads.as_ref().unwrap().store.pending_list().unwrap();
    assert_eq!(pending.len(), 1, "{pending:?}");
    assert!(pending[0].archive && pending[0].state == crate::upload_store::UploadState::Imported);
}

#[tokio::test]
async fn solid_7z_with_two_models_and_a_picture_imports_two_and_reports_one_skipped() {
    let f = Fixture::new(true);
    // The picture sits between the models inside one compressed block.
    let bytes = sevenz_bytes(&[("a.obj", obj(1)), ("bild.png", b"\x89PNG....".to_vec()), ("b.obj", obj(2))]);
    let (_, result) = run_archive(&f, "paket.7z", &bytes).await;
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(archive_row(&result)["models"], 2);
    assert_eq!(archive_row(&result)["skipped"], 1);
    assert_eq!(states(&result, "imported"), 2, "{result}");
    assert_eq!(std::fs::read(f.dir.join("models/uploads/b.obj")).unwrap(), obj(2));
}

#[tokio::test]
async fn archive_names_other_than_zip_and_7z_are_refused_with_their_own_code() {
    let f = Fixture::new(true);
    for name in ["x.rar", "x.tar", "x.tar.gz", "x.tgz"] {
        let r = f.send(f.upload_request(name, b"Rar!\x1a\x07\x00")).await;
        assert_eq!(r.status(), 415, "{name}");
        assert_eq!(value(r).await["kind"], "archive_format_unsupported", "{name}");
    }
    assert!(upload_names(&f).is_empty());
}

#[tokio::test]
async fn archive_extension_needs_the_matching_signature() {
    let f = Fixture::new(true);
    let zip = zip_bytes(&[("a.obj", obj(1))]);
    let seven = sevenz_bytes(&[("a.obj", obj(1))]);
    for (name, bytes) in [("text.zip", b"hello, not an archive".to_vec()), ("nicht.7z", zip.clone()), ("nicht.zip", seven), ("exe.zip", b"MZ\x90\x00".to_vec())] {
        let r = f.send(f.upload_request(name, &bytes)).await;
        assert_eq!(r.status(), 400, "{name}");
        assert_eq!(value(r).await["kind"], "invalid_signature");
    }
    // An empty zip is just the end-of-central-directory record.
    let empty = zip_bytes(&[]);
    assert_eq!(f.send(f.upload_request("leer.zip", &empty)).await.status(), 200);
    assert_eq!(upload_names(&f), ["leer.zip"]);
}

#[tokio::test]
async fn hostile_paths_never_write_outside_the_upload_folder() {
    let f = Fixture::new(true);
    let long_ok = format!("{}tief.obj", "d/".repeat(150));
    let too_deep = format!("{}zu_tief.obj", "dd/".repeat(3000));
    let bytes = zip_bytes(&[
        ("../evil.obj", obj(10)),
        ("..\\evil2.obj", obj(11)),
        ("/etc/abs.obj", obj(12)),
        ("C:/drive.obj", obj(13)),
        ("ok/CON.obj", obj(14)),
        ("ok/ctl\u{7}name.obj", obj(15)),
        ("A/gleich.obj", obj(16)),
        ("a/Gleich.obj", obj(17)),
        ("tools/run.exe", b"MZ".to_vec()),
        ("tools/install.sh", b"#!/bin/sh".to_vec()),
        (long_ok.as_str(), obj(18)),
    ]);
    let (_, result) = run_archive(&f, "feindlich.zip", &bytes).await;
    assert_eq!(result["state"], "completed", "{result}");
    // 4 traversal/absolute entries and 2 blocked files are skipped; the other 5 are models.
    assert_eq!(archive_row(&result)["models"], 5, "{result}");
    assert_eq!(archive_row(&result)["skipped"], 6, "{result}");
    assert!(!f.dir.join("models/evil.obj").exists());
    assert!(!f.dir.join("evil.obj").exists());
    assert!(!f.dir.join("models/evil2.obj").exists());
    let names = upload_names(&f);
    assert!(names.contains(&"_CON.obj".to_owned()), "{names:?}");
    assert!(names.contains(&"ctl_name.obj".to_owned()), "{names:?}");
    assert!(names.contains(&"tief.obj".to_owned()), "{names:?}");
    // Names that differ only in case never collide on a case-insensitive volume.
    assert!(names.contains(&"gleich.obj".to_owned()) && names.contains(&"Gleich (2).obj".to_owned()) || names.contains(&"Gleich.obj".to_owned()) && names.contains(&"gleich (2).obj".to_owned()), "{names:?}");
    assert!(names.iter().all(|n| !n.starts_with(".archive-staging-") && !n.ends_with(".exe") && !n.ends_with(".sh")), "{names:?}");
    // A path no file system accepts makes the whole archive unusable, without residue.
    let f = Fixture::new(true);
    let (_, result) = run_archive(&f, "tief.zip", &zip_bytes(&[("x.obj", obj(1)), (too_deep.as_str(), obj(2))])).await;
    assert_eq!(result["files"][0]["state"], "invalid", "{result}");
    assert_eq!(result["files"][0]["kind"], "archive_unreadable");
    assert_eq!(upload_names(&f), ["tief.zip"]);
}

#[tokio::test]
async fn symlink_entries_are_skipped() {
    use std::io::Write;
    let f = Fixture::new(true);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    zip.add_symlink("passwd.obj", "/etc/passwd", options).unwrap();
    zip.start_file("echt.obj", options).unwrap();
    zip.write_all(&obj(1)).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let (_, result) = run_archive(&f, "link.zip", &bytes).await;
    assert_eq!(archive_row(&result)["models"], 1, "{result}");
    assert_eq!(upload_names(&f), ["echt.obj", "link.zip"]);
}

#[tokio::test]
async fn archives_beyond_the_entry_or_size_limits_are_refused_before_unpacking() {
    // More than 10,000 entries.
    let f = Fixture::new(true);
    let many: Vec<(String, Vec<u8>)> = (0..=10_000).map(|i| (format!("n{i}.txt"), Vec::new())).collect();
    let refs: Vec<(&str, Vec<u8>)> = many.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
    let bytes = zip_bytes(&refs);
    assert!(bytes.len() < 1024 * 1024);
    let (_, result) = run_archive(&f, "viele.zip", &bytes).await;
    assert_eq!(result["files"][0]["state"], "invalid", "{result}");
    assert_eq!(result["files"][0]["kind"], "archive_too_large");
    assert_eq!(upload_names(&f), ["viele.zip"]);
    // A small archive that claims 3 MiB while the limit is 1 MiB (zip bomb shape).
    let f = Fixture::with_env(true, &[("KATALOG_ARCHIVE_MAX_MB", "1")]);
    let bomb = zip_bytes(&[("a.obj", obj(1)), ("nullen.bin", vec![0u8; 3 * 1024 * 1024])]);
    assert!(bomb.len() < 100 * 1024, "compresses to almost nothing");
    let (_, result) = run_archive(&f, "bombe.zip", &bomb).await;
    assert_eq!(result["files"][0]["kind"], "archive_too_large", "{result}");
    assert_eq!(upload_names(&f), ["bombe.zip"]);
}

#[tokio::test]
async fn encrypted_and_empty_archives_fail_with_their_own_codes() {
    use std::io::Write;
    let f = Fixture::new(true);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "pw");
    zip.start_file("geheim.obj", options).unwrap();
    zip.write_all(&obj(1)).unwrap();
    let (upload, result) = run_archive(&f, "geheim.zip", &zip.finish().unwrap().into_inner()).await;
    assert_eq!(result["files"][0]["kind"], "archive_encrypted", "{result}");
    // The failed archive stays an open, discardable upload.
    assert_eq!(f.invoke("discard_upload", json!({"uploadId":upload["uploadId"]})).await.status(), 200);
    assert!(upload_names(&f).is_empty());

    let (_, result) = run_archive(&f, "nur-bilder.zip", &zip_bytes(&[("a.png", b"x".to_vec())])).await;
    assert_eq!(result["files"][0]["kind"], "archive_no_models", "{result}");
}

#[tokio::test]
async fn a_name_conflict_never_overwrites_or_merges() {
    let f = Fixture::new(true);
    import_upload(&f, "a.obj", &obj(1)).await;
    let existing = std::fs::read(f.dir.join("models/uploads/a.obj")).unwrap();
    // A folder with the archive's name exists already and must stay untouched.
    std::fs::create_dir(f.dir.join("models/uploads/paket")).unwrap();
    std::fs::write(f.dir.join("models/uploads/paket/keep.txt"), b"mein").unwrap();
    let bytes = zip_bytes(&[("x/a.obj", obj(2)), ("y/a.obj", obj(3))]);
    let (_, result) = run_archive(&f, "paket.zip", &bytes).await;
    assert_eq!(states(&result, "imported"), 2, "{result}");
    let names = upload_names(&f);
    for expected in ["a (2).obj", "a (3).obj", "paket", "paket.zip"] {
        assert!(names.contains(&expected.to_owned()), "{expected} in {names:?}");
    }
    assert_eq!(std::fs::read(f.dir.join("models/uploads/a.obj")).unwrap(), existing);
    assert_eq!(std::fs::read(f.dir.join("models/uploads/paket/keep.txt")).unwrap(), b"mein");
}

#[tokio::test]
async fn repeating_the_import_start_is_refused_and_leaves_nothing_behind() {
    let f = Fixture::new(true);
    let (upload, result) = run_archive(&f, "paket.zip", &zip_bytes(&[("a.obj", obj(1))])).await;
    assert_eq!(archive_row(&result)["models"], 1);
    let before = upload_names(&f);
    for _ in 0..2 {
        let again = f.invoke("start_upload_import", json!({"uploadIds":[upload["uploadId"]]})).await;
        assert_eq!(again.status(), 409);
    }
    assert_eq!(upload_names(&f), before);
}

#[tokio::test]
async fn an_archive_and_a_plain_upload_can_share_one_job() {
    let f = Fixture::new(true);
    let archive = f.upload("paket.zip", &zip_bytes(&[("a.obj", obj(1))])).await;
    let single = f.upload("einzeln.obj", &obj(2)).await;
    let job = value(f.invoke("start_upload_import", json!({"uploadIds":[archive["uploadId"], single["uploadId"]]})).await).await;
    let result = wait_result(&f, &job["jobId"]).await;
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(states(&result, "imported"), 2);
    assert_eq!(result["total"], 3);
    assert_eq!(result["done"], 3);
}

#[tokio::test]
async fn cancelling_after_unpacking_removes_the_staging_folder_and_reopens_the_archive() {
    let f = Fixture::new(true);
    let upload = f.upload("paket.zip", &zip_bytes(&[("a.obj", obj(1))])).await;
    let rt = f.app.uploads.as_ref().unwrap();
    let id = upload["uploadId"].as_str().unwrap().to_owned();
    let entry = rt.store.begin_import(std::slice::from_ref(&id), "job").unwrap().remove(0);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let upload_cfg = f.app.config.upload.as_ref().unwrap();
    let context = crate::upload_archive::Context { store: &rt.store, upload: upload_cfg, db_path: &f.app.config.db_path, cancel: &cancel, job_id: "job", timeout: crate::upload_archive::ARCHIVE_TIMEOUT };
    let seen_staging = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let probe = seen_staging.clone();
    let raise = cancel.clone();
    let dir = f.dir.join("models/uploads");
    crate::upload_archive::AFTER_EXTRACT.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            let staged = std::fs::read_dir(&dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".archive-staging-"));
            probe.store(staged, std::sync::atomic::Ordering::SeqCst);
            raise.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
    });
    let outcome = crate::upload_archive::expand(&context, &entry, &|_, _| json!({}));
    crate::upload_archive::AFTER_EXTRACT.with(|hook| *hook.borrow_mut() = None);
    assert!(matches!(outcome, Err(crate::upload_archive::Failure::Cancelled)));
    assert!(seen_staging.load(std::sync::atomic::Ordering::SeqCst), "the models were unpacked into staging");
    assert_eq!(upload_names(&f), ["paket.zip"]);
    rt.store.reset_importing("job").unwrap();
    assert_eq!(rt.store.pending_list().unwrap()[0].state, crate::upload_store::UploadState::Uploaded);
}

#[tokio::test]
async fn after_a_restart_an_unpacked_archive_stays_handled_and_its_models_are_open_uploads() {
    let f = Fixture::new(true);
    let rt = f.app.uploads.as_ref().unwrap();
    let upload = f.upload("paket.zip", &zip_bytes(&[("a.obj", obj(1)), ("b.obj", obj(2))])).await;
    let entry = rt.store.begin_import(&[upload["uploadId"].as_str().unwrap().to_owned()], "job").unwrap().remove(0);
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let context = crate::upload_archive::Context { store: &rt.store, upload: f.app.config.upload.as_ref().unwrap(), db_path: &f.app.config.db_path, cancel: &cancel, job_id: "job", timeout: crate::upload_archive::ARCHIVE_TIMEOUT };
    let done = crate::upload_archive::expand(&context, &entry, &|_, _| json!({"state":"imported","kind":"archive"})).unwrap();
    assert_eq!(done.models, 2);
    // The process dies here: the models are journalled as importing, no catalog row exists.
    let restarted = uploads::initialize(&f.app.config).unwrap().unwrap();
    uploads::reconcile(&f.app.config, Some(&restarted)).unwrap();
    let pending = restarted.store.pending_list().unwrap();
    // Two open models plus the handled archive.
    assert_eq!(pending.len(), 3, "{pending:?}");
    assert_eq!(pending.iter().filter(|e| e.state == crate::upload_store::UploadState::Uploaded).count(), 2);
    let all = restarted.store.list().unwrap();
    let archive = all.iter().find(|e| e.name == "paket.zip").unwrap();
    assert_eq!(archive.state, crate::upload_store::UploadState::Imported);
    assert!(archive.archive);
}

// ---------- Archive failures, cancellation, time limit and leftovers ----------

fn set_hook(name: &str, hook: crate::upload_archive::Hook) {
    let mut hooks = crate::upload_archive::HOOKS.lock().unwrap();
    hooks.retain(|(n, _)| n != name);
    hooks.push((name.to_owned(), hook));
}
fn clear_hook(name: &str) {
    crate::upload_archive::HOOKS.lock().unwrap().retain(|(n, _)| n != name);
}
async fn wait_long(f: &Fixture, job: &Value) -> Value {
    for _ in 0..1000 {
        let result = value(f.invoke("get_import_result", json!({"jobId":job})).await).await;
        if matches!(result["state"].as_str(), Some("completed" | "failed" | "cancelled")) {
            return result;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("import job did not complete");
}
async fn open_uploads(f: &Fixture) -> Value {
    value(f.send(f.request("GET", "/api/uploads", Body::empty())).await).await
}

#[tokio::test]
async fn more_models_than_the_open_upload_limit_get_their_own_code() {
    let f = Fixture::new(true);
    let many: Vec<(String, Vec<u8>)> = (0..201).map(|i| (format!("m{i}.obj"), obj(i))).collect();
    let refs: Vec<(&str, Vec<u8>)> = many.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
    let (_, result) = run_archive(&f, "zuviele.zip", &zip_bytes(&refs)).await;
    assert_eq!(result["files"][0]["state"], "invalid", "{result}");
    assert_eq!(result["files"][0]["kind"], "archive_too_many_models");
    assert_eq!(upload_names(&f), ["zuviele.zip"]);
}

#[tokio::test]
async fn a_transient_store_failure_hands_the_archive_back_and_the_import_can_be_repeated() {
    let f = Fixture::new(true);
    let name = "knapp.zip";
    set_hook(name, crate::upload_archive::Hook { fail_before_adopt: Some(crate::upload_store::StoreError::new(507, "no_space")), ..Default::default() });
    let (upload, result) = run_archive(&f, name, &zip_bytes(&[("a.obj", obj(1))])).await;
    assert_eq!(result["files"][0]["state"], "uploaded", "{result}");
    assert_eq!(result["files"][0]["kind"], "no_space");
    assert_eq!(upload_names(&f), [name], "staging removed, no model left");
    assert_eq!(open_uploads(&f).await[0]["state"], "uploaded");
    clear_hook(name);
    let start = f.invoke("start_upload_import", json!({"uploadIds":[upload["uploadId"]]})).await;
    assert_eq!(start.status(), 200);
    let job = value(start).await["jobId"].clone();
    let again = wait_long(&f, &job).await;
    assert_eq!(archive_row(&again)["models"], 1, "{again}");
}

#[tokio::test]
async fn a_checksum_error_while_unpacking_means_a_damaged_archive() {
    use std::io::Write;
    let f = Fixture::new(true);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("a.obj", options).unwrap();
    zip.write_all(&obj(1)).unwrap();
    let mut bytes = zip.finish().unwrap().into_inner();
    let at = bytes.windows(7).position(|w| w == b"v 1 0 0").unwrap();
    bytes[at] = b'w';
    let (_, result) = run_archive(&f, "kaputt.zip", &bytes).await;
    assert_eq!(result["files"][0]["state"], "invalid", "{result}");
    assert_eq!(result["files"][0]["kind"], "archive_unreadable");
    assert_eq!(upload_names(&f), ["kaputt.zip"]);
}

#[tokio::test]
async fn unpacking_that_takes_too_long_is_stopped_and_cleaned_up() {
    let f = Fixture::new(true);
    let name = "langsam.zip";
    set_hook(name, crate::upload_archive::Hook { delay_per_entry: Some(std::time::Duration::from_millis(300)), timeout: Some(std::time::Duration::from_millis(500)), ..Default::default() });
    let bytes = zip_bytes(&[("a.obj", obj(1)), ("b.obj", obj(2)), ("c.obj", obj(3)), ("d.obj", obj(4))]);
    let (_, result) = run_archive(&f, name, &bytes).await;
    clear_hook(name);
    assert_eq!(result["files"][0]["kind"], "archive_timeout", "{result}");
    assert_eq!(upload_names(&f), [name]);
}

#[tokio::test]
async fn cancelling_over_http_in_the_middle_of_unpacking_leaves_nothing_behind() {
    let f = Fixture::new(true);
    let name = "abbruch.zip";
    set_hook(name, crate::upload_archive::Hook { delay_per_entry: Some(std::time::Duration::from_millis(400)), ..Default::default() });
    let bytes = zip_bytes(&[("a.obj", obj(1)), ("b.obj", obj(2)), ("c.obj", obj(3)), ("d.obj", obj(4)), ("e.obj", obj(5))]);
    let upload = f.upload(name, &bytes).await;
    let job = value(f.invoke("start_upload_import", json!({"uploadIds":[upload["uploadId"]]})).await).await["jobId"].clone();
    // Wait until the staging folder shows that unpacking is under way.
    for _ in 0..500 {
        if upload_names(&f).iter().any(|n| n.starts_with(".archive-staging-")) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(upload_names(&f).iter().any(|n| n.starts_with(".archive-staging-")), "unpacking started");
    assert_eq!(f.invoke("cancel_import", json!({"jobId":job})).await.status(), 200);
    let result = wait_long(&f, &job).await;
    clear_hook(name);
    assert_eq!(result["state"], "cancelled", "{result}");
    assert_eq!(upload_names(&f), [name], "staging removed, no model linked");
    assert_eq!(open_uploads(&f).await[0]["state"], "uploaded");
    // And the archive can be imported afterwards.
    let (_, again) = run_archive_again(&f, &upload).await;
    assert_eq!(archive_row(&again)["models"], 5, "{again}");
}
async fn run_archive_again(f: &Fixture, upload: &Value) -> ((), Value) {
    let job = value(f.invoke("start_upload_import", json!({"uploadIds":[upload["uploadId"]]})).await).await["jobId"].clone();
    ((), wait_long(f, &job).await)
}

#[tokio::test]
async fn a_handled_archive_stays_listed_and_can_be_discarded_without_touching_its_models() {
    let f = Fixture::new(true);
    let (upload, result) = run_archive(&f, "paket.zip", &zip_bytes(&[("a.obj", obj(1))])).await;
    assert_eq!(states(&result, "imported"), 1);
    let listed = open_uploads(&f).await;
    let row = listed.as_array().unwrap().iter().find(|e| e["name"] == "paket.zip").expect("archive listed");
    assert_eq!(row["state"], "imported");
    assert_eq!(row["archive"], true);
    // It cannot be imported again, but it can be discarded.
    assert_eq!(f.invoke("start_upload_import", json!({"uploadIds":[upload["uploadId"]]})).await.status(), 409);
    assert_eq!(f.invoke("discard_upload", json!({"uploadId":upload["uploadId"]})).await.status(), 200);
    assert_eq!(upload_names(&f), ["a.obj"]);
    let listed = open_uploads(&f).await;
    assert!(listed.as_array().unwrap().iter().all(|e| e["name"] != "paket.zip"));
    // The imported model keeps working in the catalog.
    let file_id = result["files"].as_array().unwrap().iter().find(|x| x["name"] == "a.obj").unwrap()["fileId"].clone();
    assert!(file_id.is_string());
}
