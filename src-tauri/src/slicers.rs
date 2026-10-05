// Looks for known 3D printing slicers (Bambu Studio, OrcaSlicer, PrusaSlicer,
// SuperSlicer, UltiMaker Cura, Creality Print) in the operating system's typical install
// locations. Read-only, best effort: an unreadable or missing folder is skipped,
// never propagated - detect_slicers() therefore always returns a Vec (empty if
// nothing was found), never a Result.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedSlicer {
    pub name: String,
    pub path: String,
}

struct SlicerDefinition {
    display_name: &'static str,
    binary_names: &'static [&'static str],
}

const SLICER_DEFINITIONS: &[SlicerDefinition] = &[
    SlicerDefinition {
        display_name: "Creality Print",
        binary_names: &["CrealityPrint", "creality-print"],
    },
    SlicerDefinition {
        display_name: "Bambu Studio",
        binary_names: &["bambu-studio", "BambuStudio", "bambustudio"],
    },
    SlicerDefinition {
        display_name: "OrcaSlicer",
        binary_names: &["orca-slicer", "OrcaSlicer"],
    },
    SlicerDefinition {
        display_name: "PrusaSlicer",
        binary_names: &["prusa-slicer", "prusaslicer"],
    },
    SlicerDefinition {
        display_name: "SuperSlicer",
        binary_names: &["superslicer"],
    },
    SlicerDefinition {
        display_name: "UltiMaker Cura",
        binary_names: &["cura", "UltiMaker-Cura"],
    },
];

fn search_path_env(binary_names: &[&str]) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in binary_names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                #[cfg(target_os = "linux")]
                if binary_names.contains(&"CrealityPrint") && !executable_file(&candidate) {
                    continue;
                }
                return Some(candidate.to_string_lossy().to_string());
            }
            #[cfg(target_os = "windows")]
            {
                let candidate_exe = dir.join(format!("{name}.exe"));
                if candidate_exe.is_file() {
                    return Some(candidate_exe.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn check_fixed_dir(dir: &Path, binary_names: &[&str]) -> Option<PathBuf> {
    for name in binary_names {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn check_opt_dirs(binary_names: &[&str]) -> Option<PathBuf> {
    for subfolder in binary_names {
        let dir = PathBuf::from("/opt").join(subfolder);
        if let Some(found) = check_fixed_dir(&dir, binary_names) {
            return Some(found);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn linux_fixed_search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/var/lib/flatpak/exports/bin"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/share/flatpak/exports/bin"));
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join("Applications"));
        dirs.push(home.join("AppImages"));
    }
    dirs
}

#[cfg(target_os = "linux")]
fn detect_linux_fixed_locations() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();
    let fixed_dirs = linux_fixed_search_dirs();
    for def in SLICER_DEFINITIONS {
        for dir in &fixed_dirs {
            if let Some(found) = check_fixed_dir(dir, def.binary_names) {
                if def.display_name == "Creality Print" && !executable_file(&found) {
                    continue;
                }
                results.push(DetectedSlicer {
                    name: def.display_name.to_string(),
                    path: found.to_string_lossy().to_string(),
                });
            }
        }
        if let Some(found) = check_opt_dirs(def.binary_names) {
            if def.display_name == "Creality Print" && !executable_file(&found) {
                continue;
            }
            results.push(DetectedSlicer {
                name: def.display_name.to_string(),
                path: found.to_string_lossy().to_string(),
            });
        }
    }
    results
}

#[cfg(target_os = "windows")]
const WINDOWS_FIXED_LOCATIONS: &[(&str, &[&str])] = &[
    ("Bambu Studio", &["Bambu Studio\\bambu-studio.exe"]),
    ("OrcaSlicer", &["OrcaSlicer\\orca-slicer.exe"]),
    (
        "PrusaSlicer",
        &[
            "Prusa3D\\PrusaSlicer\\prusa-slicer-console.exe",
            "Prusa3D\\PrusaSlicer\\prusa-slicer.exe",
        ],
    ),
    ("SuperSlicer", &["SuperSlicer\\superslicer.exe"]),
];

#[cfg(target_os = "windows")]
fn windows_program_files_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        dirs.push(PathBuf::from(pf));
    }
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
        dirs.push(PathBuf::from(pf86));
    }
    dirs
}

#[cfg(target_os = "windows")]
fn detect_windows_fixed_locations() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();
    for (display_name, relative_paths) in WINDOWS_FIXED_LOCATIONS {
        for pf in &windows_program_files_dirs() {
            for rel in *relative_paths {
                let candidate = pf.join(rel);
                if candidate.is_file() {
                    results.push(DetectedSlicer {
                        name: display_name.to_string(),
                        path: candidate.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
    results
}

#[cfg(target_os = "windows")]
fn detect_windows_cura() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();
    for pf in windows_program_files_dirs() {
        let Ok(entries) = std::fs::read_dir(&pf) else {
            continue;
        };
        for entry in entries.flatten() {
            let folder_name = entry.file_name();
            let folder_name = folder_name.to_string_lossy();
            if !is_cura_folder_name(&folder_name) {
                continue;
            }
            let dir = entry.path();
            for exe in ["UltiMaker-Cura.exe", "Cura.exe"] {
                let candidate = dir.join(exe);
                if candidate.is_file() {
                    results.push(DetectedSlicer {
                        name: "UltiMaker Cura".to_string(),
                        path: candidate.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
    results
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_cura_folder_name(name: &str) -> bool {
    name.starts_with("Ultimaker Cura") || name.starts_with("UltiMaker Cura")
}

// macOS apps are bundles (`Name.app/Contents/MacOS/binary`) with partly
// different spelling, hence a separate mapping (names from the official macOS
// downloads).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const MACOS_FIXED_LOCATIONS: &[(&str, &str, &str)] = &[
    ("Creality Print", "CrealityPrint.app", "CrealityPrint"),
    ("Bambu Studio", "BambuStudio.app", "BambuStudio"),
    ("OrcaSlicer", "OrcaSlicer.app", "OrcaSlicer"),
    ("PrusaSlicer", "PrusaSlicer.app", "PrusaSlicer"),
    ("SuperSlicer", "SuperSlicer.app", "SuperSlicer"),
    ("UltiMaker Cura", "Ultimaker Cura.app", "UltiMaker-Cura"),
];

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_slicer_candidate_path(apps_dir: &Path, app_bundle: &str, binary_name: &str) -> PathBuf {
    apps_dir
        .join(app_bundle)
        .join("Contents")
        .join("MacOS")
        .join(binary_name)
}

#[cfg(target_os = "macos")]
fn macos_applications_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Applications"));
    }
    dirs
}

#[cfg(target_os = "macos")]
fn detect_macos_fixed_locations() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();
    for (display_name, app_bundle, binary_name) in MACOS_FIXED_LOCATIONS {
        for dir in &macos_applications_dirs() {
            let candidate = macos_slicer_candidate_path(dir, app_bundle, binary_name);
            if candidate.is_file() {
                results.push(DetectedSlicer {
                    name: display_name.to_string(),
                    path: candidate.to_string_lossy().to_string(),
                });
            }
        }
    }
    results
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CrealityVersion {
    numbers: Vec<u32>,
    stable: bool,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn creality_version(name: &str) -> Option<CrealityVersion> {
    let version = name.strip_prefix("Creality Print")?.trim();
    let end = version
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(version.len());
    let mut numbers: Vec<u32> = version[..end]
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    while numbers.last() == Some(&0) {
        numbers.pop();
    }
    Some(CrealityVersion {
        numbers,
        stable: version[end..].trim().is_empty(),
    })
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn ordered_creality_candidates(
    mut items: Vec<(CrealityVersion, bool, PathBuf)>,
) -> Vec<DetectedSlicer> {
    items.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    dedupe_by_path(
        items
            .into_iter()
            .map(|(_, _, path)| DetectedSlicer {
                name: "Creality Print".into(),
                path: path.to_string_lossy().into_owned(),
            })
            .collect(),
    )
}

#[cfg(target_os = "windows")]
fn detect_windows_creality() -> Vec<DetectedSlicer> {
    use std::os::windows::process::CommandExt;
    let mut candidates = Vec::new();
    let mut roots = windows_program_files_dirs();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs"));
    }
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root.join("Creality")) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(version) = creality_version(&entry.file_name().to_string_lossy()) else {
                continue;
            };
            let exe = entry.path().join("CrealityPrint.exe");
            if exe.is_file() {
                candidates.push((version, false, exe));
            }
        }
    }
    // Query both registry views without a console or a new registry dependency.
    for view in ["/reg:64", "/reg:32"] {
        for root in [
            r"HKLM\Software\Creality",
            r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            let Ok(output) = std::process::Command::new("reg.exe")
                .args(["query", root, "/s", view])
                .creation_flags(0x08000000)
                .output()
            else {
                continue;
            };
            candidates.extend(creality_registry_candidates(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
    }
    ordered_creality_candidates(candidates)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn creality_registry_candidates(output: &str) -> Vec<(CrealityVersion, bool, PathBuf)> {
    let mut candidates = Vec::new();
    for block in output.split("HKEY_LOCAL_MACHINE\\").skip(1) {
        let mut lines = block.lines();
        let Some(key) = lines.next() else {
            continue;
        };
        let Some(major) = key
            .trim()
            .rsplit('\\')
            .next()
            .and_then(|key| key.strip_prefix("CrealityPrint-"))
        else {
            continue;
        };
        let mut location = None;
        let mut display_version = None;
        for line in lines {
            let Some((name, value)) = line.split_once("REG_SZ") else {
                continue;
            };
            let name = name.trim();
            if name == "InstallLocation" || name.starts_with('(') {
                location = Some(value.trim());
            }
            if name == "DisplayVersion" {
                display_version = Some(value.trim());
            }
        }
        let Some(location) = location else {
            continue;
        };
        let path = PathBuf::from(location);
        let folder_version = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(creality_version);
        let mut version = display_version
            .and_then(|v| creality_version(&format!("Creality Print {v}")))
            .or(folder_version)
            .or_else(|| creality_version(&format!("Creality Print {major}")));
        if let (Some(version), Some(folder)) = (
            &mut version,
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(creality_version),
        ) {
            version.stable &= folder.stable;
        }
        let exe = path.join("CrealityPrint.exe");
        if let Some(version) = version {
            if exe.is_file() {
                candidates.push((version, true, exe));
            }
        }
    }
    candidates
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn creality_appimage_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("crealityprint") && name.ends_with(".appimage")
}

#[cfg(target_os = "linux")]
fn executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn creality_flatpak_paths(home: Option<&Path>) -> Vec<PathBuf> {
    let id = "io.github.crealityofficial.CrealityPrint";
    let mut paths = vec![Path::new("/var/lib/flatpak/exports/bin").join(id)];
    if let Some(home) = home {
        paths.push(home.join(".local/share/flatpak/exports/bin").join(id));
    }
    paths
}

#[cfg(target_os = "linux")]
fn detect_linux_creality() -> Vec<DetectedSlicer> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut paths = creality_flatpak_paths(home.as_deref());
    let mut dirs = vec![PathBuf::from("/opt")];
    if let Some(home) = home {
        dirs.extend(
            [
                "AppImages",
                "Applications",
                "Downloads",
                "Programme",
                "creality",
            ]
            .map(|dir| home.join(dir)),
        );
    }
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        paths.extend(
            entries
                .flatten()
                .filter(|e| creality_appimage_name(&e.file_name().to_string_lossy()))
                .map(|e| e.path()),
        );
    }
    paths
        .into_iter()
        .filter(|p| executable_file(p))
        .map(|path| DetectedSlicer {
            name: "Creality Print".into(),
            path: path.to_string_lossy().into_owned(),
        })
        .collect()
}

fn dedupe_by_path(items: Vec<DetectedSlicer>) -> Vec<DetectedSlicer> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for item in items {
        if seen.insert(item.path.clone()) {
            result.push(item);
        }
    }
    result
}

fn dedupe_by_name(items: Vec<DetectedSlicer>) -> Vec<DetectedSlicer> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for item in items {
        if seen.insert(item.name.clone()) {
            result.push(item);
        }
    }
    result
}

pub fn detect_slicers() -> Vec<DetectedSlicer> {
    let mut results = Vec::new();

    // Versioned installations take precedence over an unversioned PATH fallback.
    #[cfg(target_os = "windows")]
    results.extend(detect_windows_creality());

    for def in SLICER_DEFINITIONS {
        if let Some(path) = search_path_env(def.binary_names) {
            results.push(DetectedSlicer {
                name: def.display_name.to_string(),
                path,
            });
        }
    }

    #[cfg(target_os = "linux")]
    {
        results.extend(detect_linux_fixed_locations());
        results.extend(detect_linux_creality());
    }

    #[cfg(target_os = "windows")]
    {
        results.extend(detect_windows_fixed_locations());
        results.extend(detect_windows_cura());
    }

    #[cfg(target_os = "macos")]
    results.extend(detect_macos_fixed_locations());

    dedupe_by_name(dedupe_by_path(results))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creality_versions_are_numeric_and_stable_precedes_beta() {
        let version = |s| creality_version(s).unwrap();
        assert!(version("Creality Print 7.10") > version("Creality Print 7.9"));
        assert!(version("Creality Print 7.3") > version("Creality Print 7.3 Beta"));
        assert!(version("Creality Print 7.3 Beta") > version("Creality Print 7.2"));
        assert_eq!(
            version("Creality Print 7.2.0"),
            version("Creality Print 7.2")
        );
        assert!(creality_version("Other 7.2").is_none());
        assert!(creality_version("Creality Print notes").is_none());
    }

    #[test]
    fn creality_selection_prefers_latest_then_registry_and_deduplicates() {
        let v = |s| creality_version(s).unwrap();
        let items = vec![
            (v("Creality Print 7.9"), true, PathBuf::from("old")),
            (v("Creality Print 7.10 Beta"), true, PathBuf::from("beta")),
            (v("Creality Print 7.10"), false, PathBuf::from("folder")),
            (v("Creality Print 7.10"), true, PathBuf::from("registry")),
            (v("Creality Print 7.10"), false, PathBuf::from("registry")),
        ];
        let sorted = ordered_creality_candidates(items);
        assert_eq!(
            sorted.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
            ["registry", "folder", "beta", "old"]
        );
        assert_eq!(dedupe_by_name(sorted)[0].path, "registry");
    }

    #[test]
    fn creality_registry_reads_custom_locations_and_ignores_unrelated_keys() {
        let dir = std::env::temp_dir().join(format!("p23-registry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("CrealityPrint.exe"), b"fake").unwrap();
        let output = format!("HKEY_LOCAL_MACHINE\\Software\\Creality\\CrealityPrint-7\n    (Default)    REG_SZ    {}\n\nHKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CrealityPrint-7\n    InstallLocation    REG_SZ    {}\n    DisplayVersion    REG_SZ    7.10\n\nHKEY_LOCAL_MACHINE\\Software\\Other\n    InstallLocation    REG_SZ    {}\n", dir.display(), dir.display(), dir.display());
        let candidates = creality_registry_candidates(&output);
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[1].0,
            creality_version("Creality Print 7.10").unwrap()
        );
        assert!(candidates
            .iter()
            .all(|c| c.1 && c.2 == dir.join("CrealityPrint.exe")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn creality_platform_paths_and_appimage_patterns() {
        assert!(creality_appimage_name(
            "CrealityPrint-V7.2.1-x86_64-Release.AppImage"
        ));
        assert!(creality_appimage_name("crealityprint-v7.APPIMAGE"));
        assert!(!creality_appimage_name("CrealityPrint-notes.txt"));
        assert!(!creality_appimage_name("OrcaSlicer.AppImage"));
        assert_eq!(creality_flatpak_paths(Some(Path::new("/home/test"))), vec![
            PathBuf::from("/var/lib/flatpak/exports/bin/io.github.crealityofficial.CrealityPrint"),
            PathBuf::from("/home/test/.local/share/flatpak/exports/bin/io.github.crealityofficial.CrealityPrint"),
        ]);
        assert_eq!(creality_flatpak_paths(None).len(), 1);
        assert!(MACOS_FIXED_LOCATIONS.contains(&(
            "Creality Print",
            "CrealityPrint.app",
            "CrealityPrint"
        )));
        for root in ["/Applications", "/home/test/Applications"] {
            assert_eq!(
                macos_slicer_candidate_path(Path::new(root), "CrealityPrint.app", "CrealityPrint"),
                Path::new(root).join("CrealityPrint.app/Contents/MacOS/CrealityPrint")
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn creality_requires_an_executable_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("p23-creality-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("CrealityPrint.AppImage");
        std::fs::write(&path, b"fake").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!executable_file(&path));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(executable_file(&path));
        assert!(!executable_file(&dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dedupe_by_path_removes_duplicates_keeping_first() {
        let items = vec![
            DetectedSlicer {
                name: "Bambu Studio".to_string(),
                path: "/usr/bin/bambu-studio".to_string(),
            },
            DetectedSlicer {
                name: "Bambu Studio (opt)".to_string(),
                path: "/usr/bin/bambu-studio".to_string(),
            },
            DetectedSlicer {
                name: "OrcaSlicer".to_string(),
                path: "/usr/bin/orca-slicer".to_string(),
            },
        ];

        let result = dedupe_by_path(items);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "/usr/bin/bambu-studio");
        assert_eq!(result[0].name, "Bambu Studio");
        assert_eq!(result[1].path, "/usr/bin/orca-slicer");
    }

    #[test]
    fn dedupe_by_path_keeps_all_when_paths_differ() {
        let items = vec![
            DetectedSlicer {
                name: "Bambu Studio".to_string(),
                path: "/usr/bin/bambu-studio".to_string(),
            },
            DetectedSlicer {
                name: "OrcaSlicer".to_string(),
                path: "/usr/bin/orca-slicer".to_string(),
            },
        ];

        let result = dedupe_by_path(items);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn is_cura_folder_name_matches_expected_prefixes() {
        assert!(is_cura_folder_name("Ultimaker Cura 5.7"));
        assert!(is_cura_folder_name("UltiMaker Cura 5.8.0"));
        assert!(is_cura_folder_name("Ultimaker Cura"));
    }

    #[test]
    fn is_cura_folder_name_rejects_unrelated_names() {
        assert!(!is_cura_folder_name("Bambu Studio"));
        assert!(!is_cura_folder_name("Cura"));
        assert!(!is_cura_folder_name("Some Other App"));
    }

    #[test]
    fn dedupe_by_name_keeps_first_occurrence_per_display_name() {
        let items = vec![
            DetectedSlicer {
                name: "OrcaSlicer".to_string(),
                path: "/usr/bin/orca-slicer".to_string(),
            },
            DetectedSlicer {
                name: "OrcaSlicer".to_string(),
                path: "/home/user/Applications/OrcaSlicer.AppImage".to_string(),
            },
            DetectedSlicer {
                name: "Bambu Studio".to_string(),
                path: "/usr/bin/bambu-studio".to_string(),
            },
        ];

        let result = dedupe_by_name(items);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "OrcaSlicer");
        assert_eq!(result[0].path, "/usr/bin/orca-slicer");
        assert_eq!(result[1].name, "Bambu Studio");
    }

    #[test]
    fn macos_slicer_candidate_path_builds_the_contents_macos_layout() {
        let candidate = macos_slicer_candidate_path(
            Path::new("/Applications"),
            "BambuStudio.app",
            "BambuStudio",
        );
        assert_eq!(
            candidate,
            Path::new("/Applications/BambuStudio.app/Contents/MacOS/BambuStudio")
        );
    }

    #[test]
    fn detect_slicers_returns_without_panicking() {
        // Best effort on the real system the tests run on: may be empty, but must
        // always return instead of panicking, whatever slicers are installed locally.
        let _ = detect_slicers();
    }
}
