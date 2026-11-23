use super::*;

#[cfg(test)]
use chrono::{DateTime, Local};
use katalog_core::date_rename::{self as core, DateRenameEffect, StdDateRenameFs};
#[cfg(test)]
use katalog_core::date_rename::{ApplyStatus, PreviewStatus};
pub use katalog_core::date_rename::{
    DateRenameItem, DateRenamePreview, DateRenameResult, DateSource,
};

#[cfg(test)]
const MAX_ITEMS: usize = 5000;

/// Keeps desktop companion handling and reversible disk operations at the host
/// boundary; the core supplies the model-row update through the commit callback.
struct DesktopDateRenameEffect<'a> {
    conn: &'a Connection,
}

impl DateRenameEffect for DesktopDateRenameEffect<'_> {
    fn rename(
        &self,
        file: &FileRecord,
        old: &Path,
        new: &Path,
        commit: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        rename::rename_file_on_disk(old, new).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CmdError::coded_with(ErrorCode::FileTargetExists, [new.display()]).message
            } else {
                error.to_string()
            }
        })?;
        let carried = match companions::carry(self.conn, file.id, old, new) {
            Ok(carried) => carried,
            Err(error) => {
                if let Err(rollback_err) = rename::rename_file_on_disk(new, old) {
                    return Err(format!(
                        "Begleitdateien konnten nicht mitgenommen werden ({error}) UND Rollback der Datei-Umbenennung fehlgeschlagen ({rollback_err}) - Datei heisst jetzt {}",
                        new.display()
                    ));
                }
                return Err(error.message);
            }
        };
        if let Err(db_err) = commit() {
            carried.undo(self.conn);
            if let Err(rollback_err) = rename::rename_file_on_disk(new, old) {
                return Err(format!(
                    "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Datei-Umbenennung fehlgeschlagen ({rollback_err}) - Datei heisst jetzt {}, DB verweist weiter auf {}",
                    new.display(),
                    old.display()
                ));
            }
            return Err(db_err);
        }
        carried.finish();
        log::debug!(target: "datei", "umbenannt: {} -> {}", old.display(), new.display());
        Ok(())
    }
}

fn preview_with_conn(
    conn: &Connection,
    file_ids: &[String],
    source: DateSource,
    sensitive: &[PathBuf],
) -> CmdResult<Vec<DateRenamePreview>> {
    core::preview(conn, file_ids, source, sensitive, &StdDateRenameFs).map_err(Into::into)
}

fn apply_with_conn(
    conn: &Connection,
    items: Vec<DateRenameItem>,
    sensitive: &[PathBuf],
) -> CmdResult<Vec<DateRenameResult>> {
    core::apply(
        conn,
        items,
        sensitive,
        &StdDateRenameFs,
        &DesktopDateRenameEffect { conn },
    )
    .map_err(Into::into)
}

/// Plans `<date>_<name>` renames without touching disk or database.
#[tauri::command]
pub fn preview_date_rename(
    state: State<AppState>,
    file_ids: Vec<String>,
    date_source: DateSource,
) -> CmdResult<Vec<DateRenamePreview>> {
    let conn = lock_db(&state)?;
    preview_with_conn(&conn, &file_ids, date_source, &state.sensitive_dirs)
}

/// Renames the confirmed files. Every entry is checked again, because disk and
/// catalog may have changed since the preview.
#[tauri::command]
pub fn apply_date_rename(
    state: State<AppState>,
    items: Vec<DateRenameItem>,
) -> CmdResult<Vec<DateRenameResult>> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let conn = lock_db(&state)?;
    apply_with_conn(&conn, items, &state.sensitive_dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let outside = unique_test_dir("date_rename_outside").canonicalize().unwrap();
        let external = add(&conn, &outside, "external.3mf");
        // A catalog entry owning the target path although its file is gone.
        let _ghost = add(&conn, &dir, &format!("{prefix}ghost.3mf"));
        std::fs::remove_file(dir.join(format!("{prefix}ghost.3mf"))).unwrap();
        let ghost_owner = add(&conn, &dir, "ghost.3mf");

        let ids = [
            &plain, &prefixed, &taken, &upper, &lower, &missing, &trashed, &long, &external,
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
        assert_eq!(status_of(&preview, &ghost_owner), PreviewStatus::SkipConflict);
        let unknown = run_preview(&conn, &[&"99999".to_string()]);
        assert_eq!(unknown[0].status, PreviewStatus::SkipMissing);
        // Planning writes nothing.
        assert!(dir.join("plain.3mf").exists());
        assert_eq!(
            db::get_file(&conn, plain.parse().unwrap()).unwrap().unwrap().name,
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
        assert_eq!(modified_name.new_name, format!("{}model.3mf", modified_prefix()));
        let imported = DateTime::parse_from_rfc3339("2025-03-04T05:06:07Z")
            .unwrap()
            .with_timezone(&Local);
        assert_eq!(
            imported_name.new_name,
            format!("{}model.3mf", import_rename::date_prefix(imported))
        );
        assert_ne!(modified_name.new_name, imported_name.new_name);
        conn.execute("UPDATE files SET imported_at = 'garbage' WHERE id = ?1", [&id])
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
            vec![DateRenameItem { id: id.clone(), new_name: new_name.clone() }],
            &[],
        )
        .unwrap();
        assert_eq!(results[0].status, ApplyStatus::Renamed, "{:?}", results[0].reason);
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
        let results = apply_with_conn(&conn, vec![DateRenameItem { id: id.clone(), new_name }], &[]).unwrap();
        import_rename::set_race_hook(None);
        assert_eq!(results[0].status, ApplyStatus::Failed);
        assert_eq!(results[0].reason.as_deref(), Some("targetTaken"));
        assert_eq!(std::fs::read(&target).unwrap(), b"created meanwhile");
        assert!(dir.join("model.3mf").exists(), "the original stays where it was");
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
                vec![DateRenameItem { id: id.clone(), new_name: new_name.clone() }],
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
        let outside = unique_test_dir("date_rename_apply_outside").canonicalize().unwrap();
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
            vec![DateRenameItem { id, new_name: format!("{prefix}model.3mf") }],
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
            vec![DateRenameItem { id: id.clone(), new_name: format!("{prefix}model.3mf") }],
            std::slice::from_ref(&dir),
        )
        .unwrap();
        assert_eq!(results[0].status, ApplyStatus::Failed);
        assert!(dir.join("model.3mf").exists());
        let many: Vec<DateRenameItem> = (0..=MAX_ITEMS)
            .map(|n| DateRenameItem { id: n.to_string(), new_name: String::new() })
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
            vec![DateRenameItem { id: id.clone(), new_name: first[0].new_name.clone() }],
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
            vec![DateRenameItem { id, new_name: first[0].new_name.clone() }],
            &[],
        )
        .unwrap();
        assert_eq!(replay[0].reason.as_deref(), Some("hasPrefix"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_carries_companions_and_restores_them_on_failure() {
        for failure in [None, Some("catalog"), Some("companion")] {
            let (conn, dir) = setup("date_rename_companions");
            let id = add(&conn, &dir, "model.3mf");
            let new_name = format!("{}model.3mf", modified_prefix());
            let old_extras = companions::extras_dir_for(&dir.join("model.3mf")).unwrap();
            let new_extras = companions::extras_dir_for(&dir.join(&new_name)).unwrap();
            std::fs::create_dir(&old_extras).unwrap();
            let old_note = old_extras.join("note.txt");
            let new_note = new_extras.join("note.txt");
            std::fs::write(&old_note, b"companion").unwrap();
            let companion_id = db::companions::insert_companion(
                &conn,
                id.parse().unwrap(),
                "note.txt",
                &old_note.to_string_lossy(),
                "text",
                Some(9),
                "2025-03-04T05:06:07Z",
            )
            .unwrap();
            if failure == Some("catalog") {
                conn.execute_batch(
                    "CREATE TRIGGER block_rename BEFORE UPDATE ON files
                     BEGIN SELECT RAISE(ABORT, 'simulated'); END;",
                )
                .unwrap();
            } else if failure == Some("companion") {
                std::fs::write(&new_extras, b"blocked").unwrap();
            }

            let results = apply_with_conn(
                &conn,
                vec![DateRenameItem {
                    id: id.clone(),
                    new_name: new_name.clone(),
                }],
                &[],
            )
            .unwrap();
            let row = db::get_file(&conn, id.parse().unwrap()).unwrap().unwrap();
            let companion = db::companions::get_companion(&conn, companion_id)
                .unwrap()
                .unwrap();
            if failure.is_none() {
                assert_eq!(results[0].status, ApplyStatus::Renamed);
                assert_eq!(row.path, dir.join(&new_name).to_string_lossy());
                assert_eq!(companion.path, new_note.to_string_lossy());
                assert_eq!(std::fs::read(new_note).unwrap(), b"companion");
                assert!(!old_extras.exists());
                assert!(dir.join(&new_name).exists());
                assert!(!dir.join("model.3mf").exists());
            } else {
                assert_eq!(results[0].status, ApplyStatus::Failed);
                assert_eq!(row.path, dir.join("model.3mf").to_string_lossy());
                assert_eq!(companion.path, old_note.to_string_lossy());
                assert_eq!(std::fs::read(old_note).unwrap(), b"companion");
                assert!(dir.join("model.3mf").exists());
                assert!(!dir.join(&new_name).exists());
                if failure == Some("companion") {
                    assert_eq!(std::fs::read(new_extras).unwrap(), b"blocked");
                } else {
                    assert!(!new_extras.exists());
                }
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
