---
task: TASK_043
title: Office → PDF conversion in the sidecar
phase: 8 — Documents and the agent browser
status: todo
depends_on: [TASK_042, TASK_076]
unblocks: [TASK_044]
estimate: 3 h
---

# TASK_043 — Office → PDF conversion in the sidecar

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The viewer's one rendering path depends on this: DOCX, XLSX and PPTX are converted to PDF on open, then displayed like any other PDF. The conversion runs in the sidecar (TASK_027) as an ordinary kernel job, so it shows on the Tasks page with progress and can be cancelled — a large deck should not freeze the app.

The screenshot that prompted this feature was a `.docx`, so this is not a corner case: it is the format the user actually opens. Fidelity matters, which is why conversion beats JS rendering.

The decision below is the one flagged as PASSPORT §18 assumption 8, and it should be settled before any code is written, because the two options lead to completely different implementations.

## Why

Without it the viewer cannot open the documents the user works with, and the whole citation loop stops at plain text.

## Depends on

- `TASK_042` — Document service — open, hash cache, PDF path.

## Read first

- `/docs/PASSPORT.md` §20 (conversion path and caching), §18 assumption 8
- `/design/DocumentViewer.dc.html` — the `converted to PDF · cached` chip

## Scope

**In scope**
- Conversion job in the sidecar for DOCX, XLSX, PPTX
- Registration into TASK_042's kind registry so conversion is transparent to the caller
- Progress, cancellation and failure handling
- Cache integration: converted PDFs keyed by source content hash
- Text extraction from the converted PDF for indexing (feeds TASK_028)

**Out of scope** (do not do these, even if tempting)
- The viewer UI (TASK_044)
- Editing or exporting office formats
- Legacy `.doc` / `.xls` unless the chosen converter handles them free

## Steps

1. Settle the decision below first.
2. Implement the conversion behind a small trait in `zvs-core` so the backend can be swapped without touching callers.
3. Wire it as a sidecar job: `job.convert` taking a source path and a target cache path, emitting progress and honouring cancellation.
4. Register the office kinds in TASK_042's kind registry so `documents.open` transparently converts and returns the PDF descriptor, with a flag saying it was converted so the viewer can show the chip.
5. Cache by **source** content hash, so the same document is converted once regardless of path.
6. Failure handling: a document the converter cannot handle fails with `UNSUPPORTED_FORMAT` and a clear message naming the format — never a blank viewer.
7. Text extraction: pull text from the converted PDF and expose it for the indexing pipeline, so TASK_028's format registry gains office support by registering here rather than by editing.
8. Timeout: a conversion that hangs must be killed and reported, not left holding the queue.
9. Tests: a known DOCX converts and produces the expected page count; conversion is cached by source hash; cancellation kills the job and leaves no partial file in the cache; an unsupported input fails cleanly; a hanging conversion is timed out.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Office conversion needs a converter. Bundling headless LibreOffice gives near-perfect fidelity but adds roughly 300 MB to the installer — it would be by far the largest thing in it. The alternative is rendering DOCX and XLSX in JavaScript, which keeps the installer small but drifts on fonts, columns and page breaks. Which do you want?"
>
> **Why it matters:** It is the single biggest size decision in the product and it changes this task's implementation entirely.
> **Options:** (a) bundle LibreOffice — faithful, +300 MB; (b) JS rendering (docx-preview, SheetJS) — small, approximate; (c) use a LibreOffice already installed on the machine if present, fall back to JS rendering if not — small installer, best-effort fidelity.
> **If the user is unavailable:** take (c). It keeps the installer lean, gives full fidelity to anyone with LibreOffice installed, and degrades honestly with a visible 'approximate rendering' note when it falls back.

## Verify

```bash
cargo test -p zvs-core
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Convert the reference document from `/references` (a real .docx) and compare the result page by page against Word. Cancel a conversion of a large file and confirm no partial artefact is cached.

## Definition of done

- [ ] DOCX, XLSX and PPTX convert and open in the viewer path
- [ ] Conversion runs as a cancellable sidecar job visible on Tasks
- [ ] Cached by source hash; cancellation leaves no partial artefact
- [ ] Unsupported and hanging cases fail with clear messages
- [ ] Extracted text available to the indexing pipeline
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
