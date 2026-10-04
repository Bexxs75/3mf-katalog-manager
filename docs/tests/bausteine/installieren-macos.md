{{download}}

Die STEP-Vorschau ist in diesem Installer enthalten. Es ist eine DMG-Datei.

Öffne die DMG-Datei und ziehe die App in den Ordner „Programme“.

Beim ersten Start warnt macOS, dass die App aus dem Internet geladen wurde und der Entwickler nicht verifiziert ist. Öffne Systemeinstellungen → Datenschutz & Sicherheit und klicke unten auf **„Trotzdem öffnen“**, dann starte die App erneut. Unter macOS 14 und älter geht auch: Rechtsklick auf die App → **Öffnen**. Auf Firmen-Macs kann die IT „Trotzdem öffnen“ gesperrt haben.

Meldet macOS stattdessen „ist beschädigt“, öffne ein Terminal und führe aus:

`xattr -cr "/Applications/{{produkt}}.app"`
