#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_are_per_client_and_lock_does_not_extend() {
        let mut limits = Limits::default();
        for _ in 0..5 {
            limits.failure("a", 10);
        }
        assert_eq!(limits.retry("a", 10, false), Some(60));
        assert_eq!(limits.retry("a", 30, false), Some(40));
        assert_eq!(limits.retry("b", 30, false), None);
        assert_eq!(limits.retry("a", 70, false), None);
        for _ in 0..5 {
            limits.failure("a", 70);
        }
        assert_eq!(limits.retry("a", 70, false), Some(120));
    }
    #[test]
    fn limits_escalate_to_fifteen_minutes_and_global_budget_spares_sessions() {
        let mut limits = Limits::default();
        let mut now = 1000;
        for duration in [60, 120, 240, 480, 900, 60] {
            for _ in 0..5 {
                limits.failure("a", now);
            }
            assert_eq!(limits.retry("a", now, false), Some(duration));
            assert_eq!(limits.retry("a", now + duration - 1, false), Some(1));
            now += duration;
        }
        limits.success("a");
        for _ in 0..5 {
            limits.failure("a", now);
        }
        assert_eq!(limits.retry("a", now, false), Some(60));
        let mut limits = Limits::default();
        for n in 0..100 {
            limits.failure(&format!("client{n}"), 1000);
        }
        assert_eq!(limits.retry("fresh", 1000, false), Some(60));
        assert_eq!(limits.retry("fresh", 1000, true), None);
        assert_eq!(limits.retry("fresh", 1060, false), None);
        limits.pending = 100;
        assert_eq!(limits.retry("fresh", 1060, false), Some(60));
    }
    #[test]
    fn session_caps_and_expiration() {
        let mut d = Data {
            auth: None,
            sessions: HashMap::new(),
            limits: Limits::default(),
            revision: 0,
        };
        let oldest = new_session(&mut d, "one", 1000).unwrap().token;
        for t in 1001..1011 {
            new_session(&mut d, "one", t).unwrap();
        }
        assert_eq!(d.sessions.len(), 10);
        assert!(!d.sessions.contains_key(&oldest));
        for n in 0..100 {
            new_session(&mut d, &format!("client{n}"), 1100 + n).unwrap();
        }
        assert_eq!(d.sessions.len(), 100);
        cleanup(&mut d, 8400);
        assert!(d.sessions.is_empty());
        let s = new_session(&mut d, "one", 10000).unwrap();
        d.sessions.get_mut(&s.token).unwrap().seen = 53199;
        cleanup(&mut d, 53199);
        assert_eq!(d.sessions.len(), 1);
        cleanup(&mut d, 53200);
        assert!(d.sessions.is_empty());
    }
}

use crate::{
    config::{parse_host, Config},
    invoke::{invoke, GeometryGate},
    password::{self, AuthFile},
};
use axum::{
    body::{to_bytes, Body},
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tower_http::limit::RequestBodyLimitLayer;

pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
/// Tests can provide both time and a peer without opening any sockets.
#[derive(Clone)]
pub struct ClientAddress(pub String);
pub struct AppConfig {
    pub config: Config,
    pub auth: Option<AuthFile>,
    pub clock: Arc<dyn Clock>,
}
#[derive(Clone)]
struct Session {
    token: String,
    csrf: String,
    client: String,
    created: u64,
    seen: u64,
}
#[derive(Default)]
struct Failure {
    times: VecDeque<u64>,
    until: u64,
    strikes: u32,
    last_failure: Option<u64>,
    running: usize,
}
#[derive(Default)]
struct Limits {
    clients: HashMap<String, Failure>,
    global: VecDeque<u64>,
    pending: usize,
}
impl Limits {
    fn prune(&mut self, now: u64) {
        self.clients.retain(|_, f| {
            while f
                .times
                .front()
                .is_some_and(|t| now.saturating_sub(*t) >= 600)
            {
                f.times.pop_front();
            }
            f.running > 0
                || f.until > now
                || !f.times.is_empty()
                || f.last_failure.is_some_and(|t| now.saturating_sub(t) < 600)
        });
    }
    fn reserve(&mut self, client: &str, now: u64, existing: bool) -> Result<(), u64> {
        if let Some(t) = self.retry(client, now, existing) {
            return Err(t);
        }
        if !self.clients.contains_key(client) && self.clients.len() >= 10_000 {
            let victim = self
                .clients
                .iter()
                .filter(|(_, f)| f.until <= now && f.running == 0)
                .min_by_key(|(_, f)| f.last_failure)
                .map(|(key, _)| key.clone());
            if let Some(key) = victim {
                self.clients.remove(&key);
            } else {
                return Err(1);
            }
        }
        self.clients.entry(client.into()).or_default().running += 1;
        if !existing {
            self.pending += 1;
        }
        Ok(())
    }
    fn release(&mut self, client: &str, existing: bool) {
        if let Some(f) = self.clients.get_mut(client) {
            f.running -= 1;
        }
        if !existing {
            self.pending -= 1;
        }
    }
    fn retry(&mut self, client: &str, now: u64, existing: bool) -> Option<u64> {
        self.prune(now);
        while self
            .global
            .front()
            .is_some_and(|t| now.saturating_sub(*t) >= 60)
        {
            self.global.pop_front();
        }
        if let Some(f) = self.clients.get(client) {
            if f.until > now {
                return Some(f.until - now);
            }
            if f.running >= 2 || f.times.len() + f.running >= 5 {
                return Some(1);
            }
        }
        if !self.clients.contains_key(client)
            && self.clients.len() >= 10_000
            && !self
                .clients
                .values()
                .any(|f| f.until <= now && f.running == 0)
        {
            return Some(1);
        }
        if !existing && self.global.len() + self.pending >= 100 {
            return Some(60);
        }
        None
    }
    fn failure(&mut self, client: &str, now: u64) {
        self.prune(now);
        self.global.push_back(now);
        let f = self.clients.entry(client.into()).or_default();
        while f
            .times
            .front()
            .is_some_and(|t| now.saturating_sub(*t) >= 600)
        {
            f.times.pop_front();
        }
        f.last_failure = Some(now);
        f.times.push_back(now);
        if f.times.len() >= 5 {
            f.until = now + (60u64.saturating_mul(1u64 << f.strikes.min(4))).min(900);
            f.strikes = f.strikes.saturating_add(1);
            f.times.clear();
        }
    }
    fn success(&mut self, client: &str) {
        if let Some(f) = self.clients.get_mut(client) {
            let running = f.running;
            *f = Failure {
                running,
                ..Failure::default()
            };
        }
    }
}
struct Data {
    auth: Option<AuthFile>,
    sessions: HashMap<String, Session>,
    limits: Limits,
    revision: u64,
}
struct App {
    config: Config,
    clock: Arc<dyn Clock>,
    data: Mutex<Data>,
    changes: Arc<tokio::sync::Mutex<()>>,
    geometry: Arc<GeometryGate>,
    downloads: Arc<tokio::sync::Semaphore>,
}
fn error(status: u16, message: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap(),
        Json(json!({"message":message,"expected":status<500})),
    )
        .into_response()
}
fn boxed_error(status: u16, message: &str) -> Box<Response> {
    Box::new(error(status, message))
}
fn random_token() -> Result<String, Box<Response>> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| boxed_error(500, "Zufallsquelle nicht verfügbar"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn event(kind: &str, client: &str, now: u64) {
    eprintln!("Anmeldung: {kind}; Client={client}; Zeit={now}");
}

pub async fn initialize(mut config: Config) -> Result<Router, String> {
    let db_path = config.db_path.clone();
    tokio::task::spawn_blocking(move || {
        katalog_core::db::open_read_only(&db_path)
            .map(|_| ())
            .map_err(|e| format!("Katalog kann nicht nur lesend geöffnet werden: {e}"))
    })
    .await
    .map_err(|_| "Katalogprüfung fehlgeschlagen".to_string())??;
    let auth = if config.allow_no_auth {
        eprintln!("WARNUNG: Anmeldung und CSRF sind ausgeschaltet (nur Loopback)!");
        None
    } else {
        Some(
            password::bootstrap(
                &config.auth_file,
                config.password.take(),
                config.password_file.take(),
            )
            .await?,
        )
    };
    Ok(build_app(AppConfig {
        config,
        auth,
        clock: Arc::new(SystemClock),
    }))
}
pub fn build_app(config: AppConfig) -> Router {
    let app = Arc::new(App {
        config: config.config,
        clock: config.clock,
        data: Mutex::new(Data {
            auth: config.auth,
            sessions: HashMap::new(),
            limits: Limits::default(),
            revision: 0,
        }),
        changes: Arc::new(tokio::sync::Mutex::new(())),
        geometry: Arc::new(GeometryGate::default()),
        downloads: Arc::new(tokio::sync::Semaphore::new(4)),
    });
    Router::new()
        .fallback(dispatch)
        .layer(RequestBodyLimitLayer::new(65536))
        .layer(middleware::from_fn_with_state(app.clone(), protect))
        .with_state(app)
}
fn secure(app: &App, h: &HeaderMap) -> bool {
    app.config.trust_proxy
        && h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) == Some("https")
}
fn client_key(ip: std::net::IpAddr) -> String {
    match ip {
        std::net::IpAddr::V4(ip) => ip.to_string(),
        std::net::IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(ip) => ip.to_string(),
            None => {
                let prefix = u128::from(ip) & (u128::MAX << 64);
                format!("{}/64", std::net::Ipv6Addr::from(prefix))
            }
        },
    }
}
fn client(app: &App, req: &Request) -> Result<String, Box<Response>> {
    if app.config.trust_proxy {
        // Across repeated header fields the last field contains the last hop.
        let ip = req
            .headers()
            .get_all("x-forwarded-for")
            .iter()
            .next_back()
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit(',').next())
            .and_then(|s| {
                s.trim()
                    .parse::<std::net::IpAddr>()
                    .ok()
                    .or_else(|| s.trim().parse::<SocketAddr>().ok().map(|s| s.ip()))
            })
            .ok_or_else(|| {
                boxed_error(400, "Proxy-Header X-Forwarded-For fehlt oder ist ungültig")
            })?;
        return Ok(client_key(ip));
    }
    if let Some(p) = req.extensions().get::<ClientAddress>() {
        return Ok(p.0.parse().map(client_key).unwrap_or_else(|_| p.0.clone()));
    }
    Ok(req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|p| client_key(p.0.ip()))
        .unwrap_or_else(|| "unknown".into()))
}
fn request_host(h: &HeaderMap) -> Result<(String, Option<u16>), Box<Response>> {
    if h.get_all(header::HOST).iter().count() != 1 {
        return Err(boxed_error(400, "Ungültiger Host-Header"));
    }
    parse_host(
        h.get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| boxed_error(400, "Ungültiger Host-Header"))?,
    )
    .map_err(|_| boxed_error(400, "Ungültiger Host-Header"))
}
fn origin_ok(app: &App, h: &HeaderMap) -> bool {
    let Some(origin) = h.get(header::ORIGIN) else {
        return true;
    };
    if h.get_all(header::ORIGIN).iter().count() != 1 {
        return false;
    }
    if origin.to_str().ok().is_none_or(|s| {
        s.split_once("://")
            .is_none_or(|(_, a)| a.contains(['/', '?', '#']))
    }) {
        return false;
    }
    let Some(uri) = origin
        .to_str()
        .ok()
        .and_then(|s| s.parse::<axum::http::Uri>().ok())
    else {
        return false;
    };
    let scheme = if secure(app, h) { "https" } else { "http" };
    if uri.scheme_str() != Some(scheme) || uri.path_and_query().is_some_and(|p| p.as_str() != "/") {
        return false;
    }
    let Some(authority) = uri.authority() else {
        return false;
    };
    let Ok((oh, op)) = parse_host(authority.as_str()) else {
        return false;
    };
    let Ok((rh, rp)) = request_host(h) else {
        return false;
    };
    let default = if scheme == "https" { 443 } else { 80 };
    oh == rh && op.unwrap_or(default) == rp.unwrap_or(default)
}
async fn protect(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let api = req.uri().path().starts_with("/api/");
    let mut response = match request_host(req.headers()) {
        Err(e) => *e,
        Ok(host) => {
            let allowed = app.config.allowed_hosts.as_ref().is_none_or(|list| {
                list.iter().any(|s| {
                    parse_host(s).ok().is_some_and(|(name, port)| {
                        name == host.0
                            && port.unwrap_or(if secure(&app, req.headers()) { 443 } else { 80 })
                                == host.1.unwrap_or(if secure(&app, req.headers()) {
                                    443
                                } else {
                                    80
                                })
                    })
                })
            });
            if !allowed {
                error(421, "Host nicht zugelassen")
            } else if req.method() != Method::GET && req.method() != Method::POST {
                error(405, "Methode nicht erlaubt")
            } else if req.method() == Method::POST
                && !app.config.allow_no_auth
                && !origin_ok(&app, req.headers())
            {
                error(403, "Origin nicht zugelassen")
            } else if req.uri().path() == "/api/invoke/get_model_geometry" {
                // Geometry processing has its own bounded admission; its body
                // still has the same 15-second read deadline as other routes.
                next.run(req).await
            } else {
                tokio::time::timeout(std::time::Duration::from_secs(15), next.run(req))
                    .await
                    .unwrap_or_else(|_| error(408, "Zeitlimit für Anfrage überschritten"))
            }
        }
    };
    for (name, value) in [
        (
            "content-security-policy",
            "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'; img-src 'self' data: blob:; font-src 'self'",
        ),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
    ] {
        response.headers_mut().insert(name, value.parse().unwrap());
    }
    if api {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    }
    response
}
fn cookie_name(secure: bool) -> &'static str {
    if secure {
        "__Host-katalog_session"
    } else {
        "katalog_session"
    }
}
fn cookie(token: &str, secure: bool, delete: bool) -> String {
    format!(
        "{}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}{}",
        cookie_name(secure),
        token,
        if delete { 0 } else { 43200 },
        if secure { "; Secure" } else { "" }
    )
}
fn with_cookie(mut response: Response, token: &str, secure: bool, delete: bool) -> Response {
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie(token, secure, delete).parse().unwrap(),
    );
    response
}
fn cleanup(data: &mut Data, now: u64) {
    data.sessions
        .retain(|_, s| now.saturating_sub(s.created) < 43200 && now.saturating_sub(s.seen) < 7200);
}
fn session(app: &App, h: &HeaderMap, now: u64) -> Option<Session> {
    let mut data = app.data.lock().unwrap();
    cleanup(&mut data, now);
    let name = cookie_name(secure(app, h));
    let token = h
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|s| s.trim().split_once('='))
        .find(|(n, _)| *n == name)?
        .1;
    let session = data.sessions.get_mut(token)?;
    if now.saturating_sub(session.seen) >= 60 {
        session.seen = now;
    }
    Some(session.clone())
}
fn new_session(data: &mut Data, client: &str, now: u64) -> Result<Session, Box<Response>> {
    let s = Session {
        token: random_token()?,
        csrf: random_token()?,
        client: client.into(),
        created: now,
        seen: now,
    };
    cleanup(data, now);
    if data
        .sessions
        .values()
        .filter(|s| s.client == client)
        .count()
        >= 10
    {
        if let Some(key) = data
            .sessions
            .values()
            .filter(|s| s.client == client)
            .min_by_key(|s| s.created)
            .map(|s| s.token.clone())
        {
            data.sessions.remove(&key);
        }
    }
    if data.sessions.len() >= 100 {
        if let Some(key) = data
            .sessions
            .values()
            .min_by_key(|s| s.created)
            .map(|s| s.token.clone())
        {
            data.sessions.remove(&key);
        }
    }
    data.sessions.insert(s.token.clone(), s.clone());
    Ok(s)
}
fn retry_response(seconds: u64) -> Response {
    let mut r = error(429, "Zu viele Fehlversuche. Bitte später erneut versuchen.");
    r.headers_mut()
        .insert(header::RETRY_AFTER, seconds.to_string().parse().unwrap());
    r
}
async fn read_body(body: Body) -> Result<axum::body::Bytes, Box<Response>> {
    tokio::time::timeout(std::time::Duration::from_secs(15), to_bytes(body, 65536))
        .await
        .map_err(|_| boxed_error(408, "Zeitlimit für Anfrage überschritten"))?
        .map_err(|_| boxed_error(413, "JSON-Körper ist zu groß"))
}
async fn json_body(body: Body) -> Result<Value, Box<Response>> {
    let bytes = read_body(body).await?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| boxed_error(400, "Ungültiger JSON-Körper"))?;
    if !value.is_object() {
        return Err(boxed_error(400, "JSON-Objekt erwartet"));
    }
    Ok(value)
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, Box<Response>> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| boxed_error(400, "Fehlendes oder ungültiges Textargument"))
}
async fn dispatch(State(app): State<Arc<App>>, req: Request) -> Response {
    let now = app.clock.now();
    let path = req.uri().path().to_owned();
    let method = req.method().clone();
    let download_id = path
        .strip_prefix("/api/files/")
        .and_then(|p| p.strip_suffix("/download"))
        .filter(|id| !id.contains('/'));
    // Reserve the complete API namespace, including unknown routes, before SPA fallback.
    if path == "/api" || path.starts_with("/api/") {
        if !matches!(
            path.as_str(),
            "/api/health" | "/api/session" | "/api/login" | "/api/logout" | "/api/password"
        ) && !path.starts_with("/api/invoke/")
            && download_id.is_none()
        {
            return error(404, "Nicht gefunden");
        }
    } else {
        if method != Method::GET {
            return error(405, "Methode nicht erlaubt");
        }
        if let Some(root) = &app.config.static_dir {
            return crate::static_files::serve(root.clone(), path).await;
        }
    }

    let peer = match client(&app, &req) {
        Ok(peer) => peer,
        Err(e) => return *e,
    };
    let headers = req.headers().clone();
    let is_secure = secure(&app, &headers);
    let current = session(&app, &headers, now);
    if let Some(id) = download_id {
        if method != Method::GET {
            return error(405, "Methode nicht erlaubt");
        }
        if !app.config.allow_no_auth {
            if current.is_none() {
                return error(401, "Anmeldung erforderlich");
            }
            if app
                .data
                .lock()
                .unwrap()
                .auth
                .as_ref()
                .is_some_and(|a| a.must_change)
            {
                return (StatusCode::FORBIDDEN, Json(json!({"message":"Bitte zuerst ein neues Passwort setzen","expected":true,"code":"password_change_required"}))).into_response();
            }
        }
        return crate::download::download(
            app.config.db_path.clone(),
            app.config.roots.clone(),
            id,
            &headers,
            app.downloads.clone(),
        )
        .await;
    }
    if method == Method::GET {
        return match path.as_str() {
            "/api/health"=>Json(json!({"ok":true})).into_response(),
            "/api/session"=>Json(json!({"authenticated":app.config.allow_no_auth || current.is_some(),"mustChangePassword":current.is_some() && app.data.lock().unwrap().auth.as_ref().is_some_and(|a|a.must_change),"csrfToken":current.as_ref().map(|s|&s.csrf)})).into_response(),
            "/"=>([(header::CONTENT_TYPE,"text/html; charset=utf-8")],include_str!("../static/index.html")).into_response(),
            "/static/app.js"=>([(header::CONTENT_TYPE,"application/javascript; charset=utf-8")],include_str!("../static/app.js")).into_response(),
            "/static/app.css"=>([(header::CONTENT_TYPE,"text/css; charset=utf-8")],include_str!("../static/app.css")).into_response(),
            p if p=="/api/login" || p=="/api/logout" || p=="/api/password" || p.starts_with("/api/invoke/")=>error(405,"Methode nicht erlaubt"),
            _=>error(404,"Nicht gefunden")
        };
    }
    if matches!(
        path.as_str(),
        "/" | "/api/health" | "/api/session" | "/static/app.js" | "/static/app.css"
    ) {
        return error(405, "Methode nicht erlaubt");
    }
    if path != "/api/login" && !app.config.allow_no_auth {
        let Some(s) = &current else {
            return error(401, "Anmeldung erforderlich");
        };
        let token = headers
            .get("x-csrf-token")
            .map(|h| h.as_bytes())
            .unwrap_or_default();
        if !bool::from(token.ct_eq(s.csrf.as_bytes())) {
            return error(403, "Ungültiges CSRF-Token");
        }
    }
    if path == "/api/logout" {
        if let Err(e) = read_body(req.into_body()).await {
            return *e;
        }
        let _change = app.changes.lock().await;
        if let Some(s) = current {
            app.data.lock().unwrap().sessions.remove(&s.token);
        }
        return with_cookie(
            Json(json!({"ok":true})).into_response(),
            "",
            is_secure,
            true,
        );
    }
    if path.starts_with("/api/invoke/")
        && !app.config.allow_no_auth
        && app
            .data
            .lock()
            .unwrap()
            .auth
            .as_ref()
            .is_some_and(|a| a.must_change)
    {
        return (StatusCode::FORBIDDEN,Json(json!({"message":"Bitte zuerst ein neues Passwort setzen","expected":true,"code":"password_change_required"}))).into_response();
    }
    if !headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            let mime = v.split(';').next().unwrap_or("").trim();
            mime == "application/json"
                || (mime.starts_with("application/") && mime.ends_with("+json"))
        })
    {
        return error(400, "Content-Type application/json erwartet");
    }
    let args = match json_body(req.into_body()).await {
        Ok(v) => v,
        Err(e) => return *e,
    };
    match path.as_str() {
        "/api/login" => {
            match tokio::spawn(login(app.clone(), peer.clone(), now, is_secure, args)).await {
                Ok(r) => r,
                Err(_) => error(500, "Anmeldung fehlgeschlagen"),
            }
        }
        "/api/password" => {
            let guard = match app.changes.clone().try_lock_owned() {
                Ok(g) => g,
                Err(_) => return error(503, "Passwortänderung bereits aktiv"),
            };
            match tokio::spawn(change_password(
                app.clone(),
                peer.clone(),
                now,
                is_secure,
                current,
                args,
                guard,
            ))
            .await
            {
                Ok(r) => r,
                Err(_) => error(500, "Passwortänderung fehlgeschlagen"),
            }
        }
        p if p.starts_with("/api/invoke/") => {
            invoke(
                app.config.db_path.clone(),
                app.config.roots.clone(),
                p[12..].into(),
                args,
                app.geometry.clone(),
            )
            .await
        }
        _ => error(404, "Nicht gefunden"),
    }
}
struct LoginAttempt {
    app: Arc<App>,
    peer: String,
    existing: bool,
}
impl Drop for LoginAttempt {
    fn drop(&mut self) {
        self.app
            .data
            .lock()
            .unwrap()
            .limits
            .release(&self.peer, self.existing);
    }
}
async fn verify_client(
    app: Arc<App>,
    peer: &str,
    hash: String,
    value: String,
    existing: bool,
) -> Result<bool, Box<Response>> {
    {
        let mut d = app.data.lock().unwrap();
        d.limits
            .reserve(peer, app.clock.now(), existing)
            .map_err(|t| {
                event("Sperre", peer, app.clock.now());
                Box::new(retry_response(t))
            })?;
    }
    let _attempt = LoginAttempt {
        app: app.clone(),
        peer: peer.into(),
        existing,
    };
    let permit = password::hash_slot()
        .await
        .map_err(|_| boxed_error(503, "Passwortprüfung derzeit ausgelastet"))?;
    {
        let d = app.data.lock().unwrap();
        // Recheck after waiting for a hash slot. Own reservation already counts
        // toward the per-client and global budgets; do not reserve it twice.
        if let Some(f) = d.limits.clients.get(peer) {
            let now = app.clock.now();
            if f.until > now {
                return Err(Box::new(retry_response(f.until - now)));
            }
        }
    }
    let valid = password::verify_with_slot(hash, value, permit)
        .await
        .map_err(|_| boxed_error(503, "Passwortprüfung derzeit ausgelastet"))?;
    if !valid {
        let now = app.clock.now();
        app.data.lock().unwrap().limits.failure(peer, now);
        event("Fehlschlag", peer, now);
    }
    Ok(valid)
}
async fn login(app: Arc<App>, peer: String, now: u64, secure: bool, args: Value) -> Response {
    if app.config.allow_no_auth {
        return Json(json!({"ok":true,"mustChangePassword":false,"csrfToken":null}))
            .into_response();
    }
    let password = match string(&args, "password") {
        Ok(s) => s.to_owned(),
        Err(e) => return *e,
    };
    let (hash, revision) = {
        let d = app.data.lock().unwrap();
        (d.auth.as_ref().unwrap().password_hash.clone(), d.revision)
    };
    let valid = match verify_client(app.clone(), &peer, hash, password, false).await {
        Ok(v) => v,
        Err(e) => return *e,
    };
    let _change = app.changes.lock().await;
    let mut d = app.data.lock().unwrap();
    if !valid || revision != d.revision {
        return error(401, "Passwort nicht korrekt");
    }
    d.limits.success(&peer);
    event("Erfolg", &peer, now);
    let s = match new_session(&mut d, &peer, now) {
        Ok(s) => s,
        Err(e) => return *e,
    };
    with_cookie(Json(json!({"ok":true,"mustChangePassword":d.auth.as_ref().unwrap().must_change,"csrfToken":s.csrf})).into_response(),&s.token,secure,false)
}
async fn change_password(
    app: Arc<App>,
    peer: String,
    now: u64,
    secure: bool,
    current: Option<Session>,
    args: Value,
    _guard: tokio::sync::OwnedMutexGuard<()>,
) -> Response {
    let Some(current) = current else {
        return error(401, "Anmeldung erforderlich");
    };
    let old = match string(&args, "current") {
        Ok(s) => s.to_owned(),
        Err(e) => return *e,
    };
    let new = match string(&args, "new") {
        Ok(s) => s.to_owned(),
        Err(e) => return *e,
    };
    let hash = {
        let d = app.data.lock().unwrap();
        if !d.sessions.contains_key(&current.token) {
            return error(401, "Anmeldung erforderlich");
        }
        d.auth.as_ref().unwrap().password_hash.clone()
    };
    match verify_client(app.clone(), &peer, hash, old.clone(), true).await {
        Ok(true) => {}
        Ok(false) => {
            return error(401, "Aktuelles Passwort nicht korrekt");
        }
        Err(e) => return *e,
    }
    if let Err(e) = password::policy(&new, Some(&old)) {
        return error(422, &e);
    }
    let hash = match password::hash_password(new).await {
        Ok(h) => h,
        Err(_) => return error(503, "Passwortprüfung derzeit ausgelastet"),
    };
    let auth = AuthFile {
        version: 1,
        password_hash: hash,
        must_change: false,
        updated_at: now,
    };
    let token = match random_token() {
        Ok(s) => s,
        Err(e) => return *e,
    };
    let csrf = match random_token() {
        Ok(s) => s,
        Err(e) => return *e,
    };
    // The async change guard serializes login/logout with publication without
    // holding the in-memory mutex during filesystem I/O.
    {
        let mut d = app.data.lock().unwrap();
        cleanup(&mut d, app.clock.now());
        if !d.sessions.contains_key(&current.token) {
            return error(401, "Anmeldung erforderlich");
        }
    }
    let path = app.config.auth_file.clone();
    let persisted = auth.clone();
    if !matches!(
        tokio::task::spawn_blocking(move || password::write_auth(&path, &persisted)).await,
        Ok(Ok(()))
    ) {
        return error(500, "Passwort konnte nicht gespeichert werden");
    }
    let now = app.clock.now();
    let mut d = app.data.lock().unwrap();
    cleanup(&mut d, now);
    let active = d.sessions.contains_key(&current.token);
    d.auth = Some(auth);
    d.revision += 1;
    d.sessions.clear();
    d.limits.success(&peer);
    event("Passwortänderung", &peer, now);
    if !active {
        return error(401, "Sitzung abgelaufen; bitte erneut anmelden");
    }
    d.sessions.insert(
        token.clone(),
        Session {
            token: token.clone(),
            csrf: csrf.clone(),
            client: peer,
            created: now,
            seen: now,
        },
    );
    with_cookie(
        Json(json!({"ok":true,"mustChangePassword":false,"csrfToken":csrf})).into_response(),
        &token,
        secure,
        false,
    )
}

#[cfg(test)]
#[path = "web_security_tests.rs"]
mod security_tests;
