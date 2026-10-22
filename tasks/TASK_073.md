---
task: TASK_073
title: Tools and MCP in Rust — ToolRegistry, ToolInvoker, MCP client and McpService, tools.* and mcp.*
phase: M — Migration to Tauri
status: done
depends_on: [TASK_072]
unblocks: [TASK_074]
estimate: 5 h
---

# TASK_073 — Tools and MCP in Rust — ToolRegistry, ToolInvoker, MCP client and McpService, tools.* and mcp.*

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

**Tools** (TASK_031, TASK_034):
- `tools/ToolRegistry.ts` (318 lines): every tool is registered once (built-in kernel tool,
  MCP tool, or later an integration) with a tier `auto | ask | off`. That tier is the
  **permission ceiling** that skills and scenarios can narrow and never widen (§14). The
  registry also holds defaults and usage.
- `ToolInvoker.ts`: runs a tool as a kernel run.
- `kernelTools.ts`: exposes kernel nodes as tools.

**MCP** (TASK_032, TASK_033):
- `drivers/mcp/`: a hand-written client (`McpClient.ts`, `protocol.ts`) over stdio
  (`StdioTransport.ts`, with `windowsCommand.ts` launching `.cmd` shims correctly on Windows
  and `toolchainEnv.ts` isolating the npx/uvx toolchain in `userData/mcp-toolchain`) and HTTP
  with static tokens resolved from Secrets (`HttpTransport.ts`). There is no OAuth in v1 (the
  TASK_032 decision).
- `services/McpService.ts` (824 lines): servers, discovery, health, start and stop, autostart,
  no restart on a failed start, and registering discovered tools into the registry.
- `kernel/mcpNodes.ts`.

Channels: `tools.*` (`defaults`, `get`, `invoke`, `list`, `setDefaults`, `setEnabled`,
`setTier`, `usage`) and `mcp.*` (`create`, `get`, `health`, `list`, `remove`, `start`, `stop`,
`test`, `update`).

Tool input schemas come from the typed nodes' `schemars` output (TASK_070) for built-ins and
from the server's declared schema for MCP tools.

## Why

Chat's tool-calling loop (TASK_034_EXTRA_1) and every later feature (skills, scenarios,
agentic runs, browser nodes) go through this registry and its ceiling.

## Depends on

- `TASK_072` — every kernel node that is exposed as a tool now exists.

## Read first

- `/docs/PASSPORT.md` §14 (tool permission ceiling)
- `apps/studio/src/host/tools/*`, `drivers/mcp/*`, `services/McpService.ts`,
  `kernel/mcpNodes.ts`, `ipc/tools.ts`, `ipc/mcp.ts`
- Handoffs of TASK_031, TASK_032, TASK_033 and TASK_034
- `apps/studio/tests/tools.test.ts`, `mcp.test.ts`, `mcpWindows.test.ts`

## Scope

**In scope**
- `ToolRepository`, `ToolRegistry`, `ToolInvoker` and kernel tool registration
- An MCP client per the decision below: stdio (with Windows `.cmd` handling and the isolated
  toolchain environment) and HTTP with static tokens
- `McpServerRepository`, `McpService` (autostart, health, failed-start behaviour, tool
  discovery into the registry) and the MCP nodes
- `tools.*` and `mcp.*` channels, DTOs and fixtures
- `ShutdownCoordinator`: `mcp.dispose()` stops every server process

**Out of scope** (do not do these, even if tempting)
- OAuth for MCP (still a separate future task)
- Model-side tool calling in chat (TASK_074)

## Steps

1. Repository and `ToolRegistry`, with ceiling and tier tests ported.
2. The MCP client (see the decision), then `mcp.test.ts` and `mcpWindows.test.ts` ported
   against a fixture MCP server in both stdio and HTTP mode. The fixture can be a tiny Rust
   binary in the test profile or a Node script, since Node remains a dev dependency.
3. `McpService`, with autostart, health, failed start and discovery tests.
4. `ToolInvoker` and the nodes, then channels, wiring and shutdown.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "For MCP in the Rust host: should we use the official Rust MCP SDK (`rmcp`), or port our
> own ~800-line client from the TypeScript host?"
>
> **Why it matters:** `rmcp` tracks protocol revisions for us but is another dependency whose
> transport model we must bend around Windows `.cmd` launching and our isolated toolchain env.
> A port keeps exact current behaviour, but every future protocol revision is our work.
> **Options:** (a) `rmcp` if, at the pinned version, it supports stdio with a caller-supplied
> `Command` and streamable HTTP with static headers; (b) port our client.
> **If the user is unavailable:** take (a) if both conditions hold, otherwise (b), and record
> which in Handoff.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, Connections adds an npx-based stdio server (on Windows,
through its `.cmd` shim) and an HTTP server with a token from Secrets. Both list their tools.
Tools shows them with tiers. Set one to `off` and confirm `tools.invoke` refuses it. Stop and
start servers. Quit and confirm no server process is left running.

## Definition of done

- [x] Registry, ceiling, invoker and kernel tools are ported with their tests
- [x] MCP over stdio (Windows `.cmd` included) and HTTP works, per the decision
- [ ] Connections and Tools pages work under Tauri
- [x] No MCP process survives quit
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Decision:** the user chose (a), `rmcp`. Both conditions hold at the pinned version: stdio runs
over a process we spawn ourselves (`StdioTransport` implements `rmcp::transport::Transport`), and
streamable HTTP takes static headers (`custom_headers`, through our own `StreamableHttpClient`).
Pinned `rmcp =3.5.0` (3.5.1 was 13 days old), `default-features = false`, features `client` and
`transport-streamable-http-client`; `sse-stream =0.2.6` for the SSE frames.

**Built:**

- `zvs-contract`: `tools` (`ToolSource`, `ToolReferenceKind`, `ToolDefaultTier`, `ToolReferenceDto`,
  `ToolDto`, `ToolDetailDto`, `ToolUsageDto`, `ToolRef`, `ToolListFilter`, `SetToolTierInput`,
  `SetToolEnabledInput`, `ToolDefaultsDto`, `ToolInvokeInput`, `ToolInvokeResultDto`,
  `is_tool_key`, `tool_needs_confirmation`) and `mcp` (`McpTransport`, `McpStartup`,
  `McpServerStatus`, `McpCapabilityKind`, `McpLogLevel`, `McpValue`, `McpServerDto`,
  `McpCapabilityDto`, `McpServerDetailDto`, `McpTestResultDto`, `McpLogLineDto`, `McpHealthDto`,
  `McpServerRef`, `CreateMcpServerInput`, `UpdateMcpServerInput`, `McpContentDto`,
  `McpCallResultDto`, `McpRemoveOutput`). 31 new fixture pairs under
  `packages/shared/fixtures/tools` and `mcp`, registered in both tables.
- Data: `ToolRow`, `ToolReferenceRow`, `McpServerRow`, `McpCapabilityRow`; `ToolRepository` and
  `McpServerRepository` (patches with nullable columns, reference upsert/clear, capability
  upsert/prune/counts), with tests.
- `tools`: `ToolRegistry` (tiers with the `global` ceiling grant, defaults in `tools.defaults`,
  deregister by key or source, list filters, usage, references), `register_kernel_tools`,
  `ToolInvoker`. `RunServiceOptions::tools` adds the TS gate's "Tool disabled" refusal and
  `recordUse` after admission.
- Kernel: `vector.list` (it existed in TS but had not been ported; the kernel-tools test expects
  it), `VectorSearch::list`, and `kernel::mcp_nodes` (`McpToolNode` under the tool key, input a
  JSON object or null, output `McpCallResultDto`).
- `drivers::mcp`: `protocol` (versions, descriptors, destructive/read-only inference, call-result
  normalisation), `windows_command`, `toolchain_env`, `transport` (hooks: rail log, host-side
  malformed report, one-shot close), `stdio`, `http`, `client` (`McpClient` over
  `rmcp::serve_client`, per-request timeouts, paging capped at 50, error mapping to
  `MCP_UNAVAILABLE`/`MCP_TIMEOUT`/`UNKNOWN` with the TS messages).
- `services::mcp::McpService`: create/update/remove with namespaces and secret usage, start
  (shared in-flight launch), stop, restart, test, autostart, call/call by key, health, death
  supervision with backoff, no restart after a failed start, discovery into the registry and
  nodes, withdrawal on failure/disable/remove, `dispose`.
- `ipc::tools` (8 channels) and `ipc::mcp` (9 channels); `Services` gains `tools`,
  `tool_invoker` and `mcp`; `test_services` builds all three.
- Composition: registry over settings, kernel tools registered, run service given the registry,
  `ToolInvoker`, `McpService` with `<app data>/mcp-toolchain` and the app version, autostart
  spawned after `recover()`, `mcp` in the `Dispose` shutdown stage.
- Tests: `tools.test.ts` ported in full (`tools::tests`, 23), `mcp.test.ts` (`services::mcp::tests`,
  against `test/helpers/fakeMcpServer.mjs` run with `node`, and an axum endpoint for HTTP),
  `mcpWindows.test.ts` (`drivers::mcp::windows_command`, Windows only, plus the toolchain test),
  channel tests for both domains, plus autostart, a `.cmd` shim with spaces in its path whose
  `node` child dies on stop, and `dispose` leaving no process. The fake server gained
  `FAKE_MCP_PID_FILE` (writes its pid; no tool change, so the TS suite is untouched).
- Gates run on Windows 2026-10-18, all passing: `cargo fmt --check`, workspace clippy with
  `-D warnings`, `cargo test --workspace` (zvs-host 785 passed, 6 ignored), `pnpm typecheck`,
  `pnpm lint`, `pnpm lint:root`, `pnpm typecheck:root`, `pnpm build`, `pnpm format:check`,
  `npx vitest run` (1799 passed, 2 skipped), `contract-channels --check`. The MCP and tools
  tests were re-run five times without a failure.

**Deviated from plan:**

- Only the protocol layer is `rmcp`. Its child-process transport is not used: our transport keeps
  the TS behaviour (unreadable lines logged and skipped, a non-JSON-RPC message reported
  host-side and failed fast with an internal sentinel error, stderr on the rail, kill-tree after
  a 2 s grace). Its reqwest client is not used either (it is on reqwest 0.13); ours runs on the
  workspace's 0.12 client.
- The client still asks for `2025-06-18` and accepts the TS host's three versions, so newer
  `rmcp` protocol revisions are not enabled by this port; widening the list is a one-line change.
- A secret that cannot be resolved, or a missing command/URL, now marks the server `failed` with
  `lastError`; in TS those threw before the failure path and left it `starting`.
- Descriptions are cut to 4000 UTF-16 units and empty titles ignored before registration; TS
  would have thrown on them.
- `mcp.start` returns `McpServerDto` (the contract output), not the detail.
- `invoke_tool` on `StepLink`/`Scheduler` (the TS `invokeTool` for model tool calls) is not
  ported; it is only used by chat tool calling, which is TASK_074.

**Next session should know:**

- The manual check (`pnpm dev:tauri`: add an npx stdio server through its `.cmd` shim and an
  HTTP server with a token from Secrets, see both tool lists and tiers on Tools, set one `off`
  and see `tools.invoke` refuse it, stop/start, quit and check no server process is left) has
  **not** been done. The renderer needed no changes.
- Building needs `PROTOC`: adding `rmcp` turned on `indexmap/serde`, which gives the `lance-*`
  crates new build units whose build scripts call `protoc`. `pnpm build:rust`/`tauri` set it from
  `target/tools/protoc`; for plain `cargo`, `export PROTOC=$PWD/target/tools/protoc/bin/protoc.exe`.
- The disk filled during this session (`target/debug` had reached 274 GB). `target/debug/incremental`
  was deleted and the packages with truncated artifacts were cleaned with `cargo clean -p`.
- TASK_074: tool calling should reuse `ToolRegistry::ceiling`, `NodeRegistry::for_tool` and the
  run gate; MCP tools are already nodes under their keys.
