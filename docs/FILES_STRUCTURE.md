# Proposed Application File Structure

Status: proposal; not implemented.

This document proposes how ZVS AI Studio should locate and manage application files on
Windows. It does not change the current architecture. [PASSPORT.md](PASSPORT.md) remains
binding; its storage rules must be updated as part of an approved implementation.

## Current layout

The application currently obtains one root through Tauri's `app_data_dir()`. On Windows,
the root is `%APPDATA%\com.zvs.aistudio`. `StudioPaths` derives the database, logs,
backups, cache, vector stores, downloads and conversation directories from that root.
Additional service directories are constructed in the shell bootstrap code.

This mixes durable user data, installed resources, browser profiles and disposable files
under Roaming. It also distributes path decisions across the application.

## Location policy

Use `%LOCALAPPDATA%\ZVS\AIStudio` as the default application state root. Keep application
binaries in the installation directory, separately from writable application state.

Resolve Windows locations through Tauri or Windows Known Folder APIs. Environment-variable
paths in this document describe their usual locations; they are not instructions to
construct paths from a user name or a hard-coded drive letter.

### Local and Roaming

Local contains the working database, installed resources, indexes, browser profiles,
caches, temporary files, logs and automatic backups. These files may be large, depend on
the current machine, or contain state that requires coordinated migration.

Do not use Roaming initially. Preferences already have a source of truth in SQLite;
splitting them between SQLite and a roaming configuration file solely to populate both
locations would introduce unnecessary synchronization rules.

If cross-machine preference transfer becomes a product requirement, a small, explicitly
defined set of portable preferences may use Roaming. Candidate values include appearance
and response-style preferences. Local paths, credentials, browser sessions, device
configuration and machine-specific grants must remain local. Define ownership and conflict
handling before introducing a second settings store.

Provide explicit export and import for user-controlled transfers. Placing files in Roaming
is not the application's backup or synchronization mechanism.

### Installation directory

A per-user installation may use `%LOCALAPPDATA%\Programs\ZVS AI Studio`:

```text
ZVS AI Studio/
├── zvs-studio.exe
├── zvs-jobd.exe
├── cli/
│   └── aistudio.exe
└── ... bundled application resources
```

The installer owns this directory. Updates replace program files without relocating user
data. Downloaded runtimes and models belong in the state root, not beside the executable.

## Proposed state tree

```text
%LOCALAPPDATA%/ZVS/AIStudio/
├── data/
│   ├── studio.sqlite
│   ├── conversations/<conversation-id>/
│   ├── scenarios/<scenario-id>/
│   ├── dashboards/
│   ├── skills/
│   ├── policy/
│   └── terminal/
├── assets/
│   ├── models/
│   │   ├── embeddings/
│   │   ├── rerankers/
│   │   └── speech/
│   ├── runtimes/
│   └── packages/
├── indexes/
│   └── vectors/<store-id>/
├── webviews/
│   ├── studio/
│   └── browser/
├── cache/
│   ├── documents/
│   ├── thumbnails/
│   ├── favicons/
│   └── packages/
├── temp/
│   ├── downloads/
│   ├── extraction/
│   └── executions/
├── logs/
│   ├── host/
│   ├── jobs/
│   └── streams/
└── backups/
```

Create directories only when their owning feature needs them. The tree defines ownership,
not a requirement to create empty directories or introduce new features.

## Directory responsibilities

| Directory            | Contents and lifecycle                                                                                                                        |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `data`               | Durable application and user data. Never removed by cache cleanup. SQLite remains the source of truth for settings and structured records.    |
| `data/conversations` | Attachments, retained tool outputs and generated files owned by a conversation.                                                               |
| `data/scenarios`     | Persistent files owned by a scenario.                                                                                                         |
| `data/dashboards`    | Saved dashboard and page artifacts.                                                                                                           |
| `data/skills`        | User-authored or adopted skill library content. Installed dependencies belong in `assets/packages`.                                           |
| `data/policy`        | User-managed policy packs. Existing database ownership of policy records remains unchanged.                                                   |
| `data/terminal`      | Terminal-specific files that need an independent file representation. Do not duplicate database settings here.                                |
| `assets/models`      | Completed model installations, grouped by purpose. Add categories when supported.                                                             |
| `assets/runtimes`    | Installed Node, Python, uv and other supported execution environments.                                                                        |
| `assets/packages`    | Installed tool and connector packages and their required dependencies.                                                                        |
| `indexes/vectors`    | Persistent vector-store data. Treat rebuilding as an explicit operation, since it may require unavailable sources, time or paid requests.     |
| `webviews`           | Separate WebView profiles for the application shell and integrated browser. Preserve their sessions during ordinary cleanup.                  |
| `cache`              | Automatically reproducible document representations, thumbnails, favicons and package download caches.                                        |
| `temp`               | Partial downloads, extraction staging and intermediate execution files. Retained results must be promoted to their owner's durable directory. |
| `logs`               | Diagnostic and stream logs, with explicit retention and rotation rules.                                                                       |
| `backups`            | Automatic recovery snapshots with a retention policy. A database-only snapshot is not a complete backup of user files.                        |

Secrets continue to use the host's existing protected storage mechanism. This proposal does
not introduce plaintext credential files or change the permission model.

## Downloads, installation and cleanup

A download is a lifecycle stage, not the permanent classification of a file:

1. Download into `temp/downloads` using a partial-file name.
2. Verify the completed download and extract into staging when necessary.
3. Publish the verified resource into its destination under `assets`.
4. Record the installation only after publication succeeds.
5. Remove staging files after success; retain resumable partial files only under a defined
   retry and expiry policy.

Use an atomic rename when source and destination share a volume. For another volume,
copy into a staging directory on the destination volume, verify it, then publish there.

Cache cleanup may remove only registered disposable roots. It must respect active tasks
and must not delete installed resources, browser sessions, vector stores or retained
execution results. Temporary execution directories must not hold the only copy of a
user-visible artifact.

Model and runtime removal are explicit uninstall operations. Log rotation and backup
retention are independent maintenance operations.

## Centralized path resolution

`StudioPaths` should be the host's single authority for application-owned paths. Resolve
the state root, installation resources and optional storage overrides during startup, then
pass concrete feature roots to services.

Services must not independently discover AppData or assemble their own top-level paths.
They may create validated child paths under the roots they receive, using existing ID and
path validation rules. The renderer continues to request operations through IPC; it does
not obtain direct filesystem access.

Storage accounting, cleanup, backup and migration must use the same resolved roots.
Classify nested paths without double-counting their sizes.

## Moving large resources

Allow models and vector-store data to use another local drive through explicit settings.
Keep the database and ordinary application state in Local by default.

Treat relocation as a managed operation: pause affected work, check the destination,
copy and verify the data, switch the recorded location, then remove the old copy only
after success. Persist the override before dependent services start. A missing external
location must produce an actionable error rather than silently creating an empty store.

## Development and test isolation

Development and production must never share writable state. Use a separate development
root, such as `%LOCALAPPDATA%\ZVS\AIStudio-Dev`.

E2E runs continue to use their explicit temporary `ZVS_DATA_DIR`. Every proposed directory,
including both WebView profiles and resource installations, must resolve beneath that
isolated root unless a test explicitly supplies another temporary location. Unit tests use
the repository's temporary-directory helpers.

Do not expand the production environment-variable override surface as a side effect of
this reorganization.

## Migration from the current layout

Implementation requires a versioned startup migration and a corresponding update to
`PASSPORT.md`, which currently places the database in Roaming.

1. Detect the legacy root and the destination before starting database-dependent services,
   background workers or WebViews. Coordinate with any existing application instance.
2. Back up the legacy state. Close database writers and account for SQLite WAL state;
   copying an actively written database file alone is insufficient.
3. Copy files into their new categories and verify the result. Preserve IDs, file ownership
   relationships and browser profiles; update application-owned stored paths as necessary.
4. Persist a migration version and completion marker. Recovery after interruption must be
   resumable and must not merge two independent databases automatically.
5. Start services against the verified destination. Retain the legacy copy for recovery
   until a defined cleanup policy permits its removal.

Do not treat a partially populated destination as a successful migration. Export, recovery
and uninstall behavior must clearly distinguish program files, durable data, installed
resources and disposable files.

## Windows references

- [Known Folder identifiers](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid)
- [Windows app restore and machine-specific data](https://learn.microsoft.com/en-us/windows/apps/develop/windows-app-restore)
