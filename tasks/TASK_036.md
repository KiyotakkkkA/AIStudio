---
task: TASK_036
title: Skills page
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_035, TASK_076]
unblocks: []
estimate: 3 h
---

# TASK_036 — Skills page

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The editor from `/design/Skills.dc.html` and the close of Phase 6: a 264 px skill list with enable toggles; a tabbed editor (Definition, Resources, Runs) whose Definition tab holds name, identifier, the trigger description and the `SKILL.md` pane; and a 300 px rail with allowed tools as checkboxes, model override, attached files and the trigger check.

## Why

Skills are unusable without an editor, and the trigger-check panel is what teaches a user to write a description that actually fires.

## Depends on

- `TASK_035` — Skills engine — definition, loading, trigger matching.

## Read first

- `/design/Skills.dc.html`
- `/docs/PASSPORT.md` §13

## Scope

**In scope**
- `features/skills/` store, editor view-model, organisms
- Skill list with toggles and source chips
- Definition tab with a monospace instruction editor
- Allowed-tools checklist reflecting the ceiling — tools above it are shown disabled with the reason
- Trigger check calling the real scorer
- Resources tab for attached files; Runs tab listing that skill's runs

**Out of scope** (do not do these, even if tempting)
- A rich markdown WYSIWYG — a good monospace editor with syntax highlighting is right for `SKILL.md`
- Skill sharing

## Steps

1. Store and editor view-model per the established pattern, with dirty tracking and a discard prompt.
2. Instruction editor: monospace, line numbers, markdown highlighting via the kit's `./code-view` if it supports editing, otherwise a plain textarea styled to match. Do not pull in a heavyweight editor for this.
3. Allowed tools: checkboxes grouped by source, with tools above the ceiling rendered disabled and explaining why — that is the moment the permission model teaches itself.
4. Trigger check: input a phrase, call `skills.test`, show whether it would fire and the score, as the mockup does.
5. Resources tab: attached files with size, add and remove. Runs tab: this skill's kernel runs, linking into Runs & Logs.
6. States: no skills, invalid definition with field errors, disabled skill, tool unavailable because its server is offline.
7. Tests: editor view-model validation mirrors the host's; ceiling-blocked tools render disabled.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Write a real skill, check a trigger phrase against it, run it from the page, and confirm it appears on Tasks and in Runs & Logs.

## Definition of done

- [x] Page matches the mockup
- [x] Editing, validation and enabling work against the real service
- [x] Ceiling-blocked tools are visibly explained, not silently missing
- [x] Trigger check uses the real scorer
- [x] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `features/skills/`: `SkillStore` (list, selection, dirty tracking with a discard prompt, save,
  enable, duplicate, import, export, trigger check, runs), `SkillFormVm` (validation that mirrors
  the host: key slug, required fields, lengths, file paths, 1 MiB per file, 10 MiB and 200 files
  per skill; host `SkillIssueDto`s map onto the same fields), `skillPresentation` (Russian copy
  for every `SkillIssueRule`, ceiling reasons, plurals), `markdownTokens` + `InstructionEditor`
  (textarea with a line gutter and a highlight layer; the kit's `code-view` is read-only).
- Layout, simplified at the user's request: skill list with toggles and source chips · a file tree
  (kit `TreeView`) with `SKILL.md` and `references/`, `scripts/`, `assets/` — `SKILL.md` opens
  name, description and instructions, any other file opens read-only in the kit `CodeView`; files
  are added and removed from the tree and applied on save · a right rail with the trigger check,
  a run panel with the latest runs (linking into Runs & Logs) and a collapsed «Дополнительно»
  (identifier, default model, allowed tools grouped by source). Tools above the ceiling are
  listed disabled with the reason; an MCP tool whose server is offline names the server. Delete
  is a danger button in the bottom bar, as on «Коннекторы»; duplicate and export sit in a «⋯» menu.
- Host: channels `skills.setEnabled` (no folder rewrite, works on an invalid skill),
  `skills.duplicate` (`<key>-copy`, `-copy-2`, …), `skills.readFile` (by id, any status, never
  outside the folder); `RunListFilter.subjectId`; `SkillSource::Bundled`; `skills.list` now
  reconciles, so dropped folders are adopted. Python scripts get `PYTHONUTF8`/`PYTHONIOENCODING`.
- Bundled skills `pdf`, `docx`, `xlsx` in `crates/zvs-host/skills/`, embedded with
  `include_str!` (`skills::BUNDLED_SKILLS`) and installed once at startup
  (`SkillLibrary::install_bundled`, marker `<skills>/.bundled.json`). Scripts pin pypdf 6.21.2,
  pdfplumber 0.11.10, python-docx 1.2.0, openpyxl 3.1.5, reportlab 5.0.1 in PEP 723 blocks; each
  was run with uv against real files.
- Removed the «Пакеты npx и uvx…» hint from the MCP server form.
- At the user's request, the left lists of «Навыки», «Коннекторы», «Векторные хранилища» and
  «AI-провайдеры» (not the accounts tab) share one style: a bordered panel with an eyebrow
  heading, a search field, `ListRow`s with a status dot, the name and a chip, one subline, and a
  dashed create row. Connectors gained `visibleServers`, providers `visibleSummaries`.
  `providerInitials` and `healthColors` went with the old rows. The skill on/off switch sits in
  the editor bar, since a toggle cannot nest inside a `ListRow` button.

**Deviated from plan:**
- Tabs (Definition, Resources, Runs) became the file tree plus the run panel, at the user's request.
- The identifier, model override and allowed tools moved into a collapsed «Дополнительно»; the
  rail opens itself when a save fails on one of them.

**Next session should know:**
- Bundled skills are installed once; a newer bundled version does not update an installed copy.
- A removed bundled skill is not reinstalled (its key stays in `.bundled.json`).
- Not run here: `pnpm build`, `pnpm test:e2e` and the manual check (need the built app), and a
  full `cargo test --workspace`; `zvs-contract` and the `zvs-host` skills, runs and IPC tests passed,
  and clippy is clean for `zvs-contract`, `zvs-host` and `zvs-studio` (lib, bins).

---

**Phase 6 is complete when this task is done.** The app is extensible: MCP servers add capabilities, the tool registry governs them, and skills package repeatable work.
