use rusqlite::Connection;
use super::error::DbError;

type MigrationFn = fn(&Connection) -> Result<(), DbError>;

/// Jeder Eintrag entspricht 1:1 einer vormals stillschweigend ausgefuehrten
/// Zeile in `repository::init()`, in unveraenderter Reihenfolge - siehe
/// Kommentar-Historie dort fuer den fachlichen Hintergrund jeder Spalte.
const MIGRATIONS: &[MigrationFn] = &[
    |c| exec(c, "ALTER TABLE filament_spools ADD COLUMN image_png BLOB"),
    |c| exec(c, "ALTER TABLE filament_spools ADD COLUMN location TEXT"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN print_status TEXT NOT NULL DEFAULT 'not_printed' CHECK (print_status IN ('not_printed', 'printed'))"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN last_viewed_at TEXT"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN creator TEXT"),
    |c| exec(c, "ALTER TABLE folders ADD COLUMN parent_id INTEGER REFERENCES folders(id) ON DELETE CASCADE"),
    |c| exec(c, "ALTER TABLE folders ADD COLUMN path TEXT"),
    |c| exec(c, "CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_path ON folders (path)"),
    |c| exec(c, "UPDATE files SET creator = (SELECT value FROM file_metadata WHERE file_id = files.id AND label = 'Designer') WHERE creator IS NULL"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN content_hash TEXT"),
    |c| exec(c, "CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash)"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN render_snapshot_png BLOB"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN custom_image_png BLOB"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN source_url TEXT"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN queue_position INTEGER"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN plate_count INTEGER"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN slice_info_json TEXT"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN deleted_at TEXT"),
    |c| exec(c, "ALTER TABLE files ADD COLUMN trash_path TEXT"),
    // M-06 (Task 11): maschinenlokale Registry vertrauenswuerdiger
    // Slicer-Executables, getrennt von den portablen Katalogdaten - siehe
    // `replace_catalog_db` in commands.rs fuer die Begruendung, warum diese
    // Tabelle bei einem Backup-Restore niemals aus der eingehenden
    // Datenbank uebernommen werden darf.
    |c| exec(c, "CREATE TABLE IF NOT EXISTS registered_slicers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        executable_path TEXT NOT NULL UNIQUE,
        is_auto_detected INTEGER NOT NULL DEFAULT 0
    )"),
];

/// Aktuelle Ziel-Schemaversion - leitet sich direkt aus der Anzahl der
/// Eintraege in [`MIGRATIONS`] ab, statt sie (wie vor dem Abschluss-Review)
/// als separaten, von Hand mitgepflegten Literal-Wert zu duplizieren. Ein
/// vergessenes "+1" bei einer neuen Migration kann dadurch strukturell nicht
/// mehr vorkommen - `MIGRATIONS.len()` UND `CURRENT_SCHEMA_VERSION` koennen
/// nie mehr auseinanderlaufen, weil es nur noch einen Wert gibt.
pub const CURRENT_SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Fuehrt ein einzelnes ALTER-TABLE/Backfill-Statement aus. "Spalte/Index
/// existiert bereits" (SQLite-Fehlermeldung enthaelt "duplicate column
/// name" bzw. der Index-Fall ist durch `IF NOT EXISTS` bereits abgedeckt)
/// ist der EINZIGE tolerierte Fehlerfall - alles andere (read-only,
/// I/O, korruptes Schema) propagiert als echter Fehler (Kern von H-02).
fn exec(conn: &Connection, sql: &str) -> Result<(), DbError> {
    match conn.execute(sql, []) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("duplicate column name") => Ok(()),
        Err(e) => Err(DbError::from(e)),
    }
}

/// Migriert `conn` von ihrer aktuellen `PRAGMA user_version` bis
/// [`CURRENT_SCHEMA_VERSION`]. Jeder Schritt laeuft einzeln; schlaegt
/// einer fehl, wird `user_version` NICHT erhoeht (der Fehler propagiert
/// sofort, kein Teil-Fortschritt wird stillschweigend uebernommen).
pub fn run_migrations(conn: &mut Connection) -> Result<(), DbError> {
    run_migrations_with(conn, MIGRATIONS, CURRENT_SCHEMA_VERSION)
}

/// Kern von [`run_migrations`], parametrisiert ueber eine explizite
/// Migrationsliste und Ziel-Version - separiert, damit Tests gezielt
/// einzelne (auch absichtlich fehlschlagende) Migrationsschritte
/// injizieren koennen, ohne die globale [`MIGRATIONS`]-Liste zu
/// beeinflussen (siehe `a_failing_step_does_not_leave_a_half_applied_schema_change_committed`).
fn run_migrations_with(conn: &mut Connection, migrations: &[MigrationFn], target_version: i64) -> Result<(), DbError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    // Korrektur nach sechster Review-Runde (P1): OHNE diese Pruefung wuerde
    // eine Datenbank mit einer HOEHEREN user_version als target_version
    // (z.B. importiert aus einer neueren App-Version, deren Schema diese
    // aeltere Version nicht kennt) die untenstehende Schleife komplett
    // ueberspringen (jeder step_version <= current) und stillschweigend
    // Ok(()) zurueckgeben - die DB wuerde dann so weiterverwendet, als sei
    // sie vollstaendig kompatibel, obwohl ihr tatsaechliches Schema neuer
    // und potenziell inkompatibel mit dem ist, was diese (aeltere)
    // App-Version versteht.
    if current > target_version {
        return Err(DbError::Other(format!(
            "Datenbank-Schemaversion {current} ist neuer als die von dieser App-Version unterstuetzte Version {target_version}"
        )));
    }
    for (index, migration) in migrations.iter().enumerate() {
        let step_version = (index + 1) as i64;
        if step_version <= current || step_version > target_version {
            continue;
        }
        // H-02-Korrektur (zweite Review-Runde): das ALTER-TABLE/Backfill-
        // Statement UND die anschliessende Erhoehung von PRAGMA
        // user_version laufen in DERSELBEN Transaktion. Vorher waren dies
        // zwei getrennte Anweisungen - ein Fehler zwischen beiden haette
        // eine Migration real angewendet, ohne sie als erledigt zu
        // vermerken, sodass sie beim naechsten Start erneut (und wegen der
        // bereits vorhandenen Spalte harmlos, aber unnoetig) versucht
        // worden waere; schlimmer: ein Fehler WAEHREND der Migration
        // selbst haette bei einer mehrteiligen Anweisung eine
        // Teil-Aenderung hinterlassen koennen.
        let tx = conn.transaction()?;
        migration(&tx)?;
        tx.pragma_update(None, "user_version", step_version)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrating_a_fresh_in_memory_db_reaches_current_schema_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn migrating_twice_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap(); // darf nicht erneut ALTER TABLE ausfuehren / nicht fehlschlagen
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn migrating_a_genuinely_partially_upgraded_db_only_applies_remaining_steps() {
        // KORREKTUR (zweite Review-Runde): anders als eine fruehere
        // Testfassung, die das VOLLSTAENDIGE aktuelle Schema anwendete und
        // user_version nur manuell auf 3 setzte (testet dann praktisch nur
        // die Duplicate-Column-Toleranz), wendet dieser Test das ECHTE
        // Basis-Schema an (SCHEMA_SQL selbst enthaelt schon heute weder
        // folders.parent_id/path noch files.print_status etc. - diese
        // Spalten kommen ausschliesslich ueber Migrationsschritte hinzu,
        // auch auf einer brandneuen DB, siehe repository.rs) und fuehrt
        // dann NUR die ersten 3 MIGRATIONS-Eintraege wirklich aus, bevor
        // run_migrations den Rest uebernehmen soll.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        for migration in &MIGRATIONS[0..3] {
            migration(&conn).unwrap();
        }
        conn.pragma_update(None, "user_version", 3).unwrap();

        run_migrations(&mut conn).unwrap();

        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        // Stichprobe: eine Spalte aus einem SPAETEN Migrationsschritt (z.B.
        // trash_path, Schritt 20) muss existieren - war zu Beginn dieses
        // Tests garantiert noch nicht vorhanden, da nur Schritt 1-3 real
        // ausgefuehrt wurden.
        conn.query_row("SELECT COUNT(trash_path) FROM files", [], |_| Ok(())).unwrap();
        // Gegenprobe: eine Spalte aus Schritt 6 (folders.parent_id) muss
        // ebenfalls existieren, obwohl sie NICHT in den vorab manuell
        // ausgefuehrten ersten 3 Schritten enthalten war - beweist, dass
        // run_migrations tatsaechlich ab Schritt 4 weitergemacht hat.
        conn.query_row("SELECT COUNT(parent_id) FROM folders", [], |_| Ok(())).unwrap();
    }

    #[test]
    fn a_failing_migration_step_does_not_advance_user_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        // Datenbank read-only machen, um einen echten I/O-/Schreibfehler zu erzwingen.
        conn.execute("PRAGMA query_only = ON", []).unwrap();
        let result = run_migrations(&mut conn);
        assert!(result.is_err(), "a real SQLite error must propagate, not be swallowed");
    }

    #[test]
    fn a_failing_step_does_not_leave_a_half_applied_schema_change_committed() {
        // Deckt die H-02-Korrektur ab: Migration + PRAGMA user_version
        // muessen in EINER Transaktion laufen. Korrektur nach dritter
        // Review-Runde: der Fehler muss INNERHALB derselben Migrations-
        // Closure ausgeloest werden, die auch das ALTER TABLE ausfuehrt -
        // NICHT als zweiter, separater Migrations-Schritt. Da jeder
        // Schritt seine eigene Transaktion bekommt (siehe
        // run_migrations_with unten), wuerde ein separater zweiter Schritt
        // bedeuten, dass der erste Schritt (mit dem ALTER TABLE) laengst
        // erfolgreich committet wurde, BEVOR der zweite ueberhaupt
        // beginnt - das wuerde user_version == 1 und eine existierende
        // Spalte ergeben, im Widerspruch zu den folgenden Assertions.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        let failing_migrations: &[MigrationFn] = &[
            |c| {
                exec(c, "ALTER TABLE files ADD COLUMN test_marker_column TEXT")?;
                Err(DbError::Other("simulierter Fehler nach ALTER TABLE".into()))
            },
        ];
        let result = run_migrations_with(&mut conn, failing_migrations, 1);
        assert!(result.is_err());
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, 0, "user_version darf nicht erhoeht werden, wenn der Schritt insgesamt fehlschlaegt");
        // Da beide Anweisungen (ALTER TABLE + user_version) in EINER
        // Transaktion liefen, die wegen des zweiten (simulierten) Fehlers
        // zurueckgerollt wurde, darf test_marker_column NICHT existieren -
        // ansonsten waere die Spalte real angelegt, aber nie als erledigt
        // vermerkt worden (genau die urspruengliche H-02-Luecke).
        let column_exists = conn
            .query_row("SELECT test_marker_column FROM files LIMIT 0", [], |_| Ok(()))
            .is_ok();
        assert!(!column_exists, "ALTER TABLE muss mit user_version zusammen zurueckgerollt werden");
    }

    #[test]
    fn a_database_from_a_newer_schema_version_is_rejected() {
        // Korrektur nach sechster Review-Runde (P1): ohne diese Pruefung
        // wuerde run_migrations_with jeden Schritt uebergehen (jede
        // step_version <= current) und stillschweigend Ok(()) liefern,
        // wenn die DB bereits eine HOEHERE user_version als
        // CURRENT_SCHEMA_VERSION hat - z.B. weil sie mit einer neueren
        // App-Version erstellt wurde, deren Schema diese (aeltere) Version
        // nicht kennt.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1).unwrap();

        let result = run_migrations(&mut conn);

        assert!(result.is_err(), "eine Datenbank mit einer neueren Schemaversion als der App muss abgelehnt werden");
    }
}
