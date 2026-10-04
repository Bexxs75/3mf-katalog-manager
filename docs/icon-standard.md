# Verbindlicher Iconstandard

Freigegeben von Andreas am 03.10.2026. Gilt für den 3MF Katalog Manager und künftige UI-Arbeit.

- Das freigegebene Set aus 32 SVGs in `docs/assets/icons/` ist die verbindliche Grundlage. Keine abweichenden Icons für bereits abgedeckte Bedeutungen verwenden.
- Seitenleiste: **24 × 24 CSS-Pixel**, **Linienstärke 1,8** im **24 × 24 ViewBox-Raster**. Runde Linienenden und -verbindungen, `currentColor` wie in den Originaldateien.
- Die Symbolformen entsprechen exakt dem freigegebenen Entwurf vom 03.10.2026. Weitere benötigte Symbole in diesem Stil ergänzen; keine ungefragten Neuentwürfe bestehender Icons.
- Schaltflächen bleiben 42 × 42 Pixel. Aktive Zustände, Fokus, lokalisierte Beschriftungen und Papierkorb-Zähler erhalten. Statusmarker separat vom Symbol führen.
- SVGs in React inline über eine zentrale typisierte Icon-Komponente verwenden; nicht als externe `<img>` einbinden, damit `currentColor` vom Button übernommen wird. SVG dekorativ (`aria-hidden="true"`, `focusable="false"`); zugänglicher Name am Button. Die Export-`title` müssen nicht zusätzlich vorgelesen werden.
- Zuordnung: Katalog → `catalog`, Material Manager → `spool`, Printer Manager → `printer`, Papierkorb → `trash`, Einstellungen → `settings`. Den Drucker-Button erst zusammen mit dem funktionsfähigen Printer Manager integrieren.

## Umsetzung

Einplanung in **v0.16.0**, GUI/Bedienbarkeit. Die erste Beobachtungswoche und Fehlerbehebungen für v0.15.x bleiben gemäß Release-Baseline vorrangig. Freigabe und SVG-Quellen sind übernommen; die Umstellung des Anwendungscodes ist noch offen. Bei UI-Arbeit betroffene Symbole aus dem Set einsetzen und verbliebene Altsymbole systematisch ersetzen. Keine neue Version, kein Commit und keine Veröffentlichung durch diese Dokumentation.

## Abnahme bei Integration

Seitenleiste in Hell/Dunkel mit 24 px / 1,8 prüfen, Klickfläche und Badge erhalten, Tastaturfokus und aktive Zustände kontrollieren. Alle unterstützten UI-Sprachen behalten zugängliche Buttonnamen. Vorhandene Wiki-/Handbuch-Screenshots bei der sichtbaren UI-Umstellung aktualisieren.

Die freigegebene klickbare Vorschau liegt privat in `docs/intern/mockups/iconset-approved-2026-10-03/index.html` (lokal öffnen). Ursprünglicher Entwurf: `/home/thebexxs/Dokumente/3mf-iconset-entwurf/`.
