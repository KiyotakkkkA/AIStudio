# runtimes

Local inference engines: llama.cpp builds downloaded from GitHub releases and run as
`llama-server` processes for local embeddings, OCR (vision models) and reranking. The crate
README's "Runtimes" section is the contract summary; this file maps it to code.

## Files

| File              | Contents                                                                                                 |
| ----------------- | -------------------------------------------------------------------------------------------------------- |
| `definitions.rs`  | `RUNTIME_DEFINITIONS` (CPU, Vulkan, CUDA, Metal builds), asset patterns per platform, `plan_accelerator` |
| `releases.rs`     | `ReleaseResolver`: GitHub releases of `ggml-org/llama.cpp`, asset selection, cache                       |
| `service.rs`      | `RuntimeService`: install, list, remove, stop; unpacking; `embed`, `read_image`, `rerank`                |
| `llama_server.rs` | `LlamaServer`: one engine process (spawn, health, idle stop, request client)                             |

## Choosing a build (`plan_accelerator`)

| Machine                            | Suggested build | Why                                                   |
| ---------------------------------- | --------------- | ----------------------------------------------------- |
| macOS on Apple Silicon             | Metal           | the native GPU path                                   |
| GPU not detected                   | CPU             | nothing to accelerate with                            |
| integrated GPU only                | CPU             | acceleration would gain little                        |
| discrete GPU (see `platform::gpu`) | Vulkan          | works on NVIDIA, AMD and Intel without a CUDA install |

The reason is shown to the user in Russian; CUDA builds stay available as a manual choice.

## Resolving a release (`releases.rs`)

1. Fetch the last 8 releases from the GitHub API, cached for 6 h. If GitHub is unreachable the
   stale cache is kept; a 403 means the API rate limit (`RATE_LIMITED`).
2. Walk releases newest first, skipping drafts and tags that are not build tags (`b` followed
   by digits, e.g. `b11696`).
3. In each release, find the definition's _primary_ asset by regex for the current platform;
   if the primary's captured variant needs a _companion_ (for example the CUDA runtime DLLs),
   that must exist in the same release too, otherwise try the next release.
4. Asset digests are accepted only as `sha256:` + 64 lowercase hex characters; the download
   job verifies them before the archive is renamed into place.

## Install pipeline

`install(definition)` → resolve → offer each asset to the download catalogue → queue the
downloads (`../downloads/README.md`). When an engine archive finishes, the download's
installer hook calls `unpack_download`: extract with `platform::archive` (path-safe), delete
the archive, find `llama-server(.exe)` inside, and record `{ definition, tag, path }` in the
`runtimes.installed` setting.

## Running a model (`LlamaServer`)

- `embed`, `read_image` and `rerank` pick an installed build that can load the model format,
  accelerated builds first, and keep one server per (build, role). `read_image` finds the
  vision projector next to the model by name overlap.
- Spawn: `CREATE_NO_WINDOW`, `kill_on_drop`, a free loopback port, a random API key. Health is
  polled every 250 ms for up to 180 s; the last 4000 characters of stderr are kept to explain a
  failed start.
- Concurrent callers share one launch. The server stops itself after 5 idle minutes and
  resets if the process exits on its own.
- A rerank caller that gives up does not stop a model that is still loading (the next rerank
  will want it); an embedding or vision caller that gives up does.
- The client to the local server bypasses any system proxy.
