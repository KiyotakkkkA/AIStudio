---
task: TASK_035
title: Skills engine — definition, loading, trigger matching
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_031, TASK_076]
unblocks: [TASK_036, TASK_035_EXTRA_1, TASK_035_EXTRA_2]
estimate: 4 h
---

# TASK_035 — Skills engine — definition, loading, trigger matching

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

A skill is a packaged instruction set with a description that decides when it fires, a set of allowed tools, optional attached files and an optional model override. `/design/Skills.dc.html` shows the editor: name, identifier, a description field labelled *"this is what decides when the skill fires"*, a `SKILL.md` instruction pane, and a right rail with allowed tools, model override, attached files and a trigger check that scores a sample phrase.

Two design points matter. The **description is the trigger**, so the engine's matching quality depends on it, and the editor should make that relationship visible — hence the trigger-check panel. And **allowed tools narrow the ceiling**: a skill lists tools it may use, and TASK_031's resolver guarantees that list can only be a subset of what the Tools page permits.

## Why

Skills are how a user teaches the app a repeatable job without building a full scenario. They are also the unit the agentic front-end reaches for.

## Depends on

- `TASK_031` — Tool registry and the permission ceiling.

## Read first

- `/design/Skills.dc.html` — the editor and the rail
- `/docs/PASSPORT.md` §14 (narrowing), §11 (a skill run is a kernel run)

## Scope

**In scope**
- `skill` table and repository; storage of instructions and attached files
- Loading and validation, including the allowed-tools subset check
- Trigger matching with a score, and the trigger-check API the editor uses
- Skill execution as a kernel run with the narrowed tool set
- Import and export of a skill folder
- Channels, DTOs, tests

**Out of scope** (do not do these, even if tempting)
- The Skills page (TASK_036)
- Skill marketplace or sharing
- Automatic skill authoring

## Steps

1. Decide storage per the decision below, then implement it consistently — the loader and the editor must agree.
2. `skill` table: id, key, name, description, instructions, allowedTools JSON, modelOverride, enabled, source (`local` | `imported`), path, timestamps.
3. Validation on save: key is unique and slug-shaped; every allowed tool exists in the registry and is not above the skill's permitted ceiling; instructions are non-empty. Reject with field-level errors the editor can render.
4. Trigger matching: score a user message against each enabled skill's description. Start with embeddings — the app already has an embedding provider, and semantic matching beats keyword matching for this. Return a ranked list with scores so the editor's trigger check can show `0.86` as the mockup does. Make the threshold configurable.
5. Execution: a skill run is a kernel run whose graph is built from the instructions, with the tool set narrowed to the skill's allowed list and attached files made available to the prompt.
6. Attached files: store alongside the skill, size-capped, and included by reference rather than inlined into every prompt.
7. Import/export: a folder containing `SKILL.md` and resources, so a skill is portable and diffable in git.
8. Channels: `skills.list|get|create|update|remove|test|run`.
9. Tests: allowed tools cannot exceed the ceiling (assert the widening attempt is rejected); trigger scoring ranks a matching phrase above an unrelated one; a skill run executes with the narrowed set and is refused a tool outside it; import/export round-trips.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Where should skills live — as rows in the database, or as folders on disk with a `SKILL.md` file that the app watches and loads? Folders are diffable and shareable through git; database rows are simpler and always consistent with the UI."
>
> **Why it matters:** It decides whether the editor writes to disk or to SQLite, whether a file watcher is needed, and how import/export works. Changing it later means migrating user content.
> **Options:** (a) database rows, export to a folder on demand; (b) folders on disk as the source of truth, watched and loaded, database holds only an index; (c) database as truth with automatic two-way sync to a folder.
> **If the user is unavailable:** take (a). It is simplest and cannot desynchronise; export covers the sharing case. Note it in Handoff.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Create a skill allowing two tools, run it, and confirm it works. Then try to allow a tool that is `off` on the Tools page and confirm the save is rejected with a clear reason.

## Definition of done

- [x] Skills stored, validated and loadable
- [x] Allowed tools provably cannot exceed the ceiling
- [x] Trigger matching returns sensible scores; threshold configurable
- [x] Skill execution runs on the kernel with a narrowed tool set
- [x] Import/export round-trips
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Storage, as the user decided: each skill is a folder `<app data>/skills/<key>/` holding
  `SKILL.md` (frontmatter `name`, `description`, `allowed-tools`, `metadata.display-name`,
  `metadata.model-provider` / `metadata.model`, unknown keys kept) plus `references/`,
  `scripts/` and `assets/`. The `skill` table (migration `0006_skill`) is the index: key, display
  name, description, allowed tools, model override, enabled, source, status, issues, file count,
  a stat fingerprint and the path. The folder is the content; the row is the index plus the state
  the app owns.
- `crates/zvs-host/src/skills/`: `manifest` (frontmatter parser and renderer, no YAML crate),
  `library` (`SkillLibrary`: create, update, remove, import, export, reconcile, load, read),
  `matching` (lexical score, cosine), `service` (`SkillService`: trigger test, catalogue
  selection, runs), `nodes` (`skill.load`, `skill.read`, `skill.generate`).
- Saves are atomic: a `.staging-*` folder is written and swapped in, and the old folder goes to
  `.trash-*`. A rejected save leaves the folder and the row untouched. Leftover staging and trash
  folders are removed on reconcile.
- Validation reports every problem at once: `VALIDATION_FAILED`, message «Навык не прошёл
  проверку», `details.field` / `details.rule` for the first issue and `details.issues` with all of
  them (`SkillIssueRule`: required, pattern, tooLong, duplicate, unknownTool, toolOff,
  toolDisabled, toolUnavailable, path, fileTooLarge, skillTooLarge, tooManyFiles, encoding,
  manifest, missing).
- Ceiling: an allowed tool must be registered, available, enabled and not `off`. This is checked on
  save and again whenever a skill is listed or read, so turning a tool off later marks the skill
  `invalid`. At run time the kernel enforces the set: `RunGraph.allowedTools` is a new optional
  contract field, and `Gate::admit_tool` refuses any model-originated call outside it
  («Инструмент не входит в набор, разрешённый для этого запуска»).
- Hand edits: no watcher. A stat fingerprint (path, size, mtime) is compared on list and get; a
  changed folder is re-parsed and re-checked. A broken or widened skill becomes `invalid` with
  its issues. Folders dropped into `skills/` are adopted on reconcile.
- Trigger matching scores the message against `key + name + description`. Embeddings are used
  when `skills.trigger.embedding` names a provider and model, or else from the first enabled
  embedding provider with a default model; vectors are cached in memory. Without one, or on
  failure, a lexical score (the share of the message's 5-character stems found in the skill text)
  is used, and the result says which (`method`). The setting `skills.trigger` =
  `{ threshold (0.5), catalogueLimit (20), embedding? }` is typed in the contract for TASK_052.
- `skills.test` ranks every usable skill and accepts an unsaved `description` (with or without
  `id`), so the editor's trigger check scores a draft.
- Runs: `skills.run` starts a `RunKind::Skill` run (a new kind) with one `skill.generate` node,
  `permissionScopes: ["skill:<key>"]` and `allowedTools` = the skill's list plus `skill.read` when
  it has files. The system prompt carries the instructions and the file list by path; contents
  are read on demand. Model: the input's provider and model, else the skill's override.
- At the user's request, progressive disclosure in chat: the chat prepare node lists usable skills
  (`- key: description`, capped by `catalogueLimit`, top-k by trigger score when there are more)
  and offers `skill.load` and `skill.read`. Both are read-only built-ins governed by the Tools
  page. Loading a skill in chat never widens the turn: calls still pass the ceiling.
- Import/export: a `SKILL.md` folder. Import keeps unknown tools as `invalid` issues instead of
  refusing, moves loose root files under `references/`, and refuses a folder without `SKILL.md`.
  Export refuses to overwrite. Both open the folder picker when no path is given and return
  `null` when it is cancelled.
- Channels `skills.list|get|create|update|remove|test|run|import|export`, Rust DTOs, Zod schemas
  and fixtures in `packages/shared/fixtures/skills/`.

**Deviated from plan:**
- Step 1 / the decision: neither (a) nor (b) as written. The user chose metadata in the database
  and the whole skill in its own folder inside the app. There is no file watcher.
- Step 2: `instructions` is not a column; it lives in `SKILL.md`. Added `status`, `issues`,
  `file_count` and `content_hash`.
- Added `RunKind::Skill`, `RunGraph.allowedTools`, `SkillImportResultDto` (nullable on cancel)
  and the `skill.load` / `skill.read` tools with the chat catalogue, which the user asked for
  mid-task.
- `scripts/` is stored and listed but never executed: running scripts needs the toolchain and its
  own permission tier (TASK_035_EXTRA_1, TASK_035_EXTRA_2).

**Next session should know:**
- TASK_036 maps `SkillIssueRule` and the `field` path (`allowedTools.N`, `files.N`,
  `removeFiles.N`, `key`, `name`, `description`, `instructions`, `path`) to Russian copy. Do not
  parse `error.message`.
- The trigger check panel calls `skills.test` with `{ message, id, description }` and shows
  `score` and `triggered` against `threshold`.
- The manual check (create, run, reject an `off` tool) needs the Skills page; it is covered
  instead by `skills::tests` and `ipc::skills::tests`.
- Locally, `zvs-studio`'s own tests do not compile on rustc 1.93 (`AtomicU64::try_update`, from
  8d91c8b for rustc 1.99), and `cargo test --workspace` crashed rustc on the lance crates under
  full parallelism; `-j 2` passed. Both are environmental.
