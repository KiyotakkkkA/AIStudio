---
task: TASK_034
title: Tools page
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_031]
unblocks: []
estimate: 3 h
---

# TASK_034 — Tools page

## Context

`/design/Tools.dc.html` presents the permission ceiling as a screen: a 230 px rail filtering by source and by tier with live counts; a table of every registered tool showing name, description, source chip, permission chip and last used; and a 340 px detail rail with the tool's description, tier segmented control, input schema, what uses it, and a try-it panel that runs the tool with a JSON input.

The note in the left rail states the rule the page enforces: *permission set here is the ceiling; a skill or scenario can narrow it, never widen it*. The try-it panel is genuinely useful — being able to invoke a tool by hand with a known input is how you tell a broken tool from a bad prompt.

## Why

It is the single place a user can see and control everything the app is able to do on their behalf. Without it the permission model is invisible and therefore untrustworthy.

## Depends on

- `TASK_031` — Tool registry and the permission ceiling.

## Read first

- `/design/Tools.dc.html`
- `/docs/PASSPORT.md` §14

## Scope

**In scope**
- `features/tools/` store and organisms
- Filterable table over the registry
- Detail rail with schema rendering and 'Used by'
- Tier changes writing to the registry
- Try-it panel invoking the tool through a channel and showing the result
- Permission-defaults dialog from the header

**Out of scope** (do not do these, even if tempting)
- Editing schemas
- Per-scope narrowing UI — narrowing is set on the skill or scenario that does it

## Steps

1. `ToolStore` over `tools.list`, with source and tier filters and a text search.
2. Table rows exactly as drawn, including the dimmed row for tools that are `off`.
3. Detail rail: description, chips, tier segmented control, input schema rendered readably from JSON Schema (required fields marked, defaults shown), 'Used by' list linking to the scenario or skill.
4. Try-it: a JSON input box seeded from the schema's defaults, a Run button, and the result with timing. Destructive tools require an explicit confirm before running here, exactly as they would in a run.
5. Permission defaults dialog: set the default tier per source class, applied to newly-registered tools only — changing a default must never silently re-grant something the user previously turned off.
6. States: no tools, source offline (MCP server down — show why), tool unavailable.
7. Tests: filter logic; schema rendering of a representative JSON Schema; the destructive-confirm path.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Set a tool to `off`, run a chat turn that would use it, and confirm it is refused. Use try-it to invoke a read-only MCP tool and see a real result.

## Definition of done

- [x] Page matches the mockup
- [x] Tier changes take effect immediately everywhere
- [x] Schema rendered readably; try-it invokes for real
- [x] Changing defaults never re-grants a previously disabled tool
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Channels `tools.invoke`, `tools.defaults` and `tools.setDefaults` in the contract, with
`ToolInvokeInput`, `ToolInvokeResultDto`, `ToolDefaultsDto` and the shared
`toolNeedsConfirmation` rule (destructive, or tier `ask`) in `@zvs/shared`. `host/tools/ToolInvoker.ts`
runs a try-it call as a one-node `job` run, so it goes through the same admission, approval gate,
usage counter and Tasks/Runs history as any other run. It refuses `off`, disabled and unavailable
tools before starting anything, refuses an unconfirmed destructive or `ask` call with
`APPROVAL_DENIED`, and on a confirmed call answers the run's approval request itself. The result
carries the run id, the step's output, timing and the error, including an MCP `isError` result.
`ToolRegistry` gained `defaults()` and `setDefaults()`, persisted under the `tools.defaults`
setting: a per-source-class tier (`inferred | auto | ask | off`) used only on the
insert path of `register`, and a destructive tool always starts `off`. `features/tools/` holds
`ToolStore` (registry and MCP server list, source/tier/text filters with faceted counts, selection,
detail, tier writes, try-it, defaults draft, 5 s polling while mounted), pure `toolFilters.ts`,
`schemaFields.ts` (JSON Schema to fields with types, required marks, defaults, nesting; input
seeding; JSON parsing) and `toolPresentation.ts`, plus the organisms `ToolFilterRail`, `ToolTable`,
`ToolDetailRail`, `SchemaView`, `TryItPanel`, `PermissionDefaultsDialog` and `ToolsWorkspace`.
Tests: 6 new in `tests/tools.test.ts` (defaults never re-grant, refusals, destructive confirm,
`ask` approval through the confirm, failure reporting, channels) and 12 in
`tests/renderer/tools.test.ts` (filter logic and counts, schema rendering of a representative
schema, seeding, offline copy, the store's destructive-confirm path, host refusals, empty state).

**Deviated from plan:** The tier control shows the effective tier: a tool switched off with the
Connections toggle (`enabled: false`) shows as `off`. Raising it to `auto` or `ask` here also
re-enables it, because that is an explicit grant by the user, not a silent one. Setting `off` only
writes the tier. Sources are grouped as "Built-in" plus one entry per MCP server, named by its
configured name. Other source classes group by `sourceRef` once they exist. "Used by" links go to
`/scenarios?id=…` and `/skills?id=…`; those pages are still placeholders and ignore the parameter.
The try-it confirm is required for `ask` tools too, not only destructive ones, because a run would
ask for them as well.

**Next session should know:** Manual check against the built app with Playwright-Electron and
`test/helpers/fakeMcpServer.mjs`: try-it on `qdrant.search` returned a real result
(`search:{"query":"rotate token"}`, ~50 ms); a destructive tool raised to `auto` opened the confirm
dialog; `qdrant.upsert` set to `off` on the page failed a scenario run at admission with
`PERMISSION_DENIED` and was refused by `tools.invoke`; disabling the server showed every one of its
tools as unavailable with the reason and a link to Connections. "Run a chat turn that would use
it" cannot be checked yet: chat does not offer tools to the model until TASK_034_EXTRA_1, and
TASK_031's chat exemption still applies to `vector.search`. Stopping an MCP server does not make
its tools unavailable, because a call (try-it included) starts it again; only disabling or removing
the server withdraws them. Studio tests: 750 passed, 2 existing skips. `pnpm typecheck`,
`pnpm lint`, `pnpm lint:tailwind` and `pnpm build` pass. `pnpm test:e2e` has 3 failures
(`chat.spec.ts`, the secret-channels case in `smoke.spec.ts`, `vectorStores.spec.ts`); the same
3 fail on `main` without this change. `pnpm format:check` still fails only on the files that
already failed before TASK_033.
