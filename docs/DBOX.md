# DBOX data transfer

Implemented on 2026-11-17. The data-management section of Settings opens export and import
as `SettingsSubPage` views, retaining the Settings sidebar. Import uses the UIKit
`InputDropZone`. The renderer owns selection and presentation; the Rust host owns reading,
validation, archives and persistence.

## Scope

Categories are conversations, vector stores, scenarios, skills, memory, connections and
portable application settings. Connections include provider configuration, discovered model
metadata, MCP configuration and integration configuration. Model binaries and run history
are excluded. Conversation transfer includes messages, tool-call records, compactions and
managed attachments, work files, outputs and notes. Run snapshots are excluded.

Store transfer includes configuration and copies of indexed source documents. Index files
are rebuilt. Imported document identities survive rebuilding so dialog citations retain
their document references. Source folders and files that have never been indexed are not
copied. Their originals remain on the source computer.

Secrets, account sessions, cookies, OAuth state, grants, browser profiles, caches, downloaded
packages and device-specific settings are excluded. Secret bindings are removed; credentials
must be assigned again. Portable settings are an explicit allowlist maintained in the
transfer repository. Selecting settings replaces those values; other categories are added.
Imported providers, MCP servers, integrations, skills and scenarios start disabled.

## Format version 1

`.dbox` is a gzip-compressed TAR archive using the existing `flate2` and `tar` dependencies.
The first two regular members are `manifest.json` and `data.json`; the remaining members
are content-addressed files at `blobs/<lowercase-sha256>`. Duplicate content is written once.

The manifest contains `format: "zvs-dbox"`, `version: 1`, the SHA-256 of `data.json` and its
byte length. The data document contains the same format and version, `producerVersion`,
`createdAt`, selected `categories`, `records` and `files`. Record kinds and fields are frozen
by the explicit version-1 `TABLES` mapping in `data/repositories/data_transfer.rs`, independent
of the row readers and SQLite's schema introspection. JSON-valued fields are JSON strings;
other fields are strings, numbers or null. Future database changes must preserve this
mapping or add an explicit format reader/migration. Unknown format versions are rejected.
Field interpretation includes the record kind: `chat_compaction.summary` is plain text and
`chat_compaction.trigger` is `manual` or `auto`; `scenario_version.summary` and
`scenario.trigger` are JSON. Compaction summaries are preserved verbatim during import.

Each file descriptor contains `category`, its entity `owner`, a relative `path`, original
absolute and stored paths, SHA-256 and byte length. Original paths are used only to rewrite
references, never as import destinations. Destinations are derived from `StudioPaths` and
fresh entity IDs. Windows device names, alternate streams, absolute paths, traversal,
symlinks, unknown members, duplicate destinations and inconsistent sizes are rejected.
Archive extensions are handled as raw TAR entries, not interpreted as extraction commands.

Limits are 8 GiB of uploaded content, 8 GiB of expanded referenced files, 64 MiB of JSON,
100,000 records, 100,000 file descriptors, seven categories and 64 directory levels while
collecting files. Upload chunks contain at most 512 KiB before base64 encoding. The host
checks available disk space before upload, expansion, publishing and archive creation.
Checksums detect damage; they do not establish the author's identity. Archives are not
encrypted.

## Import and recovery

The studio-only `dataTransfer.*` channels implement inventory, export, upload creation,
contiguous chunk append, inspection, import and disposal. Inspection validates the archive
and freezes its blobs in an owned temporary directory. It returns counts, expanded sizes,
up to twenty object names per category, category dependencies and missing-dependency
information. Inspection does not change application records. Selection includes required
categories, and import rechecks dependencies and ongoing runs.

Import allocates new IDs, rewrites structural foreign keys and file references, and resolves
name/key/namespace conflicts with valid numeric suffixes. It does not replace arbitrary text
inside messages or prompts. Existing records are preserved except for explicitly selected
portable settings. The package hash and imported categories are recorded transactionally;
reimporting an already applied category from the same package is refused. Other categories
from that package may be imported separately. A transactional map from source IDs to imported
IDs preserves dependencies between those steps; the preview marks already imported categories.
Dialogs and compactions may retain references to removed providers, as in the source database.
These historical references do not block preview or import and do not create provider
configurations. Store embedding providers and message/dialog ownership remain required.

Before publishing files, the host writes and synchronizes a journal under
`data/transfer-journals`. New conversation and skill directories and an import-owned document
directory under `data/imports/<batch>` are created without replacing existing directories.
Files and database records are validated before commit. A transaction failure rolls back
records and removes the new directories. Startup recovery runs after migrations and before
normal service construction: an uncommitted journal removes its directories; a committed
journal preserves them and is retired. Imported document copies are accounted for in vector
storage usage and are independent of cache clearing.

Export collects explicit repository records and managed files with the database writer held
and refuses unfinished runs. File snapshots detect changes during copying. The archive is
written to a temporary file beside its destination, flushed and published on success. An
existing destination is replaced only by the completed archive. Export into the application
state root is refused.

## Verification

Rust tests cover the archive round trip, every category, reference rewriting, disabled
configuration, duplicate import, rollback, upload ordering, damaged archives, unsafe members,
startup recovery and studio-only IPC. An indexing regression test covers a restored store
without a table and preservation of document IDs. Shared golden fixtures bind the Rust and
Zod IPC types. Renderer tests cover selection, upload races, bounded chunks, explicit import,
errors and disposal. Presentation is checked manually; Playwright is not used.
