//! Thingiverse search with the user's own API key: read only, no download, nothing
//! on disk. Hits and preview images live in memory for ten minutes at most.
//!
//! The service holds no key and no database access. The caller passes the key per
//! call and a gate that counts the request in the daily counter right before it goes
//! out, so cache hits and refused requests are not counted.

mod cache;
mod client;
mod limits;
mod parse;
mod token;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use crate::error_code::ErrorCode;
use cache::{Cache, Key};
use client::{classify, Http, Reply, MAX_IMAGE_BYTES, MAX_JSON_BYTES};
pub use limits::Limits;
use limits::{Limiter, Slot};
use parse::ThingRefs;
pub use token::{Token, TokenFormatError};

/// Requests per day after which the app stops asking, and from which it warns. The
/// figure is the one named for the free tier; it is an estimate, not a contract.
pub const DAILY_LIMIT: u32 = 1000;
pub const DAILY_WARNING: u32 = 800;

/// What went wrong, as a registered code plus plain data. Carries nothing of the key,
/// the address or the search term, so `{:?}` of it is safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvError {
    pub code: ErrorCode,
    pub retry_after_s: Option<u64>,
}

impl TvError {
    pub fn new(code: ErrorCode) -> Self {
        TvError { code, retry_after_s: None }
    }

    pub fn rate_limited(seconds: u64) -> Self {
        TvError { code: ErrorCode::ThingiverseRateLimited, retry_after_s: Some(seconds) }
    }

    /// The `params` of the coded error: the seconds to wait for a rate limit.
    pub fn params(&self) -> Vec<String> {
        self.retry_after_s.map(|s| s.to_string()).into_iter().collect()
    }
}

/// Checked right before a request leaves the program; an error stops it.
pub type Gate<'a> = &'a dyn Fn() -> Result<(), TvError>;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThingSummaryDto {
    pub id: u64,
    pub name: String,
    /// Empty when the answer named no author; the interface then says so.
    pub author_name: String,
    pub license_name: Option<String>,
    pub like_count: Option<u64>,
    pub preview_available: bool,
    /// Address of the thing's page, built or checked by the core.
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThingDetailDto {
    pub id: u64,
    pub name: String,
    pub author_name: String,
    pub license_name: Option<String>,
    pub like_count: Option<u64>,
    pub preview_available: bool,
    pub url: String,
    /// Plain text, cut to 4000 characters; never HTML.
    pub description: String,
    pub created_at: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThingiverseSearchDto {
    pub items: Vec<ThingSummaryDto>,
    pub has_more: bool,
    pub page: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSize {
    Thumb,
    Large,
}

#[derive(Clone)]
pub struct Config {
    pub base_url: String,
    pub timeout: Duration,
    pub limits: Limits,
    /// Tests only: images and redirects may go to the stub on the loopback address.
    pub local_test: bool,
}

impl Config {
    pub fn production() -> Self {
        Config {
            base_url: parse::API_BASE.to_string(),
            timeout: Duration::from_secs(15),
            limits: Limits::PRODUCTION,
            local_test: false,
        }
    }
}

#[derive(Clone)]
enum Cached {
    Search(ThingiverseSearchDto),
    Thing(ThingDetailDto),
    Refs(ThingRefs),
    Image(String),
}

struct State {
    limiter: Limiter,
    cache: Cache<Cached>,
}

type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

pub struct Service {
    cfg: Config,
    clock: Clock,
    state: Mutex<State>,
    http: Mutex<Option<Arc<Http>>>,
}

impl Service {
    pub fn new() -> Self {
        let start = std::time::Instant::now();
        Self::with_config(Config::production(), Arc::new(move || start.elapsed().as_secs_f64()))
    }

    pub fn with_config(cfg: Config, clock: Clock) -> Self {
        let limiter = Limiter::new(cfg.limits);
        Service { cfg, clock, state: Mutex::new(State { limiter, cache: Cache::new() }), http: Mutex::new(None) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // The state holds only caches; a panic elsewhere must not lock the area for good.
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn http(&self) -> Result<Arc<Http>, TvError> {
        let mut slot = self.http.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_none() {
            *slot = Some(Arc::new(Http::new(self.cfg.timeout, self.cfg.local_test)?));
        }
        slot.clone().ok_or_else(|| TvError::new(ErrorCode::ThingiverseOffline))
    }

    /// Drops every cached result and image and the request history. Called when the
    /// area is switched off, the key is removed or replaced, and when the program ends.
    pub fn clear(&self) {
        let mut st = self.lock();
        st.cache.clear();
        st.limiter.clear();
    }

    /// Bytes held by the cache (for tests and measurements).
    pub fn cached_bytes(&self) -> usize {
        self.lock().cache.total_bytes()
    }

    fn cached(&self, key: &Key) -> Option<Cached> {
        let now = (self.clock)();
        self.lock().cache.get(key, now)
    }

    fn store(&self, key: Key, value: Cached, bytes: usize) {
        let now = (self.clock)();
        self.lock().cache.put(key, value, bytes, now);
    }

    fn remember_refs(&self, id: u64, refs: ThingRefs) {
        let bytes = 64 + [&refs.thumb, &refs.large, &refs.page].iter().filter_map(|r| r.as_ref()).map(String::len).sum::<usize>();
        self.store(Key::Refs(id), Cached::Refs(refs), bytes);
    }

    /// One request through limiter and gate. The limiter may delay it (never past
    /// a few seconds for a single call) or refuse it without any network use.
    fn send(&self, url: reqwest::Url, token: Option<&Token>, limit: u64, kind: &'static str, gate: Gate) -> Result<Reply, TvError> {
        let slot = {
            let now = (self.clock)();
            self.lock().limiter.reserve(now)
        };
        let wait = match slot {
            Slot::After(w) => w,
            Slot::Full { retry_after_s } => return Err(TvError::rate_limited(retry_after_s.ceil() as u64)),
        };
        gate()?;
        if wait > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(wait));
        }
        self.http()?.get(url, token, limit, kind)
    }

    fn get_json(&self, url: reqwest::Url, token: &Token, kind: &'static str, gate: Gate) -> Result<Value, TvError> {
        let reply = self.send(url, Some(token), MAX_JSON_BYTES, kind, gate)?;
        classify(&reply, true)?;
        serde_json::from_slice(&reply.body).map_err(|_| TvError::new(ErrorCode::ThingiverseBadResponse))
    }

    /// Checks a key against the service and returns the user name. Not cached.
    pub fn test_token(&self, token: &Token, gate: Gate) -> Result<String, TvError> {
        let body = self.get_json(parse::me_url(&self.cfg.base_url)?, token, "me", gate)?;
        parse::parse_user_name(&body)
    }

    pub fn search(&self, token: &Token, query: &str, page: u32, gate: Gate) -> Result<ThingiverseSearchDto, TvError> {
        let query = parse::validate_query(query)?;
        let page = page.max(1);
        let key = Key::Search { query: query.clone(), page };
        if let Some(Cached::Search(hit)) = self.cached(&key) {
            return Ok(hit);
        }
        let body = self.get_json(parse::search_url(&self.cfg.base_url, &query, page)?, token, "search", gate)?;
        let parsed = parse::parse_search(&body, page)?;
        let mut items = Vec::with_capacity(parsed.things.len());
        for t in parsed.things {
            self.remember_refs(t.summary.id, t.refs);
            items.push(t.summary);
        }
        let dto = ThingiverseSearchDto { items, has_more: parsed.has_more, page };
        let bytes = serde_json::to_string(&dto).map_or(4096, |s| s.len());
        self.store(key, Cached::Search(dto.clone()), bytes);
        Ok(dto)
    }

    pub fn thing(&self, token: &Token, id: u64, gate: Gate) -> Result<ThingDetailDto, TvError> {
        let key = Key::Thing(id);
        if let Some(Cached::Thing(hit)) = self.cached(&key) {
            return Ok(hit);
        }
        let body = self.get_json(parse::thing_url(&self.cfg.base_url, id)?, token, "thing", gate)?;
        let parsed = parse::parse_detail(&body)?;
        if parsed.detail.id != id {
            return Err(TvError::new(ErrorCode::ThingiverseBadResponse));
        }
        self.remember_refs(id, parsed.refs);
        let bytes = serde_json::to_string(&parsed.detail).map_or(8192, |s| s.len());
        self.store(key, Cached::Thing(parsed.detail.clone()), bytes);
        Ok(parsed.detail)
    }

    /// Preview image as a `data:` address, or `None` when no address is known for this
    /// thing (it was not in a result of this session) or it has no image. The
    /// interface passes only the number, never an address. The key is not sent: the
    /// image host has no use for it.
    pub fn image(&self, id: u64, size: ImageSize, gate: Gate) -> Result<Option<String>, TvError> {
        let key = Key::Image { id, large: size == ImageSize::Large };
        if let Some(Cached::Image(hit)) = self.cached(&key) {
            return Ok(Some(hit));
        }
        let Some(Cached::Refs(refs)) = self.cached(&Key::Refs(id)) else { return Ok(None) };
        let address = match size {
            ImageSize::Thumb => refs.thumb,
            ImageSize::Large => refs.large.or(refs.thumb),
        };
        let Some(address) = address else { return Ok(None) };
        let url = reqwest::Url::parse(&address).map_err(|_| TvError::new(ErrorCode::ThingiverseBadResponse))?;
        if !self.image_host_ok(&url) {
            return Err(TvError::new(ErrorCode::ThingiverseBadResponse));
        }
        let reply = self.send(url, None, MAX_IMAGE_BYTES, "image", gate)?;
        classify(&reply, false)?;
        let mime = parse::sniff_image(&reply.body).ok_or_else(|| TvError::new(ErrorCode::ThingiverseBadResponse))?;
        let data = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&reply.body));
        self.store(key, Cached::Image(data.clone()), data.len());
        Ok(Some(data))
    }

    fn image_host_ok(&self, url: &reqwest::Url) -> bool {
        parse::image_host_allowed(url) || (self.cfg.local_test && url.host_str() == Some("127.0.0.1"))
    }

    /// The address to open in the browser for a thing seen in this session: the one
    /// the API named when it passes the check, else built from the number.
    pub fn page_url(&self, id: u64) -> String {
        let named = match self.cached(&Key::Refs(id)) {
            Some(Cached::Refs(r)) => r.page,
            _ => None,
        };
        parse::thing_page_url(id, named.as_deref())
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
