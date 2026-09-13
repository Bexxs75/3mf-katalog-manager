# Katalog-Backup: Export/Import

## Kontext

Die App ist bewusst lokal-only (Cloud-Anbindung wurde entfernt, siehe
CHANGELOG 0.5.0 "Removed"). Der komplette Katalogzustand (Tags, Ordner,
Sammlungen, Filament-Lager, Druckstatus, Warteschlange, Favoriten,
Thumbnails/eigene Bilder) steckt in einer einzigen SQLite-Datei
(`catalog.db` im App-Datenverzeichnis) plus fünf Einstellungen in
`localStorage` (Theme, Sprache, UI-Dichte, bevorzugte Ansicht,
Slicer-Liste). Es gibt aktuell keinen Weg, diesen Zustand zu sichern oder
auf eine andere Maschine mitzunehmen — genau die Situation, die der Nutzer
selbst beim Umzug zwischen Rechnern wiederholt hatte.

Bewusst außerhalb des Scopes: Die eigentlichen 3MF/STL-Modelldateien werden
**nicht** mitgesichert oder kopiert — sie liegen an einem vom Nutzer
gewählten, potenziell sehr großen Speicherort und werden nur per absolutem
Pfad referenziert. Fehlen sie nach einem Umzug/Restore am referenzierten
Pfad, greift bereits die bestehende "Aufräum-Vorschläge"-Funktion
(verwaiste Pfade erkennen). Backup/Restore sichert ausschließlich
Katalog-**Metadaten**, keine Binärdateien der Modelle selbst.

## Architektur

### Export

Ein neuer Tauri-Command `export_catalog` (async, braucht `AppHandle` für
den Speichern-Dialog):

1. Frontend sammelt die fünf `localStorage`-Werte (Keys `3mf-katalog-theme`,
   `3mf-katalog-display-preference`, `3mf-katalog-language`,
   `3mf-katalog-slicers`, `3mf-katalog-density`) in ein JSON-Objekt und
   übergibt es dem Command als `settings_json: String`.
2. Backend öffnet einen Speichern-Dialog (Vorschlag:
   `3mf-katalog-backup_<YYYY-MM-DD>.zip`), Filter `*.zip`.
3. Backend erstellt eine konsistente Kopie der laufenden Datenbank über
   SQLite's Online-Backup-API (`rusqlite::backup::Backup`, **nicht** ein
   simples Datei-Kopieren — die DB kann während des Exports von anderen
   Threads/Requests geöffnet sein, ein rohes `fs::copy` könnte eine
   inkonsistente WAL-Zwischenstufe erwischen) in eine temporäre Datei.
4. Backend packt `catalog.db` (die Backup-Kopie) und `settings.json`
   (`settings_json`-Inhalt) in ein ZIP-Archiv am gewählten Speicherort
   (Rust-`zip`-Crate ist bereits Projektabhängigkeit).
5. Erfolg wird als `CmdResult<()>` zurückgegeben; Fehler (z. B. Backup-API
   schlägt fehl, kein Schreibrecht am Zielort) propagieren als String-Fehler
   wie bei allen anderen Commands.

### Import

Ein neuer Tauri-Command `import_catalog` (async):

1. Backend öffnet einen Öffnen-Dialog, Filter `*.zip`.
2. Backend entpackt `catalog.db` und `settings.json` aus dem gewählten
   Archiv in einen temporären Ordner, validiert grob, dass `catalog.db`
   eine lesbare SQLite-Datei ist (`Connection::open` + eine einfache
   `SELECT 1`-Prüfung) — schlägt das fehl, wird der Import abgebrochen,
   **bevor** irgendetwas am bestehenden Zustand verändert wird.
3. Die aktuell laufende DB-Verbindung (`AppState.db`) hält die echte
   `catalog.db` offen gehalten von SQLite (WAL-Modus) — ein Hot-Swap der
   Datei während die App läuft ist riskant. Stattdessen:
   - Die bestehende `catalog.db` wird nach
     `catalog.db.bak-<Zeitstempel>` im App-Datenverzeichnis verschoben
     (nicht gelöscht — Sicherheitsnetz gegen einen fehlgeschlagenen
     Import, siehe unten).
   - Die neue `catalog.db` aus dem Archiv wird an ihre Stelle kopiert.
   - `settings.json` wird ans Frontend zurückgegeben; das Frontend schreibt
     die fünf Keys direkt in `localStorage`.
4. Der Command gibt zurück, dass ein **Neustart der App** nötig ist, um die
   neue Datenbank zu laden (die laufende `rusqlite::Connection` in
   `AppState` zeigt weiter auf die alte, inzwischen umbenannte Datei-Handle
   — ein Neustart ist der einfachste, robusteste Weg, das sauber
   aufzulösen, kein Live-Reconnect-Code nötig). Frontend zeigt einen klaren
   Hinweis-Dialog ("Import erfolgreich, bitte App neu starten") statt die
   App automatisch zu beenden (kein Datenverlust bei ungespeicherten
   Zwischenzuständen, falls der Nutzer noch etwas offen hat).
5. Alte `.bak-*`-Dateien werden **nicht** automatisch aufgeräumt (bewusst
   minimal — der Nutzer kann sie im App-Datenverzeichnis manuell löschen,
   kein Rentention-Policy-Feature in diesem Umfang).

## UI

Zwei neue Buttons im bestehenden Einstellungen-Panel (`Header.tsx`, dort wo
Theme/Sprache/Slicer-Konfiguration bereits sitzen), unter einer neuen
Sektion "Katalog-Backup": **"Katalog exportieren"** und **"Katalog
importieren"**. Import zeigt vor dem eigentlichen Vorgang einen
Bestätigungsdialog (zerstörend/ersetzend, auch wenn ein `.bak`-Sicherheitsnetz
existiert — der Nutzer soll wissen, dass der aktuelle Katalogzustand ersetzt
wird).

## Fehlerbehandlung

- Ungültiges/kein ZIP beim Import → Fehler vor jeder Änderung, bestehender
  Zustand bleibt komplett unangetastet.
- Backup-API-Fehler beim Export → Fehler-String an Frontend, keine
  unvollständige ZIP-Datei am Zielort (temporäre Datei erst nach
  erfolgreichem Packen an den finalen Pfad verschieben, nicht direkt
  hineinschreiben).
- Import bricht nach dem Umbenennen der alten DB, aber vor dem Kopieren der
  neuen ab (z. B. Festplatte voll) → alte DB liegt als `.bak-*` vor, aber
  keine `catalog.db` mehr am erwarteten Pfad — im Plan wird dafür ein
  expliziter Wiederherstellungsschritt vorgesehen (die `.bak`-Datei bei
  einem Fehler in diesem Fenster automatisch zurückbenennen), damit die App
  nicht mit fehlender DB neu startet.

## Tests

- Rust: Export erzeugt eine gültige ZIP mit beiden erwarteten Einträgen;
  Import mit gültigem Archiv verschiebt/ersetzt korrekt und legt eine
  `.bak`-Datei an; Import mit ungültigem ZIP/korrupter `catalog.db` bricht
  ab, ohne die bestehende Datei anzufassen; das Rückbenennen-bei-Fehler
  zwischen Umbenennen und Kopieren wird gezielt simuliert (z. B. Zielpfad
  read-only machen) und geprüft, dass die alte DB danach wieder unter dem
  Originalnamen liegt.
