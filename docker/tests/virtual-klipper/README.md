# Virtual Klipper printer / Virtueller Klipper-Drucker (Forgejo #42, C6)

## English
A throw-away "printer" for the web printer link: a small Python server that speaks the Moonraker REST calls the
catalog uses (`/server/info`, `/server/history/list`, thumbnails, optional `X-Api-Key`) with the JSON shape of
Moonraker v0.8. A virtual print is `in_progress` while it runs and becomes `completed` afterwards, as Klipper's
history would record it. It is **not** a real Klipper/Moonraker build (that needs a firmware MCU); the contract
the catalog depends on is the same, and the fixtures in `crates/katalog-core/tests/fixtures/moonraker` come from real printers.

Run on the test machine (ObiWan), from the repository root, with Docker and Rust:

    NAME=vk bash docker/tests/virtual-klipper/run.sh

The script builds the image, creates a private bridge network (172.29.252.0/24, no published ports, no route into the
home network is used), starts one printer with an API key and one that answers every request with a redirect to the
cloud metadata address, runs `crates/katalog-server/tests/virtual_klipper.rs` (marked `#[ignore]`), and removes
containers, image and network. Covered: connect with key, wrong key, running print not offered, finished prints once,
booking off a spool (once), forbidden addresses (loopback, metadata, public, outside the list), redirect not followed,
empty list switches the link off, API key never in an answer.

## Deutsch
Ein Wegwerf-"Drucker" für die Web-Druckeranbindung: kleiner Python-Server mit den Moonraker-Aufrufen des Katalogs
(Struktur wie Moonraker v0.8, optional `X-Api-Key`). Ein virtueller Druck ist erst `in_progress`, dann `completed`.
Es ist **kein** echtes Klipper/Moonraker (dafür wäre eine Firmware-MCU nötig); der Vertrag, auf den der Katalog sich
stützt, ist derselbe. Aufruf siehe oben (`NAME=vk bash docker/tests/virtual-klipper/run.sh`), nur auf dem Testrechner.
