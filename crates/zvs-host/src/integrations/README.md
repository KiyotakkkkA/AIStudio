# integrations

Plugins («Плагины») that connect an external account: Telegram bots, mail (IMAP/SMTP) and Git
forges (GitHub, GitLab). Each plugin contributes _triggers_ (events that can start a scenario)
and _tools_ (calls the model or a scenario can make). Accounts, workers, event dedup and the
activity journal are shared; the plugin only implements `IntegrationProvider`.

## Files

| Path         | Contents                                                                                           |
| ------------ | -------------------------------------------------------------------------------------------------- |
| `mod.rs`     | `IntegrationProvider`, `IntegrationToolSpec`, `IntegrationSession`, events, credential-field rules |
| `service.rs` | `IntegrationService`: accounts CRUD, enable/disable, test, sign-in, workers, `dispatch`, journal   |
| `tools.rs`   | Kernel nodes for plugin tools, per-account tool keys, the `IntegrationGateway` port                |
| `oauth.rs`   | Browser sign-in for plugins that use OAuth (loopback redirect, PKCE)                               |
| `common.rs`  | Field descriptor helpers, `clip` for untrusted text                                                |
| `telegram/`  | Bot API client (`getUpdates` long polling, send message/file), chat allow-list                     |
| `mail/`      | IMAP (with IDLE) and SMTP clients, message parsing, sync cursors, TLS, a local test server         |
| `git/`       | GitHub/GitLab REST client with rate-limit and ETag ledger, polling, repository discovery           |
| `stub.rs`    | Test-only HTTP stub server                                                                         |

## The provider contract

```
kind, label, describe() → form fields, instructions()
triggers() / tools() / account_tools(config)
test(config)                           – check credentials without starting
start(session) / stop(account)         – long-running worker per enabled account
call_tool(call)                        – execute one tool call
approval(call) / handled(call)         – per-call approval text and post-call bookkeeping
sign_in(request)                       – optional browser sign-in
```

Credentials are never plain fields: any field whose key looks like a credential (`password`,
`token`, `secret`, `apikey`, `privatekey`, `credential`, …) must be a reference to a secret
in the secret store (`CREDENTIAL_NOT_A_SECRET`). Required fields are validated with the
field's Russian label in the message (`required_field`).

## Workers

Each enabled account runs `provider.start(session)` under the shared `WorkerSupervisor`
(`../workers/README.md`), keyed by `worker_key(account_id)`. A permanent fault (bad config,
unknown kind) stops the worker; a transient one restarts it with backoff (1 s doubling up to
5 min, reset after 30 s of stable running). The session gives the worker its config, a
cursor store (`cursor` / `set_cursor`), the activity journal and `emit(event)`.

## Pipeline: an incoming event

1. The worker emits an `IntegrationEvent { trigger, event_id, payload }`.
2. `dispatch` checks the trigger belongs to the plugin and the id is non-empty and bounded.
3. `record_event` stores `(account, trigger, event_id)`; a repeat is counted as a duplicate and
   dropped, so a restart or a re-poll never starts the same scenario twice. Event records are
   pruned after 30 days (`EVENT_RETENTION_MS`).
4. The trigger sink starts every active scenario subscribed to that trigger; the journal
   records which scenarios started or the error.

Payload text from outside (message bodies, issue titles) is wrapped as `UntrustedTextDto` and
clipped, so it reaches the model marked as data, never as instructions.

## Plugins

**Telegram** — trigger `telegram.message`; tools `telegram.sendMessage`, `telegram.sendFile`,
`telegram.recentMessages`. The worker long-polls `getUpdates` and keeps the update `offset` as
a cursor. `allowedChats` (ids or usernames) limits which chats can trigger anything; an empty
list allows all.

**Mail** — trigger on new messages; tools `mail.send`, `mail.search`, `mail.read`. Sync per
folder uses a cursor `UIDVALIDITY:highest UID`: with a valid cursor it fetches `UID last+1:*`;
when the server's UIDVALIDITY changed (or on first run) it starts from unseen messages since a
time horizon, so a mailbox rebuild never replays old mail. Fetches are batched. With the `IDLE`
capability the worker waits for pushes and re-issues IDLE every 25 minutes
(`IDLE_REFRESH`); otherwise it polls.

**Git (GitHub, GitLab)** — tools `readFile`, `listIssues`, `createIssue`, `comment`,
`createPullRequest`; triggers for opened issues, issue comments and opened pull/merge requests, by polling.
The client keeps, per account, the remaining rate limit and the reset time (a call during a
limit fails fast with `RATE_LIMITED` and the wait) and an ETag per URL so unchanged polls cost
a `304`. Its tools follow the tool tier like any other tool.

## Tool permissions

Each plugin tool is a kernel node keyed per account (`account_tool_key`). By default it is
gated by the tool's tier on the «Инструменты» page. A spec marked `approved_per_call`
(`mail.send`) always asks: the node raises the tier to `Ask` and defers the approval to its own
`run`, so the request can show the concrete recipient and text. `side_effect` marks tools that
must not be replayed on recovery.
