---
task: TASK_054
title: Performance pass — startup, virtualisation, DB indices
phase: 10 — Production readiness
status: todo
depends_on: [TASK_053, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_054 — Performance pass — startup, virtualisation, DB indices

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

Performance work has been deferred throughout the backlog on purpose: optimising before the shape is known wastes effort. Now the shape is known. This task measures first, then fixes what the measurements justify — not what intuition suggests.

The likely candidates, in rough order of user impact: cold start time (migrations, service construction, WebView2 environment creation, first render); long lists (conversations, runs, tools, documents, models) that were built without virtualisation; chat streaming re-render cost under a fast model; database queries that were written for correctness and never indexed; and the document viewer's page rendering at high zoom.

## Why

A desktop app is judged on how it feels within the first ten seconds, and none of the earlier tasks were allowed to spend time on that.

## Depends on

- `TASK_053` — Error handling, recovery screens, local crash capture.

## Read first

- `/docs/PASSPORT.md` §16
- The Handoff blocks of TASK_026, TASK_030 and TASK_034 — those list the biggest lists in the app

## Scope

**In scope**
- Measurement harness and recorded baselines
- Startup profiling and fixes: lazy service construction, deferred non-critical work
- Virtualisation for every long list, using the kit's `@tanstack/react-virtual`
- Query analysis and indices where `EXPLAIN QUERY PLAN` shows a scan
- Streaming render cost
- Bundle size review and code splitting
- A documented performance budget

**Out of scope** (do not do these, even if tempting)
- Rust-side optimisation unless a measurement points there
- Speculative caching

## Steps

1. Build the measurement harness first: startup timing broken into phases, a way to seed a large synthetic workspace (thousands of runs, hundreds of conversations, a large vector store), and a repeatable way to record numbers. **Record baselines before changing anything** — without them there is no way to know whether a change helped.
2. Startup: measure each phase. Defer everything not needed for first paint — health checks, MCP servers set to start at launch, catalogue refreshes. Construct services lazily where construction is expensive. Target a specific number and write it into the budget.
3. Virtualise every list that can grow unbounded. The kit already bundles `@tanstack/react-virtual`, so there is no new dependency.
4. Streaming: profile a fast model's token stream. If rendering is the bottleneck, batch harder; the batching logic from TASK_025 is the place to tune.
5. Database: run `EXPLAIN QUERY PLAN` over the queries the hot pages issue. Add indices where a scan shows up on a table that grows — runs, steps, messages and vector_document above all. Re-measure; an index that does not change the plan is deleted, not kept for comfort.
6. Bundle: review what the renderer ships, confirm route-level code splitting is effective, and check whether `shiki` and `recharts` are being pulled into the initial chunk when they are only needed on some routes.
7. Write the performance budget into `/docs/` — cold start, route transition, first token, list scroll — and add a CI check for whatever is cheaply measurable.
8. Tests: the seeded-workspace harness is reusable and committed; no functional regressions from any change.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: With a large seeded workspace: cold start, switch between all thirteen routes, scroll a 5000-run history, stream a long chat answer, and open a 200-page PDF. Note anything that feels slow and compare it against the recorded numbers — trust the measurements over the feeling, but investigate any disagreement.

## Definition of done

- [ ] Baselines recorded before and after, with numbers in the Handoff
- [ ] Startup improved against a stated target
- [ ] Every unbounded list virtualised
- [ ] Indices added only where a query plan justified them
- [ ] Performance budget documented
- [ ] No functional regressions
- [ ] Gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
