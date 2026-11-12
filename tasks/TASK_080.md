---
task: TASK_080
title: Allow rules bound to approvals, listed and removable
phase: A — Access pipeline (APS)
status: done
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

- [x] `execPolicy.allowRule` accepts only a pending approval id and a scope
- [x] Interpreters and script strings cannot be allowed by rule
- [x] Rules are listed and removable in the studio
- [x] Hand-edited `allow` rules do not apply until confirmed
- [x] Contract, schema and fixture updated together
- [x] PASSPORT §14 and §7 updated
- [x] All gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- **Bound to the approval (H12).** `AllowRuleInput` is `{ runId, approvalId, scope: "folder" |
  "anywhere" }` (Rust, Zod, fixtures; `AllowRuleChangeDto` is gone). `access::tool_approval`
  stores the reviewed action and the conversation on `ApprovalService` (`remember`) for process
  and file approvals; `reviewed(run, id)` returns it only while that approval is pending on that
  run and the entry is dropped when the approval is decided, settled or found gone
  (`APPROVAL_GONE`). The IPC handler resolves the working folder from the conversation
  (`FolderAccessService::working_folder_of`) and passes action, scope and folder to
  `ExecPolicyService::allow_rule`.
- **Derivation and refusals (H11).** `user_rules::derive`: program name plus every leading
  plain-word argument (`[A-Za-z0-9][A-Za-z0-9_-]*`) up to the first flag or path, then `**`;
  `folder` adds `{folder,folder/**}` as `cwd`. File rules take the destructive operations and
  `<folder>/**` or the exact paths, a move's source included. Refusals (`ValidationFailed`,
  Russian constants): `SOURCE_RULE` (a plan with a language or source), `SCRIPT_RULE`
  (`scriptString` or `commands::script::interpreter_program`: shells, PowerShell, `cmd`, `wsl`,
  Python, JS runtimes, script hosts, `env`, `xargs`, `start`), `VENDOR_RULE` (vendor `ask` or
  `deny` matches the action), `OUTSIDE_FOLDER`, `NO_WORKING_FOLDER`, `NOTHING_TO_ALLOW`,
  `PATTERN_CHARACTERS`. `rules.rs` `resolve` now puts a vendor rule keyed on `scriptString`
  before any user rule, so no confirmed hand-written `allow powershell **` outranks
  `review-script-strings`.
- **List and remove (M13).** `execPolicy.rules` → `UserRulesDto { rules: UserRuleDto[] (id,
  text, effect, pack, createdAt, active), unconfirmed: string[] }`; `execPolicy.removeRule {
  id }` and `execPolicy.confirmPack { pack }` return the new list. The console allow-list gained
  `execPolicy.rules` only. Stored rules carry `createdAt`; `RuleSpec` accepts it.
- **Integrity (M14).** `policy/integrity.rs`: `PackKey` (keyed BLAKE3, `blake3` added to
  `zvs-host`), `<pack>.mac` beside the pack, `SealedPackKeys` keeps a generated key sealed by
  the OS keychain in the `exec.policy.packKey` setting (wired in `src-tauri/lib.rs`).
  `RuleSet::load(vendor, folder, key)` drops `allow` rules of a pack whose MAC is missing or
  wrong and reports it in `RuleSet::unconfirmed` (logged on each recompile). `PackStamp` covers
  `.mac` files. Removal re-signs only a pack that was signed.
- **Pack file.** `user.json`; `user.cli.json` is moved once on first use (or set aside as
  `user.unconfirmed*.json` when `user.json` exists).
- **Clients.** CLI: «always» is one `Request::AllowRule { run, approval, scope }` that calls
  `execPolicy.allowRule` and then `runs.approve` in order; offered for processes in
  `safety-ask` and `auto-bypass` and for files in `auto-bypass`, hidden for script files and
  script strings. Studio: `AlwaysAllowButtons` in the process and file approval blocks of the
  chat card under the same conditions (`approvalModel.alwaysAllowable`),
  `ChatStore.allowAlways` (rule, then approve; a refused rule shows the error and still
  approves); «Правила» (`RuleList`) on the judge settings page with removal, the unconfirmed
  warning and «Подтвердить» behind a confirmation. Copy in `execPolicy/rulesCopy.ts`.
- **Tests.** `user_rules.rs`: a test per refusal and per pattern shape, folder and exact file
  rules, dedupe, hand-edit ignored then confirmed, set-aside, removal from signed and unsigned
  packs, migration, damaged/absent confirm. `integrity.rs`: MAC and file signing.
  `policy/tests.rs`: approve-always → runs → removed → asks; hand-written allow ignored until
  confirmed while its deny applies; script-string review outranks a user allow; an interpreter
  refused; migration. `access/tests.rs`: the approval keeps its reviewed action until answered.
  Renderer: `policyRules.test.ts`, `chatStore.test.ts` (rule before approve, approve on
  refusal). CLI: the prompt test now covers `safety-ask` and script strings.

**Deviated from plan:**

- Hand-written packs keep loading without their `allow` rules; confirming signs the whole pack
  as it is. An unconfirmed `user.json` is renamed rather than signed when a new rule is written,
  so a planted rule can never ride along with a legitimate one.
- The migrated `user.cli.json` is not trusted: its `allow` rules wait for confirmation in the
  list. Existing console rules therefore stop applying after the update until the person
  confirms the pack once.
- A confirm channel (`execPolicy.confirmPack`) was added; the scope implies it ("unless the
  person confirms them in the list").
- With no OS keychain (`SealedPackKeys` errors) no user `allow` applies and none can be
  written; `deny`/`ask`/`review` still do.
- `cargo build` is already allowed by the vendor `allow-build-tools` family, so the manual
  check needs an unmatched command (`deploytool sync` in the tests) to see «always» change the
  outcome.
- `docs/APS_AUDIT.md` is not annotated per task, so it is unchanged.

**Next session should know:**

- The reviewed actions live in memory on `ApprovalService`; an approval that survives a host
  restart (it does not today) would have no action and `allowRule` would refuse it.
- The «Правила» row is not in `SETTING_ENTRIES` search: it lives on the judge sub-page, which
  search cannot open yet.
- Gates run locally on Windows: `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -D warnings`, `cargo test -p zvs-host -p zvs-cli -p zvs-contract`, `pnpm typecheck`,
  `pnpm lint`, `pnpm test`, `pnpm format:check`; the rest of the workspace in `target/verify`.
  Not run: `pnpm build`, `pnpm test:e2e` (CI), and the manual check in the running app.
