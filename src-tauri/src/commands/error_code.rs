//! Registry of the error codes the backend sends to the interface instead of
//! German prose. The frontend owns every wording (`src/i18n`, four languages);
//! the backend only names what went wrong and passes plain data (names, paths,
//! numbers) as `params`.
//!
//! The list is the single source: `CmdError::coded` accepts nothing else, and
//! `src/lib/errorCodes.json` mirrors it so a frontend test can check each code
//! against all four dictionaries.

/// Codes that predate the registry and are sent as a bare `message` string. They
/// stay as they are; listing them here lets the shared list and the tests cover
/// them too.
#[cfg(test)]
pub const LEGACY_CODES: &[&str] = &[
    "cancelled",
    "container",
    "containerExternalUnavailable",
    "containerSlicerUnavailable",
    "containerFileManagerUnavailable",
    "companionOpenContainer",
    "linuxMenuForeignFile",
    "linuxMenuThirdParty",
    "imageUploadTooLarge",
    "imageUploadUnsupported",
    "imageUploadUnreadable",
    "importActive",
];

macro_rules! error_codes {
    ($($(#[$doc:meta])* $variant:ident => $name:literal,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum ErrorCode { $($(#[$doc])* $variant,)* }

        impl ErrorCode {
            /// Every registered code, in declaration order.
            #[cfg(test)]
            pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$variant,)*];

            /// The camelCase text that travels as `CmdError::message`.
            pub const fn as_str(self) -> &'static str {
                match self { $(ErrorCode::$variant => $name,)* }
            }
        }
    };
}

error_codes! {
    // Folder names. The wording differs per kind, so each kind has full codes of its
    // own; only the offending character is a parameter.
    FolderNameEmpty => "folderNameEmpty",
    /// params: the character, or `U+XXXX` for a control character.
    FolderNameForbiddenChar => "folderNameForbiddenChar",
    FolderNameTooLong => "folderNameTooLong",
    FolderNameNotAllowed => "folderNameNotAllowed",
    FolderNameEndsWithDotOrSpace => "folderNameEndsWithDotOrSpace",
    FolderNameWindowsReserved => "folderNameWindowsReserved",
    // File names.
    FileNameEmpty => "fileNameEmpty",
    /// params: the character, or `U+XXXX` for a control character.
    FileNameForbiddenChar => "fileNameForbiddenChar",
    FileNameTooLong => "fileNameTooLong",
    FileNameNotAllowed => "fileNameNotAllowed",
    FileNameEndsWithDotOrSpace => "fileNameEndsWithDotOrSpace",
    FileNameWindowsReserved => "fileNameWindowsReserved",
    // Creating, renaming and moving folders.
    /// params: path of the existing folder.
    FolderExists => "folderExists",
    CatalogRootProtected => "catalogRootProtected",
    /// params: path of the existing target.
    RenameTargetFolderExists => "renameTargetFolderExists",
    FolderMoveIntoSelf => "folderMoveIntoSelf",
    FolderMoveIntoDescendant => "folderMoveIntoDescendant",
    FolderMoveIntoSelfOrDescendant => "folderMoveIntoSelfOrDescendant",
    /// params: path of the folder.
    FolderMissingOnDisk => "folderMissingOnDisk",
    /// params: path of the target folder.
    TargetFolderMissingOnDisk => "targetFolderMissingOnDisk",
    FolderMoveAlreadyThere => "folderMoveAlreadyThere",
    /// params: path that is taken.
    FolderMoveNameTaken => "folderMoveNameTaken",
    // Setting up the catalog folder.
    CatalogDirNoPermission => "catalogDirNoPermission",
    CatalogDirNameTooLong => "catalogDirNameTooLong",
    CatalogDirNameInvalid => "catalogDirNameInvalid",
    CatalogDirRaced => "catalogDirRaced",
    CatalogDirIsFile => "catalogDirIsFile",
    CatalogDirIsLink => "catalogDirIsLink",
    // Deleting a folder (app trash) and removing it from the catalog.
    FolderNotFound => "folderNotFound",
    CatalogRootNotDeletable => "catalogRootNotDeletable",
    InvalidCatalogId => "invalidCatalogId",
    ModelNotFoundOrTrashed => "modelNotFoundOrTrashed",
    CatalogBaseDirProtected => "catalogBaseDirProtected",
    // Moving and renaming files.
    FileInTrash => "fileInTrash",
    /// params: path of the file.
    FileMissingOnDisk => "fileMissingOnDisk",
    /// params: number of files, limit.
    MoveTooManyFiles => "moveTooManyFiles",
    /// params: path of the existing target.
    FileTargetExists => "fileTargetExists",
    // Companion files that travel with a model.
    ModelPathInvalid => "modelPathInvalid",
    CompanionFolderUnverifiable => "companionFolderUnverifiable",
    CompanionFolderNotReal => "companionFolderNotReal",
    CompanionFolderElsewhere => "companionFolderElsewhere",
    CompanionTargetFolderBlocked => "companionTargetFolderBlocked",
    /// params: the reason reported by the operating system.
    CompanionTargetFolderCreateFailed => "companionTargetFolderCreateFailed",
    /// params: file name of the companion.
    CompanionNameTaken => "companionNameTaken",
    // Source folders.
    SourceFolderMissing => "sourceFolderMissing",
    SourceNotAFolder => "sourceNotAFolder",
    SourceFolderUnreadable => "sourceFolderUnreadable",
    SourceFolderMoved => "sourceFolderMoved",
    SourcePathInvalid => "sourcePathInvalid",
    SourceFolderProtected => "sourceFolderProtected",
    SourceFolderInCatalog => "sourceFolderInCatalog",
    SourceFolderDuplicate => "sourceFolderDuplicate",
    SourceFolderOverlap => "sourceFolderOverlap",
    SourceFolderNotFound => "sourceFolderNotFound",
    SourceFileNotNew => "sourceFileNotNew",
    FolderNotPicked => "folderNotPicked",
    // Restoring a catalog backup.
    BackupNotZip => "backupNotZip",
    /// params: name of the missing entry.
    BackupMissingEntry => "backupMissingEntry",
    /// params: entry name, size in MB (rounded up), limit in MB.
    BackupEntryTooLarge => "backupEntryTooLarge",
    BackupSettingsNotUtf8 => "backupSettingsNotUtf8",
    BackupInvalidDatabase => "backupInvalidDatabase",
}

/// The shared list the frontend test reads (`src/lib/errorCodes.json`).
#[cfg(test)]
fn shared_list_json() -> String {
    let list = |codes: Vec<&str>| {
        codes.iter().map(|c| format!("    \"{c}\"")).collect::<Vec<_>>().join(",\n")
    };
    format!(
        "{{\n  \"registered\": [\n{}\n  ],\n  \"legacy\": [\n{}\n  ]\n}}\n",
        list(ErrorCode::ALL.iter().map(|c| c.as_str()).collect()),
        list(LEGACY_CODES.to_vec()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const SHARED_LIST: &str = include_str!("../../../src/lib/errorCodes.json");

    /// Codes that exist on one side only; empty when both sides agree.
    fn list_differences(shared: &[String], registry: &[&str]) -> Vec<String> {
        let shared_set: HashSet<&str> = shared.iter().map(String::as_str).collect();
        let registry_set: HashSet<&str> = registry.iter().copied().collect();
        let mut diff: Vec<String> = shared_set.symmetric_difference(&registry_set).map(|c| c.to_string()).collect();
        diff.sort();
        diff
    }

    fn strings(value: &serde_json::Value, key: &str) -> Vec<String> {
        value[key].as_array().unwrap_or_else(|| panic!("{key} missing"))
            .iter().map(|v| v.as_str().expect("string").to_string()).collect()
    }

    #[test]
    fn names_are_unique_camel_case() {
        let mut seen = HashSet::new();
        for code in ErrorCode::ALL {
            let name = code.as_str();
            assert!(seen.insert(name), "{name} is registered twice");
            assert!(name.chars().next().is_some_and(|c| c.is_ascii_lowercase()), "{name}");
            assert!(name.chars().all(|c| c.is_ascii_alphanumeric()), "{name}");
            assert!(!LEGACY_CODES.contains(&name), "{name} is also a legacy code");
        }
    }

    #[test]
    fn shared_list_matches_the_registry() {
        let json: serde_json::Value = serde_json::from_str(SHARED_LIST).expect("errorCodes.json parses");
        let registered: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(
            list_differences(&strings(&json, "registered"), &registered), Vec::<String>::new(),
            "src/lib/errorCodes.json is out of date; run `cargo test regenerate_shared_list -- --ignored`"
        );
        assert_eq!(list_differences(&strings(&json, "legacy"), LEGACY_CODES), Vec::<String>::new());
    }

    /// A code the backend can emit but the shared list does not know shows up as a difference.
    #[test]
    fn an_unregistered_code_is_reported() {
        let shared = vec!["folderNameEmpty".to_string()];
        assert_eq!(list_differences(&shared, &["folderNameEmpty", "madeUpCode"]), vec!["madeUpCode".to_string()]);
        assert_eq!(list_differences(&["staleCode".to_string()], &[]), vec!["staleCode".to_string()]);
    }

    /// Rewrites the shared list after a code was added; not part of the normal run.
    #[test]
    #[ignore]
    fn regenerate_shared_list() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/errorCodes.json");
        std::fs::write(path, shared_list_json()).unwrap();
    }

    /// The production part of a source file (everything before its test module).
    fn production_part(source: &str) -> &str {
        source.split("#[cfg(test)]").next().unwrap_or(source)
    }

    /// The body of `fn name`, up to the closing brace at column 0.
    fn function_body<'a>(source: &'a str, name: &str) -> &'a str {
        let production = production_part(source);
        let start = ["(", "<"].iter()
            .filter_map(|next| production.find(&format!("fn {name}{next}")))
            .min()
            .unwrap_or_else(|| panic!("fn {name} not found"));
        let rest = &production[start..];
        let end = rest.find("\n}\n").map_or(rest.len(), |i| i + 3);
        &rest[..end]
    }

    /// Free text handed to `CmdError::expected`: a string literal or `format!` that
    /// is not one of the legacy codes. Such text would reach the interface
    /// untranslated.
    fn free_text_errors(code: &str) -> Vec<String> {
        let mut found = Vec::new();
        for (index, _) in code.match_indices("CmdError::expected(") {
            let argument = code[index + "CmdError::expected(".len()..].trim_start();
            if let Some(literal) = argument.strip_prefix('"') {
                let text = literal.split('"').next().unwrap_or_default();
                if !LEGACY_CODES.contains(&text) {
                    found.push(format!("\"{text}\""));
                }
            } else if argument.starts_with("format!") {
                found.push(argument.lines().next().unwrap_or_default().to_string());
            }
        }
        found
    }

    /// Modules (or single functions) whose expected errors are fully migrated.
    /// `files.rs` and `companions.rs` hold unrelated actions too, so only the
    /// functions of the migrated actions are scanned there.
    fn migrated_sources() -> Vec<(&'static str, String)> {
        let whole = |name: &'static str, source: &str| (name, production_part(source).to_string());
        let functions = |name: &'static str, source: &str, fns: &[&str]| {
            (name, fns.iter().map(|f| function_body(source, f)).collect::<Vec<_>>().join("\n"))
        };
        vec![
            whole("folders.rs", include_str!("folders.rs")),
            whole("folder_trash.rs", include_str!("folder_trash.rs")),
            whole("trash.rs", include_str!("trash.rs")),
            whole("catalog_removal.rs", include_str!("catalog_removal.rs")),
            whole("source_folders.rs", include_str!("source_folders.rs")),
            functions("files.rs", include_str!("files.rs"), &[
                "validate_file_name", "move_file_core", "move_files_to_folder_with", "rename_file_with_conn",
            ]),
            functions("companions.rs", include_str!("companions.rs"), &["carry_with", "extras_dir_problem"]),
            functions("backup.rs", include_str!("backup.rs"), &[
                "reject_oversized_zip_entry", "read_catalog_archive", "validate_catalog_db_bytes", "validate_backup_for_import",
            ]),
        ]
    }

    #[test]
    fn migrated_code_emits_no_free_text() {
        for (name, code) in migrated_sources() {
            assert_eq!(free_text_errors(&code), Vec::<String>::new(), "{name} still emits free text");
        }
    }

    /// The scanner itself must fail on free text and accept registered and legacy codes.
    #[test]
    fn the_scanner_flags_free_text() {
        assert_eq!(free_text_errors(r#"Err(CmdError::expected("Ordner existiert bereits"))"#).len(), 1);
        assert_eq!(free_text_errors(r#"CmdError::expected(format!("x {}", 1))"#).len(), 1);
        assert!(free_text_errors(r#"CmdError::expected("cancelled")"#).is_empty());
        assert!(free_text_errors("CmdError::coded(ErrorCode::FolderNameEmpty)").is_empty());
        assert!(free_text_errors("CmdError::expected(e.to_string())").is_empty());
    }
}
