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
    // Web server: session, host checks and request shape. Every wording lives in the
    // interface; a server fault always uses `webServerInternal` and never carries text
    // from the database or the operating system.
    WebLoginRequired => "webLoginRequired",
    WebSessionExpired => "webSessionExpired",
    WebPasswordIncorrect => "webPasswordIncorrect",
    WebCurrentPasswordIncorrect => "webCurrentPasswordIncorrect",
    /// params: lowest and highest number of characters.
    WebPasswordLength => "webPasswordLength",
    WebPasswordUnchanged => "webPasswordUnchanged",
    WebPasswordGuessable => "webPasswordGuessable",
    WebPasswordChangeRequired => "webPasswordChangeRequired",
    WebPasswordChangeActive => "webPasswordChangeActive",
    WebPasswordCheckBusy => "webPasswordCheckBusy",
    WebTooManyAttempts => "webTooManyAttempts",
    WebTooManyRequests => "webTooManyRequests",
    WebCsrfInvalid => "webCsrfInvalid",
    WebHostNotAllowed => "webHostNotAllowed",
    WebHostHeaderInvalid => "webHostHeaderInvalid",
    WebOriginNotAllowed => "webOriginNotAllowed",
    WebProxyHeaderInvalid => "webProxyHeaderInvalid",
    WebMethodNotAllowed => "webMethodNotAllowed",
    WebNotFound => "webNotFound",
    WebRequestTimeout => "webRequestTimeout",
    WebBodyTooLarge => "webBodyTooLarge",
    WebCommandUnavailable => "webCommandUnavailable",
    /// The write path is busy; the client retries once on its own.
    WebWriteBusy => "webWriteBusy",
    WebServerInternal => "webServerInternal",
    // Web server: arguments of a request.
    ReqInvalidArguments => "reqInvalidArguments",
    ReqInvalidIds => "reqInvalidIds",
    /// params: the highest number of IDs allowed.
    ReqIdsTooMany => "reqIdsTooMany",
    PrinterDetailUnknown => "printerDetailUnknown",
    // Downloads and slicer links.
    FileNotFound => "fileNotFound",
    DlPathNotAllowed => "dlPathNotAllowed",
    DlRangeInvalid => "dlRangeInvalid",
    DlTooManyDownloads => "dlTooManyDownloads",
    SlicerLinkExpired => "slicerLinkExpired",
    // Tags, printers and spools (database and write path).
    TagNameEmpty => "tagNameEmpty",
    TagNameInvalid => "tagNameInvalid",
    /// params: the highest number of characters.
    TagNameTooLong => "tagNameTooLong",
    /// params: the highest number of blocked tags.
    BlockedTagsLimit => "blockedTagsLimit",
    PrinterNameEmpty => "printerNameEmpty",
    /// params: the highest number of characters.
    PrinterNameTooLong => "printerNameTooLong",
    PrinterOrderIncomplete => "printerOrderIncomplete",
    PrinterNotFound => "printerNotFound",
    /// params: the highest number of characters.
    PrinterManufacturerTooLong => "printerManufacturerTooLong",
    /// params: the highest number of characters.
    PrinterModelTooLong => "printerModelTooLong",
    /// params: lowest and highest value in mm.
    PrinterNozzleOutOfRange => "printerNozzleOutOfRange",
    /// params: axis (X, Y or Z), lowest and highest value in mm.
    PrinterBedOutOfRange => "printerBedOutOfRange",
    SpoolTemplateNotFound => "spoolTemplateNotFound",
    BottleNotFound => "bottleNotFound",
    /// params: lowest and highest count.
    RestockCountOutOfRange => "restockCountOutOfRange",
    InvalidCount => "invalidCount",
    InvalidAmount => "invalidAmount",
    AmountNotPositive => "amountNotPositive",
    ConsumeNotPositive => "consumeNotPositive",
    InvalidPrice => "invalidPrice",
    /// params: the highest number of colors.
    SpoolColorsTooMany => "spoolColorsTooMany",
    InvalidColor => "invalidColor",
    InvalidMaterialKind => "invalidMaterialKind",
    MaterialEmpty => "materialEmpty",
    InvalidDiameter => "invalidDiameter",
    ConsumeOnlyResin => "consumeOnlyResin",
    InvalidSlot => "invalidSlot",
    /// params: the highest number of entries.
    WebLimitReached => "webLimitReached",
    // Catalog entries written from the web.
    EntryNotFound => "entryNotFound",
    TextEmpty => "textEmpty",
    /// params: the highest number of characters.
    TextTooLong => "textTooLong",
    InvalidPrintStatus => "invalidPrintStatus",
    PositionTooLarge => "positionTooLarge",
    InvalidSourceUrl => "invalidSourceUrl",
    /// params: the highest number of characters.
    LicenseInvalid => "licenseInvalid",
    InvalidPrintTime => "invalidPrintTime",
    InvalidPhoto => "invalidPhoto",
    NotAListMember => "notAListMember",
    // Reading and importing files.
    /// params: the limit in bytes.
    FileTooLarge => "fileTooLarge",
    PreviewTooLarge => "previewTooLarge",
    /// params: the file extension.
    FileFormatUnsupported => "fileFormatUnsupported",
    SpoolNotFound => "spoolNotFound",
    ImportFixedFolderNeedsFiles => "importFixedFolderNeedsFiles",
    // Web server: reads and rescans.
    WebServerBusy => "webServerBusy",
    GeometryUnavailable => "geometryUnavailable",
    RescanRunning => "rescanRunning",
    RescanUnavailable => "rescanUnavailable",
    RescanTimeout => "rescanTimeout",
    CompanionTooLarge => "companionTooLarge",
    CompanionNotFound => "companionNotFound",
    // Saving a model as STL.
    StlSourceIsStl => "stlSourceIsStl",
    StlTargetIsSource => "stlTargetIsSource",
    /// params: file name of the existing target.
    StlTargetExists => "stlTargetExists",
    StlNothingToExport => "stlNothingToExport",
    /// params: number of triangles, limit.
    StlTooLarge => "stlTooLarge",
    StlSourceChanged => "stlSourceChanged",
    StlSourceUnreadable => "stlSourceUnreadable",
    StlSourceTooLarge => "stlSourceTooLarge",
    StlStepUnavailable => "stlStepUnavailable",
    /// params: the reason reported by the operating system.
    StlWriteFailed => "stlWriteFailed",
    // Thingiverse search with the user's own key. The key, the address and the search
    // term never appear in a code or its params.
    ThingiverseDisabled => "thingiverseDisabled",
    ThingiverseNoToken => "thingiverseNoToken",
    ThingiverseTokenFormat => "thingiverseTokenFormat",
    ThingiverseTokenInvalid => "thingiverseTokenInvalid",
    /// The protection service in front of Thingiverse answered instead of the API.
    ThingiverseBlocked => "thingiverseBlocked",
    /// params: seconds to wait.
    ThingiverseRateLimited => "thingiverseRateLimited",
    ThingiverseDailyLimit => "thingiverseDailyLimit",
    ThingiverseOffline => "thingiverseOffline",
    ThingiverseTimeout => "thingiverseTimeout",
    ThingiverseBadResponse => "thingiverseBadResponse",
    ThingiverseNotFound => "thingiverseNotFound",
    ThingiverseQueryInvalid => "thingiverseQueryInvalid",
    // Slicers, archives, updates and other desktop actions.
    SlicerExeNotFound => "slicerExeNotFound",
    SlicerPathNotFile => "slicerPathNotFile",
    SlicerPathNotExecutable => "slicerPathNotExecutable",
    SlicerPathNotExe => "slicerPathNotExe",
    ModelFileNotFound => "modelFileNotFound",
    BackupEntryNameInvalid => "backupEntryNameInvalid",
    BackupEntryNameUnsafeOnWindows => "backupEntryNameUnsafeOnWindows",
    UpdateNoLongerAvailable => "updateNoLongerAvailable",
    UpdateNotDownloaded => "updateNotDownloaded",
    ArchiveTargetNotApproved => "archiveTargetNotApproved",
    /// params: path of the target folder.
    ArchiveTargetMissing => "archiveTargetMissing",
    ArchiveTypeUnknown => "archiveTypeUnknown",
    OnlyWebAddresses => "onlyWebAddresses",
    LogPreviewExpired => "logPreviewExpired",
    // Startup notice after an update.
    UpdateBackupFolderMissing => "updateBackupFolderMissing",
}


#[cfg(test)]
#[path = "error_code_tests.rs"]
mod tests;
