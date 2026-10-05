use std::fmt::Display;

pub(crate) const ALLOW_MULTIPLE_ENV: &str = "MFK_ALLOW_MULTIPLE_INSTANCES";

pub(crate) trait MainWindow {
    type Error: Display;
    fn unminimize(&self) -> Result<(), Self::Error>;
    fn show(&self) -> Result<(), Self::Error>;
    fn set_focus(&self) -> Result<(), Self::Error>;
}

pub(crate) fn single_instance_enabled(debug: bool, env: Option<&str>) -> bool {
    !(debug && env == Some("1"))
}

pub(crate) fn bring_to_front(window: &impl MainWindow) {
    // A failed window operation must not prevent the remaining attempts.
    for (step, result) in [
        ("unminimize", window.unminimize()),
        ("show", window.show()),
        ("set_focus", window.set_focus()),
    ] {
        if let Err(error) = result {
            log::warn!(target: "single_instance", "could not {step} main window: {error}");
        }
    }
}

impl<R: tauri::Runtime> MainWindow for tauri::WebviewWindow<R> {
    type Error = tauri::Error;

    fn unminimize(&self) -> Result<(), Self::Error> {
        tauri::WebviewWindow::unminimize(self)
    }
    fn show(&self) -> Result<(), Self::Error> {
        tauri::WebviewWindow::show(self)
    }
    fn set_focus(&self) -> Result<(), Self::Error> {
        tauri::WebviewWindow::set_focus(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeWindow {
        calls: RefCell<Vec<&'static str>>,
        fail_at: Option<&'static str>,
    }

    impl FakeWindow {
        fn call(&self, step: &'static str) -> Result<(), &'static str> {
            self.calls.borrow_mut().push(step);
            if self.fail_at == Some(step) {
                Err("window operation failed")
            } else {
                Ok(())
            }
        }
    }

    impl MainWindow for FakeWindow {
        type Error = &'static str;
        fn unminimize(&self) -> Result<(), Self::Error> {
            self.call("unminimize")
        }
        fn show(&self) -> Result<(), Self::Error> {
            self.call("show")
        }
        fn set_focus(&self) -> Result<(), Self::Error> {
            self.call("set_focus")
        }
    }

    #[test]
    fn brings_window_to_front_in_order() {
        let window = FakeWindow::default();
        bring_to_front(&window);
        assert_eq!(*window.calls.borrow(), ["unminimize", "show", "set_focus"]);
    }

    #[test]
    fn continues_after_each_window_error() {
        for step in ["unminimize", "show", "set_focus"] {
            let window = FakeWindow {
                fail_at: Some(step),
                ..Default::default()
            };
            bring_to_front(&window);
            assert_eq!(*window.calls.borrow(), ["unminimize", "show", "set_focus"]);
        }
    }

    #[test]
    fn only_debug_with_exact_opt_out_disables_single_instance() {
        assert!(!single_instance_enabled(true, Some("1")));
        for value in [None, Some(""), Some("0"), Some("true"), Some(" 1")] {
            assert!(single_instance_enabled(true, value));
        }
        for value in [None, Some("1"), Some("0")] {
            assert!(single_instance_enabled(false, value));
        }
    }
}
