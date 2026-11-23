//! Fills `missing` of the folder list. The check touches the disk, so it is bounded three
//! ways: the core limits (entries, time), one check at a time, and a thread of its own with a
//! deadline, because a hanging network mount can block a directory read for good. When any of
//! that fails the folders simply stay "not missing" (the list never waits for the disk).
use katalog_core::{disk_report, read::FolderDto};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const MAX_FOLDERS: usize = 20_000;
const CORE_BUDGET: Duration = Duration::from_secs(3);
const DEADLINE: Duration = Duration::from_secs(5);

static RUNNING: AtomicBool = AtomicBool::new(false);

struct Release;
impl Drop for Release {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::Release);
    }
}

pub fn annotate(folders: &mut [FolderDto], roots: &[PathBuf]) {
    annotate_with(folders, roots, disk_report::Limits { max_entries: MAX_FOLDERS, max_duration: CORE_BUDGET, ..Default::default() }, DEADLINE);
}

fn annotate_with(folders: &mut [FolderDto], roots: &[PathBuf], limits: disk_report::Limits, deadline: Duration) {
    if folders.is_empty() || RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let rows: Vec<(i64, String)> = folders.iter().filter_map(|f| Some((f.id.parse().ok()?, f.path.clone()))).collect();
    let roots = roots.to_vec();
    let (tx, rx) = std::sync::mpsc::channel();
    let started = std::thread::Builder::new().name("folder-missing".into()).spawn(move || {
        // Released only when the check really ends, so a stuck one blocks further checks.
        let _release = Release;
        let _ = tx.send(disk_report::missing_folder_ids(&rows, &roots, limits).0);
    });
    if started.is_err() {
        RUNNING.store(false, Ordering::Release);
        return;
    }
    if let Ok(missing) = rx.recv_timeout(deadline) {
        for folder in folders.iter_mut() {
            folder.missing = folder.id.parse().is_ok_and(|id: i64| missing.contains(&id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // The single-flight flag is process-wide, so these tests must not overlap.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn dto(id: &str, path: &std::path::Path) -> FolderDto {
        FolderDto { id: id.into(), name: id.into(), path: path.to_str().unwrap().into(), parent_id: None, count: 0, missing: false, move_target: true }
    }
    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("folder-missing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn marks_only_folders_gone_from_disk() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tmp("mark");
        std::fs::create_dir_all(root.join("here")).unwrap();
        std::fs::write(root.join("keep.txt"), b"x").unwrap();
        let mut folders = vec![dto("1", &root.join("here")), dto("2", &root.join("gone"))];
        annotate(&mut folders, std::slice::from_ref(&root));
        assert_eq!(folders.iter().map(|f| f.missing).collect::<Vec<_>>(), vec![false, true]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_check_already_running_leaves_the_list_unchanged() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tmp("busy");
        std::fs::write(root.join("keep.txt"), b"x").unwrap();
        let mut folders = vec![dto("1", &root.join("gone"))];
        RUNNING.store(true, Ordering::Release);
        annotate(&mut folders, std::slice::from_ref(&root));
        RUNNING.store(false, Ordering::Release);
        assert!(!folders[0].missing);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_check_past_its_deadline_is_dropped_and_frees_the_flag_when_it_ends() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tmp("slow");
        std::fs::write(root.join("keep.txt"), b"x").unwrap();
        let mut folders = vec![dto("1", &root.join("gone"))];
        // A zero deadline cannot be met: the answer is discarded.
        annotate_with(&mut folders, std::slice::from_ref(&root), disk_report::Limits::default(), Duration::ZERO);
        assert!(!folders[0].missing);
        for _ in 0..100 {
            if !RUNNING.load(Ordering::Acquire) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("flag never released");
    }
}
