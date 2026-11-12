# policy

The execution policy: decides, per action the model wants to take, whether it runs, asks the
user, is reviewed by a judge model, or is denied. It sits on top of the permission model
(tool tiers and grants, `../kernel/README.md`); a tool already set to `off` never gets here.

## Files

| File            | Contents                                                                                                                                                      |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `table.rs`      | `decide(mode, family)`: the PASSPORT decision table                                                                                                           |
| `action.rs`     | `ReviewedAction` (plain, declared, browser, files, process) and its `Family`; `ReviewVerdict`                                                                 |
| `rules.rs`      | Rule packs: compile, match file / process / code actions, resolve conflicts (`RuleSet::evaluate`), the deny check (`RuleSet::denied`), `PackStamp`            |
| `judge.rs`      | The judge prompt, the `verdict` tool, verdict parsing                                                                                                         |
| `service.rs`    | `ExecPolicyService`: get/set mode and judge model, compiled rules, `review` and `refused` (implements `ExecReview`) through hard rules → mode → rules → judge |
| `user_rules.rs` | "Allow this rule" from an approval: writes a user rule to `user.cli.json`                                                                                     |
| `vendor.json`   | The built-in rule pack                                                                                                                                        |

## Decision table

Rule `deny` refuses first, in every mode (TASK_079). The table then answers what is left:

| Family ↓ / Mode →   | always ask | safety ask                      | auto       | no restrictions |
| ------------------- | ---------- | ------------------------------- | ---------- | --------------- |
| Auto                | run        | run                             | run        | run             |
| Benign              | ask        | run                             | run        | run             |
| Files (destructive) | ask        | ask                             | **review** | run             |
| Process             | ask        | **rules**: allow runs, else ask | **review** | run             |
| Browser             | ask        | ask                             | ask        | run             |
| Declared            | ask        | ask                             | ask        | run             |

Per rule outcome, for a reviewed process (`Family::Process`) and destructive files in `auto`:

| Rule outcome | always ask | safety ask | auto, judge | auto, no judge | no restrictions |
| ------------ | ---------- | ---------- | ----------- | -------------- | --------------- |
| `deny`       | refused    | refused    | refused     | refused        | refused         |
| `ask`        | ask        | ask        | ask         | ask            | run             |
| `review`     | ask        | ask        | judge       | ask            | run             |
| `allow`      | ask        | run        | run         | run            | run             |
| unmatched    | ask        | ask        | judge       | ask            | run             |

`ReviewedAction::family`: a plain action is benign; file changes are `Files` only if some
operation is destructive (overwrite, delete, move onto an existing file), otherwise benign; a
tool that declares itself destructive with an unknown effect is `Declared`. The floor (tier
`off`, a disabled tool, the graph allow-list) is not a family: it is the floor stage of the
access pipeline (`../access/README.md`) and holds in every mode. The `safety ask` rules cell
applies to processes only; destructive file changes still ask there.

## Review pipeline

`ExecPolicyService::pipeline` runs **hard rules → mode → rules → judge**:

1. **Hard rules** (`HardRulesStage`, `RuleSet::denied`). Any `deny` rule of any pack, vendor or
   user, that matches the action refuses it in every mode, `no-restrictions` included. File
   rules see every operation — create, plain move, overwrite, move onto, delete — and an empty
   `operations` list means all of them; a move is matched by its target and its source. The
   gate (`access::Gatekeeper`) also runs this stage through `ExecReview::refused` for every
   gated call, whatever the tier decided, so `fs.apply` and `shell.run` set to `auto` are still
   refused by a `deny` rule.
2. **Mode.** `decide(mode, family)`: run, ask, `rules` (safety ask, processes) or `review`
   (auto). A sensitive terminal input is not reviewable and asks.
3. **Rules.** `RuleSet::evaluate(action)` matches the vendor pack and the user's packs:
   - file rules: path globs × operations, checked for each destructive operation and for any
     other operation a rule names;
   - process rules: program name (case-insensitive, `.exe/.cmd/.bat/.com` stripped) and argv
     globs, where `**` matches the rest of the arguments; optional `cwd` glob; optional
     `scriptString`, which matches the plan's classification from `commands/script.rs`
     (`review-script-strings` is `{ "scriptString": true }` with no argv);
   - code rules: for scripts whose source is known (see `../commands/README.md`), language plus
     patterns to avoid in the source. A script's command line counts only when some rule names
     it, so its `ask`, `deny` or `review` still applies and the code rule speaks for the rest.
4. **Resolution per item.** A vendor `deny` always wins. Otherwise the strongest user rule
   wins (deny > ask > review > allow), then the strongest vendor rule. Across items (several
   files, or a script plus its process), any deny → **deny**; else any ask → **ask** with the
   rule's text as the reason; else any review → judge with that rule _flagged_; else, if every
   item has an allow, **run** with the allow rule recorded as the source. In `safety ask`
   only an allow runs; review and unmatched ask.
5. **Judge** (optional). Unmatched or flagged actions go to the configured judge model with a
   fixed prompt: judge the effect, not the tools; ask only for a nameable, plausible harm. The
   prompt lists what warrants asking: deleting user data, sending files or secrets anywhere or
   acting on remote services, reading credential stores, system changes outside the project,
   stopping processes or raising privileges, running downloaded or hidden code, and anything
   too obfuscated to understand. The rules come along as context; the reason is in Russian.
   The model must call the `verdict` tool exactly once with `allow` or `ask` and a reason. A
   timeout, an error or an unparsable answer all fall back to **ask** and are logged. With no
   judge configured the stage asks the person, with a flagged rule's text as the reason.
6. The verdict is recorded on the tool call (`ToolAuthorisationDto`: policy, rule or judge,
   with source and reason) so the user can see why something ran without a prompt.

`auto` needs no judge: `set` accepts it with `judge: null`, and `get` clears a judge whose
provider or model is gone without changing the mode.

## Vendor pack

No rule allows every command. `compile_pack` refuses an `allow` process rule whose argv is
`*` followed only by `**`, in any pack. A command no rule names is unmatched: it goes to the
judge in `auto` and to the person otherwise (TASK_079 decision a; «always for this program in
this folder» follows with TASK_080). The pack names, besides its `deny` and `ask` families:
package runners (`npx`, `bunx`, `uvx`, `npm exec`, `pnpm dlx`, `yarn dlx`, `bun x`,
`pipx run`, `uv tool run`, `go run` and `deno run` with a remote module) and Windows script
hosts (`mshta`, `wscript`, `cscript`, `rundll32`, `regsvr32`, `forfiles`, …) as `ask`; and
explicit `allow` families for read-only git, `git fetch`, version and help, project checks and
builds, formatters, read-only utilities, container inspection and package listings.

## Compiled rules

`ExecPolicyService::rules` keeps one compiled `Arc<RuleSet>` with the `PackStamp` it was built
from: the sorted list of `*.json` files in the user rule folder with their modified time and
length. Each review reads the stamp (a directory listing and one `metadata` per pack) and
recompiles only when it changed; writing a user rule drops the cache. Compile problems are
logged once per change.

## User rules

Choosing "allow this rule" in an approval (`allow_rule`) writes an `allow` rule built from
the concrete action (program and argv, or paths and operations) into `user.cli.json` in the
user rule folder. User packs are reloaded with the vendor pack; compile problems are reported,
not fatal, and a broken user pack never disables the vendor deny rules.
