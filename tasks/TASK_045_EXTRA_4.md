---
task: TASK_045_EXTRA_4
title: Skill read-first files — required references arrive with skill.load, tool names match the wire
phase: 8 — Documents and the agent browser
status: done
depends_on: [TASK_045, TASK_045_EXTRA_1]
unblocks: []
estimate: 3 h
---

# TASK_045_EXTRA_4 — Skill read-first files

## Context

`skill.load` (`crates/zvs-host/src/skills/nodes.rs`) returns a skill's instructions and a bare
file list: `path` and `size`, nothing about what a file is for or whether it must be read.
The instructions point to the files in prose, and the model has to decide to read each one.
Strong models do; weak and web models often skip straight to the scripts and call them with
wrong arguments.

The bundled dashboard skill (`crates/zvs-host/skills/dashboard/SKILL.md`) needs about six
references across its workflow, about 33k chars in total: `references/spec.md` 9.8k,
`components.md` 8.5k, `exploration.md` 5.6k, `aggregation.md` 5k, `layout.md` 2.6k and one
source guide of about 2k. Each read is one call out of the turn's budget of 15
(`CHAT_TOOL_CALLS`), so required reading alone takes 2–6 of them.

A second problem: the skill instructions name tools by key (`chat.file`, `skill.script`),
but the model sees wire names (`chat__file`, `skill__script`, from `wire_tool_name` in
`services/chat_tools.rs`). `catalogue_text` (`skills/service.rs`) substitutes wire names in
the catalogue text only, not inside the instructions `skill.load` returns.

## Why

Making the required reading part of `skill.load` removes a choice the weak model gets wrong,
and frees call budget for the actual work. Matching tool names removes a source of
unknown-tool calls.

## Depends on

- `TASK_045` — base task of this extra (backlog placement only).
- `TASK_045_EXTRA_1` — tool names may change there; do this after, so the rewrite and the
  bundled skills use the final names.

## Read first

- `/docs/PASSPORT.md` §11 (kernel nodes) — binding
- `crates/zvs-host/src/skills/nodes.rs`, `library.rs`, `manifest.rs`, `service.rs`
- `crates/zvs-host/skills/*/SKILL.md` and `crates/zvs-host/skills/skill-creator/` (it
  teaches users how to write skills, so it must describe the new key)

## Scope

**In scope**
- A manifest key `metadata.read-first: [<path>, …]` (paths under `references/` or `assets/`),
  validated by `manifest.rs` with errors surfaced like other manifest problems.
- `skill.load` inlines the read-first files in its output within a character budget (start
  at 16k chars; over budget, inline in order until full and mark the rest as required reads).
- Each entry in `files` gains `purpose`, taken from the file's first heading or first line
  (scripts: the first docstring line), at most 120 chars.
- Tool keys inside the returned instructions are rewritten to the wire names offered in that
  turn, the same way `catalogue_text` does it.
- An optional soft precondition `metadata.requires: { "scripts/build_dashboard.py":
  ["references/spec.md"] }`: running that script before the file was loaded or read in the
  current turn returns a hint naming the file instead of running. Same idea as `fs.apply`'s
  "read before you change".
- Bundled skills updated (dashboard first: `spec.md` and `exploration.md` as read-first),
  versions bumped so `install_bundled` replaces them; `skill-creator` documents the keys.
- Russian copy for any new user-visible validation message (CONVENTIONS.md).

**Out of scope** (do not do these, even if tempting)
- Inlining every reference by default.
- Changing which skills the catalogue offers or how they are matched (`matching.rs`).

## Steps

1. Manifest: parse and validate `read-first` and `requires`; tests for unknown paths,
   paths outside `references/`/`assets/`/`scripts/`, and duplicates.
2. `skill.load`: inline within budget, add `purpose`, rewrite tool names. Tests: budget
   cut-off order, binary files never inlined, names rewritten for native and web wire names.
3. Precondition in the script path (or `shell.run` with a `skill:` program after
   TASK_045_EXTRA_1): test refused-with-hint before the read, runs after it, and that the
   hint does not count as a failed call in the budget accounting if that is how budget works.
4. Update the bundled skills and `skill-creator`; bump versions.
5. Record in Handoff the number of calls the dashboard flow spends on reading, before and
   after, on the fake driver.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check: with a small local model or a Qwen web account, ask for a dashboard over a CSV
and confirm the model writes a valid spec on the first build attempt without reading
`spec.md` separately.

## Definition of done

- [x] `read-first` and `requires` parsed, validated and tested
- [x] `skill.load` inlines read-first files within budget and describes every file
- [x] Tool names in loaded instructions match the wire names
- [x] Bundled skills and `skill-creator` updated, versions bumped
- [x] Gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `metadata.read-first: [<path>, …]` and `metadata.requires: { <script>: [<path>, …] }` are
  one-line flow collections parsed in `manifest.rs` (`read_first`, `requires`) and checked in
  `library.rs` (`reference_issues`) for folder skills, editor drafts and `skill.save`. Issues
  use the existing rules on `readFirst.N` / `requires.N[.M]`: `path` (wrong folder),
  `missing` (not in the skill), `duplicate`, `manifest` (syntax). The editor labels them
  «Обязательные файлы» and «Файлы перед запуском скриптов».
- `skill.load` returns `text` (instructions, file list, required reading, script
  preconditions, inlined files) plus `files[].purpose`, `inlined`, `required`, `requires`.
  Read-first files are inlined in order up to `READ_FIRST_BUDGET` (16 000 chars); the first
  file that does not fit and all after it become required reads; binary files are never
  inlined. A purpose is a reference's first heading or first line, a script's docstring,
  `argparse` description or leading comment, at most 120 chars (`skills/purpose.rs`).
- Tool keys in a `skill.load` result are rewritten to the wire names offered in the turn
  (`chat_tools::wire_names`, on word boundaries) in the chat, `skill.generate`, agentic and
  scenario loops. Its result limit is six times the per-call share, at most 40 000 chars
  (`turn::result_limit`); before, the 8.5k dashboard instructions alone were cut at 8 000.
- `skills/turn.rs` `SkillTurn` tracks what a turn loaded and read. In the chat and
  `skill.generate` loops a `shell.run` of a script listed in `requires` is refused with
  `READ_BEFORE_RUN: skill:<key>/<path>` until the file was inlined by `skill.load` or read
  with `fs.read` in the same turn. The hint is refunded from the call budget.
- `skill.save` takes optional `readFirst` and `requires`.
- Dashboard 1.6.0: `spec.md` and `exploration.md` are read-first (14 632 chars together),
  `build_dashboard.py` requires `spec.md`. Skill-creator 1.3.0 documents both keys, file
  purposes and tool-key rewriting; `check_skill.py` validates the keys.
- Reading calls on the dashboard flow, fake driver (`the_bundled_dashboard_arrives_with_its_
  spec_and_exploration_guide_in_one_call`, `a_skill_script_waits_for_its_required_reading_
  and_the_hint_costs_no_budget`): before, `skill.load` plus `fs.read` of `spec.md` and
  `exploration.md`, 3 calls (2 when the model batches both paths in one `fs.read`); after, 1.
  The other references (`aggregation`, `layout`, `components`, one source guide) are still
  read on demand.

**Deviated from plan:**
- The precondition only applies once the skill was loaded in the same turn: `SkillTurn`
  learns `requires` from the `skill.load` result, so a script run in a later turn, with the
  load in the history, runs without the check.
- Agentic and scenario loops get the rewrite and the larger limit but not the precondition;
  there `skill.load` is reached only when the user enables it.
- A purpose is only taken from Markdown, text and scripts; a first line of JSON, HTML or CSS
  says nothing, so other assets have none.
- The flow form is the only supported form of the two keys; nested block YAML under
  `metadata` is not parsed.
- The restriction removal the user asked for went into a separate commit before this one
  (`refactor: drop skill tool and model restrictions`).

**Next session should know:**
- The manual check with a small local model or a Qwen web account was not run.
- `pnpm build`, `pnpm test:e2e` and the e2e smoke of the skill editor were not run.
- Under full parallel `cargo test` on this machine, two skill tests failed once each with
  «Отказано в доступе (os error 5)» on a folder rename and passed on rerun; it looks like the
  antivirus, not the code.
- `pnpm format:check` still flags `apps/studio/src/renderer/ui/organisms/NavRail.tsx`, which
  this task did not touch.
