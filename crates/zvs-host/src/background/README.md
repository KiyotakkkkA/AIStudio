# background

Background shells: long-running processes the model starts from a conversation (a dev server,
a watcher, a build) that keep running after the tool call returns. The model checks on them
with `shell_output` and stops them with `shell_kill`; the user sees them in the processes
panel and the CLI's `/processes`.

## Files

| File         | Contents                                                                               |
| ------------ | -------------------------------------------------------------------------------------- |
| `service.rs` | `BackgroundShells`: start, list, read output, tail, kill, ended-shell hand-off, limits |
| `nodes.rs`   | The `shell_output` and `shell_kill` kernel tools and `status_line`                     |

## Limits (`ShellLimits`, defaults from `zvs-contract`)

| Limit                  | Default      | Effect                                                        |
| ---------------------- | ------------ | ------------------------------------------------------------- |
| `SHELLS_PER_OWNER_MAX` | 4            | Running shells per conversation; more fail with `OWNER_LIMIT` |
| `SHELLS_TOTAL_MAX`     | 16           | Running shells in the app; more fail with `TOTAL_LIMIT`       |
| `SHELL_LIFETIME_SECS`  | 3600         | A shell still running after one hour is stopped (`TimedOut`)  |
| `SHELL_BUFFER_MAX`     | 256 KiB      | Scrollback kept in memory per shell                           |
| `ENDED_KEPT`           | 8            | Ended shells kept per owner before the oldest are evicted     |
| `SHELL_TAIL_MAX`       | 16 000 bytes | Cap on a tail handed to the model                             |

## Pipeline: one shell

1. `start` checks the limits, spawns the process, and opens an event stream for it
   (`StreamHandle`), so the renderer can follow output live.
2. A reader task decodes stdout and stderr with `zvs_pipe::ConsoleText` (UTF-8, OEM or ANSI
   per line) in `READ_CHUNK` (8 KiB) reads and calls `append`.
3. The supervisor waits for the first of: exit, `kill`, the lifetime timeout, or owner close.
   After the process ends, the reader gets `DRAIN_GRACE` (2 s) to flush remaining output; a
   stop waits up to `STOP_GRACE` (15 s) for the task.
4. `finish` records status (`exited` with code, `failed`, `timedOut`, `stopped`), runs the
   optional `ShellFinish` hook (artifact detection, see `../artifacts/README.md`), emits a
   final status step and notifies `on_ended` listeners.

## Output buffer with stable offsets

Each shell keeps `buffer` (the retained scrollback), `start` (the absolute byte offset of the
buffer's first byte) and `total` (bytes ever written). `append` adds text, trims the front to
`SHELL_BUFFER_MAX` and advances `start` by what was trimmed. `read(since)` therefore works
with absolute offsets:

- `since >= total`: nothing new;
- `since < start`: the reader missed trimmed output, so it gets the whole buffer and
  `truncated = true`;
- otherwise it gets `buffer[since - start ..]`, with the cut moved forward to a UTF-8 char
  boundary.

It returns the next offset to ask for, so `shell_output` polling never repeats or skips text.

## Ended-shell hand-off

When a shell started with `notify` ends, the chat service tells the model on its next turn:
`take_notified` returns each ended shell once with the last lines of output (`clipped_tail`:
last N lines, then at most `SHELL_TAIL_MAX` bytes from the end) and its artifacts.
`peek_ended` reads without consuming; `demote` turns notifications off for an owner when the
user moves on. `evict` drops the oldest ended shells beyond `ENDED_KEPT` per owner.
