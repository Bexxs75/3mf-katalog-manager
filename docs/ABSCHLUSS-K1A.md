# K1a implementation report / Abschlussbericht

## English

Advisory implementation report for independent review; no merge or release approval.
Work order SHA256 verified: `a071f80b74087c03253576bc59f249fd63bd81533a196116ac121fad3e121f8a`.
Checkout: `/mnt/claude/projekte/3mf-wt-k1a-finish`, branch `codex/k1a-finish`.
Base HEAD remains `3e84af4889e105bc891d943b60e067d941f5adb4`.
Incoming K1a tip: `8269442ee7d2645955ceb18f06d4e4d21b6bf7a0`.

### Changes

- A: applied the three-way merge content of `kern/k1a-vertrag`, preserving the current development changes. In `catalog_removal.rs`, the shared subtree CTE and bundle pruning both remain. In `printer_link.rs`, the extracted core views retain the development DTO fields (`failures`, `has_api_key`, `status`), model-only matching and confirmed slicer-report matching. Desktop-only older-history handling, retry wakeup, failure reasons and API-key arguments remain. Existing printer tests remain, with the fetch-limiter test relocated to the core.
- S7: the only `.github` edit adds `cargo test -p katalog-core --no-default-features` to `desktop-compile`, whose matrix is Windows/macOS.
- S8: reject `CONIN$` and `CONOUT$` through the existing case-insensitive basename check (including extensions). Replace every trailing ASCII dot/space in generated model/companion trash names with `_`, after truncation so truncation cannot expose an invalid suffix. Added three portable Rust tests covering reserved names, unchanged normal names/extensions, Unicode, both trash prefixes and truncation. Added English/German Unreleased changelog entries.

### Proof and limitations

- `npx tsc --noEmit`: exit 0, no output.
- `npx --no-install vitest run --configLoader runner`: exit 0; 362 test files passed, 4687 tests passed; duration 157.08 s.
- `git diff --check`: exit 0, no output. No merge conflict markers remain in source or workflows.
- Byte comparison against the incoming K1a tip: all 173 Golden files unchanged, including 165 files in `journal/`. This proves fixture preservation, **not** runtime Linux parity.
- Static comparison: all seven relocated printer DTO definitions match the development HEAD exactly. The relocated connection conversion and open-job listing bodies match after module-path substitutions. No original printer test/helper names were lost. Other automatic merge paths match the temporary Git merge result exactly; manual changes are limited to the two conflicts, core printer views and S8 names, plus CI/changelog/report additions.
- `cargo test -p katalog-core --no-default-features fs_portable::names::tests`: exit 127, `/bin/bash: cargo: command not found`. Tests were added before the implementation but no Rust red/green run was possible. Rust compilation, Clippy, runtime Golden comparison and native Windows/macOS execution remain unverified here. The agreed external Docker checks are still required.

### Captured proof output

`npx tsc --noEmit` produced no output and exited 0.
`npx --no-install vitest run --configLoader runner` exited 0:

```text
RUN  v5.0.1 /mnt/claude/projekte/3mf-wt-k1a-finish
Test Files  362 passed (362)
     Tests  4687 passed (4687)
  Start at  23:26:01
  Duration  157.08s (environment 48%, tests 35%, setup 6%, import 6%, transform 3%, worker 1%)
```

The runner also printed a jsdom performance suggestion (362 environments, 334.06 s accumulated environment time); no configuration was changed in response. Full session logs: `/tmp/k1a-tsc.log`, `/tmp/k1a-vitest.log`.

### Denied actions and deviations

1. No commits, pushes, tags or publication. The latest user instruction overrides the plan's per-step/merge commits.
2. `git merge --no-commit --no-ff kern/k1a-vertrag` failed with exit 128: Git could not create `ORIG_HEAD.lock` in the original checkout's read-only Git metadata directory. No permission escalation was attempted. Proposed and applied alternative: calculate the merge without committing in `/tmp/k1a-merge-work` (a local shared clone), then copy only its changed file content into this checkout and resolve the conflicts here. No original checkout source was edited. This delivers the merged **content**, not merge ancestry or a staged merge; the coordinator must handle integration later. HEAD and this checkout's index remain unchanged.
3. The plan's `fs/names.rs` now lives at `crates/katalog-core/src/fs_portable/names.rs` after S1; changes were made there, with existing `fsops` re-exports retained.
4. No Golden files were regenerated or edited. The existing journal fixtures use ordinary names, not the S8 console-device/trailing-suffix cases. Those cases are covered by new portable assertions. `FSOPS_GOLDEN_UPDATE=1` generation cannot run without Cargo; no generated result is claimed or hand-written replacement supplied. An external parity run must confirm that no existing Golden updates are needed.

## Deutsch

Beratender Umsetzungsbericht für die unabhängige Gegenprüfung, keine Merge- oder Release-Freigabe.
A ist als zusammengeführter Dateiinhalt umgesetzt; S7 und S8 sind ergänzt. Bündelbereinigung, Druckerstatus/API-Schlüssel, ältere Historie, Fehlergründe und bestätigte Slicer-Zuordnungen bleiben erhalten. Die sieben verlagerten DTO-Definitionen und die konfliktbetroffenen Funktionskörper wurden gegen den Entwicklungsstand verglichen. Die übrigen automatisch zusammengeführten Dateien entsprechen dem Git-Merge-Ergebnis.

TypeScript: Exit 0 ohne Ausgabe. Vitest: exit 0; 362 test files passed, 4687 tests passed; duration 157.08 s.
Alle 173 Golden-Dateien sind bytegleich zum K1a-Eingang, davon 165 Journaldateien. Dies ist kein Laufzeit-Paritätsnachweis. Rust-Tests, Clippy und Golden-Ausführung sind wegen fehlendem Cargo offen; auch die drei neuen Tests konnten nicht ausgeführt werden. Windows/macOS müssen über CI geprüft werden.

Der echte Merge scheiterte am schreibgeschützten `ORIG_HEAD.lock` im Git-Verzeichnis des ursprünglichen Checkouts. Stattdessen wurde der Merge in einer temporären lokalen Kopie ohne Commit berechnet und nur der Dateiinhalt übernommen. Kein Original-Quelltext wurde geändert. Merge-Historie und Staging bleiben beim Koordinator; HEAD und Index dieses Checkouts sind unverändert. Die Namensregeln liegen nach S1 in `fs_portable/names.rs`. Golden-Dateien wurden weder neu erzeugt noch von Hand angepasst; die S8-Sonderfälle sind neue portable Tests. Keine Commits, kein Push, kein Tag, keine Veröffentlichung.

## Changed paths / Geänderte Pfade

Includes imported files relative to the original HEAD; the deleted `fsops/names.rs` is relocated to `fs_portable/names.rs`. All paths are relative to this checkout.

- `.github/workflows/ci-checks.yml`
- `CHANGELOG.md`
- `crates/katalog-core/src/db/folder_subtree.rs`
- `crates/katalog-core/src/db/mod.rs`
- `crates/katalog-core/src/fs_portable/backend.rs`
- `crates/katalog-core/src/fs_portable/capabilities.rs`
- `crates/katalog-core/src/fs_portable/capabilities_tests.rs`
- `crates/katalog-core/src/fs_portable/mod.rs`
- `crates/katalog-core/src/fs_portable/names.rs`
- `crates/katalog-core/src/fs_portable/path.rs`
- `crates/katalog-core/src/fs_portable/path_tests.rs`
- `crates/katalog-core/src/fs_portable/recovery.rs`
- `crates/katalog-core/src/fs_portable/tests.rs`
- `crates/katalog-core/src/fsops/carry.rs`
- `crates/katalog-core/src/fsops/companion_add.rs`
- `crates/katalog-core/src/fsops/companion_edit.rs`
- `crates/katalog-core/src/fsops/context.rs`
- `crates/katalog-core/src/fsops/folders.rs`
- `crates/katalog-core/src/fsops/journal.rs`
- `crates/katalog-core/src/fsops/mod.rs`
- `crates/katalog-core/src/fsops/names.rs`
- `crates/katalog-core/src/fsops/operations.rs`
- `crates/katalog-core/src/fsops/parity_error_tests.rs`
- `crates/katalog-core/src/fsops/parity_golden_tests.rs`
- `crates/katalog-core/src/fsops/parity_path_tests.rs`
- `crates/katalog-core/src/fsops/parity_sql_tests.rs`
- `crates/katalog-core/src/fsops/paths.rs`
- `crates/katalog-core/src/fsops/trash_carry.rs`
- `crates/katalog-core/src/lib.rs`
- `crates/katalog-core/src/printer_link/mod.rs`
- `crates/katalog-core/src/printer_link/views.rs`
- `crates/katalog-core/src/upload_fs.rs`
- `crates/katalog-core/src/upload_fs/backend.rs`
- `crates/katalog-core/src/upload_fs/backend_tests.rs`
- `crates/katalog-server/src/fileops.rs`
- `crates/katalog-server/src/invoke.rs`
- `docs/ABSCHLUSS-K1A.md`
- `src-tauri/src/commands/backup.rs`
- `src-tauri/src/commands/catalog_removal.rs`
- `src-tauri/src/commands/folder_trash.rs`
- `src-tauri/src/commands/printer_link.rs`

### Golden fixtures imported unchanged / Unverändert übernommene Golden-Dateien

- `crates/katalog-core/testdata/fsops_golden/error/variants.txt`
- `crates/katalog-core/testdata/fsops_golden/journal/INDEX.txt`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.at_after_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.at_after_fsync_identity_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.at_after_rename_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.at_after_temp_create_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.at_after_temp_inode_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.at_mid_write_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.at_after_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.at_after_fsync_identity_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.at_after_rename_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.at_after_temp_create_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.at_after_temp_inode_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.at_mid_write_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_add_into_existing_folder.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_purge.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_purge.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_purge.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_purge.Unlinked.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_purge.at_after_unlink_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_purge.at_before_unlink_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_purge.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_restore.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_restore.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_restore.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_restore.at_after_companion_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_restore.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_restore.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash.at_after_companion_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash_last_file.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash_last_file.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash_last_file.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash_last_file.at_after_companion_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash_last_file.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/companion_trash_last_file.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_delete_of_missing_trash_file.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_delete_of_missing_trash_file.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_delete_of_missing_trash_file.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_move_to_no_folder.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_move_to_no_folder.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_move_to_no_folder.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_trash_of_missing_file.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_trash_of_missing_file.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/dbonly_trash_of_missing_file.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently.Unlinked.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently_with_companions.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently_with_companions.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently_with_companions.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently_with_companions.Unlinked.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently_with_companions.at_after_unlink_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently_with_companions.at_after_unlink_1.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_delete_permanently_with_companions.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.at_after_companion_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.at_after_companion_1.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.at_after_mkdir_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_move_with_companions.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case.TmpRenamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.TmpRenamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.at_after_companion_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.at_after_companion_1.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.at_after_mkdir_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_case_with_companions.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.at_after_companion_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.at_after_companion_1.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.at_after_mkdir_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_rename_with_companions.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.at_after_companion_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.at_after_companion_1.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.at_after_mkdir_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_restore_with_companions.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash_with_companions.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash_with_companions.CompanionsMoved.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash_with_companions.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash_with_companions.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash_with_companions.at_after_companion_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash_with_companions.at_after_companion_1.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash_with_companions.at_before_commit_0.json`
- `crates/katalog-core/testdata/fsops_golden/journal/file_trash_with_companions.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_create.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_create.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_create.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_create.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_move.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_move.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_move.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_move.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_move_with_models.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_move_with_models.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_move_with_models.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_move_with_models.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_remove.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_remove.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_remove.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_remove_with_models.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_remove_with_models.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_remove_with_models.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_rename.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_rename.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_rename.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_rename.end.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_rename_with_models.Committed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_rename_with_models.Prepared.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_rename_with_models.Renamed.json`
- `crates/katalog-core/testdata/fsops_golden/journal/folder_rename_with_models.end.json`
- `crates/katalog-core/testdata/fsops_golden/paths/differential_digest.txt`
- `crates/katalog-core/testdata/fsops_golden/paths/fs_context_new_table.txt`
- `crates/katalog-core/testdata/fsops_golden/paths/lexical_table.txt`
- `crates/katalog-core/testdata/fsops_golden/paths/locate_resolve_relative_table.txt`
- `crates/katalog-core/testdata/fsops_golden/recovery/rename_and_move.txt`
- `crates/katalog-core/testdata/fsops_golden/sql/available_behaviour.txt`
- `crates/katalog-core/testdata/fsops_golden/sql/prefix_behaviour.txt`
