//! A new companion file next to a model, written from memory.
//!
//! The bytes were checked once (type, content, size) and are written exactly as checked.
//! One journal entry (format version 3) carries the whole job: the companion folder is
//! made if it is missing, the bytes go to a temporary file in that folder, the temporary
//! file is renamed to a free name without replacing anything, and one database commit
//! inserts the row. Before the commit recovery removes what the entry made (found again
//! by its recorded inode, never by name alone); after the commit everything is final.

use super::carry::{path_held, read_companion_rows};
use super::operations::{read_state, Observed};
use super::*;
use crate::companion_read::same_dir;
use crate::companions::{
    classify, companion_file_name, extras_dir_for, kind_for_extension, lowercase_extension, validate_stored_fields,
    CompanionKind, Refusal, TextScan, CONTENT_CHECK_BYTES, MAX_COMPANIONS_PER_MODEL, MAX_COMPANION_BYTES_CONFIGURABLE,
};
use rusqlite::{Connection, TransactionBehavior};
use std::{
    io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

/// What a finished upload produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddedCompanion {
    pub id: i64,
    pub file_id: i64,
    pub name: String,
    pub kind: CompanionKind,
    pub size: u64,
}

/// Free space that stays untouched by an upload, as for model uploads.
pub const UPLOAD_RESERVE: u64 = 256 * 1024 * 1024;
/// Chunk size of the copy from the source into the temporary file.
const CHUNK: usize = 256 * 1024;

/// Decides from the name and the announced size alone, before any byte is read, whether a
/// file may become a companion; returns the harmless on-disk name.
pub fn prepare_companion_name(original_name: &str, size: u64, max_bytes: u64) -> Result<String> {
    if size > max_bytes {
        return Err(Error::CompanionTooLarge);
    }
    // Only the last path component counts, whichever separator the client used.
    let last = original_name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = crate::upload_fs::nfc(last)
        .chars()
        .map(|c| if names::forbidden(c) { '_' } else { c })
        .collect();
    if lowercase_extension(&cleaned).and_then(|e| kind_for_extension(&e)).is_none() {
        return Err(Error::CompanionType);
    }
    if size == 0 {
        return Err(Error::CompanionContent);
    }
    let name = companion_file_name(&cleaned).ok_or(Error::InvalidName)?;
    if validate_name(&name).map_err(|_| Error::InvalidName)? != name {
        return Err(Error::InvalidName);
    }
    Ok(name)
}

/// Judges the start of the content against the extension.
pub fn check_companion_head(name: &str, head: &[u8]) -> Result<CompanionKind> {
    classify(name, head).map_err(|r| match r {
        Refusal::Type => Error::CompanionType,
        Refusal::Content => Error::CompanionContent,
    })
}

/// What recovery does with an interrupted upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompanionAddDecision {
    /// The row exists: the upload is done.
    Committed,
    /// Nothing of the entry is on disk: forget it.
    Discard,
    /// The entry's temporary and/or final file exist and are removed; the row never came.
    Rollback,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TempSeen {
    Gone,
    /// The inode the journal recorded.
    Mine,
    /// Created, but the inode never reached the journal; empty, so nothing was written yet.
    UnrecordedEmpty,
    /// Something else sits under the entry's own unique name.
    Other,
    /// The inode was recorded, but the folder it was created in cannot be found any more
    /// (renamed or moved), so a file of ours may sit somewhere unknown.
    Unreachable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSeen {
    Gone,
    Mine,
    /// Another file took the name; it is not ours and stays.
    Foreign,
    /// The file was renamed into a folder that cannot be found any more.
    Unreachable,
}

#[derive(Clone, Copy, Debug)]
pub struct CompanionAddFacts {
    /// A row with the entry's file id, timestamp and size exists.
    pub row_exists: bool,
    pub temp: TempSeen,
    pub target: TargetSeen,
}

pub const COMPANION_ADD_TABLE: &[(CompanionAddDecision, &str)] = &[
    (CompanionAddDecision::Committed, "the row exists"),
    (CompanionAddDecision::Manual, "no row, and the entry's own temporary name holds a file that is not ours, or the folder of a recorded file cannot be found"),
    (CompanionAddDecision::Discard, "no row and nothing of ours on disk"),
    (CompanionAddDecision::Rollback, "no row; our temporary and/or final file are removed"),
];

pub fn companion_add_decision(f: CompanionAddFacts) -> CompanionAddDecision {
    use CompanionAddDecision::*;
    if f.row_exists {
        return Committed;
    }
    if f.temp == TempSeen::Other || f.temp == TempSeen::Unreachable || f.target == TargetSeen::Unreachable {
        return Manual;
    }
    if f.temp == TempSeen::Gone && f.target != TargetSeen::Mine {
        return Discard;
    }
    Rollback
}

/// The commit of an upload, inside one immediate transaction. Returns the new row id.
pub(super) fn commit_companion_add(c: &mut Connection, i: &Intent) -> Result<i64> {
    let add = i.companion_add.as_ref().ok_or(Error::Internal)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if read_state(&tx, i.before.id)?.as_ref() != Some(&i.before) {
        return Err(Error::Changed);
    }
    if read_companion_rows(&tx, i.before.id)? != i.companion_rows {
        return Err(Error::CompanionsChanged);
    }
    let active: i64 = tx.query_row(
        "SELECT COUNT(*) FROM companion_files WHERE file_id=?1 AND deleted_at IS NULL",
        [i.before.id],
        |r| r.get(0),
    )?;
    if active >= MAX_COMPANIONS_PER_MODEL {
        return Err(Error::CompanionLimit);
    }
    let root = std::path::PathBuf::from(&i.root_path);
    let path = root.join(&add.target);
    let path = path.to_str().ok_or(Error::PathUnresolvable)?;
    if path_held(&tx, path, 0)? {
        return Err(Error::CompanionsConflict);
    }
    let id = crate::db::companions::insert_companion(
        &tx,
        add.file_id,
        &add.name,
        path,
        &add.kind,
        Some(add.size as i64),
        &add.added_at,
    )
    .map_err(|_| Error::Internal)?;
    tx.commit()?;
    Ok(id)
}

/// Why a step stopped: a (simulated) crash leaves everything as it is, a failure is undone.
enum Halt {
    Crash(Error),
    /// Failed before anything of the entry's own was created on disk.
    Early(Error),
    Fail(Error),
}

impl From<Error> for Halt {
    fn from(e: Error) -> Self {
        Self::Fail(e)
    }
}

/// One read from the body; a stalled or broken body is an upload error, not an I/O error.
fn read_some(source: &mut dyn Read, buf: &mut [u8]) -> Result<usize> {
    loop {
        return match source.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err(Error::UploadTimeout),
            Err(_) => Err(Error::UploadLength),
        };
    }
}

fn read_full(source: &mut dyn Read, buf: &mut [u8]) -> Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        match read_some(source, &mut buf[filled..])? {
            0 => return Err(Error::UploadLength),
            n => filled += n,
        }
    }
    Ok(())
}

impl From<std::io::Error> for Halt {
    fn from(e: std::io::Error) -> Self {
        Self::Fail(e.into())
    }
}

/// An upload between its journal entry and its commit. It owns the open temporary file and
/// the handle of the folder it sits in, so the bytes can be streamed without any lock and
/// the file can still be removed if the folder is renamed or moved in the meantime.
pub struct PendingCompanion {
    intent: Intent,
    root: usize,
    kind: CompanionKind,
    scan: Option<TextScan>,
    temp: Resolved,
    file: Option<std::fs::File>,
    written: u64,
    folder_identity: Option<Identity>,
    identity: Option<FileIdentity>,
}

impl PendingCompanion {
    /// The announced size of the file.
    pub fn size(&self) -> u64 {
        self.intent.companion_add.as_ref().map_or(0, |a| a.size)
    }
}

/// Reads the part of the body that is judged against the extension.
pub fn read_companion_head(size: u64, source: &mut dyn Read) -> Result<Vec<u8>> {
    let mut head = vec![0u8; size.min(CONTENT_CHECK_BYTES as u64) as usize];
    read_full(source, &mut head)?;
    Ok(head)
}

/// Writes the rest of the body into the temporary file and makes it durable. Needs no lock:
/// the file is private to the upload until it is renamed.
pub fn stream_companion(p: &mut PendingCompanion, source: &mut dyn Read) -> Result<()> {
    let size = p.size();
    let inode = {
        let file = p.file.as_ref().ok_or(Error::Internal)?;
        let meta = file.metadata()?;
        (meta.dev(), meta.ino())
    };
    let mut file = p.file.take().ok_or(Error::Internal)?;
    // One chunk at a time; the body is counted as it comes and one byte too many is an error as well.
    let mut buffer = vec![0u8; CHUNK.min(size.saturating_sub(p.written).max(1) as usize)];
    let streamed = (|| -> Result<()> {
        while p.written < size {
            let want = buffer.len().min((size - p.written) as usize);
            let got = read_some(source, &mut buffer[..want])?;
            if got == 0 {
                return Err(Error::UploadLength);
            }
            if let Some(scan) = p.scan.as_mut() {
                if !scan.feed(&buffer[..got]) {
                    return Err(Error::CompanionContent);
                }
            }
            file.write_all(&buffer[..got])?;
            p.written += got as u64;
        }
        if read_some(source, &mut buffer[..1])? != 0 {
            return Err(Error::UploadLength);
        }
        if p.scan.as_ref().is_some_and(|s| !s.finish()) {
            return Err(Error::CompanionContent);
        }
        file.flush()?;
        file.sync_all()?;
        let done = file.metadata()?;
        if done.len() != size || (done.dev(), done.ino()) != inode || done.nlink() != 1 {
            return Err(Error::Changed);
        }
        Ok(())
    })();
    p.file = Some(file);
    streamed?;
    p.file = None;
    p.identity = Some(FileIdentity { dev: inode.0, ino: inode.1, size });
    Ok(())
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    /// Streams `size` bytes from `source` into a new companion file of the model. The size is
    /// the announced one: it is bounded by `max_bytes` before anything happens, and the
    /// body must match it exactly. Nothing is held in memory but one chunk; the head is
    /// judged against the extension before any file or folder is made.
    pub fn add_companion(
        &mut self,
        file_id: i64,
        original_name: &str,
        size: u64,
        max_bytes: u64,
        source: &mut dyn Read,
    ) -> Result<AddedCompanion> {
        self.add_companion_checked(file_id, original_name, size, max_bytes, source, &mut || true)
    }

    /// As `add_companion`, and `allowed` is asked once more right before the commit; a
    /// "no" undoes the upload and ends in `NotStarted`.
    pub fn add_companion_checked(
        &mut self,
        file_id: i64,
        original_name: &str,
        size: u64,
        max_bytes: u64,
        source: &mut dyn Read,
        allowed: &mut dyn FnMut() -> bool,
    ) -> Result<AddedCompanion> {
        self.ready()?;
        if file_id <= 0 || max_bytes == 0 || max_bytes > MAX_COMPANION_BYTES_CONFIGURABLE {
            return Err(Error::InvalidArguments);
        }
        prepare_companion_name(original_name, size, max_bytes)?;
        let head = read_companion_head(size, source)?;
        let mut pending = self.begin_companion_add(file_id, original_name, size, max_bytes, &head)?;
        if let Err(e) = stream_companion(&mut pending, source) {
            return Err(self.abort_companion_add(pending, e));
        }
        self.place_companion(pending, allowed)
    }

    /// First step of an upload, with the head already read: judges it, journals the job,
    /// makes the folder if needed and creates the temporary file holding the head.
    pub fn begin_companion_add(
        &mut self,
        file_id: i64,
        original_name: &str,
        size: u64,
        max_bytes: u64,
        head: &[u8],
    ) -> Result<PendingCompanion> {
        self.ready()?;
        if file_id <= 0 || max_bytes == 0 || max_bytes > MAX_COMPANION_BYTES_CONFIGURABLE {
            return Err(Error::InvalidArguments);
        }
        let wanted_name = prepare_companion_name(original_name, size, max_bytes)?;
        if head.len() as u64 != size.min(CONTENT_CHECK_BYTES as u64) {
            return Err(Error::InvalidArguments);
        }
        let kind = check_companion_head(&wanted_name, head)?;
        let mut scan = None;
        if kind == CompanionKind::Text {
            let mut text = TextScan::default();
            if !text.feed(head) {
                return Err(Error::CompanionContent);
            }
            scan = Some(text);
        }
        let model = self.state(file_id)?;
        if model.deleted_at.is_some() {
            return Err(Error::Conflict);
        }
        let (root, _) = self.context.locate(&model.path)?;
        let extras = extras_dir_for(Path::new(&model.path)).ok_or(Error::PathUnresolvable)?;
        let extras_name = extras.file_name().and_then(|n| n.to_str()).ok_or(Error::PathUnresolvable)?;
        if validate_name(extras_name).map_err(|_| Error::InvalidName)? != extras_name {
            return Err(Error::InvalidName);
        }
        let extras_str = extras.to_str().ok_or(Error::PathUnresolvable)?.to_owned();
        let rows = self.db.with_conn(|c| read_companion_rows(c, file_id))?;
        if rows.iter().filter(|r| r.deleted_at.is_none()).count() as i64 >= MAX_COMPANIONS_PER_MODEL {
            return Err(Error::CompanionLimit);
        }
        // A folder that is a link, a file, or on another device is never written to.
        let probe = self.probe_dir(&extras_str, None)?;
        if !matches!(probe.state, DirState::Absent | DirState::Plain) {
            return Err(Error::CompanionsConflict);
        }
        let want = CompanionState { id: 0, file_id, name: wanted_name, path: String::new(), deleted_at: None, trash_path: None };
        let (name, path) = self.free_name(&probe, &extras, &want, &std::collections::HashSet::new())?;
        let op_id = journal::op_id()?;
        let temp_path = extras.join(format!(".fsops-{op_id}.part"));
        let temp_path = temp_path.to_str().ok_or(Error::PathUnresolvable)?;
        let extras_dir = if probe.state == DirState::Absent {
            Some(ExtrasDir { relative: self.relative_in(root, &extras_str)?, identity: None })
        } else {
            None
        };
        // Checked once, at the start: the free space is not reserved, so other writers may use it
        // while the body streams, and a write that then runs out fails as insufficient storage.
        let reserve = size.saturating_add(UPLOAD_RESERVE);
        if self.context.roots[root].handle.available().map_err(|_| Error::Internal)? < reserve {
            return Err(Error::InsufficientStorage);
        }
        let r = &self.context.roots[root];
        let mut intent = Intent {
            op_id,
            operation: Operation::CompanionAdd,
            phase: Phase::Prepared,
            root_path: r.path.to_str().ok_or(Error::PathUnresolvable)?.to_owned(),
            root_identity: r.identity,
            source: None,
            target: None,
            temporary: None,
            identity: None,
            before: model.clone(),
            after: Some(model),
            companions: Vec::new(),
            extras_dir,
            companion_rows: rows,
            companion_add: Some(CompanionAdd {
                file_id,
                name,
                kind: kind.as_str().to_owned(),
                size,
                added_at: chrono::Utc::now().to_rfc3339(),
                temporary: self.relative_in(root, temp_path)?,
                target: self.relative_in(root, &path)?,
                temp_inode: None,
                identity: None,
            }),
        };
        self.journal.add(intent.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
        if intent.extras_dir.is_some() {
            // Undoes itself (or blocks the journal) on failure.
            self.make_extras(&mut intent, root)?;
        }
        let expected_dir = intent.extras_dir.as_ref().and_then(|d| d.identity).or(probe.identity);
        match self.stage_temp(&mut intent, root, head, expected_dir) {
            Ok((temp, file)) => Ok(PendingCompanion {
                intent,
                root,
                kind,
                scan,
                temp,
                file: Some(file),
                written: head.len() as u64,
                folder_identity: expected_dir,
                identity: None,
            }),
            Err(halt) => Err(self.settle_halt(&intent, root, halt, None)),
        }
    }

    /// Undoes a failed upload that is still between its journal entry and its commit.
    pub fn abort_companion_add(&mut self, p: PendingCompanion, e: Error) -> Error {
        self.fail_add(&p.intent, p.root, e, Some(&p.temp))
    }

    /// Last step: renames the finished temporary file to its name and commits the row.
    /// Whatever fails here is undone, also when the folder was renamed or moved meanwhile.
    pub fn place_companion(&mut self, mut p: PendingCompanion, allowed: &mut dyn FnMut() -> bool) -> Result<AddedCompanion> {
        let (root, mut intent) = (p.root, p.intent.clone());
        match self.rename_into_place(&mut intent, &mut p) {
            Ok(()) => (),
            Err(Halt::Crash(e)) => return Err(e),
            Err(halt) => return Err(self.settle_halt(&intent, root, halt, Some(&p.temp))),
        }
        #[cfg(test)]
        if super::tests::at_point("before_commit", 0) {
            return Err(self.journal.simulate_crash());
        }
        if !allowed() {
            return Err(self.fail_add(&intent, root, Error::NotStarted, Some(&p.temp)));
        }
        let id = match self.db.with_conn(|c| commit_companion_add(c, &intent)) {
            Ok(id) => id,
            Err(e) => return Err(self.fail_add(&intent, root, e, Some(&p.temp))),
        };
        #[cfg(test)]
        if super::tests::at_point("after_commit", 0) {
            return Err(self.journal.simulate_crash());
        }
        self.finish_add(&intent)?;
        let add = intent.companion_add.as_ref().ok_or(Error::Internal)?;
        Ok(AddedCompanion { id, file_id: add.file_id, name: add.name.clone(), kind: p.kind, size: add.size })
    }

    /// Ends an interrupted step the way its kind asks for.
    fn settle_halt(&mut self, intent: &Intent, root: usize, halt: Halt, held: Option<&Resolved>) -> Error {
        match halt {
            Halt::Crash(e) => e,
            Halt::Early(e) => {
                self.remove_created_extras(intent, root);
                if let Err(e) = self.journal.remove(&intent.op_id) {
                    return e;
                }
                if e == Error::Conflict {
                    Error::CompanionsConflict
                } else {
                    e
                }
            }
            Halt::Fail(e) => self.fail_add(intent, root, e, held),
        }
    }

    /// Records a fact about the entry in the caller's copy and in the journal, on disk.
    fn note_add(&mut self, i: &mut Intent, f: impl Fn(&mut CompanionAdd)) -> Result<()> {
        if let Some(a) = i.companion_add.as_mut() {
            f(a);
        }
        let id = i.op_id.clone();
        if let Some(a) = self.journal.intents.iter_mut().find(|j| j.op_id == id).and_then(|j| j.companion_add.as_mut()) {
            f(a);
        }
        self.journal.persist()
    }

    /// Creates the temporary file in the folder that was looked at and writes the head.
    fn stage_temp(
        &mut self,
        i: &mut Intent,
        root: usize,
        head: &[u8],
        expected_dir: Option<Identity>,
    ) -> std::result::Result<(Resolved, std::fs::File), Halt> {
        let add = i.companion_add.clone().ok_or(Halt::Early(Error::Internal))?;
        let early = |e: Error| Halt::Early(e);
        // The folder is the very one that was looked at (or made); a swap since is refused.
        let (folder_rel, _) = add.temporary.rsplit_once('/').ok_or(Halt::Early(Error::Internal))?;
        let folder = self.context.resolve_relative(root, folder_rel, Access::Directory).map_err(early)?;
        if Some(self.context.directory_identity(&folder).map_err(early)?) != expected_dir {
            return Err(Halt::Early(Error::CompanionsConflict));
        }
        let temp = self.context.resolve_relative(root, &add.temporary, Access::MoveTarget).map_err(early)?;
        let mut file = temp.parent.create(&temp.name).map_err(|e| Halt::Early(e.into()))?;
        file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
        let meta = file.metadata()?;
        let inode = (meta.dev(), meta.ino());
        #[cfg(test)]
        if super::tests::at_point("after_temp_create", 0) {
            return Err(Halt::Crash(self.journal.simulate_crash()));
        }
        self.note_add(i, |a| a.temp_inode = Some(inode)).map_err(Halt::Crash)?;
        #[cfg(test)]
        if super::tests::at_point("after_temp_inode", 0) {
            return Err(Halt::Crash(self.journal.simulate_crash()));
        }
        if meta.dev() != self.context.roots[root].identity.dev {
            return Err(Error::CrossDevice.into());
        }
        file.write_all(head)?;
        #[cfg(test)]
        if super::tests::at_point("mid_write", 0) {
            return Err(Halt::Crash(self.journal.simulate_crash()));
        }
        Ok((temp, file))
    }

    /// Journals the finished bytes and moves them to their name without replacing anything.
    fn rename_into_place(&mut self, i: &mut Intent, p: &mut PendingCompanion) -> std::result::Result<(), Halt> {
        let root = p.root;
        let add = i.companion_add.clone().ok_or(Halt::Fail(Error::Internal))?;
        let identity = p.identity.ok_or(Halt::Fail(Error::Internal))?;
        // The upload may have run for a long time: the folder must still be the one it began in.
        let (folder_rel, _) = add.temporary.rsplit_once('/').ok_or(Halt::Fail(Error::Internal))?;
        let folder = self.context.resolve_relative(root, folder_rel, Access::Directory)?;
        if Some(self.context.directory_identity(&folder)?) != p.folder_identity {
            return Err(Error::CompanionsConflict.into());
        }
        self.note_add(i, |a| a.identity = Some(identity)).map_err(Halt::Crash)?;
        #[cfg(test)]
        if super::tests::at_point("after_fsync_identity", 0) {
            return Err(Halt::Crash(self.journal.simulate_crash()));
        }
        p.temp.parent.sync()?;
        let temp = self.context.resolve_relative(root, &add.temporary, Access::MoveTarget)?;
        let target = self.context.resolve_relative(root, &add.target, Access::MoveTarget)?;
        // Without replacing: a name taken meanwhile (the desktop) ends in a conflict.
        super::operations::rename_checked(&temp, &target, identity)?;
        #[cfg(test)]
        if super::tests::at_point("after_rename", 0) {
            return Err(Halt::Crash(self.journal.simulate_crash()));
        }
        self.journal.phase(&i.op_id, Phase::CompanionsMoved).map_err(Halt::Crash)?;
        Ok(())
    }

    /// Undoes a failed upload and drops its entry; if the undo itself fails the journal is blocked.
    fn fail_add(&mut self, i: &Intent, root: usize, e: Error, held: Option<&Resolved>) -> Error {
        if e == Error::RestartRequired || self.remove_added(i, root, held).is_err() {
            return self.journal.block(&i.op_id);
        }
        #[cfg(test)]
        if super::tests::at_point("after_undo", 0) {
            return self.journal.simulate_crash();
        }
        if let Err(e) = self.journal.remove(&i.op_id) {
            return e;
        }
        if e == Error::Conflict {
            Error::CompanionsConflict
        } else {
            e
        }
    }

    /// Before the folder this upload made is removed, the entry stops naming files that are
    /// already gone. A restart after the folder vanished would otherwise read "a recorded file
    /// in a folder that cannot be found" and ask for a manual review of a finished undo.
    fn forget_files(&mut self, i: &Intent) -> Result<()> {
        if i.extras_dir.as_ref().is_none_or(|d| d.identity.is_none()) {
            return Ok(());
        }
        if let Some(a) = self.journal.intents.iter_mut().find(|j| j.op_id == i.op_id).and_then(|j| j.companion_add.as_mut()) {
            a.temp_inode = None;
            a.identity = None;
        }
        self.journal.persist()
    }

    fn finish_add(&mut self, i: &Intent) -> Result<()> {
        self.journal.phase(&i.op_id, Phase::Committed)?;
        self.journal.remove(&i.op_id)
    }

    /// What sits under the entry's temporary name.
    fn temp_seen(&self, root: usize, add: &CompanionAdd) -> Result<TempSeen> {
        let r = match self.context.resolve_relative(root, &add.temporary, Access::MoveTarget) {
            Ok(r) => r,
            // A recorded inode means the file existed in a folder that is no longer there.
            Err(Error::NotFound) if add.temp_inode.is_some() => return Ok(TempSeen::Unreachable),
            Err(Error::NotFound) => return Ok(TempSeen::Gone),
            Err(e) => return Err(e),
        };
        let found = match r.parent.entry_identity(&r.name) {
            Ok(id) => id,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(TempSeen::Gone),
            Err(e) => return Err(e.into()),
        };
        match add.temp_inode {
            Some(recorded) if recorded == found => Ok(TempSeen::Mine),
            Some(_) => Ok(TempSeen::Other),
            None => {
                // Only an empty, plain, unlinked file under our unique name can be ours.
                let empty = r.parent.read(&r.name).and_then(|f| f.metadata()).is_ok_and(|m| m.len() == 0 && m.nlink() == 1);
                Ok(if empty { TempSeen::UnrecordedEmpty } else { TempSeen::Other })
            }
        }
    }

    fn target_seen(&self, root: usize, add: &CompanionAdd) -> Result<TargetSeen> {
        let Some(identity) = add.identity else { return Ok(TargetSeen::Gone) };
        Ok(match self.context.resolve_relative(root, &add.target, Access::MoveTarget) {
            Ok(r) => match observe(&r, identity)? {
                Observed::Absent => TargetSeen::Gone,
                Observed::Matches => TargetSeen::Mine,
                Observed::Foreign => TargetSeen::Foreign,
            },
            Err(Error::NotFound) => TargetSeen::Unreachable,
            Err(e) => return Err(e),
        })
    }

    /// Removes the entry's own temporary and final file (only by their recorded inode) and
    /// the folder it made. Anything else at those places is left alone and stops the undo,
    /// and so does a recorded file whose folder cannot be found. With the handle of the
    /// folder the upload works in, the files are found there even after the folder moved.
    fn remove_added(&mut self, i: &Intent, root: usize, held: Option<&Resolved>) -> Result<()> {
        let add = i.companion_add.as_ref().ok_or(Error::Internal)?;
        if let Some(held) = held {
            let last = |relative: &str| relative.rsplit('/').next().unwrap_or("").to_owned();
            if let Some(identity) = add.identity {
                let name = last(&add.target);
                match held.parent.entry_identity(&name) {
                    Ok(found) if found == (identity.dev, identity.ino) => {
                        held.parent.unlink(&name)?;
                        held.parent.sync()?;
                    }
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e.into()),
                }
            }
            match (held.parent.entry_identity(&held.name), add.temp_inode) {
                (Ok(found), Some(recorded)) if found == recorded => {
                    held.parent.unlink(&held.name)?;
                    held.parent.sync()?;
                }
                (Ok(_), None) => {
                    // Created but never recorded: ours only if it is still empty and unlinked.
                    let empty = held.parent.read(&held.name).and_then(|f| f.metadata()).is_ok_and(|m| m.len() == 0 && m.nlink() == 1);
                    if !empty {
                        return Err(Error::Changed);
                    }
                    held.parent.unlink(&held.name)?;
                    held.parent.sync()?;
                }
                (Ok(_), Some(_)) => return Err(Error::Changed),
                (Err(e), _) if e.kind() == std::io::ErrorKind::NotFound => (),
                (Err(e), _) => return Err(e.into()),
            }
            self.forget_files(i)?;
            self.remove_created_extras(i, root);
            return Ok(());
        }
        match self.target_seen(root, add)? {
            TargetSeen::Mine => {
                let r = self.context.resolve_relative(root, &add.target, Access::MoveTarget)?;
                if Some(FileIdentity::read(&r)?) != add.identity {
                    return Err(Error::Changed);
                }
                r.parent.unlink(&r.name)?;
                r.parent.sync()?;
            }
            TargetSeen::Unreachable => return Err(Error::Changed),
            TargetSeen::Gone | TargetSeen::Foreign => (),
        }
        match self.temp_seen(root, add)? {
            TempSeen::Gone => (),
            TempSeen::Mine | TempSeen::UnrecordedEmpty => {
                let r = self.context.resolve_relative(root, &add.temporary, Access::MoveTarget)?;
                r.parent.unlink(&r.name)?;
                r.parent.sync()?;
            }
            TempSeen::Other | TempSeen::Unreachable => return Err(Error::Changed),
        }
        self.forget_files(i)?;
        self.remove_created_extras(i, root);
        Ok(())
    }

    /// Settles an interrupted upload. Errors leave the entry for the caller to block.
    pub(super) fn recover_companion_add(&mut self, i: &Intent) -> Result<()> {
        let root = self
            .context
            .roots
            .iter()
            .position(|r| r.path.to_str() == Some(&i.root_path) && r.identity == i.root_identity)
            .ok_or(Error::Changed)?;
        let add = i.companion_add.as_ref().ok_or(Error::Changed)?;
        // The entry has exactly the shape this code writes; anything else was not ours.
        let extras = extras_dir_for(Path::new(&i.before.path)).ok_or(Error::Changed)?;
        let under = |relative: &str| std::path::PathBuf::from(&i.root_path).join(relative);
        let (temp, target) = (under(&add.temporary), under(&add.target));
        let inside = |p: &Path| p.parent().is_some_and(|d| same_dir(d, &extras));
        let identity_ok = add.identity.is_none_or(|f| add.temp_inode == Some((f.dev, f.ino)) && f.size == add.size);
        if !i.companions.is_empty()
            || i.source.is_some()
            || i.target.is_some()
            || i.temporary.is_some()
            || i.identity.is_some()
            || i.after.as_ref() != Some(&i.before)
            || add.file_id != i.before.id
            || !inside(&temp)
            || !inside(&target)
            || temp.file_name().and_then(|n| n.to_str()) != Some(&format!(".fsops-{}.part", i.op_id))
            || target.to_str().is_none_or(|t| validate_stored_fields(&add.name, t, &add.kind).is_err())
            || validate_name(&add.name).is_err()
            || add.size == 0
            || add.size > MAX_COMPANION_BYTES_CONFIGURABLE
            || !identity_ok
            || i.extras_dir.as_ref().is_some_and(|d| under(&d.relative) != extras)
        {
            return Err(Error::Changed);
        }
        let row_exists = self.db.with_conn(|c| {
            Ok(c.query_row(
                "SELECT EXISTS(SELECT 1 FROM companion_files WHERE file_id=?1 AND added_at=?2 AND size_bytes=?3)",
                rusqlite::params![add.file_id, add.added_at, add.size as i64],
                |r| r.get::<_, bool>(0),
            )?)
        })?;
        let facts = CompanionAddFacts { row_exists, temp: self.temp_seen(root, add)?, target: self.target_seen(root, add)? };
        match companion_add_decision(facts) {
            CompanionAddDecision::Committed => self.finish_add(i),
            CompanionAddDecision::Discard => {
                self.forget_files(i)?;
                self.remove_created_extras(i, root);
                self.journal.remove(&i.op_id)
            }
            CompanionAddDecision::Rollback => {
                self.remove_added(i, root, None)?;
                self.journal.remove(&i.op_id)
            }
            CompanionAddDecision::Manual => Err(Error::Changed),
        }
    }
}
