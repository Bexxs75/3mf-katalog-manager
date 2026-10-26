//! Catalog backup as a ZIP: the same archive layout the desktop app writes
//! (`catalog.db` + `settings.json`) in the same entry layout. Whether the desktop
//! import accepts it depends on that machine (platform, paths, schema version,
//! size limit), see the hint in the web UI. Model files are not part of it.
use std::{
    fs::File,
    io,
    path::Path,
};

use rusqlite::Connection;

/// Entry names the desktop import looks up by name.
pub const DB_ENTRY: &str = "catalog.db";
pub const SETTINGS_ENTRY: &str = "settings.json";

/// Browser-side view preferences the desktop backup stores in `settings.json`.
const SETTINGS_KEYS: [&str; 4] = [
    "3mf-katalog-theme",
    "3mf-katalog-display-preference",
    "3mf-katalog-language",
    "3mf-katalog-density",
];

/// The server cannot see a browser's localStorage. Every key is written as
/// `null`, which the desktop import treats as "use the default" - the same
/// shape the desktop writes for settings that were never changed.
pub fn settings_json() -> String {
    let entries: Vec<String> = SETTINGS_KEYS.iter().map(|key| format!("\"{key}\":null")).collect();
    format!("{{{}}}", entries.join(","))
}

/// Size of the database as SQLite reports it (pages * page size); the basis for
/// the free-space check before a snapshot is taken.
pub fn database_bytes(conn: &Connection) -> Result<u64, String> {
    let pages: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    Ok((pages.max(0) as u64).saturating_mul(page_size.max(0) as u64))
}

/// Pause between attempts when another connection holds a conflicting lock.
const BUSY_PAUSE: std::time::Duration = std::time::Duration::from_millis(50);
/// About ten seconds of waiting in total, then the export fails instead of spinning.
const BUSY_ATTEMPTS: u32 = 200;

/// Copies the database into `dest` (an existing, empty file) with SQLite's online
/// backup API. A raw file copy could catch a half-written state. One backup step
/// covers the whole database inside a single read transaction, so the copy is
/// consistent. In rollback-journal mode (the catalog's mode) that read lock keeps
/// other connections from committing until the step ends: the caller must make
/// sure no writer of this process competes meanwhile, e.g. by holding the mutex
/// of the write connection while copying (or by copying through that connection).
pub fn snapshot(conn: &Connection, dest: &Path) -> Result<(), String> {
    snapshot_with(conn, dest, BUSY_ATTEMPTS, BUSY_PAUSE)
}

fn snapshot_with(conn: &Connection, dest: &Path, attempts: u32, pause: std::time::Duration) -> Result<(), String> {
    use rusqlite::backup::StepResult;
    let mut target = Connection::open(dest).map_err(|e| e.to_string())?;
    let backup = rusqlite::backup::Backup::new(conn, &mut target).map_err(|e| e.to_string())?;
    for _ in 0..attempts {
        match backup.step(-1).map_err(|e| e.to_string())? {
            StepResult::Done => {
                drop(backup);
                drop(target);
                return Ok(());
            }
            // A contended lock is retried after a pause, never in a tight loop.
            StepResult::Busy | StepResult::Locked => std::thread::sleep(pause),
            StepResult::More => {}
            _ => {}
        }
    }
    Err("database stayed locked".into())
}

/// Writes the backup archive into `out` and hands the file back. The database
/// is streamed from `db_file`, never loaded into memory as a whole.
pub fn write_zip(db_file: &Path, settings_json: &str, out: File) -> io::Result<File> {
    use io::Write;
    let mut source = File::open(db_file)?;
    let size = source.metadata()?.len();
    let mut zip = zip::ZipWriter::new(out);
    // Without the flag the writer cannot represent an entry beyond 4 GiB.
    let options = zip::write::SimpleFileOptions::default().large_file(size >= u64::from(u32::MAX));
    zip.start_file(DB_ENTRY, options).map_err(io::Error::other)?;
    io::copy(&mut source, &mut zip)?;
    zip.start_file(SETTINGS_ENTRY, zip::write::SimpleFileOptions::default()).map_err(io::Error::other)?;
    zip.write_all(settings_json.as_bytes())?;
    zip.finish().map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, unique_test_dir};
    use std::io::Read;

    fn seeded(dir: &Path) -> (std::path::PathBuf, Connection) {
        let path = dir.join("catalog.db");
        let conn = db::connect(&path).unwrap();
        db::test_insert_minimal_file(&conn, "/models/a.stl", None).unwrap();
        db::test_insert_minimal_file(&conn, "/models/b.stl", None).unwrap();
        (path, conn)
    }

    fn empty_target(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        File::create(&path).unwrap();
        path
    }

    #[test]
    fn snapshot_is_a_complete_valid_catalog_with_the_same_schema_version() {
        let dir = unique_test_dir("export-snapshot");
        let (path, _writer) = seeded(&dir);
        let reader = db::open_read_only(&path).unwrap();
        let dest = empty_target(&dir, "out.db");
        snapshot(&reader, &dest).unwrap();

        let copy = Connection::open(&dest).unwrap();
        let check: String = copy.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(check, "ok");
        let version: i64 = copy.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, db::migrations_current_version());
        let files: i64 = copy.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap();
        assert_eq!(files, 2);
        // The desktop import rejects foreign code in the schema.
        let foreign: i64 = copy
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type IN ('trigger','view') OR sql LIKE 'CREATE VIRTUAL TABLE%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(foreign, 0);
        // Tables the desktop import requires.
        for (table, column) in [("files", "trash_path"), ("folders", "path"), ("app_settings", "value"), ("printer_connections", "address")] {
            let found: i64 = copy
                .query_row(&format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name='{column}'"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(found, 1, "{table}.{column}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn snapshot_stays_consistent_while_another_connection_writes_in_wal_mode() {
        consistent_while_writing(true);
    }

    #[test]
    fn snapshot_stays_consistent_while_another_connection_writes_in_rollback_mode() {
        consistent_while_writing(false);
    }

    fn consistent_while_writing(wal: bool) {
        let dir = unique_test_dir("export-concurrent");
        let path = dir.join("live.db");
        let writer = Connection::open(&path).unwrap();
        if wal {
            writer.pragma_update(None, "journal_mode", "WAL").unwrap();
        }
        writer.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        writer.execute_batch("CREATE TABLE a(n INTEGER); CREATE TABLE b(n INTEGER); INSERT INTO a VALUES (0); INSERT INTO b VALUES (0);").unwrap();
        // Rollback mode: like the server, writer and copy take turns on one mutex.
        let gate = std::sync::Arc::new(std::sync::Mutex::new(()));
        let writer_gate = gate.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut writer = writer;
            let mut n = 1;
            while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                {
                    let _turn = (!wal).then(|| writer_gate.lock().unwrap());
                    // Both rows or neither: a torn snapshot would show different counts.
                    let tx = writer.transaction().unwrap();
                    tx.execute("INSERT INTO a VALUES (?1)", [n]).unwrap();
                    tx.execute("INSERT INTO b VALUES (?1)", [n]).unwrap();
                    tx.commit().unwrap();
                }
                // std mutexes are unfair; without a pause the copy would never get its turn.
                std::thread::sleep(std::time::Duration::from_micros(300));
                n += 1;
            }
            n
        });
        let reader = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        for round in 0..10 {
            let dest = empty_target(&dir, &format!("out{round}.db"));
            {
                let _turn = (!wal).then(|| gate.lock().unwrap());
                snapshot(&reader, &dest).unwrap();
            }
            let copy = Connection::open(&dest).unwrap();
            let counts: (i64, i64) = copy
                .query_row("SELECT (SELECT COUNT(*) FROM a), (SELECT COUNT(*) FROM b)", [], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap();
            assert_eq!(counts.0, counts.1, "round {round}");
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(thread.join().unwrap() > 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn zip_has_the_desktop_entry_layout_and_the_database_round_trips() {
        let dir = unique_test_dir("export-zip");
        let (path, _writer) = seeded(&dir);
        let reader = db::open_read_only(&path).unwrap();
        let snapshot_path = empty_target(&dir, "snap.db");
        snapshot(&reader, &snapshot_path).unwrap();
        let out = File::create(dir.join("backup.zip")).unwrap();
        write_zip(&snapshot_path, &settings_json(), out).unwrap();

        let mut archive = zip::ZipArchive::new(File::open(dir.join("backup.zip")).unwrap()).unwrap();
        let names: Vec<String> = (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().unwrap().to_string()).collect();
        assert_eq!(names, [DB_ENTRY, SETTINGS_ENTRY]);
        let mut bytes = Vec::new();
        archive.by_name(DB_ENTRY).unwrap().read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, std::fs::read(&snapshot_path).unwrap());
        assert!(bytes.starts_with(b"SQLite format 3\0"));
        let mut settings = String::new();
        archive.by_name(SETTINGS_ENTRY).unwrap().read_to_string(&mut settings).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&settings).unwrap();
        for key in SETTINGS_KEYS {
            assert!(parsed[key].is_null(), "{key}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn locked_source_fails_after_the_bounded_attempts_with_pauses() {
        let dir = unique_test_dir("export-busy");
        let path = dir.join("live.db");
        let holder = Connection::open(&path).unwrap();
        holder.execute_batch("CREATE TABLE a(n INTEGER); INSERT INTO a VALUES (1);").unwrap();
        // An exclusive writer keeps the reader out of the database file.
        holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let reader = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        reader.busy_timeout(std::time::Duration::from_millis(0)).unwrap();
        let dest = empty_target(&dir, "out.db");
        let started = std::time::Instant::now();
        let result = snapshot_with(&reader, &dest, 4, std::time::Duration::from_millis(50));
        assert!(result.is_err());
        // Three pauses at least: no tight loop.
        assert!(started.elapsed() >= std::time::Duration::from_millis(150), "{:?}", started.elapsed());
        holder.execute_batch("ROLLBACK").unwrap();
        snapshot_with(&reader, &dest, 4, std::time::Duration::from_millis(50)).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn database_bytes_matches_page_geometry() {
        let conn = db::connect_in_memory().unwrap();
        let bytes = database_bytes(&conn).unwrap();
        assert!(bytes > 0 && bytes.is_multiple_of(512));
    }
}
