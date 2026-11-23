//! Original files: bounded concurrency and memory, no parser or size ceiling.
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use crate::web::error_coded;
use katalog_core::{
    db,
    error_code::ErrorCode,
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

/// A failed read of the file to send. A missing file is a plain "not found" code:
/// the operating system's own wording names paths and internals and must not reach
/// the client. Every other I/O failure is a server fault, logged by
/// `download_failure` and answered with the fixed internal-error code.
fn io_failure(e: io::Error) -> ReadError {
    if e.kind() == io::ErrorKind::NotFound {
        ReadError::coded(ErrorCode::FileNotFound).with_code(read::GeometryErrorCode::NotFound)
    } else {
        ReadError::from(e.to_string()).with_code(read::GeometryErrorCode::Unreadable)
    }
}

/// Answers a failed open: expected rejections keep their code, anything else is
/// logged here and sent as the fixed internal-error code so no database or
/// operating-system text leaves the server.
fn download_failure(e: ReadError) -> Response {
    if e.expected || e.busy || e.code == Some(read::GeometryErrorCode::NotFound) {
        return crate::invoke::read_error(e);
    }
    eprintln!(
        "Interner Lesefehler: {}",
        e.message.chars().take(1024).flat_map(char::escape_default).collect::<String>()
    );
    error_coded(500, ErrorCode::WebServerInternal, &[])
}

pub async fn download(
    db_path: PathBuf,
    roots: Vec<PathBuf>,
    id: &str,
    headers: &HeaderMap,
    slots: Arc<Semaphore>,
    barrier: &crate::barrier::CatalogBarrier,
) -> Response {
    let catalog_guard = match barrier.reader() { Ok(g) => g, Err(_) => return crate::barrier::maintenance() };
    let Ok(numeric_id) = id.parse::<i64>() else {
        return error_coded(400, ErrorCode::ReqInvalidIds, &[]);
    };
    let permit = match tokio::time::timeout(Duration::from_millis(250), slots.acquire_owned()).await
    {
        Ok(Ok(p)) => p,
        _ => {
            let mut r = error_coded(503, ErrorCode::DlTooManyDownloads, &[]);
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
                ReadError::coded(ErrorCode::FileNotFound).with_code(read::GeometryErrorCode::NotFound)
            })?
            .name;
        drop(conn);
        let canonical = source
            .path
            .canonicalize()
            .map_err(io_failure)?;
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
            return Err(ReadError::coded(ErrorCode::DlPathNotAllowed)
                .with_code(read::GeometryErrorCode::Unreadable));
        }
        // Same path-admission guarantee as geometry; roots must not be writable
        // by untrusted users (concurrent directory replacement is not prevented).
        let admitted = std::fs::symlink_metadata(&canonical).map_err(io_failure)?;
        if !admitted.is_file() {
            return Err(io_failure(io::Error::new(
                io::ErrorKind::InvalidInput,
                "keine reguläre Datei",
            )));
        }
        let file = safe_file::open_regular(&canonical).map_err(io_failure)?;
        let opened = file.metadata().map_err(io_failure)?;
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
            catalog_guard,
        ))
    })
    .await;
    let (file, len, modified, name, permit, catalog_guard) = match opened {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return download_failure(e),
        Err(_) => return error_coded(500, ErrorCode::WebServerInternal, &[]),
    };
    serve(file, len, modified, &name, "application/octet-stream", permit, catalog_guard, headers)
}

/// Streams an opened regular file with range support. The type is chosen by the caller,
/// never sniffed; the file always goes out as an attachment.
#[allow(clippy::too_many_arguments)]
fn serve(
    mut file: std::fs::File,
    len: u64,
    modified: Option<String>,
    name: &str,
    content_type: &str,
    permit: OwnedSemaphorePermit,
    catalog_guard: crate::barrier::Guard,
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
            let mut r = error_coded(416, ErrorCode::DlRangeInvalid, &[]);
            r.headers_mut()
                .insert("content-range", format!("bytes */{len}").parse().unwrap());
            return r;
        }
    };
    let (start, count) = range.unwrap_or((0, len));
    // Seeking a regular file changes only the handle offset and does not read data.
    if start != 0 && file.seek(SeekFrom::Start(start)).is_err() {
        return error_coded(500, ErrorCode::WebServerInternal, &[]);
    }
    let body = DownloadBody::new(tokio::fs::File::from_std(file), count, permit).with_guard(catalog_guard);
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

/// Largest STL the web builds in memory: 1M triangles are 50 MB binary (ASCII about 4-5 times that), and up to four converters can run.
const WEB_STL_MAX_TRIANGLES: u64 = 1_000_000;
/// ASCII is about five times larger per triangle, so it gets a quarter of the limit (same memory ceiling).
const WEB_STL_ASCII_MAX_TRIANGLES: u64 = 250_000;

fn stl_failure(error: katalog_core::stl::StlWriteError) -> Response {
    use katalog_core::stl::StlWriteError as E;
    match error {
        E::NothingToExport => error_coded(422, ErrorCode::StlNothingToExport, &[]),
        E::PartOutOfRange { .. } => error_coded(400, ErrorCode::ReqInvalidArguments, &[]),
        E::TooManyTriangles { count, max } => error_coded(413, ErrorCode::StlTooLarge, &[count.to_string(), max.to_string()]),
        E::SourceChanged => error_coded(409, ErrorCode::StlSourceChanged, &[]),
        E::InvalidIndex | E::NonFinite => error_coded(422, ErrorCode::StlSourceUnreadable, &[]),
        E::TargetIsSource | E::TargetExists | E::Io(_) => error_coded(500, ErrorCode::WebServerInternal, &[]),
    }
}

/// "Save as STL" for a 3MF or OBJ of the catalog: the same core writer as the desktop app
/// (binary STL, or ASCII on request), built in memory under the parser gate and sent as an attachment. `plate`
/// limits the result to one plate of a 3MF. STEP is not offered (no CAD kernel in the web build).
pub async fn stl(gate: &Arc<crate::invoke::GeometryGate>, db_path: PathBuf, roots: Vec<PathBuf>, id: &str, plate: Option<u32>, format: katalog_core::stl::StlFormat) -> Response {
    use katalog_core::{db::models::FileType, stl::{self, SourceStamp, StlScope}};
    let catalog_guard = match gate.barrier.reader() { Ok(g) => g, Err(_) => return crate::barrier::maintenance() };
    let Ok(numeric_id) = id.parse::<i64>() else {
        return error_coded(400, ErrorCode::ReqInvalidIds, &[]);
    };
    let permits = match gate.acquire().await {
        Ok(p) => p,
        Err(e) => return *e,
    };
    let result = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, String), Box<Response>> {
        // The permits and the barrier guard belong to this closure: cancelling the request does not
        // release them while the parser is still running.
        let _held = (permits, catalog_guard);
        let conn = db::open_read_only(&db_path).map_err(|e| Box::new(download_failure(ReadError::from(e))))?;
        let file = db::get_file(&conn, numeric_id)
            .map_err(|e| Box::new(download_failure(ReadError::from(e))))?
            .ok_or_else(|| Box::new(error_coded(404, ErrorCode::FileNotFound, &[])))?;
        drop(conn);
        match file.file_type {
            FileType::ThreeMf | FileType::Obj => {}
            FileType::Stl => return Err(Box::new(error_coded(422, ErrorCode::StlSourceIsStl, &[]))),
            FileType::Stp => return Err(Box::new(error_coded(422, ErrorCode::StlStepUnavailable, &[]))),
            _ => return Err(Box::new(error_coded(422, ErrorCode::StlSourceUnreadable, &[]))),
        }
        let source = PathBuf::from(file.current_path());
        let stamp = SourceStamp::of(&source).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Box::new(error_coded(404, ErrorCode::FileNotFound, &[])),
            _ => Box::new(error_coded(422, ErrorCode::StlSourceUnreadable, &[])),
        })?;
        let meshes = read::load_render_meshes_with_limits(&source, Some(&roots), Some(katalog_core::import::files::WebImportLimits::default()))
            .map_err(|e| Box::new(download_failure(e)))?;
        stamp.ensure_unchanged(&source).map_err(|e| Box::new(stl_failure(e)))?;
        let selected = stl::select_meshes(&meshes, plate.map_or(StlScope::All, StlScope::Plate)).map_err(|e| Box::new(stl_failure(e)))?;
        let mut bytes = Vec::new();
        let limit = match format {
            stl::StlFormat::Binary => WEB_STL_MAX_TRIANGLES,
            stl::StlFormat::Ascii => WEB_STL_ASCII_MAX_TRIANGLES,
        };
        stl::write_stl_as(&mut bytes, &selected, format, limit).map_err(|e| Box::new(stl_failure(e)))?;
        let stem = std::path::Path::new(&file.name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "model".into());
        let name = match plate {
            Some(number) => format!("{stem}_Platte{number}.stl"),
            None => format!("{stem}.stl"),
        };
        Ok((bytes, name))
    })
    .await;
    match result {
        Ok(Ok((bytes, name))) => {
            let length = bytes.len().to_string();
            let mut response = Body::from(bytes).into_response();
            for (key, value) in [
                ("content-type", "application/octet-stream".to_owned()),
                ("content-length", length),
                ("content-disposition", disposition(&name)),
                ("cache-control", "no-store".to_owned()),
            ] {
                response.headers_mut().insert(key, value.parse().unwrap());
            }
            response
        }
        Ok(Err(response)) => *response,
        Err(_) => error_coded(500, ErrorCode::WebServerInternal, &[]),
    }
}

/// One companion file of a model, by its row id. The path comes from the catalog row
/// only; the strict open (no links, one name, inside a root) is `open_for_web`.
pub async fn companion(
    db_path: PathBuf,
    roots: Vec<PathBuf>,
    max_bytes: u64,
    id: &str,
    headers: &HeaderMap,
    slots: Arc<Semaphore>,
    barrier: &crate::barrier::CatalogBarrier,
) -> Response {
    let catalog_guard = match barrier.reader() { Ok(g) => g, Err(_) => return crate::barrier::maintenance() };
    let Ok(numeric_id) = id.parse::<i64>() else {
        return error_coded(400, ErrorCode::ReqInvalidIds, &[]);
    };
    let permit = match tokio::time::timeout(Duration::from_millis(250), slots.acquire_owned()).await {
        Ok(Ok(p)) => p,
        _ => {
            let mut r = error_coded(503, ErrorCode::DlTooManyDownloads, &[]);
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
            max_bytes,
        );
        drop(conn);
        let (row, kind, mut file, _) = opened.map_err(crate::invoke::companion_error)?;
        // The content check read the start of the file; the answer begins there again.
        file.seek(SeekFrom::Start(0)).map_err(io_failure)?;
        let meta = file.metadata().map_err(io_failure)?;
        let content_type = katalog_core::companion_read::download_content_type(kind, &row.name);
        Ok::<_, ReadError>((file, meta.len(), meta.modified().ok().and_then(http_date), row.name, content_type, permit, catalog_guard))
    })
    .await;
    let (file, len, modified, name, content_type, permit, catalog_guard) = match opened {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return download_failure(e),
        Err(_) => return error_coded(500, ErrorCode::WebServerInternal, &[]),
    };
    serve(file, len, modified, &name, content_type, permit, catalog_guard, headers)
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
    file: Option<tokio::fs::File>,
    remaining: u64,
    permit: Option<OwnedSemaphorePermit>,
    catalog_guard: Option<crate::barrier::Guard>,
    cancellation: Option<Pin<Box<dyn std::future::Future<Output = ()> + Send>>>,
}
impl DownloadBody {
    pub(crate) fn with_guard(mut self, guard: crate::barrier::Guard) -> Self {
        self.cancellation = Some(guard.stream_cancelled());
        self.catalog_guard = Some(guard);
        self
    }
    /// Streams exactly `remaining` bytes and keeps `permit` until the stream ends or is dropped.
    pub(crate) fn new(file: tokio::fs::File, remaining: u64, permit: OwnedSemaphorePermit) -> Self {
        Self { file: Some(file), remaining, permit: Some(permit), catalog_guard: None, cancellation: None }
    }
}
impl hyper::body::Body for DownloadBody {
    type Data = Bytes;
    type Error = io::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Bytes>, io::Error>>> {
        if self.cancellation.as_mut().is_some_and(|cancel| cancel.as_mut().poll(cx).is_ready()) {
            self.remaining = 0;
            self.file.take();
            self.cancellation = None;
            self.permit.take();
            self.catalog_guard.take();
            return Poll::Ready(Some(Err(io::Error::new(io::ErrorKind::TimedOut, "Catalog maintenance stream deadline"))));
        }
        if self.remaining == 0 {
            self.file.take();
            self.cancellation = None;
            self.permit.take();
            self.catalog_guard.take();
            return Poll::Ready(None);
        }
        let mut buffer = [0; 64 * 1024];
        let limit = self.remaining.min(buffer.len() as u64) as usize;
        let mut read = ReadBuf::new(&mut buffer[..limit]);
        match Pin::new(self.file.as_mut().expect("active stream has a file")).poll_read(cx, &mut read) {
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
                    self.file.take();
                    self.cancellation = None;
                    self.permit.take();
                    self.catalog_guard.take();
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

    /// A vanished file is a plain "not found" code; the operating system's text,
    /// which names the path, is dropped.
    #[test]
    fn a_missing_file_becomes_a_code_without_the_system_text() {
        let os_error = io::Error::new(io::ErrorKind::NotFound, "No such file or directory (os error 2) at /srv/models/a.3mf");
        let mapped = io_failure(os_error);
        assert_eq!(mapped.message, "fileNotFound");
        assert!(mapped.expected);
        assert_eq!(mapped.code, Some(read::GeometryErrorCode::NotFound));
    }

    /// Any other failure is logged, never sent: the answer is the fixed code.
    #[tokio::test]
    async fn other_failures_answer_with_the_fixed_internal_code_only() {
        let cause = io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied (os error 13) at /srv/models/secret.3mf");
        let response = download_failure(io_failure(cause));
        assert_eq!(response.status(), 500);
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 16).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap(), serde_json::json!({"message": "webServerInternal", "expected": false}));
        assert!(!text.contains("/srv") && !text.contains("os error"), "{text}");
    }
}
