---
task: TASK_078
title: Drivers on the pipeline — prepare, access and the Admitted token
phase: A — Access pipeline (APS)
status: done
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

- [x] No node calls `review_action`, `approvals.create` or `request_approval` directly
- [x] `defers_approval` and `DeferredApproval` are gone
- [x] `execute` cannot be called without `Admitted`
- [x] Computer sensitive input, scenario destructive tools and MCP effects covered by tests
- [x] Golden fixtures unchanged
- [x] PASSPORT §8 rewritten for prepare / access; driver READMEs updated
- [x] Rust and renderer gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- Kernel (`kernel/types.rs`, `registry.rs`, `scheduler.rs`): `NodeDef` has `type Prepared`,
  `prepare`, `access`, `execute(Admitted<Prepared>)` and `refused`; `PlainNode` is the short form
  for nodes with nothing to prepare (blanket `impl<T: PlainNode> NodeDef for T`, `Prepared =
  Input`), and its `execute` also takes `Admitted`. The registry runs `screen → prepare → access
  → admit → execute` for graph steps and model tool calls alike; a destructive minimum is applied
  after `access` (`AccessRequest::raised`). `StepContext` carries the gate (`SharedGate`);
  `SchedulerOptions::admit` and `RunServiceOptions::admit` take an `AccessGate`.
- `access/`: `Clearance` and `Admitted` (`admitted.rs`; only `access/` makes a `Clearance`),
  `AccessGate` and `Gatekeeper` (`gate.rs`: floor → tier, review through `ExecReview` with the
  node's effect, resource grants, tool approval, recheck, record), and the one approval builder
  (`person.rs`) for the plain, `process`, `files`, `folder`, `apps`, desktop, browser and
  integration kinds, with the old fields and key order. `AccessRequest` gained `gating`,
  `always_ask` and `prompt` (action, scope, fields, denial message, grant prompt, `Attendant`);
  `Effect` gained `Read`; `Resource::Folder` carries `creates`, `Resource::App` an icon.
- The six drivers moved: `shell.run` (plan and cwd folder in `prepare`), `fs.apply` (plan, quota
  and ledger in `prepare`, merged folder + diff modal), the browser (origin, element and site
  block in `prepare`; `BrowserStep::scope`; an attendant keeps the agent panel events), computer
  use (`ComputerStep::plan`; missing apps as resources; app answers through an attendant),
  integrations (`always_ask` with the provider's plan) and folder access (`FolderAccessService::
  plan`, `FolderAccess::describe`, a folder attendant over the pending-answer map).
  `defers_approval`, `DeferredApproval`, `take_deferred_approval`, `review_action`, the
  per-driver `authorise` / `approve` / `request_apps` / `confirm` functions are gone.
- Drifts: `Effect::Desktop { apps, sensitive }`; sensitive input into a terminal window
  (`computer/policy.rs` `TERMINAL_APPS`) is the `Process` family and not reviewable (M19).
  Scenario tool steps pass `destructive` (`scenarios/nodes.rs` `tool_request`, M20). MCP targets
  carry `destructive`: read-only → `Effect::Read`, destructive → `Opaque { destructive: true }`,
  otherwise `Opaque { destructive: false }` (L21).
- Registration refuses a node with a side effect whose `permission()` is `None`
  (`UNDECLARED_ACCESS`); kernel-owned steps (`chat.prepare`, `chat.persist`, scenario outputs,
  subagents) declare the new `PermissionRequirement::Kernel`.
- Tests: registration refusal, admission before execution and the no-gate refusal, MCP effects
  through a recording gate (`kernel/tests.rs`); terminal input family and the four modes,
  `always_ask` reaching the person in `no-restrictions` with the mail key order
  (`access/tests.rs`); the scenario destructive tier (`scenarios/tests.rs`); `TERMINAL_APPS` ⊂
  sensitive apps (`computer/policy.rs`). The mail node test now goes through a real
  `Gatekeeper`. Every existing driver test passes unchanged in its assertions.
- PASSPORT §11 (*Prepare, access, execute*, *One person step*, replacing *Deferred approval*),
  §14 (*Execution policy*, *Access pipeline*), the integrations and browser passages;
  READMEs of `access`, `kernel`, `commands`, `files`, `workspace`, `computer`, `integrations`.

**Deviated from plan:**

- DECISION asked: the user chose (a). Terminal input asks in every mode but `no-restrictions`:
  `Effect::reviewable()` is false for it, so the mode stage asks instead of handing it to rules
  or the judge.
- `access(&self, &Prepared, base: AccessRequest) -> AccessRequest` takes the base request the
  kernel builds from `permission_for`, so a node only refines it and the actor stays the gate's.
  Default implementations come through `PlainNode`, because stable Rust has no defaults for
  associated types.
- "Registered without `access`" is read as "a side effect with `PermissionRequirement::None`":
  a trait cannot tell whether a default method was overridden.
- There is no `GrantsStage` in the stage list. The folder and app stores compute what is
  uncovered (the folder plan in `prepare`, the computer grants), the node declares it as
  resources, and the person step asks with the store's `Attendant`. A grant met during
  `execute` (`fs.list` / `fs.read` / `fs.search` through `ensure_access`, a window
  `computer.inspect` meets) goes through `access::grant` or `StepContext::admit`.
- Resource grants are asked after the review and before the tool approval. `shell.run` with an
  uncovered `cwd` used to ask for the folder first; now a rule `deny` refuses it without a
  folder prompt. Every tool approval is followed by the tier recheck (only the plain gate did
  it before).
- The step timeout covers `execute` only, as before it did not cover admission.
- An `approved_per_call` integration tool whose tier allows used to fail with
  `APPROVAL_UNAVAILABLE`; it now asks.
- The unused approval services were removed from `Browser::new`, `Computer::new`,
  `register_command_nodes`, `ShellRunNode::new` and `IntegrationServiceOptions`.
- Not run: the manual check (`fs.apply` overwrite, `shell.run`, `browser.submit`, a computer
  click, a mail send in the app). The driver tests that assert the approval DTOs, and the new
  `always_ask` test for `no-restrictions`, cover the same paths.

**Next session should know:**

- `access/person.rs` is the only place that calls `approvals.create` and `request_approval`;
  TASK_082 can replace the folder and app attendants with one grant store behind the same
  resource list.
- `Gating::Open` is used for steps with nothing to admit, for an `fs.apply` batch that changes
  nothing and for `computer.request_access` (its apps prompt is the approval).
- Test-only helpers: `access::OpenGate`, `access::admitted` and `ShellRunNode::run` exist under
  `cfg(test)`.
