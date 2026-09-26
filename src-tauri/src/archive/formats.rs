//! Format adapters: per format "list entries" and "stream to the extractor".
//! The adapters know no target paths and never write themselves.
//!
//! RAR is read into memory in UnRAR TEST MODE and then written through the
//! extractor like every other format. Why not let UnRAR extract: the `unrar`
//! crate only passes a complete target name (DestName) when extracting (on
//! Linux also with `extract_with_base`). That turns off UnRAR's own path check,
//! and RAR5 entries of type file copy/hardlink resolve their source relative to
//! the process's working directory - a crafted archive could copy arbitrary
//! local files into the catalog. In test mode UnRAR creates no files, links or
//! copies at all; such reference entries simply deliver no data and are skipped.

use std::fs::File;
use std::cell::Cell;
use std::io::{BufReader, Read};
use std::rc::Rc;
use std::path::Path;

use super::extract::Extractor;
use super::{ArchiveError, ArchiveFormat, EntryKind, EntryMeta, MAX_ENTRIES, MAX_UNPACKED_BYTES};

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;
/// Windows attribute FILE_ATTRIBUTE_REPARSE_POINT (symlinks/junctions).
const WIN_REPARSE_POINT: u32 = 0x400;
/// 7-Zip convention: bit 15 set = the upper 16 bits hold the Unix mode.
const SEVENZ_UNIX_EXTENSION: u32 = 0x8000;
/// Limit for ONE RAR entry: it is held completely in memory.
pub(super) const MAX_RAR_ENTRY_BYTES: u64 = 1024 * 1024 * 1024;

fn unreadable(e: impl std::fmt::Display) -> ArchiveError {
    ArchiveError::Unreadable(e.to_string())
}

pub(super) fn list(path: &Path, format: ArchiveFormat) -> Result<Vec<EntryMeta>, ArchiveError> {
    match format {
        ArchiveFormat::Zip => list_zip(path),
        ArchiveFormat::SevenZ => list_7z(path),
        ArchiveFormat::Rar => list_rar(path),
        _ => list_tar(path, format),
    }
}

pub(super) fn extract(path: &Path, format: ArchiveFormat, ex: &mut Extractor<'_>) -> Result<(), ArchiveError> {
    match format {
        ArchiveFormat::Zip => extract_zip(path, ex),
        ArchiveFormat::SevenZ => extract_7z(path, ex),
        ArchiveFormat::Rar => extract_rar(path, ex),
        _ => extract_tar(path, format, ex),
    }
}

// ---------- ZIP ----------

fn zip_meta(file: &zip::read::ZipFile<'_>) -> EntryMeta {
    EntryMeta {
        name: file.name().to_string(),
        size: file.size(),
        kind: if file.is_dir() {
            EntryKind::Directory
        } else if file.is_symlink() {
            EntryKind::Other
        } else {
            EntryKind::File
        },
        encrypted: file.encrypted(),
    }
}

/// Common methods only. LZMA/XZ in ZIP practically never appear in model
/// downloads, but their decoders reserve memory according to the dictionary size
/// in the header (up to 4 GB) - a crafted archive could crash the app that way.
fn zip_method_allowed(file: &zip::read::ZipFile<'_>) -> bool {
    use zip::CompressionMethod;
    matches!(
        file.compression(),
        CompressionMethod::Stored
            | CompressionMethod::Deflated
            | CompressionMethod::Deflate64
            | CompressionMethod::Bzip2
            | CompressionMethod::Zstd
    )
}

fn check_zip_method(file: &zip::read::ZipFile<'_>) -> Result<(), ArchiveError> {
    if file.is_file() && !file.encrypted() && !zip_method_allowed(file) {
        return Err(ArchiveError::Unsupported(format!(
            "Kompressionsverfahren {:?}",
            file.compression()
        )));
    }
    Ok(())
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<BufReader<File>>, ArchiveError> {
    let file = BufReader::new(File::open(path)?);
    zip::ZipArchive::new(file).map_err(unreadable)
}

fn list_zip(path: &Path) -> Result<Vec<EntryMeta>, ArchiveError> {
    let mut archive = open_zip(path)?;
    let count = archive.len().min(MAX_ENTRIES + 1);
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        // by_index_raw neither decrypts nor decompresses - so it also works for encrypted entries.
        let file = archive.by_index_raw(index).map_err(unreadable)?;
        check_zip_method(&file)?;
        out.push(zip_meta(&file));
    }
    Ok(out)
}

fn extract_zip(path: &Path, ex: &mut Extractor<'_>) -> Result<(), ArchiveError> {
    let mut archive = open_zip(path)?;
    for index in 0..archive.len() {
        let meta = {
            let file = archive.by_index_raw(index).map_err(unreadable)?;
            check_zip_method(&file)?;
            zip_meta(&file)
        };
        if let Some(target) = ex.prepare(&meta)? {
            let mut file = archive.by_index(index).map_err(unreadable)?;
            ex.write_from(&target, &mut file)?;
        }
    }
    Ok(())
}

// ---------- TAR (+ gz/bz2/xz/zst) ----------

/// Limit for the DECOMPRESSED tar stream: payload plus generously sized
/// header/padding blocks. Also limits data that is never written (skipped
/// entries, listing in `inspect`) - otherwise a "compression bomb" could keep the
/// app busy for minutes without the write budget ever kicking in.
const TAR_STREAM_LIMIT: u64 = MAX_UNPACKED_BYTES + (MAX_ENTRIES as u64 + 16) * 4096;
/// Memory limit for the xz decoder (KiB). `xz -9` needs ~65 MiB.
const XZ_MEM_LIMIT_KIB: u32 = 256 * 1024;

/// Reads at most `remaining` bytes and remembers an overflow, so it stays
/// detectable despite the error wrapping in the tar crate.
struct LimitedReader<R> {
    inner: R,
    remaining: u64,
    exceeded: Rc<Cell<bool>>,
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => {
                    self.exceeded.set(true);
                    Err(std::io::Error::other("tar-Datenstrom ueberschreitet das Limit"))
                }
            };
        }
        let max = buf.len().min(self.remaining.min(usize::MAX as u64) as usize);
        let n = self.inner.read(&mut buf[..max])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

fn tar_error(exceeded: &Cell<bool>, e: impl std::fmt::Display) -> ArchiveError {
    if exceeded.get() {
        ArchiveError::LimitExceeded
    } else {
        unreadable(e)
    }
}

fn tar_stream(path: &Path, format: ArchiveFormat, exceeded: Rc<Cell<bool>>) -> Result<Box<dyn Read>, ArchiveError> {
    let file = BufReader::new(File::open(path)?);
    let decoded: Box<dyn Read> = match format {
        ArchiveFormat::Tar => Box::new(file),
        ArchiveFormat::TarGz => Box::new(flate2::read::MultiGzDecoder::new(file)),
        ArchiveFormat::TarBz2 => Box::new(bzip2::read::MultiBzDecoder::new(file)),
        ArchiveFormat::TarXz => Box::new(lzma_rust2::XzReader::new_mem_limit(file, true, XZ_MEM_LIMIT_KIB)),
        ArchiveFormat::TarZst => {
            Box::new(ruzstd::decoding::StreamingDecoder::new(file).map_err(unreadable)?)
        }
        other => return Err(ArchiveError::Unsupported(format!("kein tar-Format: {other:?}"))),
    };
    Ok(Box::new(LimitedReader {
        inner: decoded,
        remaining: TAR_STREAM_LIMIT,
        exceeded,
    }))
}

fn tar_meta<R: Read>(entry: &tar::Entry<'_, R>) -> EntryMeta {
    use tar::EntryType;
    EntryMeta {
        name: String::from_utf8_lossy(&entry.path_bytes()).to_string(),
        size: entry.size(),
        kind: match entry.header().entry_type() {
            EntryType::Regular | EntryType::Continuous => EntryKind::File,
            EntryType::Directory => EntryKind::Directory,
            _ => EntryKind::Other,
        },
        encrypted: false,
    }
}

fn list_tar(path: &Path, format: ArchiveFormat) -> Result<Vec<EntryMeta>, ArchiveError> {
    let exceeded = Rc::new(Cell::new(false));
    let mut archive = tar::Archive::new(tar_stream(path, format, exceeded.clone())?);
    let mut out = Vec::new();
    for entry in archive.entries().map_err(|e| tar_error(&exceeded, e))? {
        let entry = entry.map_err(|e| tar_error(&exceeded, e))?;
        out.push(tar_meta(&entry));
        if out.len() > MAX_ENTRIES {
            break;
        }
    }
    Ok(out)
}

fn extract_tar(path: &Path, format: ArchiveFormat, ex: &mut Extractor<'_>) -> Result<(), ArchiveError> {
    let exceeded = Rc::new(Cell::new(false));
    let mut archive = tar::Archive::new(tar_stream(path, format, exceeded.clone())?);
    for entry in archive.entries().map_err(|e| tar_error(&exceeded, e))? {
        let mut entry = entry.map_err(|e| tar_error(&exceeded, e))?;
        let meta = tar_meta(&entry);
        if let Some(target) = ex.prepare(&meta)? {
            ex.write_from(&target, &mut entry).map_err(|e| match e {
                ArchiveError::Io(io) => tar_error(&exceeded, io),
                other => other,
            })?;
        }
    }
    Ok(())
}

// ---------- 7z ----------

fn sevenz_error(e: sevenz_rust2::Error) -> ArchiveError {
    use sevenz_rust2::Error;
    match e {
        Error::PasswordRequired | Error::MaybeBadPassword(_) => ArchiveError::Encrypted,
        Error::UnsupportedCompressionMethod(m) if m.to_uppercase().contains("AES") => {
            ArchiveError::Encrypted
        }
        Error::UnsupportedCompressionMethod(m) => ArchiveError::Unsupported(m),
        Error::MaxMemLimited { .. } => ArchiveError::LimitExceeded,
        other => unreadable(other),
    }
}

fn sevenz_meta(entry: &sevenz_rust2::ArchiveEntry, encrypted: bool) -> EntryMeta {
    let attrs = entry.windows_attributes;
    let unix_mode = if entry.has_windows_attributes && attrs & SEVENZ_UNIX_EXTENSION != 0 {
        Some(attrs >> 16)
    } else {
        None
    };
    let is_link = unix_mode.is_some_and(|m| m & S_IFMT == S_IFLNK)
        || (entry.has_windows_attributes && attrs & WIN_REPARSE_POINT != 0);
    EntryMeta {
        name: entry.name().to_string(),
        size: entry.size(),
        kind: if entry.is_directory() {
            EntryKind::Directory
        } else if entry.is_anti_item() || is_link {
            EntryKind::Other
        } else {
            EntryKind::File
        },
        encrypted: encrypted && entry.has_stream(),
    }
}

fn open_7z(path: &Path) -> Result<(sevenz_rust2::ArchiveReader<BufReader<File>>, bool), ArchiveError> {
    let file = BufReader::new(File::open(path)?);
    let reader =
        sevenz_rust2::ArchiveReader::new(file, sevenz_rust2::Password::empty()).map_err(sevenz_error)?;
    // LZMA/LZMA2 reserve memory up to the unpacked size of the block (at most the
    // dictionary size). So limit the block sizes BEFORE decoding - `sevenz-rust2`
    // itself has no memory limit.
    let too_large = reader.archive().blocks.iter().any(|block| {
        block
            .coders
            .iter()
            .any(|c| block.get_unpack_size_for_coder(c) > MAX_UNPACKED_BYTES)
    });
    if too_large {
        return Err(ArchiveError::LimitExceeded);
    }
    let encrypted = reader.archive().blocks.iter().any(|block| {
        block
            .coders
            .iter()
            .any(|c| c.encoder_method_id() == sevenz_rust2::EncoderMethod::ID_AES256_SHA256)
    });
    Ok((reader, encrypted))
}

fn list_7z(path: &Path) -> Result<Vec<EntryMeta>, ArchiveError> {
    let (reader, encrypted) = open_7z(path)?;
    Ok(reader
        .archive()
        .files
        .iter()
        .take(MAX_ENTRIES + 1)
        .map(|e| sevenz_meta(e, encrypted))
        .collect())
}

fn extract_7z(path: &Path, ex: &mut Extractor<'_>) -> Result<(), ArchiveError> {
    let (mut reader, encrypted) = open_7z(path)?;
    if encrypted {
        return Err(ArchiveError::Encrypted);
    }
    // The library expects its own error type in the callback - we park our error
    // here and abort with Ok(false).
    let mut failure: Option<ArchiveError> = None;
    let result = reader.for_each_entries(|entry, data| {
        let meta = sevenz_meta(entry, false);
        let outcome = match ex.prepare(&meta) {
            Ok(Some(target)) => ex.write_from(&target, data),
            Ok(None) => Ok(()),
            Err(e) => Err(e),
        };
        match outcome {
            Ok(()) => Ok(true),
            Err(e) => {
                failure = Some(e);
                Ok(false)
            }
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    result.map_err(sevenz_error)
}

// ---------- RAR ----------

fn rar_error(e: unrar::error::UnrarError) -> ArchiveError {
    use unrar::error::Code;
    match e.code {
        Code::MissingPassword | Code::BadPassword => ArchiveError::Encrypted,
        Code::UnknownFormat => ArchiveError::Unsupported(e.to_string()),
        Code::ECreate | Code::EWrite | Code::EClose => {
            ArchiveError::Io(std::io::Error::other(e.to_string()))
        }
        _ => unreadable(e),
    }
}

fn rar_meta(header: &unrar::FileHeader) -> EntryMeta {
    let attr = header.file_attr;
    let is_link = attr & S_IFMT == S_IFLNK || attr & WIN_REPARSE_POINT != 0;
    EntryMeta {
        name: header.filename.to_string_lossy().to_string(),
        size: header.unpacked_size,
        kind: if header.is_directory() {
            EntryKind::Directory
        } else if is_link {
            EntryKind::Other
        } else {
            EntryKind::File
        },
        encrypted: header.is_encrypted(),
    }
}

fn reject_multipart(path: &Path) -> Result<(), ArchiveError> {
    if unrar::Archive::new(path).is_multipart() {
        return Err(ArchiveError::Unsupported(
            "mehrteilige RAR-Archive werden nicht unterstuetzt".to_string(),
        ));
    }
    Ok(())
}

fn list_rar(path: &Path) -> Result<Vec<EntryMeta>, ArchiveError> {
    reject_multipart(path)?;
    let archive = unrar::Archive::new(path).open_for_listing().map_err(rar_error)?;
    let mut out = Vec::new();
    for header in archive {
        out.push(rar_meta(&header.map_err(rar_error)?));
        if out.len() > MAX_ENTRIES {
            break;
        }
    }
    Ok(out)
}

#[derive(Debug, PartialEq, Eq)]
enum RarEntryDecision {
    /// Size fits - the entry may be read into memory.
    Read,
    /// The data read matches the header - write it.
    Write,
    /// Data length differs from the header (typically file copy/hardlink entries
    /// that deliver no data in test mode) - don't write.
    Skip,
}

/// `actual == None`: check BEFORE reading, `Some(len)`: after.
fn rar_entry_decision(declared: u64, actual: Option<usize>) -> Result<RarEntryDecision, ArchiveError> {
    match actual {
        None if declared > MAX_RAR_ENTRY_BYTES => Err(ArchiveError::LimitExceeded),
        None => Ok(RarEntryDecision::Read),
        Some(len) if len as u64 == declared => Ok(RarEntryDecision::Write),
        Some(_) => Ok(RarEntryDecision::Skip),
    }
}

/// Reads every RAR entry into memory in test mode (see module docs) and writes
/// it through the extractor. Budget and entry size are checked BEFORE reading.
fn extract_rar(path: &Path, ex: &mut Extractor<'_>) -> Result<(), ArchiveError> {
    reject_multipart(path)?;
    let mut archive = unrar::Archive::new(path).open_for_processing().map_err(rar_error)?;
    while let Some(header) = archive.read_header().map_err(rar_error)? {
        let meta = rar_meta(header.entry());
        archive = match ex.prepare(&meta)? {
            Some(target) => {
                rar_entry_decision(meta.size, None)?;
                ex.ensure_budget_for(meta.size)?;
                let (data, next) = header.read().map_err(rar_error)?;
                match rar_entry_decision(meta.size, Some(data.len()))? {
                    RarEntryDecision::Write => ex.write_from(&target, &mut &data[..])?,
                    _ => ex.skip_unsafe(),
                }
                next
            }
            None => header.skip().map_err(rar_error)?,
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limited_reader_passes_data_up_to_the_limit_and_flags_more() {
        let exceeded = Rc::new(Cell::new(false));
        let mut exact = LimitedReader { inner: &b"abcd"[..], remaining: 4, exceeded: exceeded.clone() };
        let mut out = Vec::new();
        exact.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"abcd");
        assert!(!exceeded.get());

        let mut too_much = LimitedReader { inner: &b"abcdef"[..], remaining: 4, exceeded: exceeded.clone() };
        let mut out = Vec::new();
        assert!(too_much.read_to_end(&mut out).is_err());
        assert!(exceeded.get());
    }

    #[test]
    fn rar_entry_decision_limits_size_and_skips_entries_without_matching_data() {
        // Before reading: oversized entries are never loaded into memory.
        assert!(matches!(
            rar_entry_decision(MAX_RAR_ENTRY_BYTES + 1, None),
            Err(ArchiveError::LimitExceeded)
        ));
        assert!(matches!(rar_entry_decision(MAX_RAR_ENTRY_BYTES, None), Ok(RarEntryDecision::Read)));
        // After reading: file copy/hardlink entries have a header size but no data.
        assert!(matches!(rar_entry_decision(18, Some(0)), Ok(RarEntryDecision::Skip)));
        assert!(matches!(rar_entry_decision(18, Some(17)), Ok(RarEntryDecision::Skip)));
        assert!(matches!(rar_entry_decision(18, Some(18)), Ok(RarEntryDecision::Write)));
        assert!(matches!(rar_entry_decision(0, Some(0)), Ok(RarEntryDecision::Write)));
    }

    #[test]
    fn sevenz_meta_detects_symlinks_and_reparse_points() {
        // Unix symlink via SEVENZ_UNIX_EXTENSION
        let mut entry = sevenz_rust2::ArchiveEntry::new_file("link.stl");
        entry.has_windows_attributes = true;
        entry.windows_attributes = SEVENZ_UNIX_EXTENSION | (0o120777 << 16);
        assert_eq!(sevenz_meta(&entry, false).kind, EntryKind::Other, "Unix symlink");

        // Windows reparse point (symlink/junction)
        let mut entry = sevenz_rust2::ArchiveEntry::new_file("link.stl");
        entry.has_windows_attributes = true;
        entry.windows_attributes = WIN_REPARSE_POINT;
        assert_eq!(sevenz_meta(&entry, false).kind, EntryKind::Other, "Windows reparse");

        // Regular file (not a link)
        let mut entry = sevenz_rust2::ArchiveEntry::new_file("teil.stl");
        entry.has_windows_attributes = true;
        entry.windows_attributes = SEVENZ_UNIX_EXTENSION | (0o100644 << 16);
        assert_eq!(sevenz_meta(&entry, false).kind, EntryKind::File, "Regular file");
    }
}
