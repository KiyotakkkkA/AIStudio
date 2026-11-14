# cli

The host side of the `aistudio` terminal client (`crates/zvs-cli`): the pipe protocol server,
the console gate (connection scope and held confirmations), CLI-only operations, the `PATH`
installer port and the shared CLI theme file. Which channels the console may call is decided by
the policy table in `../ipc/policy.rs` (PASSPORT §7). The pipe itself is opened by the shell
(`apps/studio/src-tauri/src/cli.rs`), which hands each accepted connection to `serve`.

## Files

| File             | Contents                                                                                      |
| ---------------- | --------------------------------------------------------------------------------------------- |
| `connection.rs`  | `serve`, `CliDispatch`, framing, the outgoing queue and the per-connection event filter       |
| `connections.rs` | `CliConnections`: open connections, owned dialogs, held confirmations                         |
| `gate.rs`        | `admit`, `settle`, `sees`: dialog scope and loosening checks for a `Caller::Cli`              |
| `mod.rs`         | CLI operations behind `cli.*` channels: open a session, link an account, add an MCP server, … |
| `notices.rs`     | `CliNotices`: broadcast of out-of-band notices (account verification shown / hidden)          |
| `installer.rs`   | The `CliInstaller` port (`UserPathInstaller` on Windows in `zvs-studio`), `NoCliInstaller`    |
| `path_list.rs`   | `path_contains`, `path_with`, `path_without` over a `;`-separated `PATH` value                |
| `theme.rs`       | `CliThemeFile`: the theme shared by the CLI and the «Параметры» page                          |

## Pipeline: one CLI connection (`serve`)

1. Split the stream; start a writer task that drains a bounded queue (`OUTGOING` = 4096
   messages) and writes one JSON object per line.
2. Read the first line. It must be `Hello { version }` with `CLI_PROTOCOL_VERSION`. On any
   other greeting the host still answers `Welcome` with its own version, so the client can
   tell the user which side is out of date, and closes.
3. Answer `Welcome` and register the connection with `CliConnections`, together with the
   client the shell identified (`CliClient`: process id, image path, `recognised`). The
   connection id, process id and image are logged. Subscribe the connection to the `EventBus`
   and replay the notices currently shown, then follow new ones. The writer task drops every
   event whose stream is not a run of one of the connection's dialogs (`gate::sees`, resolved
   once per stream). It runs in the writer task, never inside `EventSink::deliver`, so an
   emitter that holds the database is never re-entered.
4. Loop over client messages:
   - `Call { id, channel, payload }`: if the policy row admits the console, dispatch it through
     the same IPC server the renderer uses as `Caller::Cli { connection }` and send back the
     result envelope with the same `id`; otherwise answer `PERMISSION_DENIED`
     (`CLI_CHANNEL_REFUSED`). Calls run concurrently, so a long `chat.send` does not block a
     `chat.cancel`, and a call held for a confirmation does not block the others.
   - `FocusVerification`: bring the account verification window to the front.
   - A line longer than `CLI_LINE_MAX` or one that is not valid JSON drops the client.
5. On EOF, write failure or «Отключить» in the studio (`cli.connections.disconnect` cancels the
   connection's token) the writer, the notice follower and the event subscription stop, and
   the connection is removed. A confirmation it was waiting for is withdrawn.

## The console gate (`gate::admit`)

`IpcServer::dispatch` runs it before the handler for every `Caller::Cli` call:

1. The connection must still be open (`CLI_CONNECTION_CLOSED`).
2. The row's `scope` names the dialog the call reaches: a conversation id field, a run id
   (its `subjectId`), a background process id (its owner) or an owner id. A dialog the
   connection does not own is refused with `CLI_DIALOG_REFUSED`. The exception is
   `chat.conversations.get`: opening an existing dialog is always allowed without a studio
   confirmation, including when no studio sink is attached.
3. For a `loosen` row the payload decides whether this call loosens: a less strict mode or
   another judge for `execPolicy.set`, any `execPolicy.allowRule`, `always` on `runs.approve`
   and `folders.answer`. Such a call waits in `CliConnections::confirm` until the studio
   answers.

After a successful `chat.conversations.create` or `chat.conversations.get` the dialog becomes
the connection's (`gate::settle`). Failed calls do not grant ownership.

## Held confirmations (`CliConnections::confirm`)

With no studio sink attached to the event bus the call fails at once (`CLI_STUDIO_REQUIRED`).
Otherwise the host stores a `CliConsentDto`, brings the studio window forward
(`ShellPort::show_studio`), sends the connection a `consent { pending: true }` notice and emits a
`step` event with domain `cli`. The studio lists held changes with `cli.consents.list` and
answers with `cli.consents.answer`. A confirmation lets the call proceed. A decline
(`CLI_CONSENT_DECLINED`), 60 s without an answer (`CLI_CONSENT_EXPIRED`) or a closed
connection fail it with `PERMISSION_DENIED`. `cli.confirmLoosening = false` lets mode and judge
changes other than to `no-restrictions` through without a confirmation.

## The client image is not a boundary

The shell reads the client's process id and image path on connect and marks the connection
`recognised` when the image is the installed `aistudio.exe` (`zvs_pipe::same_image`). The
pipe cannot tell the real client from another process of the same user, and such a process
can run the real client, so this raises the bar and labels the confirmation. It does not
grant anything. The rules above apply to every console connection alike.

## Installing the command

`install` asks the `CliInstaller` port to add the CLI folder to the user's `PATH` and turns on
the tray setting so the app stays reachable for the CLI after its window closes. `path_list`
compares entries case-insensitively, ignoring quotes, trailing slashes and `/` vs `\`, so
installing twice never adds a duplicate and uninstalling removes every spelling.

## Theme file

`CliThemeFile` stores `CliThemeDto` as JSON in the app-data folder. Writes go to a
`.json.tmp` file first and are renamed over the original, so the CLI (which polls the file
every second) never reads a half-written theme.
