//! Static assets are independent of model roots and are never directory listings.
use axum::{
    http::header,
    response::{IntoResponse, Response},
};
use std::path::{Path, PathBuf};

fn decode(path: &str) -> Option<PathBuf> {
    let mut result = PathBuf::new();
    for segment in path.strip_prefix('/')?.split('/') {
        if segment.is_empty() {
            continue;
        }
        let bytes = segment.as_bytes();
        let mut decoded = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                let digit = |b: u8| (b as char).to_digit(16).map(|n| n as u8);
                decoded.push(digit(*bytes.get(i + 1)?)? * 16 + digit(*bytes.get(i + 2)?)?);
                i += 3;
            } else {
                decoded.push(bytes[i]);
                i += 1;
            }
        }
        let text = String::from_utf8(decoded).ok()?;
        if text == "."
            || text == ".."
            || text
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '%'))
        {
            return None;
        }
        result.push(text);
    }
    Some(result)
}
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}
fn load(root: &Path, request: &str) -> Option<(Vec<u8>, &'static str, &'static str)> {
    let relative = decode(request)?;
    let candidate = root.join(&relative);
    let canonical = candidate.canonicalize().ok();
    if canonical.as_ref().is_some_and(|p| !p.starts_with(root)) {
        return None;
    }
    let path = if let Some(path) = canonical.filter(|p| p.is_file()) {
        path
    } else if relative.extension().is_none() {
        root.join("index.html").canonicalize().ok()?
    } else {
        return None;
    };
    if !path.starts_with(root) || !path.is_file() {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    let stem = path.file_stem()?.to_str()?;
    // Vite hashes are eight URL-safe characters, including '-' and '_'.
    let suffix = stem
        .as_bytes()
        .get(stem.len().saturating_sub(9)..)
        .unwrap_or_default();
    let hashed = relative.starts_with("assets")
        && suffix.len() == 9
        && suffix[0] == b'-'
        && suffix[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
    let cache = if name == "index.html" {
        "no-cache"
    } else if hashed {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Some((std::fs::read(&path).ok()?, content_type(&path), cache))
}
pub async fn serve(root: PathBuf, path: String) -> Response {
    match tokio::task::spawn_blocking(move || load(&root, &path)).await {
        Ok(Some((bytes, mime, cache))) => (
            [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache)],
            bytes,
        )
            .into_response(),
        _ => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"message":"Nicht gefunden", "expected":true})),
        )
            .into_response(),
    }
}
