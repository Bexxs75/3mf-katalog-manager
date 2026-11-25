//! Log capture for tests: one process-wide logger that records, per test thread, every record
//! as `(level, target, text)`. Tests that look at log lines share it, because only one logger
//! can be installed.
use std::cell::RefCell;

thread_local! {
    static RECORDS: RefCell<Vec<(log::Level, String, String)>> = const { RefCell::new(Vec::new()) };
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        RECORDS.with(|r| r.borrow_mut().push((record.level(), record.target().to_string(), record.args().to_string())));
    }
    fn flush(&self) {}
}

/// Runs `f` and returns what this thread logged meanwhile.
pub(crate) fn capture(f: impl FnOnce()) -> Vec<(log::Level, String, String)> {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        log::set_logger(&Logger).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
    });
    RECORDS.with(|r| r.borrow_mut().clear());
    f();
    RECORDS.with(|r| std::mem::take(&mut *r.borrow_mut()))
}
