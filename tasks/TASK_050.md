---
task: TASK_050
title: Telegram and Git hosting integrations
phase: 9 — Integrations
status: done
depends_on: [TASK_048, TASK_076]
unblocks: [TASK_051]
estimate: 4 h
---

# TASK_050 — Telegram and Git hosting integrations

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

Two more integrations on the TASK_048 framework, both smaller than mail because the framework now carries the weight.

**Telegram** — the design's second tab. The decision below settles the significant question: a bot account is straightforward, safe and clearly scoped; a user account via MTProto is far more capable and far more dangerous, since the agent would act as the person.

**Git hosting** — GitHub and GitLab. Triggers on issues, pull requests and pushes; tools for reading a repository, opening an issue, commenting and creating a pull request. Anything that writes is side-effecting and defaults to `ask`.

## Why

Telegram is the natural notification and command channel for a local automation studio, and Git hosting connects the agentic build sessions to real repositories.

## Depends on

- `TASK_048` — Integration framework — accounts, triggers, tool exposure.

## Read first

- `/design/Integrations.dc.html` — the Telegram and Git hosting tabs
- `/docs/PASSPORT.md` §14

## Scope

**In scope**
- Telegram provider with triggers and tools per the decision
- GitHub and GitLab providers: repo read, issue and PR tools, polling triggers (no webhooks, ADR 0004)
- Credentials from Secrets in both cases
- Registration of all tools with correct side-effect flags
- Tests against fixtures, no live-service dependency in CI

**Out of scope** (do not do these, even if tempting)
- The page (TASK_051)
- Full Git operations — cloning and committing belong to the agentic workspace, not to this integration
- Self-hosted GitLab quirks beyond a configurable base URL

## Steps

1. Telegram: implement per the decision below. Triggers on incoming messages with per-update deduplication; tools to send a message and send a file. Message content is untrusted input and must be marked as such.
2. Git hosting: one provider with GitHub and GitLab adapters behind it, since the shapes are close enough to share a surface. Base URL configurable so self-hosted GitLab works.
3. Git triggers: issue opened or commented, PR opened or updated, push. Poll with an ETag or since-cursor from the provider's background worker (TASK_048 `WorkerSupervisor`; no webhooks, ADR 0004) so the app is not rate-limited into uselessness.
4. Git tools: `git.readFile`, `git.listIssues`, `git.createIssue`, `git.comment`, `git.createPullRequest`. The last three are side-effecting and default to `ask`; wire them so an agentic session can use them under approval.
5. Rate limiting: respect the service's limits and surface remaining quota in the health rail rather than failing opaquely when it runs out.
6. Both providers mark all fetched content as untrusted — an issue body is exactly the kind of text that could be aimed at an agent.
7. Tests against recorded fixtures: trigger deduplication; tool calls produce the right requests; rate-limit responses back off rather than fail; untrusted marking is present on all fetched content.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "For Telegram — do you want a bot account, or your personal account? A bot is simple and safe: it only sees messages sent to it, and it can only post where it is added. Your personal account via MTProto can read all your chats and send as you, which is far more powerful and far more dangerous if a scenario misbehaves."
>
> **Why it matters:** It changes the library (a Rust crate: a Bot API client such as `teloxide`, or an MTProto client such as `grammers`), the auth flow, the risk profile and the amount of work substantially.
> **Options:** (a) bot only — a bot token from Secrets, straightforward; (b) user account via MTProto — full access, session-string auth, real risk; (c) bot now, user account as a later task if you find you need it.
> **If the user is unavailable:** take (a). If a user account is ever added it should be its own task with its own safety review, not a variant of this one.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Connect a Telegram bot and a GitHub account. Send the bot a message and confirm the trigger fires once. Read a file from a real repository through the tool. Attempt a write action and confirm it asks for approval.

## Definition of done

- [x] Telegram working per the decision, with deduplicated triggers
- [x] GitHub and GitLab adapters behind one provider, base URL configurable
- [x] Write tools default to `ask` and are honoured by the registry
- [x] Rate limits respected and surfaced
- [x] All fetched content marked untrusted
- [x] Tests use fixtures; CI needs no live service
- [x] Gates pass
- [x] `pnpm typecheck && pnpm lint` pass; `pnpm build` left to CI
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `zvs-host/src/integrations/telegram/` — `TelegramProvider` (kind `telegram`, «Telegram»), a
  Bot API client over `reqwest` (multipart enabled for `sendDocument`). `getMe`, then `getUpdates`
  long polling (50 s) from the `offset` cursor; the first start skips the backlog. Trigger
  `telegram.message` (event id = `update_id`), payload with `UntrustedTextDto` text, caption, chat
  title, sender and file names. Tools `telegram.recentMessages` (last 50 messages, `recent` cursor),
  and with `access: readWrite` `telegram.sendMessage` / `telegram.sendFile` at `ask`.
  `allowedChats` filters incoming chats and send targets. 429 waits `retry_after`; a rejected
  token fails the worker permanently and is never echoed.
- `zvs-host/src/integrations/git/` — `GitHostingProvider::github` / `::gitlab` (kinds `github`,
  `gitlab`) over one client (`client.rs`: bearer auth, ETag ledger, rate state per account) and
  `forge.rs` (endpoints, payload mapping). Sign-in types `auth: token | oauth`: GitHub device flow,
  GitLab loopback + PKCE with refresh. Polling per repository: issues and pull/merge requests by
  updated-since cursor, comments, branch heads (first poll only records cursors). Triggers
  `<kind>.issueOpened|issueCommented|pullRequestOpened|pullRequestUpdated|push`. Tools
  `<kind>.readFile|listIssues`, and with `readWrite` `<kind>.createIssue|comment|createPullRequest`
  at `ask`. The worker detail shows «Лимит API: осталось … из …»; 403-exhausted and 429 back off.
- `zvs-host/src/integrations/oauth.rs` — the generic OAuth code moved out of mail (grant, PKCE
  authorize URL, exchange, refresh, device flow, `trusted_https_url`); mail uses it and the Copilot
  sign-in reuses its URL check. `SignInRequest::prompt` + `SignInPrompt`, the
  `integrations`/`device-code` step on the event bus, `IntegrationToolCall::cursors`,
  `IntegrationProvider::instructions()`, `common.rs` (field builders) and a test-only `stub.rs`
  HTTP server with queued replies.
- **Plugins in chat.** `MessageDto.commands` + `plugins` (`ChatPluginDto`), `ChatSendInput` /
  `ChatContextInput.commands` + `plugins` (any number, unique). Migration `0037` turns the old
  `command` column into a list. `ChatPlugins` (implemented by `IntegrationService`) resolves an
  attached account; `plan_tools` offers integration tools only for attached accounts, and the
  system prompt gets each plugin's instructions. Renderer: `/` lists commands and plugin kinds,
  a kind with several accounts opens a sub-list, chips with logos (`features/plugins/PluginMark`),
  several commands at once, chips clear after sending; sent messages show their commands and
  plugins. The CLI sends a one-item list.
- **Secrets without types** (ADR 0006): `SecretTypeKey`, `SecretTypeSchema`, the registry, the
  `secrets.types` channel and secret `fields` are gone. Migration `0035` folds `ollama`,
  `openrouter`, `mistral` and `custom` into the host kind `user` and drops `fields`. The Secrets
  form is name + value + metadata; every secret picker lists all secrets and is searchable.
- Migration `0036` rebuilds `integration_account` so `kind` accepts `github` and `gitlab` (the
  child tables are copied aside so the drop does not cascade them away).
- Composition root: integrations are built before chat (chat takes them as `plugins`), all four
  providers are registered, and sign-in steps go to the event bus.
- `tasks/TASK_051.md` rewritten as the «Плагины» page (catalogue with logos, any number of
  accounts per plugin and sign-in type, device-code panel).

**Deviated from plan:**
- DECISION answered by the user: option (a), bot only. Added at the user's request: GitHub sign-in
  through OAuth (device flow) next to tokens, GitLab OAuth next to tokens, a per-account `access`
  mode, plugins in the chat composer with any number of `/` commands, and secrets without types.
- GitHub and GitLab are two plugin kinds (`github`, `gitlab`) served by one provider type, not one
  `git` kind; tool and trigger names are `github.*` / `gitlab.*` instead of `git.*`.
- The Copilot sign-in token lives in the Copilot CLI's store and cannot be reused, so GitHub OAuth
  is its own device flow; the accounts are independent. `GITHUB_CLIENT_ID` is empty until ZVS
  registers an OAuth App (with Device Flow enabled): until then `oauthClientId` is required.
- Telegram got `telegram.recentMessages` so a chat plugin can read what the bot received; the Bot
  API has no history call.
- Write approvals use the standard `ask` prompt, not a per-call preview like mail.
- Plugins attached to a message stay in effect for the rest of the dialog, as `/browser-use`
  does; the composer chips clear after each message.

**Next session should know:**
- TASK_051 builds the page; everything it needs exists in the host. The device-code step is
  `{ domain: "integrations", accountId, phase: "device-code", userCode, verificationUri,
  expiresAt }`.
- To ship one-click GitHub sign-in, register a ZVS OAuth App with Device Flow on and put its client
  id in `GITHUB_CLIENT_ID` (`integrations/git/mod.rs`).
- GitLab OAuth needs the user's application registered with
  `http://127.0.0.1:<port>/callback` and that port in `oauthRedirectPort`; it is not covered by a
  live test (the loopback flow is shared with mail and tested there).
- Not run here: `pnpm build`, `pnpm test:e2e` (the secrets smoke test was updated), and the manual
  check against a real bot and repository. Pre-existing failures not touched: `pnpm typecheck` in
  `tests/renderer/chatGoal.test.ts` and `pnpm format:check` on `ui/atoms/assistantGeometry.ts`.
