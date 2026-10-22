---
task: TASK_017
title: Cargo workspace, zvs-core skeleton, Rust CI
phase: 3 — Rust core and vector search
status: done
depends_on: [TASK_016]
unblocks: [TASK_018]
estimate: 3 h
---

# TASK_017 — Cargo workspace, zvs-core skeleton, Rust CI

## Context

The app is a working TypeScript product: it stores credentials, connects to AI providers and discovers models. Everything so far runs in Node. This task opens the second half of the architecture — the Rust side that does the heavy work.

PASSPORT decision D1 settles the shape: a **hybrid** boundary. Fast, bounded calls (vector search, embedding a short text, hashing, chunking) go through a **napi-rs addon** loaded in the host process. Long or crash-prone work (corpus indexing, OCR, format conversion) goes to a **sidecar** — a separate binary the host can watch, pause and kill. Both are thin adapters over one crate, `zvs-core`, which holds the real implementation. That way a capability can move between the two without a rewrite.

This task builds only `zvs-core` and its CI. The addon is TASK_018, the sidecar is TASK_027. `Cargo.toml` at the repo root already exists from TASK_001 with an empty members list — fill it in.

## Why

Every Rust capability in the project lands in this crate. Setting up the workspace, lints and CI before there is any code means the standards are enforced from the first line rather than retrofitted.

## Depends on

- `TASK_016` — AI Providers page: three tabs, one form, model cards.

## Read first

- `/docs/PASSPORT.md` §10 (the Rust boundary and what lives where), §5 (crate layout), §16 (build orchestration)

## Scope

**In scope**
- `crates/zvs-core` with a module layout matching the capabilities it will hold
- Workspace-level lint configuration: `clippy` at deny-warnings, `rustfmt`
- Error type and result alias used across the crate
- A first real capability with tests — text chunking, since indexing needs it and it has no external dependencies
- Rust CI job wired into the existing workflow
- Turbo task so `pnpm build` builds Rust too

**Out of scope** (do not do these, even if tempting)
- The napi addon (TASK_018)
- The sidecar binary (TASK_027)
- LanceDB (TASK_019)
- OCR — note the module, leave it empty

## Steps

1. Fill in the root `Cargo.toml` workspace members with `crates/zvs-core`. Set `resolver = "2"` and shared `[workspace.dependencies]` so the addon and sidecar inherit consistent versions later.
2. Create `crates/zvs-core` as a library crate. Module skeleton: `chunk`, `embed`, `index`, `ocr`, `hash`, `error`. Empty modules carry a comment naming the task that fills them.
3. `error.rs` — one crate error enum with `thiserror`, covering IO, invalid input, backend failure and cancellation. Add a `code()` method returning a stable string that maps onto the TypeScript `AppErrorCode` values, so errors survive the boundary with meaning intact.
4. Cancellation convention: every long-running function takes a `CancellationToken` (from `tokio-util`) and checks it at loop boundaries. Write this down in the crate's `lib.rs` doc comment — it is the convention TASK_027 depends on.
5. Implement `chunk`: split text into overlapping chunks by token-ish size with configurable size and overlap, respecting sentence boundaries where it can. This is the real chunker TASK_028 uses.
6. Implement `hash`: content hashing (blake3 or sha256) used by the document cache in TASK_042.
7. Tests: chunk boundaries, overlap correctness, empty and single-word input, text with no sentence breaks, unicode safety (never split a grapheme).
8. CI: add a Rust job — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. Cache the cargo registry and target dir.
9. Turbo: a `build:rust` task, and make `pnpm build` depend on it.

## Verify

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm build
```

Manual check: Confirm CI runs the Rust job on a push and that clippy failures actually fail the build — introduce a warning deliberately, watch it fail, revert.

## Definition of done

- [x] `zvs-core` builds, is linted at deny-warnings and has tests
- [x] Chunking and hashing implemented and covered
- [x] Cancellation convention documented in the crate root
- [x] Error type carries codes that map to `AppErrorCode`
- [ ] Rust CI job green and proven to fail on a warning — equivalent commands passed locally; deliberate Clippy warning failed with exit 101 and was reverted. Hosted run remains to be observed after push.
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Cargo workspace with resolver 2, inherited dependencies and deny-warning lints; `zvs-core` module skeleton; typed errors mapped to shared AppErrorCode strings; cancellation-aware grapheme-safe overlapping chunking and BLAKE3 hashing. Ten integration tests cover boundaries, exact overlap, progress across size/overlap combinations, empty input, Unicode, cancellation, error codes, and hash vectors. Rust CI runs format, Clippy and tests with Cargo caching. Both existing application CI jobs install/cache Rust. Turbo runs the locked release Cargo build before package builds. Cargo.lock records the dependency graph.

**Deviated from plan:** Per the user's no-comments instruction, module handoffs and cancellation documentation live in the crate README, included as crate-root Rust documentation with a `doc` attribute. Token-ish size means whitespace-delimited grapheme runs; unbroken words remain intact, so this is not a model token budget. Hosted CI was not triggered; all equivalent Rust checks passed locally and a temporary `clone_on_copy` warning failed Clippy with exit 101 before being removed. The additional `pnpm format:check` reports pre-existing formatting in migration metadata `_journal.json`, `0002_snapshot.json`, `0004_snapshot.json`, and `0005_snapshot.json`; those files were left outside this task's scope.

**Next session should know:** Required `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `pnpm typecheck`, `pnpm lint`, and `pnpm build` passed on Windows. Observe the Rust job on the next GitHub push. TASK_018 can import `zvs_core::{Error, Result, CancellationToken}`, `chunk::{chunk_text, ChunkConfig}`, and `hash::content_hash`. The addon must own returned text across its async boundary. TASK_027 should pass child cancellation tokens into core operations. Turbo Rust caching is disabled so Cargo manages machine-specific incremental artifacts. Task files remain locally maintained under the existing tasks ignore rule.
