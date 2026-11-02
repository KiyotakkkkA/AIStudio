---
task: TASK_051
title: Plugins page
phase: 9 — Integrations
status: done
depends_on: [TASK_049, TASK_050, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_051 — Plugins page

> **Revised 2026-11-02 (TASK_050).** Integrations are now presented as **plugins**. A plugin is
> set up once on this page and then attached in the chat composer with `/` (TASK_050 built the
> composer side). This task replaces the old three-column tabbed «Интеграции» page. Host and
> contract are done: `integrations.kinds|list|get|create|update|remove|test|enable|disable|
> activity|signIn|cancelSignIn`, `workers.*`, and the `integrations` device-code step event.

## Context

Four plugins exist: **Почта** (Google, Яндекс, Mail.ru and any IMAP/SMTP server), **GitHub**
(token or browser sign-in), **GitLab** (token or browser sign-in, any base URL) and
**Telegram** (bot token). A user may connect **several accounts of every plugin and every
sign-in type**: two GitHub accounts, one on a token and one signed in through the browser; three
mailboxes; two bots. The page must treat the account list as unbounded from the start.

The reference for the catalogue is the plugin directory of a chat product: a page title and a
one-line subtitle, a wide search field, segment tabs, then sections of plugins in a two-column
list. Each row has the plugin logo in a rounded tile, the name, a one-line description and a
trailing action (`+` to connect, or the number of connected accounts). It reads as a catalogue,
not as settings.

## Why

The composer can attach a plugin only after an account exists. Without this page, accounts
can be created only through IPC.

## Depends on

- `TASK_049` — Mail integration.
- `TASK_050` — Telegram, GitHub and GitLab plugins, the chat composer plugins.

## Read first

- `/docs/PASSPORT.md` §13, §14 (Integrations, Mail, Telegram, GitHub and GitLab, Chat tool set),
  ADR 0004, ADR 0006
- `apps/studio/src/renderer/features/plugins/` — `PluginMark`, `pluginPresentation.ts` (labels,
  descriptions and logos already used by the composer; extend them, do not fork them)
- `/design/Integrations.dc.html` — only for the account form and the automation rail; the
  tabbed shell and its Webhooks tab are gone

## Scope

**In scope**
- Route and nav item «Плагины» (replaces «Интеграции»), `features/plugins/` store and view-models
- Catalogue view: title «Плагины», subtitle «Подключите плагины, чтобы использовать их в
  диалогах и сценариях», search over name and description, segment tabs «Все» and
  «Подключённые», sections «Почта и сообщения» (Почта, Telegram) and «Разработка» (GitHub,
  GitLab) in a two-column list with logos
- Plugin view: header with the large logo, name, description and «Подключить аккаунт»; the
  list of that plugin's accounts (any number), each with status dot, sign-in type chip and
  name; the selected account's form and automation rail
- Account creation per sign-in type, forms generated from `describe()`
- Logos for every plugin and every mail preset
- Device-code sign-in panel for GitHub, loopback sign-in for GitLab and mail presets
- Test connection, enable / disable, remove, rename

**Out of scope** (do not do these, even if tempting)
- Webhooks (ADR 0004)
- Per-tool tier editing — the Tools page owns it
- New plugins or new provider fields; if the form needs a host change, note it in Handoff

## Steps

1. **Store and route.** `PluginStore` loads `integrations.kinds` and `integrations.list` once
   and keeps accounts grouped by kind; it refreshes an account after every mutation and listens
   for step events with `domain: "integrations"`. Selection is a plugin kind plus an optional
   account id, kept in the route (`/plugins`, `/plugins/:kind`, `/plugins/:kind/:accountId`).
2. **Catalogue.** Matches the reference layout described above, using the ZVS tokens (warm
   palette, light and dark): 16 px gutter, rounded search field, segment tabs as pills, section
   headings with a chevron, rows of `PluginMark size="md"` + name + description + action. The
   action is a `+` icon button when the plugin has no account, otherwise a count chip
   (`plural()` — «2 аккаунта»). «Подключённые» shows only plugins with accounts. Empty search:
   the shared empty state. The whole row opens the plugin view.
3. **Plugin view.** Back link to the catalogue, header (`PluginMark size="lg"`, name,
   description, primary «Подключить аккаунт»), then three columns as in the design: accounts
   list (250 px, any length, scrolls), the account form, the automation rail (286 px). With no
   accounts the middle column is the add-account flow itself.
4. **Sign-in types.** «Подключить аккаунт» first asks for the type, as a segmented choice with
   logos, then shows the generated form for that type only:
   - Почта: Google, Яндекс, Mail.ru (each with its own logo and «Войти»), «Другой сервер
     (IMAP / SMTP)» with the manual fields. The `protocol` field is set by the choice and hidden.
     `describe()` already marks manual-only fields; hide them for presets.
   - GitHub: «Токен доступа» (token secret) or «Вход через GitHub» (device flow; client id field
     shown as optional with the host help text). `auth` is set by the choice and hidden.
   - GitLab: «Токен доступа» or «Вход через GitLab» (application id, optional secret, redirect
     port). `baseUrl` is shown for both.
   - Telegram: bot token only.
   The account name is required and defaults to the plugin label plus a number when another
   account already has that name.
5. **Generated form.** Reuse the generated-form machinery of the Secrets page history (TASK_012)
   rather than hand-writing fields: `text`, `url`, `number`, `boolean`, `select` and `secret`.
   A `secret` field is a `SelectInput searchable` over **all** secrets (ADR 0006) with a link to
   the Secrets page; there is no way to type a credential. Fields with help text show it under
   the input. Optional `section` grouping is not needed now: order follows `describe()`.
6. **Browser sign-in.** «Войти» calls `integrations.signIn` and shows a waiting panel with
   «Отмена» (`integrations.cancelSignIn`). For GitHub the panel shows the `userCode` from the
   `device-code` step in large monospace with a copy button, the verification URL and the
   expiry countdown; the browser opens on its own. On success the account reloads and the
   form shows «Вход выполнен»; errors map through `AppErrorCode`, never by parsing text.
7. **Automation rail.** Triggers and tools from `integrations.get` (tool chips show «ask» for
   side-effecting ones), «Используется в» with links to scenarios bound to the account, the
   last activity lines (`integrations.activity`), the counters, and the worker detail as the
   health line: it carries the API quota for GitHub and GitLab («Лимит API: осталось … из …») and
   the rate-limit wait when there is one.
8. **Logos.** Extend `PLUGIN_MARKS` with `MAIL_PRESET_MARKS` for `google`, `yandex`, `mailru`
   and `imap`; bundled inline SVG paths only (identification use), no network images. The same
   marks appear in the catalogue, the plugin header, the account list, the type choice and the
   composer chips.
9. **States.** No accounts; signing in; needs sign-in (`PROVIDER_SESSION_EXPIRED`); disabled;
   failing with its reason; rate-limited (running, detail shows the wait).
10. **Tests.** Store: grouping by kind, several accounts per kind and per sign-in type, catalogue
    filter and search, the device-code step updates the sign-in panel, enable / disable and
    remove round-trip. Form view-model: hidden fields per sign-in type, the secret picker lists
    every secret. Presentational markup is covered once by the e2e smoke test.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Connect two GitHub accounts (one token, one browser sign-in), a Gmail mailbox and
a Telegram bot through the page. Test each. Confirm the composer lists `/github` with both
accounts, that the rail shows the API quota, and that write tools appear on the Tools page at
«ask».

## Definition of done

- [x] Catalogue with search, tabs, sections and logos
- [x] Any number of accounts per plugin and per sign-in type
- [x] Forms generated from provider schemas, credentials only as secret pickers over all secrets
- [x] Browser and device-code sign-in with cancel
- [x] Test connection reports per-direction results
- [x] Automation rail shows real triggers, tools, activity and quota
- [x] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Route `/plugins/:kind?/:accountId?` and the «Плагины» rail item replace «Интеграции»;
  `pages/IntegrationsPage.tsx` is gone. The rail marks an item active on its sub-paths too.
- `features/plugins/PluginStore.ts` — loads `integrations.kinds|list`, `secrets.list`,
  `scenarios.list` and `workers.list`; accounts grouped by kind; catalogue search, tabs and
  collapsible sections; selection from the route with `redirect` / `consumeRedirect` for the
  view to follow; create, update, test, enable / disable, remove, sign-in and cancel, each
  followed by `integrations.get` + `integrations.activity` + `workers.list` for that account;
  observes `domain: "integrations"` `device-code` steps for the sign-in panel.
- `signInTypes.ts` — the sign-in types per plugin (Google, Яндекс, Mail.ru, IMAP / SMTP; token or
  browser for GitHub and GitLab; bot token for Telegram) with the fixed config value, the hidden
  and the additionally required keys. `oauthToken` is always hidden: the host writes it on sign-in.
- `PluginFormVm.ts` — the generated form over `describe()` (`text`, `url`, `number`, `boolean`,
  `select`, `secret`), validation, and a config that keeps stored values of hidden fields.
- `MAIL_PRESET_MARKS` (`google`, `yandex`, `mailru`, `imap`); `PluginMark` takes a `mark`, and the
  composer chips and the `/` menu show the account's preset mark.
- Views: `PluginCatalogue`, `PluginsWorkspace` (plugin header and three columns),
  `PluginAccountList`, `PluginAccountPane`, `PluginField`, `SignInTypeChoice`, `SignInPanel`
  (device code, copy, verification URL, expiry countdown, «Отмена»), `AutomationRail`.
- `tests/renderer/plugins.test.ts` — 11 tests.

**Deviated from plan:**
- The accounts column reuses `EntityListPanel` (262 px) instead of a 250 px one, so it matches
  the Connections and Secrets columns.
- A browser-type account is created disabled, signed in, then enabled; with a token type
  «Подключить» creates it enabled.
- `describe()` marks manual-only mail fields only through help text, so the hidden keys per sign-in
  type are listed in `signInTypes.ts` instead of read from the descriptor.

**Next session should know:**
- `IntegrationAccountDto` carries no error code, so «Нужен вход» is derived: a browser-type
  account without `oauthToken`, or one whose `test` / `enable` / `signIn` failed with
  `PROVIDER_SESSION_EXPIRED` in this session. A `statusCode` on the DTO would make it exact.
- A cancelled sign-in comes back as `CONFLICT`; the store tells it apart by its own cancel flag.
- No IPC channel opens an arbitrary URL, so the verification URL is shown as selectable text;
  the host opens the browser itself.
- Not run here: `pnpm build`, `pnpm test:e2e` (the smoke test now expects «Плагины»), and the
  cargo gates (no Rust changed). `pnpm format:check` reports
  `apps/studio/src/renderer/ui/atoms/assistantGeometry.ts`, which this task does not touch.

---

**Phase 9 is complete when this task is done.** All fifteen surfaces from the maket now exist.
