# dashboards

Live dashboards: HTML pages built by the bundled `dashboard` skill that can refresh their data
by re-running the fetch scripts the model wrote. Only scripts the user approved (by content
hash) ever run. The gallery and the artifact page server reach this service through
`ArtifactStore` (`../artifacts/README.md`).

## Layout on disk

```
<dashboards library>/
  sales.html                     the page shown to the user
  .zvs/                          LIVE_DIR
    settings.json                per-dashboard preferences: interval, approved script hashes
    sales/                       one folder per page stem
      meta.json                  { interval, fetch: [{ script, output }] }
      fetch_orders.py, data.json …
```

A page is _live_ when `.zvs/<stem>/meta.json` exists; otherwise it is a static artifact
(`NOT_LIVE`). Script and output paths in `meta.json` must resolve inside the stem folder
(`contained`), and at most `DASHBOARD_SCRIPTS_MAX` (16) scripts are read.

## Files

| File        | Contents                                                                                  |
| ----------- | ----------------------------------------------------------------------------------------- |
| `mod.rs`    | `DashboardService`: status, page state, data, refresh, approve, interval, settings        |
| `runner.rs` | `ProcessDashboardRunner`: runs fetch scripts and the skill's `scripts/build_dashboard.py` |

## Approval by content hash

Each fetch script is identified by its SHA-256. `approve` stores the hashes of the scripts as
they are now; `run` refuses with `NEEDS_APPROVAL` if any current script's hash is not in the
approved set. Editing a script (by the model or by hand) changes its hash, so the page asks for
approval again (`STALE_APPROVAL` in the status) instead of running code the user never saw.

## Pipeline: refresh

1. Take the stem's async lock. If another refresh was already running (`try_lock` failed) the
   caller waits for it and returns its result rather than running a second time.
2. If the last refresh finished within `fresh_within`, return the current status. Automatic
   refreshes from an open page pass 10 s; a manual refresh passes zero.
3. Check approvals, then run each fetch script in its folder (`FETCH_TIMEOUT` 120 s), writing
   its declared output, then rebuild the page with `build_dashboard.py` (`BUILD_TIMEOUT`
   300 s). Scripts run through the toolchain launcher (`uv` / Node.js).
4. Record `refreshing`, `finished` and the error message in the in-memory runtime so the page
   state can show progress and the last failure.

## Interval

A page may refresh on a timer. The effective interval is the user's preference, else
`meta.json`, clamped to `DASHBOARD_INTERVAL_MIN`..`DASHBOARD_INTERVAL_MAX` (15 s to 24 h);
`set_interval` rejects a value outside that range instead of clamping it. Approving scripts
whose hashes changed since the status was read fails with `STALE_APPROVAL`, so the user always
approves what they were shown. Preferences are written to `settings.json`; entries with
neither an interval nor approvals are dropped.
