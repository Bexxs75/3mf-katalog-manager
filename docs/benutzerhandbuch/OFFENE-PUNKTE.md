# Offene Punkte zum Auftrag DOC-1

## Rückweg mit automatischer Sicherung über den Katalog-Import

Der verlangte direkte Import einer automatischen `.db`-Sicherung ist im aktuellen Code nicht möglich:

- `src-tauri/src/updater/backup.rs`: Update- und Schema-Sicherungen sind einzelne SQLite-Dateien.
- `src-tauri/src/commands/backup.rs`, `import_catalog`: Der Dateidialog akzeptiert nur ZIP-Dateien.
- `read_catalog_archive` verlangt darin `catalog.db` und `settings.json`.
- `replace_catalog_db_with_copy_fn` migriert die eingehende Datenbank auf das Schema der laufenden App. Ein Import in die neuere App stellt daher kein älteres Schema wieder her.

**Dokumentierte Abweichung:** Das Handbuch beschreibt den ZIP-Katalog-Import Schritt für Schritt als eigenen Wiederherstellungsweg. Für automatische Datenbankkopien bleibt der manuelle Rückweg bei vollständig geschlossener App beschrieben. Ein direkter `.db`-Import wird nicht behauptet. App-Code wurde nicht geändert. Eine künftige Importfunktion für automatische Sicherungen wäre ein separater Produktauftrag.

## Aufbewahrung: Changelog und Code stimmen überein

Der Changelog-Eintrag zur Sicherung vor Migration nennt inzwischen zwei Kopien, wie `src-tauri/src/updater/backup.rs` mit `KEEP_SCHEMA = 2`. Das Handbuch folgt dem Code: drei normale Update-Sicherungen, älteste und neueste Sicherung ausgehend von einer stabilen Version, zwei Schema-Sicherungen. Die gerade erzeugte Kopie ist geschützt; fremde Dateien werden nicht bereinigt. Die Bereinigung erkennt auch das alte Update-Namensformat.

`src-tauri/src/db/repository.rs::backup_before_migration` protokolliert einen Kopierfehler und setzt den Start fort. Das Handbuch beschreibt die Schema-Sicherung deshalb nicht als garantierte Startsperre bei Fehlern.

## Tatsächliche Beschriftungen und Zuständigkeiten

- `src/i18n/de.ts`: Der Filterknopf heißt "Filter zurücksetzen"; die Ordneraktion heißt "Alle Ordner zuklappen".
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

Die Bildverweise des Handbuchs (`bilder/` für Deutsch, `bilder/en/` für Englisch, Nummern 01 bis 45) werden aus dem Screenshot-Werkzeug unter `tools/screens` erzeugt und sind im Handbuch eingebunden. `NEUE-BILDER.md` stammt aus der Zeit vor dieser Erzeugung, nennt nur die Motive 33 bis 38 und ist damit überholt. Das Demo-GIF der README (`docs/assets/demo.gif`) hat laut `tools/screens/manifest.json` noch kein Rezept.

## Prüfstand der Texte zu 0.16.0

- Der Hinweistext zum Entfernen im Festplatten-Abgleich (`diskSyncRemoveHint`) nennt „Check catalog for issues“, der Knopf in den Einstellungen heißt aber „Scan catalog for issues“ (DE in beiden Fällen „Katalog auf Probleme prüfen“). Das Handbuch verweist deshalb auf die „Katalogprüfung (Einstellungen → Katalog)“. Die englische Abweichung liegt im App-Text, nicht im Handbuch.
- Die Schwellen für „zu groß für die 3D-Ansicht“: Alle Schwellen sind im Code belegt: 3 Mio. Dreiecke bei 3MF (`src-tauri/src/threemf/model_xml.rs`, `MAX_PREVIEW_TRIANGLES`) sowie 3 Mio. Dreiecke binär bei STL, 256 MiB bei ASCII-STL und 128 MiB bei OBJ (`src-tauri/src/commands/files.rs`, `MAX_PREVIEW_TRIANGLES`, `MAX_ASCII_STL_PREVIEW_BYTES`, `MAX_OBJ_PREVIEW_BYTES`).

