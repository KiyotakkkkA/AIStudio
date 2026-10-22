---
task: TASK_057
title: End-to-end suite and the v1.0 release checklist
phase: 10 — Production readiness
status: todo
depends_on: [TASK_056, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_057 — End-to-end suite and the v1.0 release checklist

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The last task. TASK_008 built one smoke test; this expands it into a suite covering the journeys that actually matter, and produces the checklist that gates a release.

The journeys worth automating are the ones that cross layers, because those are where integration breaks: store a secret → configure a provider → discover models; create a vector store → index a folder → search it; hold a chat turn with retrieval and citations; build a scenario → run it → inspect the result; open a document from a citation; start an agentic session and answer an approval.

This task also decides the coverage threshold deferred in TASK_008, now that the shape of the codebase is known and the numbers mean something.

## Why

Without automated journeys, every release is a manual regression pass, and the manual pass gets shorter each time under pressure.

## Depends on

- `TASK_056` — Packaging — Tauri bundler, sidecar, WebView2 runtime, installer.

## Read first

- `/tasks/INDEX.md` — the milestone table lists the journeys worth covering
- `/docs/SECURITY_CHECKLIST.md` from TASK_055

## Scope

**In scope**
- Playwright suite over WebView2 CDP (the `e2e` cargo feature from TASK_075) covering the six journeys above
- Test fixtures: a seeded workspace, a fake provider, a local MCP server, a fixture site for the browser
- Coverage threshold set and enforced
- `/docs/RELEASE.md`: the checklist for cutting a release
- CI running the full suite
- Version 1.0.0 tagged

**Out of scope** (do not do these, even if tempting)
- Visual regression testing — note it as a follow-up if wanted
- Load testing

## Steps

1. Fixtures first: a seeded workspace with known content, a fake AI provider served locally so tests need no vendor credentials, a local MCP server, and a fixture site for the browser journey. Tests that depend on live third-party services are flaky by construction and will be disabled within a month.
2. Write the six journeys as e2e specs. Each asserts real outcomes — a model list actually populated, a citation actually opening the viewer at the right page — not merely that a click did not throw.
3. Stabilise: no arbitrary sleeps, wait on real conditions. Run the suite ten times and fix anything that flakes; a suite people do not trust is worse than none.
4. Coverage: measure what exists, set a threshold that reflects the testing policy (high for services, kernel, repositories and the permission model; low expectations for presentational components), and enforce it in CI.
5. `/docs/RELEASE.md`: version bump, changelog, full gate run, e2e suite, the security checklist from TASK_055, packaging, clean-machine install verification, tag, and what to do if something fails after release.
6. CI: run unit tests on every push, e2e and the full gate set on release branches, per TASK_008's matrix decision.
7. Run the release checklist end to end for real, fix whatever it exposes, and tag `v1.0.0`.

## Verify

```bash
pnpm test
pnpm test:e2e
pnpm typecheck && pnpm lint && pnpm build && pnpm package
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Execute `/docs/RELEASE.md` completely, including the clean-machine install. The checklist is only trustworthy once someone has actually followed it.

## Definition of done

- [ ] Six cross-layer journeys automated and stable over ten consecutive runs
- [ ] No test depends on a live third-party service
- [ ] Coverage threshold set and enforced in CI
- [ ] `/docs/RELEASE.md` written and executed once in full
- [ ] Security checklist re-run and clean
- [ ] v1.0.0 tagged from a clean checkout
- [ ] All gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**

---

**Phase 10 and the backlog are complete when this task is done.** The app is feature-complete, reviewed, packaged, tested and released.
