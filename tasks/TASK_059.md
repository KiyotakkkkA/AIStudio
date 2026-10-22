---
task: TASK_059
title: zvs-contract crate — Rust mirror of the DTOs, error envelope, events and golden fixtures
phase: M — Migration to Tauri
status: done
depends_on: [TASK_058]
unblocks: [TASK_060]
estimate: 4 h
---

# TASK_059 — zvs-contract crate — Rust mirror of the DTOs, error envelope, events and golden fixtures

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

`packages/shared` holds the contract today. It has Zod DTOs, branded ids, `AppErrorCode`, the
`Result` envelope, the `HostEvent` union and `contract.ts`, which maps all 79 channels to an
input and an output schema. The renderer keeps all of it: it still validates every response
(PASSPORT §7, "validate on both sides"). What changes is the other side. The host validating
input and producing output is now Rust, so the same shapes need a serde mirror. Something also
has to fail CI the moment the two drift.

This task creates `crates/zvs-contract`. It holds the primitives, ids, error codes, the
envelope, `HostEvent`, the channel-name registry and the `system.*` DTOs. It also builds the
**drift guard**: golden JSON fixtures that both toolchains parse. Domain DTOs are not ported
here. Each port task (TASK_061 – TASK_074) adds its own domain's DTOs and fixtures through the
mechanism built in this task.

## Why

Two independent definitions of one wire format drift silently. Field casing, optional versus
null, enum tags, numbers that should be strings: every one of those breaks only at runtime, in
a page nobody opened. A shared fixture set is what makes a mismatch a red test instead of a
user report.

## Depends on

- `TASK_058` — the Tauri shell and the `NOT_IMPLEMENTED` code.

## Read first

- `/docs/PASSPORT.md` §6 (DTO rules, now including the Rust mirror rules), §7 (error model,
  event channel)
- `packages/shared/src/primitives/`, `errors/`, `events/HostEvent.ts`, `ipc/contract.ts`,
  `ipc/defineContract.ts`, `system.ts`
- `packages/shared/README.md`

## Scope

**In scope**
- `crates/zvs-contract`, a pure crate with no Tauri dependency: `serde`, `serde_json`,
  `uuid`, `thiserror` and a validation approach (see step 3)
- Branded ids as newtypes (`ProviderId(Uuid)` and so on) with `#[serde(transparent)]`
- `AppErrorCode` as a Rust enum serialised in `SCREAMING_SNAKE_CASE`, identical to the TS
  values, `AppError { code, message, details }` and `ResultEnvelope<T>`
- `HostEvent` as an internally tagged enum (`#[serde(tag = "type")]`), with the same variant
  names, `streamId`, `seq` and `ts`
- The channel registry: `pub const CHANNELS: &[&str]`, generated from `contract.ts` and kept
  checked in (step 4)
- `system.*` DTOs
- Golden fixtures in `packages/shared/fixtures/<domain>/<Name>.valid.json` and
  `<Name>.invalid.json`, plus tests on both sides
- `scripts/contract-channels.mjs`, which writes `packages/shared/contract.channels.json` from
  `contract.ts`, and a check mode that CI runs

**Out of scope** (do not do these, even if tempting)
- Domain DTOs other than `system`. Each port task adds its own.
- Generating TypeScript from Rust or Rust from Zod. Codegen was considered and rejected (see
  PASSPORT §6): the renderer's Zod schemas carry refinements and Russian messages that neither
  direction round-trips cleanly.
- The IPC server itself (TASK_060)

## Steps

1. Wire rules in Rust, written once in the crate README and enforced by review:
   `#[serde(rename_all = "camelCase")]` on every struct. An optional field is `Option<T>` with
   `#[serde(default, skip_serializing_if = "Option::is_none")]`, so it is never `null` on the
   wire, matching "optional means optional" (§6). Timestamps are `i64` epoch milliseconds, and
   money and precision values are `String`. Unknown fields are ignored, not denied, because Zod
   objects strip by default.
2. Primitives, ids, `AppErrorCode`, `AppError`, `ResultEnvelope<T>` and `HostEvent`, each with
   unit tests on the exact JSON they produce.
3. Validation. Serde gives the shape. Refinements such as UUID version, string length, integer
   ranges and URL schemes go in a `Validate` trait with one `fn validate(&self) -> Result<(),
   AppError>` per DTO that needs it. Resolve and pin a validation crate (`garde` or
   `validator`) only if it maps cleanly onto the Zod refinements in use. Otherwise implement
   the trait by hand. Record the choice.
4. Channel registry. `scripts/contract-channels.mjs` imports `contract` and writes the sorted
   channel list to `packages/shared/contract.channels.json`. `--check` exits non-zero on a
   diff, and CI runs it. `zvs-contract` embeds that file with `include_str!` and exposes
   `CHANNELS`, with a test that it parses and is sorted.
5. Golden fixtures. For each DTO, write at least one valid sample (every optional field
   present) and one minimal sample (no optionals), plus the invalid cases that matter.
   - Vitest (`packages/shared/tests/fixtures.test.ts`): every `*.valid.json` parses with its
     Zod schema, and every `*.invalid.json` fails.
   - `cargo test -p zvs-contract`: every valid fixture deserialises and passes `validate()`,
     and re-serialising it gives JSON equal to the fixture. That equality is what catches
     casing and null-versus-absent drift. Every invalid fixture fails deserialisation or
     validation.
   - Fixture names map to types through one table, so a fixture without a Rust or a Zod type
     fails both suites.
6. `HostEvent` fixtures for every variant, including `approval` and `end` with each outcome.
7. Document the "how to add a DTO" recipe in `crates/zvs-contract/README.md`: Zod schema,
   serde type, fixtures, then both test suites.

## Verify

```bash
node scripts/contract-channels.mjs --check
cargo test -p zvs-contract
pnpm test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: rename one field in a Rust DTO to snake_case and confirm `cargo test` fails on
its fixture. Make one optional field serialise as `null` and confirm the same. Add a channel to
`contract.ts` without regenerating and confirm the check fails.

## Definition of done

- [x] `zvs-contract` holds the primitives, errors, envelope, `HostEvent`, `CHANNELS` and the
      `system` DTOs
- [x] Golden fixtures are parsed by both Zod and serde, and round-tripping is exact
- [x] Channel-list drift fails CI
- [x] The README documents the per-DTO recipe that later port tasks follow
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `crates/zvs-contract`, a workspace member with no Tauri dependency (`serde`,
`serde_json`, `thiserror`, `uuid =1.26.1` with only `std`, pinned in `[workspace.dependencies]`).
- `ids`: all 21 branded ids as `#[serde(transparent)]` newtypes over `Uuid`. A custom field
  (de)serialiser accepts only what Zod's `z.uuid()` accepts (hyphenated, versions 1–8, RFC variant,
  nil, max). `Uuid::parse_str` alone would also take the simple, braced and URN forms.
- `errors`: `AppErrorCode` (22 codes, `SCREAMING_SNAKE_CASE`, `ALL`, `as_str`, `Display`) and
  `AppError { code, message, details? }`, which is also a `thiserror` error.
- `envelope`: `ResultEnvelope<T>` with hand-written serde, because serde cannot tag on a bool. A
  zero-sized `T` (`()` for `z.void()` outputs) omits `data`, while `null` from a nullable output is
  kept.
- `events`: `HostEvent` is the envelope plus a flattened `HostEventDraft`, which is the internally
  tagged enum. The module also holds `RunOutcomeDto`, `RunOutcomeStatus`, `DeltaKind` and
  `EVENT_CHANNEL`.
- `primitives`: `Timestamp = i64`, `Json`, `PageRequest`, `Page<T>`, `Sort<F>`, `SortDirection`
  and `MAX_SAFE_INTEGER`.
- `system`: `DeviceTier`, `DeviceGpuDto`, `DeviceProfileDto`, `ResourceSampleDto` and the
  `system.*` channel inputs and outputs, named `System<Verb>Input` / `System<Verb>Output`.
- `wire::present` and `wire::nullable`: optional fields reject an incoming `null`, and nullable
  fields require the key. Plain serde gets both wrong.
- `validate`: the `Validate` trait, `parse`, `invalid`, `nested` and `check::*`.
- `channels`: `build.rs` turns `packages/shared/contract.channels.json` into `CHANNELS`. The crate
  also embeds the file with `include_str!` (`CHANNELS_JSON`), and tests check that it parses,
  matches `CHANNELS`, and is sorted and unique.
- `scripts/contract-channels.mjs` (`pnpm contract:channels`, `pnpm contract:channels:check`) reads
  `contract` from the built `@zvs/shared`. CI runs the check in `gates` after `pnpm build`.
- 44 fixture files (22 types × valid/invalid) under `packages/shared/fixtures/{primitives,errors,
  events,system}`. Both `tests/fixtures.test.ts` and `crates/zvs-contract/tests/fixtures.rs` hold
  the name→type table. Each also checks that the `AppErrorCode` fixture lists every code in order
  (TS enum and Rust `ALL`), and the Rust suite checks every `HostEvent` variant and `end` outcome.
- Manual checks done: a snake_case rename fails `SystemPingInput / sent`, dropping
  `skip_serializing_if` fails `SystemDemoJobInput / minimal`, and an extra channel in `contract.ts`
  makes `--check` exit 1.

**Deviated from plan:**
- The validation crate choice is hand-written `Validate`, not `garde` or `validator`. Zod counts
  string length in UTF-16 units and treats `.int()` as a JS safe integer, and neither crate maps
  onto those rules. The reasoning is in the crate README.
- `CHANNELS` is a `const` generated by `build.rs`. A `const &[&str]` cannot be parsed from
  `include_str!` at compile time. `include_str!` is still there for the parse test.
- The contract has 108 channels, not 79.
- Fixture equality is `serde_json::Value` equality, which ignores key order but not casing,
  null versus absent, or number kind. Float fixtures carry a fractional part for that reason.
- `system.demoJob`'s output is `RunHandleDto` (runs domain), so it is left to the runs port.
  `system.resources` has a `z.void()` input, so it has no DTO.
- `system.device`'s `refresh` uses `.default(false)`. In Rust it is `Option<bool>` plus
  `SystemDeviceInput::refresh()`, so a minimal payload round-trips unchanged.
- `packages/shared/tsconfig.test.json` gained `"types": ["node"]` because the fixture test reads
  the filesystem.

**Next session should know:**
- Add DTOs with the recipe in `crates/zvs-contract/README.md`. Every optional field needs
  `deserialize_with = "crate::wire::present"`, and every nullable one needs
  `deserialize_with = "crate::wire::nullable"`.
- `IpcServer::finish()` (TASK_060) should compare its handler set against
  `zvs_contract::CHANNELS`, and should parse inputs with `zvs_contract::parse::<T>` so that every
  failure becomes `VALIDATION_FAILED`.
- `cargo fmt --check` fails on `crates/zvs-core/src/pdf.rs` on this Windows checkout. This
  predates the task: CRLF line endings inside multi-line string literals.
- The workspace build needs `PROTOC`. Locally, point it at `target/tools/protoc/bin/protoc.exe`.
- On this machine, `cargo test --workspace` needs `-j 2`: the lance/datafusion test builds run out of memory
  at the default parallelism.
