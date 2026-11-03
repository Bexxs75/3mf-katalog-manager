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
