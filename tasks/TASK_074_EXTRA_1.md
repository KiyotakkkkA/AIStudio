---
task: TASK_074_EXTRA_1
title: Account providers in chat — linked accounts in the chat picker, DeepSeek web removed, ADR on account-mode limits
phase: M — Migration to Tauri
status: done
depends_on: [TASK_074]
unblocks: []
estimate: 3 h
---

# TASK_074_EXTRA_1 — Account providers in chat

## Context

TASK_074 ported chat, and its tool loop already serves account-mode adapters through the
injected-tool protocol. Two gaps remained. A linked web account could reach the chat only
after the user built an account-mode provider by hand on the Providers page. And the
`deepseek-web` family existed in the contract and both hosts but could never generate: its
adapter refused every turn with `account-mode generation is not implemented yet`.

Research over the community proxies on GitHub (qwengate, Qwen2API, deeperseeker, OpenSeek,
ds2api, deepseek-mcp-server and others) settled the second gap. Qwen's web API answers plain
authenticated requests from the user's session, which is what we already send. DeepSeek's
`chat/completion` requires a per-request proof of work (`x-ds-pow-response`,
`DeepSeekHashV1`), its anti-automation control, and every proxy gets past it with a solver.
Account mode sends what the user's own page sends and nothing more, so DeepSeek web cannot be
an account family.

## Scope

**In scope**
- Linked accounts whose family can generate appear in the chat provider picker; picking one
  creates the provider, discovers and selects its models, and switches to it.
- `deepseek-web` removed everywhere: contract enums (Zod and serde), fixtures, the TS and Rust
  hosts (adapter, identity probe, registries), the account gateway allow-lists (Tauri and
  Electron), the renderer labels and base URLs, the tests and the docs.
- A Rust migration deleting leftover `deepseek-web` accounts, their token secrets and their
  providers.
- ADR 0001 and PASSPORT D11 recording the account-mode limits.

**Out of scope**
- Any DeepSeek proof-of-work handling (not done, by decision).
- Changing Qwen's tool protocol to its native `<tool_call>` format (a possible follow-up).

## Definition of done

- [x] A linked Qwen account is selectable in chat without visiting the Providers page
- [x] No `deepseek-web` family remains in the contract, the hosts, the renderer or the tests
- [x] Existing `deepseek-web` rows are removed by a migration, with a test
- [x] ADR 0001 written, PASSPORT D11 added
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Manual check in `pnpm dev:tauri`: link Qwen, pick «Qwen · аккаунт» in chat, ask a question

## Handoff

**Built:**

- Renderer: `ChatStore` loads `accounts.list` and `providers.adapters` (both tolerate failure),
  exposes `accountChoices` (linked accounts whose family is implemented, streams and supports
  account mode, with no provider yet) and `connectAccount`, which runs `providers.create`
  (account mode, `text`, the family's base URL), `providers.probe`, `providers.update` with
  every available model selected, then `providers.get` and selects it. `ChatComposer` lists the
  choices as «Qwen · аккаунт» after the providers.
- Contract: `AdapterFamily` is `openai-compatible | qwen-web | local`, `AccountFamily` is
  `qwen-web`, in `packages/shared/src/ai.ts` and `crates/zvs-contract/src/ai.rs`. Fixtures
  updated; `AccountLinkInput.invalid.json` gained «the removed deepseek family».
- Removed: `deepseekWeb.ts`, `identity/deepseek.ts`, `deepseek_web.rs`, `identity/deepseek.rs`,
  their registry entries and exports, the now-unused `generationNotImplemented` /
  `generation_not_implemented` and `ACCOUNT_GENERATION_PENDING`, and `Payload::nested`.
  `ACCOUNT_ORIGINS` is `["https://chat.qwen.ai"]` in both shells.
- `crates/zvs-host/migrations/0002_remove_deepseek_web.sql` with
  `the_deepseek_cleanup_removes_its_accounts_and_providers_and_keeps_qwen`.
- Tests that needed a second account family now use Qwen, a second Qwen account, or the
  test-only fixture origin `http://127.0.0.1:4100`. DeepSeek-only tests were deleted.
- `docs/adr/0001-account-provider-limitations.md`, PASSPORT D11 and §12, the host and gateway
  READMEs.

**Deviated from plan:**

- The request was to make DeepSeek web chat and call tools. That needs a solver for DeepSeek's
  proof of work, which was declined. On the user's decision DeepSeek web was removed instead,
  and DeepSeek stays available through its official API or OpenRouter.
- The TS host is frozen for the migration, but it was edited too, so both shells agree on the
  contract until TASK_076 deletes it. No Drizzle migration was added: the Electron build is
  being retired and Phase M carries no data across.

**Follow-up fixes (same day, from the manual check):**

- The chat composer hid the whole picker when no usable provider existed, so the account
  choice never showed. The empty state now offers «Подключить Qwen · аккаунт». An existing
  account provider that is disabled, lacks `text` or has no selected models is reused and
  repaired instead of hiding the choice.
- `chat.prepare` built the generation request on the conversation's stored provider, not the
  one selected for the turn (inherited from the TS host), so a conversation created on a
  since-deleted provider failed with «Провайдер не найден», and retry/edit ran on the old
  provider. It now uses the turn's provider, and every turn saves its provider, model and
  settings onto the conversation, so reopening it restores the last choice.
- «Сессия аккаунта истекла» on a valid Qwen session was Alibaba's browser-verification page
  (HTTP 200 HTML) on `chats/new`, which the transport read as signed out. It is now
  `PROVIDER_UNREACHABLE` with `reason: "browser-verification"` and its own message, and the
  account is not marked for re-linking. Passing the verification is out of scope (ADR 0001).

**Next session should know:**

- The manual check has not been run.
- Qwen tool calls still use our `zvs_call_tool` JSON protocol. Qwen2API's ideas (retry a
  tool-bearing turn that ends in bare text, accept a complete call found in the thinking
  channel, reuse one `chatId` with `parentId`) are possible follow-ups.
