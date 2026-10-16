//! Shared destructive-operation budget and durable pre-mutation backups.
use katalog_core::write::Connection;
use std::{collections::VecDeque, path::Path};

#[derive(Clone, Copy)]
pub(crate) enum DestructiveKind {
    Trash,
    Permanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BudgetError {
    LimitExceeded,
    Retry(u64),
}
/// How a reservation is charged against the 10-minute and the daily window.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Charge {
    /// Same amount in both windows; above a window limit it can never fit.
    Strict,
    /// Folder removal: at most one full window per window, so it is never impossible.
    Folder,
    /// Trash-emptying blocks: the client is already throttled by the 60 s start and
    /// 2 s block intervals, so only the daily limit applies (a full 200-block would
    /// otherwise exhaust the 10-minute pot of single permanent deletions).
    DailyOnly,
}
#[derive(Default)]
struct Budget {
    // Each window has its own charge so the modes above can differ per window.
    entries: VecDeque<(u64, [usize; 2])>,
}
impl Budget {
    fn reserve(&mut self, count: usize, now: u64, daily_limit: usize, mode: Charge) -> Result<(), BudgetError> {
        let limits = [(600, 200), (86400, daily_limit)];
        let impossible = match mode {
            Charge::Strict => limits.iter().any(|(_, limit)| count > *limit),
            Charge::Folder => false,
            Charge::DailyOnly => count > daily_limit,
        };
        if impossible {
            return Err(BudgetError::LimitExceeded);
        }
        let charges = match mode {
            Charge::Strict => [count; 2],
            Charge::Folder => [count.min(200), count.min(daily_limit)],
            Charge::DailyOnly => [0, count],
        };
        let mut retry = 0;
        for (index, (window, limit)) in limits.into_iter().enumerate() {
            let count = charges[index];
            let used: usize = self.entries.iter()
                .filter(|(at, _)| now.saturating_sub(*at) < window)
                .map(|(_, n)| n[index]).sum();
            if count.saturating_add(used) > limit {
                let mut pending = used;
                for (at, n) in &self.entries {
                    if now.saturating_sub(*at) >= window { continue; }
                    pending = pending.saturating_sub(n[index]);
                    if pending.saturating_add(count) <= limit {
                        retry = retry.max(at.saturating_add(window).saturating_sub(now).max(1));
                        break;
                    }
                }
            }
        }
        if retry > 0 { return Err(BudgetError::Retry(retry)); }
        self.entries.retain(|(at, _)| now.saturating_sub(*at) < 86400);
        if count > 0 { self.entries.push_back((now, charges)); }
        Ok(())
    }
}

/// Caller holds the shared safety mutex only for admission/backups, never file I/O
/// of the operation itself. Reservations are conservative: failures do not refund.
pub(crate) struct FileOpsSafety {
    daily_limit: usize,
    trash: Budget,
    permanent: Budget,
    empty_start: Option<u64>,
    empty_block: Option<u64>,
}
impl FileOpsSafety {
    pub(crate) fn new(daily_limit: usize) -> Self {
        Self {
            daily_limit,
            trash: Budget::default(),
            permanent: Budget::default(),
            empty_start: None,
            empty_block: None,
        }
    }
    pub(crate) fn reserve(
        &mut self,
        kind: DestructiveKind,
        count: usize,
        now: u64,
    ) -> Result<(), BudgetError> {
        let budget = match kind {
            DestructiveKind::Trash => &mut self.trash,
            DestructiveKind::Permanent => &mut self.permanent,
        };
        budget.reserve(count, now, self.daily_limit, Charge::Strict)
    }
    pub(crate) fn reserve_folder(&mut self, count: usize, now: u64) -> Result<(), BudgetError> {
        self.permanent.reserve(count, now, self.daily_limit, Charge::Folder)
    }
    pub(crate) fn reserve_empty_block_items(&mut self, count: usize, now: u64) -> Result<(), BudgetError> {
        self.permanent.reserve(count, now, self.daily_limit, Charge::DailyOnly)
    }
    pub(crate) fn reserve_empty_start(&mut self, now: u64) -> Result<(), u64> {
        interval(&mut self.empty_start, now, 60)
    }
    pub(crate) fn reserve_empty_block(&mut self, now: u64) -> Result<(), u64> {
        interval(&mut self.empty_block, now, 2)
    }
    /// Calendar days use UTC, independent of the container timezone. Names and
    /// retention are separate from updater backups. Existing backups survive restart.
    pub(crate) fn ensure_backups(
        &self,
        conn: &Connection,
        db_path: &Path,
        count: usize,
        event: bool,
        now: u64,
    ) -> Result<(), BackupFailed> {
        if count == 0 {
            return Ok(());
        }
        ensure_backups(conn, db_path, event, now).map_err(|_| BackupFailed)
    }
}
fn interval(last: &mut Option<u64>, now: u64, seconds: u64) -> Result<(), u64> {
    if let Some(at) = last {
        let elapsed = now.saturating_sub(*at);
        if elapsed < seconds {
            return Err(seconds - elapsed);
        }
    }
    *last = Some(now);
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackupFailed;
const RESERVE: u64 = 64 * 1024 * 1024;
fn check_space(available: u64, size: u64) -> Result<(), BackupFailed> {
    if available < size.saturating_add(RESERVE) {
        Err(BackupFailed)
    } else {
        Ok(())
    }
}
fn ensure_backups(
    conn: &Connection,
    db_path: &Path,
    event: bool,
    now: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let db_path = db_path.canonicalize()?;
    let dir = db_path
        .parent()
        .ok_or("database parent missing")?
        .join("backups");
    if !dir.exists() {
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    }
    let metadata = std::fs::symlink_metadata(&dir)?;
    if !metadata.is_dir() || metadata.mode() & 0o022 != 0 {
        return Err("unsafe backup directory".into());
    }
    let handle = katalog_core::upload_fs::Directory::open(&dir)?;
    let mut days = Vec::new();
    let mut events = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        for (prefix, values) in [
            ("web-fileops-day-", &mut days),
            ("web-fileops-event-", &mut events),
        ] {
            if let Some(stamp) = name
                .strip_prefix(prefix)
                .and_then(|v| v.strip_suffix(".db"))
                .and_then(|v| v.parse::<u64>().ok())
            {
                let metadata = entry.metadata()?;
                if metadata.nlink() != 1
                    || metadata.mode() & 0o777 != 0o600
                    || metadata.uid() != std::fs::metadata(&dir)?.uid()
                {
                    return Err("unsafe backup file".into());
                }
                let mut header = [0u8; 16];
                use std::io::Read;
                std::fs::File::open(entry.path())?.read_exact(&mut header)?;
                if &header != b"SQLite format 3\0" {
                    return Err("invalid backup file".into());
                }
                values.push((stamp, entry.path()));
            }
        }
    }
    let today = now / 86400;
    let need_day = !days.iter().any(|(day, _)| *day == today);
    let need_event = event && !events.iter().any(|(at, _)| now.saturating_sub(*at) < 600);
    for (needed, prefix, stamp) in [(need_day, "day", today), (need_event, "event", now)] {
        if !needed {
            continue;
        }
        let pages: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        check_space(
            handle.available()?,
            (pages as u64).saturating_mul(page_size as u64),
        )
        .map_err(|_| "insufficient backup space")?;
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| "backup randomness unavailable")?;
        let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let staging = dir.join(format!(
            ".web-fileops-stage-{}-{suffix}",
            std::process::id()
        ));
        std::fs::DirBuilder::new().mode(0o700).create(&staging)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            // Core owns SQLite's online backup API. Its updater retention is
            // confined to this staging directory, never applied to fileops copies.
            let saved = katalog_core::backup::create_schema(conn, &staging, 0, 0)
                .map_err(std::io::Error::other)?;
            std::fs::set_permissions(&saved, std::fs::Permissions::from_mode(0o600))?;
            std::fs::File::open(&saved)?.sync_all()?;
            let dest = dir.join(format!("web-fileops-{prefix}-{stamp}.db"));
            // NOREPLACE publication: an existing copy can never be overwritten.
            let staged = katalog_core::upload_fs::Directory::open(&staging)?;
            staged.rename_noreplace(
                saved
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("backup name")?,
                &handle,
                dest.file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("backup name")?,
            )?;
            std::fs::File::open(&dir)?.sync_all()?;
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result?;
    }
    for (day, path) in days {
        if today.saturating_sub(day) >= 7 {
            std::fs::remove_file(path)?;
        }
    }
    events.sort_by_key(|(at, _)| *at);
    let excess = events
        .len()
        .saturating_add(usize::from(need_event))
        .saturating_sub(5);
    for (_, path) in events.into_iter().take(excess) {
        std::fs::remove_file(path)?;
    }
    handle.sync()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_above_window_limit_is_rejected_without_retry_and_leaves_budget_unchanged() {
        let mut safety = FileOpsSafety::new(1000);
        assert_eq!(safety.reserve(DestructiveKind::Trash, 201, 10), Err(BudgetError::LimitExceeded));
        assert_eq!(safety.reserve(DestructiveKind::Trash, 200, 10), Ok(()));
        let mut small = FileOpsSafety::new(50);
        assert_eq!(small.reserve(DestructiveKind::Permanent, 51, 10), Err(BudgetError::LimitExceeded));
        assert_eq!(small.reserve(DestructiveKind::Permanent, 50, 10), Ok(()));
    }
    #[test]
    fn folder_removal_is_capped_to_one_window_and_waits_with_real_retry_after() {
        let mut safety = FileOpsSafety::new(100_000);
        assert_eq!(safety.reserve_folder(5000, 100), Ok(()));
        // The 10-minute window is full: a real, finite wait, not a permanent refusal.
        assert_eq!(safety.reserve_folder(5000, 101), Err(BudgetError::Retry(599)));
        assert_eq!(safety.reserve_folder(5000, 700), Ok(()));
        let mut half = FileOpsSafety::new(100_000);
        assert_eq!(half.reserve(DestructiveKind::Permanent, 100, 100), Ok(()));
        assert_eq!(half.reserve_folder(500, 150), Err(BudgetError::Retry(550)));
    }
    #[test]
    fn empty_trash_blocks_count_only_against_daily_limit() {
        let mut safety = FileOpsSafety::new(1200);
        for block in 0..5 {
            assert_eq!(safety.reserve_empty_block_items(200, 100 + block * 2), Ok(()));
        }
        // The 10-minute pot of single permanent deletions is untouched.
        assert_eq!(safety.reserve(DestructiveKind::Permanent, 200, 112), Ok(()));
        // The daily limit still applies, with the real wait until the oldest entry expires.
        assert_eq!(safety.reserve_empty_block_items(1, 113), Err(BudgetError::Retry(86387)));
        assert_eq!(safety.reserve_empty_block_items(1, 100 + 86400), Ok(()));
        let mut small = FileOpsSafety::new(100);
        assert_eq!(small.reserve_empty_block_items(101, 1), Err(BudgetError::LimitExceeded));
    }
    #[test]
    fn budgets_are_atomic_separate_and_rolling() {
        let mut safety = FileOpsSafety::new(250);
        assert_eq!(safety.reserve(DestructiveKind::Trash, 200, 100), Ok(()));
        assert_eq!(safety.reserve(DestructiveKind::Trash, 1, 101), Err(BudgetError::Retry(599)));
        assert_eq!(safety.reserve(DestructiveKind::Permanent, 200, 101), Ok(()));
        assert_eq!(safety.reserve(DestructiveKind::Trash, 50, 700), Ok(()));
        assert_eq!(safety.reserve(DestructiveKind::Trash, 1, 1300), Err(BudgetError::Retry(85200)));
        assert_eq!(safety.reserve(DestructiveKind::Trash, 200, 86500), Ok(()));
    }
    #[test]
    fn empty_start_and_blocks_have_independent_intervals() {
        let mut safety = FileOpsSafety::new(2000);
        assert_eq!(safety.reserve_empty_start(100), Ok(()));
        assert_eq!(safety.reserve_empty_start(101), Err(59));
        assert_eq!(safety.reserve_empty_block(101), Ok(()));
        assert_eq!(safety.reserve_empty_block(102), Err(1));
        assert_eq!(safety.reserve_empty_block(103), Ok(()));
    }
    #[test]
    fn backups_are_readable_private_reused_and_zero_work_is_skipped() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("fileops-safety-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let db = dir.join("catalog.db");
        let conn = katalog_core::write::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE sample(value); INSERT INTO sample VALUES(42)")
            .unwrap();
        let safety = FileOpsSafety::new(2000);
        safety.ensure_backups(&conn, &db, 0, true, 864000).unwrap();
        assert!(!dir.join("backups").exists());
        safety.ensure_backups(&conn, &db, 1, true, 864000).unwrap();
        safety.ensure_backups(&conn, &db, 1, true, 864001).unwrap();
        let paths: Vec<_> = std::fs::read_dir(dir.join("backups"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(paths.len(), 2);
        for path in paths {
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let saved = katalog_core::write::Connection::open(path).unwrap();
            assert_eq!(
                saved
                    .query_row("SELECT value FROM sample", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                42
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn backups_keep_seven_calendar_days_and_five_events_without_pruning_foreign_files() {
        let dir = std::env::temp_dir().join(format!("fileops-retention-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let db = dir.join("db");
        let conn = katalog_core::write::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE value(v)").unwrap();
        let safety = FileOpsSafety::new(2000);
        for day in 10..20 {
            safety
                .ensure_backups(&conn, &db, 1, true, day * 86400)
                .unwrap();
        }
        let names: Vec<_> = std::fs::read_dir(dir.join("backups"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.iter().filter(|s| s.contains("-day-")).count(), 7);
        assert_eq!(names.iter().filter(|s| s.contains("-event-")).count(), 5);
        assert!(!names.contains(&"web-fileops-day-12.db".into()));
        std::fs::write(dir.join("backups/keep.db"), b"foreign").unwrap();
        safety
            .ensure_backups(&conn, &db, 1, true, 20 * 86400)
            .unwrap();
        assert_eq!(
            std::fs::read(dir.join("backups/keep.db")).unwrap(),
            b"foreign"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn corrupt_or_symlink_backup_cannot_be_reused() {
        use std::os::unix::fs::symlink;
        let dir = std::env::temp_dir().join(format!("fileops-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let db = dir.join("db");
        let conn = katalog_core::write::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE value(v)").unwrap();
        let safety = FileOpsSafety::new(2000);
        safety.ensure_backups(&conn, &db, 1, false, 864000).unwrap();
        let copy = dir.join("backups/web-fileops-day-10.db");
        std::fs::write(&copy, b"broken").unwrap();
        assert_eq!(
            safety.ensure_backups(&conn, &db, 1, false, 864001),
            Err(BackupFailed)
        );
        std::fs::remove_file(&copy).unwrap();
        symlink(&db, &copy).unwrap();
        assert_eq!(
            safety.ensure_backups(&conn, &db, 1, false, 864002),
            Err(BackupFailed)
        );
        assert!(std::fs::symlink_metadata(copy)
            .unwrap()
            .file_type()
            .is_symlink());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn relative_database_paths_can_create_backups() {
        let dir = std::env::temp_dir().join(format!("fileops-relative-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let absolute = dir.join("db");
        let conn = katalog_core::write::Connection::open(&absolute).unwrap();
        conn.execute_batch("CREATE TABLE sample(value)").unwrap();
        let mut relative = std::path::PathBuf::new();
        for component in std::env::current_dir().unwrap().components() {
            if matches!(component, std::path::Component::Normal(_)) {
                relative.push("..");
            }
        }
        relative.push(absolute.strip_prefix("/").unwrap());
        assert!(!relative.is_absolute());
        FileOpsSafety::new(2000)
            .ensure_backups(&conn, &relative, 1, false, 864000)
            .unwrap();
        assert!(dir.join("backups/web-fileops-day-10.db").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn backup_failure_and_space_shortage_are_errors() {
        assert_eq!(check_space(10, 10), Err(BackupFailed));
        let dir = std::env::temp_dir().join(format!("fileops-safety-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("backups"), b"not a directory").unwrap();
        let conn = katalog_core::write::Connection::open_in_memory().unwrap();
        assert_eq!(
            FileOpsSafety::new(2000).ensure_backups(&conn, &dir.join("db"), 1, true, 100),
            Err(BackupFailed)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
