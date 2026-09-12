# Dreh-Steuerelemente für den 3D-Viewer — Design

## Kontext

Das Drehen eines Modells im 3D-Viewer (`src/components/ModelViewer.tsx`,
nutzt `three.js`' `OrbitControls`) ist aktuell nur per Maus-Drag möglich.
Nutzer-Feedback: das ist umständlich — sowohl weil man die Maus dauerhaft
gedrückt halten und ziehen muss, um das Modell rundherum zu betrachten,
als auch weil die freie Trackball-Rotation beim Ziehen ungewollt auch
vertikal verkippt und man leicht die gerade Ansicht verliert.

## Ziel

Zusätzliche, bequeme Steuerelemente zum Drehen um die vertikale Achse
(wie ein Drehteller) auf der Modell-Detailseite — **ohne** das bestehende
freie Maus-Ziehen einzuschränken. Beides soll parallel funktionieren.

## Mechanismus

`OrbitControls` bringt eine eingebaute Auto-Rotate-Funktion mit
(`controls.autoRotate`, `controls.autoRotateSpeed`): eine kontinuierliche
Drehung um die vertikale Achse, die three.js intern automatisch pausiert,
sobald der Nutzer selbst per Maus zieht, und danach von selbst
weiterläuft. Das wird direkt für den Play/Pause-Button genutzt — kein
eigener Rotations-Loop nötig.

Für die schrittweise Drehung per ←/→-Button wird die aktuelle
Kamera-Position relativ zu `controls.target` in Kugelkoordinaten
umgerechnet (`new THREE.Spherical().setFromVector3(camera.position.clone().sub(controls.target))`),
der Azimutwinkel (`theta`) um ±15° (`Math.PI / 12`) verschoben, und die
Kamera-Position aus den neuen Kugelkoordinaten zurückberechnet
(`setFromSpherical` + `.add(controls.target)`), gefolgt von
`controls.update()`. Das ist reine öffentliche `three.js`-API — kein
Eingriff in interne, nicht exportierte `OrbitControls`-Methoden
(`rotateLeft` o. ä. sind private Closures und nicht ansprechbar).

## Komponente & Platzierung

`ModelViewer` bekommt eine neue optionale Prop `showRotationControls?: boolean`.
Ist sie `true`, rendert `ModelViewer` selbst ein zusätzliches Overlay mit
drei Buttons (← Schritt links, Play/Pause Auto-Rotation, → Schritt
rechts) — die Komponente bleibt dadurch in sich geschlossen, keine neue
Ref-API oder Prop-Drilling zum Elternteil nötig.

- `ModelDetailPage.tsx` setzt `showRotationControls={true}` (einzige
  Stelle).
- `DetailPanel.tsx` (kleines Seitenpanel, zwei `ModelViewer`-Einbindungen)
  und `BackgroundSnapshotRenderer.tsx` (unsichtbarer
  Hintergrund-Renderer) lassen die Prop weg — Standard `undefined`/`false`,
  unverändertes Verhalten dort.

**Platzierung:** Overlay unten mittig im Viewer-Bereich (`position: absolute; bottom: 12px; left: 50%; transform: translateX(-50%)`),
Pill-Design analog zum bestehenden „3D-Ansicht/Bild"-Umschalter oben
rechts (`bg-[var(--panel-2)] border border-[var(--line)] rounded-full`),
per HTML-Mockup mit dem Nutzer abgestimmt und freigegeben
(zwei Zustände: Ruhezustand mit Play-Symbol, aktiver Zustand mit
hervorgehobenem Pause-Symbol in Akzentfarbe).

## Verhalten im Detail

- Play/Pause-Button: Klick togglet internen `ModelViewer`-State
  `autoRotating`, der `ctx.controls.autoRotate` setzt. Icon wechselt
  zwischen ▶ (Ruhezustand) und ⏸ (aktiv, mit `bg-[var(--accent)]`
  hervorgehoben, wie im Mockup).
- ←/→-Buttons: jeder Klick verschiebt die Kamera um einen festen Schritt
  von 15°, unabhängig vom Play/Pause-Zustand der Auto-Rotation (beide
  Mechanismen sind unabhängig voneinander nutzbar — z. B. während die
  Auto-Rotation läuft, kann ein Klick auf → zusätzlich einen Sprung
  auslösen).
- Freies Maus-Ziehen (`OrbitControls`-Standardverhalten, inkl. vertikalem
  Verkippen) bleibt vollständig unverändert und uneingeschränkt
  funktionsfähig — die neuen Steuerelemente sind rein additiv.
- `autoRotateSpeed`: three.js-Standardwert (2.0, entspricht einer vollen
  Umdrehung in ca. 30 Sekunden bei 60fps) wird übernommen, kein Custom-Wert.

## Fehlerbehandlung

Keine neuen Fehlerfälle — die Buttons manipulieren ausschließlich
clientseitigen Kamera-/Controls-Zustand, keine Backend-Aufrufe, kein
persistenter State.

## Testing

Kein Testframework für UI-Interaktionslogik im Projekt vorhanden
(etabliertes Muster: `npx tsc --noEmit` + `npm run build` + manueller
Smoke-Test im gebauten AppImage). Manueller Smoke-Test prüft gezielt:
Play startet sichtbare Dauerdrehung, eigenes Maus-Ziehen pausiert sie
automatisch und sie läuft danach von selbst weiter, ←/→ drehen um
jeweils sichtbare 15°-Schritte, freies Maus-Ziehen (inkl. Verkippen)
funktioniert unverändert wie vor dieser Änderung, Seitenpanel
(`DetailPanel`) zeigt weiterhin keine Steuerelemente.

## Out of Scope

- Keine Steuerelemente im kleinen Seitenpanel oder im unsichtbaren
  Hintergrund-Renderer (siehe Platzierung oben).
- Keine Tastatur-Steuerung (Pfeiltasten o. ä.).
- Keine einstellbare Auto-Rotations-Geschwindigkeit.
- Keine Einschränkung/Sperre der vertikalen Achse beim freien
  Maus-Ziehen — das bleibt bewusst wie bisher, die neuen Buttons sind
  eine Ergänzung, keine Verhaltensänderung der bestehenden Steuerung.
