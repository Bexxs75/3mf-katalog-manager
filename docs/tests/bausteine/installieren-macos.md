Lade `{{paket}}` von der [Vorschau-Release-Seite]({{release}}) herunter. Es ist eine DMG-Datei.

Öffne die DMG-Datei und ziehe die App in den Ordner „Programme“.

Beim ersten Start meldet sich macOS: klicke mit der rechten Maustaste auf die App und wähle **Öffnen**, oder erlaube es unter Systemeinstellungen → Datenschutz & Sicherheit → **„Trotzdem öffnen“**.

Meldet macOS stattdessen „ist beschädigt“, öffne ein Terminal und führe aus:

`xattr -cr "/Applications/{{produkt}}.app"`
