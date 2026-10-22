use std::io;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

pub(super) fn normalized_lowercase(name: &str) -> String {
    name.nfc()
        .collect::<String>()
        .to_lowercase()
        .nfc()
        .collect()
}

fn is_case_only(old: &Path, new: &Path) -> bool {
    old.parent() == new.parent()
        && old.file_name() != new.file_name()
        && match (
            old.file_name().and_then(|s| s.to_str()),
            new.file_name().and_then(|s| s.to_str()),
        ) {
            (Some(old), Some(new)) => normalized_lowercase(old) == normalized_lowercase(new),
            _ => false,
        }
}

pub(super) fn target_collides(old: &Path, new: &Path) -> io::Result<bool> {
    let new_metadata = match std::fs::symlink_metadata(new) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
        Ok(metadata) => metadata,
    };
    if !is_case_only(old, new) {
        return Ok(true);
    }
    // Stored spelling can differ from both paths after an external rename.
    // Exactly one matching entry rules out distinct hard links and symlinks.
    let expected = normalized_lowercase(old.file_name().unwrap().to_str().unwrap());
    let mut matches = 0;
    for entry in std::fs::read_dir(
        old.parent()
            .ok_or_else(|| io::Error::other("missing parent"))?,
    )? {
        if entry?
            .file_name()
            .to_str()
            .is_some_and(|name| normalized_lowercase(name) == expected)
        {
            matches += 1;
        }
    }
    if matches != 1 {
        return Ok(true);
    }
    let old_metadata = std::fs::symlink_metadata(old)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(old_metadata.dev() != new_metadata.dev() || old_metadata.ino() != new_metadata.ino())
    }
    #[cfg(not(unix))]
    {
        // Canonical paths provide a stable fallback without unstable Windows
        // metadata APIs; metadata above also checks that both entries exist.
        Ok(old_metadata.file_type() != new_metadata.file_type()
            || std::fs::canonicalize(old)? != std::fs::canonicalize(new)?)
    }
}

fn intermediate_name(old: &Path, counter: u64) -> io::Result<String> {
    let name = old
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| io::Error::other("invalid source name"))?;
    let is_directory = std::fs::symlink_metadata(old)?.is_dir();
    let (stem, extension) = if is_directory {
        (name, String::new())
    } else {
        (
            old.file_stem().and_then(|s| s.to_str()).unwrap_or(name),
            old.extension()
                .and_then(|s| s.to_str())
                .map(|ext| format!(".{ext}"))
                .unwrap_or_default(),
        )
    };
    let suffix = format!(
        ".3mf-rename-tmp-{}-{counter}{extension}",
        std::process::id()
    );
    let budget = 255usize
        .checked_sub(suffix.len())
        .filter(|budget| *budget > 0)
        .ok_or_else(|| io::Error::other("extension too long for intermediate name"))?;
    let stem = stem.trim_start_matches('.');
    let mut end = stem.len().min(budget);
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = if end == 0 { "_" } else { &stem[..end] };
    Ok(format!("{prefix}{suffix}"))
}

/// Renames a single file inside its directory without ever replacing an existing
/// entry: the collision check alone leaves a window in which another program could
/// create the target, and `std::fs::rename` would then overwrite it on Linux and
/// macOS. `move_file` links first, which fails with `AlreadyExists` instead.
/// Case-only renames and symlinks keep `rename_on_disk`: a case-only target is the
/// file itself on a case-insensitive volume (a link would be refused), and a link
/// to a symlink would not move the symlink.
pub(super) fn rename_file_on_disk(old: &Path, new: &Path) -> io::Result<()> {
    if target_collides(old, new)? {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "rename target exists"));
    }
    let is_symlink = std::fs::symlink_metadata(old).is_ok_and(|m| m.file_type().is_symlink());
    if is_case_only(old, new) || is_symlink {
        return rename_on_disk(old, new);
    }
    super::import_rename::race_point("before-file-rename", new);
    super::move_file(old, new)
}

pub(super) fn rename_on_disk(old: &Path, new: &Path) -> io::Result<()> {
    rename_with(old, new, |from, to| std::fs::rename(from, to))
}

fn rename_with(
    old: &Path,
    new: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    if target_collides(old, new)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "rename target exists",
        ));
    }
    if !is_case_only(old, new) {
        return rename(old, new);
    }
    // A free destination permits one atomic rename, even for case-only changes.
    match std::fs::symlink_metadata(new) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return rename(old, new),
        Err(error) => return Err(error),
        Ok(_) => {}
    }
    rename_via_intermediate_with(old, new, rename)
}

fn rename_via_intermediate_with(
    old: &Path,
    new: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let parent = old
        .parent()
        .ok_or_else(|| io::Error::other("missing parent"))?;
    // A sibling keeps both rename steps on the same filesystem. Check with
    // symlink_metadata so dangling links cannot be used as intermediate names.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let intermediate = loop {
        let counter = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate = parent.join(intermediate_name(old, counter)?);
        match std::fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => break candidate,
            Err(error) => return Err(error),
            Ok(_) => continue,
        }
    };
    rename(old, &intermediate)?;
    // The source no longer aliases the destination: every existing target
    // now belongs to somebody else, including a dangling symlink.
    let second = match std::fs::symlink_metadata(new) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => rename(&intermediate, new),
        Err(error) => Err(error),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "rename target exists",
        )),
    };
    if let Err(error) = second {
        let rollback = match std::fs::symlink_metadata(old) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => rename(&intermediate, old),
            Err(e) => Err(e),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "rollback target exists",
            )),
        };
        if let Err(rollback) = rollback {
            return Err(io::Error::other(format!(
                "Umbenennung fehlgeschlagen ({error}); Rollback fehlgeschlagen ({rollback}); Daten liegen unter {}",
                intermediate.display()
            )));
        }
        return Err(error);
    }
    Ok(())
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
                    assert!(super::super::folders::validate_name(temporary, true, "Name").is_ok());
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
