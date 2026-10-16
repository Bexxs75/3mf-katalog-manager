# Vorschlag: Weg der Webversion ins Hauptprojekt (Stand 16.10.2026, zur Entscheidung)

## 1. Ausgangslage (gemessen)

| Größe | Wert |
|---|---|
| Abzweigung | `ac1d434c` = `dev/0.16.0` bei RC 12 (`v0.16.0-12`), volle gemeinsame Historie, `upstream` zeigt auf das Desktop-Repo |
| Commits im Web-Repo seit der Abzweigung | 49 (lokal, davon rund 20 nicht gepusht) |
| Betroffene Dateien | 303, +39.800 / −4.500 Zeilen |
| `crates/` (neu: `katalog-core`, `katalog-server`) | 128 Dateien, +40.600 Zeilen |
| `src-tauri/` | 97 Dateien: 78 gelöscht (Logik in den Core verschoben), 18 geändert, 1 neu; −31.200 Zeilen. Die Desktop-App delegiert an den Core. |
| `src/` (gemeinsame Oberfläche) | 131 Dateien: 39 neu, 92 geändert, +3.800 Zeilen |
| Desktop-Änderungen seit der Abzweigung (`dev/0.16.0`) | 1 Commit (Kachelbreite, `ModelGrid`), Überschneidung mit dem Web-Stand in genau diesen zwei Dateien (gleiche Idee, trivial lösbar) |
| Desktop `master` / `release/0.15.0` | 10 bzw. 1 Commit, die nicht in der Basis sind (0.15.x-Fixes); beim Zusammenführen berücksichtigen |
| Getestet | nur Linux. Der Core ist plattformübergreifend gedacht (`upload_fs` ist `cfg(unix)`, `libc` nur für Unix), Windows- und macOS-Build mit dem neuen Core sind **ungeprüft**. |

Heute sind die Konflikte klein, weil sich auf der Desktop-Seite kaum etwas bewegt hat. Das ändert sich mit jedem RC und jedem Fix-Release. Jede Desktop-Änderung an einer der 78 verschobenen Dateien wird beim Zusammenführen zu einem „gelöscht gegen geändert“-Konflikt, den man von Hand in den Core übertragen muss.

## 2. Empfohlener Weg: privat zusammenführen, dann freigeben

**Phase 0 (jetzt bis Stufe 7 abgenommen): still weiterbauen und nah am Desktop bleiben.**
- `upstream/dev/0.16.0` regelmäßig in den Web-`main` mergen (nach jedem RC und mindestens alle zwei Wochen), nie umbasieren. So bleibt der Rest klein.
- Web-Repo bleibt privat und ohne Ankündigung.

**Phase 1 (vor der Integration): Absicherung.**
- Gesamt-Sicherheitsreview der ganzen Webversion (bisher nur je Stufe) und Abhängigkeits-Audit.
- Nachbesserungen aus den Reviews, Stufe 7 abgenommen.
- **Windows- und macOS-Build** mit dem neuen Core prüfen: in einem privaten Repo mit eingeschalteten GitHub Actions (nur die Build-Matrix) oder in einem privaten Testzweig des Hauptrepos. Das ist die größte bekannte Unbekannte.
- `THIRD-PARTY-LICENSES.md` für die neuen Abhängigkeiten (`axum`, `tokio`, `argon2`, …) ergänzen.

**Phase 2 (nach der stabilen 0.16.0): Zusammenführen im privaten Repo.**
- Vom Tag `v0.16.0` einen Zweig `dev/0.17.0` im Desktop-Repo anlegen.
- Im privaten Web-Repo `v0.16.0` und `master` einmal mergen, Konflikte lösen, dann alle Tests (Rust, Frontend, E2E) und die VM-Tests auf Linux, Windows und macOS wie beim RC-Prozess.
- Das Ergebnis als `dev/0.17.0` ins Hauptrepo pushen (ein Merge-Commit, die Historie bleibt erhalten, kein Neuaufsetzen).

**Phase 3 (0.17.0): Veröffentlichung.**
- **Desktop-App 0.17.0:** enthält die Core-Aufteilung (für Nutzer unsichtbar) und die normalen Fixes.
- **Webversion als eigenes Docker-Image** (z. B. `ghcr.io/bexxs75/3mf-katalog-manager-web`), ausdrücklich als **Beta** gekennzeichnet. Das Selkies-Image bleibt unverändert und Standard, bis Stufe 5b (Archive) und Stufe 7 (Dateiverwaltung) abgenommen sind.
- Erst jetzt: Roadmap-Einträge, Discord, Wiki-Seite „Webversion“, Benutzerhandbuch, Ankündigung. Die Docker-Tester (eigener Discord-Bereich) sind die natürliche erste Gruppe.

## 3. Verworfene Alternativen

| Alternative | Warum nicht |
|---|---|
| Früh in 0.16.x integrieren | Der Core-Umzug berührt 97 Desktop-Dateien mitten in der Release-Phase. Das Risiko gehört nicht in einen RC. |
| Repos dauerhaft getrennt halten | Der Core wäre doppelt zu pflegen, jeder Desktop-Fix müsste übertragen werden, der Merge-Aufwand wächst. Nur sinnvoll, wenn die Webversion nie öffentlich wird. |
| Das Web-Repo wird das Hauptrepo | Issues, Releases, Sterne, CI-Geheimnisse und Discord-Anbindungen hängen am Hauptrepo. Der Umzug bringt nichts, was ein Merge nicht auch bringt. |
| Webversion erst nach 0.18.0 | Der Docker-Stream ist die wichtigste Nutzergruppe für das Web, und die Linux-Pakete (0.18) sind ein anderes Thema. |

## 4. Risiken und Gegenmaßnahmen

| Risiko | Maßnahme |
|---|---|
| Windows-/macOS-Build bricht | Phase 1 mit CI-Matrix; Unix-Code bleibt hinter `cfg(unix)`; Desktop-Build ohne Server-Crate prüfen |
| Rückschritt in der Desktop-App durch den Core-Umzug | Alle bisherigen Desktop-Tests unverändert grün (aktuell 1.200+ Rust, 2.000+ Frontend), Opus-Regressions-Review pro Teil (gemacht), VM-Tests je RC |
| Wachsende Merge-Konflikte | Phase 0 (regelmäßig mergen), Desktop-Fixes in verschobenen Dateien sofort im Core nachziehen |
| Neue Support-Last | Eigener Image-Name, „Beta“-Hinweis, klare Abstriche (Abschnitt 10a in `WEB-PLAN.md`), bestehender Docker-Tester-Bereich |
| Sicherheitserwartung („Webserver“) | Ehrliche Aussage: ein Passwort, kein HTTPS im Server, nur Heimnetz/VPN, standardmäßig nur lesend; Upload und Dateiverwaltung nur mit eigenen Schaltern |
| Datenbank-Schema | Der Web-Server öffnet nur die exakt erwartete Schemaversion und migriert nie; die Desktop-App migriert. „Web oder lokal“ steht in der Doku. |
| CI-Laufzeit und Kosten | Workspace-Cache; private CI nur für die Probe nutzen (macOS-Minuten zählen zehnfach) |

## 5. Entscheidungen, die du treffen musst

1. **Zielversion: 0.17 oder 0.18 (Entscheidung des Nutzers am 16.10.2026).** Festlegung nach einem Kriterium statt nach Gefühl: **0.17.0**, wenn zum Funktionsstopp von 0.17 alle Punkte der Bedingungen unten erfüllt sind, sonst **0.18.0** ohne Zeitdruck. Bedingungen für 0.17: Stufe 7 abgenommen, Gesamt-Sicherheitsreview und Abhängigkeits-Audit erledigt, die privaten Windows-/macOS-Builds grün, Merge mit `v0.16.0` gelöst und die VM-Tests auf allen drei Systemen bestanden. Hinweis zur Roadmap: v0.17.0 enthält schon viele Punkte (mehrfarbige Spulen, Varianten, Begleitdateien u. a.), v0.18.0 hat Linux-Pakete, Windows-Deinstallation und Bambu-Testhelfer. Die Webversion kommt als eigenes Image und konkurriert deshalb nicht mit dem Desktop-Umfang, braucht aber dieselbe Test- und Doku-Kapazität. **Es gibt noch keinen öffentlichen Roadmap-Eintrag** (die Webversion bleibt bis zur Freigabe still).
2. **Beta-Kennzeichnung** der Webversion im Namen oder Tag (empfohlen: ja).
3. **Image-Name** (Vorschlag oben).
4. **Private CI-Probe** für Windows und macOS: ja oder nein (empfohlen: ja, einmalig).
5. **Zeitpunkt:** Integration frühestens nach der stabilen 0.16.0.
6. **Umfang für 0.17.0:** nur Anzeigen, Bearbeiten, Material, Drucker, Upload (Stufen 0 bis 6) oder auch die Dateiverwaltung (Stufe 7)? Empfehlung: Stufe 7 abwarten, weil sie für das Ablösen des Streams nötig ist.

Aufwand (Schätzung, kein Messwert): Zusammenführen und Konflikte 1 bis 2 Arbeitstage, dazu CI-Läufe und VM-Tests; Gesamt-Sicherheitsreview und Audit etwa 1 Tag; Dokumentation (Handbuch, Wiki) etwa 1 Tag.
