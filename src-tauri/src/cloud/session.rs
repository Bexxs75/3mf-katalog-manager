use tauri::State;

use crate::cloud::config::{default_config_path, load_cloud_config};
use crate::cloud::gdrive::GoogleDriveProvider;
use crate::cloud::oauth::refresh_access_token;
use crate::cloud::provider::CloudError;
use crate::cloud::tokens::{KeyringTokenStore, StoredTokens, TokenStore};
use crate::commands::{lock_db, AppState};
use crate::db;

type CmdResult<T> = Result<T, String>;

/// Fuehrt `operation` mit einem gueltigen Google-Drive-Access-Token aus.
/// Schlaegt der erste Versuch mit einem Auth-Fehler fehl (Token abgelaufen),
/// wird per Refresh-Token ein neuer Access-Token geholt, im Schluesselbund
/// gespeichert und der Versuch genau einmal wiederholt. Schlaegt auch der
/// Refresh fehl, wechselt der Konto-Status auf 'error'.
///
/// Haelt an keiner Stelle einen DB-MutexGuard ueber einen .await-Punkt
/// hinweg: jede DB-Operation laeuft in ihrem eigenen kurzen Block.
pub async fn with_gdrive_provider<F, Fut, T>(state: &State<'_, AppState>, operation: F) -> CmdResult<T>
where
    F: Fn(GoogleDriveProvider) -> Fut,
    Fut: std::future::Future<Output = Result<T, CloudError>>,
{
    let account_label = {
        let conn = lock_db(state)?;
        let accounts = db::list_cloud_accounts(&conn).map_err(|e| e.to_string())?;
        accounts
            .into_iter()
            .find(|a| a.provider == "gdrive" && a.status != "disconnected")
            .map(|a| a.account_label)
            .ok_or_else(|| "Kein verbundenes Google-Drive-Konto".to_string())?
    };

    let account_key = format!("gdrive:{account_label}");
    let token_store = KeyringTokenStore;
    let stored = token_store
        .load(&account_key)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Kein Token im Schluesselbund gefunden - bitte erneut verbinden".to_string())?;

    let provider = GoogleDriveProvider::new(stored.access_token);
    match operation(provider).await {
        Ok(value) => Ok(value),
        Err(CloudError::Auth(_)) => {
            match refresh_and_store(&account_key, stored.refresh_token).await {
                Ok(new_access_token) => operation(GoogleDriveProvider::new(new_access_token))
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => {
                    let conn = lock_db(state)?;
                    let _ = db::set_cloud_account_status(&conn, "gdrive", "error");
                    Err(e.to_string())
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

async fn refresh_and_store(
    account_key: &str,
    refresh_token: Option<String>,
) -> Result<String, CloudError> {
    let refresh_token = refresh_token.ok_or_else(|| {
        CloudError::Auth("Kein Refresh-Token vorhanden - bitte erneut verbinden".to_string())
    })?;
    let config = load_cloud_config(&default_config_path()).map_err(|e| CloudError::Auth(e.to_string()))?;

    let new_tokens = refresh_access_token(
        &config.google_client_id,
        &config.google_client_secret,
        &refresh_token,
    )
    .await?;

    let token_store = KeyringTokenStore;
    token_store
        .save(
            account_key,
            &StoredTokens {
                access_token: new_tokens.access_token.clone(),
                refresh_token: new_tokens.refresh_token.or(Some(refresh_token)),
            },
        )
        .map_err(CloudError::Auth)?;

    Ok(new_tokens.access_token)
}
