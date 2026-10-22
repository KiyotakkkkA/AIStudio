---
task: TASK_036_EXTRA_1
title: Working folder and folder access — chat binding, access requests, the «Разрешения» page
phase: 6 — Tools, MCP and Skills
status: todo
depends_on: [TASK_036]
unblocks: [TASK_036_EXTRA_2, TASK_036_EXTRA_3]
estimate: 5 h
---

# TASK_036_EXTRA_1 — Working folder and folder access

## Context

No built-in tool can touch the user's files yet. TASK_036_EXTRA_2 adds file tools and
TASK_036_EXTRA_3 adds script and command execution. Both need two things first: a place the
assistant works in, and a record of which folders the user has opened to it. This task builds
that layer and its UI. It adds no tool.

Decided with the user (2026-10-22):

- **Working folder.** A chat may have one bound folder, its *working folder*. It is the base
  workspace: relative paths resolve against it, and later tools start processes in it. A new
  chat always starts with **no** working folder; the user attaches one when needed, from a small
  block above the composer. It can be changed or removed between turns, never while a turn is
  running.
- **Folder access is all-or-nothing.** A grant lets the assistant read and change files in that
  folder and all its subfolders. *What* it may do there is decided by the tool tiers on the
  «Инструменты» page, not by the grant.
- **Access is also possible outside the working folder**, but each new folder raises an access
  request modal. Answers: «Разрешить однократно», «Разрешить для этого диалога», «Разрешить
  всегда», «Запретить».
- **«Разрешить всегда»** stores a lasting grant on the folder. Binding that folder to any other
  chat later opens no modal.
- **Unit of a grant: the nearest parent folder** of the file being accessed. For a folder
  target (a listing) it is that folder. For a path that does not exist yet, it is the deepest
  ancestor that does exist, and the modal shows which folders will be created inside it.
- **A new «Разрешения» page** in the «Система» group of the rail lists every stored folder grant,
  each revocable at any time.
- **The working folder goes into the system prompt.**

## Why

File and command tools are what turn the chat into an assistant that can do work on the user's
machine, and they are the prerequisite for Agentic Build (TASK_040). They must not reach a folder
the user did not open to them, and the user must be able to see and withdraw every such grant.

## Depends on

- `TASK_036` — the last task of the phase; nothing in it is reused directly.
- Existing machinery: `PermissionService` and `ApprovalService` (`services/permissions.rs`,
  `services/approvals.rs`), the `StepContext` approval gate, `PickerPort`.

## Read first

- `/docs/PASSPORT.md` §1 (pages), §11 (approval gate), §14 (security; this task extends it)
- `/CONVENTIONS.md` — every string below is user-facing copy
- `crates/zvs-host/src/services/approvals.rs`, `crates/zvs-host/src/services/permissions.rs`
- `apps/studio/src/renderer/features/chat/ChatThread.tsx`, `ChatComposer.tsx`, `ChatApprovals.tsx`
- `apps/studio/src/renderer/app/navigation.ts`, `app/routes.ts`

## Scope

**In scope**

*Host — the folder access service (`crates/zvs-host/src/workspace/`)*
- `canonical(path)`: the real path, with junctions and symlinks resolved and the `\\?\` prefix
  stripped (`platform/long_path.rs`). Comparison is case-insensitive and on component boundaries,
  so `D:\work` does not cover `D:\workshop`.
- **Forbidden folders**, refused outright with no modal, and also refused as a working folder:
  a drive root, `%WINDIR%`, `Program Files`, `Program Files (x86)`, `%ProgramData%`, the bare user
  profile root (its subfolders are allowed), and every app folder from `AppPaths` (data, cache,
  logs, downloads, the browser work profile). A grant that would cover one of these, for example
  `C:\Users\<name>` covering the app data under it, is refused too.
- `grant_target(path)`: applies the nearest-parent rule above and returns the folder to ask for,
  plus the not-yet-existing tail to show in the modal.
- `ensure_access(context, conversation, paths)`: for each target folder, allowed when covered by
  a lasting grant, by a dialog grant of this chat, or by a once-grant held for the current tool
  call. Otherwise it raises **one** access request listing every uncovered folder and suspends on
  the step's approval gate. Answers apply to all folders in the request. A timeout or a closed
  modal is «Запретить». The tool call then fails with `PermissionDenied` and a Russian message
  the renderer maps by code.
- Tables (migration `0007_folder_access.sql`, schema and repository):
  `folder_grant(id, path, scope 'always' | 'conversation', conversation_id NULL, granted_at,
  last_used_at)`, with `conversation_id` set only for dialog grants and cascading on chat delete.
  A once-grant is held in memory for one tool call and never stored.
- `conversation.working_folder` (nullable text) in the same migration.

*Host — binding and the prompt*
- `chat.bindFolder` / `chat.unbindFolder` IPC. Binding a folder that is not covered raises the
  **binding** modal, which offers only «Разрешить для этого диалога», «Разрешить всегда» and
  «Отмена». A one-time grant makes no sense for a base folder. Binding a forbidden folder is
  refused. Refused while a turn of that chat is running.
- Folder choice reuses `PickerPort` with `VectorSourceKind::Folder`.
- System prompt (English, built in the chat service each turn): with a working folder, its
  absolute path, that relative paths resolve against it, and that any other folder needs the
  user's approval. Without one: that there is no working folder and every folder needs approval.
  When the working folder differs from the one the previous turn ran with, a one-line note says
  it changed. The folder each turn ran with is recorded for this comparison.
- Revoking a grant does not unbind chats. A bound chat whose folder is no longer covered reports
  that in its DTO (`workingFolderAccess: 'granted' | 'missing'`). The next access to it raises the
  standard access request.

*Contract*
- Conversation DTO: `workingFolder`, `workingFolderAccess`. Folder grant DTO and list/revoke IPC.
  The approval request (a free map today, `events.rs`) gains `kind: 'folder'` with
  `folders: [{ path, creates: string[] }]`. Rust type, Zod schema and golden fixture for each.

*Renderer*
- **Working folder block** above the composer in `ChatThread`: the folder name with its full path
  on hover, «Выбрать папку» when empty, change and remove actions, locked during a turn. When
  access is missing it shows «Нет доступа» in the warning tone.
- **Access request modal** for `kind: 'folder'` requests, built on `ConfirmModal`. It lists the
  folders and, under each, the subfolders that will be created. The tool and what it is about to
  do come from the request. Four actions as decided. Other approvals stay in `ChatApprovals`
  as they are.
- **«Разрешения» page**: route `/permissions`, rail item in «Система» between «Запуски и логи»
  and «Настройки», a shield icon. Built on `EntityListPanel` like the other list pages. Two
  sections, «Постоянные» and «В диалогах». Each row shows the path, the chats bound inside it,
  and when it was last used. «Отозвать» confirms through `ConfirmModal`. An empty state for no
  grants. Laid out so tool and site grants can be added later as tabs, but this task ships
  folders only.
- `PermissionsStore` with `createFakeBridge()` and `createFakeClock()` tests.

*Docs*
- PASSPORT §1: the page count and the «Система» row. §14: the folder access model, the forbidden
  folders, and that folder access and tool tiers are separate axes.

**Out of scope** (do not do these, even if tempting)
- Any tool that reads or writes files (TASK_036_EXTRA_2) or runs processes (TASK_036_EXTRA_3).
  This task is exercised in tests through a fake node calling `ensure_access`.
- Read-only grants: rejected by the user.
- Network (UNC) paths and removable media: refused as forbidden in this task.
- Tool and site grants on the «Разрешения» page.
- A working folder for scenarios and agentic sessions; TASK_040 reuses this service.

## Steps

1. `workspace/` module: `canonical`, the forbidden set built from `AppPaths` and the environment,
   `grant_target`, coverage checks. Unit tests first.
2. Migration, schema rows, repository, `FolderAccessService` with grant, revoke, list and
   `touch(last_used_at)`.
3. `ensure_access` on the approval gate, with the four answers and the once-grant held per tool
   call.
4. Conversation binding, IPC, DTOs, Zod schemas, fixtures.
5. System prompt section and the changed-folder note.
6. Renderer: the working folder block, the two modals, the «Разрешения» page and store.
7. PASSPORT §1 and §14.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Tests that must exist:
- Coverage: subfolders covered, sibling prefixes not, case-insensitive match, a junction pointing
  outside the grant is not covered.
- Each forbidden folder, and a grant that would contain the app data folder, is refused without
  a request.
- Nearest parent: a file asks for its folder, a listing asks for the folder itself, a missing
  path asks for the deepest existing ancestor and reports what will be created.
- Each of the four answers. «Однократно» does not cover the next tool call. «Для этого диалога»
  does not cover another chat. «Всегда» lets another chat bind with no request. A timeout denies.
- Several uncovered folders in one call raise one request.
- Binding: covered binds silently, uncovered asks with three options, forbidden is refused,
  refused while a turn runs. Revoking marks the bound chat `missing` and keeps the binding.
- Deleting a chat deletes its dialog grants.
- The system prompt with and without a working folder, and the changed-folder note.

Manual check: bind a folder, revoke it on «Разрешения», confirm the composer block shows
«Нет доступа».

## Definition of done

- [ ] Working folder can be bound, changed and removed between turns, and appears in the prompt
- [ ] Access requests with four answers, one modal per tool call, binding modal with three
- [ ] Forbidden folders refused with no modal, junction escapes caught
- [ ] «Разрешения» lists and revokes grants
- [ ] DTO changes in Rust, Zod and fixtures; PASSPORT §1 and §14 updated
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
