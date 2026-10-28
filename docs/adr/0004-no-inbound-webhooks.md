# ADR 0004 — No inbound webhooks: integrations receive events through background workers

- **Status:** accepted, 2026-10-28
- **Task:** TASK_048 (integration framework)
- **Supersedes:** the Webhooks tab of `/design/Integrations.dc.html`, TASK_048's optional
  Webhooks scope, TASK_050's "prefer webhooks" step and TASK_051's Webhooks stub

## Context

The Integrations canvas draws four tabs: Mail, Telegram, Git hosting and Webhooks. A webhook
needs a public HTTPS endpoint that the remote service can reach. ZVS AI Studio is a desktop
application on a user's machine: it has no public address, sits behind NAT and a firewall,
and is not running when the machine is off. Exposing it would mean a tunnel or relay service,
a listener that accepts unauthenticated traffic from the internet, and a signing secret per
sender. That is a server product, not a local studio, and every inbound request would be
untrusted input aimed straight at a scenario.

Every integration the product plans has a pull alternative: IMAP IDLE or polling for mail,
Bot API `getUpdates` long polling for Telegram, and ETag or since-cursor polling for GitHub
and GitLab.

## Decision

1. **The app does not receive webhooks.** There is no Webhooks tab, no HTTP listener for
   remote services, no tunnel and no relay. `IntegrationKind` is `mail | telegram | git`.
2. **Inbound events come from background workers.** `zvs-host::workers::WorkerSupervisor`
   runs named long-lived tokio tasks: long polling, IDLE, interval polling. Each worker runs
   in isolation. When it fails or panics it is restarted with exponential backoff (1 s
   doubling to 5 min, reset after 30 s of healthy running). A `WorkerFault::permanent` error,
   a configuration problem retrying cannot fix, stops it as `failed`. Workers stop on
   cancellation, and one that ignores cancellation is aborted after a 5 s grace period. Their
   state is listed through `workers.list` and a worker is restarted with `workers.restart`.
3. **An integration account is a supervised worker.** `IntegrationProvider::start` runs as
   the body of the worker `integration:<accountId>` and emits typed events through
   `IntegrationSession::emit`. The dispatcher deduplicates by `(account, trigger, eventId)`
   before any scenario starts.
4. **Git hosting triggers poll.** TASK_050 polls with an ETag or since-cursor and respects
   rate limits; it does not offer a webhook mode.

## Consequences

- Event latency is the poll or long-poll interval, not push latency. Telegram long polling and
  IMAP IDLE are close to push; Git polling is not, and the interval is the user's choice
  within the service's rate limit.
- Events are only received while the app runs. A provider keeps a cursor (last update id,
  last UID, ETag), so a restart resumes where it stopped, and the event-id deduplication keeps
  a replayed window from starting the same work twice.
- The scenario catalogue still contains `trigger.webhook` from TASK_037. Nothing fires it. Removing
  it is a scenario-editor change and is left to a separate task.
- If a webhook is ever needed, it has to be its own decision with its own security review:
  a relay, sender authentication and untrusted-input handling. It is not a variant of an
  existing integration.
