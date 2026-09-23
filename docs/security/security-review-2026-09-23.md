# Security Review — 3mf Katalog Manager

**Datum:** 2026-09-23
**Geprüfter Stand:** Commit `d806fc9` (master), Feature "Archive direkt entpacken"
**Methode:** Design- und Code-Review vor bzw. begleitend zur Implementierung (Rust-Backend
`src-tauri/src/archive/`, `src-tauri/src/commands/archives.rs`, betroffene Stellen in
`src-tauri/src/db/repository.rs`) sowie der neuen Frontend-Komponente `ArchiveImportDialog`.
Geprüft wurden der neue Entpack-Pfad und alle bestehenden Stellen, an denen Inhalte aus fremden
Archiven landen: Datenbank, Oberfläche, Parser, Dateisystem. Alle Maßnahmen sind umgesetzt und im
Umsetzungsplan mit Tests hinterlegt.

Befunde werden nach CWE klassifiziert; der Schweregrad folgt derselben Skala (Hoch/Mittel/Niedrig)
wie in den früheren Reviews.

---

## Zusammenfassung

| ID | Befund | Schweregrad | CWE | Status |
|----|--------|-------------|-----|--------|
| S-01 | Schreiben in geschützte Bereiche über Unterpfade | Hoch | CWE-22, CWE-73 | ✅ umgesetzt |
| S-02 | Gefährliche Dateitypen (Verknüpfungen, Skripte, `desktop.ini`) | Hoch | CWE-200, CWE-451 | ✅ umgesetzt |
| S-03 | Verlust der Herkunftsmarkierung (Mark of the Web / Quarantäne) | Mittel | CWE-693 | ✅ umgesetzt |
| S-04 | Frontend steuert Pfad und Löschen | Mittel | CWE-285 | ✅ umgesetzt |
| S-05 | Ressourcenerschöpfung (Speicher, Einträge, tar-Bombe) | Mittel | CWE-400, CWE-409, CWE-367 | ✅ umgesetzt |
| S-06 | Rechte aus dem Archiv (ausführbar, setuid) | Niedrig | CWE-732 | ✅ umgesetzt |
| S-07 | Panic in `update_paths_under_folder` bei inkonsistenter Ordnerhierarchie | Niedrig (bestehend) | CWE-248 | ✅ behoben |
| S-08 | Fremdbibliothek (UnRAR) schreibt direkt ans Ziel | Mittel | CWE-367, CWE-59 | ✅ umgesetzt (neu gefasst) |
| S-09 | RAR-Datei-Kopie-/Hardlink-Einträge lesen beliebige lokale Dateien | Kritisch | CWE-59, CWE-22 | ✅ behoben |

Kein Fund zu: SQL-Injection (weiterhin durchgehend parametrisiert), Pfad-Präfix-Abfragen (kein
`LIKE` auf Pfaden), XSS/HTML-Injection (React maskiert alle Namen, CSP aktiv), Parser-Seiteneffekte
(OBJ-Parser ignoriert `mtllib`, 3MF-Teilmodelle bleiben im eigenen Container), Quell-URL
(unverändert nur manuell gesetzt), Überschreiben/Symlink-Folgen am Ziel (`create_new`/O_EXCL).
Details siehe „Geprüft, unauffällig" unten.

---

## S-01 — Schreiben in geschützte Bereiche über Unterpfade (Hoch)

**Betroffen:** `src-tauri/src/commands/archives.rs` (Prüfung von `dest` vor dem Entpacken sowie der
`guard`-Closure, die an `archive::extract_archive` durchgereicht wird), `src-tauri/src/archive/paths.rs`
(`safe_folder_name`).
**CWE:** CWE-22 (Path Traversal), CWE-73 (External Control of File Name or Path)

Geprüft wurde ursprünglich nur der Zielordner selbst. Ein Archiv `.local.zip`, im Home-Verzeichnis
„zusammengeführt", hätte `~/.local` als Ziel (für sich genommen nicht geschützt) und über den
Eintrag `share/applications/x.desktop` einen Starter im Anwendungsmenü abgelegt — Codeausführung
per Social Engineering, ganz ohne kompromittiertes Frontend.

**Maßnahme:** Die Schutzliste (`reject_if_sensitive_path` / `reject_if_sensitive_path_expanded`)
prüft **jeden** neu anzulegenden Pfad, nicht nur den Zielordner — als `guard`-Closure direkt in
`archive::extract_archive` eingehängt. Zielordner-Namen beginnen dank `safe_folder_name` nie mit
einem Punkt. Seit der Abschluss-Korrekturrunde werden Zielordner und Entpack-Ordner (`dest`)
außerdem abgelehnt, wenn sie ein geschütztes Verzeichnis **enthalten** (`reject_if_ancestor_of_sensitive`,
aufgelöst über `resolve_path_for_sensitivity_check`, also auch bei Symlinks) — das sperrt u. a. das
Home-Verzeichnis selbst (enthält `~/.ssh`), `~/.local` und `/` als Ziel. Der `guard` bleibt als
zweite Verteidigungslinie bestehen.

**Test:** `guard_blocks_protected_destinations_and_entries` (`src-tauri/src/archive/tests.rs`),
`a_sensitive_target_is_rejected_before_anything_is_written`,
`merging_into_a_folder_that_contains_a_protected_path_is_refused`,
`a_target_or_destination_containing_a_protected_folder_is_rejected`,
`hidden_folder_names_from_the_frontend_are_neutralized` (`src-tauri/src/commands/archives.rs`,
Modul `tests`).

---

## S-02 — Gefährliche Dateitypen (Hoch)

**Betroffen:** `src-tauri/src/archive/mod.rs` (`is_blocked_path`), `src-tauri/src/archive/paths.rs`
(`sanitize_component`).
**CWE:** CWE-200 (Information Exposure), CWE-451 (User Interface (Mis)representation of Critical
Information)

Verknüpfungen (`.lnk`, `.url`, `.scf`, `.library-ms`, `.searchConnector-ms`) und `desktop.ini` mit
UNC-Icon-Pfad senden unter Windows schon beim Öffnen des Ordners den NTLM-Hash des Nutzers an einen
fremden Server. Programme und Skripte lassen sich per Unicode-Steuerzeichen (U+202E, Bidi-Override)
als Modell tarnen.

**Maßnahme:** Sperrliste für ausführbare Dateien, Skripte, Verknüpfungen, Starter (`.desktop`,
`.app`, `.command`), Disk-Images und `desktop.ini`/`autorun.inf`/`.directory` — diese Einträge
werden nie entpackt und im Ergebnis-Banner gezählt (`archiveSummaryBlockedSkipped`). Bidi- und
Zero-Width-Zeichen in Dateinamen werden beim Entpacken ersetzt.

**Test:** `blocked_file_types_are_never_extracted`,
`sanitize_component_neutralizes_bidi_and_zero_width_chars_and_superscript_devices`,
`sanitize_component_covers_all_windows_device_names_and_trailing_stem_padding`
(`src-tauri/src/archive/tests.rs`), `blocked_file_types_are_reported_in_the_outcome`
(`src-tauri/src/commands/archives.rs`, Modul `tests`).

---

## S-03 — Verlust der Herkunftsmarkierung (Mittel)

**Betroffen:** `src-tauri/src/archive/origin.rs`, verdrahtet in `src-tauri/src/archive/extract.rs`
(`Extractor::new`, Anwendung nach jedem erfolgreichen Entpacken eines Eintrags).
**CWE:** CWE-693 (Protection Mechanism Failure)

Browser markieren heruntergeladene Dateien als „aus dem Internet" (Windows Mark-of-the-Web,
macOS-Quarantäne-Attribut). Entpackte Dateien ohne diese Markierung umgehen SmartScreen, den
Office-Makroschutz und Gatekeeper.

**Maßnahme:** Die Markierung des Archivs selbst wird gelesen (`origin::read`) und auf jede
entpackte Datei übertragen (`origin::apply`) — plattformspezifisch über den Alternate-Data-Stream
`Zone.Identifier` (Windows) bzw. das erweiterte Attribut `com.apple.quarantine` (macOS).

**Test:** kein dedizierter Unit-Test — die Mechanik ist plattformspezifisch (ADS unter Windows,
xattr unter macOS) und in dieser Linux-Testumgebung nicht sinnvoll automatisiert prüfbar. Der
Aufruf ist aber fester, nicht optionaler Bestandteil der Extraktionspipeline
(`archive/extract.rs`) und läuft bei jedem der übrigen Roundtrip-/Extraktionstests mit.

---

## S-04 — Frontend steuert Pfad und Löschen (Mittel)

**Betroffen:** `src-tauri/src/commands/archives.rs` (`PendingArchives`, `extract_archives`).
**CWE:** CWE-285 (Improper Authorization)

Ohne Gegenmaßnahme hätte `extract_archives` jede vom Frontend übergebene Datei mit Archiv-Endung
entpackt und auf Wunsch gelöscht — relevant bei einer künftigen XSS-Lücke im Frontend. Die erste
Umsetzung vertraute dabei noch zwei Angaben des Frontends: der Pfadliste von `import_dropped` (jede
Datei ließ sich als „Drop" melden) und dem frei wählbaren Zielordner `target_dir`.

**Maßnahme (nach dem Abschluss-Review verschärft):**
- **Archive:** Serverseitige Freigabeliste `PendingArchives`. `import_files` trägt die im nativen
  Dateidialog (im Backend) gewählten Archive direkt ein. Drag & Drop beobachtet das Backend selbst:
  Ein Fenster-Ereignis-Handler (`WindowEvent::DragDrop(DragDropEvent::Drop)` in `src-tauri/src/lib.rs`)
  vermerkt die abgelegten Archive; `import_dropped` übernimmt nur Archive aus dieser Menge
  (`claim_dropped`, einmalig) und ignoriert alle anderen Archiv-Pfade. `take_authorized()` entfernt
  jeden Pfad beim Verbrauch (einmalige Verwendung); alles andere landet mit Fehler „Archiv wurde
  nicht über den Import freigegeben" im Ergebnis.
- **Zielordner:** `extract_archives` akzeptiert `target_dir` nur, wenn er exakt einem Katalogordner
  (Tabelle `folders`) entspricht oder zuvor in einem Ordner-Auswahldialog der App gewählt wurde
  (jeder `pick_folder_path`-Aufruf, also z. B. auch im Ersteinrichtungsdialog; `ApprovedTargets`);
  sonst Fehler „Zielordner wurde nicht über die App ausgewählt". Dazu kommt die Vorfahren-Prüfung aus
  S-01. Das verhindert, dass ein kompromittiertes Frontend ein **beliebiges** Ziel unterschiebt; es
  kann aber über andere Befehle weitere, nicht geschützte Ordner zu Katalogordnern machen (siehe R-7).
- **Reihenfolge:** Alle Zielordner-Prüfungen laufen **vor** dem Verbrauch der Freigaben — nach einem
  abgelehnten Ziel kann der Nutzer mit einem anderen Ordner erneut starten.
- **Löschen (Verhaltensänderung):** Das Original wird nur noch gelöscht, wenn **alle** Einträge
  entpackt wurden (keine vorhandenen, unsicheren oder gesperrten Einträge übersprungen) und die
  Datei seit der Prüfung unverändert ist; sonst bleibt es liegen und das Banner nennt den Grund.

**Test:** `only_registered_archives_are_authorized_and_only_once`,
`import_dropped_only_accepts_archives_the_backend_saw_being_dropped`,
`target_dir_must_be_a_catalog_folder_or_picked_in_the_app`,
`an_unapproved_target_is_rejected_without_consuming_the_archives`,
`delete_removes_the_archive_only_when_unchanged_since_inspect`,
`the_archive_is_kept_when_some_entries_were_not_extracted`,
`a_failed_catalog_import_removes_the_extracted_folder_and_keeps_the_archive`,
`a_broken_archive_fails_alone_and_is_never_deleted` (`src-tauri/src/commands/archives.rs`, Modul
`tests`).

---

## S-05 — Ressourcenerschöpfung (Mittel)

**Betroffen:** `src-tauri/src/archive/mod.rs` (`MAX_UNPACKED_BYTES` = 2 GiB, `MAX_ENTRIES` = 10 000),
`src-tauri/src/archive/formats.rs` (`LimitedReader`, `TAR_STREAM_LIMIT`, `XZ_MEM_LIMIT_KIB` = 256 MiB,
7z-Blockprüfung gegen `MAX_UNPACKED_BYTES`), `src-tauri/src/archive/extract.rs`
(`ensure_budget_for`, Eintragszähler gegen `MAX_ENTRIES`).
**CWE:** CWE-400 (Uncontrolled Resource Consumption), CWE-409 (Improper Handling of Highly
Compressed Data / Decompression Bomb), CWE-367 (TOCTOU)

Dekoder reservieren teils Speicher nach der Wörterbuchgröße im Header (xz, LZMA in ZIP, 7z: bis
4 GB, mit `panic = "abort"` also ein harter Absturz statt eines Fehlers). Eine tar-Bombe in
*übersprungenen* Einträgen hätte das Schreib-Budget umgehen können, da nur geschriebene Bytes
gezählt wurden. Das Eintragslimit galt ursprünglich nur in `inspect`; ein zwischen Prüfung und
Entpacken ausgetauschtes Archiv (TOCTOU) hätte es umgangen.

**Maßnahme:** xz-Speicherlimit 256 MiB; ZIP nur Stored/Deflate/Deflate64/Bzip2/Zstd (LZMA/XZ in ZIP
werden abgelehnt); 7z-Blöcke werden vor dem Dekodieren gegen `MAX_UNPACKED_BYTES` geprüft
(begrenzt damit auch das LZMA-Wörterbuch auf die Blockgröße); der dekomprimierte tar-Strom ist über
`LimitedReader`/`TAR_STREAM_LIMIT` unabhängig von übersprungenen Einträgen gedeckelt; das
Eintragslimit gilt jetzt auch während des eigentlichen Entpackens, nicht nur bei `inspect`.

**Test:** `exceeding_the_byte_budget_aborts_and_removes_a_new_destination`,
`exceeding_the_byte_budget_in_merge_mode_restores_the_previous_state`,
`entry_limit_is_enforced_during_extraction_even_without_inspect`,
`inspect_reports_too_large_for_too_many_entries`,
`zip_entries_with_lzma_or_xz_compression_are_unsupported` (`src-tauri/src/archive/tests.rs`),
`limited_reader_passes_data_up_to_the_limit_and_flags_more` (`src-tauri/src/archive/formats.rs`,
Modul `tests`).

---

## S-06 — Rechte aus dem Archiv (Niedrig)

**Betroffen:** `src-tauri/src/archive/extract.rs` (Datei-Erzeugung mit `OpenOptionsExt::mode`).
**CWE:** CWE-732 (Incorrect Permission Assignment for Critical Resource)

tar- und RAR-Archive bringen eigene Unix-Rechte mit (ausführbar, im Extremfall setuid-Bit).

**Maßnahme:** Alle entpackten Dateien werden unabhängig vom Archiv-Eintrag mit `0o644` angelegt
(`create_new` + `OpenOptionsExt::mode(0o644)`); das gilt auch für RAR, das seit S-08/S-09 über
dieselbe Schreibfunktion geschrieben wird.

**Test:** `extracted_files_are_never_executable_and_symlinked_merge_roots_are_refused`
(`src-tauri/src/archive/tests.rs`).

---

## S-07 — Panic in `update_paths_under_folder` (Niedrig, bestehend)

**Betroffen:** `src-tauri/src/db/repository.rs` (`update_paths_under_folder`).
**CWE:** CWE-248 (Uncaught Exception / hier: Rust-Panic)

Byte-Slicing `&child_old_path[old_path.len()..]` panickt bei einer inkonsistenten Ordnerhierarchie
(z. B. aus einem präparierten Katalog-Backup) und beendet die App abrupt — ein bereits vor diesem
Feature bestehendes Problem, das durch Archive mit vielen erzeugten Unterordnern relevanter wird.

**Maßnahme:** `strip_prefix` mit regulärer Fehlerrückgabe statt Byte-Slicing; die Transaktion wird
bei einem inkonsistenten Kind-Pfad zurückgerollt statt die App abstürzen zu lassen.

**Test:**
`update_paths_under_folder_rejects_a_child_whose_path_is_not_below_the_parent`
(`src-tauri/src/db/repository.rs`).

---

## S-08 — Fremdbibliothek (UnRAR) schreibt direkt ans Ziel (Mittel, Ergänzung nach Review von Task 2, neu gefasst im Abschluss-Review)

**Betroffen:** `src-tauri/src/archive/formats.rs` (`extract_rar`, `rar_entry_decision`,
`MAX_RAR_ENTRY_BYTES`).
**CWE:** CWE-367 (TOCTOU), CWE-59 (Improper Link Resolution Before File Access)

Das `unrar`-Crate schreibt Einträge beim Entpacken über die UnRAR-Bibliothek selbst auf die Platte
(`header.extract_to(...)`) statt über unsere abgesicherte Schreibfunktion. Ohne Gegenmaßnahme hätte
damit für RAR-Archive „nie überschreiben / keinem Symlink folgen" nicht gegolten — im Gegensatz zu
allen anderen Formaten, die ausschließlich über `create_new` (O_EXCL) in `extract.rs` schreiben.

**Maßnahme:** Die erste Umsetzung (UnRAR entpackt in einen privaten Staging-Ordner, von dort wird
kopiert) wurde im Abschluss-Review durch S-09 als unzureichend erkannt und ersetzt: UnRAR schreibt
jetzt **gar nichts** mehr auf die Platte. Jeder Eintrag wird mit `header.read()` im
UnRAR-**Testmodus** in den Speicher gelesen (`RAR_TEST`: UnRAR legt dabei keine Dateien, Links oder
Kopien an) und dann wie bei allen anderen Formaten über die abgesicherte Schreibfunktion
(`create_new`, `0o644`, Byte-Budget, Herkunftsmarkierung) ins Ziel geschrieben. Vor dem Lesen werden
die Header-Größe gegen `MAX_RAR_ENTRY_BYTES` (1 GiB, der Eintrag liegt vollständig im Speicher) und
gegen das verbleibende Byte-Budget (`ensure_budget_for`) geprüft. Weicht die Länge der gelesenen
Daten von der Header-Größe ab, wird nichts geschrieben und der Eintrag als unsicher gezählt.

**Bezug zu R-3 (Restrisiko, siehe unten):** RAR ist lokalem TOCTOU nicht stärker ausgesetzt als die
übrigen Formate.

**Test:** `rar_entry_decision_limits_size_and_skips_entries_without_matching_data`
(`src-tauri/src/archive/formats.rs`, Modul `tests`), `rar_budget_check_prevents_oversized_entry`,
`rar4_and_rar5_fixtures_extract`, `inspect_reports_encrypted_7z_and_rar_and_unsupported_multipart_rar`
(`src-tauri/src/archive/tests.rs`).

---

## S-09 — RAR-Datei-Kopie-/Hardlink-Einträge lesen beliebige lokale Dateien (Kritisch, Abschluss-Review)

**Betroffen:** `src-tauri/src/archive/formats.rs` (`extract_rar`, vorher `header.extract_to`).
**CWE:** CWE-59 (Improper Link Resolution Before File Access), CWE-22 (Path Traversal)

RAR5 kennt Einträge vom Typ „Datei-Kopie" und „Hardlink" (mit `rar -oi` erzeugt), die statt Daten
nur einen Verweis auf eine andere Datei enthalten. Das `unrar`-Crate übergibt beim Entpacken nur
einen vollständigen Zielnamen (DestName; unter Linux auch bei `extract_with_base`). In diesem Modus
schaltet UnRAR seine eigene Pfadbehandlung ab und löst die Quelle solcher Einträge relativ zum
Arbeitsverzeichnis des App-Prozesses auf (meist das Home-Verzeichnis). Ein präpariertes Archiv mit
einem Eintrag `modell.stl → .ssh/id_ed25519` hätte so den privaten SSH-Schlüssel des Nutzers in den
Katalogordner kopiert. `rar_meta` erkennt solche Einträge nicht, sie sehen wie normale Dateien aus;
auch der Staging-Ordner aus S-08 half nicht, da die Kopie dort als gewöhnliche Datei ankam.

**Maßnahme:** Entpacken im Testmodus in den Speicher (siehe S-08). Im Testmodus führt UnRAR keine
Kopie-, Hardlink- oder Symlink-Operation aus; Referenz-Einträge liefern keine Daten. Je nach Header
gibt es drei Ausgänge, in keinem gelangt der Inhalt der referenzierten lokalen Datei in den Katalog:
(1) Header-Größe > 0: Die gelesene Länge (0) passt nicht, der Eintrag wird übersprungen und als
unsicher gezählt (das Banner zeigt ihn an). (2) Header-Größe 0: Es entsteht eine leere Datei.
(3) Meldet UnRAR beim Testen einen Prüfsummenfehler (gespeicherte Prüfsumme passt nicht zu den
leeren Daten), bricht das ganze Archiv ab und alles bereits Entpackte wird entfernt.

**Test:** `rar_entry_decision_limits_size_and_skips_entries_without_matching_data`
(`src-tauri/src/archive/formats.rs`, Modul `tests`) für die Entscheidungslogik; die bestehenden
RAR-Fixture-Tests für den Testmodus-Pfad. Ein Fixture mit echtem Datei-Kopie-Eintrag gibt es nicht
(kein `rar`-Werkzeug in der Build-Umgebung, UnRAR kann nur entpacken).

---

## Geprüft, unauffällig

- **SQL-Injection:** Alle Abfragen mit Namen, Pfaden oder Metadaten nutzen gebundene Parameter.
  Dynamisches SQL gibt es nur mit Konstanten (Migrationen, `PRAGMA table_info` über eine feste
  Tabellenliste) und im Testcode. Datei- und Ordnernamen aus Archiven können die Datenbank daher
  nicht manipulieren.
- **Pfad-Präfix-Abfragen:** Es gibt kein `LIKE` auf Pfaden, deshalb keine Wildcard-Effekte durch
  `%`/`_` in Namen.
- **XSS / HTML-Injection:** React maskiert alle Namen und Metadaten, es gibt kein `innerHTML` im
  Produktivcode. Die CSP (`script-src 'self'`, `img-src 'self' data:`) verhindert Skripte und das
  Nachladen von außen.
- **Parser-Seiteneffekte:** Der OBJ-Parser ignoriert `mtllib` (kein Lesen fremder Dateien).
  3MF-Teilmodelle stammen nur aus dem eigenen Container, mit den bestehenden Größenlimits.
- **Quell-URL:** wird nur manuell gesetzt und von `validate_source_url` geprüft, nie aus
  Datei-Metadaten übernommen.
- **UnRAR-Version:** Das `unrar`-Crate (`unrar_sys` 0.5.8) bündelt UnRAR 7.1. Darin sind
  CVE-2022-30333 (Pfad-Traversal über Symlinks, behoben in 6.12) und CVE-2023-40477
  (Codeausführung über Recovery-Volumes, behoben in 6.23) bereits behoben. Zusätzlich lässt die App
  UnRAR selbst nichts mehr anlegen (Testmodus, S-08/S-09): Symlink-Einträge werden anhand der
  Dateiattribute übersprungen, Hardlink- und Datei-Kopie-Einträge liefern keine Daten und werden
  ebenfalls übersprungen.
- **Überschreiben/Symlink-Folgen am Ziel:** `create_new` (O_EXCL) folgt keinem Symlink an der
  Zieldatei; vorhandene Symlinks in Zwischenordnern werden erkannt und nicht betreten.

## Akzeptierte Restrisiken

- **R-1 UnRAR-Wörterbuch:** Die UnRAR-Bibliothek erlaubt Wörterbücher bis 4 GB pro Eintrag (Standard
  der DLL). Über das `unrar`-Crate lässt sich das nicht senken. Ein präpariertes RAR kann daher
  kurzzeitig bis zu 4 GB Speicher anfordern. Behebbar später über einen Crate-Fork mit
  `UCM_LARGEDICT`-Callback. Zusätzlich hält die App seit S-08/S-09 einen RAR-Eintrag (höchstens
  1 GiB) vollständig im Speicher; weil der Puffer im `unrar`-Crate schrittweise wächst (keine
  Vorab-Reservierung möglich), kann der Speicherbedarf bei einem Eintrag nahe 1 GiB kurzzeitig
  etwa 2–3 GiB erreichen.
- **R-2 7z-Header:** Ein komprimierter 7z-Header wird von `sevenz-rust2` dekodiert, bevor unsere
  Blockprüfung greift, und zwar ohne Speicherlimit.
- **R-3 Lokale TOCTOU:** Wer bereits Schreibrechte im Katalogordner hat, könnte zwischen Prüfung und
  Anlegen einen Zwischenordner gegen einen Symlink tauschen. Voll ausschließen ließe sich das nur
  mit `openat`-basiertem Schreiben (z. B. `cap-std`). Ein solcher Angreifer hat das System ohnehin
  schon kompromittiert. RAR ist davon seit S-08/S-09 nicht stärker betroffen als die übrigen
  Formate.
- **R-4 Parser-Abstürze:** STEP-Dateien werden im Prozess über OCCT gelesen. Stürzt OCCT bei einer
  präparierten Datei ab, endet die App mitten im Import. Die entpackten Dateien bleiben dann liegen,
  es entstehen aber keine halben Katalogeinträge (die Transaktion ist nicht committet). Das Risiko
  bestand schon vorher, Archive erhöhen nur die Menge der Dateien pro Vorgang.
- **R-5 Dokumente mit aktiven Inhalten** (Office-Makros, PDF) liegen außerhalb der Sperrliste. Sie
  sind über die übertragene Herkunftsmarkierung (S-03) abgesichert.
- **R-6 Vorhersehbarer Staging-Ordnername — entfällt:** Mit S-08/S-09 gibt es keinen
  RAR-Staging-Ordner mehr; das frühere Verfügbarkeits-Restrisiko (vorab angelegte Ordnernamen)
  besteht nicht mehr.
- **R-7 Katalogordner als Entpack-Ziel (neu):** Ein kompromittiertes Frontend könnte über
  `register_catalog_base_dir` oder Ordner-Drops beliebige nicht geschützte Ordner, die kein
  geschütztes Verzeichnis enthalten, zu Katalogordnern und damit zu erlaubten Entpack-Zielen machen.
  Die Auswirkung ist begrenzt: Entpackt werden kann weiterhin nur ein Archiv, das der Nutzer selbst
  per Dateidialog oder Drag & Drop hereingegeben hat; entpackte Dateien sind immer `0644` und nie
  ausführbar, gesperrte Dateitypen werden gefiltert und nichts wird überschrieben.

## Nicht enthalten (bewusst, unverändert seit der Spezifikation)

- verschachtelte Archive (Archive im Archiv)
- Entpacken beim Ordner-Import
- Verschieben in den System-Papierkorb statt Löschen
- passwortgeschützte Archive

---

# Security Review — 3mf Katalog Manager (English)

**Date:** 2026-09-23
**Reviewed state:** commit `d806fc9` (master), "Extract archives directly" feature
**Method:** Design and code review before and alongside implementation (Rust backend
`src-tauri/src/archive/`, `src-tauri/src/commands/archives.rs`, affected spots in
`src-tauri/src/db/repository.rs`) as well as the new `ArchiveImportDialog` frontend component.
Reviewed were the new extraction path and every existing spot where content from third-party
archives ends up: database, UI, parsers, filesystem. All mitigations are implemented and backed by
tests in the implementation plan.

Findings are classified by CWE; severity follows the same scale (High/Medium/Low) as the earlier
reviews.

---

## Summary

| ID | Finding | Severity | CWE | Status |
|----|---------|----------|-----|--------|
| S-01 | Writing into protected locations via subpaths | High | CWE-22, CWE-73 | ✅ implemented |
| S-02 | Dangerous file types (shortcuts, scripts, `desktop.ini`) | High | CWE-200, CWE-451 | ✅ implemented |
| S-03 | Loss of the origin mark (Mark of the Web / quarantine) | Medium | CWE-693 | ✅ implemented |
| S-04 | Frontend controls path and deletion | Medium | CWE-285 | ✅ implemented |
| S-05 | Resource exhaustion (memory, entries, tar bomb) | Medium | CWE-400, CWE-409, CWE-367 | ✅ implemented |
| S-06 | Permissions carried over from the archive (executable, setuid) | Low | CWE-732 | ✅ implemented |
| S-07 | Panic in `update_paths_under_folder` on an inconsistent folder hierarchy | Low (pre-existing) | CWE-248 | ✅ fixed |
| S-08 | Third-party library (UnRAR) writes directly to the destination | Medium | CWE-367, CWE-59 | ✅ implemented (revised) |
| S-09 | RAR file-copy/hardlink entries read arbitrary local files | Critical | CWE-59, CWE-22 | ✅ fixed |

No finding for: SQL injection (still consistently parameterized), path-prefix queries (no `LIKE`
on paths), XSS/HTML injection (React escapes all names, CSP active), parser side effects (the OBJ
parser ignores `mtllib`, 3MF sub-models stay inside their own container), source URL (still only
set manually), overwrite/symlink-following at the destination (`create_new`/O_EXCL). Details under
"Reviewed, no issue found" below.

---

## S-01 — Writing into protected locations via subpaths (High)

**Affected:** `src-tauri/src/commands/archives.rs` (the check on `dest` before extraction and the
`guard` closure passed into `archive::extract_archive`), `src-tauri/src/archive/paths.rs`
(`safe_folder_name`).
**CWE:** CWE-22 (Path Traversal), CWE-73 (External Control of File Name or Path)

Originally only the target folder itself was checked. An archive named `.local.zip`, "merged" into
the home directory, would have `~/.local` as its target (not protected by itself) and, via an
entry `share/applications/x.desktop`, could have dropped a launcher into the application menu —
code execution via social engineering, with no compromised frontend required.

**Mitigation:** The protection list (`reject_if_sensitive_path` / `reject_if_sensitive_path_expanded`)
checks **every** newly created path, not just the target folder — wired in as a `guard` closure
directly into `archive::extract_archive`. Target folder names never start with a dot, thanks to
`safe_folder_name`. Since the final fix round, the target folder and the extraction folder (`dest`)
are also rejected when they **contain** a protected directory (`reject_if_ancestor_of_sensitive`,
resolved via `resolve_path_for_sensitivity_check`, so symlinks are covered too) — this blocks, among
others, the home directory itself (it contains `~/.ssh`), `~/.local` and `/` as a target. The `guard`
stays in place as a second line of defense.

**Test:** `guard_blocks_protected_destinations_and_entries` (`src-tauri/src/archive/tests.rs`),
`a_sensitive_target_is_rejected_before_anything_is_written`,
`merging_into_a_folder_that_contains_a_protected_path_is_refused`,
`a_target_or_destination_containing_a_protected_folder_is_rejected`,
`hidden_folder_names_from_the_frontend_are_neutralized` (`src-tauri/src/commands/archives.rs`,
`tests` module).

---

## S-02 — Dangerous file types (High)

**Affected:** `src-tauri/src/archive/mod.rs` (`is_blocked_path`), `src-tauri/src/archive/paths.rs`
(`sanitize_component`).
**CWE:** CWE-200 (Information Exposure), CWE-451 (User Interface (Mis)representation of Critical
Information)

Shortcuts (`.lnk`, `.url`, `.scf`, `.library-ms`, `.searchConnector-ms`) and a `desktop.ini` with a
UNC icon path send the user's NTLM hash to a remote server on Windows the moment the folder is
opened. Programs and scripts can be disguised as models using Unicode control characters (U+202E,
bidi override).

**Mitigation:** A block list for executables, scripts, shortcuts, launchers (`.desktop`, `.app`,
`.command`), disk images, and `desktop.ini`/`autorun.inf`/`.directory` — these entries are never
extracted and are counted in the result banner (`archiveSummaryBlockedSkipped`). Bidi and
zero-width characters in filenames are replaced during extraction.

**Test:** `blocked_file_types_are_never_extracted`,
`sanitize_component_neutralizes_bidi_and_zero_width_chars_and_superscript_devices`,
`sanitize_component_covers_all_windows_device_names_and_trailing_stem_padding`
(`src-tauri/src/archive/tests.rs`), `blocked_file_types_are_reported_in_the_outcome`
(`src-tauri/src/commands/archives.rs`, `tests` module).

---

## S-03 — Loss of the origin mark (Medium)

**Affected:** `src-tauri/src/archive/origin.rs`, wired into `src-tauri/src/archive/extract.rs`
(`Extractor::new`, applied after every successfully extracted entry).
**CWE:** CWE-693 (Protection Mechanism Failure)

Browsers mark downloaded files as "from the internet" (Windows Mark-of-the-Web, macOS quarantine
attribute). Extracted files without this mark bypass SmartScreen, Office macro protection, and
Gatekeeper.

**Mitigation:** The archive's own mark is read (`origin::read`) and carried over to every extracted
file (`origin::apply`) — platform-specific, via the `Zone.Identifier` alternate data stream
(Windows) or the `com.apple.quarantine` extended attribute (macOS).

**Test:** no dedicated unit test — the mechanism is platform-specific (ADS on Windows, xattr on
macOS) and cannot be meaningfully automated in this Linux test environment. The call is, however, a
fixed, non-optional part of the extraction pipeline (`archive/extract.rs`) and runs alongside every
other roundtrip/extraction test.

---

## S-04 — Frontend controls path and deletion (Medium)

**Affected:** `src-tauri/src/commands/archives.rs` (`PendingArchives`, `extract_archives`).
**CWE:** CWE-285 (Improper Authorization)

Without a countermeasure, `extract_archives` would have extracted — and, on request, deleted — any
file with an archive extension supplied by the frontend, which matters given a future XSS
vulnerability in the frontend. The first implementation still trusted two frontend inputs: the path
list passed to `import_dropped` (any file could be reported as a "drop") and the freely chosen
target folder `target_dir`.

**Mitigation (tightened after the final review):**
- **Archives:** server-side allowlist `PendingArchives`. `import_files` registers the archives picked
  in the native file dialog (run by the backend) directly. For drag & drop the backend watches the
  drop itself: a window event handler (`WindowEvent::DragDrop(DragDropEvent::Drop)` in
  `src-tauri/src/lib.rs`) records the dropped archives; `import_dropped` only accepts archives from
  that set (`claim_dropped`, single use) and ignores every other archive path. `take_authorized()`
  removes each path when it's consumed (single use); everything else comes back in the result with
  the error "archive was not authorized via import".
- **Target folder:** `extract_archives` only accepts a `target_dir` that exactly matches a catalog
  folder (`folders` table) or was picked beforehand in one of the app's folder-selection dialogs
  (any `pick_folder_path` call, e.g. also in the first-run setup dialog; `ApprovedTargets`); otherwise
  it fails with "target folder was not chosen through the app". The ancestor check from S-01 applies
  on top. This stops a compromised frontend from substituting an **arbitrary** target; it can,
  however, turn further non-protected folders into catalog folders through other commands (see R-7).
- **Order:** all target folder checks run **before** the authorizations are consumed — after a
  rejected target the user can start again with a different folder.
- **Deletion (behavior change):** the original is only deleted when **every** entry was extracted
  (no existing, unsafe or blocked entries skipped) and the file is unchanged since the check;
  otherwise it stays in place and the banner names the reason.

**Test:** `only_registered_archives_are_authorized_and_only_once`,
`import_dropped_only_accepts_archives_the_backend_saw_being_dropped`,
`target_dir_must_be_a_catalog_folder_or_picked_in_the_app`,
`an_unapproved_target_is_rejected_without_consuming_the_archives`,
`delete_removes_the_archive_only_when_unchanged_since_inspect`,
`the_archive_is_kept_when_some_entries_were_not_extracted`,
`a_failed_catalog_import_removes_the_extracted_folder_and_keeps_the_archive`,
`a_broken_archive_fails_alone_and_is_never_deleted` (`src-tauri/src/commands/archives.rs`, `tests`
module).

---

## S-05 — Resource exhaustion (Medium)

**Affected:** `src-tauri/src/archive/mod.rs` (`MAX_UNPACKED_BYTES` = 2 GiB, `MAX_ENTRIES` = 10,000),
`src-tauri/src/archive/formats.rs` (`LimitedReader`, `TAR_STREAM_LIMIT`, `XZ_MEM_LIMIT_KIB` =
256 MiB, 7z block check against `MAX_UNPACKED_BYTES`), `src-tauri/src/archive/extract.rs`
(`ensure_budget_for`, entry counter against `MAX_ENTRIES`).
**CWE:** CWE-400 (Uncontrolled Resource Consumption), CWE-409 (Improper Handling of Highly
Compressed Data / Decompression Bomb), CWE-367 (TOCTOU)

Some decoders allocate memory based on the dictionary size declared in the header (xz, LZMA inside
ZIP, 7z: up to 4 GB, with `panic = "abort"` meaning a hard crash instead of a recoverable error). A
tar bomb hidden in entries that end up *skipped* could have bypassed the write budget, since only
written bytes were counted. The entry limit originally only applied during `inspect`; an archive
swapped out between the check and the actual extraction (TOCTOU) would have bypassed it.

**Mitigation:** xz memory limit of 256 MiB; ZIP only accepts Stored/Deflate/Deflate64/Bzip2/Zstd
(LZMA/XZ inside ZIP are rejected); 7z blocks are checked against `MAX_UNPACKED_BYTES` before
decoding (which also caps the LZMA dictionary at the block size); the decompressed tar stream is
capped via `LimitedReader`/`TAR_STREAM_LIMIT` regardless of skipped entries; the entry limit is now
also enforced during the actual extraction, not only during `inspect`.

**Test:** `exceeding_the_byte_budget_aborts_and_removes_a_new_destination`,
`exceeding_the_byte_budget_in_merge_mode_restores_the_previous_state`,
`entry_limit_is_enforced_during_extraction_even_without_inspect`,
`inspect_reports_too_large_for_too_many_entries`,
`zip_entries_with_lzma_or_xz_compression_are_unsupported` (`src-tauri/src/archive/tests.rs`),
`limited_reader_passes_data_up_to_the_limit_and_flags_more` (`src-tauri/src/archive/formats.rs`,
`tests` module).

---

## S-06 — Permissions carried over from the archive (Low)

**Affected:** `src-tauri/src/archive/extract.rs` (file creation via `OpenOptionsExt::mode`).
**CWE:** CWE-732 (Incorrect Permission Assignment for Critical Resource)

tar and RAR archives carry their own Unix permissions (executable, in the extreme case the setuid
bit).

**Mitigation:** Every extracted file is created with `0o644` regardless of the archive entry
(`create_new` + `OpenOptionsExt::mode(0o644)`); this also applies to RAR, which since S-08/S-09 is
written through the same write function.

**Test:** `extracted_files_are_never_executable_and_symlinked_merge_roots_are_refused`
(`src-tauri/src/archive/tests.rs`).

---

## S-07 — Panic in `update_paths_under_folder` (Low, pre-existing)

**Affected:** `src-tauri/src/db/repository.rs` (`update_paths_under_folder`).
**CWE:** CWE-248 (Uncaught Exception / here: a Rust panic)

Byte slicing `&child_old_path[old_path.len()..]` panics on an inconsistent folder hierarchy (e.g.
from a crafted catalog backup) and abruptly terminates the app — a problem that predates this
feature but becomes more relevant now that archives can create many subfolders at once.

**Mitigation:** `strip_prefix` with a regular error return instead of byte slicing; the transaction
is rolled back on an inconsistent child path instead of crashing the app.

**Test:** `update_paths_under_folder_rejects_a_child_whose_path_is_not_below_the_parent`
(`src-tauri/src/db/repository.rs`).

---

## S-08 — Third-party library (UnRAR) writes directly to the destination (Medium, added after the Task 2 review, revised in the final review)

**Affected:** `src-tauri/src/archive/formats.rs` (`extract_rar`, `rar_entry_decision`,
`MAX_RAR_ENTRY_BYTES`).
**CWE:** CWE-367 (TOCTOU), CWE-59 (Improper Link Resolution Before File Access)

When extracting, the `unrar` crate writes entries to disk itself via the UnRAR library
(`header.extract_to(...)`) instead of going through our hardened write function. Without a
countermeasure, "never overwrite / never follow a symlink" would not have held for RAR archives —
unlike every other format, which writes exclusively through `create_new` (O_EXCL) in `extract.rs`.

**Mitigation:** The first implementation (UnRAR extracts into a private staging folder, which is
then copied) was found insufficient in the final review because of S-09 and replaced: UnRAR no
longer writes **anything** to disk. Every entry is read into memory with `header.read()` in UnRAR's
**test mode** (`RAR_TEST`: UnRAR creates no files, links or copies) and then written to the
destination through the hardened write function (`create_new`, `0o644`, byte budget, origin
marking), just like every other format. Before reading, the header size is checked against
`MAX_RAR_ENTRY_BYTES` (1 GiB, the entry is held in memory completely) and against the remaining byte
budget (`ensure_budget_for`). If the length of the data read differs from the header size, nothing
is written and the entry is counted as unsafe.

**Relation to R-3 (residual risk, below):** RAR is no more exposed to local TOCTOU than the other
formats.

**Test:** `rar_entry_decision_limits_size_and_skips_entries_without_matching_data`
(`src-tauri/src/archive/formats.rs`, `tests` module), `rar_budget_check_prevents_oversized_entry`,
`rar4_and_rar5_fixtures_extract`, `inspect_reports_encrypted_7z_and_rar_and_unsupported_multipart_rar`
(`src-tauri/src/archive/tests.rs`).

---

## S-09 — RAR file-copy/hardlink entries read arbitrary local files (Critical, final review)

**Affected:** `src-tauri/src/archive/formats.rs` (`extract_rar`, previously `header.extract_to`).
**CWE:** CWE-59 (Improper Link Resolution Before File Access), CWE-22 (Path Traversal)

RAR5 has "file copy" and "hardlink" entries (created with `rar -oi`) that contain no data, only a
reference to another file. When extracting, the `unrar` crate passes only a full destination name
(DestName; on Linux even for `extract_with_base`). In that mode UnRAR turns off its own path handling
and resolves the source of such entries relative to the app process's working directory (usually
the home directory). A crafted archive with an entry `model.stl → .ssh/id_ed25519` would have copied
the user's private SSH key into the catalog folder. `rar_meta` does not recognize such entries, they
look like regular files; the staging folder from S-08 did not help either, since the copy arrived
there as an ordinary file.

**Mitigation:** in-memory extraction in test mode (see S-08). In test mode UnRAR performs no copy,
hardlink or symlink operation; reference entries yield no data. Depending on the header there are
three outcomes, and in none of them does the content of the referenced local file reach the catalog:
(1) header size > 0: the length read (0) does not match, so the entry is skipped and counted as
unsafe (the banner shows it). (2) header size 0: an empty file is created. (3) if UnRAR reports a
checksum error while testing (the stored checksum does not match the empty data), the whole archive
is aborted and everything already extracted is removed.

**Test:** `rar_entry_decision_limits_size_and_skips_entries_without_matching_data`
(`src-tauri/src/archive/formats.rs`, `tests` module) for the decision logic; the existing RAR
fixture tests for the test-mode path. There is no fixture with a real file-copy entry (no `rar`
tool in the build environment, UnRAR can only extract).

---

## Reviewed, no issue found

- **SQL injection:** all queries involving names, paths, or metadata use bound parameters. Dynamic
  SQL only appears with constants (migrations, `PRAGMA table_info` over a fixed table list) and in
  test code. File and folder names from archives cannot manipulate the database.
- **Path-prefix queries:** there is no `LIKE` on paths, so no wildcard effects from `%`/`_` in
  names.
- **XSS / HTML injection:** React escapes all names and metadata, there is no `innerHTML` in
  production code. The CSP (`script-src 'self'`, `img-src 'self' data:`) prevents scripts and
  loading external resources.
- **Parser side effects:** the OBJ parser ignores `mtllib` (no reading of foreign files). 3MF
  sub-models come only from their own container, subject to the existing size limits.
- **Source URL:** only ever set manually and validated by `validate_source_url`, never taken from
  file metadata.
- **UnRAR version:** the `unrar` crate (`unrar_sys` 0.5.8) bundles UnRAR 7.1, which already fixes
  CVE-2022-30333 (path traversal via symlinks, fixed in 6.12) and CVE-2023-40477 (code execution
  via recovery volumes, fixed in 6.23). On top of that, the app no longer lets UnRAR create anything
  itself (test mode, S-08/S-09): symlink entries are skipped based on their file attributes, and
  hardlink and file-copy entries yield no data and are skipped as well.
- **Overwrite/symlink-following at the destination:** `create_new` (O_EXCL) never follows a symlink
  at the destination file; existing symlinks in intermediate folders are detected and never entered.

## Accepted residual risks

- **R-1 UnRAR dictionary:** the UnRAR library allows dictionaries up to 4 GB per entry (the DLL's
  default). This cannot be lowered through the `unrar` crate. A crafted RAR can therefore briefly
  request up to 4 GB of memory. Fixable later via a crate fork with a `UCM_LARGEDICT` callback. In addition, since S-08/S-09 the
  app holds one RAR entry (at most 1 GiB) completely in memory; because the buffer inside the
  `unrar` crate grows step by step (no preallocation possible), memory use for an entry close to
  1 GiB can transiently reach about 2–3 GiB.
- **R-2 7z header:** a compressed 7z header is decoded by `sevenz-rust2` before our block check
  applies, without a memory limit.
- **R-3 Local TOCTOU:** anyone who already has write access inside the catalog folder could swap an
  intermediate folder for a symlink between the check and its creation. Fully ruling this out would
  require `openat`-based writes (e.g. `cap-std`). Such an attacker has already compromised the
  system anyway. Since S-08/S-09, RAR is no more exposed to this than the other formats.
- **R-4 Parser crashes:** STEP files are read in-process via OCCT. If OCCT crashes on a crafted
  file, the app terminates mid-import. Already-extracted files remain on disk, but no half-written
  catalog entries result (the transaction is never committed). This risk predates archives; archives
  only increase the number of files per operation.
- **R-5 Documents with active content** (Office macros, PDF) fall outside the block list. They are
  covered by the carried-over origin mark (S-03).
- **R-6 Predictable staging folder name — obsolete:** since S-08/S-09 there is no RAR staging
  folder anymore; the former availability risk (pre-created folder names) no longer exists.
- **R-7 Catalog folders as extraction targets (new):** a compromised frontend could use
  `register_catalog_base_dir` or folder drops to turn any non-protected folder that contains no
  protected directory into a catalog folder, and thus an approved extraction target. The impact is
  limited: only an archive that the user supplied through the file dialog or drag & drop can still be
  extracted; extracted files are always `0644` and never executable, blocked file types are
  filtered, and nothing is overwritten.

## Not included (deliberately, unchanged since the specification)

- nested archives (archives inside archives)
- extraction during folder import
- moving to the system trash instead of deleting
- password-protected archives
