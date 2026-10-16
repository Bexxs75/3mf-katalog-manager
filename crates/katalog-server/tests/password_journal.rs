use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn set_password_refuses_operation_journals_without_mutating_them() {
    let root = std::env::temp_dir().join(format!("password-journal-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for name in ["web-fsops.json", "web-uploads.json"] {
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
