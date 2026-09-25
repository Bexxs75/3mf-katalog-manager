use super::*;

/// Gegenstueck zu `validate_folder_name` fuer `files.name`. Auch dieser Wert
/// landet unmittelbar in einem `join` - `trash_dir.join(format!("{id}-{name}"))`
/// in `delete_file`/`cleanup_missing_files` - und darf deshalb ebenfalls nur
/// eine harmlose Pfad-Komponente sein. Anders als der Ordnername kann er aber
/// nicht nur getippt, sondern auch komplett aus einem fremden Katalog-Backup
/// importiert werden (Security-Review 2026-09-19, Finding I-1); ein Name wie
/// "../../../.config/autostart/x.desktop" wuerde die Datei beim Loeschen aus
/// dem Papierkorb-Verzeichnis herausschreiben.
fn validate_file_name(name: &str) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err("Dateiname darf nicht leer sein".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("Dateiname darf keine Pfad-Trennzeichen enthalten".to_string());
    }
    if name.contains("..") {
        return Err("Dateiname darf keine \"..\"-Folge enthalten".to_string());
    }
    Ok(())
}
/// Exportiert den kompletten Katalogzustand (DB + Frontend-Settings) als
/// ZIP-Datei. Nutzt SQLite's Online-Backup-API statt eines rohen
/// Datei-Kopierens fuer die DB-Kopie: die laufende Connection kann im
/// WAL-Modus sein, ein fs::copy koennte eine inkonsistente Zwischenstufe
/// der Datei erwischen.
#[tauri::command]
pub async fn export_catalog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings_json: String,
) -> CmdResult<()> {
    use std::io::Write;

    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP-Archiv", &["zip"])
        .set_file_name(format!(
            "3mf-katalog-backup_{}.zip",
            chrono::Utc::now().format("%Y-%m-%d")
        ))
        .blocking_save_file();

    let Some(picked) = picked else {
        return Ok(());
    };
    let dest_path = picked.into_path().map_err(|e| e.to_string())?;

    let backup_db_path =
        std::env::temp_dir().join(format!("3mf-katalog-export-{}.db", std::process::id()));
    let tmp_zip_path = dest_path.with_extension("zip.tmp");

    // Backup+Zip-Schritte in eine Closure gekapselt, damit bei jedem
    // Fehlschlag (nicht nur beim Erfolgspfad) beide Temp-Artefakte
    // aufgeraeumt werden koennen, bevor der Fehler propagiert wird.
    let result: CmdResult<()> = (|| {
        {
            // Zielpfad exklusiv reservieren (schliesst die Symlink-Race aus
            // Finding 2), bevor rusqlite ihn oeffnet und befuellt - siehe
            // `write_temp_file_exclusive` fuer die ausfuehrliche Begruendung.
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup_db_path)
                .map_err(|e| e.to_string())?;
            crate::harden_permissions(&backup_db_path);

            let conn = lock_db(&state)?;
            let mut dst = Connection::open(&backup_db_path).map_err(|e| e.to_string())?;
            let backup =
                rusqlite::backup::Backup::new(&conn, &mut dst).map_err(|e| e.to_string())?;
            backup
                .run_to_completion(5, std::time::Duration::from_millis(250), None)
                .map_err(|e| e.to_string())?;
        }

        let zip_file = std::fs::File::create(&tmp_zip_path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(zip_file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("catalog.db", options).map_err(|e| e.to_string())?;
        let db_bytes = std::fs::read(&backup_db_path).map_err(|e| e.to_string())?;
        zip.write_all(&db_bytes).map_err(|e| e.to_string())?;

        zip.start_file("settings.json", options).map_err(|e| e.to_string())?;
        zip.write_all(settings_json.as_bytes()).map_err(|e| e.to_string())?;

        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();

    // Die temporaere DB-Kopie wird in jedem Fall nicht mehr gebraucht.
    let _ = std::fs::remove_file(&backup_db_path);
    if let Err(e) = result {
        // Kein unvollstaendiges .zip.tmp sichtbar neben dem Zielpfad
        // zuruecklassen, falls das Packen mittendrin fehlschlaegt.
        let _ = std::fs::remove_file(&tmp_zip_path);
        return Err(e);
    }

    // Zip erst nach vollstaendigem, erfolgreichem Schreiben an den
    // eigentlichen Zielpfad verschieben - kein unvollstaendiges Archiv am
    // sichtbaren Zielort, falls das Packen mittendrin fehlschlaegt.
    if let Err(e) = std::fs::rename(&tmp_zip_path, &dest_path) {
        // Schlaegt auch das finale Umbenennen fehl, bleibt keine
        // verwaiste .zip.tmp sichtbar neben dem Zielpfad zurueck.
        let _ = std::fs::remove_file(&tmp_zip_path);
        return Err(e.to_string());
    }
    Ok(())
}
/// Schreibt `bytes` exklusiv (`create_new` statt `fs::write`) nach `path` und
/// haertet anschliessend die Zugriffsrechte (0600 unter Unix, siehe
/// `crate::harden_permissions`). Fuer alle Katalog-Export-/Import-
/// Temp-Dateien in `std::env::temp_dir()` verwendet - siehe Security-Review
/// 2026-09-18, Finding 2: `fs::write`/`File::create` legen neue Dateien mit
/// umask-abhaengigen (typischerweise world-readable) Rechten an und folgen
/// dabei einem an dem Pfad bereits vorhandenen Symlink, statt dessen
/// Existenz abzulehnen. Im geteilten `/tmp` auf Mehrbenutzer-Systemen ist das
/// ein CWE-377-Risiko: ein anderer lokaler Nutzer koennte die (unverschluesselte)
/// Katalog-DB waehrend des kurzen Zeitfensters mitlesen, oder den PID-basierten
/// Dateinamen vorab als Symlink auf ein anderes Ziel anlegen (TOCTOU).
/// `create_new` schlaegt fehl, sobald am Zielpfad bereits etwas liegt (Datei
/// oder Symlink), statt hindurchzuschreiben.
fn write_temp_file_exclusive(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    drop(file);
    crate::harden_permissions(path);
    Ok(())
}
/// Obergrenzen fuer die aus einem Katalog-Archiv entpackten Eintraege. Ohne
/// sie kann ein wenige Kilobyte grosses ZIP beim Entpacken zu mehreren
/// Gigabyte im Speicher werden ("Zip-Bombe", Security-Review 2026-09-19,
/// Finding A-1).
const MAX_IMPORT_DB_BYTES: u64 = 256 * 1024 * 1024; // 256 MB
const MAX_IMPORT_SETTINGS_BYTES: u64 = 1024 * 1024; // 1 MB

/// Lehnt einen ZIP-Eintrag anhand seiner im Archiv deklarierten
/// entpackten Groesse ab. Die Angabe ist nicht vertrauenswuerdig (sie kann
/// luegen), deshalb wird beim Lesen zusaetzlich mit `Read::take` hart
/// begrenzt - diese Pruefung spart nur den Leseversuch bei ehrlich
/// deklarierten Riesen-Eintraegen.
fn reject_oversized_zip_entry(name: &str, size: u64, max: u64) -> CmdResult<()> {
    if size > max {
        return Err(format!(
            "Eintrag \"{name}\" im Archiv ist zu groß ({:.1} MB) - maximal {} MB erlaubt",
            size as f64 / (1024.0 * 1024.0),
            max / (1024 * 1024)
        ));
    }
    Ok(())
}
/// Prueft, dass jede in REQUIRED_COLUMNS gelistete Tabelle existiert und
/// jede dort gelistete Spalte enthaelt. `table` stammt ausschliesslich aus
/// dieser hartkodierten Konstante (kein Nutzereingabe-Pfad), daher ist die
/// String-Interpolation in der PRAGMA-Anweisung hier unproblematisch -
/// PRAGMA-Anweisungen unterstuetzen ohnehin keine gebundenen Parameter fuer
/// Tabellennamen.
const REQUIRED_COLUMNS: &[(&str, &[&str])] = &[
    ("files", &[
        "id", "name", "path", "file_type", "folder_id", "file_size_bytes",
        "imported_at", "deleted_at", "trash_path",
    ]),
    ("folders", &["id", "name", "parent_id", "path"]),
    ("tags", &["id", "name", "color_hue"]),
    ("file_tags", &["file_id", "tag_id"]),
    ("file_metadata", &["file_id", "label", "value"]),
    ("filament_spools", &["id", "material", "remaining_weight_g", "unit_id", "slot_index", "home_location", "color_hex", "kind"]),
    ("printers", &["id", "name", "position"]),
    ("material_units", &["id", "printer_id", "name", "kind", "slot_count", "bambu_ams_index", "position"]),
    ("collections", &["id", "name", "created_at"]),
    ("collection_files", &["collection_id", "file_id", "position"]),
    ("registered_slicers", &["id", "name", "executable_path", "is_auto_detected"]),
];

fn validate_expected_schema(conn: &Connection) -> Result<(), String> {
    for (table, required_columns) in REQUIRED_COLUMNS {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| e.to_string())?;
        let existing_columns: std::collections::HashSet<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        if existing_columns.is_empty() {
            return Err(format!("Katalog-Datenbank enthaelt nicht die erforderliche Tabelle '{table}'"));
        }
        for column in *required_columns {
            if !existing_columns.contains(*column) {
                return Err(format!(
                    "Katalog-Datenbank: Tabelle '{table}' fehlt die erforderliche Spalte '{column}'"
                ));
            }
        }
    }
    Ok(())
}
/// SQLite-Speicherklasse ("storage class", siehe
/// https://sqlite.org/datatype3.html), die `typeof(spalte)` fuer eine
/// bestimmte Spalte liefern darf, ausgedrueckt als der rusqlite-`FromSql`-
/// Rust-Typ, mit dem diese Spalte tatsaechlich gelesen wird - deckungsgleich
/// mit den vier praktisch vorkommenden `rusqlite::types::Type`-Varianten
/// (NULL wird separat ueber das vierte Tupel-Feld `nullable` abgedeckt, nicht
/// hier).
#[derive(Clone, Copy)]
enum ColType {
    /// `i64` ODER `bool` (rusqlite liest `bool` intern als `i64 != 0`,
    /// akzeptiert also ebenfalls NUR `typeof(...) = 'integer'`).
    Integer,
    /// `f64` - rusqlite konvertiert dafuer sowohl INTEGER als auch REAL
    /// verlustfrei, akzeptiert also BEIDE Speicherklassen.
    Real,
    /// `String` - akzeptiert NUR `typeof(...) = 'text'`.
    Text,
    /// `Vec<u8>` - akzeptiert NUR `typeof(...) = 'blob'`.
    Blob,
}

impl ColType {
    fn allowed_typeof(self, nullable: bool) -> Vec<&'static str> {
        let mut allowed: Vec<&'static str> = match self {
            ColType::Integer => vec!["integer"],
            ColType::Real => vec!["integer", "real"],
            ColType::Text => vec!["text"],
            ColType::Blob => vec!["blob"],
        };
        if nullable {
            allowed.push("null");
        }
        allowed
    }
}
/// Fuer jede Spalte, die irgendwo im Rust-Code als konkreter (nicht generisch
/// geskippter) Typ gelesen wird, deren erwartete Speicherklasse(n) und ob
/// NULL erlaubt ist - Grundlage fuer `validate_column_types`. Herleitung
/// (Review 2026-09-23): fuer jede Tabelle aus REQUIRED_COLUMNS wurde jede
/// Stelle gesucht, an der eine ihrer Spalten per `row.get`/`query_row`/
/// `query_map` gelesen wird (repository.rs, printers.rs, collections.rs -
/// db/mod.rs selbst enthaelt kein SQL), und der dortige Rust-Zieltyp notiert.
/// Spalten, die NIRGENDS als typisierter Wert gelesen werden - nur in
/// `ORDER BY`/`WHERE`/als Bind-Parameter vorkommen, z.B. `printers.position`,
/// `material_units.position`, `collections.created_at`,
/// `collection_files.position`/`collection_id`, `file_tags.tag_id`,
/// `file_metadata.file_id`, `filament_spools.created_at` - fehlen hier
/// bewusst: ein falscher Speicherklassen-Wert dort kann keinen
/// `FromSql`-Fehler ausloesen, weil nie `row.get::<_, T>` darauf angewendet
/// wird. `table`/`column` stammen ausschliesslich aus dieser hartkodierten
/// Konstante (kein Nutzereingabe-Pfad) - siehe Kommentar an REQUIRED_COLUMNS
/// zur selben Begruendung fuer die String-Interpolation in
/// `validate_column_types`.
const COLUMN_TYPES: &[(&str, &str, ColType, bool)] = &[
    // files - siehe row_to_file/list_file_summaries (repository.rs).
    ("files", "id", ColType::Integer, false),
    ("files", "name", ColType::Text, false),
    ("files", "path", ColType::Text, false),
    ("files", "file_type", ColType::Text, false),
    ("files", "folder_id", ColType::Integer, true),
    ("files", "origin", ColType::Text, false),
    ("files", "sync_status", ColType::Text, false),
    ("files", "cloud_id", ColType::Text, true),
    ("files", "file_size_bytes", ColType::Integer, false),
    ("files", "dimension_x_mm", ColType::Real, true),
    ("files", "dimension_y_mm", ColType::Real, true),
    ("files", "dimension_z_mm", ColType::Real, true),
    ("files", "volume_cm3", ColType::Real, true),
    ("files", "object_count", ColType::Integer, true),
    ("files", "thumbnail_png", ColType::Blob, true),
    ("files", "imported_at", ColType::Text, false),
    ("files", "file_modified_at", ColType::Text, true),
    ("files", "print_status", ColType::Text, false),
    ("files", "last_viewed_at", ColType::Text, true),
    ("files", "creator", ColType::Text, true),
    ("files", "content_hash", ColType::Text, true),
    ("files", "render_snapshot_png", ColType::Blob, true),
    ("files", "custom_image_png", ColType::Blob, true),
    ("files", "source_url", ColType::Text, true),
    ("files", "queue_position", ColType::Integer, true),
    ("files", "favorite", ColType::Integer, false),
    ("files", "plate_count", ColType::Integer, true),
    ("files", "slice_info_json", ColType::Text, true),
    ("files", "deleted_at", ColType::Text, true),
    ("files", "trash_path", ColType::Text, true),
    // folders - siehe list_folders (repository.rs).
    ("folders", "id", ColType::Integer, false),
    ("folders", "name", ColType::Text, false),
    ("folders", "parent_id", ColType::Integer, true),
    ("folders", "path", ColType::Text, false),
    // tags - siehe list_tag_counts/load_tags/get_or_create_tag (repository.rs).
    ("tags", "id", ColType::Integer, false),
    ("tags", "name", ColType::Text, false),
    ("tags", "color_hue", ColType::Integer, false),
    // file_tags - nur file_id wird als Wert gelesen (list_all_file_tags);
    // tag_id kommt nur in der JOIN-Bedingung vor.
    ("file_tags", "file_id", ColType::Integer, false),
    // file_metadata - siehe load_metadata (repository.rs); file_id selbst
    // wird dort nur als Bind-Parameter genutzt, nicht zurueckgelesen.
    ("file_metadata", "label", ColType::Text, false),
    ("file_metadata", "value", ColType::Text, false),
    // filament_spools - siehe list_filament_spools/get_filament_spool
    // (repository.rs) sowie load_spool/unload_spool (printers.rs).
    ("filament_spools", "id", ColType::Integer, false),
    ("filament_spools", "material", ColType::Text, false),
    ("filament_spools", "manufacturer", ColType::Text, true),
    ("filament_spools", "color", ColType::Text, true),
    ("filament_spools", "location", ColType::Text, true),
    ("filament_spools", "diameter_mm", ColType::Real, false),
    ("filament_spools", "original_weight_g", ColType::Real, false),
    ("filament_spools", "remaining_weight_g", ColType::Real, false),
    ("filament_spools", "price", ColType::Real, true),
    ("filament_spools", "image_png", ColType::Blob, true),
    ("filament_spools", "color_hex", ColType::Text, true),
    ("filament_spools", "home_location", ColType::Text, true),
    ("filament_spools", "unit_id", ColType::Integer, true),
    ("filament_spools", "slot_index", ColType::Integer, true),
    ("filament_spools", "kind", ColType::Text, false),
    // printers - siehe list_printers (printers.rs); position ist NOT NULL
    // DEFAULT 0 im Schema, wird aber nirgends als typisierter Wert gelesen
    // (nur ORDER BY), deshalb hier bewusst nicht gelistet.
    ("printers", "id", ColType::Integer, false),
    ("printers", "name", ColType::Text, false),
    // material_units - siehe list_units (printers.rs); position aus
    // demselben Grund wie bei printers.position nicht gelistet.
    ("material_units", "id", ColType::Integer, false),
    ("material_units", "printer_id", ColType::Integer, false),
    ("material_units", "name", ColType::Text, false),
    ("material_units", "kind", ColType::Text, false),
    ("material_units", "slot_count", ColType::Integer, false),
    ("material_units", "bambu_ams_index", ColType::Integer, true),
    // collections - siehe list_collections (collections.rs); created_at wird
    // nur in ORDER BY genutzt, nicht als typisierter Wert zurueckgelesen.
    ("collections", "id", ColType::Integer, false),
    ("collections", "name", ColType::Text, false),
    // collection_files - nur file_id wird als Wert gelesen
    // (list_collection_file_ids); collection_id/position kommen nur als
    // Bind-Parameter bzw. in ORDER BY vor.
    ("collection_files", "file_id", ColType::Integer, false),
    // registered_slicers - siehe list_registered_slicers/get_registered_slicer
    // (repository.rs).
    ("registered_slicers", "id", ColType::Integer, false),
    ("registered_slicers", "name", ColType::Text, false),
    ("registered_slicers", "executable_path", ColType::Text, false),
    ("registered_slicers", "is_auto_detected", ColType::Integer, false),
];
/// Prueft, dass jede in COLUMN_TYPES gelistete Spalte NUR eine ihrer
/// erlaubten SQLite-Speicherklassen enthaelt (siehe Kommentar an
/// `COLUMN_TYPES`). Ergaenzt `validate_expected_schema` (Spalten-EXISTENZ,
/// ignoriert Typen) und `validate_printer_invariants` (Wertebereiche,
/// ignoriert Speicherklassen) um die dritte, bislang fehlende Ebene: eine
/// praeparierte Datenbank kann ihre eigenen CREATE-TABLE-Statements OHNE
/// Typ-Affinitaet/CHECK-Constraints mitbringen (SQLite erzwingt Spaltentypen
/// ohnehin nur als "Affinitaet", nicht als harte Garantie) und so z.B.
/// `material_units.slot_count = 4.5` (REAL statt INTEGER) einschleusen - das
/// besteht `slot_count NOT BETWEEN 1 AND 16` anstandslos, laesst aber jedes
/// `row.get::<_, i64>(...)` beim naechsten Lesen dieser Zeile mit
/// `InvalidColumnType` scheitern. Laeuft deshalb VOR `validate_printer_invariants`,
/// damit dessen Wertepruefungen bereits auf plausiblen Speicherklassen
/// aufsetzen.
fn validate_column_types(conn: &Connection) -> Result<(), String> {
    for &(table, column, col_type, nullable) in COLUMN_TYPES {
        let allowed = col_type.allowed_typeof(nullable);
        let placeholders = allowed.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT COUNT(*) FROM \"{table}\" WHERE typeof(\"{column}\") NOT IN ({placeholders})"
        );
        let bad: i64 = conn
            .query_row(&sql, rusqlite::params_from_iter(allowed.iter().copied()), |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if bad > 0 {
            return Err(format!(
                "Katalog-Datenbank enthaelt einen falschen Datentyp in Tabelle '{table}', Spalte '{column}'"
            ));
        }
    }
    Ok(())
}
/// Prueft die Drucker/AMS-Fach-Invarianten direkt ueber den Dateninhalt
/// (siehe Kommentar am Aufrufer in `validate_catalog_db_bytes` fuer die
/// Begruendung, warum sich das nicht auf deklarierte CHECK-Constraints/
/// Fremdschluessel der importierten Datenbank verlassen kann): jeder Drucker
/// und jede Einheit muessen alle Pflichtwerte gesetzt haben, jede Einheit
/// muss einen bekannten Typ, eine Fachanzahl zwischen 1 und 16 und (falls
/// gesetzt) eine Bambu-AMS-Nummer zwischen 0 und 3 haben und einem
/// existierenden Drucker gehoeren; jede Spule mit Fach muss auf eine
/// existierende Einheit und ein innerhalb deren Fachanzahl liegendes Fach
/// zeigen, `unit_id`/`slot_index` muessen gemeinsam gesetzt oder gemeinsam
/// leer sein, kein Fach darf doppelt belegt sein, und ein gesetzter
/// Farbwert muss dem `#rrggbb`-Format entsprechen.
///
/// Nachtrag (Re-Review): jede Bedingung unten muss `IS NULL` fuer die
/// beteiligten Pflichtspalten EXPLIZIT abdecken. SQLite behandelt einen
/// Vergleich mit NULL (auch `<>`, `NOT IN`, `NOT BETWEEN`) als UNKNOWN, nicht
/// als TRUE - eine Zeile mit z.B. `kind = NULL` erfuellt `kind NOT IN (...)`
/// deshalb NICHT und wuerde ohne die `... IS NULL OR`-Zusaetze unten
/// unentdeckt durchrutschen. Eine so eingeschleuste Zeile bricht danach bei
/// jedem App-Start `db::printers::list_printers`/`list_units` (liest
/// `name`/`kind`/`slot_count`/... als nicht-optionale Rust-Typen, siehe deren
/// `PrinterRecord`/`MaterialUnitRecord`), nicht erst beim naechsten Zugriff
/// auf das Filament-Lager.
fn validate_printer_invariants(conn: &Connection) -> Result<(), String> {
    // `printers`: `db::printers::list_printers` liest `id`/`name` als
    // nicht-optionale Felder von `PrinterRecord`; `position` wird zwar nicht
    // typed ausgelesen, ist aber laut Schema `NOT NULL DEFAULT 0` und wird
    // hier aus Konsistenz mitgeprueft.
    let bad_printers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM printers WHERE id IS NULL OR name IS NULL OR position IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_printers > 0 {
        return Err("Katalog-Datenbank enthaelt Drucker mit fehlendem Pflichtwert".to_string());
    }

    // `material_units`: `db::printers::list_units` liest `id`/`printer_id`/
    // `name`/`kind`/`slot_count` als nicht-optionale Felder von
    // `MaterialUnitRecord` (nur `bambu_ams_index` ist dort `Option<i64>`).
    let bad_units: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM material_units
                 WHERE id IS NULL
                    OR printer_id IS NULL
                    OR name IS NULL
                    OR kind IS NULL
                    OR slot_count IS NULL
                    OR position IS NULL
                    OR kind NOT IN ({})
                    OR slot_count NOT BETWEEN 1 AND 16
                    OR (bambu_ams_index IS NOT NULL AND bambu_ams_index NOT BETWEEN 0 AND 3)
                    OR printer_id NOT IN (SELECT id FROM printers)",
                db::printers::UNIT_KINDS.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
            ),
            rusqlite::params_from_iter(db::printers::UNIT_KINDS.iter().copied()),
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_units > 0 {
        return Err(
            "Katalog-Datenbank enthaelt ungueltige Drucker-Einheiten (fehlender Pflichtwert, Typ, Fachanzahl, AMS-Nummer oder Drucker-Zuordnung)"
                .to_string(),
        );
    }

    // `COALESCE(..., 0)` in der `slot_index`-Bereichspruefung: faellt die
    // Unterabfrage (falsches `unit_id` ODER eine - eigentlich schon oben
    // abgelehnte - Einheit mit NULL `slot_count`) auf NULL zurueck, macht
    // `slot_index >= NULL` (UNKNOWN) die Zeile sonst unsichtbar; mit
    // `COALESCE(..., 0)` schlaegt JEDER nicht-negative `slot_index` fehl,
    // wie es fuer eine Einheit ohne (gueltige) Fachanzahl korrekt ist.
    let bad_spools: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM filament_spools
             WHERE (unit_id IS NULL) <> (slot_index IS NULL)
                OR (unit_id IS NOT NULL AND unit_id NOT IN (SELECT id FROM material_units))
                OR (unit_id IS NOT NULL AND (
                    slot_index < 0
                    OR slot_index >= COALESCE(
                        (SELECT slot_count FROM material_units m WHERE m.id = filament_spools.unit_id), 0
                    )
                ))",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_spools > 0 {
        return Err("Katalog-Datenbank enthaelt Spulen mit ungueltiger Fach-Zuordnung".to_string());
    }

    let duplicate_slots: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                SELECT unit_id, slot_index FROM filament_spools
                WHERE unit_id IS NOT NULL
                GROUP BY unit_id, slot_index HAVING COUNT(*) > 1
             )",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if duplicate_slots > 0 {
        return Err("Katalog-Datenbank enthaelt mehrere Spulen im selben Fach".to_string());
    }

    let bad_colors: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM filament_spools
             WHERE color_hex IS NOT NULL
               AND color_hex NOT GLOB '#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_colors > 0 {
        return Err("Katalog-Datenbank enthaelt ungueltige Farbwerte".to_string());
    }

    // v0.13.1: nur 'filament'/'resin', und Resin nie in einem Fach. Eine
    // praeparierte Sicherung kann die Spalte ohne CHECK mitbringen (die
    // Migration ueberspringt sie dann als "duplicate column").
    let bad_kinds: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM filament_spools
             WHERE kind IS NULL OR kind NOT IN ('filament', 'resin')
                OR (kind = 'resin' AND unit_id IS NOT NULL)",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if bad_kinds > 0 {
        return Err("Katalog-Datenbank enthaelt Eintraege mit ungueltiger Art (oder Resin in einem Fach)".to_string());
    }

    Ok(())
}
/// Prueft, ob `bytes` eine brauchbare Katalog-Datenbank sind (oeffnbar, mit
/// einer `files`-Tabelle, und ohne `folders.path`-Eintraege in geschuetzten
/// Systemverzeichnissen) - Schutz davor, ein falsches/kaputtes ODER
/// praepariertes ZIP zu importieren, BEVOR die bestehende catalog.db
/// angefasst wird. Die zweite Pruefung schliesst Finding 1 des
/// Security-Reviews vom 2026-09-18 direkt an der Vertrauensgrenze: ohne sie
/// wuerde ein bereits an dieser Stelle abgelehnter Ordner-Eintrag sonst erst
/// spaeter, beim naechsten `move`/`rename` ueber diesen Ordner, auffallen -
/// mit den unter `reject_if_sensitive_path` beschriebenen Folgen.
/// `trash_dir` ist das echte Papierkorb-Verzeichnis dieser Installation
/// (`AppState::trash_dir`); jeder gesetzte `files.trash_path` muss darin
/// liegen - siehe `reject_if_outside_trash_dir`.
fn validate_catalog_db_bytes(
    bytes: &[u8],
    sensitive_dirs: &[PathBuf],
    trash_dir: &Path,
) -> Result<(), String> {
    let tmp_path = std::env::temp_dir().join(format!(
        "3mf-katalog-import-check-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    write_temp_file_exclusive(&tmp_path, bytes).map_err(|e| e.to_string())?;

    let result = Connection::open(&tmp_path)
        .map_err(|e| e.to_string())
        .and_then(|mut conn| {
            // Einmal vorberechnen statt pro DB-Zeile - ein grosser Katalog hat
            // zehntausende `files`-Zeilen.
            let expanded_dirs = expand_sensitive_dirs(sensitive_dirs);
            let resolved_trash_dir = resolve_path_for_sensitivity_check(trash_dir)?;
            conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))
                .map_err(|e| e.to_string())?;

            // H-05: PRAGMA quick_check erkennt strukturelle SQLite-Korruption,
            // die eine reine "kann ich SELECT COUNT(*) ausfuehren"-Pruefung
            // nicht zuverlaessig aufdeckt. Laeuft VOR jeder Migration, da
            // eine strukturell korrupte Datei ohnehin nicht sinnvoll
            // migriert werden kann.
            let quick_check: String = conn
                .query_row("PRAGMA quick_check", [], |row| row.get(0))
                .map_err(|e| e.to_string())?;
            if quick_check != "ok" {
                return Err(format!("Katalog-Datenbank ist beschaedigt (quick_check: {quick_check})"));
            }

            // H-05-Korrektur (fuenfte Review-Runde, P0): diese Anwendung
            // legt in ihrem eigenen Schema (SCHEMA_SQL + migrations.rs)
            // NIEMALS Trigger oder Views an - jedes Vorkommen in einer
            // importierten Datenbank ist deshalb per Definition fremd und
            // wird abgelehnt, BEVOR irgendein weiterer Schritt (Migration,
            // FK-Check, spaeter die M-06-Slicer-Sanierung in Task 11) auf
            // dieser Datenbank ausgefuehrt wird. Ohne diese Pruefung koennte
            // ein Trigger wie "AFTER DELETE ON registered_slicers -> INSERT
            // INTO registered_slicers (...)" die Slicer-Sanierung aus
            // Task 11 unterlaufen: das dortige DELETE wuerde den Trigger
            // ausloesen, der den geloeschten Eintrag sofort wieder
            // einfuegt, waehrend COMMIT trotzdem erfolgreich durchlaeuft.
            let mut schema_stmt = conn
                .prepare("SELECT type, name FROM sqlite_schema WHERE type IN ('trigger', 'view')")
                .map_err(|e| e.to_string())?;
            let unexpected_schema_objects: Vec<(String, String)> = schema_stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            if !unexpected_schema_objects.is_empty() {
                let names: Vec<String> = unexpected_schema_objects
                    .iter()
                    .map(|(kind, name)| format!("{kind} '{name}'"))
                    .collect();
                return Err(format!(
                    "Katalog-Datenbank enthaelt nicht erlaubte Schema-Objekte: {}",
                    names.join(", ")
                ));
            }
            drop(schema_stmt);

            // H-05-Korrektur (zweite Review-Runde): die Temp-Kopie auf das
            // aktuelle Schema heben, BEVOR schema-abhaengige Pruefungen wie
            // der Ordner-Graph-Check laufen - sonst wuerde ein legitimes
            // AELTERES Backup (z.B. ohne folders.parent_id, das erst per
            // Migration hinzukommt) bereits hier faelschlich abgelehnt.
            // Dieselbe run_migrations()-Funktion aus Task 2 wird
            // wiederverwendet, keine zweite Migrationslogik.
            crate::db::run_migrations(&mut conn).map_err(|e| e.to_string())?;

            // H-05-Korrektur (sechste Review-Runde, P1): Trigger/View-
            // Ablehnung (siehe fuenfte Runde) und die bisherigen Checks
            // pruefen Datenintegritaet und aktive Manipulation, aber nicht,
            // ob das MIGRIERTE Schema ueberhaupt noch die von der
            // Anwendung tatsaechlich benoetigten Tabellen/Spalten enthaelt.
            // Eine formal gueltige SQLite-Datei koennte z.B. eine
            // Basistabelle wie `files` komplett fehlen lassen, waehrend
            // `quick_check` trotzdem "ok" meldet (strukturelle SQLite-
            // Konsistenz ist unabhaengig davon, ob die enthaltenen
            // Tabellen den von dieser App erwarteten Vertrag erfuellen).
            // Laeuft NACH run_migrations, damit auch ein aelteres,
            // legitimes Backup erst nach vollstaendiger Migration gegen
            // das jetzt aktuelle Schema geprueft wird.
            validate_expected_schema(&conn)?;

            // Finaler Review 2026-09-23, Finding 2: `validate_expected_schema`
            // prueft nur, dass jede erwartete Spalte EXISTIERT - nicht, dass
            // sie auch die richtige SQLite-Speicherklasse enthaelt. Laeuft
            // VOR `validate_printer_invariants` (naechster Schritt unten),
            // damit dessen Wertebereichs-/NULL-Pruefungen bereits auf
            // plausiblen Speicherklassen aufsetzen - siehe Kommentar an
            // `validate_column_types` fuer das konkrete Angriffsszenario.
            validate_column_types(&conn)?;

            // Finaler Review 2026-09-23, Finding 1: `validate_expected_schema`
            // prueft nur, dass Tabellen/Spalten EXISTIEREN - nicht, dass ihre
            // CHECK-Constraints/Fremdschluessel/der eindeutige Index auch
            // tatsaechlich vorhanden sind. Eine praeparierte Datenbank mit
            // bereits aktuellem `user_version` ueberspringt oben JEDE
            // Migration (`run_migrations` ist dann ein No-Op) - ein Angreifer
            // kann `material_units`/`filament_spools` also mit eigenen
            // CREATE-TABLE-Statements OHNE diese Constraints anlegen, in
            // denen weder `quick_check` noch `foreign_key_check` (das
            // weiter unten laeuft) etwas findet, weil dort schlicht keine
            // FK-Deklaration existiert, gegen die geprueft werden koennte.
            // `validate_printer_invariants` prueft die Fach-Invarianten
            // deshalb direkt ueber den Dateninhalt, unabhaengig von jedem
            // in der importierten Datenbank deklarierten Constraint - ohne
            // sie koennte z.B. `slot_count = 1000000000` das Frontend
            // (`Array.from({length: unit.slotCount})` in PrinterColumn.tsx)
            // beim Rendern haengen lassen, oder zwei Spulen im selben Fach
            // landen.
            validate_printer_invariants(&conn)?;

            // Fremdschluessel-Verletzungen (z.B. files.folder_id zeigt auf
            // eine nicht existierende folders-Zeile) werden von quick_check
            // NICHT erfasst, da SQLite Fremdschluessel standardmaessig nicht
            // erzwingt, sofern nicht explizit aktiviert. Laeuft ERST NACH
            // der Migration, damit die Pruefung gegen das vollstaendige,
            // aktuelle Schema erfolgt.
            conn.pragma_update(None, "foreign_keys", true).map_err(|e| e.to_string())?;
            let mut fk_stmt = conn.prepare("PRAGMA foreign_key_check").map_err(|e| e.to_string())?;
            let has_violation = fk_stmt.exists([]).map_err(|e| e.to_string())?;
            if has_violation {
                return Err("Katalog-Datenbank enthaelt Fremdschluessel-Verletzungen".to_string());
            }
            drop(fk_stmt);

            let mut stmt = conn
                .prepare("SELECT path FROM folders")
                .map_err(|e| e.to_string())?;
            let paths = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for path in paths {
                reject_if_sensitive_path_expanded(Path::new(&path), &expanded_dirs)
                    .map_err(|e| format!("Ordner-Eintrag im Archiv abgelehnt: {e}"))?;
            }

            // Zyklus-Check auf folders.parent_id (H-05): eine importierte
            // DB mit A.parent_id = B und B.parent_id = A wuerde jeden
            // Code, der den Ordnerbaum von einem Blatt aus zur Wurzel
            // verfolgt (z.B. Pfad-Rekonstruktion), in eine Endlosschleife
            // schicken.
            let mut folder_edges_stmt = conn
                .prepare("SELECT id, parent_id FROM folders")
                .map_err(|e| e.to_string())?;
            let edges: Vec<(i64, Option<i64>)> = folder_edges_stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let parent_of: std::collections::HashMap<i64, Option<i64>> = edges.into_iter().collect();
            for &start in parent_of.keys() {
                let mut current = start;
                let mut steps = 0usize;
                let mut seen = std::collections::HashSet::new();
                seen.insert(current);
                while let Some(Some(parent)) = parent_of.get(&current) {
                    if !seen.insert(*parent) {
                        return Err(format!("Katalog-Datenbank enthaelt einen zyklischen Ordner-Graphen (beteiligt: Ordner-ID {parent})"));
                    }
                    current = *parent;
                    steps += 1;
                    if steps > parent_of.len() {
                        break; // defensive Obergrenze, sollte durch seen bereits abgedeckt sein
                    }
                }
            }
            drop(folder_edges_stmt);

            let mut stmt = conn
                .prepare("SELECT name, path, trash_path FROM files")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for (name, path, trash_path) in rows {
                validate_file_name(&name)
                    .map_err(|e| format!("Datei-Eintrag im Archiv abgelehnt: {e}"))?;
                reject_if_sensitive_path_expanded(Path::new(&path), &expanded_dirs)
                    .map_err(|e| format!("Datei-Eintrag im Archiv abgelehnt: {e}"))?;
                if let Some(trash_path) = trash_path.filter(|p| !p.trim().is_empty()) {
                    // Denylist UND Containment: die Denylist bleibt als
                    // zweite Schranke bestehen, falls ein Pfad ueber eine
                    // Symlink-Kette trotz passender Grenze woanders landet.
                    reject_if_sensitive_path_expanded(Path::new(&trash_path), &expanded_dirs)
                        .map_err(|e| format!("Papierkorb-Eintrag im Archiv abgelehnt: {e}"))?;
                    reject_if_outside_trash_dir(Path::new(&trash_path), &resolved_trash_dir)
                        .map_err(|e| format!("Papierkorb-Eintrag im Archiv abgelehnt: {e}"))?;
                }
            }
            Ok(())
        });

    let _ = std::fs::remove_file(&tmp_path);
    result.map_err(|e| format!("Archiv enthält keine gültige Katalog-Datenbank: {e}"))
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCatalogResultDto {
    pub imported: bool,
    pub settings_json: Option<String>,
}
/// Importiert einen per `export_catalog` erzeugten Katalog-Export. Ersetzt
/// die laufende `catalog.db` NUR nach erfolgreicher Validierung (siehe
/// `validate_catalog_db_bytes`); die eigentliche Ersetzung (Connection-Swap,
/// Umbenennen der alten DB zu `.bak-<Zeitstempel>`, Kopieren der neuen DB,
/// Restore-bei-Fehler) uebernimmt `replace_catalog_db` - siehe dort fuer
/// Details. Ein Neustart der App wird dem Nutzer danach weiterhin empfohlen
/// (Frontend-Zustand/Caches sind nicht auf einen Katalogwechsel zur Laufzeit
/// ausgelegt), auch wenn das Backend ab dann bereits wieder eine echte
/// Connection auf die neue DB haelt.
#[tauri::command]
pub async fn import_catalog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportCatalogResultDto> {
    use std::io::Read;

    let picked = app.dialog().file().add_filter("ZIP-Archiv", &["zip"]).blocking_pick_file();
    let Some(picked) = picked else {
        return Ok(ImportCatalogResultDto { imported: false, settings_json: None });
    };
    let archive_path = picked.into_path().map_err(|e| e.to_string())?;

    let file = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut db_bytes = Vec::new();
    {
        let entry = archive
            .by_name("catalog.db")
            .map_err(|_| "Archiv enthält keine catalog.db".to_string())?;
        reject_oversized_zip_entry("catalog.db", entry.size(), MAX_IMPORT_DB_BYTES)?;
        entry
            .take(MAX_IMPORT_DB_BYTES)
            .read_to_end(&mut db_bytes)
            .map_err(|e| e.to_string())?;
    }

    let mut settings_bytes = Vec::new();
    {
        let entry = archive
            .by_name("settings.json")
            .map_err(|_| "Archiv enthält keine settings.json".to_string())?;
        reject_oversized_zip_entry("settings.json", entry.size(), MAX_IMPORT_SETTINGS_BYTES)?;
        entry
            .take(MAX_IMPORT_SETTINGS_BYTES)
            .read_to_end(&mut settings_bytes)
            .map_err(|e| e.to_string())?;
    }
    let settings_json = String::from_utf8(settings_bytes).map_err(|e| e.to_string())?;

    validate_catalog_db_bytes(&db_bytes, &state.sensitive_dirs, &state.trash_dir)?;

    let tmp_db_path =
        std::env::temp_dir().join(format!("3mf-katalog-import-{}.db", std::process::id()));
    write_temp_file_exclusive(&tmp_db_path, &db_bytes).map_err(|e| e.to_string())?;

    let replace_result = replace_catalog_db(&state, &tmp_db_path);
    let _ = std::fs::remove_file(&tmp_db_path);
    replace_result?;

    Ok(ImportCatalogResultDto { imported: true, settings_json: Some(settings_json) })
}
/// Ersetzt die laufende `catalog.db` durch die Datei unter `new_db_path`.
/// Herausgezogen aus `import_catalog`, damit die riskante Kernlogik
/// (Connection-Swap, Umbenennen, Kopieren, Restore-bei-Fehler) ohne
/// `AppHandle`/Datei-Dialog direkt getestet werden kann - `import_catalog`
/// selbst bleibt duenne Verdrahtung (Dialog + Zip-Entpacken + Validierung),
/// die auf diese Funktion delegiert.
///
/// Die laufende `Connection` in `state.db` wird vor dem Umbenennen durch eine
/// In-Memory-Platzhalter-Connection ersetzt - ein blosses Freigeben des
/// Mutex-Locks wuerde das zugrundeliegende Datei-Handle NICHT schliessen, was
/// auf Windows das nachfolgende `fs::rename` mit einer Sharing-Violation zum
/// Scheitern braechte.
///
/// Die alte `catalog.db` wird zu `.bak-<Zeitstempel>` umbenannt statt
/// geloescht. Schlaegt das anschliessende Kopieren der neuen DB fehl, wird
/// versucht, die Backup-Datei zurueck nach `catalog.db` umzubenennen; schlaegt
/// *dieser* Wiederherstellungsversuch ebenfalls fehl, wird eine eigene,
/// unmissverstaendliche Fehlermeldung zurueckgegeben, die auf den Pfad der
/// Backup-Datei verweist (siehe Review-Finding: die alte Version taeuschte im
/// Fehlerfall faelschlich eine erfolgreiche Wiederherstellung vor).
///
/// M-06 / P0 (vierte Review-Runde, Task 11): `registered_slicers` ist
/// maschinenlokale Vertrauensinformation, keine portablen Katalogdaten - ein
/// importiertes Backup (moeglicherweise von einer fremden oder
/// kompromittierten Maschine) darf NIEMALS ausfuehrbare Pfade in die lokale
/// Registry einschleusen, UND ein Restore darf NIEMALS bereits lokal
/// registrierte Slicer des Nutzers vernichten. Die Sanierung passiert
/// deshalb VOLLSTAENDIG auf der EINGEHENDEN Datenbank (`new_db_path`),
/// BEVOR diese ueberhaupt zur aktiven `state.db` wird - schlaegt irgendein
/// Schritt der Sanierung fehl, kehrt diese Funktion zurueck, BEVOR
/// `state.db`/die alte `catalog.db`-Datei ueberhaupt angefasst werden. Eine
/// Datenbank mit fremden Slicer-Pfaden wird so niemals zur aktiven
/// Datenbank, auch nicht transient.
fn replace_catalog_db(state: &AppState, new_db_path: &Path) -> CmdResult<()> {
    replace_catalog_db_with_copy_fn(state, new_db_path, copy_file_default)
}
/// Nicht-generischer Wrapper um `std::fs::copy`, ausschliesslich damit er
/// als konkreter `fn(&Path, &Path) -> io::Result<u64>`-Funktionszeiger an
/// `replace_catalog_db_with_copy_fn` uebergeben werden kann - `std::fs::copy`
/// selbst ist generisch ueber `AsRef<Path>` und laesst sich nicht direkt in
/// diesen konkreten, HRTB-faehigen Funktionszeigertyp coercen.
fn copy_file_default(from: &Path, to: &Path) -> std::io::Result<u64> {
    std::fs::copy(from, to)
}
/// Kern von [`replace_catalog_db`], parametrisiert ueber die Funktion, die
/// die eigentlichen Bytes von `new_db_path` nach `state.db_path` kopiert
/// (Schritt 5) - analog zu `run_migrations_with` in `migrations.rs`, das
/// aus demselben Grund ueber eine explizite Migrationsliste parametrisiert
/// ist. Ein "der `fs::copy`-Schritt schlaegt fehl, obwohl das Umbenennen
/// der alten DB bereits erfolgreich war"-Szenario laesst sich auf POSIX
/// NICHT zuverlaessig ueber chmod/Dateisystem-Tricks erzwingen, ohne
/// entweder das vorausgehende `fs::rename` (das denselben, gemeinsamen
/// Zielverzeichnis-Schreibzugriff braucht wie die anschliessende
/// Neuerstellung unter demselben Namen) gleich mit scheitern zu lassen,
/// oder root-/Namespace-Rechte fuer eine groessenbegrenzte Testumgebung zu
/// benoetigen (beides ungeeignet fuer einen portablen, deterministischen
/// Unit-Test). Diese Parametrisierung erlaubt stattdessen, den
/// Kopiervorgang selbst gezielt und deterministisch fehlschlagen zu lassen,
/// um den Wiederherstellungs-Pfad (Rueckbenennen der Backup-Datei, erneutes
/// Reconnect) unabhaengig zu testen - siehe
/// `replace_catalog_db_restores_backup_when_the_copy_step_fails_for_a_valid_sanitized_incoming_db`.
fn replace_catalog_db_with_copy_fn(
    state: &AppState,
    new_db_path: &Path,
    copy_fn: fn(&Path, &Path) -> std::io::Result<u64>,
) -> CmdResult<()> {
    // Schritt 1: lokale `registered_slicers`-Zeilen aus der AKTUELL
    // LAUFENDEN (alten) Datenbank auslesen, bevor irgendetwas an state.db
    // geaendert wird.
    let local_slicers = {
        let guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        db::list_registered_slicers(&guard).map_err(|e| e.to_string())?
    };

    // Schritte 2-4: die EINGEHENDE Datenbank (liegt bereits unter
    // new_db_path, ist aber noch NICHT aktiv) migrieren und ihre
    // registered_slicers-Tabelle sanieren, BEVOR state.db ueberhaupt
    // angefasst wird. Schlaegt einer dieser Schritte fehl, kehrt die
    // Funktion HIER mit Err zurueck - state.db und die alte catalog.db-Datei
    // bleiben dabei komplett unveraendert und aktiv, es gab noch keinen
    // Swap. Das ist der eigentliche "fail closed"-Kern dieser Korrektur:
    // eine unsanierte Datenbank wird niemals aktiv, unabhaengig davon, an
    // welcher Stelle die Sanierung scheitert.
    {
        let mut incoming = Connection::open(new_db_path).map_err(|e| e.to_string())?;
        crate::db::run_migrations(&mut incoming).map_err(|e| e.to_string())?;
        let tx = incoming.unchecked_transaction().map_err(|e| e.to_string())?;
        // Korrektur nach fuenfter Review-Runde (Defense-in-Depth, zusaetzlich
        // zur H-05-Schema-Pruefung aus Task 5): DROP TABLE statt DELETE FROM.
        // Ein DELETE allein wuerde einen an dieser Tabelle haengenden
        // boesartigen Trigger (z.B. "AFTER DELETE ON registered_slicers ->
        // INSERT INTO registered_slicers (...)") selbst AUSLOESEN und damit
        // den geloeschten fremden Eintrag sofort wieder einfuegen, waehrend
        // die Transaktion aus Sicht von rusqlite trotzdem sauber committet.
        // DROP TABLE entfernt laut SQLite-Dokumentation automatisch auch
        // alle an dieser Tabelle definierten Trigger (nicht nur die Zeilen),
        // wodurch dieser Angriffsweg strukturell ausgeschlossen ist - selbst
        // falls die H-05-Schema-Pruefung aus irgendeinem Grund uebersprungen
        // oder umgangen wuerde, ist dies eine zweite, unabhaengige Barriere
        // direkt an der Sicherheitsgrenze selbst. Schema exakt wie in
        // migrations.rs definiert.
        tx.execute("DROP TABLE IF EXISTS registered_slicers", [])
            .map_err(|e| e.to_string())?;
        tx.execute(
            "CREATE TABLE registered_slicers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                executable_path TEXT NOT NULL UNIQUE,
                is_auto_detected INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .map_err(|e| e.to_string())?;
        for slicer in &local_slicers {
            db::insert_registered_slicer(
                &tx,
                &slicer.name,
                &slicer.executable_path,
                slicer.is_auto_detected,
            )
            .map_err(|e| e.to_string())?;
        }
        // Verifikation VOR dem Commit (zusaetzliche Absicherung): die Tabelle
        // muss nach der Sanierung exakt die Anzahl der zuvor gesicherten
        // lokalen Slicer enthalten - weicht die Anzahl ab (z.B. weil ein
        // anderer, hier nicht bedachter Mechanismus zusaetzliche Zeilen
        // eingefuegt hat), bricht die Funktion lieber mit Err ab, statt eine
        // moeglicherweise unvollstaendig sanierte Tabelle zu committen.
        let final_count: i64 = tx
            .query_row("SELECT COUNT(*) FROM registered_slicers", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if final_count as usize != local_slicers.len() {
            return Err(format!(
                "Sanierung von registered_slicers ergab eine unerwartete Zeilenzahl ({final_count} statt {}) - Restore abgebrochen",
                local_slicers.len()
            ));
        }
        tx.commit().map_err(|e| e.to_string())?;
        // `incoming` droppt hier -> das Datei-Handle auf new_db_path wird
        // geschlossen, BEVOR es unten kopiert wird.
    }

    // Schritt 5: erst jetzt, nachdem new_db_path bereits vollstaendig
    // saniert auf der Festplatte liegt, wird wie bisher die aktive
    // Datenbank ausgetauscht - unveraendert gegenueber dem urspruenglichen
    // Ablauf (Platzhalter einsetzen, alte Datei sichern, neue kopieren,
    // reconnecten). Es findet HIER KEINE Slicer-Bereinigung mehr statt -
    // new_db_path ist an dieser Stelle bereits sauber.
    {
        let mut guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let placeholder = Connection::open_in_memory().map_err(|e| e.to_string())?;
        *guard = placeholder; // alte Connection droppt hier -> OS-Handle auf catalog.db wird geschlossen
    }

    let backup_path = state.db_path.with_file_name(format!(
        "catalog.db.bak-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));

    if let Err(e) = std::fs::rename(&state.db_path, &backup_path) {
        // catalog.db liegt unveraendert unter state.db_path (das rename ist
        // fehlgeschlagen, bevor irgendetwas passiert ist) - Connection muss
        // trotzdem wieder darauf zeigen statt auf dem In-Memory-Platzhalter
        // zu bleiben, sonst ist die App bis zum naechsten Neustart unbenutzbar,
        // obwohl die Datei voellig in Ordnung ist.
        if let Ok(mut guard) = state.db.lock() {
            if let Ok(conn) = db::connect(&state.db_path) {
                *guard = conn;
            }
        }
        return Err(e.to_string());
    }

    if let Err(e) = copy_fn(new_db_path, &state.db_path) {
        let restore_result = std::fs::rename(&backup_path, &state.db_path);
        if restore_result.is_ok() {
            // Alte Datei ist wieder unter state.db_path - Connection
            // reconnecten, sonst haengt die App mit dem In-Memory-Platzhalter,
            // obwohl die Datei laengst wiederhergestellt ist.
            if let Ok(mut guard) = state.db.lock() {
                if let Ok(conn) = db::connect(&state.db_path) {
                    *guard = conn;
                }
            }
        }
        return match restore_result {
            Ok(()) => Err(format!(
                "Kopieren der neuen Datenbank fehlgeschlagen, alter Katalog wiederhergestellt: {e}"
            )),
            Err(restore_err) => Err(format!(
                "Kopieren der neuen Datenbank fehlgeschlagen UND Wiederherstellung der alten \
                 Datenbank fehlgeschlagen ({restore_err}). Die vorherige Datenbank liegt noch \
                 unter {}. Bitte manuell nach {} zurückbenennen und anschließend die App neu \
                 starten. Ursprünglicher Fehler: {e}",
                backup_path.display(),
                state.db_path.display()
            )),
        };
    }

    // Neue DB liegt jetzt unter state.db_path - Connection darauf umstellen,
    // statt sie auf dem In-Memory-Platzhalter zu belassen, damit AppState
    // sofort wieder eine echte, funktionierende Verbindung haelt (und dies
    // testbar ist). Bewusst db::connect() statt einem rohen
    // Connection::open(): db::connect() ruft zusaetzlich init() auf, was
    // PRAGMA foreign_keys = ON setzt und alte Schemata per ALTER TABLE auf
    // den aktuellen Stand migriert - beides faellt bei einem rohen
    // Connection::open() weg, was bei einem Import aus einem aelteren
    // Export (mit veraltetem Schema) zu fehlenden Spalten bzw. deaktivierten
    // Fremdschluessel-Kaskaden fuehren wuerde, bis die App neu gestartet
    // wird. Ein Neustart der App bleibt trotzdem empfohlen (siehe
    // Spec/Frontend-Flow), ist fuer die Backend-Korrektheit ab hier aber
    // nicht mehr zwingend.
    let mut guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    *guard = db::connect(&state.db_path).map_err(|e| e.to_string())?;

    Ok(())
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIssuesDto {
    pub orphaned: Vec<ModelFileDto>,
    pub duplicate_groups: Vec<Vec<ModelFileDto>>,
}
/// Groups `files` by `content_hash`, excluding any file whose id is in
/// `orphaned_ids` (a file confirmed missing on disk has nothing worth
/// "keeping" - see Finding 1 of the 2026-09-10 final review: without this
/// exclusion, an orphaned file could end up as a duplicate group's
/// index-0 "keep the oldest" anchor even though it has no surviving copy
/// on disk). Only groups with 2+ remaining members are returned, each
/// sorted oldest-first by `imported_at`, and the groups themselves are
/// sorted by their first (oldest) member's `imported_at`.
fn group_duplicates(files: Vec<FileRecord>, orphaned_ids: &HashSet<i64>) -> Vec<Vec<FileRecord>> {
    let mut by_hash: BTreeMap<String, Vec<FileRecord>> = BTreeMap::new();
    for file in files {
        if orphaned_ids.contains(&file.id) {
            continue;
        }
        if let Some(hash) = file.content_hash.clone() {
            by_hash.entry(hash).or_default().push(file);
        }
    }

    let mut groups: Vec<Vec<FileRecord>> = by_hash
        .into_values()
        .filter(|group| group.len() >= 2)
        .map(|mut group| {
            group.sort_by(|a, b| a.imported_at.cmp(&b.imported_at));
            group
        })
        .collect();
    groups.sort_by(|a, b| a[0].imported_at.cmp(&b[0].imported_at));
    groups
}
#[tauri::command]
pub fn scan_catalog_issues(state: State<AppState>) -> CmdResult<CatalogIssuesDto> {
    let conn = lock_db(&state)?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;

    // Nur ein fs::metadata-Fehler vom Typ NotFound bedeutet wirklich "Datei
    // fehlt" - PermissionDenied/IO-Fehler auf einem (noch) nicht
    // eingehaengten Netzlaufwerk sollen nicht als verwaist gelten (Finding 3
    // im finalen Review vom 2026-09-10: sonst wuerden dort liegende, aber
    // gerade nicht erreichbare Dateien faelschlich zum Loeschen markiert).
    let mut orphaned_ids: HashSet<i64> = HashSet::new();
    let mut orphaned: Vec<ModelFileDto> = Vec::new();
    for file in &files {
        if let Err(e) = std::fs::metadata(&file.path) {
            if e.kind() == std::io::ErrorKind::NotFound {
                orphaned_ids.insert(file.id);
                orphaned.push(to_dto(file.clone(), &spools));
            }
        }
    }

    let duplicate_groups: Vec<Vec<ModelFileDto>> = group_duplicates(files, &orphaned_ids)
        .into_iter()
        .map(|group| group.into_iter().map(|f| to_dto(f, &spools)).collect())
        .collect();

    Ok(CatalogIssuesDto { orphaned, duplicate_groups })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Minimaler `NewFile` fuer den Fehlerfall-Test oben - nur Pfad/Typ sind
    /// relevant, alle anderen Felder sind fuer `rescan_file` irrelevant, da die
    /// Funktion bei fehlender Datei abbricht, bevor sie sie liest.
    fn sample_new_file_for_rescan_test(path: &std::path::Path) -> NewFile {
        NewFile {
            name: "missing.3mf".to_string(),
            path: path.to_string_lossy().to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 0,
            dimensions_mm: None,
            volume_cm3: None,
            object_count: None,
            thumbnail_png: None,
            imported_at: "2026-09-13T00:00:00Z".to_string(),
            file_modified_at: None,
            materials: Vec::new(),
            metadata: BTreeMap::new(),
            tags: Vec::new(),
            print_status: "not_printed".to_string(),
            last_viewed_at: None,
            creator: None,
            content_hash: None,
            render_snapshot_png: None,
            custom_image_png: None,
            source_url: None,
            queue_position: None,
            favorite: false,
            plate_count: None,
            slice_info_json: None,
        }
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_a_real_sqlite_database_with_files_table() {
        let tmp_path = std::env::temp_dir().join(format!(
            "validate_catalog_db_test_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        {
            let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            drop(conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");

        let result = validate_catalog_db_bytes(&bytes, &[], &std::env::temp_dir());

        let _ = std::fs::remove_file(&tmp_path);
        assert!(result.is_ok(), "expected valid catalog db to pass validation: {result:?}");
    }
    /// Baut wie der Test oben eine frisch migrierte Katalog-DB, ruft davor
    /// aber `setup` mit der offenen Verbindung auf, damit Aufrufer noch
    /// zusaetzliche Zeilen einfuegen koennen. Ergebnis passt direkt als
    /// (bytes, sensitive_dirs, trash_dir) fuer `validate_catalog_db_bytes`.
    fn backup_test_db_with(setup: impl FnOnce(&Connection)) -> (Vec<u8>, Vec<PathBuf>, PathBuf) {
        let tmp_path = std::env::temp_dir().join(format!(
            "backup_test_db_with_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        {
            let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            setup(&conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");
        let _ = std::fs::remove_file(&tmp_path);
        (bytes, Vec::new(), std::env::temp_dir())
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_fractional_spool_weights() {
        // Wie validate_catalog_db_bytes_accepts_a_real_sqlite_database_with_files_table,
        // zusaetzlich eine Spule mit 612.4 g Restgewicht.
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES ('PLA', 1.75, 1000.0, 612.4, '2026-09-24')",
                [],
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash).is_ok());
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_garbage_bytes() {
        let result = validate_catalog_db_bytes(b"this is not a sqlite database", &[], &std::env::temp_dir());
        assert!(result.is_err());
    }
    fn unique_test_db_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{name}_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ))
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_corrupted_sqlite_file() {
        let bytes = b"SQLite format 3\x00this-is-not-actually-a-valid-database-body".to_vec();
        let sensitive_dirs = vec![];
        let trash_dir = unique_test_dir("validate_db_corrupt_trash");
        let result = validate_catalog_db_bytes(&bytes, &sensitive_dirs, &trash_dir);
        assert!(result.is_err());
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_foreign_key_violation() {
        let tmp_path = unique_test_db_path("validate_db_fk_violation");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            // Datei mit folder_id, die auf keine existierende Zeile in folders zeigt.
            conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
            conn.execute(
                "INSERT INTO files (name, path, file_type, folder_id, file_size_bytes, imported_at) VALUES ('x', '/tmp/x.3mf', '3mf', 999999, 1, '2026-01-01T00:00:00Z')",
                [],
            ).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err());
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_cyclic_folder_graph() {
        let tmp_path = unique_test_db_path("validate_db_folder_cycle");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute("INSERT INTO folders (name, path) VALUES ('A', '/tmp/A')", []).unwrap();
            let a_id = conn.last_insert_rowid();
            conn.execute("INSERT INTO folders (name, path) VALUES ('B', '/tmp/B')", []).unwrap();
            let b_id = conn.last_insert_rowid();
            conn.execute("UPDATE folders SET parent_id = ?1 WHERE id = ?2", params![b_id, a_id]).unwrap();
            conn.execute("UPDATE folders SET parent_id = ?1 WHERE id = ?2", params![a_id, b_id]).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err());
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_a_legitimate_pre_migration_backup() {
        // KORREKTUR (zweite Review-Runde, P1): dieser Test war zuvor ein
        // leerer Platzhalter. Er deckt jetzt genau das im Review genannte
        // Szenario ab: ein AELTERES Backup, dessen Schema NUR die Basis-
        // CREATE-TABLE-Definitionen enthaelt (kein folders.parent_id, kein
        // folders.path, keine der 20 Migrationsspalten) - muss durch die in
        // Step 3 ergaenzte run_migrations()-Vorabmigration trotzdem akzeptiert
        // werden, statt am direkten Zugriff auf eine noch fehlende Spalte zu
        // scheitern.
        let tmp_path = unique_test_db_path("validate_db_pre_migration_backup");
        {
            let conn = rusqlite::Connection::open(&tmp_path).unwrap();
            conn.execute_batch(crate::db::SCHEMA_SQL).unwrap();
            // Bewusst KEIN run_migrations() hier - simuliert exakt den
            // Zustand einer vor der parent_id/path-Migration exportierten DB.
            conn.execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at) VALUES ('x', '/tmp/x.3mf', '3mf', 1, '2026-01-01T00:00:00Z')",
                [],
            ).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();

        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));

        assert!(result.is_ok(), "a legitimate pre-migration backup must be accepted after internal migration, got: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_database_with_an_injected_trigger() {
        let tmp_path = unique_test_db_path("validate_db_injected_trigger");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute_batch(
                "CREATE TRIGGER evil AFTER DELETE ON folders
                 BEGIN
                     INSERT INTO folders (name, path) VALUES ('Injected', '/tmp/injected-by-trigger');
                 END;",
            ).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine importierte Datenbank mit einem eingeschleusten Trigger muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_database_with_an_injected_view() {
        let tmp_path = unique_test_db_path("validate_db_injected_view");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute_batch("CREATE VIEW evil_view AS SELECT * FROM folders;").unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine importierte Datenbank mit einer eingeschleusten View muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_database_missing_a_required_table() {
        // Korrektur nach siebter Review-Runde: geht bewusst von einer
        // VOLLSTAENDIG GUELTIGEN, aktuell migrierten Datenbank aus und baut
        // GEZIELT genau EINEN Defekt ein. Eine handgestrickte Minimal-DB (wie
        // in der sechsten Runde) kann bereits VOR validate_expected_schema()
        // an einer ganz anderen Stelle scheitern - dann waere `result.is_err()`
        // gruen, OHNE dass validate_expected_schema() ueberhaupt ausgefuehrt
        // wurde.
        //
        // Korrektur nach ACHTER Review-Runde (derselbe Fehler nochmal, eine
        // Ebene tiefer): `files` ist fuer dieses Beispiel die FALSCHE Tabelle,
        // weil `validate_catalog_db_bytes` bereits VOR `validate_expected_schema`
        // einen bestehenden `SELECT COUNT(*) FROM files`-Check besitzt (siehe
        // Kommentar am Anfang dieser Funktion, "H-05: PRAGMA quick_check..." -
        // der COUNT-Check steht noch davor) - eine fehlende `files`-Tabelle
        // wuerde also bereits DORT mit `Err` abbrechen, nicht erst in
        // `validate_expected_schema`, und der Test wuerde wieder aus dem
        // falschen Grund gruen. Stattdessen `tags` droppen: diese Tabelle wird
        // an keiner Stelle VOR `validate_expected_schema` abgefragt, ist aber
        // Teil von `REQUIRED_COLUMNS` - nur `validate_expected_schema` selbst
        // kann diesen Fehler also erkennen. Da `crate::db::connect()` hier
        // bereits auf `CURRENT_SCHEMA_VERSION` migriert (user_version =
        // Ziel-Version), ueberspringt `run_migrations()` innerhalb von
        // `validate_catalog_db_bytes()` zusaetzlich jeden Schritt (current ==
        // target) und wird von der fehlenden `tags`-Tabelle nicht beruehrt.
        let tmp_path = unique_test_db_path("validate_db_missing_table");
        {
            let conn = crate::db::connect(&tmp_path).unwrap(); // vollstaendiges, aktuell migriertes Schema
            conn.execute_batch("DROP TABLE tags;").unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let err = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash")).unwrap_err();
        assert!(
            err.contains("tags"),
            "validate_expected_schema muss die fehlende Tabelle 'tags' erkennen, got: {err}"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_database_missing_a_required_column() {
        // Gleiches Prinzip wie oben: vollstaendig gueltige, migrierte
        // Datenbank, dann GEZIELT genau eine erforderliche Spalte per
        // `ALTER TABLE ... DROP COLUMN` entfernen (rusqlite 0.40 mit
        // `bundled`-Feature enthaelt eine SQLite-Version >= 3.35, die das
        // unterstuetzt). run_migrations() innerhalb von
        // validate_catalog_db_bytes() ist wieder ein No-Op (user_version
        // bereits aktuell), sodass ausschliesslich validate_expected_schema()
        // fuer die Ablehnung verantwortlich sein kann.
        let tmp_path = unique_test_db_path("validate_db_missing_column");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            // Bewusst 'imported_at' statt z.B. 'path' - SQLites ALTER TABLE
            // DROP COLUMN verweigert das Entfernen einer Spalte, die Teil
            // eines UNIQUE-/PRIMARY-KEY-/FOREIGN-KEY-Constraints oder eines
            // Index ist (path ist UNIQUE, folder_id/file_type haben eigene
            // Indizes) - 'imported_at' hat ausser NOT NULL keine solche
            // Einschraenkung und laesst sich deshalb sauber entfernen.
            conn.execute_batch("ALTER TABLE files DROP COLUMN imported_at;").unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine 'files'-Tabelle ohne die erforderliche Spalte 'imported_at' muss abgelehnt werden");
    }
    /// Gemeinsame Grundlage fuer die `validate_printer_invariants`-Tests
    /// unten: eine vollstaendig migrierte DB mit genau einem Drucker und
    /// genau einer Einheit ("AMS A", 4 Faecher), Rueckgabe von
    /// (printer_id, unit_id). Ungueltige Zeilen schreiben die einzelnen
    /// Tests danach selbst per Roh-SQL hinein - `crate::db::printers`s
    /// eigene API wuerde diese Werte ja bereits selbst ablehnen.
    fn printer_with_one_ams_unit(conn: &Connection) -> (i64, i64) {
        let printer_id = crate::db::printers::insert_printer(conn, "X1C").unwrap();
        let unit_id = crate::db::printers::insert_unit(conn, printer_id, "bambu_ams", "AMS A", None).unwrap();
        (printer_id, unit_id)
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_an_unknown_kind() {
        let tmp_path = unique_test_db_path("validate_db_unit_bad_kind");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (printer_id, _) = printer_with_one_ams_unit(&conn);
            conn.execute("PRAGMA ignore_check_constraints = ON", []).unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Toaster', 'toaster', 4, NULL, 1)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Einheit mit unbekanntem 'kind' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_slot_count_out_of_range() {
        let tmp_path = unique_test_db_path("validate_db_unit_bad_slot_count");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (printer_id, _) = printer_with_one_ams_unit(&conn);
            conn.execute("PRAGMA ignore_check_constraints = ON", []).unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Riesig', 'custom', 1000000000, NULL, 1)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(
            result.is_err(),
            "eine Einheit mit riesiger 'slot_count' muss abgelehnt werden (sonst haengt PrinterColumn.tsx beim Rendern)"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_bambu_ams_index_out_of_range() {
        let tmp_path = unique_test_db_path("validate_db_unit_bad_ams_index");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (printer_id, _) = printer_with_one_ams_unit(&conn);
            conn.execute("PRAGMA ignore_check_constraints = ON", []).unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'AMS X', 'bambu_ams', 4, 99, 1)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Einheit mit 'bambu_ams_index' ausserhalb 0..=3 muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_dangling_printer_id() {
        let tmp_path = unique_test_db_path("validate_db_unit_dangling_printer");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (999999, 'Verwaist', 'custom', 4, NULL, 0)",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Einheit mit nicht existierendem 'printer_id' muss abgelehnt werden");
    }
    /// Baut `material_units` OHNE NOT NULL/CHECK-Constraints neu auf - eine
    /// regulaer (per `crate::db::connect`) angelegte Tabelle wuerde ein
    /// `INSERT ... VALUES (NULL, ...)` fuer eine NOT-NULL-Spalte bereits
    /// selbst ablehnen, BEVOR `validate_printer_invariants` ueberhaupt zum
    /// Zug kommt. Das bildet exakt das Angriffsszenario aus Finding 1 nach:
    /// eine importierte DB mit eigenem, restriktionslosem `CREATE TABLE`.
    fn drop_material_units_constraints(conn: &Connection) {
        conn.execute_batch(
            "DROP TABLE material_units;
             CREATE TABLE material_units (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                printer_id INTEGER,
                name TEXT,
                kind TEXT,
                slot_count INTEGER,
                bambu_ams_index INTEGER,
                position INTEGER
             );",
        )
        .unwrap();
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_kind() {
        // Re-Review-Nachtrag: `kind NOT IN (...)` ist bei `kind = NULL` in
        // SQLite UNKNOWN (nicht TRUE) - ohne eine explizite `kind IS NULL`-
        // Bedingung wuerde diese Zeile durchrutschen und danach bei jedem
        // App-Start `db::printers::list_units` (liest `kind` als
        // nicht-optionales `String`) zum Absturz bringen.
        let tmp_path = unique_test_db_path("validate_db_unit_null_kind");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
            drop_material_units_constraints(&conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', NULL, 4, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Einheit mit NULL 'kind' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_slot_count() {
        let tmp_path = unique_test_db_path("validate_db_unit_null_slot_count");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
            drop_material_units_constraints(&conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', NULL, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Einheit mit NULL 'slot_count' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_null_printer_id() {
        let tmp_path = unique_test_db_path("validate_db_unit_null_printer_id");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            drop_material_units_constraints(&conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (NULL, 'Kaputt', 'custom', 4, NULL, 0)",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Einheit mit NULL 'printer_id' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_printer_with_a_null_name() {
        let tmp_path = unique_test_db_path("validate_db_printer_null_name");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute_batch(
                "DROP TABLE printers;
                 CREATE TABLE printers (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT,
                    position INTEGER
                 );",
            )
            .unwrap();
            conn.execute("INSERT INTO printers (name, position) VALUES (NULL, 0)", []).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "ein Drucker mit NULL 'name' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_spool_in_a_slot_of_a_unit_with_a_null_slot_count() {
        // Re-Review-Nachtrag: deckt zusaetzlich die `COALESCE`-Absicherung
        // in der `bad_spools`-Abfrage ab, unabhaengig davon, dass diese Zeile
        // schon vorher durch `bad_units` abgelehnt wird.
        let tmp_path = unique_test_db_path("validate_db_spool_slot_in_null_slot_count_unit");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
            drop_material_units_constraints(&conn);
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', NULL, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
            let unit_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 999)",
                params![unit_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Spule mit slot_index 999 in einer Einheit ohne gueltige Fachanzahl muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_spool_with_slot_index_set_but_no_unit() {
        let tmp_path = unique_test_db_path("validate_db_spool_half_placement");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', NULL, 0)",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(
            result.is_err(),
            "eine Spule mit gesetztem 'slot_index' aber ohne 'unit_id' muss abgelehnt werden"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_spool_with_a_dangling_unit_id() {
        let tmp_path = unique_test_db_path("validate_db_spool_dangling_unit");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', 999999, 0)",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Spule mit nicht existierender 'unit_id' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_spool_with_slot_index_outside_the_units_range() {
        let tmp_path = unique_test_db_path("validate_db_spool_slot_out_of_range");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (_, unit_id) = printer_with_one_ams_unit(&conn); // 4 Faecher, gueltig sind 0..=3
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 4)",
                params![unit_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Spule mit 'slot_index' >= der Fachanzahl ihrer Einheit muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_two_spools_in_the_same_slot() {
        let tmp_path = unique_test_db_path("validate_db_duplicate_slot");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (_, unit_id) = printer_with_one_ams_unit(&conn);
            // idx_filament_spools_slot existiert in einer regulaer migrierten
            // DB bereits und wuerde das zweite INSERT selbst verhindern -
            // fuer diesen Test wird er deshalb bewusst entfernt, um exakt
            // das im Finding beschriebene Szenario (Index fehlt in einer
            // praeparierten DB) nachzustellen.
            conn.execute("DROP INDEX idx_filament_spools_slot", []).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 0)",
                params![unit_id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, unit_id, slot_index)
                 VALUES ('PETG', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', ?1, 0)",
                params![unit_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "zwei Spulen im selben Fach muessen abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_an_invalid_color_hex() {
        let tmp_path = unique_test_db_path("validate_db_bad_color_hex");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, color_hex)
                 VALUES ('PLA', 1.75, 1000, 1000, '2026-01-01T00:00:00Z', 'rot')",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "ein 'color_hex' ausserhalb des #rrggbb-Formats muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_a_valid_backup_with_loaded_spools() {
        let tmp_path = unique_test_db_path("validate_db_valid_with_loaded_spools");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let (_, unit_id) = printer_with_one_ams_unit(&conn);
            let spool_id = crate::db::insert_filament_spool(
                &conn,
                &crate::db::models::NewFilamentSpool {
                    material: "PLA".to_string(),
                    manufacturer: None,
                    color: None,
                    location: Some("Regal 2".to_string()),
                    diameter_mm: 1.75,
                    original_weight_g: 1000.0,
                    remaining_weight_g: 800.0,
                    price: None,
                    image_png: None,
                    color_hex: Some("#1a1a1a".to_string()),
                    kind: "filament".to_string(),
                },
            )
            .unwrap();
            crate::db::printers::load_spool(&conn, spool_id, unit_id, 0).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_ok(), "ein gueltiges Backup mit belegten Faechern muss akzeptiert werden: {result:?}");
    }
    // ---- Datentyp-Pruefung (Finding 2, finaler Review 2026-09-23) ----
    //
    // Die obigen `validate_printer_invariants`-Tests pruefen WERTE (Bereiche,
    // NULL) - sie decken NICHT ab, dass eine praeparierte Datenbank ihre
    // eigenen CREATE-TABLE-Statements ohne Typ-Affinitaet mitbringen und so
    // eine falsche SPEICHERKLASSE (SQLite: 'integer'/'real'/'text'/'blob'/
    // 'null', siehe https://sqlite.org/datatype3.html) in einer Spalte
    // hinterlassen kann, die WERTEMAESSIG trotzdem gueltig erscheint (z.B.
    // `slot_count = 4.5`, das `BETWEEN 1 AND 16` anstandslos besteht). Genau
    // das lassen anschliessend rusqlite's `FromSql`-Impls beim naechsten
    // Lesen scheitern (`i64`/`bool` akzeptieren nur INTEGER, `String` nur
    // TEXT, `Vec<u8>` nur BLOB, `f64` INTEGER oder REAL, `Option<T>`
    // zusaetzlich NULL) - jeder betroffene `list_*`-Befehl bricht dann bei
    // JEDEM App-Start ab.
    #[test]
    fn validate_catalog_db_bytes_rejects_a_material_unit_with_a_real_slot_count() {
        // INTEGER-Affinitaet konvertiert einen REAL-Wert nur dann verlustfrei
        // zu INTEGER, wenn er keine Nachkommastelle hat - 4.5 bleibt deshalb
        // trotz deklarierter INTEGER-Spalte als REAL-Speicherklasse erhalten
        // und besteht `slot_count NOT BETWEEN 1 AND 16` anstandslos (4.5 liegt
        // "zwischen" 1 und 16), obwohl `MaterialUnitRecord.slot_count` in Rust
        // ein `i64` ist.
        let tmp_path = unique_test_db_path("validate_db_unit_real_slot_count");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            let printer_id = crate::db::printers::insert_printer(&conn, "X1C").unwrap();
            conn.execute(
                "INSERT INTO material_units (printer_id, name, kind, slot_count, bambu_ams_index, position)
                 VALUES (?1, 'Kaputt', 'custom', 4.5, NULL, 0)",
                params![printer_id],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Einheit mit 'slot_count' als REAL-Speicherklasse muss abgelehnt werden");
    }
    /// Baut `printers` OHNE deklarierten Spaltentyp fuer `name` neu auf - ein
    /// Spaltenname ganz ohne Typangabe bekommt in SQLite BLOB-Affinitaet
    /// ("keine Affinitaet", siehe https://sqlite.org/datatype3.html#determination_of_column_affinity),
    /// wodurch ein eingefuegter INTEGER-Wert NICHT nach TEXT konvertiert wird,
    /// anders als bei der regulaeren, per `crate::db::connect` angelegten
    /// Tabelle (dort haette TEXT-Affinitaet den Wert automatisch in Text
    /// umgewandelt).
    fn drop_printers_constraints(conn: &Connection) {
        conn.execute_batch(
            "DROP TABLE printers;
             CREATE TABLE printers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name,
                position INTEGER
             );",
        )
        .unwrap();
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_printer_with_an_integer_name() {
        let tmp_path = unique_test_db_path("validate_db_printer_integer_name");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            drop_printers_constraints(&conn);
            conn.execute("INSERT INTO printers (name, position) VALUES (12345, 0)", []).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "ein Drucker mit 'name' als INTEGER-Speicherklasse muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_filament_spool_with_remaining_weight_g_stored_as_text() {
        // Kein Constraint-Umbau noetig: INTEGER-Affinitaet konvertiert einen
        // TEXT-Wert nur, wenn er wie eine Zahl AUSSIEHT - 'viel' bleibt TEXT.
        let tmp_path = unique_test_db_path("validate_db_spool_text_remaining_weight");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES ('PLA', 1.75, 1000, 'viel', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Spule mit 'remaining_weight_g' als TEXT-Speicherklasse muss abgelehnt werden");
    }
    /// Baut `filament_spools` OHNE NOT-NULL-Constraints neu auf - siehe
    /// `drop_material_units_constraints` weiter oben fuer die Begruendung des
    /// Musters (eine regulaer angelegte Tabelle wuerde `material = NULL`
    /// bereits selbst am INSERT ablehnen).
    fn drop_filament_spools_constraints(conn: &Connection) {
        conn.execute_batch(
            "DROP TABLE filament_spools;
             CREATE TABLE filament_spools (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                material TEXT, manufacturer TEXT, color TEXT, location TEXT,
                diameter_mm REAL, original_weight_g INTEGER, remaining_weight_g INTEGER,
                price REAL, image_png BLOB, created_at TEXT,
                unit_id INTEGER, slot_index INTEGER, home_location TEXT, color_hex TEXT
             );",
        )
        .unwrap();
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_filament_spool_with_a_null_material() {
        // `FilamentSpoolRecord.material` ist in Rust ein nicht-optionales
        // `String`-Feld - anders als bei `material_units.kind` (siehe
        // `validate_printer_invariants`) gibt es dafuer bislang KEINE eigene
        // NULL-Pruefung ausserhalb der neuen Typpruefung.
        let tmp_path = unique_test_db_path("validate_db_spool_null_material");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            drop_filament_spools_constraints(&conn);
            conn.execute(
                "INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                 VALUES (NULL, 1.75, 1000, 1000, '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "eine Spule mit NULL 'material' muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_a_folder_with_name_stored_as_blob() {
        // TEXT-Affinitaet konvertiert nur NUMERISCHE Werte zu Text - ein
        // BLOB-Literal bleibt trotz TEXT-Spalte als BLOB-Speicherklasse
        // erhalten (https://sqlite.org/datatype3.html), kein Constraint-Umbau
        // noetig. Bewusst `folders.name` statt `files.name`: Letzteres wird
        // bereits VOR dieser neuen Pruefung ueber `validate_file_name`
        // gelesen (siehe die Datei-Schleife weiter unten) und wuerde deshalb
        // auch ohne `validate_column_types` ablehnen - `folders.name` wird
        // innerhalb von `validate_catalog_db_bytes` an keiner anderen Stelle
        // gelesen (nur `folders.path`/`parent_id`) und deckt die neue
        // Pruefung deshalb wirklich isoliert ab.
        let tmp_path = unique_test_db_path("validate_db_folder_blob_name");
        {
            let conn = crate::db::connect(&tmp_path).unwrap();
            conn.execute(
                "INSERT INTO folders (name, path) VALUES (x'00010203', '/tmp/blob-name')",
                [],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(result.is_err(), "ein Ordner mit 'name' als BLOB-Speicherklasse muss abgelehnt werden");
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_a_full_catalog_with_data_in_every_table() {
        // Regressionsnetz fuer die neue Typpruefung: ein normal aus der App
        // heraus befuellter Katalog - mit Daten in JEDER Tabelle aus
        // REQUIRED_COLUMNS, inklusive belegter Spule, Bild-BLOBs, Tags,
        // Metadaten, Sammlung und registriertem Slicer - darf dadurch nicht
        // faelschlich abgelehnt werden.
        let tmp_path = unique_test_db_path("validate_db_full_catalog");
        {
            let mut conn = crate::db::connect(&tmp_path).unwrap();

            let folder_id =
                crate::db::insert_folder_with_parent(&conn, "Projekte", None, "/tmp/Projekte").unwrap();

            let mut metadata = BTreeMap::new();
            metadata.insert("Slicer".to_string(), "Bambu Studio".to_string());
            let new_file = NewFile {
                name: "Gehaeuse.3mf".to_string(),
                path: "/tmp/Projekte/Gehaeuse.3mf".to_string(),
                file_type: FileType::ThreeMf,
                folder_id: Some(folder_id),
                origin: "local".to_string(),
                cloud_id: None,
                sync_status: "local-only".to_string(),
                file_size_bytes: 12345,
                dimensions_mm: Some([10.0, 20.0, 30.0]),
                volume_cm3: Some(123.4),
                object_count: Some(3),
                thumbnail_png: Some(vec![1, 2, 3]),
                imported_at: "2026-09-23T00:00:00Z".to_string(),
                file_modified_at: Some("2026-09-23T00:00:00Z".to_string()),
                materials: vec![MaterialRecord { name: "PLA".to_string(), display_color: Some("#ff0000".to_string()) }],
                metadata,
                tags: vec!["Prototyp".to_string()],
                print_status: "printed".to_string(),
                last_viewed_at: Some("2026-09-23T00:00:00Z".to_string()),
                creator: Some("Max".to_string()),
                content_hash: Some("abc123".to_string()),
                render_snapshot_png: Some(vec![4, 5, 6]),
                custom_image_png: Some(vec![7, 8, 9]),
                source_url: Some("https://example.com/model".to_string()),
                queue_position: Some(1),
                favorite: true,
                plate_count: Some(2),
                slice_info_json: Some("{}".to_string()),
            };
            let file_id = crate::db::insert_file(&mut conn, &new_file).unwrap();

            let created_at = chrono::Utc::now().to_rfc3339();
            let collection_id = crate::db::create_collection(&conn, "Sammlung", &created_at).unwrap();
            crate::db::add_file_to_collection(&conn, collection_id, file_id, 0).unwrap();

            let slicer_executable = std::env::current_exe().unwrap().to_string_lossy().to_string();
            crate::db::insert_registered_slicer(&conn, "Bambu Studio", &slicer_executable, true).unwrap();

            let (_, unit_id) = printer_with_one_ams_unit(&conn);
            let spool_id = crate::db::insert_filament_spool(
                &conn,
                &crate::db::models::NewFilamentSpool {
                    material: "PLA".to_string(),
                    manufacturer: Some("Bambu".to_string()),
                    color: Some("Schwarz".to_string()),
                    location: Some("Regal 2".to_string()),
                    diameter_mm: 1.75,
                    original_weight_g: 1000.0,
                    remaining_weight_g: 800.0,
                    price: Some(19.99),
                    image_png: Some(vec![9, 9, 9]),
                    color_hex: Some("#1a1a1a".to_string()),
                    kind: "filament".to_string(),
                },
            )
            .unwrap();
            crate::db::printers::load_spool(&conn, spool_id, unit_id, 0).unwrap();
        }
        let bytes = std::fs::read(&tmp_path).unwrap();
        let result = validate_catalog_db_bytes(&bytes, &[], &unique_test_dir("trash"));
        assert!(
            result.is_ok(),
            "ein regulaer befuellter Katalog mit Daten in jeder Tabelle muss weiterhin akzeptiert werden: {result:?}"
        );
    }
    #[test]
    fn column_types_covers_every_table_in_required_columns() {
        for (table, _) in REQUIRED_COLUMNS {
            assert!(
                COLUMN_TYPES.iter().any(|(t, _, _, _)| t == table),
                "COLUMN_TYPES hat keinen Eintrag fuer Tabelle '{table}' aus REQUIRED_COLUMNS - neue Tabellen muessen hier ergaenzt werden"
            );
        }
    }
    #[test]
    fn column_types_covers_known_non_optional_columns_read_as_rust_values() {
        // Stichprobe der beim Review (2026-09-23) identifizierten, mit einem
        // konkreten (nicht generisch geskippten) Rust-Typ gelesenen Spalten
        // je Tabelle - haelt fest, was COLUMN_TYPES mindestens abdecken muss,
        // damit eine kuenftige Aenderung eine davon nicht versehentlich
        // wieder herausfallen laesst.
        let must_have: &[(&str, &str)] = &[
            ("files", "name"), ("files", "path"), ("files", "file_type"), ("files", "imported_at"), ("files", "favorite"),
            ("folders", "name"), ("folders", "path"),
            ("tags", "name"), ("tags", "color_hue"),
            ("file_metadata", "label"), ("file_metadata", "value"),
            ("filament_spools", "material"), ("filament_spools", "diameter_mm"),
            ("filament_spools", "original_weight_g"), ("filament_spools", "remaining_weight_g"),
            ("printers", "name"),
            ("material_units", "printer_id"), ("material_units", "name"), ("material_units", "kind"), ("material_units", "slot_count"),
            ("collections", "name"),
            ("registered_slicers", "name"), ("registered_slicers", "executable_path"), ("registered_slicers", "is_auto_detected"),
            ("file_tags", "file_id"),
            ("collection_files", "file_id"),
        ];
        for (table, column) in must_have {
            assert!(
                COLUMN_TYPES.iter().any(|(t, c, _, _)| t == table && c == column),
                "COLUMN_TYPES hat keinen Eintrag fuer '{table}.{column}'"
            );
        }
    }
    #[test]
    fn replace_catalog_db_backs_up_old_db_and_installs_new_one() {
        let dir = unique_test_dir("replace_catalog_db_success");
        let db_path = dir.join("catalog.db");

        // "Alte" laufende DB: leerer Katalog.
        let old_conn = crate::db::connect(&db_path).expect("connect creates schema");
        drop(old_conn);
        let old_bytes = std::fs::read(&db_path).expect("read old db bytes");

        // "Neue" DB (simuliert das aus dem Zip entpackte catalog.db) mit
        // einem Datensatz, damit sich alt/neu unterscheiden lassen.
        let new_db_path = dir.join("incoming_catalog.db");
        let new_conn = crate::db::connect(&new_db_path).expect("connect creates schema");
        new_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('new.3mf', '/incoming/new.3mf', '3mf', 42, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed new db with a marker row");
        drop(new_conn);
        let new_bytes = std::fs::read(&new_db_path).expect("read new db bytes");
        assert_ne!(old_bytes, new_bytes, "old and new db content must differ for this test to be meaningful");

        // AppState haelt zunaechst die "alte" Connection auf db_path.
        let running_conn = crate::db::connect(&db_path).expect("reopen db for AppState");
        let state = AppState {
            db: Mutex::new(running_conn),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
            sensitive_dirs: Vec::new(),
        };

        let result = replace_catalog_db(&state, &new_db_path);
        assert!(result.is_ok(), "expected successful replacement: {result:?}");

        // Alte DB wurde zu genau einer .bak-* Datei umbenannt (nicht geloescht)
        // und ihr Inhalt entspricht dem alten Katalog.
        let bak_entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert_eq!(bak_entries.len(), 1, "expected exactly one backup file");
        let bak_bytes = std::fs::read(bak_entries[0].path()).expect("read backup db bytes");
        assert_eq!(bak_bytes, old_bytes, "backup file must contain the old db content");

        // Neuer Inhalt liegt jetzt unter db_path - kein exakter Byte-Vergleich
        // mit `new_bytes` mehr moeglich (Korrektur, Task 11): die
        // M-06-Sanierung in `replace_catalog_db` migriert die eingehende
        // Datenbank und schreibt ihre `registered_slicers`-Tabelle in einer
        // eigenen Transaktion neu, BEVOR sie kopiert wird - der auf der
        // Platte liegende Byteinhalt von `new_db_path` (und damit auch der
        // kopierte Inhalt unter db_path) unterscheidet sich deshalb absichtlich
        // vom urspruenglich eingelesenen `new_bytes`. Stattdessen wird hier
        // inhaltlich geprueft, dass es sich immer noch um dieselbe (jetzt
        // sanierte) eingehende Datenbank handelt.
        let installed_bytes = std::fs::read(&db_path).expect("read installed db bytes");
        assert_ne!(installed_bytes, old_bytes, "db_path must no longer contain the old db content");

        // AppState's Connection zeigt jetzt tatsaechlich auf den neuen
        // Inhalt (nicht mehr auf den In-Memory-Platzhalter) - der Marker-
        // Datensatz aus der neuen DB ist ueber die laufende Connection
        // sichtbar. Dass `fs::rename` weiter oben ueberhaupt erfolgreich
        // war, beweist implizit, dass der Connection-Swap das alte
        // Datei-Handle vorher freigegeben hat (ein noch offenes Handle
        // haette das Umbenennen auf Windows mit einer Sharing-Violation
        // scheitern lassen).
        let guard = state.db.lock().unwrap();
        let count: i64 =
            guard.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "connection must reflect the newly installed db's content");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn replace_catalog_db_fails_closed_when_the_incoming_db_path_does_not_exist() {
        // Korrektur (Task 11, M-06): dieser Test hiess frueher
        // `replace_catalog_db_restores_backup_when_copy_of_new_db_fails` und
        // pruefte den Rename-Rueckbau-Pfad, der eintritt, wenn `fs::copy`
        // fehlschlaegt, weil `new_db_path` nicht existiert. Seit die
        // registered_slicers-Sanierung VOR jedem Datei-Swap direkt auf der
        // eingehenden Datenbank laeuft, wird dieser alte Fehlerpfad fuer
        // dieses Szenario gar nicht mehr erreicht: `Connection::open` legt
        // eine nicht existierende Datei automatisch als neue, leere
        // SQLite-Datenbank an, und die anschliessende Migration schlaegt
        // dort sofort fehl (keine der erwarteten Basistabellen existiert) -
        // lange bevor `fs::rename`/`fs::copy` auf `state.db_path` ueberhaupt
        // aufgerufen werden. Das ist eine Verbesserung, keine Regression:
        // der alte Katalog wird in diesem Fall gar nicht erst angefasst,
        // statt umbenannt und wieder zurueckbenannt werden zu muessen.
        let dir = unique_test_dir("replace_catalog_db_missing_incoming");
        let db_path = dir.join("catalog.db");

        let old_conn = crate::db::connect(&db_path).expect("connect creates schema");
        old_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('old.3mf', '/old/old.3mf', '3mf', 7, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed old db with a marker row");
        drop(old_conn);
        let old_bytes = std::fs::read(&db_path).expect("read old db bytes");

        // Existiert absichtlich nicht.
        let missing_new_db_path = dir.join("does_not_exist.db");

        let running_conn = crate::db::connect(&db_path).expect("reopen db for AppState");
        let state = AppState {
            db: Mutex::new(running_conn),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
            sensitive_dirs: Vec::new(),
        };

        let result = replace_catalog_db(&state, &missing_new_db_path);
        assert!(result.is_err(), "expected an error when the new db file is missing");

        // Es wurde ueberhaupt kein .bak-* angelegt - das Fail-Closed greift
        // VOR dem ersten Rename, state.db_path wurde nie angefasst.
        let bak_entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert!(bak_entries.is_empty(), "no backup file should ever have been created - the sanitization gate fails before any rename");
        assert!(db_path.exists(), "catalog.db must still exist under its original, untouched path");
        let restored_bytes = std::fs::read(&db_path).expect("read db bytes");
        assert_eq!(restored_bytes, old_bytes, "old db content must be completely untouched");

        // AppState's Connection wurde nie durch den In-Memory-Platzhalter
        // ersetzt (der Swap passiert erst NACH der erfolgreichen
        // Sanierung) - der Marker-Datensatz aus der alten DB muss ueber die
        // unveraendert laufende Connection weiterhin sichtbar sein.
        let guard = state.db.lock().unwrap();
        let count: i64 =
            guard.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "connection must still reflect the untouched original db");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn replace_catalog_db_restores_backup_when_the_copy_step_fails_for_a_valid_sanitized_incoming_db() {
        // Deckt eine durch die Task-11-Aenderungen verlorene Regression ab:
        // der vorherige Test `replace_catalog_db_restores_backup_when_copy_of_new_db_fails`
        // simulierte einen Kopier-Fehlschlag ueber einen fehlenden
        // `new_db_path` - dieses Szenario schlaegt seit der M-06-Sanierung
        // aber bereits FRUEHER (in der Migration, siehe
        // `replace_catalog_db_fails_closed_when_the_incoming_db_path_does_not_exist`),
        // wodurch der eigentliche "fs::copy schlaegt fehl, alte DB wird
        // zurueckbenannt" Wiederherstellungspfad (dessen Fehlermeldungstext
        // selbst schon einmal Gegenstand eines fruerheren Reviews war, siehe
        // die Kommentare an `replace_catalog_db_with_copy_fn` oben) seitdem
        // durch KEINEN Test mehr abgedeckt war.
        //
        // Ein echter `fs::copy`-Fehlschlag laesst sich hier nicht ueber
        // chmod/Dateisystem-Tricks erzwingen: `backup_path` liegt (per
        // `with_file_name`) IMMER im selben Verzeichnis wie `db_path`, and
        // sowohl das vorausgehende `fs::rename` als auch das anschliessende
        // Neuanlegen der Datei unter demselben Namen benoetigen exakt
        // dieselbe Verzeichnis-Schreibberechtigung - ein schreibgeschuetztes
        // Zielverzeichnis wuerde deshalb bereits das `fs::rename` scheitern
        // lassen (ein ANDERER, bereits separat abgedeckter Fehlerpfad),
        // nicht speziell den `fs::copy`-Schritt. Stattdessen wird hier die
        // testbare `replace_catalog_db_with_copy_fn`-Variante direkt mit
        // einer bewusst fehlschlagenden `copy_fn` aufgerufen (siehe deren
        // Doc-Kommentar) - deterministisch, portabel, ohne Root-Rechte oder
        // Race-Conditions.
        let dir = unique_test_dir("replace_catalog_db_copy_step_fails");
        let db_path = dir.join("catalog.db");

        let old_conn = crate::db::connect(&db_path).expect("connect creates schema");
        old_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('old.3mf', '/old/old.3mf', '3mf', 7, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed old db with a marker row");
        drop(old_conn);
        let old_bytes = std::fs::read(&db_path).expect("read old db bytes");

        // Eine ECHTE, valide, sanierbare eingehende Datenbank - anders als
        // im (jetzt umbenannten) Fail-Closed-Test oben, damit dieser Test
        // wirklich den Kopier-Schritt prueft und nicht erneut die
        // Sanierung.
        let new_db_path = dir.join("incoming_catalog.db");
        let new_conn = crate::db::connect(&new_db_path).expect("connect creates schema");
        new_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('new.3mf', '/incoming/new.3mf', '3mf', 42, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed new db with a marker row");
        drop(new_conn);

        let running_conn = crate::db::connect(&db_path).expect("reopen db for AppState");
        let state = AppState {
            db: Mutex::new(running_conn),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
            sensitive_dirs: Vec::new(),
        };

        let result = replace_catalog_db_with_copy_fn(&state, &new_db_path, |_from, _to| {
            Err(std::io::Error::other("simulated disk failure during copy"))
        });
        assert!(result.is_err(), "expected an error when the copy step itself fails");
        let err = result.unwrap_err();
        assert!(
            err.contains("wiederhergestellt"),
            "expected error to mention successful restoration, got: {err}"
        );

        // Alte DB wurde nach dem fehlgeschlagenen Kopieren wieder an ihren
        // urspruenglichen Platz zurueckbenannt - kein .bak-* liegt mehr da,
        // db_path enthaelt wieder den alten Inhalt.
        let bak_entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert!(bak_entries.is_empty(), "backup file must have been renamed back after successful restore");
        assert!(db_path.exists(), "catalog.db must exist again after restore");
        let restored_bytes = std::fs::read(&db_path).expect("read restored db bytes");
        assert_eq!(restored_bytes, old_bytes, "restored db must match the original content");

        // AppState's Connection muss nach dem erfolgreichen Restore
        // tatsaechlich wieder nutzbar sein und den alten (wiederhergestellten)
        // Inhalt lesen - nicht auf dem In-Memory-Platzhalter haengen bleiben
        // (Finding I1, siehe die aeltere, gleichnamig gepruefte Logik oben).
        // Der Marker-Datensatz aus der alten DB muss ueber die laufende
        // Connection sichtbar sein, und der aus der (nie aktivierten)
        // eingehenden DB darf es nicht sein.
        let guard = state.db.lock().unwrap();
        let count: i64 =
            guard.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "connection must reflect the restored old db's content, not the placeholder");
        let has_new_marker: bool = guard
            .query_row("SELECT COUNT(*) FROM files WHERE name = 'new.3mf'", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(!has_new_marker, "the never-activated incoming db's content must not be visible");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn restoring_a_catalog_backup_does_not_overwrite_the_local_slicer_registry() {
        // Deckt die zweite P0-Korrektur ab: registered_slicers ist
        // maschinenlokal und darf durch KEINEN Backup-Restore veraendert
        // werden - weder durch Uebernahme fremder Eintraege noch durch
        // Vermischen mit den lokal bereits registrierten.
        let dir = unique_test_dir("restore_preserves_local_slicers");
        let db_path = dir.join("catalog.db");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).unwrap();

        // "Lokal bereits registrierter" Slicer VOR dem Restore - std::env::
        // current_exe() ist plattformunabhaengig ein real existierender,
        // ausfuehrbarer Pfad (siehe Begruendung oben).
        let local_executable = std::env::current_exe().unwrap();
        let conn = crate::db::connect(&db_path).unwrap();
        register_slicer_with_conn(&conn, "Lokaler Slicer".into(), local_executable.to_string_lossy().to_string()).unwrap();
        drop(conn);

        // Backup-DB mit einem ANDEREN, fremden Slicer-Eintrag bauen (simuliert
        // ein Backup von einer fremden/kompromittierten Maschine). Bewusst
        // ueber db::insert_registered_slicer() direkt statt ueber
        // register_slicer_with_conn(), da letzteres intern
        // validate_slicer_path() aufruft - "/tmp/attacker-binary" existiert
        // im Testsystem nicht und wuerde dort scheitern, bevor die Fixture
        // ueberhaupt aufgebaut ist. Genau das soll dieser Test aber simulieren:
        // eine bereits manipulierte/fremde Backup-Datenbank mit einem Eintrag,
        // der nie durch die lokale Pfad-Validierung gelaufen ist - nicht einen
        // regulaeren, validierten lokalen Registrierungsvorgang.
        let backup_db_path = dir.join("incoming.db");
        let backup_conn = crate::db::connect(&backup_db_path).unwrap();
        db::insert_registered_slicer(&backup_conn, "Fremder Slicer", "/tmp/attacker-binary", false).unwrap();
        drop(backup_conn);
        let backup_bytes = std::fs::read(&backup_db_path).unwrap();

        let state = AppState {
            db: std::sync::Mutex::new(crate::db::connect(&db_path).unwrap()),
            trash_dir: trash_dir.clone(),
            db_path: db_path.clone(),
            sensitive_dirs: vec![],
        };
        validate_catalog_db_bytes(&backup_bytes, &state.sensitive_dirs, &state.trash_dir).unwrap();
        std::fs::write(&backup_db_path, &backup_bytes).unwrap();
        replace_catalog_db(&state, &backup_db_path).unwrap();

        let conn = state.db.lock().unwrap();
        let slicers = db::list_registered_slicers(&conn).unwrap();
        assert!(
            slicers.iter().all(|s| s.executable_path != "/tmp/attacker-binary"),
            "ein aus dem Backup importierter Slicer-Pfad darf niemals in der lokalen Registry landen"
        );
        assert!(
            slicers.iter().any(|s| s.name == "Lokaler Slicer"),
            "der bereits lokal registrierte Slicer darf durch den Restore nicht verloren gehen (Korrektur nach dritter Review-Runde: die vorherige Fassung loeschte registered_slicers unconditional nach jedem Restore und riss dabei auch echte, lokal gueltige Eintraege mit)"
        );
    }
    #[test]
    fn replace_catalog_db_never_activates_an_unsanitized_incoming_database() {
        // P0, vierte/fuenfte Review-Runde: die Sanierung von registered_slicers
        // muss VOR dem Live-Swap auf der EINGEHENDEN Datenbank passieren, nicht
        // danach auf der bereits aktiven state.db - sonst kann ein Fehler
        // waehrend der Sanierung die Transaktion zurueckrollen, WAEHREND die
        // importierte (fremde) Datenbank technisch schon aktiv ist, sodass
        // nicht vertrauenswuerdige Backup-Slicer sichtbar werden, obwohl
        // replace_catalog_db() einen Fehler liefert.
        //
        // Fehler-Injektion (fuenfte Review-Runde, ersetzt den urspruenglichen
        // BEFORE-DELETE-Trigger-Ansatz): die Sanierung nutzt inzwischen `DROP
        // TABLE IF EXISTS registered_slicers` gefolgt von `CREATE TABLE
        // registered_slicers (...)` statt `DELETE FROM` (siehe Korrektur in
        // Step 6) - ein an der Tabelle haengender Trigger wuerde durch DROP
        // TABLE automatisch mit entfernt und koennte den Sanierungs-Schritt
        // gar nicht mehr stoeren. Stattdessen wird hier `registered_slicers`
        // in der eingehenden Datenbank bewusst als VIEW statt als Tabelle
        // angelegt: `DROP TABLE IF EXISTS` laesst eine gleichnamige VIEW
        // unangetastet (sie ist kein TABLE-Objekt), wodurch das nachfolgende
        // `CREATE TABLE registered_slicers (...)` deterministisch mit einem
        // Namenskonflikt fehlschlaegt - simuliert eine gezielt praeparierte,
        // strukturell defekte Backup-Datei. Bewusst OHNE vorherigen Aufruf von
        // validate_catalog_db_bytes: dieser Test prueft replace_catalog_db()
        // isoliert und muss auch OHNE die vorgelagerte H-05-Pruefung aus
        // Task 5 (die eine View ohnehin ablehnen wuerde, siehe
        // validate_catalog_db_bytes_rejects_a_database_with_an_injected_view)
        // "fail closed" bleiben - beide Barrieren sind unabhaengig voneinander
        // wirksam.
        let dir = unique_test_dir("restore_fail_closed");
        let db_path = dir.join("catalog.db");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).unwrap();

        let local_executable = std::env::current_exe().unwrap();
        let conn = crate::db::connect(&db_path).unwrap();
        register_slicer_with_conn(&conn, "Lokaler Slicer".into(), local_executable.to_string_lossy().to_string()).unwrap();
        drop(conn);

        let backup_db_path = dir.join("incoming.db");
        let backup_conn = rusqlite::Connection::open(&backup_db_path).unwrap();
        backup_conn.execute_batch(crate::db::SCHEMA_SQL).unwrap();
        // Absichtlich VOR jeder Migration: registered_slicers existiert hier
        // NICHT als Tabelle, sondern als View mit einer Zeile, die als
        // "fremder Slicer" durchgehen wuerde, WENN die Sanierung faelschlich
        // erfolgreich waere.
        backup_conn.execute_batch(
            "CREATE VIEW registered_slicers AS
             SELECT 1 AS id, 'Fremder Slicer' AS name, '/tmp/attacker-binary' AS executable_path, 0 AS is_auto_detected;",
        ).unwrap();
        drop(backup_conn);
        let backup_bytes = std::fs::read(&backup_db_path).unwrap();

        let state = AppState {
            db: std::sync::Mutex::new(crate::db::connect(&db_path).unwrap()),
            trash_dir: trash_dir.clone(),
            db_path: db_path.clone(),
            sensitive_dirs: vec![],
        };
        std::fs::write(&backup_db_path, &backup_bytes).unwrap();

        let result = replace_catalog_db(&state, &backup_db_path);
        assert!(result.is_err(), "replace_catalog_db muss fehlschlagen, wenn die eingehende Datenbank nicht sanierbar ist");

        let conn = state.db.lock().unwrap();
        let slicers = db::list_registered_slicers(&conn).unwrap();
        assert!(
            slicers.iter().any(|s| s.name == "Lokaler Slicer"),
            "die alte/lokale Datenbank muss aktiv bleiben und den lokalen Slicer behalten, wenn die Sanierung fehlschlaegt"
        );
        assert!(
            slicers.iter().all(|s| s.executable_path != "/tmp/attacker-binary"),
            "der fremde Slicer aus der eingehenden Datenbank darf ueber state.db niemals sichtbar werden"
        );
        assert!(db_path.exists(), "die urspruengliche catalog.db darf nicht durch die unsanierte eingehende Datenbank ersetzt worden sein");
    }
    #[test]
    fn group_duplicates_groups_two_matching_hashes_oldest_first() {
        let older = sample_file_record(1, Some("hash-a"), "2026-09-01T00:00:00Z");
        let newer = sample_file_record(2, Some("hash-a"), "2026-09-05T00:00:00Z");
        let files = vec![newer.clone(), older.clone()];

        let groups = group_duplicates(files, &HashSet::new());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[0][0].id, older.id);
        assert_eq!(groups[0][1].id, newer.id);
    }
    #[test]
    fn group_duplicates_excludes_orphaned_member_leaving_no_group() {
        // Regression test for Finding 1 (final review, 2026-09-10): if one
        // of two same-hash files is orphaned (its file is confirmed gone),
        // it must be excluded from grouping entirely - leaving only one
        // surviving file, which is below the size-2 duplicate threshold and
        // therefore must NOT form a group. Previously the orphaned file
        // could be selected as the group's "keep the oldest" anchor while
        // also being pre-checked for deletion via the orphaned list, which
        // could wipe the last surviving copy.
        let orphaned = sample_file_record(1, Some("hash-a"), "2026-09-01T00:00:00Z");
        let surviving = sample_file_record(2, Some("hash-a"), "2026-09-05T00:00:00Z");
        let files = vec![orphaned.clone(), surviving.clone()];
        let mut orphaned_ids = HashSet::new();
        orphaned_ids.insert(orphaned.id);

        let groups = group_duplicates(files, &orphaned_ids);

        assert!(groups.is_empty());
    }
    #[test]
    fn group_duplicates_ignores_files_without_content_hash() {
        let a = sample_file_record(1, None, "2026-09-01T00:00:00Z");
        let b = sample_file_record(2, None, "2026-09-02T00:00:00Z");

        let groups = group_duplicates(vec![a, b], &HashSet::new());

        assert!(groups.is_empty());
    }
    #[test]
    fn group_duplicates_groups_three_matching_hashes_sorted() {
        let a = sample_file_record(1, Some("hash-a"), "2026-09-03T00:00:00Z");
        let b = sample_file_record(2, Some("hash-a"), "2026-09-01T00:00:00Z");
        let c = sample_file_record(3, Some("hash-a"), "2026-09-02T00:00:00Z");

        let groups = group_duplicates(vec![a, b, c], &HashSet::new());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
        assert_eq!(
            groups[0].iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_folder_path_in_sensitive_directory() {
        // Grenzpruefung direkt an der Vertrauensgrenze (Import): ein
        // praepariertes Katalog-Backup mit einem `folders.path`-Eintrag in
        // einem geschuetzten Verzeichnis darf nicht durchgehen, auch wenn die
        // DB selbst technisch gueltig ist.
        let tmp_path = std::env::temp_dir().join(format!(
            "validate_catalog_db_sensitive_test_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        // Als echtes Verzeichnis angelegt (nicht nur als Pfad-String): sonst
        // loest `resolve_path_for_sensitivity_check` fuer den zu pruefenden
        // Pfad ueber den naechsten EXISTIERENDEN Vorfahren auf (z.B. auf
        // macOS "/tmp" -> "/private/tmp"), waehrend der nicht-existierende
        // `sensitive_root` unaufgeloest bliebe - der Praefixvergleich
        // scheitert dann an einem reinen Test-Artefakt, nicht an echter
        // Sicherheitslogik.
        let sensitive_root = unique_test_dir("3mf-test-sensitive-root");
        {
            let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            db::insert_folder_with_parent(&conn, "evil", None, &sensitive_root.join("evil").to_string_lossy())
                .expect("insert malicious folder row");
            drop(conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");

        let result = validate_catalog_db_bytes(&bytes, &[sensitive_root.clone()], &std::env::temp_dir());

        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_dir_all(&sensitive_root);
        assert!(result.is_err(), "must reject an imported catalog whose folder path lies in a sensitive directory");
    }
    /// Legt eine gueltige Katalog-DB an, setzt darin genau eine `files`-Zeile
    /// auf die uebergebenen Werte und gibt die Roh-Bytes zurueck - so wie sie
    /// in einem praeparierten Backup-ZIP laegen.
    fn catalog_db_bytes_with_file_row(
        name: &str,
        path: &str,
        trash_path: Option<&str>,
    ) -> Vec<u8> {
        let tmp_path = std::env::temp_dir().join(format!(
            "validate_catalog_db_files_test_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        {
            let mut conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            let new_file = sample_new_file_for_rescan_test(std::path::Path::new(path));
            let id = db::insert_file(&mut conn, &new_file).expect("insert file row");
            conn.execute(
                "UPDATE files SET name = ?1, path = ?2, trash_path = ?3 WHERE id = ?4",
                rusqlite::params![name, path, trash_path, id],
            )
            .expect("patch file row");
            drop(conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");
        let _ = std::fs::remove_file(&tmp_path);
        bytes
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_harmless_file_rows() {
        let dir = unique_test_dir("validate_catalog_db_files_ok");
        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&dir.join("1-modell.3mf").to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[std::path::PathBuf::from("/etc")], &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "a legitimate backup must still import: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_file_name_with_path_traversal() {
        // `files.name` landet in `trash_dir.join(format!("{id}-{name}"))` -
        // ein Name mit "../" schreibt beim Loeschen aus dem Papierkorb heraus
        // (Security-Review 2026-09-19, Finding I-1).
        let dir = unique_test_dir("validate_catalog_db_files_name");
        let bytes = catalog_db_bytes_with_file_row(
            "../../../.config/autostart/evil.desktop",
            &dir.join("modell.3mf").to_string_lossy(),
            None,
        );
        let result = validate_catalog_db_bytes(&bytes, &[], &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err(), "must reject an imported file row whose name escapes the trash dir");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_file_path_in_sensitive_directory() {
        // Als echtes Verzeichnis angelegt, nicht nur als Pfad-String -
        // siehe Begruendung in `..._rejects_folder_path_in_sensitive_directory`.
        let sensitive_root = unique_test_dir("3mf-test-sensitive-file-path");
        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &sensitive_root.join("modell.3mf").to_string_lossy(),
            None,
        );
        let result = validate_catalog_db_bytes(&bytes, &[sensitive_root.clone()], &std::env::temp_dir());
        let _ = std::fs::remove_dir_all(&sensitive_root);
        assert!(result.is_err(), "must reject an imported file row pointing into a sensitive directory");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_trash_path_outside_the_real_trash_dir() {
        // Der Angriff, den die reine Denylist offen liess: `~/Dokumente/...`
        // ist kein geschuetztes Systemverzeichnis, `purge_expired_trash_on_startup`
        // haette die Datei beim naechsten App-Start trotzdem per `remove_file`
        // entfernt (Security-Review 2026-09-19, Finding I-1).
        let dir = unique_test_dir("validate_catalog_db_trash_containment");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");
        let victim = dir.join("irgendwas-wichtiges.pdf");
        std::fs::write(&victim, b"wichtig").expect("create victim file");

        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&victim.to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            result.is_err(),
            "must reject a trash_path outside the app's real trash directory: {result:?}"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_accepts_trash_path_inside_the_real_trash_dir() {
        // Gegenprobe zum Test darueber: ein echtes, unveraendertes Backup
        // dieser App traegt ausschliesslich Werte aus
        // `state.trash_dir.join(...)` und muss weiterhin importierbar sein.
        let dir = unique_test_dir("validate_catalog_db_trash_containment_ok");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");

        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&trash_dir.join("1-modell.3mf").to_string_lossy()),
        );
        let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "a legitimate backup must still import: {result:?}");
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_trash_path_escaping_the_trash_dir_via_parent_components() {
        // `<trash_dir>/../opfer.pdf` faellt im Praefix-Vergleich auf der rohen
        // Zeichenkette nicht auf - `resolve_path_for_sensitivity_check` loest
        // den Pfad vorher auf (gleiche Logik wie bei der Denylist, Finding I-3).
        let dir = unique_test_dir("validate_catalog_db_trash_containment_dotdot");
        let trash_dir = dir.join("trash");
        std::fs::create_dir_all(&trash_dir).expect("create trash dir");

        let escaping = format!("{}/../opfer.pdf", trash_dir.to_string_lossy());
        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&escaping),
        );
        let result = validate_catalog_db_bytes(&bytes, &[], &trash_dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            result.is_err(),
            "must reject a trash_path that escapes the trash dir via \"..\": {result:?}"
        );
    }
    #[test]
    fn validate_catalog_db_bytes_rejects_trash_path_in_sensitive_directory() {
        // `files.trash_path` geht ungefragt an `fs::remove_file` - unter
        // anderem in `purge_expired_trash_on_startup`, das beim App-Start
        // ganz ohne Nutzer-Interaktion laeuft.
        let dir = unique_test_dir("validate_catalog_db_trash_path");
        // Als echtes Verzeichnis angelegt, nicht nur als Pfad-String -
        // siehe Begruendung in `..._rejects_folder_path_in_sensitive_directory`.
        let sensitive_root = unique_test_dir("3mf-test-sensitive-trash-path");
        let bytes = catalog_db_bytes_with_file_row(
            "modell.3mf",
            &dir.join("modell.3mf").to_string_lossy(),
            Some(&sensitive_root.join("wichtig.conf").to_string_lossy()),
        );
        // trash_dir bewusst auf temp_dir gesetzt: der praeparierte trash_path
        // liegt darin, die Containment-Pruefung greift also NICHT - dieser
        // Test prueft weiterhin genau die Denylist.
        let result = validate_catalog_db_bytes(&bytes, &[sensitive_root.clone()], &std::env::temp_dir());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&sensitive_root);
        assert!(result.is_err(), "must reject an imported trash_path pointing into a sensitive directory");
    }
    #[test]
    fn validate_file_name_rejects_separators_and_parent_dir_sequences() {
        assert!(validate_file_name("modell.3mf").is_ok());
        assert!(validate_file_name("Modell mit Leerzeichen (v2).stl").is_ok());
        assert!(validate_file_name("").is_err());
        assert!(validate_file_name("   ").is_err());
        assert!(validate_file_name("../evil.3mf").is_err());
        assert!(validate_file_name("sub/evil.3mf").is_err());
        assert!(validate_file_name("sub\\evil.3mf").is_err());
        assert!(validate_file_name("..").is_err());
    }
    #[test]
    fn reject_oversized_zip_entry_uses_the_declared_uncompressed_size() {
        assert!(reject_oversized_zip_entry("catalog.db", 10, 100).is_ok());
        assert!(reject_oversized_zip_entry("catalog.db", 100, 100).is_ok());
        assert!(reject_oversized_zip_entry("catalog.db", 101, 100).is_err());
    }

    #[test]
    fn validate_catalog_db_bytes_accepts_an_old_backup_without_the_kind_column() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            conn.execute_batch(
                "DROP INDEX IF EXISTS idx_filament_spools_slot;
                 DROP TABLE filament_spools;
                 CREATE TABLE filament_spools (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                    diameter_mm REAL NOT NULL, original_weight_g INTEGER NOT NULL,
                    remaining_weight_g INTEGER NOT NULL, price REAL, image_png BLOB,
                    created_at TEXT NOT NULL,
                    unit_id INTEGER REFERENCES material_units(id) ON DELETE SET NULL,
                    slot_index INTEGER, home_location TEXT, color_hex TEXT
                 );
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at)
                    VALUES ('PLA', 1.75, 1000, 600, '2026-09-01');",
            )
            .unwrap();
            conn.pragma_update(None, "user_version", version - 1).unwrap();
        });
        let result = validate_catalog_db_bytes(&bytes, &sensitive, &trash);
        assert!(result.is_ok(), "aeltere Sicherung ohne kind muss gehen: {result:?}");
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_resin_in_a_slot() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute_batch(
                "INSERT INTO printers (name) VALUES ('X1C');
                 INSERT INTO material_units (printer_id, name, kind, slot_count) VALUES (1, 'AMS', 'bambu_ams', 4);
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind, unit_id, slot_index)
                    VALUES ('Standard', 1.75, 1000, 500, '2026-09-25', 'resin', 1, 0);",
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash).is_err());
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_an_unknown_kind_without_check_constraint() {
        let (bytes, sensitive, trash) = backup_test_db_with(|conn| {
            conn.execute_batch(
                "DROP INDEX IF EXISTS idx_filament_spools_slot;
                 DROP TABLE filament_spools;
                 CREATE TABLE filament_spools (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    material TEXT NOT NULL, manufacturer TEXT, color TEXT, location TEXT,
                    diameter_mm REAL NOT NULL, original_weight_g INTEGER NOT NULL,
                    remaining_weight_g INTEGER NOT NULL, price REAL, image_png BLOB,
                    created_at TEXT NOT NULL, unit_id INTEGER, slot_index INTEGER,
                    home_location TEXT, color_hex TEXT, kind TEXT
                 );
                 INSERT INTO filament_spools (material, diameter_mm, original_weight_g, remaining_weight_g, created_at, kind)
                    VALUES ('PLA', 1.75, 1000, 600, '2026-09-01', 'pla');",
            )
            .unwrap();
        });
        assert!(validate_catalog_db_bytes(&bytes, &sensitive, &trash).is_err());
    }
}
