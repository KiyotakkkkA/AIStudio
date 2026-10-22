---
task: TASK_036_EXTRA_3
title: Code and commands — a shared process runner, code.run and shell.run
phase: 6 — Tools, MCP and Skills
status: todo
depends_on: [TASK_036_EXTRA_2]
unblocks: [TASK_040]
estimate: 4 h
---

# TASK_036_EXTRA_3 — Code and commands

## Context

`skill.script` (TASK_035_EXTRA_2) already runs a process well. It has a scratch folder per run,
launch through the toolchain, a timeout, kill-tree on cancel, capped streams and fenced output.
But it only runs files shipped inside a skill. The assistant also needs to write a short script
and run it, and to run a project's own commands (`npm test`, `cargo check`) in the working folder
from TASK_036_EXTRA_1.

Folder grants do **not** limit these tools. On Windows a process runs with the user's rights
wherever it likes, and this task adds no sandbox (the same choice as TASK_035_EXTRA_2, option
(a)). The boundary is the approval, which shows the full script or command.

## Why

Most real work needs either a quick computation, a conversion, or the project's own build and
test commands. Without them the assistant can change files but cannot check its change.

## Depends on

- `TASK_036_EXTRA_2` — `permission_for`, which makes "always ask" possible per input.
- `TASK_036_EXTRA_1` — the working folder used as the command's cwd.

## Read first

- `crates/zvs-host/src/skills/script.rs` and its tests
- `crates/zvs-host/src/toolchain/definitions.rs` — `ToolchainCommand`
- `/docs/PASSPORT.md` §14

## Scope

**In scope**

*Shared runner*
- Move the process part of `skill.script` into `toolchain/process.rs`. That covers spawn, drain
  with caps, stdin, timeout, cancel, kill-tree, `script_text` and fencing. `skill.script` keeps
  its behaviour and its tests pass unchanged.

*`code.run` (destructive, minimum tier `ask`)*
- Input `{ language: 'python' | 'javascript' | 'powershell', source, args?, stdin? }`. The source
  is written to the run's scratch folder and run there.
- Python: `uv run`. Dependencies are declared by the model in a PEP 723 header
  (`# /// script` / `dependencies = [...]`). uv installs them into its own cache, with nothing
  global. The tool description says so.
- JavaScript: `node`. Dependencies come from an optional `packages[]` input, installed into the
  scratch folder with `npm install --ignore-scripts --no-audit --no-fund` before the run. Install
  scripts are disabled because they are a supply-chain path. This adds `npm` to
  `ToolchainCommand` (`npm.cmd` on Windows), resolved from the connected Node toolchain.
- PowerShell: `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File`. It is
  present on every Windows and needs no toolchain.
- cwd: the scratch folder. The working folder, when bound, is passed as `ZVS_WORKSPACE_DIR`.

*`shell.run` (destructive, minimum tier `ask`)*
- Input `{ argv: string[], cwd?, stdin? }` — an argument vector, never a shell string. `argv[0]`
  is resolved on `PATH` with the toolchain directories first. cwd defaults to the working folder;
  any other cwd goes through `ensure_access`.
- No working folder and no `cwd`: refused with a Russian message asking for a folder.

*Approval and display*
- The approval card shows the whole source or argv, the language or program, the cwd and, for
  JavaScript, the package list. The tool call card shows exit code, duration and the fenced
  streams.
- The «Разрешения» page gains one line under the folder sections: folder access does not limit
  «code.run» and «shell.run».
- Tool descriptions in Russian, registered in `kernel_tools.rs`.

**Out of scope** (do not do these, even if tempting)
- Sandboxing: job objects, restricted tokens, network policy.
- Long-running or background processes, servers, interactive input.
- A global package install tool.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Should `shell.run` have a per-folder list of commands that run without asking — for example
> `npm test` or `cargo check` — or should every command always ask?"
>
> **Why it matters:** the Agentic Build mockup shows "command outside the allowed list", which
> implies such a list. But a list keyed on the program alone is easy to abuse (`npm` with any
> arguments runs arbitrary scripts), so it must match whole argument prefixes.
> **Options:** (a) always ask in this task, list later; (b) a per-folder list of exact argv
> prefixes, edited on «Разрешения» under the folder grant, where a match runs without asking and
> anything else asks.
> **If the user is unavailable:** take (a) and note it in Handoff.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Tests that must exist:
- `skill.script` tests unchanged and passing on the shared runner.
- Each `code.run` language builds the expected command line, cwd and env. A missing toolchain
  gives the toolchain's `NotFound` with the settings hint.
- `npm install` is called with `--ignore-scripts` and the given packages before the run.
- `shell.run` takes argv only, defaults cwd to the working folder, refuses with no folder, and
  sends a foreign cwd through `ensure_access`.
- Both tools ask even when set to `auto` on «Инструменты». `off` denies.
- Timeout, cancel and output caps through the shared runner.

## Definition of done

- [ ] One process runner shared by `skill.script`, `code.run`, `shell.run`
- [ ] `code.run` for Python (PEP 723), JavaScript (scratch `npm install`), PowerShell
- [ ] `shell.run` with argv, working folder cwd, folder access for other cwds
- [ ] Both always ask; the approval shows the full source or command
- [ ] DECISION answered or defaulted and noted
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
