//! Roadmap 1g: "Import reports a duplicate, the disk sync offers the same file as new".
//!
//! These tests REPRODUCE TODAY'S BEHAVIOUR and keep it as the expected result, so
//! they are green. They do not assert that the problem is fixed. When 1g is
//! fixed, the tests marked "reproduces today's behaviour" must be rewritten.
//! See /mnt/claude/claudex-runs/docs/befund-1g-doppelt-neu.md.
use super::*;

const OBJ_A: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
const OBJ_B: &str = "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";

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
        let base = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "roadmap-1g-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir(&base).unwrap();
        let root = base.join("models");
        std::fs::create_dir(&root).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &root, &root).unwrap();
        Self { base, conn, root }
    }
    /// Offers everything the scan reports as new and applies it, like the dialog.
    fn sync_all(&mut self) -> DiskApplied {
        let preview = scan(&self.conn).unwrap();
        let selection = DiskSelection {
            new_paths: preview
                .new_folders
                .into_iter()
                .chain(preview.new_files)
                .map(|e| e.path)
                .collect(),
            missing_ids: Vec::new(),
            remove_missing: false,
        };
        apply(&mut self.conn, selection).unwrap()
    }
    fn new_files(&self) -> Vec<String> {
        scan(&self.conn)
            .unwrap()
            .new_files
            .into_iter()
            .map(|e| e.path)
            .collect()
    }
    /// Runs the real batch import (same store transaction as the job import).
    fn import(&mut self, paths: &[&Path]) -> files::ImportResultDto {
        files::import_many_with_conn(&mut self.conn, paths.iter().map(|p| p.to_path_buf()).collect())
            .unwrap()
    }
}
fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// H1, file manager variant: a copy lands in the catalog folder, a model with the
/// same content is already catalogued. Import: "duplicate (hash)", file stays on
/// disk. Sync: offers the very same file as new.
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h1_import_rejects_hash_duplicate_in_catalog_folder_and_sync_offers_it_as_new() {
    let mut f = Fixture::new();
    let original = f.root.join("original.obj");
    std::fs::write(&original, OBJ_A).unwrap();
    f.sync_all();
    let copy = f.root.join("copy.obj");
    std::fs::copy(&original, &copy).unwrap();

    let result = f.import(&[&copy]);
    assert!(result.imported.is_empty());
    assert_eq!(result.duplicate_count, 1);
    assert_eq!(result.duplicate_entries[0].kind, "hash");
    assert_eq!(result.duplicate_entries[0].path, s(&copy));
    assert!(copy.exists(), "the rejected file stays where it is");

    assert_eq!(f.new_files(), vec![s(&copy)], "sync offers the rejected file as new");
    // Applying it creates the hash duplicate the import refused.
    let applied = f.sync_all();
    assert_eq!(applied.file_ids.len(), 1);
    let files = db::list_files(&f.conn).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].content_hash, files[1].content_hash);
    assert!(f.new_files().is_empty(), "afterwards it is known by path");
}

/// H1, archive/extracted folder variant: mixed content, only the hash duplicate
/// stays unimported inside the new catalog folder and is offered by the next sync.
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h1_mixed_folder_import_leaves_hash_duplicate_that_sync_offers() {
    let mut f = Fixture::new();
    let original = f.root.join("original.obj");
    std::fs::write(&original, OBJ_A).unwrap();
    f.sync_all();
    let extracted = f.root.join("extracted");
    std::fs::create_dir(&extracted).unwrap();
    let dup = extracted.join("dup.obj");
    let fresh = extracted.join("fresh.obj");
    std::fs::write(&dup, OBJ_A).unwrap();
    std::fs::write(&fresh, OBJ_B).unwrap();

    let result = f.import(&[&extracted]);
    assert_eq!(result.imported.len(), 1);
    assert_eq!(result.imported[0].path, s(&fresh));
    assert_eq!(result.duplicate_count, 1);
    assert_eq!(result.duplicate_entries[0].kind, "hash");
    assert_eq!(result.duplicate_entries[0].path, s(&dup));

    assert_eq!(f.new_files(), vec![s(&dup)]);
}

/// H1, control: a hash duplicate OUTSIDE every catalog folder is not offered by the
/// sync. The mismatch only shows when the rejected file lies inside a catalog folder.
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h1_control_hash_duplicate_outside_catalog_folders_is_not_offered() {
    let mut f = Fixture::new();
    let original = f.root.join("original.obj");
    std::fs::write(&original, OBJ_A).unwrap();
    f.sync_all();
    let outside = f.base.join("outside.obj");
    std::fs::write(&outside, OBJ_A).unwrap();
    let result = f.import(&[&outside]);
    assert_eq!(result.duplicate_entries[0].kind, "hash");
    assert!(f.new_files().is_empty());
}

/// H2 (symlink alias): the import compares the path as a raw string. The same
/// physical file reached through a symlink to the catalog folder is therefore not
/// a "path" duplicate but only caught by its hash (mislabelled "hash"). The sync
/// resolves the alias and does not offer the file.
/// Reproduces today's behaviour, see Roadmap 1g. Not the reported symptom.
#[cfg(unix)]
#[test]
fn h2_symlink_alias_is_reported_as_hash_not_path_duplicate_and_sync_does_not_offer_it() {
    let mut f = Fixture::new();
    let original = f.root.join("a.obj");
    std::fs::write(&original, OBJ_A).unwrap();
    f.sync_all();
    let alias = f.base.join("alias");
    std::os::unix::fs::symlink(&f.root, &alias).unwrap();

    let via_alias = alias.join("a.obj");
    let result = f.import(&[&via_alias]);
    assert_eq!(result.duplicate_entries[0].kind, "hash");
    assert!(!db::file_exists_by_path(&f.conn, &s(&via_alias)).unwrap());
    assert!(f.new_files().is_empty());
}

/// H2 (symlink alias, new content): a file imported through a symlink to the catalog
/// folder is stored under the alias spelling. The sync resolves only paths below
/// the stored/canonical root, so it does NOT recognise the stored alias path and
/// offers the same physical file as new. Applying it adds a second row with the
/// same hash (the sync never checks hashes).
/// Reproduces today's behaviour, see Roadmap 1g. This is the same symptom as the
/// reported one ("imported, yet offered as new") without any hash rejection.
#[cfg(unix)]
#[test]
fn h2_symlink_alias_import_of_new_content_is_offered_again_by_sync() {
    let mut f = Fixture::new();
    let alias = f.base.join("alias");
    std::os::unix::fs::symlink(&f.root, &alias).unwrap();
    std::fs::write(f.root.join("b.obj"), OBJ_B).unwrap();
    let via_alias = alias.join("b.obj");
    let result = f.import(&[&via_alias]);
    assert_eq!(result.imported.len(), 1);
    assert_eq!(result.imported[0].path, s(&via_alias));
    assert_eq!(f.new_files(), vec![s(&f.root.join("b.obj"))]);
    f.sync_all();
    let files = db::list_files(&f.conn).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].content_hash, files[1].content_hash);
}

/// H2 (Unicode): `path_key` folds case only; NFC and NFD spellings of one name stay
/// different keys. On Linux (ext4) the two spellings are two different files, so
/// the macOS situation (APFS: one file, two spellings) can NOT be reproduced here.
/// What is shown: a catalog row stored in NFD spelling next to a disk entry in NFC
/// spelling is offered as new file (and the row as missing). Simulation, not a
/// real macOS run: "not reproduced" for the real macOS case.
#[test]
fn h2_unicode_spellings_are_different_keys_and_simulated_nfd_row_next_to_nfc_file_is_offered() {
    let nfc = "W\u{00fc}rfel.obj";
    let nfd = "Wu\u{0308}rfel.obj";
    assert_ne!(nfc, nfd);
    for case_insensitive in [false, true] {
        assert_ne!(
            path_key(Path::new(nfc), case_insensitive),
            path_key(Path::new(nfd), case_insensitive)
        );
    }
    let mut f = Fixture::new();
    let on_disk = f.root.join(nfc);
    std::fs::write(&on_disk, OBJ_A).unwrap();
    let preview = scan(&f.conn).unwrap();
    assert_eq!(preview.new_files.len(), 1);
    // Row as an import via an NFD path string would have written it.
    let nfd_path = f.root.join(nfd);
    assert!(!nfd_path.exists(), "ext4 keeps the spellings apart");
    let mut file = files::read_model_file(&on_disk, None).unwrap();
    file.path = s(&nfd_path);
    let folder = db::ensure_folder_path(&f.conn, &f.root, &f.root).unwrap();
    file.folder_id = Some(folder);
    db::insert_file(&mut f.conn, &file).unwrap();
    let preview = scan(&f.conn).unwrap();
    assert_eq!(preview.new_files.len(), 1, "NFC file offered although the model exists");
    assert_eq!(preview.missing_files.len(), 1);
}

/// H2 (case): the import path check is an exact string comparison, independent of
/// the platform. On a case-insensitive file system (macOS/Windows default) the same
/// file in another spelling would be a new path for the import. Only the string
/// comparison is shown here; Linux cannot show the file system part.
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h2_import_path_check_is_case_sensitive_string_compare() {
    let mut f = Fixture::new();
    let original = f.root.join("Model.obj");
    std::fs::write(&original, OBJ_A).unwrap();
    f.sync_all();
    assert!(db::file_exists_by_path(&f.conn, &s(&original)).unwrap());
    assert!(!db::file_exists_by_path(&f.conn, &s(&f.root.join("model.obj"))).unwrap());
}

/// H3 (i): a file that cannot be read when applying stays "new" at every run.
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h3_unreadable_file_is_offered_again_at_every_sync() {
    let mut f = Fixture::new();
    let broken = f.root.join("broken.3mf");
    std::fs::write(&broken, "incomplete archive").unwrap();
    for _ in 0..2 {
        assert_eq!(f.new_files(), vec![s(&broken)]);
        let applied = f.sync_all();
        assert!(applied.file_ids.is_empty());
        assert_eq!(applied.failed_paths.len(), 1);
    }
}

/// H3 (ii): a model removed from the catalog (row deleted, file stays on disk) is
/// offered as new by the next sync. Intended behaviour, but it looks like "comes
/// back every time".
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h3_model_removed_from_catalog_is_offered_again() {
    let mut f = Fixture::new();
    let path = f.root.join("gone.obj");
    std::fs::write(&path, OBJ_A).unwrap();
    let id: i64 = f.sync_all().file_ids[0].parse().unwrap();
    assert!(f.new_files().is_empty());
    db::delete_file(&f.conn, id).unwrap();
    assert_eq!(f.new_files(), vec![s(&path)]);
}

/// H3 (iii): hidden names. The batch import (Dropped/Folder path through
/// `collect_supported_files`) accepts a dot file, the sync never lists it.
/// Not the reported symptom (sync does not OFFER it); shows different rules.
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h3_hidden_file_is_imported_by_batch_import_but_ignored_by_sync() {
    let mut f = Fixture::new();
    let hidden = f.root.join(".hidden.obj");
    std::fs::write(&hidden, OBJ_A).unwrap();
    assert!(f.new_files().is_empty());
    let result = f.import(&[&hidden]);
    assert_eq!(result.imported.len(), 1);
    let preview = scan(&f.conn).unwrap();
    assert!(preview.new_files.is_empty());
    assert!(preview.missing_files.is_empty());
}

/// H4: a file whose row sits in the trash. The sync treats the trash row's path as
/// known (not offered). The import: the first path check ignores trash rows, but the
/// check inside the store transaction does not, so the file is reported as a "path"
/// duplicate (not as a UNIQUE failure, as the analysis assumed). Hidden from both.
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h4_file_at_trashed_path_is_neither_imported_nor_offered() {
    let mut f = Fixture::new();
    let path = f.root.join("trashed.obj");
    std::fs::write(&path, OBJ_A).unwrap();
    let id: i64 = f.sync_all().file_ids[0].parse().unwrap();
    db::soft_delete_file(&f.conn, id, None, "2026-10-20T00:00:00Z").unwrap();
    assert!(path.exists(), "same content put back at the same path");

    assert!(f.new_files().is_empty());
    assert!(!db::file_exists_by_path(&f.conn, &s(&path)).unwrap());
    let result = f.import(&[&path]);
    assert!(result.imported.is_empty());
    assert_eq!(result.duplicate_entries[0].kind, "path");
    assert!(result.skipped.is_empty());
    assert!(f.new_files().is_empty());
}

/// H4 variant: the same path with DIFFERENT content (new version of the model).
/// Hash is new, but the path row (trash) still blocks it.
/// Reproduces today's behaviour, see Roadmap 1g.
#[test]
fn h4_new_content_at_trashed_path_is_reported_as_path_duplicate() {
    let mut f = Fixture::new();
    let path = f.root.join("trashed.obj");
    std::fs::write(&path, OBJ_A).unwrap();
    let id: i64 = f.sync_all().file_ids[0].parse().unwrap();
    db::soft_delete_file(&f.conn, id, None, "2026-10-20T00:00:00Z").unwrap();
    std::fs::write(&path, OBJ_B).unwrap();
    let result = f.import(&[&path]);
    assert!(result.imported.is_empty());
    assert_eq!(result.duplicate_entries[0].kind, "path");
    assert!(f.new_files().is_empty());
}
