# drag 2.1.1 — local Linux patch / lokale Linux-Anpassung

## English

Source: CrabNebula `drag` 2.1.1, crates.io package, copied offline from
`/srv/data/home-3mf-fix0161/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/drag-2.1.1/`.
The normalized Cargo.toml, src tree, CHANGELOG.md, LICENSE_MIT and
LICENSE_APACHE-2.0 are included. License: MIT OR Apache-2.0. Version and dependencies
are unchanged. The workspace root Cargo.toml patches crates.io to this directory.

Only `src/platform_impl/gtk/mod.rs` differs from the original source:

- Lines 54–67: COPY only and an explicit `text/uri-list` target (no unserved portal formats).
- Lines 73–79 and 220–236: GLib filename-to-URI conversion, with tests for spaces,
  umlauts, percent, hash, parentheses and invalid relative paths.
- Lines 104–125 and 164–204: drop-performed only marks a drop; drag-failed reports
  cancellation at most once; drag-end reports completion at most once and removes
  the data/failure/end handlers and the context's drop-performed handler. Late
  destinations can request data after the button is released.
- Lines 138–144: remove source handlers if starting the native drag fails.

- Lines 25–33 and 43–52: query the primary button through GTK/GDK before starting;
  a released button reports Cancel through the existing callback and returns Ok.
  Missing window/seat/pointer counts as held. Signatures checked against gtk/gdk 0.18.2.

Known limitation: a destination that finishes the drag before button release can leave
GDK's pointer grab held, as in the original crate (observed with the premature test
sink, not Bambu Studio, OrcaSlicer or gedit). Holding the drag context until GDK releases
its grab would address this (linux-einfrieren-analyse.md, section 2); it is deliberately
not implemented. Touch/stylus button emulation and Wayland are not validated.

Windows/macOS sources and all other copied files are byte-identical to the cache.
No Cargo.lock was edited: the maintainer must regenerate the workspace lockfile;
`drag` becomes a path package and loses its registry source/checksum. No new runtime
dependency is intended. Running this crate's own tests may additionally resolve its
unchanged upstream dev-dependencies.

Validation inside the maintainer's Linux Docker build environment (repository root):

```sh
cargo test --workspace --no-default-features
cargo test -p drag --lib
cargo clippy --workspace --no-default-features --all-targets -- -D warnings
```

The Rust build and native interactions were not executable in the implementation
sandbox. VM acceptance must cover early/late GTK data requests, cancellation,
exactly one callback, repeated drags, and Windows/macOS regression. The existing
app watchdog releases the application session after 60 seconds; it does not itself
emit GTK drag-end or disconnect GTK handlers if GTK fails to complete a session.

To return upstream: verify a released version fixes URI encoding and delayed data
requests, remove the root Cargo.toml drag patch and this directory, update the app's
version requirement as needed, regenerate Cargo.lock, and repeat the native VM
acceptance tests. Do not remove the patch solely because a newer version exists.

## Deutsch

Herkunft ist das oben genannte crates.io-Paket aus dem lokalen Cargo-Cache;
Version, Abhängigkeiten und beide Lizenztexte bleiben erhalten. Nur die oben mit
Datei und Zeilen aufgeführten GTK-Stellen sind geändert: kodierte URIs, ausschließlich
COPY mit `text/uri-list`, Datenhandler bis `drag-end`, einmaliger Rückruf und
Aufräumen bei fehlgeschlagenem Start. Neu: Zeilen 25–33 und 43–52 prüfen die primäre
Maustaste vor dem Start; bei losgelassener Taste folgt Cancel über den vorhandenen
Rückruf und Ok. Fehlendes Fenster, Seat oder Zeiger gelten als gedrückt.
Signaturen wurden gegen gtk/gdk 0.18.2 geprüft. Windows/macOS bleiben bytegleich.

Bekannte Einschränkung: Meldet ein Ziel den Zug vor dem Loslassen als beendet, kann
GDK den Zeigergrab behalten, genauso wie im Original der Crate. Beobachtet wurde dies
am vorzeitig abschließenden Testziel, nicht bei Bambu Studio, OrcaSlicer oder gedit.
Eine Behebung würde das Zug-Objekt bis zur Freigabe durch GDK festhalten (Analyse
linux-einfrieren-analyse.md, Abschnitt 2); dies ist ausdrücklich nicht umgesetzt.
Touch-/Stift-Emulation der Maustaste und Wayland sind nicht geprüft.

Der Auftraggeber regeneriert Cargo.lock (Registry-Quelle/Prüfsumme von `drag`
entfallen) und führt die obigen Befehle in Docker sowie die nativen VM-Tests aus.
Die Crate-eigenen Tests können zusätzlich ihre unveränderten Upstream-Testabhängigkeiten
auflösen. Rust und native Interaktionen konnten im Sandkasten nicht geprüft werden.
Bei ausbleibendem GTK-Zugende gibt der App-Watchdog nur die App-Sitzung frei;
er entfernt keine GTK-Handler. Dies gehört zur VM-Prüfung auf wiederholbare Züge.

Rückweg: Upstream-Fix anhand der VM-Fälle prüfen, Patch-Eintrag und Kopie entfernen,
bei Bedarf Versionsanforderung ändern, Lockfile regenerieren, VM-Abnahme wiederholen.
