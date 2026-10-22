---
task: TASK_055
title: Security review against PASSPORT §14 and §21
phase: 10 — Production readiness
status: todo
depends_on: [TASK_054, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_055 — Security review against PASSPORT §14 and §21

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

A deliberate review pass rather than a feature. The app holds credentials, executes tools on the user's behalf, drives a logged-in browser, and reads untrusted content from web pages, documents and email. Each of those was built with its own protections; this task verifies they actually hold, together, in the finished product.

The review is against two written standards that already exist: PASSPORT §14 (security) and §21 (the browser's threat model and its five mitigations). Work through them clause by clause and prove each one with a test or a demonstration — not by reading the code and concluding it looks right.

## Why

Every individual protection was tested in isolation by the task that built it. Nobody has yet checked them as a system, and that is where the gaps are.

## Depends on

- `TASK_054` — Performance pass — startup, virtualisation, DB indices.

## Read first

- `/docs/PASSPORT.md` §14 and §21 in full — this task is an audit against them
- `/tasks/TASK_046.md` and `/tasks/TASK_047.md` — the browser's specific claims

## Scope

**In scope**
- A written checklist derived from §14 and §21, with a verdict per clause
- Adversarial tests: prompt injection fixtures, credential exfiltration attempts, path traversal, permission escalation
- Renderer and IPC surface audit: Tauri capabilities, custom commands and webview labels
- Secret handling audit across logs, DTOs, crash files and the diagnostics bundle
- Fixes for whatever is found
- The checklist committed so it can be re-run before each release

**Out of scope** (do not do these, even if tempting)
- External penetration testing
- Threat modelling of new features not yet built

## Steps

1. Write the checklist first, one line per clause in §14 and §21, each with how it will be verified. Commit it as `/docs/SECURITY_CHECKLIST.md` so future releases re-run it.
2. IPC surface audit. There are no preloads any more: what a webview can reach is its capability set. Confirm the `studio` webview reaches only `ipc_call` and `events_subscribe`. The `browser-ui` chrome reaches only `browser_command` and its state channel. Site tabs and the hidden account-gateway webviews (TASK_066) reach **nothing**: no capability lists a remote URL. Every custom command re-checks the caller's webview label. List every `#[tauri::command]` and every capability file, and check that the isolation pattern and CSP in `tauri.conf.json` are unchanged since TASK_058. Confirm the `Runtime.addBinding` function in the gateway webview cannot be reached from any vendor-served script.
3. Secret audit: run the full app through a realistic session, then grep every log file, crash file and diagnostics bundle for planted secret values. Zero hits. Also assert that no IPC response DTO anywhere contains a decrypted value.
4. Prompt injection, done adversarially: build fixture pages, documents and emails containing text that instructs an agent to exfiltrate a secret, disable a permission or call a destructive tool. Run agentic sessions against them. **The expected result is that nothing in the content changes the authorisation path.** Any deviation is a release blocker, not a bug to file.
5. Permission escalation: attempt to widen a narrowed grant through a skill, a scenario and a browser origin grant. Attempt to invoke a tool set to `off`. Attempt to submit a form with an 'always allow' grant present.
6. Path traversal: attempt to read outside an agentic workspace and outside an allowed filesystem root, including through symlinks and `..` sequences in tool arguments.
7. Navigation and CSP: confirm the studio window still blocks navigation and popups, that CSP has not been relaxed anywhere during development, and that no `unsafe-eval` crept in.
8. Dependency review: audit the tree for known advisories and note anything unpatched with its risk.
9. Fix everything found. Where a finding cannot be fixed in this task, record it explicitly in the checklist with a severity and a follow-up task — never leave a finding undocumented.

## Verify

```bash
pnpm --filter studio test
pnpm audit
cargo audit
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: The adversarial suite above, run by hand against the packaged app rather than the dev build — packaging can change CSP, capability resolution and resource loading in ways dev never exercises. Also confirm a release build ignores `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` (TASK_075) and that no `unsafe` block lacks a `// SAFETY:` comment.

## Definition of done

- [ ] `/docs/SECURITY_CHECKLIST.md` committed with a verdict per clause
- [ ] Injection fixtures provably change no authorisation path
- [ ] No secret appears in any log, crash file or diagnostics bundle
- [ ] Escalation and traversal attempts all fail
- [ ] Capabilities verified minimal: studio has two commands, chrome has one, site and gateway webviews have none
- [ ] Every finding either fixed or recorded with severity and a follow-up
- [ ] Gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
