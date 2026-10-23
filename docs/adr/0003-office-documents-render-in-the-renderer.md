# ADR 0003 — Office documents render in the renderer, LibreOffice is removed

- **Status:** accepted, 2026-10-23
- **Task:** TASK_044 (document viewer)
- **Supersedes:** PASSPORT §18 assumption 8 and the TASK_043 conversion path (D8 «office
  converted to PDF by the sidecar»)

## Context

TASK_043 converted DOCX, XLSX and PPTX to PDF with headless LibreOffice: a third toolchain
runtime (detected in Program Files or `PATH`, or a managed copy unpacked from the official
MSI with `msiexec /a`), a kernel node `document.convert`, a sidecar job
`job.document.convert` and the `zvs-core` `OfficeConverter` / `SofficeConverter`. The viewer
was to show the converted PDF and fall back to JavaScript rendering only without
LibreOffice.

On the user's machine the managed LibreOffice 26.8.1 crashed on almost every start. Run with
the exact arguments the sidecar uses (`--headless --convert-to pdf`, a fresh
`-env:UserInstallation` profile), `soffice.com` exited with code 0, wrote no PDF and left a
crash dump in its `program` folder. Twelve dumps were collected on 2026-10-23. The failure did
not depend on the work-folder layout, on whether the profile was fresh or reused, or on how
standard streams were attached; one manual run out of six succeeded. Antivirus interference
was ruled out by the user. The app reported every crash as «Формат не поддерживается», which
told the user nothing.

The integration also cost a ~360 MB download, an MSI unpacking path, a second rendering path
in the viewer and a conversion cache, for a feature that did not work on the one machine it
was verified on.

## Decision

1. **LibreOffice is removed.** No `libreoffice` toolchain runtime, no `system` toolchain
   source, no MSI extraction, no `document.convert` node, no `job.document.convert` sidecar
   job, no `OfficeConverter` in `zvs-core`, no `DocumentConverter` hook in the document
   service. «Расширения → Компоненты» lists Node.js and uv only.
2. **Office documents render in the renderer.** `documents.open` returns DOCX, XLSX and PPTX
   as `kind: "office"` with their `officeFormat`, and serves the original bytes in 4 MiB parts
   like a PDF. The viewer renders them itself:
   - DOCX with `docx-preview` (`=0.4.1`), one section per page, alt chunks and comments off,
     embedded fonts ignored; its generated stylesheet is applied as a constructed stylesheet
     so the CSP keeps refusing inline `<style>`.
   - XLSX with a reader of our own over `jszip` (`=3.10.2`): every sheet, shared and inline
     strings, booleans and numbers, at most 5 000 rows and 200 columns per sheet, shown in the
     same virtualised grid as CSV. SheetJS was not taken: its npm release is frozen at 0.18.5
     and current builds ship only from its own CDN.
   - PPTX as the text of each slide, in presentation order.
3. **The contract drops `DocumentRendering`.** `DocumentDto` loses `rendering`; the rule is
   that `officeFormat` is present exactly when `kind` is `office`. Migration
   `0011_document_office_native` deletes registry rows that were converted to PDF, so a list
   never returns a row the new contract refuses; their cached `<hash>.pdf` files are orphans
   and leave on the next eviction.
4. **The viewer is honest about fidelity.** Office documents carry a «приближённое
   отображение» chip; a presentation also says that only slide text is shown.

## Consequences

- Office layout is approximate: fonts, columns, page breaks and charts may differ from Word,
  Excel or PowerPoint. «Открыть в …» on a chat artifact hands the file to the installed
  application when the exact layout matters.
- Page numbers of a DOCX come from docx-preview's page breaks, so a citation's page and the
  page shown can disagree on long documents; the cited passage is still located by its text.
- Indexing is unaffected: it already extracts DOCX, XLSX and PPTX text from OOXML
  (TASK_028_EXTRA_2) and never used the conversion.
- A managed copy already downloaded stays under `<app data>/mcp-toolchain/libreoffice/` and
  is no longer read; it can be deleted by hand.
- Bringing conversion back would be a new decision, not a toggle: the code is gone, and this
  ADR records why.
