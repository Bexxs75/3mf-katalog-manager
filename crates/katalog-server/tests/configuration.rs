use katalog_server::config::{
    restore_journal_path, validate_password_path, Config, PrinterAllow, PrinterAllowEntry,
};
use std::path::{Path, PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!("cfg-{:032x}", u128::from_ne_bytes(nonce)));
        for dir in ["auth", "db", "models", "sources/child", "upload"] {
            std::fs::create_dir_all(path.join(dir)).unwrap();
        }
        Self(path)
    }
    fn config(&self, roots: &[PathBuf], extra: &[(&str, &str)]) -> Result<Config, String> {
        let joined = std::env::join_paths(roots).unwrap();
        Config::from_lookup(|key| {
            extra
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
                .or_else(|| match key {
                    "KATALOG_DB" => Some(self.0.join("db/catalog.db").to_str().unwrap().into()),
                    "KATALOG_AUTH_FILE" => {
                        Some(self.0.join("auth/web-auth.json").to_str().unwrap().into())
                    }
                    "KATALOG_ROOTS" => Some(self.0.join("models").to_str().unwrap().into()),
                    "KATALOG_SOURCE_ROOTS" => Some(joined.to_str().unwrap().into()),
                    _ => None,
                })
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn source_roots_are_optional_canonical_and_read_only() {
    let f = Fixture::new();
    assert!(f.config(&[], &[]).unwrap().source_roots.is_empty());
    let config = f.config(&[f.0.join("sources/child/..")], &[]).unwrap();
    assert_eq!(
        config.source_roots,
        [f.0.join("sources").canonicalize().unwrap()]
    );
    assert_eq!(std::fs::read_dir(f.0.join("sources")).unwrap().count(), 1);
    assert!(!f.0.join("models/.katalog-papierkorb").exists());
    assert!(!f.0.join("db/trash").exists());
}

#[test]
fn source_roots_reject_overlap_empty_missing_and_files() {
    let f = Fixture::new();
    for dir in [
        "db/trash/child",
        "models/.katalog-papierkorb/child",
        "auth/child",
    ] {
        std::fs::create_dir_all(f.0.join(dir)).unwrap();
    }
    for dir in [
        "",
        "auth",
        "auth/child",
        "db",
        "db/trash",
        "models",
        "models/.katalog-papierkorb/child",
        "missing",
    ] {
        assert!(f.config(&[f.0.join(dir)], &[]).is_err(), "{dir}");
    }
    let source = f.0.join("sources");
    assert!(f.config(&[source.clone(), source.clone()], &[]).is_err());
    assert!(f
        .config(&[source.clone(), source.join("child")], &[])
        .is_err());
    assert!(f.config(&[source, PathBuf::new()], &[]).is_err());
    std::fs::write(f.0.join("file"), b"not a directory").unwrap();
    assert!(f.config(&[f.0.join("file")], &[]).is_err());
    let upload = f.0.join("upload");
    assert!(f
        .config(
            std::slice::from_ref(&upload),
            &[("KATALOG_UPLOAD_DIR", upload.to_str().unwrap())]
        )
        .err()
        .unwrap()
        .contains("KATALOG_SOURCE_ROOTS"));
}

#[cfg(unix)]
#[test]
fn source_roots_resolve_aliases_and_reserve_future_trash() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    symlink(f.0.join("auth"), f.0.join("alias")).unwrap();
    assert!(f.config(&[f.0.join("alias")], &[]).is_err());
    symlink(f.0.join("sources"), f.0.join("source-alias")).unwrap();
    assert_eq!(
        f.config(&[f.0.join("source-alias")], &[])
            .unwrap()
            .source_roots,
        [f.0.join("sources")]
    );
    assert!(f.config(&[f.0.join("models")], &[]).is_err());
    symlink(f.0.join("sources/missing"), f.0.join("models/.katalog-papierkorb")).unwrap();
    assert!(f.config(&[f.0.join("sources")], &[]).is_err());
    std::fs::remove_file(f.0.join("models/.katalog-papierkorb")).unwrap();
    symlink(f.0.join("sources"), f.0.join("models/.katalog-papierkorb")).unwrap();
    assert!(f.config(&[f.0.join("sources/child")], &[]).is_err());
}

#[test]
fn printer_allow_parses_hosts_ips_and_both_cidr_families() {
    let allow = PrinterAllow::parse(" Printer.LAN.,192.168.1.4,10.1.2.3/8,fdab::/16,::1 ").unwrap();
    assert!(matches!(&allow.entries()[0], PrinterAllowEntry::Host(host) if host == "printer.lan"));
    for (host, ip) in [
        ("PRINTER.lan", "192.168.9.9"),
        ("other", "192.168.1.4"),
        ("other", "10.255.255.255"),
        ("other", "fdab::ffff"),
        ("other", "::1"),
    ] {
        assert!(allow.matches(host, ip.parse().unwrap()), "{host} {ip}");
    }
    assert!(!allow.matches("other", "192.168.1.5".parse().unwrap()));
    assert!(!allow.matches("other", "fdac::1".parse().unwrap()));
    assert!(!PrinterAllow::default().matches("printer.lan", "192.168.1.4".parse().unwrap()));
    assert!(PrinterAllow::parse("  ").unwrap().entries().is_empty());
}

#[test]
fn printer_allow_rejects_malformed_entries_and_blocks_metadata() {
    for bad in [
        ",",
        "host,",
        "http://printer",
        "printer:80",
        "[::1]:80",
        "*",
        "user@host",
        "a..b",
        "127.1",
        "999.1.1.1",
        "1.2.3.4/33",
        "::/129",
        "host/24",
        "::/-1",
        "::/+1",
        "::/1/2",
        "::/",
    ] {
        assert!(PrinterAllow::parse(bad).is_err(), "{bad}");
    }
    for text in [
        "0.0.0.0/0,::/0",
        "metadata",
        "169.254.169.254,fd00:ec2::254",
        "169.254.0.0/16,fd00::/8",
    ] {
        let allow = PrinterAllow::parse(text).unwrap();
        for ip in ["169.254.169.254", "fd00:ec2::254", "::ffff:169.254.169.254"] {
            assert!(
                !allow.matches("metadata", ip.parse().unwrap()),
                "{text} {ip}"
            );
        }
    }
    let exact = PrinterAllow::parse("192.168.1.1/32,fd00::1/128").unwrap();
    assert!(exact.matches("", "fd00::1".parse().unwrap()));
    assert!(!exact.matches("", "fd00::2".parse().unwrap()));
    let f = Fixture::new();
    assert!(f
        .config(&[], &[("KATALOG_PRINTER_ALLOW", "bad/host")])
        .is_err());
    assert_eq!(
        f.config(&[], &[("KATALOG_PRINTER_ALLOW", "printer")])
            .unwrap()
            .printer_allow
            .entries()
            .len(),
        1
    );
}

#[cfg(unix)]
#[test]
fn password_and_restore_paths_reject_resolved_collisions() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let db = f.0.join("db/custom.sqlite");
    std::fs::write(&db, b"catalog").unwrap();
    let alias = f.0.join("alias.sqlite");
    symlink(&db, &alias).unwrap();
    for suffix in ["", "-wal", "-shm", "-journal", ".lock"] {
        assert!(validate_password_path(
            Some(&alias),
            &db.with_file_name(format!("custom.sqlite{suffix}"))
        )
        .is_err());
    }
    let auth = f.0.join("auth/web-auth.json");
    let journal = restore_journal_path(&db, &auth).unwrap();
    assert!(!journal.exists());
    symlink(&db, &journal).unwrap();
    assert!(restore_journal_path(&db, &auth).is_err());
    std::fs::remove_file(&journal).unwrap();
    std::fs::write(&auth, b"password").unwrap();
    symlink(&auth, &journal).unwrap();
    assert!(restore_journal_path(&db, &auth).is_err());
    assert!(validate_password_path(None, &auth).is_err());
    std::fs::remove_file(&journal).unwrap();
    for name in ["web-restore.json", ".bak-old", "catalog.db.lock"] {
        let target = f.0.join(name);
        std::fs::write(&target, b"unchanged").unwrap();
        let alias = f.0.join("auth-alias");
        symlink(&target, &alias).unwrap();
        assert!(validate_password_path(None, &alias).is_err());
        std::fs::remove_file(alias).unwrap();
        assert_eq!(std::fs::read(target).unwrap(), b"unchanged");
    }
    assert!(validate_password_path(Some(&db), &f.0.join("elsewhere/custom.sqlite")).is_ok());
    assert!(validate_password_path(None, Path::new(".bak-new")).is_err());
}

#[test]
fn the_start_password_file_name_cannot_be_the_catalog_or_the_auth_file() {
    let f = Fixture::new();
    let auth = f.0.join("auth/web-auth.json");
    std::fs::create_dir_all(f.0.join("auth")).unwrap();
    // A catalog with that name next to the auth file would be replaced by the start password.
    let clash = f.0.join("auth/web-initial-password.txt");
    std::fs::write(&clash, b"catalog bytes").unwrap();
    let error = validate_password_path(Some(&clash), &auth).unwrap_err();
    assert!(error.contains("web-initial-password.txt"), "{error}");
    // The auth file itself must not carry the name either.
    assert!(validate_password_path(Some(&f.0.join("catalog.db")), &clash).is_err());
    assert_eq!(std::fs::read(&clash).unwrap(), b"catalog bytes");
    // Gegenprobe: a normal pair is fine.
    assert!(validate_password_path(Some(&f.0.join("catalog.db")), &auth).is_ok());
}
