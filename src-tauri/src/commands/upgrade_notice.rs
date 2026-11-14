//! The notice shown once after an update or a manual install changed the catalog
//! database: what was backed up, where, and how to go back. It only informs; the
//! app never swaps its own database back while it runs.

use super::*;
use crate::updater::{self, LastUpdateInfo};
use katalog_core::backup::parse_backup_name;
use katalog_core::db::printer_link::{get_setting, set_setting};
use katalog_core::db::{MigrationReport, BACKUP_DIR_NAME};
use katalog_core::schema_versions::app_version_for_schema;

/// Setting that remembers which (app version, schema) the user already read the
/// notice for, so it does not come back on every start.
const SEEN_KEY: &str = "upgrade_notice_seen";

/// What the migration at this start did, kept until the interface asks for it.
pub struct UpgradeReport(pub Option<MigrationReport>);
pub struct PreviousAppVersion(pub Option<String>);

const RUN_VERSION_KEY: &str = "app_run_versions";

#[derive(Serialize, serde::Deserialize)]
struct RunVersions {
    version: String,
    previous: Option<String>,
}

fn plausible_version(value: &str) -> bool {
    let (base, suffix) = value.split_once('-').map_or((value, None), |(a, b)| (a, Some(b)));
    let numeric = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    base.split('.').count() == 3 && base.split('.').all(numeric)
        && suffix.is_none_or(numeric)
}

/// Keep the previous version across restarts until the next version change.
/// Reading before writing ensures the notice never mistakes this start for the previous one.
pub fn remember_app_version(conn: &Connection, version: &str) -> CmdResult<PreviousAppVersion> {
    let stored = get_setting(conn, RUN_VERSION_KEY).map_err(|e| e.to_string())?
        .and_then(|raw| serde_json::from_str::<RunVersions>(&raw).ok());
    let previous = stored.and_then(|s| if s.version == version { s.previous } else { Some(s.version) })
        .filter(|v| plausible_version(v) && v != version);
    let json = serde_json::to_string(&RunVersions { version: version.to_string(), previous: previous.clone() })
        .map_err(|e| e.to_string())?;
    set_setting(conn, RUN_VERSION_KEY, &json).map_err(|e| e.to_string())?;
    Ok(PreviousAppVersion(previous))
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
struct SeenNotice {
    version: String,
    schema: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFileDto {
    pub file_name: String,
    pub path: String,
    pub size_bytes: u64,
    /// RFC 3339, local time; absent when the file system does not say.
    pub modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeNoticeDto {
    pub from_version: Option<String>,
    /// The previous version was worked out from the schema, not read from a copy
    /// the updater made, so it can be off for a build from between releases.
    pub from_version_derived: bool,
    pub to_version: String,
    /// `None` when the schema before the start could not be read.
    pub from_schema: Option<i64>,
    pub to_schema: i64,
    pub backup: Option<BackupFileDto>,
    /// The copy before the migration failed and no other copy exists.
    pub backup_failed: bool,
    pub folder: String,
    pub catalog_file: String,
    /// Release page of the previous version; absent when unknown, in test builds
    /// and where the app cannot open links.
    pub previous_release_url: Option<String>,
    pub container: bool,
}

/// Everything the decision needs, so tests can supply it without an app handle.
struct NoticeInputs<'a> {
    seen: Option<&'a str>,
    previous_version: Option<&'a str>,
    last_update: Option<LastUpdateInfo>,
    report: Option<&'a MigrationReport>,
    db_path: &'a Path,
    version: &'a str,
    schema: i64,
    container: bool,
    preview: bool,
}

fn backup_dir_of(db_path: &Path) -> Option<PathBuf> {
    db_path.parent().map(|dir| dir.join(BACKUP_DIR_NAME))
}

/// The copy as the interface shows it. The name only labels the file; a name that
/// is not one of ours, a link or a missing file gives `None`.
fn backup_file(dir: &Path, name: &str) -> Option<BackupFileDto> {
    parse_backup_name(name)?;
    let path = dir.join(name);
    let meta = std::fs::symlink_metadata(&path).ok().filter(|m| m.is_file())?;
    Some(BackupFileDto {
        file_name: name.to_string(),
        path: path.to_string_lossy().into_owned(),
        size_bytes: meta.len(),
        modified: meta.modified().ok().map(|t| chrono::DateTime::<chrono::Local>::from(t).to_rfc3339()),
    })
}

fn build_notice(input: &NoticeInputs) -> Option<UpgradeNoticeDto> {
    let seen = input.seen.and_then(|raw| serde_json::from_str::<SeenNotice>(raw).ok());
    if seen.is_some_and(|s| s.version == input.version && s.schema == input.schema) {
        return None;
    }
    let dir = backup_dir_of(input.db_path)?;
    // The copy the updater made carries the previous version; prefer it over the
    // schema copy, which holds the same data.
    let update_copy = input.last_update.as_ref().and_then(|info| {
        let file = backup_file(&dir, &info.backup_file)?;
        let label = parse_backup_name(&info.backup_file)?;
        Some((file, label.from_version?))
    });
    let (backup, from_version, derived) = match update_copy {
        Some((file, from)) => (Some(file), Some(from), false),
        None => {
            let file = input.report.and_then(|r| r.backup.as_deref())
                .and_then(|p| p.file_name()?.to_str())
                .and_then(|name| backup_file(&dir, name));
            let exact = input.previous_version.filter(|v| plausible_version(v) && *v != input.version);
            let from = exact.map(str::to_string)
                .or_else(|| input.report.and_then(|r| app_version_for_schema(r.from)).map(str::to_string));
            let derived = exact.is_none() && from.is_some();
            (file, from, derived)
        }
    };
    let backup_failed = backup.is_none() && input.report.is_some_and(|r| r.backup_error);
    if backup.is_none() && !backup_failed {
        return None;
    }
    let from_schema = match input.report {
        Some(r) => Some(r.from).filter(|from| *from > 0),
        None => Some(input.schema),
    };
    let previous_release_url = from_version.as_deref()
        .filter(|_| !input.container && !input.preview && !backup_failed)
        .map(|v| updater::release_page(v, false, updater::UpdateChannel::Stable));
    Some(UpgradeNoticeDto {
        from_version,
        from_version_derived: derived,
        to_version: input.version.to_string(),
        from_schema,
        to_schema: input.schema,
        backup,
        backup_failed,
        folder: dir.to_string_lossy().into_owned(),
        catalog_file: input.db_path.to_string_lossy().into_owned(),
        previous_release_url,
        container: input.container,
    })
}

fn read_last_update(conn: &rusqlite::Connection, version: &str) -> Option<LastUpdateInfo> {
    let stored = get_setting(conn, updater::LAST_UPDATE_KEY).ok().flatten()
        .and_then(|raw| serde_json::from_str::<LastUpdateInfo>(&raw).ok());
    updater::visible_last_update(stored, version)
}

/// The notice for this start, or nothing when there is nothing to tell or the
/// user already confirmed it for this version and schema.
#[tauri::command]
pub fn get_upgrade_notice(state: State<'_, AppState>, report: State<'_, UpgradeReport>, previous: State<'_, PreviousAppVersion>, environment: State<'_, RuntimeEnvironment>) -> CmdResult<Option<UpgradeNoticeDto>> {
    let version = env!("CARGO_PKG_VERSION");
    let (seen, last_update) = {
        let conn = lock_db(&state)?;
        (get_setting(&conn, SEEN_KEY).ok().flatten(), read_last_update(&conn, version))
    };
    Ok(build_notice(&NoticeInputs {
        seen: seen.as_deref(),
        previous_version: previous.0.as_deref(),
        last_update,
        report: report.0.as_ref(),
        db_path: &state.db_path,
        version,
        schema: katalog_core::db::migrations_current_version(),
        container: environment.container,
        preview: cfg!(feature = "preview"),
    }))
}

/// Remembers that the notice was read for this version and schema.
#[tauri::command]
pub fn dismiss_upgrade_notice(state: State<'_, AppState>) -> CmdResult<()> {
    let seen = SeenNotice { version: env!("CARGO_PKG_VERSION").to_string(), schema: katalog_core::db::migrations_current_version() };
    let json = serde_json::to_string(&seen).map_err(|e| e.to_string())?;
    let conn = lock_db(&state)?;
    set_setting(&conn, SEEN_KEY, &json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Opens the folder with the update copies. It takes no path: the frontend must
/// not be able to point this at anything else.
#[tauri::command]
pub fn open_update_backup_folder(state: State<'_, AppState>, environment: State<'_, RuntimeEnvironment>) -> CmdResult<()> {
    open_backup_folder(&state.db_path, *environment, open_directory_in_file_manager)
}

fn open_backup_folder(db_path: &Path, environment: RuntimeEnvironment, open: impl FnOnce(String) -> CmdResult<()>) -> CmdResult<()> {
    environment.require_desktop(DesktopFeature::FileManager)?;
    let dir = backup_dir_of(db_path).filter(|d| d.is_dir())
        .ok_or_else(|| CmdError::coded(ErrorCode::UpdateBackupFolderMissing))?;
    open(dir.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_versions_without_losing_previous_on_restart() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
        assert!(remember_app_version(&conn, "0.18.0-10").unwrap().0.is_none());
        assert_eq!(remember_app_version(&conn, "0.18.0-11").unwrap().0.as_deref(), Some("0.18.0-10"));
        assert_eq!(remember_app_version(&conn, "0.18.0-11").unwrap().0.as_deref(), Some("0.18.0-10"));
        assert_eq!(remember_app_version(&conn, "0.18.0").unwrap().0.as_deref(), Some("0.18.0-11"));
        set_setting(&conn, RUN_VERSION_KEY, r#"{"version":"invalid","previous":null}"#).unwrap();
        assert!(remember_app_version(&conn, "0.18.0").unwrap().0.is_none());
    }

    #[test]
    fn stored_version_overrides_schema_only_when_plausible() {
        let dir = temp("stored-version");
        put(&dir, SCHEMA_COPY);
        let r = report(&dir, Some(SCHEMA_COPY), false);
        let path = db(&dir);
        for value in ["0.18.0-6", "0.17.2", "invalid", "1.2", "1.2.3-beta", "1.2.3-", "1.2.3.4"] {
            let mut input = inputs(&path, Some(&r));
            input.previous_version = Some(value);
            let notice = build_notice(&input).unwrap();
            let valid = matches!(value, "0.18.0-6" | "0.17.2");
            assert_eq!(notice.from_version.as_deref(), Some(if valid { value } else { "0.17.2" }));
            assert_eq!(notice.from_version_derived, !valid);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    const CURRENT: i64 = 54;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mfk-notice-{name}-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(dir.join(BACKUP_DIR_NAME)).unwrap();
        dir
    }

    fn put(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(BACKUP_DIR_NAME).join(name);
        std::fs::write(&path, b"copy").unwrap();
        path
    }

    const SCHEMA_COPY: &str = "Katalog-Sicherung_2026-11-03_1432_vor-Schema-Migration_52_auf_54.db";
    const UPDATE_COPY: &str = "Katalog-Sicherung_2026-11-03_1430_vor-Update_0.17.1_auf_0.18.0.db";

    fn inputs<'a>(dir: &'a Path, report: Option<&'a MigrationReport>) -> NoticeInputs<'a> {
        NoticeInputs { seen: None, previous_version: None, last_update: None, report, db_path: dir, version: "0.18.0", schema: CURRENT, container: false, preview: false }
    }

    fn report(dir: &Path, name: Option<&str>, failed: bool) -> MigrationReport {
        MigrationReport { from: 52, to: CURRENT, backup: name.map(|n| dir.join(BACKUP_DIR_NAME).join(n)), backup_error: failed }
    }

    fn update_info() -> LastUpdateInfo {
        LastUpdateInfo { version: "0.18.0".into(), date: "2026-11-03".into(), backup_file: UPDATE_COPY.into() }
    }

    // `db_path` points at a file inside the data folder, as in the app.
    fn db(dir: &Path) -> PathBuf { dir.join("catalog.db") }

    #[test]
    fn a_manual_install_names_the_schema_copy_and_derives_the_previous_version() {
        let dir = temp("manual");
        put(&dir, SCHEMA_COPY);
        let r = report(&dir, Some(SCHEMA_COPY), false);
        let path = db(&dir);
        let notice = build_notice(&inputs(&path, Some(&r))).expect("a copy exists");
        assert_eq!(notice.backup.as_ref().unwrap().file_name, SCHEMA_COPY);
        assert_eq!(notice.from_version.as_deref(), Some("0.17.2"));
        assert!(notice.from_version_derived);
        assert_eq!((notice.from_schema, notice.to_schema), (Some(52), CURRENT));
        assert!(!notice.backup_failed);
        assert_eq!(notice.previous_release_url.as_deref(), Some("https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.17.2"));
        assert_eq!(notice.backup.unwrap().size_bytes, 4);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_updater_copy_wins_and_gives_the_exact_previous_version() {
        let dir = temp("updater");
        put(&dir, UPDATE_COPY);
        put(&dir, SCHEMA_COPY);
        let r = report(&dir, Some(SCHEMA_COPY), false);
        let path = db(&dir);
        let mut input = inputs(&path, Some(&r));
        input.last_update = Some(update_info());
        let notice = build_notice(&input).unwrap();
        assert_eq!(notice.backup.unwrap().file_name, UPDATE_COPY);
        assert_eq!(notice.from_version.as_deref(), Some("0.17.1"));
        assert!(!notice.from_version_derived);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_update_without_a_schema_change_still_has_a_notice_with_an_unchanged_schema() {
        let dir = temp("no-migration");
        put(&dir, UPDATE_COPY);
        let path = db(&dir);
        let mut input = inputs(&path, None);
        input.last_update = Some(update_info());
        let notice = build_notice(&input).unwrap();
        assert_eq!(notice.from_schema, Some(CURRENT));
        assert_eq!(notice.to_schema, CURRENT);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_fresh_installation_has_no_notice() {
        let dir = temp("fresh");
        let path = db(&dir);
        assert_eq!(build_notice(&inputs(&path, None)), None);
        // An update record without its copy tells nothing worth showing either.
        let mut input = inputs(&path, None);
        input.last_update = Some(update_info());
        assert_eq!(build_notice(&input), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_copy_is_reported_without_a_way_back() {
        let dir = temp("failed");
        let r = report(&dir, None, true);
        let path = db(&dir);
        let notice = build_notice(&inputs(&path, Some(&r))).unwrap();
        assert!(notice.backup_failed);
        assert!(notice.backup.is_none());
        assert!(notice.previous_release_url.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_schema_copy_does_not_hide_a_good_updater_copy() {
        let dir = temp("failed-but-updater");
        put(&dir, UPDATE_COPY);
        let r = report(&dir, None, true);
        let path = db(&dir);
        let mut input = inputs(&path, Some(&r));
        input.last_update = Some(update_info());
        let notice = build_notice(&input).unwrap();
        assert!(!notice.backup_failed);
        assert_eq!(notice.backup.unwrap().file_name, UPDATE_COPY);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_confirmed_notice_stays_away_until_version_or_schema_change() {
        let dir = temp("seen");
        put(&dir, SCHEMA_COPY);
        let r = report(&dir, Some(SCHEMA_COPY), false);
        let path = db(&dir);
        let mut input = inputs(&path, Some(&r));
        input.seen = Some(r#"{"version":"0.18.0","schema":54}"#);
        assert_eq!(build_notice(&input), None);
        input.seen = Some(r#"{"version":"0.18.0","schema":53}"#);
        assert!(build_notice(&input).is_some());
        input.seen = Some(r#"{"version":"0.17.1","schema":54}"#);
        assert!(build_notice(&input).is_some());
        input.seen = Some("not json");
        assert!(build_notice(&input).is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_seen_marker_has_the_documented_shape() {
        let json = serde_json::to_string(&SeenNotice { version: "0.18.0".into(), schema: 55 }).unwrap();
        assert_eq!(json, r#"{"version":"0.18.0","schema":55}"#);
    }

    #[test]
    fn names_that_are_not_ours_and_links_are_not_offered() {
        let dir = temp("names");
        let outside = dir.join("outside.db");
        std::fs::write(&outside, b"x").unwrap();
        put(&dir, "evil.db");
        let backups = dir.join(BACKUP_DIR_NAME);
        assert!(backup_file(&backups, "evil.db").is_none());
        assert!(backup_file(&backups, "../outside.db").is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, backups.join(SCHEMA_COPY)).unwrap();
            assert!(backup_file(&backups, SCHEMA_COPY).is_none());
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn containers_and_test_builds_get_no_link_to_open() {
        let dir = temp("container");
        put(&dir, SCHEMA_COPY);
        let r = report(&dir, Some(SCHEMA_COPY), false);
        let path = db(&dir);
        let mut input = inputs(&path, Some(&r));
        input.container = true;
        let notice = build_notice(&input).unwrap();
        assert!(notice.container && notice.previous_release_url.is_none());
        assert_eq!(notice.from_version.as_deref(), Some("0.17.2"));
        let mut input = inputs(&path, Some(&r));
        input.preview = true;
        assert!(build_notice(&input).unwrap().previous_release_url.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_dto_uses_camel_case_field_names() {
        let dir = temp("camel");
        put(&dir, SCHEMA_COPY);
        let r = report(&dir, Some(SCHEMA_COPY), false);
        let path = db(&dir);
        let value = serde_json::to_value(build_notice(&inputs(&path, Some(&r))).unwrap()).unwrap();
        for key in ["fromVersion", "fromVersionDerived", "toVersion", "fromSchema", "toSchema", "backup", "backupFailed", "folder", "catalogFile", "previousReleaseUrl", "container"] {
            assert!(value.get(key).is_some(), "{key}");
        }
        assert!(value["backup"].get("fileName").is_some() && value["backup"].get("sizeBytes").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_folder_opens_only_in_the_fixed_backup_folder() {
        let dir = temp("open");
        let path = db(&dir);
        let mut opened = Vec::new();
        open_backup_folder(&path, RuntimeEnvironment { container: false }, |p| { opened.push(p); Ok(()) }).unwrap();
        assert_eq!(opened, vec![dir.join(BACKUP_DIR_NAME).to_string_lossy().into_owned()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn opening_the_folder_is_refused_in_a_container_and_when_it_is_missing() {
        let dir = temp("open-refused");
        let path = db(&dir);
        let error = open_backup_folder(&path, RuntimeEnvironment { container: true }, |_| panic!("must not open")).unwrap_err();
        assert!(error.expected);
        assert_eq!(error.message, "containerFileManagerUnavailable");
        std::fs::remove_dir_all(dir.join(BACKUP_DIR_NAME)).unwrap();
        let error = open_backup_folder(&path, RuntimeEnvironment { container: false }, |_| panic!("must not open")).unwrap_err();
        assert_eq!(error.message, ErrorCode::UpdateBackupFolderMissing.as_str());
        let _ = std::fs::remove_dir_all(dir);
    }
}
