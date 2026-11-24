use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn set_password_refuses_operation_journals_without_mutating_them() {
    let root = std::env::temp_dir().join(format!("password-journal-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for name in ["web-fsops.json", "web-uploads.json", "web-restore.json", "web-initial-password.txt"] {
        let path = root.join(name);
        let original = b"existing durable journal must survive";
        std::fs::write(&path, original).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_katalog-server"))
            .arg("set-password")
            .env("KATALOG_AUTH_FILE", &path)
            .env_remove("KATALOG_DB")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = child
            .stdin
            .take()
            .unwrap()
            .write_all(b"a-long-test-password\n");
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Serverjournal"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn configuration_collisions_leave_targets_unchanged() {
    let root = std::env::temp_dir().join(format!("config-collisions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for db_name in [None, Some("catalog.db"), Some("custom.sqlite")] {
        let name = db_name.unwrap_or("catalog.db");
        let names = [
            "web-restore.json".to_owned(),
            name.to_owned(),
            format!("{name}-wal"),
            format!("{name}-shm"),
            format!("{name}-journal"),
            format!("{name}.lock"),
            ".bak-original".to_owned(),
        ];
        for name in names {
            let path = root.join(name);
            let original = b"unchanged target bytes";
            std::fs::write(&path, original).unwrap();
            for set_password in [true, false] {
                if !set_password && db_name.is_none() {
                    continue;
                }
                let mut command = Command::new(env!("CARGO_BIN_EXE_katalog-server"));
                command
                    .env_clear()
                    .env("KATALOG_AUTH_FILE", &path)
                    .env("KATALOG_ROOTS", &root)
                    .stdin(Stdio::null());
                if set_password {
                    command.arg("set-password");
                }
                if let Some(db) = db_name {
                    command.env("KATALOG_DB", root.join(db));
                }
                let output = command.output().unwrap();
                assert!(!output.status.success(), "{path:?}");
                let error = String::from_utf8_lossy(&output.stderr);
                assert!(
                    error.contains("kollidiert") || error.contains("KATALOG_AUTH_FILE"),
                    "{error}"
                );
                assert_eq!(std::fs::read(&path).unwrap(), original, "{path:?}");
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
