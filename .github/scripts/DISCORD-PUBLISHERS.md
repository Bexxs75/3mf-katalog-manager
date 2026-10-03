# Discord: getrennte Veröffentlichungen nach Sprache

Die Skripte benötigen nur Python 3 (Standardbibliothek). Die Sprachkanäle und
MEE6-Rollen werden separat eingerichtet. Diese Änderung setzt keine Kanalrechte.

## GitHub-Actions-Konfiguration

| Typ | Name | Inhalt |
|---|---|---|
| Secret | `ROADMAP_READ_TOKEN` | bestehender Token mit Lesezugriff auf GitHub Project 1 |
| Secret | `DISCORD_ROADMAP_WEBHOOK_DE` | Webhook im deutschen Roadmap-Kanal |
| Secret | `DISCORD_ROADMAP_WEBHOOK_EN` | Webhook im englischen Roadmap-Kanal |
| Variable | `DISCORD_ROADMAP_MSGS_DE` | `DE_TEIL1_ID,DE_TEIL2_ID` |
| Variable | `DISCORD_ROADMAP_MSGS_EN` | `EN_TEIL1_ID,EN_TEIL2_ID` |
| Secret | `DISCORD_RELEASE_WEBHOOK_DE` | Webhook im deutschen Changelog-Kanal |
| Secret | `DISCORD_RELEASE_WEBHOOK_EN` | Webhook im englischen Changelog-Kanal |

Roadmap-Nachrichten müssen vorher **mit dem jeweiligen Webhook** angelegt werden.
Der Abgleich editiert sie nur; er erzeugt keine neuen Nachrichten. Je Kanal sind
zwei Nachrichten nötig. Alle vier IDs müssen verschieden sein.

Zuerst alle sechs neuen Split-Werte vollständig einrichten (vier Secrets und zwei
Variablen), solange die bisherigen Workflows noch aktiv sind. Diese ignorieren
die neuen Namen und verwenden weiterhin die Legacy-Konfiguration. Erst danach
die aktualisierten Skripte und Workflows auf dem Default-Branch bereitstellen.
So ist kein Abschalten des bestehenden Workflows nötig. Teilweise gesetzte
Split-Werte führen im neuen Workflow absichtlich zum Fehler, bevor etwas
veröffentlicht wird.

Bei `release: published` stammt die Workflow-Version aus dem Release-Tag. Daher
muss auch der Branch, aus dem das nächste Release getaggt wird, diesen Patch
enthalten; die Änderung allein auf dem Default-Branch reicht dafür nicht.
`workflow_dispatch` kann dagegen auf dem aktualisierten Default-Branch gestartet
werden. Ein erneutes Veröffentlichen eines alten Tags nutzt weiterhin dessen
alten Workflow; bestehende Release-Tags deshalb nicht nachträglich verschieben.

Anschließend den Roadmap-Workflow manuell ausführen und beide Kanäle prüfen.
Ein manueller Release-Workflow erzeugt hingegen neue Beiträge: nur ausführen,
wenn eine erneute Ankündigung ausdrücklich gewünscht ist.

## Inhalt und Rückfall

Getrennte Release-Kanäle erhalten einen kurzen deutschen bzw. englischen Hinweis
mit Versionsnummer und Link auf die vollständigen Release Notes und Downloads.
Release-Titel und -Body werden nicht automatisch übersetzt. `preview` bleibt
auch bei manuellem Aufruf ausgeschlossen. Die GitHub-Issue-Importe und internen
Beta-/Feedback-Kanäle werden nicht geändert.

Ohne irgendwelche Split-Werte gilt die bisherige Konfiguration:
`DISCORD_ROADMAP_WEBHOOK` plus `DISCORD_ROADMAP_MSGS` (vier IDs in DE1, DE2, EN1,
EN2-Reihenfolge) und `DISCORD_RELEASE_WEBHOOK` (Release-Titel/-Body).
Zum Rückfall **alle** Split-Secrets/-Variablen der betroffenen Veröffentlichung
entfernen und die bisherigen Werte erhalten. Dabei müssen die alten Kanäle und
Webhook-Nachrichten weiter existieren.

Alle Nachrichten unterdrücken Mentions. HTTP-Fehler führen zu einem fehlgeschlagenen
Workflow; Webhook-URLs werden nicht protokolliert. Ein Netzwerkfehler nach bereits
erfolgreicher Zustellung kann eine teilweise Veröffentlichung hinterlassen.
Roadmap-Aufrufe können gefahrlos wiederholt werden (PATCH); Release-Aufrufe sind
nicht idempotent (POST) und müssen vor Wiederholung in beiden Kanälen geprüft werden.

## Lokale Prüfung

```sh
python3 -m unittest discover -s .github/scripts/tests -v
```

Die Tests verwenden keine echten Secrets und senden keine Netzwerkanfragen.
