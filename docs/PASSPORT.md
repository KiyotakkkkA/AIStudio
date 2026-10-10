# ZVS AI Studio — Architecture Passport

**Status:** locked · **Date:** 2026-10-10 · **Doc version:** 2.0

> **Revision 2.0 (2026-10-10): Tauri and a Rust host.** Electron is replaced by Tauri 2, and
> the whole host (services, kernel, repositories, drivers) is rewritten in Rust. See D10. This
> document describes the **target** architecture. The move is executed by Phase M in
> `/tasks/INDEX.md` (TASK_058 – TASK_076). Until TASK_076 lands, the Electron shell and the
> TypeScript host under `apps/studio/src/host/` still exist, frozen, as the behavioural
> specification of the port. §5 maps every old path to its new home. Version 1.1 is in git
> history.

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
- **Browser workspace** — real websites in child webviews attached to the studio window, in a
  persistent profile the agent can drive. It is opened from the rail's `Browser` item (§21).

Guiding constraints: everything runs on the user's machine; no server component; secrets
never leave the device; the host and all heavy work (indexing, OCR, embedding, vector search)
are Rust.

---

## 2. Locked decisions

| # | Decision | Choice | Why | Accepted cost |
| --- | --- | --- | --- | --- |
| D1 | Rust boundary | **In-process host + job sidecar** *(rev. 2.0; was napi-rs addon + sidecar)* | The host is Rust, so `zvs-core` is linked directly: search, embedding and hashing are plain async calls. Long or crash-prone work (PDF parsing, downloads, long jobs) stays in a killable child process with progress and cancellation, which is exactly what the Tasks page promises | A panic in-process must be caught at every core port (`catch_unwind`), and blocking work must go to `spawn_blocking` |
| D2 | SQLite ownership | **Rust host only** *(rev. 2.0; was TypeScript only)* | One writer, one schema owner, one migration runner, in `zvs-host::data`. It still upholds "host reaches data only through Entity Repository" | Hand-written SQL in repositories; no ORM or schema generator |
| D3 | IPC contract | **Hand-rolled typed contract registry; Zod in the renderer, serde mirror in `zvs-contract`, golden fixtures between them** | Full control, plain cloneable payloads that MobX can own outright, validation on both sides | Two definitions of every DTO, kept honest by fixtures both toolchains parse (§6) |
| D4 | Streaming | **One multiplexed event channel** (a Tauri `Channel`) | Token deltas, task progress, step events and log lines share one mechanism; trivially recordable and replayable | A routing layer keyed by `streamId` |
| D5 | Repo layout | **pnpm workspace + Vite + Tauri + cargo workspace** *(rev. 2.0; was electron-vite)* | Enforced package and crate boundaries, fast HMR, one command to build both toolchains | Monorepo orchestration to configure once |
| D6 | Renderer structure | **Atomic for UI, feature modules for domains** | Atomic Design has no answer for where a store or use-case goes; feature modules fill that gap without discarding it | Two organizing axes to teach |
| D7 | Execution | **One kernel, three front-ends** | Chat, scenarios and agentic runs are all Run/Step/Event; one cancellation, retry, log and progress story feeding one Tasks page | The kernel must be general enough on day one |
| D8 | Document viewer | **Right dock that expands to overlay; PDF/image/text native, office converted to PDF by the sidecar** | One rendering path and faithful office layout; read-while-chatting stays possible | A conversion step and a cache to manage |
| D9 | Agent browser | **Child webviews attached to the studio window, in their own WebView2 profile, driven over WebView2's CDP; tiered approval** *(rev. 2.0; was `BrowserWindow` + `WebContentsView`)* | No second browser to ship, every action visible live, instant takeover; the user logs into sites once | We own the automation primitives, a logged-in profile raises the stakes on prompt injection, and CDP access is WebView2-only, so automation is Windows-only |
| D10 | Shell and host | **Tauri 2 (WebView2 on Windows); the whole host in Rust; no Node runtime in the shipped app** *(decided 2026-10-10)* | One language for host and core, so no FFI layer, no Node ABI rebuilds and no JSON hop to reach `zvs-core`. A much smaller install and memory footprint than a bundled Chromium. Capability-scoped IPC instead of a preload convention | A rewrite of ~22k lines of host TypeScript (Phase M). Multi-webview needs Tauri's `unstable` feature. Only WebView2 is verified. Electron APIs are replaced one by one, as listed below |

**D10, what replaced what.** `BrowserWindow` became a Tauri window with a `studio` webview.
The preloads became capabilities: the studio webview may call only `ipc_call` and
`events_subscribe` (§7, §14). `WebContentsView` became child webviews (§21). The
`persist:browser-work` partition became a dedicated WebView2 user-data directory,
`<app data>/browser/work`, still called "the work profile". `safeStorage` became Windows
DPAPI (§9). `net.fetch` became one shared `reqwest` client for API traffic. `session.fetch`
for account mode became requests issued *inside* a hidden webview of the work profile, so
cookies never leave it (§12). `webContents.debugger` became WebView2's DevTools Protocol API.
`app.getGPUInfo` became DXGI adapter enumeration. `dialog` became `tauri-plugin-dialog`. Single
instance became `tauri-plugin-single-instance`. `better-sqlite3` and Drizzle became `rusqlite`.
The napi addon became direct linking to `zvs-core`. The Playwright-Electron e2e became
Playwright over WebView2 CDP in an `e2e`-feature build. electron-builder became the Tauri
bundler (NSIS). Electron user data is **not** migrated: the Tauri build starts fresh.

---

## 3. Stack

### Verified and pinned

| Piece | Version | Note |
| --- | --- | --- |
| Node | 26.7.0 | dev toolchain only (renderer build, Vitest, Playwright); not shipped |
| pnpm | 12.3.4 | workspace manager |
| Rust / cargo | 1.93.1 | stable channel, edition 2024; the host, the core and the sidecar |
| `@kiyotakkkka/zvs-uikit-lib` | 8.3.0 | ESM-only; peers `react@^19`, `react-dom@^19`; subpaths `.`, `./chart`, `./code-view`, `./server`, `./styles.css`; brings `clsx`, `tailwind-merge`, `shiki`, `recharts`, `@tanstack/react-virtual` |

### Chosen, pin exact versions at bootstrap

Do not copy version numbers into `package.json` from this table — resolve latest stable at
bootstrap and pin there.

| Layer | Choice |
| --- | --- |
| Shell | Tauri 2 (current stable, `unstable` feature for multi-webview); WebView2 on Windows |
| Build | Vite (renderer + browser-ui + recovery page), Tauri CLI, cargo, Turborepo |
| Language | Renderer: TypeScript, `strict: true`, `verbatimModuleSyntax`, ESM. Host: Rust, edition 2024, `unsafe_code` denied outside named Win32/COM modules |
| UI | React 19, Tailwind CSS, `@kiyotakkkka/zvs-uikit-lib` |
| State | MobX + `mobx-react-lite` |
| Routing | react-router (hash history) — **Assumption** |
| Validation / DTO | Zod in the renderer and `packages/shared`; serde (+ a `Validate` trait) in `zvs-contract` |
| Data | `rusqlite` (`bundled` SQLite), hand-written repositories, embedded SQL migrations with checksums |
| AI | Hand-written adapter families in the Rust host over one shared `reqwest` client — no vendor SDK |
| Vector | LanceDB (Rust crate) in `zvs-core`, linked in-process |
| Native | tokio (async), `tokio-util` cancellation, a plain Rust binary for the sidecar, the `windows` crate for DPAPI/DXGI/WebView2 COM |
| Test | Vitest (`packages`, renderer), `cargo test` (host, core, sidecar), Playwright over WebView2 CDP (e2e) |
| Viewer | `pdf.js` (renderer), sidecar conversion for DOCX/XLSX/PPTX |
| Browser | Tauri child webviews in a dedicated WebView2 profile + WebView2's DevTools Protocol API (CDP) |
| Package | Tauri bundler, NSIS, per-user install |

---

## 4. The three domains

```
┌──────────── host (Rust, inside the zvs-studio process) ────────────┐
│  zvs-studio: Tauri shell · windows · webviews · CDP · OS ports     │
│      ↓ composition root                                            │
│  zvs-host:  ipc handlers → services → repositories → SQLite        │
│                    ↓                                               │
│              drivers: core · sidecar · ai · mcp · integrations     │
└──────────────────────────────┬─────────────────────────────────────┘
                               │  ipc_call + events Channel (capability-scoped)
┌──────────────────────────────┴─────────────────────────────────────┐
│              renderer (React + MobX, in the `studio` webview)      │
│  pages → features → organisms → molecules → atoms                  │
└────────────────────────────────────────────────────────────────────┘

   packages/shared — Zod DTOs + IPC contract (renderer side)
   crates/zvs-contract — the serde mirror (host side)
   golden fixtures in packages/shared/fixtures bind the two
```

**Rules.** On the TypeScript side, `shared` never imports from `renderer`, and `ui/` never
imports features or stores. This is enforced by ESLint boundary rules and TypeScript project
references. On the Rust side, the crate graph is one-way:
`zvs-studio → zvs-host → { zvs-contract, zvs-core }` and `zvs-jobd → zvs-core`.
**`zvs-host` has no Tauri dependency**: anything that needs the shell (dialogs, theme,
webviews, GPU, DPAPI) reaches it through a port trait that `zvs-studio` implements. `zvs-core`
and `zvs-contract` depend on nothing of ours. A violation fails the build, not review.

---

## 5. Repo layout

```
zvs-ai-studio/
├─ apps/
│  └─ studio/                    # the Tauri app
│     ├─ vite.config.ts          # web entries: studio renderer, browser-ui, recovery page
│     ├─ src/                    # web code only — no host code lives here any more
│     │  ├─ renderer/            # React — see §13
│     │  │  ├─ app/              # bootstrap, bridge, router, providers, theme
│     │  │  ├─ ui/               # atoms · molecules · organisms · templates
│     │  │  ├─ features/         # one folder per domain
│     │  │  ├─ pages/            # one per route, composition only
│     │  │  ├─ viewer/           # document viewer dock — see §20
│     │  │  └─ stores/           # root store + cross-cutting stores
│     │  └─ browser-ui/          # chrome for the browser workspace — see §21
│     └─ src-tauri/              # crate `zvs-studio`: the shell and composition root
│        ├─ tauri.conf.json      # windows, CSP, isolation pattern, bundle
│        ├─ capabilities/        # what each webview label may invoke
│        └─ src/
│           ├─ lib.rs            # composition root: builds zvs-host, registers commands
│           ├─ commands.rs       # ipc_call · events_subscribe · browser_* — label-checked
│           ├─ browser/          # child webviews, tabs, account gateway, CDP client
│           └─ platform/         # windows, theme, window state, dialogs,
│              └─ windows/       #   DPAPI · DXGI · WebView2 COM (the only `unsafe`)
├─ packages/
│  ├─ shared/                    # Zod DTOs + contract + error codes + event types + fixtures
│  ├─ ipc/                       # renderer client (`call`/`subscribe` → typed client)
│  └─ tsconfig/  eslint-config/  # shared configs
├─ crates/
│  ├─ zvs-core/                  # pure logic: chunking, OCR, embedding, lance, graph
│  ├─ zvs-contract/              # serde mirror of packages/shared
│  ├─ zvs-host/                  # the host — no Tauri dependency — see §8
│  │  ├─ migrations/             # NNNN_name.sql, embedded, checksummed
│  │  └─ src/
│  │     ├─ ipc/                 # IpcServer + channel handlers, thin
│  │     ├─ services/            # ALL business logic
│  │     ├─ kernel/              # execution kernel — see §11
│  │     ├─ drivers/             # core, sidecar, ai, mcp, mail, telegram, git
│  │     ├─ data/                # schema rows, repositories, migration runner
│  │     ├─ indexing/ downloads/ runtimes/ tools/ documents/
│  │     ├─ browser/             # pure browser policy, lifecycle, automation policy
│  │     └─ platform/            # paths, logger, ids, clock, events, archive, metrics
│  └─ zvs-jobd/                  # sidecar binary, thin wrapper over zvs-core
├─ e2e/                          # Playwright over WebView2 CDP
├─ design/                       # the maket (source of truth for UI)
├─ docs/                         # this file, ADRs
├─ Cargo.toml                    # cargo workspace
├─ pnpm-workspace.yaml
└─ turbo.json
```

`crates/zvs-core` holds the real compute. The host links it directly, and the sidecar is a
thin adapter over it, so a capability can move between in-process and the sidecar without a
rewrite.

### Path map (Electron TS host → Rust), for tasks written before revision 2.0

| Old path (TypeScript, Electron) | New home |
| --- | --- |
| `apps/studio/src/host/main.ts` | `apps/studio/src-tauri/src/lib.rs` (composition root) |
| `host/ipc/*.ts` | `crates/zvs-host/src/ipc/*.rs` |
| `host/services/<Domain>Service.ts`, helpers | `crates/zvs-host/src/services/<domain>.rs` |
| `host/kernel/*` | `crates/zvs-host/src/kernel/*` |
| `host/data/schema`, `repositories`, `migrate.ts` | `crates/zvs-host/src/data/{schema,repositories,migrate}`; SQL in `crates/zvs-host/migrations/` |
| `host/drivers/ai`, `drivers/mcp`, `drivers/sidecar` | `crates/zvs-host/src/drivers/{ai,mcp,sidecar}` |
| `host/drivers/rust` (napi `RustCore`) | `crates/zvs-host/src/drivers/core.rs` (direct `zvs-core` calls) |
| `host/indexing`, `downloads`, `runtimes`, `tools` | `crates/zvs-host/src/{indexing,downloads,runtimes,tools}` |
| `host/browser/policy.ts`, `lifecycle.ts` | `crates/zvs-host/src/browser/` |
| `host/browser/BrowserViewManager.ts`, `sessionGateway.ts` | `apps/studio/src-tauri/src/browser/` |
| `host/platform/{paths,logger,ids,events,eventRecorder,archive,longPath,metrics,device}.ts` | `crates/zvs-host/src/platform/` |
| `host/platform/{windows,windowState,theme,gpu}.ts` | `apps/studio/src-tauri/src/platform/` |
| `host/services/CryptoService.ts` backend (`safeStorage`) | `apps/studio/src-tauri/src/platform/windows/dpapi.rs` behind `CryptoBackend` |
| `src/preload/*` | gone; `capabilities/*.json` + `src/renderer/app/tauriBridge.ts` |
| `crates/zvs-napi` | gone |
| `apps/studio/tests/*.test.ts` (host) | `#[cfg(test)]` modules and `crates/zvs-host/tests/` |

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

### The Rust mirror (`crates/zvs-contract`)

The host is Rust, so every DTO also exists as a serde type with the **same name**.
- `#[serde(rename_all = "camelCase")]` on every struct.
- An optional field is `Option<T>` with `default` and `skip_serializing_if = "Option::is_none"`.
  It is never `null` on the wire.
- Ids are `#[serde(transparent)]` newtypes, and enums carry the same tags.
- Unknown fields are ignored, as Zod strips them.
- Refinements that serde cannot express live in a `Validate` impl.

The two definitions are bound by **golden fixtures** in `packages/shared/fixtures/`. Vitest
parses each one with the Zod schema. `cargo test` deserialises, validates and re-serialises
each one and requires byte-equal JSON. That catches casing, null-versus-absent and tag drift
in CI. The channel list is generated from `contract.ts` into
`packages/shared/contract.channels.json`, which the Rust IPC server must cover exactly. Code
generation in either direction was rejected: the renderer's Zod schemas carry refinements and
Russian messages that do not round-trip.

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
- `zvs-host::ipc::IpcServer` is the host registrar. Handlers are typed by their `zvs-contract`
  input and output, and `finish()` checks the registered set against the generated channel
  list, so a forgotten channel fails a test. Every call enters through one Tauri command,
  `ipc_call(channel, payload)`, which checks that the caller is the `studio` webview and then
  dispatches. A panicking handler is caught and costs one call.

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

One channel carries a discriminated union. Under Tauri it is a `Channel` the renderer hands
to the host once, through `events_subscribe`; the host's `EventBus` writes to it through an
`EventSink` port (the Electron name `zvs:events` survives only in history):

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

### Bridge and capabilities

There is no preload. The renderer's bridge, `src/renderer/app/tauriBridge.ts`, implements the
same two functions as before, `call(channel, payload)` and `subscribe(handler)`, over Tauri's
`invoke` and a `Channel`. Nothing else in the renderer talks to Tauri. What the webview can
reach is fixed by its **capability**, not by convention. The `studio` webview may invoke
exactly `ipc_call` and `events_subscribe`, with no core plugin permissions: no filesystem, no
shell, no HTTP. Every command re-checks the caller's webview label. Tauri's isolation pattern
filters messages before they reach the core. Adding a third command is a security decision,
not a convenience.

---

## 8. Host layering

Strict one-way dependency:

```
ipc/  →  services/  →  data/repositories/  →  rusqlite
             ↓
         drivers/  (zvs-core in-process · sidecar · ai adapters · mail · telegram · git · mcp)
```

All of this lives in `crates/zvs-host`, which has no Tauri dependency. Shell capabilities
(file dialogs, window theme, webviews and the account gateway, GPU enumeration, DPAPI) are
port traits implemented in `zvs-studio` and handed in by the composition root. Async code is
tokio. Cancellation is a `CancellationToken`, the Rust form of `AbortSignal`. A database lock
is never held across an `.await`.

- **`ipc/`** handlers are thin: parse input, call one service method, map the result to a
  DTO. No branching on business rules, no DB access, no driver calls. If a handler is longer
  than about fifteen lines, logic has leaked into it.
- **`services/`** hold all business and application logic. A service may call repositories,
  drivers and other services. Services are constructed with their dependencies (plain
  constructor injection through the composition root in `apps/studio/src-tauri/src/lib.rs`)
  so they can be unit-tested without Tauri.
- **`repositories/`** are the *only* code that writes SQL. One repository per aggregate,
  exposing intention-revealing methods (`findEnabledByCapability`), never a leaked query
  builder. Repositories return entities; services map entities to DTOs.
- **`drivers/`** wrap everything external behind a trait owned by us. The AI adapters,
  `zvs-core`, the sidecar, an IMAP client: each sits behind a port so it can be faked in tests
  and swapped without touching services.

**Transactions** belong to services, not repositories: a service opens the transaction
(`db.transaction(|tx| …)`) and passes the handle down, so one unit of work can span several
repositories.

---

## 9. Data layer

- `rusqlite` (bundled SQLite) opened in **WAL** mode, `foreign_keys = ON`,
  `busy_timeout = 5000`, `synchronous = NORMAL`. One connection, one writer.
- Database file `studio.sqlite` in Tauri's app data directory
  (`%APPDATA%\<bundle identifier>`). Never bundled, never in the app directory. Electron's old
  `userData` is not read: the Tauri build started fresh.
- Row types in `zvs-host/src/data/schema/*.rs`, one module per table.
- Migrations are hand-written SQL in `crates/zvs-host/migrations/NNNN_<name>.sql`, embedded
  with `include_str!`, recorded with a checksum in `__migrations`, and applied **on startup
  before any service is constructed**. `0000_baseline.sql` is the squashed equivalent of the
  Electron-era Drizzle migrations `0000`–`0021`. Editing an applied migration is a hard
  failure. A failed migration blocks boot and shows a recovery window. The app never runs on
  a half-migrated database.
- Backup the DB file before each migration run; keep the last three.

Aggregates (initial): `secret`, `identity`, `provider`, `model`, `vector_store`,
`vector_document`, `conversation`, `message`, `scenario`, `scenario_version`, `run`, `step`,
`skill`, `mcp_server`, `tool`, `tool_permission`, `integration`, `download`, `task`,
`setting`.

**Secrets at rest**: the secret *value* is encrypted with **Windows DPAPI** (current-user
scope, called through the `windows` crate behind a `CryptoBackend` port; on other platforms
the backend reports itself unavailable until verified), and the ciphertext is stored in SQLite;
metadata (name, type, scope, tags, rotation date) is plaintext so the Secrets page can list
and filter without decrypting. Decryption happens in a service, only at point of use, and a
decrypted value is never logged, never returned in a DTO, and never crosses IPC.

---

## 10. Rust boundary (D1)

| Capability | Where | Why |
| --- | --- | --- |
| Vector search / upsert (LanceDB) | **in-process** (`zvs-core`) | Latency matters; called on every chat turn |
| Embedding a short text | **in-process** | Small, fast, bounded |
| Hashing, chunking, tokenizer counts, graph traversal | **in-process** | Pure and quick |
| Corpus indexing / re-index | **host orchestration + sidecar jobs** | Minutes long; needs progress, pause, cancel |
| PDF parsing, OCR input extraction | **sidecar** | The least trustworthy input; native parsing is the likeliest crash |
| Downloads, model file verification / conversion | **sidecar** | Long, IO-bound, killable |

**In-process rules** (they replace the old addon rules). `zvs-core` is reached only through
the `drivers::core` ports (`TextCore`, `VectorCore`, `GraphCore`). Synchronous core work runs
on `spawn_blocking`, so the async host never blocks. Every port method is wrapped in
`catch_unwind`, which turns a panic into `NATIVE_ERROR` and costs one call, not the app.
Cancellation is a `CancellationToken` passed into the core.

**Sidecar protocol.** One long-lived child process, newline-delimited JSON-RPC over
stdin/stdout, stderr to the log file. Messages: `job.start`, `job.cancel`, `job.progress`,
`job.done`, `job.error`, `ping`. The host owns the lifecycle: spawn on demand, health-check,
restart with backoff, and hard-kill on cancel. Because the sidecar is a separate process,
"pause"/"cancel" on the Tasks page are real, and a crash costs one job, not the app.

The sidecar ships as a Tauri `externalBin` next to the main executable and is located through
`platform::paths`.

**Per D2**, neither `zvs-core` nor the sidecar opens SQLite. They return results; the host
persists through repositories.

---

## 11. Execution kernel (D7)

One core in `zvs-host/src/kernel/`, three front-ends.

```rust
#[async_trait]
pub trait NodeDef: Send + Sync {
    type Input: DeserializeOwned + JsonSchema + Send;  // schema also feeds tool exposure
    type Output: Serialize + JsonSchema;
    const TYPE: &'static str;                // "llm.generate", "vector.search", "mail.send"
    fn permission(&self) -> PermissionRequirement;  // resolved against the tool ceiling — §14
    fn side_effect(&self) -> SideEffect;     // decides whether recovery may resume it
    async fn run(&self, ctx: &StepContext, input: Self::Input) -> Result<Self::Output, AppError>;
}

pub struct StepContext {
    pub run_id: RunId,
    pub cancel: CancellationToken,           // cancellation is cooperative and universal
    pub events: StepEmitter,                 // one path to the UI
    pub approvals: ApprovalGate,             // request_approval(req).await -> ApprovalDecision
    pub services: KernelServices,            // narrow, explicit
}
```

Typed nodes are registered type-erased over `serde_json::Value`. The registry validates input
on the way in and output on the way out.

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

The AI layer lives in the **host** only (`zvs-host/src/drivers/ai/`). The renderer has no
vendor code, no API keys and no session cookies. There is no vendor SDK. Every adapter family
is hand-written over one `Transport` seam. API traffic shares one `reqwest` client that uses
the OS root store (`rustls-tls-native-roots`) and the system proxy, which is what `net.fetch`
gave us under Electron.

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
  the integrated browser (§21). The host then issues requests **from inside the work
  profile** (decided 2026-10-10). An *account gateway* keeps one hidden webview per vendor
  origin in that profile, parked on a static, script-free same-origin document. Each request
  is a `fetch()` run in that page over WebView2's DevTools Protocol. The status, headers and
  body stream back through a `Runtime.addBinding` function that exists only in that hidden
  page. The vendor's cookies ride along exactly as they do in the browser tab, because it *is*
  the browser. No Tauri capability is granted to any remote origin. Where the vendor also
  requires a bearer token (Qwen does), it is captured from the vendor page's own requests
  (WebView2 `WebResourceRequested`, only when the tab's origin matches the request origin) and
  stored through `SecretService` like any other credential: encrypted at rest, never in a
  DTO, never in the renderer. **Cookies are never copied out of the profile**. Reading them
  into a Rust HTTP client was considered and rejected.

### Structure

- `drivers/ai/ports.rs` — our ports: `TextGenerationDriver`, `EmbeddingDriver`, `ImageDriver`.
  Services depend on these, never on a vendor's wire format.
- `drivers/ai/transport/` — `Transport` is the seam between the two auth modes: `ApiTransport`
  (over `reqwest`) and `AccountTransport` (over the `SessionGateway` port, which the account
  gateway in `zvs-studio` implements) both expose one `request()`/`stream()` shape. **An adapter never
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
  one event channel (D4). The renderer never sees a vendor payload.

### Accounts

An `account` row is an identity linked from the browser session: external id, masked email,
display name, avatar, optional bearer-token secret ref, expiry, status, profile (the column
keeps its Electron-era name, `partition`). Its
lifecycle is link → healthy → `needs-relink`. An expired session is a **distinct state** from
a bad API key: the Accounts tab offers Re-link, and the app never opens a login page on its
own. The schema permits several accounts per vendor; one browser profile means one live
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

- Renderer in a Tauri webview with no Node and no direct OS access. Strict CSP set in
  `tauri.conf.json` (no `unsafe-inline`, no `unsafe-eval`), the isolation pattern on, no
  remote code, navigation and new windows denied.
- IPC surface is capability-scoped (§7). The `studio` webview gets two commands, the browser
  chrome one, and site and account-gateway webviews **none**: no capability ever lists a
  remote URL. Every command checks the caller's webview label.
- Secrets: encrypted at rest via DPAPI, decrypted only inside a host service, zeroised after
  use, never in a DTO, never logged, never in the renderer.
- `unsafe` Rust exists only in named Win32/COM modules under
  `src-tauri/src/platform/windows/` and the WebView2 glue, each block with a `// SAFETY:`
  comment. `zvs-core` forbids it outright.
- No debugging surface in shipped builds. WebView2's remote-debugging port exists only in
  builds with the `e2e` cargo feature. Release builds clear `WEBVIEW2_*` environment
  overrides before the first webview is created.
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

- **Logging** — structured JSON lines to `<app data>/logs/studio.jsonl`, rotating at 5 MiB
  and keeping three files, in the same line shape as the Electron era. A run's events are also
  written per-run so Runs & Logs can replay one execution without scanning everything.
- **Config** — user settings in SQLite; machine-level flags in a small JSON file so a broken
  DB can still boot.
- **Paths** — every path is resolved through `zvs-host::platform::paths` from the Tauri
  directories handed in by the composition root. No `current_dir`, no relative IO anywhere
  else.
- **IDs** — UUID v7 (time-sortable) generated on the host.
- **Clock** — a `Clock` trait injected into services so time-dependent logic is testable.

---

## 16. Build and packaging

- `pnpm build` runs the Vite web build and `tauri build --no-bundle`. `pnpm package` builds
  the sidecar in release, copies it to its triple-suffixed `externalBin` name, builds the web
  assets, and runs `tauri build` to produce the NSIS installer.
- One executable holds the shell, the host, `zvs-core` and SQLite. The only other artefact is
  the `zvs-jobd` sidecar, placed next to the executable. No native module is rebuilt and no
  Node runtime ships.
- WebView2 is a runtime dependency. The installer's `webviewInstallMode` is decided in
  TASK_056.
- **Windows first** (the development target); macOS and Linux are structurally supported but
  unverified until someone builds them, and browser automation is WebView2-only.
  **Assumption.**
- Code signing and auto-update are **out of scope for v1**. **Assumption.**

---

## 17. Conventions

| Kind | Convention | Example |
| --- | --- | --- |
| DTO | `<Thing>Dto`, one per file, same name in Rust | `ProviderDto.ts` / `zvs_contract::providers::ProviderDto` |
| Rust module | snake_case file, PascalCase type | `services/provider.rs` holds `ProviderService` |
| Entity / table | snake_case table, snake_case Rust field | `vector_store` / `VectorStoreRow` |
| Repository | `<Aggregate>Repository` | `data/repositories/provider.rs` |
| Service | `<Domain>Service` | `ProviderService` |
| Driver | `<Thing>Driver` + a port trait | `OllamaDriver: TextGenerationDriver` |
| Shell port | `<Thing>Port` trait in `zvs-host`, implemented in `zvs-studio` | `PickerPort`, `ThemePort` |
| Tauri command | snake_case, label-checked | `ipc_call`, `browser_command` |
| Store | `<Domain>Store` | `ProviderStore.ts` |
| Component | PascalCase, one per file, default export | `ProviderCard.tsx` |
| IPC channel | `domain.verb`, dot-separated, lowercase | `providers.test` |
| Kernel node | `namespace.verb` | `vector.search` |

---

## 18. Assumptions to confirm

1. Router is react-router with hash history.
2. Packaging via the Tauri bundler (NSIS, per-user); no signing or auto-update in v1.
3. Windows is the only verified target for v1. WebView2 is the only verified webview, and
   browser automation (CDP) and the account gateway depend on it.
4. **Confirmed (TASK_010), re-implemented on DPAPI (TASK_062):** secrets encrypted with the
   OS user's key (Electron `safeStorage` before revision 2.0, DPAPI now), ciphertext in SQLite
   (not a separate vault file). Values are therefore bound to the OS user account: copy the
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
9. The browser ships one profile ("work", the WebView2 data directory `<app data>/browser/work`).
   Multiple named profiles come later, as further data directories under `browser/`.
10. **Confirmed (2026-10-10):** no import of Electron-era data. The Tauri build starts from an
    empty data directory, and existing secrets are re-entered.

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

Steps 1–9 were built on Electron with a TypeScript host. Revision 2.0 inserts the **migration
order** before the remaining steps. Each step ports a slice and leaves the Tauri app answering
more channels:
shell and bridge → contract mirror → host skeleton (paths, logger, events, IPC server) →
data layer → secrets → AI drivers → providers → browser workspace → account gateway →
accounts → in-process core, sidecar and device probes → vector stores and graph → kernel →
downloads and runtimes → indexing → tools and MCP → chat → e2e parity → cutover
(TASK_058 – TASK_076).

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
cancellable. The result is cached at `<app data>/cache/documents/<sha256>.pdf`, keyed by content
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

## 21. Agent browser workspace (D9)

> History: D9 first specified a separate `BrowserWindow`. TASK_045 (2026-09-09) attached the
> browser to the studio window like VS Code, at the user's request. Revision 2.0 keeps that
> presentation and moves it to Tauri child webviews (TASK_065).

`/browser` is a route in the studio window. The browser itself is made of **child webviews**
that the host lays over the route's content area: one for the chrome and one per tab. When
the route is left they are hidden, and tabs are kept.

```
studio window
├─ studio webview (renderer)        the app; /browser reserves the area
├─ chrome webview (browser-ui)      tabs · toolbar · Sites & cookies · agent panel · approval bar
└─ site webview per tab             the actual site — never trusted
```

The chrome webview's capability grants one command, `browser_command`, and a state channel.
**Site webviews are in no capability at all.** The page has no bridge, no IPC, and no
knowledge that it is being automated beyond what a normal browser reveals. The profile denies
every permission request, cancels downloads, refuses non-web navigation in any frame, and
turns popups into tabs (adopting the created webview, so POST and OAuth openers survive).

### Session and identity

One persistent profile, "work": a dedicated WebView2 user-data directory,
`<app data>/browser/work`. The user logs into sites once, inside the app, and those cookies
survive restarts, which is what makes the automations useful at all. The profile is separate
from the studio webview's environment and holds no app credentials.

> Update (2026-09-10, revised 2026-10-10): this profile is also the identity source for
> **account-mode AI providers** (§12). The Accounts tab's Login button opens the vendor's site
> in this browser. The host then issues that provider's API calls **inside the same profile**,
> through the account gateway's hidden webviews. Cookies stay inside the profile: the AI layer
> reads no cookie values, and the renderer sees only a masked identity. Clearing a vendor's
> cookies in Sites & cookies therefore un-links that account, and the provider must report
> `session-expired`, not a generic auth failure.

### Automation

Driven through the DevTools Protocol that WebView2 exposes on each site webview
(`CallDevToolsProtocolMethod` / `GetDevToolsProtocolEventReceiver`), through the same small
CDP client the account gateway uses. The driver in `src-tauri/src/browser/` sits behind an
`AutomationPort`, and the policy engine and `browser.*` nodes in `zvs-host` depend only on that
port. The driver exposes a small, audited set of primitives; everything else is deliberately
absent.

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
