//! In-memory replay protection for write commands.
//!
//! A client that loses a response (timeout, dropped connection) cannot tell
//! whether the server applied the change. Re-sending the same request id lets
//! the server answer from the first execution instead of applying it twice.
//! The store is deliberately not persisted: it only has to bridge the short
//! window in which a client retries, and the server never migrates schemas.
use axum::{
    body::{Body, Bytes},
    http::{header, HeaderValue, StatusCode},
    response::Response,
};
use std::{
    collections::HashMap,
    hash::{BuildHasher, Hash, Hasher},
    sync::{Arc, LazyLock, Mutex},
};
use tokio::sync::watch;

pub(crate) const TTL_SECONDS: u64 = 600;
pub(crate) const MAX_ENTRIES: usize = 2000;
/// Bounds finished and running entries together; running ones are limited by admission anyway.
pub(crate) const MAX_PER_SESSION: usize = 200;
/// Finished entries one session may keep, so a single user cannot push everyone else's out.
pub(crate) const MAX_DONE_PER_SESSION: usize = 50;
/// Sum of stored answer bytes over all sessions.
pub(crate) const MAX_TOTAL_BODY_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_BODY_BYTES: usize = 64 * 1024;
pub(crate) const HEADER: &str = "x-request-id";

pub(crate) fn valid_key(key: &str) -> bool {
    (16..=64).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// Keyed per process so neither session tokens nor argument digests can be
// predicted or compared across restarts; raw values are never kept.
static HASHER: LazyLock<std::collections::hash_map::RandomState> =
    LazyLock::new(std::collections::hash_map::RandomState::new);

pub(crate) fn session_key(token: &str) -> u64 {
    let mut h = HASHER.build_hasher();
    0u8.hash(&mut h);
    token.hash(&mut h);
    h.finish()
}

pub(crate) fn args_hash(args: &serde_json::Value) -> u64 {
    let mut h = HASHER.build_hasher();
    1u8.hash(&mut h);
    args.to_string().hash(&mut h);
    h.finish()
}

/// Whether a finished answer may be replayed. Overload, timeout and server
/// errors say "nothing durable happened", so a retry has to run again. A 409
/// `not_started` is the same promise from the file operations.
pub(crate) fn storable(status: u16, body: &[u8]) -> bool {
    status < 500
        && !matches!(status, 408 | 429)
        && !(status == 409 && body.windows(11).any(|w| w == b"not_started"))
}

pub(crate) struct Stored {
    status: u16,
    content_type: Option<HeaderValue>,
    /// `None` when the answer was too large to keep.
    body: Option<Bytes>,
}

impl Stored {
    /// `None` when the body was too large to keep.
    pub fn to_response(&self) -> Option<Response> {
        let body = self.body.as_ref()?;
        let mut r = Response::new(Body::from(body.clone()));
        *r.status_mut() = StatusCode::from_u16(self.status).unwrap_or(StatusCode::OK);
        if let Some(ct) = &self.content_type {
            r.headers_mut().insert(header::CONTENT_TYPE, ct.clone());
        }
        r.headers_mut().insert("x-idempotent-replay", HeaderValue::from_static("1"));
        Some(r)
    }
}

enum State {
    /// Waiters hold a clone; it reports closed once the owner's guard is gone.
    InFlight(watch::Receiver<()>),
    Done(Arc<Stored>),
}
struct Entry {
    command: &'static str,
    args: u64,
    /// Completion time for `Done`, used for the TTL.
    at: u64,
    seq: u64,
    state: State,
}
#[derive(Default)]
struct Inner {
    map: HashMap<(u64, String), Entry>,
    seq: u64,
}
#[derive(Default)]
pub(crate) struct Store {
    inner: Mutex<Inner>,
}

pub(crate) enum Begin {
    Run(Guard),
    Wait(watch::Receiver<()>),
    Replay(Arc<Stored>),
    Reused,
    Full,
}

impl Store {
    pub fn begin(self: &Arc<Self>, session: u64, key: &str, command: &'static str, args: u64, now: u64) -> Begin {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.map.retain(|_, e| match e.state {
            State::InFlight(_) => true,
            State::Done(_) => now.saturating_sub(e.at) < TTL_SECONDS,
        });
        if let Some(e) = inner.map.get(&(session, key.to_owned())) {
            if e.command != command || e.args != args {
                return Begin::Reused;
            }
            return match &e.state {
                State::InFlight(rx) => Begin::Wait(rx.clone()),
                State::Done(s) => Begin::Replay(s.clone()),
            };
        }
        let session_count = inner.map.keys().filter(|(s, _)| *s == session).count();
        if session_count >= MAX_PER_SESSION && !inner.evict_oldest(Some(session)) {
            return Begin::Full;
        }
        if inner.map.len() >= MAX_ENTRIES && !inner.evict_oldest(None) {
            return Begin::Full;
        }
        inner.seq += 1;
        let seq = inner.seq;
        let (tx, rx) = watch::channel(());
        inner.map.insert(
            (session, key.to_owned()),
            Entry { command, args, at: now, seq, state: State::InFlight(rx) },
        );
        Begin::Run(Guard { store: self.clone(), key: (session, key.to_owned()), done: false, _tx: tx })
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().map.len()
    }
}
impl Inner {
    fn done_count(&self, session: Option<u64>) -> usize {
        self.map.iter().filter(|(k, e)| matches!(e.state, State::Done(_)) && session.is_none_or(|s| k.0 == s)).count()
    }
    fn body_bytes(&self) -> usize {
        self.map
            .values()
            .map(|e| match &e.state {
                State::Done(s) => s.body.as_ref().map_or(0, |b| b.len()),
                State::InFlight(_) => 0,
            })
            .sum()
    }
    /// In-flight entries are never evicted: their work is still running and a
    /// retry must find it instead of starting a second execution.
    fn evict_oldest(&mut self, session: Option<u64>) -> bool {
        let oldest = self
            .map
            .iter()
            .filter(|(k, e)| matches!(e.state, State::Done(_)) && session.is_none_or(|s| k.0 == s))
            .min_by_key(|(_, e)| e.seq)
            .map(|(k, _)| k.clone());
        oldest.is_some_and(|k| self.map.remove(&k).is_some())
    }
}

/// Owned by whoever runs the work. Dropping it unfinished removes the entry so
/// a retry starts fresh; dropping the sender wakes everyone waiting on it.
pub(crate) struct Guard {
    store: Arc<Store>,
    key: (u64, String),
    done: bool,
    _tx: watch::Sender<()>,
}
impl Guard {
    pub fn finish(mut self, status: u16, content_type: Option<HeaderValue>, body: Bytes, now: u64) {
        self.done = true;
        let mut inner = self.store.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !storable(status, &body) {
            inner.map.remove(&self.key);
            return;
        }
        inner.seq += 1;
        let seq = inner.seq;
        if let Some(e) = inner.map.get_mut(&self.key) {
            e.at = now;
            e.seq = seq;
            e.state = State::Done(Arc::new(Stored {
                status,
                content_type,
                body: (body.len() <= MAX_BODY_BYTES).then_some(body),
            }));
        }
        // The new entry has the highest sequence number, so it is evicted last.
        while inner.done_count(Some(self.key.0)) > MAX_DONE_PER_SESSION && inner.evict_oldest(Some(self.key.0)) {}
        while inner.body_bytes() > MAX_TOTAL_BODY_BYTES && inner.evict_oldest(None) {}
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if !self.done {
            self.store.inner.lock().unwrap_or_else(|e| e.into_inner()).map.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &Arc<Store>, sess: u64, key: &str, now: u64) -> Guard {
        match s.begin(sess, key, "add_printer", 7, now) {
            Begin::Run(g) => g,
            _ => panic!("expected to run"),
        }
    }
    fn ok(g: Guard, now: u64) {
        g.finish(200, None, Bytes::from_static(b"{}"), now);
    }

    #[test]
    fn key_format_is_16_to_64_url_safe_characters() {
        assert!(valid_key("abcdefghijklmnop"));
        assert!(valid_key(&"a-_Z9".repeat(12)));
        assert!(!valid_key("short"));
        assert!(!valid_key(&"a".repeat(65)));
        assert!(!valid_key("abcdefghijklmno!"));
        assert!(!valid_key("abcdefghijklmnö_"));
    }
    #[test]
    fn done_entry_replays_until_ttl_then_runs_again() {
        let s = Arc::new(Store::default());
        ok(run(&s, 1, "k", 100), 100);
        assert!(matches!(s.begin(1, "k", "add_printer", 7, 100 + TTL_SECONDS - 1), Begin::Replay(_)));
        assert!(matches!(s.begin(1, "k", "add_printer", 7, 100 + TTL_SECONDS), Begin::Run(_)));
    }
    #[test]
    fn same_key_with_other_command_or_arguments_is_reuse_and_other_session_is_separate() {
        let s = Arc::new(Store::default());
        ok(run(&s, 1, "k", 0), 0);
        assert!(matches!(s.begin(1, "k", "add_printer", 8, 1), Begin::Reused));
        assert!(matches!(s.begin(1, "k", "add_unit", 7, 1), Begin::Reused));
        assert!(matches!(s.begin(2, "k", "add_printer", 7, 1), Begin::Run(_)));
    }
    #[test]
    fn in_flight_entry_is_joined_and_removed_when_owner_gives_up() {
        let s = Arc::new(Store::default());
        let g = run(&s, 1, "k", 0);
        assert!(matches!(s.begin(1, "k", "add_printer", 7, 1), Begin::Wait(_)));
        drop(g);
        assert_eq!(s.len(), 0);
        assert!(matches!(s.begin(1, "k", "add_printer", 7, 2), Begin::Run(_)));
    }
    #[test]
    fn server_errors_and_overload_are_not_kept() {
        for status in [500, 503, 429, 408] {
            let s = Arc::new(Store::default());
            run(&s, 1, "k", 0).finish(status, None, Bytes::from_static(b"{}"), 0);
            assert_eq!(s.len(), 0, "{status}");
        }
        let s = Arc::new(Store::default());
        run(&s, 1, "k", 0).finish(409, None, Bytes::from_static(br#"{"kind":"not_started"}"#), 0);
        assert_eq!(s.len(), 0);
        run(&s, 1, "j", 0).finish(422, None, Bytes::from_static(b"{}"), 0);
        assert_eq!(s.len(), 1);
    }
    #[test]
    fn oversized_answer_is_kept_without_body_and_reports_unavailable() {
        let s = Arc::new(Store::default());
        run(&s, 1, "k", 0).finish(200, None, Bytes::from(vec![b'x'; MAX_BODY_BYTES + 1]), 0);
        let Begin::Replay(r) = s.begin(1, "k", "add_printer", 7, 1) else { panic!() };
        assert!(r.to_response().is_none());
        run(&s, 1, "small", 0).finish(200, None, Bytes::from(vec![b'x'; MAX_BODY_BYTES]), 0);
        let Begin::Replay(r) = s.begin(1, "small", "add_printer", 7, 1) else { panic!() };
        let r = r.to_response().unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["x-idempotent-replay"], "1");
    }
    #[test]
    fn per_session_and_total_caps_evict_oldest_done_but_never_in_flight() {
        let s = Arc::new(Store::default());
        for i in 0..MAX_DONE_PER_SESSION {
            ok(run(&s, 1, &format!("k{i}"), 0), 0);
        }
        ok(run(&s, 1, "extra", 0), 0);
        assert_eq!(s.len(), MAX_DONE_PER_SESSION);
        assert!(matches!(s.begin(1, "k0", "add_printer", 7, 0), Begin::Run(_)));
        let s = Arc::new(Store::default());
        let _flying: Vec<_> = (0..MAX_PER_SESSION).map(|i| run(&s, 1, &format!("f{i}"), 0)).collect();
        assert!(matches!(s.begin(1, "one-more", "add_printer", 7, 0), Begin::Full));
        let s = Arc::new(Store::default());
        for i in 0..MAX_ENTRIES {
            ok(run(&s, (i / MAX_DONE_PER_SESSION) as u64, &format!("k{i}"), 0), 0);
        }
        ok(run(&s, 999, "new", 0), 0);
        assert_eq!(s.len(), MAX_ENTRIES);
        assert!(matches!(s.begin(0, "k0", "add_printer", 7, 0), Begin::Run(_)));
    }
    #[test]
    fn finished_entries_per_session_are_capped_at_the_oldest_first() {
        let s = Arc::new(Store::default());
        for i in 0..MAX_DONE_PER_SESSION + 5 {
            ok(run(&s, 1, &format!("k{i}"), 0), 0);
        }
        ok(run(&s, 2, "other", 0), 0);
        assert_eq!(s.len(), MAX_DONE_PER_SESSION + 1);
        assert!(matches!(s.begin(2, "other", "add_printer", 7, 0), Begin::Replay(_)));
        assert!(matches!(s.begin(1, &format!("k{}", MAX_DONE_PER_SESSION + 4), "add_printer", 7, 0), Begin::Replay(_)));
        assert!(matches!(s.begin(1, "k0", "add_printer", 7, 0), Begin::Run(_)));
    }
    #[test]
    fn stored_bytes_are_capped_over_all_sessions_oldest_first() {
        let s = Arc::new(Store::default());
        let big = || Bytes::from(vec![b'x'; MAX_BODY_BYTES]);
        let fits = MAX_TOTAL_BODY_BYTES / MAX_BODY_BYTES;
        // 50 entries per session keep the per-session cap out of the way.
        for i in 0..fits + 3 {
            run(&s, (i / 40) as u64, &format!("k{i}"), 0).finish(200, None, big(), 0);
        }
        assert_eq!(s.len(), fits);
        assert!(s.inner.lock().unwrap().body_bytes() <= MAX_TOTAL_BODY_BYTES);
        assert!(matches!(s.begin(((fits + 2) / 40) as u64, &format!("k{}", fits + 2), "add_printer", 7, 0), Begin::Replay(_)));
        assert!(matches!(s.begin(0, "k0", "add_printer", 7, 0), Begin::Run(_)));
    }
}
