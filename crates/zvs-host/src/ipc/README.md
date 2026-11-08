# ipc

The host's request surface. Every contract channel (`zvs_contract::CHANNELS`, mirrored by
`packages/shared/src/ipc/contract.ts`) has exactly one handler here. The renderer reaches it
through the Tauri `ipc_call` command and the CLI through the pipe allow-list
(`../cli/README.md`); both end in `IpcServer::dispatch`.

## Layout

- `server.rs` — `IpcServer`: registration, dispatch, validation, panic isolation, coverage.
- `mod.rs` — `register(server)`, which calls each domain's `register`, and the test that pins
  the exact channel list.
- One file (or folder, for large domains such as `chat/`, `runs/`, `mcp/`, `vector_stores/`)
  per channel prefix: `accounts`, `agentic`, `artifacts`, `browser_*`, `chat`, `cli`,
  `computer`, `documents`, `downloads`, `exec_policy`, `folders`, `graph_relations`,
  `integrations`, `mcp`, `memory`, `notifications`, `providers`, `runs`, `runtimes`,
  `scenarios`, `secrets`, `settings`, `shells`, `sites`, `skills`, `storage`, `system`,
  `terminal`, `toolchain`, `tools`, `vector_stores`, `voice`, `workspace`.

Handlers are thin: parse, call one service method, return its DTO. Business rules live in
`services` and the domain modules (PASSPORT §5).

## Registration

```rust
server.handle("runs.get", |services: Arc<Services>, input: GetRunInput| async move {
    services.runs.get(&input.id)
});
```

`handle::<I, O, _, _>` takes a channel name and an `async fn(Arc<Services>, I) ->
Result<O, AppError>`. Registering a name that is not in the contract, or the same name twice,
panics at start-up, so a typo cannot ship. `finish()` logs whether every contract channel is
served; `ipc::tests::every_contract_channel_is_served` requires that it is, and
`the_ported_channels_are_registered` pins the full sorted list.

## Dispatch pipeline

1. Look up the handler; none → `NOT_IMPLEMENTED` naming the channel.
2. Deserialize the payload into `I` and run its `Validate` impl. Failure →
   `VALIDATION_FAILED` «Некорректные данные запроса» without details (input is never echoed),
   logged at `warn`.
3. Run the handler inside `catch_unwind`. An `AppError` is returned as-is with its details; a
   panic becomes `UNKNOWN` «Внутренняя ошибка», logged with the panic message, and the server
   keeps serving.
4. In debug builds the output is validated against the contract too; a mismatch is a bug and
   answers `UNKNOWN`.
5. Wrap as `{ ok: true, data }` or `{ ok: false, error }`. The renderer maps
   `AppErrorCode` to copy and never parses messages (PASSPORT §7).

Adding a channel touches the Rust DTO, the Zod schema, a golden fixture
(`packages/shared/fixtures/`), the channel list in both contracts, a handler here and the
pinned list in `mod.rs`; `node scripts/contract-channels.mjs --check` verifies the two
channel lists agree.
