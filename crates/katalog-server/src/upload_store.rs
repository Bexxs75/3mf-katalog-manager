//! Instance-owned, durable upload inventory. All state transitions share one mutex.
use katalog_core::upload_fs::{self, Directory};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{self, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{SystemTime, UNIX_EPOCH},
};

const RESERVE: u64 = 256 * 1024 * 1024;
const PENDING_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const CHECK_INTERVAL: u64 = 64 * 1024 * 1024;
/// Hidden folder inside the upload folder an archive is unpacked into. Leftovers of
/// a crash are removed at startup; nothing ever looks inside one.
pub(crate) const STAGING_PREFIX: &str = ".archive-staging-";
/// Open uploads (everything not yet imported) the store accepts at most.
pub(crate) const MAX_PENDING_UPLOADS: u64 = 200;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreError {
    pub status: u16,
    pub kind: &'static str,
}
impl StoreError {
    pub(crate) fn new(status: u16, kind: &'static str) -> Self {
        Self { status, kind }
    }
}
impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        if e.raw_os_error() == Some(28) {
            Self::new(507, "no_space")
        } else {
            Self::new(500, "upload_io")
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UploadState {
    Uploaded,
    Importing,
    Imported,
    Duplicate,
    Invalid,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(rename = "uploadId")]
    pub id: String,
    pub name: String,
    pub size: u64,
    pub dev: u64,
    pub ino: u64,
    pub time: u64,
    pub state: UploadState,
    pub job_id: Option<String>,
    pub result: Option<Value>,
    /// Set once an uploaded archive has been unpacked into separate uploads. Such an
    /// entry has no catalog row of its own, so recovery must not mistake it for an
    /// interrupted import.
    #[serde(default)]
    pub archive: bool,
}
#[derive(Default, Clone, Serialize, Deserialize)]
struct Journal {
    entries: BTreeMap<String, Entry>,
    jobs: BTreeMap<String, Value>,
    #[serde(default)]
    job_order: Vec<String>,
}
impl Journal {
    fn prune(&mut self) {
        if self.job_order.is_empty() {
            self.job_order.extend(self.jobs.keys().cloned());
        }
        let terminal = |v: &Value| {
            matches!(
                v.get("state").and_then(Value::as_str),
                Some("finished" | "completed" | "failed" | "cancelled")
            )
        };
        let mut remaining = 100;
        self.job_order.retain(|id| self.jobs.contains_key(id));
        for id in self.job_order.iter().rev() {
            if self.jobs.get(id).is_some_and(terminal) {
                if remaining == 0 {
                    self.jobs.remove(id);
                } else {
                    remaining -= 1;
                }
            }
        }
        self.job_order.retain(|id| self.jobs.contains_key(id));
        let mut imported: Vec<_> = self
            .entries
            .values()
            // Archive entries are the only record of a file that still occupies disk
            // space and can be discarded; dropping them would orphan the file.
            .filter(|e| e.state == UploadState::Imported && !e.archive)
            .map(|e| (e.time, e.id.clone()))
            .collect();
        imported.sort();
        let excess = imported.len().saturating_sub(1000);
        for (_, id) in imported.into_iter().take(excess) {
            self.entries.remove(&id);
        }
    }
}
#[derive(Default)]
struct Inner {
    journal: Journal,
    active: usize,
    reserved: u64,
    active_bytes: u64,
    /// Models an archive is being unpacked into: they are not uploads yet, but
    /// already count against the pending quota and the free-space reserve.
    extracting_bytes: u64,
    extracting_models: u64,
}
pub struct UploadStore {
    dir: Directory,
    db_dir: Directory,
    journal_dir: Directory,
    journal_name: String,
    path: PathBuf,
    max_bytes: u64,
    max_active: usize,
    inner: Mutex<Inner>,
    catalogued: HashSet<PathBuf>,
    restart_required: AtomicBool,
    #[cfg(test)]
    pub sync_hook: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    #[cfg(test)]
    pub persist_count: std::sync::atomic::AtomicUsize,
    /// Simulates the process dying after the models were linked and before the journal write.
    #[cfg(test)]
    pub crash_after_link: AtomicBool,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn random_id() -> Result<String, StoreError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| StoreError::new(500, "random_failed"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn sanitize_name(encoded: &str) -> Result<String, StoreError> {
    let bad = || StoreError::new(400, "invalid_name");
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(bad());
            }
            let h = (bytes[i + 1] as char).to_digit(16).ok_or_else(bad)?;
            let l = (bytes[i + 2] as char).to_digit(16).ok_or_else(bad)?;
            decoded.push((h * 16 + l) as u8);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| bad())?;
    let name = clean_name(&decoded)?;
    let ext = extension(&name);
    if is_archive_name(&name) || is_model_extension(&ext) {
        return Ok(name);
    }
    // rar, tar and friends are recognised so the user learns the format is the
    // problem rather than a generic "wrong type".
    if katalog_core::archive::detect_format(std::path::Path::new(&name)).is_some() {
        return Err(StoreError::new(415, "archive_format_unsupported"));
    }
    Err(StoreError::new(415, "unsupported_type"))
}
/// Name rules shared by uploads and files taken out of an archive: everything
/// except the decision which extensions are acceptable.
pub(crate) fn clean_name(decoded: &str) -> Result<String, StoreError> {
    let bad = || StoreError::new(400, "invalid_name");
    let name=upload_fs::nfc(decoded).chars().filter(|c| !c.is_control() && !matches!(*c, '\u{ad}'|'\u{600}'..='\u{605}'|'\u{61c}'|'\u{6dd}'|'\u{70f}'|'\u{890}'..='\u{891}'|'\u{8e2}'|'\u{180e}'|'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}'|'\u{fff9}'..='\u{fffb}'|'\u{110bd}'|'\u{110cd}'|'\u{13430}'..='\u{13455}'|'\u{1bca0}'..='\u{1bca3}'|'\u{1d173}'..='\u{1d17a}'|'\u{e0001}'|'\u{e0020}'..='\u{e007f}')).collect::<String>();
    let name = name.trim_end_matches([' ', '.']).to_owned();
    if name.is_empty()
        || name.len() > 255
        || name.starts_with('.')
        || name.starts_with(' ')
        || name.chars().any(|c| "/\\<>:\"|?*".contains(c))
    {
        return Err(bad());
    }
    let base = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(base.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(bad());
    }
    Ok(name)
}
pub(crate) fn is_model_extension(ext: &str) -> bool {
    matches!(ext, "3mf" | "stl" | "obj" | "stp" | "step")
}
/// Archives the web upload unpacks itself; every other archive format is refused.
pub(crate) fn is_archive_name(name: &str) -> bool {
    matches!(extension(name).as_str(), "zip" | "7z")
}
pub(crate) fn extension(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}
/// Same bound as the filesystem-operation journal.
const MAX_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;

impl UploadStore {
    pub fn load(
        path: PathBuf,
        journal: PathBuf,
        db_dir: PathBuf,
        max_bytes: u64,
        max_active: usize,
        catalogued: &HashMap<PathBuf, Option<String>>,
    ) -> Result<Arc<Self>, StoreError> {
        let dir = Directory::open(&path)?;
        let db_dir = Directory::open(&db_dir)?;
        let journal_dir = Directory::open(
            journal
                .parent()
                .ok_or(StoreError::new(500, "journal_path"))?,
        )?;
        let journal_name = journal
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(StoreError::new(500, "journal_path"))?
            .to_owned();
        let journal = match journal_dir.read_private(&journal_name, MAX_JOURNAL_BYTES) {
            Ok(f) => serde_json::from_reader::<_, Journal>(f)
                .map_err(|_| StoreError::new(500, "journal_invalid"))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Journal::default(),
            Err(e) => return Err(e.into()),
        };
        let store = Arc::new(Self {
            dir,
            db_dir,
            journal_dir,
            journal_name,
            path,
            max_bytes,
            max_active,
            catalogued: catalogued.keys().cloned().collect(),
            restart_required: AtomicBool::new(false),
            #[cfg(test)]
            sync_hook: Mutex::new(None),
            #[cfg(test)]
            persist_count: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            crash_after_link: AtomicBool::new(false),
            inner: Mutex::new(Inner {
                journal,
                ..Inner::default()
            }),
        });
        Ok(store)
    }
    #[cfg(test)]
    pub fn open(
        path: PathBuf,
        journal: PathBuf,
        db_dir: PathBuf,
        max_bytes: u64,
        max_active: usize,
        catalogued: &HashMap<PathBuf, Option<String>>,
    ) -> Result<Arc<Self>, StoreError> {
        let store = Self::load(path, journal, db_dir, max_bytes, max_active, catalogued)?;
        store.reconcile(catalogued)?;
        Ok(store)
    }
    #[cfg(test)]
    pub fn reconcile(
        &self,
        catalogued: &HashMap<PathBuf, Option<String>>,
    ) -> Result<(), StoreError> {
        self.reconcile_known(catalogued, &HashSet::new())
    }
    fn reconcile_known(
        &self,
        catalogued: &HashMap<PathBuf, Option<String>>,
        known: &HashSet<String>,
    ) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let mut journal = inner.journal.clone();
        let (dir, journal_dir, path) = (&self.dir, &self.journal_dir, &self.path);
        // First: files hard-linked out of a staging folder have two names until it is
        // gone, and the check below insists on a single one.
        self.remove_stale_staging(&journal)?;
        journal.entries.retain(|_, e| {
            if dir.checked(&e.name, e.dev, e.ino).is_err()
            {
                return false;
            }
            if e.archive && e.state == UploadState::Imported {
                // Unpacked into separate uploads; the archive file itself is never catalogued.
            } else if known.contains(&e.id)
                || catalogued
                    .get(&path.join(&e.name))
                    .and_then(|hash| hash.as_ref())
                    .is_some_and(|hash| {
                        dir.checked(&e.name, e.dev, e.ino)
                            .ok()
                            .and_then(|file| {
                                katalog_core::import::files::compute_reader_hash(file).ok()
                            })
                            .as_ref()
                            == Some(hash)
                    })
            {
                e.state = UploadState::Imported;
            } else if matches!(e.state, UploadState::Importing | UploadState::Imported) {
                e.state = UploadState::Uploaded;
                e.result = None;
            }
            true
        });
        // Jobs are durable independently of the in-process worker. Reconstruct interrupted
        // work after reconciling committed paths so partial commits remain visible.
        for (job_id, job) in &mut journal.jobs {
            if matches!(
                job.get("state").and_then(Value::as_str),
                Some("finished" | "completed" | "failed" | "cancelled")
            ) {
                continue;
            }
            let files: Vec<Value> = journal.entries.values()
                .filter(|entry| entry.job_id.as_deref() == Some(job_id.as_str()))
                .map(|entry| serde_json::json!({
                    "uploadId": entry.id, "name": entry.name, "state": entry.state,
                    "fileId": entry.result.as_ref().and_then(|r| r.get("fileId")),
                    "existingFileId": entry.result.as_ref().and_then(|r| r.get("existingFileId")),
                    "existingFileName": entry.result.as_ref().and_then(|r| r.get("existingFileName")),
                    "kind": entry.result.as_ref().and_then(|r| r.get("kind")),
                })).collect();
            let done = files
                .iter()
                .filter(|file| file["state"] != "uploaded")
                .count();
            let total = job
                .get("total")
                .and_then(Value::as_u64)
                .unwrap_or(files.len() as u64);
            *job = serde_json::json!({"jobId":job_id,"state":"failed","current":null,
                "total":total,"done":done,"files":files,"kind":"interrupted"});
        }
        let mut orphans = 0;
        for name in dir.names()? {
            let untracked = dir.read(&name).is_ok()
                && !journal.entries.values().any(|e| e.name == name)
                && !catalogued.contains_key(&path.join(&name));
            if untracked {
                orphans += 1;
            }
            if let Some(id) = name
                .strip_prefix(".upload-")
                .and_then(|s| s.strip_suffix(".part"))
            {
                if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
                    if let Ok(f) = dir.read(&name) {
                        let m = f.metadata()?;
                        if m.nlink() == 1 && now().saturating_sub(m.mtime().max(0) as u64) > 3600 {
                            dir.unlink(&name)?;
                            if untracked {
                                orphans -= 1;
                            }
                        }
                    }
                }
            }
        }
        if orphans > 0 {
            eprintln!("upload recovery: {orphans} untracked files require manual review");
        }
        for name in journal_dir.names()? {
            if name
                .strip_prefix(".journal-")
                .and_then(|s| s.strip_suffix(".part"))
                .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                if let Ok(file) = journal_dir.read(&name) {
                    let m = file.metadata()?;
                    if m.nlink() == 1 && now().saturating_sub(m.mtime().max(0) as u64) > 3600 {
                        journal_dir.unlink(&name)?;
                    }
                }
            }
        }
        journal.prune();
        self.persist(&journal)?;
        inner.journal = journal;
        Ok(())
    }
    /// Existing IDs are authoritative: a relocated/replaced model is detached, never
    /// rebound to a different row at the old path. Legacy rows are repaired only by
    /// the existing inode + content-hash reconciliation below.
    pub fn reconcile_catalog(
        &self,
        ids: &HashMap<PathBuf, String>,
        hashes: &HashMap<PathBuf, Option<String>>,
    ) -> Result<(), StoreError> {
        let mut known = HashSet::new();
        {
            let mut inner = self.lock()?;
            let mut next = inner.journal.clone();
            next.entries.retain(|_, e| {
                let Some(id) = e
                    .result
                    .as_ref()
                    .and_then(|r| r.get("fileId"))
                    .and_then(Value::as_str)
                else {
                    return true;
                };
                if self.dir.checked(&e.name, e.dev, e.ino).is_err() { return false; }
                if ids.get(&self.path.join(&e.name)).is_some_and(|v| v == id) {
                    known.insert(e.id.clone());
                    true
                } else {
                    !ids.values().any(|v| v == id) && !ids.contains_key(&self.path.join(&e.name))
                }
            });
            self.persist(&next)?;
            inner.journal = next;
        }
        self.reconcile_known(hashes, &known)?;
        self.reconcile_file_ids(ids)
    }
    /// Attach missing legacy IDs in memory only; disk stays untouched until recovery
    /// has decided which entry must be durably detached/reset.
    pub(crate) fn bind_legacy(&self, upload_id: &str, file_id: i64) -> Result<(), StoreError> {
        if let Some(entry) = self.lock()?.journal.entries.get_mut(upload_id) {
            entry.result.get_or_insert_with(|| serde_json::json!({}))["fileId"] =
                Value::String(file_id.to_string());
        }
        Ok(())
    }
    pub fn detach(&self, ids: &[i64]) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let mut next = inner.journal.clone();
        next.entries.retain(|_, e| {
            !e.result
                .as_ref()
                .and_then(|r| r.get("fileId"))
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<i64>().ok())
                .is_some_and(|id| ids.contains(&id))
        });
        if next.entries.len() != inner.journal.entries.len() {
            self.persist(&next)?;
            inner.journal = next;
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn with_test_lock(&self, f: impl FnOnce()) {
        let _guard = self.inner.lock().unwrap();
        f();
    }
    fn lock(&self) -> Result<MutexGuard<'_, Inner>, StoreError> {
        self.inner
            .lock()
            .map_err(|_| StoreError::new(500, "journal_lock"))
    }
    fn persist(&self, journal: &Journal) -> Result<(), StoreError> {
        #[cfg(test)]
        self.persist_count.fetch_add(1, Ordering::Relaxed);
        let temp = format!(".journal-{}.part", random_id()?);
        let result = (|| {
            let mut f = self.journal_dir.create(&temp)?;
            serde_json::to_writer(&mut f, journal)
                .map_err(|_| StoreError::new(500, "journal_write"))?;
            f.flush()?;
            f.sync_all()?;
            self.journal_dir.rename(&temp, &self.journal_name)?;
            self.journal_dir.sync()?;
            Ok(())
        })();
        if result.is_err() {
            self.restart_required.store(true, Ordering::Release);
            let _ = self.journal_dir.unlink(&temp);
        }
        result
    }
    fn space(&self, inner: &Inner, extra: u64) -> Result<(), StoreError> {
        let needed = RESERVE.saturating_add(inner.reserved).saturating_add(extra);
        if self.dir.available()? <= needed
            || (self.dir.device()? == self.db_dir.device()? && self.db_dir.available()? <= RESERVE)
        {
            return Err(StoreError::new(507, "no_space"));
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn begin(
        self: &Arc<Self>,
        length: u64,
        header_name: &str,
    ) -> Result<PendingUpload, StoreError> {
        self.begin_limited(length, header_name, 512 * 1024 * 1024)
    }
    pub fn begin_limited(
        self: &Arc<Self>,
        length: u64,
        header_name: &str,
        mesh_limit: u64,
    ) -> Result<PendingUpload, StoreError> {
        let name = sanitize_name(header_name)?;
        let limit = if matches!(extension(&name).as_str(), "stl" | "obj") {
            self.max_bytes.min(mesh_limit.min(512 * 1024 * 1024))
        } else {
            self.max_bytes
        };
        if length == 0 {
            return Err(StoreError::new(400, "invalid_content"));
        }
        if length > limit {
            return Err(StoreError::new(413, "too_large"));
        }
        let mut inner = self.lock()?;
        if inner.active >= self.max_active {
            return Err(StoreError::new(503, "upload_busy"));
        }
        let pending: Vec<_> = inner
            .journal
            .entries
            .values()
            .filter(|e| e.state != UploadState::Imported)
            .collect();
        if pending.len() + inner.active + inner.extracting_models as usize >= 200
            || pending
                .iter()
                .map(|e| e.size)
                .sum::<u64>()
                .saturating_add(inner.active_bytes)
                .saturating_add(inner.extracting_bytes)
                .saturating_add(length)
                > PENDING_BYTES
        {
            return Err(StoreError::new(409, "import_or_discard_first"));
        }
        self.space(&inner, length)?;
        let id = random_id()?;
        let temporary = format!(".upload-{id}.part");
        let file = self.dir.create(&temporary)?;
        inner.active += 1;
        inner.reserved += length;
        inner.active_bytes += length;
        Ok(PendingUpload {
            store: Arc::clone(self),
            file,
            temporary,
            id,
            name,
            length,
            written: 0,
            next_check: CHECK_INTERVAL,
            head: Vec::new(),
            released: false,
        })
    }
    /// Removes unpacking leftovers of an earlier run. Runs only at startup, when no
    /// job exists in this process, so every staging folder is a leftover (a second
    /// server instance on the same upload folder is not supported: the journal is
    /// shared as well). Only real folders with the exact name pattern are touched;
    /// a symlink or file of that name is left alone.
    ///
    /// A crash between hard-linking a model out of staging and writing the journal
    /// leaves a second name for the staged file in the upload folder that nothing
    /// knows. Such names are recognised by sharing the inode with a staged file and
    /// removed first; the archive is still `Importing` in the journal and becomes
    /// importable again.
    fn remove_stale_staging(&self, journal: &Journal) -> Result<(), StoreError> {
        let names = self.dir.names()?;
        let mut staging = Vec::new();
        for name in &names {
            let Some(id) = name.strip_prefix(STAGING_PREFIX) else {
                continue;
            };
            if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let path = self.path.join(name);
            if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
                staging.push(path);
            }
        }
        if staging.is_empty() {
            return Ok(());
        }
        let mut staged: HashSet<(u64, u64)> = HashSet::new();
        let mut stack = staging.clone();
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else { continue };
            for item in read.flatten() {
                let Ok(meta) = std::fs::symlink_metadata(item.path()) else { continue };
                if meta.is_dir() {
                    stack.push(item.path());
                } else if meta.is_file() {
                    staged.insert((meta.dev(), meta.ino()));
                }
            }
        }
        let journalled: HashSet<&str> = journal.entries.values().map(|e| e.name.as_str()).collect();
        for name in &names {
            if name.starts_with(STAGING_PREFIX) || journalled.contains(name.as_str()) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(self.path.join(name)) else { continue };
            if meta.is_file() && meta.nlink() == 2 && staged.contains(&(meta.dev(), meta.ino())) {
                self.dir.unlink(name)?;
            }
        }
        for path in staging {
            if std::fs::remove_dir_all(&path).is_err() {
                eprintln!("upload recovery: an unpacking leftover could not be removed");
            }
        }
        Ok(())
    }
    /// Reserves room for the models of an archive BEFORE they are written: they
    /// count against the pending-upload quota and the free-space reserve from this
    /// moment on, so concurrent uploads cannot use the same space. `parent_id` is the
    /// archive itself; it leaves the pending set once unpacked, so it is not counted.
    pub fn begin_extraction(
        self: &Arc<Self>,
        parent_id: &str,
        model_bytes: u64,
        model_count: u64,
    ) -> Result<ExtractionReservation, StoreError> {
        let mut inner = self.lock()?;
        let pending: Vec<_> = inner
            .journal
            .entries
            .values()
            .filter(|e| e.state != UploadState::Imported && e.id != parent_id)
            .collect();
        let pending_bytes = pending.iter().map(|e| e.size).sum::<u64>();
        if pending.len() as u64 + inner.active as u64 + inner.extracting_models + model_count > MAX_PENDING_UPLOADS
            || pending_bytes
                .saturating_add(inner.active_bytes)
                .saturating_add(inner.extracting_bytes)
                .saturating_add(model_bytes)
                > PENDING_BYTES
        {
            return Err(StoreError::new(409, "import_or_discard_first"));
        }
        self.space(&inner, model_bytes)?;
        inner.extracting_bytes += model_bytes;
        inner.extracting_models += model_count;
        inner.reserved += model_bytes;
        Ok(ExtractionReservation {
            store: Arc::clone(self),
            bytes: model_bytes,
            models: model_count,
        })
    }
    /// Turns the models unpacked into a staging folder into ordinary uploads and
    /// marks the archive as handled, in ONE journal write: a crash leaves either the
    /// untouched archive (retry possible) or the finished state, never a mixture.
    /// The files are hard-linked out of staging, so no data is copied and every
    /// name the journal knows already exists when it is written. The staging names
    /// disappear when the caller removes the staging folder afterwards.
    pub fn adopt_extracted(
        &self,
        parent_id: &str,
        job_id: &str,
        staged: &[StagedModel],
        occupied: &HashSet<PathBuf>,
        summary: Value,
    ) -> Result<Vec<Entry>, StoreError> {
        let mut inner = self.lock()?;
        let parent = inner
            .journal
            .entries
            .get(parent_id)
            .ok_or(StoreError::new(404, "upload_unknown"))?;
        if parent.state != UploadState::Importing || parent.job_id.as_deref() != Some(job_id) {
            return Err(StoreError::new(409, "upload_state"));
        }
        let mut linked: Vec<String> = Vec::new();
        let mut entries: Vec<Entry> = Vec::new();
        let result = (|| {
            let mut taken: HashSet<String> = HashSet::new();
            for model in staged {
                let source_dir = Directory::open(&model.dir)?;
                let source = source_dir.read(&model.file)?;
                let meta = source.metadata()?;
                if meta.nlink() != 1 {
                    return Err(StoreError::new(409, "upload_changed"));
                }
                source.set_permissions(std::fs::Permissions::from_mode(0o644))?;
                let (stem, ext) = model
                    .name
                    .rsplit_once('.')
                    .ok_or(StoreError::new(400, "invalid_name"))?;
                let mut final_name = None;
                for n in 1..=1000 {
                    let name = if n == 1 {
                        model.name.clone()
                    } else {
                        let suffix = format!(" ({n}).{ext}");
                        let mut cut = stem.len().min(255 - suffix.len());
                        while !stem.is_char_boundary(cut) {
                            cut -= 1;
                        }
                        format!("{}{suffix}", &stem[..cut])
                    };
                    // Names that differ only in case would collide on a case-insensitive catalog volume.
                    if taken.contains(&name.to_lowercase())
                        || self.catalogued.contains(&self.path.join(&name))
                        || occupied.contains(&self.path.join(&name))
                    {
                        continue;
                    }
                    match self.dir.link_from(&source_dir, &model.file, &name) {
                        Ok(()) => {
                            final_name = Some(name);
                            break;
                        }
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
                let name = final_name.ok_or(StoreError::new(409, "name_conflict"))?;
                taken.insert(name.to_lowercase());
                linked.push(name.clone());
                // Two names now (staging and upload folder). The staging name is only
                // removed AFTER the journal knows the upload name: a crash in between
                // must leave the staged file recognisable (see `remove_stale_staging`).
                let linked_meta = self.dir.read(&name)?.metadata()?;
                if linked_meta.dev() != meta.dev() || linked_meta.ino() != meta.ino() || linked_meta.nlink() != 2 {
                    return Err(StoreError::new(409, "upload_changed"));
                }
                entries.push(Entry {
                    id: random_id()?,
                    name,
                    size: meta.len(),
                    dev: meta.dev(),
                    ino: meta.ino(),
                    time: now(),
                    state: UploadState::Importing,
                    job_id: Some(job_id.to_owned()),
                    result: None,
                    archive: false,
                });
            }
            self.dir.sync()?;
            #[cfg(test)]
            if self.crash_after_link.load(Ordering::SeqCst) {
                return Err(StoreError::new(500, "test_crash"));
            }
            let mut next = inner.journal.clone();
            for entry in &entries {
                next.entries.insert(entry.id.clone(), entry.clone());
            }
            let parent = next.entries.get_mut(parent_id).unwrap();
            parent.state = UploadState::Imported;
            parent.archive = true;
            parent.result = Some(summary);
            self.persist(&next)?;
            inner.journal = next;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(entries),
            Err(e) => {
                // A simulated crash does not get to clean up.
                #[cfg(test)]
                if self.crash_after_link.load(Ordering::SeqCst) {
                    return Err(e);
                }
                for name in &linked {
                    let _ = self.dir.unlink(name);
                }
                let _ = self.dir.sync();
                Err(e)
            }
        }
    }
    pub fn list(&self) -> Result<Vec<Entry>, StoreError> {
        Ok(self.lock()?.journal.entries.values().cloned().collect())
    }
    pub fn pending_list(&self) -> Result<Vec<Entry>, StoreError> {
        Ok(self
            .lock()?
            .journal
            .entries
            .values()
            // An unpacked archive stays listed: its file occupies disk space and is
            // the user's to discard.
            .filter(|entry| entry.state != UploadState::Imported || entry.archive)
            .take(500)
            .cloned()
            .collect())
    }
    pub fn path(&self, e: &Entry) -> PathBuf {
        self.path.join(&e.name)
    }
    pub fn checked_file(&self, e: &Entry) -> Result<File, StoreError> {
        self.dir
            .checked(&e.name, e.dev, e.ino)
            .map_err(|_| StoreError::new(409, "upload_changed"))
    }
    pub fn discard(&self, id: &str) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let e = inner
            .journal
            .entries
            .get(id)
            .ok_or(StoreError::new(404, "upload_unknown"))?;
        let handled_archive = e.archive && e.state == UploadState::Imported;
        if !handled_archive
            && !matches!(
                e.state,
                UploadState::Uploaded | UploadState::Duplicate | UploadState::Invalid
            )
        {
            return Err(StoreError::new(409, "upload_state"));
        }
        let metadata = self.dir.entry_identity(&e.name);
        let removable = handled_archive || matches!(e.state, UploadState::Duplicate | UploadState::Invalid);
        match metadata {
            Ok((dev, ino)) if dev == e.dev && ino == e.ino => {
                self.dir.unlink(&e.name)?;
                self.dir.sync()?;
            }
            Ok(_) if removable => {}
            Err(error) if removable && error.kind() == io::ErrorKind::NotFound => {}
            _ => return Err(StoreError::new(409, "upload_changed")),
        }
        let mut next = inner.journal.clone();
        next.entries.remove(id);
        next.prune();
        self.persist(&next)?;
        inner.journal = next;
        Ok(())
    }
    pub fn has_importing(&self) -> Result<bool, StoreError> {
        Ok(self
            .lock()?
            .journal
            .entries
            .values()
            .any(|e| e.state == UploadState::Importing))
    }
    /// Called after the database commit while the runtime import permit is held.
    pub fn reset_removed(&self, ids: &[i64]) -> Result<(), StoreError> {
        let ids: HashSet<String> = ids.iter().map(ToString::to_string).collect();
        let mut inner = self.lock()?;
        let mut next = inner.journal.clone();
        let mut changed = false;
        for entry in next.entries.values_mut() {
            if entry.state == UploadState::Imported
                && entry
                    .result
                    .as_ref()
                    .and_then(|r| r.get("fileId"))
                    .and_then(Value::as_str)
                    .is_some_and(|id| ids.contains(id))
            {
                entry.state = UploadState::Uploaded;
                entry.result = None;
                entry.job_id = None;
                changed = true;
            }
        }
        if changed {
            self.persist(&next)?;
            inner.journal = next;
        }
        Ok(())
    }
    pub fn ensure_importable(&self) -> Result<(), StoreError> {
        if self.restart_required.load(Ordering::Acquire) {
            return Err(StoreError::new(503, "restart_required"));
        }
        Ok(())
    }
    pub fn require_restart(&self) {
        self.restart_required.store(true, Ordering::Release);
    }
    pub fn begin_import(&self, ids: &[String], job_id: &str) -> Result<Vec<Entry>, StoreError> {
        self.ensure_importable()?;
        let mut inner = self.lock()?;
        if ids.is_empty() || ids.len() > 200 {
            return Err(StoreError::new(400, "invalid_upload_ids"));
        }
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(id) {
                return Err(StoreError::new(409, "upload_state"));
            }
            let e = inner
                .journal
                .entries
                .get(id)
                .ok_or(StoreError::new(404, "upload_unknown"))?;
            if e.state != UploadState::Uploaded {
                return Err(StoreError::new(409, "upload_state"));
            }
            if self.checked_file(e).is_err() {
                let mut next = inner.journal.clone();
                next.entries.get_mut(id).unwrap().state = UploadState::Invalid;
                next.prune();
                self.persist(&next)?;
                inner.journal = next;
                return Err(StoreError::new(409, "upload_changed"));
            }
        }
        if inner
            .journal
            .entries
            .values()
            .any(|e| e.state == UploadState::Importing)
        {
            return Err(StoreError::new(503, "import_busy"));
        }
        let mut next = inner.journal.clone();
        let mut entries = Vec::new();
        for id in ids {
            let e = next.entries.get_mut(id).unwrap();
            e.state = UploadState::Importing;
            e.job_id = Some(job_id.to_owned());
            entries.push(e.clone());
        }
        if next.job_order.is_empty() {
            next.job_order.extend(next.jobs.keys().cloned());
        }
        next.job_order.retain(|id| id != job_id);
        next.job_order.push(job_id.to_owned());
        next.jobs.insert(
            job_id.to_owned(),
            serde_json::json!({
                "jobId":job_id,"state":"importing","current":null,"total":entries.len(),
                "done":0,"files":[],"kind":null
            }),
        );
        next.prune();
        self.persist(&next)?;
        inner.journal = next;
        Ok(entries)
    }
    pub fn complete_batch(
        &self,
        updates: Vec<(String, UploadState, Value)>,
    ) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let mut next = inner.journal.clone();
        for (id, state, result) in updates {
            let e = next
                .entries
                .get_mut(&id)
                .ok_or(StoreError::new(404, "upload_unknown"))?;
            e.state = state;
            e.result = Some(result);
        }
        next.prune();
        self.persist(&next)?;
        inner.journal = next;
        Ok(())
    }
    /// Finish startup reconciliation using database IDs, without rewriting historical jobs.
    pub fn reconcile_file_ids(
        &self,
        catalogued: &HashMap<PathBuf, String>,
    ) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let mut next = inner.journal.clone();
        let mut repaired = HashMap::new();
        for entry in next.entries.values_mut() {
            if entry.state != UploadState::Imported {
                continue;
            }
            if let Some(file_id) = catalogued.get(&self.path.join(&entry.name)) {
                let result = serde_json::json!({
                    "uploadId":entry.id,"name":entry.name,"state":"imported",
                    "fileId":file_id,"existingFileId":null,"kind":null
                });
                repaired.insert(entry.id.clone(), (entry.job_id.clone(), result.clone()));
                entry.result = Some(result);
            }
        }
        for (job_id, job) in &mut next.jobs {
            if job.get("kind").and_then(Value::as_str) != Some("interrupted") {
                continue;
            }
            if let Some(files) = job.get_mut("files").and_then(Value::as_array_mut) {
                for file in files {
                    if let Some((entry_job_id, result)) = file
                        .get("uploadId")
                        .and_then(Value::as_str)
                        .and_then(|id| repaired.get(id))
                    {
                        if entry_job_id.as_deref() == Some(job_id.as_str()) {
                            *file = result.clone();
                        }
                    }
                }
            }
        }
        next.prune();
        self.persist(&next)?;
        inner.journal = next;
        Ok(())
    }
    pub fn save_job(&self, id: &str, result: Value) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let mut next = inner.journal.clone();
        if next.job_order.is_empty() {
            next.job_order.extend(next.jobs.keys().cloned());
        }
        next.job_order.retain(|existing| existing != id);
        next.job_order.push(id.to_owned());
        next.jobs.insert(id.to_owned(), result);
        next.prune();
        self.persist(&next)?;
        inner.journal = next;
        Ok(())
    }
    pub fn recent_job_results(&self) -> Result<Vec<Value>, StoreError> {
        let inner = self.lock()?;
        let journal = &inner.journal;
        let values: Vec<&Value> = if journal.job_order.is_empty() {
            journal.jobs.values().rev().take(32).collect()
        } else {
            journal
                .job_order
                .iter()
                .rev()
                .filter_map(|id| journal.jobs.get(id))
                .take(32)
                .collect()
        };
        Ok(values
            .into_iter()
            .map(|value| {
                if let Some(object) = value.as_object() {
                    Value::Object(
                        object
                            .iter()
                            .map(|(key, value)| {
                                let value = if key == "files" {
                                    value
                                        .as_array()
                                        .map(|files| {
                                            Value::Array(files.iter().take(200).cloned().collect())
                                        })
                                        .unwrap_or_else(|| value.clone())
                                } else {
                                    value.clone()
                                };
                                (key.clone(), value)
                            })
                            .collect(),
                    )
                } else {
                    value.clone()
                }
            })
            .collect())
    }
    #[cfg(test)]
    pub fn job_results(&self) -> Result<Vec<Value>, StoreError> {
        Ok(self.lock()?.journal.jobs.values().cloned().collect())
    }
    pub fn reset_importing(&self, job_id: &str) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let mut next = inner.journal.clone();
        for e in next.entries.values_mut() {
            if e.state == UploadState::Importing && e.job_id.as_deref() == Some(job_id) {
                e.state = UploadState::Uploaded;
            }
        }
        next.prune();
        self.persist(&next)?;
        inner.journal = next;
        Ok(())
    }
    /// Hands one archive back after a transient failure (no space, quota, I/O) so the
    /// user can start the import again; other entries of the job are left alone.
    pub fn reopen(&self, id: &str) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let mut next = inner.journal.clone();
        if let Some(e) = next.entries.get_mut(id) {
            if e.state == UploadState::Importing {
                e.state = UploadState::Uploaded;
                e.job_id = None;
            }
        }
        self.persist(&next)?;
        inner.journal = next;
        Ok(())
    }
    pub fn job_result(&self, id: &str) -> Result<Option<Value>, StoreError> {
        Ok(self.lock()?.journal.jobs.get(id).cloned())
    }
}
/// A model unpacked into a staging folder, waiting to become an upload.
pub struct StagedModel {
    /// Folder holding the file (inside the staging folder).
    pub dir: PathBuf,
    pub file: String,
    /// Wanted upload name (already validated).
    pub name: String,
}
/// Releases the quota and space reservation of `begin_extraction` when dropped.
pub struct ExtractionReservation {
    store: Arc<UploadStore>,
    bytes: u64,
    models: u64,
}
impl Drop for ExtractionReservation {
    fn drop(&mut self) {
        let mut inner = self
            .store
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.extracting_bytes = inner.extracting_bytes.saturating_sub(self.bytes);
        inner.extracting_models = inner.extracting_models.saturating_sub(self.models);
        inner.reserved = inner.reserved.saturating_sub(self.bytes);
    }
}
/// Content check on the first bytes, so a renamed executable or a text file with
/// an archive extension never reaches a parser or unpacker.
pub(crate) fn signature_ok(ext: &str, head: &[u8]) -> bool {
    match ext {
        "3mf" => head.starts_with(b"PK\x03\x04"),
        // An empty zip consists of the end record alone.
        "zip" => head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06"),
        "7z" => head.starts_with(b"7z\xBC\xAF\x27\x1C"),
        "stp" | "step" => head
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .is_some_and(|p| head[p..].starts_with(b"ISO-10303-21")),
        _ => !head.starts_with(b"MZ") && !head.starts_with(b"\x7fELF") && !head.starts_with(b"#!"),
    }
}
pub struct PendingUpload {
    store: Arc<UploadStore>,
    file: File,
    temporary: String,
    id: String,
    name: String,
    length: u64,
    written: u64,
    next_check: u64,
    head: Vec<u8>,
    released: bool,
}
impl PendingUpload {
    // The still-open temporary descriptor is the ownership authority. Never clean
    // up a name that now resolves to somebody else's inode (including Drop paths).
    fn unlink_owned(&self, name: &str) -> Result<(), StoreError> {
        let owned = self.file.metadata()?;
        let current = self.store.dir.read(name)?.metadata()?;
        if owned.dev() != current.dev() || owned.ino() != current.ino() {
            return Err(StoreError::new(409, "upload_changed"));
        }
        self.store.dir.unlink(name)?;
        Ok(())
    }
    pub fn write(&mut self, chunk: &[u8]) -> Result<(), StoreError> {
        if chunk.len() as u64 > self.length - self.written {
            return Err(StoreError::new(400, "length_mismatch"));
        }
        let take = chunk.len().min(65536usize.saturating_sub(self.head.len()));
        self.head.extend_from_slice(&chunk[..take]);
        let mut remaining = chunk;
        while !remaining.is_empty() {
            let count = remaining
                .len()
                .min((self.next_check - self.written) as usize);
            self.file.write_all(&remaining[..count])?;
            self.written += count as u64;
            self.store.lock()?.reserved -= count as u64;
            remaining = &remaining[count..];
            if self.written >= self.next_check {
                self.store.space(&*self.store.lock()?, 0)?;
                self.next_check = self.written.saturating_add(CHECK_INTERVAL);
            }
        }
        Ok(())
    }
    pub fn prepare(&self) -> Result<(), StoreError> {
        if self.written != self.length {
            return Err(StoreError::new(400, "length_mismatch"));
        }
        if !signature_ok(&extension(&self.name), &self.head) {
            return Err(StoreError::new(400, "invalid_signature"));
        }
        #[cfg(test)]
        if let Some(hook) = self.store.sync_hook.lock().unwrap().as_ref() {
            hook();
        }
        self.file
            .set_permissions(std::fs::Permissions::from_mode(0o644))?;
        self.file.sync_all()?;
        Ok(())
    }
    #[cfg(test)]
    pub fn finish(self) -> Result<Entry, StoreError> {
        self.prepare()?;
        self.register(&HashSet::new())
    }
    pub fn register(mut self, occupied: &HashSet<PathBuf>) -> Result<Entry, StoreError> {
        let mut inner = self.store.lock()?;
        self.store.space(&inner, 0)?;
        let owned = self.file.metadata()?;
        self.store
            .dir
            .checked(&self.temporary, owned.dev(), owned.ino())
            .map_err(|_| StoreError::new(409, "upload_changed"))?;
        let (stem, ext) = self
            .name
            .rsplit_once('.')
            .ok_or(StoreError::new(400, "invalid_name"))?;
        let mut final_name = None;
        for n in 1..=1000 {
            let name = if n == 1 {
                self.name.clone()
            } else {
                let suffix = format!(" ({n}).{ext}");
                let mut cut = stem.len().min(255 - suffix.len());
                while !stem.is_char_boundary(cut) {
                    cut -= 1;
                }
                format!("{}{suffix}", &stem[..cut])
            };
            if self.store.catalogued.contains(&self.store.path.join(&name))
                || occupied.contains(&self.store.path.join(&name))
            {
                continue;
            }
            match self.store.dir.link(&self.temporary, &name) {
                Ok(()) => {
                    final_name = Some(name);
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        let name = final_name.ok_or(StoreError::new(409, "name_conflict"))?;
        let result = (|| {
            let opened = self.file.metadata()?;
            let linked = self.store.dir.read(&name)?.metadata()?;
            if opened.dev() != linked.dev() || opened.ino() != linked.ino() || linked.nlink() != 2 {
                return Err(StoreError::new(409, "upload_changed"));
            }
            self.unlink_owned(&self.temporary)?;
            self.store.dir.sync()?;
            let m = self.file.metadata()?;
            let e = Entry {
                id: self.id.clone(),
                name: name.clone(),
                size: self.length,
                dev: m.dev(),
                ino: m.ino(),
                time: now(),
                state: UploadState::Uploaded,
                job_id: None,
                result: None,
                archive: false,
            };
            let mut next = inner.journal.clone();
            next.entries.insert(e.id.clone(), e.clone());
            self.store.persist(&next)?;
            inner.journal = next;
            Ok(e)
        })();
        if result.is_err() {
            let _ = self.unlink_owned(&name);
            let _ = self.store.dir.sync();
        }
        inner.active -= 1;
        inner.reserved -= self.length - self.written;
        inner.active_bytes -= self.length;
        self.released = true;
        result
    }
}
impl Drop for PendingUpload {
    fn drop(&mut self) {
        let _ = self.unlink_owned(&self.temporary);
        if !self.released {
            let mut inner = self
                .store
                .inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            inner.active = inner.active.saturating_sub(1);
            inner.active_bytes = inner.active_bytes.saturating_sub(self.length);
            inner.reserved = inner.reserved.saturating_sub(self.length - self.written);
        }
    }
}

#[cfg(test)]
#[path = "upload_store_tests.rs"]
mod tests;

/// Journal-only access when uploads are disabled. No upload directory is opened
/// or cleaned, but recovery's durable detach/reset steps must still be applied.
pub(crate) struct OfflineLedger {
    dir: Directory,
    journal: Journal,
}
impl OfflineLedger {
    pub(crate) fn load(parent: &std::path::Path) -> Result<Self, StoreError> {
        let dir = Directory::open(parent)?;
        let journal = match dir.read_private("web-uploads.json", MAX_JOURNAL_BYTES) {
            Ok(file) => serde_json::from_reader(file)
                .map_err(|_| StoreError::new(500, "journal_invalid"))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Journal::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { dir, journal })
    }
    pub(crate) fn entries(&self) -> Vec<Entry> {
        self.journal.entries.values().cloned().collect()
    }
    pub(crate) fn update(&mut self, ids: &[i64], reset: bool) -> Result<(), StoreError> {
        let mut next = self.journal.clone();
        next.entries.retain(|_, e| {
            let matched = e
                .result
                .as_ref()
                .and_then(|r| r.get("fileId"))
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<i64>().ok())
                .is_some_and(|id| ids.contains(&id));
            if matched && reset {
                e.state = UploadState::Uploaded;
                e.result = None;
            }
            !matched || reset
        });
        if serde_json::to_value(&next).ok() == serde_json::to_value(&self.journal).ok() {
            return Ok(());
        }
        let temp = format!(".journal-{}.part", random_id()?);
        let result: Result<(), StoreError> = (|| {
            let mut f = self.dir.create(&temp)?;
            serde_json::to_writer(&mut f, &next)
                .map_err(|_| StoreError::new(500, "journal_write"))?;
            f.flush()?;
            f.sync_all()?;
            self.dir.rename(&temp, "web-uploads.json")?;
            self.dir.sync()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self.dir.unlink(&temp);
        }
        result?;
        self.journal = next;
        Ok(())
    }
}
