//! What the log shows when the companion files cannot follow a rename and the model cannot be
//! put back either (issue #20, package P4): the original companion error is logged exactly once
//! with its code and parameters, and the combined text is what the caller gets back.
use super::fs_boundary::{FaultFs, Op};
use super::test_log::capture;
use super::*;
use std::io::ErrorKind::PermissionDenied;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

fn setup(name: &str) -> (Connection, PathBuf) {
    let dir = unique_test_dir(name).canonicalize().unwrap();
    let conn = db::connect_in_memory().unwrap();
    db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, &dir.to_string_lossy()).unwrap();
    (conn, dir)
}

fn model_with_companion(conn: &Connection, dir: &Path, name: &str) -> i64 {
    let path = dir.join(name);
    std::fs::write(&path, b"model").unwrap();
    let folder = db::insert_folder_with_parent(conn, "cat", None, &dir.to_string_lossy()).unwrap();
    let id = db::test_insert_minimal_file(conn, &path.to_string_lossy(), Some(folder)).unwrap();
    let source = dir.join("outside-a.png");
    std::fs::write(&source, PNG).unwrap();
    companions::add_companion_with_conn(conn, id, &source, &[]).unwrap();
    id
}

fn count(records: &[(log::Level, String, String)], level: log::Level, starts: &str) -> usize {
    records.iter().filter(|(l, _, text)| *l == level && text.starts_with(starts)).count()
}

#[test]
fn rename_logs_the_companion_error_once_and_returns_the_combined_text() {
    let (conn, dir) = setup("rename_log_companion");
    let id = model_with_companion(&conn, &dir, "Moo.3mf");
    std::fs::write(dir.join("Boo_extras"), b"a file").unwrap();
    // Moving the model back is refused (link and copy), so only the new name is left.
    let fs = FaultFs::new().fail_on(Op::HardLink, Some("Boo.3mf"), 1, PermissionDenied).fail_on(Op::CopyNew, Some("Boo.3mf"), 1, PermissionDenied);
    let mut result = None;
    let records = capture(|| result = Some(files::rename_file_with_conn_fs_case(&fs, &conn, id, "Boo.3mf".into(), &[], false)));
    let error = result.unwrap().unwrap_err();
    assert!(error.message.starts_with("Begleitdateien konnten nicht mitgenommen werden (companionTargetFolderBlocked) UND Rollback der Datei-Umbenennung fehlgeschlagen ("), "{}", error.message);
    assert_eq!(count(&records, log::Level::Info, "companionTargetFolderBlocked"), 1, "{records:?}");
    assert_eq!(count(&records, log::Level::Error, "Begleitdateien konnten nicht mitgenommen werden"), 1, "{records:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn date_rename_logs_the_companion_error_once_and_reports_the_combined_text() {
    let (conn, dir) = setup("date_log_companion");
    let id = model_with_companion(&conn, &dir, "model.3mf");
    let stamp = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_760_011_200);
    let old = dir.join("model.3mf");
    std::fs::File::options().write(true).open(&old).unwrap().set_modified(stamp).unwrap();
    let prefix = import_rename::date_prefix(chrono::DateTime::<chrono::Local>::from(stamp));
    let new_name = format!("{prefix}model.3mf");
    std::fs::write(dir.join(format!("{prefix}model_extras")), b"a file").unwrap();
    // Putting the model back finds its old name taken (someone created it in between).
    let taken = old.clone();
    import_rename::set_race_hook(Some(Box::new(move |stage, path| {
        if stage == "before-file-rename" && path == taken {
            std::fs::write(path, b"someone else").unwrap();
        }
    })));
    let mut results = None;
    let item = DateRenameItem { id: id.to_string(), new_name };
    let records = capture(|| results = Some(date_rename::apply_with_conn(&conn, vec![item], &[])));
    import_rename::set_race_hook(None);
    let results = results.unwrap().unwrap();
    // The result only says the target is taken (the model stays under its new name); the log has the cause.
    assert_eq!(results[0].status, katalog_core::date_rename::ApplyStatus::Failed);
    assert_eq!(count(&records, log::Level::Info, "companionTargetFolderBlocked"), 1, "{records:?}");
    assert_eq!(count(&records, log::Level::Error, "Begleitdateien konnten nicht mitgenommen werden"), 0, "{records:?}");
    let _ = std::fs::remove_dir_all(dir);
}
