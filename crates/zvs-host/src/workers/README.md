# workers

`WorkerSupervisor`: long-running background tasks with status, restarts and backoff, shown on
the «Фоновые процессы» list (`workers.list`, `workers.restart`). Plugin accounts
(`../integrations/README.md`) are the main users; anything that should keep running and
recover from transient failures can register a worker.

## API

| Item                                                     | Purpose                                                                                                          |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `WorkerSpec::new(key, label)`                            | Identity and display name; `with_backoff` overrides the default `Backoff`                                        |
| `WorkerBody`                                             | `async fn(WorkerContext) -> Result<(), WorkerFault>`, run again on every restart                                 |
| `WorkerContext`                                          | `is_cancelled`, `cancelled()`, `sleep(d)` (returns false if cancelled), `ready()`, `heartbeat()`, `detail(text)` |
| `WorkerFault::retry(e)` / `::permanent(e)`               | A transient failure (restart) or a final one (stop with `Failed`)                                                |
| `spawn(spec, body)`                                      | Starts (or replaces) a worker under its key                                                                      |
| `restart(key)`, `stop`, `list`, `get`, `watch(listener)` | Control and observation                                                                                          |

## Supervision loop

```
Starting ──body runs──► (body calls ready()) Running
   ▲                         │
   │                 Ok(()) ─┴─► Stopped
   │          permanent fault ──► Failed (last_error kept)
   │          transient fault ──► Retrying (retry_at = now + delay) ──┐
   └──────────────────────────────────────────────────────────────────┘
```

**Backoff.** `delay(n) = initial × 2^(n−1)`, capped at `max` (defaults: 1 s doubling to
5 min). The failure count resets to 1 when the worker had been `ready()` for at least
`stable` (30 s) before failing, so a worker that ran fine for hours and then lost its
connection retries quickly instead of inheriting an old long delay.

**Generations.** Every `spawn` or `restart` gets a new generation number; status updates from
an older generation are ignored, so a slow-dying previous instance cannot overwrite the new
one's state.

**Stopping.** Cancellation is cooperative: the body sees `cancelled()`. A body that ignores it
is aborted after `STOP_GRACE` (5 s). A panic inside the body is caught and reported as a
failure (`WORKER_PANICKED`) instead of taking down the supervisor.
