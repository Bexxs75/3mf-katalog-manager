//! Catalog folders that are gone from disk are reported by the scan and only removed
//! together with the "remove missing" option.
use super::*;

const OBJ_A: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
const OBJ_B: &str = "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";
const OBJ_C: &str = "v 0 0 0\nv 3 0 0\nv 0 3 0\nf 1 2 3\n";

struct Fixture {
    base: PathBuf,
    conn: Connection,
    root: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!(
            "disk-missing-folder-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&base).unwrap();
        let root = base.join("models");
        std::fs::create_dir(&root).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &root, &root).unwrap();
        let mut f = Self { base, conn, root };
        // A file that stays: an empty root would count as an unplugged volume.
        f.add("keep/stay.obj", OBJ_C);
        f
    }
    fn add(&mut self, relative: &str, content: &str) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        let selection = self.selection(false);
        apply(&mut self.conn, selection).unwrap();
        path
    }
    fn selection(&self, remove_missing: bool) -> DiskSelection {
        let preview = scan(&self.conn).unwrap();
        DiskSelection {
            new_paths: preview.new_folders.iter().chain(&preview.new_files).map(|e| e.path.clone()).collect(),
            missing_ids: preview.missing_files.iter().filter_map(|e| e.id.clone()).collect(),
            renames: preview
                .renamed_files
                .iter()
                .map(|r| RenameChoice { id: r.id.clone(), new_path: r.new_path.clone() })
                .collect(),
            remove_missing,
            ..Default::default()
        }
    }
    fn folder_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = db::list_folders(&self.conn).unwrap().into_iter().map(|f| f.path).collect();
        paths.sort();
        paths
    }
    fn active_files(&self) -> usize {
        db::list_files(&self.conn).unwrap().len()
    }
}

#[test]
fn deleted_subfolder_is_reported_once_at_its_top_with_the_models_below_it() {
    let mut f = Fixture::new();
    f.add("gone/a.obj", OBJ_A);
    f.add("gone/deep/b.obj", OBJ_B);
    std::fs::remove_dir_all(f.root.join("gone")).unwrap();
    let preview = scan(&f.conn).unwrap();
    assert_eq!(preview.missing_folders.len(), 1, "{:?}", preview.missing_folders);
    let folder = &preview.missing_folders[0];
    assert_eq!(folder.path, f.root.join("gone").to_string_lossy());
    assert_eq!(folder.relative_path, "gone");
    assert_eq!(folder.root, f.root.to_string_lossy());
    assert_eq!(folder.model_count, 2);
    assert_eq!(folder.id, f.folder_ids("gone"));
    assert_eq!(preview.missing_files.len(), 2);
    assert!(preview.unreachable_roots.is_empty());
}

#[test]
fn existing_folders_and_catalog_roots_are_never_reported() {
    let f = Fixture::new();
    let preview = scan(&f.conn).unwrap();
    assert!(preview.missing_folders.is_empty());
}

#[test]
fn model_count_ignores_trash_and_counts_an_empty_folder_as_zero() {
    let mut f = Fixture::new();
    let a = f.add("gone/a.obj", OBJ_A);
    f.add("gone/b.obj", OBJ_B);
    f.add("empty-one/x.obj", OBJ_A.replace("1 0 0", "9 0 0").as_str());
    let id: i64 = f.conn.query_row("SELECT id FROM files WHERE path = ?1", [a.to_string_lossy()], |r| r.get(0)).unwrap();
    db::soft_delete_file(&f.conn, id, None, "2026-01-01T00:00:00Z").unwrap();
    std::fs::remove_dir_all(f.root.join("gone")).unwrap();
    let folder_id = db::ensure_folder_path(&f.conn, &f.root, &f.root.join("never-had-models")).unwrap();
    std::fs::create_dir(f.root.join("never-had-models")).unwrap();
    std::fs::remove_dir(f.root.join("never-had-models")).unwrap();
    let preview = scan(&f.conn).unwrap();
    let by_name = |name: &str| preview.missing_folders.iter().find(|e| e.relative_path == name).unwrap();
    assert_eq!(by_name("gone").model_count, 1);
    assert_eq!(by_name("never-had-models").model_count, 0);
    assert_eq!(by_name("never-had-models").id, folder_id.to_string());
}

#[test]
fn renamed_folder_is_not_reported_as_missing() {
    let mut f = Fixture::new();
    f.add("Teile/a.obj", OBJ_A);
    f.add("Teile/sub/b.obj", OBJ_B);
    std::fs::rename(f.root.join("Teile"), f.root.join("Bauteile")).unwrap();
    let preview = scan(&f.conn).unwrap();
    assert_eq!(preview.renamed_files.len(), 2);
    assert!(preview.missing_folders.is_empty(), "{:?}", preview.missing_folders);
}

#[test]
fn folder_moved_below_another_one_is_not_reported_as_missing() {
    let mut f = Fixture::new();
    f.add("Teile/a.obj", OBJ_A);
    f.add("Teile/leer/b.obj", OBJ_B);
    std::fs::create_dir(f.root.join("Archiv")).unwrap();
    std::fs::rename(f.root.join("Teile"), f.root.join("Archiv/Teile")).unwrap();
    let preview = scan(&f.conn).unwrap();
    assert_eq!(preview.renamed_files.len(), 2);
    assert!(preview.missing_folders.is_empty(), "{:?}", preview.missing_folders);
}

#[test]
fn vanished_root_is_unreachable_not_missing_folders() {
    let mut f = Fixture::new();
    f.add("gone/a.obj", OBJ_A);
    std::fs::remove_dir_all(&f.root).unwrap();
    let preview = scan(&f.conn).unwrap();
    assert_eq!(preview.unreachable_roots.len(), 1);
    assert!(preview.missing_folders.is_empty());
}

#[test]
fn emptied_root_like_an_unmounted_volume_does_not_report_folders() {
    let mut f = Fixture::new();
    f.add("gone/a.obj", OBJ_A);
    for entry in std::fs::read_dir(&f.root).unwrap() {
        std::fs::remove_dir_all(entry.unwrap().path()).unwrap();
    }
    let preview = scan(&f.conn).unwrap();
    assert_eq!(preview.unreachable_roots.len(), 1);
    assert!(preview.missing_folders.is_empty());
}

#[test]
fn apply_removes_only_the_reported_folder_and_sends_its_models_to_the_trash() {
    let mut f = Fixture::new();
    f.add("gone/a.obj", OBJ_A);
    f.add("gone/deep/b.obj", OBJ_B);
    f.add("other/c.obj", OBJ_A.replace("1 0 0", "7 0 0").as_str());
    std::fs::remove_dir_all(f.root.join("gone")).unwrap();
    let before = f.folder_paths();
    let selection = f.selection(true);
    let applied = apply(&mut f.conn, selection).unwrap();
    assert_eq!(applied.removed_count, 2);
    let root = f.root.to_string_lossy().into_owned();
    let expected: Vec<String> = ["", "/keep", "/other"].iter().map(|s| format!("{root}{s}")).collect();
    assert_eq!(f.folder_paths(), expected);
    assert!(before.len() > expected.len());
    assert_eq!(db::list_trash(&f.conn).unwrap().len(), 2);
    assert_eq!(f.active_files(), 2);
    assert!(scan(&f.conn).unwrap().missing_folders.is_empty());
}

#[test]
fn apply_without_remove_missing_changes_nothing() {
    let mut f = Fixture::new();
    f.add("gone/a.obj", OBJ_A);
    std::fs::remove_dir_all(f.root.join("gone")).unwrap();
    let before = f.folder_paths();
    let selection = f.selection(false);
    let applied = apply(&mut f.conn, selection).unwrap();
    assert_eq!(applied.removed_count, 0);
    assert_eq!(f.folder_paths(), before);
    assert!(db::list_trash(&f.conn).unwrap().is_empty());
    assert_eq!(f.active_files(), 2);
    assert_eq!(scan(&f.conn).unwrap().missing_folders.len(), 1);
}

#[test]
fn folder_with_a_model_the_user_did_not_see_as_missing_stays() {
    let mut f = Fixture::new();
    f.add("gone/a.obj", OBJ_A);
    std::fs::remove_dir_all(f.root.join("gone")).unwrap();
    let mut selection = f.selection(true);
    selection.missing_ids.clear();
    apply(&mut f.conn, selection).unwrap();
    assert!(f.folder_paths().contains(&f.root.join("gone").to_string_lossy().into_owned()));
    assert_eq!(f.active_files(), 2);
}

#[test]
fn folder_that_exists_again_before_apply_is_kept() {
    let mut f = Fixture::new();
    f.add("gone/a.obj", OBJ_A);
    std::fs::remove_dir_all(f.root.join("gone")).unwrap();
    let selection = f.selection(true);
    std::fs::create_dir_all(f.root.join("gone")).unwrap();
    std::fs::write(f.root.join("gone/a.obj"), OBJ_A).unwrap();
    // The files are back, so the stale selection is rejected and nothing is removed.
    assert!(apply(&mut f.conn, selection).is_err());
    assert!(f.folder_paths().contains(&f.root.join("gone").to_string_lossy().into_owned()));
    assert_eq!(f.active_files(), 2);
}

#[test]
fn folder_tree_check_marks_gone_folders_and_skips_unknown_roots() {
    let mut f = Fixture::new();
    f.add("gone/a.obj", OBJ_A);
    f.add("gone/deep/b.obj", OBJ_B);
    std::fs::remove_dir_all(f.root.join("gone")).unwrap();
    let folders = db::list_folders(&f.conn).unwrap();
    let name_of = |ids: HashSet<i64>| {
        let mut names: Vec<String> = folders.iter().filter(|x| ids.contains(&x.id)).map(|x| x.name.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(name_of(missing_folder_ids(&folders)), ["deep", "gone"]);
    // Same catalog, root unreadable: every folder is unknown, none is "missing".
    std::fs::remove_dir_all(&f.root).unwrap();
    assert!(missing_folder_ids(&folders).is_empty());
    // A mount point without its volume is empty and also unknown.
    std::fs::create_dir(&f.root).unwrap();
    assert!(missing_folder_ids(&folders).is_empty());
}

impl Fixture {
    fn folder_ids(&self, relative: &str) -> String {
        let path = self.root.join(relative);
        db::list_folders(&self.conn)
            .unwrap()
            .into_iter()
            .find(|f| Path::new(&f.path) == path)
            .unwrap()
            .id
            .to_string()
    }
}
