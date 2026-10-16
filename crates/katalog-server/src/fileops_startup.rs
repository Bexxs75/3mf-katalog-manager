//! Startup order: load upload ledger, recover filesystem intents, apply ACK,
//! finish durable ledger updates, then let the caller reconcile the uploads.
use crate::{
    config::Config,
    upload_store::{OfflineLedger, UploadStore},
};
use katalog_core::fsops::{self, FileOps, FsDatabase, Identity, Journal, UploadLedger};
use katalog_core::write::Connection;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

pub(crate) struct DbAdapter {
    connection: Arc<Mutex<Connection>>,
    pub(crate) cancellation: Option<crate::fileops_cancel::Cancellation>,
}
impl DbAdapter {
    fn new(connection: Arc<Mutex<Connection>>) -> Self {
        Self {
            connection,
            cancellation: None,
        }
    }
}
impl FsDatabase for DbAdapter {
    fn with_conn<T>(
        &mut self,
        f: impl FnOnce(&mut Connection) -> fsops::Result<T>,
    ) -> fsops::Result<T> {
        let mut conn = self.connection.lock().map_err(|_| fsops::Error::Internal)?;
        if let Some(cancel) = &self.cancellation {
            let result = cancel
                .run(&mut conn, f)
                .map_err(|_| fsops::Error::Internal)?;
            if cancel.interrupted() {
                Err(fsops::Error::NotStarted)
            } else {
                result
            }
        } else {
            f(&mut conn)
        }
    }
}
pub(crate) struct ServerLedger {
    store: Option<Arc<UploadStore>>,
    offline: Option<OfflineLedger>,
    legacy: HashMap<i64, Identity>,
}
impl ServerLedger {
    fn entries(&self) -> fsops::Result<Vec<crate::upload_store::Entry>> {
        if let Some(store) = &self.store {
            store.list().map_err(|_| fsops::Error::RestartRequired)
        } else {
            Ok(self
                .offline
                .as_ref()
                .map(|s| s.entries())
                .unwrap_or_default())
        }
    }
}
impl UploadLedger for ServerLedger {
    fn detach(&mut self, ids: &[i64]) -> fsops::Result<()> {
        if let Some(store) = &self.store {
            store
                .detach(ids)
                .map_err(|_| fsops::Error::RestartRequired)?;
        }
        if let Some(store) = &mut self.offline {
            store
                .update(ids, false)
                .map_err(|_| fsops::Error::RestartRequired)?;
        }
        for id in ids {
            self.legacy.remove(id);
        }
        Ok(())
    }
    fn reset_imported(&mut self, ids: &[i64]) -> fsops::Result<()> {
        if let Some(store) = &self.store {
            store
                .reset_removed(ids)
                .map_err(|_| fsops::Error::RestartRequired)?;
        }
        if let Some(store) = &mut self.offline {
            store
                .update(ids, true)
                .map_err(|_| fsops::Error::RestartRequired)?;
        }
        for id in ids {
            self.legacy.remove(id);
        }
        Ok(())
    }
    fn identity(&mut self, id: i64) -> fsops::Result<Option<Identity>> {
        for e in self.entries()? {
            if e.result
                .as_ref()
                .and_then(|r| r.get("fileId"))
                .and_then(serde_json::Value::as_str)
                .and_then(|s| s.parse::<i64>().ok())
                == Some(id)
            {
                return Ok(Some(Identity {
                    dev: e.dev,
                    ino: e.ino,
                }));
            }
        }
        Ok(self.legacy.get(&id).copied())
    }
}
fn parent(config: &Config) -> &Path {
    config
        .auth_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
pub(crate) fn check_readonly(config: &Config) -> Result<(), String> {
    if config.file_ops || !config.file_ops_ack.is_empty() {
        return Err("Dateiverwaltung und Journal-ACK erfordern Schreibmodus".into());
    }
    if parent(config).join("web-fsops.json").exists() {
        Journal::open(
            &parent(config)
                .canonicalize()
                .map_err(|_| "Journalverzeichnis nicht zugänglich")?,
        )
        .and_then(|j| j.check_startup(false))
        .map_err(|_| {
            "Nicht abgeschlossenes Dateijournal: Schreibmodus zur Recovery erforderlich".to_owned()
        })?;
    }
    Ok(())
}
pub(crate) fn prepare(
    config: &Config,
    db: Arc<Mutex<Connection>>,
    store: Option<Arc<UploadStore>>,
) -> Result<Option<FileOps<DbAdapter, ServerLedger>>, String> {
    let journal_exists = parent(config).join("web-fsops.json").exists();
    if !config.file_ops && !journal_exists && config.file_ops_ack.is_empty() {
        return Ok(None);
    }
    let journal = Journal::open(
        &parent(config)
            .canonicalize()
            .map_err(|_| "Journalverzeichnis nicht zugänglich")?,
    )
    .map_err(|_| "Dateijournal ungültig; manuell prüfen".to_owned())?;
    if !config.file_ops && journal.is_empty() && config.file_ops_ack.is_empty() {
        return Ok(None);
    }
    let context = crate::fileops_config::context(config)?;
    let offline = if store.is_none() {
        Some(
            OfflineLedger::load(
                &parent(config)
                    .canonicalize()
                    .map_err(|_| "Journalverzeichnis nicht zugänglich")?,
            )
            .map_err(|_| "Upload-Journal ungültig")?,
        )
    } else {
        None
    };
    let mut ledger = ServerLedger {
        store,
        offline,
        legacy: HashMap::new(),
    };
    // Legacy entries lack IDs: resolve only against the original upload path,
    // retaining the journal identity for the Core's pre-mutation check.
    for e in ledger.entries().map_err(|_| "Upload-Journal ungültig")? {
        if e.result
            .as_ref()
            .and_then(|r| r.get("fileId"))
            .and_then(serde_json::Value::as_str)
            .is_some()
        {
            continue;
        }
        if let Some(upload) = &config.upload {
            let path = upload.dir.join(&e.name);
            let id = {
                let conn = db.lock().map_err(|_| "Datenbanksperre")?;
                conn.query_row(
                    "SELECT id FROM files WHERE path=?1",
                    [path.to_string_lossy().as_ref()],
                    |r| r.get::<_, i64>(0),
                )
            };
            if let Ok(id) = id {
                if let Some(store) = &ledger.store {
                    store
                        .bind_legacy(&e.id, id)
                        .map_err(|_| "Upload-Journal ungültig")?;
                }
                ledger.legacy.insert(
                    id,
                    Identity {
                        dev: e.dev,
                        ino: e.ino,
                    },
                );
            }
        } else if matches!(
            e.state,
            crate::upload_store::UploadState::Imported
                | crate::upload_store::UploadState::Importing
        ) {
            return Err("Altes Upload-Journal ohne Datei-IDs: Upload-Konfiguration einmalig zur Recovery aktivieren".into());
        }
    }
    let mut ops = FileOps {
        context,
        journal,
        db: DbAdapter::new(db),
        ledger,
    };
    let report = ops
        .recover()
        .map_err(|_| "Dateijournal-Recovery fehlgeschlagen")?;
    for id in &report.manual {
        eprintln!("fileops recovery: manual review required opId={id}");
    }
    if !config.file_ops_ack.is_empty() {
        ops.journal
            .acknowledge(&config.file_ops_ack)
            .map_err(|_| "KATALOG_FILE_OPS_ACK enthält keine manuell blockierte Operations-ID")?;
    }
    // This also finishes successful intents when other intents remain manual.
    if let Err(e) = ops.finish_recovery() {
        if e != fsops::Error::RestartRequired {
            return Err("Dateijournal-Abschluss fehlgeschlagen".into());
        }
    }
    Ok(Some(ops))
}
