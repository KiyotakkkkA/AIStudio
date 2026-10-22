---
task: TASK_032
title: MCP client driver — servers, discovery, invocation
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_031]
unblocks: [TASK_033]
estimate: 4 h
---

# TASK_032 — MCP client driver — servers, discovery, invocation

## Context

This task lets the app consume MCP servers, which is how it gets capabilities nobody had to build into it. `/design/Connections.dc.html` shows the model: a list of configured servers with transport and status, a setup card (transport, command, arguments, environment where secret values resolve from Secrets), a Test connection that reports the handshake and protocol version, and the discovered tools with per-tool enable and tier.

Two things deserve care. First, **environment values that are secrets must resolve from the Secrets store at launch**, never be typed into the form — the same rule the Providers page follows. Second, an MCP server is an external process speaking a protocol, so its failure modes (never starts, starts then dies, hangs mid-call, returns malformed results) all need handling that does not take the app with it.

Discovered tools register into the TASK_031 registry, which is what makes them appear on the Tools page and become usable by skills and scenarios.

## Why

MCP is the app's extension mechanism. Without it, every integration must be built by hand.

## Depends on

- `TASK_031` — Tool registry and the permission ceiling.

## Read first

- `/design/Connections.dc.html` — setup card, env table, test result line, tool list with tiers, health rail
- `/docs/PASSPORT.md` §14, §8 (drivers behind ports)

## Scope

**In scope**
- `mcp_server` table and repository
- Client driver supporting stdio and HTTP transports
- Lifecycle: launch on demand or at startup, health-check, restart with backoff, stop
- Handshake, protocol negotiation, tool and resource discovery
- Registration of discovered tools into the TASK_031 registry
- Invocation from kernel nodes, with timeouts and error normalisation
- Env resolution from Secrets at launch

**Out of scope** (do not do these, even if tempting)
- The Connections page (TASK_033)
- MCP prompts and sampling — discover and store them, do not surface them yet
- Being an MCP *server* (out of scope for v1)

## Steps

1. `mcp_server` table: id, name, transport, command, args JSON, env JSON (values are either literals or secret references), url, headers, enabled, startup mode, status, protocolVersion, lastError, timestamps.
2. Driver behind a port, per PASSPORT §8. stdio: spawn the process and speak the protocol over stdin/stdout. HTTP: the equivalent over the wire, honouring auth headers.
3. Env resolution: at launch, resolve secret references through `SecretService.resolve()` and pass values into the child environment. Never persist a resolved value; never log the environment.
4. Handshake and discovery: initialise, negotiate the protocol version, list tools and resources. Store the reported version — the mockup shows it in the test result line.
5. Register every discovered tool into the tool registry with its input schema and a destructive heuristic (names suggesting delete/drop/remove default to `off`, per TASK_031's rule).
6. Invocation: a kernel node that calls a tool by key, resolving through the registry so permissions apply. Timeout per call; a hung server must not hang the run.
7. Failure handling: never starts → clear error naming the command; dies mid-session → mark tools unavailable, fail in-flight calls, restart with backoff; malformed response → normalise to `UNKNOWN` with the payload logged host-side only.
8. Health rail data: uptime, restarts, call counts, median latency, error count, recent log lines.
9. Tests with a fake MCP server: discovery registers tools; a dying server marks tools unavailable and restarts; a hung call times out; secret env values are resolved and never logged; malformed responses are handled.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Which MCP transports do you actually need for v1 — local stdio servers only, or remote HTTP servers too? Remote servers mean the app makes outbound connections you configure, and OAuth flows for the ones that need auth."
>
> **Why it matters:** HTTP transport roughly doubles this task: auth flows, token storage and refresh, TLS handling. If you only run local servers, that work can wait.
> **Options:** (a) stdio only for v1 — local servers, no network, no auth; (b) stdio plus HTTP with static tokens from Secrets; (c) both plus OAuth.
> **If the user is unavailable:** take (b). It covers the mockup's `github` HTTP server without an OAuth implementation, and OAuth becomes its own task.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Configure a real MCP server (a filesystem or qdrant server is easy to run locally). Test connection, confirm tools are discovered and appear in the tool registry, then call one from a kernel run. Kill the server process and confirm the app degrades cleanly and restarts it.

## Definition of done

- [x] stdio (and HTTP per the decision) transports working
- [x] Secret env values resolved at launch, never persisted or logged
- [x] Discovery registers tools with schemas and safe default tiers
- [x] Server death, hang and malformed responses all handled without taking the app down
- [x] Invocation honours the permission ceiling
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Decision taken:** Option (b) — stdio plus HTTP with static tokens resolved from Secrets. No
OAuth in v1.

**Built:** Migration `0017_mcp_servers` adds `mcp_server` (name, namespace, transport, command,
args, env, url, headers, enabled, startup, status, protocolVersion, serverName, serverVersion,
lastError, lastConnectedAt, timestamps) and `mcp_capability` (the discovered tools, resources
and prompts, with the registry key each tool was registered under), behind
`McpServerRepository`. `host/drivers/mcp/` holds the driver: `protocol.ts` (JSON-RPC 2.0 and the
MCP result schemas, the supported protocol versions, and the destructive/read-only heuristics),
an `McpTransport` port with `StdioTransport` (spawn, NDJSON over stdin/stdout, stderr as a log
source) and `HttpTransport` (Streamable HTTP, JSON or SSE responses, `Mcp-Session-Id`, static
auth headers), and `McpClient` (handshake, version negotiation, paged discovery, invocation,
per-call timeout, cancellation notification). `McpService` owns configuration, lifecycle,
env resolution, registration and health. `kernel/mcpNodes.ts` turns each discovered tool into a
kernel node under its own registry key, and `NodeRegistry` gained `unregister`. DTOs live in
`@zvs/shared` under `mcp/`; `MCP_UNAVAILABLE` and `MCP_TIMEOUT` were appended to `AppErrorCode`.
19 tests in `apps/studio/tests/mcp.test.ts` drive a real child process
(`test/helpers/fakeMcpServer.mjs`) over real stdio, plus a stubbed fetch for HTTP.

**Deviated from plan:** Three things the scope did not spell out.

A `namespace` column was added, because a tool key has to satisfy `ToolKey`'s
`namespace.verb` shape and a server name like `qdrant-local` does not. It is derived from the
name (`qdrantLocal`), de-duplicated with a numeric suffix, and overridable.

A second table, `mcp_capability`, was added: the scope asks for prompts and resources to be
discovered and stored, and `tool` alone has nowhere to put them. It is also what maps a tool key
back to `(serverId, toolName)` at invocation.

Invocation is a kernel node *per discovered tool*, keyed by the tool key, rather than one
generic `mcp.call` node. `PermissionRequirement` is static per node definition, so a generic node
would have had to re-implement admission inside itself; a node per tool means the existing
`RunService.admitTool` path enforces the TASK_031 ceiling with no MCP-specific permission code
at all. Registration replaces whatever the previous handshake left, and a withdrawn server's
nodes are unregistered alongside its tools.

Health numbers (uptime, restarts, call counts, median latency, error count, recent log) are held
in memory per app session, not persisted. A server the app has not started this session has
nothing honest to say about its uptime, and persisting a call log would have meant a third
table for a rail that is read live.

**Next session should know:** TASK_033 needs IPC — none was added here, since the channels are
page wiring. `McpService` already returns the DTOs the mockup wants: `list()`, `get(id)`
(server plus capabilities), `create/update/remove`, `start/stop/restart`, `test(id)` →
`McpTestResultDto` (the `handshake ok · 128 ms · protocol … · server v…` line) and `health(id)`
→ `McpHealthDto` (the rail). `test()` reconnects on purpose so the latency it reports is a real
handshake; in-flight calls on that server fail while it does. There is no host event for status
changes, so the page polls.

Env and header values are `{kind:"literal", value}` or `{kind:"secret", secretId}`; the form must
never take a plaintext secret. `McpService` resolves them at launch through
`SecretService.resolve()`, hands them to the transport and keeps `secret_usage` rows in sync, so
the Secrets page counts an MCP server as a consumer and refuses to delete a secret in use.

A dying server marks its tools unavailable and restarts with backoff (1 s doubling to 60 s,
5 consecutive failures then it stays `failed`); `enabled: false` or `remove` withdraws the tools
for good. Calling a tool on a stopped server starts it — startup mode `launch` only decides
whether `startAutoStart()` brings it up at boot.

`pnpm --filter studio test` passes (575 passed, 2 pre-existing skips), as do `pnpm typecheck`,
`pnpm lint` and `pnpm build`. The manual check against a third-party server (`uvx
mcp-server-qdrant`) was **not** run — it needs network access this session did not have. The
fake server is a real child process speaking the real protocol over real pipes, and covers
handshake, discovery, invocation, death, hang and malformed responses, but a genuine
third-party server has not been tried.
