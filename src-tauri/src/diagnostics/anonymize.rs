//! Replaces personal data in a log excerpt before it leaves the computer.
//! Returns segments so the preview can highlight exactly what was replaced.
use std::collections::HashMap;

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

pub fn to_text(segs: &[Segment]) -> String {
    segs.iter().map(|s| s.text.as_str()).collect()
}

pub fn anonymize(text: &str, ctx: &Context) -> Vec<Segment> {
    let mut segs = vec![Segment { text: text.to_string(), replaced: false }];

    // Longest first, so a catalog inside the home folder becomes <katalog>, not ~/....
    let mut literals: Vec<(String, String)> = Vec::new();
    for root in &ctx.catalog_roots {
        for v in path_variants(root) {
            literals.push((v, "<katalog>".into()));
        }
    }
    if let Some(home) = &ctx.home {
        for v in path_variants(home) {
            literals.push((v, "~".into()));
        }
    }
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
        segs = replace_literal(segs, needle, repl);
    }

    let email = Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap();
    segs = replace_group(segs, &email, 0, |_| Some("<email>".into()));

    // IPv4, *.local hosts and IPv6 (at least three colons or a "::", so clock
    // times like 14:02:11 never match).
    let ip = Regex::new(
        r"\b(?:\d{1,3}\.){3}\d{1,3}\b|\b[0-9A-Za-z-]+\.local\b|\b(?:[0-9a-fA-F]{1,4}:){3,7}[0-9a-fA-F]{1,4}\b|\b[0-9a-fA-F]{1,4}(?::[0-9a-fA-F]{1,4})*::(?:[0-9a-fA-F]{1,4}(?::[0-9a-fA-F]{1,4})*)?",
    )
    .unwrap();
    let mut ips: HashMap<String, usize> = HashMap::new();
    segs = replace_group(segs, &ip, 0, |m| {
        // Real addresses always contain a digit; Rust paths like `db::list` don't.
        // `*.local` hostnames are exempt: a hostname like `qidi.local` is a real
        // address even without a digit in its name.
        if !m.to_ascii_lowercase().ends_with(".local") && !m.chars().any(|c| c.is_ascii_digit()) {
            return None;
        }
        let next = ips.len() + 1;
        let n = *ips.entry(m.to_string()).or_insert(next);
        Some(format!("<ip-{n}>"))
    });

    for (value, label) in [(&ctx.host, "<host>"), (&ctx.user, "<user>")] {
        if let Some(v) = value {
            if v.chars().count() >= 3 {
                let re = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(v))).unwrap();
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
        let in_path = Regex::new(&format!(r#"(?i)[/\\]([^/\\"<>|:\r\n]+?)\.(?:{FILE_EXTENSIONS})\b"#)).unwrap();
        segs = replace_group(segs, &in_path, 1, &mut label);
        let bare = Regex::new(&format!(r#"(?i)(?:^|[\s("'])([^\s/\\"'<>|:()]+)\.(?:{FILE_EXTENSIONS})\b"#)).unwrap();
        segs = replace_group(segs, &bare, 1, &mut label);
    }

    merge_plain(segs)
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

fn replace_literal(segs: Vec<Segment>, needle: &str, repl: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    for s in segs {
        if s.replaced || !s.text.contains(needle) {
            out.push(s);
            continue;
        }
        let mut parts = s.text.split(needle).peekable();
        while let Some(part) = parts.next() {
            push_plain(&mut out, part);
            if parts.peek().is_some() {
                out.push(Segment { text: repl.to_string(), replaced: true });
            }
        }
    }
    out
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
        to_text(&anonymize(text, ctx))
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
        let segs = anonymize("in /home/thebexxs/x", &linux());
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
