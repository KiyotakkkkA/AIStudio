---
task: TASK_053
title: Error handling, recovery screens, local crash capture
phase: 10 — Production readiness
status: todo
depends_on: [TASK_052, TASK_076]
unblocks: []
estimate: 3 h
---

# TASK_053 — Error handling, recovery screens, local crash capture

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The app handles errors per-feature. This task makes failure coherent across the whole product and ensures a crash produces something diagnosable rather than a silent disappearance.

There are three layers. **Expected errors** already flow as coded `Result` envelopes (TASK_004) and map to copy — audit that every code has copy and that no raw host message reaches a user. **Unexpected renderer errors** need error boundaries so one broken component does not blank the window. **Host crashes and unhandled rejections** need capture to a local crash file with enough context to diagnose, and nothing leaving the machine, per PASSPORT §14.

## Why

A local-first app with no telemetry has exactly one diagnostic channel: what it writes to disk. If a crash leaves nothing behind, a user-reported bug is unfixable.

## Depends on

- `TASK_052` — Settings page and the configuration surface.

## Read first

- `/docs/PASSPORT.md` §7 (error model), §14 (no telemetry), §15 (logging)

## Scope

**In scope**
- Audit and completion of `AppErrorCode` → copy coverage
- React error boundaries per route and around the viewer and browser panels
- Global failure capture in the Rust host (panic hook, failed tokio tasks) and for the WebView2 renderer process
- Crash file capture with context, redacted
- Recovery screens: migration failure (from TASK_006, re-implemented as a Tauri recovery window in TASK_061), corrupt settings, missing native artefact, sidecar unavailable
- A diagnostics bundle action the user can attach to a bug report

**Out of scope** (do not do these, even if tempting)
- Remote crash reporting — explicitly out of scope
- Automatic recovery beyond what exists

## Steps

1. Audit every `AppErrorCode` for user-facing copy. Any code without copy renders a generic message today — find them and fix them. Add a test asserting the map is exhaustive so a new code cannot ship without copy.
2. Error boundaries: one per route, plus dedicated ones around the viewer dock and the browser agent panel, since those are the most likely to fail on unexpected content. A boundary shows what failed and offers reload-this-view, not a blank page.
3. Host: install a `std::panic::set_hook` that logs fully and writes a crash file. Surface panics in spawned tasks through their `JoinHandle` (the IPC server and the core ports already contain per-call panics, since TASK_060 and TASK_068). Handle WebView2's `ProcessFailed` event for the studio webview: write a crash file and offer a reload instead of a white window. Decide deliberately whether to continue or quit. Silently continuing after an unknown failure is how corrupt state happens.
4. Crash file: timestamp, app and dependency versions, OS, the error and stack, recent log lines, and the last few kernel run ids. Run it through the log redactor from TASK_010 before writing, and test that redaction with a realistic secret in the payload.
5. Recovery screens for the four states above, each naming the problem, the file or path involved, and a concrete action.
6. Diagnostics bundle: a Settings action that zips recent logs, crash files and the settings (redacted, secrets excluded) into a file the user can attach to a report. Show them exactly what is in it before writing.
7. Tests: error-copy map is exhaustive; a thrown component error is caught by its boundary and the rest of the app keeps working; crash file redaction removes a planted secret; each recovery screen renders from its trigger condition.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Throw deliberately inside a page component and confirm only that route's boundary trips. Delete the `zvs-jobd` sidecar binary and confirm the missing-artefact recovery screen names the path and the build command. Generate a diagnostics bundle and read it — confirm no secret is in it.

## Definition of done

- [ ] Every error code has copy; exhaustiveness enforced by test
- [ ] Error boundaries isolate failures per route and per panel
- [ ] Host crashes write a redacted crash file with real context
- [ ] Four recovery screens implemented and reachable
- [ ] Diagnostics bundle contains no secrets — verified by hand
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
