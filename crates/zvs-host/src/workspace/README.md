# workspace

Folder access («Папки»): which folders the model may read and change, how access is asked
for and remembered, which folders can never be granted, and the conversation file explorer
with live updates. Every file, process and document tool resolves paths through here.

## Files

| File          | Contents                                                                                   |
| ------------- | ------------------------------------------------------------------------------------------ |
| `service.rs`  | `FolderAccessService`: working folder, grants, revoke, `covered`, `plan`, `ensure_access`  |
| `paths.rs`    | Path normalisation and comparison, `covers`, `ForbiddenFolders`, removable-drive detection |
| `explorer.rs` | `ExplorerService`: list and read inside a conversation's root, file watching with debounce |

## Path comparison (`paths.rs`)

Paths are canonicalised, then compared as lists of components, lowercased (Windows paths are
case-insensitive) with `.` dropped. `covers(grant, target)` is true when the grant's
component list is a prefix of the target's, so `C:\Work` covers `C:\Work\app\src` but not
`C:\Workshop`. A plain string prefix check would get that wrong.

## Grants

A grant is a folder plus a scope: `conversation` (only that dialog) or `always`.
`folder_access(folder, grants, conversation)` says whether a working folder is granted.
When several folders are requested at once, `minimal` keeps only the outermost ones, so a
request never lists a folder already covered by another in the same request.

`plan` resolves the targets of a call into a `FolderAccess`: the resolved paths, the minimal
uncovered folders and the grants it used. A node declares the uncovered folders as resources in
its `access` (`FolderAccess::describe`), and the access pipeline raises the folder approval
before the call runs; `ensure_access` does the same for a folder met during `execute`, through
`access::grant`. The service's `Attendant` records the pending request and turns the user's
answer (this conversation, always, once, or deny) into a grant or a refusal. The CLI shows the
same request in its access prompt.

## Forbidden folders (`ForbiddenFolders`)

These are refused even if the user tries to grant them:

- network paths and drive roots (`C:\`);
- anything inside Windows, Program Files (all variants), ProgramData, and the app's own data
  folders;
- any folder that _contains_ one of those, or that contains the user profile / home folder
  itself (granting `C:\Users\me` or `C:\Users` would expose everything under it; subfolders
  such as `C:\Users\me\Projects` are fine);
- anything on a removable drive.

## Explorer (`explorer.rs`)

`root(conversation)` is the conversation's bound working folder; it fails when none is
bound (`NO_WORKING_FOLDER_BOUND`) or the folder is no longer granted. `list` and `read` take
relative paths, canonicalise them and refuse anything that resolves outside the root (for
example through `..` or a link). `watch` starts a filesystem watcher (`notify`) and emits change
events on the conversation's stream; bursts are coalesced with a 300 ms debounce and flushed
at least every 2 s, so a build that touches thousands of files produces a handful of
refreshes. `rebind` moves the watcher when the working folder changes; `unwatch` and
`dispose` stop it.
