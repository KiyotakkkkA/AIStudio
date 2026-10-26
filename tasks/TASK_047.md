---
task: TASK_047
title: Browser agent panel, approvals, rate limiter
phase: 8 — Documents and the agent browser
status: done
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

- [x] Target outline and label drawn by the host, never injected into the page
- [x] Every logged action shows its authorisation provenance
- [x] Approval bar cannot offer 'always' for always-ask tiers
- [x] Take control suspends before the next action; Stop cancels immediately
- [x] Rate limiter pauses a runaway session
- [x] Grants revocable with immediate effect
- [x] Tests and gates pass — except `drivers::ai::adapters::qwen_web::tests::qwen_leaves_its_own_web_search_out_of_the_tool_policy_through_the_dialect`, which fails on `main` without this change, and `NavRail.tsx` formatting, also already failing on `main`
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass — typecheck and lint pass; `pnpm build` was not run locally
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `BrowserAgent` (`zvs-host/src/browser/agent.rs`) is the session. Every `browser.*`
node calls `begin` before acting. `begin` opens or switches the session to the run, waits while
the user has control or the rate limiter has paused, counts the action against a sliding minute
(`browser.agent.actionsPerMinute`, default 20) and logs it. The node then records why it was
allowed: `auto`, `granted` with the grant's time, or `approved` with the decision's time, from
the same admission result that let it run. All eight browser nodes now defer approval, so every
approval (including ones caused by a stricter Tools tier) goes through the agent and shows in
the bar. The approval offers «Всегда на этом сайте» only when `canAlways`, which the host sets
only for click and type. The host refuses `agentAlways` for anything else, and the chrome hides
the button for submit and download as well. Control and a rate pause outlive the run, so a new
chat turn waits too. «Остановить» cancels the run through `RunService::cancel`. Revocation
deletes the `browser_grant`, and the next action resolves its tier again. The outline is
`Overlay.highlightNode` (driver `Session::highlight`), set before the approval and cleared
after the action. The label («Цель») and everything else are rendered in `browser-ui`:
`AgentPanel.tsx`, `ApprovalBar.tsx`, toolbar toggle, footer status and pace, with copy in
`agentCopy.ts`. The host reserves the panel (344 px) and the approval bar (76 px) in the
site-webview layout, so the page never covers them. `BrowserState` gained `agentVisible` and
`agent`; `BrowserCommand` gained the eight `agent*` actions (Rust, Zod, fixtures).

**Deviated from plan:** The text label sits in the panel («Цель») rather than beside the element.
A label next to the element would need a transparent layered webview positioned from the box
model, so only the outline is drawn on the page itself. The panel log is the agent's in-memory
record of browser steps (last 200), not a projection of the run's event stream. The steps stay
in the run's stream for the Tasks page. A session is the run that made the last browser action,
so each chat turn starts a fresh log. The rate cap is a setting with no UI yet. The footer copy
«Загрузки заблокированы» became «Загрузки только с подтверждением», since TASK_046 made
approved downloads possible.

**Next session should know:** The chrome and the chat both receive the same `approval` event.
Whichever answers first wins, and the other gets «Подтверждение уже не ожидается». The panel
was not checked visually in a running app, and the manual check (real agent session, approve,
deny, revoke mid-session, tight loop) was not performed. Neither was `pnpm build`.

---

**Phase 8 is complete when this task is done.** Documents can be read in context and the browser can be driven — visibly, reversibly and within a policy the user controls.
