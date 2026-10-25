---
task: TASK_045_EXTRA_4
title: Skill read-first files — required references arrive with skill.load, tool names match the wire
phase: 8 — Documents and the agent browser
status: todo
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

- [ ] `read-first` and `requires` parsed, validated and tested
- [ ] `skill.load` inlines read-first files within budget and describes every file
- [ ] Tool names in loaded instructions match the wire names
- [ ] Bundled skills and `skill-creator` updated, versions bumped
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
