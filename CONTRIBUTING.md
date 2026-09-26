# Contributing

🇩🇪 **Deutsch:** [Mitwirken (Deutsch) weiter unten](#mitwirken-deutsch)

Thanks for your interest in 3MF Katalog Manager! This document collects the ways to help, whether or not you write code.

## Ways to help without writing code

- **Printer test reports.** If you own a 3D printer running Klipper/Moonraker (or want to help scope OctoPrint/Bambu Lab/PrusaLink support), the [printer test page](https://3mfkatalog.de/en/printer-test.html) (German: [druckertest.html](https://3mfkatalog.de/druckertest.html)) explains what to test and how to report the result.
- **Translations.** The UI ships in German, English, Spanish, and French. If you're fluent in one of these (especially ES/FR, which get less native review) or spot a wrong/awkward translation, see [docs/TRANSLATING.md](docs/TRANSLATING.md).
- **Testing the installation on your platform.** Try a fresh install on Linux, Windows, or macOS (see the [Releases page](https://github.com/Bexxs75/3mf-katalog-manager/releases)) and report anything that didn't work.
- **Bug reports.** Open a [GitHub issue](https://github.com/Bexxs75/3mf-katalog-manager/issues/new/choose) or post in [Discord](https://discord.gg/abfVNfFqu3).
- **Sample files.** 3MF or G-code files (only ones you're allowed to share) that trigger parsing bugs, unusual slicer metadata, or edge cases are useful test material — attach them to an issue or share them on Discord.

## Code contributions

### Prerequisites and dev commands

From the README's [Development](README.md#development) section:

Prerequisites: Node.js, the Rust toolchain (`cargo`), and the [Tauri system dependencies](https://tauri.app/start/prerequisites/) for your operating system. The default STEP-preview build additionally requires Open CASCADE 7.8 or 7.9 as dynamic system libraries, including development files.

```bash
npm install
npm run tauri dev
```

Without OCCT, or to build a variant without STEP preview:

```bash
npm run tauri dev -- -- --no-default-features
# or: npm run tauri build -- -- --no-default-features
```

Backend tests:

```bash
cd src-tauri
cargo test
```

Production build (type checking + Vite build):

```bash
npm run build
```

### Running tests

- **Frontend tests:** `npx vitest run` (or `npm test`, which runs the same command). Type checking alone: `npx tsc --noEmit`.
- **Rust tests:** `cd src-tauri && cargo test`. The default Cargo feature is `step-preview` (see `src-tauri/Cargo.toml`), which needs Open CASCADE and `cmake` to build. If you don't have OCCT set up, run `cargo test --no-default-features` instead — this is also what CI uses for Clippy and the Rust test job.

### Conventions used in this repo

- **Code comments are in English.**
- **User-facing text goes through i18n**, and needs an entry in all four language files (`src/i18n/de.ts`, `en.ts`, `es.ts`, `fr.ts`) — see [docs/TRANSLATING.md](docs/TRANSLATING.md) for details. TypeScript will fail to compile if a key is missing from any of them.
- **README, CHANGELOG, and the user guide are bilingual**, English first, German second (in the same file).
- **CHANGELOG.md follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)**, with an `[Unreleased]` section at the top that PRs should add their entry to.
- **Keep PRs small and focused** — easier to review, easier to revert if something's wrong.
- **Add tests for new behavior** where practical (Vitest for frontend, `cargo test` for backend).

### Where to start

Issues labeled [`good first issue`](https://github.com/Bexxs75/3mf-katalog-manager/labels/good%20first%20issue) or [`help wanted`](https://github.com/Bexxs75/3mf-katalog-manager/labels/help%20wanted) are good entry points.

### Security issues

Please **don't** open a public issue for a security vulnerability. Email **info@3mfkatalog.de** instead so it can be fixed before it's public. Details: [SECURITY.md](SECURITY.md).

---

## Mitwirken (Deutsch)

Danke für dein Interesse am 3MF Katalog Manager! Dieses Dokument sammelt die Möglichkeiten, mitzuhelfen – mit und ohne Code.

### Mithelfen ohne Code

- **Druckertest-Berichte.** Wenn du einen 3D-Drucker mit Klipper/Moonraker besitzt (oder bei der Eingrenzung von OctoPrint-/Bambu-Lab-/PrusaLink-Unterstützung helfen willst), erklärt die [Testseite](https://3mfkatalog.de/druckertest.html) (Englisch: [printer-test.html](https://3mfkatalog.de/en/printer-test.html)), was zu testen ist und wie man das Ergebnis meldet.
- **Übersetzungen.** Die Oberfläche gibt es auf Deutsch, Englisch, Spanisch und Französisch. Wenn du eine dieser Sprachen sicher beherrschst (besonders ES/FR bekommen weniger muttersprachliche Prüfung) oder eine falsche/holprige Übersetzung findest, siehe [docs/TRANSLATING.md](docs/TRANSLATING.md).
- **Installation auf deiner Plattform testen.** Probiere eine frische Installation auf Linux, Windows oder macOS (siehe [Releases-Seite](https://github.com/Bexxs75/3mf-katalog-manager/releases)) und melde, was nicht funktioniert hat.
- **Fehlerberichte.** Eröffne ein [GitHub-Issue](https://github.com/Bexxs75/3mf-katalog-manager/issues/new/choose) oder schreib im [Discord](https://discord.gg/abfVNfFqu3).
- **Beispieldateien.** 3MF- oder G-Code-Dateien (nur solche, die du weitergeben darfst), die Parsing-Fehler, ungewöhnliche Slicer-Metadaten oder Randfälle auslösen, sind hilfreiches Testmaterial – als Issue-Anhang oder im Discord teilen.

### Code-Beiträge

#### Voraussetzungen und Entwicklungsbefehle

Aus dem Abschnitt [Entwicklung](README.de.md#entwicklung) im README:

Voraussetzungen: Node.js, Rust-Toolchain (`cargo`), sowie die [Tauri-Systemabhängigkeiten](https://tauri.app/start/prerequisites/) für dein Betriebssystem. Für die standardmäßig aktivierte STEP-Vorschau wird zusätzlich Open CASCADE 7.8 oder 7.9 als dynamische Systembibliothek einschließlich Entwicklungsdateien benötigt.

```bash
npm install
npm run tauri dev
```

Ohne OCCT beziehungsweise für eine Fassung ohne STEP-Vorschau:

```bash
npm run tauri dev -- -- --no-default-features
# oder: npm run tauri build -- -- --no-default-features
```

Backend-Tests:

```bash
cd src-tauri
cargo test
```

Produktions-Build (Typprüfung + Vite-Build):

```bash
npm run build
```

#### Tests ausführen

- **Frontend-Tests:** `npx vitest run` (oder `npm test`, ruft denselben Befehl auf). Nur Typprüfung: `npx tsc --noEmit`.
- **Rust-Tests:** `cd src-tauri && cargo test`. Das einzige Standard-Cargo-Feature ist `step-preview` (siehe `src-tauri/Cargo.toml`), das Open CASCADE und `cmake` zum Bauen braucht. Ohne eingerichtetes OCCT stattdessen `cargo test --no-default-features` ausführen – das nutzt auch die CI für Clippy und den Rust-Testjob.

#### Im Repo übliche Konventionen

- **Code-Kommentare sind auf Englisch.**
- **Nutzertexte laufen über i18n** und brauchen einen Eintrag in allen vier Sprachdateien (`src/i18n/de.ts`, `en.ts`, `es.ts`, `fr.ts`) – Details in [docs/TRANSLATING.md](docs/TRANSLATING.md). Fehlt ein Schlüssel in einer der Dateien, schlägt die TypeScript-Kompilierung fehl.
- **README, CHANGELOG und Benutzerhandbuch sind zweisprachig**, Englisch zuerst, Deutsch danach (in derselben Datei).
- **CHANGELOG.md folgt [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)**, mit einem `[Unreleased]`-Abschnitt oben, in den PRs ihren Eintrag ergänzen sollten.
- **PRs klein und fokussiert halten** – leichter zu prüfen, leichter zurückzunehmen, falls etwas nicht passt.
- **Tests für neues Verhalten ergänzen**, wo sinnvoll (Vitest fürs Frontend, `cargo test` fürs Backend).

#### Wo anfangen

Issues mit dem Label [`good first issue`](https://github.com/Bexxs75/3mf-katalog-manager/labels/good%20first%20issue) oder [`help wanted`](https://github.com/Bexxs75/3mf-katalog-manager/labels/help%20wanted) sind gute Einstiegspunkte.

#### Sicherheitslücken

Bitte **kein** öffentliches Issue für eine Sicherheitslücke eröffnen. Schreib stattdessen an **info@3mfkatalog.de**, damit sie behoben werden kann, bevor sie öffentlich wird. Details: [SECURITY.md](SECURITY.md#sicherheitsrichtlinie-deutsch).
