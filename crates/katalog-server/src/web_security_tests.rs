use super::*;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::Duration,
};

const PASSWORD: &str = "Test-auth-security-42";
struct FixedTime;
impl Clock for FixedTime {
    fn now(&self) -> u64 {
        100
    }
}

#[tokio::test]
async fn cancelled_login_counts_failure_change_admission_and_logout_recheck() {
    let _serial = password::PASSWORD_TEST_LOCK.lock().await;
    let auth = AuthFile {
        version: 1,
        password_hash: password::hash_password(PASSWORD.into()).await.unwrap(),
        must_change: false,
        updated_at: 100,
    };
    let app = Arc::new(App {
            fileops: None,
            fileops_safety: Mutex::new(crate::fileops_safety::FileOpsSafety::new(2000)),
            uploads:None,
        writer: None,
        config: Config::from_lookup(|key| match key {
            "KATALOG_DB" => Some("/unused-security-test/catalog.db".into()),
            "KATALOG_ROOTS" => Some("/unused-security-test".into()),
            _ => None,
        })
        .unwrap(),
        clock: Arc::new(FixedTime),
        data: Mutex::new(Data {
            auth: Some(auth),
            sessions: HashMap::new(),
            limits: Limits::default(),
            revision: 0,
        }),
        changes: Arc::new(tokio::sync::Mutex::new(())),
        geometry: Arc::new(GeometryGate::default()),
        downloads: Arc::new(tokio::sync::Semaphore::new(4)),
        export: crate::export::Runtime::new(),
        slicer: None,
    });
    let request = Request::builder()
        .method("POST")
        .uri("/api/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"password":"wrong"}"#))
        .unwrap();
    let outer = tokio::spawn(dispatch(State(app.clone()), request));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if app.data.lock().unwrap().limits.pending == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    outer.abort();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if app.data.lock().unwrap().limits.global.len() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(app.data.lock().unwrap().limits.pending, 0);

    let session = new_session(&mut app.data.lock().unwrap(), "unknown", 100).unwrap();
    let guard = app.changes.clone().try_lock_owned().unwrap();
    let request = Request::builder()
        .method("POST")
        .uri("/api/password")
        .header("content-type", "application/json")
        .header("cookie", format!("katalog_session={}", session.token))
        .header("x-csrf-token", &session.csrf)
        .body(Body::from(
            serde_json::to_vec(&json!({"current":PASSWORD,"new":"Updated-security-test-84"}))
                .unwrap(),
        ))
        .unwrap();
    assert_eq!(dispatch(State(app.clone()), request).await.status(), 503);
    drop(guard);

    let guard = app.changes.clone().try_lock_owned().unwrap();
    let mut change = Box::pin(change_password(
        app.clone(),
        "unknown".into(),
        100,
        false,
        Some(session.clone()),
        json!({"current":PASSWORD,"new":"Updated-security-test-84"}),
        guard,
    ));
    // Pause at the first hash await, then model a concurrent successful logout.
    assert!(matches!(
        change
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    app.data.lock().unwrap().sessions.remove(&session.token);
    assert_eq!(change.await.status(), 401);
    assert!(app.data.lock().unwrap().sessions.is_empty());
    assert_eq!(app.data.lock().unwrap().revision, 0);
    assert!(!app.config.auth_file.exists());
}

#[test]
fn global_limit_reserves_inflight_attempts_but_existing_sessions_remain_available() {
    let mut limits = Limits {
        pending: 2,
        ..Limits::default()
    };
    for n in 0..98 {
        limits.failure(&format!("client{n}"), 100);
    }
    assert_eq!(limits.retry("fresh", 100, false), Some(60));
    assert_eq!(limits.retry("existing", 100, true), None);
}

async fn test_app() -> Arc<App> {
    Arc::new(App {
            fileops: None,
            fileops_safety: Mutex::new(crate::fileops_safety::FileOpsSafety::new(2000)),
            uploads:None,
        writer: None,
        config: Config::from_lookup(|key| match key {
            "KATALOG_DB" => Some("/unused-security-test/catalog.db".into()),
            "KATALOG_ROOTS" => Some("/unused-security-test".into()),
            _ => None,
        })
        .unwrap(),
        clock: Arc::new(FixedTime),
        data: Mutex::new(Data {
            auth: Some(AuthFile {
                version: 1,
                password_hash: password::hash_password(PASSWORD.into()).await.unwrap(),
                must_change: false,
                updated_at: 100,
            }),
            sessions: HashMap::new(),
            limits: Limits::default(),
            revision: 0,
        }),
        changes: Arc::new(tokio::sync::Mutex::new(())),
        geometry: Arc::new(GeometryGate::default()),
        downloads: Arc::new(tokio::sync::Semaphore::new(4)),
        export: crate::export::Runtime::new(),
        slicer: None,
    })
}

#[tokio::test]
async fn ipv6_prefixes_and_mapped_ipv4_share_client_limits() {
    let _serial = password::PASSWORD_TEST_LOCK.lock().await;
    let app = test_app().await;
    let key = |ip: &str| {
        let mut req = Request::new(Body::empty());
        req.extensions_mut().insert(ClientAddress(ip.into()));
        client(&app, &req).unwrap()
    };
    let first = key("2001:db8:1:2::1");
    for n in 1..=5 {
        app.data
            .lock()
            .unwrap()
            .limits
            .failure(&key(&format!("2001:db8:1:2::{n}")), 100);
    }
    assert_eq!(
        app.data.lock().unwrap().limits.retry(&first, 100, false),
        Some(60)
    );
    assert_eq!(
        app.data
            .lock()
            .unwrap()
            .limits
            .retry(&key("2001:db8:1:3::1"), 100, false),
        None
    );
    assert_eq!(key("::ffff:192.0.2.1"), key("192.0.2.1"));
    for n in 1..=11 {
        new_session(
            &mut app.data.lock().unwrap(),
            &key(&format!("2001:db8:1:2::{n}")),
            100,
        )
        .unwrap();
    }
    assert_eq!(app.data.lock().unwrap().sessions.len(), 10);
}

#[tokio::test]
async fn twenty_concurrent_wrong_attempts_do_not_exceed_five_checks() {
    let _serial = password::PASSWORD_TEST_LOCK.lock().await;
    let app = test_app().await;
    for _ in 0..4 {
        app.data.lock().unwrap().limits.failure("one", 100);
    }
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..20 {
        tasks.spawn(login(
            app.clone(),
            "one".into(),
            100,
            false,
            json!({"password":"wrong"}),
        ));
    }
    let mut checked = 4;
    while let Some(r) = tasks.join_next().await {
        let status = r.unwrap().status();
        if status == 401 {
            checked += 1;
        } else {
            assert!(status == 429 || status == 503);
        }
    }
    assert_eq!(checked, 5);
    assert_eq!(app.data.lock().unwrap().limits.global.len(), 5);
    assert_eq!(app.data.lock().unwrap().limits.pending, 0);
}

#[test]
fn expired_failure_entries_are_pruned_and_active_locks_are_not_evicted() {
    let clock = FixedTime;
    let now = clock.now();
    let mut limits = Limits::default();
    for n in 0..10_000 {
        let mut failure = Failure::default();
        failure.times.push_back(now);
        limits.clients.insert(n.to_string(), failure);
    }
    limits.retry("new", now + 600, true);
    assert!(limits.clients.is_empty());
    for n in 0..10_000 {
        limits.clients.insert(
            n.to_string(),
            Failure {
                until: now + 60,
                ..Failure::default()
            },
        );
    }
    assert!(limits.retry("new", now, true).is_some());
    assert_eq!(limits.clients.len(), 10_000);
    assert_eq!(limits.retry("0", now, true), Some(60));
}

struct StalledBody;
impl hyper::body::Body for StalledBody {
    type Data = axum::body::Bytes;
    type Error = std::convert::Infallible;
    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        Poll::Pending
    }
}
#[tokio::test]
async fn stalled_json_body_times_out_including_geometry_requests() {
    let _serial = password::PASSWORD_TEST_LOCK.lock().await;
    let app = test_app().await;
    tokio::time::pause();
    for path in [
        "/api/login",
        "/api/logout",
        "/api/invoke/get_model_geometry",
    ] {
        let s = new_session(&mut app.data.lock().unwrap(), "one", 100).unwrap();
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .header("cookie", format!("katalog_session={}", s.token))
            .header("x-csrf-token", &s.csrf)
            .body(Body::new(StalledBody))
            .unwrap();
        let task = tokio::spawn(dispatch(State(app.clone()), request));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(15)).await;
        tokio::task::yield_now().await;
        let response = tokio::time::timeout(Duration::from_millis(1), task)
            .await
            .unwrap_or_else(|_| panic!("body deadline missed: {path}"))
            .unwrap();
        assert_eq!(response.status(), 408);
    }
}

#[tokio::test]
async fn lock_is_rechecked_after_waiting_for_hash_slot() {
    let _serial = password::PASSWORD_TEST_LOCK.lock().await;
    let app = test_app().await;
    let first = password::hash_slot().await.unwrap();
    let second = password::hash_slot().await.unwrap();
    let task = tokio::spawn(login(
        app.clone(),
        "one".into(),
        100,
        false,
        json!({"password":"wrong"}),
    ));
    while app.data.lock().unwrap().limits.pending == 0 {
        tokio::task::yield_now().await;
    }
    for _ in 0..5 {
        app.data.lock().unwrap().limits.failure("one", 100);
    }
    drop(first);
    drop(second);
    assert_eq!(task.await.unwrap().status(), 429);
    assert_eq!(app.data.lock().unwrap().limits.global.len(), 5);
    assert_eq!(app.data.lock().unwrap().limits.pending, 0);
}

#[test]
fn full_failure_table_evicts_unlocked_entries_before_active_locks() {
    let mut limits = Limits::default();
    limits.clients.insert(
        "unlocked".into(),
        Failure {
            last_failure: Some(100),
            ..Failure::default()
        },
    );
    for n in 0..9999 {
        limits.clients.insert(
            n.to_string(),
            Failure {
                until: 200,
                ..Failure::default()
            },
        );
    }
    assert_eq!(limits.reserve("new", 100, true), Ok(()));
    assert_eq!(limits.clients.len(), 10_000);
    assert!(!limits.clients.contains_key("unlocked"));
    assert!(limits.clients.contains_key("0"));
    limits.release("new", true);
}

#[tokio::test]
async fn client_admission_allows_only_two_pending_hash_checks() {
    let _serial = password::PASSWORD_TEST_LOCK.lock().await;
    let app = test_app().await;
    let first = password::hash_slot().await.unwrap();
    let second = password::hash_slot().await.unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..20 {
        tasks.spawn(login(
            app.clone(),
            "one".into(),
            100,
            false,
            json!({"password":"wrong"}),
        ));
    }
    for _ in 0..18 {
        assert_eq!(tasks.join_next().await.unwrap().unwrap().status(), 429);
    }
    assert_eq!(app.data.lock().unwrap().limits.pending, 2);
    drop(first);
    drop(second);
    for _ in 0..2 {
        assert_eq!(tasks.join_next().await.unwrap().unwrap().status(), 401);
    }
    assert_eq!(app.data.lock().unwrap().limits.global.len(), 2);
    assert_eq!(app.data.lock().unwrap().limits.pending, 0);
}

#[tokio::test]
async fn whole_request_deadline_exempts_only_geometry_processing() {
    use tower::ServiceExt;
    let _serial = password::PASSWORD_TEST_LOCK.lock().await;
    let app = test_app().await;
    tokio::time::pause();
    let router = Router::new()
        .fallback(|| async {
            tokio::time::sleep(Duration::from_secs(20)).await;
            "completed"
        })
        .layer(middleware::from_fn_with_state(app, protect));
    for (path, expected) in [
        ("/api/health", 408),
        ("/api/invoke/get_model_geometry", 200),
    ] {
        let request = Request::builder()
            .uri(path)
            .header("host", "localhost:8080")
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(response.headers().contains_key("content-security-policy"));
    }
}
