use katalog_core::db;
use katalog_server::{config::Config, initialize, password};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path =
            std::env::temp_dir().join(format!("katalog-startup-{:x}", u128::from_ne_bytes(random)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn config(&self) -> Config {
        Config::from_lookup(|key| match key {
            "KATALOG_DB" => Some(self.0.join("catalog.db").to_string_lossy().into_owned()),
            "KATALOG_ROOTS" => Some(self.0.to_string_lossy().into_owned()),
            _ => None,
        })
        .unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            (
                path.file_name().unwrap().into(),
                Sha256::digest(fs::read(path).unwrap()).to_vec(),
            )
        })
        .collect();
    files.sort();
    files
}

#[tokio::test]
async fn missing_or_invalid_catalog_never_creates_auth_or_catalog() {
    let dir = Directory::new();
    let config = dir.config();
    assert!(initialize(config.clone()).await.is_err());
    assert!(snapshot(&dir.0).is_empty());
    fs::write(&config.db_path, b"not a sqlite catalog").unwrap();
    let before = snapshot(&dir.0);
    assert!(initialize(config).await.is_err());
    assert_eq!(snapshot(&dir.0), before);
}

#[tokio::test]
async fn old_schema_startup_keeps_catalog_checksum_and_directory_unchanged() {
    let dir = Directory::new();
    let config = dir.config();
    let conn = db::connect(&config.db_path).unwrap();
    conn.execute_batch("ALTER TABLE files DROP COLUMN license")
        .unwrap();
    conn.pragma_update(None, "user_version", db::migrations_current_version() - 1)
        .unwrap();
    drop(conn);
    let before = snapshot(&dir.0);
    let failure = initialize(config.clone())
        .await
        .expect_err("old schema rejected");
    assert!(failure.contains("migriert"));
    assert_eq!(snapshot(&dir.0), before);
    assert!(!config.auth_file.exists());
}

#[tokio::test]
async fn startup_bootstrap_existing_auth_and_invalid_auth_preserve_catalog() {
    const UNUSED_BOOTSTRAP_PASSWORD: &str = "Dieses Passwort wird ignoriert 42";
    let dir = Directory::new();
    let mut config = dir.config();
    drop(db::connect(&config.db_path).unwrap());
    let before = snapshot(&dir.0);
    drop(initialize(config.clone()).await.unwrap());
    let auth = password::read_auth(&config.auth_file).unwrap();
    assert!(auth.must_change);
    let mut after = snapshot(&dir.0);
    after.retain(|(name, _)| name != Path::new("web-auth.json"));
    assert_eq!(after, before);
    let existing = snapshot(&dir.0);
    config.password = Some(UNUSED_BOOTSTRAP_PASSWORD.into());
    drop(initialize(config.clone()).await.unwrap());
    assert_eq!(snapshot(&dir.0), existing);
    fs::write(&config.auth_file, b"{\"version\":2}").unwrap();
    let invalid_before = snapshot(&dir.0);
    assert!(initialize(config).await.is_err());
    assert_eq!(snapshot(&dir.0), invalid_before);
}

#[test]
fn cli_rejects_no_auth_with_proxy_before_opening_catalog() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_katalog-server"))
        .env_clear()
        .env("KATALOG_DB", "/unused/catalog.db")
        .env("KATALOG_ROOTS", "/unused")
        .env("KATALOG_ALLOW_NO_AUTH", "1")
        .env("KATALOG_TRUST_PROXY", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("nicht mit KATALOG_TRUST_PROXY"));
}

#[test]
fn external_binding_warns_about_dns_rebinding_without_aborting_configuration() {
    let run = |hosts: Option<&str>| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_katalog-server"));
        command
            .env_clear()
            .env("KATALOG_DB", "/unused/catalog.db")
            .env("KATALOG_ROOTS", "/unused")
            .env("KATALOG_BIND", "0.0.0.0:8080");
        if let Some(hosts) = hosts {
            command.env("KATALOG_ALLOWED_HOSTS", hosts);
        }
        command.output().unwrap()
    };
    let output = run(None);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("DNS-Rebinding") && stderr.contains("KATALOG_ALLOWED_HOSTS"));
    assert!(stderr.contains("Katalog kann nicht nur lesend geöffnet werden"));
    assert!(
        !String::from_utf8_lossy(&run(Some("catalog.local:8080")).stderr).contains("DNS-Rebinding")
    );
}

#[test]
fn help_explains_proxy_cookie_password_log_and_terminal_risks() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_katalog-server"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for warning in [
        "anhängen oder überschreiben",
        "nicht nach Port getrennt",
        "Host-/Domainnamen",
        "docker logs",
        "Anmelden ändern",
        "Eingabe sichtbar",
        "neu gestartet",
    ] {
        assert!(text.contains(warning), "missing help: {warning}");
    }
}
