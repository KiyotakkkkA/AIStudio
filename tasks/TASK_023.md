---
task: TASK_023
title: Node registry, permission admission, approval gates
phase: 4 — Execution kernel and Chat
status: done
depends_on: [TASK_022]
unblocks: [TASK_024, TASK_031, TASK_037]
estimate: 4 h
---

# TASK_023 — Node registry, permission admission, approval gates

## Context

The kernel can run a graph but has no nodes to run. This task adds the registry of node types and — more importantly — the permission model that decides whether a step is allowed to execute at all.

The permission spine is stated on the design canvas and in PASSPORT §14: a capability is registered once (built-in, discovered from an MCP server, or granted by a connected account), given a ceiling of `auto` / `ask` / `off`, and a skill or scenario **may narrow it further but never widen it**. This task implements that rule at step admission, which is the only place it can be enforced reliably. Every later capability — MCP tools, mail sending, browser clicks — inherits it rather than inventing its own check.

An `ask` node suspends the run and emits an `approval` event. The Agentic Build page, the Tasks page and the browser window all render that same event, which is why it belongs here and not in any one front-end.

## Why

Everything the app can do on the user's behalf passes through this gate. Building it once, at admission, is what keeps the security model from being re-implemented — inconsistently — in six places.

## Depends on

- `TASK_022` — Kernel core — Run, Step, scheduler, cancellation, checkpointing.

## Read first

- `/docs/PASSPORT.md` §11 (`NodeDef`, `PermissionRequirement`), §14 (the tool ceiling), §21 (how the browser composes with it)
- `/design/Tools.dc.html` — the auto/ask/off tiers as the user sees them
- `/design/AgenticBuild.dc.html` — the approval bar this drives

## Scope

**In scope**
- `NodeRegistry`: register, look up, validate a graph against known node types
- `NodeDef` with input/output schemas, permission requirement and a side-effect flag
- Permission admission at step start, composing node requirement with the stored ceiling
- Approval flow: suspend the run, emit the event, resume on decision, timeout handling
- Grant storage: per-scope grants with revocation
- Core nodes to prove it: `llm.generate`, `vector.search`, `flow.branch`, `flow.map`
- Tests, including that a narrowed grant cannot be widened

**Out of scope** (do not do these, even if tempting)
- The Tools page UI (TASK_034)
- MCP-provided nodes (TASK_032)
- Browser nodes (TASK_046)

## Steps

1. `NodeRegistry` with `register(def)` and `resolve(type)`. A graph referencing an unknown node type fails validation at run start, not halfway through execution.
2. `NodeDef` per PASSPORT §11, plus `sideEffect: boolean` (required by TASK_022's resume decision) and `permission: PermissionRequirement`.
3. `tool_permission` table: subject (tool or node type), scope (global, or narrowed by skill/scenario/site), tier (`auto` | `ask` | `off`), grantedAt, grantedBy. Migration committed.
4. `PermissionService.admit(nodeType, context)` returning `allow` | `ask` | `deny`. Composition rule, and it must be written so widening is impossible by construction: the effective tier is the **most restrictive** of the global ceiling and any narrowing scope. Test that explicitly with a narrowing scope that tries to grant more than the ceiling.
5. Approval flow: on `ask`, the kernel suspends the run, persists the pending approval, emits the `approval` event, and waits. `runs.approve` / `runs.deny` channels resume it. A pending approval that outlives a configurable timeout resolves as denied and the run fails with `APPROVAL_DENIED` — never leave a run hanging forever.
6. Grants: 'always allow' writes a grant row scoped appropriately. Every grant is revocable and every use of one is logged, so the Tools page and the browser panel can show what was authorised and when.
7. Implement the four core nodes. `llm.generate` streams through TASK_014's ports and emits `token` events; `vector.search` calls TASK_020's service.
8. Tests: admission returns the right decision for each tier combination; a narrowing scope cannot widen; approval suspends and resumes correctly; denial fails the run with the right code; timeout behaves as specified; an unknown node type fails graph validation up front; grants are honoured and revocable.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Run a graph containing an `ask` node; confirm the run suspends and an approval event arrives. Approve it and watch the run continue; repeat with denial and confirm the run fails cleanly.

## Definition of done

- [x] Registry validates graphs before execution
- [x] Permission composition is most-restrictive-wins, proven by test that widening is impossible
- [x] Approval suspends, resumes, denies and times out correctly
- [x] Grants stored, honoured, revocable and logged
- [x] Four core nodes working
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Registry resolution exposes schemas, permission requirements and side-effect metadata; graph validation remains upfront. Migration 0008_permissions adds tool_permission, pending_approval and immutable permission_use snapshots. PermissionService composes the global ceiling, node tier and skill/scenario/site scopes using most-restrictive-wins. RunService defaults to persisted approval admission; runs.approve/runs.deny support one-shot decisions and scoped always-allow grants, configurable expiry, cancellation cleanup, ceiling rechecks and non-retryable denial codes. Production registers llm.generate, vector.search, flow.branch and flow.map. Added 23 permission/core-node regressions and updated migration coverage.

**Deviated from plan:** Kept TASK_022's sideEffectFree spelling as a mutually exclusive compatibility alternative to sideEffect. Interrupted persisted approvals fail closed on restart rather than being replayed. Missing global permission defaults to ask. Scoped always-allow never overrides global ask/off; unscoped always-allow explicitly changes the global ceiling. flow.branch selects JSON values; flow.map projects an own-property path over an array in the static DAG. No arbitrary callbacks or nested node execution. Generation is marked as having side effects because providers can charge or alter sessions. Manual approval interaction is covered by automated graphs, event assertions and IPC handler calls; no approval UI belongs to this task.

**Next session should know:** See apps/studio/src/host/kernel/README.md for inputs, scope propagation and approval semantics. Graph producers must supply all applicable permissionScopes; scenario subject scope is included automatically. Usage snapshots retain subject, scope, tier and grant actor/time after revocation. Pending approval timeout defaults to 60 seconds. The existing documented protoc 36.1 installation was missing and was restored under ignored target/tools/protoc. Studio tests: 332 passed, 2 existing skips. Package tests: 46 passed. Verified pnpm typecheck, pnpm lint and pnpm build successfully. Changed-file formatting and git diff --check pass. Existing dependency bundle warnings remain non-fatal. Task files remain under the repository ignore rule.
