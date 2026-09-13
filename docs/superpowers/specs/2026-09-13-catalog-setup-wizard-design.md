# Katalog-Speicherort: Ersteinrichtung und importbewusste Platzierung

## Kontext

Mit der echten Ordnerstruktur (siehe `2026-09-13-real-folder-tree-design.md`,
bereits umgesetzt) hat "Ordner" im Katalog eine neue Bedeutung bekommen:
Ordner sind jetzt echte Verzeichnisse auf der Platte, und Dateien per
Drag & Drop in einen anderen Ordner zu ziehen, verschiebt sie dort
**physisch**. Das ist ein Verhalten, das ein Erstnutzer (und auch
Bestandsnutzer beim Update) nicht erwarten, wenn er es nicht erklärt
bekommt.

Zusätzlich klafft eine Lücke: "Dateien importieren" (Einzelauswahl, z. B.
aus `~/Downloads`) setzt nie einen Ordner-Bezug (`folder_id` bleibt
`null`) — unabhängig davon, welcher Ordner in der Sidebar gerade aktiv
ist. Eine einzeln importierte Datei landet immer in der virtuellen Wurzel
("Alle Modelle"), obwohl der Nutzer vielleicht gerade einen bestimmten
Ordner geöffnet hat und die Datei dort erwarten würde.

Der User-Vorschlag: eine Ersteinrichtung, die zwei Wege anbietet — eine
bereits vorhandene Ordnerstruktur (z. B. eine, in der der Nutzer seine
3D-Druckdateien schon organisiert hat) 1:1 übernehmen, oder einen neuen,
auch leeren Ort festlegen, an dem der Katalog ab sofort neu importierte
Einzeldateien ablegt. Diese Wahl muss verständlich erklärt werden und
jederzeit in den Einstellungen änderbar bleiben.

## Ziel

1. Ein Dialog beim ersten Start (und jederzeit erneut über die
   Einstellungen erreichbar), der in klarer, verständlicher Sprache
   erklärt, was sich geändert hat, und dem Nutzer zwei konkrete Wege
   anbietet — plus die Möglichkeit, die Entscheidung zu vertagen.
2. Ein persistenter "Katalog-Speicherort" (`catalogBaseDir`), der als
   Standardziel für künftige Einzeldatei-Importe dient.
3. "Dateien importieren" berücksichtigt danach den Kontext: liegt eine
   Datei gerade im aktiven Ordner (falls einer ausgewählt ist) oder im
   Katalog-Speicherort selbst — nicht mehr blind in der Wurzel.
4. Keine Überraschung für Nutzer, die den Dialog wegklicken oder noch nie
   gesehen haben (Update-Bestandsnutzer): ohne konfigurierten
   Speicherort bleibt das heutige Verhalten (Datei bleibt am
   Ursprungsort, kein Ordner-Bezug) unverändert.

## Nicht-Ziele

- "Ordner importieren" / "Ordner als Sammlung importieren" ändern sich
  NICHT — beide bringen weiterhin ihre eigene, unabhängige
  Verzeichnisstruktur mit, unabhängig vom Katalog-Speicherort. Der
  Speicherort betrifft ausschließlich die Einzeldatei-Auswahl
  ("Dateien importieren").
- Kein automatisches "Aufräumen" bestehender, bereits importierter
  Dateien in den neuen Speicherort — der Dialog wirkt nur auf zukünftige
  Einzelimporte.

## Architektur

**Persistenz des Speicherorts**: `localStorage`, exakt nach dem
etablierten Projektmuster für Anzeige-Einstellungen (siehe
`src/hooks/useDisplayPreference.ts` — Theme/Dichte/Sprache/Slicer folgen
demselben Muster). Kein Backend-`app_settings`-Mechanismus nötig, da so
etwas im Projekt für Einstellungen dieser Art ohnehin nicht existiert und
`localStorage` bereits der etablierte Weg ist.

```ts
// src/hooks/useCatalogBaseDir.ts
const STORAGE_KEY = '3mf-katalog-base-dir';
const SETUP_SEEN_KEY = '3mf-katalog-setup-seen';
// catalogBaseDir: string | null (absoluter Pfad)
// setupSeen: boolean (Dialog wurde mindestens einmal gesehen/entschieden,
//            auch bei "Später einrichten" - verhindert, dass der Dialog
//            bei jedem Start erneut aufploppt)
```

**Backend-Ergänzungen** (minimal, baut auf bereits vorhandenen Commands
auf):

```rust
#[tauri::command]
pub async fn pick_folder_path(app: tauri::AppHandle) -> CmdResult<Option<String>>
// Duenner Wrapper um app.dialog().file().blocking_pick_folder(), OHNE
// selbst zu importieren - gibt nur den gewaehlten Pfad zurueck. Analog zu
// pick_slicer_executable() (commands.rs, existierendes Muster).
// Wird vom Setup-Dialog fuer BEIDE Optionen genutzt (bestehende Struktur
// wählen ODER neuen Ort wählen/anlegen).

#[tauri::command]
pub fn register_catalog_base_dir(state: State<AppState>, path: String) -> CmdResult<FolderDto>
// Stellt sicher, dass fuer `path` eine folders-Zeile existiert (Wurzel-
// Ebene, kein parent_id) - legt das Verzeichnis auf der Platte an, falls
// es noch nicht existiert (std::fs::create_dir_all), dann
// db::ensure_folder_path(&conn, &path, &path) - diese Funktion (Task 2
// des vorherigen Plans) ist bereits exakt fuer "stelle sicher, dass eine
// Ebene existiert, idempotent" gebaut; import_root == dir ist der Fall
// "die Wurzel selbst, keine Unterebenen". Wird NUR fuer die
// "Neuen Ort einrichten"-Option gebraucht - die "Bestehende Struktur
// uebernehmen"-Option legt ihre Wurzel-Zeile bereits automatisch ueber
// den bestehenden import_dropped-Aufruf an.
```

Kein neuer Command für "bestehende Struktur übernehmen" nötig — das ist
exakt der bereits vorhandene `import_dropped(paths: Vec<String>)` mit
dem einen vom Nutzer gewählten Pfad, der über `import_many` bereits
rekursiv die komplette Hierarchie aufbaut (siehe
`2026-09-13-real-folder-tree-design.md`).

**Frontend-Ergänzung an `import_files`**: Nach erfolgreichem
`invoke('import_files')` prüft `App.tsx`, ob `catalogBaseDir` gesetzt
ist. Wenn ja, wird für jede neu importierte Datei
`invoke('move_file_to_folder', { fileId, folderId })` aufgerufen
(bereits existierender Command aus dem vorherigen Plan) —
`folderId` = die `id` des aktuell aktiven Ordners (`activeFolderId`,
falls nicht `'all'`), sonst die `id` der Wurzel-`folders`-Zeile des
Katalog-Speicherorts (Lookup über `folders.find(f => f.path ===
catalogBaseDir && !f.parentId)` in der bereits im Frontend gehaltenen
`folders`-Liste). Ist `catalogBaseDir` NICHT gesetzt, ändert sich am
Import-Verhalten nichts — die Datei bleibt am Ursprungsort, kein
Ordner-Bezug (heutiges Verhalten).

## Der Setup-Dialog (`CatalogSetupDialog.tsx`)

Modal, gleiches visuelles Muster wie `CatalogCleanupDialog.tsx`
(`fixed inset-0 z-50 ... bg-black/40` Overlay, zentriertes Panel,
`bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]`).
Breiter als der Cleanup-Dialog (`w-[560px]`), da mehr Erklärtext.

**Trigger:**
- Automatisch beim ersten Start: `!setupSeen` (unabhängig davon, ob der
  Katalog schon Dateien enthält — auch ein Update-Bestandsnutzer sieht
  ihn einmal, da die Ordner-Bedeutung sich für ihn genauso geändert hat).
- Danach jederzeit manuell über einen neuen Abschnitt "Katalog-Speicherort"
  im Einstellungen-Panel (`Rail.tsx`), der den aktuell konfigurierten Pfad
  anzeigt (oder "nicht eingerichtet") mit einem Button "Ändern"/"Einrichten".

**Inhalt (Deutsch, Grundlage für alle vier Sprachen):**

Titel: "Wie soll dein Katalog organisiert sein?"

Einleitungstext: "Ordner im Katalog sind jetzt echte Verzeichnisse auf
deiner Festplatte. Verschiebst du eine Datei im Programm in einen anderen
Ordner, wird sie dort auch tatsächlich abgelegt — nicht nur im Katalog
umsortiert. Damit neu importierte Dateien sinnvoll einsortiert werden,
legen wir jetzt einen festen Speicherort fest."

Zwei große, gleichwertige Auswahlkarten (analog zum Options-Karten-Stil
aus dem HTML-Mockup, aber hier als echte `<button>`-Kacheln):

- **Karte A — "Bestehende Ordnerstruktur übernehmen"**
  Beschreibung: "Du organisierst deine Druckdateien schon in Ordnern?
  Wähle den obersten Ordner aus — der Katalog übernimmt die komplette
  Struktur inklusive aller Unterordner und importiert alle enthaltenen
  3mf-/STL-Dateien."
  Klick → `pick_folder_path()` → bei Auswahl: Ladezustand ("Importiere…"),
  dann `invoke('import_dropped', { paths: [path] })`, danach
  `setCatalogBaseDir(path)` + `setSetupSeen(true)`, Dialog schließt,
  Erfolgsmeldung mit Anzahl importierter Dateien (bestehendes
  `ImportResultDto` liefert `imported`/`duplicateCount` — bestehendes
  `ImportSummaryBanner.tsx`-Muster wiederverwenden).

- **Karte B — "Neuen Ort einrichten"**
  Beschreibung: "Leg einen (auch leeren) Ordner fest, in dem der Katalog
  ab jetzt neu importierte Dateien ablegt. Unterordner kannst du später
  jederzeit im Programm anlegen und Dateien per Drag & Drop einsortieren."
  Klick → `pick_folder_path()` → bei Auswahl:
  `invoke('register_catalog_base_dir', { path })`, danach
  `setCatalogBaseDir(path)` + `setSetupSeen(true)`, Dialog schließt.

Unauffälliger dritter Weg (Textlink, kein Button gleichen Gewichts):
"Später einrichten" → nur `setSetupSeen(true)`, `catalogBaseDir` bleibt
`null`, Dialog schließt, heutiges Verhalten bleibt bestehen.

Fußnotiz unter beiden Karten: "Du kannst diese Wahl jederzeit in den
Einstellungen unter „Katalog-Speicherort" ändern."

Beide Karten-Klicks können fehlschlagen (Dialog vom Nutzer abgebrochen →
`pick_folder_path()` liefert `null` → nichts passiert, Dialog bleibt
offen; Import-/Register-Fehler → Fehlertext im Dialog, analog zum
bestehenden `catalogBackupError`-Anzeigemuster).

## Einstellungen-Panel-Ergänzung (`Rail.tsx`)

Neuer Abschnitt "Katalog-Speicherort" (Position: nach "Katalog-Backup",
letzter Abschnitt) zeigt:
- Aktuellen Pfad (`font-mono-ui`, `text-[var(--ink-3)]`, `truncate`) oder
  `t('catalogBaseDirNotSet')`, falls `null`.
- Button "Ändern" (falls gesetzt) / "Einrichten" (falls `null`) → öffnet
  denselben `CatalogSetupDialog`.

## i18n

Alle neuen Texte (Dialogtitel, Einleitung, beide Kartenbeschreibungen,
Später-Link, Fußnotiz, Settings-Abschnitt, Fehlermeldungen) bekommen
Schlüssel nach dem bestehenden Namensmuster (`catalogSetup*`) und werden
in `de.ts`/`en.ts`/`es.ts`/`fr.ts` + `types.ts` ergänzt — deutsche Texte
sind oben wörtlich vorgegeben, die drei anderen Sprachen sinngemäße,
natürliche Übersetzungen (keine Wort-für-Wort-Übertragung).

## Testing

- Rust: Unit-Test für `register_catalog_base_dir` (legt Verzeichnis an,
  wenn es fehlt; ist idempotent bei erneutem Aufruf mit demselben Pfad —
  über die bereits getestete `ensure_folder_path`-Logik ohnehin
  mitabgedeckt, hier nur der dünne Command-Wrapper zu prüfen).
- Frontend: `npx tsc --noEmit` sauber. Manueller Test (isolierte
  `XDG_DATA_HOME`, wie in dieser Session mehrfach praktiziert): erster
  Start zeigt den Dialog, "Später einrichten" schließt ihn dauerhaft
  (kein erneutes Aufpoppen bei App-Neustart), "Neuen Ort einrichten" mit
  leerem Testordner + anschließender Einzeldatei-Import landet
  nachweislich physisch im gewählten Ordner, "Bestehende Struktur
  übernehmen" mit einem vorbereiteten Testordner (zwei Ebenen) importiert
  sichtbar die komplette Struktur.
