//! Optional start-menu entry for the Linux AppImage.
//!
//! An AppImage is a single file that brings no menu entry of its own, and the
//! `.desktop` file embedded in it is unusable from outside (relative `Exec`,
//! no categories). With the user's consent this module writes a proper entry
//! into the user's XDG data directory, keeps its path current and removes it
//! again. Everything here takes its environment (APPIMAGE, XDG_DATA_HOME, HOME,
//! clock) as parameters so it can be tested without touching global state.
//!
//! Nothing is ever written without consent, and a file this module did not
//! write (no marker line) is never overwritten or deleted.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Marker line proving that a `.desktop` file was written by this module.
const MARKER_LINE: &str = "X-3MF-Katalog-Managed=true";
/// Entries of other tools are only inspected up to this size.
const MAX_DESKTOP_BYTES: u64 = 64 * 1024;
/// "Not now" is asked again after this long.
const NOT_NOW_SECS: u64 = 30 * 24 * 60 * 60;

// The icons are the existing app icons; they are embedded so the AppImage can
// install them without unpacking itself.
const ICON_256: &[u8] = include_bytes!("../icons/128x128@2x.png");
const ICON_128: &[u8] = include_bytes!("../icons/128x128.png");
const ICON_32: &[u8] = include_bytes!("../icons/32x32.png");
const ICONS: [(u32, &[u8]); 3] = [(256, ICON_256), (128, ICON_128), (32, ICON_32)];

/// Which app identity an entry belongs to. The preview build is a separate
/// app with its own catalog, so it gets its own file and name and can never
/// replace the regular entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant {
    /// File stem of the `.desktop` file, icon name and `StartupWMClass`. The
    /// window class must equal the stem, otherwise Wayland shows a generic icon.
    pub stem: &'static str,
    pub display_name: &'static str,
}

pub const REGULAR: Variant = Variant { stem: "mf-katalog-manager", display_name: "3MF Katalog Manager" };
pub const PREVIEW: Variant = Variant { stem: "mf-katalog-manager-preview", display_name: "3MF Katalog Manager (Preview)" };

impl Variant {
    pub fn current(preview: bool) -> Self {
        if preview { PREVIEW } else { REGULAR }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MenuError {
    /// A `.desktop` file of that name exists but was not written by us.
    ForeignFile,
    /// Another tool (AppImageLauncher, appimaged, ...) already integrates this AppImage.
    ThirdParty,
    /// The AppImage path cannot be represented in an `Exec` line.
    BadPath,
    NoDataDir,
    Io(String),
}

impl std::fmt::Display for MenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignFile => write!(f, "A start-menu entry with this name already exists and was not created by this app."),
            Self::ThirdParty => write!(f, "This AppImage is already integrated by another tool."),
            Self::BadPath => write!(f, "The AppImage path cannot be used in a start-menu entry."),
            Self::NoDataDir => write!(f, "The user data directory could not be determined."),
            Self::Io(e) => write!(f, "The start-menu entry could not be written: {e}"),
        }
    }
}

fn io_err(e: std::io::Error) -> MenuError {
    MenuError::Io(e.to_string())
}

/// Path of the running AppImage, or `None` when this is not an AppImage run
/// (package installs ship their own entry) or not Linux.
pub fn appimage_path(os_is_linux: bool, appimage_env: Option<&str>) -> Option<&str> {
    appimage_env.filter(|v| os_is_linux && !v.is_empty())
}

/// `$XDG_DATA_HOME`, else `~/.local/share`. Relative values are ignored, as
/// the XDG specification requires.
pub fn data_home(xdg_data_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let absolute = |v: &&str| !v.is_empty() && Path::new(v).is_absolute();
    if let Some(dir) = xdg_data_home.filter(absolute) {
        return Some(PathBuf::from(dir));
    }
    home.filter(absolute).map(|h| Path::new(h).join(".local/share"))
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub applications_dir: PathBuf,
    pub desktop_file: PathBuf,
    pub hicolor_dir: PathBuf,
    pub icons: Vec<(PathBuf, &'static [u8])>,
    /// The largest icon file, referenced by absolute path from the launcher.
    pub launcher_icon: PathBuf,
}

pub fn paths(variant: Variant, data_home: &Path) -> Paths {
    let applications_dir = data_home.join("applications");
    let hicolor_dir = data_home.join("icons/hicolor");
    let icons = ICONS
        .iter()
        .map(|(size, bytes)| (hicolor_dir.join(format!("{size}x{size}/apps/{}.png", variant.stem)), *bytes))
        .collect();
    let launcher_icon = hicolor_dir.join(format!("256x256/apps/{}.png", variant.stem));
    Paths { desktop_file: applications_dir.join(format!("{}.desktop", variant.stem)), applications_dir, hicolor_dir, icons, launcher_icon }
}

/// Quotes one `Exec` argument following the Desktop Entry specification: the
/// argument is wrapped in double quotes, and `"`, `` ` ``, `$`, `\` are
/// backslash-escaped. A literal `%` must be `%%` (field-code syntax). The
/// backslashes are then doubled once more because the whole value is also a
/// desktop-file string, where `\\` stands for one backslash.
pub fn quote_exec_arg(arg: &str) -> String {
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\"); // escape char (itself doubled for the string layer)
                quoted.push(c);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// Splits an `Exec` value into its arguments, reversing the two escape layers
/// of `quote_exec_arg`. Field codes (`%U`, ...) are kept as they are; `%%`
/// becomes `%`. Tolerant of unquoted values written by other tools.
pub fn parse_exec(value: &str) -> Vec<String> {
    // String layer: `\\` is one backslash (other escapes are irrelevant for paths).
    let mut unescaped = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'\\') {
            chars.next();
        }
        unescaped.push(c);
    }
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut in_quotes = false;
    let mut it = unescaped.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                in_arg = true;
            }
            '\\' if in_quotes => {
                if let Some(next) = it.next() {
                    current.push(next);
                }
            }
            '%' if it.peek() == Some(&'%') => {
                it.next();
                current.push('%');
                in_arg = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            c => {
                current.push(c);
                in_arg = true;
            }
        }
    }
    if in_arg {
        args.push(current);
    }
    args
}

fn exec_value(content: &str) -> Option<&str> {
    let mut in_entry = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry {
            if let Some(v) = line.strip_prefix("Exec=") {
                return Some(v);
            }
        }
    }
    None
}

/// The program the entry launches (first `Exec` argument).
pub fn exec_program(content: &str) -> Option<String> {
    parse_exec(exec_value(content)?).into_iter().next()
}

pub fn is_managed(content: &str) -> bool {
    content.lines().any(|l| l.trim() == MARKER_LINE)
}

/// The `Icon=` value for an icon file. KDE Plasma does not resolve the theme
/// icon name of a launcher in the user's data folder, but it does show an
/// icon given as an absolute path. The value is not quoted (unlike `Exec`),
/// only a backslash is doubled because the value is a desktop-file string.
pub fn icon_value(icon: &Path) -> Result<String, MenuError> {
    let icon = icon.to_str().ok_or(MenuError::BadPath)?;
    if !Path::new(icon).is_absolute() || icon.chars().any(|c| c.is_control()) {
        return Err(MenuError::BadPath);
    }
    Ok(icon.replace('\\', "\\\\"))
}

fn icon_line(content: &str) -> Option<&str> {
    let mut in_entry = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry {
            if let Some(v) = line.strip_prefix("Icon=") {
                return Some(v);
            }
        }
    }
    None
}

/// The text of the `.desktop` file for the AppImage at `appimage`, with the
/// launcher icon at `icon`.
pub fn render_desktop(variant: Variant, appimage: &str, icon: &Path) -> Result<String, MenuError> {
    if !Path::new(appimage).is_absolute() || appimage.chars().any(|c| c.is_control()) {
        return Err(MenuError::BadPath);
    }
    let icon = icon_value(icon)?;
    // Exactly one main category: two make KDE list the entry twice. Graphics
    // fits a model catalog best; 3DGraphics is the registered additional
    // category for it (3D printing itself has no registered category).
    Ok(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Comment=Catalog and manage 3D printing model files\n\
         Comment[de]=3D-Druck-Modelldateien katalogisieren und verwalten\n\
         Exec={exec}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Graphics;3DGraphics;\n\
         StartupWMClass={stem}\n\
         {MARKER_LINE}\n",
        name = variant.display_name,
        exec = quote_exec_arg(appimage),
        stem = variant.stem,
    ))
}

/// Marks the icon theme folder as changed. Icons are written into folders that
/// already exist, which leaves this folder's modification time untouched, and
/// a running GNOME Shell on X11 only re-reads icons when it sees that time
/// move. Unlike writing an `icon-theme.cache`, this cannot hide icons that
/// other programs add later.
pub fn touch_icon_theme_dir(hicolor_dir: &Path) {
    if let Ok(dir) = fs::File::open(hicolor_dir) {
        let _ = dir.set_modified(std::time::SystemTime::now());
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EntryState {
    Absent,
    /// Written by us; `current` tells whether its `Exec` already points at the
    /// AppImage and its `Icon` at the icon file (older entries named a theme icon).
    Managed { current: bool },
    Foreign,
}

pub fn entry_state(desktop_file: &Path, appimage: &str, icon: &str) -> Result<EntryState, MenuError> {
    // Inspect the entry itself, not what it points at: reading follows symlinks,
    // so a foreign symlink with a missing target would look absent and then be
    // replaced by the install.
    match fs::symlink_metadata(desktop_file) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Ok(EntryState::Foreign),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(EntryState::Absent),
        Err(e) => return Err(io_err(e)),
    }
    match fs::read_to_string(desktop_file) {
        Ok(content) if is_managed(&content) => {
            let current = exec_program(&content).as_deref() == Some(appimage) && icon_line(&content) == Some(icon);
            Ok(EntryState::Managed { current })
        }
        Ok(_) => Ok(EntryState::Foreign),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(EntryState::Absent),
        // Not valid UTF-8 or unreadable: certainly not written by us.
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok(EntryState::Foreign),
        Err(e) => Err(io_err(e)),
    }
}

/// True when some other `.desktop` file in the applications folder launches
/// this very AppImage (AppImageLauncher, appimaged, a hand-made entry).
pub fn third_party_entry_exists(variant: Variant, applications_dir: &Path, appimage: &str) -> bool {
    let own = format!("{}.desktop", variant.stem);
    let Ok(entries) = fs::read_dir(applications_dir) else { return false };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "desktop") || path.file_name().is_some_and(|n| *n == *own) {
            return false;
        }
        let small = entry.metadata().is_ok_and(|m| m.is_file() && m.len() <= MAX_DESKTOP_BYTES);
        if !small {
            return false;
        }
        let Ok(content) = fs::read_to_string(&path) else { return false };
        !is_managed(&content)
            && exec_value(&content).is_some_and(|v| parse_exec(v).iter().any(|a| a == appimage))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Enabled,
    /// Declined for good, or the entry was removed again by the user.
    Never,
    /// Asked again after `NOT_NOW_SECS`; the value is the unix time of the answer.
    NotNow(u64),
}

impl Decision {
    pub fn parse(stored: Option<&str>) -> Option<Self> {
        match stored? {
            "enabled" => Some(Self::Enabled),
            "never" => Some(Self::Never),
            s => s.strip_prefix("not_now:")?.parse().ok().map(Self::NotNow),
        }
    }

    pub fn serialize(self) -> String {
        match self {
            Self::Enabled => "enabled".into(),
            Self::Never => "never".into(),
            Self::NotNow(at) => format!("not_now:{at}"),
        }
    }
}

pub struct PromptInput {
    /// Linux AppImage run.
    pub applicable: bool,
    pub decision: Option<Decision>,
    pub own_entry_exists: bool,
    pub foreign_file_in_the_way: bool,
    pub third_party_entry: bool,
    pub now_secs: u64,
}

/// Whether to ask "Show in the start menu?" now.
pub fn should_prompt(i: &PromptInput) -> bool {
    if !i.applicable || i.own_entry_exists || i.foreign_file_in_the_way || i.third_party_entry {
        return false;
    }
    match i.decision {
        None => true,
        Some(Decision::NotNow(at)) => i.now_secs.saturating_sub(at) >= NOT_NOW_SECS,
        // `Enabled` without an entry means the user deleted it themselves; respect that.
        Some(Decision::Enabled | Decision::Never) => false,
    }
}

/// Writes `bytes` to `target` via a temp file in the same folder and a rename,
/// so a crash or full disk never leaves a half-written entry behind.
fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), MenuError> {
    let dir = target.parent().ok_or(MenuError::NoDataDir)?;
    fs::create_dir_all(dir).map_err(io_err)?;
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("entry");
    let tmp = dir.join(format!(".{name}.tmp{}", std::process::id()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o644);
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        // `mode` is subject to the umask; make the final permissions explicit.
        #[cfg(unix)]
        fs::set_permissions(&tmp, std::os::unix::fs::PermissionsExt::from_mode(0o644))?;
        fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(io_err)
}

/// Creates (or repairs) our entry and icons. Refuses to touch a foreign file
/// and does nothing when another tool already integrates the AppImage.
pub fn install(variant: Variant, appimage: &str, data_home: &Path) -> Result<(), MenuError> {
    let p = paths(variant, data_home);
    let content = render_desktop(variant, appimage, &p.launcher_icon)?;
    let icon = icon_value(&p.launcher_icon)?;
    match entry_state(&p.desktop_file, appimage, &icon)? {
        EntryState::Foreign => return Err(MenuError::ForeignFile),
        EntryState::Absent if third_party_entry_exists(variant, &p.applications_dir, appimage) => {
            return Err(MenuError::ThirdParty)
        }
        _ => {}
    }
    // Icons first: a menu entry whose icon is missing would show a placeholder.
    for (path, bytes) in &p.icons {
        write_atomic(path, bytes)?;
    }
    write_atomic(&p.desktop_file, content.as_bytes())
}

/// Points an existing entry of ours at the current AppImage path and icon
/// file. Returns whether anything was rewritten. Without our entry (never
/// consented, or removed) this does nothing.
pub fn sync_path(variant: Variant, appimage: &str, data_home: &Path) -> Result<bool, MenuError> {
    let p = paths(variant, data_home);
    let icon = icon_value(&p.launcher_icon)?;
    match entry_state(&p.desktop_file, appimage, &icon)? {
        EntryState::Managed { current: false } => {
            let content = render_desktop(variant, appimage, &p.launcher_icon)?;
            // Icons first, as in `install`: an entry pointing at a missing file shows a blank icon.
            for (path, bytes) in &p.icons {
                write_atomic(path, bytes)?;
            }
            write_atomic(&p.desktop_file, content.as_bytes())?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Removes our entry and icons. Returns whether the `.desktop` file was removed.
/// A foreign file of the same name is left alone and reported as an error.
pub fn remove(variant: Variant, data_home: &Path) -> Result<bool, MenuError> {
    let p = paths(variant, data_home);
    // The AppImage path is irrelevant for ownership, only the marker counts.
    let removed = match entry_state(&p.desktop_file, "", "")? {
        EntryState::Foreign => return Err(MenuError::ForeignFile),
        EntryState::Absent => false,
        EntryState::Managed { .. } => {
            fs::remove_file(&p.desktop_file).map_err(io_err)?;
            true
        }
    };
    for (path, _) in &p.icons {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(e)),
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPIMAGE: &str = "/home/u/Apps/3MF Katalog Manager.AppImage";
    const ICON: &str = "/home/u/.local/share/icons/hicolor/256x256/apps/mf-katalog-manager.png";

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("linux-menu-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn only_linux_with_a_non_empty_appimage_variable_applies() {
        assert_eq!(appimage_path(true, Some(APPIMAGE)), Some(APPIMAGE));
        assert_eq!(appimage_path(true, Some("")), None);
        assert_eq!(appimage_path(true, None), None);
        assert_eq!(appimage_path(false, Some(APPIMAGE)), None);
    }

    #[test]
    fn data_home_prefers_absolute_xdg_then_home() {
        assert_eq!(data_home(Some("/x/data"), Some("/home/u")), Some(PathBuf::from("/x/data")));
        assert_eq!(data_home(Some("rel/data"), Some("/home/u")), Some(PathBuf::from("/home/u/.local/share")));
        assert_eq!(data_home(Some(""), Some("/home/u")), Some(PathBuf::from("/home/u/.local/share")));
        assert_eq!(data_home(None, Some("/home/u")), Some(PathBuf::from("/home/u/.local/share")));
        assert_eq!(data_home(None, Some("home")), None);
        assert_eq!(data_home(None, None), None);
    }

    #[test]
    fn exec_quoting_round_trips_awkward_paths() {
        for path in [
            "/plain/app.AppImage",
            APPIMAGE,
            "/a \"quoted\" dir/app.AppImage",
            "/100%/app.AppImage",
            "/cost$/`tick`/app.AppImage",
            "/back\\slash/app.AppImage",
            "/all \"\\$`% together/app.AppImage",
        ] {
            let quoted = quote_exec_arg(path);
            assert_eq!(parse_exec(&quoted), vec![path.to_string()], "{quoted}");
            let content = render_desktop(REGULAR, path, Path::new(ICON)).unwrap();
            assert_eq!(exec_program(&content).as_deref(), Some(path));
        }
    }

    #[test]
    fn exec_quoting_escapes_per_specification() {
        assert_eq!(quote_exec_arg("/a b"), "\"/a b\"");
        assert_eq!(quote_exec_arg("/100%"), "\"/100%%\"");
        assert_eq!(quote_exec_arg("/$x"), "\"/\\\\$x\"");
        assert_eq!(quote_exec_arg("/\"x"), "\"/\\\\\"x\"");
        assert_eq!(quote_exec_arg("/`x"), "\"/\\\\`x\"");
        assert_eq!(quote_exec_arg("/\\x"), "\"/\\\\\\\\x\"");
    }

    #[test]
    fn parse_exec_reads_unquoted_values_of_other_tools() {
        assert_eq!(parse_exec("/opt/app.AppImage %U"), vec!["/opt/app.AppImage", "%U"]);
        assert_eq!(parse_exec("\"/o p/app\" --flag"), vec!["/o p/app", "--flag"]);
    }

    #[test]
    fn rendering_rejects_relative_and_control_character_paths() {
        assert_eq!(render_desktop(REGULAR, "app.AppImage", Path::new(ICON)), Err(MenuError::BadPath));
        assert_eq!(render_desktop(REGULAR, "/a\nExec=evil", Path::new(ICON)), Err(MenuError::BadPath));
    }

    #[test]
    fn rendering_rejects_unusable_icon_paths() {
        assert_eq!(render_desktop(REGULAR, APPIMAGE, Path::new("icon.png")), Err(MenuError::BadPath));
        assert_eq!(render_desktop(REGULAR, APPIMAGE, Path::new("/a\nExec=evil/icon.png")), Err(MenuError::BadPath));
        // Spaces stay unquoted, a backslash is doubled.
        assert_eq!(icon_value(Path::new("/a b/c.png")).as_deref(), Ok("/a b/c.png"));
        assert_eq!(icon_value(Path::new("/a\\b/c.png")).as_deref(), Ok("/a\\\\b/c.png"));
    }

    #[test]
    fn rendered_entry_has_the_required_keys() {
        let text = render_desktop(REGULAR, APPIMAGE, Path::new(ICON)).unwrap();
        for line in [
            "[Desktop Entry]",
            "Type=Application",
            "Name=3MF Katalog Manager",
            "Icon=/home/u/.local/share/icons/hicolor/256x256/apps/mf-katalog-manager.png",
            "Terminal=false",
            "StartupWMClass=mf-katalog-manager",
            "X-3MF-Katalog-Managed=true",
        ] {
            assert!(text.lines().any(|l| l == line), "missing {line}");
        }
        assert!(text.contains("Comment[de]="));
        // One main category only, or KDE shows the entry twice.
        assert!(text.contains("Categories=Graphics;3DGraphics;"));
        assert!(!text.contains("Utility"));
        assert!(is_managed(&text));
    }

    #[test]
    fn preview_variant_never_shares_file_icon_or_name() {
        let tmp = Tmp::new("preview");
        let regular = paths(REGULAR, &tmp.0);
        let preview = paths(PREVIEW, &tmp.0);
        assert_ne!(regular.desktop_file, preview.desktop_file);
        for ((a, _), (b, _)) in regular.icons.iter().zip(&preview.icons) {
            assert_ne!(a, b);
        }
        assert_ne!(REGULAR.display_name, PREVIEW.display_name);
        install(REGULAR, APPIMAGE, &tmp.0).unwrap();
        install(PREVIEW, "/home/u/Preview.AppImage", &tmp.0).unwrap();
        assert_eq!(exec_program(&fs::read_to_string(&regular.desktop_file).unwrap()).as_deref(), Some(APPIMAGE));
        remove(PREVIEW, &tmp.0).unwrap();
        assert!(regular.desktop_file.exists() && !preview.desktop_file.exists());
        assert!(regular.icons.iter().all(|(p, _)| p.exists()));
    }

    #[test]
    fn install_writes_entry_icons_and_permissions() {
        let tmp = Tmp::new("install");
        install(REGULAR, APPIMAGE, &tmp.0).unwrap();
        let p = paths(REGULAR, &tmp.0);
        assert!(is_managed(&fs::read_to_string(&p.desktop_file).unwrap()));
        for (path, bytes) in &p.icons {
            assert_eq!(&fs::read(path).unwrap(), bytes);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&p.desktop_file).unwrap().permissions().mode() & 0o777, 0o644);
        }
        // No temp files left behind.
        let leftovers: Vec<_> = fs::read_dir(&p.applications_dir).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.')).collect();
        assert!(leftovers.is_empty());
        // Installing again is fine.
        install(REGULAR, APPIMAGE, &tmp.0).unwrap();
    }

    #[test]
    fn foreign_file_is_never_overwritten_or_removed() {
        let tmp = Tmp::new("foreign");
        let p = paths(REGULAR, &tmp.0);
        fs::create_dir_all(&p.applications_dir).unwrap();
        let theirs = "[Desktop Entry]\nName=Mine\nExec=/bin/true\n";
        fs::write(&p.desktop_file, theirs).unwrap();
        assert_eq!(install(REGULAR, APPIMAGE, &tmp.0), Err(MenuError::ForeignFile));
        assert_eq!(remove(REGULAR, &tmp.0), Err(MenuError::ForeignFile));
        assert_eq!(sync_path(REGULAR, APPIMAGE, &tmp.0), Ok(false));
        assert_eq!(fs::read_to_string(&p.desktop_file).unwrap(), theirs);
        assert!(!p.icons.iter().any(|(i, _)| i.exists()));
    }

    #[cfg(unix)]
    #[test]
    fn dangling_foreign_symlink_is_foreign_and_left_untouched() {
        let tmp = Tmp::new("dangling");
        let p = paths(REGULAR, &tmp.0);
        fs::create_dir_all(&p.applications_dir).unwrap();
        let target = tmp.0.join("missing-target.desktop");
        std::os::unix::fs::symlink(&target, &p.desktop_file).unwrap();
        assert_eq!(entry_state(&p.desktop_file, APPIMAGE, &icon_value(&p.launcher_icon).unwrap()), Ok(EntryState::Foreign));
        assert_eq!(install(REGULAR, APPIMAGE, &tmp.0), Err(MenuError::ForeignFile));
        assert_eq!(remove(REGULAR, &tmp.0), Err(MenuError::ForeignFile));
        assert_eq!(sync_path(REGULAR, APPIMAGE, &tmp.0), Ok(false));
        assert_eq!(fs::read_link(&p.desktop_file).unwrap(), target);
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_an_existing_foreign_file_is_foreign() {
        let tmp = Tmp::new("symlink-foreign");
        let p = paths(REGULAR, &tmp.0);
        fs::create_dir_all(&p.applications_dir).unwrap();
        let target = tmp.0.join("theirs.desktop");
        fs::write(&target, "[Desktop Entry]\nName=Mine\nExec=/bin/true\n").unwrap();
        std::os::unix::fs::symlink(&target, &p.desktop_file).unwrap();
        assert_eq!(entry_state(&p.desktop_file, APPIMAGE, &icon_value(&p.launcher_icon).unwrap()), Ok(EntryState::Foreign));
    }

    #[cfg(unix)]
    #[test]
    fn regular_file_with_our_marker_is_managed() {
        let tmp = Tmp::new("managed-regular");
        install(REGULAR, APPIMAGE, &tmp.0).unwrap();
        let p = paths(REGULAR, &tmp.0);
        assert_eq!(entry_state(&p.desktop_file, APPIMAGE, &icon_value(&p.launcher_icon).unwrap()), Ok(EntryState::Managed { current: true }));
    }

    #[test]
    fn third_party_integration_blocks_install_and_is_detected() {
        let tmp = Tmp::new("third");
        let apps = tmp.0.join("applications");
        fs::create_dir_all(&apps).unwrap();
        assert!(!third_party_entry_exists(REGULAR, &apps, APPIMAGE));
        fs::write(
            apps.join("appimagekit_abc123-3MF_Katalog_Manager.desktop"),
            format!("[Desktop Entry]\nName=3MF\nExec={} %U\nX-AppImage-Integrate=false\n", quote_exec_arg(APPIMAGE)),
        ).unwrap();
        // Different AppImage and unrelated entries do not count.
        fs::write(apps.join("other.desktop"), "[Desktop Entry]\nExec=/x/other.AppImage\n").unwrap();
        assert!(third_party_entry_exists(REGULAR, &apps, APPIMAGE));
        assert!(!third_party_entry_exists(REGULAR, &apps, "/x/unrelated.AppImage"));
        assert_eq!(install(REGULAR, APPIMAGE, &tmp.0), Err(MenuError::ThirdParty));
        assert!(!paths(REGULAR, &tmp.0).desktop_file.exists());
    }

    #[test]
    fn our_own_entry_does_not_count_as_third_party() {
        let tmp = Tmp::new("own");
        install(REGULAR, APPIMAGE, &tmp.0).unwrap();
        assert!(!third_party_entry_exists(REGULAR, &paths(REGULAR, &tmp.0).applications_dir, APPIMAGE));
    }

    #[test]
    fn moved_appimage_updates_the_exec_path_silently() {
        let tmp = Tmp::new("sync");
        let p = paths(REGULAR, &tmp.0);
        assert_eq!(sync_path(REGULAR, APPIMAGE, &tmp.0), Ok(false)); // nothing consented
        assert!(!p.desktop_file.exists());
        install(REGULAR, APPIMAGE, &tmp.0).unwrap();
        assert_eq!(sync_path(REGULAR, APPIMAGE, &tmp.0), Ok(false));
        let moved = "/opt/new place/app.AppImage";
        assert_eq!(sync_path(REGULAR, moved, &tmp.0), Ok(true));
        let content = fs::read_to_string(&p.desktop_file).unwrap();
        assert_eq!(exec_program(&content).as_deref(), Some(moved));
        assert!(is_managed(&content));
        assert_eq!(sync_path(REGULAR, moved, &tmp.0), Ok(false));
    }

    #[test]
    fn launcher_points_at_the_256_px_icon_file() {
        let tmp = Tmp::new("icon-path");
        for variant in [REGULAR, PREVIEW] {
            install(variant, APPIMAGE, &tmp.0).unwrap();
            let p = paths(variant, &tmp.0);
            let content = fs::read_to_string(&p.desktop_file).unwrap();
            let expected = tmp.0.join(format!("icons/hicolor/256x256/apps/{}.png", variant.stem));
            assert_eq!(icon_line(&content), Some(expected.to_str().unwrap()));
            assert_eq!(p.launcher_icon, expected);
            assert!(expected.is_file());
            assert!(p.icons.iter().any(|(i, _)| *i == expected));
            assert_eq!(p.icons.len(), 3);
        }
    }

    #[test]
    fn data_folder_with_spaces_works() {
        let tmp = Tmp::new("spaces");
        let home = tmp.0.join("my data");
        install(REGULAR, APPIMAGE, &home).unwrap();
        let p = paths(REGULAR, &home);
        let content = fs::read_to_string(&p.desktop_file).unwrap();
        assert_eq!(icon_line(&content), Some(p.launcher_icon.to_str().unwrap()));
        assert!(icon_line(&content).unwrap().contains("my data"));
        assert_eq!(sync_path(REGULAR, APPIMAGE, &home), Ok(false));
    }

    #[test]
    fn old_entry_with_an_icon_name_is_rewritten_once() {
        let tmp = Tmp::new("old-entry");
        let p = paths(REGULAR, &tmp.0);
        fs::create_dir_all(&p.applications_dir).unwrap();
        let old = render_desktop(REGULAR, APPIMAGE, &p.launcher_icon).unwrap()
            .replace(&format!("Icon={}", p.launcher_icon.to_str().unwrap()), "Icon=mf-katalog-manager");
        assert!(old.contains("Icon=mf-katalog-manager\n"));
        fs::write(&p.desktop_file, &old).unwrap();
        // The icons are missing as well and get written again.
        assert_eq!(sync_path(REGULAR, APPIMAGE, &tmp.0), Ok(true));
        let content = fs::read_to_string(&p.desktop_file).unwrap();
        assert_eq!(icon_line(&content), Some(p.launcher_icon.to_str().unwrap()));
        assert!(is_managed(&content));
        assert!(p.icons.iter().all(|(i, _)| i.is_file()));
        assert_eq!(sync_path(REGULAR, APPIMAGE, &tmp.0), Ok(false));
    }

    #[test]
    fn foreign_entry_keeps_its_icon_on_sync() {
        let tmp = Tmp::new("foreign-icon");
        let p = paths(REGULAR, &tmp.0);
        fs::create_dir_all(&p.applications_dir).unwrap();
        let theirs = format!("[Desktop Entry]\nName=Mine\nExec={}\nIcon=mine\n", quote_exec_arg(APPIMAGE));
        fs::write(&p.desktop_file, &theirs).unwrap();
        assert_eq!(sync_path(REGULAR, APPIMAGE, &tmp.0), Ok(false));
        assert_eq!(fs::read_to_string(&p.desktop_file).unwrap(), theirs);
        assert!(!p.launcher_icon.exists());
    }

    #[test]
    fn remove_deletes_only_our_files() {
        let tmp = Tmp::new("remove");
        install(REGULAR, APPIMAGE, &tmp.0).unwrap();
        let p = paths(REGULAR, &tmp.0);
        let bystander = p.applications_dir.join("other.desktop");
        fs::write(&bystander, "[Desktop Entry]\nName=Other\n").unwrap();
        let other_icon = p.hicolor_dir.join("256x256/apps/other.png");
        fs::write(&other_icon, b"x").unwrap();
        assert_eq!(remove(REGULAR, &tmp.0), Ok(true));
        assert!(!p.desktop_file.exists());
        assert!(p.icons.iter().all(|(i, _)| !i.exists()));
        assert!(bystander.exists() && other_icon.exists());
        assert_eq!(remove(REGULAR, &tmp.0), Ok(false)); // idempotent
    }

    #[test]
    fn unwritable_target_is_an_error_not_a_panic() {
        let tmp = Tmp::new("ro");
        // A plain file where the applications directory should be.
        fs::write(tmp.0.join("applications"), b"x").unwrap();
        assert!(matches!(install(REGULAR, APPIMAGE, &tmp.0), Err(MenuError::Io(_))));
        assert!(matches!(remove(REGULAR, &tmp.0), Err(MenuError::Io(_))));
    }

    #[test]
    fn decisions_round_trip() {
        for d in [Decision::Enabled, Decision::Never, Decision::NotNow(1234)] {
            assert_eq!(Decision::parse(Some(&d.serialize())), Some(d));
        }
        assert_eq!(Decision::parse(None), None);
        assert_eq!(Decision::parse(Some("garbage")), None);
        assert_eq!(Decision::parse(Some("not_now:x")), None);
    }

    fn input() -> PromptInput {
        PromptInput {
            applicable: true,
            decision: None,
            own_entry_exists: false,
            foreign_file_in_the_way: false,
            third_party_entry: false,
            now_secs: 100 * 24 * 3600,
        }
    }

    #[test]
    fn prompt_is_shown_once_on_first_run() {
        assert!(should_prompt(&input()));
        for decision in [Decision::Never, Decision::Enabled] {
            assert!(!should_prompt(&PromptInput { decision: Some(decision), ..input() }));
        }
    }

    #[test]
    fn prompt_is_suppressed_when_not_applicable_or_integrated() {
        assert!(!should_prompt(&PromptInput { applicable: false, ..input() }));
        assert!(!should_prompt(&PromptInput { third_party_entry: true, ..input() }));
        assert!(!should_prompt(&PromptInput { own_entry_exists: true, ..input() }));
        assert!(!should_prompt(&PromptInput { foreign_file_in_the_way: true, ..input() }));
    }

    #[test]
    fn not_now_is_asked_again_only_after_a_while() {
        let now = 100 * 24 * 3600;
        assert!(!should_prompt(&PromptInput { decision: Some(Decision::NotNow(now - 1000)), ..input() }));
        assert!(should_prompt(&PromptInput { decision: Some(Decision::NotNow(now - NOT_NOW_SECS)), ..input() }));
        // A clock set back never panics.
        assert!(!should_prompt(&PromptInput { decision: Some(Decision::NotNow(now + 5)), ..input() }));
    }

    #[test]
    fn touching_the_icon_folder_moves_its_modification_time() {
        let tmp = Tmp::new("touch");
        let dir = tmp.0.join("hicolor");
        fs::create_dir_all(&dir).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::open(&dir).unwrap().set_modified(old).unwrap();
        touch_icon_theme_dir(&dir);
        assert!(fs::metadata(&dir).unwrap().modified().unwrap() > old + std::time::Duration::from_secs(3000));
        // A missing folder is not an error.
        touch_icon_theme_dir(&tmp.0.join("missing"));
    }

    /// End-to-end on a scratch data directory; also checks the result with
    /// `desktop-file-validate` when that tool is installed.
    #[test]
    fn full_lifecycle_in_a_scratch_directory() {
        let tmp = Tmp::new("lifecycle");
        let home = data_home(None, Some(tmp.0.to_str().unwrap())).unwrap();
        let p = paths(REGULAR, &home);
        install(REGULAR, APPIMAGE, &home).unwrap();
        let validate = |expect_ok: bool| {
            match std::process::Command::new("desktop-file-validate").arg(&p.desktop_file).output() {
                Ok(out) => {
                    let report = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                    assert_eq!(out.status.success(), expect_ok, "{report}");
                    // Hints (for example about several main categories) count as findings too.
                    assert!(!expect_ok || report.is_empty(), "{report}");
                }
                Err(_) => eprintln!("desktop-file-validate not installed; skipped"),
            }
        };
        validate(true);
        assert_eq!(sync_path(REGULAR, "/moved/it.AppImage", &home), Ok(true));
        validate(true);
        assert_eq!(remove(REGULAR, &home), Ok(true));
        assert!(!p.desktop_file.exists());
    }
}
