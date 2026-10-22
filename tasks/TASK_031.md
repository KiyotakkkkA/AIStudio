---
task: TASK_031
title: Tool registry and the permission ceiling
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_023]
unblocks: [TASK_032, TASK_034, TASK_035, TASK_045, TASK_048]
estimate: 3 h
---

# TASK_031 — Tool registry and the permission ceiling

## Context

TASK_023 built permission admission for kernel node types. This task generalises it into the **tool registry** the Tools page presents: one place where every capability the app can exercise is registered, described and given a ceiling.

The canvas note states the rule the whole Extend group shares: a capability is registered once — built-in, discovered from an MCP server, or granted by a connected account — given a ceiling of `auto` / `ask` / `off`, and a skill or scenario may narrow it further but never widen it. `/design/Tools.dc.html` is that model made visible: a table of tools with their source and permission, filters by source and tier, and a detail rail with the input schema and what uses it.

This task is deliberately positioned before MCP, Skills, the browser and Integrations, because all four register into it.

## Why

Four later subsystems add capabilities. If each invents its own registration and permission handling, the security model becomes four models and the Tools page becomes a lie.

## Depends on

- `TASK_023` — Node registry, permission admission, approval gates.

## Read first

- `/design/Tools.dc.html` — the table, the tiers, the detail rail with the input schema
- `/docs/PASSPORT.md` §14 (the ceiling), §11 (nodes and admission), §21 (how the browser composes with it)

## Scope

**In scope**
- `tool` table: every registered capability with source, schema, description, tier
- Registration API used by built-ins, MCP, skills, integrations and the browser
- Effective-permission resolution: ceiling composed with narrowing scopes
- Usage tracking: last used, call counts, which scenarios and skills reference it
- Channels and DTOs
- Tests, especially that narrowing cannot widen

**Out of scope** (do not do these, even if tempting)
- The Tools page (TASK_034)
- MCP discovery (TASK_032) — this provides the registration target
- Rate limiting (TASK_047 for the browser)

## Steps

1. `tool` table: id, key (`qdrant.search`), source (`builtin` | `mcp` | `skill` | `integration` | `browser`), sourceRef, displayName, description, inputSchema JSON, destructive flag, tier, enabled, lastUsedAt, callCount.
2. `ToolRegistry.register(descriptor)` — idempotent by key, so a re-discovering MCP server updates rather than duplicating. Deregistration marks tools unavailable rather than deleting rows, so history and references survive a server going offline.
3. Default tiers on registration: read-only tools default to `auto`, anything with side effects to `ask`, anything flagged destructive to `off`. Never default a destructive tool to anything else, and make that rule a single function so it cannot drift.
4. Effective permission: compose the global ceiling with narrowing scopes (skill, scenario, site). Reuse TASK_023's most-restrictive-wins resolver rather than writing a second one — if the shapes differ, refactor TASK_023's into this registry.
5. Bridge to the kernel: node admission resolves through the registry, so a tool disabled on the Tools page immediately stops being executable everywhere.
6. Usage tracking on every invocation, plus a reverse index of which scenarios and skills reference a tool, for the detail rail's 'Used by'.
7. Channels: `tools.list|get|setTier|setEnabled`, `tools.usage`.
8. Tests: registration is idempotent; default tiering is correct per class; narrowing cannot widen; disabling a tool blocks execution at admission; deregistering keeps history.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Register a built-in tool, set it to `off` through the channel, and confirm a kernel run that uses it fails admission with `PERMISSION_DENIED` rather than executing.

## Definition of done

- [x] One registry for every capability source
- [x] Destructive tools cannot default to anything but `off`
- [x] Effective permission resolution shared with TASK_023, not duplicated
- [x] Disabling a tool blocks it everywhere immediately
- [x] Usage and reverse references tracked
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Migration 0016_tool_registry adds `tool` (key, source, sourceRef, displayName,
description, inputSchema, readOnly, destructive, tier, enabled, available, lastUsedAt,
callCount, registeredAt, updatedAt) and `tool_reference` (the reverse index), with
`ToolRepository` behind them. `ToolRegistry` in `host/tools/` registers idempotently by key,
deregisters by key or by source without deleting rows, lists and filters, sets tier and
enabled state, records usage and replaces the per-subject reference index.
`defaultToolTier` is the only place a default is decided: destructive to `off`, read-only to
`auto`, everything else to `ask`. `registerKernelTools` registers every kernel node that
carries a permission requirement as a built-in capability, deriving the input schema from the
node's zod input, minus the keys in its `NOT_TOOLS` set. `PermissionService.admitSubject` — still the single resolver from TASK_023 —
now denies a disabled or unavailable tool outright and folds the registered tier into the same
most-restrictive-wins composition. `RunService.admitTool` checks the registry for every run that
goes through admission and records a use once admission passes. Channels
`tools.list|get|setTier|setEnabled|usage` with DTOs in `@zvs/shared`. 22 regressions in
`apps/studio/tests/tools.test.ts`; `data.test.ts` covers the new migration.

**Deviated from plan:** The descriptor distinguishes tiers by `readOnly` rather than an
inverted side-effect flag, matching the `read-only` chip in `/design/Tools.dc.html`.
`tool.tier` is the canonical ceiling and is mirrored into the `tool_permission` global row on
every registration and `setTier`, so TASK_023's resolver keeps working unchanged for subjects
that have no tool row; admission also takes the most restrictive of the two, so the mirror can
never be the thing that widens a capability. A capability that becomes destructive on
re-registration is pushed back to the default `off`; otherwise a re-registration keeps the
decided tier, the enabled flag and the usage counters. Deregistration sets `available: false`
and admission denies it, which also keeps a withdrawn MCP server's tools out of the executable
set. `PermissionTier` is now an alias of the shared `ToolTier`.

Chat is permission-free by product decision, so TASK_023's implicit per-run-kind grants come
first and the registry ceiling is not consulted for them: a `chat` run's `llm.generate` and
`vector.search`, an `indexing` run's `vector.index` and a `download` run's `download.fetch`
still execute with the tool disabled on the Tools page. The DoD's "blocks it everywhere"
therefore means every run that goes through admission at all — scenario, agentic, job, browser
— which is the same exemption TASK_023 shipped, not a new hole. Generation is not a tool:
`llm.generate` is in `NOT_TOOLS` and never reaches the registry, so it has no row, no ceiling
and no entry on the Tools page. Its `NodeDef` keeps its permission requirement, so scenario and
agentic runs still admit against the `tool_permission` rows the way TASK_023 wrote them and the
Chat approval bar's `llm.generate` subject is unchanged.

**Next session should know:** MCP, skills, integrations and the browser register through
`ToolRegistry.register(descriptor)` with their own `source`/`sourceRef`, and withdraw through
`deregisterSource(source, sourceRef)`. Scenarios and skills should call
`recordReferences({ kind, refId, name }, keys)` on save — it replaces that subject's rows, so
it is the whole 'Used by' contract. Tool keys must match `ToolKey` (a dotted key such as
`qdrant.search`). `tools.setTier` and `tools.setEnabled` take the tool id, not the key.
Built-in registration happens in `main.ts` after the node registry is assembled; a node whose
permission is `{ kind: "none" }` is deliberately not a tool. Studio tests: 555 passed, 2
existing skips, 1 failure — `apps/studio/tests/renderer/graphRelations.test.ts` expects
`relationName` to return `user_linked` and gets the Russian label; it fails on `main` without
this change and is untouched here. `pnpm typecheck`, `pnpm lint` and `pnpm build` all pass, but
only with the Electron dev instance closed: they run `build:rust` first, which fails with EBUSY
while the app holds `zvs-core.node`. `pnpm lint:tailwind` has two pre-existing warnings in
`VectorDocumentsPanel.tsx`.
