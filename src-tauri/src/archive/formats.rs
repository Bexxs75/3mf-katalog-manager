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
use std::io::{BufReader, Read, Seek, SeekFrom};
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

// Header decoding happens inside ArchiveReader::new, before entry limits can
// apply. Keep both its input and the sum of declared coder outputs bounded.
const MAX_7Z_HEADER_BYTES: u64 = 16 * 1024 * 1024;

struct SevenZHeader<'a> {
    bytes: &'a [u8],
}

impl SevenZHeader<'_> {
    fn skip(&mut self, count: usize) -> Result<(), ArchiveError> {
        self.bytes = self.bytes.get(count..).ok_or_else(|| unreadable("truncated 7z header"))?;
        Ok(())
    }

    fn byte(&mut self) -> Result<u8, ArchiveError> {
        let byte = *self.bytes.first().ok_or_else(|| unreadable("truncated 7z header"))?;
        self.skip(1)?;
        Ok(byte)
    }

    fn expect(&mut self, expected: u8) -> Result<(), ArchiveError> {
        if self.byte()? != expected {
            return Err(unreadable("unsupported 7z header structure"));
        }
        Ok(())
    }

    fn number(&mut self) -> Result<u64, ArchiveError> {
        let first = self.byte()?;
        let mut mask = 0x80;
        let mut value = 0;
        for i in 0..8 {
            if first & mask == 0 {
                return Ok(value | (u64::from(first & (mask - 1)) << (8 * i)));
            }
            value |= u64::from(self.byte()?) << (8 * i);
            mask >>= 1;
        }
        Ok(value)
    }

    fn count(&mut self) -> Result<usize, ArchiveError> {
        let count = self.number()?;
        if count > self.bytes.len() as u64 {
            return Err(unreadable("7z header count exceeds available bytes"));
        }
        Ok(count as usize)
    }

    fn digests(&mut self, count: usize) -> Result<(), ArchiveError> {
        let defined = if self.byte()? != 0 {
            count
        } else {
            let mut defined = 0;
            for start in (0..count).step_by(8) {
                let bits = self.byte()?;
                for bit in 0..(count - start).min(8) {
                    defined += usize::from(bits & (0x80 >> bit) != 0);
                }
            }
            defined
        };
        self.skip(defined.checked_mul(4).ok_or(ArchiveError::LimitExceeded)?)
    }

    fn folder_outputs(&mut self) -> Result<usize, ArchiveError> {
        let coders = self.count()?;
        let mut inputs = 0usize;
        let mut outputs = 0usize;
        for _ in 0..coders {
            let flags = self.byte()?;
            if flags & 0xc0 != 0 || flags & 0x0f == 0 {
                return Err(unreadable("unsupported 7z header coder"));
            }
            self.skip(usize::from(flags & 0x0f))?;
            let (coder_inputs, coder_outputs) = if flags & 0x10 == 0 {
                (1, 1)
            } else {
                (self.count()?, self.count()?)
            };
            inputs = inputs.checked_add(coder_inputs).ok_or(ArchiveError::LimitExceeded)?;
            outputs = outputs.checked_add(coder_outputs).ok_or(ArchiveError::LimitExceeded)?;
            if inputs > MAX_7Z_HEADER_BYTES as usize || outputs > MAX_7Z_HEADER_BYTES as usize {
                return Err(ArchiveError::LimitExceeded);
            }
            if flags & 0x20 != 0 {
                let properties = self.count()?;
                self.skip(properties)?;
            }
        }
        let pairs = outputs.checked_sub(1).ok_or_else(|| unreadable("7z coder has no output"))?;
        let packed = inputs.checked_sub(pairs).ok_or_else(|| unreadable("invalid 7z stream counts"))?;
        for _ in 0..pairs {
            if self.number()? >= inputs as u64 || self.number()? >= outputs as u64 {
                return Err(unreadable("invalid 7z bind pair"));
            }
        }
        if packed != 1 {
            for _ in 0..packed {
                if self.number()? >= inputs as u64 {
                    return Err(unreadable("invalid 7z packed stream"));
                }
            }
        }
        Ok(outputs)
    }
}

fn check_7z_encoded_header(bytes: &[u8]) -> Result<(), ArchiveError> {
    let mut header = SevenZHeader { bytes };
    header.expect(0x17)?; // kEncodedHeader
    let mut section = header.byte()?;
    if section == 0x06 { // kPackInfo
        header.number()?; // pack position
        let streams = header.count()?;
        section = header.byte()?;
        if section == 0x09 { // kSize
            for _ in 0..streams { header.number()?; }
            section = header.byte()?;
        }
        if section == 0x0a { // kCRC
            header.digests(streams)?;
            section = header.byte()?;
        }
        if section != 0 { return Err(unreadable("invalid 7z PackInfo")); }
        section = header.byte()?;
    }
    if section != 0x07 { return Err(unreadable("missing 7z UnpackInfo")); }
    header.expect(0x0b)?; // kFolder
    let folders = header.count()?;
    header.expect(0)?; // external folder definitions cannot be checked here
    let mut outputs = 0usize;
    for _ in 0..folders {
        outputs = outputs.checked_add(header.folder_outputs()?).ok_or(ArchiveError::LimitExceeded)?;
    }
    header.expect(0x0c)?; // kCodersUnpackSize
    let mut unpacked = 0u64;
    for _ in 0..outputs {
        unpacked = unpacked.checked_add(header.number()?).ok_or(ArchiveError::LimitExceeded)?;
        if unpacked > MAX_7Z_HEADER_BYTES { return Err(ArchiveError::LimitExceeded); }
    }
    // The remaining CRC/substream metadata cannot increase coder output sizes;
    // let the library validate it, including ordinary solid-header layouts.
    Ok(())
}

fn check_7z_header<R: Read + Seek>(reader: &mut R) -> Result<(), ArchiveError> {
    let mut signature = [0u8; 32];
    reader.read_exact(&mut signature)?;
    if &signature[..6] != b"7z\xbc\xaf\x27\x1c" || signature[6] != 0 {
        return Err(unreadable("invalid 7z signature"));
    }
    let offset = u64::from_le_bytes(signature[12..20].try_into().unwrap());
    let size = u64::from_le_bytes(signature[20..28].try_into().unwrap());
    if size > MAX_7Z_HEADER_BYTES { return Err(ArchiveError::LimitExceeded); }
    // A missing start header enables the library's recovery scan, whose guessed
    // header would not have passed this check.
    if size == 0 { return Err(unreadable("missing 7z next header")); }
    let start = 32u64.checked_add(offset).ok_or(ArchiveError::LimitExceeded)?;
    let end = start.checked_add(size).ok_or(ArchiveError::LimitExceeded)?;
    if end > reader.seek(SeekFrom::End(0))? {
        return Err(unreadable("7z header outside archive"));
    }
    reader.seek(SeekFrom::Start(start))?;
    let mut header = vec![0; size as usize];
    reader.read_exact(&mut header)?;
    if header[0] == 0x17 { check_7z_encoded_header(&header)?; }
    reader.rewind()?;
    Ok(())
}

fn open_7z(path: &Path) -> Result<(sevenz_rust2::ArchiveReader<BufReader<File>>, bool), ArchiveError> {
    let mut file = BufReader::new(File::open(path)?);
    check_7z_header(&mut file)?;
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
                    _ => ex.skip_unsafe(&meta.name),
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

    fn encoded_header_fixture(sizes: &[u64]) -> Vec<u8> {
        fn number(out: &mut Vec<u8>, value: u64) {
            out.push(0xff);
            out.extend_from_slice(&value.to_le_bytes());
        }
        let mut header = vec![0x17, 0x06];
        number(&mut header, 0);
        number(&mut header, sizes.len() as u64);
        header.push(0x09);
        for _ in sizes { number(&mut header, 0); }
        header.extend_from_slice(&[0, 0x07, 0x0b]);
        number(&mut header, sizes.len() as u64);
        header.push(0);
        for _ in sizes { header.extend_from_slice(&[1, 0x21, 0x21, 1, 16]); }
        header.push(0x0c);
        for size in sizes { number(&mut header, *size); }
        header.extend_from_slice(&[0, 0]);
        let mut signature = b"7z\xbc\xaf\x27\x1c\x00\x04".to_vec();
        signature.extend_from_slice(&[0; 4]);
        signature.extend_from_slice(&0u64.to_le_bytes());
        signature.extend_from_slice(&(header.len() as u64).to_le_bytes());
        signature.extend_from_slice(&[0; 4]);
        signature.extend_from_slice(&header);
        signature
    }

    #[test]
    fn sevenz_rejects_large_encoded_header_before_decoder() {
        let dir = std::env::temp_dir().join(format!("encoded-header-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("header.7z");
        // No compressed payload or valid CRC: reaching the decoder would return
        // Unreadable, so LimitExceeded proves rejection before decoding.
        std::fs::write(&path, encoded_header_fixture(&[33_554_432])).unwrap();
        let result = open_7z(&path);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(result, Err(ArchiveError::LimitExceeded)));
    }

    #[test]
    fn sevenz_header_limits_total_output_and_accepts_boundary() {
        for sizes in [&[MAX_7Z_HEADER_BYTES][..], &[MAX_7Z_HEADER_BYTES / 2; 2][..]] {
            let bytes = encoded_header_fixture(sizes);
            let mut reader = std::io::Cursor::new(bytes);
            check_7z_header(&mut reader).unwrap();
            assert_eq!(reader.position(), 0);
        }
        let bytes = encoded_header_fixture(&[MAX_7Z_HEADER_BYTES / 2, MAX_7Z_HEADER_BYTES / 2 + 1]);
        assert!(matches!(check_7z_header(&mut std::io::Cursor::new(bytes)), Err(ArchiveError::LimitExceeded)));
    }

    #[test]
    fn sevenz_header_rejects_truncated_external_and_overflowing_metadata() {
        let bytes = encoded_header_fixture(&[32]);
        let header = &bytes[32..];
        for end in 0..header.len() - 2 {
            assert!(check_7z_encoded_header(&header[..end]).is_err(), "prefix {end}");
        }
        let mut external = header.to_vec();
        let folder_marker = external.windows(2).position(|w| w == [0x07, 0x0b]).unwrap();
        external[folder_marker + 2 + 9] = 1;
        assert!(check_7z_encoded_header(&external).is_err());
        let mut overflow = bytes.clone();
        overflow[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(check_7z_header(&mut std::io::Cursor::new(overflow)), Err(ArchiveError::LimitExceeded)));
        let mut missing = bytes;
        missing[8..32].fill(0);
        assert!(check_7z_header(&mut std::io::Cursor::new(missing)).is_err());
        let mut huge_count = SevenZHeader { bytes: &[0xff; 9] };
        assert!(huge_count.count().is_err());
    }

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
