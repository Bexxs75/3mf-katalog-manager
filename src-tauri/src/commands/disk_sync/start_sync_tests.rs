//! The start sync reports what changed and, only when asked, takes over new files and
//! renames. It never removes anything and never adds copies of present models.
use super::*;
use std::sync::Arc;

const OBJ_A: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
const OBJ_B: &str = "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";
const OBJ_C: &str = "v 0 0 0\nv 3 0 0\nv 0 3 0\nf 1 2 3\n";
const OBJ_D: &str = "v 0 0 0\nv 4 0 0\nv 0 4 0\nf 1 2 3\n";
const OBJ_E: &str = "v 0 0 0\nv 5 0 0\nv 0 5 0\nf 1 2 3\n";

struct World {
    base: PathBuf,
    conn: Connection,
    root: PathBuf,
    gate: Arc<CatalogGate>,
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}
impl World {
    /// A catalog with a settled state on disk.
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!(
            "disk-start-sync-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&base).unwrap();
        let root = base.join("models");
        std::fs::create_dir(&root).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &root, &root).unwrap();
        let mut w = Self { base, conn, root, gate: Arc::new(CatalogGate::default()) };
        for (name, content) in [("stay.obj", OBJ_A), ("gone.obj", OBJ_B), ("old.obj", OBJ_C), ("orig.obj", OBJ_D), ("gonedir/x.obj", OBJ_E)] {
            let path = w.root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        w.catalogue_everything();
        w
    }
    fn catalogue_everything(&mut self) {
        let preview = scan(&self.conn).unwrap();
        let selection = DiskSelection {
            new_paths: preview.new_folders.iter().chain(&preview.new_files).map(|e| e.path.clone()).collect(),
            ..Default::default()
        };
        apply(&mut self.conn, selection).unwrap();
    }
    /// Missing file, missing folder, a rename, a copy of a present model and one new file.
    fn change_disk(&self) {
        std::fs::remove_file(self.root.join("gone.obj")).unwrap();
        std::fs::remove_dir_all(self.root.join("gonedir")).unwrap();
        std::fs::rename(self.root.join("old.obj"), self.root.join("renamed.obj")).unwrap();
        std::fs::write(self.root.join("copy.obj"), OBJ_D).unwrap();
        std::fs::create_dir(self.root.join("fresh")).unwrap();
        std::fs::write(self.root.join("fresh/new.obj"), "v 0 0 0\nv 9 0 0\nv 0 9 0\nf 1 2 3\n").unwrap();
    }
    fn settings(&self, on_start: bool, auto_add: bool) {
        set_flag(&self.conn, SETTING_ON_START, on_start).unwrap();
        set_flag(&self.conn, SETTING_AUTO_ADD, auto_add).unwrap();
    }
    fn run(&mut self) -> StartSyncResult {
        sync_at_start(&mut self.conn, &self.gate, vec![], read_model_with_hash).unwrap()
    }
    fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = db::list_files(&self.conn).unwrap().into_iter().map(|f| f.path).collect();
        paths.sort();
        paths
    }
    fn folders(&self) -> Vec<String> {
        let mut paths: Vec<String> = db::list_folders(&self.conn).unwrap().into_iter().map(|f| f.path).collect();
        paths.sort();
        paths
    }
    fn trash_count(&self) -> usize {
        db::list_trash(&self.conn).unwrap().len()
    }
}

#[test]
fn off_by_default_does_nothing() {
    let mut w = World::new();
    w.change_disk();
    let before = (w.paths(), w.folders());
    let result = w.run();
    assert!(!result.ran && !result.busy && result.applied.is_none());
    assert_eq!((result.new_files, result.renamed, result.missing_files), (0, 0, 0));
    assert_eq!((w.paths(), w.folders()), before);
}

#[test]
fn settings_default_to_off_and_round_trip() {
    let w = World::new();
    assert!(!flag(&w.conn, SETTING_ON_START).unwrap() && !flag(&w.conn, SETTING_AUTO_ADD).unwrap());
    w.settings(true, false);
    assert!(flag(&w.conn, SETTING_ON_START).unwrap() && !flag(&w.conn, SETTING_AUTO_ADD).unwrap());
    w.settings(false, true);
    assert!(!flag(&w.conn, SETTING_ON_START).unwrap() && flag(&w.conn, SETTING_AUTO_ADD).unwrap());
}

#[test]
fn reports_counts_without_applying_anything() {
    let mut w = World::new();
    w.change_disk();
    w.settings(true, false);
    let before = (w.paths(), w.folders());
    let result = w.run();
    assert!(result.ran && !result.busy && result.applied.is_none());
    assert_eq!(
        (result.new_files, result.renamed, result.missing_files, result.missing_folders, result.duplicates, result.unreachable_roots),
        (1, 1, 2, 1, 1, 0)
    );
    assert_eq!((w.paths(), w.folders(), w.trash_count()), (before.0, before.1, 0));
}

#[test]
fn auto_add_takes_over_new_files_and_renames_only() {
    let mut w = World::new();
    w.change_disk();
    w.settings(true, true);
    let result = w.run();
    assert_eq!(result.applied, Some(StartSyncApplied { added: 1, renamed: 1, failed: 0 }));
    // Still open for the user: the missing files (one lies below the missing folder), the folder and the copy.
    assert_eq!((result.missing_files, result.missing_folders, result.duplicates), (2, 1, 1));
    let paths = w.paths();
    let root = w.root.to_string_lossy().into_owned();
    let has = |name: &str| paths.iter().any(|p| Path::new(p) == Path::new(&root).join(name));
    assert!(has("fresh/new.obj"), "new file is taken over");
    assert!(has("renamed.obj") && !has("old.obj"), "rename keeps the row and updates its path");
    assert!(has("gone.obj"), "a missing file stays in the catalog");
    assert!(has("gonedir/x.obj"), "a file below a missing folder stays");
    assert!(!has("copy.obj"), "a copy of a present model is not added");
    assert!(w.folders().iter().any(|p| Path::new(p) == Path::new(&root).join("fresh")));
    assert!(w.folders().iter().any(|p| Path::new(p) == Path::new(&root).join("gonedir")), "a missing folder stays");
    assert_eq!(w.trash_count(), 0);
    // Nothing left to take over, so a second run changes nothing.
    let again = w.run();
    assert_eq!(again.applied, Some(StartSyncApplied::default()));
    assert_eq!((again.new_files, again.renamed), (0, 0));
}

#[test]
fn folder_only_holding_a_copy_is_not_created() {
    let mut w = World::new();
    std::fs::create_dir(w.root.join("copies")).unwrap();
    std::fs::write(w.root.join("copies/dup.obj"), OBJ_D).unwrap();
    w.settings(true, true);
    let result = w.run();
    assert_eq!((result.duplicates, result.applied), (1, Some(StartSyncApplied::default())));
    assert!(!w.folders().iter().any(|p| p.ends_with("copies")));
    assert!(!w.paths().iter().any(|p| p.ends_with("dup.obj")));
}

#[test]
fn auto_add_alone_does_nothing() {
    let mut w = World::new();
    w.change_disk();
    w.settings(false, true);
    let before = (w.paths(), w.folders());
    let result = w.run();
    assert!(!result.ran && result.applied.is_none());
    assert_eq!((w.paths(), w.folders()), before);
}

#[test]
fn unreadable_file_counts_as_failed_and_stops_nothing() {
    let mut w = World::new();
    std::fs::write(w.root.join("broken.3mf"), "incomplete archive").unwrap();
    std::fs::write(w.root.join("fine.obj"), "v 0 0 0\nv 7 0 0\nv 0 7 0\nf 1 2 3\n").unwrap();
    w.settings(true, true);
    let result = w.run();
    assert_eq!(result.applied, Some(StartSyncApplied { added: 1, renamed: 0, failed: 1 }));
    assert!(w.paths().iter().any(|p| p.ends_with("fine.obj")));
}

#[test]
fn busy_when_an_import_runs_or_the_gate_is_taken() {
    let mut w = World::new();
    w.change_disk();
    w.settings(true, true);
    let before = (w.paths(), w.folders());
    {
        let _import = w.gate.import();
        let result = w.run();
        assert!(!result.ran && result.busy && result.applied.is_none());
    }
    {
        let _other = w.gate.exclusive().unwrap();
        let result = w.run();
        assert!(!result.ran && result.busy);
    }
    assert_eq!((w.paths(), w.folders()), before);
    // Once free, the very same call goes through.
    assert!(w.run().ran);
}

#[test]
fn unreachable_root_reports_only_that_and_applies_nothing() {
    let mut w = World::new();
    w.change_disk();
    w.settings(true, true);
    let before = (w.paths(), w.folders());
    std::fs::remove_dir_all(&w.root).unwrap();
    let result = w.run();
    assert!(result.ran && result.applied.is_none());
    assert_eq!(result.unreachable_roots, 1);
    assert_eq!((result.new_files, result.renamed, result.missing_files, result.missing_folders), (0, 0, 0, 0));
    assert_eq!((w.paths(), w.folders()), before);
}

#[test]
fn serialized_result_uses_camel_case() {
    let json = serde_json::to_value(StartSyncResult {
        ran: true,
        applied: Some(StartSyncApplied { added: 2, renamed: 1, failed: 0 }),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(json["ran"], true);
    assert_eq!(json["busy"], false);
    assert_eq!(json["applied"]["added"], 2);
    for key in ["newFiles", "missingFiles", "missingFolders", "duplicates", "unreachableRoots"] {
        assert!(json.get(key).is_some(), "{key}");
    }
}
