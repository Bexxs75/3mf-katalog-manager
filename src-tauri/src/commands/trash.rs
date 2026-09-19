use super::*;

// Cloud-Löschung ist noch nicht möglich, da es keine Cloud-Anbindung gibt;
// gelöscht werden nur der DB-Eintrag und die lokale Datei.
/// Kernlogik von `delete_file`, getrennt von der `State<AppState>`-Huelle
/// gehalten (gleiche Konvention wie `move_file_to_folder_with_conn`), damit
/// sie in Tests direkt mit einem bereits geladenen `FileRecord` aufgerufen
/// werden kann, statt es intern per `get_file` zu laden.
fn delete_file_with_conn(
    conn: &Connection,
    file: &db::models::FileRecord,
    id: i64,
    trash_dir: &std::path::Path,
) -> CmdResult<()> {
    match std::fs::metadata(&file.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Pfad nicht erreichbar (z.B. umbenannter/verschobener Ordner,
            // nicht eingehaengtes Laufwerk) - es gibt nichts zu
            // verschieben, aber das heisst NICHT zwingend "Datei
            // unwiderruflich weg": eine Cloud-Mount-Umbenennung ist weit
            // haeufiger als ein tatsaechlich geloeschtes Original. Der
            // Katalog-Eintrag wird deshalb trotzdem nur WEICH geloescht
            // (landet im Papierkorb, trash_path bleibt NULL) statt hart
            // entfernt - erst "Endgueltig loeschen" oder Ablauf der
            // 7-Tage-Frist entfernt ihn wirklich. Andere Fehlerarten (z.B.
            // PermissionDenied) durchlaufen stattdessen den regulaeren
            // Fehlerpfad unten, siehe Finding 3 im Review vom 2026-09-10
            // (scan_catalog_issues).
            let deleted_at = chrono::Utc::now().to_rfc3339();
            return db::soft_delete_file(conn, id, None, &deleted_at).map_err(|e| e.to_string());
        }
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }

    let trash_path = trash_dir.join(format!("{id}-{}", file.name));
    move_file(std::path::Path::new(&file.path), &trash_path).map_err(|e| e.to_string())?;

    let deleted_at = chrono::Utc::now().to_rfc3339();
    if let Err(db_err) = db::soft_delete_file(conn, id, Some(&trash_path.to_string_lossy()), &deleted_at) {
        // Kompensation (H-01): physisch bereits in den Papierkorb
        // verschobene Datei zurueckholen, wenn der DB-Eintrag nicht als
        // geloescht markiert werden konnte - sonst "verschwindet" die
        // Datei fuer den Nutzer, obwohl der Katalog sie weiterhin am
        // Originalort fuehrt.
        if let Err(rollback_err) = move_file(&trash_path, std::path::Path::new(&file.path)) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback aus dem Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB fuehrt sie weiterhin als aktiv unter {}",
                trash_path.display(),
                file.path
            ));
        }
        return Err(db_err.to_string());
    }
    Ok(())
}
#[tauri::command]
pub fn delete_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    delete_file_with_conn(&conn, &file, id, &state.trash_dir)
}
#[tauri::command]
pub fn delete_files(state: State<AppState>, file_ids: Vec<String>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    delete_files_with_conn(&conn, &state.trash_dir, file_ids)
}
/// Kernlogik von `delete_files`, getrennt von der `State<AppState>`-Huelle
/// gehalten (gleiche Konvention wie `delete_file_with_conn`/
/// `restore_file_with_conn`), damit sie in Tests ohne eine echte
/// `tauri::State` (die sich aus Testcode heraus nicht konstruieren laesst)
/// aufgerufen werden kann.
fn delete_files_with_conn(conn: &Connection, trash_dir: &std::path::Path, file_ids: Vec<String>) -> CmdResult<()> {
    for file_id in file_ids {
        let id: i64 = match file_id.parse() {
            Ok(id) => id,
            Err(_) => {
                eprintln!("[cleanup] Ungueltige Datei-ID uebersprungen: {file_id}");
                continue;
            }
        };
        // Bereinigungs-Batch: eine zwischenzeitlich bereits geloeschte Datei
        // (z.B. doppelt in der Auswahl) wird uebersprungen statt den ganzen
        // Batch abzubrechen.
        let file = match db::get_file(conn, id) {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("[cleanup] Datei-ID {id} konnte nicht geladen werden: {e}");
                continue;
            }
        };
        // Batch nicht abbrechen (Finding 2 im finalen Review vom
        // 2026-09-10): jede ID wird einzeln versucht, ein fehlgeschlagener
        // Einzelfall wird geloggt und uebersprungen, damit das Frontend am
        // Ende zuverlaessig resyncen kann statt auf einem abgebrochenen
        // Batch mit veraltetem Zustand zu stehen.
        //
        // Finding 2 (Abschluss-Review): ruft dieselbe `delete_file_with_conn`
        // wie das Einzel-Loeschen auf, statt die Physisch-verschieben +
        // DB-Update-Sequenz hier ein zweites Mal nachzubauen - dadurch gilt
        // die dortige H-01-Kompensation (Datei aus dem Papierkorb
        // zurueckholen, wenn `soft_delete_file` fehlschlaegt) jetzt auch fuer
        // den Mehrfachauswahl-Batch. Vorher landete eine Datei bei einem
        // fehlgeschlagenen DB-Update physisch verwaist im Papierkorb,
        // waehrend die DB sie weiterhin als aktiv unter dem (nicht mehr
        // existierenden) Originalpfad fuehrte - der Fehler wurde nur
        // geloggt, der Batch lief weiter und die gesamte Operation meldete
        // trotzdem Erfolg.
        if let Err(e) = delete_file_with_conn(conn, &file, id, trash_dir) {
            eprintln!("[cleanup] Loeschen fehlgeschlagen fuer Datei-ID {id}: {e}");
        }
    }
    Ok(())
}
#[tauri::command]
pub fn list_trash(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let files = db::list_trash(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}
/// Kernlogik von `restore_file`, getrennt von der `State<AppState>`-Huelle
/// gehalten (gleiche Konvention wie `delete_file_with_conn`), damit sie in
/// Tests direkt mit einem bereits geladenen `FileRecord` aufgerufen werden
/// kann.
fn restore_file_with_conn(
    conn: &Connection,
    file: &db::models::FileRecord,
    id: i64,
) -> CmdResult<()> {
    if file.deleted_at.is_none() {
        return Err("file is not in trash".to_string());
    }

    let Some(trash_path) = file.trash_path.clone() else {
        // Kein trash_path gesetzt: die Datei war beim Loeschen bereits am
        // Original-Pfad nicht erreichbar (siehe delete_file), es gibt also
        // physisch nichts zurueckzuverschieben - nur den Katalog-Eintrag
        // wieder sichtbar machen. Ist der Pfad inzwischen wieder erreichbar
        // (z.B. Ordner zurueckbenannt), zeigt er dann wieder korrekt darauf.
        return db::restore_file(conn, id, None).map_err(|e| e.to_string());
    };

    let original = std::path::Path::new(&file.path);
    let target_path = if original.exists() {
        let stem = original.file_stem().and_then(|s| s.to_str()).unwrap_or("datei");
        let ext = original.extension().and_then(|s| s.to_str());
        let parent = original.parent().unwrap_or_else(|| std::path::Path::new("."));
        let new_name = match ext {
            Some(ext) => format!("{stem} (wiederhergestellt).{ext}"),
            None => format!("{stem} (wiederhergestellt)"),
        };
        parent.join(new_name)
    } else {
        original.to_path_buf()
    };

    move_file(std::path::Path::new(&trash_path), &target_path).map_err(|e| e.to_string())?;

    let new_path_str = target_path.to_string_lossy().to_string();
    let new_path_arg = if new_path_str == file.path { None } else { Some(new_path_str.as_str()) };
    if let Err(db_err) = db::restore_file(conn, id, new_path_arg) {
        // Kompensation (H-01): physisch bereits aus dem Papierkorb
        // wiederhergestellte Datei zurueck in den Papierkorb verschieben,
        // wenn der DB-Eintrag nicht als wiederhergestellt markiert werden
        // konnte - sonst zeigt die DB die Datei weiterhin als geloescht,
        // obwohl sie physisch bereits am Zielort liegt.
        if let Err(rollback_err) = move_file(&target_path, std::path::Path::new(&trash_path)) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback in den Papierkorb fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB fuehrt sie weiterhin als geloescht",
                target_path.display()
            ));
        }
        return Err(db_err.to_string());
    }
    Ok(())
}
#[tauri::command]
pub fn restore_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    restore_file_with_conn(&conn, &file, id)
}
#[tauri::command]
pub fn delete_file_permanently(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    if file.deleted_at.is_none() {
        return Err("file is not in trash".to_string());
    }
    if let Some(trash_path) = &file.trash_path {
        if let Err(e) = std::fs::remove_file(trash_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.to_string());
            }
        }
    }
    db::delete_file(&conn, id).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn empty_trash(state: State<AppState>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    let files = db::list_trash(&conn).map_err(|e| e.to_string())?;
    for file in files {
        if let Some(trash_path) = &file.trash_path {
            if let Err(e) = std::fs::remove_file(trash_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[trash] Entfernen fehlgeschlagen fuer Datei-ID {}: {e}", file.id);
                    continue;
                }
            }
        }
        if let Err(e) = db::delete_file(&conn, file.id) {
            eprintln!("[trash] DB-Eintrag konnte nicht geloescht werden fuer Datei-ID {}: {e}", file.id);
        }
    }
    Ok(())
}
/// Beim App-Start aufgerufen: entfernt alle Papierkorb-Eintraege, die
/// laenger als 7 Tage zurueckliegen, endgueltig. Einzelne fehlschlagende
/// Datei wird geloggt und uebersprungen, bricht den Rest nicht ab -
/// gleiches Muster wie der bestehende content_hash-Backfill.
pub fn purge_expired_trash_on_startup(conn: &Connection) {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let expired = match db::purge_expired_trash(conn, &cutoff) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("[startup] Papierkorb-Aufraeumen: Abfrage fehlgeschlagen: {e}");
            return;
        }
    };
    for file in expired {
        if let Some(trash_path) = &file.trash_path {
            if let Err(e) = std::fs::remove_file(trash_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[startup] Papierkorb-Aufraeumen: Datei fehlgeschlagen fuer ID {}: {e}", file.id);
                    continue;
                }
            }
        }
        if let Err(e) = db::delete_file(conn, file.id) {
            eprintln!("[startup] Papierkorb-Aufraeumen: DB-Eintrag fehlgeschlagen fuer ID {}: {e}", file.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_file_compensates_when_soft_delete_fails() {
        // delete_file() nutzt move_file() bereits fuer den Trash-Move - das
        // hier zu testende Kompensationsverhalten muss NACH dem physischen
        // Move in den Papierkorb greifen, falls db::soft_delete_file scheitert
        // (z.B. weil die Datei-Zeile inzwischen durch eine andere Operation
        // bereits geloescht wurde - UPDATE trifft 0 Zeilen).
        let dir = unique_test_dir("delete_file_compensation");
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("model.3mf");
        std::fs::write(&src_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &src_path.to_string_lossy(), None).unwrap();
        // db::delete_file() (HARTES Loeschen der Zeile, nicht soft_delete)
        // VOR dem Aufruf, um ein 0-Zeilen-UPDATE in soft_delete_file zu
        // erzwingen, OHNE get_file() vorher scheitern zu lassen - dafuer wird
        // delete_file_with_conn mit einem bereits geladenen FileRecord
        // aufgerufen statt es intern per get_file zu laden, damit der Test
        // die Reihenfolge exakt kontrollieren kann.
        let file = db::get_file(&conn, file_id).unwrap().unwrap();
        db::delete_file(&conn, file_id).unwrap();

        let trash_dir = unique_test_dir("delete_file_compensation_trash");
        std::fs::create_dir_all(&trash_dir).unwrap();
        let result = delete_file_with_conn(&conn, &file, file_id, &trash_dir);

        assert!(result.is_err(), "must surface the soft_delete_file failure (0 rows affected)");
        assert!(src_path.exists(), "source file must be moved back from trash after soft_delete_file failed");
    }
    #[test]
    fn bulk_delete_compensates_for_one_failing_file_without_stranding_it_in_trash() {
        // Finding 2 (Abschluss-Review): delete_files() (Mehrfachauswahl) muss
        // dieselbe H-01-Kompensation wie das Einzel-Loeschen anwenden. Zwei
        // Dateien werden geloescht; fuer die zweite wird ein 0-Zeilen-UPDATE
        // in soft_delete_file erzwungen (gleiche Technik wie
        // delete_file_compensates_when_soft_delete_fails oben: die Zeile wird
        // VOR dem eigentlichen delete_files()-Aufruf hart geloescht). Erwartet:
        // Datei 1 landet reguendlich im Papierkorb, Datei 2 landet wieder an
        // ihrem urspruenglichen Ort (nicht verwaist im Papierkorb), und der
        // Gesamtaufruf schlaegt trotz des Teilfehlers nicht fehl (Batch-
        // Semantik: log-and-continue).
        let dir = unique_test_dir("bulk_delete_compensation");
        std::fs::create_dir_all(&dir).unwrap();
        let trash_dir = unique_test_dir("bulk_delete_compensation_trash");
        std::fs::create_dir_all(&trash_dir).unwrap();

        let path1 = dir.join("model1.3mf");
        let path2 = dir.join("model2.3mf");
        std::fs::write(&path1, b"CONTENT1").unwrap();
        std::fs::write(&path2, b"CONTENT2").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, &path1.to_string_lossy(), None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, &path2.to_string_lossy(), None).unwrap();

        // Erzwingt ein 0-Zeilen-UPDATE in soft_delete_file NUR fuer id2,
        // NACHDEM delete_file_with_conn die Datei bereits physisch in den
        // Papierkorb verschoben hat - gleiche Technik wie
        // delete_file_compensates_when_soft_delete_fails.
        db::delete_file(&conn, id2).unwrap();

        let result = delete_files_with_conn(&conn, &trash_dir, vec![id1.to_string(), id2.to_string()]);

        assert!(result.is_ok(), "bulk delete must not fail the whole batch on one item's error: {result:?}");

        // id1: regulaer im Papierkorb gelandet.
        let file1 = db::get_file(&conn, id1).unwrap().unwrap();
        assert!(file1.deleted_at.is_some(), "file 1 must be soft-deleted");
        assert!(!path1.exists(), "file 1 must have been moved into the trash");

        // id2: DB-Update fehlgeschlagen (Zeile bereits geloescht) - Datei
        // darf NICHT verwaist im Papierkorb liegen bleiben, sondern muss
        // zurueck an ihren Originalort verschoben worden sein.
        assert!(path2.exists(), "file 2 must be moved back out of trash after the compensating rollback");
        let trash_entry = trash_dir.join(format!("{id2}-model2.3mf"));
        assert!(!trash_entry.exists(), "file 2 must not remain stranded in the trash directory");
    }

    #[test]
    fn restore_file_compensates_when_db_update_fails() {
        let dir = unique_test_dir("restore_file_compensation");
        std::fs::create_dir_all(&dir).unwrap();
        let trash_dir = unique_test_dir("restore_file_compensation_trash");
        std::fs::create_dir_all(&trash_dir).unwrap();

        let original_path = dir.join("model.3mf");
        let trash_path = trash_dir.join("model.3mf");
        std::fs::write(&trash_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &original_path.to_string_lossy(), None).unwrap();
        let deleted_at = chrono::Utc::now().to_rfc3339();
        db::soft_delete_file(&conn, file_id, Some(&trash_path.to_string_lossy()), &deleted_at).unwrap();

        // Erzwingt einen DB-Fehler GENAU bei der Deleted->Restored-Transition
        // (deleted_at wechselt von NOT NULL zu NULL), NACHDEM move_file bereits
        // physisch aus dem Papierkorb zurueckverschoben hat - deterministischer
        // als ein UNIQUE-Konflikt, da restore_file keine kollidierbare Spalte
        // wie `path`/`folders.path` in einer Weise setzt, die sich hier ohne
        // einen zweiten, kuenstlich kollidierenden Datensatz ausnutzen liesse.
        conn.execute_batch(
            "CREATE TRIGGER block_restore BEFORE UPDATE ON files
             WHEN NEW.deleted_at IS NULL AND OLD.deleted_at IS NOT NULL
             BEGIN SELECT RAISE(ABORT, 'simulierter Fehler bei restore_file'); END;",
        )
        .unwrap();

        let file = db::get_file(&conn, file_id).unwrap().unwrap();
        let result = restore_file_with_conn(&conn, &file, file_id);

        assert!(result.is_err(), "must surface the db::restore_file failure raised by the trigger");
        assert!(trash_path.exists(), "file must be moved back into the trash after db::restore_file failed");
        assert!(!original_path.exists(), "destination must not exist after the rollback");
        let still_deleted: Option<String> = conn
            .query_row("SELECT deleted_at FROM files WHERE id = ?1", [file_id], |r| r.get(0))
            .unwrap();
        assert!(still_deleted.is_some(), "db must still show the file as deleted after the failed restore");
    }
}
