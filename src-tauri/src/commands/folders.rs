use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent_id: Option<String>,
    pub count: i64,
}
#[tauri::command]
pub fn list_folders(state: State<AppState>) -> CmdResult<Vec<FolderDto>> {
    let conn = lock_db(&state)?;
    let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;

    fn is_descendant_or_self(folders: &[db::models::FolderRecord], candidate_id: i64, ancestor_id: i64) -> bool {
        if candidate_id == ancestor_id {
            return true;
        }
        let mut current = candidate_id;
        while let Some(f) = folders.iter().find(|f| f.id == current) {
            match f.parent_id {
                Some(pid) if pid == ancestor_id => return true,
                Some(pid) => current = pid,
                None => return false,
            }
        }
        false
    }

    let dtos = folders
        .iter()
        .map(|folder| {
            let count = files
                .iter()
                .filter(|f| f.folder_id.is_some_and(|fid| is_descendant_or_self(&folders, fid, folder.id)))
                .count() as i64;
            FolderDto {
                id: folder.id.to_string(),
                name: folder.name.clone(),
                path: folder.path.clone(),
                parent_id: folder.parent_id.map(|id| id.to_string()),
                count,
            }
        })
        .collect();

    Ok(dtos)
}
/// Sicherheits-Grenze fuer `create_folder`/`rename_folder`: `name` landet
/// unmittelbar in einem `PathBuf::join`/`with_file_name`-Aufruf und muss
/// deshalb eine einzelne, harmlose Pfad-Komponente sein. Ohne diese
/// Pruefung wuerde ein Name wie "../../../etc/x" (via `with_file_name`)
/// oder ein absoluter Pfad wie "/etc/x" (via `join`, das einen absoluten
/// zweiten Operanden den kompletten Basis-Pfad verwerfen laesst) einen
/// physischen Verzeichnis-Vorgang weit ausserhalb des beabsichtigten
/// Katalog-Ordnerbaums ausloesen - erreichbar allein durch Text-Eingabe
/// im "+ Neuer Ordner"-Feld, keine weitere Angriffskette noetig (CWE-22).
fn validate_folder_name(name: &str) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err("Ordnername darf nicht leer sein".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("Ordnername darf keine Pfad-Trennzeichen enthalten".to_string());
    }
    if name == "." || name == ".." {
        return Err("Ungueltiger Ordnername".to_string());
    }
    Ok(())
}
/// Legt einen echten Ordner auf der Platte an (unterhalb eines bestehenden
/// Ordners, oder - bei `parent_id: None` - unterhalb eines vom Nutzer per
/// Dialog gewaehlten Basisverzeichnisses) und eine dazu passende
/// `folders`-Zeile. Schlaegt fehl, wenn der Zielpfad bereits existiert
/// (`std::fs::create_dir`, kein `create_dir_all`).
#[tauri::command]
pub async fn create_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    parent_id: Option<String>,
    name: String,
) -> CmdResult<FolderDto> {
    validate_folder_name(&name)?;
    let parent: Option<i64> = parent_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
        .transpose()?;

    let new_dir = {
        let conn = lock_db(&state)?;
        match parent {
            Some(pid) => {
                let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
                let parent_folder = folders.iter().find(|f| f.id == pid).ok_or_else(|| "folder not found".to_string())?;
                std::path::PathBuf::from(&parent_folder.path).join(&name)
            }
            None => {
                let picked = app.dialog().file().blocking_pick_folder();
                let Some(picked) = picked else {
                    return Err("cancelled".to_string());
                };
                picked.into_path().map_err(|e| e.to_string())?.join(&name)
            }
        }
    };

    reject_if_sensitive_path(&new_dir, &state.sensitive_dirs)?;
    std::fs::create_dir(&new_dir).map_err(|e| e.to_string())?;

    let conn = lock_db(&state)?;
    let new_id = db::insert_folder_with_parent(&conn, &name, parent, &new_dir.to_string_lossy())
        .map_err(|e| e.to_string())?;
    Ok(FolderDto {
        id: new_id.to_string(),
        name,
        path: new_dir.to_string_lossy().to_string(),
        parent_id: parent.map(|p| p.to_string()),
        count: 0,
    })
}
/// Kernlogik von `rename_folder`, getrennt von der `State<AppState>`-Huelle
/// gehalten, damit sie in Tests direkt gegen eine In-Memory-`Connection`
/// aufgerufen werden kann (gleiche Konvention wie `move_file_to_folder_with_conn`).
fn rename_folder_with_conn(
    conn: &Connection,
    id: i64,
    name: String,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    validate_folder_name(&name)?;
    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?.clone();

    let old_path = std::path::PathBuf::from(&folder.path);
    let new_path = old_path.with_file_name(&name);
    reject_if_sensitive_path(&old_path, sensitive_dirs)?;
    reject_if_sensitive_path(&new_path, sensitive_dirs)?;

    if new_path.exists() {
        return Err(format!("Zielordner existiert bereits: {}", new_path.display()));
    }
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    let db_result = (|| -> Result<(), DbError> {
        let tx = conn.unchecked_transaction()?;
        db::rename_folder_name(&tx, id, &name)?;
        db::update_paths_under_folder(&tx, id, &folder.path, &new_path.to_string_lossy())?;
        tx.commit().map_err(DbError::from)
    })();

    if let Err(db_err) = db_result {
        if let Err(rollback_err) = std::fs::rename(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Ordner-Umbenennung fehlgeschlagen ({rollback_err}) - Ordner heisst jetzt {}, DB verweist teilweise noch auf {}",
                new_path.display(),
                old_path.display()
            ));
        }
        return Err(db_err.to_string());
    }
    Ok(())
}
/// Benennt einen echten Ordner auf der Platte um (`std::fs::rename`) und
/// aktualisiert `folders.name` sowie rekursiv `folders.path`/`files.path`
/// fuer den Ordner selbst und alle Nachfahren (beliebige Tiefe).
#[tauri::command]
pub fn rename_folder(state: State<AppState>, folder_id: String, name: String) -> CmdResult<()> {
    let id: i64 = folder_id.parse().map_err(|_| "invalid folder id".to_string())?;
    let conn = lock_db(&state)?;
    rename_folder_with_conn(&conn, id, name, &state.sensitive_dirs)
}
/// Prueft, ob `candidate_id` ein (direkter oder indirekter) Nachfahre von
/// `ancestor_id` ist, indem die `parent_id`-Kette von `candidate_id` aus
/// nach oben verfolgt wird, bis entweder `ancestor_id` gefunden wird oder
/// die Wurzel (`parent_id == None`) erreicht ist.
fn is_descendant(folders: &[db::models::FolderRecord], candidate_id: i64, ancestor_id: i64) -> bool {
    let mut current = candidate_id;
    while let Some(f) = folders.iter().find(|f| f.id == current) {
        match f.parent_id {
            Some(pid) if pid == ancestor_id => return true,
            Some(pid) => current = pid,
            None => return false,
        }
    }
    false
}
/// Kernlogik von `move_folder`, getrennt von der `State<AppState>`-Huelle
/// gehalten, damit sie in Tests direkt gegen eine In-Memory-`Connection`
/// aufgerufen werden kann.
fn move_folder_with_conn(
    conn: &Connection,
    id: i64,
    target: Option<i64>,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    let Some(target) = target else {
        // Es gibt in diesem Plan kein echtes "an die Katalog-Wurzel
        // verschieben"-Ziel (siehe "Abweichung vom Spec" im Plan). Ohne
        // diese Ablehnung wuerde der Code unten den aktuellen Elternordner
        // als "neues" Ziel berechnen (fs::rename waere ein No-Op-Rename
        // auf denselben Pfad), aber set_folder_parent(.., None) wuerde die
        // DB trotzdem so aendern, dass der Ordner keinen Parent mehr hat -
        // eine inkonsistente Mischung aus "physisch weiter verschachtelt"
        // und "DB sagt: kein Parent". Aktuell ruft das Frontend move_folder
        // nie mit None auf; dieser Fehler haelt es so.
        return Err("Ein Ordner kann nicht ohne Zielordner verschoben werden".to_string());
    };

    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?.clone();

    if target == id || is_descendant(&folders, target, id) {
        return Err("Ein Ordner kann nicht in einen eigenen Unterordner verschoben werden".to_string());
    }

    let new_parent_path = folders
        .iter()
        .find(|f| f.id == target)
        .map(|f| f.path.clone())
        .ok_or_else(|| "target folder not found".to_string())?;
    let target = Some(target);

    let old_path = std::path::PathBuf::from(&folder.path);
    let new_path = std::path::PathBuf::from(&new_parent_path).join(&folder.name);
    reject_if_sensitive_path(&old_path, sensitive_dirs)?;
    reject_if_sensitive_path(&new_path, sensitive_dirs)?;

    if new_path.exists() {
        return Err(format!("Zielordner existiert bereits: {}", new_path.display()));
    }
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    // H-01: set_folder_parent + update_paths_under_folder muessen als
    // Einheit gelten - in einer Transaktion, damit ein Fehler in der
    // rekursiven Pfad-Aktualisierung nicht eine halb aktualisierte DB
    // hinterlaesst, waehrend das Filesystem bereits vollstaendig
    // verschoben ist. `Connection::unchecked_transaction()` (statt
    // `transaction()`) braucht nur `&self`, die Signatur bleibt also bei
    // `conn: &Connection` - keine Aenderung an move_folder() noetig.
    let db_result = (|| -> Result<(), DbError> {
        let tx = conn.unchecked_transaction()?;
        db::set_folder_parent(&tx, id, target)?;
        db::update_paths_under_folder(&tx, id, &folder.path, &new_path.to_string_lossy())?;
        tx.commit().map_err(DbError::from)
    })();

    if let Err(db_err) = db_result {
        if let Err(rollback_err) = std::fs::rename(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback des Ordner-Moves fehlgeschlagen ({rollback_err}) - Ordner liegt jetzt unter {}, DB verweist teilweise noch auf {}",
                new_path.display(),
                old_path.display()
            ));
        }
        return Err(db_err.to_string());
    }
    Ok(())
}
/// Verschiebt einen echten Ordner auf der Platte (`std::fs::rename`) unter
/// einen anderen Elternordner und aktualisiert `folders.parent_id` sowie
/// rekursiv `folders.path`/`files.path` fuer den Ordner selbst und alle
/// Nachfahren. Lehnt Zyklen ab (Verschieben in den eigenen Unterordner
/// oder in sich selbst) sowie `new_parent_id: None` (kein "an die
/// Katalog-Wurzel verschieben"-Ziel in diesem Plan, siehe "Abweichung vom
/// Spec").
#[tauri::command]
pub fn move_folder(state: State<AppState>, folder_id: String, new_parent_id: Option<String>) -> CmdResult<()> {
    let id: i64 = folder_id.parse().map_err(|_| "invalid folder id".to_string())?;
    let target: Option<i64> = new_parent_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
        .transpose()?;

    let conn = lock_db(&state)?;
    move_folder_with_conn(&conn, id, target, &state.sensitive_dirs)
}
// Muss aus demselben Grund wie pick_slicer_executable async sein:
// blocking_pick_folder() blockiert den aufrufenden Thread, bis der native
// Ordner-Dialog geschlossen wird.
//
// Der gewaehlte Ordner wird als erlaubtes Entpack-Ziel vermerkt (siehe
// `ApprovedTargets`): nur Katalogordner und hier gewaehlte Ordner nimmt
// `extract_archives` an.
#[tauri::command]
pub async fn pick_folder_path(
    app: tauri::AppHandle,
    approved: State<'_, ApprovedTargets>,
) -> CmdResult<Option<String>> {
    let picked = app.dialog().file().blocking_pick_folder();
    let path = picked.and_then(|p| p.into_path().ok());
    if let Some(path) = &path {
        approved.approve(path);
    }
    Ok(path.map(|p| p.to_string_lossy().to_string()))
}
/// Kernlogik von `register_catalog_base_dir`, getrennt von der
/// `State<AppState>`-Huelle gehalten, damit sie in Tests direkt gegen eine
/// In-Memory-`Connection` aufgerufen werden kann (gleiche Konvention wie
/// `move_file_to_folder_with_conn`/`rename_folder_with_conn`).
fn register_catalog_base_dir_with_conn(conn: &Connection, dir: &Path) -> CmdResult<FolderDto> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let id = db::ensure_folder_path(conn, dir, dir).map_err(|e| e.to_string())?;

    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?;

    Ok(FolderDto {
        id: folder.id.to_string(),
        name: folder.name.clone(),
        path: folder.path.clone(),
        parent_id: folder.parent_id.map(|p| p.to_string()),
        count: 0,
    })
}
/// Registriert ein vom Nutzer gewaehltes Basisverzeichnis als
/// Katalog-Wurzelordner: legt das Verzeichnis auf der Platte an, falls es
/// noch nicht existiert, und stellt per `db::ensure_folder_path` sicher,
/// dass eine passende `folders`-Zeile existiert (idempotent bei erneutem
/// Aufruf mit demselben Pfad).
#[tauri::command]
pub fn register_catalog_base_dir(state: State<AppState>, path: String) -> CmdResult<FolderDto> {
    let dir = std::path::PathBuf::from(&path);
    let conn = lock_db(&state)?;
    register_catalog_base_dir_with_conn(&conn, &dir)
}

/// Wie `register_catalog_base_dir_with_conn`, aber legt NIE ein Verzeichnis
/// an: Beim App-Start soll ein inzwischen geloeschter Speicherort nicht
/// stillschweigend neu entstehen. `None`, wenn der Ordner fehlt.
fn register_existing_catalog_base_dir_with_conn(conn: &Connection, dir: &Path) -> CmdResult<Option<FolderDto>> {
    if !dir.is_dir() {
        return Ok(None);
    }
    register_catalog_base_dir_with_conn(conn, dir).map(Some)
}
/// Beim Start aufgerufen: stellt sicher, dass ein gesetzter Speicherort
/// eine Ordnerzeile hat (u.a. damit er als Entpack-Ziel gilt, siehe
/// `target_is_approved`).
#[tauri::command]
pub fn register_existing_catalog_base_dir(state: State<AppState>, path: String) -> CmdResult<Option<FolderDto>> {
    let conn = lock_db(&state)?;
    register_existing_catalog_base_dir_with_conn(&conn, Path::new(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_registration_of_the_base_dir_never_creates_a_missing_directory() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let missing = unique_test_dir("register-existing-missing");
        std::fs::remove_dir_all(&missing).unwrap();
        assert!(register_existing_catalog_base_dir_with_conn(&conn, &missing).unwrap().is_none());
        assert!(!missing.exists(), "Startaufruf darf keinen Ordner anlegen");

        let existing = unique_test_dir("register-existing-present");
        let first = register_existing_catalog_base_dir_with_conn(&conn, &existing).unwrap().unwrap();
        let second = register_existing_catalog_base_dir_with_conn(&conn, &existing).unwrap().unwrap();
        assert_eq!(first.id, second.id, "idempotent");
        assert_eq!(first.path, existing.to_string_lossy());
    }

    #[test]
    fn register_catalog_base_dir_creates_missing_directory_and_is_idempotent() {
        // unique_test_dir() legt das Verzeichnis bereits an - fuer diesen
        // Test wird das Zielverzeichnis stattdessen erst noch fehlend
        // gebraucht, also nur der (eindeutige) Pfad selbst verwendet und
        // sofort wieder entfernt.
        let base = unique_test_dir("register-base-dir");
        std::fs::remove_dir_all(&base).expect("remove freshly created test dir");
        assert!(!base.exists());

        let conn = crate::db::connect_in_memory().expect("connect");

        let first = register_catalog_base_dir_with_conn(&conn, &base).expect("first call should succeed");
        assert!(base.exists(), "directory must be created on disk");
        assert_eq!(first.name, base.file_name().unwrap().to_string_lossy());
        assert_eq!(first.parent_id, None);

        let second = register_catalog_base_dir_with_conn(&conn, &base).expect("second call should succeed");
        assert_eq!(first.id, second.id, "same path must resolve to the same folder id");

        assert_eq!(db::list_folders(&conn).unwrap().len(), 1, "must not create a duplicate folders row");

        let _ = std::fs::remove_dir_all(&base);
    }
    #[test]
    fn move_folder_rejects_moving_into_own_descendant() {
        // Baum A -> B (zwei Ebenen genuegen fuer den Zyklus-Check selbst,
        // da is_descendant die parent_id-Kette beliebig weit hochlaeuft).
        let tmp = unique_test_dir("move_folder_cycle");
        let a_dir = tmp.join("A");
        let b_dir = a_dir.join("B");
        std::fs::create_dir_all(&b_dir).unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", Some(a_id), &b_dir.to_string_lossy()).expect("insert B");

        let result = move_folder_with_conn(&conn, a_id, Some(b_id), &[]);
        assert!(result.is_err(), "moving A into its own descendant B must be rejected");

        // Weder Platte noch DB duerfen veraendert worden sein.
        assert!(a_dir.exists());
        assert!(b_dir.exists());
        let folders = db::list_folders(&conn).expect("list_folders");
        let a_after = folders.iter().find(|f| f.id == a_id).expect("A still present");
        assert_eq!(a_after.parent_id, None, "A's parent_id must be unchanged after rejected move");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn move_folder_rejects_none_target() {
        // move_folder(id, None) darf nicht mehr, wie frueher, den Ordner
        // physisch unveraendert lassen (fs::rename auf denselben Pfad,
        // No-Op) waehrend set_folder_parent(.., None) die DB trotzdem so
        // aendert, dass der Ordner keinen Parent mehr hat - das war eine
        // stille DB/Platte-Inkonsistenz.
        let tmp = unique_test_dir("move_folder_none_target");
        let a_dir = tmp.join("A");
        let b_dir = a_dir.join("B");
        std::fs::create_dir_all(&b_dir).unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", Some(a_id), &b_dir.to_string_lossy()).expect("insert B");

        let result = move_folder_with_conn(&conn, b_id, None, &[]);
        assert!(result.is_err(), "move_folder with new_parent_id=None must be rejected");

        assert!(b_dir.exists(), "B must remain at its old physical location");
        let folders = db::list_folders(&conn).expect("list_folders");
        let b_after = folders.iter().find(|f| f.id == b_id).expect("B still present");
        assert_eq!(b_after.parent_id, Some(a_id), "B's parent_id must be unchanged after rejected move");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn validate_folder_name_rejects_path_traversal_and_separators() {
        // Regressionstest fuer den im Security-Review 2026-09-13 gefundenen
        // Path-Traversal-Fund (CWE-22): create_folder/rename_folder duerfen
        // `name` nie ungeprueft in PathBuf::join/with_file_name uebergeben.
        assert!(validate_folder_name("../etc").is_err());
        assert!(validate_folder_name("../../tmp/evil").is_err());
        assert!(validate_folder_name("a/b").is_err());
        assert!(validate_folder_name("a\\b").is_err());
        assert!(validate_folder_name("/etc").is_err());
        assert!(validate_folder_name("..").is_err());
        assert!(validate_folder_name(".").is_err());
        assert!(validate_folder_name("").is_err());
        assert!(validate_folder_name("   ").is_err());

        assert!(validate_folder_name("Tabletop").is_ok());
        assert!(validate_folder_name("Ersatzteile 2026").is_ok());
    }
    #[test]
    fn rename_folder_with_conn_rejects_sensitive_target_path() {
        let tmp = unique_test_dir("rename_folder_sensitive");
        let a_dir = tmp.join("A");
        std::fs::create_dir_all(&a_dir).unwrap();
        let sensitive = vec![tmp.clone()];

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");

        let result = rename_folder_with_conn(&conn, a_id, "B".to_string(), &sensitive);
        assert!(result.is_err(), "rename_folder_with_conn must reject a target path under a sensitive directory");
        assert!(a_dir.exists(), "original directory must be untouched after a rejected rename");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn move_folder_with_conn_rejects_sensitive_target_path() {
        let tmp = unique_test_dir("move_folder_sensitive");
        let a_dir = tmp.join("A");
        let b_dir = tmp.join("B");
        std::fs::create_dir_all(&a_dir).unwrap();
        std::fs::create_dir_all(&b_dir).unwrap();
        let sensitive = vec![tmp.clone()];

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", None, &b_dir.to_string_lossy()).expect("insert B");

        let result = move_folder_with_conn(&conn, a_id, Some(b_id), &sensitive);
        assert!(result.is_err(), "move_folder_with_conn must reject a move into a sensitive directory");
        assert!(a_dir.exists(), "original directory must be untouched after a rejected move");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_folder_with_conn_rejects_name_with_path_traversal() {
        let tmp = unique_test_dir("rename_folder_traversal");
        let a_dir = tmp.join("A");
        std::fs::create_dir_all(&a_dir).unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");

        let result = rename_folder_with_conn(&conn, a_id, "../../escaped".to_string(), &[]);
        assert!(result.is_err(), "rename_folder_with_conn must reject a name containing path separators");

        assert!(a_dir.exists(), "original directory must be untouched after a rejected rename");
        let folders = db::list_folders(&conn).expect("list_folders");
        let a_after = folders.iter().find(|f| f.id == a_id).expect("A still present");
        assert_eq!(a_after.name, "A", "name in the DB must be unchanged after a rejected rename");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn move_folder_updates_all_descendant_paths() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        // Baum A/B/C (drei Ebenen), Datei liegt in C, plus ein separater
        // Zielordner X. move_folder(B, Some(X)) haengt B (und damit C und
        // die Datei darunter) physisch unter X um - eine 2-Ebenen-
        // Konstruktion wuerde einen Rekursionsfehler in
        // update_paths_under_folder nicht zuverlaessig aufdecken.
        let tmp = unique_test_dir("move_folder_descendants");
        let a_dir = tmp.join("A");
        let b_dir = a_dir.join("B");
        let c_dir = b_dir.join("C");
        let x_dir = tmp.join("X");
        std::fs::create_dir_all(&c_dir).unwrap();
        std::fs::create_dir_all(&x_dir).unwrap();
        let file_path = c_dir.join("model.3mf");
        write_minimal_3mf(&file_path);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", Some(a_id), &b_dir.to_string_lossy()).expect("insert B");
        let c_id = db::insert_folder_with_parent(&conn, "C", Some(b_id), &c_dir.to_string_lossy()).expect("insert C");
        let x_id = db::insert_folder_with_parent(&conn, "X", None, &x_dir.to_string_lossy()).expect("insert X");

        let imported = import_one(&mut conn, &file_path, None, None, Some(c_id)).expect("import should succeed");
        let file_id: i64 = imported.id.parse().unwrap();

        move_folder_with_conn(&conn, b_id, Some(x_id), &[]).expect("move should succeed");

        let expected_b_dir = x_dir.join("B");
        let expected_c_dir = expected_b_dir.join("C");
        let expected_file_path = expected_c_dir.join("model.3mf");

        assert!(expected_b_dir.exists(), "B must physically exist under X after move");
        assert!(expected_c_dir.exists(), "C must physically exist under the new B location");
        assert!(expected_file_path.exists(), "file must physically exist under the new C location");
        assert!(!a_dir.join("B").exists(), "B must no longer exist under its old parent A");

        let folders = db::list_folders(&conn).expect("list_folders");
        let b_after = folders.iter().find(|f| f.id == b_id).expect("B present");
        assert_eq!(b_after.parent_id, Some(x_id), "B's parent_id must now be X");
        assert_eq!(b_after.path, expected_b_dir.to_string_lossy().to_string());

        let c_after = folders.iter().find(|f| f.id == c_id).expect("C present");
        assert_eq!(c_after.path, expected_c_dir.to_string_lossy().to_string(), "C's path must carry the new B prefix");

        let file_after = db::get_file(&conn, file_id).expect("get_file").expect("file exists");
        assert_eq!(
            file_after.path,
            expected_file_path.to_string_lossy().to_string(),
            "file path must carry the new B/C prefix"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_folder_updates_own_and_descendant_paths() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        // Baum A/B/C, Datei in C. rename_folder(B, "B2") muss B's eigenen
        // Pfad UND den Pfad von C sowie der Datei darunter mitziehen -
        // wieder drei Ebenen, damit die Rekursion tatsaechlich greift.
        let tmp = unique_test_dir("rename_folder_descendants");
        let a_dir = tmp.join("A");
        let b_dir = a_dir.join("B");
        let c_dir = b_dir.join("C");
        std::fs::create_dir_all(&c_dir).unwrap();
        let file_path = c_dir.join("model.3mf");
        write_minimal_3mf(&file_path);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", Some(a_id), &b_dir.to_string_lossy()).expect("insert B");
        let c_id = db::insert_folder_with_parent(&conn, "C", Some(b_id), &c_dir.to_string_lossy()).expect("insert C");

        let imported = import_one(&mut conn, &file_path, None, None, Some(c_id)).expect("import should succeed");
        let file_id: i64 = imported.id.parse().unwrap();

        rename_folder_with_conn(&conn, b_id, "B2".to_string(), &[]).expect("rename should succeed");

        let expected_b_dir = a_dir.join("B2");
        let expected_c_dir = expected_b_dir.join("C");
        let expected_file_path = expected_c_dir.join("model.3mf");

        assert!(expected_b_dir.exists(), "B2 must physically exist");
        assert!(expected_c_dir.exists(), "C must physically exist under the renamed B2");
        assert!(expected_file_path.exists(), "file must physically exist under the renamed B2/C");
        assert!(!b_dir.exists(), "old B directory must no longer exist");

        let folders = db::list_folders(&conn).expect("list_folders");
        let b_after = folders.iter().find(|f| f.id == b_id).expect("B present");
        assert_eq!(b_after.name, "B2");
        assert_eq!(b_after.path, expected_b_dir.to_string_lossy().to_string());

        let c_after = folders.iter().find(|f| f.id == c_id).expect("C present");
        assert_eq!(c_after.path, expected_c_dir.to_string_lossy().to_string(), "C's path must carry the new B2 prefix");

        let file_after = db::get_file(&conn, file_id).expect("get_file").expect("file exists");
        assert_eq!(
            file_after.path,
            expected_file_path.to_string_lossy().to_string(),
            "file path must carry the new B2/C prefix"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_folder_with_non_ascii_name_produces_correct_child_paths() {
        // Regressionstest fuer einen Byte-vs-Zeichen-Offset-Bug in
        // update_paths_under_folder: SQLite's substr() zaehlt auf TEXT-
        // Werten in UTF-8-Zeichen, nicht in Bytes. Ein aus Rust per
        // old_path.len() (Byte-Laenge) berechneter Offset ist bei einem
        // Ordnernamen mit einem Nicht-ASCII-Zeichen (hier: ue) zu gross,
        // wodurch das Pfadtrennzeichen "verschluckt" wird und statt
        // ".../Neu/model.3mf" ein falsches ".../Neumodel.3mf" entsteht.
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        // "Pruefen" mit echtem Umlaut - alle Zeichen davor sind 2-Byte
        // UTF-8-Sequenzen, was alleine schon alte Byte-basierte Offsets
        // ausreichend weit verschiebt, um den Bug zu triggern.
        let tmp = unique_test_dir("rename_folder_non_ascii");
        let src_dir = tmp.join("Pr\u{fc}fen");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("model.3mf");
        write_minimal_3mf(&file_path);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let folder_id =
            db::insert_folder_with_parent(&conn, "Pr\u{fc}fen", None, &src_dir.to_string_lossy()).expect("insert folder");

        let imported = import_one(&mut conn, &file_path, None, None, Some(folder_id)).expect("import should succeed");
        let file_id: i64 = imported.id.parse().unwrap();

        rename_folder_with_conn(&conn, folder_id, "Neu".to_string(), &[]).expect("rename should succeed");

        let expected_dir = tmp.join("Neu");
        let expected_file_path = expected_dir.join("model.3mf");

        assert!(expected_dir.exists(), "renamed folder must physically exist");
        assert!(expected_file_path.exists(), "file must physically exist under the renamed folder");

        let file_after = db::get_file(&conn, file_id).expect("get_file").expect("file exists");
        assert_eq!(
            file_after.path,
            expected_file_path.to_string_lossy().to_string(),
            "file path must be exactly '.../Neu/model.3mf', not corrupted by a byte-vs-character substr offset"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_folder_compensates_when_db_update_fails() {
        let dir = unique_test_dir("rename_folder_compensation");
        std::fs::create_dir_all(dir.join("Alt")).unwrap();

        let conn = db::connect_in_memory().unwrap();
        let folder_id = db::insert_folder_with_parent(&conn, "Alt", None, &dir.join("Alt").to_string_lossy()).unwrap();
        // Zweiten Ordner mit dem Zielnamen anlegen, dessen `path` bereits dem
        // Zielpfad entspricht - folders.path ist ebenfalls UNIQUE, das
        // erzwingt einen echten DB-Fehler NACH dem physischen std::fs::rename.
        db::insert_folder_with_parent(&conn, "Neu", None, &dir.join("Neu").to_string_lossy()).unwrap();

        let result = rename_folder_with_conn(&conn, folder_id, "Neu".to_string(), &[]);

        assert!(result.is_err());
        assert!(dir.join("Alt").exists(), "folder must be renamed back after the DB update failed");
        assert!(!dir.join("Neu").is_dir() || std::fs::read_dir(dir.join("Neu")).unwrap().count() == 0);
    }
}
