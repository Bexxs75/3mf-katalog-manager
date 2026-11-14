//! Tauri commands for the window-close behavior ("quit" or "keep running in the notification area").
//! The decision logic and the native icon live in `crate::tray`; this layer stores the wish
//! and reports what actually works.
use super::runtime_environment::{DesktopFeature, RuntimeEnvironment};
use super::*;
use crate::db::printer_link::set_setting;
use crate::tray::{self, AppTray, CloseAction, CloseBehavior, TrayHost, TrayLabels};

/// Longest menu text accepted from the frontend; anything longer is cut.
const MAX_LABEL_CHARS: usize = 60;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloseBehaviorDto {
    /// The stored wish: `exit` or `tray`.
    pub action: &'static str,
    /// An icon exists right now. Only then does `tray` hide the window.
    pub tray_available: bool,
    /// False in the container image: there is no notification area.
    pub supported: bool,
    /// `windows`, `macos`, `linux` or `other`; the texts differ per system.
    pub platform: &'static str,
}

fn platform() -> &'static str {
    if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "macos" } else if cfg!(target_os = "linux") { "linux" } else { "other" }
}

fn dto(behavior: &CloseBehavior, container: bool) -> CloseBehaviorDto {
    CloseBehaviorDto { action: behavior.action().as_str(), tray_available: behavior.tray_ok(), supported: !container, platform: platform() }
}

fn clean_label(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() { fallback.to_string() } else { value.chars().take(MAX_LABEL_CHARS).collect() }
}

/// Stores the wish first, then brings the icon in line: a failed write changes nothing.
fn set_action(
    conn: &rusqlite::Connection, behavior: &CloseBehavior, environment: RuntimeEnvironment, host: &impl TrayHost, action: &str,
) -> CmdResult<CloseBehaviorDto> {
    let action = match action {
        "exit" => CloseAction::Exit,
        "tray" => CloseAction::Tray,
        other => return Err(CmdError::from(format!("unknown close action: {other}"))),
    };
    if action == CloseAction::Tray {
        environment.require_desktop(DesktopFeature::Tray)?;
    }
    set_setting(conn, tray::CLOSE_ACTION_KEY, action.as_str()).map_err(|e| e.to_string())?;
    behavior.apply(action, environment.container, host).map_err(|_| CmdError::expected("containerExternalUnavailable"))?;
    Ok(dto(behavior, environment.container))
}

#[tauri::command]
pub fn get_close_behavior(behavior: State<'_, CloseBehavior>, environment: State<'_, RuntimeEnvironment>) -> CloseBehaviorDto {
    dto(&behavior, environment.container)
}

// Not async on purpose: sync commands run on the main thread, where the native icon must be created.
#[tauri::command]
pub fn set_close_behavior<R: tauri::Runtime>(
    action: String, app: tauri::AppHandle<R>, state: State<'_, AppState>, behavior: State<'_, CloseBehavior>, environment: State<'_, RuntimeEnvironment>,
) -> CmdResult<CloseBehaviorDto> {
    let conn = lock_db(&state)?;
    set_action(&conn, &behavior, *environment, &AppTray(app), &action)
}

/// Menu texts in the current app language; called at start and after every language change.
#[tauri::command]
pub fn set_tray_labels<R: tauri::Runtime>(open: String, quit: String, app: tauri::AppHandle<R>, behavior: State<'_, CloseBehavior>) -> CmdResult<()> {
    let fallback = TrayLabels::default();
    let labels = TrayLabels { open: clean_label(&open, &fallback.open), quit: clean_label(&quit, &fallback.quit) };
    behavior.apply_labels(labels, &AppTray(app));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Default)]
    struct FakeHost { builds: Cell<u32>, removes: Cell<u32>, fail: bool }

    impl TrayHost for FakeHost {
        fn build(&self, _: &TrayLabels) -> Result<(), String> {
            self.builds.set(self.builds.get() + 1);
            if self.fail { Err("no tray".into()) } else { Ok(()) }
        }
        fn update_labels(&self, _: &TrayLabels) -> Result<(), String> { Ok(()) }
        fn remove(&self) { self.removes.set(self.removes.get() + 1); }
    }

    const DESKTOP: RuntimeEnvironment = RuntimeEnvironment { container: false };
    const CONTAINER: RuntimeEnvironment = RuntimeEnvironment { container: true };

    fn stored(conn: &rusqlite::Connection) -> Option<String> {
        crate::db::printer_link::get_setting(conn, tray::CLOSE_ACTION_KEY).unwrap()
    }

    #[test]
    fn choosing_the_tray_stores_the_wish_and_builds_the_icon() {
        let conn = katalog_core::db::connect_in_memory().unwrap();
        let behavior = CloseBehavior::from_db(&conn);
        let host = FakeHost::default();
        let answer = set_action(&conn, &behavior, DESKTOP, &host, "tray").unwrap();
        assert_eq!(answer, CloseBehaviorDto { action: "tray", tray_available: true, supported: true, platform: platform() });
        assert_eq!(stored(&conn).as_deref(), Some("tray"));
        let back = set_action(&conn, &behavior, DESKTOP, &host, "exit").unwrap();
        assert_eq!((back.action, back.tray_available), ("exit", false));
        assert_eq!((stored(&conn).as_deref(), host.removes.get()), (Some("exit"), 1));
    }

    #[test]
    fn a_failed_build_keeps_the_wish_but_reports_the_tray_as_unavailable() {
        let conn = katalog_core::db::connect_in_memory().unwrap();
        let behavior = CloseBehavior::from_db(&conn);
        let host = FakeHost { fail: true, ..Default::default() };
        let answer = set_action(&conn, &behavior, DESKTOP, &host, "tray").unwrap();
        assert_eq!((answer.action, answer.tray_available), ("tray", false));
        assert_eq!(behavior.decide(false, "main"), tray::CloseDecision::Close);
        // No second attempt in this run.
        set_action(&conn, &behavior, DESKTOP, &host, "tray").unwrap();
        assert_eq!(host.builds.get(), 1);
    }

    #[test]
    fn the_container_refuses_the_tray_and_changes_nothing() {
        let conn = katalog_core::db::connect_in_memory().unwrap();
        let behavior = CloseBehavior::from_db(&conn);
        let host = FakeHost::default();
        let error = set_action(&conn, &behavior, CONTAINER, &host, "tray").unwrap_err();
        assert_eq!(error.message, "containerExternalUnavailable");
        assert_eq!((host.builds.get(), stored(&conn)), (0, None));
        assert!(!dto(&behavior, true).supported);
        assert_eq!(set_action(&conn, &behavior, CONTAINER, &host, "exit").unwrap().action, "exit");
    }

    #[test]
    fn unknown_actions_are_rejected_without_a_trace() {
        let conn = katalog_core::db::connect_in_memory().unwrap();
        let behavior = CloseBehavior::from_db(&conn);
        let host = FakeHost::default();
        assert!(set_action(&conn, &behavior, DESKTOP, &host, "minimize").is_err());
        assert_eq!((host.builds.get(), stored(&conn)), (0, None));
    }

    #[test]
    fn menu_texts_are_trimmed_cut_and_never_empty() {
        assert_eq!(clean_label("  Öffnen ", "Open"), "Öffnen");
        assert_eq!(clean_label("   ", "Open"), "Open");
        assert_eq!(clean_label(&"ä".repeat(200), "Open").chars().count(), MAX_LABEL_CHARS);
    }
}
