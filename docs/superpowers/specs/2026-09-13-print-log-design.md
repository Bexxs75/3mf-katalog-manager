# Druckprotokoll pro Modell

## Kontext

`files.print_status` ist aktuell ein binärer Toggle
(`not_printed`/`printed`), der u. a. die Warteschlange steuert (aus der
Queue entfernt, sobald "gedruckt" gesetzt wird). Für ein Katalog-Tool mit
Sammel-Charakter ist das zu grob: mehrere Druckversuche desselben Modells
(unterschiedliche Einstellungen, Ausschuss, Notizen zum Ergebnis) lassen
sich damit nicht abbilden.

## Design-Entscheidung: additiv, nicht ersetzend

`print_status` bleibt unverändert bestehen (Warteschlange, Sortierung,
Filter hängen bereits daran, siehe `togglePrintStatus`/`set_print_status`
in `App.tsx`/`commands.rs`). Das Druckprotokoll ist eine **zusätzliche,
unabhängige** Journal-Funktion: mehrere Einträge pro Modell, jeder mit
Datum, optionaler Notiz, optionalem Foto. Es wird **nicht** automatisch aus
dem Protokoll abgeleitet, ob ein Modell "gedruckt" ist, und umgekehrt setzt
das Anlegen eines Protokoll-Eintrags **nicht** automatisch `print_status`.
Beide Mechanismen sind bewusst entkoppelt — der schnelle Toggle bleibt für
Warteschlange/Filter wie gehabt, das Protokoll ist ein optionales Journal
obendrauf. Das hält den Eingriff klein: kein bestehender Code-Pfad
(Queue-Entfernung, Sortierung "Zuletzt gedruckt" o. ä.) muss angepasst
werden.

## Architektur

### DB

Neue Tabelle, analog zu `file_materials`/`file_metadata` (Fremdschlüssel
mit `ON DELETE CASCADE`, damit Löschen eines Modells auch sein Protokoll
mitlöscht):

```sql
CREATE TABLE IF NOT EXISTS print_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    printed_at TEXT NOT NULL,
    note TEXT,
    photo_png BLOB,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_print_log_file_id ON print_log (file_id);
```

Kein `ALTER TABLE`-Migrationsbedarf, da es sich um eine komplett neue
Tabelle handelt (kein Bestandsdaten-Problem wie bei einer neuen Spalte auf
`files`).

### Backend

Neue Commands (Muster: `add_filament_spool`/`list_filament_spools`/
`delete_filament_spool` in `commands.rs`):

- `list_print_log_entries(file_id: String) -> Vec<PrintLogEntryDto>`
- `add_print_log_entry(file_id: String, printed_at: String, note: Option<String>, photo_base64: Option<String>) -> PrintLogEntryDto`
  (Foto-Größenlimit: bestehende `MAX_CUSTOM_IMAGE_BYTES`-Konstante
  wiederverwenden, gleiche Fehlermeldung wie bei `upload_custom_image`)
- `delete_print_log_entry(entry_id: String) -> ()`

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintLogEntryDto {
    pub id: String,
    pub printed_at: String,
    pub note: Option<String>,
    pub photo_image: Option<String>, // data:image/png;base64,... wie thumbnail_image
}
```

### Frontend

Neuer Abschnitt auf der Modell-Detailseite (`ModelDetailPage.tsx`),
unterhalb des Filamentverbrauch-Abschnitts: "Druckprotokoll" mit Liste
bestehender Einträge (neueste zuerst: Datum, Notiz-Text, Foto-Thumbnail
falls vorhanden, Lösch-Button je Eintrag) und einem "+ Eintrag
hinzufügen"-Formular (Datum vorausgefüllt mit heute, editierbar; Notiz als
optionales Textfeld; Foto-Upload über denselben Datei-Dialog wie
`onUploadImage`/`pick_and_read_image`). Liste wird beim Öffnen der
Detailseite per neuem Hook `usePrintLog(fileId)` geladen (eigener State,
nicht Teil von `ModelFile`, da potenziell viele/große Einträge pro Modell —
unnötig, sie bei jedem `list_files` mitzuladen).

## Fehlerbehandlung

- Foto zu groß → gleiche Fehlermeldung/Grenze wie beim bestehenden
  benutzerdefinierten Modellbild.
- Löschen eines Modells löscht sein Protokoll automatisch mit (Cascade) —
  auch beim Papierkorb-Fall: Soft-Delete (`deleted_at` gesetzt) lässt die
  Zeilen unangetastet (wie bei `file_materials`/`file_metadata` schon
  heute), erst der endgültige `DELETE FROM files` löst die Cascade aus.

## Tests

- Rust: Eintrag anlegen und wieder auslesen (roundtrip inkl. Foto als
  Base64 hin/zurück), mehrere Einträge pro Datei in absteigender
  Datumsreihenfolge, Löschen eines Eintrags, Cascade-Löschen beim
  endgültigen Entfernen der zugehörigen Datei, Foto-Größenlimit greift.
