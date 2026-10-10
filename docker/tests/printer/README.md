# Printer container smoke test / Drucker-Containertest

## English

Run from this checkout's root (Docker with Compose required):

```sh
docker compose -f docker/tests/printer/compose.yaml up --build --abort-on-container-exit --exit-code-from client
docker compose -f docker/tests/printer/compose.yaml down --volumes
```

The first build downloads Rust crates and Debian build dependencies. No desktop,
real printer, catalog, credentials or host ports are used. If the private test
subnet conflicts with a local network, choose another unused RFC1918 subnet.

The smallest test-only entry point is two explicitly ignored library tests:
`serve_container_fixture` keeps the existing `fake_moonraker` server alive on
port 7125; `container_network_smoke` runs the real Moonraker adapter with
`HOME_NETWORK` policy in a second, minimal client container. Both require their
explicit fixture environment variable. Normal `cargo test` skips them. No new
Cargo feature, binary or dependency is needed. The image keeps sources because
the server reads the existing JSON fixtures at their compiled source path.

Expected: the client exits 0 and prints that Docker DNS, private IP policy,
history and clock correction passed. The server clock is one hour ahead; UTC
on the server and Pacific/Honolulu on the client must not change that correction.
Loopback remains forbidden. Compose stops the long-running server after the
client finishes. This is an adapter/network smoke test; translated UI messages
and the D4 command guard are covered by the regular Rust/Vitest tests.

Not executed as part of this work order. The application rejects `.local` names
while `THREEMF_CONTAINER=1`, so the message tells users to enter the printer IP;
it rejects them before any name lookup, whatever the network mode.

## Deutsch

Die obigen Befehle im Stamm dieses Checkouts ausführen (Docker mit Compose).
Der erste Bau lädt Rust-Crates und Debian-Bauabhängigkeiten. Kein Desktop,
echter Drucker, Katalog, Zugangsschlüssel oder freigegebener Host-Port nötig.
Bei einer Netzüberlappung ein anderes freies privates RFC1918-Subnetz wählen.

Zwei ausdrücklich ignorierte Bibliothekstests sind die kleinste Testlösung:
`serve_container_fixture` hält den vorhandenen `fake_moonraker` auf Port 7125
am Leben; `container_network_smoke` prüft den echten Moonraker-Adapter mit
`HOME_NETWORK` in einem zweiten Testcontainer. Beide benötigen ihren expliziten
Fixture-Schalter. Normales `cargo test` überspringt sie. Keine neue Abhängigkeit,
kein Feature und kein Binary. Die Quelldateien bleiben im Image, damit die
vorhandenen JSON-Fixtures über ihren einkompilierten Pfad lesbar sind.

Erwartung: Client beendet sich mit 0 und bestätigt Docker-DNS, private IP-Prüfung,
Historie und Uhrkorrektur. Die Serveruhr geht eine Stunde vor; unterschiedliche
Zeitzonen (UTC und Pacific/Honolulu) dürfen das nicht verändern. Loopback bleibt
gesperrt. Compose beendet anschließend den dauerhaft laufenden Server.
Übersetzte Meldungen und D4-Anbindung prüfen die regulären Rust-/Vitest-Tests.

Dieser Aufbau wurde im Auftrag nicht ausgeführt. Die Anwendung weist `.local`-Namen
bei `THREEMF_CONTAINER=1` ab; die Meldung verweist deshalb auf die Drucker-IP.
Die Abweisung erfolgt vor jeder Namensauflösung, unabhängig vom Netzwerkmodus.
