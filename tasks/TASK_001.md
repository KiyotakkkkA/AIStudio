---
task: TASK_001
title: Workspace skeleton, tooling and CI
phase: 0 — Foundation
status: done
depends_on: []
unblocks: [TASK_002, TASK_003]
estimate: 3 h
---

# TASK_001 — Workspace skeleton, tooling and CI

## Context

The repository currently contains only `/design` (fifteen UI mockups as `.dc.html` files),
`/docs/PASSPORT.md` (the binding architecture document), `/references` (inspiration
screenshots) and `/tasks` (this backlog). **There is no code yet.** This task creates the
monorepo that everything else lands in.

ZVS AI Studio is a local-first desktop app — Electron + React + TypeScript on the outside,
Rust for heavy work (indexing, OCR, vector search) on the inside. The architecture is built
around three domains that must never leak into each other: `host` (Electron main process),
`renderer` (React UI) and `shared` (Zod DTOs and the IPC contract, imported by both, importing
neither). That boundary is the single most important structural rule in the project, and this
task makes the tooling enforce it rather than leaving it to discipline.

Verified toolchain on the development machine: Node 26.7.0, pnpm 12.3.4, cargo 1.93.1. Target
OS for v1 is Windows; the layout must not assume it.

## Why

Every later task assumes a workspace where `pnpm typecheck`, `pnpm lint` and `pnpm build`
exist and mean something. Setting the boundary lint rules now costs an hour; retrofitting them
after fifty tasks of accidental cross-imports costs days.

## Depends on

Nothing. This is the first task.

## Read first

- `/docs/PASSPORT.md` §4 (the three domains and the import rule), §5 (repo layout), §17
  (naming conventions). The tree in §5 is the target — build it exactly.

## Scope

**In scope**
- pnpm workspace, Turborepo, root `package.json` with the standard scripts
- Shared TypeScript config package and shared ESLint config package
- Boundary lint rules that fail the build on a cross-domain import
- Prettier, `.editorconfig`, `.gitignore`, `.npmrc`
- Cargo workspace manifest (empty members list is fine — TASK_017 fills it)
- GitHub Actions workflow running the gates
- `git init`, first commit

**Out of scope**
- Any Electron code (TASK_002)
- Any React code (TASK_007)
- Any Rust crates (TASK_017)
- Package installs beyond tooling — do not add React, Electron or Drizzle yet

## Steps

1. `git init`. Create `.gitignore` covering `node_modules`, `dist`, `out`, `target`,
   `*.node`, `.turbo`, `.env*`, `coverage`.
2. Create `pnpm-workspace.yaml` with `apps/*` and `packages/*`.
3. Root `package.json`: `private: true`, `packageManager` pinned to the installed pnpm,
   `"type": "module"`, and scripts that delegate to Turbo:
   `dev`, `build`, `typecheck`, `lint`, `format`, `test`.
4. `turbo.json` with tasks `build` (depends on `^build`, outputs `dist/**`), `typecheck`,
   `lint`, `test`. Nothing cached that shouldn't be.
5. `packages/tsconfig/` exporting `base.json` — `strict: true`,
   `verbatimModuleSyntax: true`, `moduleResolution: "bundler"`, `noUncheckedIndexedAccess:
   true`, `target: "ES2023"`, `isolatedModules: true`, no `any` escape hatches enabled.
6. `packages/eslint-config/` exporting a flat config with `typescript-eslint`, and the
   boundary rules — this is the important part:
   - `packages/shared` may not import from `apps/**`. Enforce with
     `import/no-restricted-paths` (or `eslint-plugin-boundaries`, your call).
   - `src/renderer/**` may not import `src/host/**`, and `src/host/**` may not import
     `src/renderer/**`.
   - Only `src/host/data/repositories/**` may import the Drizzle client. Add the rule now
     with a path that does not exist yet; it will start biting in TASK_006.
   - `src/renderer/ui/**` may not import from `src/renderer/features/**` or `stores/**`.
   Write a one-line comment above each rule saying *why*, so a future reader does not
   delete it as noise.
7. Prettier config (no semicolon debate — pick the defaults, 100 char width) and
   `.editorconfig`.
8. `.npmrc` — nothing exotic; the UI kit `@kiyotakkkka/zvs-uikit-lib` is on the public
   registry, so no auth entry is needed.
9. Root `Cargo.toml` declaring a workspace with an empty `members` array and a comment
   pointing at TASK_017.
10. `.github/workflows/ci.yml` — on push and PR: setup Node 26 + pnpm, `pnpm install
    --frozen-lockfile`, then `pnpm typecheck`, `pnpm lint`, `pnpm build`, `pnpm test`.
    Rust steps come in TASK_017; leave a commented placeholder.
11. Create a throwaway `packages/shared/` with a single `index.ts` exporting a constant so
    the workspace resolves and the gates have something to chew on. TASK_003 replaces its
    contents.
12. Verify the boundary rule actually fires: temporarily add an import from
    `packages/shared` to a fake `apps/studio` path, confirm `pnpm lint` fails, then remove
    it. **Do not skip this** — a boundary rule that silently matches nothing is worse than
    no rule, because it buys false confidence.
13. Commit: `TASK_001: workspace skeleton, tooling and CI`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Do you want a GitHub remote for this repo now, and if so what is the owner/name? I can set
> up CI to run on push, but without a remote the workflow file is just sitting there."
>
> **Why it matters:** Determines whether CI is real from day one or dormant. It also decides
> whether later tasks can use `gh` for PRs.
> **Options:** (a) create the remote now and push — CI runs from TASK_001 onward;
> (b) local-only for now, keep the workflow file dormant and push later.
> **If the user is unavailable:** take (b) — commit the workflow file, do not create a
> remote, note it in Handoff.

## Verify

```bash
pnpm install
pnpm typecheck
pnpm lint
pnpm build
```

All four must succeed on a clean checkout. Then confirm the workspace is real:

```bash
pnpm ls --depth -1
```

Manual check: `pnpm lint` must **fail** when you add a forbidden cross-domain import, and pass
when you remove it. Demonstrate this once before finishing (step 12).

## Definition of done

- [x] `pnpm install` from a clean clone succeeds
- [x] All four gate commands pass
- [x] A deliberate cross-domain import fails lint (demonstrated, then reverted)
- [x] `Cargo.toml` workspace exists with an empty members list
- [x] CI workflow committed
- [x] Handoff block filled in, status `done`, INDEX.md ticked

## Handoff

**Built:** pnpm 12.3.4 workspace, Turbo scripts, strict shared TS config and project
reference, shared flat ESLint config with resolved-path boundaries, placeholder shared
export, reserved Passport directory tree, formatting/editor/Git settings, empty Cargo
workspace, lockfile, README and Windows CI. A 13-case Node test verifies forbidden and
allowed imports, including aliases. Temporarily exporting from a fake host file in
shared caused `pnpm lint` to fail with `import/no-restricted-paths`; removing it restored
success. Frozen-lockfile installation, typecheck, lint, build, test and format:check all
passed from a fresh staged checkout in a temporary directory on Windows.

**Deviated from plan:** Git was already initialized. Latest registry TypeScript 7.0.2
and ESLint 10.10.0 exceed the chosen lint dependencies' peer ranges, so pinned the latest
compatible TypeScript 6.0.3 and ESLint 9.39.5. Added .gitattributes to enforce LF on Windows.
Preserved existing private-directory ignores: tasks (including this handoff and INDEX),
design and references remain local. Used the built-in Node test runner for boundary tests;
the application test harness remains TASK_008. Existing origin now points to
https://github.com/KiyotakkkkA/AIStudio.git; no remote was created and no push was authorized.

**Next session should know:** TASK_002 must add the app manifest and separate host/renderer
TS projects; TASK_003 replaces packages/shared/src/index.ts. Root lint scans app source even
before an app manifest exists and every build depends on lint. TASK_006 must use
host/data/client.ts and preserve repository-only access (its composition/transaction API
needs to respect this rule). pnpm 12 settings live in pnpm-workspace.yaml, including the
unrs-resolver build allowance. Cargo commands are deferred until TASK_017 adds members.
CI is committed but has not run on GitHub; the local commit has not been pushed.
