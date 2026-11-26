//! What the log shows for the folder rename and move (issue #20, package P6): a refusal logs its
//! code with the parameters once, a failed catalog update logs its text once, and when the folder
//! cannot be put back either, the combined text is the only error line (the catalog error alone
//! never had a line of its own). Recorded with the code before the move into the core and kept
//! unchanged by it.
use super::folders::{move_folder_with_conn_fs, rename_folder_with_conn_fs};
use super::fs_boundary::{FaultFs, Op};
use super::test_log::capture;
use super::*;
use std::io::ErrorKind::PermissionDenied;

type Records = Vec<(log::Level, String, String)>;

fn setup(name: &str) -> (Connection, PathBuf, i64, i64) {
    let dir = unique_test_dir(name).canonicalize().unwrap();
    let conn = db::connect_in_memory().unwrap();
    std::fs::create_dir_all(dir.join("Alt")).unwrap();
    std::fs::create_dir_all(dir.join("Ziel")).unwrap();
    let alt = db::insert_folder_with_parent(&conn, "Alt", None, &dir.join("Alt").to_string_lossy()).unwrap();
    let ziel = db::insert_folder_with_parent(&conn, "Ziel", None, &dir.join("Ziel").to_string_lossy()).unwrap();
    let model = dir.join("Alt/a.3mf");
    std::fs::write(&model, b"model").unwrap();
    db::test_insert_minimal_file(&conn, &model.to_string_lossy(), Some(alt)).unwrap();
    conn.execute_batch("CREATE TRIGGER block_paths BEFORE UPDATE OF path ON files BEGIN SELECT RAISE(ABORT, 'blocked'); END;").unwrap();
    (conn, dir, alt, ziel)
}

fn count(records: &Records, level: log::Level, starts: &str) -> usize {
    records.iter().filter(|(l, _, text)| *l == level && text.starts_with(starts)).count()
}

fn levels(records: &Records, level: log::Level) -> usize {
    records.iter().filter(|(l, _, _)| *l == level).count()
}

fn run(f: impl FnOnce() -> CmdResult<()>) -> (CmdResult<()>, Records) {
    let mut result = None;
    let records = capture(|| result = Some(f()));
    (result.unwrap(), records)
}

#[test]
fn rename_logs_a_failed_catalog_update_once() {
    let (conn, dir, alt, _) = setup("folder_log_rename_db");
    let (result, records) = run(|| rename_folder_with_conn_fs(&FaultFs::new(), &conn, alt, "Neu".into(), &[]));
    assert_eq!(result.unwrap_err().message, "database error: blocked");
    assert_eq!(count(&records, log::Level::Error, "database error: blocked"), 1, "{records:?}");
    assert_eq!(levels(&records, log::Level::Error), 1, "{records:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rename_logs_only_the_combined_text_when_the_roll_back_fails_too() {
    let (conn, dir, alt, _) = setup("folder_log_rename_both");
    let fs = FaultFs::new().fail(Op::Rename, 2, PermissionDenied);
    let (result, records) = run(|| rename_folder_with_conn_fs(&fs, &conn, alt, "Neu".into(), &[]));
    let message = result.unwrap_err().message;
    assert!(message.starts_with("DB-Update fehlgeschlagen (database error: blocked) UND Rollback der Ordner-Umbenennung fehlgeschlagen ("), "{message}");
    assert_eq!(count(&records, log::Level::Error, "DB-Update fehlgeschlagen (database error: blocked) UND Rollback der Ordner-Umbenennung fehlgeschlagen ("), 1, "{records:?}");
    assert_eq!(levels(&records, log::Level::Error), 1, "{records:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn move_logs_a_failed_catalog_update_once() {
    let (conn, dir, alt, ziel) = setup("folder_log_move_db");
    let (result, records) = run(|| move_folder_with_conn_fs(&FaultFs::new(), &conn, alt, Some(ziel), &[]));
    assert_eq!(result.unwrap_err().message, "database error: blocked");
    assert_eq!(count(&records, log::Level::Error, "database error: blocked"), 1, "{records:?}");
    assert_eq!(levels(&records, log::Level::Error), 1, "{records:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn move_logs_only_the_combined_text_when_the_roll_back_fails_too() {
    let (conn, dir, alt, ziel) = setup("folder_log_move_both");
    let fs = FaultFs::new().fail(Op::Rename, 2, PermissionDenied);
    let (result, records) = run(|| move_folder_with_conn_fs(&fs, &conn, alt, Some(ziel), &[]));
    let message = result.unwrap_err().message;
    assert!(message.starts_with("DB-Update fehlgeschlagen (database error: blocked) UND Rollback des Ordner-Moves fehlgeschlagen ("), "{message}");
    assert_eq!(count(&records, log::Level::Error, "DB-Update fehlgeschlagen (database error: blocked) UND Rollback des Ordner-Moves fehlgeschlagen ("), 1, "{records:?}");
    assert_eq!(levels(&records, log::Level::Error), 1, "{records:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn refusals_log_their_code_with_the_parameters_once_at_info_level() {
    let (conn, dir, alt, ziel) = setup("folder_log_refusals");
    std::fs::create_dir_all(dir.join("Ziel/Alt")).unwrap();
    let taken = dir.join("Ziel/Alt").display().to_string();
    let (result, records) = run(|| move_folder_with_conn_fs(&FaultFs::new(), &conn, alt, Some(ziel), &[]));
    assert!(result.unwrap_err().is(ErrorCode::FolderMoveNameTaken, &[&taken]));
    assert_eq!(count(&records, log::Level::Info, &format!("folderMoveNameTaken: {taken} (")), 1, "{records:?}");
    assert_eq!(records.len(), 1, "{records:?}");

    let (result, records) = run(|| rename_folder_with_conn_fs(&FaultFs::new(), &conn, ziel, "Alt".into(), &[]));
    let target = dir.join("Alt").display().to_string();
    assert!(result.unwrap_err().is(ErrorCode::RenameTargetFolderExists, &[&target]));
    assert_eq!(count(&records, log::Level::Info, &format!("renameTargetFolderExists: {target} (")), 1, "{records:?}");
    assert_eq!(records.len(), 1, "{records:?}");
    let _ = std::fs::remove_dir_all(dir);
}
