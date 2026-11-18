# zvs-cli · ui

Pure rendering: `draw(frame, &App)` reads the state and paints a `ratatui` frame. No input
handling and no I/O; every function takes `&App` (or a piece of it) and a width, and returns
lines or draws into an area.

## Layout

`mod.rs::draw` picks the screen by phase. While connecting it shows the banner and status.
In chat it splits the area vertically:

```
┌──────────────────────────────────┐
│ body     newest transcript lines │  Fill(1)
│ queued   messages waiting to send│  Length(queue height)
│ sheet    active picker or prompt │  Length(content, capped)
│ composer                         │  Length(input height)
│ status   mode · model · context  │  Length(1)
└──────────────────────────────────┘
```

Approvals and detail views share a sheet anchored to the bottom of the viewport. It occupies
at least 75% of the viewport and takes the place of the composer and status bar. The `/`
palette, `@` picker, and `?` reference use a compact sheet above the focused composer. Each
sheet ends with its keyboard hints. Wheel events over the chat scroll the transcript. The
chat scrollbar has a transparent track, and the composer and footer stay in place.

## Files

| File            | Contents                                                                                      |
| --------------- | --------------------------------------------------------------------------------------------- |
| `mod.rs`        | `draw`, the composer, the shared sheet, the queue, and the status line                        |
| `transcript.rs` | Transcript lines: messages, reasoning, tool call cards, subagent blocks, shell runs, spinners |
| `overlay.rs`    | Panel rows with an aligned key column, process output, selection                              |
| `prompt.rs`     | Approval, question and verification panels with their choices                                 |
| `modal.rs`      | Plan, subagent, and warning content for the shared sheet                                      |
| `banner.rs`     | The start-up wordmark, shrunk or dropped to fit the width                                     |
| `text.rs`       | `wrap`, `truncate`, inline styling and the minimal Markdown renderer                          |
| `theme.rs`      | `Colors` per `CliTheme`, the active theme, named styles, `ember` gradient, `dim`              |

## Algorithms

**Word wrap** (`text::wrap`). Each paragraph is split into words that keep their trailing
space. A word goes on the current line if it fits; otherwise the line is flushed. A word
longer than the whole width is broken character by character. Widths are terminal cells
(`unicode-width`), so CJK and emoji count as two.

**Markdown** (`text::markdown`). A line-oriented subset, enough for model answers: fenced code
blocks (language label, code background, wrapped to the width), `#` headings in the strong
style, `- ` / `* ` bullets rendered as `•` with a hanging indent, and inline `code` and
**bold** spans (`text::inline`, whichever marker comes first wins). Anything else is wrapped
as plain text.

**Spinner and elapsed time.** The runtime ticks every 100 ms; `transcript::spinner(frame)`
picks a frame from `SPINNER` and `TICKS_PER_SECOND` converts ticks to seconds for running
tool calls.

**Gradient** (`theme::ember`). The theme's gradient is a list of RGB stops; a position in
`0..=1` is mapped to the segment between two stops and interpolated linearly per channel.
The banner and the generation indicator use it.

**Subagent blocks.** A delegated run shows its last `SUBAGENT_STEPS_SHOWN` steps collapsed,
or all of them when details are expanded, and at most `REPORT_LINES` lines of its report.
