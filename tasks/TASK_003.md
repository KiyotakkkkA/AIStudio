---
task: TASK_003
title: packages/shared — DTO conventions, branded ids, error codes
phase: 0 — Foundation
status: done
depends_on: [TASK_001]
unblocks: [TASK_004]
estimate: 2 h
---

# TASK_003 — `packages/shared`: DTO conventions, branded ids, error codes

## Context

`packages/shared` is the third domain of this architecture, alongside `host` and `renderer`.
It holds every value that crosses the process boundary — as Zod schemas — plus the IPC contract
(TASK_004) and the shared error vocabulary. It is imported by both sides and imports neither;
TASK_001's lint rules enforce that.

The DTO discipline here is load-bearing for a specific reason. The renderer uses **MobX**, and
MobX takes ownership of the objects it stores. If a DTO carries class instances, `Date`
objects, `Map`s or getters, two things break: structured-clone across IPC either fails or
silently mangles them, and MobX proxies fight with them in ways that surface much later as
baffling bugs. So DTOs are strictly plain JSON-shaped data. This is not a style preference and
should not be relaxed for convenience.

DTOs are also **not** database entities. A row shape lives in the host's schema (TASK_006); the
mapping to a DTO is explicit and lives in a service. They will diverge over time and that is
correct — the UI's needs and the storage's needs are different.

## Why

Every channel added from TASK_004 onward assumes these conventions and this error vocabulary.
Establishing them once, with the rules written down next to the code, prevents fifty
inconsistent DTOs.

## Depends on

- `TASK_001` — workspace and tsconfig.

## Read first

- `/docs/PASSPORT.md` §6 (the DTO layer — this task implements it), §7 (error model), §17
  (naming).

## Scope

**In scope**
- Zod installed and a `packages/shared` structure that will scale to ~40 DTOs
- Branded id helpers and the id types the app already knows it needs
- `AppErrorCode` enum and the `Result` envelope used by every IPC response
- Common primitives: timestamps, pagination, sort
- A `README.md` inside the package stating the rules, so a future agent reads them in place
- Unit tests for the helpers and a few representative schemas

**Out of scope**
- The contract registry itself (TASK_004)
- Any feature DTOs — no Secret, Provider or Run DTOs yet; those arrive with their features
- Anything host- or renderer-specific

## Steps

1. Replace the placeholder `packages/shared` from TASK_001. Install `zod` (pin the current
   stable). Set up the package with `"type": "module"` and proper `exports`.
2. Write `packages/shared/README.md` first, stating the rules that follow. Keep it short and
   absolute:
   - Plain JSON only: no class instances, no `Date`, no `Map`/`Set`, no functions, no getters.
   - Timestamps are epoch milliseconds, `z.number().int()`. Formatting happens in the view.
   - Ids are branded strings.
   - Precision values (money, ratios that must not drift) are strings.
   - Optional means optional — never allow both `null` and `undefined` for one field.
   - One DTO per file, named `<Thing>Dto`; the TypeScript type is always `z.infer`, never
     hand-written.
   - A breaking change to a DTO means a new channel version, not an edit in place.
3. `src/primitives/branded.ts` — a `brandedId<T>(name)` helper producing
   `z.string().uuid().brand<T>()`, plus a matching constructor for host-side creation.
   Define the ids the app already knows about: `SecretId`, `IdentityId`, `ProviderId`,
   `ModelId`, `VectorStoreId`, `DocumentId`, `ConversationId`, `MessageId`, `ScenarioId`,
   `RunId`, `StepId`, `SkillId`, `McpServerId`, `ToolId`, `IntegrationId`, `DownloadId`,
   `TaskId`, `StreamId`.
4. `src/primitives/time.ts` — `Timestamp` schema (epoch ms) and a small helper for the host
   to produce one. No formatting helpers here; those belong to the renderer.
5. `src/primitives/paging.ts` — `PageRequest` (`limit`, `cursor`) and a generic
   `Page<T>` factory, since half the lists in this app will need it.
6. `src/errors/AppErrorCode.ts` — a string enum. Seed it with the codes the architecture
   already implies: `UNKNOWN`, `VALIDATION_FAILED`, `NOT_FOUND`, `CONFLICT`,
   `PERMISSION_DENIED`, `SECRET_MISSING`, `SECRET_DECRYPT_FAILED`, `PROVIDER_UNREACHABLE`,
   `PROVIDER_AUTH_FAILED`, `RATE_LIMITED`, `RUN_CANCELLED`, `RUN_FAILED`,
   `APPROVAL_DENIED`, `SIDECAR_UNAVAILABLE`, `NATIVE_ERROR`, `DB_ERROR`,
   `UNSUPPORTED_FORMAT`. Add a comment: codes are append-only, never renamed — the renderer
   maps them to copy.
7. `src/errors/Result.ts` — the envelope every IPC handler returns:
   `{ ok: true, data: T } | { ok: false, error: { code, message, details? } }`. Provide
   `ok()` and `err()` constructors and a type guard. Handlers never throw across IPC.
8. `src/index.ts` barrel export.
9. Tests (`vitest`): branded ids reject non-UUIDs and are nominally distinct at the type
   level (a `SecretId` must not be assignable to a `ProviderId` — assert with a type test);
   `Result` guards narrow correctly; `Timestamp` rejects floats and negative values.
10. Commit: `TASK_003: shared DTO conventions, ids and errors`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "For user-facing error messages, do you want them written in English in the code and
> translated in the UI later, or Russian from the start? The app's screens in the design
> mockups mix both."
>
> **Why it matters:** `AppErrorCode` maps to copy in the renderer. If the UI language is
> Russian, the mapping table and every later message string should be authored that way from
> the start rather than migrated.
> **Options:** (a) English in code, `t()` shim from the start so translation is additive;
> (b) Russian strings directly, no shim.
> **If the user is unavailable:** take (a) — PASSPORT §18 assumption 6 already says English
> with a `t()` shim. Note it in Handoff.

## Verify

```bash
pnpm --filter shared test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: try importing anything from `apps/studio` inside `packages/shared` — `pnpm lint`
must fail. Revert it afterwards.

## Definition of done

- [x] `packages/shared` builds and exports cleanly
- [x] README with the DTO rules exists and is accurate
- [x] Branded ids are nominally distinct (proven by a type test)
- [x] `Result` and `AppErrorCode` exist and are documented as append-only
- [x] Tests pass; gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:** Pinned Zod 4.5.4 and Vitest 5.0.0, package README, all 18 branded ID schemas
and inferred types, validating ID constructor, injectable timestamp helper, pagination and
sort schema factories, all 17 append-only error codes, Result schema/constructors/guard,
and public ESM exports. All 10 Vitest tests and TypeScript assertions pass. Workspace
typecheck, lint, build, shared formatting, and built-package import checks pass. The manual
shared-to-host import probe failed lint with import/no-restricted-paths as required and was removed.

**Deviated from plan:** Used Zod 4's z.uuid() spelling. UUID v7 generation remains in
the existing host platform service; createBrandedId(Schema, uuid) validates its output.
Error details are optional JSON objects rather than unrestricted unknown, enforcing the
JSON-only convention and avoiding null/undefined ambiguity. Added DOM lib declarations
for Zod's URL types and Vitest's web-platform types without adding application dependencies.

**Next session should know:** User confirmed all app labels and user-facing messages must
be Russian from the start, without a t() shim; communication with the user stays English.
PASSPORT assumption 6 and the shared README record this decision. TASK_004 should use
Result(DataSchema) at IPC boundaries, isOk only after validation, and the host's createId()
with createBrandedId. Feature DTOs and channel registry remain deferred. Task files are
private and gitignored; their completion records remain local by repository policy.
