---
task: TASK_077
title: APS core — access request, decision and the staged pipeline behind the current gate
phase: A — Access pipeline (APS)
status: todo
depends_on: [TASK_048_EXTRA_1]
unblocks: [TASK_078, TASK_079, TASK_083]
estimate: 4 h
---

# TASK_077 — APS core: access request, decision and the staged pipeline behind the current gate

## Context

A tool call is admitted today by three things that grew separately (PASSPORT §14): the tier on
the Tools page (`PermissionService::admit_subject`, `crates/zvs-host/src/services/permissions.rs`),
grants that say where a tool may act (`workspace/`, `browser/grants.rs`, `computer/`), and the
execution policy that answers a call which would ask (`crates/zvs-host/src/policy/`). They meet
in `Gate::admit_tool` (`services/runs.rs`) and again, by hand, inside every node that defers its
approval.

`docs/APS_AUDIT.md` (2026-11-10) scores this 5 / 10 and proposes the **access pipeline system
(APS)**: one engine with ordered stages — floor, hard rules, grants, mode, rules, judge, person,
audit — that every call passes through. This task builds the engine and puts it *behind the
existing gate with no change in behaviour*. Later tasks move drivers and rules onto it.

## Why

Every later APS task needs one place where a decision is made and recorded. Building it first,
behaviour-neutral, lets the existing kernel, policy and permission tests prove nothing moved.

## Depends on

- `TASK_048_EXTRA_1` — the execution policy, rule packs and judge this task wraps.

## Read first

- `/docs/APS_AUDIT.md` — sections *How a call is admitted today* and *APS*
- `/docs/PASSPORT.md` §14 *Execution policy*, *Judge*, *Audit*; §8 kernel admission
- `crates/zvs-host/src/services/runs.rs` — `Gate`
- `crates/zvs-host/src/policy/{action,table,service}.rs`
- `crates/zvs-host/src/kernel/types.rs` — `ExecReview`, `StepContext`

## Scope

**In scope**
- A new module `crates/zvs-host/src/access/` with `AccessRequest` (actor, tool, effect,
  resources), `Effect` (covering today's `ReviewedAction` variants plus `Desktop`, `Remote`,
  `Opaque`), `Decision { outcome, stage, source, reason }` and a `Stage` trait.
- `AccessPipeline`: an ordered list of stages, the first non-`Continue` result wins, and the
  trail of visited stages is kept on the decision.
- Stages wrapping today's code unchanged: `FloorStage` (graph allow-list, tool enabled, tier
  `off`), `TierStage` (`admit_subject`), `ModeStage` (`decide`), `RulesStage`
  (`RuleSet::evaluate`), `JudgeStage` (`ask_judge`).
- `Gate::admit_tool` and `ExecPolicyService::review` call the pipeline; `ReviewedAction`
  converts into `Effect`.
- Remove `Family::Floor` from `policy/table.rs` together with its tests: nothing produces it,
  and the floor is `FloorStage` now.
- `access/README.md`; PASSPORT §14 gains an *Access pipeline* paragraph naming the stages.

**Out of scope** (do not do these, even if tempting)
- Changing any decision. Every existing test must pass unmodified, apart from the removed
  `Floor` rows.
- Touching the nodes that defer approval — TASK_078.
- Rules, modes, the vendor pack — TASK_079.

## Steps

1. Add the types and the pipeline with a table-driven test: stage order, first decision wins,
   trail recorded, an error in a stage is `Ask` for decision stages and an error for the floor.
2. Wrap the existing checks as stages; keep their modules where they are.
3. Route `Gate::admit_tool` and `ExecPolicyService::review` through the pipeline.
4. Port the PASSPORT decision table test to run against the pipeline end to end.
5. Write the README and the PASSPORT paragraph.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p zvs-host access:: policy:: kernel::
cargo test --workspace
```

Manual check: in each of the four modes run `git status`, an `fs.apply` overwrite and a
`browser.click` in a chat; prompts and the tool call's authorisation line are the same as
before the change.

## Definition of done

- [ ] `access/` exists with the pipeline, the five wrapping stages and tests
- [ ] `Gate` and `ExecPolicyService` decide only through the pipeline
- [ ] No behaviour change: existing policy, kernel and permission tests pass unmodified
- [ ] `Family::Floor` removed
- [ ] PASSPORT §14 and `access/README.md` updated
- [ ] Rust gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
