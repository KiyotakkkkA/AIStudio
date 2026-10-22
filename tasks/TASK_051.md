---
task: TASK_051
title: Integrations page
phase: 9 — Integrations
status: todo
depends_on: [TASK_049, TASK_050, TASK_076]
unblocks: []
estimate: 3 h
---

# TASK_051 — Integrations page

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The page from `/design/Integrations.dc.html` and the close of Phase 9: a tab per account kind, and beneath each the same three-column shape — a 250 px accounts list, the configuration form generated from the provider's field schema, and a 286 px rail listing exposed triggers and tools, what uses them, recent activity and counters.

Because TASK_048's providers describe their own fields using the same `FieldDescriptor` shape as the secret-type registry (TASK_011), the form is generated rather than hand-written per tab — the same trick that made the Secrets page cheap.

## Why

Integrations that can only be configured through the database are not a feature. This is also where the credential rule becomes visible to the user.

## Depends on

- `TASK_049` — Mail integration (IMAP/SMTP).
- `TASK_050` — Telegram and Git hosting integrations.

## Read first

- `/design/Integrations.dc.html`
- `/docs/PASSPORT.md` §13

## Scope

**In scope**
- `features/integrations/` store, form view-model, organisms
- Tab per kind with counts
- Accounts list with status dots and protocol chips
- Generated configuration form, including grouped sections (incoming / outgoing for mail)
- Test connection with per-direction results
- Automation rail: triggers, tools, used-by, activity log, counters
- Enable/disable per account

**Out of scope** (do not do these, even if tempting)
- Webhooks tab unless TASK_048 implemented it — render it as a stub naming its task
- Per-tool tier editing — that lives on the Tools page

## Steps

1. Store and form view-model reusing the generated-form machinery from TASK_012, driven by `describe()` from each provider.
2. Field grouping: the mail form has Account, Incoming, Outgoing and Sync sections, so `FieldDescriptor` needs an optional `section` — add it in `packages/shared` and use it here rather than hardcoding a mail-specific layout.
3. Credential fields render as secret pickers by construction (TASK_048 made a raw credential unrepresentable) — verify that visually and note it in Handoff if any provider slipped through.
4. Test connection: show per-direction results where a provider reports them, as the mail result line does.
5. Automation rail: trigger and tool chips, the used-by list linking to scenarios, the recent activity log and the 24-hour counters from the mockup.
6. States: no accounts for this kind, needs re-authorisation, disabled, failing with its reason.
7. Tests: generated form renders each provider's schema; section grouping works; enable/disable round-trips.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Configure a real mail account and a Telegram bot through the page. Test both. Confirm the automation rail lists their triggers and tools, and that those tools appear on the Tools page with the right tiers.

## Definition of done

- [ ] Page matches the mockup across the tabs that have providers
- [ ] Forms generated from provider schemas, with sections
- [ ] No raw credential field exists on any tab
- [ ] Test connection reports per-direction results
- [ ] Automation rail shows real triggers, tools and activity
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**

---

**Phase 9 is complete when this task is done.** All fifteen surfaces from the maket now exist.
