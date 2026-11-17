# ZVS AI Studio

Local-first desktop AI studio for Windows: chat with API, local and account-based models,
knowledge stores with hybrid search, tools and MCP servers, file and process tools under an
execution policy, a built-in browser and computer use for the model, scenarios (automations),
plugins (Telegram, mail, GitHub/GitLab), skills, dashboards, voice input, and an `aistudio`
terminal client.

The binding architecture is [docs/PASSPORT.md](docs/PASSPORT.md). Decisions are recorded in
[docs/adr](docs/adr). Testing policy and gates: [CONTRIBUTING.md](CONTRIBUTING.md). Copy and
locale rules: [CONVENTIONS.md](CONVENTIONS.md).

## Architecture

```
┌──────────────── zvs-studio.exe (Tauri 2) ────────────────┐
│  renderer (React + MobX, WebView2)   browser-ui  computer-ui
│        │ ipc_call / events_subscribe  (Tauri commands)    │
│        ▼                                                  │
│  zvs-host: IPC server → services → kernel → repositories  │──► SQLite, LanceDB (zvs-core)
│                              │                            │──► model vendors, MCP, plugins
│                              └──► drivers ────────────────│──► zvs-jobd sidecar (downloads, PDF)
└──────────────────────────────▲───────────────────────────┘
                               │ named pipe (per-user)
                         aistudio CLI (zvs-cli)
```

The whole host is Rust (PASSPORT D10). The renderer only renders: it reaches the host through
`ipc_call` (request/response over the contract in `packages/shared`) and `events_subscribe`
(streams). All I/O, state and policy live in `zvs-host`. Data lives in
`%LOCALAPPDATA%\ZVS\AIStudio` (`AIStudio-Dev` for development builds); the layout is in
[docs/FILES_STRUCTURE.md](docs/FILES_STRUCTURE.md).

## Workspace

| Path                                                     | What it is                                                                                                   |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| [apps/studio/src-tauri](apps/studio/src-tauri/README.md) | `zvs-studio`: Tauri shell, composition root, commands, webviews, OS ports                                    |
| `apps/studio/src`                                        | Renderer (`renderer/`), browser chrome (`browser-ui/`), computer-use overlay (`computer-ui/`), recovery page |
| [crates/zvs-host](crates/zvs-host/README.md)             | The host: IPC server, services, execution kernel, repositories, drivers                                      |
| [crates/zvs-contract](crates/zvs-contract/README.md)     | DTOs, `AppError`, channel list; serde mirror of `packages/shared`                                            |
| [crates/zvs-core](crates/zvs-core/README.md)             | Pure compute: chunking, hashing, embeddings, LanceDB index, graph, PDF, OCR glue                             |
| [crates/zvs-jobd](crates/zvs-jobd/README.md)             | Sidecar for long or crash-prone jobs (downloads, document extraction)                                        |
| [crates/zvs-cli](crates/zvs-cli/README.md)               | `aistudio` terminal client                                                                                   |
| [crates/zvs-pipe](crates/zvs-pipe/README.md)             | Per-user named pipe, console text decoding, clipboard (Windows plumbing)                                     |
| [crates/zvs-qwen-web](crates/zvs-qwen-web/README.md)     | chat.qwen.ai web protocol                                                                                    |
| [crates/zvs-zai-web](crates/zvs-zai-web/README.md)       | chat.z.ai web protocol                                                                                       |
| [crates/zvs-openai](crates/zvs-openai/README.md)         | ChatGPT (Codex) account protocol                                                                             |
| [crates/zvs-copilot](crates/zvs-copilot/README.md)       | GitHub Copilot protocol                                                                                      |
| `packages/shared`                                        | Zod DTOs, the IPC contract, error codes, events, golden fixtures                                             |
| `packages/ipc`                                           | Typed renderer client over the bridge                                                                        |
| `test/helpers/`                                          | Shared fakes (`createFakeBridge`, `createFakeClock`, fake servers)                                           |

`tasks`, `design` and `references` are kept local (see `.gitignore`).

## Module guides

Every Rust module with its own folder has a README with its main types, pipelines and
algorithms. They are documentation only: no crate embeds them in a normal or release build
(`zvs-core` and `zvs-jobd` include theirs only under `cfg(doc)`), and the installer bundles no
Markdown.

**Shell** (`apps/studio/src-tauri/src`):
[browser](apps/studio/src-tauri/src/browser/README.md) ·
[browser/gateway](apps/studio/src-tauri/src/browser/gateway/README.md) ·
[platform](apps/studio/src-tauri/src/platform/README.md)

**CLI** (`crates/zvs-cli/src`):
[state](crates/zvs-cli/src/state/README.md) · [ui](crates/zvs-cli/src/ui/README.md)

**Host** (`crates/zvs-host/src`):

| Area                   | Modules                                                                                                                                                                                                                                                                                                                                                                  |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Core runtime           | [kernel](crates/zvs-host/src/kernel/README.md) · [ipc](crates/zvs-host/src/ipc/README.md) · [data](crates/zvs-host/src/data/README.md) · [platform](crates/zvs-host/src/platform/README.md) · [workers](crates/zvs-host/src/workers/README.md)                                                                                                                           |
| Services               | [services](crates/zvs-host/src/services/README.md) · [services/chat](crates/zvs-host/src/services/chat/README.md)                                                                                                                                                                                                                                                        |
| Models and connections | [drivers](crates/zvs-host/src/drivers/README.md) · [drivers/ai](crates/zvs-host/src/drivers/ai/README.md) · [drivers/mcp](crates/zvs-host/src/drivers/mcp/README.md) · [runtimes](crates/zvs-host/src/runtimes/README.md) · [downloads](crates/zvs-host/src/downloads/README.md)                                                                                         |
| Knowledge              | [indexing](crates/zvs-host/src/indexing/README.md) · [documents](crates/zvs-host/src/documents/README.md) · [images](crates/zvs-host/src/images/README.md)                                                                                                                                                                                                               |
| Tools and safety       | [tools](crates/zvs-host/src/tools/README.md) · [policy](crates/zvs-host/src/policy/README.md) · [workspace](crates/zvs-host/src/workspace/README.md) · [files](crates/zvs-host/src/files/README.md) · [commands](crates/zvs-host/src/commands/README.md) · [background](crates/zvs-host/src/background/README.md) · [toolchain](crates/zvs-host/src/toolchain/README.md) |
| Agents and automation  | [agentic](crates/zvs-host/src/agentic/README.md) · [scenarios](crates/zvs-host/src/scenarios/README.md) · [integrations](crates/zvs-host/src/integrations/README.md) · [skills](crates/zvs-host/src/skills/README.md) · [computer](crates/zvs-host/src/computer/README.md)                                                                                               |
| Outputs                | [artifacts](crates/zvs-host/src/artifacts/README.md) · [dashboards](crates/zvs-host/src/dashboards/README.md) · [chat_files](crates/zvs-host/src/chat_files/README.md)                                                                                                                                                                                                   |
| User surfaces          | [cli](crates/zvs-host/src/cli/README.md) · [terminal](crates/zvs-host/src/terminal/README.md) · [voice](crates/zvs-host/src/voice/README.md) · [notifications](crates/zvs-host/src/notifications/README.md) · [sites](crates/zvs-host/src/sites/README.md)                                                                                                               |

## Requirements

- Windows 10/11 x64 (the only verified target, PASSPORT §18.3) with the WebView2 runtime.
- Node 26 and pnpm 12.3.4 (pinned in `package.json`); dev tooling only, not shipped.
- Rust stable (the version in PASSPORT §3) with the MSVC build tools.
- `protoc` (verified with 36.1), needed by LanceDB. Point `PROTOC` at it; see
  [crates/zvs-core/README.md](crates/zvs-core/README.md).

## Commands

| Command        | What it does                                                                  |
| -------------- | ----------------------------------------------------------------------------- |
| `pnpm dev`     | Builds the workspace packages and the sidecar, then `tauri dev` with Vite HMR |
| `pnpm build`   | Web build plus `tauri build --no-bundle`: `target/release/zvs-studio.exe`     |
| `pnpm package` | `tauri build` with the NSIS installer                                         |

Gates (a change is not done until the ones it can affect pass):

```sh
pnpm install --frozen-lockfile
pnpm typecheck
pnpm lint
pnpm test
pnpm format:check
pnpm exec knip --include files,dependencies,devDependencies,unlisted,binaries,unresolved
node scripts/contract-channels.mjs --check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Image attachments

The composer offers «Загрузить изображение» for Qwen and Z.ai account providers: up to five
PNG, JPEG, GIF or WebP files, at most 20 MiB each; an image can be sent without text.
Attachments stay with the conversation after restart and are copied when branching. Other
providers cannot send a conversation containing image attachments. This is vision input;
image generation is a separate capability. The host pipeline is in
[crates/zvs-host/src/images/README.md](crates/zvs-host/src/images/README.md); provider upload
protocols are in the [Qwen](crates/zvs-qwen-web/README.md) and
[Z.ai](crates/zvs-zai-web/README.md) notes, where live-client verification is still marked
pending.
