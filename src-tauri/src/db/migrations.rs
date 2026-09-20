use rusqlite::{Connection, OptionalExtension};
use super::error::DbError;

type MigrationFn = fn(&Connection) -> Result<(), DbError>;

/// Die meisten Migrationsschritte sind ein einzelnes ALTER-TABLE/Backfill-
/// Statement, das der Runner selbst in eine Transaktion einbettet (siehe
/// `Simple`-Zweig unten). Ein CHECK-Constraint zu aendern (siehe
/// `add_stp_to_file_type_check`) erfordert dagegen einen Tabellen-Rebuild,
/// bei dem `PRAGMA foreign_keys` VOR Beginn einer Transaktion aus- und
/// danach wieder eingeschaltet werden muss - das Pragma ist laut
/// SQLite-Dokumentation ein No-Op INNERHALB einer offenen Transaktion.
/// Ein `Rebuild`-Schritt bekommt deshalb die volle `&mut Connection` und
/// verwaltet Pragma, Transaktion UND den user_version-Bump komplett selbst,
/// statt sich (wie `Simple`) eine bereits offene Transaktion vom Runner
/// injizieren zu lassen.
enum MigrationStep {
    Simple(MigrationFn),
    Rebuild(fn(&mut Connection, i64) -> Result<(), DbError>),
}

/// Jeder Eintrag entspricht 1:1 einer vormals stillschweigend ausgefuehrten
/// Zeile in `repository::init()`, in unveraenderter Reihenfolge - siehe
/// Kommentar-Historie dort fuer den fachlichen Hintergrund jeder Spalte.
const MIGRATIONS: &[MigrationStep] = &[
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN image_png BLOB")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE filament_spools ADD COLUMN location TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN print_status TEXT NOT NULL DEFAULT 'not_printed' CHECK (print_status IN ('not_printed', 'printed'))")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN last_viewed_at TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN creator TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE folders ADD COLUMN parent_id INTEGER REFERENCES folders(id) ON DELETE CASCADE")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE folders ADD COLUMN path TEXT")),
    MigrationStep::Simple(|c| exec(c, "CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_path ON folders (path)")),
    MigrationStep::Simple(|c| exec(c, "UPDATE files SET creator = (SELECT value FROM file_metadata WHERE file_id = files.id AND label = 'Designer') WHERE creator IS NULL")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN content_hash TEXT")),
    MigrationStep::Simple(|c| exec(c, "CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash)")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN render_snapshot_png BLOB")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN custom_image_png BLOB")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN source_url TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN queue_position INTEGER")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN plate_count INTEGER")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN slice_info_json TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN deleted_at TEXT")),
    MigrationStep::Simple(|c| exec(c, "ALTER TABLE files ADD COLUMN trash_path TEXT")),
    // M-06 (Task 11): maschinenlokale Registry vertrauenswuerdiger
    // Slicer-Executables, getrennt von den portablen Katalogdaten - siehe
    // `replace_catalog_db` in commands.rs fuer die Begruendung, warum diese
    // Tabelle bei einem Backup-Restore niemals aus der eingehenden
    // Datenbank uebernommen werden darf.
    MigrationStep::Simple(|c| exec(c, "CREATE TABLE IF NOT EXISTS registered_slicers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        executable_path TEXT NOT NULL UNIQUE,
        is_auto_detected INTEGER NOT NULL DEFAULT 0
    )")),
    // STP/STEP-Kataloginisierung (Variante A): CHECK-Constraint auf
    // file_type erlaubt jetzt zusaetzlich 'stp'. SQLite kennt kein ALTER
    // TABLE zum Aendern eines CHECK-Constraints - erfordert Tabellen-Rebuild,
    // siehe add_stp_to_file_type_check() unten fuer die Fremdschluessel-
    // Kaskaden-Falle, die dieser Rebuild-Schritt umgehen muss.
    MigrationStep::Rebuild(add_stp_to_file_type_check),
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

/// Erweitert den `file_type`-CHECK-Constraint auf `files` um `'stp'`.
/// SQLite kennt kein `ALTER TABLE ... ALTER/DROP CONSTRAINT` - eine
/// CHECK-Aenderung erfordert den offiziell von SQLite empfohlenen
/// "12-Schritte"-Tabellen-Rebuild (create-copy-drop-rename), siehe
/// https://www.sqlite.org/lang_altertable.html Abschnitt "Making Other
/// Kinds Of Table Schema Changes".
///
/// KRITISCH: `files` hat mehrere Kind-Tabellen mit
/// `ON DELETE CASCADE`-Fremdschluesseln (file_tags, file_metadata,
/// file_materials, collection_files, print_log). Ist `PRAGMA foreign_keys`
/// aktiv (was `repository::init` immer VOR `run_migrations` setzt), fuehrt
/// `DROP TABLE files` intern ein implizites `DELETE FROM files` aus, das
/// alle Tags/Metadaten/Materialien/Collection-Zuordnungen JEDER
/// bestehenden Datei kaskadierend loeschen wuerde - ein Rebuild ohne
/// Gegenmassnahme waere ein stiller Datenverlust-Bug. `PRAGMA
/// foreign_keys` laesst sich laut SQLite-Doku aber nur AUSSERHALB einer
/// offenen Transaktion umschalten (innerhalb ist es ein No-Op) - deshalb
/// bekommt dieser Schritt (anders als die `Simple`-Schritte oben) die
/// volle `&mut Connection` und verwaltet Pragma, Transaktion und den
/// user_version-Bump komplett selbst, statt sich eine vom Runner bereits
/// geoeffnete Transaktion injizieren zu lassen.
fn add_stp_to_file_type_check(conn: &mut Connection, step_version: i64) -> Result<(), DbError> {
    // Idempotenz-Guard: eine frische DB (SCHEMA_SQL enthaelt den neuen
    // CHECK bereits) braucht keinen Rebuild, nur den user_version-Bump
    // unten - erspart unnoetige Arbeit und ist konsistent mit dem
    // "bereits erledigt" toleranten Verhalten von exec().
    let already_migrated: bool = conn.query_row(
        "SELECT sql LIKE '%''stp''%' FROM sqlite_master WHERE type = 'table' AND name = 'files'",
        [],
        |r| r.get(0),
    )?;

    // Urspruenglichen Wert merken statt hart auf ON zurueckzusetzen: dieser
    // Schritt laeuft sowohl ueber repository::init (foreign_keys=ON) als
    // auch in Migrations-Tests, die run_migrations direkt auf einer
    // frischen Connection ohne vorheriges Pragma aufrufen (SQLite-Default:
    // OFF).
    let previously_enabled: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;

    let result = (|| -> Result<(), DbError> {
        let tx = conn.transaction()?;
        if !already_migrated {
            tx.execute_batch(
                "CREATE TABLE files_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL UNIQUE,
                    file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl', 'stp')),
                    folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
                    origin TEXT NOT NULL DEFAULT 'local'
                        CHECK (origin IN ('local')),
                    sync_status TEXT NOT NULL DEFAULT 'local-only'
                        CHECK (sync_status IN ('synced', 'outdated', 'local-only', 'cloud-only')),
                    cloud_id TEXT,
                    file_size_bytes INTEGER NOT NULL,
                    dimension_x_mm REAL,
                    dimension_y_mm REAL,
                    dimension_z_mm REAL,
                    volume_cm3 REAL,
                    object_count INTEGER,
                    thumbnail_png BLOB,
                    imported_at TEXT NOT NULL,
                    file_modified_at TEXT,
                    print_status TEXT NOT NULL DEFAULT 'not_printed'
                        CHECK (print_status IN ('not_printed', 'printed')),
                    last_viewed_at TEXT,
                    creator TEXT,
                    content_hash TEXT,
                    render_snapshot_png BLOB,
                    custom_image_png BLOB,
                    source_url TEXT,
                    queue_position INTEGER,
                    favorite INTEGER NOT NULL DEFAULT 0,
                    plate_count INTEGER,
                    slice_info_json TEXT,
                    deleted_at TEXT,
                    trash_path TEXT
                );
                INSERT INTO files_new SELECT * FROM files;
                DROP TABLE files;
                ALTER TABLE files_new RENAME TO files;
                CREATE INDEX IF NOT EXISTS idx_files_folder_id ON files (folder_id);
                CREATE INDEX IF NOT EXISTS idx_files_file_type ON files (file_type);
                CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash);",
            )?;
            // Von der SQLite-Doku fuer diesen Rebuild-Ablauf empfohlene
            // Pflichtpruefung: stellt sicher, dass keine Fremdschluessel-
            // Zeile (aus file_tags/file_metadata/file_materials/
            // collection_files/print_log) jetzt ins Leere zeigt.
            let has_violation = tx
                .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
                .optional()?
                .is_some();
            if has_violation {
                return Err(DbError::Other(
                    "STP-Migration: foreign_key_check fand verwaiste Referenzen nach dem Rebuild".into(),
                ));
            }
        }
        tx.pragma_update(None, "user_version", step_version)?;
        tx.commit()?;
        Ok(())
    })();

    // IMMER wiederherstellen, unabhaengig von Erfolg/Fehler oben - ein
    // fehlgeschlagener Rebuild darf die Verbindung nicht dauerhaft mit
    // deaktivierten Fremdschluessel-Constraints zuruecklassen.
    conn.pragma_update(None, "foreign_keys", previously_enabled)?;
    result
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
fn run_migrations_with(conn: &mut Connection, migrations: &[MigrationStep], target_version: i64) -> Result<(), DbError> {
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
        match migration {
            MigrationStep::Simple(f) => {
                // H-02-Korrektur (zweite Review-Runde): das ALTER-TABLE/
                // Backfill-Statement UND die anschliessende Erhoehung von
                // PRAGMA user_version laufen in DERSELBEN Transaktion.
                // Vorher waren dies zwei getrennte Anweisungen - ein Fehler
                // zwischen beiden haette eine Migration real angewendet,
                // ohne sie als erledigt zu vermerken, sodass sie beim
                // naechsten Start erneut (und wegen der bereits vorhandenen
                // Spalte harmlos, aber unnoetig) versucht worden waere;
                // schlimmer: ein Fehler WAEHREND der Migration selbst
                // haette bei einer mehrteiligen Anweisung eine
                // Teil-Aenderung hinterlassen koennen.
                let tx = conn.transaction()?;
                f(&tx)?;
                tx.pragma_update(None, "user_version", step_version)?;
                tx.commit()?;
            }
            // Verwaltet Pragma, Transaktion und user_version-Bump komplett
            // selbst (siehe add_stp_to_file_type_check) - der Runner darf
            // hier KEINE eigene Transaktion oeffnen, siehe deren
            // Dokumentation fuer den Grund (PRAGMA foreign_keys als No-Op
            // innerhalb einer Transaktion).
            MigrationStep::Rebuild(f) => f(conn, step_version)?,
        }
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
            match migration {
                MigrationStep::Simple(f) => f(&conn).unwrap(),
                MigrationStep::Rebuild(_) => unreachable!("erste 3 MIGRATIONS-Eintraege sind alle Simple"),
            }
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
        let failing_migrations: &[MigrationStep] = &[
            MigrationStep::Simple(|c| {
                exec(c, "ALTER TABLE files ADD COLUMN test_marker_column TEXT")?;
                Err(DbError::Other("simulierter Fehler nach ALTER TABLE".into()))
            }),
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

    /// Der wichtigste Test in dieser Datei: deckt genau die im Kommentar an
    /// `add_stp_to_file_type_check` beschriebene Kaskaden-Falle ab. Baut
    /// eine DB im ALTEN Schema (CHECK ohne 'stp', wie eine echte
    /// Bestands-DB vor diesem Upgrade), aktiviert `foreign_keys` (wie
    /// `repository::init` es tut - OHNE das waere die Falle in diesem Test
    /// gar nicht scharf), seedet Zeilen in JEDER Kind-Tabelle von `files`
    /// und prueft nach `run_migrations`, dass keine einzige davon durch den
    /// Tabellen-Rebuild kaskadierend geloescht wurde.
    #[test]
    fn migrating_preserves_all_child_rows_and_widens_the_file_type_check() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE folders (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
             CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, color_hue INTEGER NOT NULL);
             CREATE TABLE files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL,
                 path TEXT NOT NULL UNIQUE,
                 file_type TEXT NOT NULL CHECK (file_type IN ('3mf', 'stl')),
                 folder_id INTEGER REFERENCES folders (id) ON DELETE SET NULL,
                 origin TEXT NOT NULL DEFAULT 'local' CHECK (origin IN ('local')),
                 sync_status TEXT NOT NULL DEFAULT 'local-only' CHECK (sync_status IN ('synced', 'outdated', 'local-only', 'cloud-only')),
                 cloud_id TEXT,
                 file_size_bytes INTEGER NOT NULL,
                 dimension_x_mm REAL, dimension_y_mm REAL, dimension_z_mm REAL,
                 volume_cm3 REAL, object_count INTEGER, thumbnail_png BLOB,
                 imported_at TEXT NOT NULL, file_modified_at TEXT
             );
             CREATE TABLE file_tags (
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 tag_id INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
                 PRIMARY KEY (file_id, tag_id)
             );
             CREATE TABLE file_metadata (
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 label TEXT NOT NULL, value TEXT NOT NULL,
                 PRIMARY KEY (file_id, label)
             );
             CREATE TABLE file_materials (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 name TEXT NOT NULL, display_color TEXT
             );
             CREATE TABLE collections (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, created_at TEXT NOT NULL);
             CREATE TABLE collection_files (
                 collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 position INTEGER NOT NULL,
                 UNIQUE (collection_id, file_id)
             );
             CREATE TABLE print_log (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
                 printed_at TEXT NOT NULL, note TEXT, photo_png BLOB, created_at TEXT NOT NULL
             );

             INSERT INTO folders (id, name) VALUES (1, 'Root');
             INSERT INTO tags (id, name, color_hue) VALUES (1, 'Deko', 30);
             INSERT INTO files (id, name, path, file_type, folder_id, file_size_bytes, imported_at)
                 VALUES (1, 'a.3mf', '/tmp/a.3mf', '3mf', 1, 100, '2020-01-01T00:00:00Z');
             INSERT INTO files (id, name, path, file_type, folder_id, file_size_bytes, imported_at)
                 VALUES (2, 'b.stl', '/tmp/b.stl', 'stl', 1, 200, '2020-01-01T00:00:00Z');
             INSERT INTO file_tags (file_id, tag_id) VALUES (1, 1);
             INSERT INTO file_metadata (file_id, label, value) VALUES (1, 'Designer', 'Alice');
             INSERT INTO file_materials (id, file_id, name, display_color) VALUES (1, 1, 'PLA', '#ffffff');
             INSERT INTO collections (id, name, created_at) VALUES (1, 'Favoriten', '2020-01-01T00:00:00Z');
             INSERT INTO collection_files (collection_id, file_id, position) VALUES (1, 1, 0);
             INSERT INTO print_log (id, file_id, printed_at, created_at) VALUES (1, 2, '2020-01-02T00:00:00Z', '2020-01-02T00:00:00Z');",
        )
        .unwrap();
        // Wie repository::init: SCHEMA_SQL laeuft IMMER zuerst (CREATE TABLE
        // IF NOT EXISTS - legt die von den obigen ALTER-TABLE-Migrationen
        // vorausgesetzten Tabellen wie filament_spools an, ruehrt die oben
        // bewusst unvollstaendig angelegten Tabellen aber nicht an).
        conn.execute_batch(crate::db::repository::SCHEMA_SQL).unwrap();
        // Muss VOR run_migrations aktiv sein - genau wie repository::init es
        // tut - sonst wuerde dieser Test die eigentliche Kaskaden-Falle gar
        // nicht scharf schalten (DROP TABLE cascadet nur bei foreign_keys=ON).
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        run_migrations(&mut conn).unwrap();

        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);

        let fk_enabled: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert!(fk_enabled, "foreign_keys muss nach der Migration wieder eingeschaltet sein");

        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM files"), 2, "beide Dateien muessen den Rebuild ueberleben");
        assert_eq!(count("SELECT COUNT(*) FROM file_tags WHERE file_id = 1 AND tag_id = 1"), 1, "Tag-Zuordnung darf nicht kaskadierend geloescht worden sein");
        assert_eq!(count("SELECT COUNT(*) FROM file_metadata WHERE file_id = 1 AND label = 'Designer'"), 1, "Metadaten duerfen nicht verloren gehen");
        assert_eq!(count("SELECT COUNT(*) FROM file_materials WHERE file_id = 1"), 1, "Materialien duerfen nicht verloren gehen");
        assert_eq!(count("SELECT COUNT(*) FROM collection_files WHERE collection_id = 1 AND file_id = 1"), 1, "Collection-Zuordnung darf nicht verloren gehen");
        assert_eq!(count("SELECT COUNT(*) FROM print_log WHERE file_id = 2"), 1, "Druck-Log darf nicht verloren gehen");

        // id-Erhalt: file_tags/file_metadata/etc. referenzieren weiterhin
        // dieselben ids - waere ein Rebuild ohne explizite id-Spalte in der
        // Kopie erfolgt, waeren die ids der Kind-Tabellen jetzt verwaist.
        let file1_path: String = conn.query_row("SELECT path FROM files WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(file1_path, "/tmp/a.3mf");

        conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('c.stp', '/tmp/c.stp', 'stp', 1, '2020-01-01T00:00:00Z')",
            [],
        )
        .expect("file_type='stp' muss nach der Migration erlaubt sein");

        let rejected = conn.execute(
            "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('d.xyz', '/tmp/d.xyz', 'xyz', 1, '2020-01-01T00:00:00Z')",
            [],
        );
        assert!(rejected.is_err(), "ein weiterhin ungueltiger file_type muss am CHECK scheitern");

        let index_names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'files'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(index_names.contains(&"idx_files_folder_id".to_string()));
        assert!(index_names.contains(&"idx_files_file_type".to_string()));
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
