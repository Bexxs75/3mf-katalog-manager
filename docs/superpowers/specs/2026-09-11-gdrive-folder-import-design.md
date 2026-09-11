# Google-Drive-Ordner-Import

**Datum:** 2026-09-11
**Status:** Genehmigt, direkte Umsetzung (kein separater Implementierungsplan)

## Problem

Der lokale Dateisystem-Import unterstützt bereits getrennte "Dateien..."/
"Ordner..."-Optionen, wobei "Ordner..." rekursiv durch alle Unterordner
geht (`collect_supported_files`, `commands.rs`). Der Google-Drive-Import
kennt bisher nur Einzeldatei-Mehrfachauswahl über den Picker
(`open_drive_picker` im `files`-Modus) — bei großen Sammlungen in Drive
(viele Dateien über mehrere Unterordner verteilt) ist das unpraktikabel.

Die Bausteine dafür existieren bereits, sind nur nicht verdrahtet:
- `PickerMode::Folder` (Picker im Ordner-Auswahlmodus) — aktuell nur für
  die Wahl des Upload-Zielordners genutzt.
- `list_folder(folder_id)` in `gdrive.rs` — listet die direkten Kinder
  eines Drive-Ordners (Paginierung, Trash-Filter, `.3mf`/`.stl`-Filter
  client-seitig), vollständig implementiert und getestet, aber aktuell an
  keinen Tauri-Command angebunden (toter Code).
- `import_from_cloud(file_ids)` — importiert eine Liste von Drive-Datei-
  IDs fehlertolerant pro Datei.

## Lösung

### Backend

Neue Hilfsfunktion `collect_cloud_files` in `commands.rs` (cloud-Modul):
läuft **iterativ** (Warteschlange, `VecDeque`) durch den gewählten
Drive-Ordner — echte Rekursion ist für `async fn` in Rust ohne
`Box::pin`-Indirektion nicht möglich, eine Warteschlange ist hier die
einfachere Lösung. Pro Runde `list_folder(Some(folder_id))` aufrufen;
Unterordner-Einträge (`is_folder == true`) werden ans Ende der
Warteschlange gehängt, Datei-Einträge gesammelt. Schlägt ein
`list_folder`-Aufruf fehl (Netzwerk/Auth), bricht die Sammlung komplett
mit Fehler ab (`?`) — das unterscheidet sich bewusst vom Download/Import
selbst: Sammeln legt fest, WAS importiert wird (ein Fehler hier macht das
Ergebnis unvollständig auf eine für den Nutzer nicht sichtbare Art, daher
lieber klar abbrechen und erneut versuchen lassen), während der
anschließende Download+Import weiterhin pro Datei fehlertolerant bleibt
(bestehendes Verhalten, siehe Kommentar in `import_from_cloud`).

Die bestehende Download+Import-Schleife aus `import_from_cloud` (Zeilen
223 ff. in `commands.rs`, cloud-Modul) wird in eine private Hilfsfunktion
`import_cloud_file_ids(app, state, file_ids) -> CmdResult<ImportResultDto>`
ausgelagert. `import_from_cloud` (Einzeldatei-Import) wird zu einem
dünnen Wrapper, der diese Funktion direkt mit den übergebenen IDs
aufruft. Neuer Tauri-Command:

```rust
#[tauri::command]
pub async fn import_folder_from_cloud(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    folder_id: String,
) -> CmdResult<ImportResultDto> {
    let file_ids = collect_cloud_files(&state, &folder_id)
        .await
        .map_err(|e| e.to_string())?;
    import_cloud_file_ids(app, state, file_ids).await
}
```

Registrierung in `lib.rs` neben den bestehenden `commands::` Cloud-
Einträgen.

### Frontend

Neuer Menüpunkt "Ordner aus Drive importieren..." (i18n-Key
`importCloudFolderOption`) im Import-Dropdown in `Header.tsx`, direkt
unter dem bestehenden "aus Cloud importieren..."-Eintrag, nur sichtbar
wenn `cloudDriveConnected` (gleiche Bedingung wie der bestehende
Cloud-Eintrag). Neuer Handler in `App.tsx`:

```ts
const importFolderFromCloud = () => {
  invoke<PickerResultDto>('open_drive_picker', { mode: 'folder' })
    .then((result) => {
      if (result.cancelled) return;
      const folderId = result.items[0]?.id;
      if (!folderId) return;
      return invoke<ImportResultDto>('import_folder_from_cloud', { folderId })
        .then(mergeImported);
    })
    .catch((e) => {
      console.error('[cloud] Ordner-Import aus Google Drive fehlgeschlagen:', e);
      setCloudError(String(e));
    });
};
```

Analog zum bestehenden `importFromCloud`/`uploadWithFolderPicker`-Muster.
Neue Prop `onImportFolderFromCloud` auf `Header`, verdrahtet wie
`onImportFromCloud`.

## Nicht-Ziele

- Keine Übernahme der Drive-Ordnerstruktur in App-Ordner (flacher Import,
  `folder_id` bleibt `None` — konsistent mit dem heutigen lokalen
  Ordner-Import).
- Kein Fortschrittsbalken/Live-Zwischenstand während des Sammelns bei
  sehr großen Bäumen — der Command liefert erst am Ende ein Ergebnis,
  wie auch `import_from_cloud` heute.

## Risiko: `drive.file`-Scope und Ordner-Inhalte

Google-Dokumentation zufolge gewährt `drive.file` beim Auswählen eines
Ordners über den Picker (mit korrekt gesetztem `setAppId`, siehe
`fix: Google-Picker setAppId() ergänzt`) automatisch auch Zugriff auf
dessen gesamten Inhalt inkl. Unterordner — das lässt sich aber nur mit
einem echten Google-Konto verifizieren, nicht in Unit-Tests (die
bestehenden `gdrive.rs`-Tests mocken den HTTP-Layer). Falls der reale
Test zeigt, dass `list_folder` für einen per Picker gewählten Ordner
trotzdem 403/404 liefert, wäre der nächste Schritt entweder ein breiterer
Scope (CASA-Audit-Konsequenz) oder Einzeldatei-Auswahl statt Ordner-Wahl
im Picker. Dieser Punkt wird nach der Implementierung vom Nutzer live
geprüft, bevor das Feature als abgeschlossen gilt.

## Verifikation

`cargo test` für `collect_cloud_files`-Logik (mit dem bestehenden
`MockProvider`-Testmuster aus `provider.rs`: mehrstufiger Ordnerbaum,
prüfen dass alle Dateien über alle Ebenen gesammelt werden und Ordner
selbst nicht im Ergebnis landen). `npm run build` fürs Frontend. Danach
manueller Live-Test durch den Nutzer mit echtem Google-Konto (siehe
Risiko-Abschnitt oben) — das kann von hier aus nicht automatisiert
verifiziert werden.
