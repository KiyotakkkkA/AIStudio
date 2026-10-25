---
task: TASK_045_EXTRA_1
title: Tool surface consolidation — fewer, non-overlapping model tools and no leaked kernel jobs
phase: 8 — Documents and the agent browser
status: todo
depends_on: [TASK_045]
unblocks: [TASK_045_EXTRA_4]
estimate: 5 h
---

# TASK_045_EXTRA_1 — Tool surface consolidation

## Context

An audit on 2026-10-25 counted the built-in tools a chat turn can offer the model. There are
18, registered in `crates/zvs-host/src/tools/kernel_tools.rs` (`BUILTIN`) plus the two tools
the chat service adds itself (`chat.ask` in `services/chat_ask.rs`, `chat.memory` in
`services/chat_memory.rs`). Several of them do the same job on a different root:

| Group | Tools | Overlap |
| --- | --- | --- |
| Files | `fs.list`, `fs.read`, `fs.search`, `fs.apply` | — |
| Chat folder | `chat.file` (write / read / list) | the same operations as `fs.*` on one fixed folder, tier `auto` |
| Skills | `skill.load`, `skill.read`, `skill.script`, `skill.save` | `skill.read` is `fs.read` on a read-only folder; `skill.script` is `shell.run` with a skill path |
| Processes | `code.run`, `shell.run`, `skill.script` | three ways to start a process |
| Knowledge | `knowledge.list`, `knowledge.search`, `knowledge.read`, `vector.list`, `vector.search` | `knowledge_catalogue` is already in the system prompt, so `knowledge.list` repeats it; `vector.*` is `knowledge.*` without store scoping |
| Dialog | `chat.ask`, `chat.memory` | — |

Three kernel jobs leak into the chat tool picker: `vector.index`, `job.run` and
`download.fetch`. They carry `PermissionRequirement::Tool`, so `register_kernel_tools`
registers them as tools (`NOT_TOOLS` only excludes `llm.generate`), and
`ChatStore.selectableTools` (`apps/studio/src/renderer/features/chat/ChatStore.ts`) lists
every available tool. A user can enable them in a chat, and the model then gets a sidecar job
runner and a download queue as tools.

Measured spec sizes (name + description + parameters, from stored `prepare` step outputs):
`chat.file` 984 chars, `chat.ask` 882, `skill.script` 863, `chat.memory` 727, `skill.read`
501, `skill.load` 468. The `fs.*`, `code.run`, `shell.run` and `knowledge.*` descriptions are
longer (`code.run`'s description alone is about 900 chars). With everything enabled the tool
catalogue is an estimated 12–13k chars, resent on every round of a turn (up to 15).

Research supports cutting the count: RAG-MCP (arXiv 2505.03275) reports selection accuracy
falling from about 85 % with 5 tools to about 45 % with 20. The same research does **not**
support one tool with a large `action` enum: weak models make more argument errors as a
schema branches. This task merges by **namespace** (a path root), so each schema stays flat.

## Why

Overlapping tools are the main source of "which tool?" mistakes on weak and web models, and
each extra tool costs prompt tokens on every round. The leaked jobs are a correctness bug.

## Depends on

- `TASK_045` — base task of this extra (backlog placement only; no code dependency).

## Read first

- `/docs/PASSPORT.md` §14 (permission model and tiers) and §11 (kernel nodes) — binding
- `crates/zvs-host/src/tools/kernel_tools.rs`, `crates/zvs-host/src/tools/registry.rs`
- `crates/zvs-host/src/files/nodes.rs`, `crates/zvs-host/src/chat_files.rs`
- `crates/zvs-host/src/skills/nodes.rs`, `crates/zvs-host/src/skills/script.rs`
- `crates/zvs-host/src/commands/nodes.rs`, `crates/zvs-host/src/kernel/knowledge_nodes.rs`
- `crates/zvs-host/src/services/chat.rs` (`prepare`: `SKILL_TOOLS`, `KNOWLEDGE_TOOLS`, `CHAT_SCOPED_TOOLS`)
- `apps/studio/src/renderer/features/chat/toolSets.ts`, `toolCallCopy.ts`, `toolCallView.ts`

## Scope

**In scope**
- `vector.index`, `job.run`, `download.fetch` never registered as tools (add to `NOT_TOOLS`
  or an explicit internal flag); a migration removes their existing tool rows and any
  `enabledTools` entries that name them.
- Virtual roots for the file tools: `chat:` (the conversation's working folder, today
  `chat.file`) and `skill:<key>/` (a skill folder, read-only, today `skill.read`).
  `fs.read`, `fs.list` and `fs.apply` accept them; `fs.apply` refuses `skill:`.
- Tier by root: `chat:` paths keep `chat.file`'s `auto` behaviour; real paths keep today's
  folder access and approval rules. This lives in the permission resolver, not in a caller.
- `shell.run` accepts `argv[0] = "skill:<key>/scripts/<file>"`, resolves it through the same
  runtime selection as `skill.script` (uv for `.py`, Node for `.js`/`.mjs`/`.cjs`), defaults
  `cwd` to the chat folder, sets `ZVS_SKILL_DIR`, and keeps artifact detection.
- `knowledge.search` gains a read mode (`documentId` plus `section` or a chunk range) that
  replaces `knowledge.read`; `knowledge.list` and `vector.*` leave the model surface.
- Remove `chat.file`, `skill.read`, `skill.script`, `knowledge.read`, `knowledge.list` from
  `BUILTIN` and the automatic sets; old tool keys in stored conversations are migrated.
- Russian descriptions rewritten for the merged tools (CONVENTIONS.md); renderer copy for
  tool-call rows updated.
- Delete the unused `CHAT_TOOL_ROUNDS` in `services/chat_tools.rs`.

**Out of scope** (do not do these, even if tempting)
- Folding tools into one tool with an `action` enum.
- The injected-tool protocol and web adapters (TASK_045_EXTRA_2).
- The skill loading flow beyond the tool rename (TASK_045_EXTRA_4).

## ❓ DECISION

Ask the user before implementing, and record the answer in INDEX.md's decision table:

> Merge `chat.file`, `skill.read` and `skill.script` into the `fs.*` / `shell.run` tools
> through virtual roots (`chat:`, `skill:<key>/`) — (a) yes, all three; (b) only the
> read-only merge (`skill.read` → `fs.read`), keep `chat.file` and `skill.script`;
> (c) no merges, only remove the leaked jobs and the knowledge duplicates.

Fallback if unanswered: (c). It is the smallest change and the leaked jobs are a bug either way.

## Steps

1. Fix the leak first, with a test that `register_kernel_tools` never returns the three jobs
   and that the migration strips them from stored `enabledTools`.
2. Add a path-root parser shared by the file and command nodes (`chat:`, `skill:<key>/`, real
   paths) with tests for traversal, hidden names and `..` on every root.
3. Move the permission choice for `chat:` paths into the resolver; test that a `chat:` write
   never asks, a real-path `fs.apply` still asks, and a `skill:` write is refused.
4. Extend `fs.read` / `fs.list` / `fs.apply` and `shell.run`; port the `chat.file`,
   `skill.read` and `skill.script` tests onto the merged tools before deleting the old nodes.
5. Fold `knowledge.read` into `knowledge.search`; drop `knowledge.list` and `vector.*` from
   what a chat offers. Update citations handling in `chat.rs` (`knowledge_text`).
6. Update `SKILL_TOOLS`, `KNOWLEDGE_TOOLS`, `CHAT_SCOPED_TOOLS`, `toolSets.ts`, and the
   bundled skills that name the removed tools (`crates/zvs-host/skills/*/SKILL.md`).
7. Measure the tool catalogue size before and after with a unit test that prints the
   serialized `tool_specs_json` length for the full set, and record both numbers in Handoff.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check: in a chat with a working folder and all tools enabled, build a dashboard with the
bundled skill on a local CSV; confirm the picker no longer lists the three jobs and the model
completes the flow with the merged tools.

## Definition of done

- [ ] `vector.index`, `job.run`, `download.fetch` are not tools; migration tested
- [ ] Decision recorded; the chosen merges implemented with tests for every root
- [ ] `chat:` tier decided in the resolver, not in a caller
- [ ] Knowledge duplicates gone from the model surface
- [ ] Catalogue size before / after recorded in Handoff
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
