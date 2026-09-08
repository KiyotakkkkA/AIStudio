# Shared boundary data

This package is imported by host and renderer and imports neither. Keep reusable schemas
in `src/primitives`, the error vocabulary in `src/errors`, and future feature DTOs in
`src/dtos/<domain>/<Thing>Dto.ts`. Export public schemas and inferred types from `src/index.ts`.

- Plain JSON only: no class instances, `Date`, `Map`/`Set`, functions, or getters.
- Timestamps are nonnegative epoch milliseconds (`z.number().int()`). Format in the view.
- IDs are branded UUID strings. The host generates UUID v7; `createBrandedId(Schema, uuid)`
  validates and brands that string without importing host code.
- Precision values (money and ratios that must not drift) are strings.
- Optional means optional: never allow both `null` and `undefined` for one field.
- One DTO per file, named `<Thing>Dto`. Its TypeScript type is always `z.infer`, never handwritten.
- DTOs are not database entities. Services explicitly map host rows to DTOs.
- A breaking DTO change requires a new channel version; never edit the existing contract in place.

`timestampNow(clock)` accepts an injected millisecond clock, defaulting to `Date.now`.
`PageRequest` requires a positive integer `limit` and an optional nonempty opaque `cursor`.
`Page(ItemSchema)` validates `{ items, nextCursor? }`; omit `nextCursor` at the end.
`Sort(FieldSchema)` restricts `field` to the supplied schema and `direction` to `asc | desc`.

`Result(DataSchema)` validates the discriminated IPC envelope. `ok(data)` and
`err(code, message, details?)` construct it; `isOk(result)` narrows an already validated
result. Validate untrusted envelopes with the schema, not the type guard. Success data
must follow the DTO rules. Error details are optional JSON objects; omit them when absent.
Handlers never throw across IPC. Keep stacks on the host; the renderer maps error codes
to copy and never parses message strings. Error codes are append-only, never renamed.

`src/ipc/contract.ts` is the single source of truth for every channel: `domain.verb`,
lowercase, dot-separated. `defineContract` preserves the literal channel names so
`ChannelName`, `InputOf` and `OutputOf` stay exact; `@zvs/ipc` derives the typed client and
the exhaustive handler map from them. A breaking change adds `domain.verb@2` instead of
editing a released channel. `AppError` is what host code throws when it already knows the
code to report; anything else becomes `UNKNOWN` at the boundary.

All app labels and user-facing messages are authored in Russian from the start, without
a `t()` shim. Code identifiers and error codes remain English.

`src/events/HostEvent.ts` holds the one multiplexed event channel (PASSPORT §7, decision D4).
`EVENT_CHANNEL` is `zvs:events`; `HostEvent` is a discriminated union over `type` with the
variants `token`, `progress`, `step`, `log`, `approval` and `end`. Every variant carries the
same envelope — `streamId`, a per-stream monotonic `seq` and `ts`. `HostEventDraft` is the
same union without that envelope: producers describe the payload, the host stamps the rest.
`StepEventDto`, `LogLineDto` and `ApprovalRequestDto` are still loose JSON records; TASK_022
tightens the step and log shapes and TASK_023 tightens the approval request. `RunOutcomeDto`
is already closed: `status` is `ok | cancelled | failed` with an optional `AppErrorCode` and
message. Adding a variant is a breaking change for every subscriber — extend the payload of
an existing one where that is possible.
