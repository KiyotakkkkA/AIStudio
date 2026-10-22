---
task: TASK_011
title: Secret DTOs, IPC channels and the secret-type schema registry
phase: 1 — First vertical slice: Secrets
status: done
depends_on: [TASK_010]
unblocks: [TASK_012]
estimate: 3 h
---

# TASK_011 — Secret DTOs, IPC channels and the secret-type schema registry

## Context

`SecretService` (TASK_010) works but is unreachable from the UI. This task exposes it across
IPC, and builds the piece that gives the Secrets page its character.

Look at `/design/Main.dc.html`. The right-hand form has three parts: **General** (type, name,
scope) which is the same for every secret; **Ollama Cloud credentials** which is a boxed
section carrying a chip that reads `schema ollama-cloud@1`; and **Metadata** (tags, rotation,
note) which is again universal. Switching the Secret type swaps only the middle section.

That means the app needs a **secret-type schema registry**: a declarative description, per
secret type, of the fields that type needs — which is secret, which is a plain config value,
which are required, what the labels are. The form is generated from it. Adding support for a
new provider then means adding one registry entry rather than writing a new form component,
which is the difference between this being a nice app and a tedious one.

The registry lives in `packages/shared` because both sides need it: the renderer renders the
form from it, and the host validates against it. It is data, not UI.

## Why

The registry is the extension point for every credential type the app will ever support. If
the form is hand-written per type instead, TASK_014 (providers), TASK_032 (MCP) and TASK_049
(mail) each grow their own bespoke credential form.

## Depends on

- `TASK_010` — `SecretService` with create/update/list/remove.

## Read first

- `/design/Main.dc.html` — the form's three sections, the schema chip, the required-field
  asterisk, the masked value with its icons, the footer encryption note.
- `/docs/PASSPORT.md` §6 (DTO rules), §7 (contract, validation policy, error model).

## Scope

**In scope**
- `SecretTypeSchema` — the declarative field descriptor and the registry
- Entries for `ollama-cloud`, `openrouter`, `anthropic`, `mistral`, `qdrant`, `custom`
- `SecretDto`, `SecretSummaryDto`, create/update input DTOs
- Channels: `secrets.list`, `secrets.get`, `secrets.create`, `secrets.update`,
  `secrets.remove`, `secrets.types`
- Host-side validation of submitted fields **against the schema for that type**
- Tests for the registry, the DTOs and each handler

**Out of scope**
- UI (TASK_012)
- A reveal channel — TASK_010's decision block settles whether one exists at all; if the
  answer was write-only, do not add one here
- Import from `.env` — the mockup shows the button; it is a later task, leave it inert

## Steps

1. `packages/shared/src/secrets/SecretTypeSchema.ts`:
   ```ts
   FieldDescriptor = {
     key: string;
     label: string;
     kind: 'secret' | 'text' | 'url' | 'number' | 'boolean' | 'select';
     required: boolean;
     placeholder?: string;
     help?: string;
     options?: { value: string; label: string }[];  // select only
     default?: string | number | boolean;
   }
   SecretTypeSchema = {
     key: string;            // 'ollama-cloud'
     version: number;        // 1 — shown as the chip `ollama-cloud@1`
     label: string;          // 'Ollama Cloud API key'
     fields: FieldDescriptor[];
   }
   ```
   Exactly one field per type may be `kind: 'secret'` — that is the value that gets
   encrypted. Everything else lands in the row's plaintext `fields` JSON. Assert this
   invariant in a test over the whole registry, so a malformed entry cannot ship.
2. Registry entries, taken from the mockup and from what each provider actually needs:
   - `ollama-cloud` — API key (secret, required), Base URL (url, default
     `https://ollama.com/api`), Organization (text, optional), Verify TLS (boolean,
     default true)
   - `openrouter` — API key (secret, required), Base URL (url), Referer (text, optional)
   - `anthropic` — API key (secret, required), Base URL (url), API version (text)
   - `mistral` — API key (secret, required), Base URL (url)
   - `qdrant` — API key (secret, optional — local clusters often have none), URL (url,
     required, default `http://127.0.0.1:6333`)
   - `custom` — Value (secret, required), plus a free-form note; the escape hatch
3. Build a Zod validator **from** a schema (`buildFieldsSchema(schema)`) so the host can
   validate a submitted `fields` object without hand-writing a parallel schema per type.
   This function is the reason the registry is shared rather than renderer-only.
4. DTOs in `packages/shared/src/secrets/`:
   - `SecretSummaryDto` — id, type, name, scope, hint, tags, usageCount, rotationStatus,
     rotatesAt, updatedAt. **No value, no cipher.**
   - `SecretDto` — summary plus `fields` (non-secret values) and `note`.
   - `CreateSecretInput` — type, name, scope, `value` (the secret string), `fields`, tags,
     note, rotationDays.
   - `UpdateSecretInput` — id plus a partial; `value` optional, and its absence means "leave
     the credential alone" rather than "clear it". Document that in the schema comment,
     because it is the kind of thing a later agent gets wrong.
5. Add the channels to the contract. Follow the `domain.verb` rule. `secrets.types` returns
   the whole registry so the renderer never hardcodes a type list.
6. Handlers in `host/ipc/secrets.ts`. Thin, per PASSPORT §8: parse, call one service method,
   map entity → DTO. The entity→DTO mapping lives in the service, not the handler.
7. Validation chain, host-side: input DTO parses → look up the type's schema → build the
   field validator → validate `fields` → only then call the service. A submitted field the
   schema does not declare is an error, not something to store silently.
8. Wire `secrets.remove`'s `CONFLICT` error so the payload carries the consumer list —
   TASK_012 needs to name them in the UI.
9. Tests:
   - Registry invariant: every entry has exactly one `secret` field; keys are unique.
   - `buildFieldsSchema` accepts a valid payload and rejects a missing required field, an
     unknown field and a wrong type.
   - Every handler: happy path, validation failure, not-found, and conflict on delete.
   - `SecretSummaryDto` has no value field, asserted at runtime on a real response.
10. Commit: `TASK_011: secret DTOs, channels and type registry`.

## Verify

```bash
pnpm --filter shared test && pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks in DevTools:
- `await window.zvs.call('secrets.types', undefined)` returns all six schemas.
- Create a secret through `secrets.create`, then `secrets.list` — the response contains the
  hint and no value anywhere in the JSON.
- Submit an unknown field and confirm `VALIDATION_FAILED`, not a silent store.

## Definition of done

- [x] Registry with six types, invariant enforced by test
- [x] `buildFieldsSchema` drives host-side validation, no per-type hand-written schemas
- [x] Six channels wired, handlers thin, mapping in the service
- [x] No response anywhere contains a decrypted value (runtime-asserted)
- [x] Update without `value` leaves the credential untouched
- [x] Delete conflict names its consumers
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**
- `packages/shared/src/secrets/SecretTypeSchema.ts` — `FieldDescriptor`, `SecretTypeSchema`,
  the helpers `secretFieldOf` / `configFieldsOf` / `schemaChip` (the chip the mockup shows is
  `schemaChip(schema)` → `ollama-cloud@1`), and `buildFieldsSchema`. The built validator is a
  `z.strictObject` over the non-secret fields only, so an undeclared key is a hard error
  rather than something stored silently; optional fields with a `default` get it filled in on
  parse, required fields never do — a default on a required field is a UI prefill hint, not a
  server-side fallback, which is what keeps "missing required field" a real failure.
- `packages/shared/src/secrets/registry.ts` — `SECRET_TYPE_KEYS`, the `SecretTypeKey` enum,
  `SECRET_TYPE_REGISTRY` with the six entries, plus `findSecretTypeSchema` / `isSecretTypeKey`.
- DTOs, one file each: `SecretDto.ts` (`SecretSummaryDto`, `SecretDto`, and the shared
  `SecretScope` / `RotationStatus` / `SecretName` pieces), `CreateSecretInput.ts`,
  `UpdateSecretInput.ts`. The summary carries `usageCount` and `rotationStatus`; it has no
  `fields`, no `note`, no value and no cipher.
- Six channels on the contract: `secrets.types` (input `z.void()`), `secrets.list` (input
  `SecretFilter`), `secrets.get`, `secrets.create`, `secrets.update`, `secrets.remove`
  (output `{ id, removed: true }` rather than `z.void()`, which the output validator cannot
  round-trip).
- `apps/studio/src/host/ipc/secrets.ts` — thin handlers. Each one looks the schema up, runs
  `buildFieldsSchema` over the submitted `fields`, then calls exactly one service method and
  maps through `toSecretDto` / `toSecretSummaryDto`. There is no hand-written per-type schema
  anywhere in the host.
- `SecretService` gained `get(id)`, a `usageCount` on its summary (one grouped
  `countUsageBySecret()` query for `list`, a single count for `get` / `update`), and the two
  exported entity→DTO mappers. `SecretRepository` gained `countUsageBySecret()`.
- `test/helpers/secretService.ts` — a fake-crypto `SecretService` for the tests that only
  need one to satisfy `createHandlers`.
- `packages/shared/tests/secrets.test.ts` (17) — the registry invariants (exactly one
  `secret` field per entry, unique type keys, unique field keys, options only on `select`),
  `buildFieldsSchema` accepting a valid payload and rejecting a missing required field, an
  unknown field and a wrong type, and the DTO shapes including a smuggled `value` being
  stripped by `SecretSummaryDto.parse`.
- `apps/studio/tests/secretChannels.test.ts` (8) — every handler: happy path, validation
  failure, not-found and the delete conflict with its named consumers; a runtime assertion
  that no response contains the value; update-without-`value` leaving the hint untouched.
- `e2e/smoke.spec.ts` gained the task's three manual DevTools checks as a real test through
  the preload bridge: `secrets.types` returns all six schemas, a created secret comes back
  through `secrets.list` with its hint and no value anywhere in the JSON, and an unknown field
  produces `VALIDATION_FAILED`.

**Deviated from plan:**
- No `secrets.reveal`, per TASK_010's answered decision. `resolve()` stays host-internal.
- `custom`'s free-form field is `description`, not `note`, because `note` is already a
  universal Metadata field on the row and two fields labelled "Note" in one form is a trap.
- `CreateSecretInput.value` is optional, and `SecretService.create` now stores a `null` cipher
  when it is absent. `qdrant`'s API key is optional by the task's own wording, and a keyless
  local cluster is the normal case; the nullable `cipher` column and the existing
  `SECRET_MISSING` path in `resolve()` already anticipated this. The host enforces presence
  from the schema: required secret field ⇒ `value` must be non-empty.
- `UpdateSecretInput.rotationDays` is nullable-and-optional, against PASSPORT §6's "never
  both". The two states are genuinely distinct here — absent leaves the reminder alone, `null`
  clears it — and TASK_012's rotation dropdown needs a "Never" option. `value` is not like
  this: absent means leave the credential alone and there is no way to clear one.
- `SecretSummaryDto.type` is the `SecretTypeKey` enum, not a free string. The DB column stays
  free text, so a row written with a type outside the registry would fail output validation on
  `secrets.list`. That is the intended pressure — the registry is the list of types — but it
  means removing a registry entry is a breaking change for existing rows.
- `HostIpcDependencies` now requires `secrets`, so `createHandlers` call sites in
  `data.test.ts`, `events.test.ts` and `ipc.test.ts` were updated.

**Next session should know:**
- TASK_012 renders the middle section straight from `secrets.types`: `configFieldsOf(schema)`
  is the field list, `schemaChip(schema)` is the chip, `field.required` drives the asterisk,
  `field.default` prefills. It must not hardcode a type list.
- The eye icon is gone by decision — only copy-of-hint and replace-value remain.
- `secrets.update` replaces the whole `fields` object when it is present; it is not a merge.
  The form must submit every field it knows about, not just the changed ones.
- The `CONFLICT` from `secrets.remove` carries `details.consumers` as
  `[{ kind, id }]` — that is what the UI names.
- `SECRET_TYPES` in `host/data/schema/secret.ts` is now stale (it still lists `ollama`
  rather than `ollama-cloud`, and has no `anthropic` or `qdrant`). Nothing reads it; the
  registry is the source of truth. Delete it or align it when the schema is next touched.
- The Import-from-`.env` button stays inert; no channel exists for it.
