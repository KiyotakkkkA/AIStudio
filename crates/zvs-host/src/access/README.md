# access

The access pipeline system (APS): one engine that decides whether a tool call runs, asks the
person or is refused, and records which stage decided it. The proposal and the audit behind it
are in `docs/APS_AUDIT.md`; PASSPORT §14 _Access pipeline_ is the binding summary.

TASK_077 built the engine behind the existing gate with no change in behaviour. TASK_078 moved
every driver onto it: nodes describe what they will do, this module decides, and nothing else in
the host creates or requests an approval.

## Files

| File          | Contents                                                                                                            |
| ------------- | ------------------------------------------------------------------------------------------------------------------- |
| `request.rs`  | `AccessRequest` (actor, tool, gating, minimum tier, effect, resources, `always_ask`, prompt), `Effect`, `Attendant` |
| `decision.rs` | `Decision { outcome, stage, source, reason, trail }`, `Outcome`, `StageName`; audit record and refusals             |
| `pipeline.rs` | `Stage` trait, `Step`, the per-call scratch `Pass`, `AccessPipeline`                                                |
| `stages.rs`   | `FloorStage`, `TierStage`, `HardRulesStage`, `ModeStage`, `RulesStage`, `JudgeStage`                                |
| `gate.rs`     | `AccessGate` (the kernel's port), `Gatekeeper` (every stage and the person in order), `review`                      |
| `person.rs`   | The one approval builder: tool approvals, folder and app grants, `grant` for a grant needed during `execute`        |
| `admitted.rs` | `Clearance` (made only here) and `Admitted<P>` (made only from a `Clearance`)                                       |
| `tests.rs`    | The pipeline contract, the PASSPORT decision table end to end, terminal input, `always_ask`                         |

## A call through the gate

The kernel registry runs `screen → prepare → access → admit → execute` for graph steps and model
tool calls alike (`kernel/registry.rs`):

1. **Screen.** The base request built from `permission_for` passes the floor alone, so a
   disabled, `off` or unlisted tool fails before the node does any work.
2. **Prepare.** The node resolves what the call will do (`NodeDef::prepare`).
3. **Access.** The node refines the base request (`NodeDef::access`): the effect, the resources
   it needs granted, `always_ask`, and the prompt the person sees (action, scope, extra fields,
   denial message, an `Attendant`). A model call with a destructive minimum is raised after this
   (`AccessRequest::raised`), so a node cannot lower it.
4. **Admit.** `Gatekeeper::admit`:
   - `Gating::Open` (no tool, a kernel step, an empty `fs.apply`, `computer.request_access`)
     asks only for the resources it lists;
   - `Gating::AppFolder` runs the floor and the hard rules, records `auto` and asks for
     resources;
   - `Gating::Tool` runs floor → tier, then the hard rules (`ExecReview::refused`: any `deny`
     rule matching the node's effect refuses the call whatever the tier and the mode said,
     TASK_079). An allow records the tier's authorisation. An ask is reviewed through
     `ExecReview` (hard rules → mode → rules → judge) with the node's effect, unless the
     request is `always_ask`. Resource grants are asked next; a `Files` effect that still asks
     merges into the folder modal with the diff. Otherwise the tool approval follows, the tier is
     checked again, and the call is recorded as `approved` with the reason.
5. **Execute.** The node receives `Admitted<Prepared>`. A refused or cancelled admission returns
   the prepared value through `NodeDef::refused`.

`Clearance` has a private constructor, and `Admitted` is made only from one, so a node cannot
execute without passing the gate. Without a gate on the `StepContext`, only an open request with
no resources is cleared.

## Person step

`person.rs` builds every approval kind the renderer knows, with the fields and key order the
drivers produced before: plain (gate), `process`, `files`, `folder`, `apps`, the desktop action
(`apps`, `detail`), the browser action (`origin`, `target`, `text`) and the integration kinds
(`mail` and the rest, from the provider's plan). The scope is the prompt's scope (`site:<origin>`,
`app:<exe>`), else the run's first permission scope, else `global`.

Folder and app answers stay in their stores (TASK_082 merges them). The `Attendant` attached by
the folder service (`workspace/service.rs`) or the computer agent (`computer/nodes.rs`) records
the pending request when it is raised and reads the answer when it is settled; the browser's
attendant reports the approval and the authorisation to the agent panel.

## Effects

`Effect` covers every `ReviewedAction` (`From<ReviewedAction>`, `Effect::reviewed`) and adds
`Read`, `Desktop { apps, sensitive }`, `Remote { service, action }` and
`Opaque { destructive }`. Their family follows what they replace: `Read`, a non-sensitive desktop
action and a non-destructive opaque tool are benign; a remote action and a destructive opaque
tool are `Declared`; sensitive desktop input is `Browser`, except into a terminal window
(`computer/policy.rs` `TERMINAL_APPS`), which is `Process` and not reviewable: the mode stage
asks instead of handing it to rules or the judge, so it runs unasked only in `no-restrictions`.
