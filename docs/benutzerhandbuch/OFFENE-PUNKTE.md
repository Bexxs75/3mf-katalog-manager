# Offene Punkte zum Auftrag DOC-1

## Rückweg mit automatischer Sicherung über den Katalog-Import

Der verlangte direkte Import einer automatischen `.db`-Sicherung ist im aktuellen Code nicht möglich:

- `src-tauri/src/updater/backup.rs`: Update- und Schema-Sicherungen sind einzelne SQLite-Dateien.
- `src-tauri/src/commands/backup.rs`, `import_catalog`: Der Dateidialog akzeptiert nur ZIP-Dateien.
- `read_catalog_archive` verlangt darin `catalog.db` und `settings.json`.
- `replace_catalog_db_with_copy_fn` migriert die eingehende Datenbank auf das Schema der laufenden App. Ein Import in die neuere App stellt daher kein älteres Schema wieder her.

**Dokumentierte Abweichung:** Das Handbuch beschreibt den ZIP-Katalog-Import Schritt für Schritt als eigenen Wiederherstellungsweg. Für automatische Datenbankkopien bleibt der manuelle Rückweg bei vollständig geschlossener App beschrieben. Ein direkter `.db`-Import wird nicht behauptet. App-Code wurde nicht geändert. Eine künftige Importfunktion für automatische Sicherungen wäre ein separater Produktauftrag.

## Aufbewahrung: Changelog und Code unterscheiden sich

Der englische Unreleased-Eintrag zur Sicherung vor Migration nennt drei Kopien. `src-tauri/src/updater/backup.rs` setzt hingegen `KEEP_SCHEMA = 2`. Das Handbuch folgt wie im Auftrag verlangt dem Code: drei normale Update-Sicherungen, älteste und neueste Sicherung ausgehend von einer stabilen Version, zwei Schema-Sicherungen. Die gerade erzeugte Kopie ist geschützt; fremde Dateien werden nicht bereinigt. Die Bereinigung erkennt auch das alte Update-Namensformat.

`src-tauri/src/db/repository.rs::backup_before_migration` protokolliert einen Kopierfehler und setzt den Start fort. Das Handbuch beschreibt die Schema-Sicherung deshalb nicht als garantierte Startsperre bei Fehlern.

## Tatsächliche Beschriftungen und Zuständigkeiten

- `src/i18n/de.ts`: Der Filterknopf heißt "Alle löschen", nicht "Alle zurücksetzen"; die Ordneraktion heißt "Alle Ordner zuklappen".
- `src/components/PrinterManagerView.tsx`, `PrinterLinkControl.tsx`: Verwaltung und Verbindung liegen im Printer Manager. Der globale Schalter heißt "Anbindung" / "Connection".
- `src/components/FilamentView.tsx`, `PrinterColumn.tsx`: Material einlegen und neue Drucke bestätigen bleiben im Material Manager. Seine Verwaltungslinks führen zum Printer Manager.
- `src/components/Rail.tsx`: Vier Einstellungsreiter; der Verweis zum Printer Manager steht unter Allgemein.

## Belegübersicht für die übrigen Ergänzungen

| Thema | Codebelege relativ zum Checkout |
|---|---|
| Navigation, Kopfzeile, Detailbereich | `src/App.tsx`, `src/components/Rail.tsx`, `Header.tsx`, `StatusBadges.tsx`, `src/hooks/useDetailPanel.ts` |
| Filter, Sortierung, Tags, Ordner, Sammlungen | `src/hooks/useCatalogFilters.ts`, `src/lib/catalogFilters.ts`, `src/components/Header.tsx`, `Sidebar.tsx`, `TagInput.tsx`, `FolderTree.tsx`, `CollectionsGallery.tsx` |
| Detailnavigation, Farben, Platten, Fehler, Dateimanager | `src/components/ModelDetailPage.tsx`, `ModelViewer.tsx`, `PlateSelector.tsx`, `ViewerErrorCard.tsx`, `RevealFileButton.tsx`, `DetailPanel.tsx`, `ContextMenu.tsx`; `src/hooks/useDetailNavigation.ts`; `src-tauri/src/threemf/plates.rs`, `threemf/mod.rs` |
| Import und Ziehen | `src/components/Header.tsx`, `ImportProgressRow.tsx`, `ImportResultDialog.tsx`, `CatalogSetupDialog.tsx`, `DragGrip.tsx`, `DragGhost.tsx`, `FolderTree.tsx`; `src/hooks/useFileImport.ts` |
| Tastatur | `src/hooks/useKeyboardShortcuts.ts`, `useSingleKeyShortcuts.ts`; `src/components/Rail.tsx`, `Sidebar.tsx`, `Header.tsx` |
| Update-Kanal und Einzelinstanz | `src/components/UpdateChannelControl.tsx`; `src-tauri/src/lib.rs`, `single_instance.rs` |
| Sicherung und Import | `src-tauri/src/updater/backup.rs`, `db/repository.rs`, `commands/backup.rs`; `src/hooks/useCatalogBackup.ts`; `src/components/Rail.tsx` |
| Schrift und Slicer | `src/styles/theme.css`; `src-tauri/src/slicers.rs` |
| Beschriftungen und Plattformvarianten | `src/i18n/en.ts`, `de.ts`, `LanguageContext.tsx` |

Die vorhandenen Screenshots bleiben referenziert und müssen später aktualisiert werden. Die zwölf neuen Bildverweise sind in `NEUE-BILDER.md` erfasst; Bilddateien oder Platzhalter wurden nicht angelegt.
