---
task: TASK_046
title: CDP driver, browser.* nodes, policy engine
phase: 8 — Documents and the agent browser
status: todo
depends_on: [TASK_045, TASK_066, TASK_076]
unblocks: [TASK_047]
estimate: 4 h
---

# TASK_046 — CDP driver, browser.* nodes, policy engine

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

This task lets the agent drive the browser — and it is where the product's security posture is either real or theatre.

Automation goes through the Chrome DevTools Protocol that WebView2 exposes on each site tab's webview (`CallDevToolsProtocolMethod` and `GetDevToolsProtocolEventReceiver`, reached through Tauri's `with_webview`). TASK_066 already built a small typed CDP client, `zvs-studio::browser::cdp`, for the account gateway; this task extends it, it does not write a second one. CDP only works this way on Windows (WebView2). The macOS and Linux webviews have no CDP, which is acceptable because Windows is the only verified target (PASSPORT §18.3). PASSPORT §21 specifies a **small, audited set of primitives** and says everything else is deliberately absent; implement exactly that table and nothing more. The tiers are: navigate, read_page, screenshot and find are `auto`; click and type are granted **per origin**; submit and download **always ask, every time, regardless of prior grants**; and credential and payment fields are **never** typed by the agent, enforced in the driver by input type and field heuristics.

`read_page` returns the **accessibility tree, not raw DOM** — smaller, and materially less injectable. Grants are stored per origin, not per session, and compose with the global tool ceiling from TASK_031: the browser can never exceed what the Tools page allows, only narrow it.

Treat everything the page contains as hostile input. A page can carry text aimed at the agent, and the mitigations in §21 ship together because none is sufficient alone.

## Why

This is what makes the browser window useful, and it is the single largest attack surface the app has. Building the policy engine with the driver — rather than after it — is what keeps them consistent.

## Depends on

- `TASK_045` — the attached browser workspace (re-implemented on Tauri child webviews in TASK_065).
- `TASK_066` — the typed CDP client over WebView2.

## Read first

- `/docs/PASSPORT.md` §21 (the primitive table, tiers, prompt-injection section) — implement it exactly
- `/docs/PASSPORT.md` §14, §11 (browser actions are kernel steps)
- `/design/BrowserWindow.dc.html` — the target outline and action labelling

## Scope

**In scope**
- CDP driver in `apps/studio/src-tauri/src/browser/automation.rs` over the TASK_066 CDP client, behind an `AutomationPort` that `crates/zvs-host` (policy, nodes) depends on, so the policy engine stays testable without a webview
- The exact primitive set from §21 and no more
- Accessibility-tree reading with element handles
- Policy engine: per-origin grants, tier composition with the tool ceiling
- Credential-field refusal by input type and heuristics
- `browser.*` kernel nodes registering into the TASK_031 registry
- Blocked schemes and navigation policy
- Tests including refusal cases

**Out of scope** (do not do these, even if tempting)
- The agent panel UI and the rate limiter (TASK_047)
- Any primitive not in §21's table — do not add `evaluate`, `execute_script` or arbitrary DOM injection, however convenient
- Cross-tab orchestration beyond acting on the active tab

## Steps

1. Open a CDP session on a tab's webview with a clear lifecycle: enable the needed domains on the first automated action, disable them when the session ends or the tab closes. Handle the case where DevTools is already open on that webview in a debug build.
2. Implement exactly: `navigate` (blocking `file:`, `data:`, `javascript:` schemes), `read_page` (accessibility tree with stable element handles), `screenshot`, `find`, `click`, `type`, `submit`, `download`. Nothing else.
3. Element handles: return opaque ids from `find`/`read_page` that the driver maps to backend node ids. The model never supplies a raw selector that the driver executes blindly.
4. Credential refusal: before typing, inspect the target's type and attributes — `type=password`, autocomplete tokens for credentials or payment, and name heuristics. Refuse with a specific error. Test this with several real-world field shapes, because it is the rule most likely to be quietly wrong.
5. Policy engine: `browser_grant` table keyed by origin and capability. Resolution composes the origin grant with the global ceiling using the most-restrictive-wins resolver from TASK_031/023 — reuse it, do not write a third one.
6. Always-ask tiers: submit and download request approval **every time**, ignoring any stored grant. Make that impossible to bypass by putting it in the resolver rather than in a caller.
7. Register `browser.*` nodes into the tool registry so they appear on the Tools page with their tiers and can be disabled globally there.
8. Untrusted-input fencing: everything returned from a page is marked untrusted at the DTO level, exactly as document text is (TASK_042). A tool call the model proposes afterwards is authorised by the policy engine alone, never by anything the page said.
9. Tests: each primitive works against a local fixture page; blocked schemes are refused; a password field refuses typing across several field shapes; per-origin grants are honoured and do not leak to other origins; submit asks even with a grant present; disabling `browser.click` on the Tools page blocks it immediately; page-supplied text never alters the authorisation path (write this test explicitly — feed a fixture page containing instruction-shaped text and assert nothing changes).

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Against a local fixture site: read the page, click a button, type into a text field, then try to type into a password field and confirm refusal. Grant clicks for the origin, confirm they stop asking; then try a form submit and confirm it still asks.

## Definition of done

- [ ] Only §21's primitives exist — no script evaluation, no raw selector execution
- [ ] Accessibility tree used for reading, not raw DOM
- [ ] Credential and payment fields refused, tested against several field shapes
- [ ] Per-origin grants honoured; submit and download always ask regardless
- [ ] Composition with the tool ceiling reuses the existing resolver
- [ ] Page content fenced as untrusted; an injection fixture provably changes nothing
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
