# Filament-Lager: Hauptansicht + Bild-Upload — Design

## Ausgangslage

Das Filament-Lager (Spec: `docs/superpowers/specs/2026-09-10-filament-catalog-design.md`,
Plan: `docs/superpowers/plans/2026-09-10-filament-catalog-plan.md`) ist bereits
gebaut und live getestet: SQLite-Tabelle `filament_spools`, vier
CRUD-Tauri-Commands, ein modaler Dialog `FilamentDialog.tsx` mit
Karten-Raster-Optik (analog zum Modell-Grid, aber Material-Name statt
3D-Vorschau als Platzhalter), erreichbar über ein ⊙-Icon im Header.

Nutzerfeedback nach dem ersten Live-Test (zwei Screenshots geteilt):

1. Der Header-Button soll ausgeschrieben sein ("Filament"), nicht nur ein Icon.
2. Ein Popup reicht "irgendwann nicht mehr" - das Filament-Lager soll wie die
   Modell-Rasteransicht eine vollwertige Hauptansicht im Fenster sein, kein
   Overlay.
3. Statt des Material-Name-Platzhalters soll der Nutzer pro Spule ein
   eigenes Foto hochladen können, um die Spule visuell zu identifizieren.

## Ziel

Das Filament-Lager wird zu einem zweiten Hauptansicht-Modus der App
(gleichberechtigt neben dem Modell-Katalog, per Header-Umschalter
erreichbar) statt eines Popups, und jede Spule kann ein eigenes,
selbst hochgeladenes Bild bekommen.

## Architektur

### 1. Datenmodell-Erweiterung

Neue Spalte an der bestehenden Tabelle (`schema.sql`):

```sql
ALTER TABLE filament_spools ADD COLUMN image_png BLOB;
```

Analog zu `files.thumbnail_png` - Bilddaten direkt in SQLite, kein
Datei-Pfad, kein Aufräumen verwaister Dateien nötig. Bewusst keine
Größenbeschränkung/Kompression/Skalierung in dieser Ausbaustufe; kein
"Bild entfernen" (ein neuer Upload ersetzt das alte Bild).

**Backend (`db/models.rs`, `repository.rs`):** `FilamentSpoolRecord` und
`NewFilamentSpool` bekommen ein neues Feld `pub image_png: Option<Vec<u8>>`.
`insert_filament_spool`/`update_filament_spool`/`list_filament_spools`
werden um die neue Spalte erweitert (gleiches Update-Muster wie die
bestehenden Felder).

**Neuer Tauri-Command** (`commands.rs`), nach dem Muster von
`pick_slicer_executable`:

```rust
#[tauri::command]
pub async fn pick_and_read_image(app: tauri::AppHandle) -> CmdResult<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Bilder", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_file();

    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(Some(base64::engine::general_purpose::STANDARD.encode(bytes)))
}
```

Gibt das Bild direkt Base64-kodiert zurück (kein zweiter Roundtrip nötig,
das Frontend erhält bereits einen fertigen `data:`-tauglichen String).
Neue direkte Cargo-Abhängigkeit `base64 = "0.22"` (aktuell nur transitiv
vorhanden, gleiche Version wie bereits im Lockfile aufgelöst - kein
Versionskonflikt).

`FilamentSpoolDto` bekommt ein neues Feld `pub image_png: Option<String>`
(Base64), das 1:1 zwischen `Option<Vec<u8>>` (DB) und Base64-String (DTO)
konvertiert wird (`base64::engine::general_purpose::STANDARD.encode`/`.decode`).

### 2. Navigation/Layout-Umbau

**`App.tsx`:** neuer State `const [mainView, setMainView] = useState<'catalog' | 'filament'>('catalog');`.
Wenn `mainView === 'filament'`:

- `<Sidebar />` wird nicht gerendert.
- `<DetailPanel />` wird nicht gerendert.
- Der Ordner-Label/Tag-Leiste-Block unter dem Header (`<div className="flex-none h-[38px] ...">`) wird nicht gerendert.
- Der mittlere Bereich (`<ModelGrid />`/`<ModelList />`) wird durch `<FilamentView />` ersetzt, volle Breite.

**`Header.tsx`:** neue Props `mainView: 'catalog' | 'filament'` und
`onMainViewChange: (view: 'catalog' | 'filament') => void`. Wenn
`mainView === 'filament'`: Importieren-Button, Sortieren-Dropdown,
Raster/Liste-Umschalter und Dateianzahl werden nicht gerendert. Ein neuer
Button (ausgeschriebener Text, gleiches visuelles Muster wie der
⚙-Button, aber mit Text statt Icon: `h-8 px-3 ...`) zeigt je nach
aktuellem `mainView` "Filament" (im Katalog-Modus) bzw. "Katalog" (im
Filament-Modus) und ruft beim Klick `onMainViewChange` mit dem jeweils
anderen Wert auf - ein einzelner Umschalt-Button, keine zwei getrennten.

### 3. `FilamentView.tsx` (ersetzt `FilamentDialog.tsx`)

Umbenennung + Entfernen des Modal-Wrappers (`fixed inset-0 z-50 grid
place-items-center bg-black/50` sowie das feste `w-[560px] max-h-[680px]`
Panel) - wird direkt in `App.tsx`s Hauptbereich gerendert
(`flex-1 min-w-0 flex flex-col min-h-0`, analog zum bisherigen `<main>`).
Kein `onClose`-Prop mehr nötig (kein Schließen-Kreuz, der Header-Umschalter
übernimmt das Verlassen der Ansicht).

- **Karten-Raster:** Spaltenzahl wechselt von der festen 2-Spalten-Regel
  zurück auf das responsive Muster des Modell-Grids
  (`grid gap-3.5` mit `style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(178px, 1fr))' }}`),
  da jetzt die volle Fensterbreite zur Verfügung steht statt der
  480-560px-Popup-Breite.
- **Karten-Vorschaufläche:** zeigt `<img src={`data:image/png;base64,${spool.imagePng}`} className="w-full h-full object-cover" />`,
  wenn `spool.imagePng` gesetzt ist; sonst der bisherige Platzhalter
  (schraffierter Hintergrund + Material-Name zentriert).
- **Formular:** neuer Button "Bild hochladen" (`t('filamentUploadImageLabel')`)
  ruft `invoke<string | null>('pick_and_read_image')` auf, speichert das
  Ergebnis in einem neuen `imagePng`-Feld des lokalen `FormState`; zeigt
  bei vorhandenem Wert eine kleine Vorschau (z. B. `w-12 h-12` Thumbnail)
  neben dem Button. `submitForm` nimmt `imagePng` mit in die
  `FilamentSpool`-Payload auf. `startEdit` befüllt `imagePng` aus der
  bestehenden Spule (für die Vorschau beim Bearbeiten).
- Rest der Logik (Add/Edit/Delete, `editingId`, `confirmDeleteId`, ein
  Formular für Hinzufügen+Bearbeiten) bleibt unverändert.

### 4. i18n

Neue Schlüssel: `filamentNavButton` ("Filament"), `filamentBackToCatalogButton`
("Katalog"), `filamentUploadImageLabel` ("Bild hochladen"). Bestehende
Schlüssel wie `filamentDialogTitle` bleiben als Überschrift der
`FilamentView` erhalten (Text passt weiterhin, auch außerhalb eines
Dialogs).

## Fehlerbehandlung

- `pick_and_read_image` gibt bei Lesefehlern (Datei nicht lesbar o. ä.)
  `Err(String)` zurück - Frontend zeigt das über den bestehenden
  `error`-State/`filamentError`-Präfix an, gleiches Muster wie die
  anderen Filament-Commands.
- Abbruch des Datei-Dialogs (`None`) lässt das Formular unverändert
  (kein Fehler, kein Bild gesetzt/geändert).

## Testing

- Rust: `insert`/`update`/`list` decken das neue Feld über die
  bestehenden Roundtrip-Tests hinaus ab (ein Test mit gesetztem
  `image_png`, einer mit `None`). `pick_and_read_image` bekommt (wie
  `pick_slicer_executable`) keinen Test - echter blockierender
  OS-Dialog, nicht sinnvoll automatisiert testbar.
- Frontend: `tsc --noEmit` + abschließender manueller Live-Test
  (Umschalten Katalog↔Filament, Bild hochladen, Karte zeigt das Bild,
  Bearbeiten zeigt die vorhandene Vorschau).

## Out of Scope

- Bild-Kompression/-Skalierung/-Größenlimit.
- "Bild entfernen"-Funktion (nur Ersetzen durch neuen Upload).
- Persistenz des zuletzt aktiven `mainView` über Neustarts hinweg (startet
  immer im Katalog-Modus).
- Alles bereits im ursprünglichen Filament-Katalog-Spec als Out-of-Scope
  gelistete (Verbrauchstracking, Verknüpfung mit Modellen, Sortier-/
  Filterfunktionen, Mindestbestand-Warnung) gilt weiterhin.
