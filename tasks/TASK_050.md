---
task: TASK_050
title: Telegram and Git hosting integrations
phase: 9 — Integrations
status: todo
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
- GitHub and GitLab providers: repo read, issue and PR tools, webhook or polling triggers
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
3. Git triggers: issue opened or commented, PR opened or updated, push. Prefer webhooks where a public endpoint exists; otherwise poll with an ETag or since-cursor so the app is not rate-limited into uselessness.
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

- [ ] Telegram working per the decision, with deduplicated triggers
- [ ] GitHub and GitLab adapters behind one provider, base URL configurable
- [ ] Write tools default to `ask` and are honoured by the registry
- [ ] Rate limits respected and surfaced
- [ ] All fetched content marked untrusted
- [ ] Tests use fixtures; CI needs no live service
- [ ] Gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
