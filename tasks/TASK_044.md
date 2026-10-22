---
task: TASK_044
title: Document viewer dock and selection → chat
phase: 8 — Documents and the agent browser
status: todo
depends_on: [TASK_043, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_044 — Document viewer dock and selection → chat

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

This task builds the viewer itself, per `/design/DocumentViewer.dc.html` — and it is deliberately **not a page**. It docks to the right of whatever is open, and its expand control blows it up to cover the window, matching the reference screenshot the user supplied.

Header: file icon, name, breadcrumb source line, page navigation (`‹ 2 / 2 ›`), zoom control, search, download, expand and close. Body: the page rendered on a dark backdrop with a real drop shadow. Footer: the conversion chip, page and size info, the selection length, and **Спросить об этом** — the action that sends a selection back into the conversation.

That last part closes the loop the whole retrieval design is built around: the model answers with a citation, the citation opens the viewer at the right page, and a selection there feeds the next turn. Chat was already built to survive being narrowed to 468 px (TASK_025) for exactly this.

## Why

It makes citations real and gives the user a way to read the source without leaving the conversation.

## Depends on

- `TASK_043` — Office → PDF conversion in the sidecar.

## Read first

- `/design/DocumentViewer.dc.html` — the dock, header controls, page canvas, selection highlight and footer
- `/design/Chat.dc.html` — the narrowed thread it sits beside
- `/docs/PASSPORT.md` §20, §14 (document text is untrusted)

## Scope

**In scope**
- `renderer/viewer/` dock: resizable, collapsible, expandable to full overlay
- `pdf.js` rendering with page virtualisation and zoom
- Text, markdown, code and CSV renderers
- Text selection producing a `DocumentSelectionDto`
- 'Ask about this' sending the selection to the chat composer
- Opening from a citation at the right page
- `ViewerStore` as a cross-cutting store with persisted width and zoom

**Out of scope** (do not do these, even if tempting)
- Annotation or editing
- Multi-document tabs — one document at a time in v1
- Printing

## Steps

1. `ViewerStore` in `renderer/stores/` — cross-cutting, not a feature store, since chat, runs and vector stores can all open it (PASSPORT §20). Holds the descriptor, page count, current page, zoom, dock width, expanded flag and selection. Width and zoom persist per user, not per document.
2. Dock layout: a resizable right panel with a drag handle, and an expanded mode covering the window. The page it sits beside must reflow, not be overlapped — chat already supports this.
3. PDF rendering with `pdf.js`: render to canvas per page, virtualise so a 200-page document does not render 200 canvases, and keep zoom crisp by re-rendering at the device pixel ratio rather than CSS-scaling a bitmap.
4. Text kinds: markdown rendered and sanitised, code through the kit's `./code-view`, CSV as a virtualised table. Sanitising is not optional — document content is untrusted (§14).
5. Selection: capture a selection across the rendered page and produce `DocumentSelectionDto` with document id, page, character range and text. For PDF, map from the text layer, which `pdf.js` provides — do not try to reconstruct it from canvas coordinates.
6. 'Ask about this': push the selection into the chat composer as an attachment chip, as drawn. The selection text must be fenced as untrusted when it eventually reaches the model.
7. Open-from-citation: a citation chip in chat opens the viewer at the cited page and highlights the cited passage, matching the mockup's accent highlight.
8. Header controls: page navigation with keyboard support, zoom presets and fit-width, in-document search with match highlighting and next/previous, download (through the OS save dialog), expand and close.
9. States: loading, converting (with the sidecar job's progress), unsupported format, file missing since it was cited, conversion failed.
10. Tests: selection mapping produces correct ranges; store persistence round-trips; virtualisation renders only visible pages; citation-open lands on the right page.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Ask a question in chat that produces a citation into the reference `.docx`. Click the citation, confirm the viewer opens at the right page with the passage highlighted, select a different passage, press 'Ask about this' and confirm it lands in the composer. Then expand to full overlay and back.

## Definition of done

- [ ] Viewer docks, resizes, expands and closes; the page beside it reflows
- [ ] PDF renders virtualised and crisp at every zoom level
- [ ] Text kinds render sanitised
- [ ] Selection produces a correct DTO and reaches the composer
- [ ] Citation opens the right page and highlights the passage
- [ ] Converting, missing and unsupported states handled
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
