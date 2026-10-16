use super::*;
use std::{
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::Path,
};
const NAME: &str = "web-fsops.json";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Prepared,
    TmpRenamed,
    Renamed,
    Unlinked,
    Committed,
    Manual,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Rename,
    Move,
    Trash,
    Restore,
    Unlink,
    DbOnly,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub folder_id: Option<i64>,
    pub deleted_at: Option<String>,
    pub trash_path: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub dev: u64,
    pub ino: u64,
    pub size: u64,
}
impl FileIdentity {
    pub fn read(r: &Resolved) -> Result<Self> {
        let f = r.parent.read(&r.name)?;
        let m = f.metadata()?;
        if matches!((r.parent.mount_id()?,r.parent.entry_mount_id(&r.name)?),(Some(a),Some(b)) if a!=b)
        {
            return Err(Error::CrossDevice);
        }
        if m.nlink() != 1 {
            return Err(Error::PathUnresolvable);
        }
        if r.parent.entry_identity(&r.name)? != (m.dev(), m.ino()) {
            return Err(Error::Changed);
        }
        Ok(Self {
            dev: m.dev(),
            ino: m.ino(),
            size: m.len(),
        })
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub op_id: String,
    pub operation: Operation,
    pub phase: Phase,
    pub root_path: String,
    pub root_identity: Identity,
    pub source: Option<String>,
    pub target: Option<String>,
    pub temporary: Option<String>,
    pub identity: Option<FileIdentity>,
    pub before: FileState,
    pub after: Option<FileState>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    intents: Vec<Intent>,
}
pub struct Journal {
    dir: Directory,
    pub(crate) intents: Vec<Intent>,
    blocked: bool,
    #[cfg(test)]
    pub(crate) crash_at: Option<Phase>,
    #[cfg(test)]
    pub(crate) fail_sync: bool,
}
pub(crate) fn op_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        let dir = Directory::open(path)?;
        let intents = match dir.read(NAME) {
            Ok(file) => {
                let m = file.metadata()?;
                if m.nlink() != 1
                    || m.uid() != unsafe { libc::geteuid() }
                    || m.mode() & 0o777 != 0o600
                    || m.len() > 16 * 1024 * 1024
                {
                    return Err(Error::RestartRequired);
                }
                let doc: Document =
                    serde_json::from_reader(file).map_err(|_| Error::RestartRequired)?;
                if doc.version != 1 {
                    return Err(Error::RestartRequired);
                }
                let mut ids = std::collections::HashSet::new();
                if doc.intents.iter().any(|i| {
                    i.op_id.len() != 32
                        || !i.op_id.bytes().all(|b| b.is_ascii_hexdigit())
                        || !ids.insert(i.op_id.clone())
                }) {
                    return Err(Error::RestartRequired);
                }
                doc.intents
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => return Err(Error::RestartRequired),
        };
        let blocked = !intents.is_empty();
        Ok(Self {
            dir,
            intents,
            blocked,
            #[cfg(test)]
            crash_at: None,
            #[cfg(test)]
            fail_sync: false,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }
    pub fn is_blocked(&self) -> bool {
        self.blocked
    }
    pub fn pending_op_ids(&self) -> Vec<String> {
        self.intents.iter().map(|i| i.op_id.clone()).collect()
    }
    /// Read-only startup must reject any pending intent, even when fileops is disabled.
    pub fn check_startup(&self, write_mode: bool) -> Result<()> {
        if !write_mode && !self.is_empty() {
            Err(Error::RestartRequired)
        } else {
            Ok(())
        }
    }
    pub fn acknowledge(&mut self, ids: &[String]) -> Result<()> {
        self.intents.retain(|i| !ids.contains(&i.op_id));
        self.persist()?;
        self.blocked = !self.intents.is_empty();
        Ok(())
    }
    pub(crate) fn persist(&mut self) -> Result<()> {
        let temp = format!(".fsops-{}.part", op_id()?);
        let result = (|| -> Result<()> {
            let mut file = self.dir.create(&temp)?;
            serde_json::to_writer(
                &mut file,
                &Document {
                    version: 1,
                    intents: self.intents.clone(),
                },
            )
            .map_err(|_| Error::Internal)?;
            file.flush()?;
            file.sync_all()?;
            #[cfg(test)]
            if self.fail_sync {
                return Err(Error::Internal);
            }
            self.dir.rename(&temp, NAME)?;
            self.dir.sync()?;
            Ok(())
        })();
        if result.is_err() {
            self.blocked = true;
            let _ = self.dir.unlink(&temp);
            return Err(Error::RestartRequired);
        }
        Ok(())
    }
    pub(crate) fn add(&mut self, intent: Intent) -> Result<()> {
        if self.blocked {
            return Err(Error::RestartRequired);
        }
        let phase = intent.phase;
        self.intents.push(intent);
        self.persist()?;
        self.checkpoint(phase)
    }
    pub(crate) fn phase(&mut self, id: &str, phase: Phase) -> Result<()> {
        self.intents
            .iter_mut()
            .find(|i| i.op_id == id)
            .ok_or(Error::Internal)?
            .phase = phase;
        self.persist()?;
        self.checkpoint(phase)
    }
    fn checkpoint(&mut self, _phase: Phase) -> Result<()> {
        #[cfg(test)]
        if self.crash_at == Some(_phase) {
            self.blocked = true;
            return Err(Error::RestartRequired);
        }
        Ok(())
    }
    pub(crate) fn remove(&mut self, id: &str) -> Result<()> {
        self.intents.retain(|i| i.op_id != id);
        self.persist()
    }
    pub(crate) fn block(&mut self, id: &str) -> Error {
        self.blocked = true;
        if let Some(i) = self.intents.iter_mut().find(|i| i.op_id == id) {
            i.phase = Phase::Manual;
        }
        let _ = self.persist();
        log::error!("fileops requires manual review: opId={id}");
        Error::RestartRequired
    }
    pub(crate) fn recovery_complete(&mut self) {
        self.blocked = !self.intents.is_empty();
    }
}
/// Observable facts, evaluated in priority order. Phase markers can lag the filesystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryDecision {
    Discard,
    Finish,
    FinishUnlink,
    FinishTemporary,
    RollbackTemporary,
    Committed,
    Manual,
}
#[derive(Clone, Copy, Debug)]
pub struct RecoveryFacts {
    pub trusted: bool,
    pub db_before: bool,
    pub db_after: bool,
    pub source_matches: bool,
    pub source_absent: bool,
    pub target_matches: bool,
    pub temporary_matches: bool,
    pub unlink: bool,
    pub db_only: bool,
}
pub const RECOVERY_TABLE: &[(RecoveryDecision, &str)] = &[
    (
        RecoveryDecision::Manual,
        "untrusted identity/state or unresolved combination",
    ),
    (
        RecoveryDecision::Committed,
        "DB after, matching destination (or absent unlinked source)",
    ),
    (
        RecoveryDecision::Discard,
        "DB before and intact source; foreign target is harmless",
    ),
    (
        RecoveryDecision::Finish,
        "DB before, source absent, matching destination",
    ),
    (
        RecoveryDecision::FinishTemporary,
        "matching temporary and unchanged DB",
    ),
    (
        RecoveryDecision::RollbackTemporary,
        "matching temporary and changed DB",
    ),
    (
        RecoveryDecision::FinishUnlink,
        "DB before and unlink source absent",
    ),
];
pub fn recovery_decision(f: RecoveryFacts) -> RecoveryDecision {
    use RecoveryDecision::*;
    if !f.trusted {
        return Manual;
    }
    if f.db_after
        && (f.db_only
            || (f.unlink && f.source_absent)
            || (!f.unlink && f.source_absent && f.target_matches))
    {
        return Committed;
    }
    if f.temporary_matches && f.source_absent {
        return if f.db_before {
            FinishTemporary
        } else {
            RollbackTemporary
        };
    }
    if !f.db_before {
        return Manual;
    }
    if f.db_only || (f.source_matches && !f.target_matches) {
        return Discard;
    }
    if f.unlink && f.source_absent {
        return FinishUnlink;
    }
    if f.source_absent && f.target_matches {
        return Finish;
    }
    Manual
}
