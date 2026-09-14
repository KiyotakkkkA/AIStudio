import type { CatalogueItem } from "./catalogue.ts";

const GIB = 1024 ** 3;
const MIB = 1024 ** 2;

/**
 * The catalogue shipped with the app. It makes no outbound call on its own — the Downloads page
 * opens against this list alone, and only the explicit refresh action queries a live source
 * (PASSPORT §14). It therefore goes stale between releases; sizes are the published artefact
 * sizes and are used for the free-space precondition.
 *
 * Entries carry a checksum only where the artefact publishes one we can verify with the
 * algorithms the sidecar implements. Without it the transfer still completes, but the mockup's
 * "Verify checksum" indicator reads false — a knowable gap rather than a silent one.
 */
export const CURATED_CATALOGUE: readonly CatalogueItem[] = [
  {
    ref: "curated:model:qwen2.5-coder-7b-instruct-q4_k_m",
    kind: "model",
    source: "curated",
    name: "qwen2.5-coder:7b",
    displayName: "Qwen2.5 Coder 7B Instruct",
    description:
      "Coding model with strong fill-in-the-middle and repository-level completion. Q4_K_M quantisation.",
    version: "q4_K_M",
    sizeBytes: Math.round(4.68 * GIB),
    fileName: "qwen2.5-coder-7b-instruct-q4_k_m.gguf",
    url: "https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/qwen2.5-coder-7b-instruct-q4_k_m-00001-of-00002.gguf",
    tags: ["Qwen", "q4_K_M", "32k ctx", "tools"],
  },
  {
    ref: "curated:model:gemma-2-9b-it-q4_k_m",
    kind: "model",
    source: "curated",
    name: "gemma2:9b",
    displayName: "Gemma 2 9B Instruct",
    description: "General-purpose instruction model, a good default for a single workstation GPU.",
    version: "q4_K_M",
    sizeBytes: Math.round(5.76 * GIB),
    fileName: "gemma-2-9b-it-q4_k_m.gguf",
    url: "https://huggingface.co/bartowski/gemma-2-9b-it-GGUF/resolve/main/gemma-2-9b-it-Q4_K_M.gguf",
    tags: ["Google", "q4_K_M", "8k ctx"],
  },
  {
    ref: "curated:model:meta-llama-3.1-8b-instruct-q4_k_m",
    kind: "model",
    source: "curated",
    name: "llama3.1:8b",
    displayName: "Llama 3.1 8B Instruct",
    description: "Tool-calling generalist with a long context window.",
    version: "q4_K_M",
    sizeBytes: Math.round(4.92 * GIB),
    fileName: "meta-llama-3.1-8b-instruct-q4_k_m.gguf",
    url: "https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF/resolve/main/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
    tags: ["Meta", "q4_K_M", "128k ctx", "tools"],
  },
  {
    ref: "curated:embedding:nomic-embed-text-v1.5-f16",
    kind: "embedding",
    source: "curated",
    name: "nomic-embed-text",
    displayName: "Nomic Embed Text v1.5",
    description: "Long-context text embeddings with Matryoshka dimensions.",
    version: "f16",
    sizeBytes: Math.round(274 * MIB),
    dimension: 768,
    fileName: "nomic-embed-text-v1.5-f16.gguf",
    url: "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.f16.gguf",
    tags: ["Nomic", "768 dim", "8k ctx"],
  },
  {
    ref: "curated:embedding:mxbai-embed-large-v1-f16",
    kind: "embedding",
    source: "curated",
    name: "mxbai-embed-large",
    displayName: "mxbai Embed Large v1",
    description: "High-quality English retrieval embeddings.",
    version: "f16",
    sizeBytes: Math.round(670 * MIB),
    dimension: 1024,
    fileName: "mxbai-embed-large-v1-f16.gguf",
    url: "https://huggingface.co/mixedbread-ai/mxbai-embed-large-v1/resolve/main/gguf/mxbai-embed-large-v1-f16.gguf",
    tags: ["mixedbread", "1024 dim"],
  },
  {
    ref: "curated:mcp:server-filesystem",
    kind: "mcp",
    source: "curated",
    name: "server-filesystem",
    displayName: "MCP Filesystem server",
    description: "Reads and writes files under an explicit allow-list of roots.",
    version: "2025.8.21",
    sizeBytes: 38 * 1024,
    fileName: "server-filesystem-2025.8.21.tgz",
    url: "https://registry.npmjs.org/@modelcontextprotocol/server-filesystem/-/server-filesystem-2025.8.21.tgz",
    tags: ["stdio", "files"],
  },
  {
    ref: "curated:mcp:server-memory",
    kind: "mcp",
    source: "curated",
    name: "server-memory",
    displayName: "MCP Memory server",
    description: "A knowledge graph the agent can persist facts into across sessions.",
    version: "2025.9.25",
    sizeBytes: 24 * 1024,
    fileName: "server-memory-2025.9.25.tgz",
    url: "https://registry.npmjs.org/@modelcontextprotocol/server-memory/-/server-memory-2025.9.25.tgz",
    tags: ["stdio", "memory"],
  },
  {
    ref: "curated:mcp:server-sequential-thinking",
    kind: "mcp",
    source: "curated",
    name: "server-sequential-thinking",
    displayName: "MCP Sequential Thinking server",
    description: "A scratchpad tool for step-by-step reasoning over a hard problem.",
    version: "2025.7.1",
    sizeBytes: 22 * 1024,
    fileName: "server-sequential-thinking-2025.7.1.tgz",
    url: "https://registry.npmjs.org/@modelcontextprotocol/server-sequential-thinking/-/server-sequential-thinking-2025.7.1.tgz",
    tags: ["stdio", "reasoning"],
  },
];
