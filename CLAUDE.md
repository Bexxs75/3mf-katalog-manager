# 3MF Katalog Manager: notes for Claude Code

Tauri 2 desktop app: Rust backend in `src-tauri/`, React + TypeScript frontend in `src/`, end-to-end tests in `e2e/`. Read [CONTRIBUTING.md](CONTRIBUTING.md) first; this file only adds what an agent tends to get wrong.

## Checks to run before calling something done

```bash
cd src-tauri && cargo test --no-default-features && cargo clippy --no-default-features --all-targets -- -D warnings
npx tsc --noEmit && npx vitest run
```

- Always pass `--no-default-features` to cargo unless Open CASCADE is installed. Code behind `cfg(step-preview)` is then **not compiled**: when you change types it uses, check that code by hand.
- Run tests against a throw-away catalog only (set `XDG_DATA_HOME` to a temp folder). Never point the app or a test at a real catalog: the database stores absolute paths and cleanup actions delete real files.
- Before a release build, delete `node_modules/.vite` and `dist` (stale Vite cache gives old builds).

## Conventions that are easy to miss

- Code comments are English and explain **why**. No ticket numbers, finding IDs, version numbers or review rounds in comments.
- Every user-visible string goes through `src/i18n/` and needs German, English, Spanish and French entries plus the type in `types.ts`. Do not hard-code text in components.
- Backend error messages are still German strings; translating them is a separate planned task. Follow the existing error patterns and do not build a new mechanism.
- README, CHANGELOG and the user guide are bilingual in one file, English first. Add changelog entries under `[Unreleased]`.
- Keyboard use and screen readers are a requirement, not polish: real buttons, visible focus, ARIA names, and no focus loss after an action.
- Touch only what the task needs. Do not redesign icons, branding or layout while fixing something else.

## Do not do without the maintainer's approval

- Push, merge into `dev/*` or `master`, create tags or releases, or change CI secrets and workflows.
- Add dependencies.
- Talk about a security fix in a public issue, commit message or changelog before it is released (use neutral wording such as "more robust against damaged files").
