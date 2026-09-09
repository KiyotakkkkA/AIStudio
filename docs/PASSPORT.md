# ZVS AI Studio — Architecture Passport

**Status:** locked for bootstrap · **Date:** 2026-09-08 · **Doc version:** 1.1

This is the binding reference for stack and structure. Design mockups for all fifteen
surfaces live in `/design` (canvas: `zvs-ai-studio-maket.html`). Anything in this document
marked **Assumption** was not explicitly specified and may be changed cheaply before
bootstrap; anything marked **Locked** was decided and should only change through a new
revision of this file.

---

## 1. What the product is

A local-first desktop studio for building and running AI automations. Thirteen routed
pages behind one navigation rail, plus two surfaces that are not routes:

| Group | Pages |
| --- | --- |
| Workspace | Secrets & Identity · AI Providers · Vector Stores |
| Build | Chat · Agentic Build · Scenarios |
| Extend | Skills · Connections (MCP) · Integrations · Tools |
| System | Tasks · Downloads · Runs & Logs · Settings |

Two non-route surfaces:

- **Document viewer** — docks into whatever page is open; never a route of its own (§20).
- **Browser window** — a second `BrowserWindow` the agent can drive; opened from the rail's
  `Browser` item, which is the one nav entry that opens a window instead of navigating (§21).

Guiding constraints: everything runs on the user's machine; no server component; secrets
never leave the device; heavy work (indexing, OCR, embedding, vector search) is Rust.

---

## 2. Locked decisions

| # | Decision | Choice | Why | Accepted cost |
| --- | --- | --- | --- | --- |
| D1 | Rust boundary | **Hybrid: napi-rs addon + job sidecar** | Fast in-process calls for search/embed/hash; long or crash-prone work (indexing, OCR) is a killable child process with progress and cancellation — exactly what the Tasks page promises | Two integration paths to build and package |
| D2 | SQLite ownership | **TypeScript only** | One writer, one schema owner, one migration tool; upholds "host reaches data only through Entity Repository" | Bulk index metadata crosses the Rust boundary before it is persisted |
| D3 | IPC contract | **Hand-rolled typed contract registry** | Full control, no adapter dependency, plain cloneable payloads that MobX can own outright | ~300 lines of infrastructure we maintain |
| D4 | Streaming | **One multiplexed event channel** | Token deltas, task progress, step events and log lines share one mechanism; trivially recordable and replayable | A routing layer keyed by `streamId` |
| D5 | Repo layout | **pnpm workspace + electron-vite + cargo workspace** | Enforced package boundaries, fast HMR, one command to build both toolchains | Monorepo orchestration to configure once |
| D6 | Renderer structure | **Atomic for UI, feature modules for domains** | Atomic Design has no answer for where a store or use-case goes; feature modules fill that gap without discarding it | Two organizing axes to teach |
| D7 | Execution | **One kernel, three front-ends** | Chat, scenarios and agentic runs are all Run/Step/Event; one cancellation, retry, log and progress story feeding one Tasks page | The kernel must be general enough on day one |
| D8 | Document viewer | **Right dock that expands to overlay; PDF/image/text native, office converted to PDF by the sidecar** | One rendering path and faithful office layout; read-while-chatting stays possible | A conversion step and a cache to manage |
| D9 | Agent browser | **Own `BrowserWindow` embedding a `WebContentsView`, driven over CDP; persistent profile, tiered approval** | No second browser to ship, every action visible live, instant takeover; the user logs into sites once | We own the automation primitives, and a logged-in profile raises the stakes on prompt injection |

---

## 3. Stack

### Verified and pinned

| Piece | Version | Note |
| --- | --- | --- |
| Node | 26.7.0 | dev toolchain |
| pnpm | 12.3.4 | workspace manager |
| Rust / cargo | 1.93.1 | stable channel |
| `@kiyotakkkka/zvs-uikit-lib` | 8.3.0 | ESM-only; peers `react@^19`, `react-dom@^19`; subpaths `.`, `./chart`, `./code-view`, `./server`, `./styles.css`; brings `clsx`, `tailwind-merge`, `shiki`, `recharts`, `@tanstack/react-virtual` |

### Chosen, pin exact versions at bootstrap

Do not copy version numbers into `package.json` from this table — resolve latest stable at
bootstrap and pin there.

| Layer | Choice |
| --- | --- |
| Shell | Electron (current stable). Its embedded Node ABI is the rebuild target for native modules |
| Build | electron-vite (host + preload + renderer entries), Vite, Turborepo |
| Language | TypeScript, `strict: true`, `verbatimModuleSyntax`, ESM everywhere |
| UI | React 19, Tailwind CSS, `@kiyotakkkka/zvs-uikit-lib` |
| State | MobX + `mobx-react-lite` |
| Routing | react-router (hash history) — **Assumption** |
| Validation / DTO | Zod |
| Data | Drizzle ORM + `better-sqlite3` (rebuilt for Electron), `drizzle-kit` migrations |
| AI | Vercel AI SDK on the host, provider packages per vendor |
| Vector | LanceDB (Rust crate) behind the napi addon |
| Native | napi-rs (addon), tokio (async), a plain Rust binary for the sidecar |
| Test | Vitest (unit), Playwright + Electron (e2e), `cargo test` |
| Viewer | `pdf.js` (renderer), sidecar conversion for DOCX/XLSX/PPTX |
| Browser | Electron `WebContentsView` + `webContents.debugger` (CDP) |
| Package | electron-builder — **Assumption** |

---

## 4. The three domains

```
┌──────────────── host (Electron main) ─────────────────┐
│  ipc handlers → services → repositories → SQLite      │
│                    ↓                                  │
│              drivers: rust · ai · integrations        │
└───────────────────────┬───────────────────────────────┘
                        │  preload bridge (contextIsolation)
┌───────────────────────┴───────────────────────────────┐
│              renderer (React + MobX)                  │
│  pages → features → organisms → molecules → atoms     │
└───────────────────────────────────────────────────────┘

              shared — Zod DTOs + IPC contract
              (imported by both, depends on neither)
```

**Rule:** `shared` must never import from `host` or `renderer`. `host` and `renderer` must
never import each other. Enforced by `eslint-plugin-boundaries` (or
`import/no-restricted-paths`) and by TypeScript project references — a violation fails the
build, not review.

---

## 5. Repo layout

```
zvs-ai-studio/
├─ apps/
│  └─ studio/                    # the Electron app, built by electron-vite
│     ├─ electron.vite.config.ts
│     └─ src/
│        ├─ host/                # main process — see §8
│        │  ├─ ipc/              # channel handlers, thin
│        │  ├─ services/         # ALL business logic
│        │  ├─ kernel/           # execution kernel — see §11
│        │  ├─ drivers/          # rust, ai, mail, telegram, git, mcp
│        │  ├─ data/             # schema, migrations, repositories
│        │  ├─ browser/          # agent browser: window, session, CDP driver, policy
│        │  ├─ documents/        # open, convert, cache, page serving
│        │  └─ platform/         # windows, tray, safeStorage, paths, logging
│        ├─ preload/             # studio preload + a separate, narrower browser preload
│        ├─ renderer/            # React — see §13
│        │  ├─ app/              # bootstrap, router, providers, theme
│        │  ├─ ui/               # atoms · molecules · organisms · templates
│        │  ├─ features/         # one folder per domain
│        │  ├─ pages/            # one per route, composition only
│        │  ├─ viewer/           # document viewer dock — see §20
│        │  └─ stores/           # root store + cross-cutting stores
│        └─ browser-ui/          # chrome renderer for the browser window — see §21
├─ packages/
│  ├─ shared/                    # DTOs + contract + error codes + event types
│  ├─ ipc/                       # registry runtime: client, server, event bus
│  └─ tsconfig/  eslint-config/  # shared configs
├─ crates/
│  ├─ zvs-core/                  # pure logic: chunking, OCR, embedding, lance
│  ├─ zvs-napi/                  # napi-rs addon, thin wrapper over zvs-core
│  └─ zvs-jobd/                  # sidecar binary, thin wrapper over zvs-core
├─ design/                       # the maket (source of truth for UI)
├─ docs/                         # this file, ADRs
├─ Cargo.toml                    # cargo workspace
├─ pnpm-workspace.yaml
└─ turbo.json
```

`crates/zvs-core` holds the real implementation. The addon and the sidecar are both thin
adapters over it, so a capability can move between the two without a rewrite.

---

## 6. `shared` — the DTO layer

**Every** value crossing IPC is a Zod-validated DTO. DTOs are plain JSON-serializable data:
no class instances, no `Date`, no `Map`/`Set`, no functions, no getters. This is not
stylistic — it is what keeps structured-clone happy and lets MobX take full ownership of the
object on the renderer side without proxy-cloning surprises.

Rules:

- Timestamps are epoch milliseconds (`z.number().int()`). Format at the view layer.
- Ids are branded strings: `z.string().uuid().brand<'ProviderId'>()`.
- Money/precision values are strings, never floats.
- Optional means optional; never `null | undefined` both.
- One file per DTO, named `<Thing>Dto`. Type is derived, never hand-written:
  `export type ProviderDto = z.infer<typeof ProviderDto>`.
- DTOs are **not** entities. A DB row shape lives in `host/data/schema`; mapping to a DTO is
  explicit and lives in the service. They will diverge, and that is correct.
- Breaking a DTO means a new channel version (`providers.list@2`), not a silent edit.

---

## 7. IPC

### Contract registry

`packages/shared/src/contract.ts` is the single source of truth for every channel:

```ts
export const contract = defineContract({
  'providers.list':  { input: z.void(),          output: z.array(ProviderDto) },
  'providers.test':  { input: TestProviderInput, output: ProviderProbeDto },
  'vector.search':   { input: VectorSearchInput, output: VectorSearchResultDto },
  'runs.start':      { input: StartRunInput,     output: RunHandleDto },
  'runs.cancel':     { input: RunIdInput,        output: z.void() },
} as const);

export type Contract = typeof contract;
```

From that one object:

- `packages/ipc/client` produces a typed renderer client — `await ipc.call('providers.test', input)`
  is fully inferred in both directions.
- `packages/ipc/server` produces a typed registrar — the host cannot register a handler with
  the wrong signature, and cannot forget to register one (exhaustiveness is checked).

### Validation policy

Validate on **both** sides. Host-side because the renderer is the less trusted half and a
compromised renderer must not reach services with junk. Renderer-side because a DTO drift
should fail loudly at the boundary rather than corrupt a store. In production builds the
renderer-side parse can be swapped for a type assertion if profiling demands it — that is a
deliberate switch, not a default.

### Error model

Handlers never throw raw errors across IPC. Every failure becomes:

```ts
{ ok: false, error: { code: AppErrorCode, message: string, details?: unknown } }
```

`AppErrorCode` is a shared enum (`PROVIDER_UNREACHABLE`, `SECRET_MISSING`,
`PERMISSION_DENIED`, `RUN_CANCELLED`, …). The renderer maps codes to copy; it never parses
error strings. Stack traces stay on the host and go to the log file.

### The event channel (D4)

One channel, `zvs:events`, carries a discriminated union:

```ts
export const HostEvent = z.discriminatedUnion('type', [
  z.object({ type: z.literal('token'),    streamId: StreamId, seq: z.number(), delta: z.string() }),
  z.object({ type: z.literal('progress'), streamId: StreamId, seq: z.number(), done: z.number(), total: z.number() }),
  z.object({ type: z.literal('step'),     streamId: StreamId, seq: z.number(), step: StepEventDto }),
  z.object({ type: z.literal('log'),      streamId: StreamId, seq: z.number(), line: LogLineDto }),
  z.object({ type: z.literal('approval'), streamId: StreamId, seq: z.number(), request: ApprovalRequestDto }),
  z.object({ type: z.literal('end'),      streamId: StreamId, seq: z.number(), outcome: RunOutcomeDto }),
]);
```

`seq` is monotonic per stream so the renderer can detect gaps. An `EventRouter` in the
renderer dispatches by `streamId` to whichever store registered for it. Because every event
passes one point, recording a session to disk and replaying it into the UI is a few lines —
build that early, it pays for itself debugging scenarios.

### Preload

`contextIsolation: true`, `nodeIntegration: false`, `sandbox: true`. The preload exposes
exactly two functions — `call(channel, payload)` and `subscribe(handler)` — and nothing else.
No `ipcRenderer`, no `require`, no filesystem, no shell.

---

## 8. Host layering

Strict one-way dependency:

```
ipc/  →  services/  →  repositories/  →  drizzle/sqlite
             ↓
         drivers/  (rust napi · rust sidecar · ai sdk · mail · telegram · git · mcp)
```

- **`ipc/`** handlers are thin: parse input, call one service method, map the result to a
  DTO. No branching on business rules, no DB access, no driver calls. If a handler is longer
  than about fifteen lines, logic has leaked into it.
- **`services/`** hold all business and application logic. A service may call repositories,
  drivers and other services. Services are constructed with their dependencies (plain
  constructor injection through a small composition root in `host/main.ts`) so they can be
  unit-tested without Electron.
- **`repositories/`** are the *only* code that touches Drizzle. One repository per aggregate,
  exposing intention-revealing methods (`findEnabledByCapability`), never a leaked query
  builder. Repositories return entities; services map entities to DTOs.
- **`drivers/`** wrap everything external behind an interface owned by us. The AI SDK, the
  napi addon, the sidecar, an IMAP client — each sits behind a port so it can be faked in
  tests and swapped without touching services.

**Transactions** belong to services, not repositories: a service opens the transaction and
passes the handle down, so one unit of work can span several repositories.

---

## 9. Data layer

- `better-sqlite3` opened in **WAL** mode, `foreign_keys = ON`, `busy_timeout` set.
- Database file in `app.getPath('userData')`. Never bundled, never in the app directory.
- Schema in `host/data/schema/*.ts`, one file per table, exported through a barrel.
- Migrations generated by `drizzle-kit` into `host/data/migrations/`, committed, and applied
  **on startup before any service is constructed**. A failed migration blocks boot and shows
  a recovery screen; it never runs the app on a half-migrated database.
- Backup the DB file before each migration run; keep the last three.

Aggregates (initial): `secret`, `identity`, `provider`, `model`, `vector_store`,
`vector_document`, `conversation`, `message`, `scenario`, `scenario_version`, `run`, `step`,
`skill`, `mcp_server`, `tool`, `tool_permission`, `integration`, `download`, `task`,
`setting`.

**Secrets at rest** — **Assumption**: the secret *value* is encrypted with Electron
`safeStorage` (DPAPI on Windows, Keychain on macOS) and the ciphertext stored in SQLite;
metadata (name, type, scope, tags, rotation date) is plaintext so the Secrets page can list
and filter without decrypting. Decryption happens in a service, only at point of use, and a
decrypted value is never logged, never returned in a DTO, and never crosses IPC.

---

## 10. Rust boundary (D1)

| Capability | Where | Why |
| --- | --- | --- |
| Vector search / upsert (LanceDB) | **napi addon** | Latency matters; called on every chat turn |
| Embedding a short text | **napi addon** | Small, fast, bounded |
| Hashing, chunking, tokenizer counts | **napi addon** | Pure and quick |
| Corpus indexing / re-index | **sidecar** | Minutes long; needs progress, pause, cancel |
| OCR | **sidecar** | Native deps are the most likely thing to segfault |
| Model file verification / conversion | **sidecar** | Long, IO-bound, killable |

**Addon rules.** Every exported function is `async` (napi-rs runs it on a tokio pool) so the
host thread never blocks. Errors return `Result`, never panic across the boundary — a
`catch_unwind` at each entry point converts a panic into an error. Payloads are plain JSON
across the boundary; buffers only for genuine binary.

**Sidecar protocol.** One long-lived child process, newline-delimited JSON-RPC over
stdin/stdout, stderr to the log file. Messages: `job.start`, `job.cancel`, `job.progress`,
`job.done`, `job.error`, `ping`. The host owns the lifecycle: spawn on demand, health-check,
restart with backoff, and hard-kill on cancel. Because the sidecar is a separate process,
"pause"/"cancel" on the Tasks page are real, and a crash costs one job, not the app.

**Per D2**, neither the addon nor the sidecar opens SQLite. They return results; the host
persists through repositories.

---

## 11. Execution kernel (D7)

One core in `host/kernel/`, three front-ends.

```ts
interface NodeDef<I, O> {
  type: string;                       // 'llm.generate', 'vector.search', 'mail.send'
  input: z.ZodType<I>;
  output: z.ZodType<O>;
  permission: PermissionRequirement;  // resolved against the tool ceiling — §14
  run(ctx: StepContext, input: I): Promise<O>;
}

interface StepContext {
  runId: RunId;
  signal: AbortSignal;                // cancellation is cooperative and universal
  emit(event: HostEvent): void;       // one path to the UI
  requestApproval(req: ApprovalRequestDto): Promise<ApprovalDecision>;
  services: KernelServices;           // narrow, explicit
}
```

Core concepts:

- **Run** — one execution of a graph. Has a status, a checkpoint and an event stream.
- **Step** — one node invocation. Persisted with input, output, timing and error.
- **Scheduler** — walks the graph, respects concurrency limits, retries per policy.
- **Checkpoint** — after each step, so a run survives an app restart and can resume.
- **Approval gate** — a node whose permission exceeds its grant suspends the run and emits
  an `approval` event. The Agentic Build and Tasks pages both render this state.

Mapping:

| Front-end | Graph shape |
| --- | --- |
| **Chat** | Fixed short graph: retrieve → generate → persist. Streams tokens as `token` events |
| **Scenarios** | User-authored DAG from the visual editor; nodes are `NodeDef`s from a registry |
| **Agentic Build** | Self-extending plan: the model proposes steps, the kernel executes them under the same permission and approval rules |

Because all three are runs, the Tasks page is one list, Runs & Logs is one history, and
retry/cancel/progress are written once.

---

## 12. AI layer

Vercel AI SDK lives on the **host** only; the renderer has no vendor SDK, no API keys and no
session cookies.

### Two independent axes (decided 2026-09-10)

A connection to a vendor is described by two orthogonal choices, and the code must never
collapse them into one enum:

| Axis | Values | What it decides |
| --- | --- | --- |
| **Adapter family** — `provider.adapter` | `openai-compatible`, `anthropic`, `qwen-web`, `deepseek-web` | The wire shape: request/response mapping, model-list endpoint, streaming format |
| **Auth mode** — `provider.auth_mode` | `api`, `account` | How a request is credentialed and which HTTP stack carries it |

One adapter family serves many vendors: Ollama, OpenRouter, Mistral, LM Studio and vLLM are
all `openai-compatible` rows differing only by base URL and secret. A vendor whose wire
format is its own — Qwen, DeepSeek — gets its own family. **Adding a vendor that speaks a
known dialect must be a row, not a file.**

- **`api` mode** — base URL plus a `secret` reference. `SecretService.resolve()` at point of
  use, `Authorization` header, ordinary host fetch. This is the classic path.
- **`account` mode** — no API key exists. The user signs into the vendor's own web app inside
  the integrated browser (§21), and the host issues requests through Electron's
  `net.fetch` bound to the `persist:browser-work` session, so the vendor's cookies ride along
  exactly as they do in the browser tab. Where the vendor also requires a bearer token
  (Qwen does), that token is captured from the identity endpoint and stored through
  `SecretService` like any other credential — encrypted at rest, never in a DTO, never in the
  renderer. Cookies are never copied out of the partition.

### Structure

- `drivers/ai/ports.ts` — our ports: `TextGenerationDriver`, `EmbeddingDriver`, `ImageDriver`.
  Services depend on these, never on a vendor SDK.
- `drivers/ai/transport/` — `Transport` is the seam between the two auth modes: `ApiTransport`
  and `AccountTransport` both expose one `request()`/`stream()` shape. **An adapter never
  learns which mode it is running under**, which is why an adapter family can be offered over
  either.
- `drivers/ai/adapters/` — one file per adapter family, each declaring an
  `AdapterCapabilities` descriptor (supports streaming? live model list? honours `topK`?
  requires an account?) so services and UI degrade honestly instead of guessing.
- `drivers/ai/identity/` — per-family identity probes for `account` mode: a small mapper from
  the vendor's "current user" endpoint onto our `AccountIdentity`
  (`chat.qwen.ai/api/v1/auths/`, `chat.deepseek.com/api/v0/users/current`).
- A **provider registry** maps a `provider` row to a configured driver: pick the adapter
  family, build the transport for the auth mode, cache the client per provider id, invalidate
  on provider, secret or account change. **Never cache a resolved plaintext credential.**
  The three capability tabs on the AI Providers page remain three filters over one table.
- **Test connection** hits the provider's model-list endpoint (or, in `account` mode, first
  the identity endpoint), persists the discovered models and returns latency — the same call
  the Providers page shows and the health check reuses. Its outcome is a union, not a
  boolean, and it includes `account-not-linked` and `session-expired`.
- Streaming: the vendor stream is consumed on the host and re-emitted as `token` events on the
  one event channel (D4). The renderer never sees an SDK object.

### Accounts

An `account` row is an identity linked from the browser session: external id, masked email,
display name, avatar, optional bearer-token secret ref, expiry, status, partition. Its
lifecycle is link → healthy → `needs-relink`. An expired session is a **distinct state** from
a bad API key: the Accounts tab offers Re-link, and the app never opens a login page on its
own. The schema permits several accounts per vendor; one browser partition means one live
session per vendor for now.

---

## 13. Renderer

React 19 + MobX. **One component per `.tsx`**, file named for the component, default export.

### Two axes (D6)

- **`ui/`** — presentational only, domain-free, no store imports, props in / events out.
  `atoms/` (mostly re-exported or thinly wrapped kit components) → `molecules/` →
  `organisms/` → `templates/` (page skeletons: the nav rail, the three-column shell).
- **`features/<domain>/`** — everything that knows about a domain: its store, its view-models,
  its domain-aware organisms, its IPC calls. Example: `features/providers/` holds
  `ProviderStore.ts`, `ProviderFormVm.ts`, `ProviderCard.tsx`, `ModelGrid.tsx`.
- **`pages/`** — one per route, composition only: pick a template, mount feature organisms,
  wire nothing else.

A component in `ui/` that imports a store is a bug. A component in `features/` that would be
useful to another domain moves down into `ui/`.

### MobX rules

- A **root store** is created once at bootstrap and provided by context; features get their
  store from a typed hook. No singletons imported directly — it breaks tests and HMR.
- Stores hold DTOs (plain objects) in observables. Because DTOs are plain and cloneable, no
  proxy-identity problems and no serialization tricks. `makeAutoObservable` in the
  constructor.
- All mutation happens in actions; async work uses `runInAction` at each await boundary.
- Derived UI state is `computed`, never duplicated state.
- Components are wrapped in `observer` and stay dumb: read from the store, call an action.
- A store subscribes to a `streamId` on start and unsubscribes on dispose — no leaked
  listeners.

### Styling

Tailwind, with the palette exposed as CSS custom properties exactly as the maket names them:
`--color-main-50` … `--color-main-900`, `--color-accent-light|medium|dark`, plus
`--color-ok|warn|error`. The kit's `styles.css` is imported once at app root. Dark is the only
theme shipped at v1; tokens are defined so a light theme is a token swap, not a refactor.

---

## 14. Security

- Renderer sandboxed, context-isolated, no Node integration. Strict CSP; no remote code.
- Preload surface is two functions (§7).
- Secrets: encrypted at rest via `safeStorage`, decrypted only inside a host service, never
  in a DTO, never logged, never in the renderer.
- **Tool permission ceiling** — as the maket states: a tool is registered once (built-in, MCP,
  or granted by a connected integration) and given `auto | ask | off` on the Tools page. A
  skill or scenario may narrow that, never widen it. The kernel enforces it at step
  admission; the UI only reflects it.
- Sidecar runs with the app's privileges but no elevated rights; workspace roots are an
  explicit allow-list checked host-side, not in Rust.
- No telemetry, no outbound calls the user did not configure.
- **Page and document content is untrusted input.** Anything the agent reads from a web page,
  a PDF or an email is data, never instruction. Text from those sources is fenced before it
  reaches a model, and a tool call the model then proposes is authorised by the permission
  model alone — never by something the content asked for. This is the largest attack surface
  the browser window introduces; a regression here is a security bug, not a UX one.

---

## 15. Cross-cutting

- **Logging** — structured JSON lines to `userData/logs/`, rotated. A run's events are also
  written per-run so Runs & Logs can replay one execution without scanning everything.
- **Config** — user settings in SQLite; machine-level flags in a small JSON file so a broken
  DB can still boot.
- **Paths** — every path resolved through `platform/paths.ts`. No `__dirname`, no relative
  IO anywhere else.
- **IDs** — UUID v7 (time-sortable) generated on the host.
- **Clock** — injected into services so time-dependent logic is testable.

---

## 16. Build and packaging

- `turbo build` orchestrates: `cargo build --release` for the crates, napi artifact copy,
  then `electron-vite build`, then `electron-builder`.
- The napi addon is built per target triple and loaded by platform at runtime; the sidecar
  binary is packaged as an `extraResource` and located through `platform/paths.ts`.
- `better-sqlite3` is rebuilt against the Electron ABI (`electron-rebuild`) — this is the
  most common bootstrap failure; wire it into `postinstall` on day one.
- **Windows first** (the development target); macOS and Linux are structurally supported but
  unverified until someone builds them. **Assumption.**
- Code signing and auto-update are **out of scope for v1**. **Assumption.**

---

## 17. Conventions

| Kind | Convention | Example |
| --- | --- | --- |
| DTO | `<Thing>Dto`, one per file | `ProviderDto.ts` |
| Entity / table | snake_case table, camelCase TS | `vector_store` / `vectorStore` |
| Repository | `<Aggregate>Repository` | `ProviderRepository.ts` |
| Service | `<Domain>Service` | `ProviderService.ts` |
| Driver | `<Thing>Driver` + a port interface | `OllamaDriver implements TextGenerationDriver` |
| Store | `<Domain>Store` | `ProviderStore.ts` |
| Component | PascalCase, one per file, default export | `ProviderCard.tsx` |
| IPC channel | `domain.verb`, dot-separated, lowercase | `providers.test` |
| Kernel node | `namespace.verb` | `vector.search` |

---

## 18. Assumptions to confirm

1. Router is react-router with hash history.
2. Packaging via electron-builder; no signing or auto-update in v1.
3. Windows is the only verified target for v1.
4. **Confirmed (TASK_010):** secrets encrypted with `safeStorage`, ciphertext in SQLite (not
   a separate vault file). Values are therefore bound to the OS user account: copy the
   database to another machine and the metadata survives while the values do not. No
   passphrase-derived key, so no passphrase prompt at launch. Stored values are also
   **write-only** — there is no reveal channel and no IPC path that returns a plaintext;
   a value can be replaced but never read back into the renderer.
5. Single window; no multi-window or tray-only mode in v1.
6. **Confirmed (TASK_003):** all app labels and user-facing messages are Russian from the
   start, without a `t()` shim. Developer communication and code identifiers remain English.
7. `@kiyotakkkka/zvs-uikit-lib` is ESM-only and Tailwind-based; the app's Tailwind config
   must include its `dist` in `content` for class scanning.
8. Office→PDF conversion uses a bundled headless LibreOffice invoked by the sidecar. It would
   be the only heavyweight third-party binary in the installer (roughly 300 MB). If that is
   unacceptable, the fallback is JS rendering at lower fidelity — decide before §19 step 11.
9. The browser window ships one profile ("work"). Multiple named profiles come later, but the
   partition key is shaped for it from the start.

---

## 19. Bootstrap order

1. Workspace skeleton, configs, lint boundary rules, CI running `typecheck + test + clippy`.
2. `shared` DTO + contract primitives; `packages/ipc` client/server/event bus; one end-to-end
   `ping` channel proving the whole boundary.
3. Data layer: schema, first migration, repository base, one aggregate (`secret`) end-to-end.
4. Renderer shell: templates, nav rail, routing, root store, theme tokens — the maket's
   chrome with empty pages.
5. Secrets page complete, top to bottom. It is the smallest full vertical slice and unblocks
   providers.
6. Providers page + AI driver + test-connection.
7. `crates/zvs-core` + napi addon: LanceDB search. Vector Stores page.
8. Execution kernel + Chat as its first front-end.
9. Sidecar + indexing job. Tasks and Downloads pages.
10. Scenarios editor, then Skills, Connections, Integrations, Tools.
11. Document viewer: PDF/image/text path first, then sidecar conversion for office formats.
12. Browser window, CDP driver, policy engine and the browser.* kernel nodes.

---

## 20. Document viewer (D8)

**Not a route.** A dock that opens beside whatever is on screen and can expand to cover the
window. It has no nav entry and no URL of its own; it is opened by a citation in chat, a file
in a run, a document row in a vector store, or a drop onto the window.

### Rendering

| Kind | Path |
| --- | --- |
| PDF | `pdf.js` in the renderer, rendered to canvas per page, virtualised |
| Images | Native `<img>` with pan/zoom |
| Markdown / code / CSV | Rendered in-app; code via the kit's `./code-view` (shiki) |
| DOCX · XLSX · PPTX | **Converted to PDF by the sidecar on open**, then the PDF path |

Conversion is an ordinary kernel job: it shows on the Tasks page, reports progress, and is
cancellable. The result is cached at `userData/cache/documents/<sha256>.pdf`, keyed by content
hash — so reopening is instant and the cache is safe to delete at any time. Eviction is LRU
against a size ceiling set in Settings.

### State

`ViewerStore` is a cross-cutting store, not a feature store, because any feature can open it.
It holds the open document descriptor, page count, current page, zoom, dock width, expanded
flag and the current selection. Dock width and zoom persist per user, not per document.

### Selection → conversation

A text selection produces a `DocumentSelectionDto` — document id, page, character range and
extracted text. "Ask about this" sends that DTO to the chat composer as an attachment, which
is how the citation loop closes: the model answers with a citation, the citation opens the
viewer at the right page, and a selection there feeds the next turn.

**Extracted document text is untrusted input** (§14) and is fenced before it reaches a model.

### IPC

`documents.open`, `documents.page`, `documents.convert` (returns a run handle; progress
arrives on the event channel), `documents.close`.

---

## 21. Agent browser window (D9)

> Implementation update (TASK_045, 2026-09-09): the user requested the browser attached to
> the main studio window, like VS Code, superseding D9's separate-window presentation below.
> `/browser` mounts the `browser-ui` chrome and active site as native `WebContentsView`s
> within the main content panel. Switching routes detaches the views and retains tabs;
> the persistent profile, empty site preload and security boundaries below still apply.
> Cookie metadata and deletion are available through Сайты & cookies; cookie values remain
> host-only. Automation remains deferred to TASK_046/TASK_047.

**Its own window**, not a route. The rail's `Browser` item opens it, or focuses it if it is
already open. It is a separate `BrowserWindow` whose chrome is a small renderer
(`browser-ui/`) and whose page area is a `WebContentsView`.

```
BrowserWindow
├─ chrome renderer (browser-ui)   tabs · toolbar · agent panel · approval bar
└─ WebContentsView                the actual site — never trusted
```

The site's `WebContentsView` gets **its own preload**, far narrower than the studio's: it
exposes nothing at all to the page. The page has no bridge, no IPC, and no knowledge that it
is being automated beyond what a normal browser reveals.

### Session and identity

One persistent partition, `persist:browser-work`. The user logs into sites once, inside the
app, and those cookies survive restarts — which is what makes the automations useful at all.
The partition is separate from the studio's own session and holds no app credentials.

> Update (2026-09-10): this partition is also the identity source for **account-mode AI
> providers** (§12). The Accounts tab's Login button opens the vendor's site in this browser;
> the host then issues that provider's API calls with `net.fetch({ session })` against the
> same partition. Cookies stay inside the partition — the AI layer reads no cookie values and
> the renderer sees only a masked identity. Clearing a vendor's cookies in Sites & cookies
> therefore un-links that account, and the provider must report `session-expired`, not a
> generic auth failure.

### Automation

Driven through `webContents.debugger` (CDP). The driver in `host/browser/` exposes a small,
audited set of primitives; everything else is deliberately absent.

| Node | Tier | Notes |
| --- | --- | --- |
| `browser.navigate` | auto | Blocked schemes: `file:`, `data:`, `javascript:` |
| `browser.read_page` | auto | Accessibility tree, not raw DOM — smaller and less injectable |
| `browser.screenshot` | auto | |
| `browser.find` | auto | Locates an element, returns a handle |
| `browser.click` · `browser.type` | per-site | Granted once per origin, revocable any time |
| `browser.submit` · `browser.download` | always ask | Every time, regardless of prior grants |
| Credential and payment fields | **never** | The driver refuses, by input type and field heuristics |

Grants are stored per origin, not per session, and are listed and revocable in the agent panel
and in Settings. The tiers compose with the global tool ceiling (§14): the browser can never
exceed what Tools allows, only narrow it.

### Making actions visible

The target element is outlined in the page and labelled with the action about to be taken.
Every action lands in the panel log with its authorisation — "auto", "granted for this site",
"approved 20:58". **Take control** suspends the agent and hands the user the mouse; **Stop**
kills the run. A rate limiter caps actions per minute and trips a pause when exceeded, so a
runaway loop stops itself.

### Prompt injection

The threat is explicit: the agent reads pages, and pages can contain text aimed at the agent.
Mitigations, in order of importance — page text is fenced as data (§14); irreversible actions
always require a human (submit, download, purchase); credentials are never typed; the action
log makes an anomaly visible immediately; the rate limiter bounds the blast radius. None is
sufficient alone, which is why all five ship together.

### IPC and kernel

The window is a kernel front-end like any other: a browser session is a **Run**, each action a
**Step**, and the panel log is that run's event stream — the same events the Tasks page
renders. An approval request is the same `approval` event the Agentic Build page uses (§7,
§11). Nothing here is a parallel mechanism.
