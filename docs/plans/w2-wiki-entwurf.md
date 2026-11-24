# W2 Wiki-Entwurf: Abgleich und Bereinigung im Web (Forgejo #39)

Entwurf, nur lokal; nicht veröffentlicht. Kein Wiki-Ordner im Repo; Übernahme ins Wiki erst nach Freigabe.

## Deutsch: Katalog mit der Festplatte abgleichen und bereinigen (Web-Version)

Voraussetzung: Der Server läuft mit `KATALOG_WRITE=1` und `KATALOG_FILE_OPS=1`, du bist angemeldet. Ohne Schreibmodus zeigt der Bericht nur an.

1. **Abgleich.** Öffne den Abgleich mit der Festplatte. Der Bericht zeigt drei Abschnitte: neue Dateien, Modelle bzw. Begleitdateien, deren Datei fehlt, und eine Unklar-Liste. Hake an, was übernommen werden soll, und lies die Vorschau.
2. **Entfernen.** Fehlende Einträge werden nur aus dem Katalog genommen (Papierkorb des Katalogs, wiederherstellbar). Dateien werden dabei nie verschoben oder gelöscht. Entfernen verlangt einen Bestätigungshaken und zählt gegen das Tageslimit der Dateiverwaltung.
3. **Unklar.** Einträge unter einem Link, bei nicht erreichbarer oder leer wirkender Wurzel oder nach Lesefehlern stehen nur zur Ansicht in der Liste. Prüfe die Verbindung zum Ordner und gleiche erneut ab.
4. **Ablauf.** Es wird in Blöcken zu höchstens 10 Zeilen gearbeitet. „Abbrechen“ stoppt nach dem laufenden Block; Übernommenes bleibt. Der Server prüft jede Zeile direkt vor der Ausführung neu; Zeilen, die sich geändert haben, werden mit Grund übersprungen. Eine Vorschau gilt 60 Minuten.
5. **Katalog auf Probleme prüfen.** Zeigt fehlende Modelle (ein ganz gelöschter Ordner als eine Zeile) und Duplikate. Entfernte Modelle kommen in den Papierkorb. Grenzen: 500 fehlende Modelle, 200 Duplikatgruppen, 20 s.
6. **Mit Datum umbenennen.** Vorschau, dann Anwenden; bis zu 200 Dateien je Anfrage.
7. **Abgleich nach dem Start (Opt-in, aus).** Einmal je Serverstart (etwa 5 s nach dem Start) im Hintergrund; ohne den zweiten Schalter „Neue Dateien automatisch aufnehmen“ nur ein Bericht. Mit dem Schalter werden höchstens 200 neue Dateien je Start aufgenommen (nach der Tageskopie), der Rest bleibt „weiter offen“; Einträge werden nie entfernt. Ist eine Wurzel nicht erreichbar oder leer, wird nichts angewendet. Das Ergebnis liegt nur im Speicher bis zum Neustart.

## English: Compare the catalog with the disk and clean up (web version)

Requirements: the server runs with `KATALOG_WRITE=1` and `KATALOG_FILE_OPS=1` and you are signed in. Without write mode the report is display-only.

1. **Sync.** Open the disk sync. The report has three sections: new files, models or companion files whose file is missing, and an "unclear" list. Tick what to apply and read the preview.
2. **Removing.** Missing entries are only taken out of the catalog (catalog trash, restorable). Files are never moved or deleted by this. Removing needs a confirmation tick and counts against the daily limit of file management.
3. **Unclear.** Entries below a link, with an unreachable or empty-looking root, or after read errors are listed for information only. Check the connection to the folder and sync again.
4. **Run.** Work happens in blocks of at most 10 rows. "Cancel" stops after the running block; what was applied stays. The server re-checks every row right before acting; rows that changed are skipped with a reason. A preview is valid for 60 minutes.
5. **Check catalog for problems.** Shows missing models (a fully deleted folder as one line) and duplicates. Removed models go to the trash. Limits: 500 missing models, 200 duplicate groups, 20 s.
6. **Rename with date.** Preview, then apply; up to 200 files per request.
7. **Check after start (opt-in, off).** Once per server start (about 5 s after the start) in the background; without the second switch "Add new files automatically" it only reports. With the switch it adds at most 200 new files per start (after the day copy), the rest stays "still open"; entries are never removed. If a root is unreachable or empty nothing is applied. The result is kept in memory until the restart.
