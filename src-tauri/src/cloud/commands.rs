use tauri::{Manager, State};

use crate::cloud::config::{default_config_path, load_cloud_config};
use crate::cloud::gdrive::fetch_google_account_email;
use crate::cloud::oauth::run_google_oauth_flow;
use crate::cloud::provider::{CloudEntry, StorageProvider};
use crate::cloud::session::with_gdrive_provider;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{import_one, lock_db, AppState, ModelFileDto};
use crate::db;

type CmdResult<T> = Result<T, String>;

const GOOGLE_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/drive.readonly",
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
                .delete(&format!("gdrive:{old_label}"))
                .map_err(|e| e.to_string())?;
        }
    }

    token_store
        .save(
            &format!("gdrive:{email}"),
            &StoredTokens {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
            },
        )
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
pub fn disconnect_cloud_account(state: State<AppState>, provider: String) -> CmdResult<()> {
    // Der DB-Lock wird bewusst vor dem Schluesselbund-Zugriff wieder
    // freigegeben: `token_store.delete` macht blockierendes D-Bus-IPC zum
    // Secret Service (kann auf einen Keyring-Entsperr-Dialog warten), und
    // dieser Command laeuft synchron auf Tauris Main/IPC-Thread. Ein
    // gehaltener MutexGuard wuerde in dieser Zeit jeden anderen Command mit
    // DB-Zugriff blockieren.
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
        .delete(&format!("{provider}:{account_label}"))
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
pub struct CloudEntryDto {
    pub id: String,
    pub name: String,
    pub is_folder: bool,
    pub modified_time: String,
    pub size_bytes: Option<i64>,
}

impl From<CloudEntry> for CloudEntryDto {
    fn from(e: CloudEntry) -> Self {
        CloudEntryDto {
            id: e.id,
            name: e.name,
            is_folder: e.is_folder,
            modified_time: e.modified_time,
            size_bytes: e.size_bytes,
        }
    }
}

#[tauri::command]
pub async fn browse_cloud_folder(
    state: State<'_, AppState>,
    folder_id: Option<String>,
) -> CmdResult<Vec<CloudEntryDto>> {
    let entries = with_gdrive_provider(&state, |provider| {
        let folder_id = folder_id.clone();
        async move { provider.list_folder(folder_id.as_deref()).await }
    })
    .await?;

    Ok(entries.into_iter().map(CloudEntryDto::from).collect())
}

/// Importiert die uebergebenen Google-Drive-Datei-IDs in den Katalog.
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
#[tauri::command]
pub async fn import_from_cloud(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_ids: Vec<String>,
) -> CmdResult<Vec<ModelFileDto>> {
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    let mut imported = Vec::new();
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

    Ok(imported)
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
