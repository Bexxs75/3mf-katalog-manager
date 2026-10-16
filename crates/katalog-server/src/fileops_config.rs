//! Startup handles and filesystem capability probe; never resolve request paths here.
use crate::config::{validate_auth_path, Config};
use katalog_core::fsops::{FsContext, Protection};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

pub(crate) fn context(config: &Config) -> Result<FsContext, String> {
    if !config.write
        || config.allow_no_auth
        || config
            .allowed_hosts
            .as_ref()
            .is_none_or(|h| h.is_empty() || h.iter().any(|s| crate::config::parse_host(s).is_err()))
        || config.file_ops_daily_limit == 0
    {
        return Err(
            "Dateiverwaltung erfordert Schreibmodus, Anmeldung, gültige Hostliste und Tagesgrenze"
                .into(),
        );
    }
    let journal = config.auth_file.with_file_name("web-fsops.json");
    validate_auth_path(&config.db_path, &journal)?;
    if journal == config.auth_file
        || config
            .auth_file
            .file_name()
            .is_some_and(|n| n == "web-uploads.json")
    {
        return Err("Journal kollidiert mit Auth-Datei".into());
    }
    let mut protected = Vec::new();
    for file in [&config.db_path, &config.auth_file] {
        protected.push((
            parent(file)
                .canonicalize()
                .map_err(|_| "Schutzverzeichnis nicht zugänglich")?,
            Protection::Forbidden,
        ));
        if file.exists() {
            protected.push((
                parent(
                    &file
                        .canonicalize()
                        .map_err(|_| "Schutzdatei nicht zugänglich")?,
                )
                .to_path_buf(),
                Protection::Forbidden,
            ));
        }
    }
    let old_trash = parent(&config.db_path).join("trash");
    if old_trash.exists() {
        protected.push((old_trash, Protection::Forbidden));
    }
    if let Some(path) = &config.static_dir {
        protected.push((path.clone(), Protection::Forbidden));
    }
    if let Some(upload) = &config.upload {
        protected.push((upload.dir.clone(), Protection::Upload));
    }
    let conn = katalog_core::db::open_read_only(&config.db_path)
        .map_err(|_| "Katalog nicht zugänglich")?;
    let base: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key='catalog_base_dir'",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(base) = base.filter(|s| !s.is_empty()) {
        let path = PathBuf::from(base);
        if path.exists() {
            protected.push((path, Protection::CatalogBase));
        }
    }
    let roots: Vec<PathBuf> = config
        .file_roots
        .iter()
        .map(|p| {
            p.canonicalize()
                .map_err(|_| "Dateiwurzel nicht zugänglich".to_owned())
        })
        .collect::<Result<_, _>>()?;
    if roots.is_empty() {
        return Err("Keine Dateiwurzeln konfiguriert".into());
    }
    let context = FsContext::new(&roots, &protected)
        .map_err(|e| format!("Dateiverwaltung: Startprüfung {e}"))?;
    for root in &context.roots {
        let id = crate::upload_store::random_id().map_err(|_| "Schreibprobe fehlgeschlagen")?;
        let name = format!(".fsops-probe-{id}");
        let target = format!(".fsops-target-{id}");
        let result = (|| -> std::io::Result<()> {
            let mut file = root.handle.create(&name)?;
            file.write_all(b"fileops probe")?;
            file.sync_all()?;
            root.trash.create(&target)?.sync_all()?;
            // An existing target must survive; success would mean unsafe replacement.
            match root.handle.rename_noreplace(&name, &root.trash, &target) {
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e),
                Ok(()) => return Err(std::io::Error::other("noreplace")),
            }
            root.trash.unlink(&target)?;
            root.handle.rename_noreplace(&name, &root.trash, &target)?;
            root.trash.sync()?;
            root.trash.rename_noreplace(&target, &root.handle, &name)?;
            root.handle.sync()
        })();
        let _ = root.handle.unlink(&name);
        let _ = root.trash.unlink(&target);
        let cleanup = root.handle.sync().and_then(|_| root.trash.sync());
        result.and(cleanup).map_err(|_| "Dateiverwaltung benötigt schreibbare Wurzeln und atomisches NOREPLACE im selben Mount".to_owned())?;
    }
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    struct Fixture {
        path: PathBuf,
        config: Config,
    }
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "fsops-start-{}",
                crate::upload_store::random_id().unwrap()
            ));
            for d in ["models", "data", "auth", "static"] {
                std::fs::create_dir_all(path.join(d)).unwrap();
            }
            let db = path.join("data/catalog.db");
            drop(katalog_core::db::connect(&db).unwrap());
            let values = std::collections::HashMap::from([
                ("KATALOG_DB", db.to_string_lossy().into_owned()),
                (
                    "KATALOG_ROOTS",
                    path.join("models").to_string_lossy().into_owned(),
                ),
                (
                    "KATALOG_AUTH_FILE",
                    path.join("auth/auth.json").to_string_lossy().into_owned(),
                ),
                ("KATALOG_FILE_OPS", "1".into()),
                ("KATALOG_WRITE", "1".into()),
                ("KATALOG_ALLOWED_HOSTS", "localhost".into()),
            ]);
            let config = Config::from_lookup(|k| values.get(k).cloned()).unwrap();
            Self { path, config }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
    #[test]
    fn startup_probes_and_private_per_root_trash() {
        let f = Fixture::new();
        let ctx = context(&f.config).unwrap();
        assert_eq!(ctx.roots.len(), 1);
        let trash = f.path.join("models/.katalog-papierkorb");
        assert_eq!(
            std::fs::metadata(trash).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(std::fs::read_dir(f.path.join("models")).unwrap().count(), 1);
    }
    #[test]
    fn startup_rejects_overlap_protected_alias_and_unsafe_trash() {
        let mut f = Fixture::new();
        f.config.file_roots = vec![f.path.clone()];
        assert!(context(&f.config).is_err());
        f.config.file_roots = vec![f.path.join("models"), f.path.join("models")];
        assert!(context(&f.config).is_err());
        f.config.file_roots = vec![f.path.join("models")];
        let trash = f.path.join("models/.katalog-papierkorb");
        if trash.exists() {
            std::fs::remove_dir(&trash).unwrap();
        }
        symlink(f.path.join("data"), &trash).unwrap();
        assert!(context(&f.config).is_err());
        std::fs::remove_file(&trash).unwrap();
        std::fs::create_dir(&trash).unwrap();
        std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(context(&f.config).is_err());
    }
    #[test]
    fn startup_readonly_rejects_nonempty_journal_and_unknown_ack() {
        let mut f = Fixture::new();
        f.config.file_ops = false;
        f.config.write = false;
        std::fs::write(f.path.join("auth/web-fsops.json"), b"invalid journal").unwrap();
        assert!(crate::fileops_startup::check_readonly(&f.config).is_err());
        std::fs::remove_file(f.path.join("auth/web-fsops.json")).unwrap();
        f.config.write = true;
        f.config.file_ops_ack = vec!["0".repeat(32)];
        let db = std::sync::Arc::new(std::sync::Mutex::new(
            katalog_core::db::open_read_write(&f.config.db_path).unwrap(),
        ));
        assert!(crate::fileops_startup::prepare(&f.config, db, None).is_err());
    }
    fn pending(
        f: &Fixture,
        mismatch: bool,
    ) -> (
        std::sync::Arc<std::sync::Mutex<katalog_core::write::Connection>>,
        String,
    ) {
        use katalog_core::fsops::{FileIdentity, FileState, Intent, Operation, Phase};
        use std::os::unix::fs::MetadataExt;
        let ctx = context(&f.config).unwrap();
        let source = f.path.join("models/a.stl");
        let target = f.path.join("models/b.stl");
        std::fs::write(&source, b"model").unwrap();
        let m = std::fs::metadata(&source).unwrap();
        let conn = katalog_core::db::open_read_write(&f.config.db_path).unwrap();
        conn.execute("INSERT INTO files(id,name,path,file_type,file_size_bytes,imported_at) VALUES(1,'a.stl',?1,'stl',5,'2026-01-01T00:00:00Z')",[source.to_str().unwrap()]).unwrap();
        let before = FileState {
            id: 1,
            path: source.to_str().unwrap().into(),
            name: "a.stl".into(),
            folder_id: None,
            deleted_at: None,
            trash_path: None,
        };
        let mut after = before.clone();
        after.path = target.to_str().unwrap().into();
        after.name = "b.stl".into();
        let id = "1234567890abcdef1234567890abcdef".to_owned();
        let intent = Intent {
            op_id: id.clone(),
            operation: Operation::Rename,
            phase: Phase::Prepared,
            root_path: ctx.roots[0].path.to_str().unwrap().into(),
            root_identity: ctx.roots[0].identity,
            source: Some("a.stl".into()),
            target: Some("b.stl".into()),
            temporary: None,
            identity: Some(FileIdentity {
                dev: m.dev(),
                ino: if mismatch { m.ino() + 1 } else { m.ino() },
                size: m.len(),
            }),
            before,
            after: Some(after),
        };
        let journal = f.path.join("auth/web-fsops.json");
        std::fs::write(
            &journal,
            serde_json::to_vec(&serde_json::json!({"version":1,"intents":[intent],"folders":[]}))
                .unwrap(),
        )
        .unwrap();
        std::fs::set_permissions(journal, std::fs::Permissions::from_mode(0o600)).unwrap();
        let entry = crate::upload_store::Entry {
            id: "upload-one".into(),
            name: "a.stl".into(),
            size: 5,
            dev: m.dev(),
            ino: m.ino(),
            time: 1,
            state: crate::upload_store::UploadState::Imported,
            job_id: None,
            result: Some(serde_json::json!({"fileId":"1"})),
        };
        std::fs::write(
            f.path.join("auth/web-uploads.json"),
            serde_json::to_vec(&serde_json::json!({"entries":{"upload-one":entry},"jobs":{}}))
                .unwrap(),
        )
        .unwrap();
        std::fs::rename(source, target).unwrap();
        (std::sync::Arc::new(std::sync::Mutex::new(conn)), id)
    }
    #[test]
    fn startup_recovery_forwards_before_upload_reconciliation_even_switch_off() {
        for enabled in [true, false] {
            let mut f = Fixture::new();
            let (db, _) = pending(&f, false);
            let store = crate::upload_store::UploadStore::load(
                f.path.join("models"),
                f.path.join("auth/web-uploads.json"),
                f.path.join("data"),
                1024,
                10,
                &std::collections::HashMap::new(),
            )
            .unwrap();
            assert_eq!(
                store.list().unwrap().len(),
                1,
                "load must not discard missing source"
            );
            f.config.file_ops = enabled;
            let ops = crate::fileops_startup::prepare(&f.config, db.clone(), Some(store.clone()))
                .unwrap()
                .unwrap();
            assert!(ops.journal.is_empty());
            assert!(store.list().unwrap().is_empty());
            let path: String = db
                .lock()
                .unwrap()
                .query_row("SELECT path FROM files WHERE id=1", [], |r| r.get(0))
                .unwrap();
            assert!(path.ends_with("/b.stl"));
            store
                .reconcile_catalog(
                    &std::collections::HashMap::from([(PathBuf::from(path.clone()), "1".into())]),
                    &std::collections::HashMap::from([(PathBuf::from(path), None)]),
                )
                .unwrap();
            assert!(store.list().unwrap().is_empty());
        }
    }
    #[test]
    fn startup_manual_persists_until_ack_and_readonly_rejects_valid_pending() {
        let mut f = Fixture::new();
        let (db, id) = pending(&f, true);
        f.config.file_ops = false;
        f.config.write = false;
        assert!(crate::fileops_startup::check_readonly(&f.config).is_err());
        f.config.write = true;
        let ops = crate::fileops_startup::prepare(&f.config, db.clone(), None)
            .unwrap()
            .unwrap();
        assert!(ops.journal.is_blocked());
        drop(ops);
        let ops = crate::fileops_startup::prepare(&f.config, db.clone(), None)
            .unwrap()
            .unwrap();
        assert_eq!(ops.journal.pending_op_ids(), vec![id.clone()]);
        drop(ops);
        f.config.file_ops_ack = vec![id];
        let ops = crate::fileops_startup::prepare(&f.config, db, None)
            .unwrap()
            .unwrap();
        assert!(!ops.journal.is_blocked());
        assert!(ops.journal.is_empty());
        assert_eq!(
            std::fs::read(f.path.join("models/b.stl")).unwrap(),
            b"model"
        );
    }
}
