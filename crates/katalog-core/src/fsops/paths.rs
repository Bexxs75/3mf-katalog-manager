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
