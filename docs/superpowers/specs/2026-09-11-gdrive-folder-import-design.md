# Google-Drive-Ordner-Import

**Datum:** 2026-09-11
**Status:** ZURÜCKGEBAUT (Commit `fcc08cb`, Revert von `8772b8b`) — siehe
"Ergebnis" am Ende. Implementiert, live getestet, am dokumentierten
Berechtigungsrisiko gescheitert; Dokument bleibt als Aufzeichnung stehen,
damit dieselbe Sackgasse nicht erneut versucht wird.

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

## Ergebnis (2026-09-11, nach Live-Test)

Implementiert und deployed als Commit `8772b8b`. Live-Test durch den Nutzer
mit echtem Google-Konto:

1. Einzeldatei-Import ("aus Cloud importieren...") funktionierte nach dem
   `setAppId()`-Fix (`da7c2b1`) einwandfrei — bestätigt das war ein
   separates, echtes Problem und ist gelöst.
2. Ordner-Import lieferte durchgehend 0 Dateien, ohne jede Fehlermeldung
   (weder im Backend-Log noch als `cloudError` in der Sidebar) — auch nach
   vollständigem Neustart der App (schließt Anzeige-/Cache-Artefakt aus).

Root Cause: der `drive.file`-Scope unterstützt grundsätzlich **keine**
Ordner-Auflistung (`files.list` mit `'ID' in parents`) — das ist keine
Fehlkonfiguration, sondern eine von Google dokumentierte Absicht des
Scopes (Zugriff nur auf einzelne, dem Nutzer explizit gezeigte Dateien,
kein Drive-weites Suchen/Browsen). Bestätigt u. a. durch
[Choose Google Drive API scopes](https://developers.google.com/workspace/drive/api/guides/api-specific-auth)
und [Simplifying Folder Selection in Google Picker with drive.file Scope](https://iifx.dev/en/articles/460025056/simplifying-folder-selection-in-google-picker-with-drive-file-scope).
`list_folder`/`collect_cloud_files` selbst sind fehlerfrei — sie fragen
korrekt an, bekommen von Google aber eine leere Trefferliste statt eines
Fehlers zurück, weil der Scope die Anfrage gar nicht erst zulässt.

**Entscheidung des Nutzers:** Feature zurückbauen statt `drive.readonly`
einzuführen (hätte das schon einmal bewusst vermiedene, kostenpflichtige
CASA-Audit erneut fällig gemacht — siehe CHANGELOG "Changed"-Eintrag zur
Scope-Entfernung). Zurückgebaut per `git revert 8772b8b` (Commit
`fcc08cb`): `import_folder_from_cloud`, `collect_cloud_files`,
`partition_cloud_entries` samt Tests, der Menüpunkt "Ordner aus Drive
importieren..." und die zugehörigen i18n-Keys sind wieder entfernt. Der
`setAppId()`-Fix (`da7c2b1`) bleibt bestehen, da er ein echtes,
unabhängiges Problem behoben hat. Die bestehende Mehrfachauswahl im
Datei-Picker (`PickerMode::Files`, `MULTISELECT_ENABLED`) bleibt der Weg
für "mehrere Dateien auf einmal aus Drive importieren", ohne
Ordner-Rekursion.

**Falls das Thema erneut aufkommt:** Ordner-Rekursion aus Google Drive ist
mit dem aktuellen `drive.file`-Scope nicht machbar, ohne den bewusst
vermiedenen CASA-Audit-Scope einzuführen. Vor jedem neuen Versuch zuerst
diese Konsequenz mit dem Nutzer abwägen, nicht erneut implementieren und
erst danach live scheitern lassen.
