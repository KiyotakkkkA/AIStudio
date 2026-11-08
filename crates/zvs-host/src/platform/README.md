# platform

Process-wide infrastructure and the ports to the OS. Nothing here knows about chats, tools or
providers; everything else depends on it. Ports are traits implemented for real by the shell
(`apps/studio/src-tauri/src/platform`) and by fakes in tests.

## Files

| File                                           | Contents                                                                               |
| ---------------------------------------------- | -------------------------------------------------------------------------------------- |
| `paths.rs`                                     | `PathEnvironment` → `StudioPaths`: every folder and file the app uses, dev vs packaged |
| `logger.rs`                                    | `Logger`, `FileLogger` (JSON lines, rotation), `silent_logger`, secret redaction       |
| `events.rs`                                    | `EventBus`, `StreamHandle`, the `EventSink` and `EventRecorder` ports                  |
| `recorder.rs`                                  | `FileEventRecorder`, enabled by `ZVS_RECORD_EVENTS` (for e2e replay)                   |
| `clock.rs`, `ids.rs`                           | `Clock` / `SystemClock` / `FakeClock`; UUID v7 ids and the injectable `IdGenerator`    |
| `shutdown.rs`                                  | `ShutdownCoordinator`: `Stop` → `Drain` → `Dispose`, capped at 5 s                     |
| `shell.rs`                                     | `ShellPort`: tray, notifications (`NotificationKind`, `NotificationDraft`)             |
| `theme.rs`, `window_state.rs`, `navigation.rs` | Theme source and palette mirroring, window geometry, workspace routes                  |
| `picker.rs`, `external.rs`                     | File/folder dialogs; opening URLs, revealing paths, default apps (`FileLauncher`)      |
| `audio.rs`                                     | `AudioInput` / `Recording` ports for voice input                                       |
| `device.rs`, `gpu.rs`, `metrics.rs`, `disk.rs` | Hardware readings, device tier, GPU choice, resource sampling, free space              |
| `archive.rs`                                   | Zip and tar.gz extraction with path sanitising                                         |
| `long_path.rs`                                 | The Windows `\\?\` extended-path prefix                                                |

## Logging and redaction

`FileLogger` writes `logs/studio.jsonl`, rotating to `studio.1…3.jsonl` at 5 MiB. Fields keep
the caller's order (they are a slice of `(&str, Value)`), followed by `timestamp`, `level`,
`scope`, `message`. Every string value is passed through `redact_text`, which replaces
anything shaped like a credential with a placeholder: Anthropic, OpenRouter and OpenAI-style
`sk-…` keys, GitHub tokens (`ghp_`, `gho_`, …), Slack tokens (`xox?-`), JWTs (`eyJ….….…`) and
similar. Nested JSON is redacted to a fixed depth.

## Events

`EventBus::open_stream()` returns a `StreamHandle`; `seq` starts at 0 per stream, timestamps
come from the injected clock, and `end(outcome)` closes it. Emitting after the end is logged
and dropped, never a panic. There is one primary sink (`attach`: the renderer's Tauri
channel) plus any number of subscribers (`subscribe`: each CLI connection). The recorder, when
enabled, sees every event first.

## Device tier and GPU (`device.rs`, `gpu.rs`)

`select_gpu` drops software adapters and Microsoft's basic adapter, picks the one with the
most dedicated video memory, and calls it _discrete_ when it is NVIDIA, has at least 2 GiB of
dedicated memory, or the machine also has a weaker adapter (a hybrid laptop).

`device_tier` from RAM and cores: **high** at ≥ 30 GiB and ≥ 12 cores, **medium** at ≥ 14 GiB
and ≥ 6 cores, else **low**; a discrete GPU raises the tier by one step (capped at high). The
tier drives model recommendations, such as the suggested voice model.

## Resource metrics (`metrics.rs`)

`ResourceMonitor` samples CPU, memory and, through the `GpuSampler` port (`NvidiaSmi` runs
`nvidia-smi` when present), GPU load and memory at a fixed interval. A GPU sample that takes longer than 5 s is
abandoned so a hung driver tool cannot stall the monitor.

## Free space (`disk.rs`)

`space_on(mounts, path)` picks the mount point that is the longest prefix of the path, so a
folder on a mounted volume reports that volume, not the drive it is mounted under.

## Archive extraction (`archive.rs`)

Zip is read directly (central directory, stored and deflated members; ZIP64 is refused);
tar.gz goes through `tar` + `flate2`. Every member name passes `safe_member_path`:
backslashes become `/`, leading `/` is dropped (rooted names are re-anchored under the
target), drive-prefixed names and directory entries are rejected, `.` is skipped and `..`
pops a component but may never climb above the target. Tar links and devices are skipped; a
zip symlink entry is written as a plain file. The result can never write outside the target
folder.

## Shutdown (`shutdown.rs`)

Services register named tasks in a stage. Stages run in order, `Stop` (stop accepting work),
`Drain` (let runs suspend), `Dispose` (close drivers, the sidecar, the database); tasks within
a stage run concurrently; the whole sequence is capped at `DEFAULT_SHUTDOWN_CAP` (5 s). The
first exit request starts it; further requests wait (see the shell README).
