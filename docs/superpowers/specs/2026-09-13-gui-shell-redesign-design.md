# GUI-Shell-Redesign (Navigationsleiste, einheitliches Kartensystem)

## Kontext

Das aktuelle Layout wechselt zwischen Katalog und Filament-Lager über einen
schmalen Textbutton ganz rechts in `Header.tsx` (nach Sortierung,
Raster/Liste-Umschalter, Papierkorb-Icon, Zahnrad). Katalog- und
Filament-Ansicht benutzen zudem unterschiedliche Eckenradien und
Kartenformate (Katalog: 4px, `ModelGrid.tsx`; Filament: 8–10px,
`FilamentDashboard.tsx`/`FilamentTable.tsx`), wodurch der Wechsel wie ein
App-Bruch wirkt statt wie zwei Ansichten desselben Programms.

Design wurde iterativ als klickbarer HTML-Prototyp erarbeitet und vom User
freigegeben: `docs/superpowers/mockups/2026-09-13-gui-redesign.html`
(Version 3). Dieser Spec beschreibt nur den **Layout-Umbau** (Navigation,
Kartensystem, Kopfzeile) — der neue, echte Ordner-Baum mit
Drag&Drop-Verschieben ist ein eigener Spec:
`2026-09-13-real-folder-tree-design.md`. Beide Specs sind unabhängig
voneinander umsetzbar; dieser hier hat keine Backend-Abhängigkeiten und
sollte zuerst gemergt werden, da er risikoärmer ist (reines Frontend/CSS)
und die Ordner-Baum-Arbeit auf der neuen Sidebar-Struktur aufbaut.

## Ziel

Eine feste, immer sichtbare Navigationsleiste ersetzt den vergrabenen
Text-Button. Katalog und Filament-Lager teilen sich ein Kartensystem
(Radius, Innenabstände, Schatten). Die Kopfzeile zeigt nur noch
kontextabhängige Aktionen.

## Architektur

Reines Frontend-Redesign, keine Rust-/Tauri-Command-Änderungen. Betroffene
Dateien:

- `src/styles/theme.css` — neue Design-Tokens (Radius, Rail-Breite).
- `src/App.tsx` — neue `<Rail>`-Komponente statt der zwei Buttons rechts in
  `Header.tsx`; `mainView`-State bleibt (`'catalog' | 'filament' | 'trash'`),
  wird nur von der neuen Rail statt vom Header gesteuert.
- `src/components/Header.tsx` — Mode-Switch-Button (`onMainViewChange`,
  Zeilen 236–255) entfernt; Header wird pro `mainView` schmaler.
- Neu: `src/components/Rail.tsx` — die Navigationsleiste.
- `src/components/ModelGrid.tsx`, `src/components/CollectionsGallery.tsx`,
  `src/components/FilamentDashboard.tsx` — Radius/Schatten auf einen
  gemeinsamen Wert vereinheitlicht.

## Design-Tokens (Ergänzung `theme.css`)

```css
:root {
  --radius-card: 12px;   /* war 4px (compact) / 14px (comfort) je Kontext,
                             siehe unten */
  --rail-w: 60px;
}
```

`--radius-card` existiert bereits als dichte-abhängiger Token
(`[data-density="compact"]` implizit `4px` über `theme.css` Zeile 81,
`comfort` `14px`). Er wird NICHT verändert — Kompakt-/Komfort-Dichte bleibt
wie sie ist. Stattdessen bekommen alle Karten (`ModelGrid` compact-Variante,
`FilamentDashboard`, `CollectionsGallery`) einheitlich `rounded-[10px]`
fest verdrahtet statt einer Mischung aus `rounded-[4px]` und
`rounded-[10px]`/`rounded-[var(--radius-card)]` — 10px ist ein Mittelwert,
der in compact UND comfort Dichte gut aussieht (schon heute von
`FilamentSpool`-Karten in Dashboard-Ansicht verwendet).

## Navigationsleiste (`Rail.tsx`)

Feste linke Spalte, 60px breit, volle Fensterhöhe, `border-right` wie
`Sidebar`. Von oben nach unten:

1. App-Mark: 30×30px, `bg-[var(--accent)]`, Text "3MF", `border-radius: 8px`.
2. Drei Icon-Buttons (42×42px, `border-radius: 10px`), aktiver Zustand
   `bg-[var(--accent-soft)] text-[var(--accent)]` + `border` in
   `color-mix(in oklch, var(--accent) 30%, transparent)`:
   - Katalog (2×2-Raster-Icon) → `onMainViewChange('catalog')`
   - Filament-Lager (Kreis-Icon) → `onMainViewChange('filament')`
   - Papierkorb (Mülleimer-Icon, Badge mit `trashCount` falls > 0) →
     `onMainViewChange('trash')`
3. Spacer (`flex: 1`).
4. Zahnrad-Button unten → öffnet das bestehende Settings-Panel
   (`settingsOpen`/`onSettingsOpenChange`, unverändert aus `Header.tsx`
   übernommen, nur der Trigger-Button wandert).

Jeder Button zeigt einen Tooltip bei Hover (Name des Ziels), analog zum
Mockup (`.rail-tip`). Tastatur-Fokus sichtbar (`:focus-visible`), da native
`<button>`-Elemente verwendet werden.

## Kopfzeile nach dem Umbau

`Header.tsx` verliert:
- Den Mode-Switch-Button (Zeilen 236–241 im aktuellen Code).
- Das Papierkorb-Icon (Zeilen 243–255) — wandert in die Rail.

Import-Button, Sortierung, Raster/Liste-Umschalter (nur `mainView ===
'catalog'`) und Zahnrad-Panel-Inhalt (Aussehen/Dichte/Sprache/Slicer/
Cleanup/Backup) bleiben unverändert an ihrem Platz — nur der
Öffnungs-Button für das Zahnrad-Panel wandert in die Rail, das Panel selbst
(JSX, State `settingsOpen`) bleibt in `Header.tsx` bzw. wird nach `App.tsx`
gezogen, falls die Rail das Panel rendern soll (Implementierungsdetail, im
Plan festzulegen — State bleibt in jedem Fall in `App.tsx`, wo er heute
schon lebt).

## Bestätigt durch Mockup, keine offenen Design-Fragen

Der User hat den Prototyp mehrfach getestet und explizit freigegeben
("Diese Leiste soll die Buttons oben rechts ersetzen. Das erscheint mir
sehr sinnvoll."). Es gibt daher keine offenen visuellen Entscheidungen für
diesen Teil — der Plan kann direkt aus diesem Spec + dem Mockup-HTML
erzeugt werden.

## Testing

- Manuelle Prüfung: Rail-Klicks wechseln `mainView` korrekt (bestehendes
  Verhalten, nur andere Trigger-Komponente).
- Bestehende Frontend-Typecheck (`npx tsc --noEmit`) muss weiterhin sauber
  laufen.
- Visuelle Prüfung in Hell- UND Dunkel-Theme (beide Themes existieren
  bereits über `[data-app]`/`[data-app="dark"]`, keine neuen Tokens nötig
  außer den oben genannten).
