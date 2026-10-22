---
task: TASK_052
title: Settings page and the configuration surface
phase: 10 — Production readiness
status: todo
depends_on: [TASK_051, TASK_076]
unblocks: []
estimate: 3 h
---

# TASK_052 — Settings page and the configuration surface

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

Phase 10 turns a feature-complete app into a shippable one. This task collects the settings that accumulated as decisions across the backlog into one coherent page.

Several earlier tasks deliberately deferred a toggle here: provider health-check interval and whether it runs at all (TASK_015), run-history retention (TASK_026), document cache size (TASK_042), agentic budgets (TASK_040), download concurrency (TASK_029), browser rate limit (TASK_047), skill trigger threshold (TASK_035). Rather than inventing a settings model now, go and read the Handoff blocks of those tasks — they record what was actually built and what was left configurable.

## Why

Settings scattered across a dozen defaults are invisible and unchangeable. This is also where the app's data location, backups and reset live.

## Depends on

- `TASK_051` — Integrations page.

## Read first

- `/docs/PASSPORT.md` §15 (config), §9 (paths and backups)
- The Handoff blocks of TASK_015, TASK_026, TASK_029, TASK_035, TASK_040, TASK_042, TASK_047

## Scope

**In scope**
- Settings page with sections: General, AI defaults, Storage, Automation, Security, Advanced
- Typed settings schema in `packages/shared` with defaults and validation
- Data management: database location, backups, export, reset
- About: versions of the app, Tauri, the WebView2 runtime, `zvs-core` and the `zvs-jobd` sidecar
- Migration of existing ad-hoc setting keys into the typed schema

**Out of scope** (do not do these, even if tempting)
- Theming — dark only in v1
- Account or licensing

## Steps

1. Define the settings schema in `packages/shared` (Zod, for the page) **and** in `zvs-contract` and `zvs-host` (serde types plus defaults; the host is what reads and enforces settings), bound by golden fixtures as per TASK_059. Each key has a default and validation, so a corrupt value falls back rather than breaking boot.
2. Migrate the ad-hoc keys written by earlier tasks into the schema, keeping their current values.
3. Sections: General (startup, window behaviour), AI defaults (default text/embedding provider, default generation settings), Storage (database location, document cache ceiling, run retention, backup count), Automation (agentic budgets, download concurrency, browser rate limit, skill trigger threshold, health-check interval), Security (permission defaults, grant management with a revoke-all, browser profile reset), Advanced (log level, event recording toggle, developer tools).
4. Data management: show the database path with a reveal-in-folder action, list backups with restore, export the workspace (database plus settings, excluding decryptable secret values — those are machine-bound), and a reset that requires typing the workspace name to confirm.
5. About: real versions read at runtime, not hardcoded strings.
6. Changing a setting takes effect without a restart wherever possible; where a restart is genuinely required, say so on the control rather than failing quietly.
7. Tests: schema defaults and validation; a corrupt stored value falls back to the default rather than throwing; migration of legacy keys preserves values.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Change every setting, restart, confirm all persisted. Corrupt one value in the database by hand and confirm the app boots with the default rather than failing.

## Definition of done

- [ ] Every deferred setting from earlier tasks is present and effective
- [ ] Typed schema with defaults; corrupt values fall back safely
- [ ] Data management: backups, export and a guarded reset
- [ ] About shows real runtime versions
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
