# Access audit and the APS proposal

Audit date: 2026-11-10, at commit `132e6a5`. Scope: the access grant system and the
bypass-validation path across the studio, the `aistudio` console client and every driver
(files and workspaces, processes, the agent browser, computer use, MCP, integrations,
scenarios). Weighted threats, as chosen for this audit: code structure, UX friction and a local
caller. Prompt injection is covered only where it meets those three.

**Method and limits.** Everything below was traced by reading the code. Nothing was executed:
no test was run and no bypass was reproduced against a running app. Each finding names the
file that shows it. Items marked _verify_ rest on a single observation and should be confirmed
before work starts.

## Verdict

**Overall: 5 / 10.** The floor is sound: tool tiers, folder grants, forbidden folders, browser
site rules and webview capabilities fail closed and are well tested. The layer above the floor
is not: the bypass engine is a deny-list with a catch-all allow, its rules are inert in three of
four modes, there is no automatic mode without a judge model, the gate is re-implemented by
hand in six drivers, and the console pipe can change the policy itself. That is enough to
justify the new engine (APS) rather than further patches.

| #   | Area                              | Score | One-line reason                                                                                                                       |
| --- | --------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Tool tiers and the admission gate | 7     | Correct and fail-closed; every deferring node must remember to finish the gate itself                                                 |
| 2   | Folder grants and workspaces      | 8     | Canonicalised, scoped, forbidden folders enforced, ambiguous answers deny                                                             |
| 3   | Agent browser                     | 7.5   | Per-site rules, `block` before anything, submit and download never grantable                                                          |
| 4   | Computer use                      | 6     | Per-app grants and a forbidden list, but grants are by executable name and sensitive actions reuse the browser family                 |
| 5   | Execution policy table            | 5     | Clean pure function, but `Floor` is unreachable and rules run in one mode only                                                        |
| 6   | Rule engine and vendor pack       | 4     | Catch-all allow, exact-match interpreter flags, creates never matched, packs recompiled on every call                                 |
| 7   | Validation **with** a judge       | 6     | Judge is well isolated and fails to `ask`, but most commands never reach it                                                           |
| 8   | Validation **without** a judge    | 3     | No rules-only mode; `safety-ask` ignores every rule, including vendor `deny`                                                          |
| 9   | User allow rules                  | 3     | One click on a `powershell -Command` approval allows every PowerShell run; not bound to an approval; console only; no list or removal |
| 10  | Console pipe (local caller)       | 4     | Owner-only DACL is good; any same-user process can then set `no-restrictions`, write allow rules and answer approvals of every dialog |
| 11  | Webview capabilities              | 8     | Three commands, label re-checked on each                                                                                              |
| 12  | Code structure                    | 4     | Four grant stores, two mode systems, six hand-written gates                                                                           |
| 13  | UX                                | 4     | Access is configured in seven places; approvals appear to expire after 60 s                                                           |

## How a call is admitted today

```
model tool call / graph step
  └─ Scheduler → admit hook (services/runs.rs, Gate)
       ├─ graph allow-list, tool enabled/available
       ├─ PermissionService::admit_subject      tier: auto | ask | off, scoped rows
       │     auto → run        off → PermissionDenied
       │     ask  ├─ node defers?  → ticket handed to the node, gate returns Ok
       │          └─ otherwise     → ExecReview (Plain | Declared) → approval
       └─ node.execute
             └─ deferring nodes finish the gate themselves:
                fs.apply          files/nodes.rs:1194      Files → rules → judge
                shell.run, skills commands/nodes.rs:269    Process → rules → judge
                browser.*         browser/nodes.rs:396     Browser | Plain
                computer.*        computer/nodes.rs:736    "Browser" | Plain
                integrations      integrations/tools.rs:107  always asks, no review
                folder access     workspace/service.rs:497   always asks, own answer channel
```

Separate grant stores: `tool_permission` (tiers and scoped rows), `folder_grant`,
`browser_site_rule`, `app_grant`, plus rule packs on disk and the `exec.policy` setting.
Agentic sessions add their own `ReadOnly | Ask | Auto` mode (`agentic/node.rs:272`).

## Findings

Severity: **H** changes what runs without the person's consent, **M** weakens a check or costs
real friction, **L** hygiene.

### Bypass validation with a judge (`auto-bypass`)

- **H1. The vendor pack ends in `allow-unflagged` (`["*", "**"]`).** Any command no `ask` or
  `deny` rule names runs on rules alone and never reaches the judge
  (`policy/vendor.json`, `rules.rs` `resolve`). The mode is therefore a deny-list. Commands the
  pack does not name and that carry real effect: `npx <package>`, `pnpm dlx`, `uvx`, `bunx`
  (download and run code — the install rule matches verbs, not these), `docker run -v C:\:/h …`,
  `wsl rm -rf /mnt/c/…` (the `rm` rule matches a program name, the script rule needs `-c`),
  `bash script.sh` (bash is not in `commands/script.rs` `interpreter`, so the file is not read),
  `mshta`, `wscript`, `cscript`, `rundll32`, `regsvr32`, `forfiles`, `tar -x` over existing
  files.
- **H2. Interpreter flags are matched exactly.** `review-script-strings` lists
  `/c /k -c -command -encodedcommand -enc -e --eval -p --print`. PowerShell accepts any
  unambiguous prefix (`-ec`, `-en`, `-enco`, `-com`, `-comm`) and `cmd` accepts `/r` as `/c`.
  Those miss the review rule and fall to H1. Windows PowerShell also runs a bare positional
  string as a command.
- **M3. File rules never see a create.** `RuleSet::evaluate` filters to destructive operations
  before matching, so `vendor.files.deny-autostart` (no operation list, meant to cover
  everything) does not stop a _new_ file in the Startup folder, and a new `.github/workflows`
  file or `.bashrc` is `Benign` and runs in `safety-ask` and above. The folder grant still
  applies; the rule does not.
- **M4. `fs.apply` at tier `auto` skips the review entirely.** `permission_for` raises the
  tier to `ask` only for deletes. With the tool set to `auto` no ticket is created, so
  `files/nodes.rs` never calls `review_action`, and an overwrite of a path a vendor `deny`
  rule names is not checked.
- **M5. The judge sees one action, once.** No memory of earlier verdicts in the run, so a
  harmful sequence split into harmless steps passes; the 50-call cap is counted in a map that
  is never pruned (`service.rs` `spent`) and resets on restart.
- **L6. Audit conflates rule and judge.** A rule `allow` is recorded as
  `ToolAuthorisationKind::Judge` with the rule id as source (`service.rs` `review`).
- **L7. Packs are read and compiled on every review** (`ExecPolicyService::rules` →
  `RuleSet::load`): a directory scan and ~45 regex compilations per reviewed call.

What is good here: the judge receives only the structured action, the action is fenced against
instruction injection, a missing model, an error, a timeout and an unparsable answer all become
`ask`, and the judge can never answer `deny` or anything stronger than `allow`.

### Bypass validation without a judge

- **H8. There is no automatic mode without a judge.** `set` refuses `auto-bypass` with no
  judge and `get` silently drops it to `safety-ask` when the model disappears. `safety-ask`
  asks for every `shell.run` and every destructive file change and consults **no rule**: a
  `git status` asks, and so does `format C:`. The rule engine — the deterministic half of the
  design — is unreachable for everyone who has not configured a second model.
- **H9. Vendor `deny` rules hold in one mode of four.** `decide` returns `Review` only for
  `auto-bypass`. In `always-ask` and `safety-ask` a denied command is offered to the person as
  an ordinary approval; in `no-restrictions` it runs. PASSPORT §14 lists the floor as holding in
  every mode but the deny rules are not part of it.
- **L10. `Family::Floor` is dead.** `ReviewedAction::family` never returns it; the floor is
  enforced earlier, in admission. The table row and its tests assert a path nothing takes.

### User allow rules

- **H11. An allow rule can cover a whole interpreter.** `user_rules.rs` `process_rule` keeps the
  program and the next argument only when it is a plain word. For
  `powershell -Command "<text>"` the stored pattern is `powershell **`; for `cmd /c …` it is
  `cmd **`. A user `allow` outranks vendor `review` and `ask` (`rules.rs` `resolve`), so one
  «always allow» ends every later review of that interpreter, in any folder when no folder is
  given. The console hides the option for a script _file_ (`rules_apply`, `scripted`), not for
  a script _string_, and the host does not check either.
- **H12. `execPolicy.allowRule` is not bound to an approval.** The handler takes free-form
  `argv`, `cwd`, `folder` or paths (`ipc/exec_policy.rs`). Nothing ties the rule to a pending
  request the person saw. It is on `CLI_CHANNELS`.
- **M13. Rules can be added but not seen or removed.** No channel lists or deletes them; the
  studio has no «allow this rule» at all (only `zvs-cli` calls the channel), and the file is
  `user.cli.json`. The only way to revoke is to edit JSON in the app data folder.
- **M14. The policy store is writable by what it polices.** Processes run with the person's
  rights and no sandbox, so a command admitted by H1 can write a pack with `allow` rules into
  `<app data>/policy/` or edit the `exec.policy` setting. This is inherent to unsandboxed
  processes; it means rule packs need an integrity check, not that it can be closed by a rule.

### Local caller (the console pipe)

- **H15. The pipe can lower the policy.** `execPolicy.set` is on `CLI_CHANNELS`; the
  `no-restrictions` confirmation lives in the clients, not the host (_verify_ for the studio
  side). Any process running as the same user can connect (the DACL is per user, the client is
  not identified), set `no-restrictions`, `chat.send` a prompt and so drive the logged-in
  browser profile, mail and connected accounts — capabilities it does not have by itself.
- **M16. Every connection hears every dialog and can answer for it.** The connection subscribes
  to the whole event bus (`cli/connection.rs` `serve`) and `runs.approve`, `runs.deny` and
  `folders.answer` take any run id, including approvals raised in the studio window.
- **L17. The allow-list is a flat string array** with the reasoning in PASSPORT prose. Nothing
  in code says which channels mutate policy or grant access.

What is good here: owner-only DACL, first-instance creation, remote clients rejected, the client
verifies the server's SID, greeting before anything, 8 MiB line cap, secret references refused
in `cli.mcp.create`.

### Drivers

- **M18. Each deferring driver finishes the gate by hand.** Six call sites repeat
  `take_deferred_approval → review_action → approvals.create → request_approval → authorise`
  with small differences. A node that defers and forgets a step runs unapproved, and the
  compiler cannot notice. Integrations skip the review step, which is intended for mail (every
  send asks) but is not expressed anywhere except by omission.
- **M19. Computer use reuses `ReviewedAction::Browser` for sensitive input**
  (`computer/nodes.rs:736`). Typing into a granted terminal window is a process launch that no
  process rule or judge sees; in `no-restrictions` it runs silently. App grants are by lowercase
  executable name, so any `notepad.exe` on disk matches.
- **M20. Destructive tools are raised to `ask` in chat and skills but not in scenarios**
  (`scenarios/nodes.rs:664`, `minimum_tier: None`; `services/chat.rs:2272` and
  `skills/nodes.rs:489` pass `destructive`).
- **L21. MCP tools carry no effect description.** They are `Plain` unless the tool is flagged
  destructive, so in `safety-ask` every non-flagged MCP tool at tier `ask` runs.

### UX

- **M22. Access is configured in seven places:** tool tiers («Инструменты»), folder grants
  («Разрешения»), site rules (browser «Параметры»), app grants («Параметры → Устройство»), the
  mode (composer shield), the judge («Общие»), the per-session agentic mode — and user rules in
  a JSON file.
- **M23. Approvals expire after 60 s** (`DEFAULT_APPROVAL_TIMEOUT`; no override found —
  _verify_). An unanswered approval is a denial and the tool call fails.
- **M24. «Always» means four different things:** a scoped tier row, a folder grant
  (`conversation | always`), a site rule (click and type only), an app grant
  (`run | conversation | always`), and an allow rule. Scopes and wording differ per driver.

## APS — the access pipeline system

One engine that every caller and every driver goes through. The existing pieces become stages
of it; no check is removed.

### Shape

```
AccessRequest {
    actor:    run, root run, conversation, origin (model | graph | scenario | subagent)
    caller:   studio | cli(connection) | browser-ui | panel        ← who answers and configures
    tool:     registry key
    effect:   Read | Files{changes} | Process{plan} | Site{origin, action}
              | Desktop{apps, sensitive} | Remote{service, action} | Opaque{destructive}
    resources: [Folder(path) | Site(origin) | App(exe, path) | Service(id)]
}

Decision { outcome: Allow | Ask | Deny, stage, source, reason, grants_offered }
```

Stages, in order. Each returns `Continue`, or ends the pipeline with a `Decision`:

| #   | Stage                                                                                                                                        | Holds in                                 | Replaces                                                                         |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- | -------------------------------------------------------------------------------- |
| 1   | **Floor** — tool off or disabled, graph allow-list, forbidden folders and apps, outside the session workspace, credential and payment fields | every mode                               | checks scattered over `Gate`, `workspace`, `computer/policy.rs`, browser         |
| 2   | **Hard rules** — `deny` rules of every pack, for creates as well                                                                             | every mode                               | `deny` reachable in `auto-bypass` only (H9, M3, M4)                              |
| 3   | **Grants** — one store: resource × action × scope (`once`, `run`, `conversation`, `always`)                                                  | every mode                               | `folder_grant`, `app_grant`, `browser_site_rule`, scoped `tool_permission` rows  |
| 4   | **Mode** — `always-ask`, `safety-ask`, `auto`, `no-restrictions`                                                                             | —                                        | `decide`, plus the agentic session mode expressed as a ceiling on the same table |
| 5   | **Rules** — `ask`, `review`, `allow`; unmatched is _not_ allowed                                                                             | `auto`; `ask` rules also in `safety-ask` | catch-all allow (H1)                                                             |
| 6   | **Judge** — optional                                                                                                                         | `auto` when a judge is set               | —                                                                                |
| 7   | **Person** — one approval builder, one answer shape                                                                                          | whenever a stage says `Ask`              | six hand-written gates (M18)                                                     |
| 8   | **Audit** — one record with the stage trail                                                                                                  | always                                   | `ToolAuthorisationDto` with mixed kinds (L6)                                     |

### What changes in behaviour

- **`auto` works without a judge.** Rules decide what they can; unmatched and `review` go to
  the judge when one is set and to the person when it is not. The judge becomes an upgrade
  that removes prompts, not a prerequisite for the mode (H8).
- **`deny` is a floor.** Vendor and user `deny` rules refuse in every mode, including
  `no-restrictions` (H9).
- **Unmatched is never allowed by a wildcard.** `allow-unflagged` is removed; the pack grows
  explicit allow families instead, and interpreter detection moves from argv globs into code
  that understands flag prefixes and reads bash and script strings (H1, H2).
- **A node cannot run unadmitted.** Nodes describe; the kernel decides. A node implements
  `prepare(input) -> Prepared` (the diff, the process plan, the resolved element or window) and
  `access(&Prepared) -> AccessRequest`; `execute` takes an `Admitted<Prepared>` that only the
  pipeline can construct. `defers_approval` and `take_deferred_approval` disappear (M18).
- **«Always» is derived by the host from the pending request.** The answer names a scope; the
  host builds the grant or the rule from the request it holds, refuses interpreters and script
  strings, and the rule is listed and removable (H11, H12, M13).
- **Callers are identified.** Every dispatch carries a `Caller`. A channel policy table states
  per channel which callers may use it and whether it _loosens_ access; loosening from the
  console (lower mode, new allow rule, `always` grant) needs a confirmation in the studio
  window. A console connection receives events of the dialogs it opened and may answer only
  those (H15, M16, L17).
- **One «Доступ» page** lists tools, folders, sites, apps and rules from the single grant
  store, with one scope vocabulary (M22, M24).

### What does not change

The tool tier semantics, the folder grant model and its forbidden folders, the browser's
`allow | ask | block` per site with submit and download never grantable, the judge prompt and
its isolation, the owner-only pipe, the webview capabilities, and "every mail send asks".

## Backlog

| Task       | Title                                                                            | Closes                     |
| ---------- | -------------------------------------------------------------------------------- | -------------------------- |
| `TASK_077` | APS core: request, decision and the staged pipeline behind the current gate      | M18 groundwork, L10        |
| `TASK_078` | Drivers on the pipeline: `prepare` / `access` / `Admitted`                       | M18, M19, M20, L21         |
| `TASK_079` | Rules in every mode, `auto` without a judge, the vendor pack without a catch-all | H1, H2, H8, H9, M3, M4, L7 |
| `TASK_080` | Allow rules bound to approvals, listed and removable                             | H11, H12, M13, M14         |
| `TASK_081` | Caller identity and the console pipe                                             | H15, M16, L17              |
| `TASK_082` | One grant store and the «Доступ» page                                            | M22, M24                   |
| `TASK_083` | Approval lifetime, audit trail and the agentic mode on the same table            | M5, L6, M23                |

Order: 077 → 078 → 079 → 080, with 081 independent of 078–080 and 082–083 last. TASK_079 and
TASK_081 close the High findings and can be pulled forward as patches on the current code if
the full pipeline has to wait; each task says how.
