//! Builds the message shown when the app cannot open its data folder or catalog at
//! startup. The backend has no access to the frontend's i18n, so every message carries
//! English first and German below it.

use std::path::Path;

use crate::db::error::DbError;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// Which startup step failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStage {
    DataFolder,
    Catalog,
    TrashFolder,
}

/// A startup step that failed before the app state existed.
pub struct StartupFailure {
    pub stage: StartupStage,
    pub path: std::path::PathBuf,
    pub error: Box<dyn std::error::Error>,
}

impl StartupFailure {
    pub fn new(stage: StartupStage, path: &Path, error: impl std::error::Error + 'static) -> Self {
        Self { stage, path: path.to_path_buf(), error: Box::new(error) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cause {
    NewerCatalog,
    NoAccess,
    DiskFull,
    Damaged,
    Unknown,
}

fn classify(error: &(dyn std::error::Error + 'static)) -> Cause {
    use rusqlite::ErrorCode;
    use std::io::ErrorKind;

    if let Some(db_error) = error.downcast_ref::<DbError>() {
        return match db_error {
            DbError::NewerSchema { .. } => Cause::NewerCatalog,
            DbError::Sqlite(rusqlite::Error::SqliteFailure(failure, _)) => match failure.code {
                ErrorCode::ReadOnly | ErrorCode::PermissionDenied | ErrorCode::CannotOpen => Cause::NoAccess,
                ErrorCode::DiskFull => Cause::DiskFull,
                ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt => Cause::Damaged,
                _ => Cause::Unknown,
            },
            _ => Cause::Unknown,
        };
    }
    if let Some(io_error) = error.downcast_ref::<std::io::Error>() {
        return match io_error.kind() {
            ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem => Cause::NoAccess,
            ErrorKind::StorageFull => Cause::DiskFull,
            _ => Cause::Unknown,
        };
    }
    Cause::Unknown
}

/// One line, bounded: the cause is shown verbatim, so it must not blow up the dialog.
fn single_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() > 300 {
        let cut: String = joined.chars().take(300).collect();
        format!("{cut}...")
    } else {
        joined
    }
}

/// `path` is the folder or file that could not be used; only it and the error text are
/// included in the message.
pub fn startup_error_message(stage: StartupStage, path: &Path, error: &(dyn std::error::Error + 'static)) -> String {
    let cause = classify(error);
    let shown_path = if path.as_os_str().is_empty() { "(unknown)".to_string() } else { path.display().to_string() };
    let backups = crate::db::BACKUP_DIR_NAME;

    let (what_en, what_de) = match (stage, cause) {
        (StartupStage::Catalog, Cause::NewerCatalog) => (
            "This catalog was created by a newer version of the app, so this version cannot open it.",
            "Dieser Katalog stammt von einer neueren Version der App, deshalb kann diese Version ihn nicht öffnen.",
        ),
        (StartupStage::DataFolder, _) => (
            "The app's data folder could not be created or used.",
            "Der Datenordner der App konnte nicht angelegt oder verwendet werden.",
        ),
        (StartupStage::Catalog, _) => (
            "The catalog database could not be opened or updated.",
            "Die Katalog-Datenbank konnte nicht geöffnet oder aktualisiert werden.",
        ),
        (StartupStage::TrashFolder, _) => (
            "The app's trash folder could not be created or used.",
            "Der Papierkorb-Ordner der App konnte nicht angelegt oder verwendet werden.",
        ),
    };

    let (todo_en, todo_de) = match cause {
        Cause::NewerCatalog => (
            format!("Your data was not changed. Install the newer app version again, or use a backup copy from the \"{backups}\" folder next to catalog.db."),
            format!("Deine Daten wurden nicht verändert. Installiere die neuere App-Version wieder, oder verwende eine Sicherungskopie aus dem Ordner \"{backups}\" neben catalog.db."),
        ),
        Cause::NoAccess => (
            "Check that you have permission to read and write this location and that it is not read-only, then start the app again.".to_string(),
            "Prüfe, ob du an diesem Ort lesen und schreiben darfst und er nicht schreibgeschützt ist, und starte die App dann erneut.".to_string(),
        ),
        Cause::DiskFull => (
            "Check that the drive has enough free space, then start the app again.".to_string(),
            "Prüfe, ob auf dem Laufwerk genug freier Speicherplatz vorhanden ist, und starte die App dann erneut.".to_string(),
        ),
        Cause::Damaged => (
            format!("The file may be damaged. Do not delete it. Restore a backup copy from the \"{backups}\" folder next to catalog.db, or ask for help."),
            format!("Die Datei ist möglicherweise beschädigt. Lösche sie nicht. Stelle eine Sicherungskopie aus dem Ordner \"{backups}\" neben catalog.db wieder her, oder bitte um Hilfe."),
        ),
        Cause::Unknown => (
            "Check the location and permissions, then start the app again.".to_string(),
            "Prüfe den Ort und die Berechtigungen und starte die App dann erneut.".to_string(),
        ),
    };

    let cause_text = single_line(&error.to_string());
    format!(
        "{what_en}\n\nLocation: {shown_path}\nCause: {cause_text}\n\n{todo_en}\n\n\
         ----------\n\n\
         {what_de}\n\nOrt: {shown_path}\nUrsache: {cause_text}\n\n{todo_de}"
    )
}

/// Shows the message and exits once it is dismissed. `setup` runs on the main thread, so
/// the non-blocking `show` is used: `blocking_show` would wait for an event loop that
/// cannot run yet. Nothing is deleted or rewritten here.
pub fn report_and_exit(app: &tauri::AppHandle, failure: &StartupFailure) {
    log::error!(target: "startup", "startup failed at {:?} ({}): {}", failure.stage, failure.path.display(), failure.error);
    let message = startup_error_message(failure.stage, &failure.path, failure.error.as_ref());
    let handle = app.clone();
    app.dialog()
        .message(message)
        .title("3MF Katalog Manager")
        .kind(MessageDialogKind::Error)
        .show(move |_| handle.exit(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};
    use std::path::PathBuf;

    fn assert_bilingual_with_path(text: &str, path: &Path) {
        assert!(text.contains(&path.display().to_string()), "full path missing: {text}");
        assert!(text.contains("Location:") && text.contains("Ort:"), "both languages expected: {text}");
        assert!(text.contains("Cause:") && text.contains("Ursache:"));
        assert!(text.find("Location:").unwrap() < text.find("Ort:").unwrap(), "English must come first");
    }

    #[test]
    fn data_folder_not_writable() {
        let path = Path::new("/data/app");
        let text = startup_error_message(StartupStage::DataFolder, path, &Error::new(ErrorKind::PermissionDenied, "denied"));
        assert_bilingual_with_path(&text, path);
        assert!(text.contains("data folder") && text.contains("Datenordner"));
        assert!(text.contains("permission") && text.contains("darfst"));
        assert!(text.contains("denied"));
    }

    #[test]
    fn full_disk_asks_for_free_space() {
        let path = Path::new("/data/app");
        let text = startup_error_message(StartupStage::DataFolder, path, &Error::new(ErrorKind::StorageFull, "no space"));
        assert!(text.contains("free space") && text.contains("Speicherplatz"));
    }

    #[test]
    fn read_only_filesystem_counts_as_missing_access() {
        let text = startup_error_message(StartupStage::TrashFolder, Path::new("/data/trash"), &Error::new(ErrorKind::ReadOnlyFilesystem, "ro"));
        assert!(text.contains("trash folder") && text.contains("Papierkorb"));
        assert!(text.contains("read-only") && text.contains("schreibgeschützt"));
    }

    #[test]
    fn newer_catalog_explains_backup_folder_and_unchanged_data() {
        let path = Path::new("/data/app/catalog.db");
        let error = DbError::NewerSchema { found: 99, supported: 5 };
        let text = startup_error_message(StartupStage::Catalog, path, &error);
        assert_bilingual_with_path(&text, path);
        assert!(text.contains("newer version") && text.contains("neueren Version"));
        assert!(text.contains(crate::db::BACKUP_DIR_NAME));
        assert!(text.contains("was not changed") && text.contains("nicht verändert"));
    }

    #[test]
    fn damaged_catalog_warns_not_to_delete() {
        let failure = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB);
        let error = DbError::Sqlite(rusqlite::Error::SqliteFailure(failure, None));
        let text = startup_error_message(StartupStage::Catalog, Path::new("/data/catalog.db"), &error);
        assert!(text.contains("Do not delete") && text.contains("Lösche sie nicht"));
    }

    #[test]
    fn unknown_error_gets_the_generic_text_with_path_and_cause() {
        let path = PathBuf::from("/data/app");
        let error = DbError::Other("something odd\nsecond line".into());
        let text = startup_error_message(StartupStage::Catalog, &path, &error);
        assert_bilingual_with_path(&text, &path);
        assert!(text.contains("something odd second line"), "cause must be one line");
        assert!(!text.contains("was not changed") && !text.contains("nicht verändert"));
        assert!(text.contains("Check the location") && text.contains("Prüfe den Ort"));
    }

    #[test]
    fn overlong_cause_is_cut_and_missing_path_is_marked() {
        let error = DbError::Other("x".repeat(1000));
        let text = startup_error_message(StartupStage::DataFolder, Path::new(""), &error);
        assert!(text.contains("(unknown)"));
        assert!(text.len() < 2000);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("startup-error-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn trash_failure_leaves_an_old_catalog_untouched() {
        let dir = temp_dir("blocked-trash");
        let path = dir.join("catalog.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE sentinel (value TEXT); PRAGMA user_version = 0;").unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        std::fs::write(dir.join("trash"), b"blocker").unwrap();
        let error = crate::open_storage_at(&dir).expect_err("trash must fail");
        assert_eq!(error.stage, StartupStage::TrashFolder);
        assert!(std::fs::read(&path).unwrap() == before, "catalog must stay byte-identical");
        let conn = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap(), 0);
        drop(conn);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn real_catalog_from_a_newer_version_is_left_untouched_and_explained() {
        let dir = temp_dir("newer");
        let db_path = dir.join("catalog.db");
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(crate::db::SCHEMA_SQL).unwrap();
            conn.pragma_update(None, "user_version", crate::db::migrations_current_version() + 1).unwrap();
        }
        let before = std::fs::read(&db_path).unwrap();

        let error = crate::db::connect_with_warning(&db_path).expect_err("newer catalog must be rejected");
        let text = startup_error_message(StartupStage::Catalog, &db_path, &error);

        assert_bilingual_with_path(&text, &db_path);
        assert!(text.contains("newer version"));
        assert_eq!(std::fs::read(&db_path).unwrap(), before, "catalog must stay byte-identical");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn real_catalog_in_a_missing_folder_reports_access_problem() {
        let dir = temp_dir("missing");
        let db_path = dir.join("does-not-exist").join("catalog.db");

        let error = crate::db::connect_with_warning(&db_path).expect_err("open must fail");
        let text = startup_error_message(StartupStage::Catalog, &db_path, &error);

        assert_bilingual_with_path(&text, &db_path);
        assert!(text.contains("permission") && text.contains("schreibgeschützt"));
        assert!(!db_path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
