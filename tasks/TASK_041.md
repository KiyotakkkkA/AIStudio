---
task: TASK_041
title: Agentic Build page
phase: 7 — Scenarios and Agentic Build
status: todo
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

- [ ] Page matches the mockup
- [ ] Plan, stream and approvals driven by real events
- [ ] Diffs and command output render readably
- [ ] Permission mode and plan edits take effect at step boundaries
- [ ] Budget exhaustion offers extension rather than dying silently
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**

---

**Phase 7 is complete when this task is done.** All three kernel front-ends exist: chat, scenarios and agentic sessions, over one engine.
