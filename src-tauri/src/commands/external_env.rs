use std::process::Command;

/// Environment edits for a foreign Linux process. AppImage runtime paths must
/// not override its Python, loader or desktop resources; host paths stay intact.
pub(crate) fn sanitized_env(
    vars: &[(String, String)],
    appdir: Option<&str>,
) -> Vec<(String, Option<String>)> {
    let Some(appdir) = appdir.filter(|dir| !dir.is_empty()) else {
        return Vec::new();
    };
    let mounted = std::path::Path::new(appdir);
    vars.iter()
        .filter_map(|(name, value)| {
            let relevant = matches!(
                name.as_str(),
                "PYTHONHOME"
                    | "PYTHONPATH"
                    | "LD_LIBRARY_PATH"
                    | "LD_PRELOAD"
                    | "GSETTINGS_SCHEMA_DIR"
                    | "XDG_DATA_DIRS"
                    | "PATH"
                    | "QT_PLUGIN_PATH"
                    | "GST_PLUGIN_SYSTEM_PATH"
            ) || name.starts_with("GIO_")
                || name.starts_with("GDK_PIXBUF_")
                || name.starts_with("GTK_");
            if !relevant {
                return None;
            }
            let preload = name == "LD_PRELOAD";
            let entries: Vec<&str> = if preload {
                value
                    .split(|c: char| c == ':' || c.is_ascii_whitespace())
                    .filter(|entry| !entry.is_empty())
                    .collect()
            } else {
                value.split(':').collect()
            };
            let retained: Vec<&str> = entries
                .iter()
                .copied()
                .filter(|entry| !std::path::Path::new(entry).starts_with(mounted))
                .collect();
            if entries.len() == retained.len() {
                return None;
            }
            let replacement = if retained.is_empty() {
                None
            } else {
                Some(retained.join(if preload { " " } else { ":" }))
            };
            Some((name.clone(), replacement))
        })
        .collect()
}

pub(crate) fn sanitize_external_command(command: &mut Command) {
    #[cfg(target_os = "linux")]
    {
        let vars: Vec<(String, String)> = std::env::vars_os()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect();
        let appdir = std::env::var("APPDIR").ok();
        for (name, value) in sanitized_env(&vars, appdir.as_deref()) {
            match value {
                Some(value) => {
                    command.env(name, value);
                }
                None => {
                    command.env_remove(name);
                }
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn without_appdir_the_environment_is_untouched() {
        let input = vars(&[
            ("PYTHONHOME", "/tmp/.mount_x/usr"),
            ("PATH", "/tmp/.mount_x/usr/bin:/usr/bin"),
        ]);
        assert!(sanitized_env(&input, None).is_empty());
        assert!(sanitized_env(&input, Some("")).is_empty());
    }

    #[test]
    fn removes_appimage_python_and_desktop_resources() {
        let input = vars(&[
            ("PYTHONHOME", "/tmp/.mount_x/usr"),
            ("PYTHONPATH", "/tmp/.mount_x/usr/lib/python"),
            ("GIO_MODULE_DIR", "/tmp/.mount_x/usr/lib/gio"),
            ("GDK_PIXBUF_MODULE_FILE", "/tmp/.mount_x/loaders.cache"),
            ("GSETTINGS_SCHEMA_DIR", "/tmp/.mount_x/schemas"),
            ("GTK_PATH", "/tmp/.mount_x/gtk"),
        ]);
        assert_eq!(
            sanitized_env(&input, Some("/tmp/.mount_x")),
            input
                .iter()
                .map(|(name, _)| (name.clone(), None))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn mixed_lists_keep_host_entries_and_similar_prefixes() {
        let input = vars(&[
            ("PATH", "/tmp/.mount_x/usr/bin:/usr/bin:/tmp/.mount_xyz/bin"),
            ("LD_LIBRARY_PATH", "/usr/lib:/tmp/.mount_x/usr/lib"),
            ("XDG_DATA_DIRS", "/tmp/.mount_x/usr/share:/usr/share"),
            ("PYTHONPATH", "/opt/python:/tmp/.mount_x/python"),
            (
                "LD_PRELOAD",
                "/tmp/.mount_x/lib.so /opt/host.so:/tmp/.mount_x/other.so",
            ),
        ]);
        assert_eq!(
            sanitized_env(&input, Some("/tmp/.mount_x")),
            vec![
                ("PATH".into(), Some("/usr/bin:/tmp/.mount_xyz/bin".into())),
                ("LD_LIBRARY_PATH".into(), Some("/usr/lib".into())),
                ("XDG_DATA_DIRS".into(), Some("/usr/share".into())),
                ("PYTHONPATH".into(), Some("/opt/python".into())),
                ("LD_PRELOAD".into(), Some("/opt/host.so".into())),
            ]
        );
    }

    #[test]
    fn foreign_appimage_paths_survive_own_appdir_cleanup() {
        let input = vars(&[
            ("LD_LIBRARY_PATH", "/tmp/own/usr/lib:/tmp/creality/usr/lib"),
            ("QT_PLUGIN_PATH", "/tmp/creality/plugins"),
        ]);
        assert_eq!(
            sanitized_env(&input, Some("/tmp/own")),
            vec![(
                "LD_LIBRARY_PATH".into(),
                Some("/tmp/creality/usr/lib".into())
            )]
        );
    }

    #[test]
    fn unrelated_and_host_settings_remain_untouched() {
        let input = vars(&[
            ("PYTHONHOME", "/opt/python"),
            ("GTK_THEME", "Adwaita"),
            ("GDK_BACKEND", "wayland"),
            ("UNRELATED", "/tmp/.mount_x/data"),
            ("PATH", ":/usr/bin:"),
        ]);
        assert!(sanitized_env(&input, Some("/tmp/.mount_x")).is_empty());
    }
}
