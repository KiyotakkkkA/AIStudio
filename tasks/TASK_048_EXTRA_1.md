---
task: TASK_048_EXTRA_1
title: Execution policy — bypass engine, policy rules, judge model
phase: 9 — Integrations
status: todo
depends_on: [TASK_048]
unblocks: []
estimate: 4 h
---

# TASK_048_EXTRA_1 — Execution policy: bypass engine, policy rules, judge model

## Context

A tool call is admitted today on two independent axes (PASSPORT §14). The tier on the Tools
page (`auto | ask | off`, resolved by `PermissionService::admit_subject` in
`crates/zvs-host/src/services/permissions.rs`) says *whether* and with what confirmation a tool
may act; folder grants (`zvs-host/src/workspace/`) and browser site grants
(`zvs-host/src/browser/grants.rs`) say *where*. Every call whose tier resolves to `ask` raises
an approval and waits for the user, which makes long agentic runs tedious.

PASSPORT §14 *Execution policy* (2026-10-29) adds a third axis above the two: one global mode
— `always-ask`, `safety-ask`, `auto-bypass`, `no-restrictions` — that decides how a call that
would ask is answered, with a floor no mode reaches. `auto-bypass` sends destructive file and
process actions through deterministic policy rules first and an external judge model second;
anything the judge cannot decide falls back to asking.

The renderer preview already exists: `features/execPolicy/` (`ExecPolicyStore`,
`ExecPolicyMenu` — the shield button in the composer), the «Модель-арбитр» row in Preferences
→ «Общие», and the shared `features/chat/ModelFields.tsx` picker. The store holds the mode and
the judge model in memory and the host ignores them. This task makes the host enforce them.

The task sits under TASK_048 by numbering only: integration tools register into the same
registry and are governed by the same admission, but nothing here depends on integration code.

## Why

Without the engine the composer control is a preview that changes nothing: every `ask` call
still blocks on the user, and the mode the user picked is lost on restart.

## Depends on

- `TASK_048` — the tool registry and admission path that integration tools also go through.
- `TASK_036_EXTRA_2`, `TASK_036_EXTRA_3`, `TASK_046` — `fs.apply`, `shell.run` / `code.run`
  and the browser nodes whose approvals the policy answers.

## Read first

- `/docs/PASSPORT.md` §14 *Execution policy*, *Judge*, *Audit* — the binding table and
  precedence; §14 *Folder access*, *File tools*, *Process tools*; §21 browser tiers
- `crates/zvs-host/src/services/permissions.rs` — `admit_subject`, `Admission`
- `crates/zvs-host/src/files/plan.rs`, `files/nodes.rs` — how an `fs.apply` batch is planned
- `crates/zvs-host/src/commands/nodes.rs` — the `kind: "process"` approval

## Scope

**In scope**
- The mode as a stored setting with a typed channel pair (`execPolicy.get`, `execPolicy.set`)
  and the judge model (`providerId`, `modelId`) beside it; DTOs in `zvs-contract`, Zod schemas
  and golden fixtures
- An `ExecPolicy` service consulted where an approval would be raised: it answers *run*,
  *ask* or *deny* from the mode, the tool family and the action's effect, after the tier and
  the floor are resolved — never before, so `off`, forbidden folders, missing grants and
  credential fields stay refused in every mode
- Effect classification for `fs.apply`: delete, overwrite of an existing file, move onto an
  existing path are destructive; creating a file is not
- Policy rules: `allow | ask | deny` over path globs × operation for files and `argv` patterns ×
  cwd for processes; vendor and user packs, precedence vendor `deny` > user > vendor `allow`;
  a pack declares whether it contains any `allow`
- The judge: one call per `fs.apply` batch or process call, given only the structured action
  and the rule texts, answering `allow` or `ask` with a reason; error, timeout, unparseable
  answer or unavailable model is `ask`
- `no-restrictions` set only through the renderer's confirmation; the host does not need to
  repeat it
- Audit: `authorisation` on each tool call — `auto`, `granted`, `approved`, `policy:<mode>`,
  `judge:<rule id | model>` with the reason — in the run log and the tool call details
- The renderer store switched from in-memory to the channels; the composer shows the stored
  mode; clearing or losing the judge drops `auto-bypass` to `safety-ask` on the host too

**Out of scope** (do not do these, even if tempting)
- A per-dialog override of the mode
- A rules editor page; packs are files for now
- Sandboxing `shell.run` / `code.run`
- Changing browser tiers beyond what the PASSPORT table states (submit and download still ask
  outside `no-restrictions`)

## Steps

1. Contract: `ExecModeDto`, `ExecPolicyDto { mode, judge: { providerId, modelId } | null }`,
   the two channels, fixtures; `cargo test -p zvs-contract`.
2. Storage: persist the policy in settings; on load, a judge whose provider or model is gone
   is cleared and `auto-bypass` becomes `safety-ask`.
3. `ExecPolicy::decide(call, effect) -> Decision` as a pure function over the mode and the
   family table; unit-test every cell of the PASSPORT table, the floor row included.
4. Wire `decide` into the approval points: `fs.apply` (deferred approval), the process
   approval, the browser acting nodes. `run` skips the approval; `ask` raises it as today.
5. Rules: pack loader, glob and argv matchers, precedence. Tests for each precedence case.
6. Judge: prompt, request through the existing provider drivers, parse, time out. Tests with a
   fake driver for allow, ask, garbage, timeout and error.
7. Audit field on the tool call DTO (three places: Rust, Zod, fixture) and in the run log.
8. Renderer: `ExecPolicyStore` loads and saves through the channels; tests with
   `createFakeBridge()`.
9. PASSPORT: replace the *Status* paragraph with an implementation note.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Does the judge get its own token budget and rate limit, or does it share the chat model's?"
>
> **Why it matters:** a batch-heavy agent run can call the judge dozens of times.
> **Options:** a — shared with the provider (simplest, can starve the chat); b — its own
> per-run cap, after which every call asks (predictable); c — no cap.
> **If the user is unavailable:** b, with a cap of 50 judge calls per run; note it in Handoff.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check: in `safety-ask`, `fs.apply` creating a new file runs without a prompt and
deleting one asks. In `auto-bypass` with a user rule allowing deletes under a test folder, a
delete there runs and the tool call shows `judge:` or the rule id; with the judge provider
disabled, the same delete outside the rule asks. In `no-restrictions`, a path in a forbidden
folder is still refused.

## Definition of done

- [ ] Every cell of the PASSPORT §14 table is enforced and unit-tested
- [ ] No mode reaches the floor row
- [ ] Judge failure of any kind resolves to asking
- [ ] Every admitted call records its authorisation
- [ ] The mode and the judge survive a restart
- [ ] `pnpm typecheck && pnpm lint && pnpm test` and the cargo gates pass; `pnpm build` left to CI
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
