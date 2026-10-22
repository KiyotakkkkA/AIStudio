---
task: TASK_027
title: zvs-jobd sidecar — protocol, lifecycle, health, cancel
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_026]
unblocks: [TASK_028, TASK_029]
estimate: 4 h
---

# TASK_027 — zvs-jobd sidecar — protocol, lifecycle, health, cancel

## Context

The napi addon handles fast Rust calls. This task builds the other half of decision D1: **the sidecar**, a separate Rust binary for work that runs for minutes and must be pausable, cancellable and survivable.

The argument for a second process is concrete. Indexing a corpus takes minutes and needs real progress; OCR pulls in native libraries that are the most likely thing in the stack to segfault; killing a stuck job should not mean killing the app. The Tasks page built in TASK_026 already promises pause and cancel — a sidecar is what makes those promises true rather than cosmetic.

Both the addon and the sidecar are thin adapters over `zvs-core` (TASK_017), so a capability can move between them without a rewrite. This task builds the transport and lifecycle only; the first real job is TASK_028.

## Why

Indexing, conversion and downloads all need long-running, cancellable, crash-isolated work. Without the sidecar those jobs would block or crash the host.

## Depends on

- `TASK_026` — Tasks page (live) and Runs & Logs (history).

## Read first

- `/docs/PASSPORT.md` §10 (sidecar protocol and lifecycle), §11 (jobs are kernel runs), §16 (packaging the binary as an extraResource)

## Scope

**In scope**
- `crates/zvs-jobd` binary with newline-delimited JSON-RPC over stdin/stdout
- Message set: `job.start`, `job.cancel`, `job.progress`, `job.done`, `job.error`, `ping`
- Host-side `SidecarDriver`: spawn, health-check, restart with backoff, hard-kill on cancel
- Bridging sidecar progress into kernel step events
- A trivial demo job proving the whole path
- Tests on both sides, including a deliberately crashed sidecar

**Out of scope** (do not do these, even if tempting)
- Real jobs — indexing is TASK_028, conversion TASK_043
- Packaging for other platforms (TASK_056)
- Multiple concurrent sidecar processes; one is enough, with an internal job queue

## Steps

1. `crates/zvs-jobd`: read newline-delimited JSON from stdin, dispatch to a handler, write responses to stdout. stderr goes to the host's log file, never parsed as protocol.
2. Each job runs on a tokio task with a `CancellationToken`, following the convention set in TASK_017. Progress is emitted as often as is useful but rate-limited — a message per file is fine, a message per byte is not.
3. `SidecarDriver` on the host: spawn on demand using a path from `paths.resourcesDir()`, hold the process, match responses to requests by id, and surface a clear error if the binary is missing (this is the failure a contributor hits first — name the expected path and the build command).
4. Health: periodic `ping` with a timeout. On timeout or unexpected exit, mark jobs in flight as failed, restart with exponential backoff, and log every restart. Never silently respawn in a tight loop.
5. Cancel: send `job.cancel`, then hard-kill after a grace period. A job that ignores cancellation must not be able to hang the queue.
6. Bridge to the kernel: a `job.*` node type whose step forwards sidecar progress as `progress` events on the run's stream, so a sidecar job appears on the Tasks page like any other run.
7. Demo job (`job.sleep` counting to N with progress) kept permanently as a smoke test, alongside `system.ping` and the demo stream.
8. Tests: request/response matching under concurrency; cancel stops a job promptly; a crashed sidecar is detected, jobs fail cleanly, and the process restarts; a missing binary produces the diagnostic error.

## Verify

```bash
cargo test -p zvs-jobd
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Start the demo job, watch progress on the Tasks page, cancel it and confirm it stops within the grace period. Kill the sidecar process from Task Manager mid-job and confirm the app survives, the job fails cleanly, and the sidecar restarts.

## Definition of done

- [x] Sidecar binary builds and speaks the protocol
- [x] Host spawns, health-checks, restarts with backoff and hard-kills on cancel
- [x] Sidecar progress appears as kernel step events on the Tasks page
- [x] A crashed sidecar never takes down the app — demonstrated
- [x] Missing binary produces a helpful diagnostic
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `crates/zvs-jobd` — a lib plus a thin binary. `protocol.rs` holds the wire types
  (`ping`, `job.start`, `job.cancel` in; `ping`, `job.progress`, `job.done`, `job.error` out),
  `jobs.rs` the dispatcher and the `job.sleep` demo job, `server.rs` the read loop. A job is
  identified by the `id` of the `job.start` that began it — `job.cancel` carries that same id,
  so there is no second identifier to keep in step. Responses are funnelled through an
  unbounded channel into a single writer, so concurrent jobs never interleave mid-line.
  Each job is a tokio task under a `CancellationToken`; progress is rate-limited to one message
  per 50 ms with the first and last report always delivered. stdout is protocol only; stderr is
  free-form and the host copies it into the log file.
- `SidecarDriver` (`host/drivers/sidecar/`) — spawn on demand from `paths.sidecarPath`, match
  responses to requests by id, forward progress to a per-job callback, `ping` with a timeout on
  an unref'd interval, kill on a silent sidecar, fail in-flight jobs on exit and restart on the
  next job after an exponential backoff (250 ms doubling to 30 s, reset by a successful ping).
  Cancel sends `job.cancel` and hard-kills the process if the job has not ended within the
  grace period. A missing binary raises `SIDECAR_UNAVAILABLE` naming the path and
  `pnpm build:rust`.
- `job.run` kernel node (`host/kernel/jobNodes.ts`) — forwards sidecar progress as `progress`
  events on the run's stream and a job's progress `message` as a `log` line, so a sidecar job
  shows up on the Tasks page like any other run, with the step context's `signal` carrying
  cancel all the way to the tokio task. `KernelServices` gained `jobs`.
- `system.demoJob` IPC channel and a third button on the host check card in Settings: it starts
  a one-node `job.run` run over `job.sleep` and reports its progress. Kept permanently as the
  smoke test for the whole path, alongside `system.ping` and the demo stream.
- `pnpm build:rust` now also builds `zvs-jobd --release` and copies it to
  `apps/studio/resources/sidecar/<triple>/`, ready to be packaged as an extraResource.
- Tests: `cargo test -p zvs-jobd` (wire names, rate limiting, typed errors, concurrent id
  matching over a duplex pipe, prompt cancel, unreadable lines and duplicate ids, EOF cancelling
  in-flight jobs); `apps/studio/tests/sidecar.test.ts` against a fake sidecar script
  (`test/helpers/fakeSidecar.mjs`) for concurrency, polite cancel, a job that ignores cancel and
  loses the process, a crash mid-job, a deaf sidecar killed by the health check, the missing
  binary diagnostic, the demo-job graph and the `job.run` progress bridge; and
  `test/native/sidecar.test.mjs`, run by `pnpm test:rust`, which drives the real release binary.

**Deviated from plan:**

- **No pause.** `job.cancel` is the only control message; the Tasks page has no resume either
  (TASK_026 dropped pause for the same reason). A paused job would need a checkpoint format,
  which belongs with the first real job.
- **One job id, not two.** The plan implies a separate job handle; the id of the `job.start`
  doubles as the job id, which keeps the driver's pending map single-keyed.
- **A hard kill on cancel takes the whole process with it,** and with it every other in-flight
  job — there is one sidecar by design. It only happens when a job ignores `job.cancel`, and it
  is logged at error level with the job name.
- **`job.run` needs approval the first time.** `PermissionService` defaults an ungranted tool to
  `ask`, and there is no way to declare a node `auto`, so the demo job (and any later `job.*`
  work) waits for approval on the Tasks page until `job.run` is granted. The demo button shows
  «ждёт подтверждения · Задачи» while it waits.
- **`zvs-jobd` depends on `zvs-core`,** so it drags in LanceDB and needs `protoc` on the path to
  build — the same requirement the workspace already has (CI installs it; a copy lives in
  `target/tools/protoc/bin`).

**Next session should know:**

- Progress on a run's stream has one slot: `TaskStore` overwrites `run.progress` from every
  `progress` event, so a `job.run` step's fine-grained counts replace the scheduler's
  nodes-completed counts while the job runs, and the scheduler's final `(1, 1)` lands after it.
  Fine for a one-node job graph; a mixed graph would jump between two scales. Give job progress
  its own event type before putting a `job.run` node in a larger graph.
- Add a job by adding an arm to `zvs_jobd::jobs::dispatch` and a params struct beside
  `SleepParams`; the transport, cancellation and progress plumbing need no changes. TASK_028's
  indexing job should report one message per file, not per chunk.
- The driver restarts lazily — on the next `run`, never on a timer — so a crashed sidecar with
  no work queued stays down, and the backoff cannot turn into a respawn loop.
- `resources/sidecar/` is gitignored like `resources/native/`; a fresh clone needs
  `pnpm build:rust` before the demo job works, and gets the diagnostic error if it forgets.
- Packaging the binary as an `extraResource` is still TASK_056; there is no electron-builder
  config in the repo yet.
