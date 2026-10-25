# ZVS AI Studio — Task Index

The ordered backlog from empty repo to a shippable v1. **One task = one coding session.**
Every task file is self-contained: a fresh agent with no memory of this conversation should be
able to open one file and work, so tasks repeat context rather than referencing chat history.

## How to run a task

1. Open `TASK_{N}.md`. Read **Context** and **Why** before touching code.
2. Check **Depends on** — if those tasks are not `done` in this index, stop and say so.
3. If the task contains a **❓ DECISION** block, ask the user that exact question and wait for
   an answer before implementing that part. Do not guess and do not silently pick a default.
4. Do the work. Follow `/docs/PASSPORT.md` — it is binding; if the task and the passport
   disagree, the passport wins and you should flag the contradiction.
5. Run the task's **Verify** commands. All must pass.
6. Fill in the **Handoff** block at the bottom of the task file: what you actually built, any
   deviation from the plan, anything the next session needs to know.
7. Set `status: done` in the task header and tick it here in the index.
8. Commit. One commit per task, message `TASK_{N}: <title>`.

## Rules that apply to every task

- **Gates:** `pnpm typecheck && pnpm lint && pnpm build` must pass before a task is done.
  Any task that touches Rust (from Phase M on, that is every host change) also runs
  `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
- **Tests are required** for services, the kernel, repositories, the permission model, DTO
  schemas and all Rust core logic. **Not required** for presentational components.
- **Never invent versions.** Resolve the latest stable at install time and pin it.
- **Never widen scope.** If you find something out of scope, note it in Handoff; do not fix it.
- **Never commit secrets, tokens or `.env` files.**

## Where we are (2026-10-20)

The studio is a **Tauri 2 app with a Rust host** (PASSPORT D10). Phase M (TASK_058 – TASK_076)
is complete: Electron, the TS host, the napi addon, Drizzle and `better-sqlite3` are gone. The
backlog resumes at TASK_034_EXTRA_2. Open tasks written before the migration carry a
post-migration note that maps their TS host paths to the Rust ones (PASSPORT §5).

## Status legend

`todo` · `in-progress` · `done` · `blocked`

## Task numbering

`TASK_{N}` is the backlog order. `TASK_{N}_EXTRA_{i}` extends a task that already exists —
same phase, same architecture, work that was not in the original plan. An extra depends on its
base task and says so in `depends_on`; it never renumbers anything.

---

## Phase 0 — Foundation (deep detail)

Nothing runs yet. This phase produces an empty but correct app: the boundaries exist, the
gates are enforced, and one round trip works end to end.

- [x] **TASK_001** — Workspace skeleton, tooling and CI
- [x] **TASK_002** — Electron shell: host, preload, renderer, window, platform services
- [x] **TASK_003** — `packages/shared`: DTO conventions, branded ids, error codes
- [x] **TASK_004** — IPC contract registry + typed client/server + `system.ping`
- [x] **TASK_005** — Multiplexed event channel + renderer EventRouter + recorder
- [x] **TASK_006** — Data layer: Drizzle, SQLite, migration runner, repository base
- [x] **TASK_007** — Renderer shell: tokens, uikit, router, root store, nav rail, 13 empty routes
- [x] **TASK_008** — Test harness: Vitest, Playwright-Electron smoke, CI gates

## Phase 1 — First vertical slice: Secrets (deep detail)

The smallest complete feature, top to bottom. It proves the whole architecture and unblocks
everything that needs a credential.

- [x] **TASK_009** — Secret schema, repository and first migration
- [x] **TASK_010** — Crypto service (`safeStorage`) and SecretService
- [x] **TASK_011** — Secret DTOs, IPC channels and the secret-type schema registry
- [x] **TASK_012** — Secrets page: list, schema-driven form, scope tabs

## Phase 2 — Providers and the AI layer (deep detail)

Two ways to reach a vendor, decided 2026-09-10 and described in PASSPORT §12: an **API key**
(`auth_mode: 'api'`) or a **linked account** (`auth_mode: 'account'`) whose requests ride the
integrated browser's `persist:browser-work` session. The `_EXTRA_` tasks below are the account
half; run each base task before its extras.

- [x] **TASK_013** — Provider and model schema, repositories, migration
- [x] **TASK_014** — AI driver ports, transports and the OpenAI-compatible adapter
- [x] **TASK_014_EXTRA_1** — Account auth mode: schema, `AccountTransport`, identity probes
- [x] **TASK_014_EXTRA_2** — Qwen and DeepSeek adapter families: model listing
- [x] **TASK_015** — Test-connection and model-discovery service
- [x] **TASK_015_EXTRA_1** — Account linking service, login flow and `accounts.*` channels
- [x] **TASK_014_EXTRA_3** — Account-mode generation and streaming (Qwen only; DeepSeek deferred)
- [x] **TASK_016** — AI Providers page: three tabs, one form, model cards
- [x] **TASK_016_EXTRA_1** — Accounts tab: login, linked accounts, session health

Suggested order: 014 → 014_EXTRA_1 → 014_EXTRA_2 → 015 → 015_EXTRA_1 → 014_EXTRA_3 → 016 →
016_EXTRA_1. TASK_014_EXTRA_3 sits after linking because it needs a live session to develop
against; 016 can be started earlier if you are prepared to render the Account option disabled.

## Phase 3 — Rust core and vector search

- [x] **TASK_017** — Cargo workspace, `zvs-core` skeleton, Rust CI
- [x] **TASK_018** — napi-rs addon: build, load, panic guard, first async call
- [x] **TASK_019** — LanceDB in `zvs-core`: create, upsert, search
- [x] **TASK_020** — Vector store schema, repository, service
- [x] **TASK_021** — Vector Stores page: list, detail, test search
- [x] **TASK_021_EXTRA_1** — Graph relationship store in `zvs-core` — embedded graph database
- [x] **TASK_021_EXTRA_2** — Expose graph store through the napi addon
- [x] **TASK_021_EXTRA_3** — Graph relation catalog, repository and service
- [x] **TASK_021_EXTRA_4** — Relations tab on the Vector Stores page + graph-augmented test search
- [x] **TASK_021_EXTRA_5** — Retrieval evaluation set and the rerank stage *(after TASK_024_EXTRA_1)*
- [x] **TASK_021_EXTRA_6** — Hybrid search, neighbour expansion and graph-augmented chat retrieval *(after TASK_028_EXTRA_6)*

## Phase 4 — Execution kernel and Chat

- [x] **TASK_022** — Kernel core: Run, Step, scheduler, cancellation, checkpointing
- [x] **TASK_023** — Node registry, permission admission, approval gates
- [x] **TASK_024** — Chat as the first kernel front-end, with token streaming
- [x] **TASK_025** — Chat page: thread, composer, retrieval citations
- [x] **TASK_026** — Tasks page (live) and Runs & Logs (history)
- [x] **TASK_024_EXTRA_1** — Grounded chat retrieval: honest token budget, relevance floor, merged sources

## Phase 5 — Sidecar and long-running work

- [x] **TASK_027** — `zvs-jobd` sidecar: protocol, lifecycle, health, cancel
- [x] **TASK_028** — Indexing job: chunk, embed, upsert, with progress
- [x] **TASK_029** — Downloads: catalogue, queue, resume, disk accounting
- [x] **TASK_030** — Downloads page
- [x] **TASK_028_EXTRA_1** — Local embedding runtime: engine downloads, GPU plan, indexing metrics
- [x] **TASK_028_EXTRA_2** — PDF and Office extraction, and OCR through the local vision model
- [x] **TASK_028_EXTRA_3** — Smaller OCR model, removable engines, engines in the catalogue
- [x] **TASK_028_EXTRA_4** — Indexing ETA and documents that appear as they land
- [x] **TASK_028_EXTRA_5** — Reset a vector store, and Windows long-path support
- [x] **TASK_028_EXTRA_6** — Index schema v2: structure-aware chunks, page and section metadata, embedding prefixes *(after TASK_021_EXTRA_5)*

## Phase 6 — Tools, MCP and Skills

- [x] **TASK_031** — Tool registry and the permission ceiling
- [x] **TASK_032** — MCP client driver: servers, discovery, invocation
- [x] **TASK_033** — Connections page
- [x] **TASK_034** — Tools page
- [x] **TASK_034_EXTRA_1** — Model tool calling in chat: the call loop, adapters and approvals
- [x] **TASK_034_EXTRA_2** — Agentic knowledge retrieval: the model searches the knowledge base itself
- [x] **TASK_035** — Skills engine: definition, loading, trigger matching
- [x] **TASK_035_EXTRA_1** — Toolchain: uv and Node as host runtimes for MCP and skill scripts, the «Окружение» tab
- [x] **TASK_035_EXTRA_2** — Skill scripts: running `scripts/` through the toolchain under its own permission
- [x] **TASK_036** — Skills page
- [x] **TASK_036_EXTRA_1** — Working folder and folder access: chat binding, access requests, the «Разрешения» page
- [x] **TASK_036_EXTRA_2** — File tools: `fs.list`, `fs.read`, `fs.search` and the `fs.apply` batch
- [x] **TASK_036_EXTRA_3** — Code and commands: a shared process runner, `code.run` and `shell.run`
- [x] **TASK_036_EXTRA_4** — Working folder explorer: the chat side panel toggle, a live file tree and the file view
- [x] **TASK_036_EXTRA_5** — Preferences «Общие» and «Уведомления»: start with the system, the tray, OS notifications
- [x] **TASK_036_EXTRA_6** — Notification activation through `zvs-studio://`: Action Center clicks and cold start

## Phase M — Migration to Tauri (done)

Decided 2026-10-10 (PASSPORT D10): Tauri 2 replaces Electron, and the **whole host is
rewritten in Rust**, with no Node runtime in the shipped app. Account-mode providers send
requests from **inside the browser profile's webview**, so cookies never leave it. There is **no
data migration**: the Tauri build starts from an empty data directory. The Tauri shell grows
beside Electron, one domain per task, and each port reproduces the existing behaviour with
the TS implementation and its tests as the specification. TASK_075 proves parity end to end,
and TASK_076 deletes Electron.

- [x] **TASK_058** — Tauri shell beside Electron: app crate, Vite entries, renderer bridge, capabilities
- [x] **TASK_059** — `zvs-contract`: Rust mirror of the DTOs, error envelope, events, golden fixtures
- [x] **TASK_060** — `zvs-host` skeleton: paths, logger, ids, clock, event bus, Rust IPC server
- [x] **TASK_061** — Rust data layer: rusqlite, migration runner, baseline schema, settings, recovery window
- [x] **TASK_062** — Secrets in Rust: DPAPI backend, SecretService, `secrets.*`
- [x] **TASK_063** — AI drivers in Rust: HTTP client, ApiTransport, SSE, API-mode adapters, ProviderRegistry
- [x] **TASK_064** — Providers in Rust: ProviderService, probes, health checks, `providers.*`
- [x] **TASK_065** — Browser workspace on Tauri: child webviews, persistent profile, tabs, Sites & cookies
- [x] **TASK_066** — Account gateway: vendor requests inside the browser profile, CDP plumbing, bearer capture
- [x] **TASK_067** — Accounts in Rust: AccountTransport, identity probes, web adapters, AccountService
- [x] **TASK_068** — `zvs-core` in-process, sidecar driver, device and resource probes, `system.*`
- [x] **TASK_069** — Vector stores and graph in Rust: services, retrieval pipeline, evaluation
- [x] **TASK_070** — Execution kernel in Rust: Scheduler, registry, permissions, approvals, runs
- [x] **TASK_071** — Downloads and local runtimes in Rust: catalogue, queue, archives, llama.cpp engine
- [x] **TASK_072** — Indexing in Rust: walk, extraction, OOXML, PDF and OCR, IndexingService
- [x] **TASK_073** — Tools and MCP in Rust: registry, invoker, MCP client, McpService
- [x] **TASK_074** — Chat in Rust: ChatService, budget, grounded retrieval, the tool loop
- [x] **TASK_074_EXTRA_1** — Account providers in chat; DeepSeek web removed (ADR 0001)
- [x] **TASK_075** — e2e on Tauri: Playwright over WebView2 CDP, parity sign-off
- [x] **TASK_076** — Cutover: delete Electron, the TS host, napi, Drizzle and better-sqlite3

The order is a strict chain. Each task leaves the Tauri app answering more channels, and the
Electron app untouched. TASK_066's CDP client is reused by TASK_046.

## Phase 7 — Scenarios and Agentic Build

- [x] **TASK_037** — Scenario model, versioning and validation
- [x] **TASK_038** — Scenario canvas editor
- [x] **TASK_039** — Scenario execution, node inspector, run log
- [x] **TASK_039_EXTRA_1** — Memory engine: account and dialog memory, prompt injection, the «Память» preferences section
- [x] **TASK_040** — Agentic session engine: planning loop and self-extending runs
- [x] **TASK_041** — Agentic Build page

## Phase 8 — Documents and the agent browser

- [x] **TASK_042** — Document service: open, hash cache, PDF path
- [x] **TASK_043** — Office → PDF conversion in the sidecar
- [x] **TASK_044** — Document viewer dock and selection → chat
- [x] **TASK_044_EXTRA_1** — Questions to the user: the `chat.ask` bridge, a question gate in the kernel, the answer card
- [x] **TASK_045** — Attached browser workspace, WebContentsView, session partition
- [x] **TASK_045_EXTRA_1** — Tool surface consolidation: fewer, non-overlapping model tools and no leaked kernel jobs
- [x] **TASK_045_EXTRA_2** — Tool dialects in the web adapters: the injected protocol leaves the service layer
- [ ] **TASK_045_EXTRA_3** — Chat turn reattach: a reopened chat shows the live turn, its approvals and its questions
- [ ] **TASK_045_EXTRA_4** — Skill read-first files: required references arrive with `skill.load`, tool names match the wire *(after TASK_045_EXTRA_1)*
- [ ] **TASK_045_EXTRA_5** — Storage: lean run events, one folder per conversation, disk usage by category *(after TASK_045_EXTRA_3)*

The five `TASK_045_EXTRA_` tasks come from the 2026-10-25 audit of tools, tool-call parsing,
chat restore, skills and storage. They sit under TASK_045 by numbering only and do not depend
on its code. Suggested order: 045_EXTRA_3 → 045_EXTRA_5 (part A) → 045_EXTRA_1 →
045_EXTRA_4 → 045_EXTRA_2.
- [ ] **TASK_046** — CDP driver, `browser.*` nodes, policy engine
- [ ] **TASK_047** — Browser agent panel, approvals, rate limiter

## Phase 9 — Integrations

- [ ] **TASK_048** — Integration framework: accounts, triggers, tool exposure
- [ ] **TASK_049** — Mail integration (IMAP/SMTP)
- [ ] **TASK_050** — Telegram and Git hosting integrations
- [ ] **TASK_051** — Integrations page

## Phase 10 — Production readiness

- [ ] **TASK_052** — Settings page and the configuration surface
- [ ] **TASK_053** — Error handling, recovery screens, local crash capture
- [ ] **TASK_054** — Performance pass: startup, virtualisation, DB indices
- [ ] **TASK_055** — Security review against PASSPORT §14, §21
- [ ] **TASK_056** — Packaging: Tauri bundler, sidecar, WebView2 runtime, installer
- [ ] **TASK_057** — End-to-end suite and the v1.0 release checklist

---

## Dependency graph

```
001 ─┬─ 002 ─┬─ 004 ── 005 ──┬── 007 ── 008
     │       │               │
     └─ 003 ─┘               │
                  006 ───────┘

Phase 1   008 ── 009 ── 010 ── 011 ── 012
Phase 2   012 ── 013 ── 014 ── 015 ── 016
Phase 3   016 ── 017 ── 018 ── 019 ── 020 ── 021 ── 021_EXTRA_1 ── 021_EXTRA_2 ── 021_EXTRA_3 ── 021_EXTRA_4
Phase 4   021 ── 022 ── 023 ── 024 ── 025 ── 026
Phase 5   026 ── 027 ── 028
          027 ── 029 ── 030 ── 028_EXTRA_1 ── 028_EXTRA_2 ── 028_EXTRA_3 ── 028_EXTRA_4 ── 028_EXTRA_5
Phase 6   023 ── 031 ── 032 ── 033
          031 ── 034 ── 034_EXTRA_1 ── 034_EXTRA_2
Retrieval 021_EXTRA_4 + 025 ── 024_EXTRA_1 ── 021_EXTRA_5 ── 028_EXTRA_6 ── 021_EXTRA_6 ── 034_EXTRA_2
          024_EXTRA_1 ── 034_EXTRA_1
          031 ── 035 ── 036
          035 ── 035_EXTRA_1 ── 035_EXTRA_2
Phase 7   023 ── 037 ── 038 ── 039
          039 ── 040 ── 041
Phase 8   026 ── 042 ── 043 ── 044
          031 ── 045 ── 046 ── 047
Phase 9   031 ── 048 ─┬─ 049 ── 051
                      └─ 050
Phase 10  all ── 052 … 057

Phase M   034_EXTRA_1 ── 058 ── 059 ── 060 ── 061 ── 062 ── 063 ── 064 ── 065 ── 066 ── 067
          067 ── 068 ── 069 ── 070 ── 071 ── 072 ── 073 ── 074 ── 075 ── 076
          076 ── every open task (034_EXTRA_2, 035 … 057);  066 ── 046
```

Phases 6, 8 and 9 can run in parallel once Phase 4 is done — they touch different services and
different pages. Everything before that is a chain. Phase M interrupted all of them; with
TASK_076 done, the open tasks are ready to run again.

## Milestones

| Milestone            | After    | What you can do                                      |
| -------------------- | -------- | ---------------------------------------------------- |
| **Skeleton**         | TASK_008 | App boots, routes work, IPC round-trips, DB migrates |
| **First feature**    | TASK_012 | Store and use a real credential                      |
| **Talks to a model** | TASK_016 | Connect a provider, discover models                  |
| **Retrieval works**  | TASK_021 | Index nothing yet, but search a store                |
| **Usable product**   | TASK_026 | Chat with retrieval, watch runs on Tasks             |
| **On Tauri**         | TASK_076 | Same product, Tauri shell, Rust host, no Electron    |
| **Automation**       | TASK_041 | Build and run a scenario, drive an agentic session   |
| **Full surface**     | TASK_051 | All fifteen surfaces from the maket exist            |
| **Shippable**        | TASK_057 | Signed-off installer, e2e green                      |

## Decisions waiting for you

These tasks contain a **❓ DECISION** block — a question the agent must ask before
implementing that part, with a stated fallback if you are unavailable. They are listed here so
you can settle the load-bearing ones early rather than being interrupted mid-task.

| Task | The question, in short | Cheap to change later? |
| --- | --- | --- |
| TASK_001 | GitHub remote now, or local-only? | yes |
| TASK_003 (resolved) | Russian app labels and messages from the start, no `t()` shim; developer communication in English. | moderate |
| TASK_006 (resolved) | Single database in `userData`; no workspace picker. `paths.dbPath()` stays the only call site. | **no** |
| TASK_008 (resolved) | Unit gates run on Linux; the Playwright-Electron e2e smoke runs on Windows, the only verified target. No `xvfb` anywhere. | yes |
| TASK_009 (resolved) | Deleting a secret in use refuses and lists the consumers (`ON DELETE RESTRICT`). Secret types are `ollama`, `openrouter`, `mistral`, `custom`, extensible without a migration. | yes |
| TASK_010 | Can a stored secret ever be revealed back to you? And is machine-bound encryption acceptable, or do you need portable workspaces? | **no** |
| TASK_012 | What does the "Identity & Profile" tab actually hold? | yes |
| TASK_014 | Which AI providers do you want working first? | yes |
| TASK_014_EXTRA_1 (resolved) | Unlinking an account, or clearing that vendor's cookies, deletes the captured bearer token from the secret store immediately. A re-link costs one click and re-mints it. | yes |
| TASK_015 | Background health probing: opt-in, off by default (fallback c). | no |
| TASK_019 | Confirmed: cosine, 1024 dimensions; creation requires explicit dimension | **no** — fixed at store creation |
| TASK_021_EXTRA_1 (resolved) | The relationship graph is `petgraph` in memory inside `zvs-core`, rehydrated from edge rows the host keeps in SQLite (option b). No new persistence engine, no CozoDB; the memory bound is accepted until real usage disproves it. | moderate |
| TASK_022 (resolved) | Auto-resume only runs whose nodes all explicitly declare no side effects; otherwise mark interrupted. | moderate |
| TASK_026 (resolved) | Finished runs keep their summary forever; step payloads and events are dropped after 30 days (`runs.retention.days`). Whole runs are deleted only by «Очистить завершённые», which spares runs a conversation references. | yes |
| TASK_029 (resolved) | A curated catalogue ships with the app — no outbound call when the Downloads page opens. A `refresh` flag on `downloads.catalogue` runs the live sources on demand (option b). | moderate |
| TASK_028_EXTRA_2 (resolved) | PDF parsing runs in Rust in the sidecar because a PDF is the least trustworthy input the app takes; OOXML is read on the host over the existing zip reader. OCR is the Qwen2.5-VL vision model on the local engine, over a scanned page's own embedded image — no rasteriser, no Tesseract. | moderate |
| TASK_028_EXTRA_1 (resolved) | The local inference engine is downloaded, not bundled: llama.cpp release assets resolved on demand, Vulkan recommended for any discrete GPU. Machine metrics live on the Vector Stores page, next to the index they describe. | moderate |
| TASK_024_EXTRA_1 (resolved) | Follow-up questions use the heuristic join (fallback a): a turn under 8 words, or one opening with a pronoun or connective, is searched with the previous user message prepended. Pure acknowledgements are never joined. It sits behind `retrievalQuery`, so a model rewrite can replace it. | yes |
| TASK_028_EXTRA_6 | Schema v2: existing stores read-only until a manual full re-index, or rebuilt automatically? | moderate |
| TASK_032 (resolved) | stdio plus HTTP with static tokens resolved from Secrets (option b). OAuth followed with TASK_034_EXTRA_2 at the user's request: MCP authorization discovery, dynamic client registration, PKCE on a loopback redirect in the system browser, tokens encrypted in `mcp_oauth` and refreshed on expiry or a 401. | moderate |
| Phase M (resolved 2026-10-10) | Full Rust host rewrite, no Node sidecar. Account-mode requests run as `fetch()` inside a hidden webview of the browser profile, driven over CDP, and no cookie is copied out. Fresh start: no import of Electron data or `safeStorage` secrets. | **no** |
| TASK_058 | Bundle identifier, which fixes the data directory (fallback `com.zvs.aistudio`) | moderate |
| TASK_073 (resolved) | The official `rmcp` SDK (pinned `=3.5.0`, client only) speaks the protocol; the stdio transport (Windows `.cmd` launch, isolated toolchain env, kill-tree) and the streamable HTTP client (static headers over the shared `reqwest`) are ours. | yes |
| TASK_034_EXTRA_1 | Which tools a chat turn sees: per-conversation picker, everything not `off`, or built-ins only? | yes |
| TASK_034_EXTRA_2 (resolved) | Fallback (c): `hybrid` is the default for new conversations — a cheap pre-search offers its top 3 sources and the model may search further. A model that cannot call tools gets the pre-search exactly as in `auto`, so it never answers worse than before. `auto` and `agentic` are per-conversation choices in the composer. | yes |
| TASK_035 (resolved) | Neither option as written: each skill is its own folder inside the app's data directory (`SKILL.md` plus `references/`, `scripts/`, `assets/`), and the database holds only the index and the state the app owns. No watcher: a stat fingerprint is compared on read, and a changed folder is re-parsed and re-checked. Import and export copy a folder. | **no** — migrates user content |
| TASK_036_EXTRA_4 (resolved) | Fallback (a): a file opens in a split beside the thread with a draggable divider, covering the chat below 1250 px. | yes |
| TASK_036_EXTRA_5 (resolved) | Native toasts for every enabled event whatever window is in front, as in Electron. Built on `Windows.UI.Notifications` directly (own AUMID, `zvs-studio://` launch URL, tag and group), not a wrapper crate. A click opens its conversation, or «Загрузки». Action Center and cold-start clicks follow in TASK_036_EXTRA_6 through protocol activation. | yes |
| TASK_037 (resolved) | Own format matched to the kernel (option a); no n8n import. A best-effort importer is a candidate later task. | **no** |
| TASK_038 (resolved) | React Flow now, revisit if it fights the design (option c). The graph lives in our own model (`graphModel.ts` reducers, `ScenarioEditor`); React Flow only renders it, so a hand-rolled canvas stays open. | **no** |
| TASK_040 (resolved) | Option (a): 25 steps, 200 000 tokens, 15 minutes; exhaustion pauses the session and `agentic.extend` continues it. The three limits are read from settings (`agentic.budget.maxSteps`, `maxTokens`, `maxMinutes`) for TASK_052 to expose. | yes |
| TASK_043 (superseded by ADR 0003) | LibreOffice integration removed in TASK_044 after the managed copy crashed on almost every conversion. Office documents render in the renderer (docx-preview, an OOXML sheet reader, slide text). «Компоненты» holds Node.js and uv. | moderate |
| TASK_045_EXTRA_1 (resolved) | Option (a), all three: `chat.file` → `fs.*` on `chat:`, `skill.read` → `fs.read` on `skill:<key>/`, `skill.script` → `shell.run` with `argv[0] = skill:<key>/scripts/<file>`. The tier of a `chat:` or `skill:` call is decided in the permission resolver (`AppFolder`). | yes |
| TASK_045_EXTRA_5 | Existing chat folders and artifacts: migrate to `conversations/<id>/…` on first start, new layout for new conversations only, or skip the layout and do events and usage only (fallback)? | **no** — moves user data |
| TASK_049 | Does mail send automatically once approved, or per message? | yes |
| TASK_050 | Telegram: bot account, or your personal account? | **no** |
| TASK_056 | Code-sign the installer? Auto-update or manual? WebView2 runtime: download, embed bootstrapper, or offline? | moderate |

The six marked **no** are worth answering before their phase starts — TASK_006 and TASK_010
before Phase 1, TASK_019 before Phase 3, TASK_035/037/038 before Phases 6 and 7, TASK_050
before Phase 9.

## Reference material

- `/docs/PASSPORT.md` — binding architecture. Read §2 (decisions, D10 for the Tauri move) at
  minimum, plus the sections your task names. §5 has the TS→Rust path map.
- `/design/*.dc.html` — the visual spec, one file per screen. Open the file for the screen you
  are building; it is the source of truth for layout, spacing and states.
- `/design/canvas.json` — which artboard is which.
- `/references/` — the original inspiration screenshots (Claude chat, n8n editor).
