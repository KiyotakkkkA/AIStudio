# ZVS AI Studio — Architecture Passport

**Status:** locked · **Date:** 2026-10-10 · **Doc version:** 2.0

> **Revision 2.0 (2026-10-10): Tauri and a Rust host.** Electron is replaced by Tauri 2, and
> the whole host (services, kernel, repositories, drivers) is rewritten in Rust. See D10. Phase
> M in `/tasks/INDEX.md` (TASK_058 – TASK_076) carried out the move, and it is **complete**
> (2026-10-19): the Electron shell, the TypeScript host, the napi addon, Drizzle and
> `better-sqlite3` are gone. §5 maps every old path to its new home for tasks written before
> the revision. Version 1.1 is in git history.

This is the binding reference for stack and structure. Design mockups for all fifteen
surfaces live in `/design` (canvas: `zvs-ai-studio-maket.html`). Anything in this document
marked **Assumption** was not explicitly specified and may be changed cheaply before
bootstrap; anything marked **Locked** was decided and should only change through a new
revision of this file.

---

## 1. What the product is

A local-first desktop studio for building and running AI automations. Fifteen routed
pages behind one navigation rail, plus two surfaces that are not routes:

| Group | Pages |
| --- | --- |
| Workspace | Secrets & Identity · AI Providers · Vector Stores |
| Build | Chat · Agentic Build · Scenarios |
| Extend | Skills · Connections (MCP) · Integrations · Tools |
| System | Tasks · Downloads · Runs & Logs · Permissions · Settings |

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
| D8 | Document viewer | **Right dock that expands to overlay; every kind renders in the renderer, office included (ADR 0003)** | No converter to install or keep alive; read-while-chatting stays possible | Office layout is approximate |
| D9 | Agent browser | **Child webviews attached to the studio window, in their own WebView2 profile, driven over WebView2's CDP; tiered approval** *(rev. 2.0; was `BrowserWindow` + `WebContentsView`)* | No second browser to ship, every action visible live, instant takeover; the user logs into sites once | We own the automation primitives, a logged-in profile raises the stakes on prompt injection, and CDP access is WebView2-only, so automation is Windows-only |
| D10 | Shell and host | **Tauri 2 (WebView2 on Windows); the whole host in Rust; no Node runtime in the shipped app** *(decided 2026-10-10; complete 2026-10-19, TASK_076)* | One language for host and core, so no FFI layer, no Node ABI rebuilds and no JSON hop to reach `zvs-core`. A much smaller install and memory footprint than a bundled Chromium. Capability-scoped IPC instead of a preload convention | A rewrite of ~22k lines of host TypeScript (Phase M). Multi-webview needs Tauri's `unstable` feature. Only WebView2 is verified. Electron APIs are replaced one by one, as listed below |
| D11 | Account-mode providers | **Qwen only; no vendor whose web API demands a proof of work or other anti-automation challenge** *(decided 2026-10-19, ADR 0001)* | Account mode sends the requests the user's own signed-in page sends and nothing more; solving a vendor's anti-bot challenge would mean shipping code that defeats its abuse protection | DeepSeek is offered only through its official API (`openai-compatible`) or OpenRouter; existing `deepseek-web` rows are deleted by migration 0002 |
| D12 | Inbound events | **No webhooks; integrations pull through supervised background workers (long polling, IDLE, interval polling)** *(decided 2026-10-28, ADR 0004)* | A desktop app has no public endpoint, and a listener open to the internet is a server product with its own attack surface | Event latency is the poll interval; events arrive only while the app runs |

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
│  ├─ zvs-pipe/                  # the CLI pipe: per-user named pipe, owner checks
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
  z.object({ type: z.literal('question'), streamId: StreamId, seq: z.number(), request: QuestionRequestDto }),
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

### The CLI pipe

The `aistudio` console client is the only caller that is not a webview. It reaches the host
over a named pipe, `\\.\pipe\zvs-ai-studio-cli-<user SID>`, served by the running app
(`crates/zvs-pipe`, `zvs-studio/src/cli.rs`). Four rules hold:

- **Owner only.** The pipe is created as the first instance with a protected DACL that grants
  access to the current user's SID alone, and it rejects remote clients. The client reads the
  server's process token through `GetNamedPipeServerProcessId` and refuses a pipe served by
  another user, so a squatted name is never trusted.
- **Greeting first.** Nothing is sent before the client says `hello` with the protocol
  version; only then is it subscribed to the event channel. A line over 8 MiB or a malformed
  message drops the connection.
- **An allow-list, not the contract.** The pipe dispatches through the same `IpcServer`, but
  only the channels in `zvs_host::cli::CLI_CHANNELS` — chat, folder and run answers, the
  execution policy, provider and model listings, skills and MCP servers. Secrets, settings,
  accounts, provider editing and `cli.install`/`cli.uninstall` stay studio-only. Widening the
  list is a security decision, as above.
  Copilot exception (2026-10-31): `cli.copilot.login` and `cli.copilot.cancelLogin` are
  allowed. Login links only the active GitHub Copilot identity, creates its account-mode
  text provider if needed, and discovers models through the existing services. No caller
  supplied URL, credential, account family or provider patch is accepted.
- **Same events, same answers.** CLI connections are extra event-bus subscribers beside the
  window. An approval or a question reaches both; the first answer wins and the other side
  sees `Conflict`.
- **Account checks are announced.** When the account gateway shows a vendor's browser check
  (§12), every CLI connection gets a pipe-only `notice`, also on connect while the check is
  still shown. The client lets the app take the foreground (`AllowSetForegroundWindow`) and
  sends `focusVerification`, which reveals the check window again. Nothing is solved for the
  person; the check is still passed in the vendor's own page.

`cli.install` puts the folder holding `aistudio.exe` on the **user** `Path` (HKCU, no
elevation) and turns the tray on, so the app keeps serving the pipe with its window closed.
A CLI launch starts the app with `--cli`, hidden in the tray when the tray is on.

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
- `run_event` is the replay log, kept lean (TASK_045_EXTRA_5). Kernel step events are stored
  without `input` and `output`; `runs.events` puts them back from the `step` row (the output
  only when the event's status is the row's). Tokens reach the renderer one by one, but they
  are stored merged per kind: a merged row is written when another event commits, the kind
  changes, or 250 ms have passed. The row carries the last token's `seq`, so the log has gaps.
  `RunService::record` adds the tail that has not been written yet for a live run. The chat
  appends to the message row at most every 250 ms, using SQL concatenation. Chat tool-call
  progress is a `step` event `{ domain: "tool-call", index, call }` with one call. The full
  list is stored one row per call in `message_tool_call (message_id, idx)`: a change rewrites
  only that call, and reading a message assembles the list in order. An agentic session's
  model transcript is stored the same way in `agentic_transcript (session_id, seq)`: a save
  appends only the new entries, and the session row no longer carries it, so updates, plan
  edits and listings do not read or rewrite the history.
- One folder per conversation or agentic session: `<app data>/conversations/<id>/` with
  `work/` (the chat folder), `outputs/manifest.json` (created files, recorded by reference,
  never copied) and `snapshots/<runId>/` (`fs.apply` originals). A run without a conversation
  uses its own id as the folder. Removing the conversation removes the folder. Retention
  removes a pruned run's snapshots and manifest entries, but not the files they point to.
  `storage.usage` reports bytes per category, cached until `refresh`. `storage.clearCache`
  empties `cache/` and is refused while a run is unfinished.

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
    fn permission_for(&self, input: &Value) -> PermissionRequirement;  // defaults to permission()
    fn defers_approval(&self) -> bool;       // the node raises its own approval, see below
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
- **Per-input permission** — admission (graph steps and model tool calls alike) asks
  `permission_for(input)`, so one call of a tool can need a stricter minimum tier than
  another; `fs.apply` asks at least `ask` when its batch deletes a file. The ceiling still
  applies on top, and `off` still denies. A call that touches only the app's own folders
  (`chat:`, `skill:<key>/`) returns `PermissionRequirement::AppFolder`, and the permission
  service resolves it to allow unless the tool is disabled or `off`; an explicit `ask` minimum
  (an agentic session in `ask` mode) still turns it into an approval.
- **Deferred approval** — a node with `defers_approval()` does not suspend at admission. When
  the tier says `ask`, admission hands it a ticket on the `StepContext` instead, and the node
  raises one approval itself once it can show what it is about to do (`fs.apply`: the combined
  diff, merged with any folder request into a single modal).
- **Question gate** — `StepContext::ask_user` suspends the run on a question to the user,
  counted like an approval: the run is `blocked`, a `question` event carries the request, and
  nothing new is scheduled until `chat.answer` or `chat.skip` resolves it. It has no timeout;
  cancellation ends it. In chat the model reaches it through `chat.ask`, a bridge that is offered
  to every tool-capable turn but is not a registry tool: no admission, no step row, no budget.
- **Concurrent tool calls** — in the chat loop, consecutive side-effect-free calls of one
  round run concurrently; a call with side effects is a barrier and keeps its place. Results
  go back to the model in the original call order.

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

### Copilot CLI account provider (2026-10-31)

`copilot` is an account-only adapter. `zvs-copilot` contains credential-free protocol types;
the host owns a framed JSON-RPC stdio client to the installed Copilot CLI. GitHub OAuth is
performed in the system browser by the CLI's own account flow; credentials stay in its
credential store. ZVS stores the account identity and checks it before model discovery and
every turn. Browser cookie clearing does not revoke CLI credentials. Unlinking preserves
the CLI login. No vendor SDK or Node runtime is bundled in the app.

Copilot sessions have no built-in tools, hooks, skills, plugins, configuration discovery,
custom instructions or remote steering. Only the host transcript is supplied. Model tool
calls use the existing injected dialect and kernel permission gates. Children are terminated
on completion, stream drop, cancellation or failure. Model/effort availability comes from
the live CLI catalog. See `crates/zvs-copilot/README.md` for protocol observations.

The AI layer lives in the **host** only (`zvs-host/src/drivers/ai/`). The renderer has no
vendor code, no API keys and no session cookies. There is no vendor SDK. HTTP adapters are
hand-written over one `Transport` seam; CLI adapters use host-owned stdio clients. API traffic
shares one `reqwest` client that uses
the OS root store (`rustls-tls-native-roots`) and the system proxy, which is what `net.fetch`
gave us under Electron.

### Two independent axes (decided 2026-09-10)

A connection to a vendor is described by two orthogonal choices, and the code must never
collapse them into one enum:

| Axis | Values | What it decides |
| --- | --- | --- |
| **Adapter family** — `provider.adapter` | `openai-compatible`, `anthropic`, `qwen-web` | The wire shape: request/response mapping, model-list endpoint, streaming format |
| **Auth mode** — `provider.auth_mode` | `api`, `account` | How a request is credentialed and which HTTP stack carries it |

One adapter family serves many vendors: Ollama, OpenRouter, Mistral, LM Studio and vLLM are
all `openai-compatible` rows differing only by base URL and secret. A vendor whose wire
format is its own — Qwen — gets its own family. **Adding a vendor that speaks a
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
  into a Rust HTTP client was considered and rejected. Account mode is plain authenticated
  requests only: a vendor that gates its web API behind a proof of work, a CAPTCHA or another
  anti-automation challenge is offered through its official API instead. That is why Qwen is
  the only account family (and only works while Qwen does not challenge its chat
  endpoints) and DeepSeek is API-only (D11, `docs/adr/0001-account-provider-limitations.md`).

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
  (`chat.qwen.ai/api/v1/auths/`).
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

OpenAI's `openai` account family uses the public ChatGPT-plan OAuth flow instead of browser
cookies. Credentials remain in encrypted secrets; multiple issued-client/subject registrations
are independent. Its provider setting selects direct Responses or Codex app-server with the
same OAuth grant. The context meter reads `providers.usage`, showing reported windows or an
explicit unavailable state. See [ADR 0005](adr/0005-openai-chatgpt-plan-oauth.md).

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
  scenario may narrow that, never widen it. The kernel enforces it at step
  admission; the UI only reflects it.
- **Chat tool set** (`zvs-host/src/services/chat_tools.rs`, 2026-10-27). A chat has no tool
  picker: the Tools and Connections pages are the only switches. `plan_tools` offers every
  model tool the registry allows (available, enabled, tier not `off`), with three conditions.
  An MCP tool is offered only while its server is connected and signed in: `McpServers::usable`
  is a live client, status `connected`, enabled, and no pending sign-in; a call whose token
  cannot be refreshed marks the server as needing a sign-in. Knowledge tools need an
  attached store and a retrieval mode other than `auto`, and memory tools need the memory
  switches. The `browser.*` tools are offered only after a user message carries the
  `/browser-use` command (`ChatCommand`, listed by `chat.commands`). The command then holds for
  the rest of the dialog, and the bundled `browser-use` skill's instructions go into the
  system prompt. Pages can carry prompt injection, so the browser is never reachable by
  search. Core tools (`fs.*`, `code.run`, `shell.run`, `skill.load`) always go out with their
  schemas. When the other tools' schemas exceed `DEFERRED_TOKENS_MAX` (2000 tokens), those
  tools are **deferred**: the system prompt lists each by name and first line, and
  `tool.search` (`names` or `query`, at most five matches, refunded against the call budget)
  loads them from the next round. A deferred tool called by name runs and loads too. Loaded
  tools are recorded in the search call's `meta.loaded` and stay offered for the rest of the
  dialog, appended after the core tools so the cached prompt prefix holds. Folder grants,
  tiers, approvals and browser grants are unchanged; deciding *what is offered* is the only
  part this replaced.
- Sidecar runs with the app's privileges but no elevated rights; workspace roots are an
  explicit allow-list checked host-side, not in Rust.
- **Folder access** (`zvs-host/src/workspace/`, TASK_036_EXTRA_1). A chat may bind one
  *working folder*; it starts with none, and binding or unbinding is refused while a turn runs.
  Relative paths resolve against it, and its path goes into the system prompt each turn, with
  a note when it changed since the previous turn. Every other folder is reached through a
  grant: `always` (stored, any chat), `conversation` (stored, deleted with the chat) or a
  once-grant held in memory for one tool call. A grant covers the folder and all its
  subfolders; coverage compares canonical paths (junctions and symlinks resolved, `\\?\`
  stripped) case-insensitively on component boundaries. The grant unit is the nearest parent:
  a file asks for its folder, a listing for the folder itself, a missing path for its deepest
  existing ancestor plus the folders that will be created. `ensure_access` raises **one**
  `kind: "folder"` approval per call for all uncovered folders, answered «Разрешить
  однократно», «Разрешить для этого диалога», «Разрешить всегда» or «Запретить»; a timeout
  or a closed modal denies and the call fails with `PermissionDenied`. Binding an uncovered
  folder asks with the two stored scopes only. Revoking a grant on the «Разрешения» page keeps
  bindings; the chat then reports `workingFolderAccess: "missing"`.
- **Forbidden folders** are refused outright, with no request, both as targets and as
  working folders: drive roots, `%WINDIR%`, `Program Files`, `Program Files (x86)`,
  `%ProgramData%`, the bare user profile root (its subfolders are allowed), every app folder
  (data, cache, logs, downloads, the browser work profile), network (UNC) paths and removable
  media, and any grant that would contain one of them.
- **File tools** (`zvs-host/src/files/`, TASK_036_EXTRA_2) are offered in every chat whose
  model can call tools: `fs.list`, `fs.read`, `fs.search` (`auto`, side-effect free, output
  fenced) and `fs.apply` (`ask` by default). Each resolves its paths through `FolderAccessService`
  and acts on the canonical path, never the model's spelling. Two virtual roots
  (`files/roots.rs`, TASK_045_EXTRA_1) bypass folder access: `chat:` is the conversation's
  file folder (below) and `skill:<key>/` a usable skill's folder, read-only — `fs.apply` and a
  `shell.run` cwd refuse it. Every segment under a root is checked (no `..`, hidden, drive or
  reserved names) and the resolved path must stay inside the root. `chat:` changes skip the
  read-before-write rule and the diff approval; a batch that mixes roots and real paths is
  approved as one diff. `.gitignore` is honoured by
  listings and search; links are not followed. `fs.apply` takes 1–32 changes (`write`, `edit`,
  `move`, `delete`, ≤ 1 MiB of text), plans them in memory over the disk, refuses the whole
  batch on any forbidden or escaping path, on a binary or non-UTF-8 target, or on a file this
  run did not read with `fs.read` or that changed since (a run-scoped read ledger of content
  hashes). It snapshots the originals to `conversations/<id>/snapshots/<runId>/<batch>/` with a
  `manifest.json`, writes each file through a temp file renamed over the target, and restores
  everything already written if any write fails. NTFS has no cross-file transaction: this is
  all-or-nothing by rollback, not atomic. Line endings and a BOM are kept.
- **Process tools** (`zvs-host/src/commands/`, TASK_036_EXTRA_3) are offered alongside the
  file tools: `code.run` (Python through `uv run` with PEP 723 dependencies in uv's app cache,
  JavaScript through `node` as an ES module with `packages[]` installed into the run's scratch
  folder by `npm install --ignore-scripts --no-audit --no-fund`, PowerShell through
  `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File`) and `shell.run`
  (an argument vector, never a shell string; `argv[0]` resolved through the connected toolchain
  folders first, then `PATH`; cwd is the working folder or a `cwd` that goes through
  `ensure_access`, and with neither the call is refused; `argv[0] = skill:<key>/scripts/<file>`
  runs a skill script through uv (`.py`) or Node.js (`.js`, `.mjs`, `.cjs`) with
  `ZVS_SKILL_DIR`, in `chat:` by default). Both are destructive with a minimum
  tier of `ask` through `permission_for`, so `auto` still asks and `off` denies; each raises one
  `kind: "process"` approval showing the program, the cwd, the full source or argv and the
  package list. Folder grants do **not** limit them: the process runs with the user's rights
  and there is no sandbox. `code.run` and `shell.run` share one runner
  (`toolchain/process.rs`): scratch folders, a timeout, kill-tree on cancel, capped streams and
  fenced output.
- **Created files** (`zvs-host/src/artifacts.rs`, TASK_043). After a skill script runs, the
  documents it created or changed in its scratch folder (and paths named in its arguments) are
  recorded by reference in `<app data>/conversations/<id>/outputs/manifest.json` and reported in
  the tool call's `meta.artifacts` (`ArtifactDto`, id `<id>/<name>`, names unique per
  conversation). An id resolves only through the manifest, so a moved or deleted file is gone. The chat shows each as a card with «Скачать»
  (`artifacts.save`: a save dialog, then a copy) and «Перейти» (`artifacts.reveal`). Ids never
  resolve outside the artifact folder.
- **Artifact libraries and pages.** A library is a named app folder whose files are reported
  in place instead of copied: `dashboards` (`<app data>/dashboards/`, id `dashboards/<name>`).
  A skill script gets each library as `ZVS_<NAME>_DIR`, and `shell.run` reports the files
  the run created or changed directly in it; ids never resolve into subfolders or outside.
  Agentic sessions show the same cards, taken from the tool steps' output.
  `artifacts.browsers` lists the browsers registered under `StartMenuInternet` whose program
  exists; `artifacts.openInBrowser` takes a listed browser **id**, never a program path.
  `artifacts.openInApp` opens HTML, SVG, PDF and images in the agent browser (§21) without
  relaxing its web-only navigation: the host serves them from a loopback page server
  (`zvs-host/src/artifact_pages.rs`) started on first use, bound to `127.0.0.1` on a random
  port, `GET /<token>/<id>/<name>` only, with a per-launch random token, a `Host` check
  against DNS rebinding, and `Content-Security-Policy: default-src 'none'; script-src
  'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'` — a
  page can run its own inline code but cannot load or send anything.
- **Chat working folder** (`zvs-host/src/chat_files.rs`). Each conversation, and each agentic
  session, owns `<app data>/conversations/<id>/work/`, created on first use and removed with the
  conversation or session. It is the one app folder an agent reaches without a grant: in a
  chat or session a skill script runs with it as the working directory and `ZVS_CHAT_DIR`
  (other callers keep the per-run scratch folder), and the file tools reach it as `chat:`
  without an approval. Skill tools are part of every chat's core set (see *Chat tool set*), so
  a matching skill only adds its catalogue to the prompt. Every assistant message records the
  tools its turn offered and why (`offeredTools`: `base`, `knowledge`, `memory`, `command` or
  `loaded`). A call to a tool that does not exist is answered with the names that were offered.
  `chat.context` counts the same set, with each tool's source, and the composer's grey tools
  badge renders it. An agentic session reports in
  `tools` the tools its current mode offers, not the stored selection. `skill.load` returns the instructions,
  each file with a one-line purpose, and the text of the manifest's `metadata.read-first` files
  up to 16 000 characters (the rest are named as required reading); tool keys in its result
  are rewritten to the names offered in that turn, and its result limit is six times the
  per-call share, at most 40 000 characters. A script listed in `metadata.requires` is refused
  with a hint, not counted against the call budget, until its files were loaded or read in the
  same turn. The folder is capped at 200 MB and
  1000 files, and writes go through `fs.apply`'s temporary files. Names whose path has a
  segment starting with `_` are working files: they never become artifact cards. Other
  documents a script creates there are reported like scratch results (recorded in the
  conversation's `outputs/manifest.json`). Nothing in the folder is ever opened or run by the host.
- **Dialog branches** (`chat.conversations.branch`, `ChatService::branch`). «Создать ветку в
  новом диалоге» on an answer creates a new conversation with the parent's settings, model,
  stores, working folder and profile, titled `<parent> (ветка)` and unpinned. It copies the
  messages up to and including that answer (new ids, same pins and run ids), the
  conversation-scoped memory and the conversation-scoped folder grants, in one transaction.
  The host then copies `work/` and `snapshots/` and rewrites `outputs/manifest.json` and the
  artifact ids in the copied tool calls (`<parent>/<name>` becomes `<branch>/<name>`), so the
  branch never reads the parent's folder. A failed copy removes the branch. Branching is refused
  while a turn runs in the parent. After that the two dialogs are independent.
- **Working folder explorer** (`zvs-host/src/workspace/explorer.rs`, TASK_036_EXTRA_4). The
  «Файлы» side panel in the chat is the user browsing their own folder, not a tool call:
  `workspace.list` / `workspace.read` raise **no** access request, but they require the chat's
  working folder to be covered by a grant (otherwise `PermissionDenied`), and every path is
  canonicalised and refused when it leaves the working folder (`..`, an absolute path, a
  junction or symlink pointing out). Listings are one level, folders first, `.git` hidden and
  `.gitignore` matches flagged rather than dropped; reads are capped at 1 MiB, cut on a line.
  `workspace.watch` starts one recursive `notify` watcher per chat, shared by subscribers,
  debounced to 300 ms (at most 2 s) and coalesced; changes travel as `step` events
  `{ domain: "workspace", conversationId, changes }` on the watch's stream, and binding,
  unbinding or a vanished folder report the root (`""`) as modified or removed.
- **Terminal** (`zvs-host/src/terminal/`). The bottom dock in the chat is the user's own
  shell, not a tool call: like the explorer it raises **no** access request, and the model has
  no channel to it. Sessions are ConPTY pseudo-consoles (`portable-pty`) owned by
  `TerminalService`, keyed by dialog (at most 8), started in the dialog's working folder or the
  user's home, with the connected toolchain directories on `PATH`. Each session has one stream:
  output as `token` deltas, status changes as `step` events
  `{ domain: "terminal", id, status, exitCode }`, and `end` when it closes. The host keeps a
  256 KiB scrollback so `terminal.attach` can restore the screen, and returns `nextSeq` so
  the renderer drops events it already has. The host answers ConPTY's startup cursor query
  itself, because a session can start before any view is attached. Closing a session,
  deleting its dialog or quitting kills the shell's whole process tree.
- **Agentic workspace** (`zvs-host/src/agentic/`, TASK_040). A session is bound to one
  workspace folder, chosen by the user at creation, canonicalised and checked against the
  forbidden folders. File and process tools called by the session resolve paths against it and
  are refused with `PermissionDenied` outside it; the session raises no folder requests and uses
  no grants. The model never supplies the owner: the host injects the session id into each call.
- **Folder access and tool tiers are separate axes.** A grant says *where* a tool may act;
  the tier on the Tools page says *whether* and with what confirmation it may act. Neither
  widens the other.
- **Execution policy** (2026-10-29). A third axis, above the two: it decides how a call
  that would *ask* is answered, and nothing else. One global mode, shown and switched from the
  composer (the shield button beside the provider control):

  | Call | `always-ask` | `safety-ask` | `auto-bypass` | `no-restrictions` |
  | --- | --- | --- | --- | --- |
  | tier `auto` | runs | runs | runs | runs |
  | tier `ask`, not destructive | asks | runs | runs | runs |
  | `fs.apply` delete, overwrite, move onto a path | asks | asks | rules → judge → asks | runs |
  | `browser.submit`, `browser.download` | asks | asks | asks | runs |
  | `shell.run`, `code.run` | asks | asks | rules → judge → asks | runs |
  | tier `off`, disabled tool, forbidden folder, outside a grant or the session workspace, credential and payment fields | refused | refused | refused | refused |

  The last row is the floor: no mode reaches it. `always-ask` is today's behaviour. Destructive
  is decided by effect, not by the operation's name: a `write` or `edit` that replaces an
  existing file is an overwrite. A file grant is still raised as a `kind: "folder"` approval in
  every mode; the policy answers tier approvals only. `no-restrictions` is chosen through a
  confirmation that names process tools: they run with the user's rights and no sandbox.
- **Judge** (`auto-bypass`). The mode is available only once a judge model is chosen
  («Общие» → «Модель-арбитр»); without one the composer shows it inactive with a link to that
  setting. A destructive call first meets **policy rules**, matched in code: path globs ×
  operation for files, `argv` patterns × cwd for processes, each rule `allow | ask | deny`.
  Precedence is vendor `deny` > user rules > vendor `allow`; rules ship in packs, vendor and
  user, the way skills do, and a pack declares whether it contains any `allow`. A call no rule
  matches goes to the judge with only the structured action (the `fs.apply` batch as one diff,
  or program, cwd, argv and source) and the rule texts — never the conversation, which may carry
  injected content. The judge answers `allow` or `ask` with a reason; anything else, an error,
  a timeout or an unavailable model is `ask`. For processes it can only lower friction for
  commands a rule could not classify: there is no snapshot to roll back, so the judge prompt is
  biased toward `ask`. `deny` from a rule fails the call with `PermissionDenied`.
- **Audit.** Every admitted call records why it ran: `auto`, `granted`, `approved`,
  `policy:<mode>` or `judge:<rule id | model>` with the judge's reason, in the run log and on
  the tool call's details.
- **Implementation (`zvs-host/src/policy/`, TASK_048_EXTRA_1, 2026-10-29).** The mode and the
  judge are stored under the `exec.policy` setting and reached only through `execPolicy.get` /
  `execPolicy.set` (`settings.set` refuses the key). `set` refuses a judge whose provider is
  disabled or whose model is unavailable, and `auto-bypass` without a judge; `get` clears a
  judge that disappeared since, drops `auto-bypass` to `safety-ask` and stores the result.
  `decide(mode, family)` is the table above as a pure function. It is consulted through the
  `ExecReview` port in `KernelServices` at every point an approval would be raised: the gate
  in `RunService` for tools that do not defer, and the deferred approvals of `fs.apply`, the
  process tools and the browser acting nodes. A tool declared destructive whose effect the
  host cannot see asks in every mode but `no-restrictions`. Rule packs are JSON
  (`{ id, allows, rules[] }`, each rule `files: { paths, operations }`, `process: { argv, cwd }`
  or `code: { languages, avoid }`, `**` matching any run of arguments, `{a,b}` alternation,
  case-insensitive). A `code` rule matches a `code.run` script in one of its languages whose
  source matches none of the `avoid` regexes. Since 2026-10-29 the vendor pack ends in two
  catch-alls so routine work stops asking: `allow-unflagged` allows any command no `ask` or
  `deny` rule names (vendor `ask` outranks vendor `allow`), and `allow-plain-scripts` allows a
  script without network use, process launches, generated code, deleting or moving files,
  registry or credential access, decoded payloads or installs; a script that trips a pattern
  goes to the judge. Among rules of equal effect the first in the pack is reported. The vendor
  pack ships in
  `policy/vendor.json`, user packs are read from `<app data>/policy/*.json` on every review,
  and a pack that holds an `allow` it did not declare, or does not parse, is skipped with a
  warning. In a batch the destructive changes are matched one by one: any `deny` refuses the
  batch, any `ask` asks, and the batch runs on rules only when every change is allowed;
  otherwise the judge sees it. The judge is one `generate` call through the provider's own
  driver with a single `verdict` tool (`decision: allow | ask`, `reason` required, written in
  Russian), a 30 s timeout and its own cap of 50 calls per run, after which every call asks.
  `ChatToolCallDto.authorisation` (`ToolAuthorisationDto`: `kind`, `source`, `reason`) carries
  the audit to the tool call details, and each admission writes a `Tool call authorised` line
  to the run log. A folder grant raised with the diff in every mode is unchanged.
- No telemetry, no outbound calls the user did not configure. One narrow exception: **site
  icons** for the web sources a provider returned with an answer (`zvs-host/src/sites.rs`,
  2026-10-27). The renderer never loads a remote image (CSP `img-src 'self' blob:`). It asks
  `sites.icon`, and the host fetches the vendor's icon URL (Qwen's `hostlogo`) or the site's
  `/favicon.ico`. Only `http(s)` addresses are fetched, never `localhost`, and every resolved
  address must be public: loopback, private, link-local, CGNAT, documentation, unique-local
  and IPv4-mapped forms are refused. The connection is pinned to the checked address, uses no
  proxy and no cookies, follows at most three redirects (each checked again), times out after
  4 s and stops at 64 KiB. The bytes must sniff as PNG, JPEG, GIF, WebP or ICO (never SVG), and
  results, misses included, are cached per site for the session. `sites.open` opens a web
  source in the agent browser (§21) through the same page opener as `artifacts.openInApp`;
  only `http(s)` URLs without credentials pass.
- **Page and document content is untrusted input.** Anything the agent reads from a web page,
  a PDF or an email is data, never instruction. Text from those sources is fenced before it
  reaches a model, and a tool call the model then proposes is authorised by the permission
  model alone — never by something the content asked for. This is the largest attack surface
  the browser window introduces; a regression here is a security bug, not a UX one.
- **The `zvs-studio://` scheme is an untrusted, navigation-only entry point** (TASK_036_EXTRA_6).
  Any web page, document or program can open it, so it is treated like any other untrusted
  input. The shell reads only the first `zvs-studio://` argument of a launch (cold start) or of
  a second instance (warm start), drops it when it is longer than 256 characters, and parses it
  in one place, `NotificationTargetDto::from_url`, which accepts `conversation/<uuid>` and
  `downloads` and nothing else. A rejected link is logged and otherwise ignored. An accepted
  one may only show the window and open the chat route with a conversation selected, or
  «Загрузки». A conversation that does not exist opens the chat with no selection and no
  error. The scheme never approves, runs, sends, deletes or changes anything, and it never
  carries text that gets rendered. A new route needs its own task and its own note here.

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
- **Notifications** — native OS notifications are raised by the host, never by the renderer,
  so they still fire while the window is hidden in the tray. One `NotificationService` holds
  the policy (the `notifications.*` settings and the 5 s coalescing). It watches every run
  through `RunService::watch` and reacts to a chat run's approval or end, and to a download
  run's end. A notification carries a title and a display name only: no message text, no file
  contents, no paths. The shell shows it through the `ShellPort`. On Windows, that is
  `Windows.UI.Notifications` directly, with the AUMID registered per user, a `zvs-studio://`
  URL in `launch`, and the subject id and kind as tag and group. A click shows the window and
  emits a `navigation` step that opens the conversation or «Загрузки». The toast activates
  through the `zvs-studio://` protocol, so Action Center clicks and clicks after quitting reach
  the app too. Until the renderer takes it through `notifications.pending`, the service holds
  one target and a later activation replaces it; repeats of one target within 2 s are
  dropped. The URL is parsed by `NotificationTargetDto::from_url` alone, and it only navigates
  (§14).
- **Background workers** (`zvs-host/src/workers/`, TASK_048). `WorkerSupervisor` is the one
  way the host runs a long-lived loop: long polling, IDLE, interval polling. `spawn(spec, body)`
  starts a named tokio task (`key` is `^[a-z][a-z0-9.:_-]*$`). The body gets a `WorkerContext`
  with the cancellation token, `ready()` (status `running`), `heartbeat()`, `detail()` and a
  cancellable `sleep()`. Each attempt runs in its own task, so a panic counts as a failure,
  not a crash. A failure is retried with exponential backoff (`Backoff`: 1 s doubling to
  5 min, reset once the worker has been `running` for 30 s). `WorkerFault::permanent` stops it as
  `failed`. `stop`/`remove` cancel and wait; a body that ignores cancellation is aborted after
  5 s. `spawn` on an existing key replaces that worker. `watch` observes every state change.
  `workers.list` and `workers.restart` expose the state, and the composition root disposes
  every worker at the `Stop` shutdown stage. There is no inbound HTTP (D12, ADR 0004).
- **Integrations** (`zvs-host/src/integrations/`, TASK_048). An `IntegrationProvider` per
  `IntegrationKind` (`mail | telegram | git`) provides `describe()` (a `FieldDescriptor`
  list), `triggers()`, `tools()`, `test()`, `start()`, `stop()` and `call_tool()`. Accounts
  live in `integration_account` with a non-secret config map. **A credential is a secret
  reference by construction:** a config value is a `{ secretId }` reference or a plain
  text/number/boolean. A `secret` field accepts only a reference, and any other value is
  refused with `CREDENTIAL_NOT_A_SECRET`. `FieldKind` has no `password`. A provider is refused
  at registration when a non-secret field key looks like a credential (`password`, `token`,
  `secret`, `apiKey`, `privateKey`, `credential`). The plaintext is resolved only through
  `IntegrationConfigView::secret` at point of use, and secret usage is recorded as
  `integration` consumers. An enabled account runs as the worker `integration:<id>`. Its
  `start` emits `IntegrationEvent { trigger, eventId, payload }`. The dispatcher records
  `(account, trigger, eventId)` in `integration_event` (kept 30 days) before anything starts, so
  a replayed event is counted as a duplicate and starts nothing. A new event fires
  `trigger.integration` scenarios whose `accountId` and `trigger` params match, with
  `{ accountId, kind, trigger, eventId, payload }` as the run input. Tools register per
  account as `<head>.<last 8 of the account id>.<rest>` (`telegram.a1b2c3d4.send`), with source
  `integration` and the account as `sourceRef`. A side-effecting tool defaults to `ask` and
  stays under the tool ceiling (§14). Disable or remove withdraws them. Counters, `last_error`
  and the last 200 activity lines per account feed the automation rail. Channels:
  `integrations.kinds|list|get|create|update|remove|test|enable|disable|activity`.
- **Tray and autostart** — with `app.tray` on, closing the studio window hides it. Runs,
  streams and downloads keep going, and only «Выход» in the tray menu goes through the shutdown
  coordinator. `app.autostart` is mirrored from the OS registration, which is the source of
  truth and is re-read at startup. A launch by autostart with the tray on starts with the window
  hidden.

---

## 16. Build and packaging

- `pnpm build` runs the Vite web build and `tauri build --no-bundle`. `pnpm package` builds
  the sidecar in release, builds the web assets, and runs `tauri build` to produce the
  per-user NSIS installer; the sidecar ships as a bundle resource that lands next to the
  executable. Release builds on Windows link the C runtime statically (`crt-static`, set by
  `scripts/tauri.mjs`), so the installer needs no Visual C++ Redistributable.
- One executable holds the shell, the host, `zvs-core` and SQLite. The other artefacts are
  the `zvs-jobd` sidecar, placed next to the executable, and the `aistudio` console client in
  its own `cli\` folder, so that putting it on the user `Path` exposes nothing else (§7). No
  native module is rebuilt and no Node runtime ships.
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
8. **Revised (ADR 0003, 2026-10-23):** there is no office converter. LibreOffice integration
   (TASK_043) was removed after the managed copy crashed on almost every conversion; DOCX,
   XLSX and PPTX render in the renderer (docx-preview, an OOXML sheet reader over jszip,
   slide text) and are marked «приближённое отображение».
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

Steps 1–9 were built on Electron with a TypeScript host. Revision 2.0 inserted the **migration
order** before the remaining steps, and it finished on 2026-10-19. Each step ported a slice and
left the Tauri app answering more channels:
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
| DOCX · XLSX · PPTX | `kind: "office"` bytes rendered in the renderer (ADR 0003): DOCX with `docx-preview`, XLSX with the OOXML reader in `viewer/ooxml.ts` into the virtualised grid, PPTX as slide text |

Nothing is converted. Every kind is cached as a copy of the source at
`<app data>/cache/documents/<hash>.<ext>`, keyed by its content hash — so reopening is instant,
one file at two paths is one document, and the cache is safe to delete at any time. Eviction is
LRU against a size ceiling set in Settings.

### Dock

`renderer/viewer/ViewerDock` sits in the chat workspace in the slot beside the thread (where the
explorer's file view sits), keeps the thread at least 420 px wide, covers the chat below
1250 px with «Назад к диалогу», and expands to a fixed overlay over the window. Header: page
navigation (input, `PageUp`/`PageDown`, `Alt+←/→`), zoom presets and «По ширине», search
(`Ctrl+F`, CSS Custom Highlight API, no DOM mutation), download (`documents.save`, the OS save
dialog), reveal, expand, close. PDFs render one canvas per visible page at the device pixel
ratio with pdf.js's text layer, and only the pages in view plus one either side exist. The
explorer opens PDF, image, office and CSV files here; markdown and text keep its own view. A
chat artifact opens here through `{ source: "artifact" }` or in the OS default application
through `artifacts.open` («Открыть в Word», the name from `artifacts.application`).

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

### Document service (`zvs-host/src/documents/`, TASK_042)

- **Registry.** The `document` table holds one row per content hash (`id`, `content_hash`
  unique, `source_path` of the last open, `kind`, `mime_type`, `page_count`, `part_count`,
  `size_bytes`, `cached_file`, `opened_at`), so one file at two paths is one document. The hash
  is the `zvs-core` blake3 hash reached through `TextCore::hash`, the same one indexing records.
- **Open.** `documents.open` takes a `DocumentRef`: `path` (absolute, canonicalised, refused
  under a forbidden folder, §14), `citation` or `storeDocument`. A citation and a store document
  resolve through the `vector_document` row; the path a citation carries is never trusted. A
  citation's `page` comes back as `initialPage`.
- **Kind detection** checks the extension against the content signature (`%PDF-`, PNG, JPEG,
  GIF, WEBP, BMP, the OOXML package parts, text without control bytes). A mismatch fails with
  `UNSUPPORTED_FORMAT` (`reason: "mislabelled"`) and never renders. An unknown extension is
  read by its content. Native kinds: `pdf`, `image`, `markdown`, `code` (with a shiki
  `language`), `csv`, `text`. DOCX, XLSX and PPTX are `kind: "office"` with `officeFormat`, served as their
  original bytes; `officeFormat` is present exactly when the kind is `office`.
- **Cache.** `<cache>/documents/<hash>.<ext>` holds a copy of the file, so a moved or deleted
  source keeps serving and reopening costs one hash. Only hash-named files belong to the cache,
  so the indexing scratch folders beside them are ignored. Eviction is LRU by `opened_at`
  against a ceiling (2 GiB until Settings owns it, TASK_052), files without a row go first,
  and a document with an open handle is never evicted (handles are counted per open). A
  missing cache file is rebuilt from the source if its hash still matches.
- **Pages.** `partCount` is the number of `documents.page` parts. PDFs and images are served
  as base64 byte ranges of 4 MiB (`content: "bytes"`) for `pdf.js`. Text kinds are decoded
  (UTF-8, UTF-16 with a BOM, else Windows-1251), CRLF normalised, and split on lines into
  parts of at most 256 KiB, each with `firstLine` and `encoding`. `pageCount` for a PDF
  estimates page objects without parsing (the host never parses PDFs, §10); `pdf.js` is
  authoritative. Text over 32 MiB and files over 512 MiB are refused with `LIMIT_EXCEEDED`.
- **Untrusted text.** Every text part carries `untrusted: true` (a literal in both schemas),
  so the viewer and any model call fence it (§14).

### IPC

`documents.open`, `documents.page`, `documents.close`, `documents.list` (the registry,
newest first, each with `open`), `documents.reveal` (shows the source file in the OS file
manager), `documents.save` (copies the cached file through the save dialog). `documents.open`
also takes `{ source: "artifact", artifactId }`, resolved through the artifact store, which is
how a file a skill produced in app data opens despite the forbidden-folder rule.
`artifacts.open` and `artifacts.application` hand an artifact to the OS default application
and name it (`AssocQueryStringW` on Windows).

### Selections in a turn

`ChatSendInput.selections` (at most 8 `DocumentSelectionDto`, 8 000 characters each) is stored
on the user message (`message.selections`, migration `0010`). Every time the message reaches a
model — the turn itself and the history of later turns — `chat_history::with_selections`
appends each selection as a `<document_excerpt name="…" page="…">` block under an instruction
that the excerpts are untrusted; an excerpt cannot close its own fence.

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
├─ chrome webview (browser-ui)      tabs · toolbar · Sites & cookies · Network · agent panel · approval bar
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
and in Settings. A chat reaches the `browser.*` tools only through the `/browser-use` command
(§14, *Chat tool set*). The tiers compose with the global tool ceiling (§14): the browser can never
exceed what Tools allows, only narrow it.

> Implementation (TASK_046, 2026-10-26). `zvs-host/src/browser/`: `capability.rs` holds the
> table and `resolve_tier`, the one place a tier is decided; submit and download resolve to
> `ask` without looking at grants. `grants.rs` stores `browser_grant` rows (origin ×
> `browser.click` | `browser.type`, enforced by a `CHECK`); granting any other capability is
> refused. A node's `permission_for` passes the resolved tier as the minimum to
> `PermissionService::admit_subject`, so the Tools page ceiling, `off` and a disabled tool still
> win; on the Tools page navigate, click and type default to `auto` and submit and download to
> `ask`. Every browser node defers approval and raises it itself, with scope `site:<origin>`
> and, for an element, the target's role and name (TASK_047 extended this from the four acting
> nodes so that every approval reaches the agent panel). The model sees elements only as opaque handles (`e12`) issued from
> the accessibility tree and kept per tab with their origin; the driver maps a handle to a
> backend node id and refuses it when the page's origin has changed or the node is gone.
> `credentials.rs` decides a credential or payment field from the input type, autocomplete
> tokens and the words in its name, id, placeholder, aria-label and labels; the driver checks
> it before focusing and again after. A click is refused when the element, or whatever is
> under its centre point, submits a form, or when another element covers it. Downloads are
> cancelled by the profile unless `browser.download` armed one for that tab; the file goes to
> the user's Downloads folder. The driver (`src-tauri/src/browser/automation.rs`) enables
> `Page`, `DOM` and `Accessibility` on a tab's first action and drops the session with the tab.
> It runs only four fixed functions in an isolated world (element facts, hit test, focus check,
> form submit) and never sends `Runtime.evaluate`.

### Network recording

The chrome's developer tools have one panel, Network. Recording is per tab: the host enables
the CDP `Network` domain on that site webview and folds its events into a capped log
(`zvs-host/src/browser/network.rs`, 1000 entries, 64 KB of request body, 1 MB of response
body on demand). The values of `Cookie`, `Set-Cookie`, `Authorization`,
`Proxy-Authorization` and `X-CSRF-Token` never leave the host; those headers arrive marked
`redacted`, and are left out of a saved recording. A recording saves as HAR 1.2 through
the system save dialog. Agents reach the same recorder through the `NetworkCapture` port
(`start` · `stop` · `clear` · `log` · `detail` · `har`, plus `record_for` for a timed window),
which `BrowserWorkspace` implements; no kernel node uses it yet.

### Making actions visible

The target element is outlined in the page and labelled with the action about to be taken.
Every action lands in the panel log with its authorisation — "auto", "granted for this site",
"approved 20:58". **Take control** suspends the agent and hands the user the mouse; **Stop**
kills the run. A rate limiter caps actions per minute and trips a pause when exceeded, so a
runaway loop stops itself.

> Implementation (TASK_047, 2026-10-27). `zvs-host/src/browser/agent.rs` (`BrowserAgent`) is
> the session: every `browser.*` step passes `begin` before it acts, which waits while the user
> has control or the rate limiter has paused the session, and records the action. The session
> is the run that made the last browser action; its task is the run's title. Each logged action
> carries `auto`, `granted` (with the grant's time) or `approved` (with the decision's time),
> set by the node from the same admission result that let it run. The limit is the setting
> `browser.agent.actionsPerMinute` (default 20) over a sliding minute; exceeding it pauses the
> session until **Продолжить**, which also clears the window. Control and a rate pause carry
> over to the next run, so a new chat turn cannot slip past them. **Остановить** cancels the run
> through `RunService::cancel`. The chrome reaches the agent only through `browser_command`
> (`agent`, `agentApprove`, `agentAlways`, `agentDeny`, `agentControl`, `agentResume`,
> `agentStop`, `agentRevoke`); the host acts only on the approval the agent is waiting for, and
> `agentAlways` writes a `browser_grant` only for click and type. The outline is CDP
> `Overlay.highlightNode`, drawn by the browser outside the page's DOM, set before the approval
> and cleared after the action; the label is in the chrome's panel. The 344 px panel and the
> 76 px approval bar are regions the host keeps the site webview clear of
> (`AGENT_PANEL_WIDTH`, `APPROVAL_HEIGHT`). The panel log is the agent's own record of the
> browser steps, kept in memory (200 entries); the steps themselves stay in the run's event
> stream.

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
