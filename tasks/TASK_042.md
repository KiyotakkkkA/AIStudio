---
task: TASK_042
title: Document service — open, hash cache, PDF path
phase: 8 — Documents and the agent browser
status: done
depends_on: [TASK_026, TASK_076]
unblocks: [TASK_043]
estimate: 3 h
---

# TASK_042 — Document service — open, hash cache, PDF path

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The app cites documents in chat but cannot show them. This task builds the service behind the document viewer — the surface that, per decision D8, is **not a page**: it docks beside whatever is open and expands to cover the window.

The rendering strategy is one path, not several. PDF, images, markdown, code and CSV render natively; DOCX, XLSX and PPTX are converted to PDF by the sidecar on open (TASK_043) and then take the PDF path. That keeps the viewer simple and the office layout faithful.

The cache is keyed by **content hash**, not by path, using the hashing already in `zvs-core` (TASK_017). Reopening a document is then instant, moving a file does not invalidate its cache, and the cache directory is safe to delete at any time. This task builds everything except the conversion itself.

## Why

Citations that cannot be opened are not much better than no citations, and the viewer is what closes the retrieval loop from answer back to source.

## Depends on

- `TASK_026` — Tasks page (live) and Runs & Logs (history).

## Read first

- `/design/DocumentViewer.dc.html` — the header, page navigation, footer and the conversion chip
- `/docs/PASSPORT.md` §20 (the viewer), §14 (document text is untrusted input)

## Scope

**In scope**
- `document` registry: opened documents with hash, kind, page count, source
- Open by path, by citation, or by a vector-store document id
- Native path for PDF, images, markdown, code and CSV
- Hash-keyed cache directory with an LRU size ceiling
- Page serving for the renderer
- Channels and DTOs
- Tests

**Out of scope** (do not do these, even if tempting)
- Office conversion (TASK_043) — register the hook and return `UNSUPPORTED_FORMAT` for now
- The viewer UI (TASK_044)
- OCR of scanned PDFs
- Editing documents

## Steps

1. `document` table: id, contentHash, sourcePath, kind, pageCount, sizeBytes, cachedPdfPath, openedAt. Keyed by hash so the same file at two paths is one document.
2. `DocumentService.open(ref)` where ref is a path, a citation from a chat message, or a vector-store document id. Resolve to a file, hash it, look up the cache, and return a descriptor with the kind and page count.
3. Kind detection by extension and by content sniffing — never trust the extension alone, since a mislabelled file should fail cleanly rather than render as garbage.
4. Cache under `cacheDir/documents/<hash>.<ext>` per PASSPORT §20, with an LRU eviction against a size ceiling that will become a setting in TASK_052. Eviction must never delete a file for a document that is currently open.
5. Page serving: for PDFs, hand the renderer the file bytes (or a range) so `pdf.js` can render; for text kinds, return the decoded content with its detected encoding. Cap the size of what is returned in one call and paginate large text.
6. Untrusted-input handling: mark every extracted text payload as untrusted at the DTO level, so TASK_044 and any future model call fence it rather than treating it as instruction. This is the §14 rule and it starts here.
7. Channels: `documents.open|page|close|list`, plus `documents.reveal` to show a file in the OS file manager.
8. Tests: hash-keyed cache hits across two paths to the same content; kind detection resists a wrong extension; eviction never removes an open document; oversized text is paginated; an unsupported format returns `UNSUPPORTED_FORMAT` cleanly.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Open a PDF and a markdown file through the channel and confirm descriptors and content come back. Copy the PDF to a second path, open it, and confirm the cache is reused rather than duplicated.

## Definition of done

- [x] Documents open by path, citation and store-document id
- [x] Cache keyed by content hash with safe LRU eviction
- [x] Kind detection resists mislabelled extensions
- [x] Extracted text marked untrusted at the DTO level
- [x] Unsupported formats fail cleanly
- [x] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Contract `zvs_contract::documents` (`DocumentKind`, `DocumentTextEncoding`, `DocumentRef`,
  `DocumentDto`, `DocumentHandle`, `DocumentPageInput`, `DocumentPageDto`); Zod in
  `packages/shared/src/documents.ts`; fixtures in `fixtures/documents/`. Channels
  `documents.open`, `documents.page`, `documents.close`, `documents.list`, `documents.reveal`.
- Migration `0008_document.sql`, `DocumentRow` and `DocumentRepository` (`record_opened`
  upserts on the unique `content_hash`, so the id survives a second path).
- `zvs-host/src/documents/`: `kind.rs` (extension claim checked against the content
  signature, shiki language ids, a PDF page estimate that does not parse), `text.rs`
  (encoding detection and line pagination), `cache.rs` (`<hash>.<ext>` files, temp file then
  rename, the pure `eviction_plan`), `service.rs` (`DocumentService` with open-handle counts,
  the `DocumentConverter` hook and rebuilding a deleted cache). On `Services.documents`; the
  composition root shares one `ForbiddenFolders` between folder access and documents.
- `platform::external::reveal_in_file_manager` behind a `PathRevealer` port
  (`explorer /select,` on Windows).
- Tests: `documents/{kind,text,cache}.rs`, `documents/tests.rs` (cache hits across two paths,
  a moved source, byte ranges, mislabelled and unsupported files, the converter hook, eviction
  with open handles, pagination, encodings, rebuilding the cache, store-document and citation
  opens, forbidden and relative paths, reveal), the repository, and the channels in
  `ipc/documents.rs`.

**Deviated from plan:**
- The hash is the `zvs-core` blake3 `content_hash` (as the task says) rather than SHA-256 as
  PASSPORT §20 used to say; §20 is updated.
- The table column is `cached_file` (a file name in the cache folder), not `cachedPdfPath`:
  every kind is cached, not only PDFs, and a relative name survives a moved cache folder.
- Native kinds are copied into the cache as well, so a moved file keeps serving and the cache
  is the only thing `documents.page` reads.
- `documents.page` serves *parts* (`index` < `partCount`), not PDF pages: 4 MiB byte ranges
  for PDF and images, 256 KiB line-aligned text parts for text kinds. `pageCount` is what the
  viewer shows (an estimate for PDFs, since the host never parses them per §10).
- Eviction deletes the row as well as the file, so `documents.list` shows what is cached.
- Path opens are refused in forbidden folders (app data, system folders and the rest of §14);
  citations resolve through the stored `vector_document` path and ignore the path they carry.
- Added `encoding_rs = "=0.8.42"` (latest stable) for UTF-16 and Windows-1251.
- Rust 1.99's clippy flags `unnecessary_sort_by`; the two existing sorts in
  `toolchain/process.rs` and `toolchain/releases.rs` now use `sort_by_key` with `Reverse` so the
  gate passes.

**Next session should know:**
- TASK_043 plugs the converter in through `DocumentServiceOptions::converter`; the hook writes
  to a temp file in the cache folder and the service renames it to `<hash>.pdf`. Conversion as
  a kernel job (`documents.convert`, a run handle) is still TASK_043's.
- The cache ceiling is `DOCUMENT_CACHE_CEILING` (2 GiB) in the options; TASK_052 turns it
  into a setting.
- `pnpm --filter studio typecheck` already fails on `main` at
  `tests/renderer/fileChanges.test.ts:59` (a `JSONType` mismatch), which this task did not touch.
