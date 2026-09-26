# Security-Fix-Report — Review vom 2026-09-19

Behoben: 7 Findings (I-1, I-2, I-3, I-5, A-1, A-2, Z-1). Gemeinsame Wurzel:
`import_catalog` behandelte ein fremdes Backup-ZIP nur teilweise als
nicht vertrauenswürdig.

---

## I-1 (High) — `files.name` / `files.path` / `files.trash_path` aus importierter DB ungeprüft

**Geändert:** `src-tauri/src/commands.rs`

- Neu: `validate_file_name()` (nach `validate_folder_name`, ~Zeile 744) —
  Gegenstück zu `validate_folder_name` mit identischem Fehlerstil
  (deutsche Meldung, `CmdResult<()>`): lehnt leere Namen, `/`, `\` und jede
  `..`-Folge ab.
- `validate_catalog_db_bytes()` (~Zeile 2210) prüft jetzt zusätzlich zur
  bestehenden `folders.path`-Schleife:
  ```sql
  SELECT name, path, trash_path FROM files
  ```
  - `name` → `validate_file_name` → Fehler `"Datei-Eintrag im Archiv abgelehnt: …"`
  - `path` → `reject_if_sensitive_path_expanded` → `"Datei-Eintrag im Archiv abgelehnt: …"`
  - `trash_path` (nur wenn non-null und nicht leer) → `"Papierkorb-Eintrag im Archiv abgelehnt: …"`

  Der gesamte Import scheitert bei der ersten schlechten Zeile — exakt das
  Muster, das `folders.path` heute schon verwendet (`?`-Propagation aus dem
  `and_then`-Closure, umgehüllt in `"Archiv enthält keine gültige
  Katalog-Datenbank: …"`).

**Warum korrekt:** die drei Werte waren die einzigen ungeprüften Eingaben in
`trash_dir.join(format!("{id}-{name}"))` (delete_file, cleanup_missing_files),
`fs::remove_file(trash_path)` (delete_file_permanently, empty_trash,
`purge_expired_trash_on_startup` — läuft beim App-Start ohne jede
Nutzer-Interaktion) und `move_file(trash_path → file.path)` (restore_file).
Die Prüfung sitzt an der Vertrauensgrenze, vor dem DB-Austausch.

**Performance-Detail:** `reject_if_sensitive_path` kanonisiert jetzt (I-3).
Damit das nicht O(Zeilen × geschützte-Ordner) Syscalls kostet, wurde die
Aufbereitung der geschützten Ordner in `expand_sensitive_dirs()` gezogen und
in `validate_catalog_db_bytes` einmal vorberechnet.

**Tests:**
- `validate_file_name_rejects_separators_and_parent_dir_sequences`
- `validate_catalog_db_bytes_accepts_harmless_file_rows` (Nicht-Regression)
- `validate_catalog_db_bytes_rejects_file_name_with_path_traversal`
- `validate_catalog_db_bytes_rejects_file_path_in_sensitive_directory`
- `validate_catalog_db_bytes_rejects_trash_path_in_sensitive_directory`

---

## I-2 (High) — `source_url` beim Lesen nicht erneut validiert

**Backend** — `src-tauri/src/commands.rs`

- `validate_source_url` nutzt jetzt den extrahierten Helfer `is_http_url()`
  (identische Semantik wie vorher, kein Verhaltenswechsel).
- Neu: `sanitize_source_url(Option<String>) -> Option<String>` — die Lese-
  Variante: verwirft still statt zu scheitern.
- `to_dto()` (Zeile 341): `source_url: file.source_url` → `source_url: sanitize_source_url(file.source_url)`.

**Frontend**

- Neu: `src/lib/safeUrl.ts` mit `isSafeHttpUrl(url): url is string`
  (Type-Guard, damit `href={model.sourceUrl}` weiterhin typisiert bleibt).
- `src/components/DetailPanel.tsx` (2 Stellen, ~233 und ~439): aus
  `model.sourceUrl ? <a href=…> : <span>noValue</span>` wird
  `isSafeHttpUrl(...) ? <a> : model.sourceUrl ? <span>Klartext</span> : <span>noValue</span>`.
- `src/components/ModelDetailPage.tsx` (~195): `<a>` nur noch bei
  `isSafeHttpUrl`, sonst Klartext-`<span>`; der ✎-Button bleibt in beiden
  Fällen erhalten (Nutzer kann den Müllwert überschreiben).

**Warum korrekt:** doppelte Schranke. Ein `javascript:`-Wert, der per
Katalog-Import an `set_source_url` vorbei in die DB kam, erreicht das
Frontend nicht mehr; und selbst wenn er es täte, wird er nie als `href`
gerendert und kann somit nicht im App-Origin mit IPC-Zugriff ausgeführt
werden.

**Tests:** `sanitize_source_url_drops_non_http_values_instead_of_erroring`,
`to_dto_drops_a_non_http_source_url_from_an_imported_row`,
`to_dto_keeps_a_regular_http_source_url` (Rust);
`src/lib/safeUrl.test.ts` (3 Fälle, TS).

---

## I-3 (Medium) — Pfadprüfung ohne Kanonisierung

**Geändert:** `src-tauri/src/commands.rs` (~Zeile 779–858)

Vorher:
```rust
fn reject_if_sensitive_path(path: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    for dir in sensitive_dirs {
        if path == dir.as_path() || path.starts_with(dir) { return Err(...) }
    }
    Ok(())
}
```

Nachher — Signatur und Aufrufstellen unverändert, drei Funktionen:

1. `reject_if_sensitive_path(path, sensitive_dirs)` — delegiert an
   `reject_if_sensitive_path_expanded(path, &expand_sensitive_dirs(dirs))`.
2. `expand_sensitive_dirs(dirs)` — ergänzt jeden geschützten Ordner um seine
   kanonische Schreibweise (auf Linux ist `/bin` meist ein Symlink nach
   `/usr/bin`; beide Schreibweisen sollen greifen). Einmal vorberechenbar.
3. `resolve_path_for_sensitivity_check(path)` — der eigentliche Fix:
   - existiert der Pfad → `canonicalize()` (löst Symlinks **und** `..`);
   - existiert er nicht → jede literale `..`-Komponente wird abgelehnt
     (`Path::components()`, kein Dateisystemzugriff nötig), danach wird der
     **längste bereits existierende Vorfahre** kanonisiert und die restlichen
     Komponenten wörtlich angehängt.

**Warum korrekt und ohne Regression:** die wichtigste legitime Aufrufstelle
ist ein noch nicht existierendes Ziel (frisches Katalog-Basisverzeichnis,
neuer Ordner, Verschiebe-Ziel). Genau dieser Fall bleibt erlaubt — er enthält
kein `..`, und der existierende Vorfahre wird nur zur Auflösung genutzt, nicht
als Existenz-Anforderung. Der bestehende Test
`reject_if_sensitive_path_rejects_exact_match_and_descendants_but_not_siblings`
(rein hypothetische, nicht existierende Pfade) läuft unverändert durch.

**Tests:**
- `reject_if_sensitive_path_rejects_parent_dir_traversal_into_sensitive_dir`
  (prüft zugleich, dass ein regulärer neuer Unterordner weiter erlaubt ist)
- `reject_if_sensitive_path_rejects_symlink_into_sensitive_dir` (`#[cfg(unix)]`)
- `reject_if_sensitive_path_allows_a_not_yet_existing_fresh_catalog_dir`
  (Nicht-Regression für den Einrichtungs-Flow)

---

## I-5 (Low) — `open_in_slicer`s `file_path` ungeprüft

**Geändert:** `src-tauri/src/commands.rs` — neu `validate_slicer_target_file()`
direkt vor `open_in_slicer`, dort als zweiter Schritt nach
`validate_slicer_path(&slicer_path)?` aufgerufen.

Prüft in dieser Reihenfolge (gleicher Fehlerstil wie `validate_slicer_path`:
kurze englische Meldung, `CmdResult<()>`):
1. Pfad ODER Dateiname beginnt mit `-` → abgelehnt (sonst deutet der Slicer
   das Argument als CLI-Flag).
2. Endung nicht `.3mf`/`.stl` → abgelehnt; nutzt die **bestehende**
   `is_supported_extension()` (bereits case-insensitiv).
3. Datei existiert nicht bzw. ist keine reguläre Datei → abgelehnt.

**Test:** `validate_slicer_target_file_rejects_flags_unsupported_types_and_missing_files`
(7 Fälle: `.3mf` ok, `.STL` ok, `.txt` abgelehnt, `--export-gcode.3mf`
abgelehnt, `--version` abgelehnt, fehlende Datei abgelehnt, Verzeichnis abgelehnt).

**Angepasster Bestandstest:** `open_in_slicer_spawns_successfully_for_a_real_executable`
übergab bisher den Fantasie-Pfad `/tmp/model.3mf`. Er legt jetzt eine echte
temporäre `.3mf`-Datei an. Der Test prüft weiterhin dasselbe (erfolgreicher
Spawn eines echten Binaries) — nur mit gültiger Eingabe statt mit einer, die
nach dem Fix zu Recht abgelehnt wird.

---

## A-1 (Medium) — keine Größenbegrenzung beim Entpacken von ZIP-Einträgen

**`src-tauri/src/threemf/container.rs`**

- Neue Konstanten (Zeile 20–26): `MAX_MODEL_XML_BYTES` 256 MB,
  `MAX_THUMBNAIL_BYTES` 16 MB, `MAX_RELS_XML_BYTES` 16 MB.
- `read_entry_to_string` / `read_entry_to_bytes` bekommen einen
  `max_bytes: u64`-Parameter und wenden **zwei** Schranken an:
  1. `reject_oversized_entry(path, file.size(), max_bytes)` — die im Archiv
     deklarierte entpackte Größe; spart den Leseversuch und liefert eine
     brauchbare Fehlermeldung;
  2. `file.take(max_bytes)` — die eigentliche harte Grenze, da die deklarierte
     Größe aus dem Archiv stammt und lügen kann.
- Alle 6 Aufrufstellen (Zeilen 61, 62, 79, 97, 100, 139) übergeben das passende
  Limit.

**`src-tauri/src/threemf/error.rs`** — neue Variante
`ThreeMfError::EntryTooLarge { path, size, max }` samt `Display`-Arm; fügt sich
in das bestehende `Result<_, ThreeMfError>`-Muster ein.

**`src-tauri/src/commands.rs`** — `import_catalog`:
- neue Konstanten `MAX_IMPORT_DB_BYTES` (256 MB) und
  `MAX_IMPORT_SETTINGS_BYTES` (1 MB);
- neuer Helfer `reject_oversized_zip_entry(name, size, max) -> CmdResult<()>`
  (deutsche Fehlermeldung wie die umliegenden Import-Fehler);
- beide `read_to_end`-Aufrufe (`catalog.db`, `settings.json`) laufen jetzt über
  `size()`-Prüfung + `.take(limit)`.

**Tests:** `read_entry_rejects_an_entry_exceeding_the_size_limit` (container.rs,
baut ein Mini-Zip mit stark komprimierbarem Eintrag; prüft Ablehnung über dem
Limit für beide Lesefunktionen und ungestörten Erfolg darunter);
`reject_oversized_zip_entry_uses_the_declared_uncompressed_size` (commands.rs,
Grenzwerte 10/100/101 gegen 100).

---

## A-2 (Low) — `set_render_snapshot` ohne Größenlimit

**Geändert:** `src-tauri/src/commands.rs` — `set_render_snapshot` (~Zeile 1206)
prüft jetzt dieselbe Obergrenze wie `upload_custom_image` und
`add_print_log_entry`:
- zuerst die **Base64-Länge** gegen die neue Konstante
  `MAX_RENDER_SNAPSHOT_BASE64_BYTES` (= `MAX_CUSTOM_IMAGE_BYTES / 3 * 4 + 4`),
  damit ein überdimensionierter String gar nicht erst dekodiert wird;
- danach die dekodierte Länge gegen `MAX_CUSTOM_IMAGE_BYTES`, mit exakt der
  bestehenden Fehlermeldung (`"Bild ist zu groß ({:.1} MB) - maximal {} MB erlaubt"`).

**Kein Verhaltenswechsel für echte Nutzung:** die Snapshots stammen aus
`ModelViewer` (`renderer.domElement.toDataURL('image/png')`) auf einer Canvas
in Container-Größe (Detailpanel ~336 px bzw. 4:3-Box) — Größenordnung
einige hundert KB, weit unter 5 MB.

---

## Z-1 (High) — importierte Einstellungen (v.a. Slicer-Pfade) ohne Validierung wiederhergestellt

**Teil 1 — `src/hooks/useCatalogBackup.ts`:** `3mf-katalog-slicers` wird beim
**Import** übersprungen, beim **Export** unverändert mitgeschrieben
(Ist-Verhalten geprüft: `exportCatalog` iteriert über `CATALOG_SETTINGS_KEYS`
und hat den Key schon vorher exportiert — das bleibt so). Umgesetzt über
`IMPORT_SKIPPED_SETTINGS_KEYS` (ein `ReadonlySet`), das nur in der
Import-Schleife abgefragt wird; alle anderen Keys laufen unverändert hin und
zurück. Nach dem Import füllt `scanInstalledSlicers()` die Liste ohnehin von
selbst wieder.

**Teil 2 — Werte-Whitelist:** `IMPORT_ALLOWED_SETTINGS_VALUES`, gespiegelt aus
den Hooks/Contexts, denen die Keys gehören:

| Key | Erlaubte Werte | Quelle |
|---|---|---|
| `3mf-katalog-theme` | `system`, `light`, `dark` | `src/hooks/useTheme.ts` (`ThemeSetting`) |
| `3mf-katalog-display-preference` | `thumbnail`, `render` | `src/hooks/useDisplayPreference.ts` |
| `3mf-katalog-language` | `de`, `en`, `es`, `fr` | `src/i18n/types.ts` (`Language`) / `DICTIONARIES` |
| `3mf-katalog-density` | `compact`, `comfort` | `src/hooks/UiDensityContext.tsx` (`UiDensity`) |

Ein unbekannter Wert wird übersprungen (`console.warn`, bestehende lokale
Einstellung bleibt erhalten) — der Katalog-Import selbst gilt weiterhin als
erfolgreich, genau wie vom Finding gefordert. `null`/`undefined` löscht den Key
wie bisher.

**Teil 3 — Backend:** `open_in_slicer` validiert jetzt zusätzlich das
Modell-Argument (`validate_slicer_target_file`, siehe I-5 — die beiden Findings
überlappen hier). Eine **vollständige Allowlist** wurde bewusst *nicht* gebaut,
siehe „Teilweise adressiert" unten; die Begründung steht auch als Kommentar
über `validate_slicer_path`.

**Tests (`src/hooks/useCatalogBackup.test.ts`):**
- `exportCatalog still includes the slicer list (export direction unchanged)`
- `importCatalog never restores the slicer list from a backup (Finding Z-1)`
  — Backup versucht `/bin/sh` zu setzen, lokale Liste bleibt unangetastet
- `importCatalog skips settings values outside the allowed set` — prüft
  zugleich, dass ein *gültiger* Wert (`density: comfort`) weiterhin ankommt

---

## Testlauf

```
$ cd src-tauri && cargo test --lib
test result: ok. 169 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
(vorher 154 + 1 durch den Fix zu Recht gebrochener Bestandstest; 15 neue Tests)

```
$ npm test
 Test Files  20 passed (20)
      Tests  106 passed (106)
```
(4 neue Tests: 3 in `useCatalogBackup.test.ts`, 3 in `safeUrl.test.ts` —
gezielter Lauf beider Dateien: 11 passed)

```
$ npx tsc --noEmit
(keine Ausgabe, Exit 0)
```

`cargo clippy --lib`: nur die beiden **vorbestehenden** Warnungen
(`insert_file` unused/never used) — keine neuen.

---

## Geänderte Dateien

| Datei | Findings |
|---|---|
| `src-tauri/src/commands.rs` | I-1, I-2, I-3, I-5, A-1, A-2, Z-1 |
| `src-tauri/src/threemf/container.rs` | A-1 |
| `src-tauri/src/threemf/error.rs` | A-1 |
| `src/components/DetailPanel.tsx` | I-2 |
| `src/components/ModelDetailPage.tsx` | I-2 |
| `src/hooks/useCatalogBackup.ts` | Z-1 |
| `src/hooks/useCatalogBackup.test.ts` | Z-1 (Tests) |
| `src/lib/safeUrl.ts` (neu) | I-2 |
| `src/lib/safeUrl.test.ts` (neu) | I-2 (Tests) |

`src/hooks/useSlicers.ts` wurde **nicht** angefasst: die Fix-Anweisung zu Z-1
nennt `loadStored()` nur als Hintergrund (warum der superficial structural
check nicht reicht); die drei geforderten Teilmaßnahmen liegen alle woanders.
Eine zusätzliche Validierung dort hätte nichts geschlossen, was Teil 1 nicht
schon schließt, und wäre Scope-Erweiterung gewesen.

---

## Teilweise adressiert / abweichender Ansatz

**Z-1 Teil 3 — vollständige Slicer-Pfad-Allowlist: bewusst NICHT umgesetzt,
Folge-Design nötig.**
Es gibt derzeit keine backend-seitige Quelle für eine Allowlist: die
Slicer-Liste lebt ausschließlich im `localStorage` des Frontends, und
`open_in_slicer` bekommt den Pfad als Parameter. „Nur automatisch erkannte
Slicer" (`scan_installed_slicers` → `detect_slicers()`) als Allowlist zu
nehmen, würde jeden **manuell per Datei-Dialog hinzugefügten** Slicer brechen —
ein dokumentierter, legitimer Flow (`addSlicer(name, path)` mit
`source: 'manual'`). Das Backend kann „per Dialog gewählt" heute nicht von
„vom Frontend frei behauptet" unterscheiden; dafür bräuchte es eine
backend-persistierte Slicer-Liste (DB-Tabelle oder eigene Konfigdatei), in die
nur ein Rust-seitiger Datei-Picker schreibt. Das ist ein Redesign, kein
Quick-Fix, und wurde gemäß Auftrag nicht unter Zeitdruck erzwungen.
Stattdessen: der **Einschleusungsweg** ist geschlossen (Teil 1 — ein Backup
kann keine Slicer-Pfade mehr setzen), und das zweite Kommandozeilen-Argument ist
validiert (Teil 3/I-5). Die Begründung steht als Kommentar über
`validate_slicer_path`. **Empfehlung: eigenes Follow-up-Issue.**

**I-1, `files.name`: `..` wird als Teilzeichenkette abgelehnt, nicht nur als
ganze Komponente** — wie in der Anweisung wörtlich gefordert. Nebeneffekt: ein
Dateiname wie `mein..modell.3mf` in einem *importierten* Backup würde den
Import ablehnen. Betrifft nur den Import fremder/eigener Backups (nicht den
normalen Datei-Import, nicht bestehende Kataloge) und ist ein extrem seltenes
Namensmuster; die strengere Regel ist hier die sichere Wahl. Falls das je
stört, wäre die Lockerung auf „Komponente ist genau `..`" (wie
`validate_folder_name`) trivial.

---

## Self-Review

- **`is_http_url` ist case-sensitiv** (`http://`, nicht `HTTP://`). Das ist
  exakt das Verhalten des bestehenden `validate_source_url` — bewusst
  übernommen, kein Verhaltenswechsel. `JavaScript:` wird korrekt abgelehnt.
- **Doppelte Schranke bei A-1** (deklarierte `size()` *und* `take()`) ist
  Absicht: die deklarierte Größe kommt aus dem Archiv und ist nicht
  vertrauenswürdig. `take()` allein würde stillschweigend abschneiden und ein
  kaputtes XML weiterreichen; `size()` allein wäre umgehbar.
- **Performance:** `reject_if_sensitive_path` macht jetzt Dateisystemzugriffe.
  An den Einzelaufrufstellen (Ordner anlegen/umbenennen/verschieben)
  vernachlässigbar. In `validate_catalog_db_bytes` (eine Prüfung pro DB-Zeile)
  wurde die Aufbereitung der geschützten Ordner mit `expand_sensitive_dirs`
  aus der Schleife gezogen — siehe I-1.
- **`.`-Komponenten (CurDir)** in nicht existierenden Pfaden werden nicht
  entfernt, sondern wörtlich angehängt. Harmlos: `.` kann aus keinem Teilbaum
  herausführen; das schlimmste Ergebnis wäre eine zu *strenge* Ablehnung, nicht
  eine zu laxe.
- **Der Type-Guard `url is string`** in `safeUrl.ts` war nötig, weil
  `ModelFile.sourceUrl` `string | null` ist und `href` `string | undefined`
  erwartet — vorher übernahm das die `model.sourceUrl ?`-Verengung.
- **Kein `cloud.config.json`-Code angefasst** (Vorgabe eingehalten).

## Bedenken

1. **Z-1 Teil 3** braucht ein Folge-Design (siehe oben) — das ist das einzige
   Finding, das nicht restlos geschlossen ist.
2. `import_catalog` lehnt ein Backup jetzt **komplett** ab, wenn eine einzige
   `files`-Zeile schlecht ist. Das ist bei bösartiger Eingabe richtig; bei
   einem *eigenen*, historisch gewachsenen Katalog mit einem kuriosen
   Dateinamen wäre es eine harte Kante ohne Reparaturweg. Die `..`-Regel für
   `files.name` (siehe oben) ist die einzige Stelle, an der das realistisch
   greifen könnte.

---

## 2026-09-19 (Nachtrag) — die 2 Lücken aus dem Re-Review von 800e373

### Gap 1: A-1 (Zip-Bombe) — die beiden übersehenen Config-Einträge

`read_package` liest ganz zuoberst (`container.rs:55-56`) zwei
slicer-spezifische Einträge, bevor irgendeine gedeckelte Leseoperation läuft.
Beide gingen bisher ungebremst durch `read_to_string`.

Geändert:
- `src-tauri/src/threemf/container.rs:27-32` — neue Konstante
  `pub(super) const MAX_CONFIG_XML_BYTES: u64 = 16 * 1024 * 1024;` (16 MB,
  gleiche Größenordnung wie `MAX_THUMBNAIL_BYTES`/`MAX_RELS_XML_BYTES`).
- `src-tauri/src/threemf/container.rs:181` — `read_entry_to_string` von
  privat auf `pub(super)` gehoben, damit die Geschwister-Module denselben
  Helfer benutzen statt die Größenprüfung ein drittes Mal zu duplizieren.
  Das etablierte Muster (`size()`-Check + `.take(max_bytes)`) bleibt
  unverändert an genau einer Stelle.
- `src-tauri/src/threemf/plates.rs:12-24` — `count_plates` liest
  `Metadata/model_settings.config` jetzt über
  `container::read_entry_to_string(..., MAX_CONFIG_XML_BYTES)`.
- `src-tauri/src/threemf/slice_info.rs:53-65` — `parse_slice_info` liest
  `Metadata/slice_info.config` analog.

Verhalten unverändert: beide Funktionen geben weiterhin `None` zurück, wenn
die Datei fehlt oder nicht lesbar ist — ein zu großer Eintrag fällt jetzt
einfach in denselben `None`-Pfad, statt den Speicher zu füllen.

Tests:
- `threemf::plates::tests::rejects_an_oversized_model_settings_config_instead_of_reading_it`
- `threemf::slice_info::tests::rejects_an_oversized_slice_info_config_instead_of_reading_it`

Beide bauen ein Mini-ZIP mit einem entpackt 16 MB + 1 Byte großen Eintrag
(stark komprimierbar, ~16 KB im Archiv) und erwarten `None`.

### Gap 2: I-1 (Containment statt Denylist) — nur `trash_path`, bewusst nicht `files.path`

**Was umgesetzt wurde — `files.trash_path`:**

- `src-tauri/src/commands.rs:808-836` — neue Funktion
  `reject_if_outside_trash_dir(path, resolved_trash_dir)`: löst beide Seiten
  über das bestehende `resolve_path_for_sensitivity_check` auf und verlangt
  `starts_with` (Gleichheit mit dem Verzeichnis selbst wird ebenfalls
  abgelehnt).
- `src-tauri/src/commands.rs:2241` — `validate_catalog_db_bytes` bekommt
  einen dritten Parameter `trash_dir: &Path`; die aufgelöste Grenze wird
  einmal vor der Zeilenschleife berechnet (gleiches Muster wie
  `expand_sensitive_dirs`).
- `src-tauri/src/commands.rs:2299-2306` — die bestehende Denylist-Prüfung
  auf `trash_path` bleibt stehen, die Containment-Prüfung kommt daneben
  (Defense in Depth).
- `src-tauri/src/commands.rs:2374` — Aufrufstelle in `import_catalog` reicht
  `&state.trash_dir` durch.
- Leere/`NULL`-`trash_path`-Werte werden weiterhin übersprungen — die
  bestehende `.filter(|p| !p.trim().is_empty())`-Kante bleibt unverändert.

Damit ist der beschriebene Angriff geschlossen: ein präpariertes Backup mit
`trash_path = /home/user/Dokumente/irgendwas-wichtiges.pdf` und altem
`deleted_at` wird beim Import abgelehnt, statt beim nächsten App-Start von
`purge_expired_trash_on_startup` per `remove_file` gelöscht zu werden.
`restore_file` ist dadurch mit entschärft: die Quelle eines `move_file` kann
nur noch innerhalb des echten Papierkorb-Verzeichnisses liegen, und
existiert dort nichts, schlägt der Move fehl, statt ein beliebiges Ziel zu
überschreiben.

**Warum legitime Backups weiterhin importieren:**
`trash_path` entsteht im gesamten Code ausschließlich als
`state.trash_dir.join(format!("{id}-{}", file.name))` — an genau zwei
Stellen (`delete_file`, `commands.rs:645`; Cleanup-Pfad, `commands.rs:2560`).
`state.trash_dir` ist `app_data_dir.join("trash")` (`lib.rs:82`), also pro
Installation fest und nicht vom Modell-Ablageort des Nutzers abhängig. Ein
echtes Backup dieser App trägt damit immer Werte innerhalb der Grenze.
Abgesichert durch den Gegenprobe-Test
`validate_catalog_db_bytes_accepts_trash_path_inside_the_real_trash_dir`
sowie den unveränderten Bestandstest
`validate_catalog_db_bytes_accepts_harmless_file_rows`.

Tests:
- `validate_catalog_db_bytes_rejects_trash_path_outside_the_real_trash_dir`
  (genau das Szenario aus dem Re-Review: Opferdatei im selben Elternordner,
  aber außerhalb von `trash/`)
- `validate_catalog_db_bytes_accepts_trash_path_inside_the_real_trash_dir`
  (Gegenprobe: legitimes Backup)
- `validate_catalog_db_bytes_rejects_trash_path_escaping_the_trash_dir_via_parent_components`
  (`<trash_dir>/../opfer.pdf` — Symlink-/`..`-Auflösung greift)
- Bestandstest `validate_catalog_db_bytes_rejects_trash_path_in_sensitive_directory`
  wurde bewusst so parametrisiert (`trash_dir = temp_dir`), dass weiterhin
  die Denylist die ablehnende Prüfung ist.

**Was NICHT umgesetzt wurde — `files.path` (NEEDS_CONTEXT):**

Für `files.path` gibt es in diesem Codebase kein "Katalog-Basisverzeichnis",
gegen das man sinnvoll eindämmen könnte:

- `register_catalog_base_dir` (`commands.rs:1736-1764`) nimmt ein vom Nutzer
  per Dialog frei gewähltes Verzeichnis entgegen und legt es lediglich als
  weitere `folders`-Zeile an. Es gibt keinen Speicherort für "das eine"
  Basisverzeichnis und keine Beschränkung auf genau eines.
- Der Import-/Scan-Pfad (`commands.rs:1543-1600`) iteriert über beliebig
  viele `roots` aus einem Datei-/Ordner-Dialog. Jeder Aufruf kann einen
  völlig anderen Ort auf der Platte einbringen; `files.path` ist danach
  schlicht der reale Pfad der Quelldatei. Einzeldateien (`is_folder_root ==
  false`) bekommen sogar gar keinen Ordner zugeordnet.
- Der Ordnerbaum ist damit nicht an einer Wurzel verankert, sondern ist eine
  Menge unabhängiger Wurzeln.

Eine Containment-Prüfung auf `files.path` würde also mit hoher
Wahrscheinlichkeit reale, unmodifizierte Backups ablehnen (jeder Katalog,
der über mehrere Import-Aktionen aus verschiedenen Verzeichnissen gewachsen
ist — der Normalfall). Deshalb bleibt es dort bei der Denylist, und die
Entscheidung, ob die App künftig ein einziges verbindliches
Katalog-Basisverzeichnis erzwingen soll (was diese Prüfung erst möglich
machen würde), ist eine Produktentscheidung, keine Security-Korrektur.

### Testlauf

```
cd src-tauri && cargo test --lib
test result: ok. 174 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

(vorher 169 Tests; +5 neue). `cargo build` erzeugt weiterhin genau die
2 vorbestehenden Warnungen aus `db::repository` (`insert_file` nur in Tests
benutzt) — unverändert gegenüber 800e373, geprüft per `git stash`-Vergleich.
Keine TS-Dateien angefasst, daher kein `tsc`/`npm test`-Lauf nötig.

### Geänderte Dateien

- `src-tauri/src/threemf/container.rs`
- `src-tauri/src/threemf/plates.rs`
- `src-tauri/src/threemf/slice_info.rs`
- `src-tauri/src/commands.rs`

### Offene Punkte / Bedenken

1. **`files.path` ohne Containment** (siehe oben) — bleibt bewusst offen,
   braucht eine Produktentscheidung.
2. **Backup von einer anderen Maschine / einem anderen Benutzerkonto**: Die
   neue `trash_path`-Prüfung lehnt ein solches Backup ab, sobald es
   Papierkorb-Einträge enthält, weil `app_data_dir` den Benutzernamen
   enthält. Praktisch ist dieser Fall ohnehin kaputt — der Export enthält
   nur `catalog.db` + `settings.json`, die physischen Papierkorb-Dateien
   reisen nicht mit, und `files.path` zeigt danach ins Leere. Trotzdem ist
   es eine neue harte Ablehnung statt einer stillen Leerstelle. Ein
   nachsichtigerer Weg (Zeile beim Import auf `trash_path = NULL` setzen,
   statt den gesamten Import abzulehnen) wäre denkbar, würde aber die
   bestehende „validieren, nicht reparieren"-Linie von
   `validate_catalog_db_bytes` durchbrechen — daher hier nicht gemacht.
