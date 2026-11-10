---
task: TASK_078
title: Drivers on the pipeline — prepare, access and the Admitted token
phase: A — Access pipeline (APS)
status: todo
depends_on: [TASK_077]
unblocks: [TASK_082, TASK_083]
estimate: 4 h
---

# TASK_078 — Drivers on the pipeline: prepare, access and the Admitted token

## Context

A node whose approval needs something only it can show (a diff, a process plan, the element
about to be clicked) declares `defers_approval()`. The gate then hands it a ticket and returns
`Ok`; the node must finish the job itself: `take_deferred_approval` → `review_action` →
`approvals.create` → `request_approval` → `authorise`. That sequence is written by hand six
times, each slightly different:

| Driver | Where |
| --- | --- |
| `fs.apply` | `crates/zvs-host/src/files/nodes.rs` (~1190) |
| `shell.run`, skill scripts, background shells | `commands/nodes.rs` `Tools::approve` |
| `browser.*` | `browser/nodes.rs` `Browser::authorise` |
| `computer.*` | `computer/nodes.rs` `Computer::authorise`, `request_apps` |
| integration tools (mail, git) | `integrations/tools.rs` |
| folder access | `workspace/service.rs` `confirm` |

A node that defers and skips a step runs unapproved and the compiler cannot notice
(`docs/APS_AUDIT.md`, M18). TASK_077 built the pipeline; this task moves the drivers onto it.

## Why

Six copies of a security check drift. Three already have: computer use reviews sensitive input
as `ReviewedAction::Browser` (M19), scenarios do not raise destructive tools to `ask` while chat
and skills do (M20), and MCP tools carry no effect at all (L21).

## Depends on

- `TASK_077` — `AccessRequest`, `Effect`, `AccessPipeline`.

## Read first

- `/docs/APS_AUDIT.md` — *Drivers* and *APS → What changes in behaviour*
- `/docs/PASSPORT.md` §8 (*Deferred approval*, *Per-input permission*), §14, §21, §22
- `crates/zvs-host/src/kernel/{registry,scheduler,types}.rs`
- the six files in the table above

## Scope

**In scope**
- `NodeDef` gains `prepare(&self, context, input) -> Prepared` and
  `access(&self, &Prepared) -> AccessRequest`; `execute` takes `Admitted<Prepared>`.
  `Admitted` has a private constructor in `access/`, so only the pipeline can make one.
  Nodes with nothing to prepare get default implementations.
- The scheduler runs `prepare → pipeline → execute` for graph steps and model tool calls alike.
- One approval builder in `access/` turns an `AccessRequest` into the `ApprovalRequestDto`
  kinds that exist today (`files`, `folder`, `process`, `apps`, `mail`, browser, plain). The
  DTOs do not change.
- Port the six drivers; delete `defers_approval`, `DeferredApproval`, `take_deferred_approval`
  and the per-driver `authorise` functions.
- `Effect::Desktop { apps, sensitive }` for computer use. Sensitive input into a terminal
  window is decided as the `Process` family, not `Browser`.
- Scenario tool steps pass `destructive` as chat and skills do.
- MCP tools map `readOnly` → `Effect::Read`, `destructive` → `Effect::Opaque { destructive: true }`.
- "Every call asks" (mail) becomes an explicit `always_ask` on the request, not a missing
  review call.

**Out of scope** (do not do these, even if tempting)
- Changing rules, modes or the vendor pack — TASK_079.
- Merging the grant tables — TASK_082. Folder, site and app grants stay in their stores and are
  read by a `GrantsStage` adapter.
- Renderer changes. Approval DTOs and events stay byte-identical.

## Steps

1. Add `prepare` / `access` / `Admitted` to the kernel with defaults; move the scheduler.
2. Port `shell.run` first (smallest), then `fs.apply`, browser, computer, integrations, folders.
3. Delete the deferral API once nothing uses it.
4. Fix the three drifts (computer sensitive, scenarios, MCP) with a test each.
5. Add a kernel test: a node registered without `access` and with a side effect is refused at
   registration.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Typing into a granted terminal window through computer use is a process launch that no
> process rule sees. Should it (a) always ask, in every mode except no-restrictions, (b) be
> refused outright — terminals are driven through shell.run only, or (c) stay as today?"
>
> **Why it matters:** (b) closes the path but removes a capability people may use; (a) keeps it
> with a prompt.
> **Options:** a / b / c as above.
> **If the user is unavailable:** take (a) and note it in Handoff.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p zvs-host
cargo test -p zvs-contract
pnpm test
```

Manual check: with a chat bound to a folder, run an `fs.apply` overwrite, a `shell.run`, a
`browser.submit`, a computer-use click and a mail send. Each approval looks as before; the mail
send asks in `no-restrictions` too.

## Definition of done

- [ ] No node calls `review_action`, `approvals.create` or `request_approval` directly
- [ ] `defers_approval` and `DeferredApproval` are gone
- [ ] `execute` cannot be called without `Admitted`
- [ ] Computer sensitive input, scenario destructive tools and MCP effects covered by tests
- [ ] Golden fixtures unchanged
- [ ] PASSPORT §8 rewritten for prepare / access; driver READMEs updated
- [ ] Rust and renderer gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
