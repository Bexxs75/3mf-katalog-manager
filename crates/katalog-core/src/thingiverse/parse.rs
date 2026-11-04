//! Everything that depends on the shape of the Thingiverse API sits in this file:
//! endpoint paths, field names and the parsers. NOTHING here is verified against the
//! real service yet (the check with a real key is still open); the stub-server tests
//! use fixtures written after the same assumptions. When the real shape differs, only
//! this file changes.
//!
//! Assumed:
//! - `GET /users/me` -> `{ "name": "<user>" }`
//! - `GET /search/<term>?type=things&page=<n>&per_page=<k>` -> `{ "total": <n>, "hits": [<thing>] }`
//! - `GET /things/<id>` -> `<thing>`
//! - `<thing>` = `{ id, name, thumbnail, public_url, creator: { name }, license, like_count,
//!   added, description, tags: [{ name } | "text"], default_image: { url }, is_private,
//!   is_deleted }`

use serde_json::Value;

use super::{ThingDetailDto, ThingSummaryDto, TvError};
use crate::error_code::ErrorCode;

/// Hits per search page; the page size the API really uses is not confirmed.
pub const PER_PAGE: u32 = 20;
pub const MAX_QUERY_CHARS: usize = 100;
pub const MAX_DESCRIPTION_CHARS: usize = 4000;
const MAX_NAME_CHARS: usize = 300;
const MAX_TAGS: usize = 30;

pub const API_BASE: &str = "https://api.thingiverse.com";
pub const PAGE_HOST: &str = "thingiverse.com";

fn bad() -> TvError {
    TvError::new(ErrorCode::ThingiverseBadResponse)
}

/// The addresses a card needs later: the preview images and the page. They stay
/// in memory and are never handed to the interface.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThingRefs {
    pub thumb: Option<String>,
    pub large: Option<String>,
    pub page: Option<String>,
}

pub struct ParsedThing {
    pub summary: ThingSummaryDto,
    pub refs: ThingRefs,
}

pub struct ParsedDetail {
    pub detail: ThingDetailDto,
    pub refs: ThingRefs,
}

pub struct ParsedSearch {
    pub things: Vec<ParsedThing>,
    pub has_more: bool,
}

/// Search terms: not empty, at most 100 characters, no control characters.
pub fn validate_query(raw: &str) -> Result<String, TvError> {
    let q = raw.trim();
    if q.is_empty() || q.chars().count() > MAX_QUERY_CHARS || q.chars().any(char::is_control) {
        return Err(TvError::new(ErrorCode::ThingiverseQueryInvalid));
    }
    Ok(q.to_string())
}

pub fn me_url(base: &str) -> Result<reqwest::Url, TvError> {
    segments(base, &["users", "me"], &[])
}

pub fn search_url(base: &str, query: &str, page: u32) -> Result<reqwest::Url, TvError> {
    segments(
        base,
        &["search", query],
        &[("type", "things"), ("page", &page.max(1).to_string()), ("per_page", &PER_PAGE.to_string())],
    )
}

pub fn thing_url(base: &str, id: u64) -> Result<reqwest::Url, TvError> {
    segments(base, &["things", &id.to_string()], &[])
}

/// Builds a URL with percent-encoded path segments; the search term cannot break out of its segment.
fn segments(base: &str, path: &[&str], query: &[(&str, &str)]) -> Result<reqwest::Url, TvError> {
    let mut url = reqwest::Url::parse(base).map_err(|_| bad())?;
    url.path_segments_mut().map_err(|_| bad())?.pop_if_empty().extend(path);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Ok(url)
}

fn text(v: &Value, key: &str, max: usize) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(|s| bounded(s, max)).filter(|s| !s.is_empty())
}

fn bounded(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

pub fn parse_user_name(body: &Value) -> Result<String, TvError> {
    text(body, "name", MAX_NAME_CHARS).or_else(|| text(body, "username", MAX_NAME_CHARS)).ok_or_else(bad)
}

/// Strict on types (an `id` that is not a number is an error), lenient on optional fields.
fn parse_common(v: &Value) -> Result<(ThingSummaryDto, ThingRefs), TvError> {
    if !v.is_object() {
        return Err(bad());
    }
    let id = v.get("id").and_then(Value::as_u64).ok_or_else(bad)?;
    let name = text(v, "name", MAX_NAME_CHARS).ok_or_else(bad)?;
    let author = v.get("creator").and_then(|c| text(c, "name", MAX_NAME_CHARS)).unwrap_or_default();
    let thumb = text(v, "thumbnail", 2048);
    let large = v.get("default_image").and_then(|i| text(i, "url", 2048)).or_else(|| thumb.clone());
    let page = text(v, "public_url", 2048);
    let summary = ThingSummaryDto {
        id,
        name,
        author_name: author,
        license_name: text(v, "license", 120),
        like_count: v.get("like_count").and_then(Value::as_u64),
        preview_available: thumb.is_some(),
        url: thing_page_url(id, page.as_deref()),
    };
    Ok((summary, ThingRefs { thumb, large, page }))
}

pub fn parse_thing(v: &Value) -> Result<ParsedThing, TvError> {
    let (summary, refs) = parse_common(v)?;
    Ok(ParsedThing { summary, refs })
}

pub fn parse_search(body: &Value, page: u32) -> Result<ParsedSearch, TvError> {
    let hits = match body {
        Value::Array(a) => a,
        Value::Object(_) => body.get("hits").and_then(Value::as_array).ok_or_else(bad)?,
        _ => return Err(bad()),
    };
    let things = hits.iter().map(parse_thing).collect::<Result<Vec<_>, _>>()?;
    let has_more = match body.get("total").and_then(Value::as_u64) {
        Some(total) => u64::from(page.max(1)) * u64::from(PER_PAGE) < total,
        None => things.len() as u32 >= PER_PAGE,
    };
    Ok(ParsedSearch { things, has_more })
}

pub fn parse_detail(v: &Value) -> Result<ParsedDetail, TvError> {
    if v.get("is_private").and_then(Value::as_bool) == Some(true) || v.get("is_deleted").and_then(Value::as_bool) == Some(true) {
        return Err(TvError::new(ErrorCode::ThingiverseNotFound));
    }
    let (s, refs) = parse_common(v)?;
    let description = v
        .get("description")
        .and_then(Value::as_str)
        .map(|d| plain_text(d, MAX_DESCRIPTION_CHARS))
        .unwrap_or_default();
    let tags = v
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(|s| bounded(s, 60)).or_else(|| text(t, "name", 60)))
                .filter(|t| !t.is_empty())
                .take(MAX_TAGS)
                .collect()
        })
        .unwrap_or_default();
    Ok(ParsedDetail {
        detail: ThingDetailDto {
            id: s.id,
            name: s.name,
            author_name: s.author_name,
            license_name: s.license_name,
            like_count: s.like_count,
            preview_available: s.preview_available,
            url: s.url,
            description,
            created_at: text(v, "added", 40),
            tags,
        },
        refs,
    })
}

/// `https://www.thingiverse.com/thing:<id>` or `https://thingiverse.com/thing:<id>`
/// with exactly this id; nothing else (no other scheme, host, port, login part,
/// query, fragment or text after the number).
pub fn validated_thing_url(candidate: &str, id: u64) -> Option<String> {
    let url = reqwest::Url::parse(candidate).ok()?;
    let host_ok = matches!(url.host_str(), Some(h) if h == PAGE_HOST || h == format!("www.{PAGE_HOST}"));
    let plain = url.scheme() == "https"
        && host_ok
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    (plain && url.path() == format!("/thing:{id}")).then(|| url.to_string())
}

/// The page address: the one the API named when it passes the check, else built from the number.
pub fn thing_page_url(id: u64, candidate: Option<&str>) -> String {
    candidate.and_then(|c| validated_thing_url(c, id)).unwrap_or_else(|| format!("https://www.thingiverse.com/thing:{id}"))
}

/// Whether an image may be fetched from this address (production rule): https and a
/// Thingiverse host. The host list is an assumption until the check with a real key.
pub fn image_host_allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|h| h == PAGE_HOST || h.ends_with(&format!(".{PAGE_HOST}")))
}

/// Image type from the first bytes: JPEG, PNG or WebP. Everything else (SVG
/// included, which can carry script) is refused.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Reduces a description (HTML or plain text) to plain text: script and style
/// content removed, tags removed, line breaks kept, entities read, length cut.
/// The result is shown as text only, never as HTML.
pub fn plain_text(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    // ASCII lowercasing keeps every byte offset, so indices of `lower` fit `input`.
    let lower = input.to_ascii_lowercase();
    let len = input.len();
    let mut i = 0;
    while i < len {
        let bytes = input.as_bytes();
        // A '<' that cannot start a tag ("a < b") is ordinary text.
        let starts_tag = bytes[i] == b'<' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic() || *b == b'/' || *b == b'!');
        if !starts_tag {
            let ch = input[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        // The content of script and style goes away together with the tags.
        if let Some(tag) = ["script", "style"].into_iter().find(|t| lower[i..].starts_with(&format!("<{t}"))) {
            i = match lower[i..].find(&format!("</{tag}")) {
                Some(p) => lower[i + p..].find('>').map_or(len, |g| i + p + g + 1),
                None => len,
            };
            continue;
        }
        let Some(gt) = lower[i..].find('>').map(|g| i + g) else { break };
        let name: String = lower[i + 1..gt].trim_start_matches('/').trim().chars().take_while(char::is_ascii_alphanumeric).collect();
        let breaks = ["br", "p", "div", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6"];
        if breaks.contains(&name.as_str()) && !out.ends_with('\n') {
            out.push('\n');
        }
        i = gt + 1;
    }
    let decoded = decode_entities(&out);
    let mut text = String::new();
    let mut blank = 0;
    for line in decoded.lines() {
        let line: String = line.chars().filter(|c| !c.is_control()).collect();
        let line = line.trim();
        if line.is_empty() {
            blank += 1;
            if blank > 1 || text.is_empty() {
                continue;
            }
        } else {
            blank = 0;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(line);
    }
    text.trim().chars().take(max_chars).collect()
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let decoded = rest.find(';').filter(|&e| e <= 10).and_then(|e| {
            let name = &rest[1..e];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix('#')
                    .and_then(|n| match n.strip_prefix(['x', 'X']) {
                        Some(h) => u32::from_str_radix(h, 16).ok(),
                        None => n.parse().ok(),
                    })
                    .and_then(char::from_u32),
            };
            ch.map(|c| (c, e + 1))
        });
        match decoded {
            Some((c, used)) => {
                out.push(c);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_rules() {
        assert_eq!(validate_query("  cable  ").unwrap(), "cable");
        for bad in ["", "   ", "a\nb", "a\u{0}b", &"x".repeat(101)] {
            assert_eq!(validate_query(bad).unwrap_err().code, ErrorCode::ThingiverseQueryInvalid, "{bad:?}");
        }
        assert!(validate_query(&"x".repeat(100)).is_ok());
    }

    #[test]
    fn the_term_stays_inside_its_path_segment() {
        let url = search_url("https://api.example.test", "a/b?c#d e", 3).unwrap();
        assert_eq!(url.path(), "/search/a%2Fb%3Fc%23d%20e");
        assert_eq!(url.query(), Some("type=things&page=3&per_page=20"));
        assert_eq!(thing_url("https://api.example.test", 42).unwrap().path(), "/things/42");
    }

    #[test]
    fn page_urls_are_checked() {
        let ok = |u: &str| validated_thing_url(u, 5).is_some();
        assert!(ok("https://www.thingiverse.com/thing:5"));
        assert!(ok("https://thingiverse.com/thing:5"));
        for u in [
            "http://www.thingiverse.com/thing:5",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://evil.example/thing:5",
            "https://www.thingiverse.com.evil.example/thing:5",
            "https://www.thingiverse.com/thing:5/files",
            "https://www.thingiverse.com/thing:55",
            "https://www.thingiverse.com/thing:5?x=1",
            "https://www.thingiverse.com/thing:5#top",
            "https://user@www.thingiverse.com/thing:5",
            "https://www.thingiverse.com:444/thing:5",
            "not a url",
            "",
        ] {
            assert!(!ok(u), "{u}");
        }
        // A rejected address falls back to the one built from the number.
        assert_eq!(thing_page_url(5, Some("https://evil.example/thing:5")), "https://www.thingiverse.com/thing:5");
        assert_eq!(thing_page_url(5, None), "https://www.thingiverse.com/thing:5");
    }

    #[test]
    fn image_hosts_and_types() {
        let u = |s: &str| reqwest::Url::parse(s).unwrap();
        assert!(image_host_allowed(&u("https://cdn.thingiverse.com/a.jpg")));
        assert!(!image_host_allowed(&u("http://cdn.thingiverse.com/a.jpg")));
        assert!(!image_host_allowed(&u("https://cdn.thingiverse.com.evil.example/a.jpg")));
        assert!(!image_host_allowed(&u("https://evilthingiverse.com/a.jpg")));
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0, 0]), Some("image/jpeg"));
        assert_eq!(sniff_image(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]), Some("image/png"));
        assert_eq!(sniff_image(b"RIFF\x00\x00\x00\x00WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), None);
        assert_eq!(sniff_image(b"GIF89a"), None);
        assert_eq!(sniff_image(b""), None);
    }

    #[test]
    fn descriptions_become_plain_text() {
        let t = plain_text("<p>Hello &amp; <b>welcome</b></p><script>alert('x')</script><p>Line&nbsp;2 &lt;ok&gt; &#65;&#x42; &bogus;</p>", 4000);
        assert_eq!(t, "Hello & welcome\nLine 2 <ok> AB &bogus;");
        assert!(!t.contains("alert"));
        assert_eq!(plain_text("a<br>b<br/>c", 100), "a\nb\nc");
        assert_eq!(plain_text("x<style>p{}</style>y", 100), "xy");
        assert_eq!(plain_text("x<script>never closed", 100), "x");
        assert_eq!(plain_text("1\n\n\n\n2", 100), "1\n\n2");
        assert_eq!(plain_text(&"\u{e4}".repeat(5000), 4000).chars().count(), 4000);
        assert_eq!(plain_text("bell\u{7}char", 100), "bellchar");
        assert_eq!(plain_text("a < b and <unclosed", 100), "a < b and");
    }

    #[test]
    fn a_search_page_parses_and_reports_more() {
        let body = json!({"total": 45, "hits": [
            {"id": 1, "name": "A", "thumbnail": "https://cdn.thingiverse.com/a.jpg", "creator": {"name": "Ann"}, "license": "CC BY", "like_count": 7},
            {"id": 2, "name": "B"}
        ]});
        let p = parse_search(&body, 1).unwrap();
        assert_eq!(p.things.len(), 2);
        assert!(p.has_more);
        assert!(!parse_search(&body, 3).unwrap().has_more);
        let first = &p.things[0];
        assert_eq!(first.summary.author_name, "Ann");
        assert_eq!(first.summary.license_name.as_deref(), Some("CC BY"));
        assert!(first.summary.preview_available);
        assert_eq!(first.summary.url, "https://www.thingiverse.com/thing:1");
        let second = &p.things[1].summary;
        assert!(!second.preview_available);
        assert_eq!((second.author_name.as_str(), second.license_name.as_deref()), ("", None));
        // The address of the image never reaches the DTO.
        assert!(!serde_json::to_string(&first.summary).unwrap().contains("cdn.thingiverse.com"));
    }

    #[test]
    fn wrong_types_are_a_bad_response() {
        for body in [json!({"hits": [{"id": "1", "name": "A"}]}), json!({"hits": [{"id": 1}]}), json!({"hits": 5}), json!("x"), json!({"nope": []})] {
            assert_eq!(parse_search(&body, 1).err().map(|e| e.code), Some(ErrorCode::ThingiverseBadResponse), "{body}");
        }
    }

    #[test]
    fn a_detail_parses_tags_text_and_hides_private_things() {
        let v = json!({"id": 9, "name": "N", "description": "<p>Hi</p>", "added": "2025-03-12T10:00:00+00:00",
                       "tags": [{"name": "cable"}, "holder", {"x": 1}], "default_image": {"url": "https://cdn.thingiverse.com/big.jpg"}});
        let d = parse_detail(&v).unwrap();
        assert_eq!(d.detail.description, "Hi");
        assert_eq!(d.detail.tags, vec!["cable".to_string(), "holder".to_string()]);
        assert_eq!(d.refs.large.as_deref(), Some("https://cdn.thingiverse.com/big.jpg"));
        for flag in ["is_private", "is_deleted"] {
            let mut gone = v.clone();
            gone[flag] = json!(true);
            assert_eq!(parse_detail(&gone).err().map(|e| e.code), Some(ErrorCode::ThingiverseNotFound));
        }
    }

    #[test]
    fn the_user_name_comes_from_name() {
        assert_eq!(parse_user_name(&json!({"name": "Ann"})).unwrap(), "Ann");
        assert!(parse_user_name(&json!({"id": 1})).is_err());
    }
}
