# zvs-core

Shared Rust logic for the napi addon and job sidecar. Adapters own transport and
runtime integration; this crate never opens SQLite.

## Cancellation

Every long-running function accepts a `CancellationToken` from `tokio-util` and
checks it before work, at loop boundaries, and before returning its result.
Cancellation returns `Error::Cancelled` (`RUN_CANCELLED`), without partial results.
TASK_027 must pass the job token (or a child token) into every core operation.
Blocking backend calls must be bounded or support cancellation themselves.

## Chunking

`chunk::chunk_text` borrows source text and returns chunks with UTF-8 byte ranges
and token counts. `ChunkConfig` defaults to size 256 and overlap 32. Size must be
positive; overlap must be smaller than size.

A token is a run of non-whitespace graphemes, a word-count approximation rather
than a model tokenizer. Unbroken words and text without spaces remain one token,
regardless of length. Consumers needing a model's context limit must use that
model's tokenizer separately. Grapheme clusters are never divided, including
combining marks, emoji sequences, and whitespace joined to combining marks.

Chunks contain at most `size` tokens and consecutive chunks share exactly
`overlap` tokens. Interior whitespace is preserved; exterior whitespace is
omitted. Before the final chunk, the last sentence-ending token within the size
limit is preferred when it leaves enough tokens to guarantee forward progress.
Sentence endings recognize `.`, `!`, `?`, their CJK equivalents, and common
closing quotes or brackets. This is a punctuation heuristic, not language-aware
sentence parsing. Without a suitable boundary, the size limit wins.

## Hashing and errors

`hash::content_hash` returns the lowercase, 64-character BLAKE3 digest of raw bytes,
checking cancellation between 64 KiB blocks. It performs no text normalization.
TASK_042 can use the digest as a content-cache key.

`Error` and `Result<T>` are shared across capabilities. Error codes match
`packages/shared/src/errors/AppErrorCode.ts`: invalid input maps to
`VALIDATION_FAILED`, cancellation to `RUN_CANCELLED`, backend failures and general
IO failures to `NATIVE_ERROR`. Missing files and denied access retain `NOT_FOUND`
and `PERMISSION_DENIED`. IO errors preserve their source.

## Module handoffs

| Module                   | Implementation task                                                                              |
| ------------------------ | ------------------------------------------------------------------------------------------------ |
| `chunk`, `hash`, `error` | TASK_017                                                                                         |
| `index`                  | TASK_019 storage/search; TASK_028 indexing orchestration                                         |
| `embed`                  | TASK_028 embedding pipeline                                                                      |
| `ocr`                    | Reserved for sidecar document processing after TASK_027; no OCR implementation task assigned yet |

The crate root includes this README as its Rust documentation. Empty modules have
no comments, as requested for TASK_017.

## Build

Use `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`
from the repository root. `pnpm build` runs `build:rust` before package builds,
using `cargo build --workspace --release --locked`. Turbo delegates incremental
Rust caching to Cargo so target artifacts remain specific to the current machine.
