# notifications

Desktop notifications (Windows toasts through the shell's `Toaster`) for things that need the
user while the app is in the background, and the navigation that happens when a toast is
clicked. Also owns the tray setting.

## Files

| File         | Contents                                                                            |
| ------------ | ----------------------------------------------------------------------------------- |
| `service.rs` | `NotificationService`: run watcher, raising, coalescing, settings, click activation |
| `copy.rs`    | Titles and bodies (Russian), `clip` for long labels                                 |

## What raises a notification

`watcher()` is registered as a `RunWatcher`; it looks only at `Approval`, `Question` and `End`
events and maps the run to a subject (a chat conversation or a download):

| Run kind | Event                 | Kind       | Title                            | Opens            |
| -------- | --------------------- | ---------- | -------------------------------- | ---------------- |
| chat     | approval request      | `approval` | approval needed, naming the tool | the conversation |
| chat     | question to the user  | `approval` | the first question's title       | the conversation |
| chat     | run ended ok / failed | `turn`     | answer ready / answer failed     | the conversation |
| download | ended ok / failed     | `download` | download finished / failed       | downloads        |

Cancelled runs never notify. Each kind can be turned off in settings (`enabled(kind)`);
`announce` raises ad-hoc notices (for example from the CLI).

## Coalescing

`raise` remembers `(kind, tag)` with its time. A second notification for the same pair within
`COALESCE_WINDOW_MS` (5 s) is still delivered but marked `silent`, so a burst of approvals in
one conversation updates the toast without a burst of sounds.

## Click → navigation

A toast carries a `NotificationTargetDto`, encoded as a `zvs://` URL by the shell. When it is
activated:

- a repeated activation of the same target is ignored;
- while the renderer is not ready yet (cold start from a toast), the target is held as
  _pending_ and handed over by `take_pending` once the UI asks for it;
- otherwise a navigation step is emitted on a fresh event stream and the renderer opens the
  target; `clear(target)` forgets it.
