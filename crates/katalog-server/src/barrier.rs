//! Admission belongs to the work, not to the HTTP future waiting for it.
//!
//! Use the application's shared instance for every runtime catalog access. Acquire
//! `writer()` before queueing a mutating job (including printer sync or startup
//! reconciliation), and move its guard into the actual worker. Use `reader()` for
//! reads; a streaming response must keep that guard until its body ends. Never
//! acquire a second guard inside an already admitted job: draining must let its
//! cleanup finish. Cooperative jobs may inspect `snapshot().state` between units
//! of work, but must retain their guard through rollback and journal persistence.
//!
//! `enter_draining(timeout)` closes writer admission and returns an owned window
//! once writers finish. Readers remain allowed, including a backup snapshot.
//! Consume that window with `enter_exclusive(timeout)` to close reader admission
//! and wait for readers. On timeout, outstanding streams receive an abort signal
//! and admission reopens; callers must NOT replace the catalog after an error.
//! Bodies keep counting until they actually observe cancellation or are dropped.
//! Release the successful window to reopen, or complete it after a committed
//! replacement to keep admission closed until restart. Dropping either transition
//! future or an unfinished window reopens admission automatically.
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{watch, Notify};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Open,
    Draining,
    Exclusive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub state: State,
    pub writers: usize,
    pub readers: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarrierError {
    Maintenance,
    Timeout,
}

#[derive(Default, Debug)]
struct Inner {
    counts: Mutex<Snapshot>,
    changed: Notify,
    cancel_streams: watch::Sender<u64>,
}

/// Shared by HTTP handlers and background jobs, including future printer/startup jobs.
#[derive(Clone, Default, Debug)]
pub struct CatalogBarrier(Arc<Inner>);

#[derive(Debug)]
pub struct Guard {
    barrier: CatalogBarrier,
    writer: bool,
    stream_cancellation: watch::Receiver<u64>,
}

/// Owns the transition, including while an async drain is pending. Cancellation,
/// timeout and unwinding reopen admission. Only one window can exist at a time.
#[derive(Debug)]
pub struct MaintenanceWindow {
    barrier: CatalogBarrier,
    active: bool,
}

impl CatalogBarrier {
    pub fn snapshot(&self) -> Snapshot {
        *self.0.counts.lock().unwrap()
    }

    pub fn reader(&self) -> Result<Guard, BarrierError> {
        self.admit(false)
    }

    pub fn writer(&self) -> Result<Guard, BarrierError> {
        self.admit(true)
    }

    fn admit(&self, writer: bool) -> Result<Guard, BarrierError> {
        let mut counts = self.0.counts.lock().unwrap();
        if counts.state == State::Exclusive || (writer && counts.state != State::Open) {
            return Err(BarrierError::Maintenance);
        }
        if writer {
            counts.writers += 1;
        } else {
            counts.readers += 1;
        }
        Ok(Guard {
            barrier: self.clone(),
            writer,
            stream_cancellation: self.0.cancel_streams.subscribe(),
        })
    }

    pub async fn enter_draining(
        &self,
        timeout: Duration,
    ) -> Result<MaintenanceWindow, BarrierError> {
        {
            let mut counts = self.0.counts.lock().unwrap();
            if counts.state != State::Open {
                return Err(BarrierError::Maintenance);
            }
            counts.state = State::Draining;
        }
        let window = MaintenanceWindow {
            barrier: self.clone(),
            active: true,
        };
        self.wait_empty(timeout, false).await?;
        Ok(window)
    }

    async fn wait_empty(&self, timeout: Duration, readers: bool) -> Result<(), BarrierError> {
        tokio::time::timeout(timeout, async {
            loop {
                let changed = self.0.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let counts = self.snapshot();
                if counts.writers == 0 && (!readers || counts.readers == 0) {
                    return;
                }
                changed.await;
            }
        })
        .await
        .map_err(|_| BarrierError::Timeout)
    }
}

impl MaintenanceWindow {
    /// Consumes the draining window so cancellation of this future also reopens it.
    pub async fn enter_exclusive(mut self, timeout: Duration) -> Result<Self, BarrierError> {
        self.barrier.0.counts.lock().unwrap().state = State::Exclusive;
        if let Err(error) = self.barrier.wait_empty(timeout, true).await {
            // Bodies wake and close on their next poll. Never pretend that an
            // unpolled or blocked reader has already released its catalog access.
            self.barrier
                .0
                .cancel_streams
                .send_modify(|generation| *generation = generation.wrapping_add(1));
            self.reopen();
            return Err(error);
        }
        Ok(self)
    }

    pub fn release(mut self) {
        self.reopen();
    }

    /// After a committed replacement, keep admission closed until process restart.
    pub fn complete(mut self) {
        assert_eq!(self.barrier.snapshot().state, State::Exclusive);
        self.active = false;
    }

    fn reopen(&mut self) {
        if self.active {
            self.barrier.0.counts.lock().unwrap().state = State::Open;
            self.active = false;
            self.barrier.0.changed.notify_waiters();
        }
    }
}

impl Drop for MaintenanceWindow {
    fn drop(&mut self) {
        self.reopen();
    }
}

impl Guard {
    pub(crate) fn stream_cancelled(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let mut cancellation = self.stream_cancellation.clone();
        Box::pin(async move {
            let _ = cancellation.changed().await;
        })
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let mut counts = self.barrier.0.counts.lock().unwrap();
        if self.writer {
            counts.writers -= 1;
        } else {
            counts.readers -= 1;
        }
        drop(counts);
        self.barrier.0.changed.notify_waiters();
    }
}

pub(crate) fn maintenance() -> axum::response::Response {
    crate::web::error_coded(503, katalog_core::error_code::ErrorCode::Maintenance, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn readers_continue_in_draining_and_block_exclusive() {
        let barrier = CatalogBarrier::default();
        let window = barrier
            .enter_draining(Duration::from_secs(1))
            .await
            .unwrap();
        let a = barrier.reader().unwrap();
        let b = barrier.reader().unwrap();
        assert_eq!(barrier.snapshot().readers, 2);
        assert_eq!(barrier.writer().unwrap_err(), BarrierError::Maintenance);
        let waiting = tokio::spawn(window.enter_exclusive(Duration::from_secs(1)));
        tokio::task::yield_now().await;
        assert_eq!(barrier.reader().unwrap_err(), BarrierError::Maintenance);
        drop((a, b));
        waiting.await.unwrap().unwrap().release();
        assert_eq!(barrier.snapshot(), Snapshot::default());
    }

    #[tokio::test]
    async fn drain_timeout_and_cancel_restore_open() {
        let barrier = CatalogBarrier::default();
        let _writer = barrier.writer().unwrap();
        assert_eq!(
            barrier
                .enter_draining(Duration::from_millis(1))
                .await
                .unwrap_err(),
            BarrierError::Timeout
        );
        assert_eq!(barrier.snapshot().state, State::Open);
        let copy = barrier.clone();
        let pending =
            tokio::spawn(async move { copy.enter_draining(Duration::from_secs(10)).await });
        tokio::task::yield_now().await;
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        assert_eq!(barrier.snapshot().state, State::Open);
    }

    #[tokio::test]
    async fn detached_blocking_writer_counts_until_real_completion() {
        let barrier = CatalogBarrier::default();
        let guard = barrier.writer().unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finish, done) = std::sync::mpsc::channel();
        let request = tokio::spawn(async move {
            tokio::task::spawn_blocking(move || {
                let _guard = guard;
                started.send(()).unwrap();
                done.recv().unwrap();
            })
            .await
            .unwrap();
        });
        ready.await.unwrap();
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        assert_eq!(barrier.snapshot().writers, 1);
        assert_eq!(
            barrier
                .enter_draining(Duration::from_millis(1))
                .await
                .unwrap_err(),
            BarrierError::Timeout
        );
        finish.send(()).unwrap();
        barrier
            .enter_draining(Duration::from_secs(1))
            .await
            .unwrap()
            .release();
        assert_eq!(barrier.snapshot(), Snapshot::default());
    }
    #[tokio::test]
    async fn exclusive_timeout_cancel_and_drop_reopen_but_complete_stays_closed() {
        let barrier = CatalogBarrier::default();
        let reader = barrier.reader().unwrap();
        let window = barrier
            .enter_draining(Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(
            window
                .enter_exclusive(Duration::from_millis(1))
                .await
                .unwrap_err(),
            BarrierError::Timeout
        );
        assert_eq!(barrier.snapshot().state, State::Open);
        let window = barrier
            .enter_draining(Duration::from_secs(1))
            .await
            .unwrap();
        let exclusive = tokio::spawn(window.enter_exclusive(Duration::from_secs(10)));
        tokio::task::yield_now().await;
        exclusive.abort();
        assert!(exclusive.await.unwrap_err().is_cancelled());
        assert_eq!(barrier.snapshot().state, State::Open);
        drop(reader);
        let window = barrier
            .enter_draining(Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(
            barrier.enter_draining(Duration::ZERO).await.unwrap_err(),
            BarrierError::Maintenance
        );
        assert_eq!(barrier.snapshot().state, State::Draining);
        drop(
            window
                .enter_exclusive(Duration::from_secs(1))
                .await
                .unwrap(),
        );
        assert_eq!(barrier.snapshot().state, State::Open);
        barrier
            .enter_draining(Duration::from_secs(1))
            .await
            .unwrap()
            .enter_exclusive(Duration::from_secs(1))
            .await
            .unwrap()
            .complete();
        assert_eq!(barrier.snapshot().state, State::Exclusive);
        assert!(barrier.reader().is_err() && barrier.writer().is_err());
    }
}
