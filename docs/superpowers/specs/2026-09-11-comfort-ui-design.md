# Komfort-UI: Lesbarkeits-Überarbeitung als alternative Ansicht

**Datum:** 2026-09-11
**Status:** Genehmigt, bereit für Implementierungsplan

## Problem

Die bestehende Oberfläche ("Kompakt"-Ansicht) verwendet durchgängig sehr kleine
Schriftgrößen (überwiegend 9–13px, siehe `text-[9px]`…`text-[13px]` in
`ModelGrid.tsx`, `Sidebar.tsx`, `Header.tsx`, `DetailPanel.tsx`) und kleine
Grafiken (Karten-Thumbnails ab 178px, 3D-Vorschau im Detail-Panel im
4:3-Bereich einer 336px breiten Leiste). Das ist bewusst als dichte,
"Ingenieur-Blueprint"-Ästhetik gestaltet, aber schlecht lesbar.

Ziel: eine zweite, deutlich lesbarere Oberfläche schaffen, die sich am
Karten-Layout von printables.com/model orientiert, aber moderner wirkt
(abgerundete Karten, Schatten, großzügiger Weißraum statt Web-2.0-Instanz von
Printables). Die bestehende kompakte Ansicht bleibt vollständig erhalten und
wird nicht ersetzt.

## Nicht-Ziele

- Keine Änderung der bestehenden Kompakt-Ansicht über die reine
  Datenfeld-Erweiterung (Favorit) hinaus.
- Keine neue Test-Infrastruktur fürs Frontend (siehe Abschnitt Verifikation).
- Kein Cross-Plattform-Redesign der Tauri-Fensterchrome, nur Inhalt innerhalb
  des Fensters.

## Umschalt-Mechanik

Neue persistierte Einstellung `uiDensity: 'compact' | 'comfort'`, exponiert
über einen neuen Hook `useUiDensity.ts` nach demselben Muster wie
`useTheme.ts` (State + `localStorage`, eigener Storage-Key, kein Tauri-Store
nötig). Default ist `'compact'` — bestehendes Verhalten ändert sich für
niemanden ungefragt.

Der Umschalter sitzt im bestehenden Einstellungs-Panel in `Header.tsx`, direkt
neben dem Hell/Dunkel-Umschalter (Options-Menü, kein Ansichts-Button in der
Toolbar).

Das gewählte Setting wird als `data-density="comfort"` (bzw. weggelassen für
`compact`) auf demselben Root-Element gesetzt, das bereits `data-app`/
`data-app="dark"` trägt. Dichte und Farbschema sind orthogonal — alle 4
Kombinationen (hell/dunkel × kompakt/komfort) funktionieren automatisch, ohne
dass Komfort-Komponenten eigene Farbwerte definieren.

## Design-Token-Erweiterung

`src/styles/theme.css` bekommt eine zweite Token-Ebene für Größen/Abstände,
parallel zur bestehenden Farb-Token-Ebene:

```css
[data-app] {
  /* bestehende Farb-Tokens unverändert */
  --font-size-title: 12.5px;
  --font-size-body: 13px;
  --font-size-meta: 10px;
  --font-size-label: 9px;
  --space-card-pad: 0.625rem;   /* entspricht px-2.5 py-2.5 */
  --radius-card: 4px;
  --icon-badge-size: 1.125rem;  /* 18px, für Mini-Badges */
}

[data-density="comfort"] {
  --font-size-title: 17px;
  --font-size-body: 14.5px;
  --font-size-meta: 13px;
  --font-size-label: 11px;
  --space-card-pad: 1rem;
  --radius-card: 14px;
  --icon-badge-size: 2.375rem;  /* 38px, validiertes Favorit-Icon */
}
```

Komponenten werden schrittweise von hartkodierten Tailwind-Arbitrary-Values
(`text-[10px]`, `rounded-[4px]`, …) auf `var(--font-size-*)` /
`var(--radius-card)` etc. umgestellt, wo sie zwischen den beiden Dichten
variieren sollen. Werte, die in beiden Modi identisch bleiben sollen (z. B.
reine Layout-Strukturen), bleiben unverändert.

## Betroffene Komponenten

Alle Komponenten bekommen die Komfort-Behandlung; visuell abgenommen wurden
Karte, Gesamtlayout und Detail-Panel (Mockups in
`.superpowers/brainstorm/9176-1789138767/content/`), der Rest folgt dem
gleichen Token-Ansatz ohne eigene Mockups.

### ModelGrid — Karte (validiert: Hybrid A+C)

- `rounded-[14px]` Karte, `box-shadow`, 5px Akzentbalken oben
  (Verlauf `--accent` → helleres Orange).
- Vorschau-Hintergrund: schraffiertes Muster wie bisher (`--hatch`), aber
  größere Kachelfläche.
- Badges: NEU oben links (weißes Pill-Badge), Herkunft (GD/OD/…) oben rechts,
  Gedruckt-Status unten links (grünes Pill-Badge).
- **Favorit-Toggle** unten rechts: Kreis-Button, `--icon-badge-size` groß
  (38px in Komfort), leeres Herz = nicht favorisiert, gefülltes Herz in
  `--accent`-Farbe = favorisiert. Klickbar, kein Zähler (kein
  Community-Feature, rein persönliches Merkmal).
- Titel fett, serifenlos, `--font-size-title`. Tags als Pills
  (`--font-size-meta`). Meta-Zeile (Material/Zeit/Gewicht) mit Icons.

### ModelGrid — Karte (Kompakt, nur Ergänzung)

Kompakt-Karte bleibt strukturell unverändert; einziger Zusatz ist ein neues
kleines Favorit-Badge (analog Stil zu NEU/Herkunft/Gedruckt, 9px
Monospace-Icon) in der bisher freien Ecke der Vorschau (oben rechts, sofern
kein Herkunfts-Badge belegt ist — sonst kombiniert in einer Zeile).

### DetailPanel

Reihenfolge (bestätigt): 3D-Vorschau → Titel + Tags → Eigenschaften-Tabelle
(Maße, Volumen, Gewicht, Material, Dateigröße, Importiert) → Aktions-Buttons.

Aktions-Buttons in Komfort: volle Breite, Icon + Textlabel (nicht nur Icon),
primärer Button ("In Slicer öffnen") in Akzentfarbe, restliche Buttons
neutral. Eigenschaften-Zeilen: Label/Wert je `--font-size-body`,
Trennlinie pro Zeile.

Das Detail-Panel bekommt in beiden Dichten zusätzlich einen Favorit-Toggle
als eigenen Aktions-Eintrag (Komfort: Icon+Text wie die anderen Buttons;
Kompakt: kleiner Icon-Button im bestehenden Kompakt-Stil), damit Favorisieren
nicht nur über die Grid-Karte, sondern auch beim Betrachten eines einzelnen
Modells möglich ist. Ansonsten bleibt die Kompakt-Struktur des Panels
unverändert.

### Header, Sidebar, ModelList, FilamentView, ContextMenu,
    CatalogCleanupDialog, ImportSummaryBanner

Übernehmen automatisch die größeren Token-Werte unter `data-density="comfort"`
(Schriftgrößen, Innenabstände, Eckradien größerer Interaktionsflächen wie
Buttons/Zeilen). Keine strukturelle Neugestaltung, keine eigenen Mockups
nötig — das ist der Zweck der Token-Ebene.

## Datenmodell-Änderung: Favorit

- **Typ:** `favorite: boolean` neues Feld in `ModelFile`
  (`src/types/index.ts`), Default `false`.
- **Backend:** neue SQLite-Spalte via Migration (analog zum bestehenden
  `printStatus`/`print_status`-Muster), neuer Tauri-Command
  `toggle_favorite(id)` analog zu `toggle_print_status`.
- **Frontend-Wiring:** `App.tsx` bekommt `toggleFavorite`-Handler nach dem
  Muster von `togglePrintStatus`, durchgereicht an `ModelGrid` und
  `DetailPanel`.
- **Sichtbarkeit:** in beiden UI-Dichten sichtbar/klickbar (siehe oben),
  damit das Feature nicht davon abhängt, welche Ansicht gerade aktiv ist.

## Verifikation

Es existiert aktuell keine Frontend-Test-Suite (nur Rust-Backend-Tests,
`cargo test`, 106 Tests Stand 2026-09-11). Für diese Aufgabe wird bewusst
keine neue Frontend-Test-Infrastruktur eingeführt — das wäre unverhältnismäßig
zum Umfang einer UI-Überarbeitung.

Stattdessen:

1. **Backend:** `cargo test` für Migration + `toggle_favorite`-Command
   (neuer Test analog zu vorhandenen `toggle_print_status`-Tests).
2. **Frontend, manuell:** Dev-Server starten, alle 4 Kombinationen
   (hell/dunkel × kompakt/komfort) durchklicken — Grid, Detail-Panel,
   Sidebar, Listenansicht, Filament-Lager, Aufräum-Dialog, Kontextmenü,
   Import-Banner. Sichtprüfung auf Lesbarkeit, Clipping (siehe unten),
   Kontrast in beiden Themes.
3. Vor Abschluss: Screenshot-Vergleich Kompakt vs. Komfort für Grid und
   Detail-Panel, damit der Unterschied dokumentiert ist.

**Bekannte Fallstricke aus dem Mockup-Prozess:** Nicht mit `overflow: hidden`
und `line-height: 1` kombinieren, wenn Text mit Unterlängen (g, j, y, p, q)
in einem eng bemessenen Container sitzt — das schneidet Zeichen sichtbar ab
(im Mockup bei `.brand`-Titel aufgetreten und behoben).

## Rollout

- Feature schrittweise implementierbar: (1) Token-Ebene + Umschalter-Hook,
  (2) Favorit-Datenfeld end-to-end, (3) ModelGrid-Komfort-Karte, (4)
  DetailPanel-Komfort, (5) restliche Komponenten auf Tokens umstellen.
- Jeder Schritt einzeln lauffähig und committbar (kein Big-Bang-Merge nötig).
