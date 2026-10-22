---
task: TASK_049
title: Mail integration (IMAP/SMTP)
phase: 9 — Integrations
status: todo
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
- [ ] Trigger fires once per message and survives restarts without replay
- [ ] Mail content marked untrusted end to end
- [ ] `mail.send` threads correctly and respects the approval decision
- [ ] Certificate validation never disabled
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
