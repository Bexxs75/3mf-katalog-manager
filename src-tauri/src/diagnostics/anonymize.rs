//! Replaces personal data in a log excerpt before it leaves the computer.
//! Returns segments so the preview can highlight exactly what was replaced.
use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub text: String,
    pub replaced: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Context {
    pub home: Option<String>,
    pub user: Option<String>,
    pub host: Option<String>,
    pub catalog_roots: Vec<String>,
    pub printer_addresses: Vec<String>,
    pub replace_file_names: bool,
}

const FILE_EXTENSIONS: &str = "3mf|stl|obj|step|stp|gcode|bgcode|zip|7z|rar|tar|gz|xz|zst|png|jpe?g|webp|ctb|goo|pwmx";

// Fixed patterns (independent of `Context`), compiled once instead of on every
// preview/export call - this runs on the main thread and previously recompiled
// all of these regexes per call.
static EMAIL_RE: LazyLock<Result<Regex, String>> = LazyLock::new(|| compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"));
static IP_RE: LazyLock<Result<Regex, String>> = LazyLock::new(|| {
    compile(
        r"\b(?:\d{1,3}\.){3}\d{1,3}\b|\b(?:[0-9a-fA-F]{1,4}:){3,7}[0-9a-fA-F]{1,4}\b|\b[0-9a-fA-F]{1,4}(?::[0-9a-fA-F]{1,4})*::(?:[0-9a-fA-F]{1,4}(?::[0-9a-fA-F]{1,4})*)?",
    )
});
static LOCAL_HOST_RE: LazyLock<Result<Regex, String>> =
    LazyLock::new(|| compile(r"(?i)(?:^|[^.\w-])([0-9A-Za-z-]+\.local)(?:[^.\w-]|$)"));
static IN_PATH_FILE_RE: LazyLock<Result<Regex, String>> =
    LazyLock::new(|| compile(&format!(r#"(?i)[/\\]([^/\\"<>|:\r\n]+?)\.(?:{FILE_EXTENSIONS})\b"#)));
static BARE_FILE_RE: LazyLock<Result<Regex, String>> =
    LazyLock::new(|| compile(&format!(r#"(?i)(?:^|[\s("'])([^\s/\\"'<>|:()]+)\.(?:{FILE_EXTENSIONS})\b"#)));

static IMPORT_FILE_RE: LazyLock<Result<Regex, String>> = LazyLock::new(||
    compile(r"(?m)übersprungen: ([^\r\n]+) \([^\r\n]*\)$"));

fn compile(pattern: &str) -> Result<Regex, String> {
    Regex::new(pattern).map_err(|_| "Anonymisierung nicht möglich: Muster konnte nicht erstellt werden".to_string())
}

pub fn to_text(segs: &[Segment]) -> String {
    segs.iter().map(|s| s.text.as_str()).collect()
}

pub fn anonymize(text: &str, ctx: &Context) -> Result<Vec<Segment>, String> {
    let mut segs = vec![Segment { text: text.to_string(), replaced: false }];

    // The verbose import field has explicit boundaries; process it before path
    // replacements split it into segments, including relative names with spaces.
    if ctx.replace_file_names {
        segs = replace_group(segs, IMPORT_FILE_RE.as_ref().map_err(Clone::clone)?, 1, |_| Some("<datei>".into()));
    }

    // Longest first, so a catalog inside the home folder becomes <katalog>, not
    // ~/.... Matched case-insensitively: Windows and macOS paths are
    // case-insensitive and show up in varying case in logs.
    let mut ci_literals: Vec<(String, String)> = Vec::new();
    for root in &ctx.catalog_roots {
        for v in path_variants(root) {
            ci_literals.push((v, "<katalog>".into()));
        }
    }
    if let Some(home) = &ctx.home {
        for v in path_variants(home) {
            ci_literals.push((v, "~".into()));
        }
    }
    ci_literals.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
    for (needle, repl) in &ci_literals {
        segs = replace_path_prefix_ci(segs, needle, repl)?;
    }

    let mut literals: Vec<(String, String)> = Vec::new();
    for (i, addr) in ctx.printer_addresses.iter().enumerate() {
        let label = format!("<printer-{}>", i + 1);
        let addr = addr.trim();
        if addr.is_empty() {
            continue;
        }
        literals.push((addr.to_string(), label.clone()));
        if let Some((host, _port)) = addr.rsplit_once(':') {
            if !host.is_empty() && !host.contains(':') {
                literals.push((host.to_string(), label));
            }
        }
    }
    literals.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
    for (needle, repl) in &literals {
        // Configured hostnames (e.g. "printer.fritz.box") show up in logs in
        // varying case, just like home/catalog paths.
        segs = replace_literal_ci(segs, needle, repl)?;
    }

    segs = replace_group(segs, EMAIL_RE.as_ref().map_err(Clone::clone)?, 0, |_| Some("<email>".into()));

    // IPv4 and IPv6 (at least three colons or a "::", so clock times like
    // 14:02:11 never match).
    let mut ips: HashMap<String, usize> = HashMap::new();
    segs = replace_group(segs, IP_RE.as_ref().map_err(Clone::clone)?, 0, |m| {
        // Real addresses always contain a digit; Rust paths like `db::list` don't.
        if !m.chars().any(|c| c.is_ascii_digit()) {
            return None;
        }
        let next = ips.len() + 1;
        let n = *ips.entry(m.to_string()).or_insert(next);
        Some(format!("<ip-{n}>"))
    });

    // `*.local` hostnames, numbered from the same pool as the IPs above. Kept
    // as a separate pass (no digit requirement, since a hostname like
    // `qidi.local` is a real address without one) with its own boundary check,
    // so a file name that merely ends in `.local` (`.env.local`,
    // `vite.config.local.ts`) is not mistaken for a host: the name before
    // `.local` must not itself be preceded by a `.` or dotted extension.
    segs = replace_group(segs, LOCAL_HOST_RE.as_ref().map_err(Clone::clone)?, 1, |m| {
        let next = ips.len() + 1;
        let n = *ips.entry(m.to_string()).or_insert(next);
        Some(format!("<ip-{n}>"))
    });

    for (value, label) in [(&ctx.host, "<host>"), (&ctx.user, "<user>")] {
        if let Some(v) = value {
            if v.chars().count() >= 3 {
                let re = compile(&format!(r"(?i)\b{}\b", regex::escape(v)))?;
                segs = replace_group(segs, &re, 0, |_| Some(label.to_string()));
            }
        }
    }

    if ctx.replace_file_names {
        let mut names: HashMap<String, usize> = HashMap::new();
        let mut label = |m: &str| {
            let next = names.len() + 1;
            let n = *names.entry(m.to_string()).or_insert(next);
            Some(format!("<datei-{n}>"))
        };
        // After a path separator the name may contain spaces.
        segs = replace_group(segs, IN_PATH_FILE_RE.as_ref().map_err(Clone::clone)?, 1, &mut label);
        segs = replace_group(segs, BARE_FILE_RE.as_ref().map_err(Clone::clone)?, 1, &mut label);
    }

    Ok(merge_plain(segs))
}

fn path_variants(p: &str) -> Vec<String> {
    let p = p.trim_end_matches(['/', '\\']);
    if p.is_empty() {
        return Vec::new();
    }
    let mut v = vec![p.to_string(), p.replace('\\', "/"), p.replace('/', "\\")];
    v.sort();
    v.dedup();
    v
}

fn push_plain(out: &mut Vec<Segment>, text: &str) {
    if !text.is_empty() {
        out.push(Segment { text: text.to_string(), replaced: false });
    }
}

/// Replaces every occurrence of a literal needle, case-insensitively: used for
/// configured printer hostnames (which show up in logs in varying case).
fn replace_literal_ci(segs: Vec<Segment>, needle: &str, repl: &str) -> Result<Vec<Segment>, String> {
    if needle.is_empty() {
        return Ok(segs);
    }
    let re = compile(&format!("(?i){}", regex::escape(needle)))?;
    Ok(replace_group(segs, &re, 0, |_| Some(repl.to_string())))
}

/// Case-insensitive variant of `replace_literal_ci` for home/catalog paths,
/// which additionally requires a path/word boundary right after the needle -
/// without it, "/home/thebexxs" would also match inside an unrelated longer
/// name like "/home/thebexxs2" or "/home/thebexxsBackup".
fn replace_path_prefix_ci(segs: Vec<Segment>, needle: &str, repl: &str) -> Result<Vec<Segment>, String> {
    if needle.is_empty() {
        return Ok(segs);
    }
    let re = compile(&format!(r"(?i)({})(?:[^.\w-]|$)", regex::escape(needle)))?;
    // Group 1 is only the needle itself; the boundary character checked by the
    // non-capturing part (a path separator, punctuation, or end of string)
    // stays untouched in the output.
    Ok(replace_group(segs, &re, 1, |_| Some(repl.to_string())))
}

/// `f` returns `None` to keep a match as it is (e.g. `db::list` is not an IPv6 address).
fn replace_group(segs: Vec<Segment>, re: &Regex, group: usize, mut f: impl FnMut(&str) -> Option<String>) -> Vec<Segment> {
    let mut out = Vec::new();
    for s in segs {
        if s.replaced {
            out.push(s);
            continue;
        }
        let mut last = 0;
        for caps in re.captures_iter(&s.text) {
            let Some(m) = caps.get(group) else { continue };
            if m.start() < last {
                continue;
            }
            let Some(replacement) = f(m.as_str()) else { continue };
            push_plain(&mut out, &s.text[last..m.start()]);
            out.push(Segment { text: replacement, replaced: true });
            last = m.end();
        }
        push_plain(&mut out, &s.text[last..]);
    }
    out
}

fn merge_plain(segs: Vec<Segment>) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for s in segs {
        match out.last_mut() {
            Some(prev) if !prev.replaced && !s.replaced => prev.text.push_str(&s.text),
            _ => out.push(s),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, ctx: &Context) -> String {
        to_text(&anonymize(text, ctx).unwrap())
    }

    fn linux() -> Context {
        Context {
            home: Some("/home/thebexxs".into()),
            user: Some("thebexxs".into()),
            host: Some("skywalker".into()),
            catalog_roots: vec!["/home/thebexxs/3D-Katalog".into()],
            printer_addresses: vec!["192.168.2.50:7125".into(), "sv08.local".into()],
            replace_file_names: false,
        }
    }

    #[test]
    fn oversized_context_does_not_panic() {
        let ctx = Context { catalog_roots: vec!["a".repeat(2_000_000)], ..Default::default() };
        assert!(anonymize("private data", &ctx).is_err());
    }

    #[test]
    fn verbose_import_names_with_spaces_are_fully_replaced() {
        let ctx = Context { replace_file_names: true, ..Default::default() };
        let text = "WARN [import] übersprungen: Anna Weber.stl (Datei ungültig)";
        let result = run(text, &ctx);
        assert!(!result.contains("Anna"));
        assert!(!result.contains("Weber"));
        assert!(result.contains("WARN [import] übersprungen:"));
    }

    #[test]
    fn catalog_wins_over_home() {
        assert_eq!(run("Import /home/thebexxs/3D-Katalog/Deko/a.3mf", &linux()), "Import <katalog>/Deko/a.3mf");
        assert_eq!(run("Datei /home/thebexxs/Downloads/b.stl", &linux()), "Datei ~/Downloads/b.stl");
    }

    #[test]
    fn windows_paths_with_either_separator() {
        let ctx = Context { home: Some(r"C:\Users\Andreas".into()), user: Some("Andreas".into()), ..Default::default() };
        assert_eq!(run(r"C:\Users\Andreas\Desktop\x.3mf", &ctx), r"~\Desktop\x.3mf");
        assert_eq!(run("C:/Users/Andreas/Desktop/x.3mf", &ctx), "~/Desktop/x.3mf");
    }

    #[test]
    fn user_and_host_as_words_only() {
        assert_eq!(run("user thebexxs on skywalker", &linux()), "user <user> on <host>");
        assert_eq!(run("thebexxsfan stays", &linux()), "thebexxsfan stays");
    }

    #[test]
    fn short_user_names_are_left_alone() {
        let ctx = Context { user: Some("al".into()), ..Default::default() };
        assert_eq!(run("also al", &ctx), "also al");
    }

    #[test]
    fn known_printers_are_numbered_by_list_order_including_bare_host() {
        assert_eq!(
            run("GET 192.168.2.50:7125 and 192.168.2.50 and sv08.local", &linux()),
            "GET <printer-1> and <printer-1> and <printer-2>"
        );
    }

    #[test]
    fn home_and_catalog_are_matched_case_insensitively() {
        let ctx = Context { home: Some("/home/thebexxs".into()), ..Default::default() };
        assert_eq!(run("/Home/thebexxs/x", &ctx), "~/x");
        let win = Context { home: Some(r"C:\Users\Andreas".into()), ..Default::default() };
        assert_eq!(run(r"C:\users\andreas\x", &win), r"~\x");
    }

    #[test]
    fn home_and_catalog_need_a_boundary_after_the_needle() {
        // "/home/thebexxs" must not match as a prefix inside an unrelated,
        // longer directory name of another user.
        let ctx = Context { home: Some("/home/thebexxs".into()), ..Default::default() };
        assert_eq!(run("/home/thebexxs2/x", &ctx), "/home/thebexxs2/x");
        assert_eq!(run("/home/thebexxsBackup/x", &ctx), "/home/thebexxsBackup/x");
    }

    #[test]
    fn configured_printer_hostnames_are_matched_case_insensitively() {
        let ctx = Context {
            printer_addresses: vec!["printer.fritz.box".into()],
            ..Default::default()
        };
        assert_eq!(run("GET Printer.Fritz.Box/api", &ctx), "GET <printer-1>/api");
    }

    #[test]
    fn dotted_file_names_ending_in_local_are_not_hosts() {
        assert_eq!(run("Loaded config: .env.local", &Context::default()), "Loaded config: .env.local");
        assert_eq!(run("vite.config.local.ts", &Context::default()), "vite.config.local.ts");
        assert_eq!(run("GET qidi.local/x", &Context::default()), "GET <ip-1>/x");
    }

    #[test]
    fn local_hostnames_are_matched_case_insensitively() {
        assert_eq!(run("GET SV08.LOCAL/api", &Context::default()), "GET <ip-1>/api");
    }

    #[test]
    fn other_ips_and_local_hosts_are_numbered_stably() {
        assert_eq!(
            run("a 10.0.0.7 b fe80::1c2d c 10.0.0.7 d qidi.local", &Context::default()),
            "a <ip-1> b <ip-2> c <ip-1> d <ip-3>"
        );
    }

    #[test]
    fn times_and_versions_are_not_ips() {
        let s = "2026-10-03 14:02:11 INFO  [update] installiert 0.15.0, neueste 0.15.1 (ModelGrid.tsx:212:15)";
        assert_eq!(run(s, &Context::default()), s);
    }

    #[test]
    fn rust_paths_are_not_ipv6() {
        let s = "failed in app::db::list and fs::read";
        assert_eq!(run(s, &Context::default()), s);
    }

    #[test]
    fn emails() {
        assert_eq!(run("mail max.muster@example.org ok", &Context::default()), "mail <email> ok");
    }

    #[test]
    fn file_names_only_when_asked_and_stable() {
        let mut ctx = linux();
        let s = "/home/thebexxs/Downloads/Drachen v3.3mf und Drachen v3.3mf und rakete.STL";
        assert_eq!(run(s, &ctx), "~/Downloads/Drachen v3.3mf und Drachen v3.3mf und rakete.STL");
        ctx.replace_file_names = true;
        assert_eq!(run(s, &ctx), "~/Downloads/<datei-1>.3mf und Drachen <datei-2>.3mf und <datei-3>.STL");
    }

    #[test]
    fn replaced_parts_are_marked() {
        let segs = anonymize("in /home/thebexxs/x", &linux()).unwrap();
        assert_eq!(
            segs,
            vec![
                Segment { text: "in ".into(), replaced: false },
                Segment { text: "~".into(), replaced: true },
                Segment { text: "/x".into(), replaced: false },
            ]
        );
    }
}
