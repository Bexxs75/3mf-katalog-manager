use super::*;
use crate::unique_test_dir;

fn preview_with_conn(
    conn: &Connection,
    ids: &[String],
    source: DateSource,
    sensitive: &[PathBuf],
) -> CmdResult<Vec<DateRenamePreview>> {
    preview(conn, ids, source, sensitive, &StdDateRenameFs)
}

fn apply_with_conn(
    conn: &Connection,
    items: Vec<DateRenameItem>,
    sensitive: &[PathBuf],
) -> CmdResult<Vec<DateRenameResult>> {
    apply(conn, items, sensitive, &StdDateRenameFs, &TestRename)
}

struct TestRename;
impl DateRenameEffect for TestRename {
    fn rename(
        &self,
        _file: &FileRecord,
        old: &Path,
        new: &Path,
        commit: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        import_rename::race_point("before-file-rename", new);
        crate::import::placement::move_file(old, new).map_err(|e| e.to_string())?;
        if let Err(error) = commit() {
            crate::import::placement::move_file(new, old).map_err(|e| e.to_string())?;
            return Err(error);
        }
        Ok(())
    }
}
use std::time::{Duration, SystemTime};

fn setup(name: &str) -> (Connection, PathBuf) {
    let dir = unique_test_dir(name).canonicalize().unwrap();
    let conn = crate::db::connect_in_memory().expect("connect");
    db::printer_link::set_setting(
        &conn,
        folders::SETTING_CATALOG_BASE_DIR,
        &dir.to_string_lossy(),
    )
    .unwrap();
    (conn, dir)
}

/// Fixed modification time so expected names do not depend on the clock.
fn modified() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_760_000_000)
}

fn modified_prefix() -> String {
    import_rename::date_prefix(DateTime::<Local>::from(modified()))
}

fn add(conn: &Connection, dir: &Path, name: &str) -> String {
    let path = dir.join(name);
    let file = std::fs::File::create(&path).unwrap();
    file.set_modified(modified()).unwrap();
    db::test_insert_minimal_file(conn, &path.to_string_lossy(), None)
        .unwrap()
        .to_string()
}

fn status_of(preview: &[DateRenamePreview], id: &str) -> PreviewStatus {
    preview.iter().find(|p| p.id == id).unwrap().status
}

fn run_preview(conn: &Connection, ids: &[&String]) -> Vec<DateRenamePreview> {
    let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    preview_with_conn(conn, &ids, DateSource::Modified, &[]).unwrap()
}

#[test]
fn preview_reports_every_status() {
    let (conn, dir) = setup("date_rename_status");
    let prefix = modified_prefix();
    let plain = add(&conn, &dir, "plain.3mf");
    let prefixed = add(&conn, &dir, "2020-01-02_03-04_old.3mf");
    let taken = add(&conn, &dir, "taken.3mf");
    // Untracked file already using the target name.
    std::fs::write(dir.join(format!("{prefix}taken.3mf")), b"x").unwrap();
    let upper = add(&conn, &dir, "Pair.3mf");
    let lower = add(&conn, &dir, "pair.3mf");
    let missing = add(&conn, &dir, "gone.3mf");
    std::fs::remove_file(dir.join("gone.3mf")).unwrap();
    let trashed = add(&conn, &dir, "trashed.3mf");
    conn.execute(
        "UPDATE files SET deleted_at = '2026-01-01T00:00:00Z' WHERE id = ?1",
        [&trashed],
    )
    .unwrap();
    let long = add(&conn, &dir, &format!("{}.3mf", "a".repeat(246)));
    let outside = unique_test_dir("date_rename_outside")
        .canonicalize()
        .unwrap();
    let external = add(&conn, &outside, "external.3mf");
    // A catalog entry owning the target path although its file is gone.
    let _ghost = add(&conn, &dir, &format!("{prefix}ghost.3mf"));
    std::fs::remove_file(dir.join(format!("{prefix}ghost.3mf"))).unwrap();
    let ghost_owner = add(&conn, &dir, "ghost.3mf");

    let ids = [
        &plain,
        &prefixed,
        &taken,
        &upper,
        &lower,
        &missing,
        &trashed,
        &long,
        &external,
        &ghost_owner,
    ];
    let preview = run_preview(&conn, &ids);
    assert_eq!(status_of(&preview, &plain), PreviewStatus::Rename);
    assert_eq!(
        preview.iter().find(|p| p.id == plain).unwrap().new_name,
        format!("{prefix}plain.3mf")
    );
    assert_eq!(status_of(&preview, &prefixed), PreviewStatus::SkipHasPrefix);
    assert_eq!(status_of(&preview, &taken), PreviewStatus::SkipConflict);
    assert_eq!(status_of(&preview, &upper), PreviewStatus::Rename);
    assert_eq!(status_of(&preview, &lower), PreviewStatus::SkipConflict);
    assert_eq!(status_of(&preview, &missing), PreviewStatus::SkipMissing);
    assert_eq!(status_of(&preview, &trashed), PreviewStatus::SkipTrash);
    assert_eq!(status_of(&preview, &long), PreviewStatus::SkipTooLong);
    assert_eq!(status_of(&preview, &external), PreviewStatus::SkipExternal);
    assert_eq!(
        status_of(&preview, &ghost_owner),
        PreviewStatus::SkipConflict
    );
    let unknown = run_preview(&conn, &[&"99999".to_string()]);
    assert_eq!(unknown[0].status, PreviewStatus::SkipMissing);
    // Planning writes nothing.
    assert!(dir.join("plain.3mf").exists());
    assert_eq!(
        db::get_file(&conn, plain.parse().unwrap())
            .unwrap()
            .unwrap()
            .name,
        "plain.3mf"
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn preview_uses_the_selected_date_source() {
    let (conn, dir) = setup("date_rename_source");
    let id = add(&conn, &dir, "model.3mf");
    conn.execute(
        "UPDATE files SET imported_at = '2025-03-04T05:06:07Z' WHERE id = ?1",
        [&id],
    )
    .unwrap();
    let ids = [id.clone()];
    let modified_name = &preview_with_conn(&conn, &ids, DateSource::Modified, &[]).unwrap()[0];
    let imported_name = &preview_with_conn(&conn, &ids, DateSource::Imported, &[]).unwrap()[0];
    assert_eq!(
        modified_name.new_name,
        format!("{}model.3mf", modified_prefix())
    );
    let imported = DateTime::parse_from_rfc3339("2025-03-04T05:06:07Z")
        .unwrap()
        .with_timezone(&Local);
    assert_eq!(
        imported_name.new_name,
        format!("{}model.3mf", import_rename::date_prefix(imported))
    );
    assert_ne!(modified_name.new_name, imported_name.new_name);
    conn.execute(
        "UPDATE files SET imported_at = 'garbage' WHERE id = ?1",
        [&id],
    )
    .unwrap();
    let broken = &preview_with_conn(&conn, &ids, DateSource::Imported, &[]).unwrap()[0];
    assert_eq!(broken.status, PreviewStatus::SkipNoDate);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_renames_disk_and_catalog() {
    let (conn, dir) = setup("date_rename_apply");
    let prefix = modified_prefix();
    let id = add(&conn, &dir, "model.3mf");
    let new_name = format!("{prefix}model.3mf");
    let results = apply_with_conn(
        &conn,
        vec![DateRenameItem {
            id: id.clone(),
            new_name: new_name.clone(),
        }],
        &[],
    )
    .unwrap();
    assert_eq!(
        results[0].status,
        ApplyStatus::Renamed,
        "{:?}",
        results[0].reason
    );
    assert!(dir.join(&new_name).exists());
    assert!(!dir.join("model.3mf").exists());
    let row = db::get_file(&conn, id.parse().unwrap()).unwrap().unwrap();
    assert_eq!(row.name, new_name);
    assert_eq!(row.path, dir.join(&new_name).to_string_lossy());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn target_created_after_the_check_is_never_overwritten() {
    let (conn, dir) = setup("date_rename_race");
    let id = add(&conn, &dir, "model.3mf");
    let new_name = format!("{}model.3mf", modified_prefix());
    let target = dir.join(&new_name);
    let racer = target.clone();
    import_rename::set_race_hook(Some(Box::new(move |stage, _| {
        if stage == "before-file-rename" && !racer.exists() {
            std::fs::write(&racer, b"created meanwhile").unwrap();
        }
    })));
    let results = apply_with_conn(
        &conn,
        vec![DateRenameItem {
            id: id.clone(),
            new_name,
        }],
        &[],
    )
    .unwrap();
    import_rename::set_race_hook(None);
    assert_eq!(results[0].status, ApplyStatus::Failed);
    assert_eq!(results[0].reason.as_deref(), Some("targetTaken"));
    assert_eq!(std::fs::read(&target).unwrap(), b"created meanwhile");
    assert!(
        dir.join("model.3mf").exists(),
        "the original stays where it was"
    );
    let row = db::get_file(&conn, id.parse().unwrap()).unwrap().unwrap();
    assert_eq!(row.name, "model.3mf");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_rejects_names_the_preview_would_not_produce() {
    let (conn, dir) = setup("date_rename_rule");
    let id = add(&conn, &dir, "model.3mf");
    let bad = [
        "free.3mf".to_string(),
        "2026-10-12_14-41_other.3mf".to_string(),
        "2026-13-45_99-99_model.3mf".to_string(),
        "2026-10-12_14-41_model.3mf.bak".to_string(),
        "../2026-10-12_14-41_model.3mf".to_string(),
        "é".to_string(),
    ];
    for new_name in bad {
        let results = apply_with_conn(
            &conn,
            vec![DateRenameItem {
                id: id.clone(),
                new_name: new_name.clone(),
            }],
            &[],
        )
        .unwrap();
        assert_eq!(results[0].status, ApplyStatus::Failed, "{new_name}");
        assert_eq!(results[0].reason.as_deref(), Some("nameRule"), "{new_name}");
    }
    assert!(dir.join("model.3mf").exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_rechecks_target_and_state_and_continues_after_failures() {
    let (conn, dir) = setup("date_rename_partial");
    let prefix = modified_prefix();
    let good = add(&conn, &dir, "good.3mf");
    let taken = add(&conn, &dir, "taken.3mf");
    // Appears between preview and apply.
    std::fs::write(dir.join(format!("{prefix}taken.3mf")), b"x").unwrap();
    let gone = add(&conn, &dir, "gone.3mf");
    std::fs::remove_file(dir.join("gone.3mf")).unwrap();
    let outside = unique_test_dir("date_rename_apply_outside")
        .canonicalize()
        .unwrap();
    let external = add(&conn, &outside, "external.3mf");
    let item = |id: &String, name: &str| DateRenameItem {
        id: id.clone(),
        new_name: format!("{prefix}{name}"),
    };
    let results = apply_with_conn(
        &conn,
        vec![
            item(&taken, "taken.3mf"),
            item(&gone, "gone.3mf"),
            item(&external, "external.3mf"),
            item(&good, "good.3mf"),
        ],
        &[],
    )
    .unwrap();
    let reasons: Vec<_> = results.iter().map(|r| r.reason.as_deref()).collect();
    assert_eq!(
        reasons,
        [Some("targetTaken"), Some("missing"), Some("external"), None]
    );
    assert_eq!(results[3].status, ApplyStatus::Renamed);
    assert_eq!(
        std::fs::read(dir.join(format!("{prefix}taken.3mf"))).unwrap(),
        b"x",
        "an existing file must never be overwritten"
    );
    assert!(dir.join("taken.3mf").exists());
    assert!(outside.join("external.3mf").exists());
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn apply_rolls_back_the_disk_when_the_catalog_update_fails() {
    let (conn, dir) = setup("date_rename_rollback");
    let prefix = modified_prefix();
    let id = add(&conn, &dir, "model.3mf");
    conn.execute_batch(&format!(
        "CREATE TRIGGER block_rename BEFORE UPDATE ON files
             WHEN NEW.id = {id} BEGIN SELECT RAISE(ABORT, 'simulated'); END;"
    ))
    .unwrap();
    let results = apply_with_conn(
        &conn,
        vec![DateRenameItem {
            id,
            new_name: format!("{prefix}model.3mf"),
        }],
        &[],
    )
    .unwrap();
    assert_eq!(results[0].status, ApplyStatus::Failed);
    assert!(dir.join("model.3mf").exists());
    assert!(!dir.join(format!("{prefix}model.3mf")).exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_refuses_protected_folders_and_oversized_requests() {
    let (conn, dir) = setup("date_rename_protected");
    let prefix = modified_prefix();
    let id = add(&conn, &dir, "model.3mf");
    let results = apply_with_conn(
        &conn,
        vec![DateRenameItem {
            id: id.clone(),
            new_name: format!("{prefix}model.3mf"),
        }],
        std::slice::from_ref(&dir),
    )
    .unwrap();
    assert_eq!(results[0].status, ApplyStatus::Failed);
    assert!(dir.join("model.3mf").exists());
    let many: Vec<DateRenameItem> = (0..=MAX_ITEMS)
        .map(|n| DateRenameItem {
            id: n.to_string(),
            new_name: String::new(),
        })
        .collect();
    assert!(apply_with_conn(&conn, many, &[]).is_err());
    let ids: Vec<String> = (0..=MAX_ITEMS).map(|n| n.to_string()).collect();
    assert!(preview_with_conn(&conn, &ids, DateSource::Modified, &[]).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn running_it_twice_skips_everything_the_second_time() {
    let (conn, dir) = setup("date_rename_idempotent");
    let id = add(&conn, &dir, "model.3mf");
    let first = run_preview(&conn, &[&id]);
    assert_eq!(first[0].status, PreviewStatus::Rename);
    let results = apply_with_conn(
        &conn,
        vec![DateRenameItem {
            id: id.clone(),
            new_name: first[0].new_name.clone(),
        }],
        &[],
    )
    .unwrap();
    assert_eq!(results[0].status, ApplyStatus::Renamed);
    let second = run_preview(&conn, &[&id]);
    assert_eq!(second[0].status, PreviewStatus::SkipHasPrefix);
    assert_eq!(second[0].old_name, second[0].new_name);
    // Replaying the old request must not stack a second prefix.
    let replay = apply_with_conn(
        &conn,
        vec![DateRenameItem {
            id,
            new_name: first[0].new_name.clone(),
        }],
        &[],
    )
    .unwrap();
    assert_eq!(replay[0].reason.as_deref(), Some("hasPrefix"));
    let _ = std::fs::remove_dir_all(dir);
}

// These paths never exist on disk: all reads must go through the host adapter.
struct MemoryFs {
    paths: HashSet<PathBuf>,
    no_date: bool,
    denied: Option<PathBuf>,
}
impl DateRenameFs for MemoryFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.entry(path)?;
        Ok(path.to_path_buf())
    }
    fn entry(&self, path: &Path) -> io::Result<()> {
        if self.denied.as_deref() == Some(path) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        }
        if self.paths.contains(path) {
            Ok(())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }
    fn modified(&self, path: &Path) -> io::Result<SystemTime> {
        self.entry(path)?;
        if self.no_date {
            Err(io::ErrorKind::Unsupported.into())
        } else {
            Ok(modified())
        }
    }
}

#[test]
fn virtual_filesystem_preserves_unicode_claims_and_read_failures() {
    let conn = db::connect_in_memory().unwrap();
    let base = PathBuf::from("virtual-catalog");
    db::printer_link::set_setting(
        &conn,
        folders::SETTING_CATALOG_BASE_DIR,
        base.to_str().unwrap(),
    )
    .unwrap();
    let paths = [
        base.join("Übersicht.3mf"),
        base.join("u\u{308}bersicht.3mf"),
        base.join("bad:name.3mf"),
    ];
    let ids: Vec<_> = paths
        .iter()
        .map(|p| {
            db::test_insert_minimal_file(&conn, p.to_str().unwrap(), None)
                .unwrap()
                .to_string()
        })
        .collect();
    let mut fs = MemoryFs {
        paths: paths.iter().cloned().chain([base.clone()]).collect(),
        no_date: false,
        denied: None,
    };
    let results = preview(&conn, &ids, DateSource::Modified, &[], &fs).unwrap();
    assert_eq!(
        results.iter().map(|p| p.status).collect::<Vec<_>>(),
        [
            PreviewStatus::Rename,
            PreviewStatus::SkipConflict,
            PreviewStatus::SkipInvalid
        ]
    );
    let protected = preview(
        &conn,
        &ids[..1],
        DateSource::Modified,
        std::slice::from_ref(&base),
        &fs,
    )
    .unwrap();
    assert_eq!(protected[0].status, PreviewStatus::SkipInvalid);
    fs.no_date = true;
    assert_eq!(
        preview(&conn, &ids[..1], DateSource::Modified, &[], &fs).unwrap()[0].status,
        PreviewStatus::SkipNoDate
    );
    fs.no_date = false;
    fs.denied = Some(base.join(&results[0].new_name));
    assert_eq!(
        preview(&conn, &ids[..1], DateSource::Modified, &[], &fs)
            .expect_err("target metadata failure")
            .message,
        "denied"
    );
    fs.denied = Some(paths[0].clone());
    assert_eq!(
        preview(&conn, &ids[..1], DateSource::Modified, &[], &fs).unwrap()[0].status,
        PreviewStatus::SkipMissing
    );
}

#[test]
fn apply_rechecks_catalog_ownership_trash_and_current_disk_name() {
    let (conn, dir) = setup("date_rename_stale_preview");
    let owned = add(&conn, &dir, "owned.3mf");
    let trash = add(&conn, &dir, "trash.3mf");
    let changed = add(&conn, &dir, "changed.3mf");
    let planned = run_preview(&conn, &[&owned, &trash, &changed]);
    db::test_insert_minimal_file(
        &conn,
        &dir.join(&planned[0].new_name).to_string_lossy(),
        None,
    )
    .unwrap();
    conn.execute(
        "UPDATE files SET deleted_at = '2026-01-01T00:00:00Z' WHERE id = ?1",
        [&trash],
    )
    .unwrap();
    let next = dir.join("different.3mf");
    std::fs::rename(dir.join("changed.3mf"), &next).unwrap();
    db::rename_file(
        &conn,
        changed.parse().unwrap(),
        "different.3mf",
        &next.to_string_lossy(),
    )
    .unwrap();
    let mut items: Vec<_> = planned
        .into_iter()
        .map(|p| DateRenameItem {
            id: p.id,
            new_name: p.new_name,
        })
        .collect();
    items.push(DateRenameItem {
        id: "invalid".into(),
        new_name: "ignored".into(),
    });
    let results = apply_with_conn(&conn, items, &[]).unwrap();
    assert_eq!(
        results
            .iter()
            .map(|r| r.reason.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("targetTaken"),
            Some("trash"),
            Some("nameRule"),
            Some("missing")
        ]
    );
    assert!(dir.join("owned.3mf").exists());
    assert!(dir.join("trash.3mf").exists());
    assert!(next.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_reads_match_desktop_eligibility_and_target_conflicts() {
    let (conn, dir) = setup("date_rename_links");
    let outside = unique_test_dir("date_rename_link_outside")
        .canonicalize()
        .unwrap();
    std::fs::write(outside.join("model.3mf"), b"external").unwrap();
    let id = add(&conn, &dir, "model.3mf");
    let target = dir.join(format!("{}model.3mf", modified_prefix()));
    std::os::unix::fs::symlink(dir.join("absent"), &target).unwrap();
    assert_eq!(
        run_preview(&conn, &[&id])[0].status,
        PreviewStatus::SkipConflict
    );
    std::fs::remove_file(&target).unwrap();
    std::fs::remove_file(dir.join("model.3mf")).unwrap();
    std::os::unix::fs::symlink(outside.join("model.3mf"), dir.join("model.3mf")).unwrap();
    assert_eq!(
        run_preview(&conn, &[&id])[0].status,
        PreviewStatus::SkipExternal
    );
    std::fs::remove_file(outside.join("model.3mf")).unwrap();
    assert_eq!(
        run_preview(&conn, &[&id])[0].status,
        PreviewStatus::SkipExternal
    );
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(outside).unwrap();
}
