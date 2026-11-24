# W2 „Bereinigung und Abgleich im Web“ – technische Spezifikation (Forgejo #39)

Stand 24.11.2026, Basis `36b5a0cd` (Branch `feat/0.19.0-w2`). Die Spezifikation wurde nur aus dem Code, dem Auftrag `AUFTRAG-0.19.0-P3.md` (Teil A), der Übersicht und dem Issue-Text #39 samt Mockup-Freigabe vom 24.11. erstellt. Es wurde nichts gebaut, getestet oder geändert. Belege stehen als `Datei:Zeile`; was ich nicht geprüft habe, ist als „nicht geprüft“ markiert.

Inhalt: 1 Ausgangslage · 2 Befehls-Vertrag · 3 CatalogBarrier · 4 Core · 5 Server · 6 Frontend · 7 Tests · 8 Arbeitspakete · 9 Risiken und Fragen

---

## 1. Ausgangslage und Abgrenzung

### 1.1 Was es heute gibt (belegt)

- **Lesender Abgleich im Web:** Befehl `disk_sync_report` (`crates/katalog-server/src/invoke.rs:201` in `READ_COMMANDS`, Sonderbehandlung `:396-440`: eigener Thread `disk-report`, Semaphore `disk_reports` = 1, `manager_read`-Platz, Reader-Guard, 30-s-Timeout `:15`). Kern: `crates/katalog-core/src/disk_report.rs` (`snapshot :67`, `build_report :324`, `presence :203`, `walk :450`). Frontend: `src/components/CatalogDiskReport.tsx` (nur Web, `Header.tsx:124`), `src/lib/api/diskSync.ts` (`diskSyncReport`, DTO `DiskReport`).
- **Grenzen des Berichts:** 500 Treffer je Liste, 200 000 Verzeichniseinträge, 20 s (`disk_report.rs:34-37`). Nur `NotFound` zählt als „fehlt“; ein Link auf dem Weg oder ein I/O-Fehler ist „unklar“ und wird weggelassen (`presence :194-220`); ein Wurzelordner, der leer ist, obwohl der Katalog dort Modelle führt, wird nicht beurteilt (`build_report :329-343`). Der Bericht nennt nie absolute Pfade (`root` = Index, `relativePath`).
- **Reihenfolge der Wurzeln:** `open_roots` sortiert nach Tiefe (`disk_report.rs:259`). Der `root`-Index im Bericht gilt für diese sortierte Liste, nicht für die Konfigurationsreihenfolge. **Das muss der Anwenden-Pfad genauso verwenden** (siehe 4.2).
- **Dateiverwaltung im Web (Stufe 7):** `KATALOG_FILE_OPS=1` (`config.rs:191`) setzt `FileOps` samt Journal auf (`fileops_startup.rs`, `prepare`, Recovery beim Start). Alle Dateibefehle laufen durch `fileops::invoke` (`fileops.rs:332`): 404 `fileops_disabled` ohne Schalter (`fileops.rs:339`, zusätzlich vorgelagert in `web.rs:582/:594` und `web.rs:817-818`), Journal gesperrt → 503 `restart_required` (`fileops.rs:346`), Passwortprüfung nur bei `Permanent`/`Start` (`:362`), `barrier.writer()` (`:377`), Import-Tor (`upload.active` + `has_importing`, `:381-396`), Sitzungsprüfung (`:397`), Laufzeitsperre `rt.state.try_lock()` (`:401`), Abbruchbedingung `operation_cancellation` (`:294`: Barriere `Open`, Sitzung gültig, 20-s-Frist).
- **Muster Token `start`/`block`:** `empty_trash_start/_block` und `trash_folder_start/_block` (`fileops.rs:524-566`, `:593-650`): Snapshot im Speicher (`RuntimeState.snapshots/folder_snapshots`, `:17-21`), gebunden an `current.token`, Gültigkeit `now - created < 3600`, höchstens 100 Snapshots (`503 snapshot_capacity`), Fehler `404 snapshot_not_found`, Zufallstoken `random_token()` (`web.rs:343`), Block-Mindestabstand 2 s (`fileops_safety.rs:118-123`).
- **Budgets und Sicherungskopien:** `fileops_safety.rs`: `DestructiveKind::{Trash,Permanent}` (`:7`), Fenster 200 je 600 s plus Tageslimit (`KATALOG_FILE_OPS_DAILY_LIMIT`, Standard 2000, `config.rs:192`) (`Budget::reserve :34`), Lademodus `Charge::{Strict,Folder,DailyOnly}` (`:18`), `reserve_folder` (`:107`, nie „unmöglich“, schreibt min(n,200) ins 10-Minuten-Fenster), `ensure_backups` (`:126`, `:160`: Tageskopie `web-fileops-day-*`, Ereigniskopie `web-fileops-event-*` höchstens alle 10 min, Fehler → `backup_failed`). Hilfen `reserve`/`backup` in `fileops.rs:311-331`.
- **Idempotenz:** Dateibefehle tragen keine Request-ID (`web.rs:1022`, nur `remove_companion`/`restore_companion` sind wiederholbar, `write.rs:123 companion_edit_command`); Schutz gegen Doppelausführung sind Token, Journal und Statusprüfungen. Das Frontend wiederholt Dateibefehle nicht automatisch (`transport.ts:15`, `OWN_OPERATION_COMMANDS`).
- **Barriere:** `barrier.rs` (siehe 3). Fremde Dateibefehle holen `writer()` (`fileops.rs:377`), Lesebefehle `reader()` (`invoke.rs:404`), Hintergrunddienste `writer()` und prüfen `snapshot().state` (Vorbild `printer_service.rs:79-119`).
- **Größen- und Zeitgrenzen:** Schreibbefehle: Body höchstens 16 KiB (`write.rs:139-146`), Anfrage-Timeout 30 s (`web.rs:635`), Session-Token-Bucket 120 Plätze, +60/s (`write.rs:169-193`, `web.rs:1332`).
- **Desktop-Gegenstück (Verhaltensvorlage, wird nicht geändert):** `scan_catalog_disk`/`apply_catalog_disk_sync` (`src-tauri/src/commands/disk_sync.rs:1705/:1715`, Anwenden unter `gate.exclusive()`, Neuscan unter Sperre `apply_with_path_mode :1417`, **Entfernen = Papierkorb ohne Datei**: `trash::delete_missing_file_with_conn` → `db::soft_delete_file(.., None, ..)` `trash.rs:3-6`, aufgerufen `disk_sync.rs:1595-1601`), `scan_catalog_issues` (`src-tauri/src/commands/backup.rs:416`, nur `NotFound` = fehlt, `is_missing :409-415`), Start-Abgleich (`disk_sync/start_sync.rs:22-186`, Einstellungen als `app_settings`-Schlüssel `sync_on_start`/`sync_auto_add` über `db::printer_link::get_setting/set_setting`, `:7-21`), Datumspräfix (`commands/date_rename.rs:5-114`, ruft schon `katalog_core::date_rename::{preview,apply}` mit `StdDateRenameFs` und eigenem `DateRenameEffect`).
- **Core hat bereits:** `date_rename.rs` (`DateRenameFs :29`, `DateRenameEffect :41`, `preview :312`, `apply :428`, `MAX_ITEMS = 5000 :144`), `fsops::FileOps::{rename_file :440, delete_file :482, trash_companion (companion_edit.rs:189)}` samt Journal, `rescan.rs` (`open_below_roots :183`, `FileStamp :234`, `prepare_web_rescan :276`: Datei unter den Wurzeln ohne Link öffnen, Stempel vergleichen), `import::files::read_web_model` (Deskriptor-basiertes Einlesen mit `WebImportLimits`, `files.rs:338`), `db::companions::delete_companion`, `db::insert_file_within_tx` (`repository.rs:653`), `db::ensure_folder_path` (`:223`).

### 1.2 Abweichungen Auftrag ↔ Code (Stand 36b5a0cd)

| Auftrag | Code |
|---|---|
| `disk_sync_report` `invoke.rs:354-378` | `invoke.rs:396-440` (Issue-Kommentar nannte `:166`) |
| `scan_catalog_issues` `backup.rs:1606` | `src-tauri/src/commands/backup.rs:416` |
| 404 „wie `web.rs:742`“ | `web.rs:582/:594`, `:817-818`, `fileops.rs:339` (`web.rs:742` ist Sitzungsverwaltung) |
| `fileops.rs:546-587` Token `start`/`block` | `fileops.rs:524-566` (Ordner) und `:593-650` (Papierkorb leeren); Zeilen verschoben |
| W4 „Core hat kein `trash_folder`“ | `trash_folder_start/_block` und `FolderTrashSnapshot` sind in `FILE_OPS_COMMANDS` (`write.rs:28-29`) und im Kern schon da; für W2 nur als Muster relevant |
| `date_rename` „P1-S6-Modul“ | liegt im Kern (`date_rename.rs`), Desktop nutzt es schon (`commands/date_rename.rs:5`) |

### 1.3 Abgrenzung: was W2 nicht tut

- Keine Hash- und keine Umbenennungserkennung im Web (Entscheidung D4, K5 nicht in W2). „Verschoben“ = eine Zeile unter „fehlt“ plus eine unter „neu“, mit Hinweistext.
- `disk_sync_report` selbst bleibt ein reiner Lesebefehl (keine Zustandsänderung, kein Token). Er erhält nur zwei kleine Anpassungen (4.2).
- Der Desktop-Abgleich (`disk_sync.rs`, 2586 Zeilen), `scan_catalog_disk`, `apply_catalog_disk_sync` und der Desktop-Dialog `CatalogDiskSync.tsx` werden **nicht** geändert und nicht mit dem Web-Modell zusammengeführt (R1 „schmaler Core-Pfad“). Desktop-Befehle bekommen nur dort Kern-Aufrufe, wo 4.1/4.3 es nennt (Verhalten identisch, durch Paritätstests belegt).
- Kein Ordner-Abgleich (fehlende Ordner, neue Ordner als eigene Zeilen); Ordnerzeilen für aufgenommene Dateien legt die Aufnahme selbst an (4.2). `missingFolders` bleibt im Start-Bericht 0.
- Kein Wiederherstellen und kein neuer Journal-Operationstyp (Journalversionen 1–3 bleiben, Version 4 bleibt reserviert, Auftrag P3 Zeile 119).
- Keine Änderung am Schema (bleibt 52), am Upload-Import, am Import-Worker.

---

## 2. Befehls-Vertrag

Alle JSON-Felder camelCase, IDs als Zeichenketten (wie die vorhandenen DTOs). Fehlerform: `{"kind":"…","message":"…","expected":bool}` wie `fileops::failure` (`fileops.rs:81-87`), plus Header `Retry-After` bei Budget/Wartezeit; Barrieren-Sperre als codierter Fehler `503 {"message":"maintenance"}` (`barrier.rs:maintenance`).

### 2.1 Übersicht (neu / geändert)

| Befehl | Art | Klasse in `command-classes.json` | Liste (Rust) | Zweck |
|---|---|---|---|---|
| `disk_sync_report` | Lesen, vorhanden | web-only (bleibt) | `READ_COMMANDS` | Bericht; Wurzeln nur `file_roots`, Ausschlüsse (4.2) |
| `disk_sync_preview` | Datei-Aktion | web-only (neu) | `FILE_OPS_COMMANDS` + `WRITE_COMMANDS` | Auswahl prüfen, Plan (Snapshot) anlegen, Token, Zusammenfassung |
| `disk_sync_apply_start` | Datei-Aktion | web-only (neu) | dito | Bestätigung, Budget, Sicherungskopie, Plan scharf schalten |
| `disk_sync_apply_block` | Datei-Aktion | web-only (neu) | dito | nächste Zeilen anwenden (je Zeile erneut prüfen) |
| `preview_date_rename` | Datei-Aktion | **web** (von B) | dito | Vorschau Datumspräfix (Desktop-Name und -Form) |
| `apply_date_rename` | Datei-Aktion | **web** (von B) | dito | Umbenennen, je Eintrag erneut prüfen |
| `scan_catalog_issues` | Lesen | **web** (von B) | `READ_COMMANDS` | fehlende Dateien und Duplikate |
| `get_sync_on_start`, `get_sync_auto_add` | Lesen | **web** (von B) | `READ_COMMANDS` | Einstellungen |
| `set_sync_on_start`, `set_sync_auto_add` | Schreiben | **web** (von B) | `WRITE_COMMANDS` (nicht `FILE_OPS_COMMANDS`) | Einstellungen speichern |
| `run_start_sync` | Lesen | **web** (von B) | `READ_COMMANDS` | Ergebnis des einmaligen Start-Abgleichs des Servers |
| `scan_catalog_disk`, `apply_catalog_disk_sync` | – | **C** (von B) | – | Desktop-Modell mit Hash-Erkennung; Web hat eigene Befehle (K5 nicht in W2) |

Listen, die bei jedem Namen gleich lauten müssen (Test `src/lib/commandParity.test.ts:26-34`): `READ_COMMANDS` (`invoke.rs:155`), `WRITE_COMMANDS` (`write.rs:33`), `FILE_OPS_COMMANDS` (`write.rs:16`, Test `fileops_http_tests.rs:626` verlangt „alle Dateibefehle sind auditierte Schreibbefehle“), `WEB_READ_COMMANDS`/`WEB_WRITE_COMMANDS` (`src/lib/readOnly.ts:4-80`), `FILE_OPS_COMMANDS` in `src/lib/transport.ts:10` und `docs/web/command-classes.json`. `ROUTE_WRITES`/`routes` ändern sich nicht. Alle Listenänderungen werden einmal in Paket P0 gemacht (8.2), damit spätere Pakete diese Dateien nicht mehr anfassen.

Ohne `KATALOG_FILE_OPS=1`: jeder der sechs Datei-Aktions-Befehle antwortet `404 {"kind":"fileops_disabled"}` (`fileops.rs:339`, vorgelagert `web.rs:582/:594`, `:817`), und zwar vor Sicherheits- und JSON-Prüfung (Test `fileops_http_tests.rs:133`, `all_fileops_disabled_before_security_or_json`: Liste dort erweitern). `/api/session` meldet `fileOps:false` (`web.rs:948`), das Frontend zeigt den Abgleich dann nur als Anzeige mit Hinweis auf `KATALOG_FILE_OPS=1`.

### 2.2 `disk_sync_report` (Lesen, Änderung)

Argumente unverändert (`{}`; `invoke.rs:289-302` lehnt alles andere ab). Antwort `DiskReport` unverändert (`disk_report.rs:149`), zusätzlich kein neues Feld. Änderungen:

1. Wurzeln: `web.rs`-Verteiler übergibt für diesen Befehl `app.config.file_roots` statt `app.config.roots` (`web.rs:~1115`; `roots` enthält ggf. den Desktop-Papierkorb `…/trash`, `config.rs:98-101`, dessen Dateien sonst als „neu“ erscheinen würden). Der Wurzelindex der Antwort gilt dann für `ordered_roots(file_roots)` (4.2).
2. Ausschlüsse: Der Upload-Ordner (`config.upload.dir`) liegt oft unterhalb einer Wurzel; seine Dateien sind Uploads, keine „neuen“ Modelle. Er wird beim Durchlauf übersprungen und als „bekannt“ behandelt (neue Core-Option `excluded`, 4.2). Ebenso `.katalog-papierkorb` (beginnt mit `.`, wird schon von `is_ignored_import_path` übersprungen, `files.rs:38`) und `_extras`-Ordner (`disk_report.rs:487`).

Ohne `KATALOG_FILE_OPS` gilt dasselbe (Bericht bleibt Nur-Anzeige).

### 2.3 `disk_sync_preview`

Request:

```json
{
  "newFiles": [{"root": 0, "relativePath": "Haus/Tor.3mf"}],
  "missingModels": ["12", "13"],
  "danglingCompanions": ["5"]
}
```

Regeln: nur diese drei Schlüssel (sonst `422 unknown_argument`, wie `fileops.rs:197`); höchstens **1500 Zeilen insgesamt** (`422 limit_exceeded`; entspricht 3 × 500 aus dem Bericht), mindestens 1 (`422 invalid_arguments`); IDs wie `id()` in `fileops.rs:211` (nur Ziffern, > 0); doppelte Einträge werden verworfen. **Body-Grenze:** `write.rs:139 body_limit` gibt 16 KiB; für `disk_sync_preview` wird sie auf 256 KiB angehoben (eigener Zweig, 1500 Zeilen ≈ 130 KiB).

Ablauf (unter `fileops::invoke`-Schutz: Writer-Guard, Import-Tor, Sitzungsprüfung, `rt.state.try_lock`), pro Zeile **ohne Schreiben**:

- Neue Datei: Wurzelindex gültig (`< ordered_roots.len()`); `relativePath` besteht nur aus normalen Komponenten (kein `..`, kein `.`, nicht leer, nicht absolut, keine NUL, höchstens 4096 Bytes und 64 Komponenten, keine Komponente beginnt mit `.` oder endet auf `.part`/`~$…`, Endung `is_model_extension`); liegt nicht in ausgeschlossenen Ordnern (Upload-Ordner, `_extras`, `.katalog-papierkorb`); Auflösung über verankerte Handles (`disk_apply::Anchored`, siehe 2.6, Abschnitt „Sichere Auflösung“): kein Link auf irgendeiner Komponente, Ergebnis ist eine reguläre Datei; `FileStamp` (Länge, mtime, dev/ino) wird vom geöffneten Handle gelesen und zusammen mit der Wurzelidentität (dev, ino) im `PlanRow` gemerkt; kein Katalogeintrag (aktiv **oder** im Papierkorb) mit demselben Pfad, verglichen ohne Rücksicht auf Groß-/Kleinschreibung und NFC/NFD (Funktion `normalized_lowercase`, `date_rename.rs:~55`, wird `pub(crate)`). Sonst Überspringen mit Grund (2.8).
- Fehlendes Modell: Zeile existiert, aktiv, Pfad liegt unter einer Wurzel, Wurzel nicht `Unreachable`/`Empty`, `presence == Missing` (nur `NotFound`). Gemerkt werden `id`, `path`, `content_hash`, `file_size_bytes`, Wurzelidentität (dev, ino).
- Begleitzeile ohne Datei: Zeile aktiv, Modell aktiv, Pfad unter Wurzel, **dieselbe Wurzelprüfung wie beim fehlenden Modell** (gemeinsame Funktion `disk_apply::root_usable`: Wurzel nicht `Unreachable`, nicht „leer obwohl der Katalog dort Modelle führt“), `presence ∈ {Missing, Other}` (über die verankerte Auflösung); gemerkt werden `id`, `file_id`, `path` und die Wurzelidentität (dev, ino); `Other` (Ordner/Link/Sonderdatei) wird beim Anwenden übersprungen (Grund `not_a_file`), steht aber in der Zusammenfassung.

Antwort `200`:

```json
{
  "token": "<32 hex>",
  "expiresIn": 3600,
  "counts": {"import": 3, "removeModels": 2, "removeCompanions": 1},
  "skipped": [{"kind": "new", "key": "0:Haus/Tor.3mf", "reason": "already_in_catalog"}],
  "mostlyMissingRoots": ["Modelle"],
  "removes": true
}
```

`key` = `"<root>:<relativePath>"` (neu) bzw. die ID (Modell, Begleitdatei). Es entstehen weder Budget-Verbrauch noch Sicherungskopie. Ein vorhandener Plan derselben Sitzung wird ersetzt (höchstens ein Plan je Sitzung und Art); insgesamt höchstens 100 (`503 snapshot_capacity`). Sind nach dem Prüfen 0 Zeilen übrig: `{"token":"","counts":{…0},"skipped":[…]}` ohne Plan (wie `empty_trash_start`, `fileops.rs:~582`).

Plan im Speicher (`RuntimeState.plans: HashMap<String, Plan>`, `Plan { session, created, rows, cursor, seq, last_result, started, confirmed_removal, applied{imported,removedModels,removedCompanions,skipped,failed} }`). `started` ist anfangs `false` und wird **ausschließlich als letzter Schritt eines vollständig erfolgreichen `disk_sync_apply_start`** (2.4 Schritt 6) auf `true` gesetzt; `confirmed_removal` wird dort mitgesetzt. Jede `PlanRow` für Modelle und Begleiteinträge trägt die Wurzelidentität (dev, ino) der Wurzel, unter der die Zeile liegt. Die Zeilen sind in fester Reihenfolge: zuerst Aufnahmen (nach Wurzel, Pfad), dann Begleiteinträge, dann Modelle.

### 2.4 `disk_sync_apply_start`

Request `{"token":"…","confirmRemoval":true|false}`. Prüfungen in dieser Reihenfolge:

1. Plan finden: unbekannt, abgelaufen (`now - created >= 3600`) oder fremde Sitzung → `404 snapshot_not_found` (dieselbe Antwort für alle drei, wie `fileops.rs:547-550`). Bereits gestartet → `409 snapshot_mismatch`.
2. Entfernen im Plan (`removeModels + removeCompanions > 0`) und `confirmRemoval != true` → `422 confirmation_required` (Muster `delete_file_permanently`, `fileops.rs:~183`).
3. Budget: **neue eigene Reservierung `FileOpsSafety::reserve_disk_sync(n)`** mit `n = removeModels + removeCompanions` (`fileops_safety.rs`, neben `reserve_folder :107`; Lademodus wie `Charge::DailyOnly`: `n > daily_limit` → `LimitExceeded` ohne jede Buchung, sonst genau `n` ins Tagesfenster, das 10-Minuten-Fenster wird nicht belastet). `reserve_folder` wird für W2 **nicht** verwendet: `Charge::Folder` setzt `impossible = false` und bucht `min(n, daily_limit)` (`fileops_safety.rs:34-45`), so dass bei Tageslimit 10 und 100 Entfernungen der Start zugelassen und nur mit 10 belastet würde, die Blöcke aber alle 100 entfernen dürften. Budgetart: `trash` (F1-A) bzw. `permanent` (F1-B), eine Zeile im Wrapper. Fehler über `budget_error`: `n > Tageslimit` → `422 limit_exceeded` (nichts gebucht, keine Sicherung); Tageslimit zurzeit ausgeschöpft → `429 rate_limited` mit `Retry-After`. Aufnahmen verbrauchen kein Budget (nichts Zerstörendes). Gebucht wird nicht erstattet (wie überall, `fileops_safety.rs`: „failures do not refund“); ein Wiederholungsversuch nach `backup_failed` bucht erneut.
4. Sicherungskopie: `backup(app, n, event)` mit `event = (Entfernen im Plan)` (Entscheidung D5; Ereigniskopie höchstens alle 10 min, sonst nur die Tageskopie, `fileops_safety.rs:160`). Fehler → `503 backup_failed`, **vor** jeder Änderung.
5. Abbruchprüfung `allowed()` (Barriere offen, Sitzung gültig) → sonst `409 not_started`.
6. Plan scharf schalten: `confirmed_removal` setzen, dann als **letzten** Schritt `started = true`. Scheitert ein Schritt 2-5, bleibt `started == false`; der Plan ist dann für Blöcke nicht ausführbar (2.5).

Antwort: `{"total": 6, "remaining": 6}`. Die Wiederholung von `apply_start` für denselben Plan wird mit `409 snapshot_mismatch` abgewiesen (kein doppeltes Budget).

### 2.5 `disk_sync_apply_block`

Request `{"token":"…","seq":0}`; `seq` zählt ab 0 und muss dem nächsten erwarteten Wert entsprechen.

**Zulassung (vor jeder Zeilenausführung, ohne Änderung bei Ablehnung).** Der allgemeine `fileops::invoke`-Schutz kennt die Plan-Zustände nicht; deshalb prüft jeder `disk_sync_apply_block` selbst, in dieser Reihenfolge, über **eine einzige Eintrittsfunktion** `fileops_disksync::plan_for_block(rt, session, token, now)` (die Zeilenschleife `block_rows` ist nur über sie erreichbar):

1. Sitzung: Plan gehört zur aktuellen Sitzung; Ablauf: `now - created < 3600`; unbekannt/abgelaufen/fremd → `404 snapshot_not_found`.
2. **Start**: `plan.started == true` (Zustand „gestartet“ am Plan, nicht nur `seq`). Ein Plan nach `disk_sync_preview` ohne `apply_start`, oder nach einem fehlgeschlagenen `apply_start` (Bestätigung, Budget, Sicherung, Abbruchprüfung), wird mit `409 not_started` abgewiesen: keine Zeile wird angefasst, `seq`, `cursor` und `last_result` bleiben unverändert, auch eine „Wiederholungsantwort“ gibt es nicht.
3. Plan mit Entfernen-Zeilen: `confirmed_removal == true` (aus `apply_start`), sonst `409 not_started` (Gürtel zum Schritt 2.4/2).
4. Erst dann `seq`-Regeln unten.

- `seq == erwartet`: nächste Zeilen anwenden (Blockgröße 2.7), Ergebnis speichern, `seq` erhöhen.
- `seq == erwartet - 1`: **Wiederholung** (z. B. Antwort verloren): gespeicherte Antwort unverändert zurückgeben, nichts erneut ausführen.
- sonst `409 snapshot_mismatch`.

Antwort:

```json
{
  "seq": 0,
  "results": [
    {"kind": "import", "key": "0:Haus/Tor.3mf", "ok": true, "fileId": "431"},
    {"kind": "import", "key": "0:Haus/Alt.3mf", "ok": false, "reason": "changed"},
    {"kind": "removeModel", "key": "12", "ok": true},
    {"kind": "removeCompanion", "key": "5", "ok": false, "reason": "exists_again"}
  ],
  "done": 4, "total": 6, "remaining": 2,
  "counts": {"imported": 1, "removedModels": 1, "removedCompanions": 0, "skipped": 2},
  "stopped": null
}
```

`stopped` ist `"maintenance"` (Barriere nicht mehr `Open`), `"session"` (Sitzung widerrufen) oder `"deadline"` (Blockfrist 12 s, 2.7); dann ist `remaining > 0` und der Rest bleibt unangetastet. Der Block gibt nach dem letzten Teilergebnis zurück, er bricht nie mitten in einer Zeile ab (eine Zeile ist je Art atomar, 2.6). Bei `remaining == 0` wird der Plan als `finished` markiert (Wiederholung der letzten `seq` bleibt bis zum Ablauf möglich). Ergebniszeilen enthalten nie Pfade, nur `key`, `fileId`, Codes.

### 2.6 Je-Zeile-Prüfung beim Anwenden (die Sicherheitsregel von W2)

Jede Zeile wird **unmittelbar vor ihrer Änderung** gegen den dann aktuellen Zustand geprüft. Abweichung vom Plan = überspringen mit Grund, nie raten.

**Sichere Auflösung (verankerte Handles, gilt für Vorschau, Anwenden und Start-Abgleich).** Alle Dateizugriffe von W2 laufen über `disk_apply::Anchored`, eine dünne Hülle um `fsops::FsContext` (`context.rs:125 new(roots, protected)`, `:248 resolve`, `:255 resolve_relative`) und `upload_fs::Directory`; `FsContext` wird mit `ordered_roots(file_roots)` (Index = Berichtsindex) und dem Upload-Ordner als `Protection::Upload` gebaut:

- Wurzel: einmal kanonisiert und mit `Directory::open` geöffnet (`upload_fs.rs:31`, `openat` je Komponente mit `O_DIRECTORY|O_NOFOLLOW`); die Identität (dev, ino) kommt in den `PlanRow`.
- Jede weitere Komponente: `Directory::open_dir` (`upload_fs.rs:236`, `openat(O_DIRECTORY|O_NOFOLLOW)`, Handle der vorherigen Komponente als Anker) mit Geräte-/Einhängungsprüfung (`context.rs:~283`); die letzte Komponente: `Directory::read(name)` (`:69`, `O_NOFOLLOW|O_NONBLOCK`) und `entry_identity(name)` (`:108`, `fstatat(AT_SYMLINK_NOFOLLOW)`). Ein Link auf **irgendeiner** Komponente ergibt einen Fehler → `outside`. Es gibt keinen Schritt, der den Pfad als Ganzes kanonisiert und danach nur die letzte Komponente prüft.
- Ausschlüsse (Upload-Ordner, Wurzeln anderer Index, Papierkorb der Wurzel) ergeben sich aus den **Identitäten der beim Auflösen geöffneten Handles** (`Protection::Upload`/`Protected` in `resolve_relative`), nicht aus einem Pfadvergleich. Namensregeln (`.versteckt`, `_extras`, `~$`) bleiben in `validate_relative`.
- Wurzel-Wiederprüfung beim Anwenden: `Directory::open(canonical)` neu öffnen und die Identität mit dem `PlanRow` vergleichen; abweichend oder nicht lesbar → `root_unavailable`.
- `rescan::open_below_roots` (`rescan.rs:183-210`) wird für W2 **nicht** übernommen und gilt nicht als Sicherheitsgarantie: es kanonisiert den ganzen Pfad (`canonicalize` folgt Zwischenlinks), prüft nur die letzte ursprüngliche Komponente auf einen Link, und `safe_file::open_regular` (`safe_file.rs:18-24`) setzt `O_NONBLOCK`, aber kein `O_NOFOLLOW`. Ein nach der Vorschau durch einen Link auf denselben Ordner ersetzter Unterordner würde dort akzeptiert (Inode, Länge, mtime bleiben gleich). `open_below_roots` bleibt für den Rescan unverändert.
- `presence` für die Entfernen-Zeilen wird über dieselbe Auflösung berechnet (`Missing` nur, wenn alle Elternkomponenten als echte Verzeichnisse geöffnet werden konnten und die letzte `NotFound` liefert; Link, `ELOOP`, andere Fehler → `Unknown`).

**Aufnahme (`import`)**
1. Wurzel noch brauchbar: Wurzelverzeichnis lässt sich lesen, Identität (dev/ino) wie im Plan, nicht „leer obwohl Katalog dort Modelle führt“ (nur für Entfernen nötig, hier genügt die Identität). Sonst `root_unavailable`.
2. Datei über die verankerte Auflösung öffnen (siehe „Sichere Auflösung“ oben; `open_below_roots` wird nicht verwendet). `NotFound` → `changed`; Link auf dem Weg, Ausschluss, Nicht-Datei oder anderer Fehler → `outside`.
3. `FileStamp` (Länge, mtime, dev/ino) mit dem Plan vergleichen; Abweichung → `changed` („Datei hat sich seit der Vorschau geändert“).
4. Lesen mit `import::files::read_web_model(file, logischer_Pfad, name, WebImportLimits)` (Limits aus `config.upload.limits`, sonst `WebImportLimits::default()`; Muster `uploads.rs:577-585`). `resource_limit` → `too_large`, jeder andere Lesefehler → `invalid`. STEP-Dateien: wie Neueinlesen (`rescan::prepare_web_rescan_with` mit `StepReader`, `rescan.rs:294`); fehlt der Worker (`gate.step.available()`, `invoke.rs:~187`), Grund `step_unavailable`. (Ob `read_web_model` selbst `.stp/.step` verarbeitet, ist **nicht geprüft**; sonst Zweig nach rescan-Muster.)
5. Schreibschritt in **einer** Transaktion (kurz, Datenbankverbindung nur dafür, Parsen liegt vor der Sperre): Pfad noch nicht im Katalog (Regel wie Vorschau, auch Papierkorbzeilen) → sonst `already_in_catalog`; Ordnerkette der Datei anlegen (`ensure_folder_chain`, 4.2; legt nur Zeilen für **bestehende** Verzeichnisse an); `db::insert_file_within_tx` (Auto-Tag-Filter wirkt wie sonst, `repository.rs`). **Vor dem Commit** (Transaktion offen, Schreibsperre gehalten) zusätzlich: (a) `FileStamp` des Handles (`metadata`) mit dem Plan vergleichen; (b) den **aktuellen Pfadeintrag** erneut über die verankerte Auflösung lösen (Wurzelidentität prüfen, Komponenten per `open_dir`, letzte per `entry_identity`) und dessen (dev, ino) mit Handle **und** Plan vergleichen (Vorbild: `apply_rescan` prüft vor dem Speichern den Pfadeintrag, `rescan.rs:350 ff.`; das Handle allein beschreibt nach einem Austausch weiter die alte Inode). Austausch, Verschwinden, Linkbildung oder jede Abweichung → Transaktion zurückrollen, `changed` (bzw. `outside` bei Link). Das verbleibende Fenster zwischen dieser Prüfung und dem `COMMIT` ist so klein wie beim Rescan und wird so dokumentiert.
6. Kein Umbenennen, Verschieben oder Löschen der Datei.

**Fehlendes Modell (`removeModel`)** – Entfernen = Papierkorb (siehe Frage F1)
1. Zeile aktiv, `path` und `content_hash` wie im Plan → sonst `changed`.
2. Wurzel: `disk_apply::root_usable` und Identität wie im Plan, sonst `root_unavailable`. `presence == Missing` **jetzt** (verankerte Auflösung); `File`/`Other` → `exists_again`; `Unknown` → `unclear`.
3. **Ausdrücklich DB-only:** `ops.remove_missing_entry(id)` (neue Funktion, 4.5/4.6), **nicht** `ops.delete_file`. `delete_file` wechselt, sobald die Quelle vorhanden ist, auf `Operation::Trash` und verschiebt die Modelldatei (`operations.rs:482-518`); bei gewöhnlichen Katalogdateien ohne Ledger-Identität verhindert `check_source` (`:255-268`) das nicht. `remove_missing_entry` löst die Quelle selbst auf und ruft `transfer` nie auf: ist die Quelle nicht `NotFound` (vorhanden, Link, Sonderdatei, anderer Fehler), endet sie mit dem stabilen Fehler „Quelle vorhanden“ (→ Grund `exists_again`; anderer Auflösungsfehler → `unclear`), ohne Journal und ohne Änderung. Nur bei `NotFound` läuft der DB-only-Pfad (`db_only`, Journal `Operation::DbOnly`, Begleitdatei-Plan). Fehler der Operation werden zu Gründen: `HasCompanions` → `has_companions`, `Protected` → `protected`, `Conflict` → `changed`, sonst `failed`. Welche Zeilenart danach entsteht (Papierkorbzustand `deleted_at` gesetzt, `trash_path` NULL, oder hartes Löschen) hängt allein von F1 ab und ist ein Parameter dieser Funktion (4.5); der Ablauf hier gilt für beide Antworten.

**Begleiteintrag (`removeCompanion`)**
1. Zeile und Modell aktiv, `path` wie im Plan → sonst `changed`.
2. **Dieselben Wurzelprüfungen wie beim fehlenden Modell**: `root_usable` (nicht leer, erreichbar) und Wurzelidentität wie im `PlanRow`, sonst `root_unavailable` (Nach Austausch der Wurzel durch ein leeres Verzeichnis liefert `presence` sonst `Missing`). Danach `presence == Missing` jetzt (`Other` → `not_a_file`, `File` → `exists_again`, `Unknown` → `unclear`).
3. **Ausdrücklich DB-only:** `ops.remove_missing_companion(id)` (neue Funktion, `companion_edit.rs`, 4.5), **nicht** `ops.trash_companion`: dieses plant einen Dateitransfer, sobald die Begleitdatei vorhanden ist (`companion_edit.rs:189-224`). Die neue Funktion löst die Datei selbst auf, ist sie nicht `NotFound` → „Quelle vorhanden“ (`exists_again`), sonst setzt sie nur die Zeile (`source: None`, journalisiert als `CompanionTrash`).

**Journal:** keine neuen Operationen. Aufnahme ändert nur die Datenbank (eine SQLite-Transaktion, bei Absturz entweder ganz oder gar nicht; nichts auf der Platte). Entfernen läuft über die neuen DB-only-Funktionen, die bestehende Journaltypen verwenden (`DbOnly`, `CompanionTrash`); die Recovery beim Start (`fileops_startup.rs`, `ops.recover()`) deckt sie ab. Ergebnis eines abgestürzten Blocks: Zeilen, deren Operation im Journal stand, sind nach der Recovery entweder abgeschlossen oder zurückgerollt; der Plan selbst ist flüchtig und nach einem Neustart weg (der Client sieht `404 snapshot_not_found`, „Erneut prüfen“ zeigt den Rest).

### 2.7 Blockgrenzen, Abbruch, Fortschritt

- Pro Block höchstens **10 Zeilen** und höchstens **12 s** Rechenzeit (Prüfung nach jeder Zeile; die Anfrage-Grenze ist 30 s, `web.rs:635`, die Abbruchfrist `operation_cancellation` 20 s, `fileops.rs:294-300`). Aufnahmen können lange dauern (Parsen großer 3MF); wer eine Zeile begonnen hat, beendet sie.
- Kein Mindestabstand zwischen Blöcken (anders als `reserve_empty_block`: das Budget wird einmal im Start verbraucht; der Session-Bucket erlaubt 60 Anfragen/s, `write.rs:180-192`).
- **Fortschritt:** Der Client schleift `disk_sync_apply_block` bis `remaining == 0` und zeigt „done von total“. Kein SSE und kein Job-Polling; die Import-Jobs (`uploads.rs`, `rt.jobs`, SSE `/api/events`) sind an das Upload-Journal gebunden und passen nicht zu Aufnahmen „an Ort und Stelle“. Das Muster entspricht `trashFolderWeb` (`src/lib/fileOps.ts:~150-190`).
- **Abbrechen:** reine Client-Sache. Der Client sendet keinen weiteren Block; schon Übernommenes bleibt, der Plan verfällt nach 3600 s oder wird durch die nächste Vorschau ersetzt. Die Abbruchlatenz ist ein Block (≤ 10 Zeilen, in der Regel wenige Sekunden). Der Mockup-Text „stoppt nach der aktuellen Datei“ wird zu „stoppt nach dem laufenden Schritt“ (siehe Frage F4).
- **Batch-Grenzen:** Vorschau ≤ 1500 Zeilen; Blöcke ≤ 10 Zeilen; Datumspräfix ≤ 200 Einträge je Anfrage (wie `fileops.rs:~223 ids()`).

### 2.8 Gründe (stabile Codes) für übersprungene Zeilen

`changed`, `exists_again`, `already_in_catalog`, `root_unavailable`, `outside` (Pfad liegt nicht unter einer Wurzel, ist ein Link, ausgeschlossen oder ungültig), `invalid` (kein gültiges Modell), `too_large`, `step_unavailable`, `has_companions`, `not_a_file`, `unclear`, `protected`, `failed`. Die Oberfläche übersetzt sie (6.5). Gründe der Vorschau (`skipped` in 2.3) verwenden dieselben Codes.

### 2.9 Fehlercodes der Befehle (Status → `kind`)

| Status | `kind` | Wann | Anzeige (Mockup-Zustand) |
|---|---|---|---|
| 404 | `fileops_disabled` | kein `KATALOG_FILE_OPS=1` | Schreibmodus aus |
| 404 | `snapshot_not_found` | Token unbekannt, abgelaufen, fremde Sitzung | **Vorschau abgelaufen (60 Min.)** |
| 409 | `snapshot_mismatch` | falsche `seq`, doppelter Start | „Zustand geändert, bitte erneut prüfen“ |
| 409 | `not_started` | Abbruchbedingung vor Beginn; **`apply_block` für einen nicht gestarteten Plan** (nach `preview` oder nach fehlgeschlagenem `apply_start`) | „nichts geändert“ |
| 422 | `confirmation_required` | Entfernen ohne Haken | Haken fehlt |
| 422 | `limit_exceeded` | Entfernen-Zeilen `n` > Tageslimit (`reserve_disk_sync`), > 1500 Zeilen | **Tageslimit erreicht** |
| 429 | `rate_limited` + `Retry-After` | Fenster/Tag ausgeschöpft | **Tageslimit erreicht** (mit Wartezeit, `fileOpsWait`) |
| 503 | `backup_failed` | Sicherung nicht möglich | „keine Änderung begonnen“ |
| 503 | `snapshot_capacity` | 100 Pläne | „Server beschäftigt“ |
| 503 | `restart_required` | Journal gesperrt (`fileops.rs:346`) | Dateiverwaltung gesperrt |
| 503 | `{"message":"maintenance"}` | Barriere `Draining`/`Exclusive` (`barrier.rs:maintenance`) | **Abgleich abgebrochen, Sicherung wird eingespielt** |
| 503 + `Retry-After` | `crate::write::retry` | Import läuft, Laufzeitsperre belegt (`fileops.rs:383-407`) | „Server beschäftigt, gleich erneut“ |
| 401 | `session_expired` | Sitzung widerrufen | Anmeldung |

Neue Texte in `src/lib/fileOpsErrors.ts` (alle vier Sprachen, Tabellen `de/en/es/fr`, `:3-6`): `snapshot_not_found`, `limit_exceeded`, `confirmation_required`, `snapshot_capacity` (die vorhandene Schlüsselzuordnung `fileOpsMessage` `:~9` fällt sonst auf `generic` zurück). Die Zuordnung `snapshot_mismatch → conflict` besteht schon.

### 2.10 `preview_date_rename` und `apply_date_rename` (Desktop-Namen, ohne Token)

Der Desktop prüft beim Anwenden jede Zeile neu und arbeitet ohne Plan-Token (`commands/date_rename.rs:90-114`, Kern `apply :428`). Das Web übernimmt dasselbe zustandslose Modell; damit bleibt `DateRenameDialog.tsx` unverändert nutzbar.

`preview_date_rename` Request `{"fileIds":["1","2"],"dateSource":"modified"|"imported"}`; 1–200 Einträge (`422 limit_exceeded` darüber; Desktop-Grenze 5000 gilt für den Desktop weiter); Antwort `[{"id","oldName","newName","status"}]` mit `status ∈ {rename, skip_has_prefix, skip_conflict, skip_missing, skip_trash, skip_too_long, skip_external, skip_no_date, skip_invalid}` (`date_rename.rs:~150-170`). Liest nur; läuft unter Writer-Guard und `rt.state` (der Dateizugriff nutzt `FsContext`-Handles, 4.3).

`apply_date_rename` Request `{"items":[{"id","newName"}]}`, 1–200 Einträge; Antwort `[{"id","status":"renamed"|"failed","reason"?}]`, Reihenfolge wie die Anfrage. `reason` sind die Kern-Kurzcodes (`missing`, `external`, `trash`, `hasPrefix`, `nameRule`, `targetTaken`, `date_rename.rs:~392-408`) oder die `fsops`-Fehlerart (`has_companions`, `conflict`, `protected`, …); nicht mehr begonnene Einträge nach Ablauf der 20-s-Frist oder bei `Draining`: `failed` mit `reason:"not_started"`. Jeder Eintrag wird einzeln über `ops.rename_file(id, stem)` ausgeführt (journalisiert, Begleitdateien ziehen mit, `operations.rs:440`).

**Bodygrenze:** `apply_date_rename` erhält in `body_limit` (`write.rs:139`) einen eigenen Zweig mit **384 KiB** (`preview_date_rename`: 200 IDs bleiben unter 16 KiB). Begründung: ein neuer Name darf höchstens `MAX_DIR_NAME_BYTES` = 255 Bytes lang sein (`import/file_names.rs:24`, geprüft in `date_rename.rs:289`; die Prüfung des Befundes mit „100 Zeichen“ war eine Näherung); Worst Case 255 Zeichen je als `\u00XX` escaped = 1530 Bytes plus ~50 Bytes Gerüst, mal 200 = rund 316 000 Bytes < 393 216. Die Grenze gilt gleichermaßen im äußeren `protect`-Layer (`web.rs:586`) und beim JSON-Lesen (`web.rs:1043`); beide rufen `body_limit`. Der Client (`DateRenameDialog.tsx`) sendet weiter eine Anfrage; Fallback nicht nötig. Test über den echten HTTP-Pfad in 7.3.

Budget: keine Zählung (wie `rename_file`, `fileops.rs:466`). Sicherung: Tageskopie (`backup(app, n, false)`) vor dem ersten Umbenennen; Fehler → `503 backup_failed` ohne Änderung. Idempotenz: kein Token nötig; ein zweiter Aufruf mit denselben Namen scheitert je Eintrag an `hasPrefix` (Kern-Test `date_rename/tests.rs:380 running_it_twice_skips_everything_the_second_time`), das Dateisystem bleibt unverändert.

### 2.11 `scan_catalog_issues`, Einstellungen, `run_start_sync`

- `scan_catalog_issues` (Lesen, `{}`): `{"orphaned": ModelFileDto[], "duplicateGroups": ModelFileDto[][], "truncated": bool}`. Wie der Desktop (`backup.rs:416-440`): „fehlt“ nur bei `NotFound`; Zeilen unter keiner Wurzel, hinter Links oder bei I/O-Fehlern sind unklar und fehlen in der Liste. Grenzen: 500 fehlende Modelle, 200 Duplikatgruppen, 20 s (Treffer darüber: `truncated:true`). Läuft wie `disk_sync_report` (eigener Thread, `disk_reports`-Semaphore, `manager_read`, Reader-Guard). **Nicht geprüft:** ob `db::list_files` Papierkorbzeilen und Bild-Blobs liefert; für das Web wird eine schlanke Abfrage (nur aktive Zeilen, Spalten für Pfad und Hash) verlangt, die Bild-DTO nur für die Treffer baut.
- `get_sync_on_start`, `get_sync_auto_add`: `bool` (fehlender Schlüssel = `false`).
- `set_sync_on_start {enabled}`, `set_sync_auto_add {enabled}`: wie `set_source_check_on_start` (`write.rs:765`, Request-ID-Schutz über `idempotent_write`, 16-KiB-Grenze reicht). Abhängigkeit der Einstellungen untereinander entscheidet die Oberfläche (Aus bei „Start“ löscht auch „automatisch aufnehmen“, wie `DiskSyncSettings.tsx:~58`); der Server speichert beide Schlüssel unabhängig und wertet „automatisch“ nur zusammen mit „Start“ aus (wie Desktop `start_sync.rs:138-140`).
- `run_start_sync` (Lesen, `{}`): Ergebnis des **einen** Laufs nach dem Serverstart (5.4). Form wie Desktop `StartSyncResult` (`start_sync.rs:42-55`, `src/lib/api/diskSync.ts` `StartSyncResult`) plus `state`: `{"ran":bool,"busy":bool,"state":"off|running|done|skipped","newFiles","renamed":0,"missingFiles","missingFolders":0,"duplicates":0,"unreachableRoots","applied":{"added","renamed":0,"failed"}|null}`. Solange der Lauf läuft: `busy:true, state:"running"`. Ausgeschaltet oder kein `KATALOG_FILE_OPS`: `ran:false, state:"off"`. Der Befehl löst nie einen Lauf aus und ist deshalb ein reiner Lesebefehl.

---

## 3. CatalogBarrier

Regeln aus `barrier.rs:1-20`: Schreiber holen `writer()` **bevor** sie in die Warteschlange gehen und tragen den Guard in den Worker; Leser `reader()`; innerhalb eines zugelassenen Jobs nie einen zweiten Guard holen; kooperative Jobs prüfen `snapshot().state` zwischen Arbeitseinheiten und behalten ihren Guard bis nach Rollback und Journalfortschreibung. Draining sperrt neue Schreiber, Leser laufen weiter; Exclusive sperrt alles (`barrier.rs:80-97`).

| Lauf | Guard | Verhalten bei `Draining`/`Exclusive` |
|---|---|---|
| `disk_sync_report`, `scan_catalog_issues` | `reader()` (`invoke.rs:404`, Muster bleibt) | Leser dürfen in `Draining` zu Ende laufen; ein neuer Aufruf in `Exclusive` → `503 maintenance` |
| `get_sync_*`, `run_start_sync` | `reader()` bzw. gar keiner (Speicherabfrage) | wie Lesebefehle |
| `disk_sync_preview`, `…_apply_start`, `…_apply_block`, `preview_date_rename`, `apply_date_rename` | **`writer()`** in `fileops::invoke` (`fileops.rs:377`), Guard wandert in den `spawn_blocking`-Worker | Neuer Aufruf in `Draining` → `503 maintenance`, nichts geändert. Läuft ein Block, prüft er **nach jeder Zeile** `snapshot().state != Open` über `allowed()` (`operation_cancellation`, `fileops.rs:294-300`), beendet die Zeile, gibt die Teilergebnisse mit `stopped:"maintenance"` zurück und gibt den Guard frei. `apply_date_rename`: restliche Einträge `failed/not_started`. |
| Start-Abgleich des Servers | `writer()` einmal je Lauf (Vorbild `printer_service.rs:92-109`), Guard im `spawn_blocking`; ein Beobachter setzt das Abbruchflag, sobald `state != Open` (`printer_service.rs:110-118`) | Wird der Guard nicht erteilt (Wartung läuft), startet der Lauf nicht und merkt `state:"skipped"`. Läuft er und eine Sicherung beginnt, bricht er nach der aktuellen Zeile ab, schon Aufgenommenes bleibt, Bericht mit `state:"done"` und `aborted:true` (nur im Protokoll). |

Das Einspielen (`restore.rs:87-114`) ruft `enter_draining` (wartet auf alle Schreiber, Zeitgrenze `WAIT` 30 s, `restore.rs:8`), danach `enter_exclusive`. Ein laufender W2-Block hält deshalb höchstens bis zum Ende seiner aktuellen Zeile (≤ einige Sekunden; große Aufnahmen können länger dauern, dann `RestoreDrainTimeout`, die Barriere öffnet sich wieder, nichts wird ersetzt). Pläne liegen im Speicher und sterben mit dem Prozess; nach erfolgreichem Einspielen (Neustart, `complete()`) ist jeder Token ungültig.

**Test (Pflicht, Auftrag P3 Zeile 26):** Ein Start-Abgleich (bzw. ein Block) läuft mit einem durch `walk_hook`/Testhaken verlangsamten Schritt; gleichzeitig `barrier.enter_draining(..)`. Erwartet: der Austausch wartet bis der Lauf nach der laufenden Zeile abbricht oder endet; neue Blöcke bekommen `503 maintenance`; Zählerstand der Barriere geht auf `writers == 0`; schon Aufgenommenes steht im Katalog. Dazu zwei kleine Einheitentests: (a) Guard liegt im Worker bis zum echten Ende (Muster `barrier.rs:detached_blocking_writer_counts_until_real_completion`), (b) `Draining` zwischen zwei Blöcken → zweiter Block 503, erster Block unversehrt.

---

## 4. Core-Änderungen (`crates/katalog-core`)

Leitlinie (R1): Logik, Prüfungen und SQL kommen in den Kern, Dateieffekte bleiben je Plattform. Desktop-Code ändert sich nur an den ausdrücklich genannten Stellen; vorher/nachher laufen die vorhandenen Desktop-Tests unverändert.

### 4.1 `catalog_issues.rs` (A1, neu)

```rust
pub trait MissingProbe { fn is_missing(&self, path: &Path) -> std::io::Result<bool>; }
pub struct StdMissingProbe;                       // wie backup.rs:409: metadata() == NotFound
pub struct RootsMissingProbe<'a> { /* roots */ } // Web: nur unter Wurzeln, presence()-Regeln, Link/Fehler = unklar
pub struct Issues { pub orphaned: Vec<i64>, pub duplicate_groups: Vec<Vec<i64>>, pub truncated: bool }
pub struct IssueLimits { pub max_orphaned: usize, pub max_groups: usize, pub max_duration: Duration }
pub fn scan(conn: &Connection, probe: &dyn MissingProbe, limits: IssueLimits) -> Result<Issues, String>;
pub fn group_duplicates(files: Vec<FileRecord>, orphaned: &HashSet<i64>) -> Vec<Vec<FileRecord>>; // aus backup.rs:350 verschoben
```

Verschoben werden `group_duplicates` und `is_missing`-Logik aus `src-tauri/src/commands/backup.rs:350-415`. `scan_catalog_issues` im Desktop ruft `catalog_issues::scan` mit `StdMissingProbe` und `IssueLimits` ohne Grenzen (`usize::MAX`, ohne Zeit) und baut weiter `to_dto(file, &spools)`; Ergebnis byte-gleich (Paritätstest 7.2). `RootsMissingProbe` wiederverwendet `disk_report::presence` (wird `pub(crate)`).

### 4.2 `disk_report.rs` (Erweiterungen) und `disk_apply.rs` (neu)

`disk_report.rs`:
- `pub fn ordered_roots(configured: &[PathBuf]) -> Vec<PathBuf>`: dieselbe Reihenfolge wie `open_roots` (`:250-261`), gemeinsam benutzt von Bericht und Anwenden.
- `pub struct ReportOptions<'a> { pub limits: Limits, pub excluded: &'a [PathBuf] }` und `pub fn build_report_with(snapshot, configured, options) -> DiskReport`; `build_report` bleibt (Aufruf mit leerem `excluded`). Ausgeschlossene Ordner (kanonisch) werden im Durchlauf nicht betreten (Dateien darin gelten nicht als „neu“).
- `presence`, `Presence`, `locate`, `open_roots`-Hilfen werden `pub(crate)`.

`disk_apply.rs` (neu, reine Logik, keine Plattformabhängigkeit außer Unix-Stempeln):

```rust
pub struct FileStamp { len: u64, modified_ns: i128, dev: u64, ino: u64 }   // aus rescan.rs:234 verallgemeinert
pub enum RowKind { New, MissingModel, DanglingCompanion }
pub struct PlanRow { kind: RowKind, key: String, /* je Art: Wurzel+relativ+Stempel | id+path+hash | id+file_id+path */ }
pub enum SkipReason { Changed, ExistsAgain, AlreadyInCatalog, RootUnavailable, Outside, Invalid, TooLarge, StepUnavailable, HasCompanions, NotAFile, Unclear, Protected, Failed }
pub fn validate_relative(path: &str) -> Result<PathBuf, SkipReason>;                  // 2.3-Regeln
pub fn plan_new_file(conn, roots: &[PathBuf], excluded: &[PathBuf], root: usize, rel: &str) -> Result<PlanRow, SkipReason>;
pub fn plan_missing_model(conn, roots, id: i64) -> Result<PlanRow, SkipReason>;
pub fn plan_dangling_companion(conn, roots, id: i64) -> Result<PlanRow, SkipReason>;
pub fn recheck_missing(row: &PlanRow, conn, roots) -> Result<(), SkipReason>;          // 2.6, ohne zu schreiben
pub fn recheck_dangling(row: &PlanRow, conn, roots) -> Result<(), SkipReason>;
pub fn read_new_file(row: &PlanRow, roots, limits: WebImportLimits, step: Option<StepReader>) -> Result<PreparedNew, SkipReason>; // öffnen, Stempel, read_web_model
pub fn store_new_file(conn: &Connection, prepared: PreparedNew) -> Result<i64, SkipReason>;  // Pfad-Kollision, ensure_folder_chain, insert_file_within_tx
pub fn ensure_folder_chain(conn: &Connection, root_dir: &Path, dir: &Path) -> Result<i64, String>; // legt nur Zeilen an, nie Verzeichnisse
```

`ensure_folder_chain` ist `db::ensure_folder_path` (`repository.rs:223`) plus die Regel „nur Zeilen für bestehende Verzeichnisse“ (das Verzeichnis wurde im Bericht gelesen, der Kern legt nie etwas auf der Platte an). `rescan.rs`: nur `FileStamp` wird nach `disk_apply` verallgemeinert bzw. `pub(crate)` (Verhalten gleich, Rescan-Tests unverändert). `open_below_roots` wird **nicht** wiederverwendet und bleibt unverändert (siehe 2.6, „Sichere Auflösung“: kein komponentenweiser Schutz); `disk_apply` bekommt `Anchored` (Hülle um `FsContext`/`upload_fs::Directory`) mit `open_file`, `entry_identity`, `presence`, `root_identity`, `root_usable`.

Die Funktionen bekommen Verbindung und Wurzeln übergeben und halten keine Sperre; die Zeitpunkte (Parsen vor der DB-Sperre, Schreiben kurz) bestimmt der Server (5.1). Entfernen läuft **nicht** hier, sondern über die bestehenden `FileOps`-Operationen (4.4).

### 4.3 `date_rename.rs` (Web-Anschluss, rückwärtskompatibel)

Heute prüft der Kern den Ort „innerhalb der Katalogordner“ mit der DB-Einstellung `catalog_base_dir` (`catalog_base :69`, `eligibility :247-267`: kanonischer Pfad liegt unter dem Basisordner, sonst `SkipExternal`). Im Docker kann diese Einstellung ein Desktop-Pfad sein (aus importiertem Katalog) → alles wäre „extern“. Änderungen:

```rust
pub trait DateRenameFs {
    // neu, mit Standardwert: Some(true/false) ersetzt die Basisordner-Regel; None = bisheriges Verhalten
    fn within_catalog(&self, _path: &Path) -> Option<bool> { None }
    // bisherige Methoden unverändert
}
pub struct VerifiedRename { pub file: FileRecord, pub old_path: PathBuf, pub new_path: PathBuf, pub stem: String }
pub fn verify_item(fs: &dyn DateRenameFs, conn: &Connection, item: &DateRenameItem, base: Option<&Path>, sensitive: &[PathBuf]) -> Result<VerifiedRename, String>;
```

`verify_item` ist der Teil von `apply_one` bis vor `effect.rename` (Zustand, Papierkorb, fehlt, extern, Namensregel `follows_rule`, `targetTaken`, `validate_name`, sensible Ordner); `apply_one` ruft `verify_item` und danach den `DateRenameEffect` (Verhalten für Desktop identisch). Der Server nutzt `verify_item`, danach `FileOps::rename_file(id, stem)` mit `stem` = neuer Name ohne die Endung des alten (`fs_portable::names::rename_name(old, base)` hängt die Endung wieder an, `names.rs:32`). `StdDateRenameFs` bleibt unverändert (`within_catalog → None`). Der Web-Adapter (`WebDateRenameFs`, im Server, 5.2) liest über `FsContext`-Handles: `entry` = `Directory::entry_identity` (`upload_fs.rs:108`, ohne Link zu folgen), `modified` = `Directory::read(name)` → `metadata().modified()` (O_NOFOLLOW; ein Link als Datei ergibt „kein Datum“, strenger als der Desktop), `canonicalize` liefert den Pfad unverändert, wenn `FsContext::resolve(.., Access::Source)` ihn auflöst, sonst Fehler; `within_catalog` = `FsContext::locate(path).is_ok()`. `sensitive` ist im Web leer (die Schutzregeln liefert `FsContext` über `Protection`).

Die Sperre „höchstens 5000“ (`MAX_ITEMS`) bleibt im Kern; das Web erzwingt 200 im Server.

### 4.4 Start-Einstellungen (`disk_sync_settings.rs`, neu, klein)

`pub const SETTING_ON_START = "sync_on_start"`, `SETTING_AUTO_ADD = "sync_auto_add"`, `fn flag(conn, key)`, `fn set_flag(conn, key, bool)` (Inhalt aus `disk_sync/start_sync.rs:7-21`). Der Desktop ruft diese Funktionen (Verhalten gleich, gleiche Schlüssel, damit exportierte/importierte Kataloge übereinstimmen).

### 4.5 Was NICHT angefasst wird

`src-tauri/src/commands/disk_sync.rs` (außer nichts), `disk_sync/start_sync.rs` (nur Konstanten/`flag`/`set_flag` auf Kern umgestellt), `commands/date_rename.rs` (unverändert, nutzt weiter `apply`), `import/` (Importmaschine, Worker, `CatalogGate`), `catalog_restore`, `barrier`. `fsops/` bleibt unverändert **bis auf die kleine Schnittstellenänderung in 4.6** (Codex-Befund W2-03). Keine neue Journalart, keine neue Operation.

### 4.6 `fsops`: DB-only-Pfad „Eintrag entfernen“ (neu, klein)

Problem: `delete_file` (`operations.rs:482-518`) und `trash_companion` (`companion_edit.rs:189-224`) sind für vorhandene Dateien Dateitransfers (`Operation::Trash`, `CompanionTrash` mit Quelle). Taucht eine Datei zwischen der Wiederprüfung und dem Operationsbeginn wieder auf, würde der Abgleich sie in den Papierkorb verschieben, obwohl er nur fehlende Einträge entfernen soll. Lösung (zwei neue `pub`-Funktionen, kein neuer Journaltyp):

```rust
// fsops/operations.rs, neben delete_file (:482)
pub fn remove_missing_entry(&mut self, id: i64, mode: MissingRemoval) -> Result<()>;
// fsops/companion_edit.rs, neben trash_companion (:189)
pub fn remove_missing_companion(&mut self, companion_id: i64) -> Result<()>;
pub enum MissingRemoval { Trash /* deleted_at gesetzt, trash_path NULL (F1-A) */, Delete /* Zeile hart löschen (F1-B), db_only(before, None, ..) */ }
```

Vertrag: Die Funktion löst die Quelle selbst auf (`FsContext::resolve(.., Access::Source)`, verankert). **Nur bei `Error::NotFound`** läuft `db_only`; jedes andere Ergebnis (vorhanden, Link, Sonderdatei, anderer Fehler) bricht ab. Für „vorhanden“ wird ein eigener Fehler `Error::SourcePresent` in `fs_portable/mod.rs:Error` (`:23`) eingeführt und in `error_code.rs` sowie dem Fehlermapping des Servers (`fileops.rs`) auf `exists_again` abgebildet (Alternative nur mit Begründung: `Error::Changed` weiterverwenden und per Test festnageln). Weder `transfer` noch `Operation::Trash` sind aus diesen Funktionen erreichbar (strukturelle Garantie, kein „Prüfen und hoffen“). `HasCompanions` bleibt (`db_only`, `operations.rs:280-283`). Das Fenster zwischen Auflösung und DB-Commit bleibt, ist aber harmlos: Es wird nur die Datenbank geändert, die Datei auf der Platte bleibt unberührt und erscheint beim nächsten Abgleich wieder unter „Neu“. **F1 bleibt offen:** der Parameter `MissingRemoval` ist die einzige Stelle, die von der Antwort abhängt (`Trash` = Papierkorbzustand wie der Desktop; `Delete` = hart löschen, dann Budgetart `permanent` und Passwort laut F3); der Rest des Vertrags ist für beide Antworten gleich.

---

## 5. Server-Änderungen je Datei

### 5.1 `crates/katalog-server/src/fileops.rs` (A2, A3) – sicherheitskritisch

- `RuntimeState` bekommt `plans: HashMap<String, Plan>` (neben `snapshots`, `folder_snapshots`, `fileops.rs:17-21`). `Plan` wie 2.3.
- `Command` (`:153`) und `parse` (`:170`) bekommen `DiskPreview`, `DiskApplyStart(token, confirm)`, `DiskApplyBlock(token, seq)`, `DatePreview`, `DateApply`. Schlüsselliste je Befehl wie in 2.3-2.5 und 2.10 (`unknown_argument` bei Fremdschlüsseln); die ID-Prüfung `id()`/`ids()` (`:211`, `:230`) wird für die 200er-Grenze mitbenutzt.
- `execute` (`:447`): neue Arme. Reihenfolge je Arm wie 2.3-2.6. Hilfen `reserve_folder_trash`, `backup`, `eligible` bleiben; neu `fn block_rows(...)`, das pro Zeile `allowed()` prüft und `progress.set_file_ids(..)`/`set_companion_ids(..)` füllt (Audit, `write.rs:201-210`), `completed_items` (`:720`) bekommt Arme für die neuen Antworten.
- Die Aufnahmen laufen im selben `execute`-Aufruf: **Lesen/Parsen vor** dem Datenbankzugriff (`disk_apply::read_new_file`, ohne DB-Sperre), dann `ops.db.with_conn(|c| disk_apply::store_new_file(c, prepared))` (kurze Sperre, `DbAdapter` `fileops_startup.rs:~20-37`, mit `cancellation` wie `RemoveFolder`, `fileops.rs:~512-522`). Entfernen: `ops.remove_missing_entry(id, mode)`/`ops.remove_missing_companion(id)` (4.6; **nicht** `delete_file`/`trash_companion`), danach `note_journal` über den vorhandenen Abschluss (`:411-415`: blockiert das Journal, setzt `rt.blocked`).
- `plan_for_block` (2.5) ist die einzige Eintrittsfunktion in die Zeilenschleife: Sitzung, Ablauf, `started`, `confirmed_removal` werden dort geprüft; `fileops::invoke` allein genügt dafür nicht.
- Der Import-Gate-Schutz (`:381-396`) gilt unverändert: Läuft ein Upload-Import, antwortet jeder W2-Befehl `503 Retry-After` (`write::retry`), der Client versucht es später.
- Upload-Ledger: Aufnahme berührt es nicht; Entfernen eines Modells, das aus einem Upload stammt, geht über `FileOps::remove_missing_entry`, das über `db_only`/`finish` dasselbe `ledger.detach` ausführt wie `delete_file` (`operations.rs:269-277`; Verhalten für `reset_imported` beim Bau gegen `delete_file` prüfen) (`UploadLedger`, `operations.rs:24-33`, `fileops_startup.rs:~68-100`).
- Prüfungsreihenfolge und Fehler wie 2.9. Kein Passwort nötig (kein endgültiges Löschen; das Entfernen geht in den Papierkorb; siehe Frage F1 für die Alternative).

### 5.2 Neue Datei `crates/katalog-server/src/fileops_disksync.rs` (A2, A3)

Enthält die Planverwaltung (Plan anlegen/ersetzen/Ablauf, `seq`-Wiederholung), `WebDateRenameFs` (4.3) und die Zeilenschleife; `fileops.rs` bleibt dünn. Registrierung in `lib.rs`.

### 5.3 `write.rs`, `invoke.rs`, `web.rs`, `config.rs`

- `write.rs`: `FILE_OPS_COMMANDS` (`:16`) und `WRITE_COMMANDS` (`:33`) um die sechs Befehle (2.1); `WRITE_COMMANDS` zusätzlich um `set_sync_on_start`, `set_sync_auto_add`; `body_limit` (`:139`) Sonderfall `disk_sync_preview` = 256 KiB; `execute` (`:757`) Arme für `set_sync_*` (Kern `disk_sync_settings::set_flag` im Transaktionsrahmen wie `set_auto_tagging`; Änderungszähler 1).
- `invoke.rs`: `READ_COMMANDS` (`:155`) um `scan_catalog_issues`, `get_sync_on_start`, `get_sync_auto_add`, `run_start_sync`; Argumentprüfung (`:289-302`) für diese vier („keine Argumente“); `scan_catalog_issues` mit denselben Mitteln wie `disk_sync_report` (`:396-440`: Manager-Platz, `disk_reports`-Semaphore, eigener Thread, Reader-Guard, Timeout, Fehler `RescanRunning/RescanUnavailable/RescanTimeout`); `get_sync_*` über `db::open_read_only` + `get_setting`. `GeometryGate` (`:20-33`) bekommt `start_sync: Arc<StartSyncStore>`; `run_start_sync` liest daraus (kein DB-Zugriff). `disk_sync_report`: Wurzeln und `excluded` kommen vom Aufrufer (`GeometryGate`/Signatur von `invoke` erhält `report_roots`/`excluded` oder ein kleines Struct; Entscheidung des Bauers, solange `web.rs` `file_roots` und `upload.dir` liefert).
- `web.rs`: (a) `disk_sync_report` bekommt `file_roots` und `excluded` (Upload-Ordner) (siehe 2.2); (b) nach `printer_service::start(&app)` (`web.rs:454`) wird `crate::disk_start_sync::start(&app)` aufgerufen (5.4); sonst keine Routenänderung, denn die sechs Befehle laufen über `FILE_OPS_COMMANDS` (`web.rs:1079-1083`) durch `fileops::invoke`. `/api/session` bleibt (`fileOps` kommt schon aus `capability`).
- `config.rs`: kein neuer Schalter. `KATALOG_FILE_OPS`, `KATALOG_FILE_OPS_DAILY_LIMIT` und `KATALOG_FILE_OPS_ACK` gelten unverändert (`config.rs:191-194`, `:620-625`). Optional (nicht nötig): Umgebungsvariable für die Blockgröße ist **nicht** vorgesehen.

### 5.4 Start-Abgleich: neue Datei `crates/katalog-server/src/disk_start_sync.rs` (A4)

Aufruf: `start(&Arc<App>)` direkt hinter `printer_service::start(&app)` (`web.rs:454`). Zu diesem Zeitpunkt sind Journal-Recovery (`fileops_startup::prepare` im `build_app_with_writer`, `web.rs:425-427`) und `uploads::reconcile` (`:428`) schon gelaufen; die Funktion kehrt sofort zurück und startet einen Hintergrundtask.

Bedingungen (alle nötig, sonst `StartSyncStore` auf `off`): `app.config.file_ops && app.config.write`, `app.fileops` ist `Some` und `available()`, Einstellung `sync_on_start` ist wahr (gelesen mit kurzer Verbindung). Der Task hält nur eine **schwache** Referenz auf `App` (Vorbild `printer_service.rs:85-91`), wartet 5 s (Start der Hauptdienste), holt `barrier.writer()` (sonst `skipped`), nimmt den Import-Platz (`upload.active.try_acquire_owned()`, `has_importing()`; belegt → `busy`, einmal nach 60 s erneut versuchen, danach `skipped`), `rt.state.try_lock()` (belegt → ebenso), dann `spawn_blocking`:

1. `snapshot` + `build_report_with(..)` über `ordered_roots(file_roots)` mit Ausschlüssen (Grenzen wie der Bericht: 500/200 000/20 s).
2. Nicht erreichbare Wurzel (`status != Ok`) → nur zählen: `unreachableRoots`, **nichts anwenden** (wie Desktop `start_sync.rs:146-148`).
3. Ist `sync_auto_add` aktiv: bis zu **200** neue Dateien über dieselben Funktionen wie `disk_sync_apply_block` (Plan intern, Stempel aus dem eben gelesenen Zustand; je Zeile `read_new_file` → `store_new_file`), zwischen den Zeilen Abbruch bei `barrier.snapshot().state != Open` oder Ablauf der Serverfrist (120 s). Weitere neue Dateien bleiben offen und werden als „Weiter offen“ gezählt. **Es werden nie Einträge entfernt**, kein Budget, keine Sicherungskopie nötig (nur Hinzufügen; die Tageskopie wird trotzdem ensured: `backup(count, false)`, Fehler → Lauf endet ohne Änderung, `skipped`).
4. Ergebnis in `StartSyncStore` (`Mutex<StartSyncState>`; `state: running|done|skipped|off`, Zahlen wie 2.11). Der Bericht liegt nur im Speicher und ist bis zum Neustart abrufbar. Es gibt **keine** Änderung ohne Einstellung.

Der Lauf geschieht einmal je Serverstart (kein Intervall). Das Frontend fragt `run_start_sync` nach der Anmeldung ab, solange `busy:true` ist (6.4). Für jede Aufnahme gilt dieselbe Zeilenprüfung wie im manuellen Anwenden (2.6); dieselben Funktionen, nicht kopiert.

### 5.5 Nicht geändert (und eine kleine Ausnahme)

`fileops_safety.rs` bekommt genau eine Funktion `reserve_disk_sync(n)` (2.4 Schritt 3; `reserve_folder` bleibt unverändert und wird von W2 nicht benutzt). Unverändert:

`barrier.rs`, `restore*.rs`, `uploads.rs`/`upload_store.rs`, `idempotency.rs`, `fileops_startup.rs`, `fileops_config.rs`, `download.rs`.

---

## 6. Frontend-Änderungen (A5)

### 6.1 Transport und Listen

`src/lib/transport.ts:10` `FILE_OPS_COMMANDS` + die sechs Datei-Aktions-Befehle; `src/lib/readOnly.ts` `WEB_READ_COMMANDS` (+ `scan_catalog_issues`, `get_sync_on_start`, `get_sync_auto_add`, `run_start_sync`) und `WEB_WRITE_COMMANDS` (+ sechs Befehle, `set_sync_on_start`, `set_sync_auto_add`); `docs/web/command-classes.json` laut 2.1. Alle sechs laufen als „eigene Operation“ ohne Request-ID und ohne automatische Wiederholung (`transport.ts:15`); die Fehlerauswertung ist `fileOpsError`/`fileOpsMessage` (`src/lib/fileOps.ts`, `fileOpsErrors.ts`). Neu `src/lib/fileOps.ts`: `runDiskSync(selection, onProgress, shouldStop)` nach dem Muster `trashFolderWeb` (`:~160-195`): Vorschau, Start, Blockschleife mit `seq`, Wiederholung desselben `seq` nach Netzfehler (höchstens 2-mal), Abbruch zwischen Blöcken, Ergebnis `{imported, removedModels, removedCompanions, skipped[], aborted:'user'|'maintenance'|'session'|null}`. Bei Antwort `503 maintenance` bricht die Schleife ab und meldet „abgebrochen, Sicherung wird eingespielt“ (Barriere `Draining`), Zähler bleiben erhalten.

### 6.2 Komponenten

- `src/components/CatalogDiskReport.tsx` wird erweitert (nicht ersetzt) und bei Bedarf aufgeteilt: `CatalogDiskReport` (Knopf + Dialograhmen, Zustandsmaschine 6.3), `DiskReportSelect` (drei Abschnitte, Zähl-Chips, „Alle auswählen/Keine“ je Abschnitt, Zeilen mit Kontrollkästchen), `DiskReportPreview` (Zusammenfassung, Hinweise, Haken „Verstanden“), `DiskReportRun` (Fortschritt, „Abbrechen“), `DiskReportResult` (Zusammenfassung, übersprungene Zeilen mit Grund, „Erneut prüfen“, „Schließen“). Ohne Schreibmodus (`useCanManageFiles()` falsch, `webSession.ts:73-74`) bleibt es bei der heutigen Nur-Anzeige (`diskReportReadOnly`) plus neuem Hinweis auf `KATALOG_FILE_OPS=1`.
- Vorauswahl laut Entscheidung: „Neu“ und „Begleitdateien ohne Datei“ vorausgewählt, „Auf der Festplatte nicht mehr vorhanden“ **nicht**. Wurzeln mit Status `unreachable`/`empty` liefern ohnehin keine Zeilen (Kern). Abschnitt „fehlt“ zeigt den Hinweis zu Verschiebungen (Tags/Favorit/Verbrauch werden nicht übernommen).
- „Weiter (n Änderungen)“ gesperrt bei 0. Im Schritt Vorschau: Zusammenfassung je Aktion, der Satz „Jede Zeile wird beim Übernehmen noch einmal geprüft …“, Haken „Verstanden“ nur, wenn Entfernen im Plan (Knopf „Änderungen übernehmen“ bleibt ohne Haken gesperrt; der Server verlangt ihn zusätzlich, 2.4), Hinweis auf die Sicherungskopie.
- `DateRenameDialog.tsx`: unverändert, außer `filesApi.DATE_RENAME_MAX_ITEMS` (`src/lib/api/files.ts:39`) wird im Web 200 (`isReadOnly() ? 200 : 5000`), und `onDateRenamed` wird im Web nicht mehr abgeschaltet (`App.tsx:857`). Der Menüeintrag erscheint im Web nur mit `useCanManageFiles()` (`BulkActionToolbar.tsx:335`, `selectedIds && onDateRenamed`). Reihenfolge der Quelle (Änderungsdatum/Importdatum), Tabelle alt → neu und Gründe sind schon da (`dateRenameSkip*`, `DateRenameDialog.tsx:12-23`). Das `DateRenameBanner` (`App.tsx:1091`) liefert den Ergebnis-Hinweis.
- Einstellungen: `DiskSyncSettings.tsx` wird im Web mit eigenen Texten (`webSync*`, 6.5) gezeigt, nur wenn `fileOps` (`useCanManageFiles()`); die bekannten Kopien (`countKnownCopies` usw.) sind Desktop-only und werden im Web ausgeblendet. Ein Aufruf in den Katalog-Reiter der Einstellungen (genaue Stelle im Web-Zweig von `SettingsPanel.tsx`: **nicht geprüft**, der Wartungsreiter-Web-Zweig ist `:~570`).
- Start-Hinweis: `useStartSync` (`src/hooks/useStartSync.ts`) ist heute auf dem Web ausgeschaltet (`App.tsx:276 !web && …`). Im Web `ready = web && canManageFiles() && Katalog geladen`, der Hook fragt `runStartSync()` ab, wiederholt alle 5 s bis `!busy` (höchstens 24 Mal), zeigt dann `StartSyncBanner` („Beim Start gefunden/aufgenommen …“, Knöpfe „Ansehen“ öffnet den Abgleich-Dialog über `requestDiskSync`/`onDiskSyncRequest`, `diskSync.ts`; „Später“ schließt). `CatalogDiskReport` hört im Web auf dieses Ereignis.
- Bereinigung: der Knopf (`ToolsSection.tsx:~180`: `VITE_TARGET !== 'web'`) und der Wartungsabschnitt (`SettingsPanel.tsx:~570`) werden im Web freigeschaltet, `useCatalogCleanup` nutzt `scan_catalog_issues`; das Löschen im Web ist **ausdrücklich neu anzubinden**. Heute ruft `useCatalogCleanup.ts:32-47` `filesApi.deleteFiles`, das unmittelbar `invoke('delete_files', {fileIds})` sendet (`src/lib/api/files.ts:49`); weder `fileOpsBatch` noch `checkResults` (`fileOps.ts:53/:90`) werden aufgerufen. Folgen: mehr als 200 IDs werden vom Server abgewiesen (`fileops.rs:175`), und bei HTTP 200 mit einzelnen fehlgeschlagenen Ergebnissen behandelt der Hook alle IDs als gelöscht und schließt den Dialog. Vertrag für den Web-Zweig: `deleteSelectedCleanupFiles` ruft im Web `fileOpsBatch('delete_files', ids, …)` (Blöcke ≤ 200, Auswertung je Ergebnis über `checkResults`/`ItemResult`; Desktop-Zweig unverändert) bzw. eine gleichwertige Funktion, die je Block die Einzelergebnisse zurückgibt. `onDeleted` bekommt **nur die erfolgreich entfernten IDs**; fehlgeschlagene und nicht begonnene IDs (Abbruch, `not_started`) bleiben in `cleanupIssues` und im Dialog, der Fehler bleibt sichtbar, der Dialog schließt nur, wenn nichts übrig bleibt. **Nicht geprüft:** ob `delete_files` im Web für ein Modell mit fehlender Datei DB-only in den Papierkorb legt (laut `operations.rs:495-505` ja, aber `delete_files` wechselt bei wieder aufgetauchter Datei auf einen Dateitransfer; für den Bereinigungsdialog gilt das als gewollt, da der Nutzer dort einzelne Modelle ausdrücklich löscht).

### 6.3 Zustandsmaschine des Abgleich-Dialogs

```
closed ─open→ scanning ─ok→ select ─Weiter→ previewing ─ok→ preview ─Übernehmen→ starting ─ok→ running ─fertig→ done
   ▲              │err        ▲  ▲               │err          │Zurück                 │err           │Abbrechen→ cancelling→ cancelled
   │              ▼           │  └───────────────┘◄────────────┘                        ▼
   └────── Schließen ◄── error(kind) ────────────────────────────── Erneut prüfen → scanning
```

- `scanning`: `diskSyncReport()` (läuft bis 30 s; Anzeige „Prüfe …“, `aria-busy`).
- `previewing`: `disk_sync_preview` mit der Auswahl; Server-`skipped` erscheint in der Vorschau.
- `starting`/`running`: `disk_sync_apply_start` mit `confirmRemoval`, dann Blockschleife. „Abbrechen“ setzt `stop`; nach dem laufenden Block → `cancelled` mit „Abgebrochen nach n von m“.
- Fehlerzustände: `snapshot_not_found` → „Vorschau abgelaufen“ mit Knopf „Erneut prüfen“; `limit_exceeded`/`rate_limited` → „Tageslimit … Es wurde nichts geändert“ (bei `Retry-After` mit Wartezeit); `maintenance` mitten im Lauf → „Abgleich abgebrochen, weil eine Sicherung eingespielt wird. Bereits Übernommenes bleibt.“ (Zähler anzeigen); `maintenance` im Start → „Es wurde nichts geändert.“; `backup_failed` → „Sicherung fehlgeschlagen, nichts geändert“; `restart_required` → Dateiverwaltung gesperrt (`blockFileOperations()`, wie `fileOps.ts:~60`).
- Nach `done`/`cancelled` mit Änderungen: Listen neu laden (`refreshAfterFileOps`, `reloadAfterDiskSync` `App.tsx:269-273`, `onDiskSyncApplied` wird auch für das Web verdrahtet) und „Erneut prüfen“ anbieten.
- `useModalDialog({open, onClose, busy})` wie heute (`CatalogDiskReport.tsx:23`): Esc/Schließen nur, wenn nicht `running`.

### 6.4 `src/lib/api/diskSync.ts` (Web-Teil)

Neue Typen/Funktionen: `diskSyncPreview(sel)`, `diskSyncApplyStart(token, confirmRemoval)`, `diskSyncApplyBlock(token, seq)` (Typen aus 2.3-2.5), `DiskSkipReason`; bestehende Desktop-Funktionen und `runStartSync`/`get/set…` bleiben (`:68-76`). `src/lib/api/cleanup.ts` bleibt, `CatalogIssues` bekommt `truncated?: boolean`.

### 6.5 i18n (de/en/es/fr)

Alle neuen Schlüssel in `src/i18n/types.ts` (Interface `Translations`), `de.ts`, `en.ts`, `es.ts`, `fr.ts`. Vorhandene Schlüssel werden weiterverwendet: `diskSyncNewHeading`, `diskSyncMissingHeading`, `diskReportCompanionHeading`, `diskSyncSelectAll/None/AllIn/NoneIn`, `diskSyncApply`, `diskSyncClose`, `diskSyncRetry`, `diskReport*` (Zähler, Abschneide-Hinweise), `dateRename*`, `startSync*`, `cleanup*`, `catalogCleanup*`. Neu (de Vorschlag; en/es/fr sinngemäß, Mehrzahlformen `{one, other}` wie `diskReportNew`):

| Schlüssel | de | en | es | fr |
|---|---|---|---|---|
| `diskApplyReadOnlyHint` | Nur Anzeige. Zum Übernehmen muss der Server mit KATALOG_FILE_OPS=1 gestartet werden. | Display only. To apply changes, the server must be started with KATALOG_FILE_OPS=1. | Solo vista. Para aplicar cambios, el servidor debe iniciarse con KATALOG_FILE_OPS=1. | Affichage seul. Pour appliquer des changements, le serveur doit être lancé avec KATALOG_FILE_OPS=1. |
| `diskApplyNewAction` | Wird in den Katalog aufgenommen | Will be added to the catalog | Se añadirá al catálogo | Sera ajouté au catalogue |
| `diskApplyMissingAction` | Eintrag wird aus dem Katalog entfernt (Papierkorb) *[je nach F1]* | Entry is removed from the catalog (trash) | La entrada se quita del catálogo (papelera) | L’entrée est retirée du catalogue (corbeille) |
| `diskApplyMissingHint` | Wurde die Datei nur verschoben oder umbenannt, steht sie hier und unter „Neu“. Tags, Favorit und Verbrauch werden nicht übernommen. Nur entfernen, wenn die Datei wirklich weg ist. | If the file was only moved or renamed, it appears here and under “New”. Tags, favorite and usage are not carried over. Only remove it if the file is really gone. | Si el archivo solo se movió o renombró, aparece aquí y en «Nuevo». Etiquetas, favorito y consumo no se trasladan. Elimínelo solo si el archivo ya no existe. | Si le fichier a seulement été déplacé ou renommé, il figure ici et sous « Nouveau ». Étiquettes, favori et consommation ne sont pas repris. Ne le retirez que s’il a vraiment disparu. |
| `diskApplyCompanionAction` | Eintrag wird entfernt | Entry is removed | Se quita la entrada | L’entrée est retirée |
| `diskApplyNext` | {one: "Weiter ({count} Änderung)", other: "Weiter ({count} Änderungen)"} | {one: "Next ({count} change)", other: "Next ({count} changes)"} | {one: "Siguiente ({count} cambio)", other: "Siguiente ({count} cambios)"} | {one: "Suivant ({count} changement)", other: "Suivant ({count} changements)"} |
| `diskApplyBack` | Zurück | Back | Atrás | Retour |
| `diskApplyPreviewTitle` | Vorschau und Bestätigung | Preview and confirmation | Vista previa y confirmación | Aperçu et confirmation |
| `diskApplySummaryImport` | {one: "{count} Datei wird aufgenommen", other: "{count} Dateien werden aufgenommen"} | {one: "{count} file will be added", other: "{count} files will be added"} | {one: "Se añadirá {count} archivo", other: "Se añadirán {count} archivos"} | {one: "{count} fichier sera ajouté", other: "{count} fichiers seront ajoutés"} |
| `diskApplySummaryRemove` | {one: "{count} Modell wird aus dem Katalog entfernt", other: "{count} Modelle werden aus dem Katalog entfernt"} | {one: "{count} model will be removed from the catalog", other: "{count} models will be removed from the catalog"} | {one: "Se quitará {count} modelo del catálogo", other: "Se quitarán {count} modelos del catálogo"} | {one: "{count} modèle sera retiré du catalogue", other: "{count} modèles seront retirés du catalogue"} |
| `diskApplySummaryCompanions` | {one: "{count} Begleiteintrag wird entfernt", other: "{count} Begleiteinträge werden entfernt"} | {one: "{count} companion entry will be removed", other: "{count} companion entries will be removed"} | {one: "Se quitará {count} entrada complementaria", other: "Se quitarán {count} entradas complementarias"} | {one: "{count} entrée de fichier associé sera retirée", other: "{count} entrées de fichiers associés seront retirées"} |
| `diskApplyRecheckNote` | Jede Zeile wird beim Übernehmen noch einmal geprüft, geänderte werden übersprungen. Die Vorschau gilt 60 Minuten. | Each row is checked again when applying; changed rows are skipped. The preview is valid for 60 minutes. | Cada fila se vuelve a comprobar al aplicar; las modificadas se omiten. La vista previa es válida 60 minutos. | Chaque ligne est revérifiée à l’application ; les lignes modifiées sont ignorées. L’aperçu est valable 60 minutes. |
| `diskApplyBackupNote` | Vor dem Entfernen legt der Server eine Sicherungskopie des Katalogs an. | Before removing, the server creates a backup copy of the catalog. | Antes de quitar, el servidor crea una copia de seguridad del catálogo. | Avant le retrait, le serveur crée une copie de sauvegarde du catalogue. |
| `diskApplyConfirmLoss` | Verstanden: Notizen, Tags und Verbrauch gehen verloren *[F1: Papierkorb ⇒ „… gehen beim endgültigen Löschen verloren“]* | Understood: notes, tags and usage are lost | Entendido: se pierden notas, etiquetas y consumo | Compris : notes, étiquettes et consommation sont perdues |
| `diskApplyRunning` | {done} von {total} | {done} of {total} | {done} de {total} | {done} sur {total} |
| `diskApplyCancel` | Abbrechen | Cancel | Cancelar | Annuler |
| `diskApplyCancelling` | Wird nach dem laufenden Schritt gestoppt … | Stopping after the current step … | Se detendrá tras el paso en curso … | Arrêt après l’étape en cours … |
| `diskApplyResult` | aufgenommen: {added} · entfernt: {removed} · Begleiteinträge entfernt: {companions} | added: {added} · removed: {removed} · companion entries removed: {companions} | añadidos: {added} · quitados: {removed} · entradas complementarias quitadas: {companions} | ajoutés : {added} · retirés : {removed} · entrées associées retirées : {companions} |
| `diskApplyCancelled` | Abgebrochen nach {done} von {total}. Bereits Übernommenes bleibt. | Cancelled after {done} of {total}. What was already applied stays. | Cancelado tras {done} de {total}. Lo ya aplicado se conserva. | Annulé après {done} sur {total}. Ce qui est déjà appliqué est conservé. |
| `diskApplySkippedHeading` | {one: "{count} Zeile übersprungen", other: "{count} Zeilen übersprungen"} | {one: "{count} row skipped", other: "{count} rows skipped"} | {one: "{count} fila omitida", other: "{count} filas omitidas"} | {one: "{count} ligne ignorée", other: "{count} lignes ignorées"} |
| `diskApplyRecheck` | Erneut prüfen | Check again | Comprobar de nuevo | Revérifier |
| `diskApplyExpired` | Die Vorschau ist abgelaufen (60 Minuten) oder nicht mehr gültig. Es wurde nichts geändert. | The preview has expired (60 minutes) or is no longer valid. Nothing was changed. | La vista previa ha caducado (60 minutos) o ya no es válida. No se cambió nada. | L’aperçu a expiré (60 minutes) ou n’est plus valide. Rien n’a été modifié. |
| `diskApplyLimit` | Das Tageslimit für Dateiaktionen ist erreicht. Es wurde nichts geändert. | The daily limit for file actions has been reached. Nothing was changed. | Se alcanzó el límite diario de acciones de archivo. No se cambió nada. | La limite quotidienne des actions sur fichiers est atteinte. Rien n’a été modifié. |
| `diskApplyMaintenanceStart` | Eine Sicherung wird eingespielt. Es wurde nichts geändert. | A backup is being restored. Nothing was changed. | Se está restaurando una copia. No se cambió nada. | Une sauvegarde est en cours de restauration. Rien n’a été modifié. |
| `diskApplyMaintenanceRun` | Abgleich abgebrochen, weil eine Sicherung eingespielt wird. Bereits Übernommenes bleibt. | Sync stopped because a backup is being restored. What was already applied stays. | Sincronización detenida porque se está restaurando una copia. Lo ya aplicado se conserva. | Rapprochement interrompu car une sauvegarde est restaurée. Ce qui est déjà appliqué est conservé. |
| `diskApplyBackupFailed` | Die Sicherung ist fehlgeschlagen. Es wurde nichts geändert. | The backup failed. Nothing was changed. | La copia de seguridad falló. No se cambió nada. | La sauvegarde a échoué. Rien n’a été modifié. |
| `diskSkip_changed` | Datei hat sich seit der Vorschau geändert | File changed since the preview | El archivo cambió desde la vista previa | Le fichier a changé depuis l’aperçu |
| `diskSkip_exists_again` | Datei ist wieder da | File is back | El archivo ha vuelto | Le fichier est de retour |
| `diskSkip_already_in_catalog` | schon im Katalog (oder im Papierkorb) | already in the catalog (or in the trash) | ya está en el catálogo (o en la papelera) | déjà dans le catalogue (ou la corbeille) |
| `diskSkip_root_unavailable` | Ordner nicht erreichbar | folder not reachable | carpeta no accesible | dossier inaccessible |
| `diskSkip_outside` | außerhalb der freigegebenen Ordner | outside the permitted folders | fuera de las carpetas permitidas | hors des dossiers autorisés |
| `diskSkip_invalid` | keine gültige Modelldatei | not a valid model file | no es un archivo de modelo válido | fichier de modèle invalide |
| `diskSkip_too_large` | Datei zu groß | file too large | archivo demasiado grande | fichier trop volumineux |
| `diskSkip_step_unavailable` | STEP-Dateien werden auf diesem Server nicht gelesen | STEP files are not read on this server | los archivos STEP no se leen en este servidor | les fichiers STEP ne sont pas lus sur ce serveur |
| `diskSkip_has_companions` | hat noch Begleitdateien auf der Platte | still has companion files on disk | aún tiene archivos complementarios en disco | a encore des fichiers associés sur le disque |
| `diskSkip_not_a_file` | keine normale Datei | not a regular file | no es un archivo normal | pas un fichier normal |
| `diskSkip_unclear` | Zustand unklar, nicht angewendet | state unclear, not applied | estado incierto, no aplicado | état incertain, non appliqué |
| `diskSkip_protected` | geschützter Speicherort | protected location | ubicación protegida | emplacement protégé |
| `diskSkip_failed` | fehlgeschlagen | failed | falló | échec |
| `webSyncOnStartLabel` | Katalogordner nach dem Start abgleichen | Compare catalog folders after start | Comparar las carpetas del catálogo tras el inicio | Comparer les dossiers du catalogue après le démarrage |
| `webSyncOnStartDescription` | Der Server prüft nach dem Start einmal im Hintergrund. Gibt es Änderungen, erscheint ein Hinweis. Braucht KATALOG_FILE_OPS=1. | After start the server checks once in the background. If there are changes, a notice appears. Requires KATALOG_FILE_OPS=1. | Tras el inicio el servidor comprueba una vez en segundo plano. Si hay cambios, aparece un aviso. Requiere KATALOG_FILE_OPS=1. | Après le démarrage, le serveur vérifie une fois en arrière-plan. En cas de changements, un avis s’affiche. Nécessite KATALOG_FILE_OPS=1. |
| `webSyncAutoAddLabel` | Neue Dateien automatisch aufnehmen | Add new files automatically | Añadir archivos nuevos automáticamente | Ajouter automatiquement les nouveaux fichiers |
| `webSyncAutoAddDescription` | Neue Dateien werden nach dem Start ohne Rückfrage aufgenommen (höchstens 200 je Start). Es werden nie automatisch Einträge entfernt. Braucht die Einstellung darüber. | New files are added after start without asking (at most 200 per start). Entries are never removed automatically. Requires the setting above. | Los archivos nuevos se añaden tras el inicio sin preguntar (máximo 200 por inicio). Nunca se quitan entradas automáticamente. Requiere el ajuste anterior. | Les nouveaux fichiers sont ajoutés après le démarrage sans demander (200 au maximum par démarrage). Aucune entrée n’est retirée automatiquement. Nécessite le réglage ci-dessus. |

Zusätzlich `fileOpsErrors.ts`: `snapshot_not_found` (= `diskApplyExpired`-Text, allgemein formuliert), `limit_exceeded`, `confirmation_required`, `snapshot_capacity` in vier Sprachen. Prüftest: bestehender Schlüssel-Paritätstest der Sprachen (`src/i18n/*.test.ts`, nicht im Einzelnen geprüft) muss grün bleiben.

### 6.6 Barrierefreiheit

`role="dialog" aria-modal="true" aria-label`, Fokus beim Öffnen in den Dialog (`useModalDialog`), `Esc` schließt (gesperrt während `running`, Fokus bleibt auf „Abbrechen“); Abschnitte als `<section aria-labelledby>`, Kontrollkästchen mit sprechender Beschriftung (Pfad), „Alle auswählen/Keine“ mit Bezug auf den Abschnitt (`diskSyncSelectAllIn`); Zähl-Chips als Text, nicht nur Farbe; Fortschritt `role="progressbar"` mit `aria-valuenow/-max` plus `role="status"` für „n von m“; Fehler `role="alert"`; Ergebnis per `aria-live="polite"`, danach Fokus auf die Ergebnisüberschrift (`tabIndex=-1`, wie im Auftrag P3 Zeile 131); der Haken „Verstanden“ ist das einzige Hindernis, wenn „Übernehmen“ gesperrt ist: Gesperrt-Grund über `aria-describedby`; kein Fokusverlust nach Schrittwechsel (Fokus auf neue Überschrift). Tastatur: alle Aktionen per Tab/Leertaste/Enter; keine Tastenkürzel nötig.

---

## 7. Tests (A6)

### 7.1 Rust, Core (`cargo test -p katalog-core`)

`disk_report_tests.rs` (Erweiterung): `excluded`-Ordner; `ordered_roots` stimmt mit der Indizierung des Berichts überein; unveränderter Bericht ohne `excluded` (Golden gegen vorher). `disk_apply_tests.rs` (neu): `validate_relative` (`..`, absolut, leer, NUL, 4096+ Bytes, Tiefe 65, `.versteckt`, `~$x.3mf`, `x.3mf.part`, falsche Endung, Backslash als normales Zeichen), Planen/Wiederprüfen je Art, Stempel-Abweichung, Datei zwischen Vorschau und Anwenden **geändert** (Länge, mtime, ersetzt per Rename = andere Inode), **gelöscht**, **wieder da** (`exists_again`), Pfad inzwischen im Katalog (aktiv und Papierkorb, Groß-/Kleinschreibung, NFC/NFD), Link als letzte Komponente und als Zwischenverzeichnis, Pfad zeigt per Link aus der Wurzel heraus, Datei im ausgeschlossenen Upload-Ordner, Wurzel leer/ersetzt (dev/ino) **für Modell- und Begleitzeilen**, Wurzel nicht erreichbar → nie „fehlt“, **Unterordner nach der Vorschau umbenannt und durch einen Link auf denselben Ordner innerhalb der Wurzel ersetzt (gleiche Inode, Länge, mtime) → `outside`/`changed`**, Zwischenkomponenten-Link in jeder Tiefe, **Pfadeintrag während des Lesens ausgetauscht (andere Datei unter demselben Namen), gelöscht oder durch Link ersetzt → Rollback vor dem Commit, `changed`/`outside`, Katalog byte-gleich**, Ordnerkette nur für bestehende Verzeichnisse, Zurückrollen bei geänderter Datei nach dem Lesen, `read_web_model`-Grenzen (zu groß, kaputtes ZIP), Doppelte in der Auswahl. `fsops`-Tests (4.6): `remove_missing_entry` mit fehlender Quelle → Zeile entfernt/Papierkorbzustand; Quelle **vor** dem Aufruf vorhanden → Fehler „Quelle vorhanden“, Zeile, Journal und Platte unverändert; **Wettlauf Wiederprüfung ↔ Operationsbeginn**: Haken (`tests::at_point`, neuer Punkt direkt vor `db_only`) legt die Datei an → Datei bleibt am Ort (gleiche Inode, nichts im `.katalog-papierkorb`), nur die Datenbank wurde berührt bzw. der Aufruf lehnt ab; kein Pfad erreicht `transfer`; dasselbe für `remove_missing_companion`; Absturzpunkte wie `parity_golden_tests.rs`. `catalog_issues_tests.rs`: `NotFound` vs. `PermissionDenied` (`EACCES`-Verzeichnis), Link auf dem Weg = unklar, Duplikatgruppen unverändert (Verschiebung der `group_duplicates`-Tests aus `backup.rs`). `date_rename/tests.rs` (Erweiterung): `within_catalog` überschreibt die Basisordner-Regel, `verify_item` liefert dieselben Gründe wie `apply` (Parität: bestehende Tests laufen unverändert), `StdDateRenameFs` verhält sich wie vorher. `disk_sync_settings`: Lesen/Schreiben, Standard `false`.

### 7.2 Desktop-Parität (`cargo test` in `src-tauri`, nur gelesene Läufe, ohne Änderung der Tests)

Alle vorhandenen Tests in `commands/backup.rs` (`scan_catalog_issues`), `disk_sync/start_sync_tests.rs`, `disk_sync.rs`-Tests, `commands/date_rename.rs`-Tests bleiben unverändert und grün; neu je ein Paritätstest: `scan_catalog_issues` Desktop-Ergebnis gegen vorher eingefrorenes JSON (gleiche Eingabe), Start-Einstellungen lesen/schreiben dieselben Schlüssel.

### 7.3 Rust-HTTP (`crates/katalog-server`, Fixture wie `fileops_http_tests.rs:11-124`; Erweiterung von `tests/disk_report.rs`)

- Ohne `KATALOG_FILE_OPS`: alle sechs Befehle `404 fileops_disabled`, auch ohne Sitzung/JSON (Erweiterung von `all_fileops_disabled_before_security_or_json`, `:133`); `disk_sync_report` weiter nutzbar; `run_start_sync` → `ran:false`.
- Sitzung/CSRF/Origin wie `fileops_security_gate_and_session_capability` (`:149`); **Token einer fremden Sitzung** → `404 snapshot_not_found` (Muster `snapshot_is_session_bound_and_blocks_are_throttled`, `:349`); **abgelaufen** (Uhr +3600 s per Testuhr `Time`, `:5`) → 404; zweite Vorschau ersetzt die erste; 100-Pläne-Grenze → `503 snapshot_capacity`.
- Idempotenz/Doppelaufruf: `apply_start` zweimal → 2. Mal `409`; Budget nur einmal verbraucht; `apply_block` zweimal mit **gleicher** `seq` → identische Antwort, nichts doppelt (Zähler im Katalog gleich, kein zweiter Journaleintrag); falsche `seq` → `409`.
- Aufnahme: neue Dateien (`.3mf`, `.stl`, `.obj`) werden Katalogzeilen mit Ordnerkette; **Datei zwischen Vorschau und Anwenden geändert** → Zeile `changed`, nichts im Katalog; **gelöscht** → `changed`; **Pfad inzwischen im Katalog** → `already_in_catalog`; **Datei im Upload-Ordner** → in Bericht und Vorschau nie vorhanden; **Symlink** (Datei, Zwischenordner, Wurzelausbruch) → `outside`, nie gelesen; Pfadflucht `../x.3mf`, absolut, `a//b` → `422 invalid_arguments` bzw. Zeile übersprungen (gemäß 2.3); **Unbekannte außerhalb der Wurzeln** (Wurzelindex ≥ Anzahl; `missingModels`-ID eines Modells außerhalb der Wurzeln) → übersprungen `outside`, nichts geändert; **Groß-/Kleinschreibung**: `Foo.3MF` neben Katalogzeile `foo.3mf` → `already_in_catalog`.
- **Zulassung des Blocks (W2-01):** `disk_sync_apply_block(token, 0)` direkt nach `disk_sync_preview` → `409 not_started`, Katalog, Budget, Sicherung und Plan (`seq`) unverändert; Block nach fehlgeschlagenem Start (`confirmRemoval:false` → 422; `backup_failed`; Tageslimit) → `409 not_started`, nichts angefasst; Block eines abgelaufenen/fremden Plans → 404; nach erfolgreichem Start läuft derselbe Block; Aufnahmen-Plan ohne Entfernen kann ohne `apply_start` ebenfalls nicht angewendet werden.
- Entfernen (Papierkorb, F1): Modell, dessen Datei fehlt, landet im Papierkorb (`deleted_at` gesetzt, `trash_path` NULL, Journal leer danach); Datei **wieder da** vor dem Block → `exists_again`, **auch wenn sie erst nach der Wiederprüfung, vor dem Operationsbeginn erscheint (Testhaken): Datei bleibt unberührt, nichts im Papierkorbordner**; Wurzel inzwischen leer/ausgehängt (Verzeichnis umbenannt) → `root_unavailable`, keine Änderung; **Modell mit Begleitdateien auf der Platte** → `has_companions`; Begleiteintrag ohne Datei → Papierkorbzustand; Begleiteintrag mit Ordner am Pfad → `not_a_file`; `confirmRemoval:false` → `422 confirmation_required`, kein Budget, keine Sicherung.
- **Tageslimit (W2-02):** Limit 10 und 100 Entfernungen → `apply_start` `422 limit_exceeded`, nichts gebucht (danach ist ein Plan mit 10 Entfernungen sofort startbar), Blöcke dafür `409 not_started`; Limit 10 und genau 10 → erlaubt, ein zweiter Plan mit 1 → `429` mit `Retry-After`; Einheitentest `reserve_disk_sync` in `fileops_safety`; `reserve_folder` wird von W2 nicht aufgerufen (Test/Review).
- **Wurzeln der Begleitzeilen (W2-06):** Wurzel durch leeres Verzeichnis ersetzt bzw. leer/unerreichbar → Vorschau lässt die Begleitzeile aus (`root_unavailable`), Anwenden → `root_unavailable`; dasselbe für fehlende Modelle; Test je Art (leer, unerreichbar, ersetzt).
- **Datumspräfix-Bodygrenze (W2-07):** `apply_date_rename` mit 200 Namen zu je 255 Bytes (auch mit `\u00XX`-Escapes, rund 316 000 Bytes) → 200 statt 413; Body über 384 KiB → 413; `preview_date_rename` mit 200 IDs unter 16 KiB; über den echten Axum-Pfad (`protect` + `json_body`).
- Budget/Tageslimit: Tageslimit klein (`KATALOG_FILE_OPS_DAILY_LIMIT`) → `apply_start` `422 limit_exceeded` bzw. `429` mit `Retry-After` (Muster `empty_trash_blocks_still_hit_the_daily_limit_with_real_retry_after`, `:677`); Ablehnung kostet nichts und legt keine Sicherung an; Aufnahmen allein verbrauchen kein Budget.
- Sicherungskopie: vor dem ersten Entfernen existiert `web-fileops-event-*.db` (Muster `folder_commands_and_backups_before_removal`, `:288`); Sicherung scheitert (Verzeichnis schreibgeschützt) → `503 backup_failed`, Katalog unverändert (Muster `backup_failure_rejects_before_filesystem_change_and_noop_does_not_backup`, `:393`).
- **Absturz im Block:** Panik/Abbruch mitten im Entfernen (Testhaken `before_commit` aus `fsops/tests.rs`) → Journal-Eintrag bleibt, Neustart-Recovery (`fileops_startup::prepare`) schließt ab; Plan nach Neustart `404`; kein halber Zustand (Muster `db_only_removal_revoked_after_first_delete_rolls_back_all_rows`, `:508`, und `detached_worker_keeps_import_gate_until_completion`, `:458`). Gesperrtes Journal → `503 restart_required` für alle W2-Befehle (Muster `blocked_runtime_disables_capability_without_retry_and_keeps_reads`, `:437`).
- Import läuft (Upload-Import aktiv) → `503` mit `Retry-After`, Plan bleibt.
- Datumspräfix: Vorschau/Anwenden mit Begleitdatei (Mitnahme), Zielname vergeben, Datei fehlt, Papierkorb, außerhalb der Wurzeln, zu lang, hat schon Datum, 201 Einträge → `422 limit_exceeded`, zweiter Aufruf = alles `hasPrefix`, Frist abgelaufen → Rest `not_started`.
- Lesen: `scan_catalog_issues` (fehlende Datei, `chmod 000`-Verzeichnis zählt nicht als fehlend, Duplikatgruppe, Zeile außerhalb der Wurzeln), `get/set_sync_*` (Request-ID-Replay, ohne `KATALOG_WRITE` nicht schreibbar), `run_start_sync` nach dem Lauf.
- **Start-Abgleich:** Katalog mit neuen Dateien, `sync_on_start=1`: erst Recovery, dann Lauf; ohne `auto_add` nur Bericht (Katalog byte-gleich); mit `auto_add` werden höchstens 200 aufgenommen, kein Entfernen, auch nicht bei fehlenden Dateien; ohne `KATALOG_FILE_OPS` läuft nichts; nicht erreichbare Wurzel → nichts angewendet; Lauf bei Journal-Recovery-Rest `manual` startet nicht.
- **Barriere:** siehe 3 (Pflichttest Start-Abgleich gegen gleichzeitiges `enter_draining`; Block in `Draining` → 503; `restore`-Austausch wartet).
- Sicherheitsregressionen: alle sechs Befehle in `fileops_commands_are_all_audited_writes` (`:626`); Audit enthält nie Pfade (`progress.set_file_ids`); Fehlerantworten ohne Pfade.

### 7.4 Frontend (Vitest)

`CatalogDiskReport.web.test.tsx` (bestehend, erweitern): Nur-Anzeige ohne `fileOps` (mit Hinweistext, keine Kontrollkästchen), Vorauswahl (Neu + Begleit an, Fehlt aus), „Alle auswählen/Keine“ je Abschnitt, „Weiter“ gesperrt bei 0, Vorschau-Schritt (Haken nur mit Entfernen, „Übernehmen“ gesperrt ohne Haken), Lauf mit Fortschritt (Fake-`fetch`), Abbrechen zwischen Blöcken → „Abgebrochen nach n von m“, Ergebnis mit übersprungenen Zeilen und Gründen, „Erneut prüfen“, Fehlerzustände (`snapshot_not_found`, `limit_exceeded`, `429` mit `Retry-After`, `maintenance` im Start und mittendrin, `backup_failed`, `restart_required`), Esc während `running` gesperrt, Fokus/`role`/`aria-live`. `lib/api/diskSync.test.ts`: Befehlsnamen und Argumente. `lib/fileOps.web.test.ts`: `runDiskSync` mit `seq`-Wiederholung nach Netzfehler, Abbruch, Stopp bei `maintenance`. `DateRenameDialog.test.tsx`: Web-Grenze 200. **`useCatalogCleanup.test.ts` / Dialog (W2-08):** Web-Zweig ruft `fileOpsBatch`; 450 gewählte Einträge → drei Anfragen (200/200/50); gemischte Ergebnisse (HTTP 200, einzelne `ok:false`) → `onDeleted` nur mit den erfolgreichen IDs, Fehlgeschlagene bleiben in der Liste, Dialog bleibt offen mit Meldung; Fehler im zweiten Block → dritter Block `not_started`, erste Ergebnisse bleiben erhalten; Desktop-Zweig unverändert. `useStartSync.test.tsx`: Web, `busy` → Wiederholung, Hinweis nur bei Treffern. `DiskSyncSettings`-Web (neu/erweitert): Abhängigkeit „automatisch“ von „Start“, kein Bekannte-Kopien-Bereich. `commandParity.test.ts` grün (nach P0). Vorhandene i18n-Paritätstests. Hinweis: Lastflakes bei `CatalogDiskSync` bekannt, einzeln wiederholen (Auftrag P3 Zeile 31).

### 7.5 Docker-Web-Test (Wegwerf-Modellordner)

Erweiterung von `docker-web/scripts/ci-smoke.sh` oder neues `w2-smoke.sh` (ohne neue Abhängigkeit im Repo, R5; Browserprüfung mit dem `pw`-Wrapper außerhalb des Repos): Container mit `KATALOG_WRITE=1`, `KATALOG_FILE_OPS=1`, `KATALOG_ALLOWED_HOSTS`, Wegwerf-Volume mit 3 Beispielmodellen, einem fehlenden Modell, einer Begleitzeile ohne Datei. Ablauf: Bericht → alle Vorauswahlen übernehmen → Katalog-Zähler (SQLite im Volume) → zweiter Lauf zeigt „alles gut“; „fehlt“-Zeile anwählen → Haken verlangt → Papierkorbzustand; Datumspräfix für 2 Dateien; Server mit `sync_on_start=1,auto_add=1` neu starten → Hinweisleiste und Katalogstand. Vor dem Test: Wegwerfkatalog und aktive Sicherungskopie (`ensure_backups`, Auftrag P3 Zeile 120). Builds/Tests nur auf ObiWan, immer nur einer zugleich; hier nicht ausgeführt.

---

## 8. Zerlegung in Arbeitspakete

### 8.1 Reihenfolge und Abhängigkeiten

```
P0 (Listen/Verträge) ─┬─> P1 Core A1+Settings ──────────────┐
                      ├─> P2 Core disk_apply + disk_report ─┼─> P4 Server fileops (Plan, Blöcke) ─> P7 Tests/Docker/Doku
                      ├─> P2b fsops DB-only (4.6) ──────────┤
                      ├─> P3 Core date_rename ──────────────┘          │
                      │                                                 └─> P5b Start-Abgleich
                      └─> P5a Server Lesen+Einstellungen (nach P1) ───────────────┐
                           P6 Frontend (gegen die Verträge aus 2, parallel ab P0) ┘
```

### 8.2 Pakete

| Paket | Inhalt | Dateien (nur diese) | Abhängig von | Parallel zu | Aufwand | Modell | Extern prüfen? |
|---|---|---|---|---|---|---|---|
| **P0 Verträge** | alle Befehlsnamen in die sieben Listen, Klassen, `body_limit` (`disk_sync_preview` 256 KiB, `apply_date_rename` 384 KiB), i18n-Gerüst (Schlüssel + de-Texte), leere Handler geben `404 command_unavailable` | `write.rs:16/33/139`, `invoke.rs:155`, `src/lib/readOnly.ts`, `src/lib/transport.ts:10`, `docs/web/command-classes.json`, `src/i18n/types.ts` | – | – | 2,5 h | Sonnet | nein |
| **P1 Core A1 + Einstellungen** | 4.1, 4.4, Desktop auf Kern umstellen | `crates/katalog-core/src/{catalog_issues.rs,disk_sync_settings.rs,lib.rs}`, `src-tauri/src/commands/{backup.rs,disk_sync/start_sync.rs}` | P0 | P2, P3 | 6 h | Sonnet | nein (Parität) |
| **P2 Core Anwenden** | 4.2 (`disk_report`-Erweiterung, `disk_apply.rs` mit `Anchored`: verankerte Auflösung statt `open_below_roots`, Wurzelprüfung für Modelle und Begleitzeilen, Pfadeintrag-Wiederprüfung vor dem Commit; `rescan.rs` nur `FileStamp`) + Kerntests | `crates/katalog-core/src/{disk_report.rs,disk_report_tests.rs,disk_apply.rs,disk_apply_tests.rs,rescan.rs,lib.rs}` | P0 | P1, P2b, P3 | 16 h | Sonnet (bauen), Claude prüft | **ja** (Datenverlust, Pfadsicherheit, Linkwechsel) |
| **P2b fsops DB-only** | 4.6: `remove_missing_entry`, `remove_missing_companion`, `Error::SourcePresent`, Haken für den Wettlauftest, `fsops`-Tests | `crates/katalog-core/src/fsops/{operations.rs,companion_edit.rs,tests.rs}`, `fs_portable/mod.rs`, `error_code.rs`, `lib.rs` | P0 | P1, P2, P3 | 4 h | Sonnet | **ja** (Dateitransfer-Ausschluss, Journal) |
| **P3 Core Datumspräfix** | 4.3 | `crates/katalog-core/src/date_rename.rs`, `date_rename/tests.rs` | P0 | P1, P2 | 3 h | Sonnet | nein |
| **P4 Server Plan und Blöcke** | 5.1, 5.2, `plan_for_block`-Zulassung (2.5), `reserve_disk_sync` (`fileops_safety.rs`), Datumspräfix-Befehle, HTTP-Tests der Befehle | `crates/katalog-server/src/{fileops.rs,fileops_disksync.rs,fileops_safety.rs,fileops_http_tests.rs,lib.rs}` | P2, P2b, P3 | P6 | 17 h | Sonnet + Claude-Prüfung (oder Codex bis 30.11.) | **ja** (kritisch: Löschen, Budget, Barriere, Journal) |
| **P5a Server Lesen/Einstellungen** | `scan_catalog_issues`, `get/set_sync_*`, `run_start_sync`-Speicher, Report-Wurzeln/Ausschluss | `invoke.rs`, `write.rs` (`execute`), `web.rs` (Report-Aufruf), `tests/disk_report.rs` | P1, P2 (nur `ordered_roots`/`excluded`) | P4, P6 | 6 h | Sonnet | nein |
| **P5b Start-Abgleich** | 5.4 | `crates/katalog-server/src/disk_start_sync.rs`, `web.rs:454` (ein Aufruf), `lib.rs` | P4, P5a | P6 | 6 h | Sonnet | **ja** (automatisches Schreiben beim Start) |
| **P6 Frontend** | 6.1-6.6 (inkl. Bereinigungsdialog-Anbindung an `fileOpsBatch`, W2-08) | `src/components/{CatalogDiskReport.tsx,DiskReport*.tsx,DiskSyncSettings.tsx,SettingsPanel.tsx,ToolsSection.tsx,BulkActionToolbar.tsx}`, `src/App.tsx`, `src/hooks/{useStartSync.ts,useCatalogCleanup.ts}`, `src/lib/{fileOps.ts,fileOpsErrors.ts,api/diskSync.ts,api/files.ts}`, `src/i18n/{de,en,es,fr}.ts` + Tests | P0 (Verträge) | P1–P5 | 15 h | Sonnet | nein (aber Dialog im Browser prüfen) |
| **P7 Tests/Docker/Doku** | 7.3 (Rest), 7.5, Barrieretest 3, `docker-web/README.md`, WEB-PLAN.md (Zeilen 97/101 berichtigen), CHANGELOG `[Unreleased]`, `releaseNotes.ts` (4 Sprachen), Wiki, Forgejo #39 | `crates/katalog-server/src/*_tests.rs`, `docker-web/scripts/*`, Doku-Dateien | P4, P5b, P6 | – | 9 h | Sonnet | nein |
| | **Summe** | | | | **84,5 h** | | |

Hinweise zur Schätzung: Der Auftrag (A1-A6) setzt 44 h an. Die Spezifikation liegt höher, weil (a) der Anwenden-Pfad zeilenweise Wiederprüfung mit Stempeln und Wurzelidentität braucht (P2: 12 statt 14 h für A2, dafür P4 14 h für Server), (b) der Web-Adapter für `date_rename` und das Zeilenprotokoll Neuland sind, (c) der Start-Abgleich eigenen Barrierentest hat, (d) die Doku-/Release-Schritte mitgezählt sind. Nach der Codex-Prüfung vom 24.11. (Abschnitt 10) gestiegen von 69 auf 84,5 h: P0 +0,5, P2 +4, P2b neu 4, P4 +3, P6 +3, P7 +1. Nicht enthalten: Codex-Prüfrunden und Nacharbeit (+ etwa 15 %), Mockup-Runden, RC-Phase.

**Parallelitätsregel (Dateikonflikte):** P1, P2, P3 berühren unterschiedliche Core-Dateien (`lib.rs` nur durch je eine `mod`-Zeile: zuerst P0 trägt die drei `mod`-Zeilen mit leeren Dateien ein, dann kollidiert nichts). P4 und P5a berühren beide `fileops.rs` nicht gemeinsam; **P5a und P4 sind aber beide in `write.rs`/`invoke.rs`/`web.rs`**: P0 erledigt alle Listenänderungen, P5a ändert danach nur `execute`-Arme (`write.rs:757`) und den Report-Zweig (`invoke.rs:396-440`), P4 ändert `write.rs` nicht mehr. Alles, was `readOnly.ts`, `command-classes.json`, `invoke.rs:155` oder `write.rs:16/33` ändert, steht ausschließlich in P0 (Übersicht, Parallelitätsregel). Testläufe stehen in einer Schlange auf ObiWan.

**Sicherheitskritisch (Codex-Prüfung Pflicht, dann Claude gegen echte Dateien und Tests):** P2 (Zeilenprüfung, verankertes Öffnen, Pfadeintrag-Wiederprüfung, Ordnerkette), P2b (kein Dateitransfer im Entfernen-Pfad), P4 (Plan/Token/Budget/Bestätigung/Barriere/Journal, Entfernen), P5b (automatisches Hinzufügen beim Start, Wurzel-Unklarheit). Prüfgegenstand je Paket: Diff plus diese Spezifikation (Abschnitte 2.6, 3).

---

## 9. Risiken und offene Fragen

### 9.1 Fragen an den User (echte Entscheidungen)

**Entscheidungen des Users (24.11.2026):** F1 = „Eintrag entfernen“ geht in den **Papierkorb** (`MissingRemoval::Trash`; `Delete` bleibt im Code vorgesehen, die Oberfläche bietet es nicht an). F2 = kein Hinweis bei gleichem Inhalt. F3 = kein Passwort beim Papierkorb-Entfernen. F4 = Text „Abbrechen stoppt nach dem laufenden Block“ (Schlüssel `diskApplyCancelling`, `diskApplyCancelHint`). Die folgenden Punkte sind damit entschieden; der Text darunter bleibt als Begründung stehen.

- **F1 – Was heißt „Eintrag aus dem Katalog entfernen“?** Der Code hat zwei Wege:
  - **A (Empfehlung, Spezifikation geht davon aus):** In den Papierkorb legen, wie der Desktop es beim Abgleich tut (`disk_sync.rs:1595-1601`, `trash.rs:3-6`; Web: `FileOps::delete_file`, journalisiert, `operations.rs:482-505`). Notizen, Tags und Verbrauch bleiben, bis jemand den Papierkorb leert; das ist rückholbar und hält die Risiken des falschen „fehlt“ (Netzlaufwerk) klein. Der Mockup-Satz „Notizen, Tags und Verbrauch gehen verloren“ wird dann zu „… gehen beim endgültigen Löschen verloren“ (wie `diskSyncRemoveHint`, `de.ts:269`).
  - **B:** Zeile hart löschen (`write::web::remove_files_from_catalog`, `write/web.rs:570`; Budget `Permanent`, kein Rückweg außer der Sicherungskopie), genau wie der Mockup-Text sagt. Mehraufwand: keiner; Nachteil: Datenverlust bei falschem „fehlt“.
  Der freigegebene Mockup nennt „gehen verloren“; ich konnte den Mockup selbst nicht einsehen, nur die Beschreibung im Auftrag. Bitte entscheiden, bevor P4 gebaut wird. Nach der Codex-Prüfung (W2-03) ist die Entscheidung **nur noch der Parameter `MissingRemoval::{Trash, Delete}`** der DB-only-Funktion (4.6) plus Budgetart, Passwort (F3) und Textschlüssel; Ablauf, Fehlercodes und Tests in 2.6/4.6/7 gelten für beide Antworten. F1 ist weiterhin **offen und nicht entschieden**.
- **F2 – Doppelte Inhalte:** Neue Dateien mit gleichem Inhalt (Hash) wie ein vorhandenes Modell werden im Web wie im Desktop-Import aufgenommen (der Desktop trennt sie als eigenen, nicht vorausgewählten Abschnitt, `diskSyncDuplicateHeading`); der Web-Mockup kennt diesen Abschnitt nicht (D4). Soll W2 im Ergebnis einen Hinweis „n Dateien haben denselben Inhalt wie ein vorhandenes Modell“ zeigen (zusätzlich ein Hash-Vergleich in der Aufnahme, ohne Umbenennungserkennung), oder reicht „aufgenommen“? Empfehlung: ohne Hinweis.
- **F3 – Passwort beim Entfernen:** Der Server verlangt bisher nur für endgültiges Löschen und Papierkorb leeren das Passwort (`fileops.rs:362`). Bei F1-A verlangt W2 keines (Haken „Verstanden“ + Sicherungskopie genügen). Bei F1-B empfehle ich das Passwort wie bei `delete_file_permanently`. Einverstanden?
- **F4 – Wortlaut „stoppt nach der aktuellen Datei“:** Technisch stoppt der Abbruch nach dem laufenden Block (höchstens 10 Zeilen oder 12 s). Soll der Text zu „stoppt nach dem laufenden Schritt“ werden, oder soll die Blockgröße auf 1 Zeile (mehr Anfragen, mehr Audit-Zeilen) gesenkt werden? Empfehlung: Text anpassen.

### 9.2 Risiken

- **Falsches „fehlt“:** Gegenmaßnahmen sind eingebaut: nur `NotFound`; Wurzel leer/anders/nicht erreichbar → nichts; Wiederprüfung unmittelbar vor der Zeile; Warnung `mostlyMissing` (bleibt Anzeige, sperrt nicht – Entscheidung offen, falls der User Sperre will); Sicherungskopie; bei F1-A Papierkorb. Nicht abgedeckt: ein Dateisystem, das ein vorhandenes Verzeichnis kurz als `NotFound` liefert (flackernde Netzfreigabe): wäre nur mit Papierkorb (F1-A) schadlos.
- **Aufnahme großer Dateien im Serverprozess:** Parsen läuft im `spawn_blocking`-Thread unter Writer-Guard, Import-Tor und `rt.state` (andere Dateiaktionen warten mit 503). Grenzen: `WebImportLimits` (1 GiB/512 MiB/Eckpunkte), 12-s-Blockfrist, 10 Zeilen je Block. Eine einzelne Datei kann die Frist überschreiten (die Zeile wird zu Ende geführt) und blockiert dann bis zu ihrer Parsezeit das Einspielen einer Sicherung (`RestoreDrainTimeout` nach 30 s, `restore.rs:8`). Alternative (nicht in W2): Parsen im STEP-Worker-Stil in einem Unterprozess.
- **Plan nur im Speicher:** Neustart zwischen Vorschau und Übernehmen → `snapshot_not_found`; gewollt (kein Zustand auf der Platte, keine neue Journalart).
- **Mehrere Browser-Sitzungen:** ein Plan je Sitzung, `rt.state.try_lock` serialisiert; zwei gleichzeitige Blöcke → einer bekommt `503 Retry-After`.
- **Gleichzeitiger Desktop:** Web und Desktop teilen keine Sperre (WEB-PLAN.md K8, `:374`); die Zeilenprüfung fängt die meisten Fälle, nicht alle.
- **`catalog_base_dir` im Docker:** nur relevant für Desktop-Pfade; der Web-Adapter ersetzt die Regel durch „unter einer Wurzel“. **Nicht geprüft:** wie viele reale Docker-Kataloge Pfade mit anderem Präfix als die Wurzel führen (Modelle „außerhalb der Wurzeln“ werden gezählt, nie angewendet).
- **STEP-Aufnahme:** Reihenfolge und Verfügbarkeit des Step-Workers sind P3-D (W6) abhängig; ohne Worker `step_unavailable`. `read_web_model` und STEP: **nicht geprüft**.
- **`delete_files` für fehlende Dateien (Bereinigungsdialog):** laut `operations.rs:495-505` DB-only, wechselt aber bei wieder aufgetauchter Datei auf einen Dateitransfer (gewollt im Dialog, nicht im Abgleich, 4.6). Verhalten bei Begleitdateien (`HasCompanions`) muss der Dialog je Eintrag melden; die Auswertung (`checkResults`) wird erst durch die Anbindung an `fileOpsBatch` (6.2) wirksam, sie läuft heute auf diesem Pfad nicht.
- **Verankerte Auflösung (W2-04/05):** `FsContext::new` liest die Einhängungen (`mount_locations`) und öffnet die Wurzeln; je Block einmal bauen, nicht je Zeile (Kosten nicht gemessen). Plattformen ohne `openat`/`O_NOFOLLOW` (Windows-Docker gibt es nicht; Desktop nutzt den Kern nicht) sind nicht betroffen. Bleibende Restfenster (zwischen letzter Prüfung und `COMMIT`; DB-only-Entfernen bei Wiedererscheinen) sind dokumentiert und ändern nie Dateien. **Nicht geprüft:** ob `FsContext` für Wurzeln mit Link-Komponenten im konfigurierten Pfad (Docker-Volumes) die kanonisierte Wurzel wie erwartet liefert (`context.rs:~150` kanonisiert; Umgang mit Wurzel-Symlinks im Test abdecken).
- **Auftrag-Zeilen veraltet:** Arbeitspakete verweisen auf Zeilen von 36b5a0cd; vor dem Bau gegen `git diff` des Zielzweigs (`dev/0.19.0`, Forgejo-Stand ≥ 36b5a0cd) prüfen.
- **Nicht geprüft insgesamt:** reale Ausführung aller Tests (Server ausgelastet, kein Build), Verhalten von `read_web_model` für STEP, `db::list_files`-Semantik, Größe von `ModelFileDto` bei vielen Treffern, die genaue Stelle der Web-Einstellungen im `SettingsPanel`, ob `Translations`-Interface Pflichtschlüssel in allen vier Dateien verlangt (vermutlich ja), sowie der freigegebene Mockup selbst (nur aus der Auftragsbeschreibung bekannt).

---

## 10. Änderungen nach der Codex-Prüfung (24.11.2026)

Quelle: `docs/plans/w2-spec-codex-befunde.md` (Verdikt REVISE). Jeder Befund wurde gegen den Code (Basis `36b5a0cd`) geprüft; alle acht bestätigt. Die Spezifikation ist geändert, es wurde nichts gebaut oder getestet. Gesamtschätzung neu: **84,5 h** (vorher 69 h).

| Befund | Ergebnis mit Beleg | Geänderte Abschnitte | Neue Tests |
|---|---|---|---|
| W2-01 (hoch) Block ohne Start | **bestätigt**: Ablauf prüfte nur `seq`; `fileops::invoke` (`fileops.rs:332-407`) kennt keine Planzustände | 2.3 (`started`), 2.4 (Schritt 6), 2.5 (neue „Zulassung“, `plan_for_block`), 2.9, 5.1 | 7.3: Block direkt nach `preview`; Block nach fehlgeschlagenem Start (Bestätigung/Sicherung/Limit); abgelaufen/fremd |
| W2-02 (hoch) Tageslimit | **bestätigt**: `Charge::Folder` → `impossible=false`, bucht `min(n, daily_limit)` (`fileops_safety.rs:34-45`, `reserve_folder :107`) | 2.4 (Schritt 3, neue `reserve_disk_sync`), 2.9, 5.5, 8.2 (P4) | 7.3: Limit 10 / 100 Entfernungen, genau 10, Folgeplan; Einheitentest `reserve_disk_sync` |
| W2-03 (hoch) Dateitransfer | **bestätigt**: `delete_file` wechselt bei vorhandener Quelle auf `Operation::Trash` (`operations.rs:482-518`), `check_source :255-268` prüft nur Ledger-Identität; `trash_companion` plant Transfer (`companion_edit.rs:189-224`) | 2.6 (removeModel/removeCompanion), 4.5, neu 4.6 (`remove_missing_entry`/`remove_missing_companion`, `Error::SourcePresent`), 5.1, 8.1/8.2 (P2b), 9.1 (F1 weiter offen, nur Parameter `MissingRemoval`) | 7.1 `fsops`-Tests; 7.3: Datei erscheint nach Wiederprüfung vor Operationsbeginn (Testhaken) |
| W2-04 (hoch) Linkfreiheit | **bestätigt**: `open_below_roots` ruft `canonicalize` auf den ganzen Pfad, prüft nur die letzte Komponente (`rescan.rs:183-210`); `open_regular` ohne `O_NOFOLLOW` (`safe_file.rs:18-24`). Vorhandene Bausteine: `upload_fs::Directory::{open,open_dir,read,entry_identity}` (`upload_fs.rs:31/236/69/108`, `openat`+`O_NOFOLLOW`), `FsContext::resolve_relative` (`context.rs:255`) | 2.3, 2.6 („Sichere Auflösung“), 4.2 (`open_below_roots` nicht übernommen, `Anchored`), 8.2 (P2) | 7.1: Unterordner nach der Vorschau durch Link ersetzt, Link in jeder Tiefe, Upload-Ausschluss über Handle-Identität |
| W2-05 (mittel) Pfadeintrag beim Commit | **bestätigt**: Handle-`metadata` sieht keinen Austausch; `apply_rescan` prüft den Pfadeintrag (`rescan.rs:350 ff.`) | 2.6 (Aufnahme, Schritt 5), 9.2 | 7.1: Eintrag während des Lesens ausgetauscht/gelöscht/verlinkt → Rollback |
| W2-06 (mittel) Begleitzeilen-Wurzeln | **bestätigt**: Spezifikation verlangte `root_usable`/Identität nur für Modelle (2.3, 2.6) | 2.3 (Begleitzeile, `PlanRow`), 2.6 (removeCompanion), 3 unverändert | 7.1/7.3: leere, unerreichbare, ersetzte Wurzel für Modelle und Begleitzeilen, Vorschau und Anwenden |
| W2-07 (mittel) Bodygrenze | **bestätigt**, präzisiert: Name bis 255 Bytes (`file_names.rs:24`), nicht 100 Zeichen; `body_limit` 16 KiB (`write.rs:139-146`), angewendet `web.rs:586` und `:1043` | 2.10 (384 KiB-Zweig mit Rechnung), 8.2 (P0) | 7.3: 200 × 255 Bytes (auch mit Escapes) → 200; > 384 KiB → 413; über den echten HTTP-Pfad |
| W2-08 (mittel) Bereinigungsdialog | **bestätigt**: `useCatalogCleanup.ts:32-47` → `filesApi.deleteFiles` → `invoke('delete_files')` (`api/files.ts:49`), kein `fileOpsBatch`/`checkResults`; Server `delete_files` höchstens 200 (`fileops.rs:175`) | 6.2 (Bereinigung, falsche Aussage berichtigt), 9.2, 8.2 (P6) | 7.4: > 200 Einträge (450 → 200/200/50), gemischte Ergebnisse, Fehler im Folgeblock |

Nebenbefund (nicht geändert, zur Entscheidung): `disk_sync_preview` begrenzt den Body auf 256 KiB (2.3), erlaubt aber Pfade bis 4096 Bytes; 500 solcher Pfade überschreiten die Grenze, obwohl sie gültig wären. Gleiches Muster wie W2-07; bei Bedarf Pfadlänge in der Vorschau oder Grenze anpassen.

Nicht geprüft: ob `FsContext` und `Anchored` mit konfigurierten Wurzeln, deren Pfad selbst Links enthält, wie gewünscht arbeiten; ob `delete_file`-Aufrufer (`reset_imported`) vom neuen Pfad dasselbe Ledger-Verhalten brauchen (5.1); Aufwandsänderung ist eine Schätzung ohne Bau.
