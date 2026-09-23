use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::{
    formats, is_blocked_path, origin, safe_relative_path, ArchiveError, ArchiveFormat, EntryKind,
    EntryMeta, MAX_ENTRIES,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractStats {
    pub written_files: u32,
    /// Beim Zusammenfuehren: Datei existierte schon und wurde NICHT angefasst.
    pub existing_skipped: u32,
    /// Unsichere oder nicht anlegbare Eintraege (Zip-Slip, Links, geschuetzte
    /// Zielpfade, …).
    pub unsafe_skipped: u32,
    /// Gesperrte Dateitypen (ausfuehrbare Dateien, Verknuepfungen, …).
    pub blocked_skipped: u32,
}

/// Ergebnis eines erfolgreichen Entpackens. Haelt die Liste aller in diesem
/// Durchlauf neu angelegten Pfade, damit der Aufrufer bei einem SPAETEREN
/// Fehler (z.B. Datenbank) trotzdem vollstaendig aufraeumen kann.
#[derive(Debug)]
pub struct Extraction {
    pub stats: ExtractStats,
    created: Vec<PathBuf>,
}

impl Extraction {
    /// Entfernt alles, was dieser Durchlauf angelegt hat - in umgekehrter
    /// Reihenfolge, sodass Ordner erst nach ihrem Inhalt entfernt werden.
    /// Vorher vorhandene Dateien/Ordner werden nie angefasst.
    pub fn rollback(self) {
        remove_created(&self.created);
    }
}

fn remove_created(created: &[PathBuf]) {
    for path in created.iter().rev() {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => {
                let _ = fs::remove_dir(path);
            }
            Ok(_) => {
                let _ = fs::remove_file(path);
            }
            Err(_) => {}
        }
    }
}

/// Einzige Stelle, die beim Entpacken auf die Platte schreibt.
pub(super) struct Extractor<'g> {
    root: PathBuf,
    byte_limit: u64,
    written_bytes: u64,
    entries_seen: usize,
    /// `false` = Pfad liegt in einem geschuetzten Bereich (vom Aufrufer
    /// entschieden, siehe `commands::reject_if_sensitive_path`).
    guard: &'g dyn Fn(&Path) -> bool,
    /// Herkunftsmarkierung des Archivs, wird auf jede Datei uebertragen.
    origin_mark: Option<Vec<u8>>,
    created: Vec<PathBuf>,
    stats: ExtractStats,
}

impl<'g> Extractor<'g> {
    fn new(
        root: &Path,
        merge: bool,
        byte_limit: u64,
        guard: &'g dyn Fn(&Path) -> bool,
        origin_mark: Option<Vec<u8>>,
    ) -> Result<Self, ArchiveError> {
        if !guard(root) {
            return Err(ArchiveError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("Zielordner liegt in einem geschuetzten Bereich: {}", root.display()),
            )));
        }
        let mut extractor = Extractor {
            root: root.to_path_buf(),
            byte_limit,
            written_bytes: 0,
            entries_seen: 0,
            guard,
            origin_mark,
            created: Vec::new(),
            stats: ExtractStats::default(),
        };
        match fs::symlink_metadata(root) {
            Ok(meta) if merge && meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(ArchiveError::Io(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("Zielordner existiert bereits: {}", root.display()),
                )))
            }
            Err(_) => {
                fs::create_dir(root)?;
                extractor.created.push(root.to_path_buf());
            }
        }
        Ok(extractor)
    }

    /// Legt `rel` (relativ zu `root`) Ebene fuer Ebene an. `false`, wenn eine
    /// Ebene schon als Datei oder Symlink existiert oder geschuetzt ist - dann
    /// darf NICHTS darunter geschrieben werden (sonst koennte ein vom Nutzer
    /// angelegter Symlink im Zusammenfuehren-Ordner das Ziel umlenken).
    fn ensure_dir(&mut self, rel: &Path) -> Result<bool, ArchiveError> {
        let mut current = self.root.clone();
        for component in rel.components() {
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => return Ok(false),
                Ok(_) => {}
                Err(_) => {
                    if !(self.guard)(&current) {
                        return Ok(false);
                    }
                    fs::create_dir(&current)?;
                    self.created.push(current.clone());
                }
            }
        }
        Ok(true)
    }

    /// Zielpfad fuer einen Datei-Eintrag, oder `None`, wenn er uebersprungen
    /// wird. Ordner-Eintraege werden hier direkt angelegt. Zaehlt JEDEN
    /// Eintrag gegen `MAX_ENTRIES` - auch wenn `inspect` schon geprueft hat,
    /// denn das Archiv kann seitdem ausgetauscht worden sein.
    pub(super) fn prepare(&mut self, meta: &EntryMeta) -> Result<Option<PathBuf>, ArchiveError> {
        self.entries_seen += 1;
        if self.entries_seen > MAX_ENTRIES {
            return Err(ArchiveError::TooManyEntries);
        }
        let Some(rel) = safe_relative_path(&meta.name) else {
            self.stats.unsafe_skipped += 1;
            return Ok(None);
        };
        if meta.kind == EntryKind::Other {
            self.stats.unsafe_skipped += 1;
            return Ok(None);
        }
        if is_blocked_path(&rel) {
            self.stats.blocked_skipped += 1;
            return Ok(None);
        }
        if meta.kind == EntryKind::Directory {
            if !self.ensure_dir(&rel)? {
                self.stats.unsafe_skipped += 1;
            }
            return Ok(None);
        }
        if meta.encrypted {
            return Err(ArchiveError::Encrypted);
        }
        if let Some(parent) = rel.parent() {
            if !self.ensure_dir(parent)? {
                self.stats.unsafe_skipped += 1;
                return Ok(None);
            }
        }
        let target = self.root.join(&rel);
        if fs::symlink_metadata(&target).is_ok() {
            self.stats.existing_skipped += 1;
            return Ok(None);
        }
        if !(self.guard)(&target) {
            self.stats.unsafe_skipped += 1;
            return Ok(None);
        }
        Ok(Some(target))
    }

    /// Schreibt gestreamt und zaehlt die TATSAECHLICH geschriebenen Bytes
    /// gegen das Budget. `create_new` garantiert, dass nie etwas
    /// ueberschrieben und kein Symlink am Ziel verfolgt wird.
    pub(super) fn write_from(&mut self, target: &Path, reader: &mut dyn Read) -> Result<(), ArchiveError> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        // Nie ausfuehrbar, egal was das Archiv an Rechten mitbringt.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o644);
        let mut file = options.open(target)?;
        self.created.push(target.to_path_buf());
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.written_bytes += n as u64;
            if self.written_bytes > self.byte_limit {
                return Err(ArchiveError::LimitExceeded);
            }
            file.write_all(&buf[..n])?;
        }
        drop(file);
        self.finish_file(target);
        Ok(())
    }

    fn finish_file(&mut self, target: &Path) {
        if let Some(mark) = &self.origin_mark {
            origin::apply(target, mark);
        }
        self.stats.written_files += 1;
    }

    /// Zaehlt einen Eintrag, den ein Format-Adapter nach `prepare` doch nicht
    /// schreiben darf (z.B. RAR-Referenz-Eintrag ohne Daten).
    pub(super) fn skip_unsafe(&mut self) {
        self.stats.unsafe_skipped += 1;
    }

    /// Prueft, ob `size` Bytes innerhalb des verbleibenden Budgets passen.
    /// Wird VOR dem Schreiben aufgerufen, um Overrun zu verhindern.
    pub(super) fn ensure_budget_for(&self, size: u64) -> Result<(), ArchiveError> {
        if self.written_bytes.saturating_add(size) > self.byte_limit {
            return Err(ArchiveError::LimitExceeded);
        }
        Ok(())
    }
}

/// Entpackt `archive` nach `dest`. Bei `merge == false` darf `dest` noch
/// nicht existieren. `guard` wird fuer `dest` und jeden neu anzulegenden
/// Pfad gefragt (`false` = geschuetzt, nicht anlegen). Bei JEDEM Fehler wird
/// alles, was dieser Durchlauf angelegt hat, wieder entfernt, bevor der
/// Fehler zurueckkommt.
pub fn extract_archive(
    archive: &Path,
    format: ArchiveFormat,
    dest: &Path,
    merge: bool,
    byte_limit: u64,
    guard: &dyn Fn(&Path) -> bool,
) -> Result<Extraction, ArchiveError> {
    let mut extractor = Extractor::new(dest, merge, byte_limit, guard, origin::read(archive))?;
    match formats::extract(archive, format, &mut extractor) {
        Ok(()) => Ok(Extraction {
            stats: extractor.stats,
            created: extractor.created,
        }),
        Err(e) => {
            remove_created(&extractor.created);
            Err(e)
        }
    }
}
