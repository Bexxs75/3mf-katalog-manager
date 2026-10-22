//! Date prefix for imported model files (optional setting) and the placement that
//! applies it. Shared by the import worker and the desktop rename of existing files.
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::file_names as folders;
use super::placement::{move_file, reject_if_sensitive_path};
use super::{ImportError as CmdError, ImportResult as CmdResult};
use crate::db;

pub const SETTING: &str = "rename_files_on_import";

pub fn enabled(conn: &Connection) -> CmdResult<bool> {
    Ok(db::printer_link::get_setting(conn, SETTING)
        .map_err(|e| format!("database: {e}"))?
        .as_deref()
        == Some("1"))
}

/// Prefix layout shared by the import and the rename of existing files, so both
/// produce and recognise the same names.
const PREFIX_FORMAT: &str = "%Y-%m-%d_%H-%M_";
/// Byte length of a formatted prefix (all ASCII).
pub const PREFIX_LEN: usize = 17;

pub fn date_prefix(time: chrono::DateTime<chrono::Local>) -> String {
    time.format(PREFIX_FORMAT).to_string()
}

pub fn has_date_prefix(name: &str) -> bool {
    static PREFIX_PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PREFIX_PATTERN
        .get_or_init(|| regex::Regex::new(r"^\d{4}-\d{2}-\d{2}_\d{2}-\d{2}_").unwrap())
        .is_match(name)
}

pub fn import_name(original: &str, prefix: Option<&str>, number: u32) -> String {
    if prefix.is_none() && number == 1 {
        return original.to_string();
    }
    let prefix = if has_date_prefix(original) {
        ""
    } else {
        prefix.unwrap_or("")
    };
    let path = Path::new(original);
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    // A source name valid on Unix can still contain characters forbidden on Windows.
    let mut stem: String = stem
        .chars()
        .map(|c| {
            if c.is_control() || folders::FORBIDDEN_DIR_NAME_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let suffix = if number > 1 {
        format!(" ({number})")
    } else {
        String::new()
    };
    let budget = folders::MAX_DIR_NAME_BYTES - prefix.len() - suffix.len() - extension.len();
    while stem.len() > budget {
        stem.pop();
    }
    format!("{prefix}{stem}{suffix}{extension}")
}

pub fn catalog_base(conn: &Connection) -> CmdResult<Option<PathBuf>> {
    Ok(
        db::printer_link::get_setting(conn, folders::SETTING_CATALOG_BASE_DIR)
            .map_err(|e| format!("database: {e}"))?
            .and_then(|p| Path::new(&p).canonicalize().ok()),
    )
}

pub fn is_internal(path: &Path, base: Option<&Path>) -> bool {
    base.zip(path.canonicalize().ok())
        .is_some_and(|(base, path)| path.starts_with(base))
}

// Lets tests act as another program that touches the disk between a check and the
// file operation that follows it.
#[cfg(any(test, feature = "test-support"))]
pub type RaceHook = Box<dyn Fn(&str, &Path)>;
#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static RACE: std::cell::RefCell<Option<RaceHook>> =
        const { std::cell::RefCell::new(None) };
}
/// Installs (or, with `None`, removes) the hook of the current test thread.
#[cfg(any(test, feature = "test-support"))]
pub fn set_race_hook(hook: Option<RaceHook>) {
    RACE.with(|race| *race.borrow_mut() = hook);
}
#[cfg(any(test, feature = "test-support"))]
pub fn race_point(stage: &str, path: &Path) {
    RACE.with(|race| {
        if let Some(hook) = race.borrow().as_ref() {
            hook(stage, path);
        }
    });
}
#[cfg(not(any(test, feature = "test-support")))]
pub fn race_point(_stage: &str, _path: &Path) {}

/// Only called for rows created by this import, never for rescans or manual moves.
pub fn place(
    conn: &Connection,
    id: i64,
    target: Option<i64>,
    prefix: Option<&str>,
    sensitive: &[PathBuf],
) -> CmdResult<()> {
    if prefix.is_none() {
        return super::placement::move_file_to_folder_with_conn(conn, id, target, sensitive);
    }
    let file = db::get_file(conn, id)
        .map_err(|e| format!("database: {e}"))?
        .ok_or_else(|| CmdError::from("file missing"))?;
    let old = PathBuf::from(&file.path);
    if target.is_none() && !is_internal(&old, catalog_base(conn)?.as_deref()) {
        return Ok(());
    }
    reject_if_sensitive_path(&old, sensitive).map_err(|_| CmdError::expected("protected"))?;
    let (dir, folder_id) = if let Some(target) = target {
        let folders = db::list_folders(conn).map_err(|e| format!("database: {e}"))?;
        let folder = folders
            .iter()
            .find(|f| f.id == target)
            .ok_or_else(|| CmdError::expected("targetMissing"))?;
        (PathBuf::from(&folder.path), Some(target))
    } else {
        (
            old.parent().ok_or("invalid file path")?.to_path_buf(),
            file.folder_id,
        )
    };
    if !dir.is_dir() {
        return Err(CmdError::expected("targetMissing"));
    }
    reject_if_sensitive_path(&dir, sensitive).map_err(|_| CmdError::expected("protected"))?;
    let Some(original) = old.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let mut number = 1;
    let new = loop {
        let name = import_name(original, prefix, number);
        folders::validate_name(&name, true, "Dateiname")?;
        let candidate = dir.join(name);
        reject_if_sensitive_path(&candidate, sensitive)
            .map_err(|_| CmdError::expected("protected"))?;
        if candidate == old {
            break candidate;
        }
        // `move_file` links instead of renaming, so a target that appeared since the
        // name was chosen is reported as AlreadyExists, never replaced. In the same
        // directory the link keeps the inode and thus all file metadata, like a rename.
        race_point("before-move", &candidate);
        match move_file(&old, &candidate) {
            Ok(()) => break candidate,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => number += 1,
            Err(e) => return Err(e.to_string().into()),
        }
    };
    let updated = (|| -> Result<(), db::error::DbError> {
        let tx = conn.unchecked_transaction()?;
        db::update_file_folder(&tx, id, folder_id, &new.to_string_lossy())?;
        db::rename_file(
            &tx,
            id,
            new.file_name().unwrap().to_str().unwrap(),
            &new.to_string_lossy(),
        )?;
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = updated {
        race_point("before-rollback", &old);
        // Never replaces a file that was created at the old path in the meantime; the
        // failure is reported together with the location of the renamed file.
        let rollback = if new == old {
            Ok(())
        } else {
            move_file(&new, &old)
        };
        if let Err(rollback) = rollback {
            return Err(format!(
                "database: {error}; Rollback fehlgeschlagen ({rollback}); Datei: {}",
                new.display()
            )
            .into());
        }
        return Err(format!("database: {error}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unique_test_dir;
    const PREFIX: &str = "2026-10-12_14-41_";

    #[test]
    fn prefix_format_idempotence_and_utf8_length() {
        use chrono::TimeZone;
        let time = chrono::FixedOffset::east_opt(7200)
            .unwrap()
            .with_ymd_and_hms(2026, 10, 12, 14, 41, 59)
            .unwrap();
        assert_eq!(time.format("%Y-%m-%d_%H-%M_").to_string(), PREFIX);
        let name = "Basteln_Cardboard+Crafting+Tools.3mf";
        assert_eq!(
            import_name(name, Some(PREFIX), 1),
            format!("{PREFIX}{name}")
        );
        assert_eq!(import_name(name, None, 1), name);
        let prefixed = format!("{PREFIX}{name}");
        assert_eq!(
            import_name(&prefixed, Some("2027-01-01_00-00_"), 1),
            prefixed
        );
        for suffix in [".3mf", ".STL", ".step"] {
            let long = format!("{}{suffix}", "ä🦀".repeat(60));
            for number in [1, 2, 100] {
                let result = import_name(&long, Some(PREFIX), number);
                assert!(result.starts_with(PREFIX));
                assert!(result.ends_with(suffix));
                assert!(result.len() <= 255);
                folders::validate_name(&result, true, "Dateiname").unwrap();
            }
        }
    }

    fn fixture(internal: bool) -> (Connection, PathBuf, PathBuf, i64, i64) {
        let root = unique_test_dir("import_rename");
        let catalog = root.join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        let source = if internal {
            catalog.join("model.stl")
        } else {
            root.join("model.stl")
        };
        std::fs::write(&source, b"original").unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::printer_link::set_setting(
            &conn,
            folders::SETTING_CATALOG_BASE_DIR,
            catalog.to_str().unwrap(),
        )
        .unwrap();
        let folder =
            db::insert_folder_with_parent(&conn, "catalog", None, catalog.to_str().unwrap())
                .unwrap();
        let id = db::test_insert_minimal_file(
            &conn,
            source.to_str().unwrap(),
            internal.then_some(folder),
        )
        .unwrap();
        (conn, root, source, folder, id)
    }

    #[test]
    fn default_off_and_setting_persists() {
        let conn = db::connect_in_memory().unwrap();
        assert!(!enabled(&conn).unwrap());
        db::printer_link::set_setting(&conn, SETTING, "1").unwrap();
        assert!(enabled(&conn).unwrap());
        db::printer_link::set_setting(&conn, SETTING, "0").unwrap();
        assert!(!enabled(&conn).unwrap());
    }

    #[test]
    fn external_files_move_only_when_placement_is_requested() {
        for placement in [false, true] {
            let (conn, root, source, folder, id) = fixture(false);
            place(&conn, id, placement.then_some(folder), Some(PREFIX), &[]).unwrap();
            let row = db::get_file(&conn, id).unwrap().unwrap();
            if placement {
                assert_eq!(row.name, format!("{PREFIX}model.stl"));
                assert_eq!(row.folder_id, Some(folder));
                assert!(!source.exists());
            } else {
                assert_eq!(row.path, source.to_str().unwrap());
                assert_eq!(std::fs::read(&source).unwrap(), b"original");
            }
            assert_eq!(std::fs::read(&row.path).unwrap(), b"original");
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn disabled_placement_preserves_name_and_moves_source() {
        let (conn, root, source, folder, id) = fixture(false);
        place(&conn, id, Some(folder), None, &[]).unwrap();
        let row = db::get_file(&conn, id).unwrap().unwrap();
        assert_eq!(row.name, "model.stl");
        assert!(!source.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn internal_file_is_renamed_and_collision_is_numbered() {
        let (conn, root, source, _, id) = fixture(true);
        let first = root.join("catalog").join(format!("{PREFIX}model.stl"));
        std::fs::write(&first, b"other").unwrap();
        place(&conn, id, None, Some(PREFIX), &[]).unwrap();
        let row = db::get_file(&conn, id).unwrap().unwrap();
        assert_eq!(row.name, format!("{PREFIX}model (2).stl"));
        assert!(!source.exists());
        assert_eq!(std::fs::read(first).unwrap(), b"other");
        place(&conn, id, None, Some(PREFIX), &[]).unwrap();
        assert_eq!(db::get_file(&conn, id).unwrap().unwrap().path, row.path);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn long_unicode_names_with_collisions_stay_valid_on_disk() {
        let (conn, root, source, folder, id) = fixture(false);
        let name = format!("{}.3mf", "ä".repeat(125));
        let long_source = source.with_file_name(&name);
        std::fs::rename(&source, &long_source).unwrap();
        db::rename_file(&conn, id, &name, long_source.to_str().unwrap()).unwrap();
        let collision = root
            .join("catalog")
            .join(import_name(&name, Some(PREFIX), 1));
        std::fs::write(&collision, b"existing").unwrap();
        place(&conn, id, Some(folder), Some(PREFIX), &[]).unwrap();
        let row = db::get_file(&conn, id).unwrap().unwrap();
        assert!(row.name.ends_with(" (2).3mf"));
        folders::validate_name(&row.name, true, "Dateiname").unwrap();
        assert_eq!(std::fs::read(&row.path).unwrap(), b"original");
        assert_eq!(std::fs::read(collision).unwrap(), b"existing");
        assert!(!long_source.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_path_placement_still_attaches_the_catalog_folder() {
        let (conn, root, source, folder, id) = fixture(true);
        let prefixed = source.with_file_name(format!("{PREFIX}model.stl"));
        std::fs::rename(&source, &prefixed).unwrap();
        let source = prefixed;
        db::rename_file(
            &conn,
            id,
            source.file_name().unwrap().to_str().unwrap(),
            source.to_str().unwrap(),
        )
        .unwrap();
        db::update_file_folder(&conn, id, None, source.to_str().unwrap()).unwrap();
        place(&conn, id, Some(folder), Some(PREFIX), &[]).unwrap();
        let row = db::get_file(&conn, id).unwrap().unwrap();
        assert_eq!(row.folder_id, Some(folder));
        assert_eq!(row.name, format!("{PREFIX}model.stl"));
        assert!(source.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_restores_disk_and_all_database_columns() {
        for internal in [false, true] {
            let (conn, root, source, folder, id) = fixture(internal);
            let before = db::get_file(&conn, id).unwrap().unwrap();
            conn.execute_batch("CREATE TRIGGER reject_import_name BEFORE UPDATE OF name ON files BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
            assert!(place(&conn, id, Some(folder), Some(PREFIX), &[]).is_err());
            let row = db::get_file(&conn, id).unwrap().unwrap();
            assert_eq!(
                (row.name, row.path, row.folder_id),
                (before.name, before.path, before.folder_id)
            );
            assert_eq!(std::fs::read(source).unwrap(), b"original");
            assert!(!root
                .join("catalog")
                .join(format!("{PREFIX}model.stl"))
                .exists());
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    fn on_race(hook: impl Fn(&str, &Path) + 'static) {
        RACE.with(|race| *race.borrow_mut() = Some(Box::new(hook)));
    }

    #[test]
    fn target_created_after_the_free_check_is_never_overwritten() {
        for internal in [true, false] {
            let (conn, root, source, folder, id) = fixture(internal);
            let target = root.join("catalog").join(format!("{PREFIX}model.stl"));
            let expected = target.clone();
            on_race(move |stage, _| {
                if stage == "before-move" && !expected.exists() {
                    std::fs::write(&expected, b"created meanwhile").unwrap();
                }
            });
            place(&conn, id, Some(folder), Some(PREFIX), &[]).unwrap();
            RACE.with(|race| *race.borrow_mut() = None);
            assert_eq!(std::fs::read(&target).unwrap(), b"created meanwhile");
            let row = db::get_file(&conn, id).unwrap().unwrap();
            assert_eq!(row.name, format!("{PREFIX}model (2).stl"));
            assert_eq!(std::fs::read(&row.path).unwrap(), b"original");
            assert!(!source.exists());
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn rollback_never_overwrites_a_source_recreated_meanwhile() {
        for internal in [true, false] {
            let (conn, root, source, folder, id) = fixture(internal);
            conn.execute_batch("CREATE TRIGGER reject_import_name BEFORE UPDATE OF name ON files BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
            let recreated = source.clone();
            on_race(move |stage, _| {
                if stage == "before-rollback" {
                    std::fs::write(&recreated, b"recreated meanwhile").unwrap();
                }
            });
            let error = place(&conn, id, Some(folder), Some(PREFIX), &[]).unwrap_err();
            RACE.with(|race| *race.borrow_mut() = None);
            assert!(error.to_string().contains("Rollback"), "{error}");
            assert_eq!(std::fs::read(&source).unwrap(), b"recreated meanwhile");
            let moved = root.join("catalog").join(format!("{PREFIX}model.stl"));
            assert_eq!(std::fs::read(moved).unwrap(), b"original");
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
