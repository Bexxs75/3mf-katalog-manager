<!--
Release notes template (EN first, then DE). Title: "vX.Y.Z — <English> / <Deutsch>".
How to release: run the "Release (draft)" workflow with the version → fill this draft's body from this template (fill in the <…> parts, keep the full changelog lists in the <details> blocks) → set the title as above → publish the draft.
The release workflow creates the draft with renamed, signed files, latest*.json and SHA256SUMS.txt.
Re-running it for the same version creates a second draft: delete the old one first.
latest.json only reaches users once the draft is published — that's what makes the update visible to the in-app updater.
-->
![3MF Katalog Manager demo](https://github.com/Bexxs75/3mf-katalog-manager/raw/master/docs/assets/demo.gif)

## Highlights

- **<Highlight 1>** — <one sentence>
- **<Highlight 2>** — <one sentence>
- **<Highlight 3>** — <one sentence>

## Who should update?

<Who benefits, e.g. "Everyone using the printer connection"; mention required actions such as a catalog backup before a database change.>

## Known limitations

- <e.g. packages are unsigned — see "Is it safe to install?" in the README>
- <feature limits of this release>

## Download

| System | Standard | With STEP preview |
|---|---|---|
| Windows | `3MF-Katalog-Manager-X.Y.Z-Windows-x64.msi` | `3MF-Katalog-Manager-X.Y.Z-Windows-x64-STEP.msi` |
| macOS (Intel + Apple Silicon) | `3MF-Katalog-Manager-X.Y.Z-macOS-universal.dmg` | `3MF-Katalog-Manager-X.Y.Z-macOS-universal-STEP.dmg` |
| Linux | `3MF-Katalog-Manager-X.Y.Z-Linux-x86_64.AppImage` | `3MF-Katalog-Manager-X.Y.Z-Linux-x86_64-STEP.AppImage` |

Not sure? Take the **Standard** file. Checksums: `SHA256SUMS.txt` · [Is it safe to install?](https://github.com/Bexxs75/3mf-katalog-manager#is-it-safe-to-install) · [Full changelog](https://github.com/Bexxs75/3mf-katalog-manager/blob/master/CHANGELOG.md)

<details><summary>All changes</summary>

<Added / Changed / Fixed from CHANGELOG.md>

</details>

---

## Highlights (Deutsch)

- **<Highlight 1>** — <ein Satz>
- **<Highlight 2>** — <ein Satz>
- **<Highlight 3>** — <ein Satz>

**Für wen lohnt sich das Update?** <…>

**Bekannte Einschränkungen:** <…>

**Download:** Windows `.msi`, macOS `.dmg`, Linux `.AppImage`; mit `-step` zusätzlich 3D-Vorschau für STEP-Dateien. Im Zweifel die Datei ohne `-step`. Prüfsummen: `SHA256SUMS.txt` · [Ist die Installation sicher?](https://github.com/Bexxs75/3mf-katalog-manager/blob/master/README.de.md#ist-die-installation-sicher) · [Vollständiges Changelog](https://github.com/Bexxs75/3mf-katalog-manager/blob/master/CHANGELOG.md#changelog-deutsch)

<details><summary>Alle Änderungen</summary>

<Hinzugefügt / Geändert / Behoben aus CHANGELOG.md>

</details>
