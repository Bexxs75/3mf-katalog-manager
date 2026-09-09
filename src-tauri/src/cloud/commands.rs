use tauri::State;

use crate::cloud::config::{default_config_path, load_cloud_config};
use crate::cloud::gdrive::fetch_google_account_email;
use crate::cloud::oauth::run_google_oauth_flow;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{lock_db, AppState};
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
    let conn = lock_db(&state)?;
    let accounts = db::list_cloud_accounts(&conn).map_err(|e| e.to_string())?;
    let account = accounts
        .into_iter()
        .find(|a| a.provider == provider)
        .ok_or_else(|| "Konto nicht gefunden".to_string())?;

    let token_store = KeyringTokenStore;
    token_store
        .delete(&format!("{provider}:{}", account.account_label))
        .map_err(|e| e.to_string())?;

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
