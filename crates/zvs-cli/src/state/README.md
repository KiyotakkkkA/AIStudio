# zvs-cli · state

The whole CLI model: `App` and the pure update function that drives it. Nothing here touches
the terminal, the pipe or the clock directly, so every behaviour is tested by feeding `Msg`s
and asserting on the resulting state and `Request`s (`tests.rs`).

## The update contract

```
Msg ──► App::update(&mut self, msg) ──► Vec<Request>
                                           │
               runtime::dispatch runs each request, posts the result back as a Msg
```

- `Msg` is everything that can happen: keys, pastes, mouse, ticks, connection progress, host
  events, call results (`Msg::Opened`, `Msg::Providers`, …).
- `Request` is everything the CLI can ask for: IPC calls (`Send`, `Load`, `Attach`, `Policy`,
  `Servers`, `QueueAdd`, …), a shell command, a desktop hand-off, a clipboard copy.
- After every message `update` re-derives cached views (the subagent list, the visible
  transcript) so the renderer reads plain fields.

## Phases

`Phase::Connecting(status)` until the link is up, then `Phase::Access` when the working folder
is not granted yet (the user grants it for this conversation, always, or exits), then
`Phase::Chat`. `App::exit` set to `Exit::Quiet` or `Exit::Message` ends the session; the
runtime prints the message and the `--resume` hint after leaving the alternate screen.

## Files

| File             | Contents                                                                                      |
| ---------------- | --------------------------------------------------------------------------------------------- |
| `mod.rs`         | `App`, `Msg`, `Request`, `Phase`, model choice, goal state, and the update logic              |
| `input.rs`       | `Input`: the multi-line composer buffer, cursor movement by grapheme, word jumps, history     |
| `composer.rs`    | Sending: model selection checks, attachments, queued messages, `/goal` text                   |
| `attachments.rs` | `@` file references and image paths: discovery in the working folder, labels, references      |
| `palette.rs`     | The `/` command palette: commands, subcommands, argument slots and value completion           |
| `overlay.rs`     | Panels opened by commands (models, effort, resume, MCP, processes, permissions, theme, …)     |
| `prompt.rs`      | Approval, question and folder-access prompts built from host requests                         |
| `transcript.rs`  | `Transcript`: entries, the live (streaming) assistant turn, tool calls, subagents, shell runs |
| `shortcuts.rs`   | `Keymap`, `Shortcut`, `Chord`: configurable key bindings and their JSON form                  |
| `modal.rs`       | Modal content (help, keys) shown over the chat                                                |

## Algorithms

**Command palette matching** (`palette::matches`). Text after `/` is matched in three
modes. If the cursor sits in an argument slot (`/models <here>`), the slot's values are
offered (models, servers, conversations, themes). If the text is `<command> <partial>`, the
command's subcommands whose name starts with the partial are offered, sorted by name.
Otherwise every command is scored against the query: `0` for a prefix match, `1` for a
substring, `2` for a subsequence (all query characters appear in order), no match otherwise.
Results sort by score, then by their position in `COMMANDS`, which is the curated display
order. Argument values use the same score.

**Approval prompts** (`prompt::from_request`). A host `ApprovalRequestDto` is tried as a
folder request, then a file-change request, then a process request, falling back to a plain
yes/no. In `ExecMode::AutoBypass`, `rules_apply` decides whether the prompt offers "allow
this rule": only for process runs with a concrete argv that are not scripts, and for file
changes with at least one change.

**Transcript rebuild** (`Transcript::from_messages`). History from the host is flattened
into entries; tool calls are pulled out of `tool-call` steps, and calls whose parent is a
subagent run are grouped under it instead of shown inline. Live deltas append to `Live`
until the turn ends, then become a regular `Entry::Assistant`.

**Bounded buffers.** Shell output keeps the last `SHELL_LINES_KEPT` (2000) lines, process
output panels the last `OUTPUT_LINES_MAX` (2000), and a running process is polled every
`OUTPUT_POLL_TICKS` (10 ticks, one second).

**Compaction.** `/compact [focus]` sends `Request::Compact` and holds events like a send.
`compacting` drives the status line; the job's `end` reloads the dialog (`Request::Load`),
whose transcript shows `Entry::Compaction` after the boundary message, and the status reports
the saving. A `compaction` step with `trigger: auto` inside a turn switches the live label and
leaves a notice. A `recap` step from the recap job adds `Entry::Recap` under the last answer
once (`Transcript::add_recap`).

**Settings.** `/config` opens `Panel::Config` with two tabs: «Диалоги» toggles the host's
`autoCompact` and `recaps` (`Request::SetAssistant`), «Общие» toggles the local `auto_copy`
(`preferences.json`, watched like the theme). With auto-copy off a mouse selection stays on
screen; Ctrl+C copies it and Esc drops it.

**Keymap.** `Keymap::from_json` starts from the defaults and overlays the file's bindings:
a known shortcut id with a chord string that parses rebinds it, `null` unbinds it, and
anything else (unknown id, unparsable chord, other JSON) is skipped, so a hand-edited file
with a mistake keeps the default for that shortcut. The file carries `KEYMAP_VERSION`.
