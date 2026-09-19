# Security Review — 3mf Katalog Manager

**Datum:** 2026-09-19
**Geprüfter Stand:** Commit `a568ca6` (master), Fixes in `800e373` + `0da319f`
**Methode:** Allgemeine, ISO/IEC 27001:2022/27002:2022-orientierte Bestandsaufnahme des gesamten
Codebase (nicht nur der letzten Änderungen) — Rust-Backend (`src-tauri/src/`, 9.271 Zeilen),
`tauri.conf.json`, `capabilities/`, komplettes React-Frontend. Kontrollzuordnung sinngemäß, nicht
als zertifizierte Konformitätsaussage.

---

## Zusammenfassung

| # | Befund | Schweregrad | Status |
|---|--------|-------------|--------|
| Z-1 | Slicer-Pfadliste aus fremdem Backup ungeprüft in `localStorage` → beliebiges Programm startbar | **Hoch** | ✅ Teil 1+2 behoben 2026-09-19, Teil 3 (Backend-Allowlist) zurückgestellt — siehe unten |
| I-1 | `files.name`/`path`/`trash_path` aus importiertem Katalog unvalidiert → beliebiges Schreiben/Löschen/Verschieben | **Hoch** | ✅ `name` + `trash_path` vollständig behoben (Containment); `path` nur Denylist — architektonisch bedingt, siehe unten |
| I-2 | `source_url` nur beim Schreiben validiert, nicht beim Lesen → `javascript:`-Link → Skriptausführung im App-Kontext | **Hoch** | ✅ behoben 2026-09-19 |
| V-1 | Google-OAuth-Client-Secret + API-Key im Klartext (ungetrackte Datei) | Mittel | ✅ Datei gelöscht 2026-09-19 (Widerruf der Credentials liegt beim Maintainer) |
| I-3 | Pfad-Sicherheitsprüfung ohne Kanonisierung — umgehbar via `../` oder Symlinks | Mittel | ✅ behoben 2026-09-19 |
| A-1 | Kein Größenlimit beim Entpacken von 3MF-/Backup-Zip-Einträgen (Zip-Bomb → OOM) | Mittel | ✅ behoben 2026-09-19 (inkl. Nachbesserung für zwei zunächst übersehene Einträge) |
| I-5 | `open_in_slicer`: `file_path` ohne Validierung als Argument übergeben | Niedrig | ✅ behoben 2026-09-19 |
| A-2 | `set_render_snapshot` ohne Größenlimit (inkonsistent zu Schwester-Commands) | Niedrig | ✅ behoben 2026-09-19 |
| Z-3 | `import_dropped` akzeptiert beliebige Pfade ohne Dialog | Niedrig | Kein Fix — durch Extension-Whitelist funktional begrenzt, akzeptiertes Restrisiko |
| F-1 | Interne Pfade in UI-Fehlermeldungen | Niedrig | Kein Fund — bewusst und funktional begründet |
| F-2 | Kein Audit-Log für Löschen/Export/Import | Info | Kein Fund — für Solo-Desktop-App vertretbar |

Kein Fund zu: SQL-Injection (durchgehend parametrisiert), Zip-Slip (keine pfadbasierte Extraktion),
Command-Injection-Grundkonstruktion (argv statt Shell-String), Tauri-Capabilities (Least-Privilege
bereits vorbildlich), CSP (bereits vorbildlich), Prototype Pollution, XSS via
`dangerouslySetInnerHTML`/`eval`, unsichere Deserialisierung/XXE, unsichere Default-Konfiguration.

---

## Kernbefund: `import_catalog` behandelte ein fremdes Backup nur teilweise als nicht vertrauenswürdig

Drei der vier Hoch-Findings (Z-1, I-1, I-2) hängen an derselben Ursache: ein importiertes
Katalog-Backup (ZIP mit `catalog.db` + `settings.json`) wurde nach dem Import punktuell, aber nicht
durchgängig als potenziell feindlich behandelt. `folders.path` war bereits seit dem Review vom
2026-09-18 geprüft, `files.*` und `settings.json` nicht.

## 1. Z-1 — Slicer-Startliste aus Backup (Hoch)

**Betroffen:** `src/hooks/useCatalogBackup.ts`, `src/hooks/useSlicers.ts`,
`src-tauri/src/commands.rs` (`open_in_slicer`, `validate_slicer_target_file`).
**CWE:** CWE-829 (Inclusion of Functionality from Untrusted Control Sphere)

`importCatalog()` schrieb die `settings.json` aus dem fremden ZIP ungeprüft in `localStorage` —
darunter `3mf-katalog-slicers`, die Liste startbarer externer Programme. `open_in_slicer` prüfte
nur, dass das Ziel eine existierende, ausführbare Datei ist (trifft z.B. auf `/bin/sh` zu).

**Exploit-Szenario:** Präpariertes Backup setzt einen Slicer-Eintrag auf `/bin/sh`. Klickt der
Nutzer bei irgendeinem Modell auf „In Slicer öffnen", wird `/bin/sh <modellpfad>` ausgeführt.

**Fix (2026-09-19, Commit `800e373`):**
1. `3mf-katalog-slicers` wird beim **Import** explizit übersprungen (`IMPORT_SKIPPED_SETTINGS_KEYS`
   in `useCatalogBackup.ts`) — Export bleibt unverändert, betrifft nur die Wiederherstellung.
2. Übrige Settings-Schlüssel (Theme, Sprache, Dichte, Anzeige-Präferenz) werden gegen eine
   Whitelist der tatsächlich gültigen Werte geprüft (`IMPORT_ALLOWED_SETTINGS_VALUES`); ungültige
   Werte werden übersprungen statt geschrieben, der Import schlägt dafür nicht fehl.
3. `open_in_slicer` erhielt zusätzlich die I-5-Prüfungen (siehe unten) für das Datei-Argument.

**Zurückgestellt:** ein vollständiges Backend-Allowlisting („nur ein per Dialog gewählter oder
automatisch erkannter Slicer-Pfad darf gestartet werden") würde eine größere Änderung erfordern
(persistente, nur Rust-seitig schreibbare Slicer-Liste). Da Teil 1 den einzigen bekannten
Einschleusungsweg (Import) schließt, ist das unmittelbare Risiko geschlossen — die Absicherung
hängt aber jetzt mit von I-2 ab (ohne einen weiteren `javascript:`/Script-Weg ins App-Origin gibt
es aktuell keinen Weg mehr, `localStorage` von außen zu beschreiben). **Empfehlung: als eigenes,
nicht dringendes Follow-up-Issue tracken.**

## 2. I-1 — Unvalidierte Datei-Metadaten aus importiertem Katalog (Hoch)

**Betroffen:** `src-tauri/src/commands.rs` (`validate_catalog_db_bytes`, `restore_file`,
`delete_file_permanently`, `empty_trash`, `purge_expired_trash_on_startup`).
**CWE:** CWE-22 (Path Traversal), CWE-829

`validate_catalog_db_bytes` validierte nur `folders.path`. Die `files`-Tabelle wurde nur auf
Existenz geprüft. `files.name`, `files.path` und `files.trash_path` gingen ungeprüft in
Dateisystemoperationen: `name` in einen Trash-Dateinamen-Join (Traversal → Schreiben an beliebigem
Ort), `trash_path` in `remove_file` (auch beim automatischen Start-Purge, ohne Nutzerinteraktion),
`path`+`trash_path` in `move_file` bei `restore_file`.

**Fix (2026-09-19, `800e373` + Nachbesserung `0da319f`):**
- `files.name`: neue `validate_file_name` (analog `validate_folder_name`) lehnt Pfadtrenner und
  `..` ab — schließt den Schreib-Vektor vollständig.
- `files.trash_path`: zusätzlich zur bestehenden Sensible-Verzeichnisse-Denylist jetzt echtes
  **Containment** (`reject_if_outside_trash_dir`) — der Pfad muss, symlink-sicher kanonisiert
  (dieselbe Auflösung wie beim I-3-Fix), innerhalb des echten Trash-Verzeichnisses liegen. Schließt
  den automatischen Lösch-Vektor vollständig.
- `files.path`: bleibt bei der Denylist-Prüfung. Ein echtes Containment ist hier architektonisch
  nicht möglich, ohne legitime Multi-Root-Kataloge zu brechen — die App unterstützt mehrere,
  unabhängig registrierte Katalog-Wurzelverzeichnisse (`register_catalog_base_dir` legt keinen
  einzelnen Stamm fest, `import_many_with_conn` scannt beliebige, pro Aufruf gewählte `roots`).
  Ein Containment-Check würde legitime Importe aus mehreren Ordnern über die Zeit hinweg ablehnen.
  **Empfehlung: eigenes Issue, sofern künftig ein einzelnes verpflichtendes Basisverzeichnis
  eingeführt wird — bis dahin bleibt hier nur die Denylist als Schutz.**

Fünf neue Tests (`commands.rs`) inkl. Nicht-Regressions-Test für harmlose Zeilen; drei weitere
Tests für die Trash-Containment (innerhalb/außerhalb/`..`-Escape).

## 3. I-2 — `source_url` beim Lesen nicht revalidiert (Hoch)

**Betroffen:** `src-tauri/src/commands.rs` (`to_dto`), `src/components/DetailPanel.tsx`,
`src/components/ModelDetailPage.tsx`.
**CWE:** CWE-79 (verwandt — hier über einen `javascript:`-URI-Sink statt HTML-Injection)

`validate_source_url` beschränkte nur `set_source_url` auf `http(s)://`. Ein importierter Wert
umging das und wurde beim Lesen unverändert als `<a href={sourceUrl}>` gerendert — ein
`javascript:`-Wert hätte beim Klick Code im App-Origin mit vollem IPC-Zugriff ausgeführt.

**Fix (2026-09-19):** doppelt abgesichert. Backend: `sanitize_source_url` filtert beim Lesen
(`to_dto`) — nicht-http(s)-Werte werden zu `None`. Frontend: neues `src/lib/safeUrl.ts`
(`isSafeHttpUrl`) prüft unabhängig vor jedem Rendern als Link an allen drei Stellen; ungültige
Werte werden als Text statt als Link dargestellt.

## 4. V-1 — Google-OAuth-Secret im Klartext (Mittel)

**Betroffen:** `src-tauri/cloud.config.json` (nie in Git, Rest der 2026-09-12 entfernten
Drive-Integration).

**Fix (2026-09-19):** Datei gelöscht (nichts im Code referenziert sie mehr). **Wichtig: die
enthaltenen Credentials (Client-Secret, Picker-API-Key) sollten in der Google Cloud Console
widerrufen/rotiert werden — das kann nicht automatisiert erledigt werden.**

## 5. I-3 — Pfadprüfung ohne Kanonisierung (Mittel)

**Betroffen:** `src-tauri/src/commands.rs` (`reject_if_sensitive_path`).

`Path::starts_with`-Vergleich ohne Normalisierung — umgehbar über `../`-Sequenzen oder Symlinks in
ein sensibles Verzeichnis.

**Fix (2026-09-19):** neue `resolve_path_for_sensitivity_check` — kanonisiert existierende Pfade
vollständig (löst auch Symlinks auf); bei noch nicht existierenden Pfaden werden literale
`..`-Komponenten abgelehnt und das längste existierende Elternverzeichnis kanonisiert, der Rest
literal angehängt. Legitime, noch nicht angelegte Zielverzeichnisse (z.B. beim Ersteinrichten eines
Katalog-Ordners) bleiben zulässig — mit Test abgedeckt.

## 6. A-1 — Kein Größenlimit beim Zip-Entpacken (Mittel)

**Betroffen:** `src-tauri/src/threemf/container.rs`, `plates.rs`, `slice_info.rs`,
`src-tauri/src/commands.rs` (`import_catalog`).

Unbegrenztes `read_to_string`/`read_to_end` auf Zip-Einträgen — eine kleine 3MF-/Backup-Datei kann
mehrere GB dekomprimieren (Zip-Bomb → OOM).

**Fix (2026-09-19, `800e373` + Nachbesserung `0da319f`):** deklarierte Zip-Eintragsgröße wird vor
jedem Lesen geprüft (`reject_oversized_entry`), zusätzlich als Absicherung gegen einen
manipulierten Header per `.take(max_bytes)` begrenzt gelesen. Grenzen: 256 MB Modell-XML, 16 MB
Thumbnails/Konfigurationsdateien, 256 MB `catalog.db`, 1 MB `settings.json`. Die Nachbesserung
schloss zwei zunächst übersehene Einträge im selben 3MF-Archiv (`Metadata/model_settings.config`,
`Metadata/slice_info.config`), die vor jeder anderen Prüfung gelesen wurden.

## 7. I-5 — `open_in_slicer`: `file_path` unvalidiert (Niedrig)

**Fix (2026-09-19):** neue `validate_slicer_target_file` — Datei muss existieren, Endung `.3mf`/
`.stl` haben, Dateiname darf nicht mit `-` beginnen (Flag-Injection-Schutz).

## 8. A-2 — `set_render_snapshot` ohne Größenlimit (Niedrig)

**Fix (2026-09-19):** dieselbe `MAX_CUSTOM_IMAGE_BYTES`-Grenze wie bei `upload_custom_image`,
geprüft an der Base64-Zeichenkettenlänge **vor** dem Dekodieren.

---

## Geprüft, unauffällig

- **SQL-Injection:** alle Queries parametrisiert, keine String-Konkatenation mit Nutzereingaben.
- **Zip-Slip:** weder `import_catalog` noch der 3MF-Parser extrahieren pfadbasiert auf die Platte.
- **Tauri-Capabilities:** `capabilities/default.json` gewährt exakt `core:default` +
  `dialog:default`, kein `fs:`/`shell:`/`http:`-Plugin — Least Privilege eingehalten.
- **CSP:** restriktiv, kein `unsafe-inline`/`unsafe-eval`.
- **Prototype Pollution:** keine `Object.assign`/Spread-Merges mit angreiferkontrollierten Schlüsseln
  aus `JSON.parse`-Ergebnissen.
- **XSS:** kein `dangerouslySetInnerHTML`/`innerHTML`/`eval` im gesamten Frontend.
- **`import_dropped`** (Z-3): akzeptiert beliebige Pfade ohne Dialog, aber durch
  Extension-Whitelist (`.3mf`/`.stl`) funktional auf Katalogisierung von Modelldateien begrenzt —
  akzeptiertes Restrisiko, kein Fix.
- **Fehlermeldungen mit internen Pfaden** (F-1): bewusst und für manuelle Reparatur bei
  fehlgeschlagenem Import notwendig — auf einem Einzelplatzsystem kein Information-Disclosure.
- **Fehlendes Audit-Log** (F-2): für eine Solo-Desktop-App nach ISO 27002 A.8.15 vertretbar.

## Offene Punkte für Folge-Issues

1. **Z-1 Teil 3:** Backend-seitiges Allowlisting für Slicer-Pfade (nur automatisch erkannte oder
   per Dialog gewählte Pfade), statt jedem `localStorage`-Wert zu vertrauen. Nicht dringend, da der
   einzige bekannte Einschleusungsweg bereits geschlossen ist.
2. **I-1 (`files.path`):** Containment auf ein einzelnes Katalog-Basisverzeichnis ist erst möglich,
   wenn ein solches Basisverzeichnis architektonisch verpflichtend eingeführt wird (aktuell:
   mehrere unabhängige Katalog-Wurzeln erlaubt). Bis dahin bleibt nur die Denylist als Schutz.

## Bezug zu früheren Reviews

- [Review 2026-09-18](security-review-2026-09-18.md): Finding 1 (Ordner-Pfade aus Backup) und
  Finding 2 (unsichere Temp-Dateien) bleiben wirksam behoben, keine Regression. Dieses Review
  erweitert den damaligen Fix um die bislang ungeprüften `files.*`-Spalten und `settings.json`.
- [Review 2026-09-11](security-review-2026-09-11.md): unverändert referenziert.
