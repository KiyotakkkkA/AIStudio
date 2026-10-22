---
task: TASK_056
title: Packaging — Tauri bundler, sidecar, WebView2 runtime, installer
phase: 10 — Production readiness
status: todo
depends_on: [TASK_055, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_056 — Packaging — Tauri bundler, sidecar, WebView2 runtime, installer

> **Rewritten 2026-10-10 for the Tauri migration (Phase M, TASK_058 – TASK_076).** The
> Electron version of this task (electron-builder, napi addon, `better-sqlite3` rebuild) no
> longer applies. Host code is Rust, so the gates include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

## Context

This task turns the built app into something installable. After Phase M, the studio is a
single Tauri 2 executable (`zvs-studio`). The host is compiled in, SQLite is bundled into it
through `rusqlite`'s `bundled` feature, and `zvs-core` is linked directly. No napi addon and no
native module rebuild are left. The things that can still go wrong are these:
- **The sidecar.** `zvs-jobd` must ship as a Tauri `bundle.externalBin`, named with the
  target triple at build time (`binaries/zvs-jobd-x86_64-pc-windows-msvc.exe`) and placed next
  to the main executable at install time. `StudioPaths::sidecar_path` must find it there.
  This is the most likely packaged-only failure.
- **The WebView2 runtime.** It ships with Windows 11 and current Windows 10, but not with
  every machine. The bundler's `webviewInstallMode` decides what happens on a machine without
  it.
- **The frontend assets.** `dist/index.html`, `dist/browser-ui/index.html` and
  `dist/recovery.html` must be embedded, and CSP and the isolation pattern must behave the
  same packaged as in `tauri dev`.
- **First run.** On an empty `%APPDATA%\<identifier>`, the app must migrate a new database,
  create its folders and land on a sensible page.

PASSPORT §18 assumption 3 says Windows is the only verified target for v1, and §16 says
signing and auto-update are out of scope. The decision below confirms both. An unsigned
installer shows a SmartScreen warning.

## Why

The app is not shippable until it installs and runs on a machine that never had the source.

## Depends on

- `TASK_055` — Security review against PASSPORT §14 and §21.
- `TASK_076` — the Tauri cutover. There is one shell to package.

## Read first

- `/docs/PASSPORT.md` §16 (build and packaging, Tauri), §18 assumptions 2 and 3
- Handoffs of TASK_058 (identifier, versions), TASK_068 (sidecar location) and TASK_075
  (release env hardening)
- `apps/studio/src-tauri/tauri.conf.json`

## Scope

**In scope**
- `tauri.conf.json` `bundle` section: product name, identifier (already fixed in TASK_058),
  version, icons, NSIS target with `installMode: currentUser` (no elevation), and
  `webviewInstallMode` per the decision
- `externalBin` for `zvs-jobd`, with a build step that copies the release binary to the
  triple-suffixed name before `tauri build`
- `pnpm package`: one command from a clean checkout. It builds the sidecar, then the web
  assets, then runs `tauri build` (bundle).
- Resource check: the packaged app finds the sidecar, the embedded frontend and the recovery
  page
- Install-and-run verification on a clean Windows VM
- `/docs/RELEASE_BUILD.md`: the release build process

**Out of scope** (do not do these, even if tempting)
- Auto-update, unless the decision changes it
- macOS and Linux bundles beyond keeping the config extensible
- Store distribution (MSIX, winget)

## Steps

1. Configure the bundle: product metadata, icons generated with `tauri icon`, and NSIS
   per-user install.
2. Sidecar: the copy-and-rename step, `externalBin`, and a packaged-build test that
   `system.demoJob` completes, which proves the sidecar spawned.
3. WebView2 install mode per the decision. Test on a VM with WebView2 removed, or a Windows
   10 image without it.
4. `pnpm package` from a clean clone, wired into Turbo.
5. Clean-VM verification with no Node, no Rust and no build tools. Install, launch, add a
   provider, run a chat turn, index a small folder, open the browser workspace and log into a
   site, quit, relaunch, and confirm the login persisted.
6. First run on an empty data directory lands on a sensible page with no error.
7. Uninstall leaves user data in place unless the user ticks removal, and the documentation
   says so.
8. Write `/docs/RELEASE_BUILD.md`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Three packaging questions. Should the Windows installer be code-signed? Unsigned means
> everyone who installs it sees a SmartScreen warning, and a certificate costs money and needs
> a place to live in CI. Do you want auto-update (the Tauri updater plugin over GitHub
> Releases), or manual distribution? And on machines without the WebView2 runtime, should the
> installer download it (small installer, needs internet), embed the bootstrapper, or bundle
> the full offline runtime (~130 MB more)?"
>
> **Why it matters:** signing changes the build and needs secret handling in CI. Auto-update
> needs an update key pair and a release feed. The WebView2 mode trades installer size against
> offline installs.
> **Options:** signing — (a) unsigned for v1; (b) signed, you supply the certificate. Update —
> (c) manual; (d) updater plugin from GitHub Releases. WebView2 — (e) `downloadBootstrapper`;
> (f) `embedBootstrapper`; (g) `offlineInstaller`.
> **If the user is unavailable:** take (a), (c) and (f), matching PASSPORT §18 assumption 2,
> and note signing and updates as follow-up tasks.

## Verify

```bash
pnpm package
pnpm typecheck && pnpm lint && pnpm build && pnpm test
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: the clean-VM install and smoke run from the steps. Do not skip this. It is the
only way to catch packaging-layout bugs.

## Definition of done

- [ ] One command produces an NSIS installer from a clean checkout
- [ ] Installs per-user and runs on a machine with no development toolchain
- [ ] The sidecar spawns from the packaged layout, and the WebView2 runtime is handled per the
      decision
- [ ] First run on a fresh machine creates its data directory cleanly
- [ ] Release build process documented
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
