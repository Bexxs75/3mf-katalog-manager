# Bevorzugte Ansicht (Bild vs. gerenderte 3D-Ansicht) — Design

## Kontext

Jede Datei kann bis zu drei Bildquellen haben, bereits heute getrennt in
der DB gespeichert (`custom_image_png`, `thumbnail_png`,
`render_snapshot_png` in `src-tauri/src/db/schema.sql`):

- **`custom_image_png`**: vom Nutzer manuell hochgeladenes Bild
  (`upload_custom_image`-Command, „Bild hochladen"-Button).
- **`thumbnail_png`**: in der `.3mf`/`.stl`-Datei eingebettetes
  Vorschaubild.
- **`render_snapshot_png`**: ein einmal aufgenommener Schnappschuss der
  3D-Live-Ansicht (`ModelViewer`), aktuell nur ausgelöst, wenn die
  Detailseite einer Datei ohne jedes vorhandene Bild geöffnet wird.

Aktuell fasst `resolve_display_image` (`src-tauri/src/commands.rs`) diese
drei serverseitig zu einem einzigen `displayImage`-Feld zusammen (Priorität
custom > thumbnail > snapshot), bevor die Daten ans Frontend gehen. Die
Modell-Detailseite (`ModelDetailPage.tsx`) hat bereits einen manuellen
Umschalter „3D-Ansicht"/„Bild", der aber bei jedem Öffnen fest mit
„3D-Ansicht" startet, unabhängig davon was der Nutzer zuletzt bevorzugt
hat.

## Ziel

Eine neue, global gültige Einstellung „Bevorzugte Ansicht" (Werte:
„Eingebettetes Bild" oder „Gerenderte Ansicht") legt fest,

1. mit welchem Tab die Modell-Detailseite standardmäßig startet, und
2. welche Bildquelle die Katalog-Karten (Grid/Liste) bevorzugt anzeigen.

Ist „Gerenderte Ansicht" aktiv, rendert die App fehlende Schnappschüsse
zusätzlich automatisch im Hintergrund nach — nicht erst beim manuellen
Öffnen einer Detailseite.

## Datenmodell (Backend)

`ModelFileDto` (`src-tauri/src/commands.rs`) verliert das zusammengeführte
`display_image`-Feld und bekommt stattdessen drei getrennte Felder:

```rust
#[serde(rename_all = "camelCase")]
pub struct ModelFileDto {
    // ... bestehende Felder unveraendert ...
    pub custom_image: Option<String>,
    pub thumbnail_image: Option<String>,
    pub render_snapshot_image: Option<String>,
}
```

`resolve_display_image` wird durch eine einfache, wiederverwendbare
Hilfsfunktion ersetzt, die genau EIN Bild kodiert (statt drei zu
priorisieren):

```rust
pub(crate) fn encode_image(bytes: Option<Vec<u8>>) -> Option<String> {
    use base64::Engine;
    bytes.map(|b| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b)
        )
    })
}
```

In `to_dto`:

```rust
custom_image: encode_image(file.custom_image_png),
thumbnail_image: encode_image(file.thumbnail_png),
render_snapshot_image: encode_image(file.render_snapshot_png),
```

`upload_custom_image` (Command) gibt weiterhin `CmdResult<Option<String>>`
zurück, jetzt aber direkt `encode_image(...)` des frisch hochgeladenen
Custom-Bildes (unverändertes Verhalten für den Aufrufer — das Frontend
setzt das Ergebnis künftig auf `customImage` statt auf `displayImage`).

Die drei bestehenden Rust-Tests, die `resolve_display_image` direkt
testen (`resolve_display_image_falls_back_to_embedded_thumbnail` etc.),
werden durch einfachere Tests für `encode_image` ersetzt (nur noch
Kodierung, keine Priorisierungslogik mehr nötig).

## Datenmodell (Frontend)

`ModelFile` (`src/types/index.ts`) spiegelt die Backend-Änderung:

```ts
export interface ModelFile {
  // ... bestehende Felder unveraendert ...
  customImage: string | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
}
```

Neue zentrale Auflösungsfunktion `src/lib/resolveDisplayImage.ts`:

```ts
import type { DisplayPreference } from '../hooks/useDisplayPreference';

export function resolveDisplayImage(
  model: { customImage: string | null; thumbnailImage: string | null; renderSnapshotImage: string | null },
  preference: DisplayPreference,
): string | null {
  if (model.customImage) return model.customImage;
  return preference === 'render'
    ? model.renderSnapshotImage ?? model.thumbnailImage
    : model.thumbnailImage ?? model.renderSnapshotImage;
}
```

Ein manuell hochgeladenes Bild (`customImage`) gewinnt immer, unabhängig
von der Einstellung — es ist eine bewusste Nutzerentscheidung, keine
automatisch ermittelte Quelle.

## Einstellung

Neuer Hook `src/hooks/useDisplayPreference.ts`, exakt nach dem Muster von
`useTheme.ts` (localStorage, kein Backend-Sync):

```ts
export type DisplayPreference = 'thumbnail' | 'render';
```

Default: `'thumbnail'` — entspricht dem heutigen Verhalten (eingebettetes
Bild/Custom-Bild hat schon immer Vorrang vor dem Rendern), damit
Bestandsnutzer keine Verhaltensänderung ohne aktives Zutun erleben.

UI in `Header.tsx`, im Einstellungen-Panel direkt nach dem bestehenden
„UI-Dichte"-Abschnitt (gleiches Segment-Button-Muster wie Theme/Dichte,
inkl. Beschreibungstext darunter):

```tsx
<div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('displayPreferenceTitle')}</div>
<div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
  {(['thumbnail', 'render'] as DisplayPreference[]).map((opt) => (
    <button
      key={opt}
      onClick={() => onDisplayPreferenceChange(opt)}
      className={`${segBase} flex-1 ${displayPreference === opt ? segActive : segInactive}`}
    >
      {opt === 'thumbnail' ? t('displayPreferenceThumbnail') : t('displayPreferenceRender')}
    </button>
  ))}
</div>
<div className="mt-2 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">
  {displayPreference === 'thumbnail' ? t('displayPreferenceDescriptionThumbnail') : t('displayPreferenceDescriptionRender')}
</div>
```

Neue i18n-Schlüssel (`types.ts` + alle 4 Sprachdateien):
`displayPreferenceTitle`, `displayPreferenceThumbnail`,
`displayPreferenceRender`, `displayPreferenceDescriptionThumbnail`,
`displayPreferenceDescriptionRender`.

## Katalog-Karten

`ModelGrid.tsx` (beide Karten-Varianten) und ggf. `ModelList.tsx` (falls
dort ein Bild gezeigt wird) nutzen `resolveDisplayImage(m, displayPreference)`
statt direkt `m.displayImage`. Ist das Ergebnis `null`, bleibt der
bestehende Platzhalter (schraffierter Hintergrund + „3D-Vorschau"-Label)
unverändert bestehen.

## Modell-Detailseite

`ModelDetailPage.tsx`: der Umschalter-Zustand (aktuell `showCustomImage`)
startet abhängig von der Einstellung und davon, ob überhaupt ein Bild
existiert:

```ts
const resolvedImage = resolveDisplayImage(model, displayPreference);
const [showCustomImage, setShowCustomImage] = useState(
  () => displayPreference === 'thumbnail' && resolvedImage !== null,
);
```

Die `<img>`-Quelle im Umschalter wird zu `resolvedImage` statt
`model.displayImage`. Die Toggle-Buttons erscheinen weiterhin nur, wenn
`resolvedImage` nicht `null` ist. `needsSnapshot` an `ModelViewer` wird zu
`model.renderSnapshotImage === null` (statt `model.displayImage === null`)
— dadurch wird beim Öffnen der 3D-Ansicht künftig auch dann ein
Schnappschuss aufgenommen, wenn bereits ein eingebettetes Bild existiert,
aber noch kein Snapshot vorliegt (ergänzt das automatische Nachrendern
unten für den Fall des manuellen Öffnens).

`DetailPanel.tsx` (kleines Seitenpanel bei Einzelauswahl, zwei
`ModelViewer`-Einbindungen): dieselbe Änderung, `needsSnapshot` von
`model.displayImage === null` zu `model.renderSnapshotImage === null`.

## Automatisches Nachrendern

Solange `displayPreference === 'render'`, hält die App eine unsichtbare
Hintergrund-Instanz von `ModelViewer` aktiv, die Dateien ohne
`renderSnapshotImage` **sequenziell** (eine nach der anderen, nie
parallel) nachrendert — unabhängig davon, welche Ansicht (Katalog,
Filament, Papierkorb) gerade aktiv ist.

Neue Komponente `src/components/BackgroundSnapshotRenderer.tsx`:

```tsx
import { ModelViewer } from './ModelViewer';

interface Props {
  fileId: string;
  onSnapshotCaptured: (base64: string) => void;
}

/**
 * Rendert unsichtbar (echte Groesse, aber ausserhalb des sichtbaren
 * Bereichs positioniert und transparent) im Hintergrund einen
 * Schnappschuss fuer eine einzelne Datei nach. ModelViewer braucht eine
 * reale Container-Groesse (ResizeObserver-basiert) - display:none wuerde
 * dazu fuehren, dass nie gerendert wird, daher opacity+position statt
 * display.
 */
export function BackgroundSnapshotRenderer({ fileId, onSnapshotCaptured }: Props) {
  return (
    <div
      style={{ position: 'fixed', top: -9999, left: -9999, width: 400, height: 300, opacity: 0, pointerEvents: 'none' }}
      aria-hidden="true"
    >
      <ModelViewer fileId={fileId} needsSnapshot onSnapshotCaptured={onSnapshotCaptured} />
    </div>
  );
}
```

In `App.tsx`, immer gemountet (unabhängig von `mainView`), gesteuert über
eine abgeleitete Warteschlange:

```ts
const pendingSnapshotIds = useMemo(
  () => models.filter((m) => m.renderSnapshotImage === null).map((m) => m.id),
  [models],
);
```

```tsx
{displayPreference === 'render' && pendingSnapshotIds.length > 0 && (
  <BackgroundSnapshotRenderer
    key={pendingSnapshotIds[0]}
    fileId={pendingSnapshotIds[0]}
    onSnapshotCaptured={(base64) => captureRenderSnapshot(pendingSnapshotIds[0], base64)}
  />
)}
```

Der `key={pendingSnapshotIds[0]}` sorgt dafür, dass React bei jedem
Fortschritt in der Warteschlange sauber den internen State der Komponente
zurücksetzt (`status` in `ModelViewer`), auch wenn `fileId` sich ändert -
`ModelViewer` selbst behandelt File-Wechsel zwar bereits über einen
zweiten `useEffect` mit `fileId` als Dependency, ein frischer `key` macht
das Verhalten aber unabhängig von Timing-Details in `ModelViewer` robust.

`captureRenderSnapshot` in `App.tsx` wird angepasst: setzt künftig
`renderSnapshotImage` statt `displayImage`, ohne die bisherige
`m.displayImage === null`-Bedingung (die Bedingung ergab nur Sinn, als
`displayImage` ein einzelnes zusammengeführtes Feld war; jetzt wird
`renderSnapshotImage` immer gesetzt, wenn ein Snapshot eintrifft):

```ts
const captureRenderSnapshot = (id: string, base64: string) => {
  const renderSnapshotImage = `data:image/png;base64,${base64}`;
  setModels((prev) => prev.map((m) => (m.id === id ? { ...m, renderSnapshotImage } : m)));
  invoke('set_render_snapshot', { fileId: id, imageBase64: base64 }).catch((e) => {
    console.error('[render-snapshot] Speichern fehlgeschlagen:', e);
  });
};
```

`uploadCustomImage` in `App.tsx` setzt entsprechend `customImage` statt
`displayImage`.

## Fehlerbehandlung

- Backend: `encode_image` kann nicht fehlschlagen (reine Kodierung).
- Frontend: schlägt `set_render_snapshot` beim Hintergrund-Nachrendern
  fehl (z. B. Datei zwischenzeitlich gelöscht), wird der Fehler wie
  bisher nur geloggt (`console.error`); die Warteschlange verkleinert
  sich in diesem Fall nicht automatisch für diese Datei — sie bleibt
  ohne Snapshot und wird beim nächsten `models`-Update (z. B. nach einem
  Neustart, wo die Datei ggf. nicht mehr existiert und aus der Liste
  verschwindet) korrekt nicht mehr Teil der Warteschlange. Kein
  Endlosschleifen-Risiko, da `pendingSnapshotIds` sich nur ändert, wenn
  sich `models` ändert - ein dauerhaft fehlschlagender Snapshot blockiert
  höchstens den Fortschritt für DIESE eine Datei, nicht für die übrigen
  (da beim naechsten `models`-Update aus anderem Grund, z.B. Tag-Aenderung
  an anderer Datei, `pendingSnapshotIds[0]` weiterhin dieselbe bleibt,
  bis ihr Snapshot tatsaechlich gesetzt wird oder die Datei aus der Liste
  verschwindet - das ist gewolltes Verhalten, kein Bug: ein dauerhaft
  fehlschlagender Snapshot soll nicht stillschweigend uebersprungen
  werden, sondern sichtbar "haengen" bleiben, was sich im vorhandenen
  Konsolen-Log zeigt).

## Testing

- Rust: `encode_image`-Tests (Some/None-Fall) ersetzen die alten
  `resolve_display_image`-Tests.
- Frontend: `resolveDisplayImage` ist eine reine Funktion — auch ohne
  Testframework im Projekt keine Notwendigkeit für neue Test-Infrastruktur,
  Verifikation über `npx tsc --noEmit` + `npm run build` + manuellen
  Smoke-Test (etabliertes Projektmuster). Manueller Smoke-Test deckt
  gezielt ab: Einstellung umschalten → Karten wechseln automatisch die
  Bildquelle; Einstellung auf „Gerenderte Ansicht" → Dateien ohne
  Snapshot bekommen im Hintergrund nacheinander einen erzeugt (beobachtbar
  z. B. an kurzzeitig erhöhter GPU-Last / an Karten, die sich nacheinander
  von Platzhalter zu Bild ändern); Detailseite startet mit dem laut
  Einstellung erwarteten Tab.

## Out of Scope

- Kein Backend-Sync der Einstellung (rein lokal wie Theme/Dichte/Slicer).
- Keine Fortschrittsanzeige/kein UI-Indikator für die laufende
  Hintergrund-Warteschlange (läuft still, wie das bisherige einmalige
  Snapshot-Verhalten beim Öffnen einer Detailseite auch schon).
- Keine Möglichkeit, das automatische Nachrendern pro Datei einzeln
  abzulehnen/zu überspringen.
- macOS/Windows-spezifisches WebGL-Verhalten wird nicht gesondert
  behandelt (nutzt exakt denselben `ModelViewer`, der bereits
  plattformübergreifend im Einsatz ist).
