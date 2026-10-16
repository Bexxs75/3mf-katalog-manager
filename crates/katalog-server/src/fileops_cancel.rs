//! Cancellation for atomic DB-only removals. Never attach this to filesystem
//! mutations: those must finish their journal/rollback after cancellation.
use katalog_core::write::Connection;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
#[derive(Clone)]
pub(crate) struct Cancellation {
    valid: Arc<dyn Fn() -> bool + Send + Sync>,
    interrupted: Arc<AtomicBool>,
}
impl Cancellation {
    pub(crate) fn new(valid: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            valid: Arc::new(valid),
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }
    pub(crate) fn interrupted(&self) -> bool {
        self.interrupted.load(Ordering::Acquire)
    }
    /// Hooks are scoped to a DB callback, including unwind. Interrupt only once:
    /// SQLite/Core rollback and transaction-drop cleanup must remain executable.
    pub(crate) fn run<T>(
        &self,
        conn: &mut Connection,
        f: impl FnOnce(&mut Connection) -> T,
    ) -> Result<T, ()> {
        if self.interrupted() {
            return Err(());
        }
        let committed = Arc::new(AtomicBool::new(false));
        let progress = self.clone();
        let progress_commit = committed.clone();
        conn.progress_handler(
            1,
            Some(move || {
                if progress.interrupted() || progress_commit.load(Ordering::Acquire) {
                    return false;
                }
                if !(progress.valid)() {
                    return !progress.interrupted.swap(true, Ordering::AcqRel);
                }
                false
            }),
        )
        .map_err(|_| ())?;
        let commit = self.clone();
        if conn
            .commit_hook(Some(move || {
                if commit.interrupted() || !(commit.valid)() {
                    commit.interrupted.store(true, Ordering::Release);
                    true
                } else {
                    committed.store(true, Ordering::Release);
                    false
                }
            }))
            .is_err()
        {
            let _ = conn.progress_handler(0, None::<fn() -> bool>);
            return Err(());
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(conn)));
        let cleared_progress = conn.progress_handler(0, None::<fn() -> bool>);
        let cleared_commit = conn.commit_hook(None::<fn() -> bool>);
        match result {
            Err(panic) => std::panic::resume_unwind(panic),
            Ok(value) => {
                if cleared_progress.is_err() || cleared_commit.is_err() {
                    Err(())
                } else {
                    Ok(value)
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::hooks::Action;
    #[test]
    fn revocation_after_first_delete_rolls_back_whole_core_removal() {
        let mut conn = katalog_core::db::connect_in_memory().unwrap();
        let a = katalog_core::db::test_insert_minimal_file(&conn, "/a.3mf", None).unwrap();
        let b = katalog_core::db::test_insert_minimal_file(&conn, "/b.3mf", None).unwrap();
        let valid = Arc::new(AtomicBool::new(true));
        let update_valid = valid.clone();
        conn.update_hook(Some(
            move |action: Action, _db: &str, table: &str, _id: i64| {
                if action == Action::SQLITE_DELETE && table == "files" {
                    update_valid.store(false, Ordering::Release)
                }
            },
        ))
        .unwrap();
        let check = valid.clone();
        let cancel = Cancellation::new(move || check.load(Ordering::Acquire));
        let result = cancel
            .run(&mut conn, |c| {
                katalog_core::write::web::remove_files_from_catalog(c, &[a, b])
            })
            .unwrap();
        assert!(result.is_err());
        assert!(cancel.interrupted());
        assert!(conn.is_autocommit());
        assert_eq!(
            conn.query_row("SELECT count(*) FROM files", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        // Hooks were removed: a new independent removal is not interrupted.
        assert!(katalog_core::write::web::remove_files_from_catalog(&mut conn, &[a, b]).is_ok());
    }
    #[test]
    fn hooks_are_removed_after_unwind() {
        let mut conn = Connection::open_in_memory().unwrap();
        let cancel = Cancellation::new(|| true);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cancel.run(&mut conn, |_| panic!("test panic"))
        }));
        assert!(panic.is_err());
        conn.execute_batch("CREATE TABLE t(id INTEGER);INSERT INTO t VALUES(1)")
            .unwrap();
    }
}
