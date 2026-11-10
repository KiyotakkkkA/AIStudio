---
task: TASK_079
title: Rules in every mode, auto without a judge, the vendor pack without a catch-all
phase: A — Access pipeline (APS)
status: todo
depends_on: [TASK_077]
unblocks: [TASK_080]
estimate: 4 h
---

# TASK_079 — Rules in every mode, auto without a judge, the vendor pack without a catch-all

## Context

The execution policy (`crates/zvs-host/src/policy/`, PASSPORT §14) has four modes. Policy rules
(`rules.rs`, `vendor.json`) and the judge model (`judge.rs`) are consulted only in
`auto-bypass`, and `auto-bypass` exists only when a judge model is configured. `docs/APS_AUDIT.md`
found five problems here, three of them High:

- **H8** — without a judge the strictest automatic option is `safety-ask`, which asks for every
  process and destructive file change and reads no rule.
- **H9** — vendor `deny` rules (`format`, `reg`, `git push --force`, `.ssh/**`) refuse only in
  `auto-bypass`. In the asking modes the person is offered the call; in `no-restrictions` it runs.
- **H1** — the pack ends in `vendor.process.allow-unflagged` (`["*", "**"]`), so any command no
  rule names runs on rules alone: `npx <package>`, `docker run -v C:\:/h`, `wsl rm -rf`,
  `bash script.sh`, `mshta`, `rundll32` never reach the judge.
- **H2** — `review-script-strings` matches flags exactly; PowerShell accepts prefixes (`-ec`,
  `-en`, `-com`), `cmd` accepts `/r`.
- **M3 / M4** — file rules skip creates (a new file in the Startup folder is not stopped by
  `deny-autostart`), and `fs.apply` at tier `auto` skips the review altogether.

## Why

These are the findings that let something run without consent. They also explain most of the
friction: the deterministic half of the design is unavailable to anyone without a second model.

## Depends on

- `TASK_077` — stages. If APS has to wait, this task can be done on the current
  `ExecPolicyService::review` instead; the steps are the same and Handoff must say which was done.

## Read first

- `/docs/APS_AUDIT.md` — *Bypass validation with a judge*, *without a judge*
- `/docs/PASSPORT.md` §14 *Execution policy*, *Judge*, *Implementation*
- `crates/zvs-host/src/policy/{table,rules,service}.rs`, `vendor.json`, `tests.rs`
- `crates/zvs-host/src/commands/script.rs`
- `crates/zvs-host/src/files/nodes.rs` — `permission_for` of `FsApplyNode`

## Scope

**In scope**
- **Hard-rules stage in every mode.** A `deny` from any pack refuses in all four modes,
  including `no-restrictions`. File rules are matched for creates and plain moves too; an
  empty `operations` list means every operation.
- **`fs.apply` is always reviewed.** The hard-rules stage runs even when the tier resolves to
  `auto`.
- **`auto` without a judge.** `ExecMode::AutoBypass` no longer requires a judge. Rule `allow`
  runs, rule `ask` asks, `review` and unmatched go to the judge when one is set and to the
  person when it is not. `set` stops refusing the mode and `get` stops downgrading it.
- **`ask` rules in `safety-ask`.** A rule `allow` lets a process run in `safety-ask`; everything
  else asks as today. `always-ask` is unchanged apart from `deny`.
- **Remove `allow-unflagged`.** Add explicit allow families for what it was covering (read-only
  inspection of containers, package listings, formatters, and whatever the decision below
  settles).
- **Interpreter detection in code.** A function in `commands/script.rs` classifies argv as
  *script string* (PowerShell flag prefixes of `-Command` and `-EncodedCommand`, a bare
  positional command for Windows PowerShell, `cmd /c /k /r`, `-c`/`-e`/`--eval` for the other
  interpreters, `wsl <program> …`) and reads script files for `bash`, `sh` and `zsh` as it does
  for Python, Node and PowerShell. The plan carries the classification; the `review` rule keys
  on it instead of argv globs.
- **Package runners** (`npx`, `pnpm dlx`, `yarn dlx`, `bunx`, `uvx`, `pipx run`, `go run <url>`,
  `deno run <url>`) get an `ask` rule: they download and run code.
- **Compile packs once.** Cache the `RuleSet` and reload when a pack file's modified time or the
  directory listing changes.
- Renderer and CLI copy for the mode hints (Russian, `CONVENTIONS.md`); the composer no longer
  shows the mode inactive without a judge, and names the judge as optional.
- PASSPORT §14 table and prose, `policy/README.md`.

**Out of scope** (do not do these, even if tempting)
- User allow rules — TASK_080.
- The judge prompt.
- Renaming the `auto-bypass` wire value: it is in settings and fixtures.

## Steps

1. Table tests first: for each mode × {deny, ask, review, allow, unmatched} × {judge, no judge}
   assert the outcome. This is the new PASSPORT table.
2. Hard-rules stage; creates and moves matched; `fs.apply` at `auto`.
3. Judge optional in `auto`; `ask`/`allow` rules in `safety-ask`.
4. Script-string classification with a test per flag form, including `-ec`, `-enco`, `-comm`,
   `/r`, a bare positional string and `wsl rm`.
5. Rewrite the pack tail; run the existing `policy/tests.rs` and fix expectations that relied
   on the catch-all, each with a note of why.
6. Cache; copy; docs.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "With the catch-all allow removed, a command no rule names needs a decision. Without a judge
> model, should it (a) ask the person every time, (b) ask the first time and offer «always for
> this program in this folder», or (c) keep running, as today, with only named commands asking?"
>
> **Why it matters:** (a) is safest and noisiest; (b) depends on TASK_080 to be safe; (c) keeps
> finding H1 open.
> **Options:** a / b / c as above.
> **If the user is unavailable:** take (a), and note that (b) follows with TASK_080.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p zvs-host policy:: commands::
cargo test -p zvs-contract
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check, no judge configured, mode `auto`: `git status` runs; `npx cowsay hi` asks;
`powershell -ec <base64>` asks; `format C:` is refused in all four modes; an `fs.apply` creating
a file under `Start Menu\Programs\Startup` is refused.

## Definition of done

- [ ] `deny` refuses in every mode, for every file operation
- [ ] `auto` is selectable and works with no judge
- [ ] No rule matches `["*", "**"]` with `allow`
- [ ] Script strings are classified in code with tests for every flag form listed
- [ ] Packs are compiled once and reloaded on change
- [ ] PASSPORT §14 table rewritten; `policy/README.md` updated
- [ ] All gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
