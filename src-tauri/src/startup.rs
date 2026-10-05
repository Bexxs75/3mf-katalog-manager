use tauri::{Manager, Theme, WebviewWindow};
use tauri::window::Color;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
struct StartupShown(AtomicBool);

fn show_once(window: &WebviewWindow) -> tauri::Result<()> {
    if !window.state::<StartupShown>().0.swap(true, Ordering::SeqCst) {
        if let Err(error) = window.show() {
            window.state::<StartupShown>().0.store(false, Ordering::SeqCst);
            return Err(error);
        }
    }
    Ok(())
}

const MARKER: &str = "window-theme";

/// Explicit choices override the OS; system keeps following the platform theme.
fn background(marker: &str, system: Option<Theme>) -> Color {
    let dark = match marker.trim() {
        "dark" => true,
        "light" => false,
        "system:dark" | "system:light" => system.map(|theme| theme == Theme::Dark)
            .unwrap_or(marker.trim() == "system:dark"),
        _ => system == Some(Theme::Dark),
    };
    // sRGB equivalents of the first-frame colors in public/theme-boot.css.
    if dark { Color(16, 14, 12, 255) } else { Color(245, 243, 241, 255) }
}

pub fn create_window(app: &tauri::App) -> tauri::Result<()> {
    app.manage(StartupShown::default());
    let marker = app.path().app_data_dir().ok()
        .and_then(|dir| std::fs::read_to_string(dir.join(MARKER)).ok()).unwrap_or_default();
    let config = &app.config().app.windows[0];
    let window = tauri::WebviewWindowBuilder::from_config(app, config)?
        .visible(false).background_color(background(&marker, None)).build()?;
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

#[tauri::command]
pub fn save_window_theme(window: WebviewWindow, setting: String, resolved: String) -> Result<(), String> {
    if !matches!(setting.as_str(), "light" | "dark" | "system") || !matches!(resolved.as_str(), "light" | "dark") {
        return Err("Ungültiges Design".into());
    }
    let marker = if setting == "system" { format!("system:{resolved}") } else { setting };
    let dir = window.app_handle().path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(MARKER);
    std::fs::write(&path, &marker).map_err(|e| e.to_string())?;
    crate::harden_permissions(&path);
    // The frontend's media query is authoritative if the platform cannot report it.
    window.set_background_color(Some(background(&marker, Some(if resolved == "dark" { Theme::Dark } else { Theme::Light }))))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_and_system_backgrounds() {
        let dark = Color(16, 14, 12, 255);
        let light = Color(245, 243, 241, 255);
        assert_eq!(background("dark", Some(Theme::Light)), dark);
        assert_eq!(background("light", Some(Theme::Dark)), light);
        assert_eq!(background("system:light", Some(Theme::Dark)), dark);
        assert_eq!(background("system:dark", Some(Theme::Light)), light);
        assert_eq!(background("system:dark", None), dark);
        assert_eq!(background("system:light", None), light);
        assert_eq!(background("invalid", Some(Theme::Dark)), dark);
        assert_eq!(background("", None), light);
    }
}
