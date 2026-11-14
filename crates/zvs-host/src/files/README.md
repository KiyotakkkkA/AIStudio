# files

The file tools the model uses on the user's folders: `fs.list`, `fs.read`, `fs.search` and
`fs.apply`. Reads are free inside granted folders; every change goes through one atomic,
reviewable change set with a diff the user approves.

## Files

| File        | Contents                                                                                   |
| ----------- | ------------------------------------------------------------------------------------------ |
| `nodes.rs`  | The four kernel tools, grant checks, approvals, quota, output shaping                      |
| `roots.rs`  | `VirtualRoots` and `PathRoot`: `chat:` (conversation folder), `skill:<key>/…`, real paths  |
| `walk.rs`   | Directory walk with `.gitignore` support, depth/entry limits and cancellation              |
| `text.rs`   | `decode` (UTF-8 with BOM/CRLF detection, binary refusal), `encode`, SHA-256 `content_hash` |
| `plan.rs`   | `plan(changes)`: turns writes, edits, moves and deletes into before/after bytes and diffs  |
| `diff.rs`   | `diff_lines`: unified line diff with added/removed counts                                  |
| `commit.rs` | `snapshot`, `commit`: applies a plan atomically and rolls back on failure                  |
| `ledger.rs` | `ReadLedger`: which file versions a run has read                                           |

## Paths

`PathRoot` parses every path argument: `chat:` maps to the conversation's working folder
(`../chat_files/README.md`), `skill:<key>/<rest>` to a skill's folder (read-only), anything
else is a real path (absolute, or relative to the conversation's working folder) that must be
inside a granted folder. Outside a grant the tool asks for folder access instead of failing.

## Read-before-write

`fs.read` records `(run, path) → SHA-256 of the bytes read` in the `ReadLedger` (the last 64
runs, `LEDGER_RUNS`; paths compared case-insensitively). For existing files outside the
conversation's own folder, `fs.apply` refuses to edit or replace a file the run has not read
(`NOT_READ`), or whose current bytes no longer match what was read (`STALE`). The model therefore always edits the version it has seen, never a file the
user changed in the meantime.

## Pipeline: `fs.apply`

1. **Plan** (`plan.rs`). Changes are applied to an in-memory overlay of the disk, in order, so
   later changes see earlier ones:
   - `Write` sets new content (a new file gets LF, no BOM: `fresh_format`);
   - `Edit` replaces each `old` fragment, which must occur exactly once (`EDIT_NOT_FOUND`,
     `EDIT_AMBIGUOUS`, `EDIT_EMPTY`);
   - `Move` requires a free target unless it is deleted in the same set (`MOVE_TARGET_EXISTS`);
   - `Delete` removes.
     Binary or non-UTF-8 files cannot be changed (`BINARY_REFUSED`). Text keeps its original
     format: content is edited with LF and re-encoded with the file's BOM and CRLF choice. The
     result is a list of `PlannedFile { path, before, after }` plus a `FileDiffDto` per file.
2. **Check** dialog notes (`../notes/README.md`: every planned file under `chat:notes/` is
   validated and gets its host-managed identity and dates; one invalid note fails the batch),
   then the chat-folder quota for chat paths and the read ledger for existing files.
3. **Approve.** Steps 1 and 2 run in `prepare`; `access` declares `Effect::Files` with the
   combined diff and any uncovered folders, and the access pipeline (`../access`) reviews it
   through the execution policy or asks the user, folders and diff in one modal. `execute`
   commits only the admitted plan. A batch that only touches the conversation's own folder
   needs neither a folder grant nor diff confirmation; a batch that changes nothing asks
   nothing. `skill:` paths are refused (`SKILL_READ_ONLY`).
4. **Commit** (`commit.rs`), on a blocking thread:
   - `unchanged_since_plan` re-reads every file and aborts if anything changed while the user
     was looking at the diff;
   - `snapshot` writes every file's previous bytes and a `manifest.json` (path, snapshot name,
     deleted flag) to a per-step folder, so the change can be reverted later;
   - files are applied in order; writes go to a hidden `.name.<id>.zvs-tmp` file, are
     `fsync`ed and renamed over the target, so a reader never sees a half-written file;
     missing parent folders are created and remembered;
   - on the first failure, every file already applied is restored from its `before` bytes and
     created folders are removed; `CommitFailure` reports which file failed and whether the
     restore succeeded.

## Note reads

`fs.read` of a valid note under a chat base adds a `NoteSourceDto` (id, title,
`chat:notes/<name>`, SHA-256 of the bytes read) to `FsReadOutput.notes`; the chat tool loop
turns these into the answer's `note` sources. `fs.apply` reports each written note's canonical
path and id in `FsApplyOutput.notes` and in its summary text.

## Diff algorithm (`diff.rs`)

`diff_lines(before, after, limit)` trims the common prefix and suffix, then runs a classic
LCS table over the remaining middle (dynamic programming from the end, then a forward walk
emitting equal / delete / insert). If the middle would need more than `LCS_CELLS` (4 million)
cells, it falls back to "delete all, insert all" for that middle, which is still a correct diff
and keeps memory bounded. Operations are grouped into unified-diff hunks with context, the
text is cut at `limit`, and added/removed line counts are always exact.

## Listing and search (`walk.rs`)

The walk skips `.git` and applies `.gitignore` files found on the way down, each scoped to its
folder. Rules support `#` comments, `!` negation, a trailing `/` for folder-only rules and
anchoring when the pattern contains `/`; the last matching rule wins, as in Git. Depth, entry
count and the step's cancellation token bound the walk. `fs.search` reuses it and searches text
files only.
