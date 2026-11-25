//! Desktop side of renaming a file or folder in its directory (issue #20, package P4). The
//! logic (collision check, case-only rename through an intermediate name, rollback) lives in
//! `katalog_core::file_rename`; this module adapts the desktop file-system boundary (`Fs`) to it
//! and keeps the function names the folder rename and the tests use.
use std::ffi::OsString;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

use super::fs_boundary::{move_file_with, Fs};
#[cfg(test)]
use super::fs_boundary::RealFs;
#[cfg(test)]
use katalog_core::file_rename as core;
use katalog_core::file_rename::FileRenameFs;
#[cfg(test)]
use katalog_core::file_rename::is_case_only;

/// On these systems two names that differ only in letter case are the same entry.
pub(super) const CASE_INSENSITIVE_PATHS: bool = cfg!(any(windows, target_os = "macos"));

/// The core's file-system trait on top of the desktop boundary: every call goes to the same `Fs`
/// method as before, so the sequence of calls (and the injected faults of the tests) is unchanged.
pub(super) struct FileRenameFsAdapter<'a>(pub(super) &'a dyn Fs);

impl FileRenameFs for FileRenameFsAdapter<'_> {
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        self.0.symlink_metadata(path)
    }
    fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<OsString>> {
        self.0.read_dir_names(dir)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.0.canonicalize(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.0.rename(from, to)
    }
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        move_file_with(self.0, from, to)
    }
}

#[cfg(test)]
pub(super) fn target_collides(old: &Path, new: &Path) -> io::Result<bool> {
    core::target_collides(&FileRenameFsAdapter(&RealFs), old, new)
}

#[cfg(test)]
pub(super) fn rename_on_disk(old: &Path, new: &Path) -> io::Result<()> {
    core::rename_on_disk(&FileRenameFsAdapter(&RealFs), old, new)
}

#[cfg(test)]
fn rename_with(old: &Path, new: &Path, rename: impl FnMut(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    core::rename_with(&FileRenameFsAdapter(&RealFs), old, new, rename)
}

#[cfg(test)]
fn rename_via_intermediate_with(old: &Path, new: &Path, rename: impl FnMut(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    core::rename_via_intermediate(&FileRenameFsAdapter(&RealFs), old, new, rename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_only_missing_target_uses_one_rename() {
        let dir = crate::commands::unique_test_dir("rename_single_step");
        let old = dir.join("A.stl");
        let new = dir.join("a.stl");
        std::fs::write(&old, b"original").unwrap();
        if new.exists() {
            println!("skipped: missing case variant requires a case-sensitive volume");
            std::fs::remove_dir_all(dir).unwrap();
            return;
        }
        let mut calls = 0;
        rename_with(&old, &new, |from, to| {
            calls += 1;
            std::fs::rename(from, to)
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(std::fs::read(&new).unwrap(), b"original");
        assert!(!old.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unicode_case_and_normalization_decision() {
        for (old, new) in [
            ("Übersicht.stl", "übersicht.stl"),
            ("STRAẞE.stl", "straße.stl"),
            ("übersicht.stl", "u\u{308}bersicht.stl"),
            ("Übersicht.stl", "u\u{308}bersicht.stl"),
        ] {
            assert!(
                is_case_only(Path::new(old), Path::new(new)),
                "{old} -> {new}"
            );
        }
        assert!(!is_case_only(
            Path::new("straße.stl"),
            Path::new("strasse.stl")
        ));
    }

    #[test]
    fn intermediate_is_visible_preserves_extension_and_fits_name_limit() {
        for (name, directory) in [
            ("Rakete.stl".to_string(), false),
            ("Models".into(), true),
            (format!("{}.stl", "Ü".repeat(125)), false),
            ("Ü".repeat(127), true),
            (".Hidden.stl".into(), false),
            ("Models.v1".into(), true),
        ] {
            let dir = crate::commands::unique_test_dir("rename_visible");
            let old = dir.join(&name);
            if directory {
                std::fs::create_dir(&old).unwrap();
            } else {
                std::fs::write(&old, b"original").unwrap();
            }
            let mut calls = 0;
            rename_via_intermediate_with(&old, &dir.join(name.to_lowercase()), |from, to| {
                calls += 1;
                if calls == 1 {
                    let temporary = to.file_name().unwrap().to_str().unwrap();
                    assert!(!temporary.starts_with('.'));
                    assert!(temporary.contains(".3mf-rename-tmp-"));
                    assert!(super::super::folders::validate_name(temporary, true, super::super::folders::NameKind::File).is_ok());
                    if !directory {
                        assert_eq!(to.extension(), old.extension());
                    } else {
                        assert!(temporary
                            .starts_with(name.trim_start_matches('.').chars().next().unwrap()));
                    }
                }
                std::fs::rename(from, to)
            })
            .unwrap();
            assert_eq!(calls, 2);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn target_appearing_after_first_step_is_preserved_and_source_rolled_back() {
        let dir = crate::commands::unique_test_dir("rename_target_appears");
        let old = dir.join("A.stl");
        let new = dir.join("a.stl");
        std::fs::write(&old, b"original").unwrap();
        // Two distinct spellings are needed to restore both files on this volume.
        if new.exists() {
            println!("skipped: rollback alongside a distinct case variant requires a case-sensitive volume");
            std::fs::remove_dir_all(dir).unwrap();
            return;
        }
        let mut calls = 0;
        let error = rename_via_intermediate_with(&old, &new, |from, to| {
            calls += 1;
            std::fs::rename(from, to)?;
            if calls == 1 {
                std::fs::write(&new, b"other")?;
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(calls, 2);
        assert_eq!(std::fs::read(&old).unwrap(), b"original");
        assert_eq!(std::fs::read(&new).unwrap(), b"other");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unicode_rename_uses_only_required_steps_and_preserves_data() {
        for (old_name, new_name) in [
            ("Übersicht.stl", "übersicht.stl"),
            ("STRAẞE.stl", "straße.stl"),
            ("u\u{308}bersicht.stl", "übersicht.stl"),
        ] {
            let dir = crate::commands::unique_test_dir("rename_unicode");
            let old = dir.join(old_name);
            let new = dir.join(new_name);
            std::fs::write(&old, b"original").unwrap();
            let expected_calls = if new.exists() { 2 } else { 1 };
            let mut calls = 0;
            rename_with(&old, &new, |from, to| {
                calls += 1;
                std::fs::rename(from, to)
            })
            .unwrap();
            assert_eq!(calls, expected_calls);
            assert_eq!(std::fs::read(&new).unwrap(), b"original");
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn case_only_decision_requires_same_parent_and_different_names() {
        assert!(is_case_only(
            Path::new("models/A.stl"),
            Path::new("models/a.stl")
        ));
        assert!(!is_case_only(
            Path::new("models/A.stl"),
            Path::new("models/A.stl")
        ));
        assert!(!is_case_only(
            Path::new("models/A.stl"),
            Path::new("other/a.stl")
        ));
        assert!(!is_case_only(
            Path::new("models/A.stl"),
            Path::new("models/b.stl")
        ));
    }
    #[test]
    fn case_only_second_step_failure_restores_original() {
        let dir = crate::commands::unique_test_dir("rename_second_step");
        let old = dir.join("A.stl");
        let new = dir.join("a.stl");
        std::fs::write(&old, b"original").unwrap();
        let mut calls = 0;
        let error = rename_via_intermediate_with(&old, &new, |from, to| {
            calls += 1;
            if calls == 2 {
                // Exercise an OS error after the original has moved aside,
                // without depending on host permissions or case sensitivity.
                return std::fs::rename(from, dir.join("missing").join("destination"));
            }
            std::fs::rename(from, to)
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(calls, 3);
        assert_eq!(std::fs::read(&old).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn case_only_failed_rollback_preserves_recoverable_intermediate() {
        let dir = crate::commands::unique_test_dir("rename_failed_rollback");
        let old = dir.join("A.stl");
        let new = dir.join("a.stl");
        std::fs::write(&old, b"original").unwrap();
        let mut calls = 0;
        let error = rename_via_intermediate_with(&old, &new, |from, to| {
            calls += 1;
            if calls >= 2 {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "blocked"));
            }
            std::fs::rename(from, to)
        })
        .unwrap_err();
        assert_eq!(calls, 3);
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(std::fs::read(&entries[0]).unwrap(), b"original");
        assert!(error
            .to_string()
            .contains(&entries[0].display().to_string()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn case_only_distinct_entries_and_hard_links_collide() {
        let dir = crate::commands::unique_test_dir("rename_distinct_entries");
        let old = dir.join("A.stl");
        let new = dir.join("a.stl");
        std::fs::write(&old, b"original").unwrap();
        for hard_link in [false, true] {
            if hard_link {
                std::fs::hard_link(&old, &new).unwrap();
            } else {
                std::fs::write(&new, b"other").unwrap();
            }
            assert!(target_collides(&old, &new).unwrap());
            assert_eq!(
                rename_on_disk(&old, &new).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            assert_eq!(std::fs::read(&old).unwrap(), b"original");
            assert_eq!(
                std::fs::read(&new).unwrap(),
                if hard_link {
                    b"original".as_slice()
                } else {
                    b"other".as_slice()
                }
            );
            std::fs::remove_file(&new).unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn case_only_dangling_symlink_collides() {
        let dir = crate::commands::unique_test_dir("rename_dangling_link");
        let old = dir.join("A.stl");
        let new = dir.join("a.stl");
        std::fs::write(&old, b"original").unwrap();
        if !new.exists() {
            std::os::unix::fs::symlink(dir.join("missing"), &new).unwrap();
            assert!(target_collides(&old, &new).unwrap());
            assert!(rename_on_disk(&old, &new).is_err());
            assert_eq!(std::fs::read(&old).unwrap(), b"original");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn case_only_existing_alias_on_case_insensitive_volume() {
        let dir = crate::commands::unique_test_dir("rename_insensitive");
        let probe = dir.join("CaseProbe");
        std::fs::write(&probe, b"probe").unwrap();
        let insensitive = dir.join("caseprobe").exists();
        std::fs::remove_file(probe).unwrap();
        if !insensitive {
            println!("skipped: case-sensitive volume; case-insensitive rename test requires Windows/macOS volume without case sensitivity");
            std::fs::remove_dir_all(dir).unwrap();
            return;
        }
        // The catalog retains its spelling even when another app changes it.
        for (stored, actual, requested) in [
            ("A.stl", "A.stl", "a.stl"),
            ("a.stl", "a.stl", "A.stl"),
            ("Rakete.stl", "rakete.stl", "rakete.stl"),
            ("Rakete.stl", "rakete.stl", "RAKETE.stl"),
            ("Übersicht.stl", "übersicht.stl", "ÜBERSICHT.stl"),
            ("Straße.stl", "straße.stl", "STRAßE.stl"),
        ] {
            let old = dir.join(stored);
            let new = dir.join(requested);
            std::fs::write(dir.join(actual), b"original").unwrap();
            assert!(!target_collides(&old, &new).unwrap());
            let mut calls = 0;
            rename_with(&old, &new, |from, to| {
                calls += 1;
                std::fs::rename(from, to)
            })
            .unwrap();
            assert_eq!(calls, 2);
            let entries: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            assert_eq!(entries.len(), 1);
            // macOS can return decomposed names even for composed input.
            assert_eq!(
                entries[0].to_str().unwrap().nfc().collect::<String>(),
                requested.nfc().collect::<String>()
            );
            assert_eq!(std::fs::read(&new).unwrap(), b"original");
            std::fs::remove_file(&new).unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
