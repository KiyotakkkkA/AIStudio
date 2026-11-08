# commands

The `shell.run` kernel tool: runs a project program (`npm test`, `cargo check`) or a skill
script on the model's behalf, in the foreground or as a background shell, after the user
approves the exact command. There is no shell parser on purpose: `argv` is an array, so pipes,
redirections and substitutions cannot be smuggled into an approved command.

## Files

| File        | Contents                                                                                          |
| ----------- | ------------------------------------------------------------------------------------------------- |
| `nodes.rs`  | `ShellRunNode`, `ShellRunInput`, `ProcessToolOutput`, `shell_spec`, `skill_script`, `server_like` |
| `script.rs` | `script_source`: finds the script an interpreter will run, so the approval can show its code      |

## Pipeline: one `shell.run` call

1. **Tier.** The node always asks (`permission_for` → `ToolTier::Ask`) and defers the
   approval to its own `run`, so the approval request can carry the resolved program,
   working folder and script source instead of raw arguments.
2. **Program.** `argv[0] = skill:<key>/scripts/<file>` resolves through `skill_script`: the
   skill library locates the file, `ScriptRuntime` picks `uv` for `.py` or Node.js for
   `.js/.mjs/.cjs`, and the toolchain launcher builds the command. Otherwise `shell_spec`
   looks the program up in the connected toolchain folders first, then `PATH`, honouring
   `PATHEXT`, and fails with `PROGRAM_NOT_FOUND`.
3. **Working folder.** `cwd` (a path or `chat:`), else the conversation's working folder, else
   the chat folder for skill scripts. A folder outside the grants raises a folder approval.
4. **Environment.** `PATH` is set to the toolchain folders; skill scripts get
   `ZVS_SKILL_DIR`, `ZVS_SCRATCH_DIR` and `ZVS_CHAT_DIR`; Python gets `PYTHONUTF8=1` and
   `PYTHONIOENCODING=utf-8` so output decodes predictably.
5. **Approval.** A `ProcessPlanDto` (program, argv, cwd, reason, background flag, and the
   script source when known) goes to the approval service. The execution policy
   (`../policy`) may auto-allow it by rule; otherwise the user decides.
6. **Run.** Foreground runs go through `toolchain::run_process` with time and output limits
   and the step's cancellation token. `background: true` hands the process to
   `BackgroundShells` and returns its id at once; `notify: true` asks the chat to continue
   with the result when it ends.
7. **Artifacts.** Before and after the run the working folder is snapshotted and new or
   changed result files are registered (`../artifacts/README.md`), skipping working files
   whose path has a `_`-prefixed component.

## Algorithms

**Server detection** (`server_like`). `notify: true` is refused for servers and watchers,
because they never finish. A command is server-like when the program stem is a known server
program (`nodemon`, `vite`, `http-server`, `live-server`, `serve`, `webpack-dev-server`,
`cargo-watch`), when a script runner (`npm`, `pnpm`, `yarn`, `bun`) runs a script named `dev`,
`start`, `serve`, `watch` or `preview` (directly or via `run`), or when any argument is
`serve`, `runserver`, `watch`, `--watch`, `--watch=…`, `http-server` or ends in `.server`.

**Script source** (`script_source`). If `argv[0]`'s stem is an interpreter (`python*`, `py`,
`pyw`, `uv run`, `node`, `deno`, `bun`, `tsx`, `pwsh`, `powershell`), the first non-flag
argument with a matching extension is the script. Its text is read only if it resolves
inside the working folder and is below `PROCESS_SOURCE_MAX`; otherwise a placeholder says
it could not be read. With no script file, inline code from stdin is shown. The approval
then displays what will actually execute.

**Input limits.** `stdin` is capped at 1 MiB of characters and the composed `PATH` at
32 767 characters (the Windows limit).
