---
task: TASK_082
title: One grant store and the «Доступ» page
phase: A — Access pipeline (APS)
status: done
depends_on: [TASK_078, TASK_080, TASK_081]
unblocks: []
estimate: 4 h
---

# TASK_082 — One grant store and the «Доступ» page

## Context

"Where may a tool act" is stored four ways, each with its own table, repository, scopes and
settings surface:

| What | Table | Scopes | Managed on |
| --- | --- | --- | --- |
| Tool tiers and scoped tier rows | `tool_permission` | `global`, `conversation:*`, `scenario:*`, `site:*`, `app:*` | «Инструменты» |
| Folders | `folder_grant` | `once` (memory), `conversation`, `always` | «Разрешения» |
| Sites | `browser_site_rule` | per origin × action, `allow \| ask \| block` | browser «Параметры» |
| Apps | `app_grant` | `run` (memory), `conversation`, `always` | «Параметры → Устройство» |

Add the execution mode in the composer, the judge in «Общие», user rules (TASK_080) and the
per-session agentic mode: `docs/APS_AUDIT.md` counts seven places (M22) and four meanings of
«always» (M24). After TASK_078 every driver reads grants through one `GrantsStage` adapter, so
the stores can be merged behind it.

## Why

A person cannot answer "what can the agent reach right now" without visiting four pages, and a
new driver has to invent a fifth store.

## Depends on

- `TASK_078` — drivers read grants through the pipeline.
- `TASK_080` — user rules have list and remove channels.
- `TASK_081` — `always` answers from the console are confirmed in the studio.

## Read first

- `/docs/APS_AUDIT.md` — *UX*, *APS → Shape*
- `/docs/PASSPORT.md` §14 *Folder access*, §21 site rules, §22 *Access and policy*
- `crates/zvs-host/src/data/{schema,repositories}/{permission,folder_grant,app_grant,browser_site_rule}.rs`
- `crates/zvs-host/src/data/README.md` — the migration runner
- `apps/studio/src/renderer/pages/` — the «Разрешения» page; memory note *UI template
  consistency*: pages on one template must match, fix visual rules in the shared component

## Scope

**In scope**
- Table `access_grant`: `id`, `resource_kind` (`folder | site | app | tool`), `resource`,
  `action`, `decision` (`allow | ask | block`), `scope` (`conversation | always`), `owner`
  (conversation id or null), `granted_by` (`user | cli-confirmed`), `granted_at`, `used_at`.
  `once` and `run` grants stay in memory, held by the pipeline in one place.
- A migration that copies the four tables into it and drops them; a test with a populated
  database for each source.
- `AccessGrants` service with `covers(request)`, `grant`, `revoke`, `list(filter)`; the four
  existing services become thin adapters or are removed. Existing channels (`folders.*`,
  `browserSites.*`, `computer.grants`, `computer.revoke`) keep their DTOs and read the new store.
- One channel `access.list` returning grants grouped by kind, plus user rules and the mode, for
  the new page.
- **«Доступ» page** replacing «Разрешения»: tabs «Папки», «Сайты», «Приложения»,
  «Инструменты», «Правила», each with revoke; a header showing the mode and the judge. The
  browser and device settings keep their own lists as views of the same data. Built from the
  shared settings-page components.
- One scope vocabulary in every approval: «Один раз», «В этом диалоге», «Всегда». Constants in
  one `accessCopy.ts`.
- Deleting a dialog removes its `conversation` grants in the same transaction, as today.

**Out of scope** (do not do these, even if tempting)
- Changing what is grantable: submit and download stay ungrantable, mail still asks every time.
- Tool tier semantics.
- Redesigning the approval cards beyond the scope labels.

## Steps

1. Schema, migration and migration tests.
2. `AccessGrants` and the pipeline's `GrantsStage` on it; adapters for the four old services.
3. Run every existing workspace, browser and computer test against the new store.
4. `access.list`, contract and fixtures.
5. The page; the scope labels.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Should «Доступ» replace the «Разрешения» page in the System group, with tool tiers shown
> there too, or should «Инструменты» keep the tiers and «Доступ» link to it?"
>
> **Why it matters:** moving tiers changes the Tools page that the maket specifies.
> **Options:** (a) one page, tiers move; (b) tiers stay on «Инструменты», «Доступ» shows them
> read-only with a link.
> **If the user is unavailable:** take (b) and note it in Handoff.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p zvs-host data:: workspace:: browser:: computer:: access::
cargo test -p zvs-contract
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check: on a database from before the migration, every folder, site and app grant is
listed on «Доступ» and still admits the same calls; revoking one makes the next call ask.

## Definition of done

- [x] One table holds stored grants; the four old tables are gone
- [x] Migration covered by tests for each source table
- [x] Existing grant channels return the same DTOs
- [x] «Доступ» lists and revokes folders, sites, apps and rules
- [x] One scope vocabulary in every approval
- [x] PASSPORT §14, §21, §22 updated; `data/README.md` updated
- [x] All gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `access_grant` (migration 0043) with `resource_kind`, `resource`, `action`, `decision`,
  `scope`, `owner` (FK to `conversation`, cascade), `granted_by`, `granted_at`, `used_at` and
  `detail`; `tool_permission`, `folder_grant`, `app_grant` and `browser_site_rule` are copied in
  and dropped. One migration test per source table plus the cascade on dialog deletion.
- `AccessGrantRepository`; the folder, app, site and tier repositories are now views over the
  table with unchanged row structs, so every existing service and channel returns the same DTOs.
- `access::AccessGrants` (`covers`, `grant`, `revoke`, `list`, `tool_grants`, and `hold` /
  `held` / `release` for a run's in-memory grants). The gatekeeper drops covered folder and app
  resources before asking; the computer agent's per-run app grants moved into it; the folder
  service's coverage check goes through it. `RunService::grants()` is the shared instance.
- `granted_by` is `cli-confirmed` for `folders.answer` and `runs.approve` from the console.
- `access.list` → `AccessListDto { policy, folders, sites, apps, tools, rules }`, with
  `ToolGrantDto`, Zod schemas and golden fixtures.
- «Доступ» page in place of «Разрешения» (decision (b): tool tiers read-only with a link to
  «Инструменты»): header with the mode and the judge, tabs «Папки», «Сайты», «Приложения»,
  «Инструменты», «Правила»; revoke for folders, sites, apps and rules.
- `features/access/accessCopy.ts` with «Один раз», «В этом диалоге», «Всегда», used by the chat,
  folder, app, agentic, scenario and browser-panel approvals.

**Deviated from plan:**
- An extra `detail` column holds an app grant's path and icon, which the DTO keeps.
- Tool tier rows keep their permission scope (`global`, `skill:…`, `scenario:…`, `site:…`,
  `app:…`) in `action`; none of them is dialog-scoped, so all are `always` rows.
- There is no separate `GrantsStage` in the stage list: grants are resources, not a verdict, so
  the step is `AccessGrants::uncovered`, run by `Gatekeeper::admit` before any approval.
- Folder `once` answers still live with the one call they cover (`FolderAccess`), not in
  `AccessGrants`; app run grants moved.
- The route stays `/permissions` and the store stays `PermissionsStore`; only the label and the
  page changed.
- Removed an unused import in `preferences/cliCopy.ts` left by `baec67a`, which failed `pnpm lint`.

**Next session should know:**
- Not run here: `pnpm build`, `pnpm test:e2e`, and `cargo test` for `zvs-jobd` and `zvs-studio`
  (the running dev build locks `target/debug/zvs-jobd.exe`). The manual check on a pre-migration
  database was not done.
- `access.list` returns empty rules when the rules folder is not configured instead of failing.
- TASK_083 can put approval lifetime and the agentic mode next to `access_grant`.
