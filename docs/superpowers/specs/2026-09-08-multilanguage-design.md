# Mehrsprachigkeit (i18n) — Design

**Datum:** 2026-09-08
**Status:** Zur Freigabe

## Ziel

Die Anwendung soll auf vier Sprachen umschaltbar sein: Deutsch (Standard), Englisch, Spanisch, Französisch. Der User wählt die Sprache manuell über einen neuen Abschnitt im Einstellungen-Panel — es gibt keine automatische Systemsprachenerkennung (im Gegensatz zum Theme-Switch, der einen „System"-Modus kennt).

## Nicht-Ziele

- Keine automatische Spracherkennung/„System"-Option.
- Keine Übersetzung von Backend-Fehlermeldungen (aktuell nirgends user-facing angezeigt).
- Keine neue automatisierte Test-Infrastruktur im Frontend (Projekt hat bisher keine).
- Keine Einführung einer i18n-Bibliothek (react-i18next, react-intl) — custom Lösung.

## Abschnitt 1 — Architektur & Dateistruktur

Neues Verzeichnis `src/i18n/`:

```
src/i18n/
  types.ts          // Translations-Interface, Language-Typ
  de.ts              // Referenz-Wörterbuch (Deutsch)
  en.ts
  es.ts
  fr.ts
  format.ts          // Intl-basierte Datums-/Zahlen-/Volumen-Formatierung
  LanguageContext.tsx // Provider + useLanguage() + useT()
```

**`useLanguage()`** spiegelt das bestehende Muster aus `src/hooks/useTheme.ts`:
- localStorage-Key: `3mf-katalog-language`
- Default: `'de'`
- Kein „System"-Wert (anders als Theme)
- API: `{ language, setLanguage }`

**Unterschied zu Theme:** Sprache wird über **React Context** bereitgestellt (nicht wie Theme nur an `Header` durchgereicht), weil praktisch jede Komponente Übersetzungen braucht. `useT()` liest den Context und liefert eine `t(key: keyof Translations) => string`-Funktion.

**Locale-Mapping für `Intl`-APIs:**
| Sprache | BCP-47 Locale |
|---|---|
| de | de-DE |
| en | en-US |
| es | es-ES |
| fr | fr-FR |

## Abschnitt 2 — Backend-Änderungen (`src-tauri`)

`ModelFileDto` (in `commands.rs`) wird von vorformatierten auf rohe Felder umgestellt:

**Entfernt:**
- `sync_time_label: String`
- `volume_label: String`
- `filesize_label: String`
- `meta: Vec<MetaRow>` (und der `MetaRow`-Typ selbst)

**Neu hinzugefügt (roh, unformatiert):**
- `dimensions_mm: Option<[f64; 3]>`
- `volume_cm3: Option<f64>`
- `object_count: Option<i64>`
- `materials: Vec<MaterialDto>` mit `{ name: String, display_color: Option<String> }`

**Wiederverwendet** (bereits vorhanden aus der Sortier-Implementierung):
- `file_size_bytes: i64`
- `imported_at: String`

**`format.rs` wird vollständig gelöscht** (Datei + alle Tests), da nach der Umstellung kein Aufrufer mehr existiert. Die dort enthaltene Formatierungslogik (Bytes, Volumen, Dimensionen, Datum, relative Zeit) wird äquivalent — aber locale-abhängig — in `src/i18n/format.ts` neu implementiert, basierend auf `Intl.NumberFormat`, `Intl.DateTimeFormat` und `Intl.RelativeTimeFormat`.

Entsprechend wird `src/types/index.ts`s `ModelFile`-Interface analog angepasst: `syncTimeLabel`/`volumeLabel`/`filesizeLabel`/`meta` weichen den rohen Feldern `dimensionsMm`, `volumeCm3`, `objectCount`, `materials`.

## Abschnitt 3 — Frontend-Integration & String-Migration

**Sprache-Umschalter:** Neue Sektion „Sprache" im Einstellungen-Panel (`Header.tsx`), direkt unter „Erscheinungsbild", als 2×2-Button-Raster (bestehende `segBase`/`segActive`/`segInactive`-Klassen, im Grid statt in einer Reihe). Sprachnamen bleiben immer im Original („Deutsch", „English", „Español", „Français") — UX-Konvention für Sprachauswahl, unabhängig von der aktiven UI-Sprache.

**String-Migration:** Alle hartcodierten deutschen Strings wandern als Keys in die vier Wörterbücher. Mehrfach vorkommende Strings bekommen jeweils **einen gemeinsamen Key** statt Duplikate:
- Sync-Status-Labels (`synced`/`outdated`/`local-only`/`cloud-only`) — bisher zwei leicht unterschiedliche Maps in `ModelList.tsx` und `DetailPanel.tsx`, werden zu einem gemeinsamen Key-Satz zusammengeführt.
- „Abbrechen"/„Löschen"/„In Slicer öffnen" — bisher dupliziert zwischen `DetailPanel.tsx` und `ContextMenu.tsx`, werden zu gemeinsamen Keys.

**Betroffene Komponenten** (vollständige Inventur aus der Exploration):
- `Header.tsx`: Import-Buttons, Sortier-Optionen, Ansicht-Umschalter, Dateizähler, Einstellungen-Panel-Texte.
- `Sidebar.tsx`: Suchfeld-Platzhalter, Ordner-/Tags-/Cloud-Konten-Überschriften, Cloud-Status-Labels.
- `ModelGrid.tsx`: „3D Vorschau"-Overlay-Text.
- `ModelList.tsx`: Spaltenüberschriften, Sync-Status-Labels (gemeinsamer Key, s.o.).
- `DetailPanel.tsx`: Leerzustand-Text, Sync-Status-Labels (gemeinsamer Key), Viewer-Hinweis, Metadaten-Überschrift und -Zeilenlabels, Hashtags-Bereich, Löschbestätigung, Buttons.
- `ContextMenu.tsx`: Löschbestätigung, Buttons (gemeinsame Keys mit `DetailPanel.tsx`).
- `ModelViewer.tsx`: Lade-/Fehlertext.

**Metadaten-Zeilen:** Werden nicht mehr vom Backend geliefert, sondern im Frontend über eine `buildMetaRows(model, t, locale)`-Hilfsfunktion in `DetailPanel.tsx` aus den rohen DTO-Feldern zusammengesetzt.

**Pluralisierung:** Einziger Fall ist „{count} Dateien" im Header. Jedes Wörterbuch definiert `filesCount: { one: string; other: string }`; eine `formatCount(dict, n)`-Funktion wählt die passende Form (einfache eins/mehr-Unterscheidung genügt für alle vier Sprachen, kein volles ICU-Pluralsystem nötig).

**Außerhalb des Scopes:** Backend-Fehlermeldungen (`Err(String)` aus Tauri-Commands) werden aktuell nirgends in der UI angezeigt und bleiben unangetastet.

## Abschnitt 4 — Testplan

**Backend (Rust):**
- `format.rs` inkl. aller Unit-Tests wird komplett gelöscht.
- Tests in `commands.rs`/DB-Layer, die auf alte DTO-Felder zugreifen, werden auf die neuen Rohfelder angepasst.
- Keine neuen Rust-Tests — Formatierungslogik liegt jetzt vollständig im Frontend.

**Frontend (TypeScript):**
- Ein `Translations`-Interface (aus `de.ts` als Referenz abgeleitet) muss von `en.ts`/`es.ts`/`fr.ts` implementiert werden — fehlende Keys sind ein Compile-Fehler, kein Laufzeit-Fallback.
- Manueller Test im Browser: Sprachumschaltung in allen vier Sprachen, Metadaten-Formatierung (Datum, Volumen, Dateigröße) je Sprache stichprobenartig, Pluralisierung bei 0/1/mehreren Dateien, localStorage-Persistenz über Reload.
- Keine neue automatisierte Test-Infrastruktur.

## Offene Entscheidungen

Keine — alle vier Abschnitte wurden vom User bestätigt.
