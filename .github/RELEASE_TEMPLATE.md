<!--
Release notes template (EN first, then DE). Title: "vX.Y.Z — <English> / <Deutsch>".
How to release: set the version with `python3 tools/release_assets.py set-version X.Y.Z` and commit the version changes (otherwise `check-version` in release.yml fails) → run the "Release (draft)" workflow with the version → fill this draft's body from this template (fill in the <…> parts, keep the full changelog lists in the <details> blocks) → set the title as above → publish the draft.
The release workflow creates the draft with one STEP-enabled package family without a -STEP suffix, signed updater packages, identical latest.json and latest-step.json and SHA256SUMS.txt.
Re-running it for the same version creates a second draft: delete the old one first.
Test versions for testers go through preview.yml (release `preview`), not through this template.
Test releases use a numeric pre-release like 0.15.0-1: the Windows MSI rejects "rc.1". The workflow can only be
started by hand once release.yml is on the default branch.
Both manifests only reach users once the draft is published — that's what makes the update visible to the in-app updater.
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

| System | Download (STEP preview included) |
|---|---|
| Windows | `3MF-Katalog-Manager-X.Y.Z-Windows-x64.msi` |
| macOS (Intel + Apple Silicon) | `3MF-Katalog-Manager-X.Y.Z-macOS-universal.dmg` |
| Linux | `3MF-Katalog-Manager-X.Y.Z-Linux-x86_64.AppImage` |

Checksums: `SHA256SUMS.txt` · [Is it safe to install?](https://github.com/Bexxs75/3mf-katalog-manager#is-it-safe-to-install) · [Full changelog](https://github.com/Bexxs75/3mf-katalog-manager/blob/master/CHANGELOG.md)

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

**Download:** Windows `.msi`, macOS `.dmg`, Linux `.AppImage`; die STEP-Vorschau ist immer enthalten, die Dateinamen haben keinen `-STEP`-Zusatz. Prüfsummen: `SHA256SUMS.txt` · [Ist die Installation sicher?](https://github.com/Bexxs75/3mf-katalog-manager/blob/master/README.de.md#ist-die-installation-sicher) · [Vollständiges Changelog](https://github.com/Bexxs75/3mf-katalog-manager/blob/master/CHANGELOG.md#changelog-deutsch)

<details><summary>Alle Änderungen</summary>

<Hinzugefügt / Geändert / Behoben aus CHANGELOG.md>

</details>
