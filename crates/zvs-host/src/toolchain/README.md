# toolchain

The language runtimes the app itself needs: **uv** (Python, for `uvx` MCP servers and Python
skill scripts) and **Node.js** (for `npx` MCP servers and JS skill scripts). Each can be found
on the user's `PATH` or installed as a managed copy under app data. This module also owns the
generic process runner used by `shell.run`, skill scripts and dashboards.

## Files

| File             | Contents                                                                                           |
| ---------------- | -------------------------------------------------------------------------------------------------- |
| `definitions.rs` | `TOOLCHAIN_DEFINITIONS` (uv ≥ 0.4.0, Node.js ≥ 18.0.0), binary names per platform, `Version`       |
| `probe.rs`       | `VersionProbe` / `ProcessProbe`: runs `<binary> --version` (15 s timeout) and parses the version   |
| `releases.rs`    | `ToolchainResolver`: newest Node.js LTS from `nodejs.org/dist/index.json`, uv from GitHub releases |
| `service.rs`     | `ToolchainService`: list, scan, install, update, select, remove, connected runtimes, launching     |
| `launch.rs`      | `LaunchSpec`, `prepend_path`                                                                       |
| `process.rs`     | `run_process`: one child with stdin, time and output limits, cancellation; `prepare_scratch`       |

## Connected runtime

For each runtime the service records which copy is _connected_ (setting `CONNECTED_KEY`):
source (`path` or `managed`), executable path and version. Everything that launches a program
asks the service, so MCP servers, skills and the model's `shell.run` all use the same `node` /
`uv` the user sees on the «Расширения → Компоненты» page.

- **scan** probes every candidate on `PATH`; the first whose version meets the minimum is
  connected; otherwise the reason (not found, too old, failed to run) is reported.
- **install / update** resolves the newest build (Node.js: newest LTS for the platform's file
  key; uv: the newest release asset), downloads it through the download queue with its
  published SHA-256, unpacks it under the toolchain root (`unpack_download`) and connects it.
- **select** switches between found and managed copies; **remove** deletes a managed copy.

## Launching (`launch_command`, `program_dirs`)

`program_dirs` puts the connected runtimes' folders first, then the user's `PATH`.
`prepend_path(dir, PATH)` builds a `PATH` with `dir` first and any duplicate of it removed.
MCP stdio servers additionally get the isolated npm/uv cache env
(`../drivers/mcp/README.md`).

## Process runner (`process.rs`)

`run_process(spec, stdin, limits, cancel, not_started)`:

- spawns with `CREATE_NO_WINDOW`, writes `stdin` if given and closes it (interactive input is
  not supported);
- captures stdout and stderr concurrently, each up to `output_bytes` (default 64 KiB), then
  decodes them with `zvs_pipe::console_text`; a cut stream ends with an
  `[output truncated …]` note;
- ends on exit, on `timeout` (default 120 s, `TimedOut`) or on cancellation, killing the whole
  process tree;
- returns exit code, both outputs and how it ended. A program that cannot start reports the
  caller's `not_started` message.
