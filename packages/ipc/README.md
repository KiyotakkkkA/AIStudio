# IPC runtimes

The two halves of the process boundary, both derived from the one contract in
`@zvs/shared` (PASSPORT §7). Nothing here knows about a feature: add a channel to the
contract, add a handler, done.

`createIpcServer(contract, handlers, { ipcMain, logger, validateOutput })` registers one
`ipcMain.handle` per channel and returns `{ channels, dispose }`. It parses input before the
handler runs, parses handler output while `validateOutput` is on, and converts every failure
into a `Result` envelope — a handler never throws across IPC. `AppError` keeps its code,
`ZodError` becomes `VALIDATION_FAILED`, everything else becomes `UNKNOWN`; the stack is
logged host-side and never sent.

`handlers` is `IpcHandlers<Contract>`, a mapped type over every channel, so a missing or
mistyped handler is a compile error (`tests/types.test.ts` pins this).

`createIpcClient(contract, bridge, { validateResponses })` wraps the preload's `call`. It
validates input, validates the response envelope by default, resolves with the channel's
output and rejects with `IpcError`. Switch on `IpcError.code`; never parse its `message`.

`ipcMain` is injected through a structural `IpcMainLike`, so the package stays importable
from plain Node and Vitest without Electron.
