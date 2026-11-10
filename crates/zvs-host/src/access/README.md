# access

The access pipeline system (APS): one engine that decides whether a tool call runs, asks the
person or is refused, and records which stage decided it. The proposal and the audit behind it
are in `docs/APS_AUDIT.md`; PASSPORT §14 _Access pipeline_ is the binding summary.

TASK_077 built the engine behind the existing gate with no change in behaviour: the stages wrap
the checks that already existed, in the modules where they already live.

## Files

| File          | Contents                                                                                                |
| ------------- | ------------------------------------------------------------------------------------------------------- |
| `request.rs`  | `AccessRequest` (actor, tool, minimum tier, effect, resources), `Actor`, `Effect`, `Resource`           |
| `decision.rs` | `Decision { outcome, stage, source, reason, trail }`, `Outcome`, `StageName`; audit record and refusals |
| `pipeline.rs` | `Stage` trait, `Step`, the per-call scratch `Pass`, `AccessPipeline`                                    |
| `stages.rs`   | `FloorStage`, `TierStage`, `ModeStage`, `RulesStage`, `JudgeStage`                                      |
| `tests.rs`    | The pipeline contract and the PASSPORT decision table end to end                                        |

## Pipeline

`AccessPipeline::decide` runs its stages in order. Each returns `Step::Continue` or
`Step::Decide(Decision)`; the first decision wins and carries the trail of visited stages. A
pipeline that runs out of stages asks the person (`StageName::Person`).

A stage that fails:

- **fails closed** (`Stage::fails_closed`, the floor and the tier): the error is returned and the
  call does not run;
- otherwise (mode, rules, judge): the decision is `Ask` at that stage with source `error`, and
  the failure is logged;
- a cancelled run (`RunCancelled`) is returned as an error from any stage.

## Stages

| Stage        | Wraps                                                                         | Decides                                                       |
| ------------ | ----------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `FloorStage` | graph allow-list, graph shortcuts, tool enabled, `PermissionService::refused` | deny `unlisted`, `disabled`, `off`; allow `graph` (no record) |
| `TierStage`  | `PermissionService::admit_subject`, `granted`                                 | allow `auto` or `granted`, ask, deny                          |
| `ModeStage`  | `ExecPolicyService::get`, `policy::decide`                                    | allow `policy:<mode>`, ask, or continue on `Review`           |
| `RulesStage` | `ExecPolicyService::rules`, `RuleSet::evaluate`                               | deny, ask, allow by rule; continue on `review` or unmatched   |
| `JudgeStage` | `ExecPolicyService::judge` (`ask_judge`)                                      | allow by the judge or ask                                     |

Two pipelines are assembled today:

- **Admission** (`services/runs.rs`, `Gate`): floor → tier. An app-folder tool runs the floor
  alone. When the tier asks, the gate hands the ticket to a deferring node, or reviews the call
  through the `ExecReview` port and raises the approval.
- **Review** (`ExecPolicyService::pipeline`, behind `ExecReview::review`): mode → rules → judge.
  Deferring nodes reach it through `review_action` as before.

`Decision::authorisation` gives the record kept on the tool call (`ToolAuthorisationDto`), and
`Decision::refusal` the error a denied call fails with; both reproduce what the gate wrote
before. A rule `allow` is still recorded with kind `judge` (audit finding L6, TASK_083).

## Effects

`Effect` covers every `ReviewedAction` (`From<ReviewedAction>`, `Effect::reviewed`) and adds
`Desktop { apps, sensitive }`, `Remote { service, action }` and `Opaque { destructive }`, which
no driver produces yet. Their family follows what they replace: a sensitive desktop action is
`Browser`, a remote action and a destructive opaque tool are `Declared`, the rest are benign.
