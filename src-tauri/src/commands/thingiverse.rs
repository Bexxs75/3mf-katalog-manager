//! Tauri commands of the Thingiverse area. The key stays in Rust: the interface gets
//! only whether one exists and a mask with its last four characters. Network work
//! runs in `spawn_blocking`; the database is locked only to read the settings and to
//! count a request, never while a request is on its way.

use super::*;
use serde::Serialize;
use std::sync::Arc;
use tauri::Manager;

use crate::db::thingiverse as store;
use katalog_core::thingiverse::{
    Gate, ImageSize, Service, ThingDetailDto, ThingiverseSearchDto, Token, TvError, DAILY_LIMIT,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingiverseSettingsDto {
    pub enabled: bool,
    pub has_token: bool,
    pub token_mask: Option<String>,
    pub usage_today: u32,
    pub daily_limit: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingiverseTokenTestDto {
    pub ok: bool,
    pub user_name: Option<String>,
}

fn tv_error(e: TvError) -> CmdError {
    CmdError::coded_with(e.code, e.params())
}

fn settings_with_conn(conn: &Connection) -> CmdResult<ThingiverseSettingsDto> {
    let token = store::get_token(conn)?;
    Ok(ThingiverseSettingsDto {
        enabled: store::get_enabled(conn)?,
        has_token: token.is_some(),
        token_mask: token.map(|t| t.mask()),
        usage_today: store::usage_today(conn, &store::local_today())?,
        daily_limit: DAILY_LIMIT,
    })
}

/// The stored key, or the reason the area cannot be used.
fn ready_with_conn(conn: &Connection) -> CmdResult<Token> {
    if !store::get_enabled(conn)? {
        return Err(CmdError::coded(ErrorCode::ThingiverseDisabled));
    }
    store::get_token(conn)?.ok_or_else(|| CmdError::coded(ErrorCode::ThingiverseNoToken))
}

fn enabled_with_conn(conn: &Connection) -> CmdResult<()> {
    if store::get_enabled(conn)? {
        Ok(())
    } else {
        Err(CmdError::coded(ErrorCode::ThingiverseDisabled))
    }
}

/// Counts a request for today right before it is sent; refuses it at the day's cap.
fn count_request(conn: &Connection, today: &str) -> Result<(), TvError> {
    match store::bump_usage(conn, today, DAILY_LIMIT) {
        Ok(store::Usage::Counted(_)) => Ok(()),
        Ok(store::Usage::AtLimit) => Err(TvError::new(ErrorCode::ThingiverseDailyLimit)),
        // Cannot count: better not to send than to lose track of the day's limit.
        Err(_) => Err(TvError::new(ErrorCode::ThingiverseBadResponse)),
    }
}

/// Runs `job` on a worker thread with the stored key, the service and a gate that
/// counts the request in the daily counter.
async fn with_key<T, F>(app: tauri::AppHandle, job: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Service, &Token, Gate) -> Result<T, TvError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let service = app.state::<Arc<Service>>();
        let token = {
            let conn = state.db.lock().map_err(|_| CmdError::from("database lock poisoned"))?;
            ready_with_conn(&conn)?
        };
        let gate = || {
            let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
            count_request(&conn, &store::local_today())
        };
        job(&service, &token, &gate).map_err(tv_error)
    })
    .await
    .map_err(|e| CmdError::from(e.to_string()))?
}

#[tauri::command]
pub fn get_thingiverse_settings(state: State<AppState>) -> CmdResult<ThingiverseSettingsDto> {
    let conn = lock_db(&state)?;
    settings_with_conn(&conn)
}

/// Switching off also drops everything held in memory.
#[tauri::command]
pub fn set_thingiverse_enabled(state: State<AppState>, service: State<'_, Arc<Service>>, enabled: bool) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    store::set_enabled(&conn, enabled)?;
    drop(conn);
    if !enabled {
        service.clear();
    }
    Ok(())
}

/// `Some(key)`: checks this key and, if it works, stores it. `None`: checks the stored one.
#[tauri::command]
pub async fn test_thingiverse_token(app: tauri::AppHandle, token: Option<String>) -> CmdResult<ThingiverseTokenTestDto> {
    let candidate = match token {
        Some(raw) => Some(Token::parse(&raw).map_err(|_| CmdError::coded(ErrorCode::ThingiverseTokenFormat))?),
        None => None,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let service = app.state::<Arc<Service>>();
        let key = {
            let conn = state.db.lock().map_err(|_| CmdError::from("database lock poisoned"))?;
            match &candidate {
                Some(_) => {
                    enabled_with_conn(&conn)?;
                    None
                }
                None => Some(ready_with_conn(&conn)?),
            }
        };
        let gate = || {
            let conn = state.db.lock().unwrap_or_else(|p| p.into_inner());
            count_request(&conn, &store::local_today())
        };
        let name = match (&candidate, &key) {
            (Some(token), _) | (None, Some(token)) => service.test_token(token, &gate).map_err(tv_error)?,
            (None, None) => return Err(CmdError::coded(ErrorCode::ThingiverseNoToken)),
        };
        if let Some(token) = &candidate {
            let conn = state.db.lock().map_err(|_| CmdError::from("database lock poisoned"))?;
            store::set_token(&conn, token)?;
            drop(conn);
            // Results of the old key must not outlive the change.
            service.clear();
        }
        Ok(ThingiverseTokenTestDto { ok: true, user_name: Some(name) })
    })
    .await
    .map_err(|e| CmdError::from(e.to_string()))?
}

/// Removes the key from this computer only; at Thingiverse it stays valid until revoked there.
#[tauri::command]
pub fn remove_thingiverse_token(state: State<AppState>, service: State<'_, Arc<Service>>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    store::clear_token(&conn)?;
    drop(conn);
    service.clear();
    Ok(())
}

#[tauri::command]
pub async fn thingiverse_search(app: tauri::AppHandle, query: String, page: u32) -> CmdResult<ThingiverseSearchDto> {
    with_key(app, move |service, token, gate| service.search(token, &query, page, gate)).await
}

#[tauri::command]
pub async fn thingiverse_thing(app: tauri::AppHandle, thing_id: u64) -> CmdResult<ThingDetailDto> {
    with_key(app, move |service, token, gate| service.thing(token, thing_id, gate)).await
}

/// Preview image as a `data:` address; `None` when the thing has none or was not in a result.
#[tauri::command]
pub async fn thingiverse_image(app: tauri::AppHandle, thing_id: u64, size: String) -> CmdResult<Option<String>> {
    let size = match size.as_str() {
        "thumb" => ImageSize::Thumb,
        "large" => ImageSize::Large,
        _ => return Err(CmdError::coded(ErrorCode::ReqInvalidArguments)),
    };
    with_key(app, move |service, _token, gate| service.image(thing_id, size, gate)).await
}

/// Opens the thing's page in the browser; in a container the interface shows the address instead.
#[tauri::command]
pub fn open_thingiverse_thing(
    state: State<AppState>,
    service: State<'_, Arc<Service>>,
    environment: State<'_, RuntimeEnvironment>,
    thing_id: u64,
) -> CmdResult<()> {
    {
        let conn = lock_db(&state)?;
        enabled_with_conn(&conn)?;
    }
    open_url(&service.page_url(thing_id), *environment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use katalog_core::db::connect_in_memory;

    const KEY: &str = "abcd1234efgh5678a3f9";

    #[test]
    fn the_settings_never_contain_the_key() {
        let conn = connect_in_memory().unwrap();
        let fresh = settings_with_conn(&conn).unwrap();
        assert_eq!((fresh.enabled, fresh.has_token, fresh.token_mask.is_none(), fresh.usage_today), (false, false, true, 0));
        store::set_enabled(&conn, true).unwrap();
        store::set_token(&conn, &Token::parse(KEY).unwrap()).unwrap();
        let dto = settings_with_conn(&conn).unwrap();
        assert!(dto.has_token);
        let json = serde_json::to_string(&dto).unwrap();
        assert!(!json.contains(KEY) && !json.contains("abcd1234"), "{json}");
        assert_eq!(dto.token_mask.as_deref(), Some("\u{2022}\u{2022}\u{2022}\u{2022}a3f9"));
        assert_eq!(dto.daily_limit, 1000);
    }

    #[test]
    fn commands_refuse_when_the_area_is_off_or_the_key_is_missing() {
        let conn = connect_in_memory().unwrap();
        assert!(ready_with_conn(&conn).err().expect("refused").is(ErrorCode::ThingiverseDisabled, &[]));
        assert!(enabled_with_conn(&conn).unwrap_err().is(ErrorCode::ThingiverseDisabled, &[]));
        store::set_enabled(&conn, true).unwrap();
        assert!(ready_with_conn(&conn).err().expect("refused").is(ErrorCode::ThingiverseNoToken, &[]));
        store::set_token(&conn, &Token::parse(KEY).unwrap()).unwrap();
        assert!(ready_with_conn(&conn).is_ok());
    }

    #[test]
    fn the_gate_counts_and_stops_at_the_daily_cap() {
        let conn = connect_in_memory().unwrap();
        for _ in 0..DAILY_LIMIT {
            count_request(&conn, "2026-11-04").unwrap();
        }
        assert_eq!(count_request(&conn, "2026-11-04").unwrap_err().code, ErrorCode::ThingiverseDailyLimit);
        assert_eq!(store::usage_today(&conn, "2026-11-04").unwrap(), DAILY_LIMIT);
        // Midnight lifts the stop.
        count_request(&conn, "2026-11-05").unwrap();
    }

    #[test]
    fn a_rate_limit_error_carries_only_the_seconds() {
        let e = tv_error(TvError::rate_limited(42));
        assert!(e.is(ErrorCode::ThingiverseRateLimited, &["42"]));
    }

    /// No generic command may hand out `app_settings` rows: the key sits in one of them.
    #[test]
    fn no_command_reads_settings_by_name() {
        let lib = include_str!("../lib.rs");
        let handler = lib.split("generate_handler!").nth(1).unwrap().split(']').next().unwrap();
        for line in handler.lines().map(str::trim) {
            let name = line.trim_end_matches(',').rsplit("::").next().unwrap_or("");
            let generic = ["get_setting", "read_setting", "list_settings", "get_app_setting", "get_settings", "dump_settings"];
            assert!(!generic.contains(&name), "{name} could expose the key");
        }
        assert!(handler.contains("get_thingiverse_settings"), "the handler list was found");
    }
}
