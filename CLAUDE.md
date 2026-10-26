# CLAUDE.md

Guidance for Claude Code working in this repository.

## What this is

ZVS AI Studio: a Tauri desktop app with a Rust host. The renderer is React + MobX +
Tailwind; the host owns all I/O, state and policy.

```
apps/studio/src/renderer   React renderer: features/, ui/, app/, pages/, stores/
apps/studio/src-tauri      the shell: Tauri commands, ports, webviews
crates/zvs-contract        DTOs and AppError, mirrored by packages/shared
crates/zvs-host            services, execution kernel, repositories, drivers
crates/zvs-core            vectors and indexing      crates/zvs-jobd  sidecar
packages/shared            Zod schemas + fixtures     packages/ipc   typed client
docs/PASSPORT.md           binding architecture reference
e2e/, test/helpers/        Playwright suite and shared test fakes
```

Read `docs/PASSPORT.md` before changing architecture, the IPC contract or the permission
model — it is binding, not background. `CONTRIBUTING.md` owns the testing policy and the
gates. `CONVENTIONS.md` owns user-facing text and the locale rules.

## Gates

A change is not done until these pass. Run the ones your change can affect, always including
the formatters:

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`pnpm build`, `pnpm test:e2e` and `zvs-studio` clippy need Windows and a built app; CI runs
them. In a Linux container they fail on missing GTK (`gdk-3.0`) and WebView2 — verify the
host crates and the renderer instead, and say in the PR what you could not run rather than
reporting a full pass.

`cargo` needs `protoc` (LanceDB). Node is pinned to 26 and the package manager is pnpm; never
run `npm install` here.

## Locale: read `CONVENTIONS.md` before writing a string

One locale, no `t()` shim (PASSPORT §18.6). **Every label, hint, empty state, confirmation and
user-visible error message is Russian; code identifiers, logs, system prompts and developer
communication are English.** Host `AppError` messages that the renderer shows verbatim — the
`HOST_WORDED` sets, and stores that render `error.message` — are user-facing copy and are
Russian too. Built-in tool descriptions in `kernel_tools.rs` are Russian: they appear on the
«Инструменты» page and go to the model.

Tone is neutral and formal. No `мы` about what the app is doing (`Проверка…`, not
`Проверяем…`), no anthropomorphised model (`Обработка запроса`, not `Думает`), no
reassurance, no implementation trivia, no notes about what is still unbuilt, no exclamation
marks or emoji. Counts go through `plural()`; numbers through `toLocaleString("ru-RU")`.
Terminology is fixed — «диалог», «хранилище», «фрагмент», «инструмент», «коннектор».

Assert against the exported string constant, never against a substring of Russian prose: a
test that greps for `"active turn"` breaks the moment the message is translated.

## Working here

- Keep copy beside what renders it, or in the feature's `*Presentation.ts` / `*Copy.ts` when
  two components share it.
- The renderer maps `AppErrorCode` to copy and never parses error strings (PASSPORT §7).
- Every DTO change touches three places: the Rust type, the Zod schema and a golden fixture
  in `packages/shared/fixtures/`. `cargo test -p zvs-contract` requires byte-equal JSON.
- Read `crates/zvs-qwen-web/README.md` before any work with the Qwen web API (the crate, the
  `qwen_web` adapter, its fixtures). Every change to `zvs-qwen-web` updates that README in the
  same commit, with the new observation date.
- Stores take clock and bridge dependencies so they can be tested — use `createFakeBridge()`
  and `createFakeClock()` from `test/helpers/`, not real time or a real Tauri runtime.
- Tests are mandatory for services, the kernel, repositories, the permission model and the
  contract; presentational markup is covered once by the e2e smoke test. Test the logic a
  surface carries, not its markup.
- Do not widen scope. Note what you find and leave it; `mcp__ccd_session__spawn_task` or a
  line in the PR is the right place for it.
- Never invent a version — resolve the latest stable and pin it exactly. Never commit
  secrets, tokens or `.env` files.
- One commit per task, `TASK_{N}: <title>`, when working a numbered backlog task; otherwise a
  conventional prefix (`fix:`, `feat:`, `l10n:`) and a body that says why.
