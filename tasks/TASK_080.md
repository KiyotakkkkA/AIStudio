---
task: TASK_080
title: Allow rules bound to approvals, listed and removable
phase: A — Access pipeline (APS)
status: todo
depends_on: [TASK_079]
unblocks: [TASK_082]
estimate: 3 h
---

# TASK_080 — Allow rules bound to approvals, listed and removable

## Context

Answering an approval in the console client with «always allow» calls `execPolicy.allowRule`
(`crates/zvs-host/src/ipc/exec_policy.rs`), which writes an `allow` rule into
`<app data>/policy/user.cli.json` (`policy/user_rules.rs`). A user `allow` outranks vendor
`ask` and `review` (`rules.rs` `resolve`). `docs/APS_AUDIT.md` found:

- **H11** — `process_rule` keeps the program and the next argument only when it is a plain
  word. For `powershell -Command "<text>"` the stored pattern is `powershell **`; for
  `cmd /c …` it is `cmd **`. One answer ends every later review of that interpreter. The console
  hides the option for a script file, not for a script string, and the host checks neither.
- **H12** — the channel takes free-form `argv`, `cwd`, `folder` or paths. Nothing ties the rule
  to an approval the person saw, and the channel is on the console allow-list.
- **M13** — there is no channel to list or delete rules and the studio has no «always allow» at
  all. Revoking means editing JSON.
- **M14** — processes run unsandboxed, so one can write its own pack into the policy folder.

## Why

The allow rule is the most powerful grant in the app — it removes the judge and the person for
a whole class of commands — and it is the least controlled.

## Depends on

- `TASK_079` — script-string classification on the process plan, and rules active in every mode.

## Read first

- `/docs/APS_AUDIT.md` — *User allow rules*
- `/docs/PASSPORT.md` §14 *Implementation*; §7 *The CLI pipe*
- `crates/zvs-host/src/policy/user_rules.rs`, `ipc/exec_policy.rs`
- `crates/zvs-host/src/services/approvals.rs`
- `crates/zvs-cli/src/state/prompt.rs` (`rules_apply`), `state/mod.rs` (`allow_rule`)

## Scope

**In scope**
- **The rule is derived by the host.** `AllowRuleInput` becomes
  `{ runId, approvalId, scope: "folder" | "anywhere" }`. The host keeps the reviewed action with
  the pending approval and builds the rule from it. Contract change: Rust type, Zod schema,
  golden fixture.
- **Refusals in the host:** an interpreter or script-string plan, a source-bearing plan, a
  program on the vendor `ask`/`deny` lists, a path outside the working folder when the scope is
  `folder`. Each refusal is a `ValidationFailed` with a Russian message constant.
- **Narrower patterns.** Keep the program and every leading plain-word argument up to the first
  flag or path (so `git stash list` stores `git stash list **`, not `git stash **`).
- **List and remove:** `execPolicy.rules` returns user rules (id, text, pack, created time);
  `execPolicy.removeRule { id }` deletes one. The studio may call both; the console may list.
- **Studio UI.** «Всегда разрешать» on process and file approvals in the chat approval card,
  under the same conditions as the console; a «Правила» list with removal where the judge model
  is configured. Copy in a `*Copy.ts` beside the feature, Russian, per `CONVENTIONS.md`.
- **Integrity.** The host writes a keyed MAC of each user pack beside it (key in the secret
  store). A pack whose MAC does not match is loaded with its `allow` rules ignored — `deny` and
  `ask` still apply — and a warning is logged and shown in the rules list.
- Rename the pack file to `user.json`; read `user.cli.json` once and migrate.

**Out of scope** (do not do these, even if tempting)
- A rule editor. Rules are created from approvals and removed from the list.
- Hand-written user packs keep working, without `allow`, unless the person confirms them in the
  list (the confirmation writes the MAC).

## Steps

1. Keep the reviewed action on the pending approval; change the DTO in all three places.
2. Host-side derivation and refusals, with a test per refusal and per pattern shape.
3. List and remove channels; console allow-list gets `execPolicy.rules` only.
4. MAC on write, check on load.
5. Console client: send the approval id; studio: the button and the list.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p zvs-host policy:: cli::
cargo test -p zvs-cli
cargo test -p zvs-contract
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check: approve `cargo build` with «always» → `cargo build` runs unasked afterwards and
the rule is in the list; remove it → it asks again. An approval for `powershell -Command …`
offers no «always». Append an `allow` rule to `user.json` by hand → it is ignored and flagged.

## Definition of done

- [ ] `execPolicy.allowRule` accepts only a pending approval id and a scope
- [ ] Interpreters and script strings cannot be allowed by rule
- [ ] Rules are listed and removable in the studio
- [ ] Hand-edited `allow` rules do not apply until confirmed
- [ ] Contract, schema and fixture updated together
- [ ] PASSPORT §14 and §7 updated
- [ ] All gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
