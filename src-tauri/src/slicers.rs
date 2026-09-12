// Sucht bekannte 3D-Drucker-Slicer (Bambu Studio, OrcaSlicer, PrusaSlicer,
// SuperSlicer, UltiMaker Cura) an typischen Installationsorten des
// Betriebssystems. Rein lesend, best-effort: ein nicht lesbarer oder nicht
// existierender Ordner wird uebersprungen, nie propagiert - detect_slicers()
// gibt daher immer ein Vec zurueck (leer, wenn nichts gefunden wurde), nie
// ein Result.

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
        display_name: "Bambu Studio",
        binary_names: &["bambu-studio", "BambuStudio"],
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
                results.push(DetectedSlicer {
                    name: def.display_name.to_string(),
                    path: found.to_string_lossy().to_string(),
                });
            }
        }
        if let Some(found) = check_opt_dirs(def.binary_names) {
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

    for def in SLICER_DEFINITIONS {
        if let Some(path) = search_path_env(def.binary_names) {
            results.push(DetectedSlicer {
                name: def.display_name.to_string(),
                path,
            });
        }
    }

    #[cfg(target_os = "linux")]
    results.extend(detect_linux_fixed_locations());

    #[cfg(target_os = "windows")]
    {
        results.extend(detect_windows_fixed_locations());
        results.extend(detect_windows_cura());
    }

    dedupe_by_name(dedupe_by_path(results))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn detect_slicers_returns_without_panicking() {
        // Best-effort auf dem echten System, in dem die Tests laufen: darf
        // leer sein, muss aber immer zurueckkehren statt zu paniken, egal
        // welche Slicer lokal installiert sind.
        let _ = detect_slicers();
    }
}
