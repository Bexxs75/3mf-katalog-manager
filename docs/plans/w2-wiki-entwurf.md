# W2 Wiki-Entwurf: Abgleich und Bereinigung im Web (Forgejo #39)

Entwurf, nur lokal; nicht veröffentlicht. Kein Wiki-Ordner im Repo; Übernahme ins Wiki erst nach Freigabe und nach Prüfung des Start-Abgleichs (P5b, siehe Markierung).

## Deutsch: Katalog mit der Festplatte abgleichen und bereinigen (Web-Version)

Voraussetzung: Der Server läuft mit `KATALOG_WRITE=1` und `KATALOG_FILE_OPS=1`, du bist angemeldet. Ohne Schreibmodus zeigt der Bericht nur an.

1. **Abgleich.** Öffne den Abgleich mit der Festplatte. Der Bericht zeigt drei Abschnitte: neue Dateien, Modelle bzw. Begleitdateien, deren Datei fehlt, und eine Unklar-Liste. Hake an, was übernommen werden soll, und lies die Vorschau.
2. **Entfernen.** Fehlende Einträge werden nur aus dem Katalog genommen (Papierkorb des Katalogs, wiederherstellbar). Dateien werden dabei nie verschoben oder gelöscht. Entfernen verlangt einen Bestätigungshaken und zählt gegen das Tageslimit der Dateiverwaltung.
3. **Unklar.** Einträge unter einem Link, bei nicht erreichbarer oder leer wirkender Wurzel oder nach Lesefehlern stehen nur zur Ansicht in der Liste. Prüfe die Verbindung zum Ordner und gleiche erneut ab.
4. **Ablauf.** Es wird in Blöcken zu höchstens 10 Zeilen gearbeitet. „Abbrechen“ stoppt nach dem laufenden Block; Übernommenes bleibt. Der Server prüft jede Zeile direkt vor der Ausführung neu; Zeilen, die sich geändert haben, werden mit Grund übersprungen. Eine Vorschau gilt 60 Minuten.
5. **Katalog auf Probleme prüfen.** Zeigt fehlende Modelle (ein ganz gelöschter Ordner als eine Zeile) und Duplikate. Entfernte Modelle kommen in den Papierkorb. Grenzen: 500 fehlende Modelle, 200 Duplikatgruppen, 20 s.
6. **Mit Datum umbenennen.** Vorschau, dann Anwenden; bis zu 200 Dateien je Anfrage.
7. **Abgleich nach dem Start (Opt-in, aus).** <!-- P5B-PRUEFEN: Wortlaut nach Abschluss von P5b gegen den gebauten Stand prüfen --> Einmal je Serverstart im Hintergrund, nur Hinzufügen neuer Dateien, nie Entfernen.

## English: Compare the catalog with the disk and clean up (web version)

Requirements: the server runs with `KATALOG_WRITE=1` and `KATALOG_FILE_OPS=1` and you are signed in. Without write mode the report is display-only.

1. **Sync.** Open the disk sync. The report has three sections: new files, models or companion files whose file is missing, and an "unclear" list. Tick what to apply and read the preview.
2. **Removing.** Missing entries are only taken out of the catalog (catalog trash, restorable). Files are never moved or deleted by this. Removing needs a confirmation tick and counts against the daily limit of file management.
3. **Unclear.** Entries below a link, with an unreachable or empty-looking root, or after read errors are listed for information only. Check the connection to the folder and sync again.
4. **Run.** Work happens in blocks of at most 10 rows. "Cancel" stops after the running block; what was applied stays. The server re-checks every row right before acting; rows that changed are skipped with a reason. A preview is valid for 60 minutes.
5. **Check catalog for problems.** Shows missing models (a fully deleted folder as one line) and duplicates. Removed models go to the trash. Limits: 500 missing models, 200 duplicate groups, 20 s.
6. **Rename with date.** Preview, then apply; up to 200 files per request.
7. **Check after start (opt-in, off).** <!-- P5B-PRUEFEN: verify wording against the built state once P5b is done --> Once per server start in the background, only adds new files, never removes.
