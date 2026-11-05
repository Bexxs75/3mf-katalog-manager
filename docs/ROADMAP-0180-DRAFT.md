# Roadmap draft / Roadmap-Entwurf

## English

Local draft only; no issue or public roadmap changed. Web integration B/C is present in this checkout; phase D remains conditional on owner checks and a post-P2 delta audit. The v0.17.1 fix `4e06d73a` is an ancestor of HEAD. This is not a claim that every release commit has been merged. Phase E publication is outside this order. Process diagrams still need reconciliation with the final release workflow; no release process is marked accepted here.

Tag dictionary: all 19 entries explicitly listed in the specification already exist. TODO: owner supplies a standalone checkpointed catalog copy without WAL/journal sidecars; run `python3 tools/tag_review.py --catalog-copy /path/to/copy.db`, inspect the top 40, reject proper names, numbers, material abbreviations, filename noise and ambiguous words; propose up to 20 new nouns in four languages for owner approval. No new entries are justified without this evidence. Alias merge removes alias colors; back up before opening the catalog with an expanded dictionary.

Translation review: `node scripts/translation-review.mjs > /private/path/review.csv` produces all current dictionary keys, plural forms and tag names in DE/EN/ES/FR, including all additions since 0.17.0. This superset avoids silently missing edited translations. The CSV is a private review artifact, not shipped. Usage points list source files/lines for literal keys; dynamic call sites still need reviewer inspection. Tests: `npx vitest run scripts/translation-review.test.js`; `python3 -m unittest discover -s tools -p test_tag_review.py`. Native-speaker review is pending and non-blocking.

## Deutsch

Nur lokaler Entwurf; keine Issues oder öffentliche Roadmap geändert. Web-Integration B/C ist im Checkout vorhanden; Phase D wartet auf Eigentümerprüfungen und Delta-Audit nach P2. Der v0.17.1-Fix `4e06d73a` ist Vorfahr von HEAD; das bestätigt nicht die Übernahme jedes Release-Commits. Veröffentlichung (Phase E) gehört nicht zum Auftrag. Prozessdiagramme müssen noch mit dem endgültigen Release-Ablauf abgeglichen werden; keine Prozessabnahme wird behauptet.

Tag-Wörterbuch: Alle 19 ausdrücklich genannten Einträge existieren bereits. TODO: eigenständige konsistente Katalogkopie ohne WAL/Journal liefern, `python3 tools/tag_review.py --catalog-copy /pfad/zur/kopie.db` ausführen, Top 40 prüfen, Eigennamen/Zahlen/Materialkürzel/Dateiwort-Rauschen/Mehrdeutiges aussortieren; bis zu 20 neue Substantive mit vier Übersetzungen zur Freigabe vorlegen. Ohne diese Grundlage keine neuen Einträge. Die Zusammenführung verliert Alias-Farben; vor dem Öffnen mit erweitertem Wörterbuch sichern.

Prüfblatt: `node scripts/translation-review.mjs > /privater/pfad/pruefblatt.csv` liefert sämtliche aktuellen Schlüssel, Pluralformen und Tag-Namen in DE/EN/ES/FR einschließlich aller Ergänzungen seit 0.17.0. Diese Obermenge erfasst auch geänderte Übersetzungen. CSV nur privat zur Prüfung, nicht ausliefern. Verwendungsorte nennen Quelldateien/Zeilen für literale Schlüssel; dynamische Aufrufe muss der Prüfer nachsehen. Tests wie oben. Muttersprachliche Prüfung offen und nicht blockierend.
