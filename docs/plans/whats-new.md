# Plan: "What's new" window after an update (target 0.18.2, Forgejo issue to be created)

Owner request (2026-11-17): like Dualfile 0.3.5, after every update show once on the first start a window listing what is new, improved and fixed, in the UI language. No mockup (owner saw the Dualfile version). Reference implementation for behaviour and look: Dualfile `src/components/dialogs/WhatsNewDialog.tsx`, `src/state/releaseNotes.ts`, `src/release-notes.ts` in /mnt/claude/projekte/dualfile (read-only reference; adapt to this codebase's components, styling and i18n, do not copy blindly).

## Behaviour
- New localStorage key `3mf-katalog-last-seen-version` (frontend only; same storage as the other UI settings).
- Decision on start (desktop and web frontend; in the web build it is harmless and uses the same logic):
  - **Fresh install** = `firstRunRef` true (no `3mf-katalog-setup-seen`): no window; set the marker to the current version.
  - Marker missing but not a fresh install (update from ≤ 0.18.1): show only the current version's notes.
  - Marker semver-lower than current: show every version `> marker` and `<= current`, newest first. Real semver compare (`0.18.10 > 0.18.9`), tolerate pre-release suffixes (`0.18.2-rc.1` counts as 0.18.2 for selection; never crash on garbage — treat unparsable marker as missing).
  - Equal or higher (downgrade): nothing.
- Marker is written on close (button, Escape, X), not on open. **Monotonic (lesson WN-01 from Dualfile):** only write if current is semver-higher than the stored marker or the marker is missing/unparsable. Manual reopen is independent of the auto filter: it always shows the current version's notes.
- Queue: the window waits for the first-run setup dialog and for `UpgradeNoticeDialog` (`upgradeNoticeSettled`). `PrintTechnologyPrompt`, `LinuxMenuPrompt` and the open-files requests (`openRequestsReady`) additionally wait until the what's-new window is settled, so start-up dialogs never stack. Add a `settled` signal analogous to `UpgradeNoticeDialog.onSettled`.
- Reopen any time: entry "Neuigkeiten" / "What's new" next to the existing version/update info in Settings (About/Update area, `SettingsPanel.tsx` / `UpdatePanel.tsx`) and in the Linux app menu / command or help menu if the app has one (find the existing place where "Keyboard tips" or "About" is offered and add it there).
- Dialog: heading "Neu in 3MF Katalog Manager X" (translated), groups New / Improved / Fixed, empty groups hidden; when several versions are shown, one section per version with its date, newest first; link "Full changelog on GitHub" → `https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/vX` opened via the existing `open_release_url` command (desktop) / normal link (web); button "Got it" with initial focus; Escape closes; scrollable body with the button always visible; follows all color schemes (Sand, White, Slate, Dark) via existing CSS variables; accessible (`role="dialog"`, `aria-modal`, labelled heading). Reuse the app's existing dialog shell/styles.

## Content
- New file `src/releaseNotes.ts` (or `src/lib/releaseNotes.ts`): per version `{ date, notes: { de, en, es, fr: { new: string[], improved: string[], fixed: string[] } } }`. Short user-facing sentences, not CHANGELOG text. Bundled, offline.
- Missing language → fall back to English (guard below makes this rare).
- Seed:
  - **0.18.2** (date `2026-11-17` placeholder, updated at release): New: "White color scheme selectable again (Settings → Appearance)"; "This window: after an update you see what's new". 
  - **0.18.1** (2026-11-16): "Light" became "Sand", new manual scheme "Slate" — so the guard passes for the current package version 0.18.1.
  - All four languages (de/en/es/fr), natural wording.
- Vitest guard: the version in `package.json` (and `src-tauri/tauri.conf.json`, must be equal) has an entry with non-empty text in all four languages.
- Backup: `3mf-katalog-last-seen-version` is not restored by catalog backup import (add to `IMPORT_SKIPPED_SETTINGS_KEYS`) and not exported if export filters keys. Test.
- CHANGELOG `[Unreleased]` entry in English and German, existing bilingual format (append to the existing White entry section).

## Files (expected)
`src/lib/releaseNotes*.ts` (+ pure selection module with tests), `src/components/WhatsNewDialog.tsx` (+ test), `src/App.tsx` (queue + reopen state), `SettingsPanel.tsx`/`UpdatePanel.tsx` (reopen entry), menu file if any, `src/i18n/{de,en,es,fr,types}.ts`, `src/hooks/useCatalogBackup.ts`, `CHANGELOG.md`.

## Tests
Selection logic (fresh install, missing marker, skipped versions, downgrade, garbage marker, pre-release, monotonic marker incl. regression: marker 0.18.3 → run 0.18.2, reopen manually and close → marker stays 0.18.3); dialog renders in DE and EN, close writes marker, Escape closes, focus on button; not shown on fresh install; queue (does not open while setup or upgrade notice is open; LinuxMenuPrompt waits for it); reopen from settings; guard test; backup skip.

## Not in scope
No version bump, no release, no wiki/website/screenshots (at release), no commit, no Rust changes unless strictly required.

## Proof
`npx tsc --noEmit && npx vitest run`
