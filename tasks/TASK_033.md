---
task: TASK_033
title: Connections page
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_032]
unblocks: []
estimate: 3 h
---

# TASK_033 — Connections page

## Context

The page for MCP servers, per `/design/Connections.dc.html`: a 262 px server list with status dots and transport chips; a detail column with the Server setup card (name, transport segmented control, startup mode, command, arguments, the environment table with secret pickers, Test connection and its result line) and an Exposed tools card listing discovered tools with per-tool toggles and tier chips; and a 286 px health rail with uptime, call counts, latency, errors, what uses the server, and a recent log tail.

## Why

Without it, MCP servers can only be configured by editing the database, which makes the app's main extension point unusable.

## Depends on

- `TASK_032` — MCP client driver — servers, discovery, invocation.

## Read first

- `/design/Connections.dc.html`
- `/docs/PASSPORT.md` §13

## Scope

**In scope**
- `features/connections/` store, form view-model, organisms
- Server CRUD with the transport-dependent form
- Env table with literal-or-secret rows
- Test connection with the real handshake result
- Tool list with enable toggles and tier selection, writing through the tool registry
- Health rail and log tail
- Add-from-registry entry point (a simple curated list is fine)

**Out of scope** (do not do these, even if tempting)
- OAuth flows unless TASK_032's decision included them
- Editing tool schemas — they come from the server

## Steps

1. Store and form view-model following the established pattern. The form's fields change with transport: stdio shows command, args and env; HTTP shows URL and headers.
2. Env table: each row is a key plus either a literal value or a secret reference, rendered with the key icon as in the mockup. Adding a row offers both.
3. Test connection: call the driver, render the handshake line (`handshake ok · 128 ms · protocol … · server v…`) or the failure with useful copy.
4. Tool list: per-tool toggle and tier chip. Changing a tier writes through the tool registry so the Tools page agrees immediately — do not keep a second copy of that state.
5. Health rail from the driver's stats; log tail from the server's stderr, capped and scrolled.
6. States: never connected, connected, needs re-authorisation, disabled, crashed-and-restarting.
7. Tests: form view-model transport switching; tier changes propagate to the registry.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Add a real local MCP server through the page, test it, enable two tools and disable one, then confirm the Tools page reflects exactly that.

## Definition of done

- [x] Page matches the mockup
- [x] Server CRUD and Test connection work against real servers
- [x] Env secrets are pickers, never free text
- [x] Tier changes are single-sourced through the registry
- [x] Health and logs render real data
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Channels `mcp.list|get|create|update|remove|start|stop|test|health` in the contract,
served by `host/ipc/mcp.ts` over the TASK_032 `McpService` and wired in `main.ts`.
`McpHealthDto` gained `restarting` (a backoff retry is pending), which is what tells
"crashed and restarting" apart from "failed for good". `features/connections/` holds
`ConnectionStore` (list, selection, detail, registry tool rows, used-by, health for every
server, 3 s polling while the page is mounted), `ServerFormVm` (transport-dependent form, env
and header tables of literal-or-secret rows, args tokeniser in `args.ts`), presentation and
error copy, a curated `serverRegistry.ts` (filesystem, memory, fetch, git, time,
sequential-thinking, qdrant, github, everything) and the organisms: `ServerList`,
`ServerSetupCard`, `ValueTable`, `TestResultLine`, `ExposedToolsCard`, `TierPicker`,
`HealthRail`, `RegistryDialog`, `ConnectionsWorkspace`. Tests: 12 in
`tests/renderer/connections.test.ts` (transport switching, validation, presets, tier and
enable writes through the registry, used-by, save-then-test, states) and 3 more in
`tests/mcp.test.ts` (channel round trip through the contract, a tier set through `tools.*`
on an MCP tool, the `restarting` flag).

**Deviated from plan:** The mockup's "Import config" header button and the "Open full log"
button are not built — import is not in scope, and the log is the host's 50-line in-memory
tail, which the rail already shows whole and scrollable. "Add from registry" opens a catalogue
dialog; a header "Каталог" button opens the same one. "Needs re-authorisation" is derived from
`failed` plus a 401/403 or credentials error in `lastError`, since v1 has no OAuth (TASK_032
decision). Test connection on a new or edited form saves it first, because
`McpService.test` works on a saved server. The enable switch persists immediately for a saved
server and is not part of the form's dirty state. Tool rows keep the registry's `ToolDto`s as
returned by `tools.list` / `tools.setTier` / `tools.setEnabled` and nothing else, so there is no
second copy of tier or enabled state. Start/Stop buttons sit next to Test connection.

**Next session should know:** The manual check was run against the built app with
Playwright-Electron: a stdio server (`node test/helpers/fakeMcpServer.mjs`) was added through
the form with a literal env row, tested (`handshake ok · 44 ms · protocol 2025-06-18 …`), six
tools discovered, one tool disabled and one tier changed in the page, and `tools.list`
returned exactly those values. The Tools page itself (TASK_034) does not exist yet, so "the
Tools page reflects it" was checked at the registry it will read. A genuine third-party
server was not tried. Kit dropdown menus stay mounted at `opacity: 0`, so e2e selectors for
menu items need `.nth()` or scoping. `pnpm format:check` fails on 12 files that already failed
before this task (migration snapshots and `test/native/sidecar.test.mjs`); none of this
task's files are among them. Studio tests: 680 passed, 2 existing skips.
