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
        let metadata = with_gdrive_provider(&state, {
            let file_id = file_id.clone();
            move |provider| {
                let file_id = file_id.clone();
                async move { provider.get_metadata(&file_id).await }
            }
        })
        .await?;

        let data = with_gdrive_provider(&state, {
            let file_id = file_id.clone();
            move |provider| {
                let file_id = file_id.clone();
                async move { provider.download(&file_id).await }
            }
        })
        .await?;

        let cache_path = cache_dir.join(&metadata.name);
        std::fs::write(&cache_path, &data).map_err(|e| e.to_string())?;

        // Ein durchgehender Lock-Scope genuegt hier: zwischen den beiden
        // DB-Aufrufen liegt kein .await, daher kein Konflikt mit der
        // "nie ueber .await halten"-Regel aus den Global Constraints.
        let dto = {
            let mut conn = lock_db(&state)?;
            let dto = import_one(&mut conn, &cache_path, "gdrive", Some(file_id))?;
            let id: i64 = dto.id.parse().map_err(|_| "invalid file id".to_string())?;
            db::set_file_modified_at(&conn, id, &metadata.modified_time).map_err(|e| e.to_string())?;
            dto
        };

        imported.push(dto);
    }

    Ok(imported)
}
