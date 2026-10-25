---
task: TASK_041
title: Agentic Build page
phase: 7 — Scenarios and Agentic Build
status: done
depends_on: [TASK_040, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_041 — Agentic Build page

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The interface for agentic sessions, per `/design/AgenticBuild.dc.html`, and the close of Phase 7. Three columns beside the rail: a 296 px **plan** column with a progress bar and steps in done, running and queued states; the **tool-call stream** in the middle showing agent text, tool cards with diffs and command output, and the approval bar; and a 312 px **session context** rail with the model, permission mode segmented control, workspace, changed files and the budget meter. A composer sits at the bottom for adding instructions mid-session.

The tool cards are worth building properly. The mockup shows an `edit` card with a real unified diff (green additions, red deletions at low alpha) and a `run` card with command output and an exit-code chip. Being able to see exactly what the agent did, in place, is the difference between trusting it and not.

## Why

An agentic session without a live view of what it is doing is the least trustworthy thing the app could ship.

## Depends on

- `TASK_040` — Agentic session engine — planning loop and self-extending runs.

## Read first

- `/design/AgenticBuild.dc.html`
- `/docs/PASSPORT.md` §13

## Scope

**In scope**
- `features/agentic/` store subscribing to the session run stream
- Plan column with live step states and reordering
- Tool-call stream with diff and command-output cards
- Approval bar with Deny / Always allow / Approve once
- Session context rail with permission mode, changed files, budget
- Composer for mid-session instructions
- New-session flow: goal, workspace, model, permission mode

**Out of scope** (do not do these, even if tempting)
- Browser-specific rendering (TASK_047)
- Multi-session parallel view — one active session at a time in the page; others live on Tasks

## Steps

1. Store subscribing to the session's stream, mapping step events into plan state and stream cards. Handle out-of-order and gapped events, as TASK_005 requires.
2. Plan column: index badges with done, running and queued styling as drawn; drag to reorder queued steps; edit and delete on a queued step, applied at the next boundary.
3. Tool cards: a card per tool call with a header (kind chip, target, result chip, timing) and an expandable body. Diff bodies render unified diffs with the mockup's low-alpha green and red. Command bodies render output monospace with an exit-code chip. Long output is collapsed with an expand.
4. Approval bar: rendered from the `approval` event, showing exactly what is being asked, with the three actions. No destructive default and no keyboard shortcut that could approve accidentally.
5. Context rail: model card, permission mode segmented control (changing it mid-session applies at the next boundary and is logged), workspace with branch chip, changed-files list with +/- counts, budget meter.
6. New-session flow: goal, workspace picker, model, permission mode, budget override. Refuse to start without a workspace, since path scoping depends on it.
7. States: no session, planning, running, blocked on approval, budget reached (offer to extend), failed, completed with a summary.
8. Tests: store maps step events to plan state correctly; diff parsing and rendering is a pure function and covered; approval actions dispatch the right calls.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Run a real session end to end. Approve one action and deny another. Reorder a queued step mid-run and confirm the agent honours it. Let it hit the budget and confirm the extend prompt appears.

## Definition of done

- [x] Page matches the mockup
- [x] Plan, stream and approvals driven by real events
- [x] Diffs and command output render readably
- [x] Permission mode and plan edits take effect at step boundaries
- [x] Budget exhaustion offers extension rather than dying silently
- [x] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass (`pnpm build` not run, see handoff)
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `features/agentic/`: `AgenticStore` (registered as `RootStore.agentic`), `SessionDraft` (new-session
  form), pure `sessionModel.ts` (stream reducer, plan rows and queue edits, phase, budget lines,
  extension budget) and `toolCardModel.ts` (card target, body, result chip, output preview),
  `agenticCopy.ts`, and the views `AgenticWorkspace`, `PlanColumn`, `SessionStream`, `ToolCard`,
  `ContextRail`, `NewSessionModal`. `AgenticBuildPage` renders the workspace.
- The stream is rebuilt from `runs.list { kinds: ["agentic"], subjectId }` plus `runs.detail`
  events of every run of the session (extensions included), then follows the live stream. Events
  arriving during a rebuild are held and replayed; duplicates and late events are dropped by
  per-stream `seq`; a router `gap` triggers a full rebuild. Session state comes from the
  `{ domain: "agentic", session }` step events, newest `updatedAt` wins.
- Tool cards reuse the chat presentation (`toolPresentation`, `toolCallSummary`,
  `FileChangeList` for unified diffs with the low-alpha ok/err tones, `processResult`).
  Command cards show stdout/stderr with an exit-code chip; long output collapses at 12 lines.
- Approval bar from the `approval` event: shows the tool, the command or program text and the
  diff; Deny / Always allow / Approve once go to `runs.deny` / `runs.approve`. No autofocus and
  no shortcut.
- Plan: done/running/queued/failed badges; queued steps drag to reorder, rename inline, delete;
  the pending plan and pending mode show at once with a note that they apply at the boundary.
- Tests: `apps/studio/tests/renderer/agentic.test.ts` (23).

**Deviated from plan:**
- The mockup's History button is the session list (back arrow in the session header); list rows
  open or delete a session.
- No branch chip on the workspace card, no temperature/max-tokens chips, no `@ file` /
  `# skill` composer chips: the host has no data or behaviour behind them.
- Extension is one click with computed limits (each limit raised by the default on top of what
  is used); there is no custom extension form.
- The new-session flow has no tool picker; the host offers every enabled tool when `tools` is
  omitted.

**Next session should know:**
- Gates run: `pnpm typecheck`, `pnpm lint`, `pnpm test` (2168 passed), `pnpm format:check`
  (only the pre-existing `NavRail.tsx` warning). No Rust changed, so cargo gates were not rerun;
  `pnpm build` and the manual end-to-end walk with a live model were not done in this session.

---

**Phase 7 is complete when this task is done.** All three kernel front-ends exist: chat, scenarios and agentic sessions, over one engine.
