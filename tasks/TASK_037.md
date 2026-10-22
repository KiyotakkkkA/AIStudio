---
task: TASK_037
title: Scenario model, versioning and validation
phase: 7 — Scenarios and Agentic Build
status: todo
depends_on: [TASK_023, TASK_076]
unblocks: [TASK_038]
estimate: 3 h
---

# TASK_037 — Scenario model, versioning and validation

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

Scenarios are the n8n-shaped automation builder from `/design/Scenario.dc.html`. This task defines what a scenario *is* — the persisted graph, its versions and its validation — before any canvas is drawn.

A scenario is a user-authored DAG over the node types in the TASK_023 registry. The mockup shows the shape it must support: a trunk running left to right (trigger → agent → action) plus **sub-nodes hanging below on dashed ports** — a chat model, a memory, a tool attached to the AI Agent node. That second kind of edge is not decoration; it is a different connection class, and the model has to represent it explicitly.

Versioning matters because a scenario that is running should not change under itself. Saving creates a new version; a run pins the version it started with. The mockup's `v14` chip is that.

## Why

TASK_038's editor and TASK_039's execution both serialise to this model. Getting the graph representation and the sub-node concept right first avoids reworking both.

## Depends on

- `TASK_023` — Node registry, permission admission, approval gates.

## Read first

- `/design/Scenario.dc.html` — the trunk, the dashed sub-node ports, the version chip, the node inspector's fields
- `/docs/PASSPORT.md` §11 (a scenario is a user-authored DAG over `NodeDef`s)

## Scope

**In scope**
- `scenario` and `scenario_version` tables plus migration
- Graph schema: nodes, main edges, sub-node attachments, positions, node parameters
- Validation: unknown node types, cycles, missing required attachments, unreachable nodes, unsatisfied required inputs
- Version-on-save with a diff summary
- Active/inactive state and trigger binding
- Channels, DTOs, tests

**Out of scope** (do not do these, even if tempting)
- The canvas editor (TASK_038)
- Execution (TASK_039)
- Importing n8n workflows unless the decision below says otherwise

## Steps

1. `scenario` table: id, name, tags, active, currentVersionId, timestamps. `scenario_version` table: id, scenarioId, version, graph JSON, note, createdAt.
2. Graph schema in `packages/shared`, Zod-validated, with its serde mirror and golden fixtures in `crates/zvs-contract` (the TASK_059 recipe): `nodes[]` (id, type, position, params), `edges[]` (from, to, fromPort, toPort) and `attachments[]` (parentNodeId, childNodeId, slot — `model` | `memory` | `tool` | `embedding`). Keeping attachments separate from edges is what makes the dashed-port rendering and validation straightforward.
3. Validation as a pure function returning structured issues with node ids, so the editor can highlight them: unknown node type; cycle in the main graph; a required attachment slot empty (the mockup marks `Chat model *` required); a required parameter missing; an unreachable node; two triggers in one scenario if that is disallowed.
4. Version on save: write a new `scenario_version`, move `currentVersionId`, keep history. Generate a short diff summary (nodes added, removed, parameters changed) for the version list.
5. Trigger binding: a scenario's trigger node declares what starts it — a chat message, a mail arrival, a schedule, a webhook. Store the binding so TASK_048's integrations can fire it.
6. Activation: activating validates first and refuses to activate an invalid scenario, with the issues attached to the error.
7. Channels: `scenarios.list|get|create|update|remove|validate|activate|versions`.
8. Tests: every validation rule with a graph that violates it; version-on-save preserves history; activation is refused for an invalid graph; attachments round-trip.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Should scenarios use their own format, or should the app be able to import and export n8n workflow JSON? The editor is n8n-shaped visually, but matching their format is a real constraint on the model."
>
> **Why it matters:** n8n compatibility would let you bring existing workflows in, but it ties the schema to someone else's evolving format and their node semantics, which do not map cleanly onto this app's kernel and permission model.
> **Options:** (a) own format, clean and matched to the kernel — no import; (b) own format plus a best-effort n8n importer that maps what it can and reports what it could not; (c) n8n-compatible format as the native representation.
> **If the user is unavailable:** take (a) for v1 and note (b) as a candidate later task. The kernel's permission model has no n8n equivalent, so (c) would compromise it.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Create a scenario through channels with a deliberately invalid graph (missing required chat model) and confirm activation is refused with the offending node id named.

## Definition of done

- [ ] Graph schema represents main edges and sub-node attachments distinctly
- [ ] Validation covers every rule, returning node-addressable issues
- [ ] Version-on-save with history and diff summaries
- [ ] Invalid scenarios cannot be activated
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
