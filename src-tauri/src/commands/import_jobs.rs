//! Serialized import jobs and their independent control path.
use super::*;
#[path = "import_jobs_commands.rs"]
mod api;
#[path = "import_jobs_worker.rs"]
mod worker;
pub use api::*;
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use worker::{heartbeat, run_archives, run_models};

#[derive(Debug, Default)]
pub struct CatalogGate {
    state: Mutex<(usize, bool)>,
    changed: Condvar,
}
#[derive(Debug)]
pub struct CatalogShare {
    gate: Arc<CatalogGate>,
    exclusive: bool,
}
impl CatalogGate {
    pub fn import(self: &Arc<Self>) -> CatalogShare {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while state.1 {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.0 += 1;
        CatalogShare {
            gate: self.clone(),
            exclusive: false,
        }
    }
    pub fn exclusive(self: &Arc<Self>) -> CmdResult<CatalogShare> {
        let mut state = self.state.lock().map_err(|_| "catalog gate poisoned")?;
        loop {
            if state.0 != 0 {
                return Err(CmdError::expected("importActive"));
            }
            if !state.1 {
                break;
            }
            state = self
                .changed
                .wait(state)
                .map_err(|_| "catalog gate poisoned")?;
        }
        state.1 = true;
        Ok(CatalogShare {
            gate: self.clone(),
            exclusive: true,
        })
    }
}
impl Drop for CatalogShare {
    fn drop(&mut self) {
        let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
        if self.exclusive {
            state.1 = false;
        } else {
            state.0 -= 1;
        }
        self.gate.changed.notify_all();
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImportSource {
    Files,
    Folder,
    Dropped,
    SetupAdopt,
    Archive,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum ImportState {
    Queued,
    Scanning,
    Importing,
    Placing,
    Cancelling,
    Cancelled,
    Finished,
    Failed,
}
impl ImportState {
    fn from_control(value: u8) -> Self {
        match value {
            0 => Self::Queued,
            1 => Self::Scanning,
            2 => Self::Importing,
            3 => Self::Placing,
            4 => Self::Cancelling,
            5 => Self::Cancelled,
            6 => Self::Finished,
            _ => Self::Failed,
        }
    }
    fn terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Finished | Self::Failed)
    }
}
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCounts {
    pub imported: usize,
    pub imported_not_placed: usize,
    pub duplicate: usize,
    pub skipped: usize,
    pub archive: usize,
    pub known: usize,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedEntry {
    pub path: String,
    pub file_id: String,
    pub folder_id: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotPlacedEntry {
    pub path: String,
    pub file_id: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateEntry {
    pub path: String,
    pub kind: String,
    pub existing_file_id: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct SkippedEntry {
    pub path: String,
    pub reason: String,
}
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportGroups {
    pub imported: Vec<ImportedEntry>,
    pub imported_not_placed: Vec<NotPlacedEntry>,
    pub duplicate: Vec<DuplicateEntry>,
    pub skipped: Vec<SkippedEntry>,
    pub archive: Vec<serde_json::Value>,
}
impl ImportGroups {
    fn counts(&self) -> ImportCounts {
        let mut c = ImportCounts {
            imported: self.imported.len(),
            imported_not_placed: self.imported_not_placed.len(),
            duplicate: self.duplicate.len(),
            skipped: self.skipped.len(),
            archive: self.archive.len(),
            known: 0,
        };
        c.known = c.imported + c.imported_not_placed + c.duplicate + c.skipped + c.archive;
        c
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ImportJobError {
    pub kind: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportJobResult {
    pub job_id: String,
    pub source: ImportSource,
    pub state: ImportState,
    pub parent_job_id: Option<String>,
    pub job_error: Option<ImportJobError>,
    pub scan_complete: bool,
    pub placement_required: bool,
    pub groups: ImportGroups,
    pub counts: ImportCounts,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub job_id: String,
    pub state: ImportState,
    pub scan_complete: bool,
    pub total: Option<usize>,
    pub done: usize,
    pub in_flight: usize,
    pub counts: ImportCounts,
    pub current: Option<String>,
    pub elapsed_ms: u64,
}
pub trait ImportClock: Send + Sync {
    fn now_ms(&self) -> u64;
}
struct RealClock(Instant);
impl ImportClock for RealClock {
    fn now_ms(&self) -> u64 {
        self.0.elapsed().as_millis() as u64
    }
}
pub trait ImportEvents: Send + Sync {
    fn emit(&self, name: &str, payload: serde_json::Value);
}
struct NoEvents;
impl ImportEvents for NoEvents {
    fn emit(&self, _: &str, _: serde_json::Value) {}
}
struct JobData {
    state: ImportState,
    scan_complete: bool,
    known: usize,
    candidates: Vec<String>,
    groups: ImportGroups,
    in_flight: usize,
    current: Option<String>,
    result: Option<ImportJobResult>,
    finished_at: Option<u64>,
    share: Option<CatalogShare>,
}
pub struct ImportJob {
    pub id: String,
    pub source: ImportSource,
    pub target_folder_id: Option<String>,
    pub placement_required: bool,
    pub parent_job_id: Option<String>,
    pub cancel: AtomicBool,
    control_state: AtomicU8,
    terminal_at: AtomicU64,
    input: JobInput,
    legacy: Mutex<Option<ImportResultDto>>,
    archive_legacy: Mutex<Option<ArchiveImportResultDto>>,
    started: u64,
    data: Mutex<JobData>,
    clock: Arc<dyn ImportClock>,
    events: Arc<dyn ImportEvents>,
}
impl ImportJob {
    fn result(&self) -> Option<ImportJobResult> {
        self.data.lock().unwrap().result.clone()
    }
    fn progress(&self) -> ImportProgress {
        let d = self.data.lock().unwrap();
        let counts = d.groups.counts();
        ImportProgress {
            job_id: self.id.clone(),
            state: ImportState::from_control(self.control_state.load(Ordering::Acquire)),
            scan_complete: d.scan_complete,
            total: d.scan_complete.then_some(d.known),
            done: counts.known,
            in_flight: d.in_flight,
            counts,
            current: d.current.clone(),
            elapsed_ms: self.clock.now_ms().saturating_sub(self.started),
        }
    }
    fn event(&self) {
        self.events.emit(
            "import://progress",
            serde_json::to_value(self.progress()).unwrap(),
        );
    }
    fn transition(&self, state: ImportState) {
        {
            let mut d = self.data.lock().unwrap();
            if d.state.terminal() {
                return;
            }
            d.state = if self.cancel.load(Ordering::Acquire) && !state.terminal() {
                ImportState::Cancelling
            } else {
                state
            };
            self.control_state.store(d.state as u8, Ordering::Release);
        }
        self.event();
    }
    fn finish(&self, mut error: Option<ImportJobError>) {
        let result = {
            let mut d = self.data.lock().unwrap();
            if d.state.terminal() {
                return;
            }
            // A parser panic or infrastructure failure still accounts for every
            // scanned candidate. Committed rows already have their own group.
            let mut resolved: HashMap<String, usize> = HashMap::new();
            for path in d
                .groups
                .imported
                .iter()
                .map(|e| e.path.clone())
                .chain(d.groups.imported_not_placed.iter().map(|e| e.path.clone()))
                .chain(d.groups.duplicate.iter().map(|e| e.path.clone()))
                .chain(d.groups.skipped.iter().map(|e| e.path.clone()))
                .chain(
                    d.groups
                        .archive
                        .iter()
                        .filter_map(|e| e["path"].as_str().map(String::from)),
                )
            {
                *resolved.entry(path).or_default() += 1;
            }
            for path in d.candidates.clone() {
                let remaining = resolved.entry(path.clone()).or_default();
                if *remaining > 0 {
                    *remaining -= 1;
                } else {
                    d.groups.skipped.push(SkippedEntry {
                        path,
                        reason: "notStarted".into(),
                    });
                    if error.is_none() && !self.cancel.load(Ordering::Acquire) {
                        error = Some(ImportJobError {
                            kind: "internal".into(),
                            message: "Import ended before processing all candidates".into(),
                        });
                    }
                }
            }
            d.state = if error.is_some() {
                ImportState::Failed
            } else if self.cancel.load(Ordering::Acquire) {
                ImportState::Cancelled
            } else {
                ImportState::Finished
            };
            let result = ImportJobResult {
                job_id: self.id.clone(),
                source: self.source,
                state: d.state,
                parent_job_id: self.parent_job_id.clone(),
                job_error: error,
                scan_complete: d.scan_complete,
                placement_required: self.placement_required,
                groups: d.groups.clone(),
                counts: d.groups.counts(),
            };
            d.in_flight = 0;
            d.current = None;
            d.finished_at = Some(self.clock.now_ms());
            self.terminal_at
                .store(self.clock.now_ms(), Ordering::Release);
            self.control_state.store(d.state as u8, Ordering::Release);
            d.result = Some(result.clone());
            d.share.take();
            result
        };
        self.event();
        self.events
            .emit("import://finished", serde_json::to_value(result).unwrap());
    }
}
#[derive(Default)]
struct Queue {
    waiting: VecDeque<Arc<ImportJob>>,
    jobs: HashMap<String, Arc<ImportJob>>,
    running: Option<Arc<ImportJob>>,
}
pub struct ImportJobs {
    reader: Arc<dyn ImportReader>,
    target: Arc<dyn TargetChecker>,
    scan_limit: usize,
    pub drops: PathGrants,
    pub pickers: PathGrants,
    archives: Mutex<HashMap<(String, PathBuf), ArchiveGrant>>,
    queue: Mutex<Queue>,
    clock: Arc<dyn ImportClock>,
    events: Arc<dyn ImportEvents>,
    pub gate: Arc<CatalogGate>,
}
impl Default for ImportJobs {
    fn default() -> Self {
        Self {
            reader: Arc::new(FileReader),
            target: Arc::new(DiskTarget),
            scan_limit: usize::MAX,
            drops: PathGrants::default(),
            pickers: PathGrants::default(),
            archives: Mutex::new(HashMap::new()),
            queue: Mutex::new(Queue::default()),
            clock: Arc::new(RealClock(Instant::now())),
            events: Arc::new(NoEvents),
            gate: Arc::new(CatalogGate::default()),
        }
    }
}
impl ImportJobs {
    #[cfg(test)]
    fn enqueue(
        &self,
        source: ImportSource,
        target: Option<String>,
        placement: bool,
        parent: Option<String>,
    ) -> CmdResult<Arc<ImportJob>> {
        self.enqueue_work(
            source,
            target,
            placement,
            parent,
            JobInput::Models(Vec::new()),
        )
    }
    #[cfg(test)]
    fn enqueue_work(
        &self,
        source: ImportSource,
        target: Option<String>,
        placement: bool,
        parent: Option<String>,
        input: JobInput,
    ) -> CmdResult<Arc<ImportJob>> {
        self.enqueue_checked(source, target, placement, parent, input, None)
    }
    fn enqueue_checked(
        &self,
        source: ImportSource,
        target: Option<String>,
        placement: bool,
        parent: Option<String>,
        input: JobInput,
        error: Option<ImportJobError>,
    ) -> CmdResult<Arc<ImportJob>> {
        let share = self.gate.import();
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let id = format!(
            "{:x}-{:x}-{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let job = Arc::new(ImportJob {
            id: id.clone(),
            source,
            target_folder_id: target,
            placement_required: placement,
            parent_job_id: parent,
            input,
            legacy: Mutex::new(None),
            archive_legacy: Mutex::new(None),
            cancel: AtomicBool::new(false),
            control_state: AtomicU8::new(ImportState::Queued as u8),
            terminal_at: AtomicU64::new(u64::MAX),
            started: self.clock.now_ms(),
            clock: self.clock.clone(),
            events: self.events.clone(),
            data: Mutex::new(JobData {
                state: ImportState::Queued,
                scan_complete: false,
                known: 0,
                candidates: Vec::new(),
                groups: ImportGroups::default(),
                in_flight: 0,
                current: None,
                result: None,
                finished_at: None,
                share: Some(share),
            }),
        });
        if let (JobInput::Models(paths), None) = (&job.input, &error) {
            let mut grants = self.archives.lock().unwrap();
            for (index, path) in paths
                .iter()
                .enumerate()
                .filter(|(_, p)| p.is_file() && crate::archive::is_archive_path(p))
            {
                grants.insert(
                    (job.id.clone(), path.clone()),
                    ArchiveGrant {
                        parent: job.id.clone(),
                        id: format!("{}-{index}", job.id),
                        expires: None,
                    },
                );
            }
        }
        let mut q = self.queue.lock().map_err(|_| "import queue poisoned")?;
        q.jobs.retain(|_, job| {
            let terminal_at = job.terminal_at.load(Ordering::Acquire);
            terminal_at == u64::MAX || self.clock.now_ms().saturating_sub(terminal_at) < 600_000
        });
        q.jobs.insert(id, job.clone());
        if error.is_none() {
            q.waiting.push_back(job.clone());
        }
        drop(q);
        if let Some(error) = error {
            job.finish(Some(error));
        } else {
            job.event();
        }
        Ok(job)
    }
    fn next(&self) -> Option<Arc<ImportJob>> {
        let mut q = self.queue.lock().unwrap();
        if q.running.as_ref().is_some_and(|j| {
            !ImportState::from_control(j.control_state.load(Ordering::Acquire)).terminal()
        }) {
            return None;
        }
        q.running = None;
        while let Some(job) = q.waiting.pop_front() {
            if ImportState::from_control(job.control_state.load(Ordering::Acquire)).terminal() {
                continue;
            }
            q.running = Some(job.clone());
            drop(q);
            job.transition(ImportState::Scanning);
            return Some(job);
        }
        None
    }
    fn get(&self, id: &str) -> Option<Arc<ImportJob>> {
        let job = self.queue.lock().unwrap().jobs.get(id)?.clone();
        let terminal_at = job.terminal_at.load(Ordering::Acquire);
        if terminal_at != u64::MAX && self.clock.now_ms().saturating_sub(terminal_at) >= 600_000 {
            return None;
        }
        Some(job)
    }
    fn cancel(&self, id: &str) -> Option<ImportState> {
        let job = self.get(id)?;
        loop {
            let state = ImportState::from_control(job.control_state.load(Ordering::Acquire));
            if state.terminal() || state == ImportState::Cancelling {
                return Some(state);
            }
            job.cancel.store(true, Ordering::Release);
            if job
                .control_state
                .compare_exchange(
                    state as u8,
                    ImportState::Cancelling as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
            {
                continue;
            }
            if state == ImportState::Queued {
                self.archives
                    .lock()
                    .unwrap()
                    .retain(|(parent, _), _| *parent != job.id);
                job.finish(None);
                return Some(ImportState::Cancelled);
            }
            // Event delivery may wait for a snapshot, the control response does not.
            std::thread::spawn(move || job.event());
            return Some(ImportState::Cancelling);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_cancellation_is_terminal_without_running_work() {
        let service = ImportJobs::default();
        let first = service
            .enqueue(ImportSource::Dropped, None, false, None)
            .unwrap();
        let second = service
            .enqueue(ImportSource::Dropped, None, false, None)
            .unwrap();
        assert_eq!(service.next().unwrap().id, first.id);
        assert_eq!(service.cancel(&second.id), Some(ImportState::Cancelled));
        assert!(service.next().is_none());
        assert_eq!(second.result().unwrap().counts.known, 0);
    }
    #[test]
    fn import_share_excludes_writers_until_last_queued_job_finishes() {
        let gate = std::sync::Arc::new(CatalogGate::default());
        let share = gate.import();
        assert_eq!(gate.exclusive().unwrap_err().message, "importActive");
        drop(share);
        assert!(gate.exclusive().is_ok());
    }
}

/// The parser is injectable so a slow read cannot hide a blocked control path.
pub trait ImportReader: Send + Sync {
    fn read(&self, path: &Path) -> CmdResult<NewFile>;
}
struct FileReader;
impl ImportReader for FileReader {
    fn read(&self, path: &Path) -> CmdResult<NewFile> {
        let mut file = files::read_model_file(path, None)?;
        file.content_hash = Some(files::compute_content_hash(path)?);
        Ok(file)
    }
}
pub trait TargetChecker: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
}
struct DiskTarget;
impl TargetChecker for DiskTarget {
    fn exists(&self, path: &Path) -> bool {
        path.is_dir()
    }
}
#[derive(Clone)]
enum JobInput {
    Models(Vec<PathBuf>),
    Archives {
        target: PathBuf,
        requests: Vec<ArchiveRequest>,
        delete: bool,
    },
}
struct AppEvents(tauri::AppHandle);
impl ImportEvents for AppEvents {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.0.emit(name, payload);
    }
}
impl ImportJobs {
    pub fn for_app(app: tauri::AppHandle) -> Self {
        Self {
            events: Arc::new(AppEvents(app)),
            ..Self::default()
        }
    }
}

#[derive(Default)]
pub struct PathGrants(Mutex<HashMap<PathBuf, u64>>);
impl PathGrants {
    pub fn register(&self, paths: &[PathBuf], now: u64, ttl: u64) {
        let mut grants = self.0.lock().unwrap();
        grants.retain(|_, expiry| *expiry > now);
        for path in paths {
            grants.insert(path.clone(), now.saturating_add(ttl));
        }
    }
    fn claim(&self, paths: &[PathBuf], now: u64) -> bool {
        let mut grants = self.0.lock().unwrap();
        let unique: HashSet<_> = paths.iter().collect();
        if !unique
            .iter()
            .all(|path| grants.get(*path).is_some_and(|expiry| *expiry > now))
        {
            return false;
        }
        for path in unique {
            grants.remove(path);
        }
        true
    }
}
struct ArchiveGrant {
    parent: String,
    id: String,
    expires: Option<u64>,
}

#[cfg(test)]
#[path = "import_job_tests.rs"]
mod import_job_tests;

impl ImportJobs {
    pub fn observe_drop(&self, paths: &[PathBuf]) {
        self.drops.register(paths, self.clock.now_ms(), 60_000);
    }
    pub fn observe_picker(&self, path: &Path) {
        self.pickers
            .register(&[path.to_path_buf()], self.clock.now_ms(), 600_000);
    }
}

impl ImportJobs {
    pub(super) fn authorize_models(
        &self,
        source: ImportSource,
        paths: &[PathBuf],
        allow_picker: bool,
        sensitive: &[PathBuf],
    ) -> CmdResult<ImportSource> {
        let source = match source {
            ImportSource::Dropped if self.drops.claim(paths, self.clock.now_ms()) => source,
            ImportSource::Dropped
                if allow_picker
                    && paths.len() == 1
                    && self.pickers.claim(paths, self.clock.now_ms()) =>
            {
                ImportSource::SetupAdopt
            }
            ImportSource::SetupAdopt if self.pickers.claim(paths, self.clock.now_ms()) => source,
            ImportSource::Files | ImportSource::Folder => source,
            _ => return Err(CmdError::expected("unauthorized")),
        };
        for path in paths {
            if path.is_dir() {
                reject_if_sensitive_path(path, sensitive)
                    .map_err(|_| CmdError::expected("unauthorized"))?;
            }
        }
        Ok(source)
    }
}

impl ImportJobs {
    fn finish(&self, job: &ImportJob, error: Option<ImportJobError>) {
        let pending: HashSet<String> = job
            .data
            .lock()
            .unwrap()
            .groups
            .archive
            .iter()
            .filter_map(|entry| entry["grantId"].as_str().map(String::from))
            .collect();
        self.archives.lock().unwrap().retain(|_, grant| {
            if grant.parent != job.id {
                return grant
                    .expires
                    .is_none_or(|expiry| expiry > self.clock.now_ms());
            }
            if !pending.contains(&grant.id) {
                return false;
            }
            grant.expires = Some(self.clock.now_ms().saturating_add(1_800_000));
            true
        });
        job.finish(error);
    }
    fn claim_archives(
        &self,
        requests: &[ArchiveRequest],
        parent: Option<&str>,
    ) -> Option<Option<String>> {
        let mut grants = self.archives.lock().unwrap();
        let now = self.clock.now_ms();
        let mut seen = HashSet::new();
        let mut selected = Vec::new();
        for request in requests {
            if !seen.insert(&request.path) {
                return None;
            }
            let key = grants
                .iter()
                .filter(|((job, path), grant)| {
                    path == Path::new(&request.path)
                        && parent.is_none_or(|p| p == job)
                        && grant.expires.is_some_and(|expiry| expiry > now)
                })
                .map(|(key, _)| key)
                .max()
                .cloned()?;
            selected.push(key);
        }
        let parents: HashSet<_> = selected.iter().map(|(parent, _)| parent.clone()).collect();
        if parents.len() > 1 {
            return None;
        }
        for key in selected {
            grants.remove(&key);
        }
        Some(if parents.len() == 1 {
            parents.into_iter().next()
        } else {
            None
        })
    }
}

#[cfg(test)]
#[path = "import_handoff_tests.rs"]
mod import_handoff_tests;

impl ImportJobs {
    pub(super) fn authorize_archives(
        &self,
        sensitive: &[PathBuf],
        target: &Path,
        target_approved: bool,
        requests: &[ArchiveRequest],
        parent: Option<&str>,
    ) -> CmdResult<Option<String>> {
        archives::check_target_location(target, &expand_sensitive_dirs(sensitive))?;
        if !target_approved {
            return Err(CmdError::expected(
                "Zielordner wurde nicht ueber die App ausgewaehlt",
            ));
        }
        self.claim_archives(requests, parent)
            .ok_or_else(|| CmdError::expected("unauthorized"))
    }
    #[cfg(test)]
    pub(super) fn test_handoff(&self, paths: Vec<PathBuf>) -> String {
        let job = self
            .enqueue_work(
                ImportSource::Dropped,
                None,
                false,
                None,
                JobInput::Models(paths.clone()),
            )
            .unwrap();
        self.next().unwrap();
        let mut conn = db::connect_in_memory().unwrap();
        run_models(self, &job, &mut &mut conn, &paths, &[]).unwrap();
        self.finish(&job, None);
        job.id.clone()
    }
}

impl ImportJobs {
    fn partition_archives(
        &self,
        requests: Vec<ArchiveRequest>,
    ) -> Vec<(Option<String>, Vec<ArchiveRequest>)> {
        let grants = self.archives.lock().unwrap();
        let mut groups: Vec<(Option<String>, Vec<ArchiveRequest>)> = Vec::new();
        for request in requests {
            let parent = grants
                .iter()
                .filter(|((_, path), grant)| {
                    path == Path::new(&request.path)
                        && grant
                            .expires
                            .is_some_and(|expiry| expiry > self.clock.now_ms())
                })
                .map(|((parent, _), _)| parent.clone())
                .max();
            if let Some((_, group)) = groups
                .last_mut()
                .filter(|(previous, _)| *previous == parent)
            {
                group.push(request);
            } else {
                groups.push((parent, vec![request]));
            }
        }
        groups
    }
}
