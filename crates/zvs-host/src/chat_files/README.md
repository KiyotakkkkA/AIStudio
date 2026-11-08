# chat_files

The per-conversation working folder: a private folder each conversation (or agentic session)
gets for files the model writes when no workspace folder is bound. The module root is
`../chat_files.rs`; this folder holds its tests. Skill scripts and process tools find the
folder through the `ZVS_CHAT_DIR` environment variable (`CHAT_DIR_ENV`).

## Functions

| Function                        | Purpose                                                                                       |
| ------------------------------- | --------------------------------------------------------------------------------------------- |
| `chat_dir(store, owner)`        | The owner's folder, created on demand; fails with `NO_CHAT` outside a conversation or session |
| `copy_chat_dir(store, a, b)`    | Copies the folder when a conversation is branched; a failed copy removes the partial target   |
| `remove_chat_dir(store, owner)` | Deletes the folder with the conversation                                                      |
| `chat_files(folder)`            | Every file as `(relative path with "/", size)`, depth-first, sorted by name per level         |
| `within_quota(folder, changes)` | Checks a pending set of writes and deletes against the quota before they happen               |
| `is_working(relative)`          | True when any path component starts with `_` (`WORKING_PREFIX`)                               |

## Quota algorithm (`within_quota`)

The quota is `CHAT_FOLDER_MAX_BYTES` (200 MiB) and `CHAT_FOLDER_MAX_FILES` (1000). A file tool
passes the changes it is about to make as `(path, Some(new size))` for a write and
`(path, None)` for a delete. The check builds the current `path → size` map, applies the
changes to it, and rejects with `LIMIT_EXCEEDED` (`CHAT_FOLDER_FULL`) when the resulting total
exceeds the byte limit, or when the change _adds_ files and the count exceeds the file limit.
Overwrites and deletes are therefore always allowed to bring a full folder back under the
limit, and the check happens before anything is written.

## Working files

Paths with a component starting with `_` (for example `_scratch/notes.md`) are the model's
working files. They stay in the folder but are not offered to the user as results: artifact
detection after a process run (`../commands/nodes.rs`) skips them (`is_working`).
