# Bevorzugte Ansicht Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine neue globale Einstellung „Bevorzugte Ansicht" legt fest, ob Katalog-Karten und die Modell-Detailseite standardmäßig das eingebettete Datei-Bild oder eine gerenderte 3D-Ansicht bevorzugen; bei „Gerenderte Ansicht" rendert die App fehlende Schnappschüsse zusätzlich automatisch im Hintergrund nach.

**Architecture:** Die bisher serverseitig zu einem Feld (`displayImage`) zusammengeführten drei Bildquellen (Custom-Bild, eingebettetes Thumbnail, Render-Schnappschuss) werden getrennt ans Frontend durchgereicht. Eine zentrale, reine Funktion `resolveDisplayImage` wählt im Frontend anhand der neuen Einstellung, welche Quelle angezeigt wird. Eine unsichtbar gemountete `ModelViewer`-Instanz rendert im Hintergrund sequenziell fehlende Schnappschüsse nach.

**Tech Stack:** Rust (Tauri Commands, serde), React/TypeScript, localStorage (bestehendes Muster aus `useTheme.ts`).

## Global Constraints

- Ein manuell hochgeladenes Bild (`customImage`) hat immer Vorrang vor der Einstellung — sie beeinflusst nur die Wahl zwischen eingebettetem Thumbnail und Render-Schnappschuss.
- Default der neuen Einstellung ist `'thumbnail'` (entspricht dem heutigen Verhalten) — Bestandsnutzer erleben ohne aktives Zutun keine Verhaltensänderung.
- Automatisches Nachrendern läuft **sequenziell** (eine Datei nach der anderen, nie parallel) und **unabhängig von der aktiven Ansicht** (Katalog/Filament/Papierkorb).
- Kein Backend-Sync der Einstellung — rein `localStorage`, gleiches Muster wie `useTheme.ts`/`useSlicers.ts`.
- Kein Test-Framework im Frontend vorhanden — Verifikation über `npx tsc --noEmit` + `npm run build` + manuellen Smoke-Test (etabliertes Projektmuster).

---

### Task 1: Backend-Datenmodell (Rust)

**Files:**
- Modify: `src-tauri/src/commands.rs` (`ModelFileDto`-Struct ~Zeile 26-51, `resolve_display_image`/`to_dto` ~Zeile 150-210, `upload_custom_image` ~Zeile 524-532, Tests ~Zeile 1442-1472)

**Interfaces:**
- Produces: `ModelFileDto` mit drei neuen Feldern `custom_image: Option<String>`, `thumbnail_image: Option<String>`, `render_snapshot_image: Option<String>` (camelCase im JSON: `customImage`, `thumbnailImage`, `renderSnapshotImage`) statt `display_image: Option<String>`. Neue Hilfsfunktion `pub(crate) fn encode_image(bytes: Option<Vec<u8>>) -> Option<String>`. Beide werden von Task 2 (Frontend-Typ) als Vorgabe für die exakten Feldnamen benötigt.

- [ ] **Step 1: `ModelFileDto` umbauen**

In `src-tauri/src/commands.rs`, im `ModelFileDto`-Struct, die Zeile

```rust
    pub display_image: Option<String>,
```

ersetzen durch:

```rust
    pub custom_image: Option<String>,
    pub thumbnail_image: Option<String>,
    pub render_snapshot_image: Option<String>,
```

- [ ] **Step 2: `resolve_display_image` durch `encode_image` ersetzen**

Den kompletten Block

```rust
pub(crate) fn resolve_display_image(
    custom_image_png: Option<Vec<u8>>,
    thumbnail_png: Option<Vec<u8>>,
    render_snapshot_png: Option<Vec<u8>>,
) -> Option<String> {
    use base64::Engine;
    custom_image_png
        .or(thumbnail_png)
        .or(render_snapshot_png)
        .map(|bytes| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        })
}
```

ersetzen durch:

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

- [ ] **Step 3: `to_dto` anpassen**

In `to_dto`, die Zeilen

```rust
    let display_image = resolve_display_image(
        file.custom_image_png,
        file.thumbnail_png,
        file.render_snapshot_png,
    );
```

ersetzen durch:

```rust
    let custom_image = encode_image(file.custom_image_png);
    let thumbnail_image = encode_image(file.thumbnail_png);
    let render_snapshot_image = encode_image(file.render_snapshot_png);
```

Und im `ModelFileDto { ... }`-Konstruktor am Ende von `to_dto`, die Zeile

```rust
        display_image,
```

ersetzen durch:

```rust
        custom_image,
        thumbnail_image,
        render_snapshot_image,
```

- [ ] **Step 4: `upload_custom_image` anpassen**

In `upload_custom_image`, den bestehenden Rückgabewert (bereits korrekt kodiertes Custom-Bild, unverändertes Verhalten) beibehalten - keine Code-Änderung nötig, da die Funktion bereits direkt

```rust
    Ok(Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    )))
```

zurückgibt (das war schon vorher identisch mit dem, was jetzt `customImage` heißt). Nur zur Klarheit: dieser Rückgabewert wird künftig vom Frontend (Task 4) auf `customImage` statt `displayImage` gemappt - hier in `commands.rs` ist keine Änderung nötig.

- [ ] **Step 5: Tests ersetzen**

Die vier Tests

```rust
    #[test]
    fn resolve_display_image_prefers_custom_over_embedded_over_snapshot() { ... }

    #[test]
    fn resolve_display_image_falls_back_to_embedded_thumbnail() { ... }

    #[test]
    fn resolve_display_image_falls_back_to_render_snapshot() { ... }

    #[test]
    fn resolve_display_image_returns_none_without_any_source() { ... }
```

komplett entfernen und durch diese zwei ersetzen:

```rust
    #[test]
    fn encode_image_encodes_bytes_as_data_url() {
        use base64::Engine;
        let result = encode_image(Some(vec![1, 2, 3])).expect("some image");
        let b64 = result.strip_prefix("data:image/png;base64,").expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).expect("valid base64");
        assert_eq!(decoded, vec![1, 2, 3]);
    }

    #[test]
    fn encode_image_returns_none_for_none() {
        assert_eq!(encode_image(None), None);
    }
```

- [ ] **Step 6: Build und Tests verifizieren**

Run: `cd src-tauri && cargo build --release`
Expected: baut ohne Fehler. Es entstehen Kompilierfehler an allen Stellen, die noch `display_image`/`resolve_display_image` referenzieren, außerhalb dieser Datei gibt es dazu keine (geprüft: `display_image` wird nur in `commands.rs` referenziert) - falls der Compiler doch weitere Stellen findet, diese ebenfalls auf die drei neuen Felder ummünzen.

Run: `cd src-tauri && cargo test --lib`
Expected: alle Tests bestehen, inklusive der beiden neuen `encode_image_*`-Tests.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat: ModelFileDto liefert customImage/thumbnailImage/renderSnapshotImage getrennt statt displayImage"
```

---

### Task 2: Frontend-Datenmodell (Typ, Resolver, Einstellungs-Hook, i18n)

**Files:**
- Modify: `src/types/index.ts` (`ModelFile`-Interface, Zeile ~24)
- Create: `src/lib/resolveDisplayImage.ts`
- Create: `src/hooks/useDisplayPreference.ts`
- Modify: `src/i18n/types.ts` (nach Zeile 36, `densityDescriptionComfort`)
- Modify: `src/i18n/de.ts`, `src/i18n/en.ts`, `src/i18n/es.ts`, `src/i18n/fr.ts` (jeweils nach Zeile 31, `densityDescriptionComfort`)

**Interfaces:**
- Consumes: Backend-Feldnamen `customImage`, `thumbnailImage`, `renderSnapshotImage` aus Task 1 (JSON-Deserialisierung ist implizit über die bestehende `invoke`-Typisierung - keine Laufzeit-Prüfung nötig, TypeScript-Typ muss nur exakt passen).
- Produces: `export type DisplayPreference = 'thumbnail' | 'render';` und `export function useDisplayPreference(): { preference: DisplayPreference; setPreference: (p: DisplayPreference) => void }`, `export function resolveDisplayImage(model: { customImage: string | null; thumbnailImage: string | null; renderSnapshotImage: string | null }, preference: DisplayPreference): string | null`. Beide werden von Task 3 und Task 4 importiert.

- [ ] **Step 1: `ModelFile`-Typ anpassen**

In `src/types/index.ts`, im `ModelFile`-Interface, die Zeile

```ts
  displayImage: string | null;
```

ersetzen durch:

```ts
  customImage: string | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
```

- [ ] **Step 2: `resolveDisplayImage.ts` erstellen**

Erstelle `src/lib/resolveDisplayImage.ts`:

```ts
import type { DisplayPreference } from '../hooks/useDisplayPreference';

interface ImageSources {
  customImage: string | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
}

/**
 * Waehlt aus den drei getrennt gespeicherten Bildquellen einer Datei die
 * anzuzeigende aus. Ein manuell hochgeladenes Bild (customImage) hat immer
 * Vorrang - es ist eine bewusste Nutzerentscheidung, keine automatisch
 * ermittelte Quelle. Danach entscheidet die globale "Bevorzugte Ansicht"-
 * Einstellung, welche der beiden automatischen Quellen zuerst versucht
 * wird; existiert diese nicht, faellt die Funktion auf die jeweils andere
 * zurueck, statt nichts anzuzeigen.
 */
export function resolveDisplayImage(model: ImageSources, preference: DisplayPreference): string | null {
  if (model.customImage) return model.customImage;
  return preference === 'render'
    ? model.renderSnapshotImage ?? model.thumbnailImage
    : model.thumbnailImage ?? model.renderSnapshotImage;
}
```

- [ ] **Step 3: `useDisplayPreference.ts` erstellen**

Erstelle `src/hooks/useDisplayPreference.ts`:

```ts
import { useCallback, useState } from 'react';

export type DisplayPreference = 'thumbnail' | 'render';

const STORAGE_KEY = '3mf-katalog-display-preference';

/**
 * Verwaltet die globale Einstellung, ob Katalog-Karten und die Modell-
 * Detailseite standardmaessig das eingebettete Datei-Bild ("thumbnail")
 * oder eine gerenderte 3D-Ansicht ("render") bevorzugen. Persistiert in
 * localStorage nach demselben Muster wie Theme/Dichte (useTheme.ts).
 * Default ist "thumbnail" - entspricht dem Verhalten vor Einfuehrung
 * dieser Einstellung, damit Bestandsnutzer keine Aenderung ohne aktives
 * Zutun erleben.
 */
export function useDisplayPreference() {
  const [preference, setPreferenceState] = useState<DisplayPreference>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === 'render' ? 'render' : 'thumbnail';
  });

  const setPreference = useCallback((next: DisplayPreference) => {
    setPreferenceState(next);
    localStorage.setItem(STORAGE_KEY, next);
  }, []);

  return { preference, setPreference };
}
```

- [ ] **Step 4: i18n-Schlüssel hinzufügen**

In `src/i18n/types.ts`, nach Zeile 36 (`densityDescriptionComfort: string;`), einfügen:

```ts
  displayPreferenceTitle: string;
  displayPreferenceThumbnail: string;
  displayPreferenceRender: string;
  displayPreferenceDescriptionThumbnail: string;
  displayPreferenceDescriptionRender: string;
```

In `src/i18n/de.ts`, nach Zeile 31 (`densityDescriptionComfort: 'Größere Schrift und Grafiken — besser lesbar.',`), einfügen:

```ts
  displayPreferenceTitle: 'Bevorzugte Ansicht',
  displayPreferenceThumbnail: 'Eingebettetes Bild',
  displayPreferenceRender: 'Gerenderte Ansicht',
  displayPreferenceDescriptionThumbnail: 'Zeigt bevorzugt das in der Datei hinterlegte Vorschaubild.',
  displayPreferenceDescriptionRender: 'Zeigt bevorzugt eine gerenderte 3D-Ansicht; fehlende Schnappschüsse werden automatisch im Hintergrund nachgerendert.',
```

In `src/i18n/en.ts`, an derselben Stelle nach `densityDescriptionComfort`, einfügen:

```ts
  displayPreferenceTitle: 'Preferred view',
  displayPreferenceThumbnail: 'Embedded image',
  displayPreferenceRender: 'Rendered view',
  displayPreferenceDescriptionThumbnail: 'Prefers the thumbnail image stored inside the file.',
  displayPreferenceDescriptionRender: 'Prefers a rendered 3D view; missing snapshots are rendered automatically in the background.',
```

In `src/i18n/es.ts`, an derselben Stelle, einfügen:

```ts
  displayPreferenceTitle: 'Vista preferida',
  displayPreferenceThumbnail: 'Imagen incrustada',
  displayPreferenceRender: 'Vista renderizada',
  displayPreferenceDescriptionThumbnail: 'Muestra preferentemente la imagen de vista previa guardada en el archivo.',
  displayPreferenceDescriptionRender: 'Muestra preferentemente una vista 3D renderizada; las capturas faltantes se generan automáticamente en segundo plano.',
```

In `src/i18n/fr.ts`, an derselben Stelle, einfügen:

```ts
  displayPreferenceTitle: 'Vue préférée',
  displayPreferenceThumbnail: 'Image intégrée',
  displayPreferenceRender: 'Vue rendue',
  displayPreferenceDescriptionThumbnail: "Affiche en priorité l'aperçu intégré au fichier.",
  displayPreferenceDescriptionRender: 'Affiche en priorité une vue 3D rendue ; les captures manquantes sont générées automatiquement en arrière-plan.',
```

- [ ] **Step 5: TypeScript verifizieren**

Run: `npx tsc --noEmit`
Expected: Fehler an allen Stellen, die noch `displayImage` referenzieren (`App.tsx`, `ModelGrid.tsx`, `ModelDetailPage.tsx`, `DetailPanel.tsx`) - das ist an dieser Stelle im Plan erwartet, diese werden in Task 3/4 behoben. Keine Fehler bezüglich der in diesem Task neu erstellten/geänderten Dateien selbst (`types/index.ts`, `resolveDisplayImage.ts`, `useDisplayPreference.ts`, `i18n/*`).

- [ ] **Step 6: Commit**

```bash
git add src/types/index.ts src/lib/resolveDisplayImage.ts src/hooks/useDisplayPreference.ts src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "feat: Frontend-Datenmodell fuer getrennte Bildquellen, useDisplayPreference-Hook, i18n"
```

---

### Task 3: Einstellungs-UI in Header.tsx + Verdrahtung in App.tsx

**Files:**
- Modify: `src/components/Header.tsx` (`Props`-Interface ~Zeile 10-31, Einstellungen-Panel ~Zeile 266-281)
- Modify: `src/App.tsx` (Imports ~Zeile 1-18, Hook-Aufrufe ~Zeile 25-30, `<Header>`-Aufruf ~Zeile 446-477)

**Interfaces:**
- Consumes: `useDisplayPreference` und `DisplayPreference` aus `src/hooks/useDisplayPreference.ts` (Task 2).
- Produces: `Header`-Komponente rendert den neuen Umschalter und ruft `onDisplayPreferenceChange` auf; `App.tsx` hält `displayPreference`/`setDisplayPreference` im State, verfügbar für Task 4 und Task 5 (dort als Prop an `ModelGrid`/`ModelDetailPage` weiterzureichen bzw. für die Hintergrund-Warteschlange zu nutzen).

- [ ] **Step 1: `Header.tsx` Props erweitern**

In `src/components/Header.tsx`, ergänze den Import (nach `import type { UiDensity } from '../hooks/UiDensityContext';`):

```ts
import type { DisplayPreference } from '../hooks/useDisplayPreference';
```

Im `Props`-Interface, nach den Zeilen

```ts
  uiDensity: UiDensity;
  onUiDensityChange: (d: UiDensity) => void;
```

einfügen:

```ts
  displayPreference: DisplayPreference;
  onDisplayPreferenceChange: (p: DisplayPreference) => void;
```

In der Funktionssignatur von `Header` (Destrukturierung der Props), ergänze `displayPreference` und `onDisplayPreferenceChange` an der Stelle, an der auch `uiDensity`/`onUiDensityChange` destrukturiert werden.

- [ ] **Step 2: Umschalter-UI einfügen**

In `src/components/Header.tsx`, nach dem bestehenden Block (Zeilen 266-280):

```tsx
            <div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('densityTitle')}</div>
            <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
              {(['compact', 'comfort'] as UiDensity[]).map((opt) => (
                <button
                  key={opt}
                  onClick={() => onUiDensityChange(opt)}
                  className={`${segBase} flex-1 ${uiDensity === opt ? segActive : segInactive}`}
                >
                  {opt === 'compact' ? t('densityCompact') : t('densityComfort')}
                </button>
              ))}
            </div>
            <div className="mt-2 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">
              {uiDensity === 'compact' ? t('densityDescriptionCompact') : t('densityDescriptionComfort')}
            </div>
```

und vor dem Block, der mit `{t('languageTitle')}` beginnt, einfügen:

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
              {displayPreference === 'thumbnail'
                ? t('displayPreferenceDescriptionThumbnail')
                : t('displayPreferenceDescriptionRender')}
            </div>
```

- [ ] **Step 3: `App.tsx` verdrahten**

In `src/App.tsx`, ergänze den Import (nach `import { useSlicers } from './hooks/useSlicers';`):

```ts
import { useDisplayPreference } from './hooks/useDisplayPreference';
```

Nach der Zeile

```ts
  const { slicers, lastUsedId, addSlicer, removeSlicer, setLastUsed, mergeDetected } = useSlicers();
```

einfügen:

```ts
  const { preference: displayPreference, setPreference: setDisplayPreference } = useDisplayPreference();
```

Im `<Header ... />`-Aufruf, nach den Zeilen

```tsx
        uiDensity={density}
        onUiDensityChange={setDensity}
```

einfügen:

```tsx
        displayPreference={displayPreference}
        onDisplayPreferenceChange={setDisplayPreference}
```

- [ ] **Step 4: TypeScript verifizieren**

Run: `npx tsc --noEmit`
Expected: dieselben, bereits aus Task 2 bekannten Fehler zu `displayImage` in `ModelGrid.tsx`/`ModelDetailPage.tsx`/`DetailPanel.tsx` (werden in Task 4 behoben) - aber KEINE neuen Fehler zu `Header.tsx`, `App.tsx`, `displayPreference` oder `DisplayPreference`.

- [ ] **Step 5: Commit**

```bash
git add src/components/Header.tsx src/App.tsx
git commit -m "feat: Einstellungs-UI fuer bevorzugte Ansicht in Header, State-Verdrahtung in App.tsx"
```

---

### Task 4: Konsumenten umstellen (ModelGrid, ModelDetailPage, DetailPanel, App.tsx-Handler)

**Files:**
- Modify: `src/components/ModelGrid.tsx` (`Props`-Interface, beide Karten-Varianten, Zeilen ~1-16, ~47-52, ~147-148)
- Modify: `src/components/ModelDetailPage.tsx` (`Props`-Interface, `showCustomImage`-State, Bild-Rendering, Zeilen ~7-45, ~78-95)
- Modify: `src/components/DetailPanel.tsx` (zwei `needsSnapshot`-Stellen, Zeilen ~167, ~408)
- Modify: `src/App.tsx` (`captureRenderSnapshot`, `uploadCustomImage`, beide `ModelGrid`-Aufrufe, `ModelDetailPage`-Aufruf)

**Interfaces:**
- Consumes: `resolveDisplayImage` aus `src/lib/resolveDisplayImage.ts`, `DisplayPreference` aus `src/hooks/useDisplayPreference.ts` (beide Task 2), `displayPreference`-State aus `App.tsx` (Task 3).

- [ ] **Step 1: `ModelGrid.tsx` — Props und Import ergänzen**

In `src/components/ModelGrid.tsx`, ergänze die Imports:

```ts
import { resolveDisplayImage } from '../lib/resolveDisplayImage';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
```

Im `Props`-Interface, nach `onToggleBulkSelect: (id: string) => void;`, einfügen:

```ts
  displayPreference: DisplayPreference;
```

In der Funktionssignatur von `ModelGrid`, `displayPreference` zur Destrukturierung hinzufügen.

- [ ] **Step 2: Beide Karten-Varianten auf `resolveDisplayImage` umstellen**

`ModelGrid` hat zwei Karten-Layouts, die sich je nach `density` denselben `models.map((m) => density === 'comfort' ? (...) : renderCompactCard(m))`-Aufruf teilen - die Compact-Variante lebt in der separaten Funktion `renderCompactCard(m)`, die Comfort-Variante ist inline im Ternary. Beide werden unabhängig voneinander direkt per Inline-Aufruf umgestellt (keine Zwischenvariable nötig).

In `renderCompactCard`, die Zeilen

```tsx
          {m.displayImage ? (
            <img
              src={m.displayImage}
              alt=""
              className="absolute inset-0 w-full h-full object-cover"
            />
          ) : (
```

ersetzen durch:

```tsx
          {resolveDisplayImage(m, displayPreference) ? (
            <img
              src={resolveDisplayImage(m, displayPreference) ?? undefined}
              alt=""
              className="absolute inset-0 w-full h-full object-cover"
            />
          ) : (
```

In der Comfort-Karten-Variante (inline im Ternary), die Zeile

```tsx
              {m.displayImage ? (
                <img src={m.displayImage} alt="" className="absolute inset-0 w-full h-full object-cover" />
              ) : (
```

ersetzen durch:

```tsx
              {resolveDisplayImage(m, displayPreference) ? (
                <img src={resolveDisplayImage(m, displayPreference) ?? undefined} alt="" className="absolute inset-0 w-full h-full object-cover" />
              ) : (
```

- [ ] **Step 3: `ModelDetailPage.tsx` umstellen**

In `src/components/ModelDetailPage.tsx`, ergänze die Imports:

```ts
import { resolveDisplayImage } from '../lib/resolveDisplayImage';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
```

Im `Props`-Interface, ergänze:

```ts
  displayPreference: DisplayPreference;
```

In der Funktionssignatur, `displayPreference` zur Destrukturierung hinzufügen.

Die Zeile

```ts
  const [showCustomImage, setShowCustomImage] = useState(false);
```

ersetzen durch:

```ts
  const resolvedImage = resolveDisplayImage(model, displayPreference);
  const [showCustomImage, setShowCustomImage] = useState(
    () => displayPreference === 'thumbnail' && resolvedImage !== null,
  );
```

Den Block

```tsx
            {showCustomImage && model.displayImage ? (
              <img src={model.displayImage} alt={model.name} className="absolute inset-0 w-full h-full object-contain" />
            ) : (
              <ModelViewer
                fileId={model.id}
                needsSnapshot={model.displayImage === null}
                onSnapshotCaptured={onSnapshotCaptured}
              />
            )}
            {model.displayImage && (
```

ersetzen durch:

```tsx
            {showCustomImage && resolvedImage ? (
              <img src={resolvedImage} alt={model.name} className="absolute inset-0 w-full h-full object-contain" />
            ) : (
              <ModelViewer
                fileId={model.id}
                needsSnapshot={model.renderSnapshotImage === null}
                onSnapshotCaptured={onSnapshotCaptured}
              />
            )}
            {resolvedImage && (
```

- [ ] **Step 4: `DetailPanel.tsx` — `needsSnapshot` an beiden Stellen anpassen**

In `src/components/DetailPanel.tsx`, an BEIDEN Stellen (Zeile ~167 und Zeile ~408), die Zeile

```tsx
            needsSnapshot={model.displayImage === null}
```

ersetzen durch:

```tsx
            needsSnapshot={model.renderSnapshotImage === null}
```

- [ ] **Step 5: `App.tsx` — Handler und Komponenten-Aufrufe anpassen**

Die Funktion

```ts
  const uploadCustomImage = (id: string) => {
    invoke<string | null>('upload_custom_image', { fileId: id })
      .then((displayImage) => {
        if (displayImage === null) return;
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, displayImage } : m)));
      })
      .catch((e) => {
        console.error('[custom-image] Hochladen fehlgeschlagen:', e);
      });
  };

  const captureRenderSnapshot = (id: string, base64: string) => {
    const displayImage = `data:image/png;base64,${base64}`;
    setModels((prev) =>
      prev.map((m) => (m.id === id && m.displayImage === null ? { ...m, displayImage } : m)),
    );
    invoke('set_render_snapshot', { fileId: id, imageBase64: base64 }).catch((e) => {
      console.error('[render-snapshot] Speichern fehlgeschlagen:', e);
    });
  };
```

ersetzen durch:

```ts
  const uploadCustomImage = (id: string) => {
    invoke<string | null>('upload_custom_image', { fileId: id })
      .then((customImage) => {
        if (customImage === null) return;
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, customImage } : m)));
      })
      .catch((e) => {
        console.error('[custom-image] Hochladen fehlgeschlagen:', e);
      });
  };

  const captureRenderSnapshot = (id: string, base64: string) => {
    const renderSnapshotImage = `data:image/png;base64,${base64}`;
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, renderSnapshotImage } : m)));
    invoke('set_render_snapshot', { fileId: id, imageBase64: base64 }).catch((e) => {
      console.error('[render-snapshot] Speichern fehlgeschlagen:', e);
    });
  };
```

In BEIDEN `<ModelGrid ... />`-Aufrufen (Trash-Ansicht ~Zeile 517 und Katalog-Ansicht ~Zeile 682), jeweils die Zeile `onToggleBulkSelect={...}` ergänzen um (direkt danach, bzw. vor dem schließenden `/>` bzw. `readOnly`):

```tsx
                  displayPreference={displayPreference}
```

Im `<ModelDetailPage ... />`-Aufruf, nach der Zeile `onSnapshotCaptured={(base64) => captureRenderSnapshot(detailModel.id, base64)}`, einfügen:

```tsx
                displayPreference={displayPreference}
```

- [ ] **Step 6: TypeScript und Build verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler.

Run: `npm run build`
Expected: baut ohne Fehler.

- [ ] **Step 7: Commit**

```bash
git add src/components/ModelGrid.tsx src/components/ModelDetailPage.tsx src/components/DetailPanel.tsx src/App.tsx
git commit -m "feat: ModelGrid/ModelDetailPage/DetailPanel und App.tsx-Handler auf getrennte Bildquellen umgestellt"
```

---

### Task 5: Automatisches Hintergrund-Nachrendern

**Files:**
- Create: `src/components/BackgroundSnapshotRenderer.tsx`
- Modify: `src/App.tsx` (Import, `pendingSnapshotIds`-Berechnung, Mount-Stelle)

**Interfaces:**
- Consumes: `ModelViewer` (`src/components/ModelViewer.tsx`, unverändert), `displayPreference`-State und `captureRenderSnapshot`-Funktion aus `App.tsx` (Task 3/4).

- [ ] **Step 1: `BackgroundSnapshotRenderer.tsx` erstellen**

Erstelle `src/components/BackgroundSnapshotRenderer.tsx`:

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

- [ ] **Step 2: In `App.tsx` einbinden**

Ergänze den Import (nach `import { CatalogCleanupDialog } from './components/CatalogCleanupDialog';`):

```ts
import { BackgroundSnapshotRenderer } from './components/BackgroundSnapshotRenderer';
```

Nach der Zeile

```ts
  const { preference: displayPreference, setPreference: setDisplayPreference } = useDisplayPreference();
```

einfügen:

```ts
  const pendingSnapshotIds = useMemo(
    () => models.filter((m) => m.renderSnapshotImage === null).map((m) => m.id),
    [models],
  );
```

(`useMemo` ist bereits importiert - `App.tsx` nutzt es an anderer Stelle bereits.)

Direkt nach dem öffnenden `<div ... style={{ fontSize: 14 }}>`-Tag des Root-Returns (vor `<Header ... />`), einfügen:

```tsx
      {displayPreference === 'render' && pendingSnapshotIds.length > 0 && (
        <BackgroundSnapshotRenderer
          key={pendingSnapshotIds[0]}
          fileId={pendingSnapshotIds[0]}
          onSnapshotCaptured={(base64) => captureRenderSnapshot(pendingSnapshotIds[0], base64)}
        />
      )}
```

- [ ] **Step 3: TypeScript und Build verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler.

Run: `npm run build`
Expected: baut ohne Fehler.

- [ ] **Step 4: Manueller Smoke-Test**

Run: `cd src-tauri && cargo build --release && cd .. && npm run tauri dev`
Erwartet:
- Einstellungen öffnen, „Bevorzugte Ansicht" ist sichtbar, Standard „Eingebettetes Bild" ausgewählt.
- Auf „Gerenderte Ansicht" umschalten: Dateien ohne vorhandenen Render-Schnappschuss werden nacheinander im Hintergrund gerendert (beobachtbar z. B. an kurzzeitig erhöhter GPU-Last, DevTools-Netzwerktab zeigt keine Auffälligkeit nötig - stattdessen prüfen, dass Karten sich nach kurzer Zeit von Platzhalter/eingebettetem Bild zu gerendertem Bild ändern, sofern kein eingebettetes Bild vorrangig gezeigt wird).
- Modell-Detailseite öffnen: Standard-Tab entspricht der Einstellung.
- Zurück auf „Eingebettetes Bild" umschalten: Karten und Detailseite zeigen wieder bevorzugt das eingebettete Bild, aber Render-Schnappschüsse bleiben als Fallback nutzbar (z. B. bei einer Datei ohne eingebettetes Bild).
- Manuelles Hochladen eines Custom-Bildes funktioniert weiterhin unabhängig von der Einstellung.

- [ ] **Step 5: Commit**

```bash
git add src/components/BackgroundSnapshotRenderer.tsx src/App.tsx
git commit -m "feat: automatisches sequenzielles Hintergrund-Nachrendern fehlender Schnappschuesse"
```
