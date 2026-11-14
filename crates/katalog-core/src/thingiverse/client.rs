//! The HTTP side: one GET with a hard total time limit, a size limit and no leak of
//! the address, the search term or the key into errors or logs.
//!
//! Only `GET` exists here; the app never writes to Thingiverse.

use std::io::Read;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::token::Token;
use super::TvError;
use crate::error_code::ErrorCode;

pub(super) const MAX_JSON_BYTES: u64 = 1024 * 1024;
pub(super) const MAX_IMAGE_BYTES: u64 = 512 * 1024;
const MAX_REDIRECTS: usize = 3;

/// A finished answer. A body is read only for 2xx; an error status needs none, so
/// whatever the server echoes back (the key included) is never read.
#[derive(Debug)]
pub(super) struct Reply {
    pub status: u16,
    pub content_type: Option<String>,
    pub retry_after: Option<String>,
    /// Header of the protection service (`cf-ray`, `cf-mitigated`) is present.
    pub protection_service: bool,
    pub body: Vec<u8>,
}

pub(super) struct Http {
    client: reqwest::blocking::Client,
    timeout: Duration,
}

impl Http {
    /// `local_test`: tests may follow redirects to the stub on the loopback address.
    pub fn new(timeout: Duration, local_test: bool) -> Result<Http, TvError> {
        let policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                return attempt.stop();
            }
            let ok = if local_test {
                attempt.url().host_str() == Some("127.0.0.1")
            } else {
                super::parse::image_host_allowed(attempt.url())
            };
            if ok { attempt.follow() } else { attempt.stop() }
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout.min(Duration::from_secs(8)))
            // A descriptive agent; anonymous clients are more likely to be filtered.
            .user_agent(concat!("3MF-Katalog-Manager/", env!("CARGO_PKG_VERSION")))
            .redirect(policy)
            .build()
            .map_err(|_| TvError::new(ErrorCode::ThingiverseOffline))?;
        Ok(Http { client, timeout })
    }

    /// GET with a total limit of `self.timeout`. reqwest gives every single `read()` a
    /// fresh time limit, so the request runs on a helper thread and the caller only
    /// waits with `recv_timeout`; a thread that hangs on finishes later into the void.
    ///
    /// `kind` names the request for the log ("search", "thing", "image"). The log
    /// never gets the address: it holds the search term.
    pub fn get(&self, url: reqwest::Url, token: Option<&Token>, limit: u64, kind: &'static str) -> Result<Reply, TvError> {
        let client = self.client.clone();
        let timeout = self.timeout;
        let auth = match token {
            Some(t) => {
                let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", t.expose()))
                    .map_err(|_| TvError::new(ErrorCode::ThingiverseTokenFormat))?;
                // Keeps the key out of debug output of the request.
                value.set_sensitive(true);
                Some(value)
            }
            None => None,
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(fetch(&client, url, auth, limit, timeout, kind));
        });
        rx.recv_timeout(timeout).unwrap_or_else(|_| Err(TvError::new(ErrorCode::ThingiverseTimeout)))
    }
}

fn network_error(e: &reqwest::Error) -> TvError {
    // The text of a reqwest error contains the address; only its kind is used.
    TvError::new(if e.is_timeout() { ErrorCode::ThingiverseTimeout } else { ErrorCode::ThingiverseOffline })
}

fn fetch(
    client: &reqwest::blocking::Client,
    url: reqwest::Url,
    auth: Option<reqwest::header::HeaderValue>,
    limit: u64,
    timeout: Duration,
    kind: &'static str,
) -> Result<Reply, TvError> {
    let deadline = Instant::now() + timeout;
    let mut request = client.get(url);
    if let Some(auth) = auth {
        request = request.header(reqwest::header::AUTHORIZATION, auth);
    }
    let mut resp = request.send().map_err(|e| network_error(&e))?;
    let status = resp.status().as_u16();
    log::debug!(target: "thingiverse", "GET {kind} -> {status}");
    let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let mut reply = Reply {
        status,
        content_type: header("content-type"),
        retry_after: header("retry-after"),
        protection_service: resp.headers().contains_key("cf-ray") || resp.headers().contains_key("cf-mitigated"),
        body: Vec::new(),
    };
    if !(200..300).contains(&status) {
        return Ok(reply);
    }
    let mut chunk = [0u8; 8192];
    loop {
        if Instant::now() >= deadline {
            return Err(TvError::new(ErrorCode::ThingiverseTimeout));
        }
        let n = resp.read(&mut chunk).map_err(|e| {
            TvError::new(if e.kind() == std::io::ErrorKind::TimedOut { ErrorCode::ThingiverseTimeout } else { ErrorCode::ThingiverseOffline })
        })?;
        if n == 0 {
            break;
        }
        reply.body.extend_from_slice(&chunk[..n]);
        if reply.body.len() as u64 > limit {
            return Err(TvError::new(ErrorCode::ThingiverseBadResponse));
        }
    }
    Ok(reply)
}

/// Turns the status and the kind of content into an error code, in this order: status
/// (401, 403, 404, 429, anything else not 2xx), then the content type of a good answer.
pub(super) fn classify(reply: &Reply, expect_json: bool) -> Result<(), TvError> {
    let html = reply.content_type.as_deref().is_some_and(|c| c.to_ascii_lowercase().contains("text/html"));
    match reply.status {
        200..=299 => {
            if expect_json && html {
                return Err(TvError::new(ErrorCode::ThingiverseBadResponse));
            }
            Ok(())
        }
        401 => Err(TvError::new(ErrorCode::ThingiverseTokenInvalid)),
        // A 403 from the protection service is an HTML page, not the API's JSON.
        403 if html || reply.protection_service => Err(TvError::new(ErrorCode::ThingiverseBlocked)),
        403 => Err(TvError::new(ErrorCode::ThingiverseTokenInvalid)),
        404 => Err(TvError::new(ErrorCode::ThingiverseNotFound)),
        429 => Err(TvError::rate_limited(retry_after_seconds(reply.retry_after.as_deref()))),
        _ => Err(TvError::new(ErrorCode::ThingiverseBadResponse)),
    }
}

/// `Retry-After` in whole seconds; 60 without a usable value, at most an hour.
fn retry_after_seconds(value: Option<&str>) -> u64 {
    value.and_then(|v| v.trim().parse::<u64>().ok()).map_or(60, |s| s.clamp(1, 3600))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(status: u16, content_type: Option<&str>, cf: bool, retry: Option<&str>) -> Reply {
        Reply {
            status,
            content_type: content_type.map(str::to_string),
            retry_after: retry.map(str::to_string),
            protection_service: cf,
            body: Vec::new(),
        }
    }

    fn code(r: &Reply) -> Option<ErrorCode> {
        classify(r, true).err().map(|e| e.code)
    }

    #[test]
    fn statuses_map_to_codes() {
        assert_eq!(code(&reply(200, Some("application/json"), false, None)), None);
        assert_eq!(code(&reply(401, None, false, None)), Some(ErrorCode::ThingiverseTokenInvalid));
        assert_eq!(code(&reply(403, Some("application/json"), false, None)), Some(ErrorCode::ThingiverseTokenInvalid));
        assert_eq!(code(&reply(403, Some("text/html; charset=UTF-8"), false, None)), Some(ErrorCode::ThingiverseBlocked));
        assert_eq!(code(&reply(403, Some("application/json"), true, None)), Some(ErrorCode::ThingiverseBlocked));
        assert_eq!(code(&reply(404, None, false, None)), Some(ErrorCode::ThingiverseNotFound));
        assert_eq!(code(&reply(500, None, false, None)), Some(ErrorCode::ThingiverseBadResponse));
        assert_eq!(code(&reply(200, Some("text/html"), false, None)), Some(ErrorCode::ThingiverseBadResponse));
        assert_eq!(classify(&reply(200, Some("image/png"), false, None), false), Ok(()));
    }

    #[test]
    fn retry_after_is_read_with_defaults_and_bounds() {
        let secs = |v: Option<&str>| classify(&reply(429, None, false, v), true).unwrap_err().retry_after_s;
        assert_eq!(secs(Some("30")), Some(30));
        assert_eq!(secs(None), Some(60));
        assert_eq!(secs(Some("soon")), Some(60));
        assert_eq!(secs(Some("0")), Some(1));
        assert_eq!(secs(Some("999999")), Some(3600));
    }
}
