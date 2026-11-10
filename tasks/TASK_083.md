---
task: TASK_083
title: Approval lifetime, the audit trail and the agentic mode on one table
phase: A — Access pipeline (APS)
status: todo
depends_on: [TASK_077, TASK_078]
unblocks: []
estimate: 3 h
---

# TASK_083 — Approval lifetime, the audit trail and the agentic mode on one table

## Context

Three leftovers from `docs/APS_AUDIT.md` that sit on the pipeline built in TASK_077 and
TASK_078:

- **M23 — approvals expire.** `ApprovalService` is created with `DEFAULT_APPROVAL_TIMEOUT`,
  60 seconds (`crates/zvs-host/src/services/approvals.rs`); the audit found no place that
  overrides it. An unanswered approval is a denial and the tool call fails. **Confirm this
  first**: read how `RunServiceOptions::approval_timeout` is set where the services are built,
  and time an approval in the running app.
- **L6 — the audit record mixes kinds.** A rule `allow` is stored as
  `ToolAuthorisationKind::Judge` with the rule id as the source (`policy/service.rs`). The tool
  call details cannot tell "a rule allowed it" from "the judge allowed it", and no record says
  which stages were visited.
- **M5 — the judge has no memory.** Each call is judged alone; the per-run cap of 50 is counted
  in a map that is never pruned and resets on restart.
- **Two mode systems.** Agentic sessions carry `AgenticPermissionMode { ReadOnly, Ask, Auto }`
  (`agentic/node.rs` `minimum_tier`) beside the global `ExecMode`. Which one wins for a given
  call is decided by where the code happens to check.

## Why

A person who was away for a minute finds a failed run; a person reading why something ran
cannot see the path it took; and an agentic session's stated mode and the composer's mode can
disagree.

## Depends on

- `TASK_077` — `Decision` with a stage trail.
- `TASK_078` — every driver decides through the pipeline.

## Read first

- `/docs/APS_AUDIT.md` — findings M5, L6, M23 and the mode row of the APS stage table
- `/docs/PASSPORT.md` §14 *Audit*; §8 *Approval gate*
- `crates/zvs-host/src/services/approvals.rs`, `services/runs.rs`
- `crates/zvs-contract/src/` — `ToolAuthorisationDto`, `ToolAuthorisationKind`
- `crates/zvs-host/src/agentic/node.rs`

## Scope

**In scope**
- **Approval lifetime** as decided below; the pending approval is restored to the studio and
  the console after either reconnects.
- **Audit.** `ToolAuthorisationKind` gains `Rule`; `ToolAuthorisationDto` gains `stages`
  (the ordered stage names the decision passed). Rust type, Zod schema, golden fixture. The
  tool call details show «Правило», «Арбитр», «Режим», «Разрешение» or «Подтверждено» with the
  source and the reason.
- **Judge context.** The judge prompt receives the last few verdicts of the same root run
  (action summary and decision only, never conversation text), capped by size. The spent
  counter moves to the run row or is pruned when the root run ends.
- **One mode table.** The agentic session mode becomes a ceiling applied in the mode stage:
  `ReadOnly` refuses every effect but `Read`, `Ask` behaves as `always-ask`, `Auto` defers to
  the global mode. `minimum_tier` in `agentic/node.rs` is removed.

**Out of scope** (do not do these, even if tempting)
- New modes.
- Changing the judge's instructions beyond adding the history block.

## Steps

1. Confirm the 60 s finding. If it is wrong, record what the real value is in Handoff and skip
   to step 3.
2. Implement the lifetime decision.
3. `Rule` kind and `stages`; renderer and console copy.
4. Judge history and counter clean-up, with a test that the history never contains more than
   the action summary.
5. Agentic ceiling in the mode stage; port the agentic permission tests.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "An unanswered approval is denied after 60 seconds today. Should an approval (a) wait until
> answered or the run is cancelled, (b) wait a configurable time with a default of 30 minutes
> and a notification, or (c) stay at 60 seconds?"
>
> **Why it matters:** (a) can leave a run blocked overnight holding a shell or a browser tab;
> (c) fails runs whenever the person looks away.
> **Options:** a / b / c as above.
> **If the user is unavailable:** take (b) and note it in Handoff.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p zvs-host services:: policy:: agentic::
cargo test -p zvs-contract
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check: raise an approval and leave it for two minutes — it is still answerable. A
command allowed by a rule shows «Правило» and the rule text in its details; one allowed by the
judge shows «Арбитр» and the reason. A read-only agentic session refuses `fs.apply` in
`no-restrictions`.

## Definition of done

- [ ] Approval lifetime follows the decision and survives a client reconnect
- [ ] Rule and judge decisions are distinguishable, with the stage trail, in details and logs
- [ ] The judge sees a bounded history of its own verdicts; the counter does not leak
- [ ] The agentic mode is applied in the mode stage only
- [ ] Contract, schema and fixture updated together
- [ ] PASSPORT §8 and §14 updated
- [ ] All gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
