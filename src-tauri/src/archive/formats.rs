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

fn unreadable(e: impl std::fmt::Display) -> ArchiveError {
    ArchiveError::Unreadable(e.to_string())
}

pub(super) fn list(path: &Path, format: ArchiveFormat) -> Result<Vec<EntryMeta>, ArchiveError> {
    match format {
        ArchiveFormat::Zip => list_zip(path),
        // Adapter fuer 7z und RAR folgen in Task 2.
        ArchiveFormat::SevenZ | ArchiveFormat::Rar => {
            Err(ArchiveError::Unsupported(format!("{format:?}")))
        }
        _ => list_tar(path, format),
    }
}

pub(super) fn extract(path: &Path, format: ArchiveFormat, ex: &mut Extractor<'_>) -> Result<(), ArchiveError> {
    match format {
        ArchiveFormat::Zip => extract_zip(path, ex),
        ArchiveFormat::SevenZ | ArchiveFormat::Rar => {
            Err(ArchiveError::Unsupported(format!("{format:?}")))
        }
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
