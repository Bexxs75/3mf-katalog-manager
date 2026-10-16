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
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

const RESERVE: u64 = 256 * 1024 * 1024;
const PENDING_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const CHECK_INTERVAL: u64 = 64 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreError {
    pub status: u16,
    pub kind: &'static str,
}
impl StoreError {
    fn new(status: u16, kind: &'static str) -> Self {
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
}
#[derive(Default, Clone, Serialize, Deserialize)]
struct Journal {
    entries: BTreeMap<String, Entry>,
    jobs: BTreeMap<String, Value>,
    #[serde(default)]
    job_order: Vec<String>,
}
#[derive(Default)]
struct Inner {
    journal: Journal,
    active: usize,
    reserved: u64,
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
    let name=upload_fs::nfc(&decoded).chars().filter(|c| !c.is_control() && !matches!(*c, '\u{ad}'|'\u{600}'..='\u{605}'|'\u{61c}'|'\u{6dd}'|'\u{70f}'|'\u{890}'..='\u{891}'|'\u{8e2}'|'\u{180e}'|'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}'|'\u{fff9}'..='\u{fffb}'|'\u{110bd}'|'\u{110cd}'|'\u{13430}'..='\u{13455}'|'\u{1bca0}'..='\u{1bca3}'|'\u{1d173}'..='\u{1d17a}'|'\u{e0001}'|'\u{e0020}'..='\u{e007f}')).collect::<String>();
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
    let ext = extension(&name);
    if matches!(ext.as_str(), "zip" | "7z" | "rar") {
        return Err(StoreError::new(415, "archive_desktop_only"));
    }
    if !matches!(ext.as_str(), "3mf" | "stl" | "obj" | "stp" | "step") {
        return Err(StoreError::new(415, "unsupported_type"));
    }
    Ok(name)
}
fn extension(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}
impl UploadStore {
    pub fn open(
        path: PathBuf,
        journal: PathBuf,
        db_dir: PathBuf,
        max_bytes: u64,
        max_active: usize,
        catalogued: &HashSet<PathBuf>,
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
        let mut journal = match journal_dir.read(&journal_name) {
            Ok(f) => serde_json::from_reader::<_, Journal>(f)
                .map_err(|_| StoreError::new(500, "journal_invalid"))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Journal::default(),
            Err(e) => return Err(e.into()),
        };
        journal.entries.retain(|_, e| {
            if !dir
                .read(&e.name)
                .and_then(|f| f.metadata())
                .is_ok_and(|m| m.dev() == e.dev && m.ino() == e.ino)
            {
                return false;
            }
            if catalogued.contains(&path.join(&e.name)) {
                e.state = UploadState::Imported;
            } else if e.state == UploadState::Importing {
                e.state = UploadState::Uploaded;
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
        for name in dir.names()? {
            if let Some(id) = name
                .strip_prefix(".upload-")
                .and_then(|s| s.strip_suffix(".part"))
            {
                if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
                    if let Ok(f) = dir.read(&name) {
                        let m = f.metadata()?;
                        if m.nlink() == 1 && now().saturating_sub(m.mtime().max(0) as u64) > 3600 {
                            dir.unlink(&name)?;
                        }
                    }
                }
            }
        }
        let store = Arc::new(Self {
            dir,
            db_dir,
            journal_dir,
            journal_name,
            path,
            max_bytes,
            max_active,
            inner: Mutex::new(Inner {
                journal,
                ..Inner::default()
            }),
        });
        store.persist(&store.lock()?.journal)?;
        Ok(store)
    }
    fn lock(&self) -> Result<MutexGuard<'_, Inner>, StoreError> {
        self.inner
            .lock()
            .map_err(|_| StoreError::new(500, "journal_lock"))
    }
    fn persist(&self, journal: &Journal) -> Result<(), StoreError> {
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
            let _ = self.journal_dir.unlink(&temp);
        }
        result
    }
    fn space(&self, inner: &Inner, extra: u64) -> Result<(), StoreError> {
        let pending = inner
            .journal
            .entries
            .values()
            .filter(|e| e.state != UploadState::Imported)
            .map(|e| e.size)
            .sum::<u64>();
        let needed = RESERVE
            .saturating_add(inner.reserved)
            .saturating_add(pending)
            .saturating_add(extra);
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
        if pending.len() + inner.active >= 200
            || pending
                .iter()
                .map(|e| e.size)
                .sum::<u64>()
                .saturating_add(inner.reserved)
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
    pub fn list(&self) -> Result<Vec<Entry>, StoreError> {
        Ok(self.lock()?.journal.entries.values().cloned().collect())
    }
    pub fn pending_list(&self) -> Result<Vec<Entry>, StoreError> {
        Ok(self
            .lock()?
            .journal
            .entries
            .values()
            .filter(|entry| entry.state != UploadState::Imported)
            .take(200)
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
        if !matches!(
            e.state,
            UploadState::Uploaded | UploadState::Duplicate | UploadState::Invalid
        ) {
            return Err(StoreError::new(409, "upload_state"));
        }
        let metadata = self
            .dir
            .read(&e.name)
            .and_then(|file| file.metadata())
            .map_err(|_| StoreError::new(409, "upload_changed"))?;
        if metadata.dev() != e.dev || metadata.ino() != e.ino {
            return Err(StoreError::new(409, "upload_changed"));
        }
        self.dir.unlink(&e.name)?;
        self.dir.sync()?;
        let mut next = inner.journal.clone();
        next.entries.remove(id);
        self.persist(&next)?;
        inner.journal = next;
        Ok(())
    }
    pub fn begin_import(&self, ids: &[String], job_id: &str) -> Result<Vec<Entry>, StoreError> {
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
        self.persist(&next)?;
        inner.journal = next;
        Ok(())
    }
    pub fn job_result(&self, id: &str) -> Result<Option<Value>, StoreError> {
        Ok(self.lock()?.journal.jobs.get(id).cloned())
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
            remaining = &remaining[count..];
            if self.written >= self.next_check {
                self.store.space(&*self.store.lock()?, 0)?;
                self.next_check = self.written.saturating_add(CHECK_INTERVAL);
            }
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<Entry, StoreError> {
        if self.written != self.length {
            return Err(StoreError::new(400, "length_mismatch"));
        }
        let valid = match extension(&self.name).as_str() {
            "3mf" => self.head.starts_with(b"PK\x03\x04"),
            "stp" | "step" => self
                .head
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .is_some_and(|p| self.head[p..].starts_with(b"ISO-10303-21")),
            _ => {
                !self.head.starts_with(b"MZ")
                    && !self.head.starts_with(b"\x7fELF")
                    && !self.head.starts_with(b"#!")
            }
        };
        if !valid {
            return Err(StoreError::new(400, "invalid_signature"));
        }
        self.file
            .set_permissions(std::fs::Permissions::from_mode(0o644))?;
        self.file.sync_all()?;
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
        inner.reserved -= self.length;
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
            inner.reserved = inner.reserved.saturating_sub(self.length);
        }
    }
}

#[cfg(test)]
#[path = "upload_store_tests.rs"]
mod tests;
