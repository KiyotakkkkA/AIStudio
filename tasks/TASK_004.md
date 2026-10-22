---
task: TASK_004
title: IPC contract registry, typed client/server, system.ping
phase: 0 — Foundation
status: done
depends_on: [TASK_002, TASK_003]
unblocks: [TASK_005, TASK_006]
estimate: 4 h
---

# TASK_004 — IPC contract registry, typed client/server, `system.ping`

## Context

The app has a window (TASK_002) and a shared DTO package (TASK_003), but the two processes
cannot talk. This task builds the mechanism they will use for every request/response for the
rest of the project.

The architecture deliberately rejects tRPC and Comlink-style proxies in favour of a
hand-rolled **contract registry** (PASSPORT decision D3). The reasoning: full control over
~300 lines beats an adapter dependency in a long-lived desktop app, and the payloads stay
plain cloneable objects, which is exactly what MobX needs on the renderer side. Proxy-based
RPC fights structured clone; that was the failure mode we are designing away from.

The shape is one object in `packages/shared` mapping channel names to input and output Zod
schemas. From that single object, two things are derived: a typed client for the renderer
(`await ipc.call('system.ping', input)` fully inferred both ways) and a typed registrar for
the host that cannot register a handler with the wrong signature — and, importantly, cannot
forget one, because exhaustiveness is checked at compile time.

This task proves the whole boundary with one trivial channel, `system.ping`. That channel
stays in the codebase forever as the smoke test.

## Why

Everything after this is a channel. Getting validation, error mapping and type inference right
once means every later feature is a two-line addition to the contract plus a handler.

## Depends on

- `TASK_002` — preload file exists and is wired into the window.
- `TASK_003` — `Result`, `AppErrorCode`, DTO conventions.

## Read first

- `/docs/PASSPORT.md` §7 (IPC — contract registry, validation policy, error model, preload
  rules). This task is the implementation of that section.
- `/docs/PASSPORT.md` §14 (why the preload surface stays at two functions).

## Scope

**In scope**
- `defineContract` and the contract object in `packages/shared`
- `packages/ipc` with `client` (renderer side) and `server` (host side) runtimes
- Two-sided Zod validation with the documented policy
- Error mapping: handlers never throw across IPC; everything becomes a `Result`
- The preload bridge — `call` only (`subscribe` arrives in TASK_005)
- `system.ping` channel, wired end to end, with tests
- Exhaustiveness check proving every contract channel has a handler

**Out of scope**
- The event channel and streaming (TASK_005)
- Any feature channel — no secrets, providers or runs yet
- Rate limiting or auth on channels (there is one trusted renderer; revisit if that changes)

## Steps

1. In `packages/shared/src/ipc/`, write `defineContract` — a helper that takes a record of
   `{ [channel]: { input: ZodType, output: ZodType } }` and returns it with strong literal
   types preserved (`as const` plus a generic constraint). Export the derived helper types:
   `ChannelName`, `InputOf<C>`, `OutputOf<C>`.
2. Create `contract.ts` with exactly one entry to start:
   `'system.ping': { input: z.object({ sentAt: Timestamp }), output: z.object({ pong: z.literal(true), hostTime: Timestamp, roundTripHint: z.number() }) }`.
   Add a comment block above the object stating the naming rule (`domain.verb`, lowercase,
   dot-separated) and the versioning rule (breaking change → `domain.verb@2`, never an edit).
3. `packages/ipc/src/server.ts` — `createIpcServer(contract, handlers)`:
   - Registers one `ipcMain.handle` per channel.
   - Parses input with the channel's schema. On failure, returns
     `err(VALIDATION_FAILED)` — never throws.
   - Calls the handler, parses its output in development (catch DTO drift early), returns
     `ok(data)`.
   - Catches everything. Maps known error types to their `AppErrorCode`, unknown ones to
     `UNKNOWN`. **Logs the stack host-side, never sends it across.**
   - Type-level exhaustiveness: the `handlers` argument must be `Record<ChannelName, …>`, so
     omitting a channel is a compile error.
4. `packages/ipc/src/client.ts` — `createIpcClient(contract, bridge)`:
   - `call(channel, input)` → validates input, sends, validates the response envelope,
     returns `data` on `ok` and throws a typed `IpcError` carrying `code` on `err`.
   - The renderer catches `IpcError` and maps `code` to copy. It must never parse
     `message` — add that as a comment where the error class is defined.
5. Validation policy, exactly as PASSPORT §7 states: validate on **both** sides. Host-side
   because the renderer is the less-trusted half; renderer-side because DTO drift should
   fail loudly at the boundary rather than corrupt a MobX store. Make the renderer-side
   parse switchable by build flag, defaulting to on, so a later profiling pass has a lever.
6. `apps/studio/src/preload/index.ts` — expose via `contextBridge.exposeInMainWorld('zvs',
   { call })`. **Nothing else.** No `ipcRenderer`, no channel list, no `require`. Add a
   comment stating that this file is a security boundary and additions need review.
7. Type the bridge in `apps/studio/src/renderer/env.d.ts` so the renderer sees `window.zvs`
   without `any`.
8. Implement the `system.ping` handler in `apps/studio/src/host/ipc/system.ts` — return the
   host clock and echo. Register it from the composition root in the gap TASK_002 left.
9. In the placeholder renderer, call `ping` on mount and render the round-trip time. This
   is temporary UI; TASK_007 replaces it, but keep the ping call somewhere reachable.
10. Tests:
    - Unit: server maps a thrown error to the right code and never leaks a stack.
    - Unit: bad input is rejected with `VALIDATION_FAILED` before the handler runs.
    - Unit: the client throws `IpcError` with the right `code` on an `err` envelope.
    - Type test: a contract with a missing handler fails to compile.
11. Commit: `TASK_004: IPC contract registry and system.ping`.

## Verify

```bash
pnpm --filter ipc test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks:
- The window shows a real round-trip time from the host.
- In DevTools, `Object.keys(window.zvs)` returns exactly `['call']`.
- `window.zvs.call('system.ping', { sentAt: 'not-a-number' })` rejects with
  `VALIDATION_FAILED` and the host log shows no stack sent to the renderer.
- `window.zvs.call('nope.nope', {})` fails cleanly rather than hanging.

## Definition of done

- [ ] `system.ping` round-trips with full type inference on both sides
- [ ] Preload exposes exactly one function
- [ ] Invalid input is rejected before reaching the handler
- [ ] A thrown handler error becomes a coded `Result`, stack stays host-side
- [ ] Missing handler is a compile error (proven by a type test)
- [ ] Tests and gates pass
- [ ] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:** `defineContract` plus `ChannelName`/`InputOf`/`OutputOf` and the contract object
(`packages/shared/src/ipc/`), with the naming and versioning rules documented in the package README;
`AppError`/`isAppError` and an exported `ErrorDetails` type in `packages/shared/src/errors/`.
New `@zvs/ipc` package: `createIpcServer` (one `ipcMain.handle` per channel, input parsed
before the handler runs, output parsed while `validateOutput` is on, every failure mapped to
a `Result`, stacks logged host-side only, `dispose()` for shutdown), `createIpcClient`
(input validation, envelope validation switchable, resolves output or throws `IpcError`),
and `IpcError`, whose never-parse-`message` rule is stated in `packages/ipc/README.md`. Preload exposes exactly
`{ call }`; `window.zvs` is typed in
`src/renderer/env.d.ts`. `system.ping` is implemented in `src/host/ipc/system.ts`, assembled
in `src/host/ipc/index.ts` behind the exhaustive `IpcHandlers<Contract>` return type, and
registered in `main.ts` before the window with `validateOutput: !app.isPackaged`; the shutdown
hook disposes it. The placeholder renderer pings on mount and shows the round trip. 14 Vitest
tests plus 3 compile-time assertions in `packages/ipc`, 2 Node tests in `apps/studio`; the
type assertions were confirmed to fail without their directives. All gates plus
`pnpm test` and `pnpm format:check` pass from a wiped `dist`/`out`/turbo-cache tree.
A Playwright run against the production build confirmed the window shows a real round trip,
`Object.keys(window.zvs)` is `['call']`, a bad `sentAt` returns `VALIDATION_FAILED` with no
stack in the host log, `nope.nope` rejects cleanly, and the console is clean.

**Deviated from plan:** The user asked for no code comments, so the explanatory blocks the
task calls for (contract naming and versioning, the preload security boundary, the
never-parse-`message` rule) live in `packages/ipc/README.md` and `packages/shared/README.md`
instead of inline. The renderer imports `@zvs/ipc` only through
`src/renderer/ipc.ts`, which re-exports `isIpcError`, so `main.tsx` stays on relative imports. The contract lives in `packages/shared/src/ipc/contract.ts` rather
than `src/contract.ts` as PASSPORT §7 spells it — the task's step 1 put `defineContract`
under `src/ipc/`, and splitting the pair across two directories read worse; it is exported
from the package root either way. The server takes `ipcMain` through a structural
`IpcMainLike` instead of importing `electron`, so the package stays testable under plain Node
and Vitest. `AppError` went into `@zvs/shared` next to the error codes rather than into
`@zvs/ipc`, so host services can throw a coded error without importing the transport.
Renderer-side validation is a `define`d build flag (`ZVS_VALIDATE_IPC=false` to disable),
but Vite does not apply `define` to renderer sources in dev, so the flag is read through a
`typeof` guard that keeps validation on when it is undefined — dev always validates.
Two gate-ordering fixes were needed once `studio` started importing workspace packages,
which resolve through their built `dist`: `typecheck` and `lint` now depend on `^build` in
`turbo.json`, and `lint:root` no longer walks `apps/**` and `packages/**` (each package
already lints itself), which also breaks the build/lint cycle that would otherwise deadlock
a clean checkout. Added `apps/studio/tsconfig.test.json`; the app's tests were typechecked
by nothing before, and its `typecheck`/`test` scripts now run it.

**Next session should know:** Adding a channel is three edits — an entry in
`packages/shared/src/ipc/contract.ts`, a handler file under `apps/studio/src/host/ipc/`, and
a spread in `createHandlers`; the exhaustive `IpcHandlers<Contract>` return type makes a
forgotten handler a compile error. TASK_005 adds `subscribe` to the preload (the second and
final function) and the `zvs:events` channel; keep the preload CJS and its surface closed.
Error copy is Russian: "Некорректные данные запроса" for `VALIDATION_FAILED`, "Внутренняя
ошибка" for `UNKNOWN`. Zod now ships in the renderer bundle (~743 kB unminified-ish total)
because validation is two-sided; that is the documented trade, revisit only with profiling.
`pnpm build` must run before an editor's ESLint/TS server resolves `@zvs/shared` and
`@zvs/ipc` from `apps/studio`, since resolution goes through `dist`. The temporary
Playwright smoke lives in the ignored `node_modules/.task002/smoke004.mjs`.
