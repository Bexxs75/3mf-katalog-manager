# Plan: White color scheme selectable again (Forgejo #1)

Owner request (2026-11-16/17): since 0.18.1 "Hell" became "Sand" (stored value `light`) and the old white light design can no longer be chosen. Bring it back as its own option. No mockup needed (owner decision).

## Decisions (fixed)
- New stored value **`white`**. Stored `light` keeps meaning Sand, so no existing user sees a change.
- Labels: DE "Weiß", EN "White", ES "Blanco", FR "Blanc" (new i18n key `themeWhite`, all four dictionaries + `src/i18n/types.ts`).
- Order in the settings segmented control: System, Weiß, Sand, Schiefer, Dunkel (`system`, `white`, `light`, `slate`, `dark`).
- "System" behaviour is unchanged: light system → Sand (`light`), dark system → Dunkel. `system` never resolves to `white` (same rule as slate). White is a manual choice only.
- Palette: exactly the old light palette from before commit 73dbce9f (`git show 73dbce9f^:src/styles/theme.css`, block `[data-app] { ... }`), as a new block `[data-app="white"]` with `color-scheme: light`. Do not change Sand, Slate or Dark values.

## Places to change (mirror how `slate` is wired; lessons of the 0.18.1 Codex review)
- `src/styles/theme.css`: new `[data-app="white"]` block.
- `public/theme-boot.css`: white boot colours (take the pre-73dbce9f light values: `git show 73dbce9f^:public/theme-boot.css`).
- `public/theme-init.js`: accept `white` as a fixed value; unknown values still behave like `system`.
- `src/hooks/useTheme.ts`: types `ThemeSetting`/`ResolvedTheme`, stored-value parsing, doc comment.
- `src/components/SettingsPanel.tsx`: label map + options order.
- `src/hooks/useCatalogBackup.ts`: backup import allowlist includes `white`.
- `src-tauri/src/startup.rs`: window background for `white` (old light background colour, see `git show 73dbce9f^:src-tauri/src/startup.rs`), allowlist in `save_window_theme`, reject `system` + `white` like `system` + `slate`; unit tests.
- Anything else that lists theme values (grep for `'slate'` / `"slate"`), including the web frontend if it has its own list.
- Tests: `theme.contrast.test.js` (add white block), `themeBoot.test.js` (white cases incl. both system preferences), `useCatalogBackup.test.ts` (white restored), `SettingsPanel.test.tsx` (option present, order), startup.rs tests.
- `CHANGELOG.md` `[Unreleased]`: one entry, German and English, following the file's existing bilingual format.

## Not in scope
No release, no version bump, no wiki/website/screenshots (done at release time), no commit.

## Proof
`npx tsc --noEmit && npx vitest run` (Rust tests are run by the host in Docker).
