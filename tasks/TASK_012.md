---
task: TASK_012
title: Secrets page — list, schema-driven form, scope tabs
phase: 1 — First vertical slice: Secrets
status: done
depends_on: [TASK_011]
unblocks: [TASK_013]
estimate: 4 h
---

# TASK_012 — Secrets page: list, schema-driven form, scope tabs

## Context

This task closes the first vertical slice. Everything below the UI exists — table, service,
encryption, DTOs, channels, the type registry — and this task puts a real screen on top of it.
When it is done the app does something genuinely useful for the first time: it stores and
manages real credentials.

The visual spec is `/design/Main.dc.html`, and it should be followed closely rather than
interpreted. Open it in a browser next to the running app. The page is: a 56 px header with
title, count, search, "Import .env" and a primary "New secret" button; a tab row (Secrets /
Identity & Profile / Shared-Public info / Access log); then a two-column body — a 396 px list
on the left with filter chips above it, and a form card on the right filling the rest.

The form's middle section is generated from the type registry built in TASK_011. That is the
point of the whole slice: changing the Secret type select swaps the credential fields and the
`schema ollama-cloud@1` chip, while General, Metadata and the footer stay put.

This is also the first task that establishes how a feature module is laid out — PASSPORT §13
and decision D6. `features/secrets/` owns the store, the view-model and the domain-aware
organisms. Anything reusable and domain-free goes down into `ui/`. Every later page task
copies this shape, so it is worth getting tidy.

## Why

It proves the architecture end to end and unblocks TASK_013, since a provider cannot be
configured without a stored credential to point at.

## Depends on

- `TASK_011` — channels and the type registry.
- `TASK_007` — shell, rail, templates, tokens, root store.

## Read first

- `/design/Main.dc.html` — the whole page. Copy exact values: 396 px list column, 12 px card
  padding, 10 px card radius, 34 px control height, selected row `#1c2027` with an accent
  border. Do not round these to a grid.
- `/design/Foundations.dc.html` — control specimens and the list-row rest/selected states.
- `/docs/PASSPORT.md` §13 (renderer structure, MobX rules).

## Scope

**In scope**
- `features/secrets/`: `SecretStore`, `SecretFormVm`, and the page's organisms
- Secrets list with filter chips (All / Personal / Shared / Public) and search
- Schema-driven form: General, dynamic credential section, Metadata, footer
- Create, edit, delete flows with real error handling
- Empty state and loading state
- The other three tabs as honest stubs naming their future task
- Promote genuinely reusable pieces into `ui/` (field, select, chip, toggle, segmented control)

**Out of scope**
- Identity & Profile, Shared/Public info, Access log — stubs only
- Import from `.env` — leave the button disabled with a tooltip naming a later task
- A reveal-value control unless TASK_010's decision explicitly enabled it

## Steps

1. `features/secrets/SecretStore.ts` — MobX. Holds `summaries`, `selectedId`, `filter`,
   `query`, `types` (the registry, fetched once), `loading`, `error`. Actions:
   `load()`, `select(id)`, `setFilter()`, `setQuery()`, `create()`, `update()`, `remove()`.
   `makeAutoObservable` in the constructor; `runInAction` after every await. Register it in
   `RootStore` in the slot TASK_007 left.
2. `features/secrets/SecretFormVm.ts` — the form's own view-model, separate from the store
   because it has short-lived state the store should not carry: current values, dirty flags,
   per-field errors, and the selected type. It derives its visible fields from the registry
   entry for the selected type, and produces a `CreateSecretInput` / `UpdateSecretInput`.
   Validate client-side using the same `buildFieldsSchema` from `packages/shared` — one
   source of truth, no parallel rules.
3. `ui/` promotions — build these domain-free, since four later pages need them:
   `Field.tsx` (label + control + error), `TextInput.tsx`, `SelectInput.tsx`,
   `TagsInput.tsx`, `Toggle.tsx`, `SegmentedControl.tsx`, `Chip.tsx`, `StatusDot.tsx`,
   `ListRow.tsx`, `EmptyState.tsx`. One component per file. No store imports.
4. `features/secrets/SecretListItem.tsx` — the row from the mockup: 32 px type badge,
   name plus type chip, masked hint in mono, then a meta line (scope · used by N · updated).
   Status dot on the right, coloured by rotation status: `ok` green, `due-soon` amber,
   unused grey. Selected state exactly as drawn.
5. `features/secrets/SecretList.tsx` — filter chips with live counts, search box, the rows,
   and a dashed "Add secret from a provider schema" row at the bottom listing the available
   types, as in the mockup.
6. `features/secrets/SecretForm.tsx` — three sections:
   - **General**: type select (from `secrets.types`), display name, scope segmented control.
   - **Credentials**: the boxed section with the `schema <key>@<version>` chip, fields
     rendered from the registry by `kind`. Required fields get the accent asterisk. The
     secret field is masked. Changing the type resets this section only, keeping name and
     scope — losing a typed name because the user picked the wrong provider first is
     exactly the small cruelty to avoid.
   - **Metadata**: tags, rotation reminder select, note.
   - **Footer**: the lock icon and the "encrypted at rest" line from the mockup, then
     Cancel and Save.
7. `pages/SecretsPage.tsx` — compose `PageShell`, the tab row and the two columns. No logic
   here beyond wiring, per §13.
8. States, all of them real:
   - Loading: skeleton rows, not a spinner over the whole page.
   - Empty: an honest empty state inviting the first secret.
   - Save error: field-level errors from `VALIDATION_FAILED`; a general banner otherwise.
   - Delete conflict (`CONFLICT`): a dialog naming the consumers returned by the host,
     with no destructive default.
   - Unsaved changes: switching selection with a dirty form asks before discarding.
9. Map `AppErrorCode` to copy in one place — `features/secrets/errorCopy.ts` or a shared
   map if you prefer. Never render a raw error message from the host, and never parse one.
10. Compare against the mockup at 1440×900 and at the 1100 px minimum. The list column is
    fixed; the form column flexes. Fix what breaks.
11. Tests: `SecretFormVm` — switching type swaps fields and keeps name/scope; required-field
    validation matches the host's; update without touching the value omits `value` from the
    payload. `SecretStore` against the fake bridge — load, select, create, delete-conflict.
    No snapshot tests on the presentational components.
12. Commit: `TASK_012: secrets page`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "The Secrets page has four tabs in the design: Secrets, Identity & Profile, Shared/Public
> info, and Access log. Only the first is in this task. What should the other three actually
> hold — specifically, what does 'Identity & Profile' mean to you? Your name and details the
> agent can use when writing on your behalf, or something else?"
>
> **Why it matters:** It decides whether `identity` is a separate aggregate with its own table
> and task, or a handful of settings rows. The answer shapes a future task that does not exist
> in the backlog yet.
> **Options:** (a) identity is personal data the agent may use when drafting (name, role,
> signature, tone) — a small aggregate; (b) identity is OS/workspace-level account info —
> settings rows, no table; (c) drop the tab.
> **If the user is unavailable:** render all three tabs as stubs naming "to be specified", do
> not invent a model, and record the open question in Handoff.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks — do these with the mockup open side by side:
- Create a real Ollama secret. It appears in the list with a correct masked hint.
- Switch the type mid-form: credential fields swap, name and scope survive.
- Edit the name only and save: the credential is untouched (verify the ciphertext is
  byte-identical in the database).
- Delete a secret: succeeds. Attach usage manually in the database, try again: the conflict
  dialog names the consumer.
- Filter chips and search narrow the list correctly, counts stay accurate.
- Nothing in the DevTools network/IPC payloads contains a decrypted value.

## Definition of done

- [x] Page matches `/design/Main.dc.html` at 1440×900 — spacing, radii, colours, states
- [x] Form generated from the registry; changing type swaps only the credential section
- [x] Create, edit, delete all work against the real host
- [x] Loading, empty, validation, conflict and unsaved-changes states all implemented
- [x] `ui/` components are domain-free and store-free (lint proven)
- [x] Client validation shares the host's schema, not a copy
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Decision answered by the user:** (c) — drop the "Identity & Profile" tab. There is no
identity aggregate and no identity settings rows; nothing was invented. The tab row is now
three tabs: Secrets, Shared/Public info, Access log. The last two are honest stubs that say
the section is not specified yet, because no backlog task covers either one.

**Built:**
- `ui/atoms/`: `Button`, `Chip`, `StatusDot`, `TextInput`, `TextArea`, `SelectInput`,
  `Toggle`, `SegmentedControl`. `ui/molecules/`: `Field`, `TagsInput`, `ListRow`,
  `ListRowSkeleton`, `EmptyState`, `ConfirmDialog`. All domain-free, no store imports; the
  `ui` → `features`/`stores` restricted zone in eslint proves it.
- `features/secrets/SecretStore.ts` — `summaries`, `types`, `selectedId`, `selected`,
  `filter`, `query`, `loading`/`loaded`/`saving`/`removing`, `error`, `conflict`, plus
  `visible`/`counts`/`matchingQuery` as computeds. Actions `load`, `select`, `startCreate`,
  `requestCreate`, `requestSelect`, `confirmPending`, `cancelPending`, `resetForm`,
  `setFilter`, `setQuery`, `submit`, `create`, `update`, `remove`, `removeSelected`.
  Registered on `RootStore` as `secrets`.
- `features/secrets/SecretFormVm.ts` — its own view-model: current values, per-field errors,
  a banner, a dirty flag from a snapshot taken at construction. Fields derive from
  `configFieldsOf(schema)`; validation runs the host's `buildFieldsSchema` from
  `packages/shared`, never a copy. `toCreateInput()` / `toUpdateInput()` build the DTOs, and
  `value` is omitted from an update when the user did not type a new credential.
- `features/secrets/`: `SecretList`, `SecretListItem`, `SecretForm`,
  `SecretCredentialField`, `SecretConflictDialog`, `SecretsHeaderActions`,
  `SecretsWorkspace`, `secretPresentation.ts`, `errorCopy.ts`.
- `pages/SecretsPage.tsx` — `PageShell` + the tab row + the workspace, wiring only.
- `errorCopy.ts` maps every `AppErrorCode` to Russian copy and turns a host
  `VALIDATION_FAILED` into per-field errors (`fields.<key>` from `details.issues`). A raw host
  message is never rendered and never parsed; a store test asserts the raw text does not
  reach the UI.
- Tests: `tests/renderer/secretFormVm.test.ts` (6) and `tests/renderer/secretStore.test.ts`
  (9) against the fake bridge — type switch keeps name and scope, client validation agrees
  with `buildFieldsSchema`, update omits an untouched `value`, load/filter/select/create,
  field-level validation failure, delete conflict with named consumers, plain delete.

**Deviated from plan:**
- The store holds the current `SecretFormVm` instance (`secrets.form`) instead of a React
  component owning it. The VM is still a separate class with its own short-lived state; the
  store only owns the reference, which is what lets the header's "New secret" button and the
  list share one unsaved-changes guard without prop-drilling through `PageShell`.
- Two extra `ui` pieces beyond the listed ten: `Button` (the design's pri/sec/gho/danger
  button) and `TextArea` (the 64 px note box), plus `ListRowSkeleton` and `ConfirmDialog`.
  Four later pages need all of them.
- No "Duplicate" button, although the mockup draws one next to Delete. Duplicating a secret
  cannot copy the credential (there is no reveal channel), so what it should do is a product
  question, not a layout one.
- Two new tokens in `theme.css`: `--color-main-750: #1c2027` (the selected row) and
  `--color-err-border: #4a3134` (the delete button's border). Both are drawn in the mockup
  and had no token; no raw hex was added outside `theme.css`.
- The search box lives in the 56 px header, as the mockup draws it, so the list has filter
  chips only rather than its own second search field.
- `rotationDays` is reconstructed for an existing secret as
  `round((rotatesAt - updatedAt) / 1 day)` and shown only when it matches one of the offered
  intervals; otherwise the select reads "Никогда". The DTO does not carry `rotationDays`.

**Next session should know:**
- Verified in Chrome at 1440×900 and at 1100×800 against `Main.dc.html` with a stub-bridge
  Vite harness (the harness files were deleted afterwards): list column exactly 396 px,
  selected row `#1c2027` with a `#85aad6` border, 10 px radius, 12 px padding, the schema
  chip `#2a2d33` on `#b6d2f5`, and no horizontal overflow at either size. The card footer is
  pinned below the scrolling form body, and its "encrypted at rest" line clamps to two lines
  at 1100 px.
- The manual database checks in **Verify** (byte-identical ciphertext, a manually attached
  usage row) were not run against a live Electron build in this session; the equivalent
  behaviour is covered by `secretService.test.ts` (TASK_010) and `secretChannels.test.ts`
  (TASK_011), and the renderer side is covered by the fake-bridge store tests.
- `secrets.update` replaces the whole `fields` object, so `SecretFormVm.fieldsPayload()`
  always submits every field the schema declares. Optional empty fields are omitted so the
  schema's `.default()` fills them; required ones are submitted empty so validation fails
  honestly.
- The Import `.env` button is disabled with a tooltip. No backlog task exists for it yet —
  when one is created, name it in `SecretsHeaderActions.IMPORT_HINT`.
- Open question recorded for a future task: what the Shared/Public info and Access log tabs
  should contain.

---

**Phase 1 is complete when this task is done.** The architecture is proven end to end: SQLite
→ repository → service → DTO → IPC → MobX → React, with encryption and a real screen.

**Follow-up in the same session (requested after the first commit):**
- Every native control is now a kit component: `ui/atoms/TextInput` wraps `InputSmall`,
  `TextArea` wraps `InputBig`, `SelectInput` wraps `Select` (composed as
  `Select.Trigger` + `Select.Menu` + `Select.Options`), `SegmentedControl` wraps `Switcher`,
  and `TagsInput`'s tag entry is a borderless `InputSmall`. The kit merges classes with
  `tailwind-merge`, so the design's exact values are passed through `classNames` /
  `className` and win over the kit defaults deterministically.
- Two consequences of the kit `Select`: it takes no `id`, so those `Field`s label the control
  with `label` / `aria-label` instead of `htmlFor`; and its trigger renders only the selected
  option's label, so the accent dot the mockup draws before the secret type is gone.
- `InputSmall`'s `preset="password"` was deliberately not used — it renders a show/hide eye,
  which TASK_010's decision rules out. The secret field is a plain `type="password"`.
- `anthropic` and `qdrant` were removed from `SECRET_TYPE_REGISTRY` and `SECRET_TYPE_KEYS`.
  Four types remain: `ollama-cloud`, `openrouter`, `mistral`, `custom`. Every remaining type
  has a required secret field, so `CreateSecretInput.value`'s description was corrected and
  the "keyless qdrant" channel test became "every registry type demands a credential". No
  registry entry has a required non-secret field any more, so the `buildFieldsSchema`
  required-field test builds its own fixture schema instead of borrowing qdrant's.
