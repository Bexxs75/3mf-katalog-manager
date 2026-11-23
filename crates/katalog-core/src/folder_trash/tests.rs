use super::*;
use crate::import::file_names::SETTING_CATALOG_BASE_DIR;
use crate::unique_test_dir;

fn folder_trash_summary_with_conn(
    conn: &Connection,
    id: &str,
    sensitive: &[PathBuf],
) -> CmdResult<FolderTrashSummary> {
    super::folder_trash_summary_with_conn(&StdFolderTrashFs, conn, id, sensitive)
}

struct TestTrash<'a>(&'a Path);
impl ModelTrash for TestTrash<'_> {
    fn trash_model(&self, conn: &Connection, file: &db::models::FileRecord) -> CmdResult<()> {
        let path = Path::new(&file.path);
        if !path.exists() {
            db::soft_delete_file(conn, file.id, None, "now")?;
            return Ok(());
        }
        let target = self.0.join(format!("{}-{}", file.id, file.name));
        if target.exists() {
            return Err("target exists".to_string().into());
        }
        std::fs::rename(path, &target).map_err(|e| e.to_string())?;
        if let Err(e) = db::soft_delete_file(conn, file.id, Some(&target.to_string_lossy()), "now")
        {
            std::fs::rename(&target, path).unwrap();
            return Err(e.into());
        }
        Ok(())
    }
}
fn trash_folder_with_conn(
    conn: &Connection,
    id: &str,
    trash: &Path,
    sensitive: &[PathBuf],
) -> CmdResult<FolderTrashResult> {
    super::trash_folder_with_conn(&StdFolderTrashFs, &TestTrash(trash), conn, id, sensitive)
}

struct Fixture {
    conn: Connection,
    root: PathBuf,
    trash: PathBuf,
    a: i64,
    sensitive: Vec<PathBuf>,
}

/// catalog/ (base) -> A -> {B, C}; models: A/a.3mf, B/b.3mf, C/c.3mf, plus the base folder row.
fn fixture(name: &str) -> Fixture {
    let tmp = unique_test_dir(name);
    let root = tmp.join("catalog");
    let trash = tmp.join("trash");
    for dir in [
        root.join("A").join("B"),
        root.join("A").join("C"),
        trash.clone(),
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let conn = db::connect_in_memory().unwrap();
    db::printer_link::set_setting(&conn, SETTING_CATALOG_BASE_DIR, &root.to_string_lossy())
        .unwrap();
    let root_id =
        db::insert_folder_with_parent(&conn, "catalog", None, &root.to_string_lossy()).unwrap();
    let mk = |parent: i64, rel: &str| {
        let path = root.join(rel);
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        db::insert_folder_with_parent(&conn, &name, Some(parent), &path.to_string_lossy()).unwrap()
    };
    let a = mk(root_id, "A");
    let b = mk(a, "A/B");
    let c = mk(a, "A/C");
    for (folder, rel) in [(a, "A/a.3mf"), (b, "A/B/b.3mf"), (c, "A/C/c.3mf")] {
        let path = root.join(rel);
        std::fs::write(&path, rel.as_bytes()).unwrap();
        db::test_insert_minimal_file(&conn, &path.to_string_lossy(), Some(folder)).unwrap();
    }
    Fixture {
        conn,
        root,
        trash,
        a,
        sensitive: vec![tmp.join("sensitive")],
    }
}

fn folder_rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))
        .unwrap()
}

fn active_files(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM files WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn trashes_models_drops_rows_and_removes_empty_directories() {
    let f = fixture("folder_trash_full");
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert_eq!(result.trashed_model_count, 3);
    assert!(
        result.failed.is_empty() && result.kept_on_disk.is_empty(),
        "{result:?}"
    );
    assert_eq!(result.removed_folder_count, 3);
    assert_eq!(active_files(&f.conn), 0);
    assert_eq!(db::list_trash(&f.conn).unwrap().len(), 3);
    // Only the base folder row is left.
    assert_eq!(folder_rows(&f.conn), 1);
    assert!(
        !f.root.join("A").exists(),
        "empty directories must be removed"
    );
    assert!(f.root.exists(), "the catalog base folder must stay");
    assert_eq!(
        std::fs::read_dir(&f.trash).unwrap().count(),
        3,
        "models must sit in the app trash"
    );
}

#[test]
fn folder_with_a_non_model_file_stays_and_the_file_is_untouched() {
    let f = fixture("folder_trash_other_file");
    let note = f.root.join("A").join("B").join("notes.txt");
    std::fs::write(&note, b"keep me").unwrap();
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert_eq!(result.trashed_model_count, 3);
    assert_eq!(std::fs::read(&note).unwrap(), b"keep me");
    let kept: Vec<_> = result.kept_on_disk.iter().map(PathBuf::from).collect();
    assert_eq!(kept, vec![f.root.join("A"), f.root.join("A").join("B")]);
    assert!(
        !f.root.join("A").join("C").exists(),
        "the sibling that is really empty goes"
    );
    // Catalog rows are gone because no models are left, even though the directory stays.
    assert_eq!(folder_rows(&f.conn), 1);
}

#[test]
fn an_unknown_empty_subdirectory_is_kept_and_reported() {
    let f = fixture("folder_trash_unknown_dir");
    let unknown = f.root.join("A").join("B").join("not-in-catalog");
    std::fs::create_dir(&unknown).unwrap();
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert!(unknown.is_dir());
    assert!(result
        .kept_on_disk
        .iter()
        .any(|p| Path::new(p) == f.root.join("A").join("B")));
}

#[test]
fn an_empty_directory_outside_the_chosen_folder_is_never_removed() {
    let f = fixture("folder_trash_outside_dir");
    let outside = f.root.parent().unwrap().join("elsewhere");
    std::fs::create_dir(&outside).unwrap();
    db::insert_folder_with_parent(&f.conn, "elsewhere", Some(f.a), &outside.to_string_lossy())
        .unwrap();
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert!(outside.is_dir(), "a directory outside the folder stays");
    assert!(result.kept_on_disk.iter().any(|p| Path::new(p) == outside));
    assert!(
        !f.root.join("A").exists(),
        "counter-check: the folder's own empty directories still go"
    );
}

#[test]
fn catalog_root_and_its_ancestors_are_refused() {
    let f = fixture("folder_trash_root");
    let root_id: i64 = f
        .conn
        .query_row("SELECT id FROM folders WHERE parent_id IS NULL", [], |r| {
            r.get(0)
        })
        .unwrap();
    let err =
        trash_folder_with_conn(&f.conn, &root_id.to_string(), &f.trash, &f.sensitive).unwrap_err();
    assert!(
        matches!(
            err,
            CmdError::Coded {
                code: ROOT_ERROR,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(active_files(&f.conn), 3);
    assert!(f.root.join("A").join("a.3mf").exists());
    assert!(folder_summary(&f, root_id).is_catalog_root_or_ancestor);
    assert!(!folder_summary(&f, f.a).is_catalog_root_or_ancestor);

    // An ancestor folder of the base (base lives one level deeper).
    let parent = f.root.parent().unwrap();
    let parent_id =
        db::insert_folder_with_parent(&f.conn, "tmp", None, &parent.to_string_lossy()).unwrap();
    assert!(folder_summary(&f, parent_id).is_catalog_root_or_ancestor);
    assert!(
        trash_folder_with_conn(&f.conn, &parent_id.to_string(), &f.trash, &f.sensitive).is_err()
    );
    assert_eq!(active_files(&f.conn), 3);
}

fn folder_summary(f: &Fixture, id: i64) -> FolderTrashSummary {
    folder_trash_summary_with_conn(&f.conn, &id.to_string(), &f.sensitive).unwrap()
}

#[test]
fn sensitive_paths_are_refused_without_touching_anything() {
    let f = fixture("folder_trash_sensitive");
    let sensitive = vec![f.root.join("A").join("B")];
    let err = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &sensitive);
    assert!(
        err.is_err(),
        "a sensitive subfolder must stop the whole operation"
    );
    assert_eq!(active_files(&f.conn), 3);
    assert!(f.root.join("A").join("B").join("b.3mf").exists());
}

#[cfg(unix)]
#[test]
fn symlinks_are_neither_followed_nor_removed() {
    let f = fixture("folder_trash_symlink");
    let outside = unique_test_dir("folder_trash_symlink_outside");
    std::fs::write(outside.join("precious.txt"), b"outside").unwrap();
    let link = f.root.join("A").join("C").join("link");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(outside.join("precious.txt")).unwrap(),
        b"outside"
    );
    assert!(result
        .kept_on_disk
        .iter()
        .any(|p| Path::new(p) == f.root.join("A").join("C")));
    // The summary counts the link as one other file and does not enter it.
    let f2 = fixture("folder_trash_symlink_summary");
    std::os::unix::fs::symlink(&outside, f2.root.join("A").join("link")).unwrap();
    let summary = folder_summary(&f2, f2.a);
    assert_eq!(summary.other_file_count, 1);
    assert!(!summary.other_file_count_truncated);
}

#[test]
fn one_unmovable_file_does_not_stop_the_rest_and_keeps_its_folder() {
    let f = fixture("folder_trash_partial");
    // The trash target already exists, so exactly this file cannot be moved.
    let blocked_id: i64 = f
        .conn
        .query_row("SELECT id FROM files WHERE name = 'b.3mf'", [], |r| {
            r.get(0)
        })
        .unwrap();
    std::fs::write(f.trash.join(format!("{blocked_id}-b.3mf")), b"squatter").unwrap();
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert_eq!(result.trashed_model_count, 2);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].file_id, blocked_id.to_string());
    assert!(!result.failed[0].reason.is_empty());
    assert!(f.root.join("A").join("B").join("b.3mf").exists());
    assert_eq!(active_files(&f.conn), 1);
    // B and its ancestor A keep their rows and directories; C is gone.
    assert_eq!(folder_rows(&f.conn), 3);
    assert!(f.root.join("A").join("B").is_dir() && f.root.join("A").is_dir());
    assert!(!f.root.join("A").join("C").exists());
    assert!(result
        .kept_on_disk
        .iter()
        .any(|p| Path::new(p) == f.root.join("A").join("B")));
}

/// Adds catalog rows to folder A that a crafted backup could contain, and returns
/// `(row path, expected reason, content)`. Everything that must survive sits outside
/// A or is not a model.
fn plant_manipulated_rows(f: &Fixture) -> Vec<(PathBuf, &'static str, Vec<u8>)> {
    let outside = unique_test_dir("folder_trash_manip_outside");
    let mut planted = Vec::new();
    let mut add_row = |row: PathBuf, disk: PathBuf, reason: &'static str, content: &[u8]| {
        std::fs::write(&disk, content).unwrap();
        db::test_insert_minimal_file(&f.conn, &row.to_string_lossy(), Some(f.a)).unwrap();
        planted.push((disk, reason, content.to_vec()));
    };
    let p = outside.join("Rechnung.pdf");
    add_row(p.clone(), p, "outside_folder", b"invoice");
    let p = outside.join("other.3mf");
    add_row(p.clone(), p, "outside_folder", b"outside model");
    // `..` keeps the textual prefix of the folder but leaves it.
    add_row(
        f.root.join("A").join("..").join("escaped.3mf"),
        f.root.join("escaped.3mf"),
        "outside_folder",
        b"escaped",
    );
    // Inside the folder, but not a model.
    let p = f.root.join("A").join("notes.txt");
    add_row(p.clone(), p, "not_a_model", b"notes");
    // Sibling folder whose name starts like the folder name.
    std::fs::create_dir_all(f.root.join("A2")).unwrap();
    let p = f.root.join("A2").join("x.3mf");
    add_row(p.clone(), p, "outside_folder", b"sibling");
    #[cfg(unix)]
    {
        let target = outside.join("linked.3mf");
        std::fs::write(&target, b"link target").unwrap();
        let link = f.root.join("A").join("link.3mf");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        db::test_insert_minimal_file(&f.conn, &link.to_string_lossy(), Some(f.a)).unwrap();
        planted.push((link, "symlink", Vec::new()));
    }
    planted
}

#[test]
fn manipulated_rows_never_move_files_outside_the_folder_or_non_models() {
    let f = fixture("folder_trash_manipulated");
    let planted = plant_manipulated_rows(&f);
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert_eq!(
        result.trashed_model_count, 3,
        "only the three real models move: {result:?}"
    );
    for (path, reason, content) in &planted {
        assert!(
            std::fs::symlink_metadata(path).is_ok(),
            "{} must stay",
            path.display()
        );
        if *reason != "symlink" {
            assert_eq!(
                &std::fs::read(path).unwrap(),
                content,
                "{} must be unchanged",
                path.display()
            );
        } else {
            assert_eq!(std::fs::read(path).unwrap(), b"link target");
        }
        let failure = result.failed.iter().find(|x| {
            Path::new(&x.path) == path.as_path()
                || (path.ends_with("escaped.3mf") && x.path.ends_with("escaped.3mf"))
        });
        assert_eq!(
            failure.map(|x| x.reason.as_str()),
            Some(*reason),
            "{}",
            path.display()
        );
    }
    assert_eq!(
        std::fs::read_dir(&f.trash).unwrap().count(),
        3,
        "nothing but the models in the trash"
    );
    // The folder that still holds refused rows keeps its row and directory.
    assert!(f.root.join("A").is_dir());
    let rows: i64 = f
        .conn
        .query_row("SELECT COUNT(*) FROM folders WHERE id = ?1", [f.a], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rows, 1);
}

#[test]
fn summary_counts_only_files_the_delete_would_really_move() {
    let f = fixture("folder_trash_manipulated_summary");
    plant_manipulated_rows(&f);
    let summary = folder_summary(&f, f.a);
    assert_eq!(summary.model_count, 3);
}

#[test]
fn summary_counts_subfolders_models_and_other_files() {
    let f = fixture("folder_trash_summary");
    std::fs::write(f.root.join("A").join("readme.txt"), b"x").unwrap();
    std::fs::create_dir(f.root.join("A").join("B").join("unknown")).unwrap();
    std::fs::write(
        f.root.join("A").join("B").join("unknown").join("y.bin"),
        b"y",
    )
    .unwrap();
    // A trashed model is not a model any more.
    let extra = f.root.join("A").join("gone.3mf");
    std::fs::write(&extra, b"z").unwrap();
    let gone = db::test_insert_minimal_file(&f.conn, &extra.to_string_lossy(), Some(f.a)).unwrap();
    db::soft_delete_file(&f.conn, gone, Some("/t"), "now").unwrap();
    let summary = folder_summary(&f, f.a);
    assert_eq!(summary.name, "A");
    assert_eq!(summary.subfolder_count, 2);
    assert_eq!(summary.model_count, 3);
    // readme.txt, y.bin and gone.3mf (soft-deleted rows do not count as known).
    assert_eq!(summary.other_file_count, 3);
    assert!(!summary.other_file_count_truncated);
    assert!(!summary.is_catalog_root_or_ancestor);
}

#[test]
fn summary_scan_is_capped() {
    let tmp = unique_test_dir("folder_trash_cap");
    for i in 0..OTHER_FILES_SCAN_LIMIT + 5 {
        std::fs::write(tmp.join(format!("f{i}")), b"").unwrap();
    }
    let (count, truncated) = count_other_files(&StdFolderTrashFs, &tmp, &HashSet::new());
    assert_eq!(count, OTHER_FILES_SCAN_LIMIT as i64);
    assert!(truncated);
    let _ = std::fs::remove_dir_all(tmp);
}

#[test]
fn successful_models_record_origin_but_failed_models_do_not() {
    let f = fixture("folder_trash_origins");
    let blocked: i64 = f
        .conn
        .query_row("SELECT id FROM files WHERE name = 'b.3mf'", [], |r| {
            r.get(0)
        })
        .unwrap();
    std::fs::write(f.trash.join(format!("{blocked}-b.3mf")), b"occupied").unwrap();
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert_eq!(result.trashed_model_count, 2);
    let origins: i64 = f
        .conn
        .query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get(0))
        .unwrap();
    assert_eq!(origins, 2);
    let failed_origins: i64 = f
        .conn
        .query_row(
            "SELECT COUNT(*) FROM folder_trash_origin WHERE file_id = ?1",
            [blocked],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(failed_origins, 0);
}

#[test]
fn missing_models_are_admitted_and_record_origin() {
    let f = fixture("folder_trash_missing");
    let path = f.root.join("A/a.3mf");
    std::fs::remove_file(&path).unwrap();
    let plan = plan_model_trash(
        &StdFolderTrashFs,
        1,
        path.to_string_lossy().into_owned(),
        &f.root.join("A"),
        &[],
    );
    assert_eq!(plan.refusal_reason, None);
    assert_eq!(folder_summary(&f, f.a).model_count, 3);
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert_eq!(result.trashed_model_count, 3);
    let origins: i64 = f
        .conn
        .query_row("SELECT COUNT(*) FROM folder_trash_origin", [], |r| r.get(0))
        .unwrap();
    assert_eq!(origins, 3);
}

#[test]
fn origin_insert_failure_is_best_effort_like_desktop() {
    let f = fixture("folder_trash_origin_failure");
    f.conn.execute_batch("CREATE TRIGGER refuse_origin BEFORE INSERT ON folder_trash_origin BEGIN SELECT RAISE(FAIL, 'origin blocked'); END;").unwrap();
    let result = trash_folder_with_conn(&f.conn, &f.a.to_string(), &f.trash, &f.sensitive).unwrap();
    assert_eq!(result.trashed_model_count, 3);
    assert!(result.failed.is_empty());
    assert_eq!(active_files(&f.conn), 0);
}

#[test]
fn invalid_and_unknown_folder_ids_have_desktop_codes() {
    let f = fixture("folder_trash_ids");
    for id in ["0", "-1", "abc", "9223372036854775808"] {
        let err = folder_trash_summary_with_conn(&f.conn, id, &[]).unwrap_err();
        assert!(matches!(
            err,
            CmdError::Coded {
                code: ErrorCode::InvalidCatalogId,
                ..
            }
        ));
        let err = trash_folder_with_conn(&f.conn, id, &f.trash, &[]).unwrap_err();
        assert!(matches!(
            err,
            CmdError::Coded {
                code: ErrorCode::InvalidCatalogId,
                ..
            }
        ));
    }
    let err = trash_folder_with_conn(&f.conn, "99999", &f.trash, &[]).unwrap_err();
    assert!(matches!(
        err,
        CmdError::Coded {
            code: ErrorCode::FolderNotFound,
            ..
        }
    ));
    assert_eq!(active_files(&f.conn), 3);
}

#[test]
fn model_plans_use_host_reads_and_preserve_refusal_order() {
    struct VirtualFs;
    impl FolderTrashFs for VirtualFs {
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            Ok(path.to_path_buf())
        }
        fn kind(&self, path: &Path, follow_links: bool) -> io::Result<SourceKind> {
            if follow_links {
                return Ok(SourceKind::Directory);
            }
            match path.file_name().unwrap().to_str().unwrap() {
                "missing.txt" => Err(io::ErrorKind::NotFound.into()),
                "denied.3mf" => Err(io::ErrorKind::PermissionDenied.into()),
                "link.3mf" | "link.txt" => Ok(SourceKind::Symlink),
                _ => Ok(SourceKind::File),
            }
        }
        fn list(&self, _: &Path) -> io::Result<SourceEntries<'_>> {
            panic!("planning must not scan")
        }
        fn remove_empty_dir(&self, _: &Path) -> io::Result<()> {
            panic!("planning must not mutate")
        }
    }
    let root = Path::new("/virtual/A");
    for (path, reason) in [
        ("/virtual/A/model.3mf", None),
        ("/virtual/A/missing.txt", None),
        ("/virtual/A/denied.3mf", None),
        ("/virtual/A/link.3mf", Some("symlink")),
        ("/virtual/A/link.txt", Some("not_a_model")),
        ("/virtual/A2/model.3mf", Some("outside_folder")),
        ("/virtual/A/../missing.txt", Some("outside_folder")),
    ] {
        let plan = plan_model_trash(&VirtualFs, 42, path.to_string(), root, &[]);
        assert_eq!(plan.file_id, 42);
        assert_eq!(plan.path, path);
        assert_eq!(plan.refusal_reason, reason, "{path}");
    }
    let plan = plan_model_trash(
        &VirtualFs,
        42,
        "/virtual/A/model.3mf".into(),
        root,
        &[root.to_path_buf()],
    );
    assert_eq!(plan.refusal_reason, Some("outside_folder"));
}
