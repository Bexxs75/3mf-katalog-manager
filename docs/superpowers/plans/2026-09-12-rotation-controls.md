# Dreh-Steuerelemente Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Der 3D-Viewer auf der Modell-Detailseite bekommt eine Auto-Rotation (Play/Pause) plus zwei 15°-Schritt-Buttons zum Drehen um die vertikale Achse, zusätzlich zum bestehenden freien Maus-Ziehen.

**Architecture:** `OrbitControls.autoRotate` (three.js-Standardfunktion) übernimmt die Dauerdrehung; die Schritt-Buttons verschieben die Kamera über `THREE.Spherical`-Koordinaten um `controls.target`. Alles kapselt sich in `ModelViewer.tsx` selbst hinter einer neuen optionalen Prop `showRotationControls`.

**Tech Stack:** React/TypeScript, three.js (`OrbitControls`, bereits Projekt-Dependency).

## Global Constraints

- Freies Maus-Ziehen (inkl. vertikalem Verkippen) bleibt vollständig unverändert — die neuen Steuerelemente sind rein additiv, keine Einschränkung der bestehenden `OrbitControls`.
- Nur `ModelDetailPage.tsx` setzt `showRotationControls={true}`. `DetailPanel.tsx` (zwei Stellen) und `BackgroundSnapshotRenderer.tsx` lassen die Prop weg (Standardverhalten unverändert).
- Schrittweite: 15° (`Math.PI / 12`) pro Klick.
- Platzierung: Overlay unten mittig im Viewer, Pill-Design analog zum bestehenden „3D-Ansicht/Bild"-Umschalter (`bg-[var(--panel-2)] border border-[var(--line)] rounded-full`), mit dem Nutzer per HTML-Mockup abgestimmt.
- Kein Test-Framework im Frontend vorhanden — Verifikation über `npx tsc --noEmit` + `npm run build` + manuellen Smoke-Test (etabliertes Projektmuster).

---

### Task 1: Auto-Rotation + Schritt-Buttons in ModelViewer

**Files:**
- Modify: `src/components/ModelViewer.tsx` (komplette Datei betroffen: Props-Interface, neuer State, neue Handler, JSX)
- Modify: `src/components/ModelDetailPage.tsx` (ein Zeile, `<ModelViewer>`-Aufruf)

**Interfaces:**
- Consumes: nichts Neues aus anderen Tasks (eigenständiges Feature).
- Produces: `ModelViewer`-Prop `showRotationControls?: boolean`. Keine neuen Exporte, die andere Dateien konsumieren.

- [ ] **Step 1: Props-Interface erweitern**

In `src/components/ModelViewer.tsx`, im `Props`-Interface, nach `onError?: () => void;` einfügen:

```ts
  showRotationControls?: boolean;
```

In der Funktionssignatur von `ModelViewer`, `showRotationControls` zur Destrukturierung hinzufügen:

```ts
export function ModelViewer({ fileId, needsSnapshot, onSnapshotCaptured, onError, showRotationControls }: Props) {
```

- [ ] **Step 2: State für Auto-Rotation hinzufügen**

Direkt nach der bestehenden Zeile

```ts
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
```

einfügen:

```ts
  const [autoRotating, setAutoRotating] = useState(false);
```

- [ ] **Step 3: Auto-Rotation mit OrbitControls synchronisieren**

Nach dem zweiten bestehenden `useEffect` (Modellwechsel-Effekt, endet mit `}, [fileId]);`), einen neuen `useEffect` einfügen:

```ts
  // Synchronisiert den Auto-Rotation-Button-Zustand mit OrbitControls'
  // eingebauter autoRotate-Funktion. OrbitControls pausiert autoRotate
  // intern automatisch, sobald der Nutzer selbst per Maus zieht, und setzt
  // sie danach von selbst fort - kein eigener Rotations-Loop noetig.
  useEffect(() => {
    if (ctxRef.current) {
      ctxRef.current.controls.autoRotate = autoRotating;
    }
  }, [autoRotating]);
```

- [ ] **Step 4: Schritt-Rotation-Handler hinzufügen**

Direkt nach dem in Step 3 eingefügten `useEffect`, einfügen:

```ts
  // Dreht die Kamera um einen festen Schritt (15 Grad) um die vertikale
  // Achse, unabhaengig vom Auto-Rotation-Zustand. Reine oeffentliche
  // three.js-API (THREE.Spherical) - OrbitControls' interne
  // rotateLeft/rotateRight-Methoden sind private Closures, nicht von
  // aussen ansprechbar.
  const rotateStep = (direction: 1 | -1) => {
    const ctx = ctxRef.current;
    if (!ctx) return;
    const offset = ctx.camera.position.clone().sub(ctx.controls.target);
    const spherical = new THREE.Spherical().setFromVector3(offset);
    spherical.theta += direction * (Math.PI / 12);
    const newOffset = new THREE.Vector3().setFromSpherical(spherical);
    ctx.camera.position.copy(ctx.controls.target).add(newOffset);
    ctx.camera.lookAt(ctx.controls.target);
    ctx.controls.update();
  };
```

- [ ] **Step 5: Overlay-JSX hinzufügen**

Der bestehende Rückgabewert von `ModelViewer` endet aktuell mit:

```tsx
  return (
    <div className="relative w-full h-full">
      <div ref={containerRef} className="absolute inset-0" />
      {status === 'loading' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none">
          {t('loadingPreview')}
        </div>
      )}
      {status === 'error' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none px-4 text-center">
          {t('previewUnavailable')}
        </div>
      )}
    </div>
  );
}
```

Ersetze das komplette `return (...)` durch:

```tsx
  return (
    <div className="relative w-full h-full">
      <div ref={containerRef} className="absolute inset-0" />
      {status === 'loading' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none">
          {t('loadingPreview')}
        </div>
      )}
      {status === 'error' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none px-4 text-center">
          {t('previewUnavailable')}
        </div>
      )}
      {showRotationControls && status === 'ready' && (
        <div className="absolute bottom-3 left-1/2 -translate-x-1/2 flex items-center gap-1.5 bg-[var(--panel-2)] border border-[var(--line)] rounded-full p-1 shadow-[var(--shadow)]">
          <button
            onClick={() => rotateStep(-1)}
            title={t('rotateLeftAria')}
            className="w-7 h-7 grid place-items-center rounded-full text-[var(--ink-2)] hover:bg-white/10 hover:text-[var(--ink)]"
          >
            <svg viewBox="0 0 24 24" className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M15 18l-6-6 6-6" />
            </svg>
          </button>
          <div className="w-px h-4 bg-[var(--line-strong)]" />
          <button
            onClick={() => setAutoRotating((prev) => !prev)}
            title={autoRotating ? t('pauseRotationAria') : t('playRotationAria')}
            className={`w-7 h-7 grid place-items-center rounded-full ${
              autoRotating
                ? 'bg-[var(--accent)] text-[var(--accent-ink)]'
                : 'text-[var(--ink-2)] hover:bg-white/10 hover:text-[var(--ink)]'
            }`}
          >
            {autoRotating ? (
              <svg viewBox="0 0 24 24" className="w-3.5 h-3.5" fill="currentColor">
                <rect x="6" y="5" width="4" height="14" />
                <rect x="14" y="5" width="4" height="14" />
              </svg>
            ) : (
              <svg viewBox="0 0 24 24" className="w-3.5 h-3.5" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
          </button>
          <div className="w-px h-4 bg-[var(--line-strong)]" />
          <button
            onClick={() => rotateStep(1)}
            title={t('rotateRightAria')}
            className="w-7 h-7 grid place-items-center rounded-full text-[var(--ink-2)] hover:bg-white/10 hover:text-[var(--ink)]"
          >
            <svg viewBox="0 0 24 24" className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M9 18l6-6-6-6" />
            </svg>
          </button>
        </div>
      )}
    </div>
  );
}
```

(Das Overlay erscheint erst, sobald `status === 'ready'` ist — kein Verwaisen der Buttons während des Ladens oder bei einem Fehler.)

- [ ] **Step 6: i18n-Schlüssel hinzufügen**

In `src/i18n/types.ts`, nach der Zeile `displayPreferenceDescriptionRender: string;` (aus einem vorherigen Feature), einfügen:

```ts
  rotateLeftAria: string;
  rotateRightAria: string;
  playRotationAria: string;
  pauseRotationAria: string;
```

In `src/i18n/de.ts`, an derselben Stelle (nach `displayPreferenceDescriptionRender`), einfügen:

```ts
  rotateLeftAria: 'Nach links drehen',
  rotateRightAria: 'Nach rechts drehen',
  playRotationAria: 'Automatische Drehung starten',
  pauseRotationAria: 'Automatische Drehung pausieren',
```

In `src/i18n/en.ts`, an derselben Stelle, einfügen:

```ts
  rotateLeftAria: 'Rotate left',
  rotateRightAria: 'Rotate right',
  playRotationAria: 'Start auto-rotation',
  pauseRotationAria: 'Pause auto-rotation',
```

In `src/i18n/es.ts`, an derselben Stelle, einfügen:

```ts
  rotateLeftAria: 'Girar a la izquierda',
  rotateRightAria: 'Girar a la derecha',
  playRotationAria: 'Iniciar rotación automática',
  pauseRotationAria: 'Pausar rotación automática',
```

In `src/i18n/fr.ts`, an derselben Stelle, einfügen:

```ts
  rotateLeftAria: 'Tourner à gauche',
  rotateRightAria: 'Tourner à droite',
  playRotationAria: 'Démarrer la rotation automatique',
  pauseRotationAria: 'Mettre en pause la rotation automatique',
```

- [ ] **Step 7: `ModelDetailPage.tsx` verdrahten**

In `src/components/ModelDetailPage.tsx`, im `<ModelViewer>`-Aufruf (dort wo bereits `needsSnapshot={model.renderSnapshotImage === null}` steht), nach `onSnapshotCaptured={onSnapshotCaptured}` einfügen:

```tsx
                showRotationControls
```

(JSX-Shorthand für `showRotationControls={true}`.)

- [ ] **Step 8: TypeScript und Build verifizieren**

Run: `npx tsc --noEmit`
Expected: 0 Fehler.

Run: `npm run build`
Expected: baut ohne Fehler.

- [ ] **Step 9: Manueller Smoke-Test**

Run: `cd src-tauri && cargo build --release && cd .. && npm run tauri dev`
Erwartet: Modell-Detailseite öffnen. Unten mittig im Viewer erscheinen drei Buttons (←, ▶, →). Klick auf ▶ startet eine sichtbare Dauerdrehung, Icon wechselt zu ⏸ mit Akzentfarbe. Eigenes Maus-Ziehen im Viewer pausiert die Drehung automatisch, sie läuft danach von selbst weiter. Klick auf ← / → dreht das Modell um einen sichtbaren 15°-Schritt, funktioniert auch während die Auto-Rotation läuft. Freies Maus-Ziehen (inkl. Kippen nach oben/unten) funktioniert unverändert. Im kleinen Seitenpanel (Einzelauswahl, nicht die Detailseite) erscheinen KEINE Steuerelemente.

- [ ] **Step 10: Commit**

```bash
git add src/components/ModelViewer.tsx src/components/ModelDetailPage.tsx src/i18n/types.ts src/i18n/de.ts src/i18n/en.ts src/i18n/es.ts src/i18n/fr.ts
git commit -m "feat: Auto-Rotation und Dreh-Buttons im 3D-Viewer der Detailseite"
```
