use tauri::{Manager, State};

use crate::cloud::config::{default_config_path, load_cloud_config};
use crate::cloud::gdrive::fetch_google_account_email;
use crate::cloud::oauth::run_google_oauth_flow;
use crate::cloud::picker::{self, PickerMode, PickerOutcome};
use crate::cloud::provider::{CloudEntry, StorageProvider};
use crate::cloud::session::{get_fresh_access_token, with_gdrive_provider};
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens};
use crate::commands::{compute_content_hash, import_one, lock_db, to_dto, AppState, ImportResultDto, ModelFileDto};
use crate::db;

type CmdResult<T> = Result<T, String>;

// drive.readonly wurde bewusst entfernt: als "restricted scope" verlangt es
// Googles kostenpflichtiges, jaehrlich zu wiederholendes CASA-Sicherheitsaudit
// fuer die OAuth-Verifizierung. drive.file (nicht sensibel, kein Audit noetig)
// genuegt fuer Import/Upload/Sync-Check, solange die Datei-/Ordnerauswahl
// selbst ueber Googles eigenes Picker-Widget laeuft (siehe cloud::picker) statt
// ueber einen frei im gesamten Drive navigierenden eigenen Browser-Dialog -
// drive.file gewaehrt nur Zugriff auf Dateien/Ordner, die der Nutzer der App
// ueber den Picker explizit gezeigt oder die die App selbst erstellt hat.
const GOOGLE_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/drive.file",
    "https://www.googleapis.com/auth/userinfo.email",
];

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudAccountDto {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[tauri::command]
pub async fn connect_google_drive(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<CloudAccountDto> {
    let config = load_cloud_config(&default_config_path()).map_err(|e| e.to_string())?;

    let tokens = run_google_oauth_flow(
        &app,
        &config.google_client_id,
        &config.google_client_secret,
        GOOGLE_SCOPES,
    )
    .await
    .map_err(|e| e.to_string())?;

    let email = fetch_google_account_email(&tokens.access_token)
        .await
        .map_err(|e| e.to_string())?;

    let token_store = KeyringTokenStore;

    // Falls bereits ein anderes Google-Konto verbunden war (die
    // `UNIQUE(provider)`-Zeile wird beim Upsert unten ueberschrieben),
    // muss dessen Schluesselbund-Eintrag hier explizit geloescht werden -
    // sonst bleibt das alte Refresh-Token unerreichbar im OS-Schluesselbund
    // liegen. Eigener, kurzlebiger Lock-Scope, getrennt vom Upsert unten.
    let previous_label = {
        let conn = lock_db(&state)?;
        db::list_cloud_accounts(&conn)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|a| a.provider == "gdrive")
            .map(|a| a.account_label)
    };
    if let Some(old_label) = previous_label {
        if old_label != email {
            token_store
                .delete_async(&format!("gdrive:{old_label}"))
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    token_store
        .save_async(
            &format!("gdrive:{email}"),
            &StoredTokens {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    let connected_at = chrono::Utc::now().to_rfc3339();
    {
        let conn = lock_db(&state)?;
        db::upsert_cloud_account(&conn, "gdrive", &email, &connected_at).map_err(|e| e.to_string())?;
    }

    Ok(CloudAccountDto {
        id: "gdrive".to_string(),
        name: email,
        status: "connected".to_string(),
    })
}

#[tauri::command]
pub async fn disconnect_cloud_account(state: State<'_, AppState>, provider: String) -> CmdResult<()> {
    // Der DB-Lock wird bewusst vor dem Schluesselbund-Zugriff wieder
    // freigegeben: `token_store.delete_async` macht blockierendes D-Bus-IPC
    // zum Secret Service (kann auf einen Keyring-Entsperr-Dialog warten), das
    // ueber `spawn_blocking` vom Tokio-Worker-Thread ferngehalten wird. Ein
    // gehaltener MutexGuard wuerde in dieser Zeit jeden anderen Command mit
    // DB-Zugriff blockieren, daher bleibt der Lock-Scope trotzdem kurz.
    let account_label = {
        let conn = lock_db(&state)?;
        let accounts = db::list_cloud_accounts(&conn).map_err(|e| e.to_string())?;
        accounts
            .into_iter()
            .find(|a| a.provider == provider)
            .ok_or_else(|| "Konto nicht gefunden".to_string())?
            .account_label
    };

    let token_store = KeyringTokenStore;
    token_store
        .delete_async(&format!("{provider}:{account_label}"))
        .await
        .map_err(|e| e.to_string())?;

    let conn = lock_db(&state)?;
    db::set_cloud_account_status(&conn, &provider, "disconnected").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_cloud_accounts(state: State<AppState>) -> CmdResult<Vec<CloudAccountDto>> {
    let conn = lock_db(&state)?;
    let accounts = db::list_cloud_accounts(&conn).map_err(|e| e.to_string())?;
    Ok(accounts
        .into_iter()
        .map(|a| CloudAccountDto {
            id: a.provider,
            name: a.account_label,
            status: a.status,
        })
        .collect())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedItemDto {
    pub id: String,
    pub name: String,
    pub is_folder: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerResultDto {
    pub cancelled: bool,
    pub items: Vec<PickedItemDto>,
}

/// Zeigt Googles Picker-Widget (siehe `cloud::picker`) zur Datei- oder
/// Ordnerauswahl an. `mode` ist `"files"` (Mehrfachauswahl fuer den Import)
/// oder `"folder"` (Einzelauswahl als Upload-Zielordner) - jeder andere Wert
/// faellt auf `"files"` zurueck.
#[tauri::command]
pub async fn open_drive_picker(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mode: String,
) -> CmdResult<PickerResultDto> {
    let config = load_cloud_config(&default_config_path()).map_err(|e| e.to_string())?;
    let access_token = get_fresh_access_token(&state).await?;

    let picker_mode = match mode.as_str() {
        "folder" => PickerMode::Folder,
        _ => PickerMode::Files,
    };

    let outcome = picker::run_picker_flow(
        &app,
        &config.google_picker_api_key,
        &access_token,
        &config.google_cloud_project_number,
        picker_mode,
    )
        .await
        .map_err(|e| e.to_string())?;

    Ok(match outcome {
        PickerOutcome::Cancelled => PickerResultDto {
            cancelled: true,
            items: Vec::new(),
        },
        PickerOutcome::Picked(items) => PickerResultDto {
            cancelled: false,
            items: items
                .into_iter()
                .map(|i| PickedItemDto {
                    id: i.id,
                    name: i.name,
                    is_folder: i.is_folder,
                })
                .collect(),
        },
    })
}

/// Importiert die uebergebenen Google-Drive-Datei-IDs in den Katalog.
/// Gemeinsam genutzt von `import_from_cloud` (Nutzer waehlt Dateien direkt
/// per Picker) und `import_folder_from_cloud` (IDs kommen aus
/// `collect_cloud_files`, das einen per Picker gewaehlten Ordner rekursiv
/// nach Dateien durchsucht) - beide reichen ihre IDs unveraendert hier
/// durch, damit Download/Import-Logik nur einmal existiert.
///
/// Analog zu `import_many` (lokaler Import, `commands.rs`) ist diese
/// Funktion pro Datei fehlertolerant: ein einzelner Fehlschlag (Metadaten,
/// Download, nicht unterstuetztes Format, bereits importiert, Parse-/
/// Insert-Fehler) wird geloggt und ueberspringt nur diese eine Datei -
/// er bricht NICHT den ganzen Batch per `?` ab. Andernfalls wuerden bereits
/// erfolgreich importierte (und in SQLite committete) Dateien aus dem
/// zurueckgegebenen `Vec` verschwinden, weil die Funktion stattdessen
/// `Err` liefert und das Frontend seinen `.catch`-Pfad statt `mergeImported`
/// nimmt (Finding 3 der Abschluss-Review). Nur der einmalige Setup-Schritt
/// (Cache-Verzeichnis anlegen) darf den ganzen Command scheitern lassen.
async fn import_cloud_file_ids(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_ids: Vec<String>,
) -> CmdResult<ImportResultDto> {
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    let mut imported = Vec::new();
    let mut duplicate_count = 0i64;
    for file_id in file_ids {
        let metadata = match with_gdrive_provider(&state, {
            let file_id = file_id.clone();
            move |provider| {
                let file_id = file_id.clone();
                async move { provider.get_metadata(&file_id).await }
            }
        })
        .await
        {
            Ok(metadata) => metadata,
            Err(e) => {
                eprintln!("[cloud-import] Metadaten fehlgeschlagen fuer {file_id}: {e}");
                continue;
            }
        };

        // Finding 2 (Path-Traversal): `metadata.name` kommt von Drive und
        // ist nicht vertrauenswuerdig - Drive erlaubt "/" und ".." in
        // Dateinamen, die dort keine Pfad-Komponenten sind. Der Cache-Pfad
        // wird daher NIE aus dem Remote-Namen abgeleitet, sondern aus der
        // bereits bekannten, opaken und dateisystemsicheren `file_id` plus
        // einer validierten Erweiterung.
        let extension = match std::path::Path::new(&metadata.name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .filter(|e| e == "3mf" || e == "stl")
        {
            Some(ext) => ext,
            None => {
                eprintln!(
                    "[cloud-import] nicht unterstuetztes Dateiformat fuer {file_id} ({}): uebersprungen",
                    metadata.name
                );
                continue;
            }
        };
        let cache_path = cache_dir.join(format!("{file_id}.{extension}"));
        let cache_path_str = cache_path.to_string_lossy().to_string();

        // Finding 3 (Kollisionen): erneuter Import einer bereits
        // importierten Drive-Datei wuerde denselben Cache-Pfad treffen und
        // damit gegen den `UNIQUE(path)`-Constraint laufen. Vor dem
        // (teuren) Download pruefen und einfach ueberspringen - gleiches
        // Verhalten wie beim lokalen Import in `import_many`. Kurzer,
        // synchroner Lock-Scope ohne .await, analog zur bestehenden
        // Lock-Disziplin dieser Funktion.
        let already_imported = {
            let conn = lock_db(&state)?;
            match db::file_exists_by_path(&conn, &cache_path_str) {
                Ok(exists) => exists,
                Err(e) => {
                    eprintln!("[cloud-import] Duplikatpruefung fehlgeschlagen fuer {file_id}: {e}");
                    continue;
                }
            }
        };
        if already_imported {
            continue;
        }

        let data = match with_gdrive_provider(&state, {
            let file_id = file_id.clone();
            move |provider| {
                let file_id = file_id.clone();
                async move { provider.download(&file_id).await }
            }
        })
        .await
        {
            Ok(data) => data,
            Err(e) => {
                eprintln!("[cloud-import] Download fehlgeschlagen fuer {file_id}: {e}");
                continue;
            }
        };

        if let Err(e) = std::fs::write(&cache_path, &data) {
            eprintln!("[cloud-import] Schreiben in Cache fehlgeschlagen fuer {file_id}: {e}");
            continue;
        }

        // Hash erst NACH dem Schreiben moeglich (die Bytes liegen vorher nur
        // im RAM, nicht unter cache_path) - anders als beim lokalen Import in
        // import_many, wo die Quelldatei schon vor dem Import existiert.
        let content_hash = match compute_content_hash(&cache_path) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[cloud-import] Hash fehlgeschlagen fuer {file_id}: {e}");
                continue;
            }
        };
        let is_duplicate = {
            let conn = lock_db(&state)?;
            match db::file_exists_by_hash(&conn, &content_hash) {
                Ok(exists) => exists,
                Err(e) => {
                    eprintln!("[cloud-import] Duplikatpruefung (Hash) fehlgeschlagen fuer {file_id}: {e}");
                    continue;
                }
            }
        };
        if is_duplicate {
            duplicate_count += 1;
            continue;
        }

        // Ein durchgehender Lock-Scope genuegt hier: zwischen den beiden
        // DB-Aufrufen liegt kein .await, daher kein Konflikt mit der
        // "nie ueber .await halten"-Regel aus den Global Constraints.
        let dto = {
            let mut conn = lock_db(&state)?;
            let dto = match import_one(
                &mut conn,
                &cache_path,
                "gdrive",
                Some(file_id.clone()),
                Some(&metadata.name),
                Some(content_hash),
            ) {
                Ok(dto) => dto,
                Err(e) => {
                    eprintln!("[cloud-import] Import fehlgeschlagen fuer {file_id}: {e}");
                    continue;
                }
            };
            let id: i64 = match dto.id.parse() {
                Ok(id) => id,
                Err(_) => {
                    eprintln!("[cloud-import] ungueltige Datei-ID nach Import fuer {file_id}");
                    continue;
                }
            };
            if let Err(e) = db::set_file_modified_at(&conn, id, &metadata.modified_time) {
                eprintln!("[cloud-import] file_modified_at fehlgeschlagen fuer {file_id}: {e}");
            }
            dto
        };

        imported.push(dto);
    }

    Ok(ImportResultDto { imported, duplicate_count })
}

#[tauri::command]
pub async fn import_from_cloud(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_ids: Vec<String>,
) -> CmdResult<ImportResultDto> {
    import_cloud_file_ids(app, state, file_ids).await
}

/// Sammelt iterativ (Warteschlange statt echter Rekursion - async fn kann
/// sich in Rust nicht ohne `Box::pin`-Indirektion selbst aufrufen) alle
/// Datei-IDs unterhalb von `root_folder_id`, inklusive aller Unterordner.
/// Ein fehlschlagender `list_folder`-Aufruf bricht die gesamte Sammlung ab
/// (anders als der anschliessende Download/Import in
/// `import_cloud_file_ids`, der bewusst pro Datei fehlertolerant bleibt):
/// ein Teilergebnis waere hier fuer den Nutzer nicht als unvollstaendig
/// erkennbar, ein klarer Fehler mit Wiederholungsmoeglichkeit ist die
/// sicherere Wahl.
async fn collect_cloud_files(state: &State<'_, AppState>, root_folder_id: &str) -> CmdResult<Vec<String>> {
    let mut file_ids = Vec::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    queue.push_back(root_folder_id.to_string());

    while let Some(folder_id) = queue.pop_front() {
        let entries = with_gdrive_provider(state, {
            let folder_id = folder_id.clone();
            move |provider| {
                let folder_id = folder_id.clone();
                async move { provider.list_folder(Some(&folder_id)).await }
            }
        })
        .await?;

        let (subfolder_ids, mut found_file_ids) = partition_cloud_entries(entries);
        queue.extend(subfolder_ids);
        file_ids.append(&mut found_file_ids);
    }

    Ok(file_ids)
}

/// Teilt eine Drive-Ordner-Auflistung in Unterordner-IDs (zum
/// Weiterdurchsuchen in `collect_cloud_files`) und Datei-IDs (zum
/// Importieren) auf. Eigene, von der Tauri-`State`/Auth-Infrastruktur
/// entkoppelte Funktion, damit die Verzweigungslogik ohne Google-Konto
/// testbar ist.
fn partition_cloud_entries(entries: Vec<CloudEntry>) -> (Vec<String>, Vec<String>) {
    let mut folder_ids = Vec::new();
    let mut file_ids = Vec::new();
    for entry in entries {
        if entry.is_folder {
            folder_ids.push(entry.id);
        } else {
            file_ids.push(entry.id);
        }
    }
    (folder_ids, file_ids)
}

/// Importiert alle Dateien eines per Picker gewaehlten Google-Drive-Ordners,
/// inklusive aller Unterordner (siehe `collect_cloud_files`).
#[tauri::command]
pub async fn import_folder_from_cloud(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    folder_id: String,
) -> CmdResult<ImportResultDto> {
    let file_ids = collect_cloud_files(&state, &folder_id).await?;
    import_cloud_file_ids(app, state, file_ids).await
}

#[tauri::command]
pub async fn check_cloud_sync_status(state: State<'_, AppState>, file_id: String) -> CmdResult<String> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;

    let (cloud_id, last_known_modified, current_status) = {
        let conn = lock_db(&state)?;
        let file = db::get_file(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "file not found".to_string())?;
        (file.cloud_id, file.file_modified_at, file.sync_status)
    };

    let Some(cloud_id) = cloud_id else {
        return Ok(current_status);
    };

    let metadata = with_gdrive_provider(&state, {
        let cloud_id = cloud_id.clone();
        move |provider| {
            let cloud_id = cloud_id.clone();
            async move { provider.get_metadata(&cloud_id).await }
        }
    })
    .await?;

    let new_status = if last_known_modified.as_deref() == Some(metadata.modified_time.as_str()) {
        "synced"
    } else {
        "outdated"
    };

    {
        let conn = lock_db(&state)?;
        db::set_file_sync_status(&conn, id, new_status).map_err(|e| e.to_string())?;
    }

    Ok(new_status.to_string())
}

/// Laedt eine bisher rein lokale Katalogdatei zu Google Drive hoch und
/// verknuepft sie danach mit dem entstandenen Drive-Eintrag. Bereits mit
/// einem Cloud-Konto verknuepfte Dateien (origin != "local") werden
/// zurueckgewiesen - ein erneuter Upload/Ueberschreiben-Fluss ist nicht
/// Teil dieser MVP-Funktion; Aktualisierungen laufen weiterhin ueber
/// check_cloud_sync_status. `folder_id` ist die Drive-Ordner-ID aus dem
/// Zielordner-Dialog im Frontend (None = Drive-Wurzelverzeichnis).
#[tauri::command]
pub async fn upload_file_to_cloud(
    state: State<'_, AppState>,
    file_id: String,
    folder_id: Option<String>,
) -> CmdResult<ModelFileDto> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;

    let file = {
        let conn = lock_db(&state)?;
        db::get_file(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Datei nicht gefunden".to_string())?
    };

    if file.origin != "local" {
        return Err("Datei ist bereits mit einem Cloud-Konto verknuepft".to_string());
    }

    let data = std::fs::read(&file.path).map_err(|e| e.to_string())?;

    let entry = with_gdrive_provider(&state, {
        let file_name = file.name.clone();
        let folder_id = folder_id.clone();
        move |provider| {
            let file_name = file_name.clone();
            let folder_id = folder_id.clone();
            let data = data.clone();
            async move { provider.upload(folder_id.as_deref(), &file_name, &data).await }
        }
    })
    .await?;

    {
        let conn = lock_db(&state)?;
        db::set_file_cloud_link(&conn, id, "gdrive", &entry.id, "synced", &entry.modified_time)
            .map_err(|e| e.to_string())?;
    }

    let updated = {
        let conn = lock_db(&state)?;
        db::get_file(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Datei nach Upload nicht gefunden".to_string())?
    };
    Ok(to_dto(updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, is_folder: bool) -> CloudEntry {
        CloudEntry {
            id: id.to_string(),
            name: id.to_string(),
            is_folder,
            modified_time: "2026-09-11T12:00:00Z".to_string(),
            size_bytes: if is_folder { None } else { Some(1024) },
        }
    }

    #[test]
    fn partition_cloud_entries_splits_folders_from_files() {
        let entries = vec![
            entry("folder-a", true),
            entry("file-1", false),
            entry("folder-b", true),
            entry("file-2", false),
        ];

        let (folder_ids, file_ids) = partition_cloud_entries(entries);

        assert_eq!(folder_ids, vec!["folder-a".to_string(), "folder-b".to_string()]);
        assert_eq!(file_ids, vec!["file-1".to_string(), "file-2".to_string()]);
    }

    #[test]
    fn partition_cloud_entries_handles_only_files_or_only_folders() {
        let (folder_ids, file_ids) = partition_cloud_entries(vec![entry("file-1", false)]);
        assert!(folder_ids.is_empty());
        assert_eq!(file_ids, vec!["file-1".to_string()]);

        let (folder_ids, file_ids) = partition_cloud_entries(vec![entry("folder-a", true)]);
        assert_eq!(folder_ids, vec!["folder-a".to_string()]);
        assert!(file_ids.is_empty());
    }

    #[test]
    fn partition_cloud_entries_returns_empty_for_empty_input() {
        let (folder_ids, file_ids) = partition_cloud_entries(vec![]);
        assert!(folder_ids.is_empty());
        assert!(file_ids.is_empty());
    }
}
