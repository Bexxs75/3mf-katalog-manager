# katalog-server

Nur lesender HTTP-Zugriff auf einen bestehenden Katalog. Konfiguration und
Startbedingungen: `katalog-server --help`.

<!-- Proxy-Vertrauen setzt einen abschirmenden Proxy voraus: Der Proxy muss
X-Forwarded-For anhängen oder überschreiben. Der letzte Eintrag ist verbindlich;
ein fehlender oder ungültiger Eintrag wird mit HTTP 400 abgewiesen. -->

Bei `KATALOG_TRUST_PROXY=1` muss der Server ausschließlich hinter einem
abschirmenden Proxy stehen. Der Proxy muss `X-Forwarded-For` **anhängen oder
überschreiben**. Ein fehlender oder ungültiger letzter Eintrag wird abgewiesen.
Dieser Modus ist mit `KATALOG_ALLOW_NO_AUTH=1` nicht kombinierbar.
Bei Nicht-Loopback-Bindung `KATALOG_ALLOWED_HOSTS` gegen DNS-Rebinding setzen.

Cookies sind nicht nach Port getrennt. Andere HTTP-Dienste auf demselben Host
erhalten das Sitzungscookie `katalog_session`; einen eigenen Host-/Domainnamen
verwenden. Das beim ersten Start ausgegebene Zufallspasswort bleibt dauerhaft
im Log (etwa `docker logs`) und sollte nach dem ersten Anmelden geändert werden.
`set-password` liest die Eingabe sichtbar im Terminal. Ein laufender Server
übernimmt die neue Passwortdatei erst nach einem Neustart.

Maximal 256 Verbindungen; überschüssige Verbindungen werden sofort geschlossen.
Header müssen innerhalb von 10 Sekunden vollständig eintreffen. Keep-alive ist
aktiv, mit höchstens 60 Sekunden Inaktivität; Hyper kann wartende nächste Header
bereits nach 10 Sekunden schließen. Anfrage und Body haben 15 Sekunden Zeit;
ausgenommen ist die Geometrie-Verarbeitung, deren Body weiterhin begrenzt ist.
SIGINT/SIGTERM stoppen die Annahme neuer Verbindungen und erlauben aktiven
Verbindungen bis zu 60 Sekunden zum Abschluss.
