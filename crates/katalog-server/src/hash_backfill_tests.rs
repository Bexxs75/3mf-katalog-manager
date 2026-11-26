use super::*;
use std::io::Read;

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let dir = std::env::temp_dir()
            .join(format!("hash-backfill-{:032x}", u128::from_ne_bytes(random)));
        for sub in ["models/sub", "outside"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let dir = dir.canonicalize().unwrap();
        std::fs::write(dir.join("models/sub/a.stl"), b"inside").unwrap();
        std::fs::write(dir.join("outside/secret.stl"), b"outside").unwrap();
        Self(dir)
    }
    fn roots(&self) -> Vec<PathBuf> {
        vec![self.0.join("models")]
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_file_below_a_root_is_opened() {
    let t = Tree::new();
    let mut text = String::new();
    open_admitted(&t.roots(), &t.0.join("models/sub/a.stl"))
        .expect("admitted")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "inside");
}

#[test]
fn paths_outside_the_roots_are_refused_without_reading() {
    let t = Tree::new();
    let roots = t.roots();
    for path in [
        t.0.join("outside/secret.stl"),
        t.0.join("models/../outside/secret.stl"),
        t.0.join("models-other/a.stl"),
        PathBuf::from("models/sub/a.stl"),
        PathBuf::from("/etc/hostname"),
        PathBuf::new(),
    ] {
        assert!(open_admitted(&roots, &path).is_none(), "{}", path.display());
    }
    // No root at all admits nothing.
    assert!(open_admitted(&[], &t.0.join("models/sub/a.stl")).is_none());
}

#[cfg(unix)]
#[test]
fn links_are_refused_whether_they_are_the_file_or_a_folder_on_the_way() {
    let t = Tree::new();
    let roots = t.roots();
    std::os::unix::fs::symlink(t.0.join("outside/secret.stl"), t.0.join("models/link.stl")).unwrap();
    std::os::unix::fs::symlink(t.0.join("outside"), t.0.join("models/away")).unwrap();
    // Even a link that stays inside the roots is no catalog file.
    std::os::unix::fs::symlink(t.0.join("models/sub/a.stl"), t.0.join("models/inner.stl")).unwrap();
    for name in ["models/link.stl", "models/away/secret.stl", "models/inner.stl"] {
        assert!(open_admitted(&roots, &t.0.join(name)).is_none(), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn a_root_reached_through_a_link_still_admits_its_own_files_only() {
    let t = Tree::new();
    std::os::unix::fs::symlink(t.0.join("models"), t.0.join("catalog")).unwrap();
    let roots = vec![t.0.join("catalog")];
    assert!(open_admitted(&roots, &t.0.join("catalog/sub/a.stl")).expect("admitted").is_ok());
    assert!(open_admitted(&roots, &t.0.join("outside/secret.stl")).is_none());
}

#[test]
fn missing_files_and_folders_are_failures_not_refusals() {
    let t = Tree::new();
    let roots = t.roots();
    for name in ["models/gone.stl", "models/sub"] {
        assert!(open_admitted(&roots, &t.0.join(name)).expect("admitted").is_err(), "{name}");
    }
}

#[test]
fn a_cancelled_read_ends_at_the_next_chunk() {
    let cancel = AtomicBool::new(false);
    let mut reader = Cancellable(&b"abcdef"[..], &cancel);
    let mut buf = [0; 3];
    assert_eq!(reader.read(&mut buf).unwrap(), 3);
    cancel.store(true, Ordering::SeqCst);
    assert!(reader.read(&mut buf).is_err());
    assert!(compute_reader_hash(Cancellable(&b"abcdef"[..], &cancel)).is_err());
}
