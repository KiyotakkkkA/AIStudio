# zvs-cli

The `aistudio` terminal client. It holds no state of its own beyond the user's theme and
shortcut files: every conversation, model, tool and permission lives in the desktop app, and
the CLI drives it over a per-user named pipe. The UI is `ratatui`, laid out as an Elm-style
loop: messages in, state update, requests out, redraw.

```
aistudio                       open a session in the current folder
aistudio --resume <id>         reopen a conversation
aistudio mcp add [-e K=V]... [-H "Name: value"]... <name> <url | command [args]...>
aistudio --version | --help
```

## Modules

| Module                      | Contents                                                                 |
| --------------------------- | ------------------------------------------------------------------------ |
| `args`                      | Argument parsing into `Args` and the Russian usage/error text            |
| `launch`                    | Finding, starting and connecting to the desktop app                      |
| `link`                      | `Link`: the request/response and event protocol over the pipe            |
| `runtime`                   | `run()`: terminal setup, the event loop, and the executor for `Request`s |
| `state`                     | `App` and its update function (see `src/state/README.md`)                |
| `ui`                        | Rendering of `App` into a frame (see `src/ui/README.md`)                 |
| `shell`                     | `!` mode: runs a `cmd` line in the working folder and streams its output |
| `mcp_add`                   | `mcp add` word splitting and `CreateMcpServerInput` building             |
| `theme_file`, `keymap_file` | The CLI theme and shortcut files in the app-data folder                  |
| `format`                    | Russian number, plural, date, duration and percent formatting            |
| `copy`                      | Every user-visible string                                                |
| `version`                   | Version and release channel label                                        |

## Pipeline: start-up and connection

1. `args::parse` turns the command line into `Args`. `mcp add` runs once without a UI and exits.
2. `runtime::run` enters the alternate screen and starts `session`.
3. `launch::connect` resolves the pipe name (`zvs_pipe::pipe_name`, keyed by the user's SID)
   and tries to open it. If nothing is listening, it looks for `zvs-studio.exe` next to the
   CLI or one folder up (`app_candidates`), starts it detached with `CLI_LAUNCH_ARG`, and
   retries every `RETRY_EVERY` (250 ms) for up to `LAUNCH_WAIT` (40 s). Progress is reported
   to the UI as `Msg::Connecting`.
4. `Link::open` performs the handshake: the client sends `Hello { version }`, the app answers
   `Welcome { version }`. A different `CLI_PROTOCOL_VERSION` on either side is a protocol
   mismatch and the session stops with a message, rather than talking a wrong dialect.
5. After the handshake the CLI asks the host to open a session for the working folder
   (`cli.open`), which reports whether the folder is granted, and loads providers and policy.

## Protocol

Both directions are newline-delimited JSON (`CliClientMessage`, `CliServerMessage` from
`zvs-contract`). The client sends `Call { id, channel, payload }`; the host answers with the
same result envelope the renderer gets (`{ ok, data | error }`), and `Link` resolves the
matching waiter by `id`. Host events (`HostEvent`) and CLI notices (verification prompts and
the like) arrive unsolicited and are forwarded into the event loop as `Incoming`. If the pipe
closes, every pending call fails with `copy::DISCONNECTED` and the UI shows the session as
closed. The host only accepts the channels listed in `zvs-host/src/cli/connection.rs`
(`CLI_CHANNELS`); anything else is refused.

## Pipeline: the event loop

`session` funnels four sources into one `mpsc` channel of `Signal`s:

- a blocking thread reading terminal events, batched up to `BATCH_MAX` per wake-up so a paste
  arrives as one batch (`translate` turns key events into `Msg`s; bracketed paste and
  "typed later" detection keep pasted newlines from submitting),
- a 100 ms tick (`Msg::Tick`) driving spinners, elapsed times and output polling,
- the connection task, then the host's events and notices,
- a 1 s poll of the theme file, so a theme changed in the desktop app applies live.

Each turn of the loop draws the frame, waits for the next signal, drains everything already
queued, and feeds each message to `App::update`. `update` returns `Vec<Request>`; `dispatch`
runs each request on the tokio runtime (an IPC call, a shell command, a clipboard write, a
desktop hand-off) and posts the result back as another `Msg`. State changes therefore happen
in exactly one place and are testable without a terminal.

## Shell mode

`!command` runs through `shell::run`: `cmd.exe /D /S /C` in the working folder (AutoRun skipped), stdout and stderr
decoded with `zvs_pipe::ConsoleText` (UTF-8, OEM or ANSI per line), streamed into the
transcript. Only the last `SHELL_LINES_KEPT` (2000) lines are kept. `ShellControl::stop` kills
the process tree.

## Tests

`cargo test -p zvs-cli`. `state/tests.rs` covers the update function message by message;
`ui` tests render into a `ratatui` test backend.
