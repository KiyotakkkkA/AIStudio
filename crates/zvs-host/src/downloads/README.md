# downloads

Model and engine downloads: the catalogue of what can be downloaded, the download queue, the
transfer itself (delegated to the `zvs-jobd` sidecar), installation after transfer, and disk
usage. The crate README's "Downloads" section is the short contract; this file maps it to code.

## Files

| File           | Contents                                                                                           |
| -------------- | -------------------------------------------------------------------------------------------------- |
| `service.rs`   | `DownloadService`: start, pause, resume, prioritise, cancel, remove, queue pump, execute, recover  |
| `catalogue.rs` | `CatalogueService` and its providers: `CuratedProvider`, `OllamaProvider`, offered engine archives |
| `curated.rs`   | The built-in curated list of models                                                                |
| `disk.rs`      | `DiskService`: usage per category (models, engines, stores, user data), free space, the probe port |

## Pipeline: one download

```
start ──► row "queued" ──► pump ──► run "download" (node download.fetch)
                                       │
                                       ▼
                       DownloadService::execute ──► sidecar job.download
                                       │             (resume from <target>.part,
                                       │              checksum, then rename)
                                       ▼
                             installer (RuntimeService) ──► row "succeeded"
```

1. **Start.** Refused when the catalogue does not know the ref, the item is already active,
   or the target volume lacks the artefact size plus 1 GiB. Otherwise a `queued` row is
   written and the pump is asked to run.
2. **Pump.** Fills free slots up to `downloads.concurrency` (1–8, default 2) with
   `next_queued`: `ORDER BY priority DESC, size_bytes ASC, created_at ASC`, so the user's
   priority wins, then small items finish first, then first come first served.
3. **Execute.** Each row becomes a kernel run with one `download.fetch` node, which calls
   `execute`; the sidecar streams progress. Byte counts are written to the database at most
   once per second; events go to the renderer as they arrive.
4. **Install.** If an installer is attached (engines: unpack and register), it runs after the
   transfer and before the row turns `succeeded`.

## Queue pump: serialised and re-entrant safe

`pump` may be called from many places at once (a start, a finished run, a resume). It sets a
`pump_requested` flag, then tries to take the `pumping` lock without blocking:

- if another caller holds the lock, it returns at once; the holder will see the flag;
- the holder loops `while pump_requested.swap(false)` running `pump_once`;
- after releasing the lock it checks the flag once more and loops if a request slipped in
  between, so no request is ever lost and `pump_once` never runs concurrently.

## Pause, cancel, shutdown, recovery

Pause and cancel record an _intent_ on the row and cancel the run. When the run ends,
the intent decides the status (`paused` keeps the `.part` file, `cancelled` deletes it). A
cancellation with no intent is an app shutdown: the row is left `paused` with its partial file.
`recover()` at boot requeues rows still `running` after a crash.

## Catalogue

`CatalogueService` merges its providers into one list:

- `CuratedProvider` — the static list in `curated.rs`;
- `OllamaProvider` — `/api/tags` from the first enabled Ollama provider (a live provider);
- engine archives offered by `RuntimeService` (`offer`), which `find` checks first.

The merged list and the installed snapshot are cached for 60 s. Live providers are skipped
unless `refresh` is requested, and a refresh that reaches nothing keeps the previous list
instead of showing an empty catalogue. `installed_key(kind, name)` matches catalogue items to
what is already on disk.

## Disk usage

`DiskService` sums each category's folder (`directory_size`), reports free space for the
downloads volume, caches a measurement for 30 s, and shares one in-flight measurement
between concurrent callers so opening several pages does not walk the disk several times.
