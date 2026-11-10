---
task: TASK_082
title: One grant store and the «Доступ» page
phase: A — Access pipeline (APS)
status: todo
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

- [ ] One table holds stored grants; the four old tables are gone
- [ ] Migration covered by tests for each source table
- [ ] Existing grant channels return the same DTOs
- [ ] «Доступ» lists and revokes folders, sites, apps and rules
- [ ] One scope vocabulary in every approval
- [ ] PASSPORT §14, §21, §22 updated; `data/README.md` updated
- [ ] All gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
