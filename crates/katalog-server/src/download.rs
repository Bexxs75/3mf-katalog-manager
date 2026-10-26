//! Original files: bounded concurrency and memory, no parser or size ceiling.
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use katalog_core::{
    db,
    read::{self, ReadError},
    safe_file,
};
use std::{
    io::{self, Seek, SeekFrom},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::{OwnedSemaphorePermit, Semaphore},
};

fn error(status: u16, message: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap(),
        Json(serde_json::json!({"message":message,"expected":status<500})),
    )
        .into_response()
}

pub async fn download(
    db_path: PathBuf,
    roots: Vec<PathBuf>,
    id: &str,
    headers: &HeaderMap,
    slots: Arc<Semaphore>,
) -> Response {
    let Ok(numeric_id) = id.parse::<i64>() else {
        return error(400, "ID muss eine numerische Zeichenfolge sein");
    };
    let permit = match tokio::time::timeout(Duration::from_millis(250), slots.acquire_owned()).await
    {
        Ok(Ok(p)) => p,
        _ => {
            let mut r = error(503, "Zu viele Downloads");
            r.headers_mut().insert("retry-after", "1".parse().unwrap());
            return r;
        }
    };
    let id = id.to_owned();
    // The permit also covers blocking setup: cancelling the request must not
    // release a slot while its file is still being opened in the blocking pool.
    let opened = tokio::task::spawn_blocking(move || {
        let conn = db::open_read_only(&db_path).map_err(ReadError::from)?;
        let source = read::load_geometry_source(&conn, &id)?;
        let name = db::get_file(&conn, numeric_id)
            .map_err(ReadError::from)?
            .ok_or_else(|| {
                ReadError::expected("file not found").with_code(read::GeometryErrorCode::NotFound)
            })?
            .name;
        drop(conn);
        let canonical = source
            .path
            .canonicalize()
            .map_err(read::geometry_io_error)?;
        let mut allowed = false;
        for root in roots {
            if root
                .canonicalize()
                .is_ok_and(|root| canonical.starts_with(root))
            {
                allowed = true;
            }
        }
        if !allowed {
            return Err(ReadError::expected(
                "Dateipfad liegt außerhalb der freigegebenen Verzeichnisse",
            )
            .with_code(read::GeometryErrorCode::Unreadable));
        }
        // Same path-admission guarantee as geometry; roots must not be writable
        // by untrusted users (concurrent directory replacement is not prevented).
        let admitted = std::fs::symlink_metadata(&canonical).map_err(read::geometry_io_error)?;
        if !admitted.is_file() {
            return Err(read::geometry_io_error(io::Error::new(
                io::ErrorKind::InvalidInput,
                "keine reguläre Datei",
            )));
        }
        let file = safe_file::open_regular(&canonical).map_err(read::geometry_io_error)?;
        let opened = file.metadata().map_err(read::geometry_io_error)?;
        // Reject a final path component replaced between admission and opening,
        // including replacement by a symlink. Never read from a mismatched handle.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if admitted.dev() != opened.dev() || admitted.ino() != opened.ino() {
                return Err(ReadError::from("Datei während des Öffnens ausgetauscht"));
            }
        }
        let len = opened.len();
        Ok((
            file,
            len,
            opened.modified().ok().and_then(http_date),
            name,
            permit,
        ))
    })
    .await;
    let (file, len, modified, name, permit) = match opened {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return crate::invoke::read_error(e),
        Err(_) => return error(500, "Interner Fehler"),
    };
    serve(file, len, modified, &name, "application/octet-stream", permit, headers)
}

/// Streams an opened regular file with range support. The type is chosen by the caller,
/// never sniffed; the file always goes out as an attachment.
fn serve(
    mut file: std::fs::File,
    len: u64,
    modified: Option<String>,
    name: &str,
    content_type: &str,
    permit: OwnedSemaphorePermit,
    headers: &HeaderMap,
) -> Response {
    // No entity tag is emitted. Only our exact Last-Modified date can match;
    // weak tags, unknown tags and malformed validators require a full response.
    let if_range_matches = headers.get_all("if-range").iter().count() <= 1
        && headers.get("if-range").is_none_or(|value| {
            value
                .to_str()
                .ok()
                .zip(modified.as_deref())
                .is_some_and(|(a, b)| a == b)
        });
    let range = if !if_range_matches || headers.get_all("range").iter().count() > 1 {
        Ok(None)
    } else {
        headers.get("range").map_or(Ok(None), |v| {
            v.to_str().ok().map_or(Ok(None), |s| byte_range(s, len))
        })
    };
    let range = match range {
        Ok(r) => r,
        Err(()) => {
            let mut r = error(416, "Ungültiger Bytebereich");
            r.headers_mut()
                .insert("content-range", format!("bytes */{len}").parse().unwrap());
            return r;
        }
    };
    let (start, count) = range.unwrap_or((0, len));
    // Seeking a regular file changes only the handle offset and does not read data.
    if start != 0 && file.seek(SeekFrom::Start(start)).is_err() {
        return error(500, "Interner Fehler");
    }
    let body = DownloadBody {
        file: tokio::fs::File::from_std(file),
        remaining: count,
        permit: Some(permit),
    };
    let mut response = Body::new(body).into_response();
    if range.is_some() {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        response.headers_mut().insert(
            "content-range",
            format!("bytes {start}-{}/{len}", start + count - 1)
                .parse()
                .unwrap(),
        );
    }
    for (key, value) in [
        ("content-type", content_type.into()),
        ("content-length", count.to_string()),
        ("content-disposition", disposition(name)),
        ("accept-ranges", "bytes".into()),
    ] {
        response.headers_mut().insert(key, value.parse().unwrap());
    }
    if let Some(modified) = modified {
        response
            .headers_mut()
            .insert("last-modified", modified.parse().unwrap());
    }
    response
}

/// One companion file of a model, by its row id. The path comes from the catalog row
/// only; the strict open (no links, one name, inside a root) is `open_for_web`.
pub async fn companion(
    db_path: PathBuf,
    roots: Vec<PathBuf>,
    id: &str,
    headers: &HeaderMap,
    slots: Arc<Semaphore>,
) -> Response {
    let Ok(numeric_id) = id.parse::<i64>() else {
        return error(400, "ID muss eine numerische Zeichenfolge sein");
    };
    let permit = match tokio::time::timeout(Duration::from_millis(250), slots.acquire_owned()).await {
        Ok(Ok(p)) => p,
        _ => {
            let mut r = error(503, "Zu viele Downloads");
            r.headers_mut().insert("retry-after", "1".parse().unwrap());
            return r;
        }
    };
    // The permit travels through the blocking closure: cancelling the request must not
    // release a slot while the file is still being opened in the blocking pool.
    let opened = tokio::task::spawn_blocking(move || {
        let conn = db::open_read_only(&db_path).map_err(ReadError::from)?;
        let opened = katalog_core::companion_read::open_for_web(
            &conn,
            numeric_id,
            &roots,
            katalog_core::companions::MAX_COMPANION_BYTES,
        );
        drop(conn);
        let (row, kind, mut file, _) = opened.map_err(crate::invoke::companion_error)?;
        // The content check read the start of the file; the answer begins there again.
        file.seek(SeekFrom::Start(0)).map_err(read::geometry_io_error)?;
        let meta = file.metadata().map_err(read::geometry_io_error)?;
        let content_type = katalog_core::companion_read::download_content_type(kind, &row.name);
        Ok::<_, ReadError>((file, meta.len(), meta.modified().ok().and_then(http_date), row.name, content_type, permit))
    })
    .await;
    let (file, len, modified, name, content_type, permit) = match opened {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return crate::invoke::read_error(e),
        Err(_) => return error(500, "Interner Fehler"),
    };
    serve(file, len, modified, &name, content_type, permit, headers)
}

pub(crate) fn byte_range(value: &str, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some((unit, value)) = value.split_once('=') else {
        return Ok(None);
    };
    if !unit.eq_ignore_ascii_case("bytes") || value.contains(',') {
        return Ok(None);
    }
    let Some((a, b)) = value.split_once('-') else {
        return Ok(None);
    };
    if (a.is_empty() && b.is_empty()) || !a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    // Decimal positions can exceed u64. Saturate after validating syntax so
    // huge suffixes/end positions still work and huge starts remain unsatisfiable.
    let number = |s: &str| -> u64 { s.parse().unwrap_or(u64::MAX) };
    if !a.is_empty() && !b.is_empty() {
        let a = a.trim_start_matches('0');
        let b = b.trim_start_matches('0');
        if (a.len(), a) > (b.len(), b) {
            return Ok(None);
        }
    }
    if len == 0 {
        return Err(());
    }
    if a.is_empty() {
        let count = number(b).min(len);
        if count == 0 {
            return Err(());
        }
        return Ok(Some((len - count, count)));
    }
    let start = number(a);
    let end = if b.is_empty() {
        len - 1
    } else {
        number(b).min(len - 1)
    };
    if start >= len {
        return Err(());
    }
    Ok(Some((start, end - start + 1)))
}

// IMF-fixdate in UTC, without adding a dependency to the locked server graph.
// Bound the year before conversion; filesystem timestamps can be out of range.
pub(crate) fn http_date(time: std::time::SystemTime) -> Option<String> {
    let seconds = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok()?,
        Err(e) => {
            -i64::try_from(e.duration().as_secs()).ok()?
                - i64::from(e.duration().subsec_nanos() != 0)
        }
    };
    if !(-11644473600..=253402300799).contains(&seconds) {
        return None;
    }
    let days = seconds.div_euclid(86400);
    let daytime = seconds.rem_euclid(86400);
    let weekday =
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][(days + 4).rem_euclid(7) as usize];
    let leap = |y: i64| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let mut year = 1601;
    let mut day = days + 134774;
    loop {
        let count = if leap(year) { 366 } else { 365 };
        if day < count {
            break;
        }
        day -= count;
        year += 1;
    }
    let lengths = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while day >= lengths[month] {
        day -= lengths[month];
        month += 1;
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][month];
    Some(format!(
        "{weekday}, {:02} {month} {year:04} {:02}:{:02}:{:02} GMT",
        day + 1,
        daytime / 3600,
        daytime / 60 % 60,
        daytime % 60
    ))
}

// Unicode General_Category=Cf: invisible formatting must not disguise a
// downloaded extension or reorder the filename shown by the browser.
fn is_format_control(c: char) -> bool {
    matches!(c, '\u{00ad}' | '\u{0600}'..='\u{0605}' | '\u{061c}' | '\u{06dd}'
        | '\u{070f}' | '\u{0890}'..='\u{0891}' | '\u{08e2}' | '\u{180e}'
        | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206f}' | '\u{feff}'
        | '\u{fff9}'..='\u{fffb}' | '\u{110bd}' | '\u{110cd}'
        | '\u{13430}'..='\u{1343f}' | '\u{1bca0}'..='\u{1bca3}'
        | '\u{1d173}'..='\u{1d17a}' | '\u{e0001}' | '\u{e0020}'..='\u{e007f}')
}

pub(crate) fn disposition(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| !c.is_control() && !is_format_control(*c) && !matches!(c, '/' | '\\' | '"'))
        .collect();
    let mut name = String::new();
    // Limit UTF-8 bytes, never split a character. Trim before limiting so dots
    // cannot consume the budget and leave an unusable basename.
    for c in clean.trim().trim_start_matches('.').chars() {
        if name.len() + c.len_utf8() > 180 {
            break;
        }
        name.push(c);
    }
    if name.is_empty() {
        name.push_str("modell");
    }
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii() { c } else { '_' })
        .collect();
    let encoded: String = name
        .bytes()
        .map(|c| {
            if c.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&c) {
                (c as char).to_string()
            } else {
                format!("%{c:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

pub(crate) struct DownloadBody {
    file: tokio::fs::File,
    remaining: u64,
    permit: Option<OwnedSemaphorePermit>,
}
impl DownloadBody {
    /// Streams exactly `remaining` bytes and keeps `permit` until the stream ends or is dropped.
    pub(crate) fn new(file: tokio::fs::File, remaining: u64, permit: OwnedSemaphorePermit) -> Self {
        Self { file, remaining, permit: Some(permit) }
    }
}
impl hyper::body::Body for DownloadBody {
    type Data = Bytes;
    type Error = io::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Bytes>, io::Error>>> {
        if self.remaining == 0 {
            self.permit.take();
            return Poll::Ready(None);
        }
        let mut buffer = [0; 64 * 1024];
        let limit = self.remaining.min(buffer.len() as u64) as usize;
        let mut read = ReadBuf::new(&mut buffer[..limit]);
        match Pin::new(&mut self.file).poll_read(cx, &mut read) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                let result = result.and_then(|()| {
                    if read.filled().is_empty() {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "Datei während Download verkürzt",
                        ));
                    }
                    self.remaining -= read.filled().len() as u64;
                    Ok(hyper::body::Frame::data(Bytes::copy_from_slice(
                        read.filled(),
                    )))
                });
                if result.is_err() {
                    self.remaining = 0;
                }
                if self.remaining == 0 {
                    self.permit.take();
                }
                Poll::Ready(Some(result))
            }
        }
    }
    fn is_end_stream(&self) -> bool {
        self.remaining == 0
    }
    fn size_hint(&self) -> hyper::body::SizeHint {
        hyper::body::SizeHint::with_exact(self.remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_dates_cover_epoch_leap_days_and_supported_limits() {
        for (seconds, expected) in [
            (-11644473600_i64, "Mon, 01 Jan 1601 00:00:00 GMT"),
            (-1, "Wed, 31 Dec 1969 23:59:59 GMT"),
            (0, "Thu, 01 Jan 1970 00:00:00 GMT"),
            (951782400, "Tue, 29 Feb 2000 00:00:00 GMT"),
            (4107542400, "Mon, 01 Mar 2100 00:00:00 GMT"),
            (253402300799, "Fri, 31 Dec 9999 23:59:59 GMT"),
        ] {
            let duration = Duration::from_secs(seconds.unsigned_abs());
            let time = if seconds < 0 {
                std::time::UNIX_EPOCH - duration
            } else {
                std::time::UNIX_EPOCH + duration
            };
            assert_eq!(http_date(time).as_deref(), Some(expected));
        }
        assert!(http_date(std::time::UNIX_EPOCH + Duration::from_secs(253402300800)).is_none());
        assert!(http_date(std::time::UNIX_EPOCH - Duration::from_secs(11644473601)).is_none());
    }

    #[test]
    fn disposition_removes_format_and_bidi_controls() {
        assert_eq!(
            disposition("Modell\u{202e}lts.exe"),
            disposition("Modelllts.exe")
        );
        for c in [
            '\u{00ad}', '\u{200b}', '\u{200f}', '\u{202a}', '\u{202e}', '\u{2060}', '\u{2064}',
            '\u{2066}', '\u{2069}', '\u{feff}',
        ] {
            assert_eq!(disposition(&format!("a{c}b.stl")), disposition("ab.stl"));
        }
    }

    #[test]
    fn disposition_sanitizes_empty_long_and_utf8_names() {
        assert_eq!(
            disposition(".../\\\"\n\r\t"),
            "attachment; filename=\"modell\"; filename*=UTF-8''modell"
        );
        assert_eq!(
            disposition("..Ä Ü%.stl"),
            "attachment; filename=\"_ _%.stl\"; filename*=UTF-8''%C3%84%20%C3%9C%25.stl"
        );
        let value = disposition(&"Ä".repeat(200));
        let (ascii, utf8) = value.split_once("; filename*=UTF-8''").unwrap();
        assert_eq!(
            ascii,
            format!("attachment; filename=\"{}\"", "_".repeat(90))
        );
        assert_eq!(utf8, "%C3%84".repeat(90));
    }
}
