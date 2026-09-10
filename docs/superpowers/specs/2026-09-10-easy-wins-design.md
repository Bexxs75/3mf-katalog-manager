# "Leicht"-Features aus der Konkurrenz-App-Analyse — Design

## Ausgangslage

Beim Sichten zweier WhatsApp-Screenshots einer Konkurrenz-App (2026-09-10)
wurde deren Feature-Liste nach Aufwand bewertet. Vier Punkte wurden als
"leicht" eingestuft, weil die nötige Datenbasis in 3mf-katalog-manager
bereits (teilweise) vorhanden ist:

1. Druckstatus-Badge + Gewicht pro Modell
2. "Zuletzt angesehen" als Sortieroption + "NEU"-Badge für kürzlich
   importierte Modelle
3. "Creators" als eigene Filterkategorie (Designer-Metadatum wird beim
   3MF-Import schon geparst, aber bisher nirgends als Filter ausgestellt)
4. Erkennung exakter Duplikate beim Import (Datei-Inhalts-Hash)

Dieses Spec bündelt alle vier in einem gemeinsamen Plan (vier
unabhängige, aber gleich gelagerte Tasks) statt vier getrennter Zyklen -
jeder Punkt ist für sich zu klein für einen eigenen vollen
Spec→Plan→SDD-Durchlauf.

**Wichtige Korrektur während der Konzeption:** Für Punkt 4 wurde
ursprünglich ein interaktiver "User entscheidet"-Dialog angenommen (wie
beim bestehenden Cloud-Sync-Change-Detection-Muster). Eine Prüfung des
Codes ergab, dass es diesen Dialog-Mechanismus für Import-Konflikte gar
nicht gibt - sowohl der lokale Import (`import_many` in
`src-tauri/src/commands.rs`) als auch der Cloud-Import
(`import_from_cloud` in `src-tauri/src/cloud/commands.rs`) überspringen
Duplikate bereits heute komplett still, ohne jedes Frontend-Feedback. Um
im "Leicht"-Aufwand zu bleiben, wurde daraufhin auf automatisches
Überspringen + eine kurze Zusammenfassungsmeldung reduziert (siehe
Baustein 4) statt einen neuen interaktiven Dialog zu bauen.

## Globale Rahmenbedingungen

- SQLite ohne Migrationsframework: Alle vier neuen Spalten sind
  `NULL`-fähig oder haben einen `DEFAULT`, zusätzlich je eine
  fehlertolerante `ALTER TABLE ... ADD COLUMN`-Zeile in `repository.rs`s
  `init()` (analog zum `image_png`-Fix beim Filament-Feature), da die
  echte, bereits befüllte Produktions-DB sonst die neuen Spalten nicht
  bekäme.
- i18n: neue Texte in allen vier Sprachdateien (de/en/es/fr).
- Alle vier Bausteine sind voneinander unabhängig lauffähig und
  testbar - keiner ist Voraussetzung für einen anderen.

## Baustein 1: Druckstatus + Gewicht

### Datenmodell

```sql
ALTER TABLE files ADD COLUMN print_status TEXT NOT NULL DEFAULT 'not_printed'
    CHECK (print_status IN ('not_printed', 'printed'));
```

(Als Teil von `CREATE TABLE IF NOT EXISTS files` für Neuinstallationen,
zusätzlich per fehlertoleranter `ALTER TABLE` in `init()` für
Bestands-DBs, exakt wie oben inklusive `CHECK` - SQLite erlaubt einen
`CHECK`-Constraint bei `ALTER TABLE ADD COLUMN`, solange er sich nur auf
die neue Spalte bezieht und der `DEFAULT`-Wert ihn erfüllt, beides ist
hier der Fall.)

### Backend

- `FileRecord`/`ModelFileDto` bekommen `print_status: String` (Rust) /
  `printStatus: 'not_printed' | 'printed'` (Frontend-DTO-Feld,
  camelCase).
- Neue Repository-Funktion, gleiches Muster wie das bestehende
  `set_file_sync_status` (`repository.rs:400`) - keine eigene
  Rust-seitige Enum-Validierung, der `CHECK`-Constraint auf der Spalte
  ist die einzige Durchsetzung, ein ungültiger Wert kommt als
  `DbError` aus der SQL-Ausführung zurück:
  ```rust
  pub fn set_print_status(conn: &Connection, file_id: i64, status: &str) -> Result<(), DbError>;
  ```
- Neuer Tauri-Command:
  ```rust
  #[tauri::command]
  pub fn set_print_status(state: State<AppState>, file_id: String, status: String) -> CmdResult<()>;
  ```

**Gewicht-Berechnung** (kein neues DB-Feld, live berechnet):

```rust
const MATERIAL_DENSITY_G_CM3: &[(&str, f64)] = &[
    ("pla", 1.24),
    ("petg", 1.27),
    ("abs", 1.04),
    ("tpu", 1.21),
    ("asa", 1.05),
    ("pc", 1.20),
    ("nylon", 1.14),
];
const DEFAULT_DENSITY_G_CM3: f64 = 1.24;

pub fn estimate_weight_g(volume_cm3: Option<f64>, material_name: Option<&str>) -> Option<f64> {
    let volume = volume_cm3?;
    let density = material_name
        .and_then(|name| {
            let lower = name.to_lowercase();
            MATERIAL_DENSITY_G_CM3
                .iter()
                .find(|(key, _)| lower.contains(key))
                .map(|(_, d)| *d)
        })
        .unwrap_or(DEFAULT_DENSITY_G_CM3);
    Some(volume * density)
}
```

Diese Funktion lebt in `src-tauri/src/commands.rs` neben den anderen
DTO-Hilfsfunktionen. `to_dto()` ruft sie auf und setzt ein neues Feld
`estimated_weight_g: Option<f64>` auf `ModelFileDto` (camelCase
`estimatedWeightG` im Frontend) - kein eigenes DB-Feld, wird bei jedem
`list_files`/`to_dto`-Aufruf neu berechnet. Materialname kommt aus
`materials.first().map(|m| m.name.as_str())` (erstes Material der
Datei).

### Frontend

- `ModelFile` (`src/types/index.ts`) bekommt `printStatus:
  'not_printed' | 'printed'` und `estimatedWeightG: number | null`.
- `DetailPanel.tsx`: neuer Toggle-Button ("Gedruckt markieren" /
  "Als nicht gedruckt markieren", je nach aktuellem Status), ruft
  `invoke('set_print_status', { fileId, status })` und aktualisiert den
  lokalen State optimistisch (gleiches Muster wie bestehende
  Detail-Panel-Aktionen). Gewicht steht als Text neben
  Volumen/Dimensionen, z. B. "≈ 42 g" - mit `≈`-Präfix, da es eine
  Schätzung ist, nicht Baustein 1 des `format.ts`-Locale-Systems umgeht
  (`formatWeightG` aus dem Filament-Feature wird wiederverwendet).
- `ModelGrid.tsx`: kleiner Badge unten auf der Karte, nur sichtbar wenn
  `printStatus === 'printed'` (z. B. "✓" + `t('printedBadge')`,
  gleiche Font-Mono-Ecke wie der Cloud-Origin-Indikator). Kein
  Gewichtstext auf der Karte (bewusst nur DetailPanel, siehe
  Nutzerentscheidung).

## Baustein 2: Zuletzt angesehen + NEU-Badge

### Datenmodell

```sql
ALTER TABLE files ADD COLUMN last_viewed_at TEXT;
```

Nullable, `NULL` = nie angesehen.

### Backend

- `FileRecord`/`ModelFileDto` bekommen `last_viewed_at: Option<String>`
  / `lastViewedAt: string | null`.
- Neue Repository-Funktion:
  ```rust
  pub fn mark_file_viewed(conn: &Connection, file_id: i64) -> Result<(), DbError>;
  ```
  Setzt `last_viewed_at = <jetzt, RFC3339>` für die gegebene `file_id`.
- Neuer Tauri-Command:
  ```rust
  #[tauri::command]
  pub fn mark_file_viewed(state: State<AppState>, file_id: String) -> CmdResult<()>;
  ```

### Frontend

- `SortKey` (`src/types/index.ts`) erweitert um `'viewed'`.
- `App.tsx`: `onSelect`-Handler ruft zusätzlich
  `invoke('mark_file_viewed', { fileId: id })` auf - Fire-and-forget
  (`.catch(() => {})`, kein Warten, kein UI-Update nötig, da die
  Sortierung erst beim nächsten Wechsel auf `sort === 'viewed'`
  relevant wird). Neuer Sortier-Zweig in der bestehenden
  `useMemo`-Kette:
  ```ts
  if (sort === 'viewed') {
    return (b.lastViewedAt ?? '').localeCompare(a.lastViewedAt ?? '');
  }
  ```
  (leere Strings sortieren durch `localeCompare` ans Ende, da sie
  lexikografisch vor jedem echten ISO-Zeitstempel liegen -
  `''.localeCompare('2026-01-01...')` ist negativ, landet also nach
  hinten in der absteigenden Sortierung.)
- `Header.tsx`: neue `<option value="viewed">{t('sortLastViewed')}</option>`
  im bestehenden Sortier-Dropdown.
- `ModelGrid.tsx`: "NEU"-Badge, rein clientseitig berechnet aus
  `importedAt`, keine neue Datenquelle:
  ```ts
  const isNew = Date.now() - new Date(m.importedAt).getTime() < 24 * 60 * 60 * 1000;
  ```
  Badge in der gleichen visuellen Familie wie der Druckstatus-Badge aus
  Baustein 1, oben links auf der Karte (Druckstatus-Badge bleibt unten,
  keine Überlappung).

## Baustein 3: Creators-Filter

### Datenmodell

```sql
ALTER TABLE files ADD COLUMN creator TEXT;
```

Nullable - STL-Dateien und 3MF-Dateien ohne "Designer"-Metadatum haben
`NULL`.

### Backend

- `NewFile`/`FileRecord`/`ModelFileDto` bekommen `creator: Option<String>`
  / `creator: string | null`.
- `import_one` (`src-tauri/src/commands.rs`) extrahiert den Wert direkt
  neben der bestehenden `dimensions_mm`/`volume_cm3`-Destrukturierung:
  ```rust
  let creator = metadata.get("Designer").cloned();
  ```
  (nur für den `Some("3mf")`-Zweig - `metadata` ist im `Some("stl")`-Zweig
  bereits eine leere `BTreeMap`, `creator` wird dort `None`.)
- Neue Repository-Funktion, exaktes Vorbild `list_tag_counts`:
  ```rust
  pub fn list_creator_counts(conn: &Connection) -> Result<Vec<(String, i64)>, DbError>;
  ```
  `SELECT creator, COUNT(*) FROM files WHERE creator IS NOT NULL GROUP BY creator ORDER BY creator`.
- Neuer Tauri-Command:
  ```rust
  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct CreatorCountDto { pub label: String, pub count: i64 }

  #[tauri::command]
  pub fn list_creators(state: State<AppState>) -> CmdResult<Vec<CreatorCountDto>>;
  ```

### Frontend

- Neuer Typ `CreatorCount { label: string; count: number }`
  (`src/types/index.ts`) - kein `colorHue`, reiner Textfilter ohne
  Farbpunkt.
- `Sidebar.tsx`: neue Sektion "Creators" unterhalb von "Tags", exakt
  gleiches Listen-Muster (Zeile mit Label + Anzahl, Klick togglet
  Auswahl), aber ohne den farbigen `oklch(...)`-Punkt der Tags.
- `App.tsx`: neuer State `activeCreator: string | null`, in der
  bestehenden Filter-`useMemo`-Kette als zusätzliche UND-Bedingung neben
  `activeFolderId`/`activeTag`/`query`:
  ```ts
  .filter((f) => activeCreator === null || f.creator === activeCreator)
  ```
  `list_creators` wird beim Mount und nach jedem Import neu geladen
  (gleicher Zeitpunkt wie der bestehende `list_tags`-Aufruf).

## Baustein 4: Exakte Duplikate

### Datenmodell

```sql
ALTER TABLE files ADD COLUMN content_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files (content_hash);
```

### Backend

- Neue Cargo-Dependency: `sha2 = "0.10"`.
- Neue freie Funktion in `src-tauri/src/commands.rs`, neben
  `collect_supported_files`:
  ```rust
  use sha2::{Digest, Sha256};

  fn compute_content_hash(path: &Path) -> CmdResult<String> {
      let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
      Ok(format!("{:x}", Sha256::digest(&bytes)))
  }
  ```
  Liest die Datei ein zweites Mal ein (zusätzlich zum Read, den
  `parse_3mf_file`/`parse_stl_file` intern für sich selbst machen) -
  bewusst kein Umbau der Parser-Signaturen, um Bytes durchzureichen;
  bei den hier üblichen Dateigrößen (einzelne 3MF/STL-Modelle) ist der
  zusätzliche Read vernachlässigbar und deutlich einfacher als ein
  Parser-Refactor.
- `import_many` (`src-tauri/src/commands.rs`) ruft
  `compute_content_hash` direkt nach der bestehenden Pfad-Prüfung auf
  (Reihenfolge: erst der billige Pfad-Check, dann - nur falls der Pfad
  neu ist - Hash berechnen und prüfen; so wird für Dateien, die schon
  per Pfad als Duplikat erkannt sind, gar nicht erst gehasht):
  ```rust
  let mut duplicate_count = 0i64;
  // ... in der bestehenden Schleife, nach dem file_exists_by_path-Check:
  let content_hash = match compute_content_hash(&path) {
      Ok(h) => h,
      Err(e) => { eprintln!("[import] Hash fehlgeschlagen für {path_str}: {e}"); continue; }
  };
  match db::file_exists_by_hash(&conn, &content_hash) {
      Ok(true) => { duplicate_count += 1; continue; }
      Ok(false) => {}
      Err(e) => { eprintln!("[import] Duplikatprüfung (Hash) fehlgeschlagen für {path_str}: {e}"); continue; }
  }
  ```
  `import_one` bekommt einen neuen Parameter `content_hash: String` und
  reicht ihn nur noch an `NewFile` durch (berechnet ihn nicht mehr
  selbst) - so bleibt der Hash an genau einer Stelle pro Importpfad
  berechnet. `import_from_cloud` (`src-tauri/src/cloud/commands.rs`)
  übernimmt exakt dasselbe Muster: `compute_content_hash` auf den
  gerade geschriebenen `cache_path` anwenden, direkt nach dem
  bestehenden `already_imported`-Pfad-Check und vor dem
  `import_one`-Aufruf.
  ```rust
  pub fn file_exists_by_hash(conn: &Connection, hash: &str) -> Result<bool, DbError>;
  ```
  (neue Repository-Funktion, Vorbild `file_exists_by_path`.)
  `NewFile` bekommt `content_hash: String`.
- Der einzige Frontend-Aufrufer von `import_from_cloud` ist
  `App.tsx:84` (`invoke<ModelFile[]>('import_from_cloud', { fileIds })`
  im Picker-Import-Flow) - auch dieser Call wird auf
  `ImportResultDto` umgestellt.
- Alle drei Tauri-Commands, die `import_many` aufrufen
  (`import_files`, `import_folder`, `import_dropped`), sowie
  `import_from_cloud`, geben künftig zurück:
  ```rust
  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct ImportResultDto {
      pub imported: Vec<ModelFileDto>,
      pub duplicate_count: i64,
  }
  ```
  statt bisher `Vec<ModelFileDto>` direkt. **Das ist die einzige
  Breaking Change der vier Bausteine** - alle vier
  Frontend-Aufrufstellen in `App.tsx` (`importFiles` Zeile 199,
  `importFolder` Zeile 200, der Drag-and-Drop-Handler Zeile 147, und der
  Picker-Import-Aufruf Zeile 84) müssen entsprechend angepasst werden.

### Frontend

- `App.tsx`: `mergeImported` wird umbenannt/erweitert, um
  `ImportResultDto` statt `ModelFile[]` entgegenzunehmen; extrahiert
  `imported` für den bestehenden Merge-in-Grid-Schritt und
  `duplicateCount` für die neue Anzeige.
- Neue, erste Toast-/Banner-Komponente `ImportSummaryBanner.tsx`:
  einfache, feste Position (z. B. unten rechts, `fixed bottom-4
  right-4`), erscheint nur wenn `duplicateCount > 0`, Text
  `"{imported} importiert, {duplicates} Duplikate übersprungen"`
  (i18n-Key mit Platzhaltern, gleiches `{{placeholder}}`-Muster wie
  bestehende i18n-Keys mit dynamischen Werten, z. B. `filesCount`).
  Schließt sich nach 5 Sekunden automatisch (`setTimeout` + lokaler
  `visible`-State) oder per Klick auf ein ✕. Kein Einfluss auf
  bestehende UI, wenn `duplicateCount === 0` (Banner wird dann gar
  nicht gemountet).

## Fehlerbehandlung

- Alle neuen Commands folgen dem bestehenden Muster: `CmdResult<T>` =
  `Result<T, String>`, DB-Fehler werden zu lesbaren Strings gemappt.
- `estimate_weight_g` gibt `None` zurück, wenn `volume_cm3` fehlt
  (z. B. bei einem STL ohne erfolgreiche Geometrie-Extraktion) - das
  Frontend zeigt dann keinen Gewichtstext an (wie bei fehlendem Volumen
  schon heute üblich).
- Hash-Berechnung schlägt fehl → wie jeder andere Lesefehler in
  `import_one`, Datei wird übersprungen und geloggt, kein Absturz des
  gesamten Imports.

## Testing

- Rust: Roundtrip-Tests in `db/mod.rs` für `set_print_status`,
  `mark_file_viewed`, `list_creator_counts`, `file_exists_by_hash`
  (gleiches Muster wie bestehende Repository-Tests). Unit-Test für
  `estimate_weight_g` (bekanntes Material → erwartete Dichte,
  unbekanntes Material → Fallback-Dichte, `None`-Volumen → `None`).
- Frontend: `tsc --noEmit` sauber.
- Live-Verifikation im laufenden `npm run tauri dev` vor Abschluss:
  Druckstatus togglen, nach "Zuletzt angesehen" sortieren, nach Creator
  filtern, eine bereits importierte Datei erneut importieren und die
  Zusammenfassungs-Banner sehen.

## Out of Scope

- Druckstatus als mehrstufiger Workflow mit echter Drucker-Integration
  (nur zwei manuelle Zustände, siehe Nutzerentscheidung).
- Gewicht als editierbares/gemessenes Feld (nur Schätzung aus Volumen ×
  Dichte, keine Waage-Eingabe).
- Eigener "Zuletzt angesehen"-Sidebar-Bereich (nur Sortieroption, siehe
  Nutzerentscheidung).
- Interaktiver Konflikt-Dialog für Duplikate (automatisches Überspringen
  + Zusammenfassung reicht, siehe Korrektur oben).
- Unscharfe/ähnliche Duplikate (nur exakter Hash-Treffer, keine
  Ähnlichkeitserkennung - das war ohnehin im "aufwendig"-Topf der
  ursprünglichen Analyse).
- Mehrfachauswahl bei Creator-/Tag-Filtern (bleibt Single-Select wie
  bisher bei Tags).
