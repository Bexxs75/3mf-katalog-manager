//! Bounded, atomic UI preferences shared by desktop and web adapters.
use crate::{error_code::ErrorCode, safe_file};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::Path, sync::Mutex};

pub const MAX_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
// All adapters and backup operations share the lock, so read/modify/write cannot lose updates.
static LOCK: Mutex<()> = Mutex::new(());
pub type Values = BTreeMap<String, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub schema_version: u64,
    pub migrated_from_webview: bool,
    pub values: Values,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            migrated_from_webview: false,
            values: Values::new(),
            extra: BTreeMap::new(),
        }
    }
}

fn valid(key: &str, value: &str) -> bool {
    if !key.starts_with("3mf-katalog-")
        || key.len() > 256
        || key.chars().any(char::is_control)
        || value.len()
            > if matches!(
                key,
                "3mf-katalog-sidebar-expanded" | "3mf-katalog-collapsed-folders"
            ) {
                4 * MAX_VALUE_BYTES
            } else {
                MAX_VALUE_BYTES
            }
    {
        return false;
    }
    let choices: Option<&[&str]> = match key.trim_start_matches("3mf-katalog-") {
        "theme" => Some(&["system", "white", "light", "slate", "dark"]),
        "language" => Some(&["de", "en", "es", "fr"]),
        "density" => Some(&["compact", "comfort"]),
        "display-preference" => Some(&["thumbnail", "render"]),
        "detail-panel" => Some(&["auto", "pinned"]),
        "single-key-shortcuts" | "dnd-tip-dismissed" => Some(&["true", "false"]),
        "setup-seen" => Some(&["1"]),
        "bundle-collapse" => Some(&["on", "off"]),
        "filament-kind" => Some(&["filament", "resin"]),
        "filament-layout" => Some(&["list", "dashboard"]),
        "filament-group-mode" => Some(&["single", "grouped"]),
        "view" => Some(&["grid", "list", "groupedGrid", "groupedList"]),
        _ => None,
    };
    if let Some(choices) = choices {
        return choices.contains(&value);
    }
    if key.starts_with("3mf-katalog-preview-attempt:") || key == "3mf-katalog-slicers" {
        return false;
    }
    match key.trim_start_matches("3mf-katalog-") {
        "sidebar-width" | "detail-width" => value
            .parse::<f64>()
            .is_ok_and(|n| n.is_finite() && n > 0.0 && n <= 10000.0),
        "collapsed-folders" | "sidebar-expanded" | "hidden-slicer-ids" => {
            serde_json::from_str::<Vec<String>>(value).is_ok()
        }
        "bundle-suggestions" => serde_json::from_str::<serde_json::Value>(value).is_ok_and(|v| {
            v.get("hint").is_some_and(serde_json::Value::is_boolean)
                && v.get("dismissed")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|a| a.iter().all(serde_json::Value::is_string))
        }),
        // Unknown namespaced strings remain inert, preserving preferences from newer releases.
        _ => true,
    }
}
fn encode(settings: &Settings) -> Result<Vec<u8>, ErrorCode> {
    if settings.schema_version != 1 || !settings.values.iter().all(|(k, v)| valid(k, v)) {
        return Err(ErrorCode::UiSettingsInvalid);
    }
    let bytes = serde_json::to_vec(settings).map_err(|_| ErrorCode::UiSettingsInvalid)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ErrorCode::UiSettingsInvalid);
    }
    Ok(bytes)
}
fn read(dir: &Path, recover: bool) -> Result<Settings, ErrorCode> {
    let path = dir.join("ui-settings.json");
    // Allow oversized older documents to recover individual values, while bounding allocations.
    let bytes = match safe_file::read_bounded(&path, (4 * MAX_FILE_BYTES) as u64) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            return if recover {
                quarantine(dir)
            } else {
                Err(ErrorCode::UiSettingsInvalid)
            };
        }
        Err(_) => return Err(ErrorCode::UiSettingsUnavailable),
    };
    if let Ok(serde_json::Value::Object(mut object)) = serde_json::from_slice(&bytes) {
        let schema_version = object
            .remove("schema_version")
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0)
            .unwrap_or(1);
        // A downgrade cannot interpret a future schema or safely overwrite its preferences.
        if schema_version != 1 {
            return Ok(Settings {
                schema_version,
                ..Settings::default()
            });
        }
        let migrated_from_webview = object
            .remove("migrated_from_webview")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut values = Values::new();
        if let Some(serde_json::Value::Object(entries)) = object.remove("values") {
            for (key, value) in entries {
                if let Some(value) = value.as_str().filter(|v| valid(&key, v)) {
                    values.insert(key, value.to_owned());
                }
            }
        }
        let mut settings = Settings {
            schema_version,
            migrated_from_webview,
            values,
            extra: object.into_iter().collect(),
        };
        // Keep the small preferences when a document exceeds the current write budget.
        // Reads never rewrite a valid document, especially one from a newer release.
        while serde_json::to_vec(&settings)
            .map_err(|_| ErrorCode::UiSettingsInvalid)?
            .len()
            > MAX_FILE_BYTES
        {
            // Opaque extensions must fit the same budget as preferences; discard largest first.
            if let Some(key) = settings
                .extra
                .iter()
                .max_by_key(|(key, value)| (key.len() + value.to_string().len(), *key))
                .map(|(key, _)| key.clone())
            {
                settings.extra.remove(&key);
                continue;
            }
            let Some(key) = settings
                .values
                .iter()
                .max_by_key(|(k, v)| (v.len(), *k))
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            settings.values.remove(&key);
        }
        return Ok(settings);
    }
    if recover {
        quarantine(dir)
    } else {
        Ok(Settings::default())
    }
}
fn quarantine(dir: &Path) -> Result<Settings, ErrorCode> {
    // Preserve damaged and over-budget files for diagnosis without blocking future writes.
    let mut broken = dir.join("ui-settings.json.broken");
    let mut suffix = 1;
    while broken.exists() {
        broken = dir.join(format!("ui-settings.json.{suffix}.broken"));
        suffix += 1;
    }
    std::fs::rename(dir.join("ui-settings.json"), broken)
        .map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    Ok(Settings::default())
}
fn write(dir: &Path, settings: &Settings) -> Result<(), ErrorCode> {
    let bytes = encode(settings)?;
    std::fs::create_dir_all(dir).map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    let (temp, mut file) = safe_file::create_private_temp(dir, ".ui-settings", ".tmp")
        .map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, dir.join("ui-settings.json"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result.map_err(|_| ErrorCode::UiSettingsUnavailable)
}
pub fn get(dir: &Path) -> Result<Settings, ErrorCode> {
    let _guard = LOCK.lock().map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    read(dir, true)
}
/// Inspect preferences without repairing or renaming files on read-only deployments.
pub fn get_readonly(dir: &Path) -> Result<Settings, ErrorCode> {
    let _guard = LOCK.lock().map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    Ok(read(dir, false).unwrap_or_default())
}
pub fn set(dir: &Path, key: String, value: Option<String>) -> Result<(), ErrorCode> {
    let _guard = LOCK.lock().map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    if !valid(&key, value.as_deref().unwrap_or("1")) && value.is_some() {
        return Err(ErrorCode::UiSettingsInvalid);
    }
    let mut settings = read(dir, true)?;
    if let Some(value) = value {
        settings.values.insert(key, value);
    } else {
        settings.values.remove(&key);
    }
    write(dir, &settings)
}
pub fn migrate(dir: &Path, values: Values) -> Result<Settings, ErrorCode> {
    let _guard = LOCK.lock().map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    let mut settings = read(dir, true)?;
    if settings.schema_version != 1 || settings.migrated_from_webview {
        return Ok(settings);
    }
    // Prefer small preferences, preserving backend values and committing the marker even if full.
    settings.migrated_from_webview = true;
    let mut candidates: Vec<_> = values.into_iter().collect();
    candidates.sort_by_key(|(key, value)| (value.len(), key.clone()));
    for (key, value) in candidates {
        if valid(&key, &value) && !settings.values.contains_key(&key) {
            settings.values.insert(key.clone(), value);
            if encode(&settings).is_err() {
                settings.values.remove(&key);
            }
        }
    }
    write(dir, &settings)?;
    Ok(settings)
}

pub const BACKUP_KEYS: [&str; 4] = [
    "3mf-katalog-theme",
    "3mf-katalog-display-preference",
    "3mf-katalog-language",
    "3mf-katalog-density",
];
pub fn backup(dir: &Path) -> Result<String, ErrorCode> {
    let _guard = LOCK.lock().map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    let settings = read(dir, false).unwrap_or_default();
    let values: BTreeMap<_, _> = BACKUP_KEYS
        .iter()
        .map(|key| (*key, settings.values.get(*key)))
        .collect();
    serde_json::to_string(&values).map_err(|_| ErrorCode::UiSettingsInvalid)
}
pub fn restore(dir: &Path, json: &str) -> Result<(), ErrorCode> {
    if json.len() > MAX_FILE_BYTES {
        return Err(ErrorCode::UiSettingsInvalid);
    }
    let values: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|_| ErrorCode::UiSettingsInvalid)?;
    let _guard = LOCK.lock().map_err(|_| ErrorCode::UiSettingsUnavailable)?;
    let mut settings = read(dir, true)?;
    // Backups are portable: never import executable paths or machine-specific UI state.
    for key in BACKUP_KEYS {
        match values.get(key) {
            None | Some(serde_json::Value::Null) => {
                settings.values.remove(key);
            }
            Some(serde_json::Value::String(value)) if valid(key, value) => {
                settings.values.insert(key.into(), value.clone());
            }
            _ => {}
        }
    }
    settings.migrated_from_webview = true;
    write(dir, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(name: &str) -> std::path::PathBuf {
        crate::unique_test_dir(name)
    }
    fn values(pairs: &[(&str, &str)]) -> Values {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    #[test]
    fn migration_is_once_and_preserves_backend_values_and_unknown_keys() {
        let dir = dir("ui-migrate");
        set(&dir, "3mf-katalog-theme".into(), Some("dark".into())).unwrap();
        let migrated = migrate(
            &dir,
            values(&[
                ("3mf-katalog-theme", "light"),
                ("3mf-katalog-future", "opaque"),
                ("other", "ignored"),
                ("3mf-katalog-density", "invalid"),
            ]),
        )
        .unwrap();
        assert!(migrated.migrated_from_webview);
        assert_eq!(
            migrated.values,
            values(&[
                ("3mf-katalog-theme", "dark"),
                ("3mf-katalog-future", "opaque")
            ])
        );
        set(&dir, "3mf-katalog-theme".into(), None).unwrap();
        assert!(!migrate(&dir, values(&[("3mf-katalog-theme", "light")]))
            .unwrap()
            .values
            .contains_key("3mf-katalog-theme"));
    }
    #[test]
    fn missing_empty_corrupt_and_wrong_types_recover_without_losing_broken_copies() {
        let dir = dir("ui-broken");
        assert!(get(&dir).unwrap().values.is_empty());
        for (index, content) in ["", "{bad"].iter().enumerate() {
            std::fs::write(dir.join("ui-settings.json"), content).unwrap();
            assert!(get(&dir).unwrap().values.is_empty());
            let name = if index == 0 {
                "ui-settings.json.broken".into()
            } else {
                format!("ui-settings.json.{index}.broken")
            };
            assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), *content);
        }
        migrate(&dir, Values::new()).unwrap();
        assert!(get(&dir).unwrap().migrated_from_webview);
    }
    #[test]
    fn large_folder_lists_migrate_but_remain_bounded_and_dead_slicers_are_dropped() {
        let dir = dir("ui-large-folders");
        let folders = serde_json::to_string(&vec!["a".repeat(36); 2000]).unwrap();
        let keys = [
            "3mf-katalog-sidebar-expanded",
            "3mf-katalog-collapsed-folders",
        ];
        let mut legacy = Values::new();
        for key in keys {
            legacy.insert(key.into(), folders.clone());
        }
        legacy.insert("3mf-katalog-slicers".into(), "/opt/old-slicer".into());
        let result = migrate(&dir, legacy).unwrap();
        assert!(!result.values.contains_key("3mf-katalog-slicers"));
        for key in keys {
            assert_eq!(result.values[key], folders);
            let huge = serde_json::to_string(&vec!["a".repeat(36); 7000]).unwrap();
            assert_eq!(
                set(&dir, key.into(), Some(huge)),
                Err(ErrorCode::UiSettingsInvalid)
            );
        }
    }
    #[test]
    fn future_documents_are_untouched_by_reads_and_writes_and_current_extensions_survive() {
        let dir = dir("ui-future");
        let path = dir.join("ui-settings.json");
        let document = r#"{"schema_version":2,"migrated_from_webview":false,"future":{"x":1},"values":{"3mf-katalog-theme":"dark","3mf-katalog-density":42}}"#;
        std::fs::write(&path, document).unwrap();
        assert_eq!(get_readonly(&dir).unwrap().schema_version, 2);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), document);
        assert!(!dir.join("ui-settings.json.broken").exists());
        assert!(get(&dir).unwrap().values.is_empty());
        assert_eq!(
            set(&dir, "3mf-katalog-theme".into(), Some("white".into())),
            Err(ErrorCode::UiSettingsInvalid)
        );
        assert_eq!(migrate(&dir, Values::new()).unwrap().schema_version, 2);
        assert_eq!(restore(&dir, "{}"), Err(ErrorCode::UiSettingsInvalid));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), document);
        assert!(!dir.join("ui-settings.json.broken").exists());
        std::fs::write(
            &path,
            document.replace("schema_version\":2", "schema_version\":1"),
        )
        .unwrap();
        set(&dir, "3mf-katalog-theme".into(), Some("white".into())).unwrap();
        assert_eq!(get(&dir).unwrap().extra["future"]["x"], 1);
    }
    #[test]
    fn oversized_extensions_are_pruned_in_memory_and_do_not_block_writes() {
        let dir = dir("ui-extra-budget");
        let path = dir.join("ui-settings.json");
        let document = serde_json::json!({"schema_version": 1, "large": "x".repeat(2 * MAX_FILE_BYTES), "small": true,
            "values": {"3mf-katalog-theme": "dark"}}).to_string();
        std::fs::write(&path, &document).unwrap();
        let settings = get(&dir).unwrap();
        assert!(!settings.extra.contains_key("large"));
        assert_eq!(settings.extra["small"], true);
        assert_eq!(settings.values["3mf-katalog-theme"], "dark");
        assert!(encode(&settings).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), document);
        migrate(&dir, Values::new()).unwrap();
        set(&dir, "3mf-katalog-theme".into(), Some("white".into())).unwrap();
        restore(&dir, "{}").unwrap();
        assert!(std::fs::metadata(path).unwrap().len() <= MAX_FILE_BYTES as u64);
    }
    #[test]
    fn oversized_and_invalid_values_do_not_discard_other_preferences() {
        let dir = dir("ui-tolerant");
        let mut entries = serde_json::Map::new();
        entries.insert("3mf-katalog-theme".into(), "dark".into());
        entries.insert("3mf-katalog-density".into(), 42.into());
        entries.insert("3mf-katalog-preview-attempt:42".into(), "1".into());
        entries.insert(
            "3mf-katalog-large".into(),
            "x".repeat(MAX_VALUE_BYTES + 1).into(),
        );
        for i in 0..20 {
            entries.insert(
                format!("3mf-katalog-future-{i}"),
                "x".repeat(MAX_VALUE_BYTES).into(),
            );
        }
        let bytes =
            serde_json::to_vec(&serde_json::json!({"schema_version":1,"values":entries})).unwrap();
        std::fs::write(dir.join("ui-settings.json"), &bytes).unwrap();
        let settings = get(&dir).unwrap();
        assert_eq!(settings.values["3mf-katalog-theme"], "dark");
        assert!(!settings.values.contains_key("3mf-katalog-density"));
        assert!(!settings.values.contains_key("3mf-katalog-large"));
        assert!(!settings
            .values
            .contains_key("3mf-katalog-preview-attempt:42"));
        assert!(encode(&settings).is_ok());
        assert_eq!(std::fs::read(dir.join("ui-settings.json")).unwrap(), bytes);
    }
    #[test]
    fn backup_never_repairs_or_fails_for_unreadable_settings() {
        let dir = dir("ui-readonly-backup");
        let path = dir.join("ui-settings.json");
        for bytes in [b"{broken".to_vec(), vec![b'x'; 4 * MAX_FILE_BYTES + 1]] {
            std::fs::write(&path, &bytes).unwrap();
            let json: serde_json::Value = serde_json::from_str(&backup(&dir).unwrap()).unwrap();
            assert!(json["3mf-katalog-theme"].is_null());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(backup(&dir).is_ok());
    }
    #[test]
    fn missing_invalid_schema_and_oversized_files_remain_writable() {
        for content in [
            "{}",
            r#"{"values":{}}"#,
            r#"{"schema_version":"1"}"#,
            r#"{"schema_version":0}"#,
            r#"{"schema_version":1.5}"#,
        ] {
            let dir = dir("ui-schema-recovery");
            std::fs::write(dir.join("ui-settings.json"), content).unwrap();
            assert_eq!(get(&dir).unwrap().schema_version, 1);
            assert!(migrate(&dir, Values::new()).unwrap().migrated_from_webview);
            set(&dir, "3mf-katalog-theme".into(), Some("dark".into())).unwrap();
            restore(&dir, r#"{"3mf-katalog-theme":"white"}"#).unwrap();
            assert_eq!(get(&dir).unwrap().values["3mf-katalog-theme"], "white");
        }
        let dir = dir("ui-oversize-recovery");
        let bytes = vec![b'x'; 4 * MAX_FILE_BYTES + 1];
        std::fs::write(dir.join("ui-settings.json"), &bytes).unwrap();
        assert!(get(&dir).unwrap().values.is_empty());
        assert_eq!(
            std::fs::read(dir.join("ui-settings.json.broken")).unwrap(),
            bytes
        );
        set(&dir, "3mf-katalog-theme".into(), Some("dark".into())).unwrap();
        assert_eq!(get(&dir).unwrap().values["3mf-katalog-theme"], "dark");
    }
    #[test]
    fn type_and_size_rejections_leave_previous_file_unchanged() {
        let dir = dir("ui-limits");
        set(&dir, "3mf-katalog-language".into(), Some("de".into())).unwrap();
        let before = std::fs::read(dir.join("ui-settings.json")).unwrap();
        for (key, value) in [
            ("3mf-katalog-theme", "wrong".into()),
            ("3mf-katalog-sidebar-expanded", "[1]".into()),
            ("3mf-katalog-sidebar-width", "NaN".into()),
            ("3mf-katalog-future", "a".repeat(MAX_VALUE_BYTES + 1)),
        ] {
            assert_eq!(
                set(&dir, key.into(), Some(value)),
                Err(ErrorCode::UiSettingsInvalid)
            );
        }
        let huge = (0..20)
            .map(|i| {
                (
                    format!("3mf-katalog-future-{i}"),
                    "x".repeat(MAX_VALUE_BYTES),
                )
            })
            .collect();
        assert_eq!(std::fs::read(dir.join("ui-settings.json")).unwrap(), before);
        let migrated = migrate(&dir, huge).unwrap();
        assert!(migrated.migrated_from_webview);
        assert_eq!(migrated.values["3mf-katalog-language"], "de");
        assert!(encode(&migrated).is_ok());
        assert!(migrated.values.len() < 21);
        set(
            &dir.join("single"),
            "3mf-katalog-future".into(),
            Some("x".repeat(MAX_VALUE_BYTES)),
        )
        .unwrap();
    }
    #[test]
    fn writes_replace_complete_json_and_clean_temporary_files_on_failure() {
        let dir = dir("ui-atomic");
        set(&dir, "3mf-katalog-theme".into(), Some("dark".into())).unwrap();
        set(&dir, "3mf-katalog-theme".into(), Some("white".into())).unwrap();
        assert_eq!(get(&dir).unwrap().values["3mf-katalog-theme"], "white");
        std::fs::remove_file(dir.join("ui-settings.json")).unwrap();
        std::fs::create_dir(dir.join("ui-settings.json")).unwrap();
        assert!(write(&dir, &Settings::default()).is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }
    #[test]
    fn concurrent_updates_do_not_lose_keys() {
        let dir = dir("ui-concurrent");
        std::thread::scope(|scope| {
            for i in 0..12 {
                let dir = &dir;
                scope.spawn(move || {
                    set(dir, format!("3mf-katalog-future-{i}"), Some("value".into())).unwrap()
                });
            }
        });
        assert_eq!(get(&dir).unwrap().values.len(), 12);
    }
    #[test]
    fn old_and_new_backups_restore_only_portable_settings() {
        let dir = dir("ui-backup");
        set(&dir, "3mf-katalog-base-dir".into(), Some("/local".into())).unwrap();
        restore(&dir, r#"{"3mf-katalog-theme":"slate","3mf-katalog-language":"en","3mf-katalog-slicers":"/bin/sh","3mf-katalog-last-seen-version":"99.0.0"}"#).unwrap();
        let old = get(&dir).unwrap();
        assert_eq!(old.values["3mf-katalog-theme"], "slate");
        assert_eq!(old.values["3mf-katalog-base-dir"], "/local");
        assert!(!old.values.contains_key("3mf-katalog-slicers"));
        let json = backup(&dir).unwrap();
        let other = dir.join("other");
        restore(&other, &json).unwrap();
        assert_eq!(backup(&other).unwrap(), json);
        restore(
            &other,
            r#"{"3mf-katalog-theme":null,"3mf-katalog-language":"invalid"}"#,
        )
        .unwrap();
        let restored = get(&other).unwrap();
        assert!(!restored.values.contains_key("3mf-katalog-theme"));
        assert_eq!(restored.values["3mf-katalog-language"], "en");
        assert!(restored.migrated_from_webview);
    }
}
