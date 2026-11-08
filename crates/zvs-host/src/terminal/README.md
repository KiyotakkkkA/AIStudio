# terminal

Interactive terminals in a conversation («Терминал» panel): real PTY sessions running
PowerShell, `cmd` or Git Bash in the conversation's working folder, streamed to an xterm view
in the renderer. Unlike `shell.run` these are the user's own shells; the model does not type
into them.

## Files

| File         | Contents                                                                                    |
| ------------ | ------------------------------------------------------------------------------------------- |
| `service.rs` | `TerminalService`: create, attach, write, resize, rename, clear, restart, close; scrollback |
| `pty.rs`     | `PtySpawner` / `PtyControl` / `PtyChild` ports; `NativePty` over `portable-pty` (ConPTY)    |
| `shells.rs`  | `ShellLocator` / `SystemShells`: which shells exist and how to start them                   |

## Shell detection (`SystemShells`)

| Shell          | Found at                                                                                |
| -------------- | --------------------------------------------------------------------------------------- |
| PowerShell     | `pwsh` on `PATH` or under Program Files, else Windows PowerShell in System32            |
| Command Prompt | `cmd.exe` in System32                                                                   |
| Git Bash       | `bin\bash.exe` next to the `git` found on `PATH`, else `Program Files\Git\bin\bash.exe` |

Git Bash starts as `chcp.com 65001 >/dev/null 2>&1; exec bash -i` so its output is UTF-8.
Every session gets `TERM=xterm-256color` and `COLORTERM=truecolor`.

## Session pipeline

1. `create` checks the per-conversation limit (`TERMINAL_SESSIONS_MAX`, 8), resolves the
   shell and the folder, spawns the PTY with the requested size, and opens an event stream.
2. A reader thread reads 8 KiB chunks, decodes them with `zvs_pipe::ConsoleText`, appends to
   the scrollback and emits each piece on the stream.
3. **Cursor query.** ConPTY asks the terminal for the cursor position (`ESC [6n`) at start-up
   and blocks until it gets an answer. The service answers the first query itself
   (`ESC [1;1R`) and strips it from the output, so a shell starts even before the renderer has
   attached.
4. `attach` returns the current scrollback and the stream id, so a re-opened panel shows the
   history and continues live. `write` sends keystrokes, `resize` resizes the PTY.
5. `restart` kills and respawns in place under a new _generation_; output from the old process
   is ignored. Closing a conversation closes its sessions.

## Scrollback

Kept up to `TERMINAL_SCROLLBACK_MAX` (256 KiB). `trim_scrollback` cuts from the front, moves
the cut to a UTF-8 boundary and then to just after the next newline, so the retained text
always starts at the beginning of a line.
