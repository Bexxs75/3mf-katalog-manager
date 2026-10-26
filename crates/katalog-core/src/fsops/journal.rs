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
    CompanionsMoved,
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
    /// One companion file goes to the web trash on its own; the model stays.
    CompanionTrash,
    /// One companion file comes back from the web trash on its own.
    CompanionRestore,
    /// One companion file that was removed on its own is unlinked for good (empty trash).
    CompanionPurge,
    /// A new companion file is written next to a model; the model stays.
    CompanionAdd,
}
impl Operation {
    /// Operations of format version 3: they act on a single companion, not on a model.
    pub fn is_companion_edit(self) -> bool {
        matches!(self, Self::CompanionTrash | Self::CompanionRestore | Self::CompanionPurge | Self::CompanionAdd)
    }
}
/// One `companion_files` row as the journal remembers it, so recovery can tell
/// "database still as before" from "commit already happened" row by row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionState {
    pub id: i64,
    pub file_id: i64,
    pub name: String,
    pub path: String,
    pub deleted_at: Option<String>,
    pub trash_path: Option<String>,
}
/// A companion file that travels with its model. `source`/`target` are root-relative
/// and absent when the file is gone from disk and only its row is rewritten.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanionStep {
    pub before: CompanionState,
    pub after: Option<CompanionState>,
    pub source: Option<String>,
    pub target: Option<String>,
    pub identity: Option<FileIdentity>,
}
/// The new companion file of a `CompanionAdd` entry. The row id only exists after the
/// insert, so the entry names the row by what it will hold (`file_id`, `added_at`).
/// `temp_inode` is recorded right after the temporary file was created, before any byte
/// is written, `identity` once the bytes are on disk; recovery only ever removes the
/// very inodes it finds recorded here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanionAdd {
    pub file_id: i64,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub added_at: String,
    /// Root-relative: `<extras>/.fsops-<op_id>.part`.
    pub temporary: String,
    /// Root-relative: `<extras>/<free name>`.
    pub target: String,
    pub temp_inode: Option<(u64, u64)>,
    pub identity: Option<FileIdentity>,
}
/// A companion folder this operation created. Its inode is only known after
/// `mkdir` returned; recovery never removes a folder whose inode was not recorded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtrasDir {
    pub relative: String,
    pub identity: Option<Identity>,
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
    /// Companion data only appears for operations that carry companions; an entry
    /// without it is byte-identical to the format before companions existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companions: Vec<CompanionStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras_dir: Option<ExtrasDir>,
    /// Every row of the model before the operation (trashed ones included).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companion_rows: Vec<CompanionState>,
    /// Only for `CompanionAdd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_add: Option<CompanionAdd>,
}
impl Intent {
    pub(crate) fn has_companion_data(&self) -> bool {
        !self.companions.is_empty() || self.extras_dir.is_some() || !self.companion_rows.is_empty()
    }
}
/// Format 1 predates companion data. Entries with companion data are written as
/// version 2 so an older binary refuses the file instead of ignoring the new fields.
/// Version 3 is only written while an entry acts on a single companion file; an older
/// binary then stops with `RestartRequired` instead of meeting an unknown operation.
fn document_version(intents: &[Intent], folders: &[FolderIntent]) -> u32 {
    if intents.iter().any(|i| i.operation.is_companion_edit()) {
        3
    } else if intents.iter().any(Intent::has_companion_data) || folders.iter().any(FolderIntent::has_companion_data) {
        2
    } else {
        1
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    intents: Vec<Intent>,
    #[serde(default)]
    folders: Vec<FolderIntent>,
}
pub struct Journal {
    dir: Directory,
    pub(crate) intents: Vec<Intent>,
    pub(crate) folders: Vec<FolderIntent>,
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
        let (intents, folders) = match dir.read(NAME) {
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
                // A file never carries data of a newer format than its version says, and a
                // version-3 file exists only while it holds a single-companion entry.
                let needed = document_version(&doc.intents, &doc.folders);
                if !matches!(doc.version, 1..=3)
                    || (doc.version == 1 && needed != 1)
                    || (doc.version == 2 && needed > 2)
                    || (doc.version == 3 && needed != 3)
                {
                    return Err(Error::RestartRequired);
                }
                // The add-specific record belongs to exactly the add operation.
                if doc.intents.iter().any(|i| (i.operation == Operation::CompanionAdd) != i.companion_add.is_some()) {
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
                if doc.folders.iter().any(|i| {
                    i.op_id.len() != 32
                        || !i.op_id.bytes().all(|b| b.is_ascii_hexdigit())
                        || !ids.insert(i.op_id.clone())
                }) {
                    return Err(Error::RestartRequired);
                }
                (doc.intents, doc.folders)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), Vec::new()),
            Err(_) => return Err(Error::RestartRequired),
        };
        let blocked = !intents.is_empty() || !folders.is_empty();
        Ok(Self {
            dir,
            intents,
            folders,
            blocked,
            #[cfg(test)]
            crash_at: None,
            #[cfg(test)]
            fail_sync: false,
        })
    }
    /// Removes `.fsops-<id>.part` files left behind by a crash between create and
    /// rename. Only regular files owned by this user and untouched for `max_age`
    /// qualify, so a concurrent writer's fresh temp file is never taken. Returns
    /// the number of removed files. Not for read-only startup (it writes).
    pub fn remove_stale_temp_files(&self, max_age: std::time::Duration) -> usize {
        let Ok(names) = self.dir.names() else { return 0 };
        let mut removed = 0;
        for name in names {
            let is_temp = name
                .strip_prefix(".fsops-")
                .and_then(|n| n.strip_suffix(".part"))
                .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()));
            if !is_temp {
                continue;
            }
            // `read` refuses symlinks and anything but regular files.
            let Ok(file) = self.dir.read(&name) else { continue };
            let Ok(meta) = file.metadata() else { continue };
            let old_enough = meta
                .modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .is_some_and(|age| age >= max_age);
            if meta.uid() == unsafe { libc::geteuid() } && old_enough && self.dir.unlink(&name).is_ok() {
                removed += 1;
            }
        }
        removed
    }
    pub fn is_empty(&self) -> bool {
        self.intents.is_empty() && self.folders.is_empty()
    }
    pub fn is_blocked(&self) -> bool {
        self.blocked
    }
    pub fn pending_op_ids(&self) -> Vec<String> {
        self.intents
            .iter()
            .map(|i| i.op_id.clone())
            .chain(self.folders.iter().map(|i| i.op_id.clone()))
            .collect()
    }
    /// Only the entries that wait for a person. An entry still running (a long upload) or one
    /// that recovery rolls back by itself is not acknowledged, and naming it would make the
    /// acknowledgement fail at the next start.
    pub fn pending_manual_op_ids(&self) -> Vec<String> {
        self.intents
            .iter()
            .filter(|i| i.phase == Phase::Manual)
            .map(|i| i.op_id.clone())
            .chain(self.folders.iter().filter(|i| i.phase == Phase::Manual).map(|i| i.op_id.clone()))
            .collect()
    }
    /// Read-only startup must reject any pending intent, even when fileops is disabled.
    pub fn check_startup(&self, write_mode: bool) -> Result<()> {
        if !write_mode && !self.is_empty() {
            Err(Error::RestartRequired)
        } else {
            Ok(())
        }
    }
    /// Call only after recover(): ACK is exclusively for manually inspected intents.
    pub fn acknowledge(&mut self, ids: &[String]) -> Result<()> {
        if ids.iter().any(|id| {
            !self
                .intents
                .iter()
                .any(|i| i.op_id == *id && i.phase == Phase::Manual)
                && !self
                    .folders
                    .iter()
                    .any(|i| i.op_id == *id && i.phase == Phase::Manual)
        }) {
            return Err(Error::InvalidArguments);
        }
        self.intents.retain(|i| !ids.contains(&i.op_id));
        self.folders.retain(|i| !ids.contains(&i.op_id));
        self.persist()?;
        self.blocked = !self.is_empty();
        Ok(())
    }
    pub(super) fn check_capacity(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&Document {
            version: document_version(&self.intents, &self.folders),
            intents: self.intents.clone(),
            folders: self.folders.clone(),
        })
        .map_err(|_| Error::Internal)?;
        // Leave room for phase changes and the inode recorded after mkdir.
        if bytes.len() > 16 * 1024 * 1024 - 4096 {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
    pub(crate) fn persist(&mut self) -> Result<()> {
        let temp = format!(".fsops-{}.part", op_id()?);
        let result = (|| -> Result<()> {
            let mut file = self.dir.create(&temp)?;
            serde_json::to_writer(
                &mut file,
                &Document {
                    version: document_version(&self.intents, &self.folders),
                    intents: self.intents.clone(),
                    folders: self.folders.clone(),
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
        if let Err(e) = self.check_capacity() {
            self.intents.pop();
            return Err(e);
        }
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
    /// Stops the process at an arbitrary point as a crash would: nothing is
    /// rolled back and the journal stays blocked.
    #[cfg(test)]
    pub(super) fn simulate_crash(&mut self) -> Error {
        self.blocked = true;
        Error::RestartRequired
    }
    pub(super) fn checkpoint(&mut self, _phase: Phase) -> Result<()> {
        #[cfg(test)]
        if self.crash_at == Some(_phase) {
            self.blocked = true;
            return Err(Error::RestartRequired);
        }
        Ok(())
    }
    pub(crate) fn remove(&mut self, id: &str) -> Result<()> {
        self.intents.retain(|i| i.op_id != id);
        self.folders.retain(|i| i.op_id != id);
        self.persist()
    }
    pub(crate) fn block(&mut self, id: &str) -> Error {
        self.blocked = true;
        if let Some(i) = self.intents.iter_mut().find(|i| i.op_id == id) {
            i.phase = Phase::Manual;
        }
        if let Some(i) = self.folders.iter_mut().find(|i| i.op_id == id) {
            i.phase = Phase::Manual;
        }
        let _ = self.persist();
        log::error!("fileops requires manual review: opId={id}");
        Error::RestartRequired
    }
    pub(crate) fn recovery_complete(&mut self) {
        self.blocked = !self.is_empty();
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
    /// The model is still at its source but some companions already moved: put them back.
    RollbackCompanions,
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
    /// The model's companion rows equal the journal's picture from before the operation.
    pub companions_db_before: bool,
    /// Every row the operation rewrites already has its new state.
    pub companions_db_after: bool,
    /// Every companion that travels on disk is still at its source (true when none travels).
    pub companions_all_source: bool,
    /// Every companion that travels on disk is at its target (true when none travels).
    pub companions_all_target: bool,
    /// Each companion is at its source or its target, but not all at the same side.
    pub companions_mixed: bool,
    /// At least one companion travels on disk.
    pub companions_any: bool,
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
    (
        RecoveryDecision::RollbackCompanions,
        "DB before, model at its source, companions partly or fully at their targets",
    ),
    (
        RecoveryDecision::FinishUnlink,
        "delete with companions: DB before, some file already unlinked; the rest follows",
    ),
];
pub fn recovery_decision(f: RecoveryFacts) -> RecoveryDecision {
    use RecoveryDecision::*;
    if !f.trusted {
        return Manual;
    }
    if f.db_after
        && f.companions_db_after
        && (f.db_only
            || (f.unlink && f.source_absent)
            || (!f.unlink && f.source_absent && f.target_matches && f.companions_all_target))
    {
        return Committed;
    }
    if f.temporary_matches && f.source_absent {
        if f.db_before {
            // The model only reaches its temporary name after every companion moved.
            return if f.companions_db_before && f.companions_all_target {
                FinishTemporary
            } else {
                Manual
            };
        }
        // Moving only the model back would strand companions at the target.
        return if f.companions_any { Manual } else { RollbackTemporary };
    }
    if !f.db_before {
        return Manual;
    }
    if f.db_only {
        return Discard;
    }
    if f.unlink && f.companions_any {
        // A companion that is already unlinked cannot come back, so a delete never
        // goes backwards once the first file is gone.
        if !f.companions_db_before || !(f.companions_all_source || f.companions_all_target || f.companions_mixed) {
            return Manual;
        }
        if f.source_matches && f.companions_all_source {
            return Discard;
        }
        return if f.source_matches || f.source_absent { FinishUnlink } else { Manual };
    }
    if f.source_matches && !f.target_matches {
        if f.companions_all_source {
            return Discard;
        }
        return if f.companions_mixed || f.companions_all_target {
            RollbackCompanions
        } else {
            Manual
        };
    }
    if f.unlink && f.source_absent {
        return FinishUnlink;
    }
    if f.source_absent && f.target_matches {
        return if f.companions_all_target && f.companions_db_before {
            Finish
        } else {
            Manual
        };
    }
    Manual
}
