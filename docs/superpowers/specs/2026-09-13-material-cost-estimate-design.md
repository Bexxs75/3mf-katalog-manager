# Materialkosten-Schätzung pro Modell

## Kontext

Der Katalog kennt seit dem Filamentverbrauch-Feature (siehe
`2026-09-13-slice-filament-usage-design.md`) den realen, vom Slicer
berechneten Filamentverbrauch pro Modell (Typ, Farbe, Gramm, Meter je
Druckplatte/Filament). Das Filament-Lager kennt unabhängig davon Preis und
Ursprungsgewicht jeder Spule. Beide Datenquellen existieren bereits, sind
aber nicht verknüpft — der Nutzer kann die Materialkosten eines Modells
aktuell nur manuell im Kopf überschlagen.

Bewusst außerhalb des Scopes (Diskussion mit dem Nutzer, 2026-09-13): keine
manuelle Zuordnung "dieses Modell wurde mit dieser konkreten Spule gedruckt"
und kein automatischer Bestandsabzug — das war eine separate, nicht gewählte
Idee. Dieses Feature schätzt nur, es bucht nichts.

## Design-Entscheidung: Schätzung statt exakter Zuordnung

Ein Filament im `slice_info` kennt nur Typ (z. B. "PLA") und Farbe (Hex),
keine Spulen-ID — die Slicer-Software weiß nicht, aus welcher physischen
Spule im Lager gedruckt wurde. Eine exakte Kostenzuordnung wäre nur über
manuelle Spulen-Auswahl pro Druck möglich (bewusst nicht gewählt). Stattdessen:

**Durchschnittspreis pro Gramm je Materialtyp aus dem Filament-Lager.** Für
jedes Filament im `slice_info` wird nach Spulen im Lager gesucht, deren
`material`-Feld den Filament-Typ enthält (case-insensitive
Teilstring-Vergleich, gleiche Toleranz wie die bestehende
`estimate_weight_g`-Dichte-Zuordnung in `commands.rs`). Aus allen
Treffern mit gesetztem `price` wird ein Durchschnittspreis pro Gramm
gebildet (`price / original_weight_g`, gemittelt über alle Treffer). Farbe
wird **nicht** zum Matching herangezogen (Farbnamen/Hex-Werte zwischen
Slicer und Lager-Eintrag stimmen zu selten exakt überein, das würde häufiger
zu "kein Treffer" führen als zu einem korrekten Treffer).

Kosten pro Filament = `used_g × Ø-Preis/g für den Materialtyp`.
Gesamtkosten = Summe über alle Filamente aller Platten.

**Fehlender Treffer:** Gibt es im Lager keine Spule mit passendem
Materialtyp oder keine mit gesetztem Preis, bleibt die Kostenschätzung für
dieses Filament `null` (nicht 0 — 0 würde fälschlich "kostenlos" suggerieren)
und wird im UI als "unbekannt" ausgewiesen; die Gesamtsumme wird nur über
die Filamente mit bekanntem Preis gebildet, mit einem Hinweis, falls nicht
alle Filamente eingepreist werden konnten.

**Nur bei vorhandenem `slice_info`:** Ohne echten Slicer-Filamentverbrauch
(nur die grobe Volumen-Schätzung) ist keine Kostenschätzung möglich, da
keine Aufschlüsselung nach Materialtyp existiert — die Kostenzeile
erscheint dann gar nicht (analog dazu, dass die Filament-Aufschlüsselung
selbst auch nur bei vorhandenem `slice_info` erscheint).

**Keine Währung:** Wie beim bestehenden `filament_spools.price`-Feld (siehe
`FilamentDashboard.tsx`/`FilamentTable.tsx`, `formatPrice()`) wird keine
Währungseinheit geführt oder angezeigt — reine Zahl, Interpretation bleibt
beim Nutzer.

## Architektur

**Backend:** Neue Funktion `estimate_material_cost` in `commands.rs`, die
`SliceInfoDto` (aus Task 5 des Filamentverbrauch-Features) und die Liste der
`FilamentSpoolDto`s (bereits über `list_filament_spools` verfügbar) entgegennimmt
und einen `CostEstimateDto` zurückgibt:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimateDto {
    pub total_cost: Option<f64>,
    pub has_unpriced_filaments: bool,
}
```

Wird in `to_dto` berechnet (braucht dafür die Spulenliste — `to_dto` bekommt
dafür einen zusätzlichen Parameter `spools: &[FilamentSpoolRecord]`, alle
Aufrufer von `to_dto` müssen entsprechend angepasst werden, siehe Plan) und
als neues Feld `cost_estimate: Option<CostEstimateDto>` an `ModelFileDto`
angehängt (nur `Some`, wenn `slice_info` vorhanden ist).

**Frontend:** `ModelFile.costEstimate: { totalCost: number | null;
hasUnpricedFilaments: boolean } | null`. Neue Zeile im Filamentverbrauch-
Abschnitt der Detailseite (`ModelDetailPage.tsx`), unterhalb der
Platten-Aufschlüsselung: "Geschätzte Materialkosten: X,XX" (via
`formatPrice`), mit einem Hinweis-Icon/Tooltip-Text, falls
`hasUnpricedFilaments` true ist ("nicht für alle Filamente ein
Lagerpreis gefunden").

## Fehlerbehandlung

- Keine Spule im Lager, kein Preis gesetzt → `total_cost: null`, keine
  Kostenzeile im UI (wie "kein Wert" bei anderen Feldern, `noValue`-Key).
- Division durch 0 (`original_weight_g == 0`) kann in der DB aktuell nicht
  vorkommen (Formular erzwingt eine Zahl > 0), wird aber defensiv per
  `if original_weight_g > 0` vor der Division abgesichert.

## Tests

- Rust-Unit-Tests für `estimate_material_cost`: Treffer mit einer Spule,
  Treffer mit mehreren Spulen (Durchschnitt), kein Treffer (Materialtyp
  nicht im Lager), Treffer ohne gesetzten Preis, gemischt (ein Filament
  bepreist, eines nicht → `has_unpriced_filaments: true`, Summe nur über
  das bepreiste Filament).
- Kein Test nötig für "ohne `slice_info`" auf Ebene dieser Funktion, da sie
  nur aufgerufen wird, wenn `slice_info` bereits vorhanden ist (Aufrufer-
  seitige Bedingung, in `to_dto` getestet).
