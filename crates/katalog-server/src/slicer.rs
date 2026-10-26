//! "Open in slicer": short-lived, signed download links.
//!
//! A slicer is a separate program that cannot present the browser session, so
//! the link itself is the credential. It is therefore a bearer token: it lives
//! only briefly, is bound to one file and to the state of that file on disk,
//! and is never extended. The signing key exists only in memory; a restart
//! invalidates every link, which is acceptable for something this short-lived.
use crate::download::{byte_range, disposition, http_date, DownloadBody};
use axum::{
    body::Body,
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use katalog_core::{db, safe_file};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{File, Metadata},
    io::{Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

/// Name of the session command that mints a link.
pub const COMMAND: &str = "slicer_link";
/// URL prefix of the download route; it lives outside `/api` on purpose.
pub const PREFIX: &str = "/dl/";
pub const DEFAULT_TTL_SECONDS: u64 = 3600;
pub const MIN_TTL_SECONDS: u64 = 60;
pub const MAX_TTL_SECONDS: u64 = 86400;

const PURPOSE: &str = "slicer-download";
const REVISION_HEX: usize = 16;
const SIGNATURE_LEN: usize = 43;
const LINKS_PER_MINUTE: u32 = 60;
const BAD_ATTEMPTS_PER_MINUTE: u32 = 20;
const WINDOW_SECONDS: u64 = 60;
const MAX_TRACKED: usize = 10_000;
const CONCURRENT_DOWNLOADS: usize = 4;

#[derive(Default)]
struct WindowState {
    counts: HashMap<String, (u64, u32)>,
    /// Second of the last sweep: stale keys are removed at most once per second.
    swept: u64,
}
type Windows = Mutex<WindowState>;

/// Where the catalog and its files live.
pub(crate) struct Catalog {
    pub db_path: PathBuf,
    pub roots: Vec<PathBuf>,
}

pub(crate) struct Slicer {
    /// Replaced on a password change; locked only for the copy, never across an await.
    key: Mutex<[u8; 32]>,
    pub ttl: u64,
    slots: Arc<Semaphore>,
    /// Whole-transfer and no-progress limits of a download body.
    body_limits: BodyLimits,
    bad_attempts: Windows,
    mints: Windows,
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    // RFC 2104 for keys up to the block size; ours is always 32 bytes.
    assert!(key.len() <= 64);
    let mut inner_pad = [0x36u8; 64];
    let mut outer_pad = [0x5cu8; 64];
    for (i, byte) in key.iter().enumerate() {
        inner_pad[i] ^= byte;
        outer_pad[i] ^= byte;
    }
    let inner = Sha256::new().chain_update(inner_pad).chain_update(message).finalize();
    Sha256::new().chain_update(outer_pad).chain_update(inner).finalize().into()
}

/// Counts events per key in a fixed window. `Err` carries the seconds until the
/// window ends once the limit is used up. A full table stops adding keys; the
/// per-key limits already bound each source, so this only caps memory.
fn hit(windows: &Windows, key: &str, now: u64, limit: u32) -> Result<(), u64> {
    let mut state = windows.lock().unwrap();
    if state.counts.len() >= MAX_TRACKED && state.swept != now {
        state.swept = now;
        state.counts.retain(|_, (start, _)| now.saturating_sub(*start) < WINDOW_SECONDS);
    }
    if !state.counts.contains_key(key) && state.counts.len() >= MAX_TRACKED {
        return Ok(());
    }
    let entry = state.counts.entry(key.to_owned()).or_insert((now, 0));
    if now.saturating_sub(entry.0) >= WINDOW_SECONDS {
        *entry = (now, 0);
    }
    if entry.1 >= limit {
        return Err((entry.0 + WINDOW_SECONDS).saturating_sub(now).max(1));
    }
    entry.1 += 1;
    Ok(())
}

fn limited(seconds: u64) -> Response {
    let mut response = crate::web::error(429, "Zu viele Anfragen. Bitte später erneut versuchen.");
    response.headers_mut().insert("retry-after", seconds.to_string().parse().unwrap());
    response
}

/// State of the file that a link was made for. Any change to the file on disk
/// changes this value, so an older link stops working.
///
/// Limit: the value only covers inode, size, modification time and the catalog's
/// content hash. Putting the same file back (restoring it from the trash by
/// rename) or replacing it in place with identical size and modification time
/// leaves all of these unchanged, so a link issued earlier works again until it
/// expires.
fn revision(meta: &Metadata, content_hash: Option<&str>) -> String {
    let nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", meta.dev(), meta.ino())
    };
    #[cfg(not(unix))]
    let identity = String::new();
    let digest = Sha256::digest(format!(
        "rev1|{}|{nanos}|{identity}|{}",
        meta.len(),
        content_hash.unwrap_or("")
    ));
    digest.iter().take(REVISION_HEX / 2).map(|b| format!("{b:02x}")).collect()
}

/// Opens a catalog file under the same admission rules as the normal download:
/// canonical path below a root, regular file, and the opened handle must be the
/// file that was admitted (no swap for a symlink in between).
fn open_admitted(roots: &[PathBuf], path: &str) -> Option<(File, Metadata)> {
    let canonical = PathBuf::from(path).canonicalize().ok()?;
    let inside = roots
        .iter()
        .any(|root| root.canonicalize().is_ok_and(|root| canonical.starts_with(root)));
    if !inside {
        return None;
    }
    let admitted = std::fs::symlink_metadata(&canonical).ok()?;
    if !admitted.is_file() {
        return None;
    }
    let file = safe_file::open_regular(&canonical).ok()?;
    let opened = file.metadata().ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if admitted.dev() != opened.dev() || admitted.ino() != opened.ino() {
            return None;
        }
    }
    Some((file, opened))
}

/// Last URL segment shown by the slicer. Slicers display it without decoding
/// it again, so it is restricted to characters that never need escaping. The
/// server ignores it; the real name travels in Content-Disposition.
pub(crate) fn url_name(name: &str, file_type: &str) -> String {
    const SAFE: &str = "-._~()";
    let clean = |text: &str| -> String {
        let mapped: String = text
            .chars()
            .flat_map(|c| match c {
                'ß' => "ss".chars().collect::<Vec<_>>(),
                'æ' => "ae".chars().collect(),
                'Æ' => "AE".chars().collect(),
                'ø' => vec!['o'],
                'Ø' => vec!['O'],
                'đ' | 'ð' => vec!['d'],
                'Đ' | 'Ð' => vec!['D'],
                'þ' => "th".chars().collect(),
                'Þ' => "Th".chars().collect(),
                'œ' => "oe".chars().collect(),
                'Œ' => "OE".chars().collect(),
                'ł' => vec!['l'],
                'Ł' => vec!['L'],
                'ı' => vec!['i'],
                c => vec![c],
            })
            .collect();
        let mut out = String::new();
        for c in mapped.nfkd().filter(|c| !is_combining_mark(*c)) {
            let c = if c.is_ascii_alphanumeric() || SAFE.contains(c) { c } else { '_' };
            if c == '_' && out.ends_with('_') {
                continue;
            }
            out.push(c);
        }
        out
    };
    // Like a path splitter: leading dots belong to the stem, not to an extension.
    let lead = name.len() - name.trim_start_matches('.').len();
    let (stem, ext) = match name[lead..].rfind('.') {
        Some(i) => name.split_at(lead + i),
        None => (name, ""),
    };
    let stem: String = clean(stem)
        .trim_matches(['_', '.'])
        .chars()
        .take(100)
        .collect();
    let stem = stem.trim_end_matches(['_', '.']);
    let ext = clean(ext);
    let is_model = matches!(ext.to_ascii_lowercase().as_str(), ".3mf" | ".stl" | ".stp" | ".step" | ".obj");
    // A name without a model extension still has to look like one to the slicer.
    let ext = if is_model { ext } else { format!("{ext}.{file_type}") };
    format!("{}{ext}", if stem.is_empty() { "model" } else { stem })
}

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.len() <= 18 && text.bytes().all(|b| b.is_ascii_digit())
}

/// A file id as logged: only plain numbers, never attacker-controlled text.
fn loggable(id: &str) -> &str {
    if all_digits(id) { id } else { "-" }
}

/// One line per request. It carries no link, signature or path.
fn audit_line(now: u64, id: &str, status: u16, size: u64) -> String {
    format!("slicer_dl time={now} file_id={} status={status} size={size}", loggable(id))
}

fn audit(now: u64, id: &str, status: u16, size: u64) {
    eprintln!("{}", audit_line(now, id, status, size));
}

impl Slicer {
    pub fn new(ttl: u64) -> Result<Self, String> {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|_| "Zufallsquelle nicht verfügbar".to_string())?;
        Ok(Self::with_key(key, ttl))
    }
    pub fn with_key(key: [u8; 32], ttl: u64) -> Self {
        Self {
            key: Mutex::new(key),
            body_limits: BodyLimits::default(),
            ttl,
            slots: Arc::new(Semaphore::new(CONCURRENT_DOWNLOADS)),
            bad_attempts: Mutex::default(),
            mints: Mutex::default(),
        }
    }

    /// Draws a new key so that every link issued so far stops working. Used when
    /// the password changes: whoever held a link of the old login must not keep access.
    pub fn rotate(&self) {
        let mut fresh = [0u8; 32];
        let mut key = self.key.lock().unwrap();
        if getrandom::fill(&mut fresh).is_err() {
            // No randomness: derive the next key from the current secret instead,
            // which an outsider cannot compute, rather than keeping the old one.
            fresh = hmac_sha256(&*key, b"rotate");
        }
        *key = fresh;
    }

    fn sign(&self, id: i64, revision: &str, expires: u64) -> String {
        let message = format!("v1|{id}|{revision}|{expires}|{PURPOSE}");
        URL_SAFE_NO_PAD.encode(hmac_sha256(&{ *self.key.lock().unwrap() }, message.as_bytes()))
    }

    /// `<expiry>.<revision>.<signature>`: only characters that are valid in a
    /// path segment without escaping.
    fn token(&self, id: i64, revision: &str, expires: u64) -> String {
        format!("{expires}.{revision}.{}", self.sign(id, revision, expires))
    }

    /// Checks the signature only; expiry and file state are separate steps so
    /// an unsigned guess never reaches the catalog.
    fn verify(&self, id: i64, token: &str) -> Option<(u64, String)> {
        let mut parts = token.split('.');
        let (expires, revision, signature) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some()
            || !all_digits(expires)
            || revision.len() != REVISION_HEX
            || !revision.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || signature.len() != SIGNATURE_LEN
        {
            return None;
        }
        let expires: u64 = expires.parse().ok()?;
        let expected = self.sign(id, revision, expires);
        bool::from(expected.as_bytes().ct_eq(signature.as_bytes())).then(|| (expires, revision.to_owned()))
    }
}

fn plain(status: u16, message: &str) -> Response {
    let mut response = crate::web::error(status, message);
    response.headers_mut().insert("cache-control", "no-store".parse().unwrap());
    response
}

fn numeric_id(args: &Value) -> Option<i64> {
    let object = args.as_object().filter(|o| o.len() == 1)?;
    let text = object.get("fileId")?.as_str()?;
    if !all_digits(text) {
        return None;
    }
    text.parse().ok()
}

/// Session command: mints a link for one file in the catalog.
pub(crate) async fn mint(
    slicer: &Slicer,
    Catalog { db_path, roots }: Catalog,
    rate_key: &str,
    now: u64,
    args: &Value,
) -> Response {
    let Some(id) = numeric_id(args) else {
        return crate::web::error(400, "ID muss eine numerische Zeichenfolge sein");
    };
    if let Err(seconds) = hit(&slicer.mints, rate_key, now, LINKS_PER_MINUTE) {
        return limited(seconds);
    }
    let found = tokio::task::spawn_blocking(move || {
        let conn = db::open_read_only(&db_path).ok()?;
        let file = db::get_file(&conn, id).ok()??;
        drop(conn);
        // The trash holds files that are removed from the catalog's view.
        if file.deleted_at.is_some() {
            return None;
        }
        let (_handle, meta) = open_admitted(&roots, file.current_path())?;
        Some((revision(&meta, file.content_hash.as_deref()), file))
    })
    .await;
    let Ok(Some((revision, file))) = found else {
        return plain(404, "Datei nicht gefunden");
    };
    let expires = now + slicer.ttl;
    let name = url_name(&file.name, file.file_type.as_str());
    let path = format!("{PREFIX}{id}/{}/{name}", slicer.token(id, &revision, expires));
    log::info!(target: "slicer", "Slicer-Link erstellt");
    eprintln!("slicer_link time={now} file_id={id} status=200");
    let mut response = Json(json!({"path": path, "expiresAt": expires, "name": file.name})).into_response();
    response.headers_mut().insert("cache-control", "no-store".parse().unwrap());
    response
}

/// Unauthenticated route behind the signature. All failures that could help a
/// guesser look the same; only a correctly signed but outdated link learns more.
pub(crate) async fn download(
    slicer: &Slicer,
    Catalog { db_path, roots }: Catalog,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
    client: &str,
    now: u64,
) -> Response {
    if *method != Method::GET && *method != Method::HEAD {
        return plain(405, "Methode nicht erlaubt");
    }
    let segments: Vec<&str> = path.strip_prefix(PREFIX).unwrap_or("").split('/').collect();
    // Only rejected requests count and only they are throttled: a correctly signed
    // link must keep working for a client whose address also sent guesses (shared
    // NAT, proxy), otherwise anyone could lock a user out by guessing from there.
    let refuse = |id: &str| match hit(&slicer.bad_attempts, client, now, BAD_ATTEMPTS_PER_MINUTE) {
        Err(seconds) => limited(seconds),
        Ok(()) => {
            audit(now, id, 404, 0);
            plain(404, "Nicht gefunden")
        }
    };
    let [id_text, token, name] = segments[..] else { return refuse("-") };
    let Some(id) = all_digits(id_text).then(|| id_text.parse::<i64>().ok()).flatten().filter(|id| id.to_string() == id_text)
    else {
        return refuse("-");
    };
    if name.is_empty() {
        return refuse(id_text);
    }
    let Some((expires, wanted_revision)) = slicer.verify(id, token) else { return refuse(id_text) };
    // `now` is wall-clock time: setting the server clock back extends links that exist at that moment.
    if expires <= now {
        audit(now, id_text, 410, 0);
        return plain(410, "Link abgelaufen");
    }
    let permit = match tokio::time::timeout(Duration::from_millis(250), slicer.slots.clone().acquire_owned()).await {
        Ok(Ok(permit)) => permit,
        _ => {
            let mut response = plain(503, "Zu viele Downloads");
            response.headers_mut().insert("retry-after", "1".parse().unwrap());
            return response;
        }
    };
    // The permit also covers the blocking open, so a dropped request cannot
    // free a slot while its file is still being opened.
    let opened = tokio::task::spawn_blocking(move || {
        let conn = db::open_read_only(&db_path).ok()?;
        let file = db::get_file(&conn, id).ok()??;
        drop(conn);
        if file.deleted_at.is_some() {
            return None;
        }
        let (handle, meta) = open_admitted(&roots, file.current_path())?;
        let current = revision(&meta, file.content_hash.as_deref());
        bool::from(current.as_bytes().ct_eq(wanted_revision.as_bytes())).then_some((handle, meta, file.name, permit))
    })
    .await;
    let Ok(Some((mut file, meta, stored_name, permit))) = opened else {
        audit(now, id_text, 404, 0);
        return plain(404, "Nicht gefunden");
    };
    let len = meta.len();
    let modified = meta.modified().ok().and_then(http_date);
    let if_range_matches = headers.get_all("if-range").iter().count() <= 1
        && headers.get("if-range").is_none_or(|value| {
            value.to_str().ok().zip(modified.as_deref()).is_some_and(|(a, b)| a == b)
        });
    let range = if !if_range_matches || headers.get_all("range").iter().count() > 1 {
        Ok(None)
    } else {
        headers.get("range").map_or(Ok(None), |v| v.to_str().ok().map_or(Ok(None), |s| byte_range(s, len)))
    };
    let range = match range {
        Ok(range) => range,
        Err(()) => {
            let mut response = plain(416, "Ungültiger Bytebereich");
            response.headers_mut().insert("content-range", format!("bytes */{len}").parse().unwrap());
            return response;
        }
    };
    let (start, count) = range.unwrap_or((0, len));
    if start != 0 && file.seek(SeekFrom::Start(start)).is_err() {
        return plain(500, "Interner Fehler");
    }
    let mut response = if *method == Method::HEAD {
        // The body is never sent for HEAD; release the slot and the file now.
        drop((file, permit));
        Body::empty().into_response()
    } else {
        Body::new(GuardedBody::new(DownloadBody::new(tokio::fs::File::from_std(file), count, permit), slicer.body_limits)).into_response()
    };
    if range.is_some() {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        response
            .headers_mut()
            .insert("content-range", format!("bytes {start}-{}/{len}", start + count - 1).parse().unwrap());
    }
    for (key, value) in [
        ("content-type", "application/octet-stream".to_owned()),
        ("content-length", count.to_string()),
        ("content-disposition", disposition(&stored_name)),
        ("accept-ranges", "bytes".to_owned()),
        ("cache-control", "no-store".to_owned()),
        ("x-content-type-options", "nosniff".to_owned()),
    ] {
        response.headers_mut().insert(key, value.parse().unwrap());
    }
    if let Some(modified) = modified {
        response.headers_mut().insert("last-modified", modified.parse().unwrap());
    }
    log::info!(target: "slicer", "Slicer-Download bedient");
    audit(now, id_text, response.status().as_u16(), count);
    response
}

/// Limits for one download body.
#[derive(Clone, Copy)]
pub(crate) struct BodyLimits {
    /// The whole transfer.
    pub total: Duration,
    /// How long the reader may take no data before the transfer is dropped.
    pub idle: Duration,
}
impl Default for BodyLimits {
    fn default() -> Self {
        Self { total: Duration::from_secs(2 * 3600), idle: Duration::from_secs(120) }
    }
}

struct Guarded {
    inner: Option<DownloadBody>,
    timed_out: bool,
    last_progress: std::time::Instant,
}

/// Response body that a watchdog can end from outside. A reader that stops
/// reading is never polled again by the connection, so a timer inside `poll_frame`
/// would never run; the watchdog drops the inner body (and with it the download
/// slot and the file handle) on its own.
struct GuardedBody {
    shared: Arc<Mutex<Guarded>>,
}
impl GuardedBody {
    fn new(inner: DownloadBody, limits: BodyLimits) -> Self {
        let started = std::time::Instant::now();
        let shared = Arc::new(Mutex::new(Guarded { inner: Some(inner), timed_out: false, last_progress: started }));
        let watched = Arc::downgrade(&shared);
        let tick = (limits.idle.min(limits.total) / 4).clamp(Duration::from_millis(10), Duration::from_secs(5));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tick).await;
                let Some(shared) = watched.upgrade() else { return };
                let mut guarded = shared.lock().unwrap();
                if guarded.inner.is_none() {
                    return;
                }
                if started.elapsed() >= limits.total || guarded.last_progress.elapsed() >= limits.idle {
                    guarded.inner = None;
                    guarded.timed_out = true;
                    return;
                }
            }
        });
        Self { shared }
    }
}
impl hyper::body::Body for GuardedBody {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;
    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, std::io::Error>>> {
        use std::task::Poll;
        let mut guarded = self.shared.lock().unwrap();
        let Some(inner) = guarded.inner.as_mut() else {
            return Poll::Ready(guarded.timed_out.then(|| {
                Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Zeitlimit für den Download überschritten"))
            }));
        };
        let polled = std::pin::Pin::new(inner).poll_frame(cx);
        match &polled {
            Poll::Ready(Some(Ok(_))) => guarded.last_progress = std::time::Instant::now(),
            Poll::Ready(_) => guarded.inner = None,
            Poll::Pending => {}
        }
        polled
    }
    fn is_end_stream(&self) -> bool {
        self.shared.lock().unwrap().inner.as_ref().is_none_or(hyper::body::Body::is_end_stream)
    }
    fn size_hint(&self) -> hyper::body::SizeHint {
        self.shared
            .lock()
            .unwrap()
            .inner
            .as_ref()
            .map_or_else(|| hyper::body::SizeHint::with_exact(0), hyper::body::Body::size_hint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn hmac_matches_rfc_4231_vectors() {
        assert_eq!(
            hex(hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn token_binds_file_revision_and_expiry() {
        let s = Slicer::with_key([7; 32], 3600);
        let token = s.token(5, "0123456789abcdef", 2000);
        assert_eq!(s.verify(5, &token), Some((2000, "0123456789abcdef".into())));
        assert_eq!(s.verify(6, &token), None);
        let other_expiry = token.replacen("2000", "2001", 1);
        assert_eq!(s.verify(5, &other_expiry), None);
        let other_revision = token.replacen("0123456789abcdef", "0123456789abcdee", 1);
        assert_eq!(s.verify(5, &other_revision), None);
        assert_eq!(Slicer::with_key([8; 32], 3600).verify(5, &token), None);
        for bad in ["", "a.b.c", "2000.0123456789abcdef", &format!("{token}.x")] {
            assert_eq!(s.verify(5, bad), None, "{bad}");
        }
    }

    #[test]
    fn token_uses_only_path_safe_characters() {
        let token = Slicer::with_key([1; 32], 60).token(1, "0123456789abcdef", 9);
        assert!(token.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)));
    }

    #[test]
    fn url_names_never_need_escaping_and_end_in_a_model_extension() {
        assert_eq!(url_name("Würfel groß.stl", "stl"), "Wurfel_gross.stl");
        assert_eq!(url_name("a+b (1)&x;y='z.3mf", "3mf"), "a_b_(1)_x_y_z.3mf");
        assert_eq!(url_name("..hidden", "obj"), "hidden.obj");
        assert_eq!(url_name("///", "stl"), "model.stl");
        assert_eq!(url_name("part.STEP", "stp"), "part.STEP");
        assert_eq!(url_name("report.pdf", "stl"), "report.pdf.stl");
        let long = url_name(&format!("{}.stl", "x".repeat(300)), "stl");
        assert_eq!(long.len(), 104);
        for name in [url_name("a/b\\c%d?e#f.stl", "stl"), url_name("日本語.stl", "stl")] {
            assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._~()".contains(&b)), "{name}");
        }
    }

    #[test]
    fn windows_limit_per_key_and_reset() {
        let windows = Windows::default();
        for _ in 0..3 {
            assert!(hit(&windows, "a", 100, 3).is_ok());
        }
        assert_eq!(hit(&windows, "a", 130, 3), Err(30));
        assert!(hit(&windows, "b", 130, 3).is_ok());
        assert!(hit(&windows, "a", 160, 3).is_ok());
    }

    #[test]
    fn stale_windows_are_swept_at_most_once_per_second() {
        let windows = Windows::default();
        for n in 0..MAX_TRACKED {
            assert!(hit(&windows, &n.to_string(), 100, 1).is_ok());
        }
        // Table full and nothing stale: new keys are not tracked, old ones still limited.
        assert!(hit(&windows, "new", 100, 1).is_ok());
        assert!(!windows.lock().unwrap().counts.contains_key("new"));
        assert_eq!(hit(&windows, "0", 100, 1), Err(60));
        // After the window, one sweep per second makes room again.
        assert!(hit(&windows, "late", 200, 1).is_ok());
        assert_eq!(windows.lock().unwrap().counts.len(), 1);
        assert_eq!(windows.lock().unwrap().swept, 200);
    }

    #[test]
    fn rotating_the_key_invalidates_existing_tokens() {
        let s = Slicer::with_key([3; 32], 3600);
        let token = s.token(5, "0123456789abcdef", 2000);
        assert!(s.verify(5, &token).is_some());
        s.rotate();
        assert!(s.verify(5, &token).is_none());
        assert!(s.verify(5, &s.token(5, "0123456789abcdef", 2000)).is_some());
    }

    async fn body_over(len: usize, slots: &Arc<Semaphore>, limits: BodyLimits) -> (GuardedBody, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("guarded-{}-{}", std::process::id(), len));
        std::fs::write(&path, vec![7u8; len]).unwrap();
        let permit = slots.clone().acquire_owned().await.unwrap();
        let file = tokio::fs::File::from_std(std::fs::File::open(&path).unwrap());
        (GuardedBody::new(DownloadBody::new(file, len as u64, permit), limits), path)
    }

    #[tokio::test]
    async fn a_reader_that_stops_reading_loses_the_slot() {
        use http_body_util::BodyExt;
        let slots = Arc::new(Semaphore::new(1));
        let limits = BodyLimits { total: Duration::from_secs(60), idle: Duration::from_millis(150) };
        let (mut body, path) = body_over(200_000, &slots, limits).await;
        assert!(body.frame().await.unwrap().is_ok(), "first frame is delivered");
        assert_eq!(slots.available_permits(), 0);
        // The connection no longer polls the body; the watchdog frees the slot anyway.
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(slots.available_permits(), 1);
        let ended = body.frame().await.unwrap();
        assert_eq!(ended.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn the_whole_transfer_has_a_deadline_even_with_steady_progress() {
        use http_body_util::BodyExt;
        let slots = Arc::new(Semaphore::new(1));
        let limits = BodyLimits { total: Duration::from_millis(200), idle: Duration::from_secs(60) };
        let (mut body, path) = body_over(64 * 1024 * 40, &slots, limits).await;
        let mut failed = false;
        for _ in 0..40 {
            match body.frame().await {
                Some(Ok(_)) => tokio::time::sleep(Duration::from_millis(30)).await,
                Some(Err(e)) => {
                    assert_eq!(e.kind(), std::io::ErrorKind::TimedOut);
                    failed = true;
                    break;
                }
                None => break,
            }
        }
        assert!(failed);
        assert_eq!(slots.available_permits(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn a_prompt_reader_gets_the_whole_body_and_the_slot_back() {
        use http_body_util::BodyExt;
        let slots = Arc::new(Semaphore::new(1));
        let (body, path) = body_over(150_000, &slots, BodyLimits::default()).await;
        assert_eq!(body.collect().await.unwrap().to_bytes().len(), 150_000);
        assert_eq!(slots.available_permits(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn audit_line_has_no_token_or_path() {
        let line = audit_line(5, "42", 200, 4096);
        assert_eq!(line, "slicer_dl time=5 file_id=42 status=200 size=4096");
        assert_eq!(audit_line(5, "../etc/passwd", 404, 0), "slicer_dl time=5 file_id=- status=404 size=0");
    }

    #[test]
    fn logged_ids_are_plain_numbers_only() {
        assert_eq!(loggable("42"), "42");
        assert_eq!(loggable("4 2\n"), "-");
        assert_eq!(loggable("99999999999999999999"), "-");
    }
}
