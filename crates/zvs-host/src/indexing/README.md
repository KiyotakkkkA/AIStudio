# indexing

Indexing for vector stores («хранилища»): walk the store's sources, extract text with
structure (sections, pages), chunk it, embed the chunks and upsert them into LanceDB, keeping
the store's document rows in step with the files on disk. Searching is
`services::vector_stores`; the chunker and the index are `zvs-core`.

## Files

| File            | Contents                                                                                        |
| --------------- | ----------------------------------------------------------------------------------------------- |
| `service.rs`    | `IndexingService::index`: the whole run, embedding with batching and retry, progress, tallies   |
| `walk.rs`       | `walk_sources`: files under each source, include/exclude patterns, limits, unreachable roots    |
| `patterns.rs`   | Glob matching for include/exclude (`accepts`, `excludes`, anchored and unanchored patterns)     |
| `extraction.rs` | `Extractor`, `ExtractorRegistry`, `Segment`; plain text, Markdown and CSV extractors            |
| `ooxml.rs`      | DOCX, XLSX and PPTX extraction straight from the OOXML parts                                    |
| `documents.rs`  | `PdfExtractor` (sidecar job, optional OCR of image-only pages), `ImageExtractor`, the `OcrPort` |

## Pipeline: `IndexingService::index`

1. **Preconditions.** LanceDB store, current schema, at least one source, an embedding
   provider that exists and supports embeddings, and no other run on the same store
   (`ALREADY_RUNNING`).
2. **Walk** the sources (`walk_sources`): files accepted by the include/exclude patterns,
   ordered with a locale-aware comparison, bounded by `WalkLimits`. Roots that cannot be read
   are reported as unreachable; a walk that hit a limit is marked truncated.
3. **Forget removed files.** Every known document whose path was not discovered is deleted
   from the index, **unless** the walk was truncated or the path is under an unreachable root,
   so an unplugged drive or a limit never wipes a store.
4. **Per file:**
   - skip with a reason if no extractor supports the extension;
   - hash the bytes (BLAKE3); if the hash equals the stored one and this is not a full rebuild,
     count it `unchanged` and move on;
   - extract `Segment`s (text, section path, page);
   - chunk with the store's `chunk_size` / `chunk_overlap` (`zvs-core` chunker);
   - embed and upsert, replacing the document's previous vectors.
5. **Report** progress at most every `progress_interval_ms` (250 ms) and a final tally:
   indexed, unchanged, removed, skipped, chunks, notes (for example pages OCR could not read).

## Embedding

Each chunk is embedded as `embedding_input`:

```
<document prefix><file stem> › <section> › <subsection>
<chunk text>
```

so the vector carries the document title and its heading path, not just the fragment.
Chunks are sent in batches of `batch_size` (32) with up to `concurrency` batches in flight.
A batch that fails with `RATE_LIMITED` is retried up to `max_attempts` (5) with exponential
backoff from `backoff_ms` (500 ms × 2^(attempt−1)); any other error, or cancellation, ends the
run. The provider must return exactly one vector per text (`UNEXPECTED_VECTORS`) of the
store's dimension (`DIMENSION_MISMATCH`).

`estimate_chunks(bytes)` (≈ 6 bytes per token, stride = size − overlap) gives the progress
bar a total before any file is read.

## Extractors

| Extensions                                                     | Extractor                                                                       |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| text and source code (`.txt`, `.json`, `.rs`, `.py`, `.ts`, …) | whole file as one segment                                                       |
| `.md`, `.markdown`, `.mdx`                                     | `#` headings open sections; headings inside fenced code are ignored             |
| `.csv`, `.tsv`                                                 | each row as `column: value` pairs named by the header, quoted fields respected  |
| `.docx`                                                        | paragraphs from `word/document.xml`; heading styles become sections             |
| `.xlsx`                                                        | cell values row by row across sheets (shared strings resolved)                  |
| `.pptx`                                                        | one segment per slide, page = slide number, section = slide title               |
| `.pdf`                                                         | `zvs-jobd` `pdf` job: text per page; pages under `minCharsPerPage` can be OCR'd |
| images                                                         | OCR through the `OcrPort` (a vision model), when configured                     |

Legacy formats (`.doc`, `.odt`, `.rtf`, `.ppt`, `.xls`, `.ods`, `.epub`) are registered as
_deferred_: they are skipped with an explicit reason instead of being silently ignored.

## OOXML parsing (`ooxml.rs`)

No XML library: the parts are read from the zip and scanned with regexes for text runs
(`<w:t>`, `<a:t>`, `<v>` / `<t>` in sheets) with entity decoding (`decode_xml_text`, numeric references via `js_parse_int`).
`heading_levels` reads `styles.xml` to map paragraph style ids to heading levels, so a DOCX
keeps its outline as section paths. `tidy` collapses blank lines.
