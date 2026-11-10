# policy

The execution policy: decides, per action the model wants to take, whether it runs, asks the
user, is reviewed by a judge model, or is denied. It sits on top of the permission model
(tool tiers and grants, `../kernel/README.md`); a tool already set to `off` never gets here.

## Files

| File            | Contents                                                                                                                                    |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `table.rs`      | `decide(mode, family)`: the PASSPORT decision table                                                                                         |
| `action.rs`     | `ReviewedAction` (plain, declared, browser, files, process) and its `Family`; `ReviewVerdict`                                               |
| `rules.rs`      | Rule packs: compile, match file / process / code actions, resolve conflicts (`RuleSet::evaluate`)                                           |
| `judge.rs`      | The judge prompt, the `verdict` tool, verdict parsing                                                                                       |
| `service.rs`    | `ExecPolicyService`: get/set mode and judge model, load rules, `review` (implements `ExecReview`) through the mode → rules → judge pipeline |
| `user_rules.rs` | "Allow this rule" from an approval: writes a user rule to `user.cli.json`                                                                   |
| `vendor.json`   | The built-in rule pack                                                                                                                      |

## Decision table (`table.rs`)

| Family ↓ / Mode →   | always ask | safety ask | auto (review) | no restrictions |
| ------------------- | ---------- | ---------- | ------------- | --------------- |
| Auto                | run        | run        | run           | run             |
| Benign              | ask        | run        | run           | run             |
| Files (destructive) | ask        | ask        | **review**    | run             |
| Process             | ask        | ask        | **review**    | run             |
| Browser             | ask        | ask        | ask           | run             |
| Declared            | ask        | ask        | ask           | run             |

`ReviewedAction::family`: a plain action is benign; file changes are `Files` only if some
operation is destructive (overwrite, delete, move onto an existing file), otherwise benign; a
tool that declares itself destructive with an unknown effect is `Declared`. The floor (tier
`off`, a disabled tool, the graph allow-list) is not a family: it is the floor stage of the
access pipeline (`../access/README.md`) and holds in every mode.

## Review pipeline (auto mode, files and processes)

1. **Rules.** `RuleSet::evaluate(action)` matches the vendor pack and the user's packs:
   - file rules: path globs × operations, checked for each destructive file operation;
   - process rules: program name (case-insensitive, `.exe/.cmd/.bat/.com` stripped) and argv
     globs, where `**` matches the rest of the arguments; optional `cwd` glob;
   - code rules: for scripts whose source is known (see `../commands/README.md`), language plus
     patterns to avoid in the source.
2. **Resolution per item.** A vendor `deny` always wins. Otherwise the strongest user rule
   wins (deny > ask > review > allow), then the strongest vendor rule. Across items (several
   files, or a script plus its process), any deny → **deny**; else any ask → **ask** with the
   rule's text as the reason; else any review → judge with that rule _flagged_; else, if every
   item has an allow, **run** with the allow rule recorded as the source.
3. **Judge.** Unmatched or flagged actions go to the configured judge model with a fixed
   prompt: judge the effect, not the tools; ask only for a nameable, plausible harm. The
   prompt lists what warrants asking: deleting user data, sending files or secrets anywhere or
   acting on remote services, reading credential stores, system changes outside the project,
   stopping processes or raising privileges, running downloaded or hidden code, and anything
   too obfuscated to understand. The rules come along as context; the reason is in Russian. The model must call the `verdict` tool exactly once
   with `allow` or `ask` and a reason. A missing judge, a timeout, an error or an unparsable
   answer all fall back to **ask** and are logged.
4. The verdict is recorded on the tool call (`ToolAuthorisationDto`: policy, rule or judge,
   with source and reason) so the user can see why something ran without a prompt.

## User rules

Choosing "allow this rule" in an approval (`allow_rule`) writes an `allow` rule built from
the concrete action (program and argv, or paths and operations) into `user.cli.json` in the
user rule folder. User packs are reloaded with the vendor pack; compile problems are reported,
not fatal, and a broken user pack never disables the vendor deny rules.
