//! Tauri commands for the "detailed log" switch (see `diagnostics::verbose`)
//! and for exporting a log excerpt for a bug report.

use super::*;
use crate::db::printer_link::{get_setting, set_setting};
use crate::diagnostics::{anonymize, export, form_url, verbose};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerboseLoggingDto {
    pub enabled: bool,
    pub until_ms: Option<i64>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Reads the switch, turns it off once it has expired and sets the log level
/// accordingly. Called at startup, hourly and by the commands.
pub(crate) fn apply_verbose_state(conn: &Connection) -> VerboseLoggingDto {
    let until = verbose::parse_until(get_setting(conn, verbose::SETTING_KEY).ok().flatten());
    let active = verbose::is_active(until, now_ms());
    if !active && until.is_some() {
        let _ = set_setting(conn, verbose::SETTING_KEY, "");
        log::info!(target: "diagnose", "Ausführliches Protokoll automatisch ausgeschaltet");
    }
    log::set_max_level(verbose::level(active));
    VerboseLoggingDto { enabled: active, until_ms: if active { until } else { None } }
}

#[tauri::command]
pub fn get_verbose_logging(state: State<AppState>) -> CmdResult<VerboseLoggingDto> {
    let conn = lock_db(&state)?;
    Ok(apply_verbose_state(&conn))
}

#[tauri::command]
pub fn set_verbose_logging(state: State<AppState>, enabled: bool) -> CmdResult<VerboseLoggingDto> {
    let conn = lock_db(&state)?;
    let value = if enabled { verbose::until_from(now_ms()).to_string() } else { String::new() };
    set_setting(&conn, verbose::SETTING_KEY, &value).map_err(|e| e.to_string())?;
    log::info!(target: "diagnose", "Ausführliches Protokoll {}", if enabled { "eingeschaltet" } else { "ausgeschaltet" });
    Ok(apply_verbose_state(&conn))
}

/// Pure ordering logic for the preview cache, kept separate from the Tauri
/// command so the "newer wins, save only for the id shown" rule can be unit
/// tested without going through `State`/`AppHandle`.
#[derive(Default)]
struct PreviewCache {
    next_id: u64,
    stored: Option<(u64, String)>,
}

impl PreviewCache {
    fn reserve_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Ignores results from a request older than the one already cached, so
    /// a slow, superseded `preview_log_export` call can't clobber what the
    /// user is currently looking at.
    fn store_if_newer(&mut self, id: u64, text: String) {
        if self.stored.as_ref().is_none_or(|(cur, _)| id > *cur) {
            self.stored = Some((id, text));
        }
    }

    /// Only returns the text if `id` is exactly the currently cached one, so
    /// saving always writes exactly what was shown, never an older or
    /// already-superseded preview.
    fn take_for_save(&self, id: u64) -> Option<String> {
        self.stored.as_ref().filter(|(cur, _)| *cur == id).map(|(_, text)| text.clone())
    }
}

#[derive(Default)]
pub struct DiagnosticsState {
    cache: Mutex<PreviewCache>,
}

impl DiagnosticsState {
    fn reserve_preview_id(&self) -> u64 {
        self.cache.lock().unwrap().reserve_id()
    }

    fn store_preview(&self, id: u64, text: String) {
        self.cache.lock().unwrap().store_if_newer(id, text);
    }

    fn preview_for_save(&self, id: u64) -> Option<String> {
        self.cache.lock().unwrap().take_for_save(id)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BugReportInfoDto {
    pub version: String,
    pub os: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPreviewDto {
    /// Identifies exactly this computation; the frontend must pass it back
    /// to `save_log_export` unchanged, and only the request holding the
    /// newest id ever gets cached (see `PreviewCache`).
    pub id: u64,
    pub segments: Vec<anonymize::Segment>,
    pub contains_debug: bool,
    pub replace_file_names: bool,
    pub empty: bool,
}

#[tauri::command]
pub fn get_bug_report_info() -> BugReportInfoDto {
    BugReportInfoDto {
        version: form_url::core_version(env!("CARGO_PKG_VERSION")).to_string(),
        os: form_url::current_os().to_string(),
    }
}

fn anonymize_context(app: &tauri::AppHandle, conn: &Connection, replace_file_names: bool) -> anonymize::Context {
    use tauri::Manager;
    let catalog_roots = db::list_folders(conn)
        .map(|fs| fs.into_iter().filter(|f| f.parent_id.is_none()).map(|f| f.path).collect())
        .unwrap_or_default();
    let printer_addresses = crate::db::printer_link::list_connections(conn)
        .map(|cs| cs.into_iter().map(|c| c.address).collect())
        .unwrap_or_default();
    anonymize::Context {
        home: app.path().home_dir().ok().map(|p| p.to_string_lossy().into_owned()),
        user: std::env::var("USER").or_else(|_| std::env::var("USERNAME")).ok(),
        host: gethostname::gethostname().into_string().ok(),
        catalog_roots,
        printer_addresses,
        replace_file_names,
    }
}

#[tauri::command]
pub async fn preview_log_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    diag: State<'_, DiagnosticsState>,
    replace_file_names: Option<bool>,
) -> CmdResult<LogPreviewDto> {
    use tauri::Manager;
    // Reserved before the (comparatively slow) read+anonymize below; this
    // only makes a *newer* id likely for a *later* call - it's not a
    // guarantee. Correctness doesn't depend on that ordering: it comes from
    // `store_if_newer`/`take_for_save` requiring an exact id match, so a
    // save can never write anything but the id it was actually given.
    let id = diag.reserve_preview_id();
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    // The file read and the anonymizer's regex passes run on up to 1 MB of
    // text; run them off the async runtime's own thread so the UI thread
    // stays responsive.
    let raw = tauri::async_runtime::spawn_blocking(move || export::collect_tail(&export::read_logs(&dir), export::EXPORT_LIMIT))
        .await
        .map_err(|e| e.to_string())?;
    let contains_debug = export::contains_debug(&raw);
    let empty = raw.trim().is_empty();
    let replace = replace_file_names.unwrap_or(contains_debug);
    let ctx = {
        let conn = lock_db(&state)?;
        anonymize_context(&app, &conn, replace)
    };
    let segments = tauri::async_runtime::spawn_blocking(move || anonymize::anonymize(&raw, &ctx))
        .await
        .map_err(|e| e.to_string())?;
    diag.store_preview(id, anonymize::to_text(&segments));
    Ok(LogPreviewDto { id, empty, segments, contains_debug, replace_file_names: replace })
}

/// Picks the export destination: the OS download folder, or the home folder if
/// that one isn't configured (some minimal Linux setups have no XDG user dirs).
fn choose_export_dir(download_dir: Option<std::path::PathBuf>, home_dir: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    download_dir.or(home_dir)
}

#[tauri::command]
pub fn save_log_export(app: tauri::AppHandle, diag: State<DiagnosticsState>, id: u64) -> CmdResult<String> {
    use tauri::Manager;
    let text = diag
        .preview_for_save(id)
        .ok_or_else(|| CmdError::expected("Die Vorschau ist veraltet, bitte erneut anzeigen"))?;
    let dir = choose_export_dir(app.path().download_dir().ok(), app.path().home_dir().ok())
        .ok_or_else(|| "kein Download- oder Benutzerordner verfügbar".to_string())?;
    // The folder may not exist yet (e.g. a download dir configured but never created).
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let path = export::write_new_export(&dir, &date, &text).map_err(|e| e.to_string())?;
    log::info!(target: "diagnose", "Logdatei für Fehlerbericht gespeichert");
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn open_log_folder(app: tauri::AppHandle) -> CmdResult<()> {
    use tauri::Manager;
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    super::security::open_external(&dir.to_string_lossy())
}

#[tauri::command]
pub fn open_bug_report_form(lang: String, with_log: bool) -> CmdResult<()> {
    let url = form_url::bug_report_url(&lang, env!("CARGO_PKG_VERSION"), form_url::current_os(), with_log);
    super::security::open_external(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_dir_falls_back_to_home_when_download_dir_is_unavailable() {
        let home = std::path::PathBuf::from("/home/someone");
        assert_eq!(choose_export_dir(None, Some(home.clone())), Some(home));
        assert_eq!(choose_export_dir(None, None), None);
        let download = std::path::PathBuf::from("/home/someone/Downloads");
        assert_eq!(choose_export_dir(Some(download.clone()), Some(std::path::PathBuf::from("/home/someone"))), Some(download));
    }

    #[test]
    fn expired_setting_is_cleared_and_reported_off() {
        let conn = crate::db::connect_in_memory().unwrap();
        set_setting(&conn, verbose::SETTING_KEY, "1").unwrap(); // 1970 → long expired
        let dto = apply_verbose_state(&conn);
        assert_eq!(dto, VerboseLoggingDto { enabled: false, until_ms: None });
        assert_eq!(get_setting(&conn, verbose::SETTING_KEY).unwrap().as_deref(), Some(""));
    }

    #[test]
    fn preview_cache_ids_increase_and_start_empty() {
        let mut cache = PreviewCache::default();
        assert_eq!(cache.reserve_id(), 1);
        assert_eq!(cache.reserve_id(), 2);
        assert_eq!(cache.take_for_save(1), None);
    }

    #[test]
    fn newer_preview_replaces_older() {
        let mut cache = PreviewCache::default();
        cache.store_if_newer(1, "old".into());
        cache.store_if_newer(2, "new".into());
        assert_eq!(cache.take_for_save(2), Some("new".into()));
    }

    #[test]
    fn older_result_finishing_later_does_not_replace_a_newer_one() {
        let mut cache = PreviewCache::default();
        // id 2's request started after id 1's, but its computation (e.g. a
        // faster "no file-name replacement" pass) finishes first.
        cache.store_if_newer(2, "new".into());
        cache.store_if_newer(1, "old".into());
        assert_eq!(cache.take_for_save(2), Some("new".into()));
        // The stale id must not be servable either, even though it was
        // computed - the user is no longer looking at it.
        assert_eq!(cache.take_for_save(1), None);
    }

    #[test]
    fn save_with_a_stale_id_fails() {
        let mut cache = PreviewCache::default();
        cache.store_if_newer(1, "first".into());
        cache.store_if_newer(2, "second".into());
        // id 1 was superseded by id 2 before it could be saved.
        assert_eq!(cache.take_for_save(1), None);
        assert_eq!(cache.take_for_save(2), Some("second".into()));
    }
}
