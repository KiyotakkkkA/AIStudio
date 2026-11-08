# cli

The host side of the `aistudio` terminal client (`crates/zvs-cli`): the pipe protocol server,
the allow-list of channels the CLI may call, CLI-only operations, the `PATH` installer port
and the shared CLI theme file. The pipe itself is opened by the shell
(`apps/studio/src-tauri/src/cli.rs`), which hands each accepted connection to `serve`.

## Files

| File            | Contents                                                                                      |
| --------------- | --------------------------------------------------------------------------------------------- |
| `connection.rs` | `serve`, `CliDispatch`, `CLI_CHANNELS`, framing and the outgoing queue                        |
| `mod.rs`        | CLI operations behind `cli.*` channels: open a session, link an account, add an MCP server, … |
| `notices.rs`    | `CliNotices`: broadcast of out-of-band notices (account verification shown / hidden)          |
| `installer.rs`  | The `CliInstaller` port (`UserPathInstaller` on Windows in `zvs-studio`), `NoCliInstaller`    |
| `path_list.rs`  | `path_contains`, `path_with`, `path_without` over a `;`-separated `PATH` value                |
| `theme.rs`      | `CliThemeFile`: the theme shared by the CLI and the «Параметры» page                          |

## Pipeline: one CLI connection (`serve`)

1. Split the stream; start a writer task that drains a bounded queue (`OUTGOING` = 4096
   messages) and writes one JSON object per line.
2. Read the first line. It must be `Hello { version }` with `CLI_PROTOCOL_VERSION`. On any
   other greeting the host still answers `Welcome` with its own version, so the client can
   tell the user which side is out of date, and closes.
3. Answer `Welcome`, subscribe the connection to the `EventBus` (every `HostEvent` becomes a
   `CliServerMessage::Event`), replay the notices currently shown, and follow new ones.
4. Loop over client messages:
   - `Call { id, channel, payload }`: if `channel` is in `CLI_CHANNELS`, dispatch it through
     the same IPC server the renderer uses and send back the result envelope with the same
     `id`; otherwise answer `PERMISSION_DENIED` (`CLI_CHANNEL_REFUSED`). Calls run
     concurrently, so a long `chat.send` does not block a `chat.cancel`.
   - `FocusVerification`: bring the account verification window to the front.
   - A line longer than `CLI_LINE_MAX` or one that is not valid JSON drops the client.
5. On EOF or write failure a cancellation token stops the writer, the notice follower and the
   event subscription.

The allow-list is the security boundary: the pipe is already restricted to the current user
(`zvs-pipe`), and the list keeps the CLI to chat, queue, goal, MCP, policy, approval and
process operations, without settings, secrets, storage or file-system administration.

## Installing the command

`install` asks the `CliInstaller` port to add the CLI folder to the user's `PATH` and turns on
the tray setting so the app stays reachable for the CLI after its window closes. `path_list`
compares entries case-insensitively, ignoring quotes, trailing slashes and `/` vs `\`, so
installing twice never adds a duplicate and uninstalling removes every spelling.

## Theme file

`CliThemeFile` stores `CliThemeDto` as JSON in the app-data folder. Writes go to a
`.json.tmp` file first and are renamed over the original, so the CLI (which polls the file
every second) never reads a half-written theme.
