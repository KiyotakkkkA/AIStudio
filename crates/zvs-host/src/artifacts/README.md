# artifacts

Files the model produced for the user: detection after a tool call, registration under an
owner (a conversation or a library), resolution of artifact ids back to paths, opening and
revealing them, live dashboards, and the gallery with thumbnails. The module root is
`../artifacts.rs`; this folder holds `gallery.rs` and its tests. HTML artifacts are served
to the viewer by `../artifact_pages.rs`.

## Detection: which files did a tool call produce?

Process tools cannot report their outputs, so the host infers them:

1. Before the call, `snapshot(folder)` records `(size, mtime)` for every file, walking at
   most `SNAPSHOT_DEPTH` (4) levels and `SNAPSHOT_FILES_MAX` (4096) files so a huge workspace
   cannot stall a tool call.
2. `mentioned_paths(argv, cwd)` collects arguments that look like artifact paths (not flags,
   resolved against the working folder).
3. After the call, `produced(folder, before, mentioned, since)` returns:
   - mentioned paths that exist and were modified at or after the call started, in argument
     order, then
   - every file whose `(size, mtime)` differs from the snapshot (new or changed), sorted,
   - filtered to `ARTIFACT_EXTENSIONS` (documents, spreadsheets, slides, Markdown, HTML,
     images, CSV, zip), at most `ARTIFACT_MAX_BYTES` (512 MiB), capped at
     `ARTIFACTS_PER_CALL` (16).

## Ids and resolution

An artifact id is `<owner>/<name>`. `keep(owner, run, files)` records the outputs through the
folder service and returns `ArtifactDto`s. `resolve(id)` validates the id, maps the owner to
a library folder or to the recorded output, canonicalises the path and requires a regular
file. For library owners the file must sit directly in the library folder
(`path.parent() == folder`), so `..` or nested names cannot escape it. Every user action
(`open`, `reveal`, `application`, `remove_artifact`, thumbnails) goes through `resolve`.

## Libraries

Named folders (for example `dashboards`) registered with `with_library`. Their files appear in
the gallery even if no conversation produced them; `adopt` registers existing files. Skill
scripts receive library folders through environment variables named by `library_env`.

## Dashboards

HTML files in the `dashboards` library are live pages: `ArtifactStore` implements
`LivePages` over the dashboard runner (`../dashboards`), exposing state, data, refresh and the
refresh interval. An automatic refresh is skipped when the data is younger than
`AUTOMATIC_FRESHNESS` (10 s); a manual one always runs. Running a dashboard script requires
approval (`approve_dashboard`).

## Gallery and thumbnails (`gallery.rs`)

`gallery()` lists library folders and conversation outputs as `GalleryItemDto`s, newest
first, capped at `GALLERY_ITEMS_MAX`; `kind_of` classifies by format (dashboard, document,
spreadsheet, image, archive, …).

`thumbnail(id)`:

- Images (raster up to 8 MiB, SVG up to 2 MiB) and PDFs (up to 16 MiB) are returned as data
  directly; larger files get no thumbnail.
- HTML is rendered to PNG by `HeadlessCapture`: the first installed Chromium-family browser
  (Edge, Chrome, Chromium, Brave, Yandex, Vivaldi, Opera) runs `--headless=new` at 1280×800
  with extensions, GPU and background networking off and a 4 s virtual-time budget, against
  the page server's `?preview` URL, with a 30 s timeout.
- Captures are cached as `<path hash>-<mtime ms>-<size>.png`, so an edited file gets a new
  key and old captures for the same path are deleted (`forget_thumbnails`). Concurrent
  requests are serialised by `capture_gate` with a re-check after acquiring it, so one file is
  captured once. A capture is written to `.part.png` and renamed, so a failed capture never
  leaves a broken cache entry.
