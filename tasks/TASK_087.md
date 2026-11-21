---
task: TASK_087
title: Add shared AI Studio resources and rename the Desktop app
phase: C — Shared dialogs and standalone CLI
status: todo
depends_on: [TASK_086]
unblocks: [TASK_088]
estimate: 1–2 engineer-weeks
---

# TASK_087 — Shared AI Studio directory and Desktop rename

## Context

Studio currently resolves one writable state root through `StudioPaths`. The shipped app,
browser profile, skills, caches and provider account metadata follow that layout. Desktop and
CLI will each have their own boot path and need a common location for resources they share.

Create sibling state roots named `AI Studio Desktop` and `AI Studio` under the existing vendor
base in local application data. Define `AI Studio CLI` as the third sibling for TASK_088.
Rename the desktop product to **AI Studio Desktop** across its public surfaces.

## Why

The CLI should be able to initialize shared resources and sign in without Studio installed.
Desktop upgrades and uninstall must also be able to distinguish shared data from Desktop
profiles, settings and logs.

## Depends on

- `TASK_086` — versioned Desktop boot layout and the shared dialog storage API.

## Read first

- `AGENTS.md`, `CONTRIBUTING.md`, `CONVENTIONS.md`, `docs/PASSPORT.md` and `docs/FILES_STRUCTURE.md`.
- `crates/zvs-host/src/platform/paths.rs`, Desktop boot and migration modules,
  `apps/studio/src-tauri/tauri.conf.json` and the installer, launcher and packaging scripts.
- Host secrets, provider identity and token refresh, skills discovery and CLI install paths.

## Scope

**In scope**

- Shared path resolution and an idempotent initializer callable by either executable without
  Tauri. Resolve the Windows base through platform APIs. Preserve supported overrides and
  isolate tests from real user roots.
- The common `AI Studio` root for skills, shared caches and metadata, provider account
  metadata and credential references, and CLI–Desktop link metadata. Document an owner and
  cleanup policy for each shared path. Record dialog-store locations where discovery needs
  them. Regular dialogs stay in the owning app's state root.
- A common OS-backed credential interface for shareable providers. Keep tokens, API keys and
  refresh secrets in secure storage. Coordinate refresh across processes. Keep Copilot
  credentials with its vendor client and browser sessions in the Desktop profile. Verify
  which existing encrypted credentials can be reused through the common interface.
- Versioned boot relocation from the current layout. Preserve supported settings, JSONL
  dialogs, skills, profiles and provider identities. Validate any credential handoff before
  retiring its original reference. Handle existing destination roots and interrupted moves.
- Rename the product in packaging and user-facing Desktop entry points. Verify installed
  upgrades, shortcuts, CLI lookup, tray and protocol activation. Preserve stable application
  identifiers where compatible with the renamed product and state layout.

**Out of scope**

- Building the standalone CLI. TASK_088 adds its executable, private boot path and UI.
- Legacy SQLite dialog compatibility. TASK_086 defines the dialog cutover.
- Changes to provider protocols, browser session ownership or unrelated cache behavior.

## Steps

1. Inventory paths, product identity and readers. Define shared versus Desktop ownership
   and update the Passport and file-structure docs.
2. Implement shared root discovery, versioning and boot initialization. Add tests for
   first creation and concurrent initialization.
3. Implement and test relocation and credential access from a Tauri-free process.
4. Update the desktop display name and installer integration. Test a clean install, upgrade,
   interrupted relocation and startup with an existing shared root.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check && pnpm build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check on Windows: upgrade an existing installation and confirm the app appears as
AI Studio Desktop, opens its JSONL dialogs, loads shared skills and can still use its accounts.
Run the shared initializer independently and verify it resolves the same resources.

## Definition of done

- [ ] Desktop and shared roots have explicit owners and versioned, restart-safe boot handling.
- [ ] The public product name is AI Studio Desktop and upgrade and launch entry points work.
- [ ] Shared skills and eligible provider sign-ins are accessible through common code.
  Secrets and Desktop browser profiles retain their storage boundaries.
- [ ] Verification passes, Handoff is filled in, status is set to `done` and `INDEX.md` is ticked.

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
