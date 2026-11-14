use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use katalog_server::{build_app, config::Config, AppConfig, SystemClock};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tower::ServiceExt;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    app: Router,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "katalog-static-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<html>React</html>").unwrap();
        for name in [
            "app-AbCd1234.js",
            "app-BVkgg-FE.css",
            "font-AbCd1234.woff2",
            "icon.svg",
        ] {
            std::fs::write(dir.join("assets").join(name), "asset").unwrap();
        }
        let config = Config::from_lookup(|key| match key {
            "KATALOG_DB" => Some(dir.join("catalog.db").to_string_lossy().into_owned()),
            "KATALOG_ROOTS" => Some(dir.to_string_lossy().into_owned()),
            "KATALOG_ALLOWED_HOSTS" => Some("localhost:8080,localhost:1420".into()),
            "KATALOG_STATIC_DIR" => Some(dir.to_string_lossy().into_owned()),
            _ => None,
        })
        .unwrap();
        Self {
            dir,
            app: build_app(AppConfig {
                config,
                auth: None,
                clock: Arc::new(SystemClock),
            }),
        }
    }
    async fn request(&self, method: &str, path: &str) -> axum::response::Response {
        self.app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("host", "localhost:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}
#[tokio::test]
async fn serves_assets_spa_types_and_cache_without_authentication() {
    let f = Fixture::new();
    for path in ["/", "/index.html", "/models/42"] {
        let r = f.request("GET", path).await;
        assert_eq!(r.status(), 200, "{path}");
        assert_eq!(r.headers()["content-type"], "text/html; charset=utf-8");
        assert_eq!(r.headers()["cache-control"], "no-cache");
        assert_eq!(
            to_bytes(r.into_body(), 4096).await.unwrap(),
            "<html>React</html>"
        );
    }
    for (name, content_type) in [
        ("app-AbCd1234.js", "application/javascript; charset=utf-8"),
        ("app-BVkgg-FE.css", "text/css; charset=utf-8"),
        ("font-AbCd1234.woff2", "font/woff2"),
        ("icon.svg", "image/svg+xml"),
    ] {
        let r = f.request("GET", &format!("/assets/{name}")).await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["content-type"], content_type);
        if name != "icon.svg" {
            assert_eq!(
                r.headers()["cache-control"],
                "public, max-age=31536000, immutable"
            );
        }
        assert!(r.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("img-src 'self' data: blob:; font-src 'self'"));
    }
    assert_eq!(f.request("GET", "/missing.js").await.status(), 404);
}
#[tokio::test]
async fn rejects_unsafe_paths_and_symlinks_and_never_lists_directories() {
    let f = Fixture::new();
    for path in [
        "/../secret",
        "/./index.html",
        "/%2e%2e/secret",
        "/assets%2fapp.js",
        "/a%5cb",
        "/a:b",
        "/%00",
        "/%zz",
        "/%252e%252e/secret",
    ] {
        assert!(
            f.request("GET", path).await.status().is_client_error(),
            "{path}"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", f.dir.join("outside.txt")).unwrap();
        assert_eq!(f.request("GET", "/outside.txt").await.status(), 404);
    }
    let r = f.request("GET", "/assets").await;
    assert!(
        !String::from_utf8_lossy(&to_bytes(r.into_body(), 4096).await.unwrap())
            .contains("app-AbCd1234")
    );
}
#[tokio::test]
async fn api_takes_precedence_and_static_methods_are_405_before_auth() {
    let f = Fixture::new();
    assert_eq!(f.request("GET", "/api/session").await.status(), 200);
    for method in ["GET", "POST"] {
        let r = f.request(method, "/api/missing").await;
        assert_eq!(r.status(), 404);
        assert_eq!(r.headers()["cache-control"], "no-store");
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(r.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(value["expected"], true);
    }
    for method in ["HEAD", "POST"] {
        for path in ["/", "/models/42", "/assets/app-AbCd1234.js", "/missing.css"] {
            assert_eq!(
                f.request(method, path).await.status(),
                405,
                "{method} {path}"
            );
        }
    }
}

#[tokio::test]
async fn dev_proxy_preserved_host_accepts_matching_origin_with_port() {
    let f = Fixture::new();
    for (origin, status) in [
        ("http://localhost:1420", 400),
        ("http://localhost:8080", 403),
    ] {
        let request = Request::builder()
            .method("POST")
            .uri("/api/login")
            .header("host", "localhost:1420")
            .header("origin", origin)
            .body(Body::empty())
            .unwrap();
        // 400 is the JSON parser: matching Origin passed the outer protection.
        assert_eq!(
            f.app.clone().oneshot(request).await.unwrap().status(),
            status
        );
    }
}

#[test]
fn static_configuration_rejects_missing_directory_and_regular_file() {
    let f = Fixture::new();
    for path in [f.dir.join("missing"), f.dir.join("index.html")] {
        assert!(Config::from_lookup(|key| match key {
            "KATALOG_STATIC_DIR" => Some(path.to_string_lossy().into_owned()),
            "KATALOG_DB" => Some(f.dir.join("catalog.db").to_string_lossy().into_owned()),
            "KATALOG_ROOTS" => Some(f.dir.to_string_lossy().into_owned()),
            _ => None,
        })
        .is_err());
    }
}
