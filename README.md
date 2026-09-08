# ZVS AI Studio

Local-first desktop automation studio. The binding architecture is in
[docs/PASSPORT.md](docs/PASSPORT.md). This commit provides the workspace and tooling;
Electron, React, IPC implementations, and Rust crates arrive in subsequent tasks.

Use Node 26 and pnpm 12.3.4 (the exact package manager is recorded in `package.json`).

```sh
pnpm install --frozen-lockfile
pnpm typecheck
pnpm lint
pnpm build
pnpm test
pnpm format:check
```

`pnpm format` formats maintained source and configuration. `pnpm dev` is reserved for
the app's development script in TASK_002. The Cargo workspace is intentionally empty
until TASK_017, so Cargo gates are deferred.

`apps/studio/src` reserves host, preload, renderer, and browser UI directories.
`packages/shared` builds the temporary shared export; `packages/ipc` is reserved.
Shared TypeScript and ESLint configuration live in their own workspace packages.
New TypeScript packages should extend `@zvs/tsconfig/base.json`, use a package-local
`rootDir`, and declare project references for their dependencies. Add new projects to
the root solution config. TASK_002 must create separate host and renderer TS projects.

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

CI runs the install and gates on Windows for pushes and pull requests. The existing
ignore policy keeps `tasks`, `design`, and `references` local; task status and handoffs
are maintained locally as well.
