//! Registry of the error codes the backend sends to the interface instead of
//! German prose. The frontend owns every wording (`src/i18n`, four languages);
//! the backend only names what went wrong and passes plain data (names, paths,
//! numbers) as `params`.
//!
//! The list is the single source: `CmdError::coded` accepts nothing else, and
//! `src/lib/errorCodes.json` mirrors it so a frontend test can check each code
//! against all four dictionaries. It lives in the core so the web server and the
//! desktop adapter name the same codes.

/// Codes that predate the registry and are sent as a bare `message` string. They
/// stay as they are; listing them here lets the shared list and the tests cover
/// them too.
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
    SourceNoCatalogFolder => "sourceNoCatalogFolder",
    SourceNoFilesSelected => "sourceNoFilesSelected",
    // Restoring a catalog backup.
    BackupNotZip => "backupNotZip",
    /// params: name of the missing entry.
    BackupMissingEntry => "backupMissingEntry",
    /// params: entry name, size in MB (rounded up), limit in MB.
    BackupEntryTooLarge => "backupEntryTooLarge",
    BackupSettingsNotUtf8 => "backupSettingsNotUtf8",
    BackupInvalidDatabase => "backupInvalidDatabase",
    // Startup notice after an update.
    UpdateBackupFolderMissing => "updateBackupFolderMissing",
}


#[cfg(test)]
#[path = "error_code_tests.rs"]
mod tests;
