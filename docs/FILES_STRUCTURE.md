# Application File Structure

This document describes where AI Studio keeps its files on Windows, how each folder is
maintained, and how the application moves data from one layout version to the next.
[PASSPORT.md](PASSPORT.md) §9 holds the binding summary.

## Location policy

The Windows Local App Data known folder contains three sibling roots under `ZVS`.
`AI Studio Desktop` owns Desktop state, `AI Studio` owns shared resources, and
`AI Studio CLI` is reserved for the standalone CLI.

```text
%LOCALAPPDATA%/ZVS/
├── AI Studio Desktop/    Desktop settings, dialogs, profiles and indexes
├── AI Studio/            Shared skills, caches, credentials and discovery metadata
└── AI Studio CLI/        Standalone CLI settings and dialogs (TASK_088)
```

Program files and bundled resources stay in the installation directory. Tauri resolves
bundled resources; the host resolves the state layout independently through the OS known
folder. Environment-variable paths in this document describe their usual locations.

### Local and Roaming

Local holds everything: the working database, installed resources, indexes, WebView profiles,
caches, temporary files, logs and automatic backups. These files are large, machine-specific
or need coordinated migration.

Roaming is not used. Desktop preferences have one source of truth in SQLite. If cross-machine transfer
of preferences becomes a requirement, a small, explicitly defined set may move to Roaming, with
ownership and conflict handling defined first. Local paths, credentials, browser sessions,
device configuration and machine-specific grants stay local. Explicit export and import, not
Roaming, is the way to move data between machines.

### Installation directory

A new per-user installation defaults to `%LOCALAPPDATA%\AI Studio Desktop`. An upgrade
from ZVS AI Studio reuses its registered installation directory.

```text
AI Studio Desktop/
├── zvs-studio.exe
├── zvs-jobd.exe
├── cli/
│   └── aistudio.exe
└── resources/
    └── notification.png
```

The installer owns this directory, and updates replace it without touching user data. Files
that ship with the program are bundled resources (`bundle.resources` in `tauri.conf.json`),
read from `resource_dir()`; the application never copies its own files into the state root.
Downloaded runtimes and models belong in the state root.

## Shared state

```text
%LOCALAPPDATA%/ZVS/AI Studio/
├── layout.json
├── skills/<skill-key>/
├── cache/
│   └── packages/
│       ├── npm/
│       ├── uv/
│       └── pip/
└── metadata/
    └── credentials/
        ├── <credential-id>.json
        └── <credential-id>.json.lock
```

The sibling `AI Studio.lock` serializes shared initialization and metadata registration.

| Path in `AI Studio`     | Owner and cleanup                                                                                                                                                                                          |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `layout.json`           | The shared initializer records its schema version and the app-owned dialog roots. Desktop records its executable for CLI link discovery.                                                                   |
| `skills/`               | Both apps use the same installed skill files. Explicit skill removal deletes them.                                                                                                                         |
| `cache/packages/`       | Package managers share their download caches. These caches can be rebuilt. Desktop cache clearing leaves them available to other processes.                                                                |
| `metadata/credentials/` | The shared credential store owns provider references and DPAPI-sealed values. Unlinking an OpenAI account removes its credential. The OS locks are kept so peers continue to coordinate on the same files. |

Desktop reset and uninstall preserve shared state. OpenAI OAuth can use the shared store;
the Copilot client owns its credentials. Desktop owns browser sessions and the secrets
that depend on them. Desktop-private API keys remain in its encrypted Secrets store.

## Desktop state tree

The top level classifies files by lifecycle. `StudioPaths` in
`zvs-host/src/platform/paths.rs` resolves these paths.

```text
%LOCALAPPDATA%/ZVS/AI Studio Desktop/
├── aistudio.toml
├── data/
│   ├── studio.sqlite
│   ├── conversations/<conversation-id>/
│   │   ├── history.jsonl
│   │   ├── attachments/
│   │   ├── work/
│   │   ├── outputs/
│   │   └── snapshots/<run-id>/
│   ├── scenarios/
│   ├── imports/<batch-id>/stores/<store-id>/
│   ├── transfer-journals/<batch-id>.json
│   ├── library/
│   │   ├── dashboards/
│   │   └── policy/
│   └── settings/
│       └── cli/
├── assets/
│   ├── models/
│   ├── embeddings/
│   ├── runtimes/<runtime-id>/
│   ├── toolchain/
│   ├── packages/
│   │   ├── mcp/
│   │   └── skills/
│   ├── environments/
│   │   └── computer/
│   └── extensions/
│       ├── enabled/<slug>/
│       └── disabled/<slug>/
├── indexes/
│   └── vectors/<store-id>/
├── profiles/
│   ├── shell/
│   └── browser/work/
├── cache/
│   ├── documents/
│   ├── thumbnails/
│   ├── favicons/
│   ├── capture/
│   └── voice/
├── temp/
│   ├── downloads/<download-id>/
│   ├── staging/
│   ├── runs/<run-id>/
│   └── boot/
├── logs/
│   └── streams/
└── backups/
```

Directories are created when their owning feature first writes to them. Folders for features
that store no files yet are added when they need them.

The sibling `AI Studio Desktop.lock` coordinates access to the state root. Hosts hold a shared OS lock
throughout their lifetime. Factory reset requires the exclusive lock and leaves its request
pending while another process holds it. The lock file is outside the directory reset removes.

The dialog history repository places `history.jsonl` next to the dialog's owned files. Its
writer locks live in `data/conversations/.locks/`, outside each dialog folder, so removing an
owned folder preserves the identity of the corresponding lock.

## Root rules

Desktop uninstall preserves the Desktop state tree by default. The app-data option removes it.

| Root       | Backup                 | Cleanup                                           | Relocation    |
| ---------- | ---------------------- | ------------------------------------------------- | ------------- |
| `data`     | Full                   | Never                                             | No            |
| `assets`   | No; reinstallable      | Never; explicit uninstall only                    | `models` only |
| `indexes`  | Optional               | Never; explicit rebuild only                      | Yes           |
| `profiles` | No                     | Never; explicit sign-out or reset only            | No            |
| `cache`    | No                     | «Очистить кэш», when no run is unfinished         | No            |
| `temp`     | No                     | At startup and with the cache, except `downloads` | No            |
| `logs`     | No                     | Rotation                                          | No            |
| `backups`  | Is the recovery source | Retention policy                                  | No            |

## Directory responsibilities

| Directory                      | Contents                                                                                                                        |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| `aistudio.toml`                | Versions of the on-disk schemas and the legacy migration record. See [Boot and schema migrations](#boot-and-schema-migrations). |
| `data/studio.sqlite`           | Source of truth for settings and structured records, together with its `-wal` and `-shm` files.                                 |
| `data/conversations`           | Attachments, working files, retained outputs with their `manifest.json`, and pre-run snapshots owned by a conversation.         |
| `data/scenarios`               | Persistent files written by scenarios.                                                                                          |
| `data/library`                 | User-authored or adopted dashboards and policy packs.                                                                           |
| `data/settings/cli`            | `theme.json` and `shortcuts.json`, read by `aistudio.exe`, which has no database access.                                        |
| `assets/models`, `embeddings`  | Downloaded model files. Discovery reads the folder, so the purpose is part of the location.                                     |
| `assets/runtimes`              | Engines such as llama.cpp and whisper.cpp, one folder per runtime id.                                                           |
| `assets/toolchain`             | The managed Node and uv with their Python installs, `npm-global`, uv tools.                                                     |
| `assets/packages`              | Downloaded MCP and skill packages.                                                                                              |
| `assets/environments/computer` | The computer-use sidecar scripts and their environment.                                                                         |
| `assets/extensions`            | Unpacked browser extensions; turning one off moves it from `enabled` to `disabled`.                                             |
| `indexes/vectors`              | LanceDB vector stores. Rebuilding may need unavailable sources, time or paid requests.                                          |
| `profiles/shell`               | WebView2 profile shared by the studio, computer and browser-chrome webviews.                                                    |
| `profiles/browser/work`        | The integrated browser's work profile with the user's site sessions.                                                            |
| `cache`                        | Disposable document representations, thumbnails, favicons, the capture profile and voice cache.                                 |
| `temp/downloads`               | Downloads in progress, one folder per download id, with resumable `.part` files.                                                |
| `temp/runs`                    | Working folders of script runs without a conversation folder. Kept results are copied into the conversation's `outputs`.        |
| `temp/boot`                    | WebView2 profile for the development boot and recovery previews. Actual file relocation uses a separate temporary profile.      |
| `logs`                         | Rotated `studio.jsonl` files and dated stream logs under `logs/streams`.                                                        |
| `backups`                      | Database snapshots taken before database migrations. A database snapshot is not a complete backup of user files.                |

Secrets use the host's existing protected storage. Nothing here introduces plaintext
credential files or changes the permission model.

## Standalone CLI state tree (TASK_088)

```text
%LOCALAPPDATA%/ZVS/AI Studio CLI/
├── config.toml
└── data/
    └── conversations/
        ├── .locks/
        └── <conversation-id>/
            ├── history.jsonl
            ├── attachments/
            ├── work/
            ├── outputs/
            └── snapshots/<run-id>/
```

`config.toml` will hold CLI-wide settings, workspace configuration, dialog aliases, access
rules, MCP configuration and the Desktop link. Per-dialog settings will live with their
dialog history. Sensitive provider and MCP values use protected storage. Desktop reset and
uninstall preserve this root. The shared layout records its dialog path for discovery.

TASK_087 reserves the path. TASK_088 creates the CLI state and defines its boot and cleanup
rules. The bundled thin client currently reads settings from Desktop's `data/settings/cli/`.

## Downloads and installation

A download is a lifecycle stage, not a location:

1. The sidecar writes `temp/downloads/<download-id>/<file>.part` and resumes from it.
2. It verifies the checksum and renames the file within the staging folder.
3. Installers run on the staged file: an engine or toolchain archive is extracted into its
   `assets` folder and the archive is deleted.
4. A file that no installer consumed is published into `assets` by rename; across volumes it is
   copied under a `.part` name, checked by size and renamed.
5. Only then does the download row turn `succeeded`, and the staging folder is removed.

At startup the download service removes staging folders that no queued, running, paused or
failed download owns, and those older than 14 days. Model and runtime removal are explicit
uninstall operations.

## Path resolution and stored paths

`StudioPaths::resolve` derives every folder from the state root and the installation
directory. The shell bootstrap passes concrete folders to services; services only create
validated child paths under the folders they receive. The renderer reaches files through IPC.

Columns that record where an installed resource lives are stored relative to their root:
`download.target_path` as `assets:models/<file>`, `skill.path` as `skills:<key>`.
`StoredPaths` encodes and decodes them; DTOs carry absolute paths. Moving a root changes the
root and rewrites no rows. Other text that mentions a path, such as message bodies and
settings JSON, keeps absolute paths and is rewritten by the migration step that moves it.

Storage accounting («Хранилище») reports one figure per category, built from these folders
without nesting, so nothing is counted twice.

## Moving large resources

`assets/models` and `indexes/vectors` are the roots that may later move to another local
drive through an explicit setting. The stored-path encoding is in place for that; the setting
and the managed move (pause affected work, copy, verify, switch, remove the old copy) are not
part of this layout version.

## Development and test isolation

Unpackaged builds use the `-Dev` variants of the app and shared directories. A debug
`aistudio.exe` reads its settings from `AI Studio Desktop-Dev`. E2E runs set `ZVS_DATA_DIR` to a
temporary Desktop root, with the shared and CLI paths beneath it. Unit tests use temporary
folders.

## Boot and schema migrations

A schema is a versioned on-disk format that the application owns outside SQLite. The first
schema is `project_struct`, this tree. The database schema is not one: its version stays in
the database and `zvs-host/src/data/migrate.rs` owns it.

### Expected versions

The versions a build expects are constants, `SCHEMAS` in `zvs-contract/src/layout.rs`. The
ordered steps of each schema are `STEPS` in `zvs-host/src/boot/mod.rs`. Expected versions are
never read from environment variables: an override could make a build treat its own layout as
older and migrate live data again, and environment variables reach `zvs-jobd`, MCP servers,
terminals and runtimes.

### Recorded versions

Each state root records its versions in `aistudio.toml`:

```toml
[schemas]
project_struct = 3
dialog_history = 1

[legacy]
source = 'C:\Users\<user>\AppData\Roaming\com.zvs.aistudio'
migrated_at = 1763000000000
```

The application creates and updates this file; the installer does not, because one
installation serves the production, development and E2E roots and a missing file has to be
handled anyway. `[legacy]` exists only until the old root is removed. The file holds no
settings. Writes go to `aistudio.toml.tmp` and are renamed over the file.

### Boot sequence

The boot module runs in the shell's `setup`, after the single-instance check and before the
logger, the database, the sidecar or any webview starts:

| State on disk                                                 | Action                                                      |
| ------------------------------------------------------------- | ----------------------------------------------------------- |
| No `aistudio.toml`, no `data/studio.sqlite`, legacy root      | Run every step from version 0                               |
| No `aistudio.toml`, otherwise                                 | Record the current versions and start                       |
| A recorded version lower than expected                        | Run the missing steps in order                              |
| All recorded versions equal to expected                       | Start                                                       |
| A recorded version higher than expected, or an unknown schema | Refuse to start against this root; show the recovery window |
| A malformed `aistudio.toml`                                   | Refuse to start; show the recovery window                   |

Steps run on a background thread behind a small progress window («Обновление структуры
данных») that shows the amount moved. The new version is recorded after each completed step,
so an interrupted migration resumes from the last completed one, and every step is safe to
repeat. A failed step opens the recovery window with the error and the location of the
untouched data. Only the studio application migrates: `aistudio.exe` reads its settings only
from a root whose recorded versions match its build, and `zvs-jobd` never reads the root, it
receives concrete paths from the host.

`project_struct` 1 → 2 converts supported regular dialogs to the `dialog_history = 1`
format. The boot step applies the pre-cutover migrations to a private SQLite snapshot,
stages JSONL and validates the projection. It publishes each history into its existing dialog
folder and resumes from histories already published. Unsupported dialog files move to
`backups/dialogs-layout-v1/`. The subsequent database migration backs up the live database
and replaces the old message, tool-call and compaction tables. Startup recovers pending
transfers and rebuilds the projection before opening Studio.

`project_struct` 2 → 3 renames `AIStudio` to `AI Studio Desktop` while holding the old
root's exclusive lock. It moves installed skills one entry at a time into the independently
versioned shared root, adopts OpenAI OAuth credentials after validating the DPAPI handoff,
and rewrites stored paths in SQLite. The SQLite transaction clears the old ciphertext
after the shared copy has been read back and verified. Desktop sends OpenAI OAuth reads
and refreshes to the shared credential store after the upgrade.

The version stays at 2 until these steps finish. A restart resumes from the relocated
Desktop root and can continue after skills were partially moved. An existing populated
Desktop destination or a conflicting shared skill stops the upgrade and preserves both
copies. Desktop's existing private cache is disposable. Shared package caches populate
on demand.

## Migration from the old layout

`project_struct` 0 → 1 moves the old `%APPDATA%\com.zvs.aistudio` root, and the shell's
profile in `%LOCALAPPDATA%\com.zvs.aistudio\EBWebView`, into this tree. It copies and never
modifies the source, so the old root is the backup of the step.

| Old location                                   | New location                                | Transfer   |
| ---------------------------------------------- | ------------------------------------------- | ---------- |
| `studio.sqlite` with `-wal` and `-shm`         | `data/studio.sqlite`                        | Copy, last |
| `conversations/`                               | `data/conversations/`                       | Copy       |
| `scenario-files/`                              | `data/scenarios/`                           | Copy       |
| `skills/`                                      | `data/library/skills/`                      | Copy       |
| `dashboards/`                                  | `data/library/dashboards/`                  | Copy       |
| `policy/`                                      | `data/library/policy/`                      | Copy       |
| `cli/`                                         | `data/settings/cli/`                        | Copy       |
| `downloads/models/`, `downloads/embeddings/`   | `assets/models/`, `assets/embeddings/`      | Hard link  |
| `downloads/runtimes/`                          | `assets/runtimes/`                          | Hard link  |
| `downloads/mcp/`, `downloads/skills/`          | `assets/packages/mcp/`, `.../skills/`       | Hard link  |
| `mcp-toolchain/`                               | `assets/toolchain/`                         | Hard link  |
| `mcp-toolchain/{npm-cache,pip-cache,uv/cache}` | `cache/packages/{npm,pip,uv}`               | Not moved  |
| `mcp-toolchain/uv/{tools,bin}`                 | `assets/toolchain/uv/{tools,bin}`           | Not moved  |
| `computer/`                                    | `assets/environments/computer/`             | Copy       |
| `browser/extensions/`, `extensions-off/`       | `assets/extensions/enabled/`, `disabled/`   | Copy       |
| `browser/work/`                                | `profiles/browser/work/`                    | Copy       |
| `%LOCALAPPDATA%\com.zvs.aistudio\EBWebView`    | `profiles/shell/EBWebView`                  | Copy       |
| `vectors/`                                     | `indexes/vectors/`                          | Copy       |
| `cache/`, including `code-runs` and `gallery`  | `cache/`, `temp/runs/`, `cache/thumbnails/` | Not moved  |
| `shell/notification.png`                       | Bundled resource                            | Not moved  |
| `logs/`                                        | `logs/`                                     | Copy       |
| `backups/`                                     | `backups/`                                  | Hard link  |

Hard links keep large, immutable files on disk once; a link that fails, across volumes for
example, falls back to a copy. Caches are rebuilt on demand. uv tool environments are not
moved because they embed absolute paths to their Python; they are reinstalled when used.

The step runs in this order:

1. Plan every file, then copy or link each through a `.migrating` name, skipping files that
   already match by size.
2. Verify that every planned file exists with its source size.
3. Copy the database with its WAL to `data/studio.sqlite.migrating`, rewrite stored paths,
   checkpoint, and rename it last. An existing `data/studio.sqlite` means this stage is done.
4. Record `project_struct = 1` and the `[legacy]` source and time.

The path rewrite stores `download.target_path` and `skill.path` root-relative and replaces old
absolute prefixes in every text column of every table, in native, JSON-escaped and slashed
form, only at a path boundary.

The old root is kept for 30 days. After that, a packaged build removes it, together with the
old shell profile, if nothing in it changed since the migration; a change means an older build
still uses it, and it stays. Development builds never remove it. Because the development root
is separate, a development build copies the old root into its own Desktop state and leaves
it for the packaged build.

## Windows references

- [Known Folder identifiers](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid)
- [Windows app restore and machine-specific data](https://learn.microsoft.com/en-us/windows/apps/develop/windows-app-restore)
