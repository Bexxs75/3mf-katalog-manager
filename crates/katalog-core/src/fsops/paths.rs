//! Text helpers for database and journal paths. They go through `NativePath`/`RelPath`, so the
//! rules are the portable ones. A path that is not valid under them gives `None`/`false`, and
//! every caller maps that to the error its own check used to end in.
use super::*;

fn parse(path: &str) -> Option<NativePath> {
    NativePath::parse(path, Style::Posix).ok()
}
/// Whether both texts are valid and name the same place.
pub(super) fn same_place(a: &str, b: &str) -> bool {
    matches!((parse(a), parse(b)), (Some(a), Some(b)) if a.same_place(&b))
}
/// The folder text plus one name, with the join rule of the platform for text that the next
/// check (`FsContext::locate`) validates anyway.
pub(super) fn join_unchecked(folder: &str, name: &str) -> String {
    if folder.is_empty() || folder.ends_with('/') {
        format!("{folder}{name}")
    } else {
        format!("{folder}/{name}")
    }
}
/// `base` with the relative path `rel` (one or more segments) below it.
pub(super) fn join(base: &str, rel: &str) -> Option<String> {
    let joined = parse(base)?.join(&RelPath::parse(rel).ok()?).ok()?;
    Some(joined.as_str().to_owned())
}
pub(super) fn parent(path: &str) -> Option<String> {
    Some(parse(path)?.parent()?.as_str().to_owned())
}
pub(super) fn file_name(path: &str) -> Option<String> {
    parse(path)?.file_name().map(str::to_owned)
}
pub(super) fn with_file_name(path: &str, name: &str) -> Option<String> {
    Some(parse(path)?.with_file_name(name).ok()?.as_str().to_owned())
}
/// Whether `path` is `base` or lies below it. For text that is not valid under the portable
/// rules (stale or damaged rows) the answer stays that of the platform's path comparison:
/// safety checks use this to find rows that must not exist, so a damaged row must still count.
pub(super) fn under(path: &str, base: &str) -> bool {
    match (parse(path), parse(base)) {
        (Some(p), Some(b)) => p.starts_with(&b),
        _ => std::path::Path::new(path).starts_with(base),
    }
}
/// `path` with the leading `old` replaced by `new` (the same place becomes `new`).
pub(super) fn rebase(path: &str, old: &str, new: &str) -> Option<String> {
    Some(parse(path)?.rebase(&parse(old)?, &parse(new)?).ok()?.as_str().to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_splits_and_rebases_valid_text() {
        assert_eq!(join("/m/a", "b/c").as_deref(), Some("/m/a/b/c"));
        assert_eq!(parent("/m/a/b").as_deref(), Some("/m/a"));
        assert_eq!(parent("/m"), None);
        assert_eq!(file_name("/m/a/b.3mf").as_deref(), Some("b.3mf"));
        assert_eq!(with_file_name("/m/a/b", "c").as_deref(), Some("/m/a/c"));
        assert_eq!(rebase("/m/a/b", "/m/a", "/m/z").as_deref(), Some("/m/z/b"));
        assert!(same_place("/m/a", "/m/a") && !same_place("/m/a", "/m/b"));
    }

    #[test]
    fn damaged_text_gives_nothing_instead_of_a_guess() {
        for bad in ["", "m/a", "/m//a", "/m/a/", "/m/./a", "/m/../a", "/m\\a"] {
            assert_eq!(join(bad, "x"), None, "{bad:?}");
            assert_eq!(parent(bad), None, "{bad:?}");
            assert_eq!(file_name(bad), None, "{bad:?}");
            assert_eq!(with_file_name(bad, "x"), None, "{bad:?}");
            assert!(!same_place(bad, bad), "{bad:?}");
        }
        assert_eq!(join("/m", "../x"), None);
        assert_eq!(with_file_name("/m/a", "b/c"), None);
        assert_eq!(rebase("/m/ab", "/m/a", "/m/z"), None);
    }

    #[test]
    fn under_stays_conservative_for_damaged_rows() {
        assert!(under("/m/a/b", "/m/a") && under("/m/a", "/m/a"));
        assert!(!under("/m/ab", "/m/a"));
        // The platform comparison ignores the doubled slash, so a damaged row still counts as inside.
        assert!(under("/m//a/b", "/m/a"));
    }

    #[test]
    fn the_probe_join_does_not_double_a_trailing_slash() {
        assert_eq!(join_unchecked("/m/a", "p"), "/m/a/p");
        assert_eq!(join_unchecked("/m/a/", "p"), "/m/a/p");
        assert_eq!(join_unchecked("", "p"), "p");
    }
}
