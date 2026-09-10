use std::path::Path;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CloudConfig {
    pub google_client_id: String,
    pub google_client_secret: String,
    /// API-Key fuer Googles Picker-Widget (separat vom OAuth-Client, ueber
    /// die Google Cloud Console angelegt). Nur fuer den Picker-Datei-/
    /// Ordnerauswahl-Dialog noetig, nicht fuer den OAuth-Login selbst.
    pub google_picker_api_key: String,
}

#[derive(Debug)]
pub enum ConfigError {
    Missing(std::path::PathBuf),
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Missing(path) => write!(
                f,
                "Cloud-Konfiguration fehlt: {} nicht gefunden. Siehe cloud.config.example.json.",
                path.display()
            ),
            ConfigError::Invalid(msg) => write!(f, "Cloud-Konfiguration ungueltig: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

pub fn load_cloud_config(config_path: &Path) -> Result<CloudConfig, ConfigError> {
    if !config_path.exists() {
        return Err(ConfigError::Missing(config_path.to_path_buf()));
    }
    let contents =
        std::fs::read_to_string(config_path).map_err(|e| ConfigError::Invalid(e.to_string()))?;
    serde_json::from_str(&contents).map_err(|e| ConfigError::Invalid(e.to_string()))
}

/// Pfad zu `cloud.config.json` relativ zum `src-tauri`-Verzeichnis, zur
/// Kompilierzeit ueber CARGO_MANIFEST_DIR aufgeloest. Funktioniert damit
/// unabhaengig vom Arbeitsverzeichnis in `cargo build`/`npm run tauri dev`.
/// Fuer signierte Release-Pakete (noch nicht Teil dieses Projekts, siehe
/// Schritt 9 des urspruenglichen Projektauftrags) muesste dies durch einen
/// nutzerbeschreibbaren App-Datenverzeichnis-Pfad ersetzt werden.
pub fn default_config_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cloud.config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_missing_error_for_nonexistent_file() {
        let result = load_cloud_config(Path::new("/tmp/does-not-exist-3mf-katalog-cloud.json"));
        assert!(matches!(result, Err(ConfigError::Missing(_))));
    }

    #[test]
    fn loads_valid_config() {
        let dir = std::env::temp_dir().join(format!("3mf-cloud-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("cloud.config.json");
        std::fs::write(
            &path,
            r#"{"google_client_id": "test-id", "google_client_secret": "test-secret", "google_picker_api_key": "test-picker-key"}"#,
        )
        .expect("write config");

        let config = load_cloud_config(&path).expect("load");
        assert_eq!(config.google_client_id, "test-id");
        assert_eq!(config.google_client_secret, "test-secret");
        assert_eq!(config.google_picker_api_key, "test-picker-key");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn returns_invalid_error_for_malformed_json() {
        let dir =
            std::env::temp_dir().join(format!("3mf-cloud-config-test-invalid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("cloud.config.json");
        std::fs::write(&path, "not json").expect("write config");

        let result = load_cloud_config(&path);
        assert!(matches!(result, Err(ConfigError::Invalid(_))));

        std::fs::remove_dir_all(&dir).ok();
    }
}
