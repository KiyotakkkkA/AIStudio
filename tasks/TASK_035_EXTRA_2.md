---
task: TASK_035_EXTRA_2
title: Skill scripts — running scripts/ through the toolchain under its own permission
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_035, TASK_035_EXTRA_1]
unblocks: []
estimate: 3 h
---

# TASK_035_EXTRA_2 — Skill scripts

## Context

TASK_035 stores a skill as `<app data>/skills/<key>/` with `SKILL.md`, `references/`, `scripts/`
and `assets/`. `skill.load` lists the files and `skill.read` returns them, but nothing executes
`scripts/`. TASK_035_EXTRA_1 adds the uv and Node runtimes with `launch(runtime, args)`. This
task adds a tool that runs a skill's script with one of those runtimes.

## Why

Many published skills do their real work in a script (`scripts/extract.py`, `scripts/build.js`).
Without execution they can only be read, not used.

## Depends on

- `TASK_035` — skill folders, `skill.read`, the narrowed run.
- `TASK_035_EXTRA_1` — the toolchain and its launch specs.

## Read first

- `/docs/PASSPORT.md` §14 — the ceiling, and untrusted input
- `crates/zvs-host/src/skills/` — library, nodes, the narrowed run

## Scope

**In scope**
- A `skill.script` tool: key, path under `scripts/`, arguments, optional stdin; the runtime is
  picked from the extension (`.py` → `uv run`, `.js`/`.mjs`/`.cjs` → `node`)
- Process execution is high risk: the tool is `destructive` and defaults to `ask`. The working
  directory is a per-run scratch folder; the skill folder is read-only input
- Time limit, output cap, cancellation that kills the process tree; stdout and stderr returned as
  fenced, untrusted tool output
- Tests: refused outside `scripts/`, refused when the runtime is missing, cancelled on run cancel,
  output capped

**Out of scope**
- Sandboxing beyond the OS user's rights
- Network policy for scripts

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Should a script see the network and the user's files, or only its scratch folder and the skill's own files?"
>
> **Why it matters:** it decides whether a sandbox (job object, restricted token) is part of this task.
> **Options:** (a) the user's rights, gated by `ask`; (b) a restricted token with no network.
> **If the user is unavailable:** take (a) and note it in Handoff.

## Verify

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm build
```

## Definition of done

- [x] `skill.script` runs a script with the connected runtime and returns capped output
- [x] Default tier `ask`; refused outside `scripts/` and when the runtime is missing
- [x] Cancellation kills the process tree
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `crates/zvs-host/src/skills/script.rs`: the `skill.script` node and tool. Input `{ key, path,
  args?, stdin? }` (at most 64 arguments of 4096 characters, stdin up to 1M characters). The
  runtime follows the extension (`ScriptRuntime`): `.py` → `uv run <file> <args>`, `.js`/`.mjs`/
  `.cjs` → `node <file> <args>`, launched through `CommandLauncher` on the connected toolchain.
  Any other extension is refused with `UnsupportedFormat`.
- `SkillLibrary::script(key, path)` resolves the path the same way `read` does (one shared
  `locate`), and also requires the `scripts/` prefix. Anything outside it is refused with
  `PermissionDenied` (`SKILL_SCRIPT_OUTSIDE`) before a launcher is called.
- A runtime that is not connected is refused by the toolchain's own `NotFound` with the
  «Настройки → Окружение» hint, before a scratch folder is made or a process starts. No launcher
  at all → `SCRIPT_NO_TOOLCHAIN`.
- The working directory is `<cache>/skill-scripts/<runId>`, kept for the run, so a later call in
  the same run can read what an earlier one wrote. When a new folder is made, only the newest
  `SCRATCH_KEEP` (16) are kept. The child gets `ZVS_SKILL_DIR` (the skill folder) and
  `ZVS_SCRATCH_DIR` on top of the toolchain's isolated env.
- Limits: `SCRIPT_TIMEOUT` 120 s and `SCRIPT_OUTPUT_BYTES` 64 KiB for each stream; both are
  options. Each stream is drained to EOF and anything past the cap is dropped, so a chatty
  script cannot block on a full pipe. On timeout or run cancel, `kill_tree` (taskkill `/T /F`
  on Windows, now exported from `drivers/mcp/stdio.rs`) stops the process tree. A timeout
  still returns the output captured so far (`timedOut`, `isError`); a cancel returns
  `RunCancelled`.
- Output `{ exitCode, timedOut, truncated, stdout, stderr, text, isError }`. `text` holds the exit
  code, the timeout and truncation notes, and the two streams fenced as `<stdout>`/`<stderr>`,
  with any closing tag inside the output neutralised. The chat then wraps it in `<tool_result>`
  under the untrusted-data instruction. A non-zero exit is `isError`, so the call is recorded as
  failed.
- Permission: the tool is `destructive`. `ToolDescriptor` gained `tier: Option<ToolTier>`, and
  `register_kernel_tools` declares `ask` for built-in destructive tools, because the registry's
  default for a destructive tool is `off`. The declared tier applies only at first registration
  (or when a tool becomes destructive). A tier the user sets later is kept.
- Offered in chat with the other `SKILL_TOOLS`, and added to a skill run's narrowed set when the
  skill has a runnable file under `scripts/`. The catalogue instruction and the run prompt name
  the tool only when it is actually offered.
- Tests (`skills/script/tests.rs`, run against the test binary as a stub child process):
  extension mapping, tool tier and description, refusal outside `scripts/` and for traversal,
  missing runtime through the real `ToolchainService`, scratch cwd and env, stdin, fencing,
  non-zero exit, output cap, timeout, run cancellation, scratch pruning, and prompt wording.
  Registry test for the declared tier.

**Deviated from plan:**
- DECISION answered by the user: option (a). Scripts run with the OS user's rights, gated by
  `ask`. No job object or restricted token. The skill folder is read-only by contract (stated
  in the tool description), not enforced by the OS.
- `uv run <file>` without `--script`: the toolchain accepts uv from 0.4.0, and older 0.4
  releases may not have the flag. Plain `uv run` on a `.py` file already reads PEP 723
  metadata.

**Next session should know:**
- `cargo clippy`/`cargo test` for `zvs-studio` tests do not compile on this machine's rustc
  1.93 (`atomic_try_update` in `browser/gateway/tests.rs`, from commit 8d91c8b, which targets
  rustc 1.99). That predates this task. The rest of the workspace, `cargo check -p zvs-studio`,
  and all pnpm gates including `pnpm build` passed.
- The kill-tree test checks that cancellation returns promptly. It does not check grandchild
  processes individually.
