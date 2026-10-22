---
task: TASK_047
title: Browser agent panel, approvals, rate limiter
phase: 8 — Documents and the agent browser
status: todo
depends_on: [TASK_046, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_047 — Browser agent panel, approvals, rate limiter

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The final piece of the browser and the close of Phase 8: making what the agent does **visible**, which PASSPORT §21 lists as one of the five mitigations that ship together.

`/design/BrowserWindow.dc.html` shows it: the agent's target element outlined in the page and labelled with the action about to be taken (`click · button[type=submit]`); a 344 px right panel with the session's task, an action log where every entry carries its authorisation ("auto", "granted for this site", "approved 20:58"), the per-site rights summary, and Take control / Stop; an approval bar over the page for anything always-ask; and a status bar reading "Agent paused — waiting for your decision".

The rate limiter belongs here too: a cap on actions per minute that trips a pause when exceeded, so a runaway loop stops itself rather than racing through a site.

## Why

An agent acting invisibly in a logged-in browser is the least defensible thing this product could ship. Visibility, instant takeover and a bound on velocity are what make the feature acceptable.

## Depends on

- `TASK_046` — CDP driver, browser.* nodes, policy engine.

## Read first

- `/design/BrowserWindow.dc.html` — the outline and label, the panel, the approval bar, the status bar
- `/docs/PASSPORT.md` §21 (making actions visible, prompt injection)

## Scope

**In scope**
- Target outlining and action labelling in the page, drawn by the host, not the page
- Agent panel: task, action log with authorisation provenance, per-site rights, controls
- Approval bar rendered from the `approval` event
- Take control (suspend the agent, hand over the mouse) and Stop (kill the run)
- Rate limiter with a pause on exceed
- Per-origin grant management and revocation from the panel
- Status bar

**Out of scope** (do not do these, even if tempting)
- New automation primitives — TASK_046 fixed the set
- Recording and replaying browser sessions as macros

## Steps

1. Target outlining: draw the highlight and label as an **overlay owned by the host**, not by injecting elements into the page. On Tauri/WebView2 the outline is CDP's `Overlay.highlightNode`/`Overlay.highlightQuad`, which the browser draws outside the page's DOM. The text label belongs to the `browser-ui` chrome, or to a transparent, non-interactive child webview layered over the site view and positioned by the host from the node's box model. Neither one is reachable from the page. Injected UI can be read or manipulated by the page, and a hostile page that can see what the agent is about to do is a worse position than one that cannot.
2. Agent panel in `browser-ui/`: session task, step position, and the action log. Every entry shows what was done, to what, and **why it was allowed** — auto, granted for this site with the timestamp, or approved at a time. That provenance is the point of the log.
3. Approval bar over the page: rendered from the `approval` event, naming the action and the origin, with Deny / Always on this site / Approve once. No destructive default, no keyboard shortcut that can approve by accident, and 'Always on this site' must never appear for the always-ask tiers.
4. Take control: suspends the agent at the next boundary, detaches automation input, and shows clearly in the status bar that the user has the mouse. Resuming is explicit.
5. Stop: cancels the kernel run immediately through the existing cancellation path.
6. Rate limiter: a configurable cap (default a modest number per minute). On exceed, pause the session, log the reason, and require an explicit resume. Include it in the status bar so the user can see the pace.
7. Per-site rights: the panel lists what is granted for the current origin with the tiers from the mockup, and every grant is revocable from there with immediate effect.
8. States: no session, running, paused by rate limit, blocked on approval, user has control, stopped.
9. Tests: log entries carry correct provenance; the rate limiter trips and pauses; revoking a grant takes effect on the next action; 'always' is never offered for always-ask tiers; take-control genuinely suspends before the next action.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Run a real agent session against a local fixture site. Watch the outline and label track the target. Approve one action, deny another, revoke a grant mid-session and confirm the next action asks again. Trigger the rate limiter with a tight loop and confirm it pauses.

## Definition of done

- [ ] Target outline and label drawn by the host, never injected into the page
- [ ] Every logged action shows its authorisation provenance
- [ ] Approval bar cannot offer 'always' for always-ask tiers
- [ ] Take control suspends before the next action; Stop cancels immediately
- [ ] Rate limiter pauses a runaway session
- [ ] Grants revocable with immediate effect
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**

---

**Phase 8 is complete when this task is done.** Documents can be read in context and the browser can be driven — visibly, reversibly and within a policy the user controls.
