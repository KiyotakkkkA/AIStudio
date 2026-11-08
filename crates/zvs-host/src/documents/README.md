# documents

The document viewer's backend: opening a file (by path, citation, store document or artifact),
detecting its real format, caching an immutable copy by content hash, paging text, and saving
a copy elsewhere. The viewer always reads the cached copy, so a citation keeps pointing at the
exact bytes that were indexed even if the original changes.

## Files

| File         | Contents                                                                                 |
| ------------ | ---------------------------------------------------------------------------------------- |
| `service.rs` | `DocumentService`: open, close, list, reveal, save, page; reference resolution; eviction |
| `kind.rs`    | `detect`: format detection by extension claim plus magic bytes; `pdf_page_estimate`      |
| `text.rs`    | `looks_textual`, `decode` (BOM / UTF-8 / Windows-1251), `paginate`, `first_line`         |
| `cache.rs`   | `DocumentCache`: hash-named files in the cache folder; `eviction_plan`                   |

## Limits

| Constant                 | Value   | Meaning                                |
| ------------------------ | ------- | -------------------------------------- |
| `DOCUMENT_OPEN_MAX`      | 512 MiB | Largest file the viewer opens          |
| `DOCUMENT_TEXT_MAX`      | 32 MiB  | Largest text document paged as text    |
| `DOCUMENT_CACHE_CEILING` | 2 GiB   | Cache size above which eviction starts |

## Pipeline: open

1. **Resolve** the `DocumentRef`:
   - `Path`: must be absolute; canonicalised; its folder must not be refused by the
     forbidden-folder policy (`DOCUMENT_FORBIDDEN`);
   - `Citation`: the cited store document, with the cited page as the initial page;
   - `StoreDocument` / `Document`: the indexed source path;
   - `Artifact`: through `ArtifactStore::resolve` (`../artifacts/README.md`).
2. **Read** at most `open_max` bytes on a blocking thread.
3. **Detect** the format (below). Mislabelled files are refused rather than rendered wrongly.
4. **Hash** with the text core (BLAKE3, `zvs-core`) and **prepare** page and part counts:
   PDF pages are estimated, text is paginated, Office files are counted by parts.
5. **Record** the document row (source, hash, size, kind, counts, `opened_at`) and
   **materialise** the bytes into the cache as `<hash>.<ext>` (`cache_file_name`) if absent.
6. **Evict** (below) and return the `DocumentDto`.

`save` copies the cached file to a location chosen in the save dialog. `page(index)` serves
one text page or reports `DOCUMENT_PAGE_NOT_FOUND`.

## Format detection (`kind::detect`)

The extension gives a _claim_; the first bytes give a _signature_. They must agree:

| Claim (extension)       | Accepted when                                                                       |
| ----------------------- | ----------------------------------------------------------------------------------- |
| PDF                     | `%PDF-` within the first header window                                              |
| image                   | signature is PNG, JPEG, GIF, WebP or BMP                                            |
| Office (docx/xlsx/pptx) | signature is ZIP **and** the archive contains `word/`, `xl/` or `ppt/` respectively |
| text / code             | no binary signature and `looks_textual`                                             |
| unknown                 | PDF or image by signature, else plain text if textual, else unsupported             |

A claim that does not match is `MISLABELLED_DOCUMENT`; an unknown binary is
`UNSUPPORTED_DOCUMENT`. `code_language` maps source extensions to a highlight language.

`pdf_page_estimate` counts `/Type /Page` objects (not `/Pages`) with a regex, which is cheap
and good enough for a page count before the renderer loads the file.

## Text (`text.rs`)

- **`looks_textual`**: UTF-16 with a BOM must have an even length; otherwise the first 8 KiB
  (after a UTF-8 BOM) must contain no C0 control bytes except tab, LF, VT, FF, CR and ESC.
- **`decode`**: UTF-16LE/BE by BOM, UTF-8 by BOM or validity, else Windows-1251 (the common
  legacy encoding for Russian text). CRLF becomes LF.
- **`paginate(text, max_bytes)`**: splits into byte ranges of at most `max_bytes`, ending each
  page after the last newline before the limit (on a char boundary); a single line longer than
  a page is cut at the limit. `first_line` gives the 1-based line number a page starts at.

## Cache eviction (`cache::eviction_plan`)

When the total size of cached files exceeds the ceiling, candidates are all cached files
except those of currently open documents, ordered by the document's `opened_at` (oldest
first, never-opened first of all) and then by name. Files are removed in that order until
the total is back under the ceiling. The plan is computed as a pure function and tested on
its own; the service then deletes the files.
