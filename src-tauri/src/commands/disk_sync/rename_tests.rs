//! Disk sync recognises files that were renamed or moved outside the app (same content
//! hash) and offers copies of present models in their own group.
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
            "disk-rename-{}-{}",
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
    /// Writes a model into the catalog folder and catalogues it through the real sync.
    fn add(&mut self, relative: &str, content: &str) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        self.apply_all();
        path
    }
    fn id_of(&self, path: &Path) -> i64 {
        self.conn
            .query_row("SELECT id FROM files WHERE path = ?1", [path.to_string_lossy()], |r| r.get(0))
            .unwrap()
    }
    fn selection(&self, include_duplicates: bool) -> DiskSelection {
        let preview = scan(&self.conn).unwrap();
        let mut new_paths: Vec<String> = preview
            .new_folders
            .iter()
            .chain(&preview.new_files)
            .map(|e| e.path.clone())
            .collect();
        if include_duplicates {
            new_paths.extend(preview.duplicate_files.iter().map(|d| d.entry.path.clone()));
        }
        DiskSelection {
            new_paths,
            renames: preview
                .renamed_files
                .iter()
                .map(|r| RenameChoice { id: r.id.clone(), new_path: r.new_path.clone() })
                .collect(),
            ..Default::default()
        }
    }
    fn apply_all(&mut self) -> DiskApplied {
        let selection = self.selection(true);
        apply(&mut self.conn, selection).unwrap()
    }
    fn scan(&self) -> DiskScan {
        scan(&self.conn).unwrap()
    }
    fn folder(&self, path: &Path) -> Option<db::models::FolderRecord> {
        db::list_folders(&self.conn).unwrap().into_iter().find(|f| Path::new(&f.path) == path)
    }
}
fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// ---- detection ----

#[test]
fn renamed_file_is_paired_by_content() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    let id = f.id_of(&old);
    let new = f.root.join("Vase_v2.obj");
    std::fs::rename(&old, &new).unwrap();
    let preview = f.scan();
    assert_eq!(preview.renamed_files.len(), 1);
    assert_eq!(preview.renamed_files[0].id, id.to_string());
    assert_eq!(preview.renamed_files[0].old_path, s(&old));
    assert_eq!(preview.renamed_files[0].new_path, s(&new));
    assert_eq!(preview.renamed_files[0].new_relative_path, "Vase_v2.obj");
    assert!(preview.new_files.is_empty());
    assert!(preview.missing_files.is_empty());
    assert!(preview.duplicate_files.is_empty());
}

#[test]
fn file_moved_to_another_folder_is_paired_and_the_new_folder_is_not_offered() {
    let mut f = Fixture::new();
    let old = f.add("Deko/Vase.obj", OBJ_A);
    f.add("Halter/Clip.obj", OBJ_B);
    std::fs::create_dir(f.root.join("Neu")).unwrap();
    let new = f.root.join("Neu/Vase.obj");
    std::fs::rename(&old, &new).unwrap();
    let preview = f.scan();
    assert_eq!(preview.renamed_files.len(), 1);
    assert_eq!(preview.renamed_files[0].new_path, s(&new));
    assert!(preview.new_files.is_empty() && preview.missing_files.is_empty());
    assert!(preview.new_folders.is_empty(), "a folder that only holds a moved file is not new");
}

#[test]
fn renamed_folder_pairs_every_file() {
    let mut f = Fixture::new();
    f.add("Drucker-Teile/Halter.obj", OBJ_A);
    f.add("Drucker-Teile/Clip.obj", OBJ_B);
    std::fs::rename(f.root.join("Drucker-Teile"), f.root.join("Druckerteile")).unwrap();
    let preview = f.scan();
    assert_eq!(preview.renamed_files.len(), 2);
    assert!(preview.new_files.is_empty() && preview.missing_files.is_empty() && preview.new_folders.is_empty());
}

#[test]
fn two_new_files_with_the_same_content_are_not_paired() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    std::fs::rename(&old, f.root.join("a.obj")).unwrap();
    std::fs::write(f.root.join("b.obj"), OBJ_A).unwrap();
    let preview = f.scan();
    assert!(preview.renamed_files.is_empty());
    assert_eq!(preview.new_files.len(), 2);
    assert_eq!(preview.missing_files.len(), 1);
}

#[test]
fn two_missing_files_with_the_same_content_are_not_paired() {
    let mut f = Fixture::new();
    let first = f.add("a.obj", OBJ_A);
    let second = f.add("b.obj", OBJ_A);
    std::fs::rename(&first, f.root.join("c.obj")).unwrap();
    std::fs::remove_file(&second).unwrap();
    let preview = f.scan();
    assert!(preview.renamed_files.is_empty());
    assert_eq!(preview.new_files.len(), 1);
    assert_eq!(preview.missing_files.len(), 2);
}

#[test]
fn missing_file_without_stored_hash_is_never_paired() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    f.conn.execute("UPDATE files SET content_hash = NULL", []).unwrap();
    std::fs::rename(&old, f.root.join("Vase2.obj")).unwrap();
    let preview = f.scan();
    assert!(preview.renamed_files.is_empty());
    assert_eq!(preview.new_files.len(), 1);
    assert_eq!(preview.missing_files.len(), 1);
}

#[test]
fn changed_extension_is_not_paired() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    std::fs::rename(&old, f.root.join("Vase.stl")).unwrap();
    let preview = f.scan();
    assert!(preview.renamed_files.is_empty());
    assert_eq!(preview.new_files.len(), 1);
    assert_eq!(preview.missing_files.len(), 1);
}

#[test]
fn copy_of_a_present_model_goes_to_the_duplicate_group_not_to_new() {
    let mut f = Fixture::new();
    let original = f.add("Vase.obj", OBJ_A);
    let copy = f.root.join("Vase - Kopie.obj");
    std::fs::copy(&original, &copy).unwrap();
    std::fs::write(f.root.join("Schale.obj"), OBJ_B).unwrap();
    let preview = f.scan();
    assert_eq!(preview.duplicate_files.len(), 1);
    assert_eq!(preview.duplicate_files[0].entry.path, s(&copy));
    assert_eq!(preview.duplicate_files[0].duplicate_of_path, s(&original));
    assert_eq!(preview.duplicate_files[0].duplicate_of_name, "Vase.obj");
    assert_eq!(preview.new_files.len(), 1);
    assert!(preview.new_files[0].path.ends_with("Schale.obj"));
    assert!(preview.renamed_files.is_empty());
}

#[test]
fn copy_of_a_present_model_is_not_taken_for_the_move_of_a_missing_twin() {
    let mut f = Fixture::new();
    let first = f.add("a.obj", OBJ_A);
    let second = f.add("b.obj", OBJ_A);
    std::fs::remove_file(&first).unwrap();
    std::fs::copy(&second, f.root.join("c.obj")).unwrap();
    let preview = f.scan();
    assert!(preview.renamed_files.is_empty());
    assert_eq!(preview.duplicate_files.len(), 1);
    assert_eq!(preview.missing_files.len(), 1);
}

#[test]
fn scan_does_not_hash_when_nothing_could_match() {
    let mut f = Fixture::new();
    f.add("a.obj", OBJ_A);
    std::fs::write(f.root.join("b.obj"), OBJ_B).unwrap();
    let hashed = std::cell::Cell::new(0);
    let mut preview = scan(&f.conn).unwrap();
    assert_eq!(preview.new_files.len(), 1);
    // Only the new file may be hashed, never the catalogued one.
    classify_by_content(&f.conn, &mut preview, false, &|_| {
        hashed.set(hashed.get() + 1);
        None
    })
    .unwrap();
    assert_eq!(hashed.get(), 1);
}

#[test]
fn selections_from_before_renames_existed_still_deserialize() {
    let old: DiskSelection =
        serde_json::from_str(r#"{"newPaths":["/a"],"missingIds":["1"],"removeMissing":true}"#).unwrap();
    assert!(old.renames.is_empty());
    assert_eq!(old.new_paths, vec!["/a".to_string()]);
}

// ---- applying ----

#[test]
fn apply_moves_the_existing_row_and_keeps_everything_attached_to_it() {
    let mut f = Fixture::new();
    let old = f.add("Deko/Vase.obj", OBJ_A);
    let id = f.id_of(&old);
    db::add_tag_to_file(&f.conn, id, "deko").unwrap();
    db::set_favorite(&f.conn, id, true).unwrap();
    db::insert_print_log_entry(
        &f.conn,
        &db::models::NewPrintLogEntry {
            file_id: id,
            printed_at: "2026-01-01".into(),
            note: Some("gut geworden".into()),
            photo_png: None,
        },
    )
    .unwrap();
    let collection = db::create_collection(&f.conn, "Favoriten", "2026-01-01").unwrap();
    db::add_file_to_collection(&f.conn, collection, id, 0).unwrap();
    let tags_before: i64 = f.conn.query_row("SELECT count(*) FROM file_tags WHERE file_id = ?1", [id], |r| r.get(0)).unwrap();
    assert!(tags_before >= 1);
    let files_before: i64 = f.conn.query_row("SELECT count(*) FROM files", [], |r| r.get(0)).unwrap();

    std::fs::create_dir(f.root.join("Neu")).unwrap();
    let new = f.root.join("Neu/Vase_v2.obj");
    std::fs::rename(&old, &new).unwrap();
    let applied = f.apply_all();
    assert_eq!(applied.renamed_count, 1);
    assert_eq!(applied.renamed_ids, vec![id.to_string()]);
    assert!(applied.file_ids.is_empty() && applied.failed_paths.is_empty());

    let files_after: i64 = f.conn.query_row("SELECT count(*) FROM files", [], |r| r.get(0)).unwrap();
    assert_eq!(files_before, files_after, "no second entry");
    let (path, name, folder_id, favorite): (String, String, i64, bool) = f
        .conn
        .query_row("SELECT path, name, folder_id, favorite FROM files WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!(path, s(&new));
    assert_eq!(name, "Vase_v2.obj");
    assert_eq!(f.folder(&f.root.join("Neu")).unwrap().id, folder_id);
    assert!(favorite);
    let count = |sql: &str| -> i64 { f.conn.query_row(sql, [id], |r| r.get(0)).unwrap() };
    assert_eq!(count("SELECT count(*) FROM file_tags WHERE file_id = ?1"), tags_before);
    assert_eq!(count("SELECT count(*) FROM print_log WHERE file_id = ?1 AND note = 'gut geworden'"), 1);
    assert_eq!(count("SELECT count(*) FROM collection_files WHERE file_id = ?1"), 1);
    let again = f.scan();
    assert!(again.renamed_files.is_empty() && again.new_files.is_empty() && again.missing_files.is_empty());
}

#[test]
fn deselected_rename_changes_nothing() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    std::fs::rename(&old, f.root.join("Vase2.obj")).unwrap();
    let mut selection = f.selection(false);
    selection.renames.clear();
    selection.new_paths.clear();
    let applied = apply(&mut f.conn, selection).unwrap();
    assert_eq!(applied.renamed_count, 0);
    assert_eq!(f.conn.query_row("SELECT path FROM files", [], |r| r.get::<_, String>(0)).unwrap(), s(&old));
}

#[test]
fn content_changed_before_apply_prevents_the_pairing() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    let new = f.root.join("Vase2.obj");
    std::fs::rename(&old, &new).unwrap();
    let selection = f.selection(false);
    assert_eq!(selection.renames.len(), 1);
    // The user edits the file after the dialog was shown: the fresh scan no longer pairs.
    std::fs::write(&new, OBJ_C).unwrap();
    let error = apply(&mut f.conn, selection).err().unwrap();
    assert_eq!(error.code, "diskSyncStale");
    assert_eq!(f.conn.query_row("SELECT path FROM files", [], |r| r.get::<_, String>(0)).unwrap(), s(&old));
}

#[test]
fn verify_rename_rejects_a_target_whose_hash_changed_after_the_scan() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    let new = f.root.join("Vase2.obj");
    std::fs::rename(&old, &new).unwrap();
    let preview = f.scan();
    let rename = &preview.renamed_files[0];
    assert!(verify_rename(&f.conn, &preview, rename, &[]).is_ok());
    std::fs::write(&new, OBJ_C).unwrap();
    assert!(verify_rename(&f.conn, &preview, rename, &[]).is_err());
}

#[test]
fn target_replaced_while_other_models_are_read_leaves_the_row_untouched() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    let new = f.root.join("Vase2.obj");
    std::fs::rename(&old, &new).unwrap();
    // A new model forces the (possibly minutes long) read after the first check.
    let fresh = f.root.join("Fresh.obj");
    std::fs::write(&fresh, OBJ_B).unwrap();
    let selection = f.selection(false);
    assert_eq!(selection.renames.len(), 1);
    let target = new.clone();
    let applied = apply_with_reader(&mut f.conn, selection, |path| {
        // Another program puts a different model at the pairing target meanwhile.
        std::fs::write(&target, OBJ_C).unwrap();
        let mut file = files::read_import_model_file(path, None)?;
        file.content_hash = Some(files::compute_content_hash(path)?);
        Ok(file)
    })
    .unwrap();
    assert_eq!(applied.renamed_count, 0);
    assert_eq!(applied.failed_paths.len(), 1);
    assert_eq!(applied.failed_paths[0].path, s(&new));
    assert_eq!(applied.file_ids.len(), 1, "the other chosen model is still imported");
    let row: String = f
        .conn
        .query_row("SELECT path FROM files WHERE path = ?1", [s(&old)], |r| r.get(0))
        .unwrap();
    assert_eq!(row, s(&old));
}

#[test]
fn rename_to_an_unlisted_target_is_rejected() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    std::fs::rename(&old, f.root.join("Vase2.obj")).unwrap();
    let outside = f.base.join("elsewhere");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("Vase.obj"), OBJ_A).unwrap();
    let mut selection = f.selection(false);
    selection.renames[0].new_path = s(&outside.join("Vase.obj"));
    assert_eq!(apply(&mut f.conn, selection).err().unwrap().code, "diskSyncStale");
}

#[test]
fn target_in_a_sensitive_directory_fails_only_that_pairing() {
    let mut f = Fixture::new();
    let kept = f.add("ok/A.obj", OBJ_A);
    let blocked = f.add("blocked/B.obj", OBJ_B);
    std::fs::create_dir(f.root.join("secret")).unwrap();
    std::fs::rename(&kept, f.root.join("ok/A2.obj")).unwrap();
    std::fs::rename(&blocked, f.root.join("secret/B.obj")).unwrap();
    let mut selection = f.selection(false);
    selection.sensitive_dirs = vec![f.root.join("secret")];
    let applied = apply(&mut f.conn, selection).unwrap();
    assert_eq!(applied.renamed_count, 1);
    assert_eq!(applied.failed_paths.len(), 1);
    assert!(applied.failed_paths[0].path.ends_with("secret/B.obj"));
    let paths: Vec<String> = db::list_files(&f.conn).unwrap().into_iter().map(|file| file.path).collect();
    assert!(paths.contains(&s(&f.root.join("ok/A2.obj"))));
    assert!(paths.contains(&s(&blocked)), "the blocked row keeps its old path");
}

#[test]
fn a_taken_target_path_fails_that_pairing_and_leaves_the_row_untouched() {
    let mut f = Fixture::new();
    let old = f.add("Vase.obj", OBJ_A);
    let id = f.id_of(&old);
    let new = f.root.join("Vase2.obj");
    std::fs::rename(&old, &new).unwrap();
    let preview = f.scan();
    // A row that appeared after the scan and already owns the target path.
    db::test_insert_minimal_file(&f.conn, &s(&new), None).unwrap();
    let rename = preview.renamed_files[0].clone();
    let tx = f.conn.transaction().unwrap();
    let resolver = KeyResolver::new(&preview.roots, false);
    let key = |p: &Path| resolver.key(p);
    let mut known = HashSet::new();
    tx.execute_batch("SAVEPOINT t").unwrap();
    assert!(apply_rename(&tx, &preview, &rename, &mut known, &key, false).is_err());
    tx.execute_batch("ROLLBACK TO t; RELEASE t").unwrap();
    let path: String = tx.query_row("SELECT path FROM files WHERE id = ?1", [id], |r| r.get(0)).unwrap();
    assert_eq!(path, s(&old));
}

// ---- folders ----

fn folder_count(f: &Fixture) -> usize {
    db::list_folders(&f.conn).unwrap().len()
}

#[test]
fn renamed_folder_keeps_its_row_when_every_file_moved() {
    let mut f = Fixture::new();
    let a = f.add("Drucker-Teile/Halter.obj", OBJ_A);
    f.add("Drucker-Teile/Clip.obj", OBJ_B);
    f.add("Drucker-Teile/Unter/Tief.obj", OBJ_C);
    let folder = f.folder(&f.root.join("Drucker-Teile")).unwrap();
    let sub = f.folder(&f.root.join("Drucker-Teile/Unter")).unwrap();
    let id = f.id_of(&a);
    let folders_before = folder_count(&f);
    std::fs::rename(f.root.join("Drucker-Teile"), f.root.join("Druckerteile")).unwrap();
    let applied = f.apply_all();
    assert_eq!(applied.renamed_count, 3);
    assert!(applied.failed_paths.is_empty());
    assert_eq!(folder_count(&f), folders_before, "no second folder row beside an empty old one");
    let moved = f.folder(&f.root.join("Druckerteile")).unwrap();
    assert_eq!(moved.id, folder.id);
    assert_eq!(moved.name, "Druckerteile");
    assert_eq!(f.folder(&f.root.join("Druckerteile/Unter")).unwrap().id, sub.id);
    assert!(f.folder(&f.root.join("Drucker-Teile")).is_none());
    let folder_id: i64 = f.conn.query_row("SELECT folder_id FROM files WHERE id = ?1", [id], |r| r.get(0)).unwrap();
    assert_eq!(folder_id, folder.id);
    let again = f.scan();
    assert!(again.new_files.is_empty() && again.missing_files.is_empty() && again.new_folders.is_empty());
}

#[test]
fn folder_moved_to_another_parent_follows_with_its_row() {
    let mut f = Fixture::new();
    f.add("Deko/Teile/Halter.obj", OBJ_A);
    let folder = f.folder(&f.root.join("Deko/Teile")).unwrap();
    std::fs::rename(f.root.join("Deko/Teile"), f.root.join("Teile")).unwrap();
    f.apply_all();
    let moved = f.folder(&f.root.join("Teile")).unwrap();
    assert_eq!(moved.id, folder.id);
    assert_eq!(moved.parent_id, f.folder(&f.root).map(|root| root.id));
}

#[test]
fn partly_moved_folder_is_left_alone() {
    let mut f = Fixture::new();
    f.add("Alt/bleibt.obj", OBJ_A);
    let moved = f.add("Alt/wandert.obj", OBJ_B);
    let old_folder = f.folder(&f.root.join("Alt")).unwrap();
    std::fs::create_dir(f.root.join("Neu")).unwrap();
    std::fs::rename(&moved, f.root.join("Neu/wandert.obj")).unwrap();
    let applied = f.apply_all();
    assert_eq!(applied.renamed_count, 1);
    assert_eq!(f.folder(&f.root.join("Alt")).unwrap().id, old_folder.id);
    let new_folder = f.folder(&f.root.join("Neu")).unwrap();
    assert_ne!(new_folder.id, old_folder.id);
    let folder_id: i64 = f
        .conn
        .query_row("SELECT folder_id FROM files WHERE path = ?1", [s(&f.root.join("Neu/wandert.obj"))], |r| r.get(0))
        .unwrap();
    assert_eq!(folder_id, new_folder.id);
}

#[test]
fn folder_rows_stay_when_a_sibling_folder_still_exists_on_disk() {
    let mut f = Fixture::new();
    let moved = f.add("Alt/a.obj", OBJ_A);
    // Empty catalogued subfolder: it is not offered by the scan but has a row.
    let leer = f.root.join("Alt/Leer");
    std::fs::create_dir(&leer).unwrap();
    db::ensure_folder_path(&f.conn, &f.root, &leer).unwrap();
    let alt = f.folder(&f.root.join("Alt")).unwrap();
    let leer_row = f.folder(&leer).unwrap();
    std::fs::create_dir(f.root.join("Neu")).unwrap();
    std::fs::rename(&moved, f.root.join("Neu/a.obj")).unwrap();
    let applied = f.apply_all();
    assert_eq!(applied.renamed_count, 1);
    // Alt and Alt/Leer are untouched on disk, so their rows must not move.
    assert_eq!(f.folder(&f.root.join("Alt")).unwrap().id, alt.id);
    assert_eq!(f.folder(&leer).unwrap().id, leer_row.id);
    assert!(f.folder(&f.root.join("Neu/Leer")).is_none());
    assert_ne!(f.folder(&f.root.join("Neu")).unwrap().id, alt.id);
    assert!(db::list_files(&f.conn).unwrap()[0].path.ends_with("Neu/a.obj"));
}

#[test]
fn folder_with_a_renamed_file_inside_is_not_renamed_as_a_folder() {
    let mut f = Fixture::new();
    f.add("Alt/a.obj", OBJ_A);
    let folder = f.folder(&f.root.join("Alt")).unwrap();
    std::fs::rename(f.root.join("Alt"), f.root.join("Neu")).unwrap();
    std::fs::rename(f.root.join("Neu/a.obj"), f.root.join("Neu/b.obj")).unwrap();
    f.apply_all();
    // Folder and file name both changed: the pairing is applied per file and the old
    // folder row stays as it is instead of being guessed at.
    assert_eq!(f.folder(&f.root.join("Alt")).unwrap().id, folder.id);
    assert_eq!(db::list_files(&f.conn).unwrap()[0].path, s(&f.root.join("Neu/b.obj")));
}

#[test]
fn unticked_file_does_not_leave_an_empty_folder_row() {
    let mut f = Fixture::new();
    std::fs::create_dir(f.root.join("Neu")).unwrap();
    let file = f.root.join("Neu/a.obj");
    std::fs::write(&file, OBJ_A).unwrap();
    let preview = f.scan();
    // The folder is selected but none of the files below it.
    let selection = DiskSelection {
        new_paths: preview.new_folders.iter().map(|e| e.path.clone()).collect(),
        ..Default::default()
    };
    assert_eq!(preview.new_folders.len(), 1);
    let applied = apply(&mut f.conn, selection).unwrap();
    assert!(applied.file_ids.is_empty() && applied.folder_ids.is_empty());
    assert!(f.folder(&f.root.join("Neu")).is_none());
}
