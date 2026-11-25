use tauri::{Manager, Theme, WebviewWindow};
use tauri::window::Color;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
struct StartupShown(AtomicBool);

/// Whether the window should be maximized once it is visible (saved state, else the configured default).
struct WantMaximized(bool);

fn show_once(window: &WebviewWindow) -> tauri::Result<()> {
    if !window.state::<StartupShown>().0.swap(true, Ordering::SeqCst) {
        if let Err(error) = window.show() {
            window.state::<StartupShown>().0.store(false, Ordering::SeqCst);
            return Err(error);
        }
        // GTK under Wayland (GNOME) ignores maximizing a window that is not mapped yet; ask again now that it is
        // shown and, because the compositor answers late, check once more shortly afterwards.
        if window.state::<WantMaximized>().0 {
            if let Err(error) = window.maximize() { log::warn!(target: "startup", "maximizing after show failed: {error}"); }
            let late = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let maximized = late.is_maximized().unwrap_or(true);
                log::info!(target: "startup", "maximized 500 ms after show: {maximized}");
                if !maximized {
                    if let Err(error) = late.maximize() { log::warn!(target: "startup", "late maximize failed: {error}"); }
                }
            });
        }
    }
    Ok(())
}

/// The "maximized" flag of the main window in the file written by tauri-plugin-window-state.
fn saved_maximized(json: &str) -> Option<bool> {
    serde_json::from_str::<serde_json::Value>(json).ok()?.get("main")?.get("maximized")?.as_bool()
}

const MARKER: &str = "window-theme";

/// Explicit choices override the OS; system keeps following the platform theme.
fn background(marker: &str, system: Option<Theme>) -> Color {
    const DARK: Color = Color(16, 14, 12, 255);
    const SAND: Color = Color(211, 206, 202, 255);
    const WHITE: Color = Color(245, 243, 241, 255);
    const SLATE: Color = Color(62, 58, 54, 255);
    let dark = match marker.trim() {
        "white" => return WHITE,
        "slate" => return SLATE,
        "dark" => true,
        "light" => false,
        "system:dark" | "system:light" => system.map(|theme| theme == Theme::Dark)
            .unwrap_or(marker.trim() == "system:dark"),
        _ => system == Some(Theme::Dark),
    };
    // sRGB equivalents of the first-frame colors in public/theme-boot.css.
    if dark { DARK } else { SAND }
}

pub fn create_window(app: &tauri::App) -> tauri::Result<()> {
    app.manage(StartupShown::default());
    let marker = app.path().app_data_dir().ok()
        .and_then(|dir| std::fs::read_to_string(dir.join(MARKER)).ok()).unwrap_or_default();
    let config = &app.config().app.windows[0];
    let saved = app.path().app_config_dir().ok()
        .and_then(|dir| std::fs::read_to_string(dir.join(".window-state.json")).ok())
        .and_then(|json| saved_maximized(&json));
    let maximized = saved.unwrap_or(config.maximized);
    app.manage(WantMaximized(maximized));
    let window = tauri::WebviewWindowBuilder::from_config(app, config)?
        .maximized(maximized).visible(false).background_color(background(&marker, None)).build()?;
    // Query the platform only after the native window exists, while it is hidden.
    window.set_background_color(Some(background(&marker, window.theme().ok())))?;
    let reserve = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if let Err(error) = show_once(&reserve) { log::warn!(target: "startup", "showing startup reserve failed: {error}"); }
    });
    Ok(())
}

#[tauri::command]
pub fn frontend_ready(window: WebviewWindow) -> Result<(), String> {
    show_once(&window).map_err(|e| e.to_string())
}

fn valid_theme_pair(setting: &str, resolved: &str) -> bool {
    matches!(setting, "white" | "light" | "slate" | "dark" | "system")
        && matches!(resolved, "white" | "light" | "slate" | "dark")
        && !(setting == "system" && matches!(resolved, "white" | "slate"))
}

#[tauri::command]
pub fn save_window_theme(window: WebviewWindow, setting: String, resolved: String) -> Result<(), String> {
    if !valid_theme_pair(&setting, &resolved) {
        return Err("Ungültiges Design".into());
    }
    let marker = if setting == "system" { format!("system:{resolved}") } else { setting };
    let dir = window.app_handle().path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(MARKER);
    std::fs::write(&path, &marker).map_err(|e| e.to_string())?;
    crate::safe_file::harden_permissions(&path);
    // The frontend's media query is authoritative if the platform cannot report it.
    window.set_background_color(Some(background(&marker, Some(if matches!(resolved.as_str(), "white" | "light") { Theme::Light } else { Theme::Dark }))))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_and_system_backgrounds() {
        let dark = Color(16, 14, 12, 255);
        let light = Color(211, 206, 202, 255);
        let white = Color(245, 243, 241, 255);
        let slate = Color(62, 58, 54, 255);
        for system in [Some(Theme::Light), Some(Theme::Dark), None] {
            assert_eq!(background("white", system), white);
        }
        assert_eq!(background("dark", Some(Theme::Light)), dark);
        assert_eq!(background("light", Some(Theme::Dark)), light);
        assert_eq!(background("system:light", Some(Theme::Dark)), dark);
        assert_eq!(background("system:dark", Some(Theme::Light)), light);
        assert_eq!(background("system:dark", None), dark);
        assert_eq!(background("system:light", None), light);
        assert_eq!(background("slate", Some(Theme::Light)), slate);
        assert_eq!(background("slate", None), slate);
        assert_eq!(background("invalid", Some(Theme::Dark)), dark);
        assert_eq!(background("", None), light);
    }

    #[test]
    fn reads_the_saved_maximized_flag() {
        assert_eq!(saved_maximized(r#"{"main":{"width":1100,"maximized":true}}"#), Some(true));
        assert_eq!(saved_maximized(r#"{"main":{"maximized":false}}"#), Some(false));
        assert_eq!(saved_maximized(r#"{"other":{"maximized":true}}"#), None);
        assert_eq!(saved_maximized("not json"), None);
        assert_eq!(saved_maximized(""), None);
    }

    #[test]
    fn accepts_fixed_themes_and_only_light_or_dark_for_system() {
        for theme in ["white", "light", "slate", "dark"] {
            assert!(valid_theme_pair(theme, theme));
        }
        assert!(valid_theme_pair("system", "light"));
        assert!(valid_theme_pair("system", "dark"));
        assert!(!valid_theme_pair("system", "white"));
        assert!(!valid_theme_pair("system", "slate"));
        assert!(!valid_theme_pair("unknown", "light"));
        assert!(!valid_theme_pair("light", "unknown"));
        assert!(!valid_theme_pair("system", "system"));
    }
}
