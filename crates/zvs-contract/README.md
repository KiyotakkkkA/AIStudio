# zvs-contract

The serde mirror of `packages/shared`: every DTO that crosses `ipc_call` or the event channel
exists here under the **same name** as its Zod schema. The crate is pure data. It depends on
`serde`, `serde_json`, `uuid`, `thiserror` and `zeroize` and never on Tauri, so the IPC server
(TASK_060) and every port task can share it.

What lives here today:

| Module          | Contents                                                                                           |
| --------------- | -------------------------------------------------------------------------------------------------- |
| `ids`           | Branded ids (`ProviderId`, `StreamId`, …) as `#[serde(transparent)]` newtypes                      |
| `primitives`    | `Timestamp`, `Json`, `PageRequest`, `Page<T>`, `Sort<F>`, `SortDirection`                          |
| `errors`        | `AppErrorCode`, `AppError`, `ErrorDetails`                                                         |
| `envelope`      | `ResultEnvelope<T>`, the `{ ok, data }` / `{ ok, error }` wire envelope                            |
| `events`        | `HostEvent`, `HostEventDraft`, `RunOutcomeDto`, `DeltaKind`, `EVENT_CHANNEL`                       |
| `secrets`       | The secret DTOs, `SecretInput`, the type registry and the `secrets.*` channel I/O                  |
| `settings`      | `SettingKey` validation and the `settings.*` channel inputs/outputs                                |
| `system`        | `DeviceProfileDto`, `ResourceSampleDto` and the `system.*` channel inputs/outputs                  |
| `ai`            | `AdapterFamily`, `AuthMode`, `ToolMode`, `AdapterCapabilitiesDto`, `AdapterDescriptorDto`          |
| `providers`     | The provider enums and DTOs, `ProbeOutcomeDto`, `ProbeRequest` and the `providers.*` I/O           |
| `vector_stores` | The vector store, source, document, search, chunk and evaluation DTOs and the `vectorStores.*` I/O |
| `graph`         | The relation, edge, traversal and reconcile DTOs and the `graphRelations.*` I/O                    |
| `accounts`      | `AccountDto`, `AccountLinkResult` (tagged on `status`) and the `accounts.*` I/O                    |
| `enums`         | `WireEnum` and `wire_enum!`, for string enums with `ALL`, `as_str` and `parse`                     |
| `channels`      | `CHANNELS`, generated from `packages/shared/contract.channels.json`                                |
| `validate`      | The `Validate` trait, `parse`, the `check::*` helpers, `js_trim`, `normalize_url`                  |
| `wire`          | `present` and `nullable`, the two field deserialisers the wire rules need                          |

Domain DTOs arrive with their port task (TASK_061 – TASK_074), following the recipe below.

## Wire rules

These mirror PASSPORT §6. Review enforces them; the golden fixtures catch what review misses.

- Every struct carries `#[serde(rename_all = "camelCase")]`. Enums carry the exact Zod tags
  (`rename_all = "lowercase"`, `"SCREAMING_SNAKE_CASE"`, or per-variant `rename`).
- **Optional** (`.optional()` in Zod) is `Option<T>` with
  `#[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::wire::present")]`.
  It is absent on the wire, never `null`, and an incoming `null` is rejected exactly as Zod
  rejects it.
- **Nullable** (`.nullable()` in Zod) is `Option<T>` with
  `#[serde(deserialize_with = "crate::wire::nullable")]`. The key is required and the value may
  be `null`. Without `deserialize_with`, serde would quietly accept a missing key.
- A Zod `.default(x)` field is `Option<T>` under the optional rule plus an accessor that applies
  the default (`SystemDeviceInput::refresh`), so a minimal payload round-trips unchanged.
- Timestamps are `i64` epoch milliseconds (`Timestamp`). Other non-negative integers are `u64`.
  Plain `z.number()` is `f64`. Money and precision values are `String`.
- Ids are newtypes from `branded_ids!`. They accept only the hyphenated RFC 9562 form Zod's
  `z.uuid()` accepts (versions 1–8, RFC variant, plus nil and max) and always serialise
  lowercase.
- Loose JSON records (`z.record(z.string(), z.json())`) are `serde_json::Map<String, Value>`.
- Unknown fields are ignored, never denied: Zod objects strip them.
- `ResultEnvelope<T>` omits `data` only when `T` is zero-sized (`()`, i.e. a `z.void()`
  output). A `null` from a nullable output is kept.
- `HostEvent` is the envelope (`streamId`, `seq`, `ts`) with a flattened `HostEventDraft`,
  an internally tagged enum (`#[serde(tag = "type")]`) with the same variant names as Zod.

## Validation

Serde checks shape; `Validate` checks refinements. Every DTO implements it, even when it has
nothing to check (`no_refinements!`), so callers and the fixture suite can treat all types
alike. `validate::parse::<T>(value)` does both and turns any failure into
`VALIDATION_FAILED` with `details.field` and `details.rule` (or `details.reason` for a shape
error). Nested DTOs report a dotted path through `validate::nested`.

The refinements are **hand-written**, through the helpers in `validate::check`. `garde` and
`validator` were considered and rejected because they do not map cleanly onto the Zod rules
in use:

- Zod string lengths count UTF-16 code units; both crates count bytes or chars.
  `check::max_len` counts UTF-16 units.
- Zod `.int()` means a JavaScript safe integer. `check::safe_integer` and `check::timestamp`
  enforce `0..=2^53-1`; neither crate has that rule.
- UUID strictness, optional-versus-nullable and literal values are already handled by serde
  attributes, which leaves too little for a derive crate to earn its proc-macro dependency.

Output DTOs whose Zod schema defaults a field (`VectorStoreDto.rerank`, `VectorSearchHitDto.text`,
`VectorEvalModeResultDto.fallbacks`, the `VectorRerankConfig`/`VectorOcrConfig` fields) use
`#[serde(default)]` and always serialise the field, because the host always has the value.
Their fixtures therefore spell every defaulted field out.

## Golden fixtures

Fixtures live in `packages/shared/fixtures/<domain>/<Name>.valid.json` and
`<Name>.invalid.json`. Each file is a non-empty JSON array of `{ "name": string, "value": any }`.

- `packages/shared/tests/fixtures.test.ts` (Vitest): every valid case parses with the Zod
  schema and every invalid case fails.
- `crates/zvs-contract/tests/fixtures.rs` (`cargo test -p zvs-contract`): every valid case
  deserialises, passes `validate()`, and re-serialises to JSON equal to the fixture. Every
  invalid case fails deserialisation or validation.
- Both suites hold one table from `<domain>/<Name>` to a type. A fixture file without an entry
  fails that suite, and an entry without a `.valid.json` fails it too.

JSON equality is compared as `serde_json::Value`, so key order does not matter but casing,
`null` versus absent, tags and number kinds do. A float field round-trips only when the
fixture value has a fractional part (`37.5`, not `37`), because serde writes an `f64` as
`37.0`; write float fixtures that way.

## Channel registry

`packages/shared/contract.channels.json` is the sorted list of every channel in
`contract.ts`. Regenerate it with `pnpm contract:channels` after building `@zvs/shared`;
`pnpm contract:channels:check` (run in CI) fails on any drift. `build.rs` turns the file into
`CHANNELS`, and `is_channel` looks a name up. TASK_060's `IpcServer::finish()` checks its
registered handlers against `CHANNELS`.

## Recipe: adding a DTO

1. **Zod.** Write or locate the schema in `packages/shared/src`. For an inline channel schema,
   the Rust name is `<Domain><Verb>Input` / `<Domain><Verb>Output`
   (`system.ping` → `SystemPingInput`).
2. **Serde.** Add the type to `src/<domain>.rs` under the same name, following the wire rules
   above. Re-export it from `lib.rs`.
3. **Validate.** Implement `Validate` with `check::*` for every Zod refinement (lengths,
   ranges, safe integers, timestamps, non-empty, literals), or list the type in
   `no_refinements!` if there are none. Validate nested DTOs through `nested`.
4. **Fixtures.** Add `packages/shared/fixtures/<domain>/<Name>.valid.json` with at least one
   case that sets every optional field and one minimal case with none, plus
   `<Name>.invalid.json` with the cases that matter: each refinement, a missing required key,
   `null` for an optional field, a missing nullable key, and an unknown enum tag.
5. **Tables.** Add `"<domain>/<Name>"` to the table in `packages/shared/tests/fixtures.test.ts`
   and to `TABLE` in `crates/zvs-contract/tests/fixtures.rs`.
6. **Run both suites:** `pnpm test` and `cargo test -p zvs-contract`.

A new `AppErrorCode` goes into the TS enum, the Rust enum, `AppErrorCode::ALL`, `as_str`
and `errors/AppErrorCode.valid.json`, in the same position in all five.
