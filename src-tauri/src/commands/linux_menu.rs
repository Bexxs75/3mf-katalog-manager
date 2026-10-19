//! Tauri commands for the optional start-menu entry of the Linux AppImage.
//! The logic lives in `crate::linux_menu`; this layer reads the environment
//! and the stored decision and refreshes the desktop caches.
use super::*;
use crate::db::printer_link::{get_setting, set_setting};
use crate::linux_menu::{self as menu, Decision, MenuError, PromptInput, Variant};
use std::path::PathBuf;

const DECISION_KEY: &str = "linux_menu_decision";

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinuxMenuStatus {
    /// Linux AppImage run; everything else hides the feature.
    pub available: bool,
    /// Our entry exists.
    pub installed: bool,
    /// Another tool already integrates this AppImage.
    pub third_party: bool,
    /// A `.desktop` file of that name exists that is not ours.
    pub conflict: bool,
    /// Path of that file, so the user can rename or delete it.
    pub conflict_path: Option<String>,
    pub should_prompt: bool,
}

/// Everything the environment tells us, resolved once per call.
struct Context {
    variant: Variant,
    appimage: String,
    data_home: PathBuf,
}

fn context() -> Option<Context> {
    let appimage_env = std::env::var("APPIMAGE").ok();
    let appimage = menu::appimage_path(cfg!(target_os = "linux"), appimage_env.as_deref())?.to_string();
    let home = std::env::var("HOME").ok().or_else(|| dirs::home_dir().map(|h| h.to_string_lossy().into_owned()));
    let data_home = menu::data_home(std::env::var("XDG_DATA_HOME").ok().as_deref(), home.as_deref())?;
    Some(Context { variant: Variant::current(cfg!(feature = "preview")), appimage, data_home })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn to_cmd_error(e: MenuError) -> CmdError {
    match e {
        MenuError::ForeignFile => CmdError::expected("linuxMenuForeignFile"),
        MenuError::ThirdParty => CmdError::expected("linuxMenuThirdParty"),
        other => CmdError::expected(other.to_string()),
    }
}

fn read_decision(state: &State<'_, AppState>) -> CmdResult<Option<Decision>> {
    let conn = lock_db(state)?;
    Ok(Decision::parse(get_setting(&conn, DECISION_KEY).map_err(|e| e.to_string())?.as_deref()))
}

fn write_decision(state: &State<'_, AppState>, decision: Decision) -> CmdResult<()> {
    let conn = lock_db(state)?;
    set_setting(&conn, DECISION_KEY, &decision.serialize()).map_err(|e| e.to_string())?;
    Ok(())
}

fn status(state: &State<'_, AppState>) -> CmdResult<LinuxMenuStatus> {
    let Some(ctx) = context() else { return Ok(LinuxMenuStatus::default()) };
    let p = menu::paths(ctx.variant, &ctx.data_home);
    let icon = menu::icon_value(&p.launcher_icon).map_err(to_cmd_error)?;
    let entry = menu::entry_state(&p.desktop_file, &ctx.appimage, &icon).map_err(to_cmd_error)?;
    let third_party = menu::third_party_entry_exists(ctx.variant, &p.applications_dir, &ctx.appimage);
    let installed = matches!(entry, menu::EntryState::Managed { .. });
    let conflict = entry == menu::EntryState::Foreign;
    let should_prompt = menu::should_prompt(&PromptInput {
        applicable: true,
        decision: read_decision(state)?,
        own_entry_exists: installed,
        foreign_file_in_the_way: conflict,
        third_party_entry: third_party,
        now_secs: now_secs(),
    });
    let conflict_path = conflict.then(|| p.desktop_file.to_string_lossy().into_owned());
    Ok(LinuxMenuStatus { available: true, installed, third_party, conflict, conflict_path, should_prompt })
}

/// Best effort: menus and icon themes pick up changes on their own at the
/// latest after a new login, so failures here are ignored.
fn refresh_desktop_caches(ctx: &Context) {
    let p = menu::paths(ctx.variant, &ctx.data_home);
    let run = |program: &str, args: &[&std::ffi::OsStr]| {
        let mut cmd = std::process::Command::new(program);
        super::external_env::sanitize_external_command(&mut cmd);
        // The AppImage runtime variables would point these host tools at the app's bundled files.
        for var in ["APPIMAGE", "APPDIR", "ARGV0", "OWD"] {
            cmd.env_remove(var);
        }
        let _ = cmd
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    };
    run("update-desktop-database", &[p.applications_dir.as_os_str()]);
    menu::touch_icon_theme_dir(&p.hicolor_dir);
    // Only refresh a cache that already exists: creating one would hide icons
    // that other programs later add to this folder without updating it.
    if p.hicolor_dir.join("icon-theme.cache").exists() {
        run("gtk-update-icon-cache", &["-f".as_ref(), "-t".as_ref(), p.hicolor_dir.as_os_str()]);
    }
}

/// Called once at startup: keeps an entry we wrote pointing at the AppImage
/// that is running now (it may have been moved or replaced by hand).
pub(crate) fn sync_linux_menu_path() {
    let Some(ctx) = context() else { return };
    match menu::sync_path(ctx.variant, &ctx.appimage, &ctx.data_home) {
        Ok(true) => {
            log::info!(target: "linux_menu", "start-menu entry updated (AppImage path or icon)");
            refresh_desktop_caches(&ctx);
        }
        Ok(false) => {}
        Err(e) => log::warn!(target: "linux_menu", "start-menu entry not updated: {e}"),
    }
}

#[tauri::command]
pub fn get_linux_menu_status(state: State<'_, AppState>) -> CmdResult<LinuxMenuStatus> {
    status(&state)
}

#[tauri::command]
pub fn set_linux_menu_enabled(enabled: bool, state: State<'_, AppState>) -> CmdResult<LinuxMenuStatus> {
    let Some(ctx) = context() else { return Ok(LinuxMenuStatus::default()) };
    if enabled {
        menu::install(ctx.variant, &ctx.appimage, &ctx.data_home).map_err(to_cmd_error)?;
        write_decision(&state, Decision::Enabled)?;
    } else {
        menu::remove(ctx.variant, &ctx.data_home).map_err(to_cmd_error)?;
        // Removed on purpose: do not ask again.
        write_decision(&state, Decision::Never)?;
    }
    refresh_desktop_caches(&ctx);
    status(&state)
}

/// Answer to the first-run question: "yes", "not_now" or "never".
#[tauri::command]
pub fn answer_linux_menu_prompt(answer: String, state: State<'_, AppState>) -> CmdResult<LinuxMenuStatus> {
    match answer.as_str() {
        "yes" => return set_linux_menu_enabled(true, state),
        "not_now" => write_decision(&state, Decision::NotNow(now_secs()))?,
        "never" => write_decision(&state, Decision::Never)?,
        other => return Err(CmdError::from(format!("unknown answer: {other}"))),
    }
    status(&state)
}
