# zvs-pipe

Windows plumbing shared by the desktop app, the `aistudio` CLI and the host's process runners:
the per-user named pipe the CLI talks over, console-output decoding, clipboard writes and the
foreground hand-off. Off Windows only the platform-neutral parts compile (`ConsoleText`,
`console_text`, `sddl_for`), so the host crates still build and test on Linux.

## Public surface

| Item                                 | Purpose                                                                                      |
| ------------------------------------ | -------------------------------------------------------------------------------------------- |
| `pipe_name()`                        | The CLI pipe name for the current user: `zvs_contract::cli_pipe_name(<user SID>)`            |
| `create_server(name, first)`         | One named-pipe server instance, restricted to the current user's SID                         |
| `connect(name)`                      | A client handle to an existing pipe                                                          |
| `sddl_for(sid)`                      | The security descriptor `D:P(A;;GA;;;<sid>)`: protected DACL, full access for the owner only |
| `current_user_sid()`                 | The process token's user SID as a string                                                     |
| `ConsoleText`, `console_text(bytes)` | Streaming decoder for child-process output (see below)                                       |
| `oem_text`, `ansi_text`              | One-shot decoding through the console OEM / ANSI code pages                                  |
| `copy_text(text)`                    | Puts Unicode text on the clipboard; returns the number of UTF-16 units written               |
| `allow_foreground()`                 | `AllowSetForegroundWindow` for any process, so the app can raise itself when the CLI asks    |

## Pipe security

The pipe name embeds the user SID, and every server instance is created with the DACL from
`sddl_for`, so another account on the same machine can neither open nor squat the pipe.
`first = true` sets `FILE_FLAG_FIRST_PIPE_INSTANCE`: if another process already owns the name,
creation fails instead of silently sharing it. The app's CLI listener
(`apps/studio/src-tauri/src/cli.rs`) creates the first instance and then a fresh instance per
accepted client.

## Console text decoding

Windows tools write output in whatever encoding they like: UTF-8, the OEM code page (`cmd`,
`dir`, most console tools) or the ANSI code page (many GUI-era tools). `ConsoleText` turns a
byte stream into text without knowing the encoding up front.

Algorithm, per `push(bytes)`:

1. Append to a pending buffer. `incomplete_tail` looks at the last 1–3 bytes and holds back a
   UTF-8 sequence that has started but not finished, so a multi-byte character split across
   two reads is not broken.
2. If everything before the held tail is valid UTF-8, emit it as-is. This is the fast path and
   the common case.
3. Otherwise fall back to _legacy_ decoding, line by line. Bytes are emitted up to the last line
   break; the partial line stays pending. A pending line longer than `LINE_HOLD_MAX` (256
   bytes) is flushed anyway so a tool that never prints a newline still streams.
4. Each legacy line is decoded with both the OEM and the ANSI page and scored: `+1` per letter,
   `-2` per replacement character, box-drawing character (U+2500–U+259F) or non-whitespace
   control character. The higher score wins. On a tie the page that won the previous line wins,
   which keeps one stream stable instead of flipping per line.

`finish()` flushes whatever is left. `console_text(bytes)` is push + finish for a complete
buffer. Users: the CLI's `!` shell mode, the host's terminal, background shells, toolchain
probes and the MCP stdio transport.

## Tests

`cargo test -p zvs-pipe`. The decoder tests run on every platform; the pipe and code-page
functions are exercised on Windows only.
