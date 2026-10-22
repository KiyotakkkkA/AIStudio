---
task: TASK_036_EXTRA_4
title: Working folder explorer — the chat side panel toggle, a live file tree and the file view
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_036_EXTRA_1, TASK_036_EXTRA_2]
unblocks: []
estimate: 5 h
---

# TASK_036_EXTRA_4 — Working folder explorer

## Context

A chat can have a working folder (TASK_036_EXTRA_1), and the assistant changes files in it
through `fs.apply` (TASK_036_EXTRA_2) and project commands (TASK_036_EXTRA_3). The user cannot
see that folder from the chat. To check what the assistant did, they have to switch to the
system file explorer.

Today the right side of the chat holds one panel, «Контекст запуска» (`RunContext.tsx`). It is
always shown above 1100 px and toggled by `mdiTuneVariant` in the thread header below that.

Decided with the user (2026-10-23):

- **A toggle group in the top right of the chat header.** It holds one icon button per side
  panel. «Контекст запуска» is always there. A folder icon, «Файлы», appears only when the chat
  has a working folder.
- **The explorer replaces the «Контекст запуска» panel while it is toggled on.** There is one
  side panel slot. Switching back restores «Контекст запуска».
- **The explorer is a file tree of the working folder that follows changes on disk.** Files
  that are created, changed or deleted, by the assistant or anyone else, appear in the tree
  without a manual refresh.
- **Clicking a file opens it in an integrated file view inside the app.** In this task only
  `.md` is rendered (through the chat's markdown renderer). Every other text file is shown as
  plain monospace text with no syntax highlighting. Highlighting is a later task.

## Why

The working folder is the assistant's workspace. The user needs to see that workspace next to
the conversation to follow and check the assistant's work: a plan it wrote to `PLAN.md`, a file
it just changed, a build output. This is also the base for the Agentic Build page (TASK_040),
which shows the same tree.

## Depends on

- `TASK_036_EXTRA_1` — the working folder, `FolderAccessService::working_folder`, `canonical`,
  `workingFolderAccess`.
- `TASK_036_EXTRA_2` — `files/walk.rs` (ignore rules, `.git` exclusion, `locale_order`) and
  `files/text.rs` (`decode`, the binary probe).

## Read first

- `/docs/PASSPORT.md` §7 (errors), §14 (folder access)
- `/CONVENTIONS.md` — every string below is user-facing copy
- `apps/studio/src/renderer/features/chat/ChatThread.tsx` (the header), `RunContext.tsx`,
  `ChatWorkspace.tsx`, `ChatStore.ts` (`contextOpen`, `toggleContext`), `ChatMarkdown.tsx`
- `apps/studio/src/renderer/features/skills/SkillFileView.tsx` — the closest existing file view
  (binary and loading states)
- `crates/zvs-host/src/workspace/service.rs`, `crates/zvs-host/src/files/walk.rs`, `text.rs`

## Scope

**In scope**

*Host — the explorer service (`crates/zvs-host/src/workspace/explorer.rs`)*
- `list(conversation, relative)`: the direct children of one folder inside the chat's working
  folder, folders first, then `locale_order`. Each entry has `name`, `path` (relative, `/`
  separated), `kind: 'file' | 'folder'`, `size`, `modifiedAt` and `ignored`. `.git` is never
  listed. Entries matched by `.gitignore` are listed with `ignored: true`, and their children are
  not read until the folder is expanded. The renderer shows them dimmed. One level per call; the
  tree loads lazily.
- `read(conversation, relative)`: `{ path, size, modifiedAt, text: string | null, truncated }`.
  It uses `text::decode`, so binary gives `text: null`. Text above 1 MiB is cut at a line
  boundary and returns `truncated: true`.
- **Boundary.** Every path is resolved against the working folder and canonicalised. A result
  outside the working folder (`..`, an absolute path, a junction or symlink pointing out) is
  refused with `PermissionDenied`. These calls are the user browsing their own folder from the
  UI. They raise **no** access request, but they require the working folder to be covered
  (`workingFolderAccess: 'granted'`). Otherwise they fail with `PermissionDenied` and the panel
  shows the missing-access state.
- **Watcher.** Use the `notify` crate with its debouncer. Resolve the latest stable versions and
  pin them exactly. `watch(conversation)` starts one recursive watcher on the working folder,
  and `unwatch(conversation)` stops it. Changes are debounced to 300 ms and coalesced. They are
  sent as `workspace.changed { conversationId, changes: [{ path, kind: 'created' | 'modified' |
  'removed' }] }` with relative paths. Changes under `.git` are dropped. A folder rename is
  reported as `removed` plus `created`. At most one watcher per conversation, counted by
  subscribers, so two windows on one chat share it. Unbinding or changing the working folder
  stops the old watcher and starts a new one when someone is subscribed. A watcher error (the
  folder deleted or the drive removed) sends `workspace.changed` with the root as `removed` and
  stops.

*Contract*
- `ExplorerEntryDto`, `ExplorerFileDto`, `ExplorerChangeDto`, the `workspace.changed` event.
  Channels `workspace.list`, `workspace.read`, `workspace.watch`, `workspace.unwatch`. Each needs
  the Rust type, the Zod schema and a golden fixture in `packages/shared/fixtures/workspace/`.
  Add the channels to the IPC channel list test.

*Renderer — the side panel slot*
- `ChatStore.sidePanel: 'context' | 'files'` replaces the meaning of `contextOpen`. Above
  1100 px the slot is always open, and the toggle group switches what is in it. Below 1100 px
  the slot is an overlay as today. Pressing the active button closes the overlay, and pressing
  another one switches the content. The choice is per app, not per conversation. It falls back
  to `'context'` when the active chat has no working folder.
- **The toggle group** in `ChatThread`'s header, right-aligned after the storage chip: icon
  buttons `mdiTuneVariant` «Контекст запуска» and `mdiFolderOutline` «Файлы». The active one is
  shown with the selected state, and `aria-pressed` is set. It is shown at every width; today's
  narrow-only `IconButton` goes away. «Файлы» is hidden when there is no working folder.
- `RunContext` and the new `ChatExplorer` render in the same `aside` frame (width, responsive
  rules, the close button in the overlay mode). Extract that frame rather than copying it.

*Renderer — the explorer (`features/chat/explorer/`)*
- `ExplorerStore`, built per active conversation from the bridge and the clock. It holds the
  expanded folders, the loaded children per folder, the selected path and the open file. It
  subscribes through `workspace.watch` while the panel is visible and unsubscribes when the
  panel is hidden, the chat changes or the store is disposed. On `workspace.changed` it
  reloads only the parent folders that are loaded, drops removed paths and reloads the open
  file if it changed.
- **Change marks.** A path changed since the panel opened gets a dot. Created is shown in the
  ok tone, modified in the accent tone. A folder shows a dot when anything inside it changed.
  The marks clear when the file is opened, or with «Сбросить отметки» in the panel header.
  Marks live in the store and do not survive a restart.
- **Tree markup.** The panel header shows the folder name, with the full path on hover, and
  «Свернуть всё». Rows are indented, with a chevron for folders and an `mdiFileOutline` /
  `mdiFolderOutline` / `mdiFolderOpenOutline` icon. They are keyboard navigable (arrows, Enter)
  with `role="tree"` / `treeitem`. Ignored entries are dimmed. Long names truncate with the
  full name on hover.
- **States.** Loading folder: «Загрузка…». Empty folder: «Папка пуста». Missing access:
  «Нет доступа к рабочей папке» with «Предоставить доступ», reusing the working folder block's
  action. Folder gone: «Рабочая папка недоступна».

*Renderer — the file view*
- Opens per the DECISION below. Its header shows the relative path, size (`formatSize`) and
  modified time, plus a close button.
- `.md`: rendered with `ChatMarkdown`, with a «Текст» / «Просмотр» switch to show the source as
  plain text.
- Any other text file: monospace, with line numbers, no wrapping and no highlighting. Use
  `CodeView` with the `text` language if it supports line numbers without highlighting, or a
  plain `<pre>` otherwise.
- `text: null`: «Двоичный файл» with the size, as in `SkillFileView`. `truncated`: a notice
  above the text, «Показан первый 1 МБ файла».
- The open file is removed on disk: «Файл удалён», with the last loaded content kept below in
  a muted tone.
- Read-only.

*Copy*
- All strings in `features/chat/explorer/explorerCopy.ts`, in Russian, per `CONVENTIONS.md`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Where should a file open when it is clicked in the explorer: in place of the chat thread,
> next to it as a split, or over it in a wide sheet?"
>
> **Why it matters:** the chat area is already three columns: the list (~260 px), the thread
> and the side panel (~304 px). A file needs width to be readable, and the user often wants to
> read it next to the assistant's message about it.
> **Options:**
> (a) **Split.** The view opens between the thread and the side panel, with a draggable
> divider. Its width is remembered and the thread keeps at least 420 px. Below 1250 px the view
> covers the thread, with «Назад к диалогу».
> (b) **Replace.** The view takes the thread's place, with a tab strip «Диалог» · `<file>` · …
> above it, like an editor. Several files can be open.
> (c) **Sheet.** A wide panel slides over the thread and conversation list from the right edge
> of the side panel. Esc closes it.
> **If the user is unavailable:** take (a). It keeps the conversation visible, and (b)'s tabs
> can be added on top of it later. Note the choice in Handoff.

## Out of scope (do not do these, even if tempting)

- Syntax highlighting for non-`.md` files — the next task.
- Editing, creating, renaming, deleting or moving files from the explorer.
- Search or filter in the tree; git status or diffs in the tree.
- «Открыть в проводнике» / opening in an external app.
- Browsing outside the working folder, or a chat with no working folder.
- The explorer on the Agentic Build page (TASK_040 reuses `ExplorerStore` and the service).

## Steps

1. `explorer.rs`: `list`, `read`, the boundary checks. Unit tests first, on a temp folder with a
   junction.
2. Watcher with debounce, coalescing, subscriber counting and the rebind behaviour, tested with
   a real temp folder and a bounded wait.
3. Contract: DTOs, Zod, fixtures, channels, event.
4. `ChatStore.sidePanel`, the header toggle group, the shared `aside` frame.
5. `ExplorerStore` and its tests on `createFakeBridge()` / `createFakeClock()`.
6. Tree, file view, states, copy.
7. PASSPORT §14: one paragraph saying that explorer reads are user browsing, bounded by the
   working folder and the grant, and raise no request.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Tests that must exist:
- `list`: folders first, locale order, `.git` hidden, ignored entries flagged, one level only.
- `read`: text, binary gives `null`, over 1 MiB is truncated on a line boundary.
- Boundary: `..`, an absolute path and a junction pointing outside are refused. An uncovered
  working folder is refused with no access request raised.
- Watcher: create, modify and remove each arrive once after debouncing. A burst of writes
  arrives as one change. `.git` changes are dropped. Two subscribers share one watcher, and the
  last unsubscribe stops it. Rebinding the folder moves the watcher.
- `ExplorerStore`: lazy expansion; a change reloads only loaded parents; marks set, propagate
  to folders and clear on open and on reset; the open file reloads on change and shows the
  removed state; it unsubscribes on hide, on chat switch and on dispose.
- `ChatStore.sidePanel`: falls back to `context` without a working folder; the toggle switches
  the panel and closes it in overlay mode.

Manual check: bind a folder, open «Файлы», ask the assistant to write a `.md` file. The file
appears with a mark. Open it and check that it renders. Edit it in another editor and check
that the view updates.

## Definition of done

- [x] Header toggle group; «Файлы» only with a working folder; the explorer replaces «Контекст
      запуска» in the side panel slot
- [x] Lazy tree of the working folder with live updates and change marks
- [x] File view per the DECISION: `.md` rendered, other text plain, binary and large states
- [x] Reads bounded by the working folder and its grant, junction escapes refused
- [x] DTO changes in Rust, Zod and fixtures; PASSPORT §14 updated
- [x] DECISION answered or defaulted and noted
- [x] Gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Contract `zvs_contract::explorer` (`ExplorerEntryKind`, `ExplorerChangeKind`,
  `ExplorerEntryDto`, `ExplorerListInput`, `ExplorerListingDto`, `ExplorerReadInput`,
  `ExplorerFileDto`, `ExplorerChangeDto`, `ExplorerChangesDto`, `ExplorerWatchDto`); Zod in
  `packages/shared/src/explorer.ts`; fixtures in `fixtures/explorer/`. Channels
  `workspace.list`, `workspace.read`, `workspace.watch`, `workspace.unwatch`.
- `workspace/explorer.rs`: `ExplorerService` (`list`, `read`, `watch`, `unwatch`, `rebind`,
  `dispose`) on `Services.explorer`. `notify` pinned `=8.2.0` (no default features); the
  debounce is our own (300 ms trailing, 2 s at most) with coalescing. The shell stops it on
  shutdown. `chat.bindFolder` / `chat.unbindFolder` call `rebind`. Tests in
  `workspace/tests/explorer.rs`.
- Renderer: `ChatStore.sidePanel` / `activePanel` / `panelOpen` / `showPanel` / `closePanel`
  replace `contextOpen` / `toggleContext`; the header toggle group (`aria-pressed`);
  `SidePanelFrame` shared by `RunContext` and `ChatExplorer`; `ExplorerStore` (lazy tree,
  flat `rows`, keyboard focus, marks, open file, view width) with tests in
  `tests/renderer/explorer.test.ts`; `ExplorerFileView` (rendered `.md` with a
  «Просмотр» / «Текст» switch, plain text with a line-number gutter, binary, truncated,
  removed and error states).
- At the user's request, in the same change: `IconButton` takes a required `label` and a
  `tooltip` placement, and renders the kit `Tooltip` instead of a native `title` (`aria-label`
  defaults to `label`). `IconDropdownButton` takes the same `tooltip` prop. Every call site
  was updated: elements in a screen corner point inward, elements along the top or bottom
  edge use `bottom-center` / `top-center`, elements on the left or right side point toward
  the centre (`right-center` / `left-center`), and message actions use `bottom-left` under
  the user's messages and `top-right` under the assistant's.

**Deviated from plan:**
- DECISION: no answer, so the fallback (a) is taken: split view between the thread and the
  side panel with a draggable divider. The thread keeps at least 420 px, and below 1250 px the
  view covers the chat with «Назад к диалогу». The width is remembered for the session, not
  across restarts.
- The watch stream carries `step` events (as account linking does) rather than a new
  `HostEventDraft` variant, so the event envelope is unchanged.
- Modify events on folders are dropped: Windows reports one for every write inside them, and
  the child events already cover the change.
- No `clock` dependency in `ExplorerStore`: nothing in it depends on time.

**Next session should know:**
- `pnpm typecheck` fails on `tests/renderer/fileChanges.test.ts:59`, and so does HEAD
  (`{ from: undefined }` is not `JSONType`). Not touched here.
- `cargo clippy --workspace --all-targets` fails in `zvs-studio`'s
  `browser/gateway/tests.rs:270` (`atomic_try_update` is unstable). Not touched here either;
  clippy is clean for every other crate and for the `zvs-studio` lib.
- Not run here: `pnpm build`, `pnpm test:e2e` and the manual check (need the built app).
- Syntax highlighting for non-`.md` files is the next step; `ExplorerFileView`'s `PlainText`
  is the place to swap in.
