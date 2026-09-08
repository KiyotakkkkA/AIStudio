# ZVS AI Studio

Local-first desktop automation studio. The binding architecture is in
[docs/PASSPORT.md](docs/PASSPORT.md). The workspace includes a sandboxed Electron shell
and a minimal React renderer. IPC implementations and Rust crates arrive in later tasks.

Use Node 26 and pnpm 12.3.4 (the exact package manager is recorded in `package.json`).

```sh
pnpm install --frozen-lockfile
pnpm typecheck
pnpm lint
pnpm build
pnpm test
pnpm format:check
```

`pnpm dev` (or `pnpm --filter studio dev`) opens the studio with renderer HMR.
After building, `pnpm --filter studio start` opens the unpackaged production build.
Installation runs `install-electron` to download Electron's runtime (required by Electron 44).
`pnpm format` formats maintained source and configuration. The Cargo workspace is
intentionally empty until TASK_017, so Cargo gates are deferred.

`apps/studio/src` reserves host, preload, renderer, and browser UI directories.
`packages/shared` builds the temporary shared export; `packages/ipc` is reserved.
Shared TypeScript and ESLint configuration live in their own workspace packages.
New TypeScript packages should extend `@zvs/tsconfig/base.json`, use a package-local
`rootDir`, and declare project references for their dependencies. Add new projects to
the root solution config. Studio has separate host, preload, renderer, and build-config
TypeScript projects. Host/config skip checking upstream declaration files because Electron
references DOM types and electron-vite references optional SWC types; application source
remains strictly checked, and the host does not gain DOM globals.

The host composition root resolves platform paths, starts logging, and creates the window.
Persistent data lives under Electron's `userData` directory for `ZVS AI Studio`; logs are
JSON lines in `logs/studio.jsonl`, rotating at 5 MiB with three backups. Each line includes
an epoch timestamp, level, and scope. The platform logger takes its directory and clock as
dependencies. UUID v7 identifiers come from `host/platform/ids.ts`.

The preload exposes nothing until TASK_004. CSP permits local scripts/styles only and adds
the local Vite WebSocket origin only in development. CSS is linked as a local file so it
does not need inline-style exceptions. Vite HMR remounts the placeholder React root;
React Fast Refresh and state preservation are not configured yet.

The root lint task scans all application files, including directories without a package
manifest. It runs without caching, and builds depend on lint. Path resolution honors
application TypeScript configs, including aliases. Missing imports also fail lint.
Shared cannot import applications; host and renderer cannot import one another; UI
cannot import features or stores. Only host repositories may import
`apps/studio/src/host/data/client.ts`. Keep that entry point when implementing TASK_006.
Its composition and transaction API must preserve the repository boundary.

Boundary regression tests use Node's test runner and temporary fixtures, checking both
allowed and forbidden dependencies. The application test harness arrives in TASK_008.

Tool versions are exact. TypeScript 6.0.3 and ESLint 9.39.5 are the newest releases
compatible with the installed `typescript-eslint` and `eslint-plugin-import` peer ranges
at bootstrap; revisit these together when upgrading. pnpm settings such as `saveExact`
and the resolver build allowance live in `pnpm-workspace.yaml`; `.npmrc` holds the public
registry configuration.

Electron 44.2.0 and electron-vite 5.0.0 were resolved at installation time. Vite 7.3.6 is
the latest compatible major for electron-vite 5; Vite 8 is outside its peer range.

CI runs the install and gates on Windows for pushes and pull requests. The existing
ignore policy keeps `tasks`, `design`, and `references` local; task status and handoffs
are maintained locally as well.
