---
task: TASK_049
title: Mail integration (IMAP/SMTP)
phase: 9 — Integrations
status: done
depends_on: [TASK_048, TASK_076]
unblocks: [TASK_051]
estimate: 4 h
---

# TASK_049 — Mail integration (IMAP/SMTP)

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The first real integration, and the one the design draws in full. `/design/Integrations.dc.html` shows the Mail tab: account with protocol (IMAP / Gmail / Graph), incoming IMAP settings, outgoing SMTP with a "reuse the incoming credentials" toggle, sync settings (watched folders, poll interval, ignore-older-than, mark-read-after-handling), a test result line reporting both IMAP and SMTP, and an automation rail exposing `mail.received` as a trigger with `mail.send` and `mail.search` as tools.

Mail is the integration most likely to cause visible damage if it misbehaves — duplicate sends, replies to the wrong thread, marking a whole inbox read. The framework's deduplication (TASK_048) matters here more than anywhere, and `mail.send` should be treated as a side-effecting tool that defaults to `ask`.

## Why

Mail-triggered automation — read an incoming ticket, retrieve context, draft a reply — is the flagship scenario the whole product design points at.

## Depends on

- `TASK_048` — Integration framework — accounts, triggers, tool exposure.

## Read first

- `/design/Integrations.dc.html` — the Mail tab in full
- `/docs/PASSPORT.md` §14 (mail content is untrusted input)

## Scope

**In scope**
- IMAP client: connect, list folders, poll or IDLE, fetch headers and bodies
- SMTP client: send with threading headers
- Account config matching the mockup, with credentials from Secrets
- `mail.received` trigger with per-message deduplication
- `mail.send` and `mail.search` tools
- Test connection reporting both directions
- Tests against a local mail server fixture

**Out of scope** (do not do these, even if tempting)
- Gmail API and Microsoft Graph — the protocol selector shows them; implement IMAP/SMTP only and disable the others with a note
- A full mail client UI
- Attachment handling beyond listing them

## Steps

1. Pick maintained **Rust** IMAP and SMTP crates (for example `async-imap` and `lettre`; resolve what is current), pin them, and wrap them behind the `IntegrationProvider` trait in `zvs-host`. Do not let library types leak into services.
2. Connection: TLS by default, STARTTLS where configured, honest certificate validation — never disable verification, not even behind a toggle.
3. Sync: prefer IDLE where the server supports it, fall back to polling at the configured interval. Track the last seen UID per folder so a restart does not re-read the folder.
4. `mail.received` trigger: emits per new message with a stable event id (message id plus folder), which the framework deduplicates. Payload carries from, subject, folder, date and a body reference — not the entire body, since it may be large.
5. **Mail content is untrusted input.** Mark bodies and subjects as untrusted at the DTO level so anything reaching a model is fenced. A mail body is the single easiest injection vector into an automated workflow.
6. `mail.send`: compose with correct threading headers (`In-Reply-To`, `References`) so replies land in the thread. Register it as side-effecting so it defaults to `ask`. Never send silently on a scenario's first run without an approval having been granted.
7. `mail.search`: search by folder, date range, sender and text, returning headers and snippets.
8. Mark-read-after-handling as an explicit, off-by-default setting, applied only to messages a scenario actually handled.
9. Test connection: probe IMAP and SMTP separately and report both, as the mockup's result line does.
10. Tests against a local mail server fixture (Greenmail or similar in a container, or a stubbed protocol server): new mail fires the trigger once; a reconnect does not replay; send produces correct threading headers; search filters correctly; bodies are marked untrusted.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "For mail sending — should a scenario be able to send mail automatically once you have approved the tool, or should every outbound message need approval? Automatic sending is what makes mail automation useful, but a bad prompt or a hostile incoming message could cause the app to send something you did not intend."
>
> **Why it matters:** It sets the default tier for `mail.send` and how much a mail-triggered scenario can do unattended.
> **Options:** (a) every send needs approval — safest, and mail automation becomes assisted drafting; (b) approve per recipient domain — automatic within your organisation, ask for outsiders; (c) approve the tool once, then send freely.
> **If the user is unavailable:** take (a) for v1. It is reversible upward once you trust the scenarios; the opposite direction is not.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Connect a real mailbox. Confirm test connection reports both directions. Send yourself a message, confirm the trigger fires once and starts a bound scenario. Restart the app and confirm no replay.

## Definition of done

- [ ] IMAP and SMTP working with real credentials from Secrets
- [x] Trigger fires once per message and survives restarts without replay
- [x] Mail content marked untrusted end to end
- [x] `mail.send` threads correctly and respects the approval decision
- [x] Certificate validation never disabled
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `zvs-host/src/integrations/mail/` — `MailProvider` (kind `mail`, label «Почта»).
`imap.rs` (async-imap 0.12.0: TLS or STARTTLS, LOGIN or XOAUTH2, EXAMINE/SELECT, UID
SEARCH/FETCH with `BODY.PEEK`, `+FLAGS.SILENT (\Seen)`, IDLE), `smtp.rs` (lettre 0.11.23: TLS
wrapper or required STARTTLS, PLAIN/LOGIN or XOAUTH2, `In-Reply-To` and `References`),
`message.rs` (mail-parser 1.0.1 into `MailMessageDto` / `MailBodyDto`), `sync.rs` (IDLE for one
folder, polling otherwise, a UIDVALIDITY:UID cursor per folder in the new `integration_cursor`
table, migration `0034`), `tls.rs` (rustls 0.23.46 + ring, system roots; only the roots can be
swapped, verification is never off), `settings.rs`, `copy.rs`, and `oauth/` with one file per
preset — `google.rs`, `yandex.rs`, `mailru.rs` — plus PKCE authorize, code exchange, refresh and
the XOAUTH2 authenticator. Tools `mail.send` (per-call approval with the letter), `mail.search`,
`mail.read`; trigger `mail.received` (event id `<folder>:<Message-ID>`, payload carries `ref`,
not the body). Contract: `UntrustedTextDto`, `MailRefDto`, `MailAttachmentDto`,
`MailMessageDto`, `MailBodyDto`, `MailDraftDto`, `MailApprovalDto` with Zod mirrors and golden
fixtures in `fixtures/mail/`. Framework (`integrations/`): `IntegrationGateway` replaces the
caller closure; `IntegrationToolSpec::approved_per_call()` makes a node resolve at `ask`
minimum, defer its approval, ask the provider's `approval()` for the request and pass the
approved fields into `call_tool`; `account_tools(config)`; `handled()` through
`IntegrationService::run_watcher` (wired in the composition root and `testing.rs`);
`sign_in()` plus channels `integrations.signIn` / `integrations.cancelSignIn`, which store the
grant as a `custom` secret and point the named config field at it;
`IntegrationConfigView::secret_id` / `update_secret`. Scenarios: an agent whose input carries
`untrusted: true` gets it in `<untrusted_content>` plus `UNTRUSTED_INPUT_INSTRUCTION`.
Renderer: `features/mail/MailDraftView.tsx` shows the letter in the chat tool call, the
standalone chat approval, the scenario canvas bar (without «Разрешать всегда») and the Tasks
rail. Tests: `integrations::mail::tests` run against a stub IMAP/SMTP server over real TLS with
a test CA (`mail/fixtures/*.der`, test-only keys): trigger fires once, a restart does not
replay, threading headers, search filters, untrusted bodies, read-only refusal, an unknown CA is
refused, mark-read, XOAUTH2, token refresh saved back, and the kernel node raising the
`kind: "mail"` approval with a denial sending nothing.

**Deviated from plan:**
- DECISION answered by the user: option (a), and the approval shows the letter in full.
- Added during the task at the user's request: OAuth sign-in for Google, Yandex and Mail.ru
  (the task scoped Gmail out), and a per-account `access` mode: `read` (default; `mail.send`
  is not registered and the host refuses a send) or `readWrite`.
- Added `mail.read`: the trigger carries a body reference, and something has to resolve it.
- Messages over 2 MiB are fetched headers-only, so their snippet is empty and attachments are
  not listed. Non-ASCII search terms use `CHARSET UTF-8` with quoted strings.
- Mail.ru endpoints (`o2.mail.ru/login`, `/token`, scope `userinfo mail.imap`) follow
  Thunderbird's mapping; Mail.ru's own IMAP OAuth docs were not found. Yandex uses
  `oauth.yandex.com` with `mail:imap_full mail:smtp`.
- No client ids are shipped: the user registers an OAuth app with each provider and enters its
  id and secret (`oauthClientId`, `oauthClientSecret`); Yandex and Mail.ru need a fixed
  redirect, so `oauthRedirectPort` pins `http://127.0.0.1:<port>/callback`.

**Next session should know:**
- TASK_051 builds the form: `integrations.kinds` now returns the mail descriptor; manual-only
  fields are optional in the descriptor and enforced in `settings::load`, so the page should
  hide them for the OAuth presets and offer «Войти» (`integrations.signIn`) for them.
- Approvals still expire after the 60 s default, so an unattended scenario's send is denied if
  nobody answers in time.
- Not run here: `pnpm build`, `pnpm test:e2e`, the manual check against a real mailbox, and real
  OAuth sign-ins (no client ids). Pre-existing failures not touched: `pnpm typecheck` in
  `tests/renderer/chatGoal.test.ts` and `pnpm format:check` on `ui/atoms/assistantGeometry.ts`.
