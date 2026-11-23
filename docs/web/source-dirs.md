# Source directory commands

The server uses the configured `KATALOG_SOURCE_ROOTS`. Requests use camelCase
arguments; absolute filesystem paths remain internal. These commands do not import,
copy, move, or delete source files.

| Command | Arguments | Result |
| --- | --- | --- |
| `list_source_dirs` | `{}` | `{roots: [{rootIdx, name}], entries: [], truncated: false}` |
| `list_source_dirs` | `{rootIdx, rel?: ""}` | Roots, selected root index and relative path, immediate entries, and `truncated` |
| `list_source_folders` | `{}` | Core folder DTOs with display-only `path`, plus nullable `rootIdx` and `rel` |
| `scan_source_folders` | `{}` | Core scan DTOs; `folderPath` is a display label and file `path` is a relative selection token |
| `get_source_check_on_start` | `{}` | Boolean |
| `add_source_folder` | `{rootIdx, rel}` | Folder DTO as above |
| `remove_source_folder` | `{id: string}` | Null |
| `reject_source_files` | `{paths: string[]}` | Number of rejected files |
| `reset_rejected_source_files` | `{}` | Number of cleared rejections |
| `set_source_check_on_start` | `{enabled: boolean}` | Null |

The final five commands require the existing authenticated write capability and
CSRF validation (`KATALOG_WRITE`). The first five are read commands. Unknown
arguments are rejected. No roots configured means an empty `roots` array when
requesting the root list.

A browser entry contains `rel`, `name`, `directory`, `fileCount`, and
`truncated`. Only directories and supported model files are returned. Each request
inspects at most `MAX_SCAN_ENTRIES` (5000) entries, including unsupported and
unreadable entries; recursive counts inspect at most `MAX_SCAN_DEPTH` (3)
subdirectory levels below the selected directory. When the inspection is limited,
counts are lower bounds and `truncated` signals this. Navigation into a deeper
directory starts a new bounded inspection. Enumeration order at the cutoff is
filesystem-dependent; the returned entries are sorted by relative path.

File selection tokens currently have the form `<rootIdx>/<relative path>`.
Clients must return the token from a scan unchanged when rejecting files. The
server resolves it under the current root configuration and validates it against
a fresh core scan. Tokens are not durable IDs across configuration changes.

Stored desktop source folders outside the configured roots remain in the database.
Their list/scan results are unreachable, their display path is only the final
name, and list results have null location fields. They can still be forgotten by
ID. An allowed but currently unavailable source keeps its root-relative location.

Every filesystem operation resolves directory handles without following links,
including the configured root and its ancestors. Symlinks and special files are
not offered. Hash reads open regular files through their parent handle and are
bounded by the checked size. The standard desktop adapter is unchanged.

The socket-free router tests are in
`crates/katalog-server/src/source_dirs_http_tests.rs`; run them with
`cargo test -p katalog-server --no-default-features --offline source_dirs`.
Source import and the browser UI are separate work.
