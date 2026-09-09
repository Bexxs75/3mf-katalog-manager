# "In Slicer öffnen" — Design

## Ausgangslage

Der Button/Menüpunkt "In Slicer öffnen" existiert bereits im Frontend
(`DetailPanel.tsx`, `ContextMenu.tsx`), ruft aber nur einen leeren Stub in
`App.tsx` auf:

```ts
const openInSlicer = (_id: string) => {
  // Tauri: Pfad an registrierten Slicer übergeben
};
```

Kein Backend-Command existiert dafür bisher.

## Ziel

Statt für jeden Slicer-Hersteller (Bambu Studio, PrusaSlicer, OrcaSlicer,
Cura, ...) eine eigene Integration zu bauen, wählt der Nutzer selbst den
Pfad zu seinem/seinen Slicer-Programm(en) über einen nativen
Datei-Dialog. Die App startet die gewählte Datei mit dem Pfad des
ausgewählten Modells als Argument - herstellerunabhängig.

**Mehrere Slicer werden unterstützt** (nicht nur einer), da manche Nutzer
mehrere Slicer parallel verwenden (z. B. einen zum Kalibrieren, einen für
den finalen Druck).

**Plattform-Scope:** Windows und Linux jetzt. **macOS wird explizit nicht
unterstützt** - Programme sind dort typischerweise `.app`-Bundles (Ordner,
keine einzelnen ausführbaren Dateien) und benötigen einen eigenen
Start-Mechanismus (`open -a` statt direktem Spawnen) sowie eine
Ordner- statt Datei-Auswahl im Dialog. Das Gesamtprojekt hat ohnehin noch
keinen getesteten macOS-Build, macOS-Unterstützung für dieses Feature ist
bewusst als spätere, eigenständige Ergänzung zurückgestellt.

## Architektur

### 1. Datenmodell & Persistenz (Frontend)

Neuer Hook `src/hooks/useSlicers.ts`, nach dem Vorbild von `useTheme.ts`
(gleiches `localStorage`-Persistenzmuster wie Theme/Sprache - keine
Backend-/DB-Beteiligung nötig für eine kurze Konfigurationsliste):

```ts
export interface SlicerConfig {
  id: string;
  name: string;
  path: string;
}

interface SlicersState {
  slicers: SlicerConfig[];
  lastUsedId: string | null;
}
```

Gespeichert unter dem `localStorage`-Key `3mf-katalog-slicers` als JSON.
`id` wird beim Hinzufügen per `crypto.randomUUID()` erzeugt.

Der Hook liefert: `slicers`, `lastUsedId`, `addSlicer(name, path)`,
`removeSlicer(id)`, `setLastUsed(id)`.

### 2. Einstellungen-UI (`src/components/Header.tsx`)

Neuer Abschnitt "Slicer" im bestehenden ⚙-Einstellungsmenü, nach dem
Abschnitt "Sprache":

- Liste der konfigurierten Slicer: Name + Pfad (Pfad gekürzt/mono
  dargestellt), jeweils mit einem Entfernen-Button (✕).
- "+ Hinzufügen"-Button ruft den neuen Backend-Command
  `pick_slicer_executable` auf (siehe Abschnitt 4) - konsistent mit dem
  bestehenden Muster, bei dem `import_files`/`import_folder` den nativen
  Datei-Dialog ebenfalls im Backend öffnen, nicht direkt im Frontend. Der
  vorgeschlagene Name wird im Frontend aus dem zurückgelieferten Pfad
  abgeleitet (Dateiname ohne Endung, z. B. `bambu-studio` aus
  `/usr/bin/bambu-studio`), ist aber vor dem Speichern editierbar
  (einfaches Inline-Textfeld, das nach Dialog-Abschluss erscheint).
  Bricht der Nutzer den Dialog ab, passiert nichts.
- Ist die Liste leer, ein Hinweistext statt der Liste.

### 3. Aufruf-Stellen

**`DetailPanel.tsx`** - Split-Button nach dem Vorbild des bestehenden
Import-Buttons in `Header.tsx` (Hauptbutton + schmaler Pfeil-Button
daneben, gleiches visuelles Muster):

- Hauptklick: startet direkt den Slicer mit `id === lastUsedId` (falls
  vorhanden), sonst - siehe "Kein Slicer konfiguriert" unten.
- Pfeil-Klick: öffnet ein Dropdown mit allen konfigurierten Slicern; Klick
  auf einen Eintrag startet ihn und setzt `lastUsedId` neu.
- Der Pfeil-Button wird nur angezeigt, wenn mindestens ein Slicer
  konfiguriert ist (bei leerer Liste nur der einfache Hauptbutton, der ins
  Einstellungsmenü führt).

**`ContextMenu.tsx`** - einfacher Menüeintrag ohne Untermenü, startet
immer den zuletzt genutzten Slicer (gleiche Logik wie der Hauptklick im
DetailPanel). Für die Auswahl eines anderen Slicers nutzt man das
DetailPanel.

**Kein Slicer konfiguriert:** Klick auf "In Slicer öffnen" (egal an
welcher Stelle) öffnet stattdessen das Einstellungsmenü (⚙), damit der
Nutzer direkt einen Slicer hinzufügen kann, statt nur eine
Fehlermeldung zu sehen.

### 4. Backend-Commands (`src-tauri/src/commands.rs`)

Zwei neue Commands, beide nach dem Muster der bestehenden
`import_files`/`import_folder` (die den nativen Dialog ebenfalls im
Backend über `app.dialog()` öffnen):

```rust
#[tauri::command]
pub fn pick_slicer_executable(app: tauri::AppHandle) -> CmdResult<Option<String>> {
    let mut dialog = app.dialog().file();
    #[cfg(target_os = "windows")]
    {
        dialog = dialog.add_filter("Programme", &["exe"]);
    }
    let picked = dialog.blocking_pick_file();
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn open_in_slicer(slicer_path: String, file_path: String) -> CmdResult<()> {
    std::process::Command::new(&slicer_path)
        .arg(&file_path)
        .spawn()
        .map_err(|e| format!("Slicer konnte nicht gestartet werden: {e}"))?;
    Ok(())
}
```

Keine neue Cargo-Abhängigkeit nötig (`std::process::Command` reicht für
Windows/Linux, `tauri_plugin_dialog` ist bereits vorhanden). Beide Commands
in `lib.rs`s `tauri::generate_handler![...]`-Liste ergänzen.

Der Dateipfad kommt aus dem bereits im Frontend vorhandenen
`ModelFile.path` (kein neuer Datenbank-Zugriff nötig - `fileId` muss
nicht extra aufgelöst werden, das Frontend kennt den Pfad des
ausgewählten Modells bereits).

## Fehlerbehandlung

- `spawn()` schlägt fehl (Pfad existiert nicht mehr, keine
  Ausführungsrechte, o. ä.) → Command gibt `Err(String)` zurück, Frontend
  zeigt eine Fehlermeldung (gleiches Muster wie der bestehende
  `cloudError`-State in `App.tsx`) statt stillschweigend nichts zu tun.
- Der gestartete Slicer-Prozess wird nicht überwacht (kein Warten auf
  Beendigung, kein Erfassen von stdout/stderr) - die App startet ihn nur
  und läuft unabhängig weiter, wie ein normaler "Öffnen mit..."-Aufruf.

## Out of Scope

- macOS-Unterstützung (siehe oben).
- Automatische Erkennung installierter Slicer (der Nutzer wählt den Pfad
  immer manuell).
- Übergabe zusätzlicher Slicer-spezifischer Kommandozeilen-Optionen.
