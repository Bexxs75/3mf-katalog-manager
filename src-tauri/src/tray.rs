//! Close behavior: "quit" (default) or "keep running in the notification area".
//!
//! The window-close hook must never wait for the database lock, so the wish
//! (`close_action` in `app_settings`) is mirrored in [`CloseBehavior`]. What counts is
//! not the stored wish but whether the tray icon was actually built (`tray_ok`):
//! without a reachable icon a hidden window could never be brought back.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rusqlite::Connection;

/// `app_settings` key. Values: `exit` (default, also for a missing or unknown value) and `tray`.
pub(crate) const CLOSE_ACTION_KEY: &str = "close_action";
const MAIN_WINDOW: &str = "main";
const TRAY_ID: &str = "main";
const MENU_OPEN: &str = "tray-open";
const MENU_QUIT: &str = "tray-quit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloseAction {
    Exit,
    Tray,
}

impl CloseAction {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value {
            Some("tray") => Self::Tray,
            _ => Self::Exit,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Tray => "tray",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloseDecision {
    Hide,
    Close,
}

/// What a close request on `label` does. `Hide` only with a built tray, outside a container.
pub(crate) fn close_decision(action: CloseAction, tray_ok: bool, container: bool, label: &str) -> CloseDecision {
    if action == CloseAction::Tray && tray_ok && !container && label == MAIN_WINDOW {
        CloseDecision::Hide
    } else {
        CloseDecision::Close
    }
}

/// Menu texts come from the frontend (it owns every wording); until then an English fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrayLabels {
    pub open: String,
    pub quit: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        Self { open: "Open".into(), quit: "Quit".into() }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TrayError {
    /// Container image: there is no notification area.
    Unsupported,
}

/// Builds and removes the native icon. A trait so tests need no display (pattern of `single_instance::MainWindow`).
pub(crate) trait TrayHost {
    fn build(&self, labels: &TrayLabels) -> Result<(), String>;
    /// Replaces the menu texts of an existing icon.
    fn update_labels(&self, labels: &TrayLabels) -> Result<(), String>;
    fn remove(&self);
}

#[derive(Default)]
pub(crate) struct CloseBehavior {
    tray: AtomicBool,
    tray_ok: AtomicBool,
    /// A build failed in this run. Not retried until the next start: after a panic inside the
    /// lazily loaded appindicator library a second attempt is unlikely to behave better.
    tray_failed: AtomicBool,
    labels: Mutex<Option<TrayLabels>>,
}

impl CloseBehavior {
    /// Start state: the stored wish only; no icon exists yet.
    pub(crate) fn from_db(conn: &Connection) -> Self {
        let stored = crate::db::printer_link::get_setting(conn, CLOSE_ACTION_KEY).ok().flatten();
        let behavior = Self::default();
        behavior.tray.store(CloseAction::parse(stored.as_deref()) == CloseAction::Tray, Ordering::SeqCst);
        behavior
    }

    pub(crate) fn action(&self) -> CloseAction {
        if self.tray.load(Ordering::SeqCst) { CloseAction::Tray } else { CloseAction::Exit }
    }

    pub(crate) fn tray_ok(&self) -> bool {
        self.tray_ok.load(Ordering::SeqCst)
    }

    pub(crate) fn decide(&self, container: bool, label: &str) -> CloseDecision {
        close_decision(self.action(), self.tray_ok(), container, label)
    }

    fn labels(&self) -> TrayLabels {
        self.labels.lock().ok().and_then(|labels| labels.clone()).unwrap_or_default()
    }

    /// Remembers new menu texts; returns whether an icon exists whose menu must be rebuilt.
    pub(crate) fn set_labels(&self, labels: TrayLabels) -> bool {
        if let Ok(mut slot) = self.labels.lock() {
            *slot = Some(labels);
        }
        self.tray_ok()
    }

    pub(crate) fn tray_failed(&self) -> bool {
        self.tray_failed.load(Ordering::SeqCst)
    }

    /// Remembers new menu texts and refreshes the menu of an existing icon.
    pub(crate) fn apply_labels(&self, labels: TrayLabels, host: &impl TrayHost) {
        if self.set_labels(labels.clone()) {
            if let Err(error) = host.update_labels(&labels) {
                log::warn!(target: "tray", "could not update the tray menu: {error}");
            }
        }
    }

    /// Brings the icon in line with `action`. Returns whether the tray is available afterwards.
    /// A panic while building (Linux without libayatana-appindicator) counts as a failed build;
    /// the close behavior then stays "quit".
    pub(crate) fn apply(&self, action: CloseAction, container: bool, host: &impl TrayHost) -> Result<bool, TrayError> {
        if action == CloseAction::Tray && container {
            return Err(TrayError::Unsupported);
        }
        self.tray.store(action == CloseAction::Tray, Ordering::SeqCst);
        match action {
            CloseAction::Exit => {
                if self.tray_ok.swap(false, Ordering::SeqCst) {
                    host.remove();
                }
                Ok(false)
            }
            CloseAction::Tray => {
                if !self.tray_ok() && !self.tray_failed() {
                    let labels = self.labels();
                    let built = match catch_unwind(AssertUnwindSafe(|| host.build(&labels))) {
                        Ok(Ok(())) => true,
                        Ok(Err(error)) => {
                            log::warn!(target: "tray", "could not create the tray icon: {error}");
                            false
                        }
                        Err(_) => {
                            log::warn!(target: "tray", "creating the tray icon panicked (missing system library?)");
                            false
                        }
                    };
                    self.tray_ok.store(built, Ordering::SeqCst);
                    self.tray_failed.store(!built, Ordering::SeqCst);
                }
                Ok(self.tray_ok())
            }
        }
    }
}

#[cfg(target_os = "macos")]
/// Raw RGBA (black glyph, alpha only): decoding a PNG would need an extra tauri feature.
const MACOS_TEMPLATE_ICON: &[u8] = include_bytes!("../icons/tray-template.rgba");
#[cfg(target_os = "macos")]
const MACOS_TEMPLATE_SIZE: u32 = 44;

fn tray_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>, labels: &TrayLabels) -> Result<tauri::menu::Menu<R>, String> {
    use tauri::menu::{Menu, MenuItem};
    let open = MenuItem::with_id(app, MENU_OPEN, &labels.open, true, None::<&str>).map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(app, MENU_QUIT, &labels.quit, true, None::<&str>).map_err(|e| e.to_string())?;
    Menu::with_items(app, &[&open, &quit]).map_err(|e| e.to_string())
}

/// The real icon of a running app.
pub(crate) struct AppTray<R: tauri::Runtime>(pub tauri::AppHandle<R>);

impl<R: tauri::Runtime> TrayHost for AppTray<R> {
    fn build(&self, labels: &TrayLabels) -> Result<(), String> {
        use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        let app = &self.0;
        let mut builder = TrayIconBuilder::with_id(TRAY_ID)
            .menu(&tray_menu(app, labels)?)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id.as_ref() {
                MENU_OPEN => show_main_window(app),
                MENU_QUIT => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    show_main_window(tray.app_handle());
                }
            });
        // macOS shows a single-color template image in the menu bar; the colored app icon would look wrong there.
        #[cfg(target_os = "macos")]
        {
            builder = builder.icon(tauri::image::Image::new(MACOS_TEMPLATE_ICON, MACOS_TEMPLATE_SIZE, MACOS_TEMPLATE_SIZE)).icon_as_template(true);
        }
        #[cfg(not(target_os = "macos"))]
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
        builder.build(app).map(|_| ()).map_err(|e| e.to_string())
    }

    fn update_labels(&self, labels: &TrayLabels) -> Result<(), String> {
        let Some(tray) = self.0.tray_by_id(TRAY_ID) else { return Ok(()) };
        tray.set_menu(Some(tray_menu(&self.0, labels)?)).map_err(|e| e.to_string())
    }

    fn remove(&self) {
        let _ = self.0.remove_tray_by_id(TRAY_ID);
    }
}

/// Shows and focuses the main window (tray click/menu, macOS dock click).
pub(crate) fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        crate::single_instance::bring_to_front(&window);
    } else {
        log::warn!(target: "tray", "main window is unavailable");
    }
}

/// macOS dock click: bring the hidden window back only when nothing is visible.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn reopen_needs_window(has_visible_windows: bool) -> bool {
    !has_visible_windows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct FakeHost {
        builds: Cell<u32>,
        removes: Cell<u32>,
        seen_labels: RefCell<Vec<TrayLabels>>,
        updates: RefCell<Vec<TrayLabels>>,
        mode: &'static str, // "", "err", "panic"
    }

    impl TrayHost for FakeHost {
        fn build(&self, labels: &TrayLabels) -> Result<(), String> {
            self.builds.set(self.builds.get() + 1);
            self.seen_labels.borrow_mut().push(labels.clone());
            match self.mode {
                "err" => Err("no tray".into()),
                "panic" => panic!("library not found"),
                _ => Ok(()),
            }
        }
        fn update_labels(&self, labels: &TrayLabels) -> Result<(), String> {
            self.updates.borrow_mut().push(labels.clone());
            Ok(())
        }
        fn remove(&self) {
            self.removes.set(self.removes.get() + 1);
        }
    }

    #[test]
    fn close_decision_hides_only_with_built_tray_outside_container_for_main() {
        use CloseAction::*;
        for action in [Exit, Tray] {
            for tray_ok in [false, true] {
                for container in [false, true] {
                    for label in ["main", "other"] {
                        let expected = action == Tray && tray_ok && !container && label == "main";
                        let got = close_decision(action, tray_ok, container, label) == CloseDecision::Hide;
                        assert_eq!(got, expected, "{action:?} ok={tray_ok} container={container} {label}");
                    }
                }
            }
        }
        assert_eq!(close_decision(Tray, true, false, "main"), CloseDecision::Hide);
        assert_eq!(close_decision(Tray, false, false, "main"), CloseDecision::Close);
        assert_eq!(close_decision(Exit, true, false, "main"), CloseDecision::Close);
    }

    #[test]
    fn close_action_defaults_to_exit_and_reads_from_the_database() {
        assert_eq!(CloseAction::parse(None), CloseAction::Exit);
        assert_eq!(CloseAction::parse(Some("")), CloseAction::Exit);
        assert_eq!(CloseAction::parse(Some("minimize")), CloseAction::Exit);
        assert_eq!(CloseAction::parse(Some("TRAY")), CloseAction::Exit);
        assert_eq!(CloseAction::parse(Some("tray")), CloseAction::Tray);
        assert_eq!(CloseAction::Tray.as_str(), "tray");
        assert_eq!(CloseAction::Exit.as_str(), "exit");

        let conn = katalog_core::db::connect_in_memory().unwrap();
        assert_eq!(CloseBehavior::from_db(&conn).action(), CloseAction::Exit);
        crate::db::printer_link::set_setting(&conn, CLOSE_ACTION_KEY, "tray").unwrap();
        let behavior = CloseBehavior::from_db(&conn);
        assert_eq!(behavior.action(), CloseAction::Tray);
        // The wish alone never hides the window: no icon has been built yet.
        assert!(!behavior.tray_ok());
        assert_eq!(behavior.decide(false, "main"), CloseDecision::Close);
        crate::db::printer_link::set_setting(&conn, CLOSE_ACTION_KEY, "garbage").unwrap();
        assert_eq!(CloseBehavior::from_db(&conn).action(), CloseAction::Exit);
    }

    #[test]
    fn applying_tray_builds_once_and_exit_removes() {
        let behavior = CloseBehavior::default();
        let host = FakeHost::default();
        assert_eq!(behavior.apply(CloseAction::Tray, false, &host), Ok(true));
        assert_eq!(behavior.decide(false, "main"), CloseDecision::Hide);
        assert_eq!(behavior.apply(CloseAction::Tray, false, &host), Ok(true));
        assert_eq!(host.builds.get(), 1, "an existing icon is not built twice");
        assert_eq!(behavior.apply(CloseAction::Exit, false, &host), Ok(false));
        assert_eq!(host.removes.get(), 1);
        assert_eq!(behavior.decide(false, "main"), CloseDecision::Close);
        assert_eq!(behavior.apply(CloseAction::Exit, false, &host), Ok(false));
        assert_eq!(host.removes.get(), 1, "nothing to remove the second time");
    }

    #[test]
    fn failed_or_panicking_build_falls_back_to_exit() {
        for mode in ["err", "panic"] {
            let behavior = CloseBehavior::default();
            let host = FakeHost { mode, ..Default::default() };
            assert_eq!(behavior.apply(CloseAction::Tray, false, &host), Ok(false), "{mode}");
            assert_eq!(host.builds.get(), 1);
            assert!(!behavior.tray_ok());
            assert_eq!(behavior.decide(false, "main"), CloseDecision::Close, "{mode}");
        }
    }

    #[test]
    fn a_failed_build_is_not_retried_until_restart() {
        for mode in ["err", "panic"] {
            let behavior = CloseBehavior::default();
            let host = FakeHost { mode, ..Default::default() };
            assert_eq!(behavior.apply(CloseAction::Tray, false, &host), Ok(false));
            assert!(behavior.tray_failed());
            assert_eq!(behavior.apply(CloseAction::Tray, false, &host), Ok(false), "{mode}");
            assert_eq!(host.builds.get(), 1, "{mode}: no second attempt");
            assert_eq!(behavior.decide(false, "main"), CloseDecision::Close);
            // Choosing "quit" and then "tray" again still does not try again.
            behavior.apply(CloseAction::Exit, false, &host).unwrap();
            assert_eq!(behavior.apply(CloseAction::Tray, false, &host), Ok(false));
            assert_eq!(host.builds.get(), 1);
        }
        let working = CloseBehavior::default();
        assert!(!working.tray_failed());
        working.apply(CloseAction::Tray, false, &FakeHost::default()).unwrap();
        assert!(!working.tray_failed());
    }

    #[test]
    fn new_labels_refresh_only_an_existing_icon() {
        let behavior = CloseBehavior::default();
        let host = FakeHost::default();
        let labels = TrayLabels { open: "Abrir".into(), quit: "Salir".into() };
        behavior.apply_labels(labels.clone(), &host);
        assert!(host.updates.borrow().is_empty(), "no icon yet, nothing to update");
        behavior.apply(CloseAction::Tray, false, &host).unwrap();
        assert_eq!(host.seen_labels.borrow()[0], labels, "the stored texts are used for the build");
        let next = TrayLabels { open: "Ouvrir".into(), quit: "Quitter".into() };
        behavior.apply_labels(next.clone(), &host);
        assert_eq!(*host.updates.borrow(), vec![next]);
    }

    #[test]
    fn container_refuses_the_tray_without_building() {
        let behavior = CloseBehavior::default();
        let host = FakeHost::default();
        assert_eq!(behavior.apply(CloseAction::Tray, true, &host), Err(TrayError::Unsupported));
        assert_eq!(host.builds.get(), 0);
        assert_eq!(behavior.action(), CloseAction::Exit);
        assert_eq!(behavior.decide(true, "main"), CloseDecision::Close);
        // Quitting stays allowed in a container.
        assert_eq!(behavior.apply(CloseAction::Exit, true, &host), Ok(false));
    }

    #[test]
    fn menu_labels_are_used_for_the_build_with_english_fallback() {
        let behavior = CloseBehavior::default();
        assert!(!behavior.set_labels(TrayLabels { open: "Öffnen".into(), quit: "Beenden".into() }));
        let host = FakeHost::default();
        behavior.apply(CloseAction::Tray, false, &host).unwrap();
        assert_eq!(host.seen_labels.borrow()[0].open, "Öffnen");
        assert!(behavior.set_labels(TrayLabels::default()), "an existing icon needs a menu rebuild");

        let fresh = CloseBehavior::default();
        let host = FakeHost::default();
        fresh.apply(CloseAction::Tray, false, &host).unwrap();
        assert_eq!(host.seen_labels.borrow()[0], TrayLabels::default());
    }

    #[test]
    fn restoring_a_backup_with_tray_wish_builds_nothing() {
        // A restore only rewrites `app_settings`; the live state changes solely through `apply`.
        let conn = katalog_core::db::connect_in_memory().unwrap();
        let behavior = CloseBehavior::from_db(&conn);
        let host = FakeHost::default();
        crate::db::printer_link::set_setting(&conn, CLOSE_ACTION_KEY, "tray").unwrap();
        assert_eq!(behavior.action(), CloseAction::Exit);
        assert!(!behavior.tray_ok());
        assert_eq!(host.builds.get(), 0);
        assert_eq!(behavior.decide(false, "main"), CloseDecision::Close);
    }

    #[test]
    fn the_macos_template_image_is_a_44_pixel_rgba_square() {
        let bytes = include_bytes!("../icons/tray-template.rgba");
        assert_eq!(bytes.len(), 44 * 44 * 4);
        assert!(bytes.chunks(4).all(|px| px[..3] == [0, 0, 0]), "template images carry the shape in alpha only");
        assert!(bytes.chunks(4).any(|px| px[3] == 255) && bytes.chunks(4).any(|px| px[3] == 0));
    }

    #[test]
    fn reopen_shows_the_window_only_when_none_is_visible() {
        assert!(reopen_needs_window(false));
        assert!(!reopen_needs_window(true));
    }
}
