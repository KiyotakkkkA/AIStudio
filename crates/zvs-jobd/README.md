# zvs-jobd

The ZVS AI Studio sidecar: one long-lived child process for work that runs for minutes and
must be pausable, cancellable and survivable.

It speaks newline-delimited JSON-RPC over stdin and stdout; stderr is free-form diagnostics
that the host copies into its log file and never parses as protocol. Every message carries an
`id`, and a job is identified by the `id` of the `job.start` that began it.

| Direction   | Message        | Payload                                      |
| ----------- | -------------- | -------------------------------------------- |
| host → jobd | `ping`         | `{ id }`                                     |
| host → jobd | `job.start`    | `{ id, job, params }`                        |
| host → jobd | `job.cancel`   | `{ id }` — the id of the `job.start` to stop |
| jobd → host | `ping`         | `{ id }`                                     |
| jobd → host | `job.progress` | `{ id, done, total, message? }`              |
| jobd → host | `job.done`     | `{ id, result }`                             |
| jobd → host | `job.error`    | `{ id, code, message }`                      |

Each job runs on its own tokio task under a `CancellationToken`, so several may be in flight
at once and a cancel stops one without touching the others. Progress is rate-limited to one
message per `PROGRESS_INTERVAL`; the first and last report always go out.

`job.sleep` is a demo job kept permanently as a smoke test for the whole path — it counts to
`steps`, reporting progress, and is cancellable between steps.

`job.document.convert` converts one office document to PDF with a LibreOffice executable the
host names: `{ executable, source, target, timeoutMs? }`. It works in a `.<target>.work`
folder beside the target, renames the PDF onto `target`, and always removes the work folder;
a failed, cancelled or timed-out conversion leaves no target. Set `ZVS_SOFFICE` to a
`soffice` executable to run the real conversion test against `tests/fixtures/two-pages.docx`.

Debug builds (`debug_assertions`, so the test profile) add two jobs and a flag for the host
driver's fault tests in `tests/driver.rs`: `debug.crash` exits the process mid-job,
`debug.hang` never finishes and ignores cancellation, and `--deaf` stops answering `ping`.
Release builds have none of them.
