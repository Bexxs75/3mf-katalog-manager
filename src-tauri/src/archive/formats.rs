//! Format-Adapter: pro Format "auflisten" und "an den Extractor streamen".
//! Die Adapter kennen keine Zielpfade und schreiben nie selbst - mit
//! Ausnahme von RAR, dessen Bibliothek nur in eine Datei entpacken kann;
//! den Zielpfad bestimmt aber auch dort der `Extractor`.

use std::fs::File;
use std::cell::Cell;
use std::io::{BufReader, Read};
use std::rc::Rc;
use std::path::Path;

use super::extract::Extractor;
use super::{ArchiveError, ArchiveFormat, EntryKind, EntryMeta, MAX_ENTRIES, MAX_UNPACKED_BYTES};

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;
/// Windows-Attribut FILE_ATTRIBUTE_REPARSE_POINT (Symlinks/Junctions).
const WIN_REPARSE_POINT: u32 = 0x400;
/// 7-Zip-Konvention: Bit 15 gesetzt = obere 16 Bit enthalten den Unix-Modus.
const SEVENZ_UNIX_EXTENSION: u32 = 0x8000;

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

/// Nur gaengige Verfahren. LZMA/XZ in ZIP sind in Modell-Downloads
/// praktisch nie anzutreffen, ihre Dekoder reservieren aber Speicher nach
/// der Woerterbuchgroesse im Header (bis 4 GB) - ein praepariertes Archiv
/// koennte die App so zum Absturz bringen.
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
        // by_index_raw entschluesselt/dekomprimiert nicht - funktioniert
        // daher auch bei verschluesselten Eintraegen.
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

/// Obergrenze fuer den DEKOMPRIMIERTEN tar-Datenstrom: Nutzdaten plus
/// grosszuegig bemessene Header/Padding-Bloecke. Begrenzt auch Daten, die
/// gar nicht geschrieben werden (uebersprungene Eintraege, Auflisten in
/// `inspect`) - sonst koennte eine "Kompressionsbombe" die App minutenlang
/// beschaeftigen, ohne dass das Schreib-Budget je greift.
const TAR_STREAM_LIMIT: u64 = MAX_UNPACKED_BYTES + (MAX_ENTRIES as u64 + 16) * 4096;
/// Speichergrenze fuer den xz-Dekoder (KiB). `xz -9` braucht ~65 MiB.
const XZ_MEM_LIMIT_KIB: u32 = 256 * 1024;

/// Liest hoechstens `remaining` Bytes und merkt sich eine Ueberschreitung,
/// damit sie trotz der Fehler-Verpackung im tar-Crate erkennbar bleibt.
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
        other => unreachable!("kein tar-Format: {other:?}"),
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
    // LZMA/LZMA2 reservieren Speicher bis zur entpackten Groesse des Blocks
    // (hoechstens Woerterbuchgroesse). Deshalb die Blockgroessen VOR dem
    // Dekodieren begrenzen - `sevenz-rust2` selbst hat kein Speicherlimit.
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
    // Die Bibliothek erwartet einen eigenen Fehlertyp im Callback - unseren
    // Fehler parken wir hier und brechen mit Ok(false) ab.
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

/// Unrar begrenzt die Ausgabe je Eintrag auf die Header-Groesse; zusammen
/// mit der Header-Pruefung in `inspect` und `account_written` bleibt das
/// Byte-Budget auch hier eingehalten.
fn extract_rar(path: &Path, ex: &mut Extractor<'_>) -> Result<(), ArchiveError> {
    reject_multipart(path)?;
    let mut archive = unrar::Archive::new(path).open_for_processing().map_err(rar_error)?;
    while let Some(header) = archive.read_header().map_err(rar_error)? {
        let meta = rar_meta(header.entry());
        archive = match ex.prepare(&meta)? {
            Some(target) => {
                ex.mark_created(&target);
                let next = header.extract_to(&target).map_err(rar_error)?;
                ex.account_written(&target)?;
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
}
