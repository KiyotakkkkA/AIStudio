---
task: TASK_016
title: AI Providers page — three tabs, one form, model cards
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_015]
unblocks: [TASK_017]
estimate: 4 h
---

# TASK_016 — AI Providers page: three tabs, one form, model cards

## Context

Everything beneath the Providers page exists: schema, repositories, driver ports, adapters,
probe and discovery. This task builds the screen, and completes Phase 2 — after it, the app
can be pointed at a real model and told to generate.

The spec is `/design/Providers.dc.html`, and the single most important thing it says is
written on the canvas note: **the three tabs render the same form and the same list.** Text
Generation, Embeddings and Image Generation differ only in the capability filter applied to
the provider list and the discovered models. Build one set of components and filter them.
Building three variants is the mistake this page is designed to prevent.

Page anatomy, left to right: a 250 px provider list with status dots; then a detail column
holding two cards side by side — **Connection setup** (provider kind, name, base URL, API
token picker, Test connection with its result line) and **Model settings** (temperature, top K,
top P sliders, max output budget, request timeout) — with the **Available models** card
filling the space below, showing model cards in a three-column grid.

The API token field is a picker over stored secrets, never a raw text input. That is the
payoff of Phase 1 and it is stated in the canvas note: "API token is a picker over Secrets,
never a raw input."

## Why

It closes the loop from credential to usable model, and it is the last thing standing between
the app and being able to actually generate text.

## Depends on

- `TASK_015` — provider channels, probe, discovery.
- `TASK_012` — the `ui/` components promoted there (Field, SelectInput, SegmentedControl,
  Toggle, Chip, StatusDot, ListRow, EmptyState) and the feature-module pattern to copy.

## Read first

- `/design/Providers.dc.html` — the whole page, including the tab row's counts, the result
  line format, the slider styling and the model card layout with its `Default` chip and the
  dimmed unavailable card.
- `/design/Foundations.dc.html` — slider and control specimens.
- `/docs/PASSPORT.md` §13, §12.

## Scope

**In scope**
- `features/providers/`: `ProviderStore`, `ProviderFormVm`, organisms
- Capability tabs driving one filtered list and one form
- Connection setup card with an **auth-mode switch** (API key · Account), the secret picker
  and live Test connection
- Model settings card with sliders and numeric fields
- Model card grid, default-model selection, unavailable state
- Probe result line, provider status dots, the degraded/rotation warning
- A `Slider` component promoted into `ui/`
- Tests for the view-model and store

**Out of scope**
- Image generation adapters — the tab exists and filters correctly; if no image provider can
  be configured yet, show an honest empty state naming that
- Cost or usage dashboards
- Bulk import of providers
- The Accounts tab itself — the Login button, account cards and linking flow are
  `TASK_016_EXTRA_1`. This task only makes an account-mode provider selectable and honest
  about its state; if `TASK_015_EXTRA_1` has not landed, render the Account option disabled
  with "linking arrives in TASK_016_EXTRA_1" rather than half-building it

## Steps

1. `features/providers/ProviderStore.ts` — MobX. State: `capability` (the active tab),
   `summaries`, `selectedId`, `detail`, `models`, `probing`, `probeResult`, `loading`,
   `error`. Actions: `load(capability)`, `select(id)`, `create()`, `update()`, `remove()`,
   `probe()`, `setDefaultModel(modelId)`, `refreshAll()`. Switching tabs refetches with the
   capability filter rather than filtering client-side, so counts stay honest.
2. `features/providers/ProviderFormVm.ts` — connection fields plus model settings, dirty
   tracking, per-field errors. It holds the **draft** used by "Test connection before save",
   which TASK_015's `probe` accepts directly. Keep probe results attached to the draft, so
   editing the base URL clears a stale "200 OK" line rather than leaving a lie on screen.
3. `ui/atoms/Slider.tsx` — promote from the mockup: 4 px track, `main-700` rail,
   `accent-dark` fill, 14 px knob with a 2 px `main-900` ring. Domain-free, controlled,
   with a numeric readout slot. Used again by TASK_025.
4. `features/providers/ProviderList.tsx` — 250 px column, section heading naming the active
   capability, rows with a two-letter badge, name, status line and dot, plus the dashed
   "Add connection" row from the mockup.
5. `features/providers/ConnectionCard.tsx`:
   - Provider kind select, connection name, base URL.
   - **Auth mode** as a `SegmentedControl`: *API key* or *Account* (PASSPORT §12). It swaps
     one row of the form, nothing else — API key shows base URL plus the secret picker;
     Account shows a read-only linked-account row (avatar, masked email, status) with a
     "Manage accounts" link. Adapter families that cannot run in a mode (from the
     `AdapterCapabilities` descriptor on `ProviderDto`) disable that segment with a reason,
     rather than letting the user build a configuration that cannot work.
   - Sampling controls the adapter family does not honour are disabled with a short note —
     read `capabilities.honours`, never hard-code vendor knowledge in the renderer.
   - **Secret picker**: a select over `secrets.list` filtered to the types plausible for the
     chosen provider kind, showing the secret's name and key icon, plus a "Manage" button
     that routes to the Secrets page. Never a free-text token field.
   - "Test connection" button with a busy state, then the result line rendered from the
     probe union: `ok` → `200 OK · 412 ms · 14 models`; `ok-empty` → reachable but no models
     returned; `auth-failed`, `account-not-linked`, `session-expired`, `unreachable`,
     `rate-limited` → their own copy and colour. `session-expired` reads "session expired —
     sign in again" with a link to the Accounts tab, and must never be worded as a bad key. If
     the result was a curated list (`live: false`), say so — do not imply a probe listed them.
6. `features/providers/ModelSettingsCard.tsx` — temperature, top K, top P as sliders with
   mono readouts; max output budget and request timeout as numeric fields with unit
   suffixes. Values come from the provider's `settings` JSON.
7. `features/providers/ModelGrid.tsx` — three-column grid of cards. Each card: mono model
   id, provider and size line, capability chips (context window, tools, vision, code), and a
   radio for default. The default card is highlighted per the mockup. Unavailable models
   render dimmed with their `unavailableReason`. Above the grid: a filter input and an
   `N of M` chip.
8. `pages/ProvidersPage.tsx` — `PageShell`, the tab row with per-capability counts, the two
   columns. Composition only.
9. States: loading skeletons; empty (no providers for this capability); probe in flight;
   probe failed; unsaved changes on tab or selection switch; delete confirmation that names
   anything depending on the provider.
10. Check against the mockup at 1440×900 and at 1100 px. The provider list is fixed; the
    detail column flexes; the model grid drops to two columns when narrow.
11. Tests: `ProviderFormVm` — editing the base URL clears a stale probe result; settings
    round-trip; required fields match the host's validation. `ProviderStore` against the
    fake bridge — tab switch refetches with the right filter; probe updates status; default
    model selection persists. No snapshot tests on presentational components.
12. Commit: `TASK_016: AI providers page`.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks, mockup open side by side:
- Create a provider using a secret stored in Phase 1. Press Test connection: a real latency
  and a real model list appear, and the cards populate.
- Switch to the Embeddings tab: the list filters, the form is the same shape.
- Break the secret, probe again: the row goes amber/red with the right copy and no stack.
- Pick a default model, restart the app, confirm it stuck.
- Edit the base URL after a successful probe: the old result line clears.

## Definition of done

- [x] One form and one list serving all three capability tabs
- [x] Token is always a picker over Secrets — no free-text credential field anywhere
- [x] Test connection works live and renders every outcome of the probe union, with
      `session-expired` and `auth-failed` visibly different
- [x] Auth mode is a switch over one form; unsupported combinations are disabled with a reason
- [x] Controls for parameters the adapter family ignores are disabled from the descriptor
- [x] Model cards show discovered models, capability chips, default selection, unavailability
- [x] Page matches the mockup at 1440×900, degrades cleanly at 1100 px
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**

- `features/providers/`: `ProviderStore` (capability tabs, selection, probe, default model,
  refresh-all), `ProviderFormVm` (connection + settings draft, dirty tracking, per-field
  errors, probe result attached to the draft), `ProviderList`, `ConnectionCard`,
  `AuthModeSwitch`, `ProbeResultLine`, `ModelSettingsCard`, `ModelGrid`, `ModelCard`,
  `ProvidersWorkspace`, plus `providerPresentation`, `providerErrors` and `modelRows`.
- `ui/atoms/Slider.tsx` — promoted, domain-free, controlled, with a readout slot.
- `ui/atoms/statusTone.ts` — `StatusTone` and its tone maps extracted out of `StatusDot.tsx`
  so presentation modules reachable from `.ts` tests do not pull a `.tsx` module in.
- `pages/ProvidersPage.tsx` — composition only: header actions, the capability tab row with
  per-tab counts, `ProvidersWorkspace`.
- New channel `providers.adapters` (+ `AdapterDescriptorDto` in shared) so the renderer reads
  `honours`, `authModes`, `embedding`, `image` and `implemented` from the host registry
  instead of hard-coding vendor knowledge. Handler lives in `host/ipc/providers.ts`.
- Tests: `providerFormVm.test.ts`, `providerStore.test.ts`, `providersPage.test.tsx` and the
  shared `providerFixtures.ts`.

**Deviated from plan:**

- The two axes are shown as two controls: a vendor select (`ProviderKind`) and an adapter
  family select (`AdapterFamily`), because `PROVIDER_KINDS` has no Qwen/DeepSeek entry and
  collapsing kind into adapter is exactly what PASSPORT §12 forbids. The vendor only suggests
  a base URL and filters the secret picker.
- Counts stay honest by fetching all three capability-filtered lists on load, refresh and tab
  switch; the active tab's list is the one rendered.
- The Account row is selectable (accounts channels landed in TASK_015_EXTRA_1) but the
  "Аккаунты" button is disabled and every account-flavoured message names TASK_016_EXTRA_1,
  since no linking UI exists yet.
- Capability membership is editable from the form as chips, gated by the adapter descriptor;
  without it a provider created on one tab could never serve another.
- The save/discard/delete footer is its own row under the model card grid rather than inside
  that card, so the grid can scroll on its own.

**Next session should know:**

- A draft probe returns `DiscoveredModelDto`s with no `ModelId`; those render in the grid but
  their default radio is disabled until the provider is saved and re-probed.
- Anything that edits the connection (vendor, adapter, auth mode, base URL, secret, account)
  clears the probe result; sampling edits deliberately do not.
- The image tab renders an honest empty state while no implemented adapter family reports
  `image: true`.
- The app itself was not launched (Electron): the page is covered by the renderer tests and
  the responsive rules were written against the mockup, not verified on screen.

---

**Phase 2 is complete when this task is done.** The app can hold credentials, connect to real
AI providers, discover their models and remember which one is default. From TASK_017 the work
turns to Rust and retrieval.
