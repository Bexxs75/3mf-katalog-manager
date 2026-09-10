# Filament-Katalog (Spulenverwaltung) — Design

## Ausgangslage

3mf-katalog-manager verwaltet bisher nur 3D-Modelldateien. Beim Import
wird pro Modell zwar bereits ein Material-Feld aus der 3MF-Datei
geparst (`file_materials`-Tabelle, `display_color` optional), aber es
gibt keine eigene Verwaltung der tatsächlich vorhandenen
Filament-**Spulen** des Nutzers (Bestand, Restgewicht, Preis, ...).

Angeregt durch das "Filament-Lager"-Feature einer Konkurrenz-App
(Screenshot vom Nutzer geteilt), aber in dieser Phase deutlich
reduzierter Umfang: **reine Spulenverwaltung**, kein Verbrauchstracking.

## Ziel

Der Nutzer kann seine Filamentspulen als eigenständige Liste erfassen,
bearbeiten und löschen - unabhängig vom Modell-Katalog. Kein Bezug zu
Modellen/Druckvorgängen in dieser Phase.

**Explizit für später zurückgestellt (nicht Teil dieses Specs):**
Verbrauchstracking pro Druck (welches Modell hat wie viel Gramm welcher
Spule verbraucht, automatisches Runterzählen des Restgewichts). Das
würde eine Verknüpfung zwischen einem neuen "Druck"-Ereignis, einem
Modell (`files`) und einer Spule (`filament_spools`) erfordern - bewusst
nicht jetzt, um diesen Schritt klein zu halten. Die Datenbank-Struktur
unten (separate Tabelle mit eigenem Primärschlüssel) steht einer
späteren Erweiterung um eine Verknüpfungstabelle nicht im Weg.

## Architektur

### 1. Datenmodell (`src-tauri/src/db/schema.sql`)

```sql
CREATE TABLE IF NOT EXISTS filament_spools (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    material TEXT NOT NULL,
    manufacturer TEXT,
    color TEXT,
    diameter_mm REAL NOT NULL,
    original_weight_g INTEGER NOT NULL,
    remaining_weight_g INTEGER NOT NULL,
    price REAL,
    created_at TEXT NOT NULL
);
```

- **Pflicht:** `material`, `diameter_mm`, `original_weight_g`,
  `remaining_weight_g` - die Kernfelder, ohne die ein Eintrag keinen
  Sinn ergibt.
- **Optional (nullable):** `manufacturer`, `color`, `price` - nicht
  jeder Nutzer kennt/trackt das bei jeder Spule.
- `color` ist reiner Freitext (kein Hex-Farbwert, kein Farb-Swatch im
  UI - anders als `file_materials.display_color` bei Modellen).
- Keine Fremdschlüssel zu `files` - bewusst unabhängig (siehe "Ziel").

### 2. Backend (`src-tauri/src/db/models.rs`, `repository.rs`)

Neue Structs, nach dem Vorbild von `FolderRecord`/`NewFile`:

```rust
#[derive(Debug, Clone)]
pub struct FilamentSpoolRecord {
    pub id: i64,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
    pub created_at: String,
}

pub struct NewFilamentSpool {
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
}
```

Neue Repository-Funktionen in `repository.rs`, Standard-CRUD analog zu
den bestehenden Funktionen (`insert_folder`, `list_folders`, ...):

```rust
pub fn insert_filament_spool(conn: &Connection, spool: &NewFilamentSpool) -> Result<i64, DbError>;
pub fn list_filament_spools(conn: &Connection) -> Result<Vec<FilamentSpoolRecord>, DbError>;
pub fn update_filament_spool(conn: &Connection, id: i64, spool: &NewFilamentSpool) -> Result<(), DbError>;
pub fn delete_filament_spool(conn: &Connection, id: i64) -> Result<(), DbError>;
```

`update_filament_spool` überschreibt alle Felder in einem UPDATE
(kein partielles Patchen nötig - das Frontend-Formular schickt immer
den kompletten Datensatz, siehe Abschnitt 4).

### 3. Tauri-Commands (`src-tauri/src/commands.rs`)

Kein eigenes Submodul (anders als `cloud/`) - dafür ist der Umfang zu
klein, landet direkt neben `list_folders`/`add_tag` etc.:

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilamentSpoolDto {
    pub id: String,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
}

#[tauri::command]
pub fn list_filament_spools(state: State<AppState>) -> CmdResult<Vec<FilamentSpoolDto>>;

#[tauri::command]
pub fn add_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto>;

#[tauri::command]
pub fn update_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto>;

#[tauri::command]
pub fn delete_filament_spool(state: State<AppState>, spool_id: String) -> CmdResult<()>;
```

`id` in `FilamentSpoolDto` ist bei `add_filament_spool` ein Platzhalter
(leerer String, wird ignoriert) - gleiches Muster wie beim Erzeugen
neuer Modelle, wo die DB die ID vergibt und der Rückgabewert die
vollständige DTO mit echter ID liefert.

Alle vier Commands in `lib.rs`s `tauri::generate_handler![...]`
ergänzen.

### 4. Frontend

**Neuer Typ** `src/types/index.ts`:

```ts
export interface FilamentSpool {
  id: string;
  material: string;
  manufacturer: string | null;
  color: string | null;
  diameterMm: number;
  originalWeightG: number;
  remainingWeightG: number;
  price: number | null;
}
```

**Neue Komponente** `src/components/FilamentDialog.tsx` (modales
Overlay, gleiches Grundmuster wie die bisherigen `*Dialog.tsx`-
Komponenten: `fixed inset-0 z-50 grid place-items-center bg-black/50`):

- Liste vorhandener Spulen als Zeilen/Karten: Material, Hersteller,
  Farbe, Restgewicht von Ursprungsgewicht (z. B. "620 g / 1000 g"),
  Durchmesser, Preis - je Zeile ein Bearbeiten-Icon (✎) und ein
  Löschen-Icon (✕, mit Bestätigung analog zum bestehenden
  Modell-Löschen-Muster in `DetailPanel.tsx`).
- Ein Formular (Material, Hersteller, Farbe, Durchmesser,
  Ursprungsgewicht, Restgewicht, Preis) zum Hinzufügen. Klick auf das
  Bearbeiten-Icon einer Zeile füllt **dasselbe Formular** mit den
  Werten dieser Spule vorbefüllt (kein zweiter UI-Pfad für Bearbeiten
  vs. Hinzufügen) - Absenden ruft `update_filament_spool` statt
  `add_filament_spool`, erkennbar an einer lokalen
  `editingId`-Zustandsvariable.
- Leerer Zustand (keine Spulen vorhanden): Hinweistext statt Liste,
  gleiches Muster wie `noSlicersConfigured` im Einstellungsmenü.
- Lädt beim Öffnen einmal `list_filament_spools`, hält die Liste
  danach im lokalen State der Dialog-Komponente (kein globaler State in
  `App.tsx` nötig, da nichts außerhalb des Dialogs auf Spulen
  zugreift).

**Header-Icon** (`src/components/Header.tsx`): neuer Button neben dem
bestehenden ⚙-Button, gleiches visuelles Muster (`w-8 h-8`,
`rounded-[3px]`, `border`), Symbol **⊙** (Spule von der Seite,
passt zum bestehenden Font-Mono-Icon-Stil wie ✕/▾/↑/↻ - kein Emoji).
Klick togglet einen neuen `filamentDialogOpen`-State in `App.tsx`,
analog zum bisherigen `cloudBrowserOpen`-Muster.

**i18n:** neue Keys in allen vier Sprachdateien - Dialog-Titel,
Feldlabels (Material/Hersteller/Farbe/Durchmesser/
Ursprungsgewicht/Restgewicht/Preis), Buttons (Hinzufügen/Speichern/
Abbrechen/Löschen), Leerzustand-Text, Lösch-Bestätigungsfrage
(kann den bereits vorhandenen generischen `deleteConfirmQuestion`-Key
wiederverwenden, kein neuer Text nötig).

## Fehlerbehandlung

- CRUD-Commands geben bei DB-Fehlern `Err(String)` zurück (gleiches
  Muster wie alle bestehenden Commands), Frontend zeigt die
  Fehlermeldung als kurzen Text im Dialog (gleiches Muster wie
  `cloudUploadError` in `DetailPanel.tsx`).
- Keine Validierung der Zahlenfelder über das native HTML
  `type="number"`-Verhalten hinaus (z. B. kein Check
  "Restgewicht ≤ Ursprungsgewicht") - bewusst einfach gehalten für
  diese erste Ausbaustufe.

## Testing

- Rust: Insert/List/Update/Delete-Roundtrip-Tests in `db/mod.rs`,
  gleiches Muster wie die bestehenden Datei-/Ordner-Tests
  (`connect_in_memory()`, dann CRUD-Sequenz prüfen).
- Frontend: `tsc --noEmit` sauber. Live-Verifikation im laufenden
  `npm run tauri dev` (Dialog öffnen, Spule anlegen/bearbeiten/löschen)
  vor Abschluss der Implementierung.

## Out of Scope

- Verbrauchstracking pro Druck (siehe "Ziel" oben) - eigener,
  späterer Schritt.
- Verknüpfung/Abgleich mit dem beim Modell-Import bereits geparsten
  Material-Feld (z. B. "passende Spule vorschlagen") - erst relevant,
  sobald Verbrauchstracking existiert.
- Farbwert/Swatch-Darstellung für Spulen (nur Freitext, siehe oben).
- Sortier-/Filterfunktionen innerhalb des Dialogs (bei der erwartbaren
  kleinen Spulenanzahl nicht nötig, YAGNI).
- Mindestbestand-Warnung ("Spule wird knapp").
