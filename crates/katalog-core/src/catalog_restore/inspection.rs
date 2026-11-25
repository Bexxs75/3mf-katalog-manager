use super::*;

/// A validated catalog path outside the host's configured catalog roots.
/// Identifies a row without exposing an absolute path in preview text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PathWarning {
    pub table: String,
    pub column: String,
    pub row_id: i64,
    /// A shortened basename, never a path to join or use for filesystem actions.
    pub display_path: String,
}

/// Preview of an isolated database copy, before any restore sanitization.
/// Counts include trash entries. Source paths are original database values:
/// hosts must redact them as appropriate for display and must not treat them
/// as authorization to access those locations.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RestoreReport {
    /// Version in the supplied bytes, before migration of the temporary copy.
    pub schema_version: i64,
    pub migration_needed: bool,
    pub model_count: i64,
    pub folder_count: i64,
    pub collection_count: i64,
    pub source_folder_count: i64,
    pub path_warnings: Vec<PathWarning>,
    pub unreachable_source_folders: Vec<String>,
}

/// Inspect SQLite database bytes (use `read_catalog_archive` for ZIP input).
///
/// Uses the same validation/migration pipeline as `prepare_catalog`, without
/// sanitizing or activating the copy. The temporary owner is dropped on every
/// exit. `known_roots` and `policy` must come from host configuration, not the
/// backup. Invalid paths and paths forbidden by the trash/sensitivity policy
/// remain errors; being outside `known_roots` alone is only a warning.
///
/// `source_directory_reachable` is a read-only host probe: it must enforce the
/// host's source-root policy, return false on denied/inaccessible/non-directory
/// paths, and never create anything. It receives only lexically valid paths
/// without parent traversal. It runs before the sensitive-destination check so
/// that paths outside the host boundary are never resolved on the filesystem;
/// a path it accepts is then still rejected if it resolves to a sensitive
/// destination. Unknown network paths are reported unavailable without probing them. Reachability is a snapshot,
/// not a guarantee for a later import.
pub fn inspect_backup(
    bytes: &[u8],
    sensitive_dirs: &[PathBuf],
    policy: &TrashPolicy<'_>,
    known_roots: &[PathBuf],
    fs: &dyn RestoreFs,
    source_directory_reachable: &dyn Fn(&Path) -> bool,
) -> CmdResult<RestoreReport> {
    let mut temporary = fs.temporary_catalog(bytes)?;
    let conn = temporary.connection();
    // Reading this pragma cannot execute schema objects. All table queries and
    // migrations stay behind the existing foreign-object rejection.
    let schema_version = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| CmdError::coded(ErrorCode::BackupInvalidDatabase))?;
    let known: Vec<String> = known_roots
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect();
    // Everything the host itself configured as a place for catalog files. An
    // imported path outside of it is never resolved on the filesystem.
    let boundary = host_boundary(known_roots, policy);
    validate_connection(conn, sensitive_dirs, policy, &known, fs, Some(&boundary))?;
    let resolved_roots: Vec<PathBuf> = known_roots
        .iter()
        .map(|root| fs.resolve(root))
        .collect::<CmdResult<_>>()?;
    let mut report = RestoreReport {
        schema_version,
        migration_needed: schema_version < db::migrations_current_version(),
        model_count: 0,
        folder_count: 0,
        collection_count: 0,
        source_folder_count: 0,
        path_warnings: Vec::new(),
        unreachable_source_folders: Vec::new(),
    };
    let invalid = |_| CmdError::coded(ErrorCode::BackupInvalidDatabase);
    report.model_count = conn
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .map_err(invalid)?;
    report.folder_count = conn
        .query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))
        .map_err(invalid)?;
    report.collection_count = conn
        .query_row("SELECT COUNT(*) FROM collections", [], |r| r.get(0))
        .map_err(invalid)?;
    report.source_folder_count = conn
        .query_row("SELECT COUNT(*) FROM source_folders", [], |r| r.get(0))
        .map_err(invalid)?;
    // Constants only; include companion paths because those also drive file operations.
    for (table, column) in [
        ("files", "path"),
        ("folders", "path"),
        ("files", "trash_path"),
        ("companion_files", "path"),
        ("companion_files", "trash_path"),
    ] {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, {column} FROM {table} WHERE {column} IS NOT NULL ORDER BY id"
            ))
            .map_err(invalid)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(invalid)?;
        for row in rows {
            let (row_id, path) = row.map_err(invalid)?;
            if path.trim().is_empty() {
                continue;
            }
            let local = Path::new(&path);
            if local
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(CmdError::coded(ErrorCode::BackupInvalidDatabase));
            }
            // Outside the host boundary the path is only reported, never resolved.
            let outside = if import_path_in_boundary(local, &boundary) {
                let resolved = fs.resolve(local)?;
                !resolved_roots.iter().any(|root| resolved.starts_with(root))
            } else {
                true
            };
            if outside {
                let display_path = path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or("")
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(120)
                    .collect();
                report.path_warnings.push(PathWarning {
                    table: table.into(),
                    column: column.into(),
                    row_id,
                    display_path,
                });
            }
        }
    }
    let mut sensitive = sensitive_dirs.to_vec();
    for dir in sensitive_dirs {
        sensitive.push(fs.resolve(dir)?);
    }
    let mut stmt = conn
        .prepare("SELECT DISTINCT path FROM source_folders ORDER BY path")
        .map_err(invalid)?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?;
    for row in rows {
        let path = row.map_err(invalid)?;
        let local = Path::new(&path);
        let safe = local.is_absolute()
            && validate_imported_path_length(&path, &known).is_ok()
            && !path.chars().any(char::is_control)
            && !local
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            // The host boundary comes first: resolving an imported path touches the
            // filesystem, and a path outside the host's source roots (for example a
            // dead network mount) must never be probed, not even to classify it.
            && source_directory_reachable(local)
            && reject_sensitive(fs, local, &sensitive).is_ok();
        if !safe {
            report.unreachable_source_folders.push(path);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_reports_current_empty_catalog_without_changing_source() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.target/inspection-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("source.db");
        drop(db::connect(&path).unwrap());
        let bytes = std::fs::read(&path).unwrap();
        let report = inspect_backup(
            &bytes,
            &[],
            &TrashPolicy {
                desktop_trash: None,
                catalog_roots: &[],
            },
            &[],
            &StdRestoreFs::new(dir.clone()),
            &|_| panic!("no source folders"),
        )
        .unwrap();
        assert_eq!(report.schema_version, db::migrations_current_version());
        assert!(!report.migration_needed);
        assert_eq!((report.model_count, report.folder_count), (0, 0));
        assert!(report.path_warnings.is_empty());
        assert!(report.unreachable_source_folders.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn fixture(name: &str, setup: impl FnOnce(&Connection, &Path)) -> (Vec<u8>, PathBuf) {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.target/inspection-tests")
            .join(name);
        // A leftover from an aborted run would collide with the rows inserted below.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let path = dir.join("source.db");
        let conn = db::connect(&path).unwrap();
        setup(&conn, &dir);
        drop(conn);
        (std::fs::read(path).unwrap(), dir)
    }

    fn inspect(bytes: &[u8], dir: &Path) -> CmdResult<RestoreReport> {
        inspect_backup(
            bytes,
            &[],
            &TrashPolicy {
                desktop_trash: Some(&dir.join("trash")),
                catalog_roots: &[dir.join("models")],
            },
            &[dir.join("models")],
            &StdRestoreFs::new(dir.to_path_buf()),
            &|path| std::fs::read_dir(path).is_ok(),
        )
    }

    #[test]
    fn inspection_rejects_foreign_objects_types_fk_and_newer_schema() {
        for (name, sql) in [
            ("trigger", "CREATE TRIGGER injected AFTER INSERT ON files BEGIN DELETE FROM files; END; PRAGMA user_version = 40"),
            ("view", "CREATE VIEW injected AS SELECT * FROM files"),
            ("virtual", "CREATE VIRTUAL TABLE injected USING fts5(content)"),
            ("types", "UPDATE files SET file_size_bytes = 'wrong'"),
            ("fk", "PRAGMA foreign_keys = OFF; UPDATE files SET folder_id = 999"),
            ("newer", "PRAGMA user_version = 67"),
        ] {
            let (bytes, dir) = fixture(name, |conn, dir| {
                db::test_insert_minimal_file(conn, &dir.join("model.3mf").to_string_lossy(), None).unwrap();
                conn.execute_batch(sql).unwrap();
            });
            let error = inspect(&bytes, &dir).expect_err(name);
            assert_eq!(error, CmdError::coded(ErrorCode::BackupInvalidDatabase), "{name}");
            assert_eq!(std::fs::read(dir.join("source.db")).unwrap(), bytes);
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn inspection_reports_foreign_paths_and_unreachable_sources() {
        let (bytes, dir) = fixture("paths", |conn, dir| {
            std::fs::create_dir_all(dir.join("reachable")).unwrap();
            std::fs::write(dir.join("regular-file"), b"x").unwrap();
            db::insert_folder_with_parent(
                conn,
                "outside",
                None,
                &dir.join("outside").to_string_lossy(),
            )
            .unwrap();
            db::test_insert_minimal_file(conn, &dir.join("outside/x.3mf").to_string_lossy(), None)
                .unwrap();
            db::test_insert_minimal_file(conn, &dir.join("models/y.3mf").to_string_lossy(), None)
                .unwrap();
            db::test_insert_minimal_file(
                conn,
                &dir.join("models-other/z.3mf").to_string_lossy(),
                None,
            )
            .unwrap();
            for name in ["reachable", "missing", "regular-file"] {
                db::sources::insert_source_folder(conn, &dir.join(name).to_string_lossy(), "now")
                    .unwrap();
            }
        });
        let report = inspect(&bytes, &dir).unwrap();
        assert_eq!((report.model_count, report.folder_count), (3, 1));
        assert_eq!((report.collection_count, report.source_folder_count), (0, 3));
        assert_eq!(report.path_warnings.len(), 3);
        assert!(report
            .path_warnings
            .iter()
            .all(|w| !w.display_path.contains('/') && !w.display_path.contains('\\')));
        assert_eq!(
            report.unreachable_source_folders,
            vec![
                dir.join("missing").to_string_lossy().into_owned(),
                dir.join("regular-file").to_string_lossy().into_owned(),
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Records every path handed to `resolve` so a test can prove which locations
    /// were (not) looked at on the filesystem.
    struct RecordingFs {
        inner: StdRestoreFs,
        resolved: std::sync::Mutex<Vec<PathBuf>>,
    }
    impl RestoreFs for RecordingFs {
        fn resolve(&self, path: &Path) -> CmdResult<PathBuf> {
            self.resolved.lock().unwrap().push(path.to_path_buf());
            self.inner.resolve(path)
        }
        fn temporary_catalog(&self, bytes: &[u8]) -> CmdResult<Box<dyn TemporaryCatalog>> {
            self.inner.temporary_catalog(bytes)
        }
    }

    #[test]
    fn source_folder_outside_host_boundary_is_never_resolved() {
        let (bytes, dir) = fixture("boundary", |conn, _| {
            db::sources::insert_source_folder(conn, "/net/untrusted/models", "now").unwrap();
        });
        let fs = RecordingFs {
            inner: StdRestoreFs::new(dir.clone()),
            resolved: Default::default(),
        };
        let report = inspect_backup(
            &bytes,
            &[dir.join("config")],
            &TrashPolicy {
                desktop_trash: None,
                catalog_roots: &[],
            },
            &[],
            &fs,
            // Host boundary: nothing is inside the allowed source roots.
            &|_| false,
        )
        .unwrap();
        assert_eq!(report.unreachable_source_folders, vec!["/net/untrusted/models"]);
        assert!(
            fs.resolved
                .lock()
                .unwrap()
                .iter()
                .all(|p| !p.starts_with("/net")),
            "an imported path outside the boundary was resolved on the filesystem"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Runs the inspection with host roots `<dir>/models` and a protected `<dir>/config`.
    fn inspect_recorded(bytes: &[u8], dir: &Path) -> (CmdResult<RestoreReport>, Vec<PathBuf>) {
        let fs = RecordingFs {
            inner: StdRestoreFs::new(dir.to_path_buf()),
            resolved: Default::default(),
        };
        let result = inspect_backup(
            bytes,
            &[dir.join("config")],
            &TrashPolicy {
                desktop_trash: None,
                catalog_roots: &[dir.join("models")],
            },
            &[dir.join("models")],
            &fs,
            &|_| false,
        );
        let resolved = fs.resolved.lock().unwrap().clone();
        (result, resolved)
    }

    fn assert_never_resolved(resolved: &[PathBuf]) {
        assert!(
            resolved.iter().all(|p| !p.starts_with("/net")),
            "an imported path outside the boundary was resolved on the filesystem: {resolved:?}"
        );
    }

    fn add_extras_companion(conn: &Connection, file_id: i64, path: &str, trash: Option<&str>) {
        conn.execute(
            "INSERT INTO companion_files (file_id, name, path, kind, added_at, trash_path)
             VALUES (?1, 'a.png', ?2, 'image', 'now', ?3)",
            rusqlite::params![file_id, path, trash],
        )
        .unwrap();
    }

    #[test]
    fn file_path_outside_host_boundary_is_warned_and_never_resolved() {
        let (bytes, dir) = fixture("boundary-file", |conn, _| {
            db::test_insert_minimal_file(conn, "/net/untrusted/m.3mf", None).unwrap();
        });
        let (report, resolved) = inspect_recorded(&bytes, &dir);
        let report = report.unwrap();
        assert_eq!(report.path_warnings.len(), 1);
        assert_eq!(report.path_warnings[0].display_path, "m.3mf");
        assert_never_resolved(&resolved);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn folder_path_outside_host_boundary_is_warned_and_never_resolved() {
        let (bytes, dir) = fixture("boundary-folder", |conn, _| {
            conn.execute(
                "INSERT INTO folders (name, path) VALUES ('f', '/net/untrusted/f')",
                [],
            )
            .unwrap();
        });
        let (report, resolved) = inspect_recorded(&bytes, &dir);
        let report = report.unwrap();
        assert_eq!(report.path_warnings.len(), 1);
        assert_eq!(report.path_warnings[0].table, "folders");
        assert_never_resolved(&resolved);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn trash_path_outside_host_boundary_is_refused_and_never_resolved() {
        let (bytes, dir) = fixture("boundary-trash", |conn, dir| {
            let id = db::test_insert_minimal_file(
                conn,
                &dir.join("models/a.3mf").to_string_lossy(),
                None,
            )
            .unwrap();
            conn.execute(
                "UPDATE files SET trash_path = ?1 WHERE id = ?2",
                rusqlite::params![format!("/net/untrusted/{id}-aaaaaaaa-a.3mf"), id],
            )
            .unwrap();
        });
        let (report, resolved) = inspect_recorded(&bytes, &dir);
        assert_eq!(
            report.unwrap_err(),
            CmdError::coded(ErrorCode::BackupInvalidDatabase)
        );
        assert_never_resolved(&resolved);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn companion_paths_outside_host_boundary_are_never_resolved() {
        let (bytes, dir) = fixture("boundary-companion", |conn, dir| {
            let id = db::test_insert_minimal_file(
                conn,
                &dir.join("models/a.3mf").to_string_lossy(),
                None,
            )
            .unwrap();
            add_extras_companion(conn, id, "/net/untrusted/a_extras/a.png", None);
        });
        let (report, resolved) = inspect_recorded(&bytes, &dir);
        let report = report.unwrap();
        assert_eq!(report.path_warnings.len(), 1);
        assert_eq!(report.path_warnings[0].table, "companion_files");
        assert_never_resolved(&resolved);
        std::fs::remove_dir_all(dir).unwrap();

        let (bytes, dir) = fixture("boundary-companion-trash", |conn, dir| {
            let id = db::test_insert_minimal_file(
                conn,
                &dir.join("models/a.3mf").to_string_lossy(),
                None,
            )
            .unwrap();
            add_extras_companion(
                conn,
                id,
                &dir.join("models/a_extras/a.png").to_string_lossy(),
                Some("/net/untrusted/companion-1-aaaaaaaa-a.png"),
            );
        });
        let (report, resolved) = inspect_recorded(&bytes, &dir);
        assert!(report.is_err());
        assert_never_resolved(&resolved);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paths_inside_host_boundary_are_still_resolved_and_not_warned() {
        let (bytes, dir) = fixture("boundary-inside", |conn, dir| {
            std::fs::create_dir_all(dir.join("models")).unwrap();
            db::test_insert_minimal_file(
                conn,
                &dir.join("models/a.3mf").to_string_lossy(),
                None,
            )
            .unwrap();
        });
        let (report, resolved) = inspect_recorded(&bytes, &dir);
        assert!(report.unwrap().path_warnings.is_empty());
        assert!(resolved.iter().any(|p| p.ends_with("models/a.3mf")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn path_directly_below_protected_directory_outside_boundary_is_still_rejected() {
        let (bytes, dir) = fixture("boundary-sensitive", |conn, dir| {
            db::test_insert_minimal_file(
                conn,
                &dir.join("config/secret.3mf").to_string_lossy(),
                None,
            )
            .unwrap();
        });
        let (report, _) = inspect_recorded(&bytes, &dir);
        assert_eq!(
            report.unwrap_err(),
            CmdError::coded(ErrorCode::BackupInvalidDatabase)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn desktop_validation_without_boundary_still_resolves_every_path() {
        let (bytes, dir) = fixture("boundary-desktop", |conn, _| {
            db::test_insert_minimal_file(conn, "/net/untrusted/m.3mf", None).unwrap();
        });
        let fs = RecordingFs {
            inner: StdRestoreFs::new(dir.clone()),
            resolved: Default::default(),
        };
        validate_catalog_db_bytes(
            &bytes,
            &[dir.join("config")],
            &TrashPolicy {
                desktop_trash: None,
                catalog_roots: &[],
            },
            &[],
            &fs,
        )
        .unwrap();
        assert!(fs
            .resolved
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.starts_with("/net/untrusted")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn inspection_checks_trash_policy_for_web_exports_and_fallback_snapshots() {
        let (_, dir) = fixture("archives", |conn, dir| {
            for (index, trash) in [
                dir.join("trash/1-x.3mf"),
                dir.join("models/.katalog-papierkorb/2-deadbeef-y.3mf"),
            ]
            .iter()
            .enumerate()
            {
                let id = db::test_insert_minimal_file(
                    conn,
                    &dir.join(format!("models/{index}.3mf")).to_string_lossy(),
                    None,
                )
                .unwrap();
                conn.execute(
                    "UPDATE files SET deleted_at = 'now', trash_path = ?1 WHERE id = ?2",
                    rusqlite::params![trash.to_string_lossy(), id],
                )
                .unwrap();
            }
        });
        let conn = db::connect(&dir.join("source.db")).unwrap();
        let fallback =
            crate::backup::create(&conn, &dir.join("backups"), "0.18.7", "0.19.0").unwrap();
        let snapshot = dir.join("export.db");
        crate::catalog_export::snapshot(&conn, &snapshot).unwrap();
        drop(conn);
        let zip = dir.join("export.zip");
        crate::catalog_export::write_zip(
            &snapshot,
            &crate::catalog_export::settings_json(),
            std::fs::File::create(&zip).unwrap(),
        )
        .unwrap();
        let (web, _) =
            read_catalog_archive(std::io::Cursor::new(std::fs::read(zip).unwrap())).unwrap();
        for bytes in [web, std::fs::read(fallback).unwrap()] {
            let report = inspect(&bytes, &dir).unwrap();
            assert_eq!(report.model_count, 2);
            assert_eq!(report.path_warnings.len(), 1);
            assert_eq!(report.path_warnings[0].column, "trash_path");
            assert_eq!(report.path_warnings[0].display_path, "1-x.3mf");
            assert!(inspect_backup(
                &bytes,
                &[],
                &TrashPolicy {
                    desktop_trash: None,
                    catalog_roots: &[]
                },
                &[],
                &StdRestoreFs::new(dir.clone()),
                &|_| true
            )
            .is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn inspection_does_not_probe_unsafe_source_paths() {
        let (bytes, dir) = fixture("unsafe-sources", |conn, _| {
            for path in ["../outside", "//unknown/share", "/models/../private"] {
                db::sources::insert_source_folder(conn, path, "now").unwrap();
            }
            #[cfg(not(windows))]
            db::sources::insert_source_folder(conn, r"C:\models", "now").unwrap();
        });
        let report = inspect_backup(
            &bytes,
            &[],
            &TrashPolicy {
                desktop_trash: None,
                catalog_roots: &[],
            },
            &[],
            &StdRestoreFs::new(dir.clone()),
            &|_| panic!("unsafe source was probed"),
        )
        .unwrap();
        assert_eq!(
            report.unreachable_source_folders.len(),
            if cfg!(windows) { 3 } else { 4 }
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn inspection_warns_for_symlink_escape_and_rejects_parent_traversal() {
        let (bytes, dir) = fixture("symlink", |conn, dir| {
            std::fs::create_dir_all(dir.join("models")).unwrap();
            std::fs::create_dir_all(dir.join("outside")).unwrap();
            std::os::unix::fs::symlink(dir.join("outside"), dir.join("models/link")).unwrap();
            db::test_insert_minimal_file(
                conn,
                &dir.join("models/link/x.3mf").to_string_lossy(),
                None,
            )
            .unwrap();
        });
        let report = inspect(&bytes, &dir).unwrap();
        assert_eq!(report.path_warnings.len(), 1);
        assert_eq!(report.path_warnings[0].display_path, "x.3mf");
        let conn = db::connect(&dir.join("source.db")).unwrap();
        conn.execute(
            "UPDATE files SET path = ?1",
            [dir.join("models/../outside/x.3mf").to_string_lossy()],
        )
        .unwrap();
        drop(conn);
        assert!(inspect(&std::fs::read(dir.join("source.db")).unwrap(), &dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Activation (web) with the same host roots as `inspect_recorded`.
    fn prepare_recorded(bytes: &[u8], dir: &Path) -> (CmdResult<()>, Vec<PathBuf>) {
        let fs = RecordingFs {
            inner: StdRestoreFs::new(dir.to_path_buf()),
            resolved: Default::default(),
        };
        let local = db::connect_in_memory().unwrap();
        let local = db::thingiverse::local_settings(&local).unwrap();
        let result = prepare_catalog_within(
            bytes,
            &[dir.join("config")],
            &TrashPolicy {
                desktop_trash: None,
                catalog_roots: &[dir.join("models")],
            },
            &[dir.join("models")],
            &fs,
            &[],
            &local,
        )
        .map(drop);
        let resolved = fs.resolved.lock().unwrap().clone();
        (result, resolved)
    }

    /// Rows in every table whose paths validation resolves: files, folders,
    /// companions and remembered copies, all outside the host roots, plus a
    /// Windows path from a backup made on another computer.
    fn foreign_rows(conn: &Connection) {
        let id = db::test_insert_minimal_file(conn, "/net/untrusted/m.3mf", None).unwrap();
        conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
             VALUES ('w.3mf', ?1, '3mf', 0, 'now')",
            [r"C:\Modelle\w.3mf"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folders (name, path) VALUES ('f', '/net/untrusted/f')",
            [],
        )
        .unwrap();
        add_extras_companion(conn, id, "/net/untrusted/m_extras/a.png", None);
        conn.execute(
            "INSERT INTO disk_sync_known_copies (path, size_bytes, content_hash, noted_at)
             VALUES ('/net/untrusted/k.3mf', 1, ?1, 'now')",
            [&"a".repeat(64)],
        )
        .unwrap();
    }

    #[test]
    fn activation_never_resolves_paths_outside_host_boundary_and_matches_preview() {
        let (bytes, dir) = fixture("boundary-activation", |conn, _| foreign_rows(conn));
        let (report, inspected) = inspect_recorded(&bytes, &dir);
        assert!(report.is_ok());
        assert_never_resolved(&inspected);
        let (prepared, resolved) = prepare_recorded(&bytes, &dir);
        assert_eq!(
            prepared,
            Ok(()),
            "activation must accept what the preview accepted"
        );
        assert_never_resolved(&resolved);
        assert!(
            !resolved
                .iter()
                .any(|p| p.to_string_lossy().starts_with("C:")),
            "a Windows path is not below a Linux root and must not be resolved: {resolved:?}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn desktop_activation_without_boundary_is_unchanged() {
        let (bytes, dir) = fixture("boundary-activation-desktop", |conn, _| foreign_rows(conn));
        let fs = RecordingFs {
            inner: StdRestoreFs::new(dir.clone()),
            resolved: Default::default(),
        };
        let local = db::connect_in_memory().unwrap();
        let local = db::thingiverse::local_settings(&local).unwrap();
        prepare_catalog(
            &bytes,
            &[dir.join("config")],
            &TrashPolicy {
                desktop_trash: None,
                catalog_roots: &[],
            },
            &[],
            &fs,
            &[],
            &local,
        )
        .map(drop)
        .unwrap();
        let resolved = fs.resolved.lock().unwrap();
        for path in [
            "/net/untrusted/m.3mf",
            "/net/untrusted/f",
            "/net/untrusted/m_extras/a.png",
            "/net/untrusted/k.3mf",
        ] {
            assert!(resolved.iter().any(|p| p == Path::new(path)), "{path}");
        }
        drop(resolved);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn activation_inside_host_boundary_still_resolves_and_accepts() {
        let (bytes, dir) = fixture("boundary-activation-inside", |conn, dir| {
            std::fs::create_dir_all(dir.join("models")).unwrap();
            let id = db::test_insert_minimal_file(
                conn,
                &dir.join("models/a.3mf").to_string_lossy(),
                None,
            )
            .unwrap();
            add_extras_companion(
                conn,
                id,
                &dir.join("models/a_extras/a.png").to_string_lossy(),
                None,
            );
        });
        let (prepared, resolved) = prepare_recorded(&bytes, &dir);
        assert_eq!(prepared, Ok(()));
        assert!(resolved.iter().any(|p| p.ends_with("models/a.3mf")));
        assert!(resolved
            .iter()
            .any(|p| p.ends_with("models/a_extras/a.png")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn activation_and_preview_refuse_the_same_outside_paths() {
        // Below a protected directory (lexically) and with parent traversal: both
        // refuse, and the escaping path is never resolved on the way.
        for (name, path) in [
            ("boundary-activation-sensitive", "config/secret.3mf"),
            ("boundary-activation-parent", "models/../config/x.3mf"),
        ] {
            let (bytes, dir) = fixture(name, |conn, dir| {
                db::test_insert_minimal_file(conn, &dir.join(path).to_string_lossy(), None)
                    .unwrap();
            });
            let invalid = CmdError::coded(ErrorCode::BackupInvalidDatabase);
            assert_eq!(
                inspect_recorded(&bytes, &dir).0.unwrap_err(),
                invalid,
                "{name}"
            );
            let (prepared, resolved) = prepare_recorded(&bytes, &dir);
            assert_eq!(prepared.unwrap_err(), invalid, "{name}");
            assert!(
                !resolved.iter().any(|p| p.ends_with(path)),
                "{name}: {resolved:?}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn boundary_check_is_lexical_and_component_wise() {
        let roots = [PathBuf::from("/srv/models")];
        assert!(import_path_in_boundary(
            Path::new("/srv/models/a.3mf"),
            &roots
        ));
        assert!(import_path_in_boundary(
            Path::new("/srv/models/./a.3mf"),
            &roots
        ));
        for outside in [
            "/srv/models-other/a.3mf",
            "/srv/models/../etc/passwd",
            "srv/models/a.3mf",
            r"C:\srv\models\a.3mf",
            "/net/untrusted/a.3mf",
        ] {
            assert!(
                !import_path_in_boundary(Path::new(outside), &roots),
                "{outside}"
            );
        }
    }
}
